//! A small FTP server on 127.0.0.1 for the tests of `ftp.rs`: plain threads, passive mode, no
//! TLS. What it holds (`ITEMS`):
//! - `/loop/`: the file `a.jpg` and `again`, a link to the folder itself; as on Linux, a path
//!   through more than 40 links is refused;
//! - `/move/`, for a move (#183): `a.jpg`, `sub/b.jpg`, `keep/back` (a link to `/move/`) and
//!   `out` (a link to `/loop/`);
//! - `/album/`, as a hostile server could list it (audit 2.3): `a.jpg` and `../victim.txt`, read
//!   at the path the client makes with that name;
//! - `/beach/`, with a name Windows cannot write (audit 2.4): `a.jpg` and `Mare 12:30.jpg`;
//! - `/links/`, listed as SFTPGo lists a folder (a Hetzner Storage Box, L17 of 2026-11-21, `GO`):
//!   `due.jpg`, `self` (a link to the folder itself), `uno.jpg`, and `zeta.jpg`, a link to `uno.jpg`.
//!
//! Files are measured (SIZE), read (RETR) and deleted (DELE), folders deleted only when empty
//! (RMD), as a real server does. It has no MLSD (as vsftpd) but in `GO`: the client falls back to
//! LIST, whose Unix lines describe the links.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::ftp::Target;

/// Links the kernel follows in one path.
const LINKS: usize = 40;
const PHOTO: &[u8] = b"not a jpeg";
const HOSTILE: &[u8] = b"written by the server";
const UNO: &[u8] = b"the first photo, longer than the path of a link";

/// The folder listed as SFTPGo does (its MLSD and LIST as seen on a Storage Box, 2026-11-21):
/// MLSD tells a link as the file it is on the disk ("Type=file;Size=1" for a link to "."), LIST
/// with Go's letter "L" and no "-> where". A link to a folder read (RETR) starts a transfer with
/// no bytes, then "550 … is a directory"; SIZE of it is refused.
const GO: &str = "links";

enum Kind {
    Dir,
    File(&'static [u8]),
    /// A link to this folder (its real path).
    Link(&'static str),
    /// A link to this file (its real folder and name), as `GO` lists it: SIZE and RETR follow it.
    FileLink(&'static str, &'static str),
}

/// What the server holds: real folder (no slash at its ends, "" for "/"), name, kind.
const ITEMS: &[(&str, &str, Kind)] = &[
    ("", "loop", Kind::Dir),
    ("loop", "a.jpg", Kind::File(PHOTO)),
    ("loop", "again", Kind::Link("loop")),
    ("", "move", Kind::Dir),
    ("move", "a.jpg", Kind::File(PHOTO)),
    ("move", "keep", Kind::Dir),
    ("move/keep", "back", Kind::Link("move")),
    ("move", "out", Kind::Link("loop")),
    ("move", "sub", Kind::Dir),
    ("move/sub", "b.jpg", Kind::File(PHOTO)),
    ("", "album", Kind::Dir),
    ("album", "a.jpg", Kind::File(PHOTO)),
    ("album", "../victim.txt", Kind::File(HOSTILE)),
    ("", "beach", Kind::Dir),
    ("beach", "a.jpg", Kind::File(PHOTO)),
    ("beach", "Mare 12:30.jpg", Kind::File(PHOTO)),
    ("", "links", Kind::Dir),
    ("links", "uno.jpg", Kind::File(UNO)),
    ("links", "self", Kind::Link("links")),
    ("links", "due.jpg", Kind::File(PHOTO)),
    ("links", "zeta.jpg", Kind::FileLink("links", "uno.jpg")),
];

/// What the client did: the folders listed, in order, and the items deleted.
#[derive(Default)]
pub struct Seen {
    pub listed: Mutex<Vec<String>>,
    /// Real folder and name of each item deleted.
    removed: Mutex<HashSet<(String, String)>>,
}

impl Seen {
    /// The item `name` of the real folder `dir`, unless deleted.
    fn item(&self, dir: &str, name: &str) -> Option<&'static Kind> {
        let gone = self.removed.lock().unwrap().contains(&(dir.to_string(), name.to_string()));
        ITEMS.iter().find(|(d, n, _)| *d == dir && *n == name && !gone).map(|(_, _, k)| k)
    }

    /// An item listed with a name that holds a path (audit 2.3), at the path the client makes
    /// with it ("/album/../victim.txt").
    fn verbatim(&self, path: &str) -> Option<&'static Kind> {
        ITEMS.iter().find(|(d, n, _)| n.contains('/') && format!("/{d}/{n}") == path).map(|(_, _, k)| k)
    }

    /// The real path of the folder `path`, links followed; `None`: not a folder, or through too
    /// many links.
    fn folder(&self, path: &str) -> Option<String> {
        let (mut real, mut links) = (String::new(), 0);
        for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
            real = match self.item(&real, part)? {
                Kind::Dir if real.is_empty() => part.to_string(),
                Kind::Dir => format!("{real}/{part}"),
                Kind::Link(to) => {
                    links += 1;
                    to.to_string()
                }
                Kind::File(_) | Kind::FileLink(..) => return None,
            };
        }
        (links <= LINKS).then_some(real)
    }

    /// The real folder holding the last item of `path`, and its name.
    fn split<'a>(&self, path: &'a str) -> Option<(String, &'a str)> {
        let (parent, name) = path.trim_end_matches('/').rsplit_once('/')?;
        Some((self.folder(parent)?, name))
    }

    /// The lines of LIST for a folder; in `GO` as SFTPGo writes them.
    fn listing(&self, path: &str) -> Option<String> {
        let dir = self.folder(path)?;
        let go = dir == GO;
        let lines = ITEMS.iter().filter(|(d, n, _)| *d == dir && self.item(d, n).is_some()).map(|(_, name, kind)| match kind {
            Kind::Dir => format!("drwxr-xr-x    2 me       users        4096 Oct 30 09:10 {name}\r\n"),
            Kind::File(c) => format!("-rw-r--r--    1 me       users {:>11} Oct 30 09:10 {name}\r\n", c.len()),
            Kind::Link(_) | Kind::FileLink(..) if go => format!("Lrwxrwxrwx 1 ftp ftp {:>12} Nov 21 08:25 {name}\r\n", link_size(kind)),
            Kind::Link(to) => format!("lrwxrwxrwx    1 me       users           1 Oct 30 09:10 {name} -> /{to}\r\n"),
            Kind::FileLink(d, n) => format!("lrwxrwxrwx    1 me       users           1 Oct 30 09:10 {name} -> /{d}/{n}\r\n"),
        });
        Some(lines.collect())
    }

    /// The lines of MLSD for a folder of `GO`; `None` elsewhere (no MLSD there).
    fn mlsd(&self, path: &str) -> Option<String> {
        let dir = self.folder(path).filter(|d| d == GO)?;
        let lines = ITEMS.iter().filter(|(d, n, _)| *d == dir && self.item(d, n).is_some()).map(|(_, name, kind)| match kind {
            Kind::Dir => format!("Type=dir;Size=0;Modify=20261121082501; {name}\r\n"),
            Kind::File(c) => format!("Type=file;Size={};Modify=20261121082501; {name}\r\n", c.len()),
            Kind::Link(_) | Kind::FileLink(..) => format!("Type=file;Size={};Modify=20261121082504; {name}\r\n", link_size(kind)),
        });
        Some(lines.collect())
    }

    /// The file an item is, a link to a file followed; `None`: not a file.
    fn content(&self, path: &str) -> Option<&'static [u8]> {
        let kind = self.split(path).and_then(|(dir, name)| self.item(&dir, name)).or_else(|| self.verbatim(path))?;
        match kind {
            Kind::File(c) => Some(c),
            Kind::FileLink(d, n) => match self.item(d, n)? {
                Kind::File(c) => Some(c),
                _ => None,
            },
            _ => None,
        }
    }

    /// DELE of a file or a link, RMD of an empty folder: whether it went.
    fn delete(&self, path: &str, folder: bool) -> bool {
        let Some((dir, name)) = self.split(path) else { return false };
        let empty = || self.listing(path).is_some_and(|l| l.is_empty());
        let deletable = match self.item(&dir, name) {
            Some(Kind::Dir) => folder && empty(),
            Some(_) => !folder,
            None => false,
        };
        deletable && self.removed.lock().unwrap().insert((dir, name.to_string()))
    }
}

/// The size a link has on the disk: the length of the path it holds.
fn link_size(kind: &Kind) -> usize {
    match kind {
        Kind::Link(to) if *to == GO => 1,
        Kind::Link(to) => to.len() + 1,
        Kind::FileLink(_, n) => n.len(),
        _ => 0,
    }
}

/// Sends `data` on the passive connection, between "150" and "226".
fn send(say: &mut impl FnMut(&str) -> std::io::Result<()>, listener: TcpListener, data: &[u8]) -> std::io::Result<()> {
    say("150 here it comes")?;
    let (mut to, _) = listener.accept()?;
    to.write_all(data)?;
    drop(to);
    say("226 sent")
}

fn session(control: TcpStream, seen: &Seen) -> std::io::Result<()> {
    let mut out = control.try_clone()?;
    let mut say = |line: &str| out.write_all(format!("{line}\r\n").as_bytes());
    say("220 test server")?;
    let mut data: Option<TcpListener> = None;
    for line in BufReader::new(control).lines() {
        let line = line?;
        let (command, path) = line.split_once(' ').unwrap_or((&line, ""));
        match command.to_ascii_uppercase().as_str() {
            "USER" => say("331 password, please")?,
            "PASS" => say("230 logged in")?,
            "TYPE" | "NOOP" => say("200 fine")?,
            "PWD" => say("257 \"/\" is the current folder")?,
            "QUIT" => return say("221 bye"),
            "PASV" => {
                let listener = TcpListener::bind("127.0.0.1:0")?;
                let port = listener.local_addr()?.port();
                say(&format!("227 Entering Passive Mode (127,0,0,1,{},{})", port >> 8, port & 255))?;
                data = Some(listener);
            }
            "CWD" => say(if seen.folder(path).is_some() { "250 fine" } else { "550 no such folder" })?,
            "LIST" => match (seen.listing(path), data.take()) {
                (Some(lines), Some(listener)) => {
                    seen.listed.lock().unwrap().push(path.to_string());
                    send(&mut say, listener, lines.as_bytes())?;
                }
                _ => say("550 no such folder")?,
            },
            "MLSD" => match (seen.mlsd(path), data.take()) {
                (Some(lines), Some(listener)) => {
                    seen.listed.lock().unwrap().push(format!("MLSD {path}"));
                    send(&mut say, listener, lines.as_bytes())?;
                }
                (None, listener) => {
                    data = listener;
                    say("500 unknown command")?
                }
                _ => say("550 no such folder")?,
            },
            "RETR" => match (seen.content(path), data.take()) {
                (Some(content), Some(listener)) => send(&mut say, listener, content)?,
                // SFTPGo: a folder read starts the transfer, sends nothing and then refuses.
                (None, Some(listener)) if seen.folder(path).is_some() => {
                    say("150 Using transfer connection")?;
                    drop(listener.accept()?);
                    say(&format!("550 Issue during transfer: read {path}: is a directory"))?
                }
                _ => say("550 no such file")?,
            },
            "SIZE" => match seen.content(path) {
                Some(content) => say(&format!("213 {}", content.len()))?,
                None => say("550 no such file")?,
            },
            "DELE" | "RMD" => say(if seen.delete(path, command.eq_ignore_ascii_case("RMD")) { "250 deleted" } else { "550 not deleted" })?,
            _ => say("500 unknown command")?,
        }
    }
    Ok(())
}

/// Starts the server on a free port of 127.0.0.1. Where to connect, and what the server saw.
pub fn start() -> (Target, Arc<Seen>) {
    let seen = Arc::new(Seen::default());
    let control = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = control.local_addr().unwrap().port();
    let shared = seen.clone();
    std::thread::spawn(move || {
        for connection in control.incoming().flatten() {
            let seen = shared.clone();
            // A client that leaves in the middle of a command ends its session, nothing more.
            std::thread::spawn(move || session(connection, &seen));
        }
    });
    (Target { host: "127.0.0.1".into(), port, user: "tester".into(), password: "x".into(), secure: false }, seen)
}
