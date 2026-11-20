//! A small FTP server on 127.0.0.1 for the tests of the audit, family 1: plain threads, passive
//! mode, no TLS, no MLSD (the client falls back to LIST), files held in memory. Folders and files
//! are given by their absolute paths as the client writes them (`"/album"`, `"/album/a.jpg"`);
//! a listed name is any text, `../x` too, as a hostile server could send it.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::ftp::Target;

/// What the server holds: folder → its listed names (`None`: a folder, `Some`: a file's
/// absolute path as the client asks for it); file path → content.
#[derive(Default)]
pub struct Tree {
    pub folders: HashMap<String, Vec<(String, Option<String>)>>,
    pub files: HashMap<String, Vec<u8>>,
    /// The commands received with a path, in order.
    pub log: Vec<String>,
}

impl Tree {
    /// A folder (absolute, no final "/") with files `(name, content)`; the file's path is
    /// `folder/name`, written as the client will ask for it.
    pub fn folder(&mut self, dir: &str, files: &[(&str, &[u8])]) {
        let entries = self.folders.entry(dir.to_string()).or_default();
        for (name, content) in files {
            let path = format!("{dir}/{name}");
            entries.push((name.to_string(), Some(path.clone())));
            self.files.insert(path, content.to_vec());
        }
        // Its parent lists it.
        if let Some((parent, own)) = dir.rsplit_once('/')
            && !own.is_empty()
        {
            let up = self.folders.entry(parent.to_string()).or_default();
            if !up.iter().any(|(n, _)| n == own) {
                up.push((own.to_string(), None));
            }
        }
    }
}

fn key(path: &str) -> String {
    let p = path.trim_end_matches('/');
    if p.starts_with('/') || p.is_empty() { p.to_string() } else { format!("/{p}") }
}

fn lines(tree: &Tree, dir: &str) -> Option<String> {
    let entries = tree.folders.get(&key(dir))?;
    Some(
        entries
            .iter()
            .map(|(name, file)| match file {
                None => format!("drwxr-xr-x    2 me       users        4096 Oct 30 09:10 {name}\r\n"),
                Some(path) => format!("-rw-r--r--    1 me       users {:>12} Oct 30 09:10 {name}\r\n", tree.files.get(path).map_or(0, Vec::len)),
            })
            .collect(),
    )
}

fn session(control: TcpStream, tree: &Mutex<Tree>) -> std::io::Result<()> {
    let mut out = control.try_clone()?;
    let mut say = |line: &str| out.write_all(format!("{line}\r\n").as_bytes());
    say("220 test server")?;
    let (mut data, mut rest): (Option<TcpListener>, usize) = (None, 0);
    for line in BufReader::new(control).lines() {
        let line = line?;
        let (command, path) = line.split_once(' ').unwrap_or((&line, ""));
        let command = command.to_ascii_uppercase();
        if !path.is_empty() && !matches!(command.as_str(), "USER" | "PASS" | "TYPE") {
            tree.lock().unwrap().log.push(format!("{command} {path}"));
        }
        let t = tree.lock().unwrap();
        match command.as_str() {
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
            "REST" => {
                rest = path.parse().unwrap_or(0);
                say("350 restarting")?;
            }
            "CWD" => say(if t.folders.contains_key(&key(path)) { "250 fine" } else { "550 no such folder" })?,
            "SIZE" => match t.files.get(&key(path)) {
                Some(f) => say(&format!("213 {}", f.len()))?,
                None => say("550 no such file")?,
            },
            "LIST" | "RETR" => {
                let body = if command == "LIST" { lines(&t, path).map(String::into_bytes) } else { t.files.get(&key(path)).map(|f| f[rest.min(f.len())..].to_vec()) };
                rest = 0;
                match (body, data.take()) {
                    (Some(body), Some(listener)) => {
                        say("150 here it comes")?;
                        let (mut to, _) = listener.accept()?;
                        to.write_all(&body)?;
                        drop(to);
                        say("226 sent")?;
                    }
                    (b, d) => {
                        eprintln!("fake ftp: {command} {path}: content {}, data connection {}", b.is_some(), d.is_some());
                        say("550 no such file or folder")?
                    }
                }
            }
            _ => say("500 unknown command")?,
        }
    }
    Ok(())
}

/// Starts the server on a free port of 127.0.0.1 with `tree`. Where to connect, and the tree
/// (its `log` grows).
pub fn start(tree: Tree) -> (Target, Arc<Mutex<Tree>>) {
    let tree = Arc::new(Mutex::new(tree));
    let control = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = control.local_addr().unwrap().port();
    let shared = tree.clone();
    std::thread::spawn(move || {
        for connection in control.incoming().flatten() {
            let tree = shared.clone();
            std::thread::spawn(move || session(connection, &tree));
        }
    });
    (Target { host: "127.0.0.1".into(), port, user: "tester".into(), password: "x".into(), secure: false }, tree)
}
