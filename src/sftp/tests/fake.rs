//! A small SFTP server on 127.0.0.1 for the tests of `sftp.rs`, made with the server halves of
//! `russh` and `russh-sftp` (no dependency added). It answers what reading needs (listings,
//! attributes, `realpath`, reads) and deletions (`remove`; `rmdir` of an empty folder only, as a
//! real server). What it holds:
//! - `/loop/`: the file `a.jpg` and `again`, a link to the folder itself. As on Linux, a path
//!   through more than 40 links is refused (ELOOP: measured with `stat` on a real folder);
//! - `/move/`, for a move (#183): `a.jpg`, `sub/b.jpg`, `keep/back` (a link to `/move/`) and
//!   `out` (a link to `/loop/`);
//! - the files given to `start`, in "/": a name, the size the server declares for it and its
//!   real content, which may be much shorter.
//!
//! The client finds the server's key in a `known_hosts` of its own, in a temporary folder that
//! takes the place of `~/.ssh` (`Server::with_ssh_dir`): `~/.ssh` is never read nor written, and
//! the environment of the process is not touched, so these tests run with all the others.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use russh::keys::PrivateKey;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::server::{Auth, ChannelOpenHandle, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version};

use crate::sftp::{Server, Target};

/// Links the kernel follows in one path.
const LINKS: usize = 40;
const PHOTO: &[u8] = b"not a jpeg";

/// What the server holds besides the files of `start`: real folder (no slash at its ends, "" for
/// "/"), name, item (a link holds the real path of its folder).
const ITEMS: &[(&str, &str, Item<'static>)] = &[
    ("", "loop", Item::Dir),
    ("loop", "a.jpg", Item::File(PHOTO.len() as u64, PHOTO)),
    ("loop", "again", Item::Link("loop")),
    ("", "move", Item::Dir),
    ("move", "a.jpg", Item::File(PHOTO.len() as u64, PHOTO)),
    ("move", "keep", Item::Dir),
    ("move/keep", "back", Item::Link("move")),
    ("move", "out", Item::Link("loop")),
    ("move", "sub", Item::Dir),
    ("move/sub", "b.jpg", Item::File(PHOTO.len() as u64, PHOTO)),
];

pub struct World {
    files: Vec<(String, u64, Vec<u8>)>,
    /// The folders listed, in order, as the client asked for them.
    pub listed: Mutex<Vec<String>>,
    /// Real folder and name of each item deleted.
    removed: Mutex<HashSet<(String, String)>>,
    /// The temporary folder in place of `~/.ssh`, removed with the server.
    ssh: std::path::PathBuf,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ssh);
    }
}

#[derive(Clone, Copy)]
enum Item<'a> {
    Dir,
    File(u64, &'a [u8]),
    Link(&'a str),
}

impl World {
    /// The items of the real folder `dir` not deleted: name and item.
    fn items(&self, dir: &str) -> Vec<(&str, Item<'_>)> {
        let removed = self.removed.lock().unwrap();
        let root = self.files.iter().filter(|_| dir.is_empty()).map(|(n, declared, content)| (n.as_str(), Item::File(*declared, content)));
        let items = ITEMS.iter().filter(|(d, _, _)| *d == dir).map(|(_, n, item)| (*n, *item));
        root.chain(items).filter(|(n, _)| !removed.contains(&(dir.to_string(), n.to_string()))).collect()
    }

    fn item(&self, dir: &str, name: &str) -> Option<Item<'_>> {
        self.items(dir).into_iter().find(|(n, _)| *n == name).map(|(_, item)| item)
    }

    /// The real folder holding the last item of `path` and its name, links followed on the way.
    fn split<'p>(&self, path: &'p str) -> Result<(String, &'p str), StatusCode> {
        let (parent, name) = path.trim_end_matches('/').rsplit_once('/').unwrap_or(("", path));
        match self.at(parent)? {
            (real, Item::Dir) => Ok((real.trim_start_matches('/').to_string(), name)),
            _ => Err(StatusCode::NoSuchFile),
        }
    }

    /// What is at `path`, links followed, and where it really is.
    fn at(&self, path: &str) -> Result<(String, Item<'_>), StatusCode> {
        let (mut real, mut item, mut links) = (String::new(), Item::Dir, 0);
        for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
            if !matches!(item, Item::Dir) {
                return Err(StatusCode::NoSuchFile);
            }
            item = self.item(&real, part).ok_or(StatusCode::NoSuchFile)?;
            real = match item {
                Item::Link(to) => {
                    links += 1;
                    item = Item::Dir;
                    to.to_string()
                }
                _ if real.is_empty() => part.to_string(),
                _ => format!("{real}/{part}"),
            };
        }
        if links > LINKS {
            return Err(StatusCode::Failure); // as OpenSSH answers ELOOP
        }
        Ok((format!("/{real}"), item))
    }

    fn attrs(&self, path: &str) -> Result<FileAttributes, StatusCode> {
        Ok(match self.at(path)?.1 {
            Item::File(declared, _) => attrs(0o100_644, declared),
            _ => attrs(0o040_755, 4096),
        })
    }

    /// `remove` of a file or a link (`folder` false), `rmdir` of an empty folder.
    fn delete(&self, path: &str, folder: bool) -> Result<(), StatusCode> {
        let (dir, name) = self.split(path)?;
        let inside = if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
        let deletable = match self.item(&dir, name).ok_or(StatusCode::NoSuchFile)? {
            Item::Dir => folder && self.items(&inside).is_empty(),
            _ => !folder,
        };
        if !deletable {
            return Err(StatusCode::Failure);
        }
        self.removed.lock().unwrap().insert((dir, name.to_string()));
        Ok(())
    }
}

fn attrs(mode: u32, size: u64) -> FileAttributes {
    FileAttributes { size: Some(size), uid: None, user: None, gid: None, group: None, permissions: Some(mode), atime: Some(1_700_000_000), mtime: Some(1_700_000_000) }
}

fn ok(id: u32) -> Status {
    Status { id, status_code: StatusCode::Ok, error_message: "Ok".into(), language_tag: "en-US".into() }
}

/// One SFTP session: the folders opened and not yet read.
struct Sftp {
    world: Arc<World>,
    unread: HashMap<String, String>,
    opened: u32,
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> StatusCode {
        StatusCode::OpUnsupported
    }

    async fn init(&mut self, _version: u32, _extensions: HashMap<String, String>) -> Result<Version, StatusCode> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        Ok(Name { id, files: vec![File::dummy(self.world.at(&path)?.0)] })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        Ok(Attrs { id, attrs: self.world.attrs(&path)? })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let (dir, name) = self.world.split(&path)?;
        let link = matches!(self.world.item(&dir, name), Some(Item::Link(_)));
        let a = self.world.attrs(&path)?;
        Ok(Attrs { id, attrs: if link { attrs(0o120_777, 1) } else { a } })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, StatusCode> {
        let (real, item) = self.world.at(&path)?;
        if !matches!(item, Item::Dir) {
            return Err(StatusCode::Failure);
        }
        self.world.listed.lock().unwrap().push(path);
        self.opened += 1;
        let handle = format!("dir-{}", self.opened);
        self.unread.insert(handle.clone(), real);
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, StatusCode> {
        let Some(real) = self.unread.remove(&handle) else { return Err(StatusCode::Eof) };
        let items = self.world.items(real.trim_start_matches('/'));
        let files = items.into_iter().map(|(name, item)| match item {
            Item::Dir => File::new(name, attrs(0o040_755, 4096)),
            Item::File(declared, _) => File::new(name, attrs(0o100_644, declared)),
            Item::Link(_) => File::new(name, attrs(0o120_777, 1)),
        });
        Ok(Name { id, files: files.collect() })
    }

    async fn open(&mut self, id: u32, filename: String, _flags: OpenFlags, _attrs: FileAttributes) -> Result<Handle, StatusCode> {
        match self.world.at(&filename)? {
            (real, Item::File(..)) => Ok(Handle { id, handle: real }),
            _ => Err(StatusCode::Failure),
        }
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, StatusCode> {
        Ok(Attrs { id, attrs: self.world.attrs(&handle)? })
    }

    async fn read(&mut self, id: u32, handle: String, offset: u64, len: u32) -> Result<Data, StatusCode> {
        let (_, Item::File(_, content)) = self.world.at(&handle)? else { return Err(StatusCode::Failure) };
        let from = usize::try_from(offset).unwrap_or(usize::MAX).min(content.len());
        let data = &content[from..content.len().min(from.saturating_add(len as usize))];
        if data.is_empty() { Err(StatusCode::Eof) } else { Ok(Data { id, data: data.to_vec() }) }
    }

    async fn close(&mut self, id: u32, _handle: String) -> Result<Status, StatusCode> {
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, StatusCode> {
        self.world.delete(&filename, false).map(|()| ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        self.world.delete(&path, true).map(|()| ok(id))
    }
}

/// One SSH connection: any password is accepted, the only subsystem is "sftp".
struct Ssh {
    world: Arc<World>,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl russh::server::Handler for Ssh {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, russh::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(&mut self, channel: Channel<Msg>, reply: ChannelOpenHandle, _session: &mut Session) -> Result<(), russh::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(&mut self, id: ChannelId, name: &str, session: &mut Session) -> Result<(), russh::Error> {
        match self.channels.remove(&id) {
            Some(channel) if name == "sftp" => {
                session.channel_success(id)?;
                let sftp = Sftp { world: self.world.clone(), unread: HashMap::new(), opened: 0 };
                russh_sftp::server::run(channel.into_stream(), sftp).await;
            }
            _ => session.channel_failure(id)?,
        }
        Ok(())
    }
}

struct Listener(Arc<World>);

impl russh::server::Server for Listener {
    type Handler = Ssh;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> Ssh {
        Ssh { world: self.0.clone(), channels: HashMap::new() }
    }
}

/// A temporary folder in place of `~/.ssh`, whose `known_hosts` has the key of the server on
/// `port`: one per server (the port is in its name), as the tests run together.
fn ssh_with_known_host(port: u16, key: &PrivateKey) -> std::path::PathBuf {
    let ssh = std::env::temp_dir().join(format!("eyecansee-sftp-fake-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&ssh).unwrap();
    let line = format!("[127.0.0.1]:{port} {}\n", key.public_key().to_openssh().unwrap());
    std::fs::write(ssh.join("known_hosts"), line).unwrap();
    ssh
}

/// Starts the server on a free port of 127.0.0.1, in the runtime of the caller, with these files
/// in "/" (name, declared size, content). The client connected to it, and what the server saw.
pub async fn start(files: &[(&str, u64, &[u8])]) -> (Arc<Server>, Arc<World>) {
    let files = files.iter().map(|(name, declared, content)| (name.to_string(), *declared, content.to_vec())).collect();
    // A fixed key, only for this server.
    let key = PrivateKey::from(Ed25519Keypair::from_seed(&[7; 32]));
    let socket = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = socket.local_addr().unwrap().port();
    let ssh = ssh_with_known_host(port, &key);
    let world = Arc::new(World { files, listed: Mutex::new(Vec::new()), removed: Mutex::new(HashSet::new()), ssh: ssh.clone() });
    let config = Arc::new(russh::server::Config { keys: vec![key], ..Default::default() });
    let mut listener = Listener(world.clone());
    tokio::spawn(async move { listener.run_on_socket(config, &socket).await });
    let target = Target { user: "tester".into(), host: "127.0.0.1".into(), port: Some(port), password: Some("x".into()), key: None, saved: true };
    (Server::with_ssh_dir(target, ssh), world)
}
