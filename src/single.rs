//! One EyeCanSee at a time: a phone can be held by one program only, so a second window could
//! not reach it. The first instance listens on a local socket (Linux: abstract Unix socket,
//! Windows: named pipe); a later one hands it the address to open and exits.
//! `EYECANSEE_MULTI=1` turns this off for one process (see `multi`).
// ponytail: two instances started in the same instant can both miss each other and both run.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::Sender;

use eframe::egui::Context;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream, prelude::*};

use crate::loader::Msg;

/// One name per user: abstract sockets and named pipes are visible to every user of the computer.
fn name() -> Option<Name<'static>> {
    socket().to_ns_name::<GenericNamespaced>().ok()
}

/// "eyecansee-<user>.sock" (the test copy's has its own name: `test_copy::NAME`).
pub(crate) fn socket() -> String {
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    format!("{}-{user}.sock", crate::test_copy::NAME)
}

/// `EYECANSEE_MULTI=1` (any value but empty or `0`): this process runs on its own, for tests that
/// need several windows at once. It neither hands its address to a running EyeCanSee nor listens,
/// so a normal one started later does not hand its address to it either; at start it leaves the
/// video copies in the temporary folder alone (another instance may be playing them).
/// Give each such instance its own settings and cache: Linux `XDG_CONFIG_HOME` and
/// `XDG_CACHE_HOME`, Windows `APPDATA` and `LOCALAPPDATA` (there the downloaded components and the
/// kept edits move with it), and its own `EYECANSEE_LOG` file. Otherwise settings.json and
/// servers.json are rewritten whole by whichever closes or saves last; thumbs.db can be shared
/// (SQLite WAL, 5 s busy timeout). A phone is held by one program only, whichever instance opens it.
pub fn multi() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| on(std::env::var_os("EYECANSEE_MULTI").as_deref()))
}

fn on(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

/// Hands `address` ("" = none: just come forward) to an instance already running.
/// `true` = done, this instance must exit.
pub fn hand_over(address: &str) -> bool {
    if multi() {
        log!("single instance: off (EYECANSEE_MULTI)");
        return false;
    }
    let Some(mut stream) = name().and_then(|n| Stream::connect(n).ok()) else { return false };
    stream.write_all(format!("{address}\n").as_bytes()).is_ok()
}

/// This is the instance: addresses handed over by later ones arrive as `Msg::Open`.
pub fn listen(tx: Sender<Msg>, ctx: Context) {
    if multi() {
        return;
    }
    let listener = match name().map(|n| ListenerOptions::new().name(n).create_sync()) {
        Some(Ok(l)) => l,
        Some(Err(e)) => {
            log!("single instance: {e}");
            return;
        }
        None => return,
    };
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(conn).read_line(&mut line).is_ok() {
                if tx.send(Msg::Open(line.trim_end().to_string())).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    #[test]
    fn multi_value() {
        assert!(!super::on(None));
        assert!(!super::on(Some(OsStr::new(""))));
        assert!(!super::on(Some(OsStr::new("0"))));
        assert!(super::on(Some(OsStr::new("1"))));
        assert!(super::on(Some(OsStr::new("yes"))));
    }
}
