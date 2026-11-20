//! A small FTP server on 127.0.0.1 for the tests of `ftp.rs`: plain threads, passive mode, no
//! TLS. It holds one folder, `/loop/`, with the file `a.jpg` and `again`, a link to the folder
//! itself; as on Linux, a path through more than 40 links is refused. It has no MLSD (as vsftpd):
//! the client falls back to LIST, whose Unix lines describe the link.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::ftp::Target;

/// The link of `/loop/` to itself.
const LINK: &str = "again";
/// Links the kernel follows in one path.
const LINKS: usize = 40;

/// The folders listed, in order, as the client asked for them.
#[derive(Default)]
pub struct Seen {
    pub listed: Mutex<Vec<String>>,
}

/// The lines of LIST for a folder, links followed; `None`: not a folder, or through too many links.
fn listing(path: &str) -> Option<&'static str> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let real: Vec<&str> = parts.iter().copied().filter(|p| *p != LINK).collect();
    match real.as_slice() {
        [] if parts.is_empty() => Some("drwxr-xr-x    2 me       users        4096 Oct 30 09:10 loop\r\n"),
        ["loop"] if parts.len() - real.len() <= LINKS => Some(concat!(
            "-rw-r--r--    1 me       users          10 Oct 30 09:10 a.jpg\r\n",
            "lrwxrwxrwx    1 me       users           1 Oct 30 09:10 again -> .\r\n",
        )),
        _ => None,
    }
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
            "CWD" => say(if listing(path).is_some() { "250 fine" } else { "550 no such folder" })?,
            "LIST" => match (listing(path), data.take()) {
                (Some(lines), Some(listener)) => {
                    seen.listed.lock().unwrap().push(path.to_string());
                    say("150 here it comes")?;
                    let (mut to, _) = listener.accept()?;
                    to.write_all(lines.as_bytes())?;
                    drop(to);
                    say("226 sent")?;
                }
                _ => say("550 no such folder")?,
            },
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
