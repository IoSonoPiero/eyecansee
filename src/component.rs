//! Add-ons downloaded the first time they are needed, not shipped in the package (decided by the
//! user on 2026-10-24): today the video engine, libmpv, which carries patented decoders (HEVC).
//! The file comes from a release of ours ("components"); its SHA-256 is written here, in the
//! signed executable, so a different file is refused.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
pub struct Component {
    /// Name of the file once in `dir()`.
    pub file: &'static str,
    /// gzip-compressed file.
    pub url: &'static str,
    /// SHA-256 of what is downloaded (the .gz).
    pub sha256: &'static str,
    /// Bytes downloaded.
    pub size: u64,
}

/// libmpv for Windows: `libmpv-2.dll` of mpv-winbuild-cmake (shinchiro) 20261021, mpv git
/// e471496c56, self-contained (121 MB, 47 MB compressed).
#[cfg(windows)]
pub const VIDEO: Option<Component> = Some(Component {
    file: "libmpv-2.dll",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/libmpv-2-windows-x64-20261021.dll.gz",
    sha256: "f9cd5b1783024cb09bb5feb713166fdf822f3dee87d6adf8ad2aa52a29405c12",
    size: 46_998_894,
});

#[cfg(not(windows))]
pub const VIDEO: Option<Component> = None;

/// Where components go: the app's data folder (`%LOCALAPPDATA%\eyecansee\components` on Windows,
/// `~/.local/share/eyecansee/components` on Linux).
pub fn dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::home_dir().map(|h| h.join(".local/share")));
    Some(base?.join("eyecansee").join("components"))
}

pub fn path(c: &Component) -> Option<PathBuf> {
    Some(dir()?.join(c.file))
}

/// Downloads, checks and unpacks `c`; `done` counts the bytes downloaded. Nothing is left in place
/// of the file unless it is complete and its SHA-256 matches.
pub fn install(c: &Component, done: &AtomicU64) -> Result<(), String> {
    let target = path(c).ok_or("no data folder")?;
    std::fs::create_dir_all(target.parent().ok_or("no data folder")?).map_err(|e| e.to_string())?;
    let part = target.with_extension("part");
    let r = (|| {
        let body = crate::update::agent().get(c.url).call().map_err(|e| e.to_string())?.into_body().into_reader();
        let mut hashed = Hashed { inner: body, ctx: ring::digest::Context::new(&ring::digest::SHA256), done };
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        std::io::copy(&mut flate2::read::GzDecoder::new(&mut hashed), &mut out).map_err(|e| e.to_string())?;
        // Whatever follows the gzip stream counts for the hash too.
        std::io::copy(&mut hashed, &mut std::io::sink()).map_err(|e| e.to_string())?;
        let got: String = hashed.ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
        if got != c.sha256 {
            return Err(format!("SHA-256 mismatch: {got}, expected {}", c.sha256));
        }
        drop(out);
        std::fs::rename(&part, &target).map_err(|e| e.to_string())
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    r
}

/// A download under way, followed by the interface.
#[derive(Default)]
pub struct Download {
    pub done: AtomicU64,
    /// Set at the end.
    pub result: std::sync::Mutex<Option<Result<(), String>>>,
}

/// `install` in a thread of its own; `repaint` when it ends.
pub fn start(c: Component, repaint: impl Fn() + Send + 'static) -> std::sync::Arc<Download> {
    let d = std::sync::Arc::new(Download::default());
    let d2 = d.clone();
    std::thread::spawn(move || {
        let r = install(&c, &d2.done);
        *d2.result.lock().unwrap() = Some(r);
        repaint();
    });
    d
}

/// The download, hashed and counted as it is read.
struct Hashed<'a, R> {
    inner: R,
    ctx: ring::digest::Context,
    done: &'a AtomicU64,
}

impl<R: Read> Read for Hashed<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.ctx.update(&buf[..n]);
        self.done.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test component_live -- --ignored --nocapture` with `LOCALAPPDATA`/`XDG_DATA_HOME` on
    /// a temporary folder: downloads the real component.
    #[test]
    #[ignore]
    fn component_live() {
        let Some(c) = VIDEO else { return };
        let done = AtomicU64::new(0);
        let t = std::time::Instant::now();
        install(&c, &done).unwrap();
        println!("{} bytes in {:?} to {:?}", done.load(Ordering::Relaxed), t.elapsed(), path(&c));
        assert_eq!(done.load(Ordering::Relaxed), c.size);
        assert!(crate::mpv::available());
    }
}
