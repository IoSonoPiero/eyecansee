//! The diagnostic log written by the `log!` macro (defined in `main.rs`, before the modules):
//! to stderr with `EYECANSEE_LOG=1`, appended to a file with `EYECANSEE_LOG=<file>`.

pub(crate) fn log_line(text: &str) {
    use std::io::Write;
    static FILE: std::sync::OnceLock<Option<std::sync::Mutex<std::fs::File>>> = std::sync::OnceLock::new();
    let file = FILE.get_or_init(|| {
        let to = std::env::var_os("EYECANSEE_LOG").filter(|v| v != "1")?;
        std::fs::OpenOptions::new().create(true).append(true).open(to).ok().map(std::sync::Mutex::new)
    });
    let line = format!("[{:8.3}] {text}\n", log_time());
    match file.as_ref().and_then(|f| f.lock().ok()) {
        Some(mut f) => {
            let _ = f.write_all(line.as_bytes());
        }
        None => eprint!("{line}"),
    }
}

pub(crate) fn log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("EYECANSEE_LOG").is_some())
}

fn log_time() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}
