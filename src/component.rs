//! Add-ons downloaded the first time they are needed, not shipped in the package (decided by the
//! user on 2026-10-24): the video engine, libmpv, which carries patented decoders (HEVC), and
//! FFmpeg, which converts videos.
//! The file comes from a release of ours ("components"); its SHA-256 is written here, in the
//! signed executable, so a different file is refused.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
pub struct Component {
    /// What it is and which version: it goes in `dir()/<kind>-<version>/`, so a new EyeCanSee that
    /// wants another version asks for it, and the old ones are deleted once it is there.
    pub kind: &'static str,
    pub version: &'static str,
    /// Name of the file in its folder (the library to load).
    pub file: &'static str,
    /// gzip-compressed: the file itself, or with `archive` a tar of it and the libraries it needs.
    pub url: &'static str,
    pub archive: bool,
    /// SHA-256 of what is downloaded (the .gz).
    pub sha256: &'static str,
    /// Bytes downloaded.
    pub size: u64,
}

/// libmpv for Windows: `libmpv-2.dll` of mpv-winbuild-cmake (shinchiro) 20261021, mpv git
/// e471496c56, self-contained (121 MB, 47 MB compressed).
#[cfg(windows)]
pub const VIDEO: Option<Component> = Some(Component {
    kind: "libmpv",
    version: "20261021",
    file: "libmpv-2.dll",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/libmpv-2-windows-x64-20261021.dll.gz",
    archive: false,
    sha256: "f9cd5b1783024cb09bb5feb713166fdf822f3dee87d6adf8ad2aa52a29405c12",
    size: 46_998_894,
});

/// libmpv for Linux, used when the system has none (the AppImage on a live system):
/// `tools/build-mpv-linux.sh`, mpv 0.35.1 with FFmpeg 6.0.1 and dav1d inside, built on Ubuntu
/// 22.04 (glibc 2.35), with libass and three libraries it needs (24 MB, 10 MB compressed).
#[cfg(not(windows))]
pub const VIDEO: Option<Component> = Some(Component {
    kind: "libmpv",
    version: "0.35.1",
    file: "libmpv.so.2",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/libmpv-linux-x64-0.35.1.tar.gz",
    archive: true,
    sha256: "4639f8fa245dcab04bbc02bd19148f53cce3129e9e5c2312469f605868e52a8e",
    size: 10_266_355,
});

/// FFmpeg 9.0 of BtbN/FFmpeg-Builds (`ffmpeg-n9.0-latest-*-gpl-9.0`, built 2026-10-26), only the
/// `ffmpeg` program and its licence: converts videos (`ffmpeg.rs`). Used when the system has none.
#[cfg(windows)]
pub const FFMPEG: Option<Component> = Some(Component {
    kind: "ffmpeg",
    version: "9.0-20261026",
    file: "ffmpeg.exe",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/ffmpeg-windows-x64-9.0-20261026.tar.gz",
    archive: true,
    sha256: "45e0fcc058ada98efa54abd1fa08e13ab3398b81003bbd39803c55b05998fcb2",
    size: 63_690_885,
});

#[cfg(not(windows))]
pub const FFMPEG: Option<Component> = Some(Component {
    kind: "ffmpeg",
    version: "9.0-20261026",
    file: "ffmpeg",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/ffmpeg-linux-x64-9.0-20261026.tar.gz",
    archive: true,
    sha256: "869db08c1b8220c382759b0dd966667d2885462a7fbbf4913cd45e3f20e695b9",
    size: 67_805_579,
});

/// Where components go: the app's data folder (`%LOCALAPPDATA%\eyecansee\components` on Windows,
/// `~/.local/share/eyecansee/components` on Linux).
pub fn dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::home_dir().map(|h| h.join(".local/share")));
    Some(base?.join("eyecansee").join("components"))
}

fn folder(c: &Component) -> String {
    format!("{}-{}", c.kind, c.version)
}

pub fn path(c: &Component) -> Option<PathBuf> {
    Some(dir()?.join(folder(c)).join(c.file))
}

/// Downloads, checks and unpacks `c`; `done` counts the bytes downloaded. Nothing is left in place
/// of the file unless it is complete and its SHA-256 matches.
pub fn install(c: &Component, done: &AtomicU64) -> Result<(), String> {
    let dir = dir().ok_or("no data folder")?;
    // Unpacked aside, moved in only once complete and matching its SHA-256.
    let staging = dir.join(".part");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let r = (|| {
        let body = crate::update::agent().get(c.url).call().map_err(|e| e.to_string())?.into_body().into_reader();
        let mut hashed = Hashed { inner: body, ctx: ring::digest::Context::new(&ring::digest::SHA256), done };
        let mut gz = flate2::read::GzDecoder::new(&mut hashed);
        if c.archive {
            untar(&mut gz, &staging)?;
        } else {
            let mut out = std::fs::File::create(staging.join(c.file)).map_err(|e| e.to_string())?;
            std::io::copy(&mut gz, &mut out).map_err(|e| e.to_string())?;
        }
        drop(gz);
        // Whatever follows the gzip stream counts for the hash too.
        std::io::copy(&mut hashed, &mut std::io::sink()).map_err(|e| e.to_string())?;
        let got: String = hashed.ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
        if got != c.sha256 {
            return Err(format!("SHA-256 mismatch: {got}, expected {}", c.sha256));
        }
        if !staging.join(c.file).is_file() {
            return Err(format!("{} missing from the download", c.file));
        }
        // A program (FFmpeg) must be executable; tar's modes are not kept by `untar`.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(staging.join(c.file), std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        let target = dir.join(folder(c));
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&staging, &target).map_err(|e| e.to_string())?;
        remove_old(&dir, c);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    r
}

/// The other versions of `c` in `dir`, and what 0.59.0 left loose there (`libmpv-2.dll`).
fn remove_old(dir: &std::path::Path, c: &Component) {
    let keep = folder(c);
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(c.kind) && name != keep {
            let p = entry.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
}

/// The plain files of a tar into `into`, by their bare name: no folders, no links, nothing
/// outside `into`.
// ponytail: ustar names up to 100 bytes, no GNU long names; enough for our own archives
fn untar(tar: &mut impl Read, into: &std::path::Path) -> Result<(), String> {
    let mut header = [0u8; 512];
    loop {
        tar.read_exact(&mut header).map_err(|e| e.to_string())?;
        if header.iter().all(|&b| b == 0) {
            return Ok(()); // end of the archive
        }
        let text = |r: std::ops::Range<usize>| String::from_utf8_lossy(&header[r]).trim_matches(|c| c == '\0' || c == ' ').to_string();
        let size = u64::from_str_radix(&text(124..136), 8).map_err(|e| format!("tar: {e}"))?;
        let name = text(0..100);
        let name = name.trim_start_matches("./");
        let mut body = tar.take(size.div_ceil(512) * 512);
        let plain = matches!(header[156], b'0' | 0) && !name.is_empty() && !name.contains(['/', '\\']) && name != "..";
        if plain {
            let mut out = std::fs::File::create(into.join(name)).map_err(|e| e.to_string())?;
            std::io::copy(&mut (&mut body).take(size), &mut out).map_err(|e| e.to_string())?;
        }
        std::io::copy(&mut body, &mut std::io::sink()).map_err(|e| e.to_string())?;
    }
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

    #[test]
    fn old_versions_removed() {
        let dir = std::env::temp_dir().join(format!("eyecansee-components-{}", std::process::id()));
        for d in ["libmpv-1", "libmpv-2", "other-1"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("libmpv-2.dll"), b"0.59.0").unwrap();
        let c = Component { kind: "libmpv", version: "2", file: "x", url: "", archive: false, sha256: "", size: 0 };
        remove_old(&dir, &c);
        let mut left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["libmpv-2", "other-1"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn plain_files_from_tar() {
        let dir = std::env::temp_dir().join(format!("eyecansee-untar-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/sub")).unwrap();
        std::fs::write(dir.join("src/libx.so.2"), vec![7u8; 1000]).unwrap();
        std::fs::write(dir.join("src/sub/nested"), b"no").unwrap();
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let tar = std::process::Command::new("tar").args(["-C", "src", "-cf", "-", "."]).current_dir(&dir).output().unwrap().stdout;
        untar(&mut &tar[..], &dir.join("out")).unwrap();
        assert_eq!(std::fs::read(dir.join("out/libx.so.2")).unwrap(), vec![7u8; 1000]);
        assert!(!dir.join("out/nested").exists() && !dir.join("out/sub").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

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

    /// As `component_live`, for FFmpeg: downloaded, checked and made executable (not run).
    #[test]
    #[ignore]
    fn ffmpeg_component_live() {
        let Some(c) = FFMPEG else { return };
        let done = AtomicU64::new(0);
        install(&c, &done).unwrap();
        let p = path(&c).unwrap();
        println!("{} bytes to {p:?}", done.load(Ordering::Relaxed));
        assert_eq!(done.load(Ordering::Relaxed), c.size);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o111, 0o111);
        }
    }
}
