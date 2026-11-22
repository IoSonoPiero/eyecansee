//! The diagnostic log written by the `log!` macro (defined in `main.rs`, before the modules):
//! to stderr with `EYECANSEE_LOG=1`, appended to a file with `EYECANSEE_LOG=<file>`; without the
//! variable, with Options > Diagnostics > "Write a diagnostic log" on (#189), to a file of its own
//! for each start in the data folder (`folder`), a new one past `FILE_LIMIT`, at most
//! `KEPT_FILES` of them. The variable wins over the switch. Where it goes is chosen once, at the
//! start of `main` (`start`), so a change of the switch counts from the next start: the first
//! lines are written before the window exists, and only a log from the start has the version, the
//! system and the graphics card. With the log on, a panic in any thread is written there too
//! (`install_panic_hook`).
//! Nothing here may write to the log or panic: the panic hook writes through it.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// A log file of the switch past this size: the next line begins a new one.
pub(crate) const FILE_LIMIT: u64 = 5 << 20;
/// Log files of the switch kept in the folder, the one being written included: the oldest go.
pub(crate) const KEPT_FILES: usize = 10;
/// Lines kept in memory until the start knows whether it keeps its window (`keep`): more are dropped.
const WAITING_LIMIT: usize = 1 << 20;
/// The folder of the logs of the switch, in the app's data folder.
const FOLDER: &str = "logs";

/// Where the log goes.
#[derive(Debug, PartialEq)]
pub(crate) enum Target {
    Off,
    Stderr,
    /// `EYECANSEE_LOG=<file>`: appended to that file.
    File(PathBuf),
    /// The switch: a file of its own for each start in that folder.
    Folder(PathBuf),
}

/// The variable first (`1` or empty: stderr, anything else a file), then the switch, which writes
/// in `folder` (none known: no log).
pub(crate) fn target(var: Option<&OsStr>, switch: bool, folder: Option<PathBuf>) -> Target {
    match var {
        Some(v) if v.is_empty() || v == "1" => Target::Stderr,
        Some(v) => Target::File(v.into()),
        None if switch => folder.map_or(Target::Off, Target::Folder),
        None => Target::Off,
    }
}

fn variable() -> Option<OsString> {
    std::env::var_os("EYECANSEE_LOG")
}

/// The log of this process.
struct Sink {
    target: Target,
    out: Mutex<Out>,
}

enum Out {
    Off,
    Stderr,
    File(File),
    Folder(Rolling),
}

impl Sink {
    fn new(target: Target) -> Self {
        let out = match &target {
            Target::Off => Out::Off,
            Target::Stderr => Out::Stderr,
            // As up to 0.77.0: a file that cannot be opened leaves the log on stderr.
            Target::File(to) => std::fs::OpenOptions::new().create(true).append(true).open(to).map_or(Out::Stderr, Out::File),
            Target::Folder(folder) => Out::Folder(Rolling::new(folder.clone(), stamp)),
        };
        Self { target, out: Mutex::new(out) }
    }

    fn out(&self) -> std::sync::MutexGuard<'_, Out> {
        self.out.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static SINK: OnceLock<Sink> = OnceLock::new();

/// Without `start` (the tests), from the variable alone, as up to 0.77.0.
fn sink() -> &'static Sink {
    SINK.get_or_init(|| Sink::new(target(variable().as_deref(), false, None)))
}

/// At the start of `main`, before anything is logged: the switch of Options read from the settings
/// (`settings::log_switch`). The lines of the switch's log wait in memory until `keep`.
pub(crate) fn start(switch: bool) {
    let _ = SINK.set(Sink::new(target(variable().as_deref(), switch, folder())));
}

/// This start keeps its window (`single::start` did not hand its address to a copy running):
/// the log of the switch gets its file, with the lines written so far. A start that hands its
/// address over and exits never calls it, and makes no file.
pub(crate) fn keep() {
    if let Out::Folder(r) = &mut *sink().out() {
        r.keep();
    }
}

/// The folder of the logs of the switch: `logs` in the app's data folder (next to the components).
pub(crate) fn folder() -> Option<PathBuf> {
    Some(crate::component::dir()?.parent()?.join(FOLDER))
}

/// `EYECANSEE_LOG` is set: it decides the log, the switch is not read.
pub(crate) fn by_variable() -> bool {
    variable().is_some()
}

/// The file this start writes its log in, when the switch's.
pub(crate) fn file_now() -> Option<PathBuf> {
    match &*sink().out() {
        Out::Folder(r) => r.file.as_ref().map(|(_, p)| p.clone()),
        _ => None,
    }
}

pub(crate) fn log_line(text: &str) {
    let line = format!("[{:8.3}] {text}\n", log_time());
    // One write per line, and nothing here may panic: the panic hook writes through this too.
    let mut out = sink().out();
    let _ = match &mut *out {
        Out::Off => Ok(()),
        Out::File(f) => f.write_all(line.as_bytes()),
        Out::Stderr => std::io::stderr().write_all(line.as_bytes()),
        Out::Folder(r) => r.write(line.as_bytes()),
    };
}

pub(crate) fn log_enabled() -> bool {
    sink().target != Target::Off
}

fn log_time() -> f64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

/// The local date and time in the name of a log file, `2026-11-22_143005`: sorted by name, the
/// files are in the order they were made.
fn stamp() -> String {
    let t = jiff::Zoned::now().datetime();
    format!("{:04}-{:02}-{:02}_{:02}{:02}{:02}", t.year(), t.month(), t.day(), t.hour(), t.minute(), t.second())
}

/// The name of the `n`th file (from 1) begun in the second `stamp`: `eyecansee-<stamp>.log`, then
/// `eyecansee-<stamp>_02.log` (after it by name: `_` comes after `.`). The test copy's begin with
/// its own name.
fn file_name(stamp: &str, n: u32) -> String {
    let name = crate::test_copy::NAME;
    if n <= 1 { format!("{name}-{stamp}.log") } else { format!("{name}-{stamp}_{n:02}.log") }
}

/// A name `file_name` makes: only those files are ever deleted from the folder.
fn is_log_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(crate::test_copy::NAME).and_then(|r| r.strip_prefix('-')).and_then(|r| r.strip_suffix(".log")) else { return false };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if rest.len() < 17 || !rest.is_char_boundary(17) {
        return false;
    }
    let (stamp, n) = rest.split_at(17);
    let stamp_ok = stamp.as_bytes()[10] == b'_' && stamp.split(['-', '_']).all(digits);
    stamp_ok && (n.is_empty() || n.strip_prefix('_').is_some_and(digits))
}

/// The log files of the switch in `folder` past the newest `keep` deleted (what cannot be
/// deleted stays).
fn prune(folder: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let mut names: Vec<String> = entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| is_log_name(n)).collect();
    names.sort();
    for old in &names[..names.len().saturating_sub(keep)] {
        let _ = std::fs::remove_file(folder.join(old));
    }
}

/// The log of the switch: a file of its own for each start, a new one past `FILE_LIMIT`.
struct Rolling {
    folder: PathBuf,
    /// Until `keep`: the lines written so far.
    waiting: Option<Vec<u8>>,
    file: Option<(File, PathBuf)>,
    written: u64,
    /// No file could be made: the lines go to stderr.
    failed: bool,
    stamp: fn() -> String,
}

impl Rolling {
    fn new(folder: PathBuf, stamp: fn() -> String) -> Self {
        Self { folder, waiting: Some(Vec::new()), file: None, written: 0, failed: false, stamp }
    }

    fn write(&mut self, line: &[u8]) -> std::io::Result<()> {
        if let Some(waiting) = &mut self.waiting {
            if waiting.len() + line.len() <= WAITING_LIMIT {
                waiting.extend_from_slice(line);
            }
            return Ok(());
        }
        if self.file.is_none() && !self.failed || self.written > 0 && self.written + line.len() as u64 > FILE_LIMIT {
            self.begin();
        }
        let r = match &mut self.file {
            Some((f, _)) => f.write_all(line),
            None => std::io::stderr().write_all(line),
        };
        self.written += line.len() as u64;
        r
    }

    /// The lines waiting go into the first file.
    fn keep(&mut self) {
        if let Some(waiting) = self.waiting.take() {
            self.begin();
            let _ = self.write(&waiting);
        }
    }

    /// A new file, never over another one; then the oldest deleted. Once failed, not tried again.
    fn begin(&mut self) {
        self.written = 0;
        if self.failed {
            return;
        }
        self.file = new_file(&self.folder, &(self.stamp)());
        self.failed = self.file.is_none();
        prune(&self.folder, KEPT_FILES);
    }
}

fn new_file(folder: &Path, stamp: &str) -> Option<(File, PathBuf)> {
    std::fs::create_dir_all(folder).ok()?;
    (1..100).find_map(|n| {
        let path = folder.join(file_name(stamp, n));
        std::fs::OpenOptions::new().write(true).create_new(true).open(&path).ok().map(|f| (f, path))
    })
}

/// With the log on, a panic in any thread also goes to the log (on Windows the release build has
/// no stderr, so a log file is the only place it can be read), then to the hook that was there,
/// so what happens next does not change (a `catch_unwind` still catches it). The lines of the
/// switch's log still waiting go into their file first: a panic before the window is made would
/// leave nothing.
pub(crate) fn install_panic_hook() {
    if !log_enabled() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        keep();
        log_line(&panic_text(info));
        previous(info);
    }));
}

/// A thread with a name, so that a panic in it says in the log which work it was doing. Like
/// `std::thread::spawn`, it panics if the system cannot start the thread.
pub(crate) fn spawn_named<F, T>(name: &str, f: F) -> std::thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new().name(name.into()).spawn(f).expect("spawn")
}

/// Message, thread, place in the source and the backtrace, always: a report sent with the log
/// must say where the panic came from, also when nobody set `RUST_BACKTRACE` (Piero, 20/11/2026).
/// The hook runs only with the log on, so the cost of the capture is paid only then.
fn panic_text(info: &std::panic::PanicHookInfo) -> String {
    let message = info.payload_as_str().unwrap_or("(no message)");
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("<unnamed>");
    let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
    let mut text = format!("panic in thread '{name}' at {place}: {message}");
    text.push_str(&format!("\n{}", std::backtrace::Backtrace::force_capture()));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set by the test below on the copy of the test program it starts, never by hand.
    const CHILD: &str = "EYECANSEE_TEST_PANIC_CHILD";

    /// Audit 7.4: with `EYECANSEE_LOG=<file>` a panic in a working thread must leave its message,
    /// thread and place in that file (on Windows the release build has no stderr at all). The hook
    /// is global to the process and the log is read from the environment once, so both happen in
    /// a copy of this test program started with its own environment, not in the one running the
    /// other tests.
    #[test]
    fn audit_7_4_panic_in_the_log() {
        if std::env::var_os(CHILD).is_some() {
            super::install_panic_hook();
            super::log_line("before");
            let worker = std::thread::Builder::new().name("worker".into());
            let _ = worker.spawn(|| panic!("the picture broke")).unwrap().join();
            super::log_line("after");
            return;
        }
        let file = std::env::temp_dir().join(format!("eyecansee-audit-7-4-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let name = format!("{}::audit_7_4_panic_in_the_log", module_path!().split_once("::").unwrap().1);
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .env("EYECANSEE_LOG", &file)
            .env_remove("RUST_BACKTRACE")
            .env_remove("RUST_LIB_BACKTRACE")
            .output()
            .unwrap();
        let log = std::fs::read_to_string(&file).unwrap_or_default();
        let _ = std::fs::remove_file(&file);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && stdout.contains("1 passed"), "the copy of the test did not run:\n{stdout}");
        assert!(log.contains("before") && log.contains("after"), "the log was not written:\n{log}");
        assert!(log.contains("the picture broke"), "the panic is not in the log:\n{log}");
        assert!(log.contains("'worker'") && log.contains("logging.rs:"), "thread or place missing:\n{log}");
        // The backtrace also without RUST_BACKTRACE: its frames are numbered "  0: ".
        assert!(log.contains("  0: "), "no backtrace in the log:\n{log}");
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-logging-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).map(|e| e.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()).unwrap_or_default();
        names.sort();
        names
    }

    fn fixed() -> String {
        "2026-11-22_143005".into()
    }

    /// #189: the variable wins over the switch, `1` or empty on stderr, anything else a file; the
    /// switch alone writes in the folder; neither, no log.
    #[test]
    fn the_variable_wins_over_the_switch() {
        let folder = || Some(PathBuf::from("data/logs"));
        for switch in [false, true] {
            assert_eq!(target(Some(OsStr::new("1")), switch, folder()), Target::Stderr);
            assert_eq!(target(Some(OsStr::new("")), switch, folder()), Target::Stderr);
            assert_eq!(target(Some(OsStr::new("/tmp/a.log")), switch, folder()), Target::File("/tmp/a.log".into()));
        }
        assert_eq!(target(None, true, folder()), Target::Folder("data/logs".into()));
        assert_eq!(target(None, true, None), Target::Off);
        assert_eq!(target(None, false, folder()), Target::Off);
    }

    /// #189: the file of a start is named by its date and time, after the program; a second one in
    /// the same second does not take its place, and comes after it by name.
    #[test]
    fn a_file_named_by_date_and_time() {
        let n = crate::test_copy::NAME;
        assert_eq!(file_name("2026-11-22_143005", 1), format!("{n}-2026-11-22_143005.log"));
        assert_eq!(file_name("2026-11-22_143005", 2), format!("{n}-2026-11-22_143005_02.log"));
        assert!(file_name("2026-11-22_143005", 1) < file_name("2026-11-22_143005", 2));
        assert!(file_name("2026-11-22_143005", 12) < file_name("2026-11-22_143006", 1));
        let s = stamp();
        assert!(is_log_name(&file_name(&s, 1)) && is_log_name(&file_name(&s, 3)), "{s}");
        for other in ["notes.log".to_string(), format!("{n}-2026-11-22_143005.txt"), format!("{n}-x-2026-11-22_143005.log"), format!("{n}-2026-11-22.log"), format!("{n}.log")] {
            assert!(!is_log_name(&other), "{other}");
        }
        let dir = test_dir("names");
        let first = new_file(&dir, "2026-11-22_143005").unwrap().1;
        let second = new_file(&dir, "2026-11-22_143005").unwrap().1;
        assert_ne!(first, second);
        assert_eq!(files(&dir), [file_name("2026-11-22_143005", 1), file_name("2026-11-22_143005", 2)]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #189: the lines written before the start knows it keeps its window wait in memory: a start
    /// that hands its address over makes no file; one that keeps the window has them in its file.
    #[test]
    fn a_start_that_hands_over_makes_no_file() {
        let dir = test_dir("hand-over");
        let mut r = Rolling::new(dir.clone(), fixed);
        r.write(b"started\n").unwrap();
        drop(r);
        assert!(!dir.exists(), "a file for a start that exits");
        let mut r = Rolling::new(dir.clone(), fixed);
        r.write(b"started\n").unwrap();
        r.keep();
        r.write(b"window\n").unwrap();
        drop(r);
        let names = files(&dir);
        assert_eq!(names, [file_name(&fixed(), 1)]);
        assert_eq!(std::fs::read_to_string(dir.join(&names[0])).unwrap(), "started\nwindow\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #189: past `FILE_LIMIT` the next line begins a new file; with more than `KEPT_FILES` the
    /// oldest go, and files of another name stay.
    #[test]
    fn a_new_file_past_the_limit_and_ten_kept() {
        let dir = test_dir("rotation");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "mine").unwrap();
        for i in 0..KEPT_FILES {
            std::fs::write(dir.join(file_name(&format!("2026-11-{:02}_120000", i + 1), 1)), "old").unwrap();
        }
        let mut r = Rolling::new(dir.clone(), fixed);
        r.keep();
        let line = vec![b'x'; 1 << 20];
        for _ in 0..5 {
            r.write(&line).unwrap();
        }
        assert_eq!(r.file.as_ref().unwrap().1, dir.join(file_name(&fixed(), 1)), "5 MB fit in the first file");
        r.write(b"one more\n").unwrap();
        assert_eq!(r.file.as_ref().unwrap().1, dir.join(file_name(&fixed(), 2)));
        drop(r);
        let names = files(&dir);
        assert_eq!(names.len(), KEPT_FILES + 1, "{names:?}");
        assert!(names.contains(&"notes.txt".to_string()));
        assert!(!names.contains(&file_name("2026-11-01_120000", 1)) && !names.contains(&file_name("2026-11-02_120000", 1)), "{names:?}");
        assert!(names.contains(&file_name("2026-11-03_120000", 1)), "{names:?}");
        assert_eq!(std::fs::metadata(dir.join(file_name(&fixed(), 1))).unwrap().len(), 5 << 20);
        assert_eq!(std::fs::read_to_string(dir.join(file_name(&fixed(), 2))).unwrap(), "one more\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Set by the test below on the copies of the test program it starts.
    const SWITCH_CHILD: &str = "EYECANSEE_TEST_LOG_SWITCH_CHILD";

    /// #189, as `main` does it, in copies of this test program with settings and data folders of
    /// their own: with the switch on in settings.json, a start that keeps its window writes its
    /// lines, those before `keep` too, in a file of the folder of the logs; one that hands its
    /// address over makes none; with `EYECANSEE_LOG` set the variable wins.
    #[test]
    fn the_switch_from_settings_to_the_file() {
        if let Some(how) = std::env::var_os(SWITCH_CHILD) {
            start(crate::settings::log_switch());
            log_line("started");
            if how == "keep" {
                keep();
            }
            log_line("window");
            return;
        }
        let dir = test_dir("switch");
        let (config, data) = (dir.join("config"), dir.join("data"));
        let settings = crate::test_copy::NAME;
        for d in [config.join(settings), data.clone()] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(config.join(settings).join("settings.json"), r#"{"diagnostic_log": true}"#).unwrap();
        let name = format!("{}::the_switch_from_settings_to_the_file", module_path!().split_once("::").unwrap().1);
        let run = |how: &str, var: Option<&Path>| {
            let mut c = std::process::Command::new(std::env::current_exe().unwrap());
            c.args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"]).env(SWITCH_CHILD, how);
            c.env("XDG_CONFIG_HOME", &config).env("XDG_DATA_HOME", &data).env("APPDATA", &config).env("LOCALAPPDATA", &data).env_remove("FLATPAK_ID");
            match var {
                Some(v) => c.env("EYECANSEE_LOG", v),
                None => c.env_remove("EYECANSEE_LOG"),
            };
            let out = c.output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
        };
        let logs = data.join(settings).join(FOLDER);
        run("hand-over", None);
        assert!(files(&logs).is_empty(), "a file for a start that handed over: {:?}", files(&logs));
        run("keep", None);
        let names = files(&logs);
        assert!(names.len() == 1 && is_log_name(&names[0]), "{names:?}");
        let text = std::fs::read_to_string(logs.join(&names[0])).unwrap();
        assert!(text.contains("] started\n") && text.contains("] window\n"), "{text}");
        let other = dir.join("by-variable.log");
        run("keep", Some(&other));
        assert_eq!(files(&logs).len(), 1, "the variable wins");
        assert!(std::fs::read_to_string(&other).unwrap().contains("] window\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The folder of the logs is next to the components, in the app's data folder.
    #[test]
    fn the_folder_is_in_the_data_folder() {
        if let (Some(logs), Some(components)) = (folder(), crate::component::dir()) {
            assert_eq!(logs.parent(), components.parent());
            assert!(logs.ends_with(FOLDER));
        }
    }
}
