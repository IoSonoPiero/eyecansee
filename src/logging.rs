//! The diagnostic log written by the `log!` macro (defined in `main.rs`, before the modules):
//! to stderr with `EYECANSEE_LOG=1`, appended to a file with `EYECANSEE_LOG=<file>`.
//! With the log on, a panic in any thread is written there too (`install_panic_hook`).

use std::io::Write;

pub(crate) fn log_line(text: &str) {
    static FILE: std::sync::OnceLock<Option<std::sync::Mutex<std::fs::File>>> = std::sync::OnceLock::new();
    let file = FILE.get_or_init(|| {
        let to = std::env::var_os("EYECANSEE_LOG").filter(|v| v != "1")?;
        std::fs::OpenOptions::new().create(true).append(true).open(to).ok().map(std::sync::Mutex::new)
    });
    let line = format!("[{:8.3}] {text}\n", log_time());
    // One write per line, and nothing here may panic: the panic hook writes through this too.
    let _ = match file {
        Some(f) => f.lock().unwrap_or_else(|e| e.into_inner()).write_all(line.as_bytes()),
        None => std::io::stderr().write_all(line.as_bytes()),
    };
}

pub(crate) fn log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("EYECANSEE_LOG").is_some())
}

fn log_time() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

/// With the log on, a panic in any thread also goes to the log (on Windows the release build has
/// no stderr, so a log file is the only place it can be read), then to the hook that was there,
/// so what happens next does not change (a `catch_unwind` still catches it).
pub(crate) fn install_panic_hook() {
    if !log_enabled() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
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

/// Message, thread, place in the source and, when `RUST_BACKTRACE` asks for it, the backtrace.
fn panic_text(info: &std::panic::PanicHookInfo) -> String {
    let message = info.payload_as_str().unwrap_or("(no message)");
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("<unnamed>");
    let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
    let mut text = format!("panic in thread '{name}' at {place}: {message}");
    let backtrace = std::backtrace::Backtrace::capture();
    if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
        text.push_str(&format!("\n{backtrace}"));
    }
    text
}

#[cfg(test)]
mod tests {
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
            .output()
            .unwrap();
        let log = std::fs::read_to_string(&file).unwrap_or_default();
        let _ = std::fs::remove_file(&file);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success() && stdout.contains("1 passed"), "the copy of the test did not run:\n{stdout}");
        assert!(log.contains("before") && log.contains("after"), "the log was not written:\n{log}");
        assert!(log.contains("the picture broke"), "the panic is not in the log:\n{log}");
        assert!(log.contains("'worker'") && log.contains("logging.rs:"), "thread or place missing:\n{log}");
    }
}
