//! One EyeCanSee at a time: a phone can be held by one program only, so a second window could
//! not reach it. The first instance listens on a local socket (Linux: abstract Unix socket,
//! Windows: named pipe) from the start of `main`, before its window is made, so starts close
//! together find it (one process per file chosen in a file manager); a later one hands it the
//! address to open and exits. The addresses wait in a channel until the window takes them.
//! `EYECANSEE_MULTI=1` turns this off for one process (see `multi`).

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui::Context;
use interprocess::local_socket::{GenericNamespaced, Listener, ListenerOptions, Name, Stream, prelude::*};

use crate::App;
use crate::loader::Msg;

/// The longest address read: a connection that sends more is dropped (audit 1.10).
const LONGEST: u64 = 64 * 1024;
/// A connection that sends nothing is dropped after this. Named pipes have no time limits: there
/// it only keeps its own thread until the other side closes it.
const READ: Duration = Duration::from_secs(2);
/// Connections read at once; more are dropped.
const AT_ONCE: usize = 16;
/// Hand-over and listening tried in turn: the name can be taken between the two.
const ATTEMPTS: usize = 5;

/// One name per user: abstract sockets and named pipes are visible to every user of the computer.
/// `socket`: `socket()`, or a name of a test's own.
fn ns_name(socket: &str) -> Option<Name<'static>> {
    socket.to_owned().to_ns_name::<GenericNamespaced>().ok()
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

/// What `start` found.
pub enum Start {
    /// A copy running took the address: this one exits.
    HandedOver,
    /// This is the copy: the later starts hand their addresses to it.
    Listening(Inbox),
    /// Neither (`EYECANSEE_MULTI`, a screenshot, a name that could not be made): a window of its own.
    Alone,
    /// A window of its own next to a copy that holds the name but did not take the address
    /// (another user's, on Linux): that copy may be using its files.
    NextTo,
}

/// At the start of `main`: hands `address` ("" = none: just come forward) to a copy running, or
/// becomes the copy, before the window is made.
pub fn start(address: &str) -> Start {
    if multi() {
        log!("single instance: off (EYECANSEE_MULTI)");
        return Start::Alone;
    }
    start_on(&socket(), address)
}

/// `start` on `socket`. Two starts at once both find nobody: one makes the name, the other finds
/// it taken and hands its address over to it (audit 1.10).
fn start_on(socket: &str, address: &str) -> Start {
    for attempt in 1..=ATTEMPTS {
        match hand_over_to(socket, address) {
            Handover::Done => return Start::HandedOver,
            Handover::Foreign => return Start::NextTo,
            Handover::Nobody => {}
        }
        match listen_on(socket) {
            Ok(inbox) => return Start::Listening(inbox),
            Err(e) => log!("single instance: {e} (attempt {attempt})"),
        }
    }
    Start::NextTo
}

/// How a hand-over went.
#[derive(Debug, PartialEq)]
enum Handover {
    Done,
    /// Nobody listens: this start may become the copy.
    Nobody,
    /// The name is another user's (Linux): nothing is sent to it.
    Foreign,
}

/// `address` given to the copy listening on `socket`.
fn hand_over_to(socket: &str, address: &str) -> Handover {
    let Some(stream) = ns_name(socket).and_then(|n| Stream::connect(n).ok()) else { return Handover::Nobody };
    if !same_user_as_me(&stream) {
        log!("single instance: {socket} is another user's");
        return Handover::Foreign;
    }
    if (&stream).write_all(format!("{address}\n").as_bytes()).is_ok() { Handover::Done } else { Handover::Nobody }
}

/// This copy listens: the addresses handed over wait here until the window takes them
/// (`App::answer_handed`), also those that came before it was made.
pub struct Inbox {
    rx: Receiver<String>,
    /// The window, once made: woken when an address arrives.
    ctx: Arc<OnceLock<Context>>,
}

impl Inbox {
    /// The addresses waiting.
    fn take(&self) -> Vec<String> {
        self.rx.try_iter().collect()
    }
}

/// Listens on `socket`; `Err` when the name is taken (or cannot be made).
fn listen_on(socket: &str) -> std::io::Result<Inbox> {
    let name = ns_name(socket).ok_or_else(|| std::io::Error::other("no name"))?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    let (tx, rx) = mpsc::channel();
    let ctx = Arc::new(OnceLock::new());
    let wake = Arc::clone(&ctx);
    crate::logging::spawn_named("single-listen", move || accept_all(&listener, &tx, &wake));
    Ok(Inbox { rx, ctx })
}

/// Each connection read by a thread of its own, so one that sends nothing stops no other.
fn accept_all(listener: &Listener, tx: &Sender<String>, ctx: &Arc<OnceLock<Context>>) {
    let busy = Arc::new(AtomicUsize::new(0));
    let mut errors = 0;
    loop {
        let conn = match listener.accept() {
            Ok(conn) => conn,
            Err(e) => {
                errors += 1;
                let Some(pause) = pause_after(errors) else {
                    log!("single instance: no longer listening ({e})");
                    return;
                };
                std::thread::sleep(pause);
                continue;
            }
        };
        errors = 0;
        if busy.fetch_add(1, Ordering::SeqCst) >= AT_ONCE {
            busy.fetch_sub(1, Ordering::SeqCst);
            continue;
        }
        let (tx, ctx, busy) = (tx.clone(), Arc::clone(ctx), Arc::clone(&busy));
        crate::logging::spawn_named("single-read", move || {
            if let Some(address) = take_in(&conn) {
                let _ = tx.send(address);
                if let Some(ctx) = ctx.get() {
                    ctx.request_repaint();
                }
            }
            busy.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// `accept` failing again and again: a pause that grows, so it does not spin, and after 50 in a
/// row the listening stops (`None`).
fn pause_after(errors: u32) -> Option<Duration> {
    (errors < 50).then(|| Duration::from_millis(100 * u64::from(errors.min(10))))
}

/// The address a connection sends, if it is this user's (Linux) and sends one line, not too long,
/// in time.
fn take_in(conn: &Stream) -> Option<String> {
    if !same_user_as_me(conn) {
        log!("single instance: a connection of another user refused");
        return None;
    }
    // Named pipes have no time limits (`READ`).
    let _ = conn.set_recv_timeout(Some(READ));
    read_address(conn)
}

/// One line of at most `LONGEST` bytes, without its end.
fn read_address(conn: impl Read) -> Option<String> {
    let mut line = Vec::new();
    BufReader::new(conn.take(LONGEST + 1)).read_until(b'\n', &mut line).ok()?;
    if line.pop() != Some(b'\n') {
        return None;
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    String::from_utf8(line).ok()
}

/// Linux: the other side of `conn` runs as this user (`SO_PEERCRED`, both ways: the copy listening
/// refuses other users, and a start does not hand its address to another user's name; D3).
#[cfg(unix)]
fn same_user_as_me(conn: &Stream) -> bool {
    // SAFETY: geteuid has no failure and no side effects.
    same_user(conn.peer_creds().ok().and_then(|c| c.euid()), unsafe { libc::geteuid() })
}

/// Windows: the named pipe is made with the default rights, which let other users only read it.
#[cfg(windows)]
fn same_user_as_me(_: &Stream) -> bool {
    true
}

#[cfg(unix)]
fn same_user(peer: Option<libc::uid_t>, me: libc::uid_t) -> bool {
    peer == Some(me)
}

impl App {
    /// The listening started in `main`, taken by the window.
    pub(crate) fn with_single(mut self, ctx: &Context, inbox: Option<Inbox>) -> Self {
        if let Some(inbox) = &inbox {
            let _ = inbox.ctx.set(ctx.clone());
            ctx.request_repaint();
        }
        self.single = inbox;
        self
    }

    /// Every pass, also with the window hidden (`App::logic`): the addresses handed over go to the
    /// window as `Msg::Open`.
    pub(crate) fn answer_handed(&mut self, ctx: &Context) {
        let Some(inbox) = &self.single else { return };
        for address in inbox.take() {
            let _ = self.tx.send(Msg::Open(address));
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::io::Write;
    use std::time::{Duration, Instant};

    use interprocess::local_socket::{Stream, prelude::*};

    use super::{Handover, Inbox, Start};

    /// A name of the test's own, never the user's copy (`socket`).
    fn test_socket(tag: &str) -> String {
        format!("eyecansee-test-{}-{tag}.sock", std::process::id())
    }

    /// The addresses handed over, once `n` have arrived; 10 s at most.
    fn arrived(inbox: &Inbox, n: usize) -> Vec<String> {
        let start = Instant::now();
        let mut all = Vec::new();
        while all.len() < n && start.elapsed() < Duration::from_secs(10) {
            all.extend(inbox.take());
            std::thread::sleep(Duration::from_millis(10));
        }
        all
    }

    /// Audit 1.10: a connection that sends nothing stopped the listening, and every later start
    /// handed its address to nobody.
    #[test]
    fn a_silent_connection_does_not_stop_the_next() {
        let socket = test_socket("silent");
        let inbox = super::listen_on(&socket).unwrap();
        let _silent = Stream::connect(super::ns_name(&socket).unwrap()).unwrap();
        assert_eq!(super::hand_over_to(&socket, "/next"), Handover::Done);
        assert_eq!(arrived(&inbox, 1), ["/next"]);
    }

    /// Audit 1.10: a connection was read with no limit; one longer than `LONGEST` is dropped,
    /// and the next one arrives.
    #[test]
    fn a_long_line_is_dropped() {
        let socket = test_socket("long");
        let inbox = super::listen_on(&socket).unwrap();
        let mut long = Stream::connect(super::ns_name(&socket).unwrap()).unwrap();
        // Dropped after `LONGEST`: the rest of the writing may fail.
        let _ = long.write_all(&vec![b'a'; 1 << 20]);
        let _ = long.write_all(b"\n");
        drop(long);
        assert_eq!(super::hand_over_to(&socket, "/next"), Handover::Done);
        let first = arrived(&inbox, 1);
        assert_eq!(first, ["/next"], "{} bytes arrived first", first.first().map_or(0, String::len));
    }

    /// Audit 1.10: starts at the same moment (a file manager starts one per file chosen) all
    /// missed each other and opened a window each; now one listens and the others hand over.
    #[test]
    fn starts_at_once_give_one_copy() {
        for round in 0..10 {
            let socket = test_socket(&format!("race-{round}"));
            let gate = std::sync::Arc::new(std::sync::Barrier::new(6));
            let starts: Vec<_> = (0..6)
                .map(|i| {
                    let (socket, gate) = (socket.clone(), gate.clone());
                    std::thread::spawn(move || {
                        gate.wait();
                        super::start_on(&socket, &format!("/{i}"))
                    })
                })
                .collect();
            let ends: Vec<Start> = starts.into_iter().map(|s| s.join().unwrap()).collect();
            let inboxes: Vec<&Inbox> = ends.iter().filter_map(|e| if let Start::Listening(i) = e { Some(i) } else { None }).collect();
            let handed = ends.iter().filter(|e| matches!(e, Start::HandedOver)).count();
            assert_eq!((inboxes.len(), handed), (1, 5), "round {round}: one copy listens, the others hand over");
            assert_eq!(arrived(inboxes[0], 5).len(), 5, "round {round}");
        }
    }

    /// One line, not too long, without its end; anything else is no address.
    #[test]
    fn address_read() {
        let read = |bytes: &[u8]| super::read_address(bytes);
        assert_eq!(read(b"/a b \n").as_deref(), Some("/a b "));
        assert_eq!(read(b"C:\\x\r\n").as_deref(), Some("C:\\x"));
        assert_eq!(read(b"\n").as_deref(), Some(""));
        assert_eq!(read(b"/cut"), None);
        assert_eq!(read(&[b'a'; super::LONGEST as usize + 1]), None);
        let mut longest = vec![b'a'; super::LONGEST as usize];
        longest.push(b'\n');
        assert_eq!(read(&longest).map(|a| a.len()), Some(super::LONGEST as usize));
    }

    /// `accept` failing again and again pauses, longer and longer, then stops.
    #[test]
    fn accept_errors_pause() {
        assert_eq!(super::pause_after(1), Some(Duration::from_millis(100)));
        assert_eq!(super::pause_after(30), Some(Duration::from_secs(1)));
        assert_eq!(super::pause_after(50), None);
    }

    /// D3: only the same user, never one whose credentials are unknown.
    #[cfg(unix)]
    #[test]
    fn same_user_only() {
        assert!(super::same_user(Some(1000), 1000));
        assert!(!super::same_user(Some(1001), 1000));
        assert!(!super::same_user(None, 1000));
    }

    #[test]
    fn multi_value() {
        assert!(!super::on(None));
        assert!(!super::on(Some(OsStr::new(""))));
        assert!(!super::on(Some(OsStr::new("0"))));
        assert!(super::on(Some(OsStr::new("1"))));
        assert!(super::on(Some(OsStr::new("yes"))));
    }
}
