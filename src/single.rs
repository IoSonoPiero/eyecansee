//! One EyeCanSee at a time: a phone can be held by one program only, so a second window could
//! not reach it. The first instance listens on a local socket (Linux: abstract Unix socket,
//! Windows: named pipe); a later one hands it the address to open and exits.
// ponytail: two instances started in the same instant can both miss each other and both run.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::Sender;

use eframe::egui::Context;
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Name, Stream, prelude::*};

use crate::loader::Msg;

/// One name per user: abstract sockets and named pipes are visible to every user of the computer.
fn name() -> Option<Name<'static>> {
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    format!("eyecansee-{user}.sock").to_ns_name::<GenericNamespaced>().ok()
}

/// Hands `address` ("" = none: just come forward) to an instance already running.
/// `true` = done, this instance must exit.
pub fn hand_over(address: &str) -> bool {
    let Some(mut stream) = name().and_then(|n| Stream::connect(n).ok()) else { return false };
    stream.write_all(format!("{address}\n").as_bytes()).is_ok()
}

/// This is the instance: addresses handed over by later ones arrive as `Msg::Open`.
pub fn listen(tx: Sender<Msg>, ctx: Context) {
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
