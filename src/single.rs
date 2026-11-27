//! One EyeCanSee at a time: a phone can be held by one program only, so a second window could
//! not reach it. The first instance listens on a local socket (Linux: abstract Unix socket,
//! Windows: named pipe) from the start of `main`, before its window is made, so starts close
//! together find it (one process per file chosen in a file manager); a later one hands it the
//! address to open and exits once the window of that copy has taken it (#192). A copy whose
//! window does not take it in time does not count: the new start opens its own window.
//! `EYECANSEE_MULTI=1` turns this off for one process (see `multi`).
//!
//! The exchange: the start sends the address and a line end; the copy answers `wait` while its
//! window is not made yet (then the start waits longer), then `ok` once the window took it, `no`
//! if it did not in time, `again` if the copy is closing or busy (the start tries again). A copy
//! of 0.77.0 or before closes without a word once it read the address: taken, as then.

use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::Context;
#[cfg(unix)]
use interprocess::local_socket::ListenerNonblockingMode;
use interprocess::local_socket::{GenericNamespaced, Listener, ListenerOptions, Name, Stream, prelude::*};

use crate::App;
use crate::loader::Msg;

/// The longest address read: a connection that sends more is dropped (audit 1.10).
const LONGEST: u64 = 64 * 1024;
/// A connection that sends nothing is dropped after this. Named pipes have no time limits: there
/// it only keeps its own thread until the other side closes it.
const READ: Duration = Duration::from_secs(2);
/// Connections served at once; more are told `again`.
const AT_ONCE: usize = 64;
/// Hand-over and listening tried in turn: the name can be taken between the two, or a copy
/// closing lets it go.
const ATTEMPTS: usize = 10;
/// Before trying again after `again`.
const PAUSE: Duration = Duration::from_millis(100);
/// How long the copy waits for its window less than the start waits for the answer, so its `no`
/// arrives in time.
const MARGIN: Duration = Duration::from_millis(500);
/// How long the copy waits for the first answer and the ack of the window (`answer`), and for
/// the ack once it said `wait` (`starting`).
#[derive(Clone, Copy)]
struct Times {
    answer: Duration,
    starting: Duration,
}

/// D2 (#192): 2-3 s for a copy running. 10 s for one still making its window: not measured, to be
/// compared with a cold start on Windows.
const TIMES: Times = Times { answer: Duration::from_secs(3), starting: Duration::from_secs(10) };

const WAIT: &str = "wait";
const OK: &str = "ok";
const NO: &str = "no";
const AGAIN: &str = "again";

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
    /// A window of its own next to a copy that holds the name but did not take the address in
    /// time, or is another user's (Linux): that copy may be using its files.
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
    start_within(socket, address, TIMES)
}

fn start_within(socket: &str, address: &str, times: Times) -> Start {
    for attempt in 1..=ATTEMPTS {
        match hand_over_within(socket, address, times) {
            Handover::Done => return Start::HandedOver,
            Handover::Silent | Handover::Foreign => return Start::NextTo,
            Handover::Again => std::thread::sleep(PAUSE),
            Handover::Nobody => {}
        }
        match listen_within(socket, times) {
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
    /// The copy is closing, or busy: try again.
    Again,
    /// No answer in time (#192): the window of that copy does not work.
    Silent,
    /// The name is another user's (Linux): nothing is sent to it.
    Foreign,
}

/// What the thread of `deliver` says.
enum Step {
    /// The copy is making its window: the start waits `Times::starting`.
    Wait,
    End(Handover),
}

/// `address` given to the copy listening on `socket`, within `TIMES` (#192).
#[cfg(test)]
fn hand_over_to(socket: &str, address: &str) -> Handover {
    hand_over_within(socket, address, TIMES)
}

/// The whole exchange in a thread of its own, so connecting (on Windows with no time limit),
/// writing and reading never hold the start longer than `times`. Late, the thread sends nothing
/// more: this start has opened its own window.
fn hand_over_within(socket: &str, address: &str, times: Times) -> Handover {
    let (tx, rx) = mpsc::channel();
    let late = Arc::new(AtomicBool::new(false));
    let (socket, address, gave_up) = (socket.to_owned(), address.to_owned(), Arc::clone(&late));
    crate::logging::spawn_named("single-hand-over", move || deliver(&socket, &address, &gave_up, &tx));
    let (mut until, mut waited) = (Instant::now() + times.answer, false);
    loop {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            // Only the first `wait` gives the copy `starting`: a copy says it once, and one that
            // says it again and again must not hold this start for ever (audit A.1).
            Ok(Step::Wait) if !waited => (until, waited) = (Instant::now() + times.starting, true),
            Ok(Step::Wait) => {}
            Ok(Step::End(handover)) => return handover,
            Err(_) => {
                late.store(true, Ordering::SeqCst);
                log!("single instance: no answer in time");
                return Handover::Silent;
            }
        }
    }
}

fn deliver(socket: &str, address: &str, late: &AtomicBool, tx: &Sender<Step>) {
    let end = |handover| drop(tx.send(Step::End(handover)));
    let Some(Ok(conn)) = ns_name(socket).map(Stream::connect) else { return end(Handover::Nobody) };
    if !same_user_as_me(&conn) {
        log!("single instance: {socket} is another user's");
        return end(Handover::Foreign);
    }
    if late.load(Ordering::SeqCst) {
        return;
    }
    if (&conn).write_all(format!("{address}\n").as_bytes()).is_err() {
        return end(Handover::Again);
    }
    for line in BufReader::new((&conn).take(LONGEST)).lines() {
        match line.as_deref() {
            Ok(WAIT) => drop(tx.send(Step::Wait)),
            Ok(OK) => return end(Handover::Done),
            Ok(NO) => return end(Handover::Silent),
            // `again`, or the connection broken.
            _ => return end(Handover::Again),
        }
    }
    // Closed without a word: a copy of 0.77.0 or before, which took it as it read it.
    end(Handover::Done);
}

/// An address handed over, and where the window says it took it.
struct Handed {
    address: String,
    taken: SyncSender<()>,
}

/// What the listening threads and the window share.
struct Shared {
    /// The window, once made: woken when an address arrives.
    ctx: OnceLock<Context>,
    /// The window has taken the addresses once (`Inbox::take`): no more `wait` (D9).
    running: AtomicBool,
    /// Closing (`Inbox` dropped): the listening stops.
    closing: AtomicBool,
    busy: AtomicUsize,
    times: Times,
}

/// This copy listens: the addresses handed over wait here, each start with its connection open,
/// until the window takes them (`App::answer_handed`), also those that came before it was made.
/// Dropped, it stops listening first, so a copy still closing does not catch the next starts.
pub struct Inbox {
    rx: Receiver<Handed>,
    shared: Arc<Shared>,
    socket: String,
    /// The listening thread let the name go.
    stopped: Receiver<()>,
}

impl Inbox {
    /// The addresses waiting, each said taken to its start; those whose start no longer waits are
    /// left (it opened its own window).
    fn take(&self) -> Vec<String> {
        self.shared.running.store(true, Ordering::SeqCst);
        self.rx.try_iter().filter(|h| h.taken.send(()).is_ok()).map(|h| h.address).collect()
    }
}

impl Drop for Inbox {
    fn drop(&mut self) {
        self.shared.closing.store(true, Ordering::SeqCst);
        // `accept` is waiting: a connection of its own wakes it.
        let socket = self.socket.clone();
        crate::logging::spawn_named("single-stop", move || drop(ns_name(&socket).map(Stream::connect)));
        if self.stopped.recv_timeout(Duration::from_secs(1)).is_err() {
            log!("single instance: still listening after 1 s");
        }
    }
}

/// Listens on `socket`; `Err` when the name is taken (or cannot be made).
#[cfg(test)]
fn listen_on(socket: &str) -> std::io::Result<Inbox> {
    listen_within(socket, TIMES)
}

fn listen_within(socket: &str, times: Times) -> std::io::Result<Inbox> {
    let name = ns_name(socket).ok_or_else(|| std::io::Error::other("no name"))?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    let (tx, rx) = mpsc::channel();
    let (stop, stopped) = mpsc::channel();
    let shared = Arc::new(Shared { ctx: OnceLock::new(), running: AtomicBool::new(false), closing: AtomicBool::new(false), busy: AtomicUsize::new(0), times });
    let both = Arc::clone(&shared);
    crate::logging::spawn_named("single-listen", move || {
        accept_all(&listener, &tx, &both);
        drop(listener);
        let _ = stop.send(());
    });
    Ok(Inbox { rx, shared, socket: socket.to_owned(), stopped })
}

/// Each connection served by a thread of its own, so one that sends nothing stops no other.
fn accept_all(listener: &Listener, tx: &Sender<Handed>, shared: &Arc<Shared>) {
    let mut errors = 0;
    loop {
        let conn = listener.accept();
        if shared.closing.load(Ordering::SeqCst) {
            return let_go(listener, conn.ok());
        }
        let conn = match conn {
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
        if shared.busy.fetch_add(1, Ordering::SeqCst) >= AT_ONCE {
            shared.busy.fetch_sub(1, Ordering::SeqCst);
            say(&conn, AGAIN);
            continue;
        }
        let (tx, shared) = (tx.clone(), Arc::clone(shared));
        crate::logging::spawn_named("single-serve", move || {
            serve(&conn, &tx, &shared);
            shared.busy.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// Closing: the starts already connected are told `again` (left waiting, they would take the
/// close for a copy of 0.77.0 that took the address), so that they try again once the name is free.
/// Linux: also those still waiting to be accepted. Not on Windows, where a nonblocking `accept`
/// is not tried: a start that connected in that instant reads the close as taken [Unverified].
fn let_go(listener: &Listener, first: Option<Stream>) {
    first.iter().for_each(|conn| say(conn, AGAIN));
    #[cfg(unix)]
    if listener.set_nonblocking(ListenerNonblockingMode::Accept).is_ok() {
        for conn in std::iter::from_fn(|| listener.accept().ok()).take(AT_ONCE) {
            say(&conn, AGAIN);
        }
    }
    #[cfg(windows)]
    let _ = listener;
}

/// `accept` failing again and again: a pause that grows, so it does not spin, and after 50 in a
/// row the listening stops (`None`).
fn pause_after(errors: u32) -> Option<Duration> {
    (errors < 50).then(|| Duration::from_millis(100 * u64::from(errors.min(10))))
}

/// One start: its address to the window, and the window's answer back, in time (#192).
fn serve(conn: &Stream, tx: &Sender<Handed>, shared: &Shared) {
    let Some(address) = take_in(conn) else { return say(conn, NO) };
    // `wait` only before the window has taken anything (D9): a window stuck later gets no more time.
    let starting = !shared.running.load(Ordering::SeqCst);
    if starting {
        say(conn, WAIT);
    }
    let (taken, answer) = mpsc::sync_channel(1);
    if shared.closing.load(Ordering::SeqCst) || tx.send(Handed { address, taken }).is_err() {
        return say(conn, AGAIN);
    }
    if let Some(ctx) = shared.ctx.get() {
        ctx.request_repaint();
    }
    let times = shared.times;
    let wait = if starting { times.starting } else { times.answer };
    match answer.recv_timeout(wait.saturating_sub(MARGIN)) {
        Ok(()) => say(conn, OK),
        // The inbox dropped: closing.
        Err(RecvTimeoutError::Disconnected) => say(conn, AGAIN),
        Err(RecvTimeoutError::Timeout) => {
            log!("single instance: the window did not take an address in time");
            say(conn, NO);
        }
    }
}

/// One line of the answer; a start that no longer reads it (late, or of 0.77.0 or before) is no
/// matter.
fn say(conn: &Stream, line: &str) {
    let _ = writeln!(&*conn, "{line}");
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
    same_user(peer_cred(conn).as_ref(), unsafe { libc::geteuid() })
}

/// Windows: the named pipe is made with the default rights, which give other users read access
/// only (Microsoft's documentation of `CreateNamedPipe`).
#[cfg(windows)]
fn same_user_as_me(_: &Stream) -> bool {
    true
}

/// What the kernel says of the other side of `conn` (`SO_PEERCRED`), as it says it; `None` if it
/// cannot be read.
#[cfg(unix)]
fn peer_cred(conn: &Stream) -> Option<libc::ucred> {
    use std::os::fd::{AsFd, AsRawFd};
    let Stream::UdSocket(conn) = conn;
    // What an unconnected socket gives: nobody.
    let mut cred = libc::ucred { pid: 0, uid: libc::uid_t::MAX, gid: libc::gid_t::MAX };
    let size = size_of::<libc::ucred>() as libc::socklen_t;
    let mut len = size;
    // SAFETY: the descriptor is open as long as `conn` is borrowed; `cred` and `len` are valid
    // for writes, and `len` is the size of `cred`.
    let read = unsafe {
        libc::getsockopt(conn.as_fd().as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, (&raw mut cred).cast(), &raw mut len)
    };
    (read == 0 && len == size).then_some(cred)
}

/// D3: only the same user, never one whose credentials are unknown. The user alone decides: the
/// pid is 0 when the other side is in another PID namespace, as between two `flatpak run`, and the
/// kernel still gives its user (#361; `peer_creds` of `interprocess` gives an error then). With no
/// credentials at all the user is -1, and 65534 for one this user namespace cannot name: never
/// this user.
#[cfg(unix)]
fn same_user(peer: Option<&libc::ucred>, me: libc::uid_t) -> bool {
    peer.is_some_and(|c| c.uid == me)
}

impl App {
    /// The listening started in `main`, taken by the window.
    pub(crate) fn with_single(mut self, ctx: &Context, inbox: Option<Inbox>) -> Self {
        if let Some(inbox) = &inbox {
            let _ = inbox.shared.ctx.set(ctx.clone());
            ctx.request_repaint();
        }
        self.single = inbox;
        self
    }

    /// Every pass, also with the window hidden (`App::logic`): the addresses handed over go to the
    /// window as `Msg::Open`, and their starts are told so (#192).
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

    /// `hand_over_to` in a thread: it waits for the window, which the test plays (`arrived`).
    fn handing(socket: &str, address: &str) -> std::sync::mpsc::Receiver<Handover> {
        let (tx, rx) = std::sync::mpsc::channel();
        let (socket, address) = (socket.to_owned(), address.to_owned());
        std::thread::spawn(move || tx.send(super::hand_over_to(&socket, &address)));
        rx
    }

    /// Audit 1.10: a connection that sends nothing stopped the listening, and every later start
    /// handed its address to nobody.
    #[test]
    fn a_silent_connection_does_not_stop_the_next() {
        let socket = test_socket("silent");
        let inbox = super::listen_on(&socket).unwrap();
        let _silent = Stream::connect(super::ns_name(&socket).unwrap()).unwrap();
        let handed = handing(&socket, "/next");
        assert_eq!(arrived(&inbox, 1), ["/next"]);
        assert_eq!(handed.recv_timeout(Duration::from_secs(10)), Ok(Handover::Done));
    }

    /// Audit 1.10: a connection was read with no limit; one longer than `LONGEST` is dropped,
    /// and the next one arrives.
    #[test]
    fn a_long_line_is_dropped() {
        let socket = test_socket("long");
        let inbox = super::listen_on(&socket).unwrap();
        let mut long = Stream::connect(super::ns_name(&socket).unwrap()).unwrap();
        // Dropped after `LONGEST`. Only a little more than that is written, and the answer read:
        // on a named pipe a writer the copy no longer reads waits for ever, and the copy's side,
        // flushing its answer as it closes, waits for it to be read; the two held each other and
        // every pipe closed later in the process (seen in a Windows 11 VM, 22/11). A thread of its
        // own, so the next start is tried meanwhile.
        std::thread::spawn(move || {
            let _ = long.write_all(&vec![b'a'; super::LONGEST as usize + 16]);
            let _ = long.write_all(b"\n");
            let _ = std::io::copy(&mut long, &mut std::io::sink());
        });
        let handed = handing(&socket, "/next");
        let first = arrived(&inbox, 1);
        assert_eq!(first, ["/next"], "{} bytes arrived first", first.first().map_or(0, String::len));
        assert_eq!(handed.recv_timeout(Duration::from_secs(10)), Ok(Handover::Done));
    }

    /// Audit 1.10: starts at the same moment (a file manager starts one per file chosen) all
    /// missed each other and opened a window each; now one listens and the others hand over.
    /// The window of the one listening comes 0.3 s later: the others wait for it (D9).
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
                        match super::start_on(&socket, &format!("/{i}")) {
                            Start::Listening(inbox) => {
                                std::thread::sleep(Duration::from_millis(300));
                                Some(arrived(&inbox, 5))
                            }
                            Start::HandedOver => None,
                            _ => panic!("round {round}: start {i} opened a window of its own"),
                        }
                    })
                })
                .collect();
            let ends: Vec<_> = starts.into_iter().map(|s| s.join().unwrap()).collect();
            let windows: Vec<_> = ends.into_iter().flatten().collect();
            assert_eq!(windows.len(), 1, "round {round}: one copy listens, the others hand over");
            assert_eq!(windows[0].len(), 5, "round {round}");
        }
    }

    /// #192: a copy whose window does not take the address (stuck, or gone while the process
    /// lives on) took it all the same, and the new start showed nothing.
    #[test]
    fn a_copy_that_does_not_answer_is_not_handed_over() {
        let socket = test_socket("mute");
        let inbox = super::listen_on(&socket).unwrap();
        // Its window ran once, then got stuck: no more `wait` (D9).
        assert!(inbox.take().is_empty());
        let start = Instant::now();
        assert_ne!(super::hand_over_to(&socket, "/x"), Handover::Done);
        assert!(matches!(super::start_on(&socket, "/x"), Start::NextTo));
        assert!(start.elapsed() < Duration::from_secs(9), "{:?}", start.elapsed());
    }

    /// #192: a name nobody accepts on: the new start waited with no limit (Windows), or handed
    /// its address to nobody (Linux).
    #[test]
    fn a_name_nobody_accepts_on() {
        let socket = test_socket("deaf");
        let name = super::ns_name(&socket).unwrap();
        let _deaf = interprocess::local_socket::ListenerOptions::new().name(name).create_sync().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(super::hand_over_to(&socket, "/x")));
        let handed = rx.recv_timeout(Duration::from_secs(10));
        assert!(handed.as_ref().is_ok_and(|h| *h != Handover::Done), "{handed:?}");
    }

    /// #192: closing, the copy stops listening first, so the next start becomes the copy.
    #[test]
    fn closing_lets_the_name_go() {
        let socket = test_socket("closing");
        drop(super::listen_on(&socket).unwrap());
        assert!(matches!(super::start_on(&socket, "/y"), Start::Listening(_)));
    }

    /// A copy whose window takes the address: handed over.
    #[test]
    fn a_copy_that_answers_takes_it() {
        let socket = test_socket("answers");
        let inbox = super::listen_on(&socket).unwrap();
        let handed = handing(&socket, "/z");
        assert_eq!(arrived(&inbox, 1), ["/z"]);
        assert_eq!(handed.recv_timeout(Duration::from_secs(10)), Ok(Handover::Done));
    }

    /// D9: a copy that says `wait` and never `ok`: the start opens its own window in time.
    #[test]
    fn a_copy_that_only_says_wait() {
        let socket = test_socket("waits");
        let fake = interprocess::local_socket::ListenerOptions::new().name(super::ns_name(&socket).unwrap()).create_sync().unwrap();
        std::thread::spawn(move || {
            let conns: Vec<_> = fake.incoming().take(1).flatten().collect();
            for conn in &conns {
                assert!(super::read_address(conn).is_some());
                super::say(conn, super::WAIT);
            }
            std::thread::sleep(Duration::from_secs(30));
        });
        let times = super::Times { answer: Duration::from_millis(500), starting: Duration::from_secs(1) };
        let start = Instant::now();
        assert!(matches!(super::start_within(&socket, "/w", times), Start::NextTo));
        let took = start.elapsed();
        assert!(took >= Duration::from_secs(1) && took < Duration::from_secs(10), "{took:?}");
    }

    /// Audit A.1 (1): a copy that keeps saying `wait` does not make the start wait without end:
    /// after the first `wait` the start waits `starting` in all, the later ones add nothing.
    #[test]
    fn a_copy_that_says_wait_again_and_again() {
        let socket = test_socket("waits-again");
        let fake = interprocess::local_socket::ListenerOptions::new().name(super::ns_name(&socket).unwrap()).create_sync().unwrap();
        std::thread::spawn(move || {
            let conns: Vec<_> = fake.incoming().take(1).flatten().collect();
            for conn in &conns {
                assert!(super::read_address(conn).is_some());
                for _ in 0..25 {
                    super::say(conn, super::WAIT);
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            // Silent then, the connection still open: closed, it would read as a copy of 0.77.0.
            std::thread::sleep(Duration::from_secs(30));
        });
        let times = super::Times { answer: Duration::from_millis(500), starting: Duration::from_secs(1) };
        let start = Instant::now();
        assert!(matches!(super::start_within(&socket, "/w", times), Start::NextTo));
        let took = start.elapsed();
        assert!(took >= Duration::from_secs(1) && took < Duration::from_millis(2500), "{took:?}");
    }

    /// A copy of 0.77.0 or before reads the address and closes without a word: handed over, as
    /// then.
    #[test]
    fn a_copy_of_before_takes_it() {
        let socket = test_socket("old");
        let old = interprocess::local_socket::ListenerOptions::new().name(super::ns_name(&socket).unwrap()).create_sync().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for conn in old.incoming().take(1).flatten() {
                let _ = tx.send(super::read_address(&conn));
            }
        });
        assert_eq!(super::hand_over_to(&socket, "/old"), Handover::Done);
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(Some("/old".to_string())));
    }

    /// A copy closing while a start waits for its window: the start tries again, and becomes the
    /// copy once the name is free.
    #[test]
    fn a_start_waiting_on_a_copy_closing_becomes_the_copy() {
        let socket = test_socket("closes");
        let inbox = super::listen_on(&socket).unwrap();
        let s = socket.clone();
        let start = std::thread::spawn(move || super::start_on(&s, "/c"));
        let at = Instant::now();
        while inbox.shared.busy.load(std::sync::atomic::Ordering::SeqCst) == 0 && at.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(inbox);
        assert!(matches!(start.join().unwrap(), Start::Listening(_)));
    }

    /// The whole way in the app: the listening kept by the window (as `with_single` does), the address
    /// taken in `App::logic` (`answer_handed`), opened as `Msg::Open`, and the start told `ok`.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_window_takes_the_address() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("single::tests::the_window_takes_the_address") {
            return;
        }
        let other = folder("single-other", &["b.png"]);
        let mut h = Harness::new(&folder("single-first", &["a.png"]));
        let socket = test_socket("window");
        let inbox = super::listen_on(&socket).unwrap();
        let _ = inbox.shared.ctx.set(h.ctx.clone());
        h.app.single = Some(inbox);
        let target = format!("{}/", other.display());
        let handed = handing(&socket, &target);
        let start = Instant::now();
        let mut answer = None;
        while answer.is_none() && start.elapsed() < Duration::from_secs(10) {
            eframe::App::logic(&mut h.app, &h.ctx.clone(), &mut eframe::Frame::_new_kittest());
            h.frame(Vec::new());
            answer = handed.try_recv().ok();
        }
        assert_eq!(answer, Some(Handover::Done));
        h.until(|a| a.dir.trim_end_matches('/').ends_with("single-other"), "the folder handed over opened");
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

    #[cfg(unix)]
    fn cred(pid: libc::pid_t, uid: libc::uid_t) -> libc::ucred {
        libc::ucred { pid, uid, gid: uid }
    }

    /// D3: only the same user, never one whose credentials are unknown.
    #[cfg(unix)]
    #[test]
    fn same_user_only() {
        assert!(super::same_user(Some(&cred(4242, 1000)), 1000));
        assert!(!super::same_user(Some(&cred(4242, 1001)), 1000));
        assert!(!super::same_user(Some(&cred(4242, 0)), 1000));
        assert!(!super::same_user(None, 1000));
    }

    /// #361: the kernel gives a pid of 0 for a process in another PID namespace (each `flatpak run`
    /// has its own) and still its user: the user alone decides. Another user stays refused there
    /// too, and so does a socket with no credentials (pid 0, uid -1).
    #[cfg(unix)]
    #[test]
    fn same_user_in_another_pid_namespace() {
        assert!(super::same_user(Some(&cred(0, 1000)), 1000));
        assert!(!super::same_user(Some(&cred(0, 1001)), 1000));
        assert!(!super::same_user(Some(&cred(0, 0)), 1000));
        assert!(!super::same_user(Some(&cred(0, libc::uid_t::MAX)), 1000));
        // The user the kernel gives for one it cannot name in this user namespace.
        assert!(!super::same_user(Some(&cred(0, 65534)), 1000));
    }

    /// The credentials read on a real socket, on both sides: this process and this user.
    #[cfg(unix)]
    #[test]
    fn peer_cred_read() {
        let socket = test_socket("cred");
        let listener = interprocess::local_socket::ListenerOptions::new().name(super::ns_name(&socket).unwrap()).create_sync().unwrap();
        let start = Stream::connect(super::ns_name(&socket).unwrap()).unwrap();
        let copy = listener.accept().unwrap();
        // SAFETY: geteuid has no failure and no side effects.
        let me = unsafe { libc::geteuid() };
        for side in [&start, &copy] {
            let cred = super::peer_cred(side).unwrap();
            assert_eq!((cred.pid, cred.uid), (std::process::id() as libc::pid_t, me));
            assert!(super::same_user_as_me(side));
        }
    }

    /// The start of `in_another_pid_namespace`, run by it in a PID namespace of its own.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "run by a_start_in_another_pid_namespace_hands_over"]
    fn in_another_pid_namespace() {
        let Ok(socket) = std::env::var(NAMESPACE_SOCKET) else { return };
        assert_eq!(std::process::id(), 1, "not in a PID namespace of its own");
        assert_eq!(super::hand_over_to(&socket, "/sandbox"), Handover::Done);
    }

    #[cfg(target_os = "linux")]
    const NAMESPACE_SOCKET: &str = "EYECANSEE_TEST_NAMESPACE_SOCKET";

    /// #361 on real sockets: a start in another PID namespace (as a second `flatpak run`) sees a
    /// pid of 0 for the copy listening, and still hands its address over.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs unshare and user namespaces open to users: cargo test a_start_in_another_pid_namespace -- --ignored"]
    fn a_start_in_another_pid_namespace_hands_over() {
        let socket = test_socket("namespace");
        let inbox = super::listen_on(&socket).unwrap();
        // SAFETY: geteuid has no failure and no side effects.
        let me = unsafe { libc::geteuid() };
        let mut start = std::process::Command::new("unshare")
            .args(["--user", &format!("--map-user={me}"), "--pid", "--fork"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "single::tests::in_another_pid_namespace", "--ignored", "--nocapture"])
            .env(NAMESPACE_SOCKET, &socket)
            .spawn()
            .unwrap();
        assert_eq!(arrived(&inbox, 1), ["/sandbox"]);
        assert!(start.wait().unwrap().success());
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
