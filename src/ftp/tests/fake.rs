//! A small FTP server on 127.0.0.1 for the tests of `ftp.rs`: plain threads, passive mode, no
//! TLS. What it holds (`ITEMS`):
//! - `/loop/`: the file `a.jpg` and `again`, a link to the folder itself; as on Linux, a path
//!   through more than 40 links is refused;
//! - `/move/`, for a move (#183): `a.jpg`, `sub/b.jpg`, `keep/back` (a link to `/move/`) and
//!   `out` (a link to `/loop/`);
//! - `/album/`, as a hostile server could list it (audit 2.3): `a.jpg` and `../victim.txt`, read
//!   at the path the client makes with that name;
//! - `/beach/`, with a name Windows cannot write (audit 2.4): `a.jpg` and `Mare 12:30.jpg`.
//!
//! Files are measured (SIZE), read (RETR) and deleted (DELE), folders deleted only when empty
//! (RMD), as a real server does. It has no MLSD (as vsftpd): the client falls back to LIST, whose
//! Unix lines describe the links.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::ftp::Target;

/// Links the kernel follows in one path.
const LINKS: usize = 40;
const PHOTO: &[u8] = b"not a jpeg";
const HOSTILE: &[u8] = b"written by the server";

enum Kind {
    Dir,
    File(&'static [u8]),
    /// A link to this folder (its real path).
    Link(&'static str),
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
                Kind::File(_) => return None,
            };
        }
        (links <= LINKS).then_some(real)
    }

    /// The real folder holding the last item of `path`, and its name.
    fn split<'a>(&self, path: &'a str) -> Option<(String, &'a str)> {
        let (parent, name) = path.trim_end_matches('/').rsplit_once('/')?;
        Some((self.folder(parent)?, name))
    }

    /// The lines of LIST for a folder.
    fn listing(&self, path: &str) -> Option<String> {
        let dir = self.folder(path)?;
        let lines = ITEMS.iter().filter(|(d, n, _)| *d == dir && self.item(d, n).is_some()).map(|(_, name, kind)| match kind {
            Kind::Dir => format!("drwxr-xr-x    2 me       users        4096 Oct 30 09:10 {name}\r\n"),
            Kind::File(c) => format!("-rw-r--r--    1 me       users {:>11} Oct 30 09:10 {name}\r\n", c.len()),
            Kind::Link(to) => format!("lrwxrwxrwx    1 me       users           1 Oct 30 09:10 {name} -> /{to}\r\n"),
        });
        Some(lines.collect())
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
            "RETR" => match (seen.split(path).and_then(|(dir, name)| seen.item(&dir, name)).or_else(|| seen.verbatim(path)), data.take()) {
                (Some(Kind::File(content)), Some(listener)) => send(&mut say, listener, content)?,
                _ => say("550 no such file")?,
            },
            "SIZE" => match seen.split(path).and_then(|(dir, name)| seen.item(&dir, name)) {
                Some(Kind::File(content)) => say(&format!("213 {}", content.len()))?,
                _ => say("550 no such file")?,
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
