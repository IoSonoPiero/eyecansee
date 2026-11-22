//! Add-ons downloaded the first time they are needed, not shipped in the package: the video
//! engine, libmpv, which carries patented decoders (HEVC), and FFmpeg, which converts videos.
//! The file comes from a release of ours ("components"); its SHA-256 is written here, in the
//! signed executable, so a different file is refused.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
/// `tools/build-mpv-linux.sh`, mpv 0.42.0 with FFmpeg 9.0.2, dav1d and libplacebo inside, built
/// on Ubuntu 22.04 (glibc 2.35), with libass and three libraries it needs (28 MB, 12 MB compressed).
#[cfg(not(windows))]
pub const VIDEO: Option<Component> = Some(Component {
    kind: "libmpv",
    version: "0.42.0",
    file: "libmpv.so.2",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/libmpv-linux-x64-0.42.0.tar.gz",
    archive: true,
    sha256: "9ccb8e17798953941bb07d1526188e4a74d78ccaf9e722a60c8b52ee817a8c32",
    size: 11_920_684,
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

/// The CAD engine (`occt.rs`): Open CASCADE Technology 8.0.1 with our C interface
/// (`tools/occt/ecs_occt.cpp`), for STEP, IGES and BREP, built by `tools/build-occt-linux.sh`
/// (Ubuntu 22.04, libstdc++ inside: 28 MB, 11 MB compressed). Loaded as soon as it is there.
#[cfg(not(windows))]
pub const OCCT: Option<Component> = Some(Component {
    kind: "occt",
    version: "8.0.1-2",
    file: "libecs_occt.so",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/occt-linux-x64-8.0.1-2.tar.gz",
    archive: true,
    sha256: "55559a64aa3e9297ac2f9131d890ab76a0696e065ce2fde4b671f7f4c0e9d0d9",
    size: 11_197_731,
});

/// The same on Windows, built by `tools/build-occt-windows.ps1` (static C runtime, only system DLLs:
/// 8 MB compressed).
#[cfg(windows)]
pub const OCCT: Option<Component> = Some(Component {
    kind: "occt",
    version: "8.0.1-1",
    file: "ecs_occt.dll",
    url: "https://github.com/IoSonoPiero/eyecansee/releases/download/components/occt-windows-x64-8.0.1-1.tar.gz",
    archive: true,
    sha256: "e61cbecbba1804efe897b171710efe58e9dd48ecd5effa7ca4dc3e74c57db66b",
    size: 7_822_247,
});

/// Where components go: the app's data folder (`%LOCALAPPDATA%\eyecansee\components` on Windows,
/// `~/.local/share/eyecansee/components` on Linux, `~/.var/app/<id>/data/eyecansee/components` in
/// the Flatpak, whose `XDG_DATA_HOME` main.rs points at the host's for the trash).
pub fn dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = match std::env::var_os("FLATPAK_ID") {
        Some(id) => std::env::home_dir().map(|h| h.join(".var/app").join(id).join("data")),
        None => std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::home_dir().map(|h| h.join(".local/share"))),
    };
    Some(base?.join(crate::test_copy::NAME).join(folder_name(cfg!(windows) && crate::update::packaged())))
}

/// The components' folder: its own for the Microsoft Store copy. A packaged app reads and writes
/// the real `%LOCALAPPDATA%\eyecansee\components` when the installer or the zip made it first (seen
/// on a PC with both), and the installer's uninstaller deletes that folder: the Store copy lost
/// its components with it. A folder of its own, made by the Store copy, goes to the package's
/// private place and away with the Store copy (decision of 15/11).
fn folder_name(store: bool) -> &'static str {
    if store { "components-store" } else { "components" }
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
    install_in(c, &dir().ok_or("no data folder")?, &crate::update::agent(), done)
}

/// `install` into the components' folder `dir`, downloading with `agent` (the tests pass their own).
fn install_in(c: &Component, dir: &Path, agent: &ureq::Agent, done: &AtomicU64) -> Result<(), String> {
    // Downloaded and unpacked aside, moved in only once complete and matching its SHA-256.
    let staging = staging(dir, c);
    let gz = staging.with_extension("gz");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let r = fetch(c, agent, &gz, done).and_then(|()| unpack(c, &gz, &staging)).and_then(|()| {
        let target = dir.join(folder(c));
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&staging, &target).map_err(|e| e.to_string())?;
        remove_old(dir, c);
        Ok(())
    });
    let _ = std::fs::remove_file(&gz);
    let _ = std::fs::remove_dir_all(&staging);
    r
}

/// The download of `c` into the file `to`, hashed and counted as it arrives. More bytes than
/// `c.size` or another SHA-256 is an error, before anything is unpacked: a file that is not ours
/// never expands on the disk.
fn fetch(c: &Component, agent: &ureq::Agent, to: &Path, done: &AtomicU64) -> Result<(), String> {
    let body = agent.get(c.url).call().map_err(|e| e.to_string())?.into_body().into_reader();
    // One byte more than expected is enough to know it is too large.
    let mut hashed = Hashed { inner: body.take(c.size + 1), ctx: ring::digest::Context::new(&ring::digest::SHA256), done };
    let mut out = std::fs::File::create(to).map_err(|e| e.to_string())?;
    let mut buf = vec![0; 1 << 16];
    let mut total = 0;
    loop {
        let n = hashed.read(&mut buf).map_err(crate::update::read_error)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > c.size {
            return Err(format!("larger than expected: more than {} bytes", c.size));
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    let got: String = hashed.ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
    if got != c.sha256 {
        return Err(format!("SHA-256 mismatch: {got}, expected {}", c.sha256));
    }
    Ok(())
}

/// The checked download `gz` unpacked into `staging`.
fn unpack(c: &Component, gz: &Path, staging: &Path) -> Result<(), String> {
    let file = std::fs::File::open(gz).map_err(|e| e.to_string())?;
    let mut gz = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    if c.archive {
        untar(&mut gz, staging)?;
    } else {
        let mut out = std::fs::File::create(staging.join(c.file)).map_err(|e| e.to_string())?;
        std::io::copy(&mut gz, &mut out).map_err(|e| e.to_string())?;
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
    Ok(())
}

/// Where `c` is unpacked before being moved in, one folder per component: with a single `.part`
/// the video engine and FFmpeg downloaded together emptied each other's folder (2026-10-29).
fn staging(dir: &Path, c: &Component) -> PathBuf {
    dir.join(format!(".part-{}", c.kind))
}

/// The other versions of `c` in `dir`, and what 0.59.0 left loose there (`libmpv-2.dll`).
fn remove_old(dir: &Path, c: &Component) {
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
fn untar(tar: &mut impl Read, into: &Path) -> Result<(), String> {
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
    begin(c.kind, move |done| install(&c, done), repaint)
}

/// The downloads started, by component: a second start of one under way gives it back (audit
/// 2.9: a second `install` removed the folder the first one was unpacking into).
static UNDER_WAY: std::sync::Mutex<Vec<(&'static str, std::sync::Weak<Download>)>> = std::sync::Mutex::new(Vec::new());

/// `work` (the download of the component `kind`) in a thread of its own, counting into `done`;
/// `repaint` when it ends. The one of `kind` under way, if there is one: its own `repaint` stays.
fn begin(kind: &'static str, work: impl FnOnce(&AtomicU64) -> Result<(), String> + Send + 'static, repaint: impl Fn() + Send + 'static) -> std::sync::Arc<Download> {
    let mut under_way = UNDER_WAY.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    under_way.retain(|(_, d)| d.upgrade().is_some_and(|d| d.result.lock().unwrap().is_none()));
    if let Some(d) = under_way.iter().filter(|(k, _)| *k == kind).find_map(|(_, d)| d.upgrade()) {
        log!("component {kind}: already downloading, the same download given back");
        return d;
    }
    let d = std::sync::Arc::new(Download::default());
    under_way.push((kind, std::sync::Arc::downgrade(&d)));
    let ends = Ends(d.clone());
    crate::logging::spawn_named("component-download", move || {
        let r = work(&ends.0.done);
        *ends.0.result.lock().unwrap() = Some(r);
        repaint();
    });
    d
}

/// A download that stops without a result (its thread panicked) ends with an error: not under
/// way for ever, so a new start can try again.
struct Ends(std::sync::Arc<Download>);

impl Drop for Ends {
    fn drop(&mut self) {
        let mut r = self.0.result.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if r.is_none() {
            *r = Some(Err("the download stopped".into()));
        }
    }
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

/// What the first-start window and Options > Add-ons show of an add-on (#212). The list is here,
/// not in the window: a new component appears in both without touching them.
#[derive(Clone, Copy)]
pub struct Addon {
    pub component: Component,
    /// Fluent ids: its name, and one plain line about what it is for.
    pub name: &'static str,
    pub about: &'static str,
    /// The picture of its row.
    pub icon: crate::icons::Icon,
    /// The system's own copy, which stands in for the download when it fits (`engine::Status`, the
    /// same rule as the red buttons); none where there is no such copy.
    pub system: Option<fn() -> crate::engine::Status>,
    /// Versions to choose from, for example two fonts of different size: none uses it yet.
    #[expect(dead_code, reason = "room for the PDF engine's fonts (#212); no add-on has variants yet")]
    pub variants: &'static [Variant],
}

/// One of the versions of an add-on to choose from (`Addon::variants`).
#[derive(Clone, Copy)]
#[expect(dead_code, reason = "no add-on has variants yet (#212)")]
pub struct Variant {
    /// Fluent id of its name.
    pub name: &'static str,
    pub component: Component,
}

/// The add-ons of this system, in the order they are shown.
pub fn addons() -> Vec<Addon> {
    use crate::icons::Icon;
    let addon = |component, name, about, icon, system| Addon { component, name, about, icon, system, variants: &[] };
    let (mpv, ffmpeg): (fn() -> _, fn() -> _) = (crate::mpv_load::status, crate::ffmpeg::status);
    [
        VIDEO.map(|c| addon(c, "addon-video", "addon-video-about", Icon::EngineVideo, Some(mpv))),
        FFMPEG.map(|c| addon(c, "addon-ffmpeg", "addon-ffmpeg-about", Icon::Convert, Some(ffmpeg))),
        OCCT.map(|c| addon(c, "addon-cad", "addon-cad-about", Icon::Model, None)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Where an add-on is on this computer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Have {
    /// Neither downloaded nor on the system: Download.
    Missing,
    /// An older version downloaded, which this EyeCanSee no longer uses: Update.
    Older,
    /// The version this EyeCanSee wants, downloaded.
    Downloaded,
    /// The system's copy, which fits: nothing to download (its version).
    OnSystem(String),
    /// None that can be used, and the Options say "the system's only": a download would not be
    /// used (the same text as the red buttons, `batch-video-system-only`).
    SystemOnly,
}

/// Where `a` is: downloaded, on the system, or missing.
pub fn have(a: &Addon) -> Have {
    let status = a.system.map(|f| f());
    match dir() {
        Some(d) => have_in(&d, &a.component, status),
        None => Have::Missing,
    }
}

fn have_in(dir: &Path, c: &Component, status: Option<crate::engine::Status>) -> Have {
    use crate::engine::{Source, Status};
    if dir.join(folder(c)).join(c.file).is_file() {
        return Have::Downloaded;
    }
    match status {
        Some(Status::Ready(Source::System, v)) => Have::OnSystem(v),
        Some(Status::SystemOnly) => Have::SystemOnly,
        _ if older_in(dir, c) => Have::Older,
        _ => Have::Missing,
    }
}

/// Another version of `c` in `dir`, which `remove_old` deletes once this one is there.
fn older_in(dir: &Path, c: &Component) -> bool {
    let (keep, prefix) = (folder(c), format!("{}-", c.kind));
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.starts_with(&prefix) && name != keep && e.path().is_dir()
    })
}


#[cfg(test)]
mod tests {
    /// The rows of the first-start window and of Options > Add-ons: every add-on has a name and a
    /// line about it in the languages, and the sizes are those of the components.
    #[test]
    fn addons_named_and_sized() {
        let en = include_str!("../locales/en.ftl");
        for a in addons() {
            for id in [a.name, a.about] {
                assert!(en.lines().any(|l| l.starts_with(&format!("{id} ="))), "{id} missing in en.ftl");
            }
            assert!(a.component.size > 1_000_000, "{}", a.component.kind);
        }
        let kinds: Vec<_> = addons().iter().map(|a| a.component.kind).collect();
        assert_eq!(kinds, ["libmpv", "ffmpeg", "occt"]);
    }

    #[test]
    fn have_by_what_is_on_disk_and_system() {
        use crate::engine::{Source, Status};
        let dir = scratch("have");
        let c = Component { kind: "thing", version: "2", file: "thing.so", url: "", archive: false, sha256: "", size: 1 };
        assert_eq!(have_in(dir.as_path(), &c, None), Have::Missing);
        assert_eq!(have_in(dir.as_path(), &c, Some(Status::Missing)), Have::Missing);
        let lacks = Status::Lacks { found: "8.1".into(), missing: "HEVC".into() };
        assert_eq!(have_in(dir.as_path(), &c, Some(lacks)), Have::Missing, "a system copy without the phones' codecs");
        assert_eq!(have_in(dir.as_path(), &c, Some(Status::Ready(Source::System, "8.1".into()))), Have::OnSystem("8.1".into()));
        assert_eq!(have_in(dir.as_path(), &c, Some(Status::SystemOnly)), Have::SystemOnly);
        // Another component whose name starts the same does not count, a file neither.
        std::fs::create_dir(dir.as_path().join("thingy-1")).unwrap();
        std::fs::write(dir.as_path().join("thing-1.gz"), "").unwrap();
        assert_eq!(have_in(dir.as_path(), &c, None), Have::Missing);
        std::fs::create_dir(dir.as_path().join("thing-1")).unwrap();
        assert_eq!(have_in(dir.as_path(), &c, None), Have::Older);
        std::fs::create_dir(dir.as_path().join("thing-2")).unwrap();
        std::fs::write(dir.as_path().join("thing-2/thing.so"), "").unwrap();
        assert_eq!(have_in(dir.as_path(), &c, Some(Status::Ready(Source::System, "8.1".into()))), Have::Downloaded);
        std::fs::remove_dir_all(dir).unwrap();
    }

    use super::*;

    /// The Store copy has a folder of its own, which the installer's uninstaller does not delete.
    #[test]
    fn store_copy_folder_apart() {
        assert_eq!(folder_name(false), "components");
        assert_eq!(folder_name(true), "components-store");
    }

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

    /// Two components downloaded together never share where they are unpacked, and finishing one
    /// leaves the other's download alone.
    #[test]
    fn staging_per_component() {
        let dir = std::env::temp_dir().join(format!("eyecansee-staging-{}", std::process::id()));
        let video = Component { kind: "libmpv", version: "2", file: "x", url: "", archive: false, sha256: "", size: 0 };
        let ffmpeg = Component { kind: "ffmpeg", ..video };
        assert_ne!(staging(&dir, &video), staging(&dir, &ffmpeg));
        std::fs::create_dir_all(staging(&dir, &ffmpeg)).unwrap();
        remove_old(&dir, &video);
        assert!(staging(&dir, &ffmpeg).is_dir());
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

    /// As `component_live`, for the CAD engine: downloaded, checked, and a STEP file read with it
    /// (`EYECANSEE_STEP_FILE`, if given).
    #[test]
    #[ignore]
    fn occt_component_live() {
        let Some(c) = OCCT else { return };
        let done = AtomicU64::new(0);
        let t = std::time::Instant::now();
        install(&c, &done).unwrap();
        println!("{} bytes in {:?} to {:?}", done.load(Ordering::Relaxed), t.elapsed(), path(&c));
        assert_eq!(done.load(Ordering::Relaxed), c.size);
        assert!(crate::occt::available());
        if let Ok(f) = std::env::var("EYECANSEE_STEP_FILE") {
            let m = crate::occt::read(&std::fs::read(&f).unwrap(), &f).unwrap();
            println!("{f}: {} triangles", m.tris.len());
        }
    }

    /// A components' folder of its own for one test, in the temporary folder: never the real one.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-component-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A component served once from 127.0.0.1 (`update::tests::serve_once`).
    fn served(kind: &'static str, file: &'static str, archive: bool, body: Vec<u8>, sent: usize, hold: std::time::Duration) -> Component {
        let sha256: String = ring::digest::digest(&ring::digest::SHA256, &body).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        let size = body.len() as u64;
        let url = crate::update::tests::serve_once(body, sent, hold).leak();
        Component { kind, version: "1", file, url, archive, sha256: sha256.leak(), size }
    }

    fn gzip(fill: impl FnOnce(&mut dyn std::io::Write)) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        fill(&mut gz);
        gz.finish().unwrap()
    }

    fn quick() -> ureq::Agent {
        crate::update::agent_with(std::time::Duration::from_secs(5))
    }

    /// What is in `dir`, sorted.
    fn listing(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        v.sort();
        v
    }

    /// A good download, a plain file and an archive: unpacked in its folder, nothing else left.
    #[test]
    fn installed_when_good() {
        let dir = scratch("good");
        let c = served("plain", "lib.bin", false, gzip(|gz| gz.write_all(&[3; 5000]).unwrap()), usize::MAX, std::time::Duration::ZERO);
        let done = AtomicU64::new(0);
        install_in(&c, &dir, &quick(), &done).unwrap();
        assert_eq!(done.load(Ordering::Relaxed), c.size);
        assert_eq!(std::fs::read(dir.join("plain-1/lib.bin")).unwrap(), vec![3; 5000]);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/prog"), b"program").unwrap();
        std::fs::write(dir.join("src/libx.so"), b"library").unwrap();
        let tar = std::process::Command::new("tar").args(["-C", "src", "-cf", "-", "prog", "libx.so"]).current_dir(&dir).output().unwrap().stdout;
        std::fs::remove_dir_all(dir.join("src")).unwrap();
        let c = served("tarred", "prog", true, gzip(|gz| gz.write_all(&tar).unwrap()), usize::MAX, std::time::Duration::ZERO);
        install_in(&c, &dir, &quick(), &AtomicU64::new(0)).unwrap();
        assert_eq!(listing(&dir), ["plain-1", "tarred-1"]);
        assert_eq!(listing(&dir.join("tarred-1")), ["libx.so", "prog"]);
        assert_eq!(std::fs::read(dir.join("tarred-1/prog")).unwrap(), b"program");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join("tarred-1/prog")).unwrap().permissions().mode() & 0o111, 0o111);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A download longer than the size written for the component stops there, and nothing is left.
    #[test]
    fn larger_than_expected() {
        let dir = scratch("larger");
        let mut c = served("larger", "lib.bin", false, gzip(|gz| gz.write_all(&[3; 5000]).unwrap()), usize::MAX, std::time::Duration::ZERO);
        c.size -= 1;
        let r = install_in(&c, &dir, &quick(), &AtomicU64::new(0));
        assert_eq!(r, Err(format!("larger than expected: more than {} bytes", c.size)));
        assert!(listing(&dir).is_empty(), "{:?}", listing(&dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Audit 2.15 (b): as `update::tests::audit_2_15_stalled_body`, for a component: the server
    /// sends the headers and the first 100 kB of a gzip of 1 MB, then nothing more, with the
    /// connection open. `install` ends with an error once no byte has come for the stall time
    /// (here 1 s), and leaves nothing.
    #[test]
    fn audit_2_15_stalled_component() {
        use std::time::Duration;
        let dir = scratch("stalled");
        // Bytes that do not compress, so that the gzip is about as long as they are.
        let mut x = 1u32;
        let noise: Vec<u8> = std::iter::repeat_with(|| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .take(1 << 20)
        .collect();
        let c = served("audit-stalled", "noise.bin", false, gzip(|gz| gz.write_all(&noise).unwrap()), 100_000, Duration::from_secs(3600));
        let (tx, rx) = std::sync::mpsc::channel();
        let (to, started) = (dir.clone(), std::time::Instant::now());
        std::thread::spawn(move || tx.send(install_in(&c, &to, &crate::update::agent_with(Duration::from_secs(1)), &AtomicU64::new(0))));
        let r = rx.recv_timeout(Duration::from_secs(10));
        let elapsed = started.elapsed();
        let left = listing(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(r, Ok(Err(crate::update::STALLED.into())), "after {elapsed:.1?}");
        assert!(elapsed < Duration::from_secs(5), "{elapsed:.1?}");
        assert!(left.is_empty(), "{left:?}");
    }

    /// Audit 2.15 (c): a download that is not the file expected (its SHA-256 differs) is refused
    /// before it is unpacked. Here 64 kB of gzip that hold 64 MB of zeros, with the right size and a
    /// wrong SHA-256: nothing may be unpacked. A thread watches the unpacked file meanwhile.
    #[test]
    fn audit_2_15_unpacked_before_sha() {
        use std::sync::atomic::AtomicBool;
        const UNPACKED: u64 = 64 << 20;
        let dir = scratch("bomb");
        let zeros = vec![0u8; 1 << 20];
        let body = gzip(|gz| (0..UNPACKED >> 20).for_each(|_| gz.write_all(&zeros).unwrap()));
        let c = Component { sha256: "00", ..served("audit-bomb", "zeros.bin", false, body, usize::MAX, std::time::Duration::ZERO) };
        let unpacked = staging(&dir, &c).join(c.file);
        let (stop, peak) = (std::sync::Arc::new(AtomicBool::new(false)), std::sync::Arc::new(AtomicU64::new(0)));
        let watch = {
            let (stop, peak) = (stop.clone(), peak.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    peak.fetch_max(std::fs::metadata(&unpacked).map_or(0, |m| m.len()), Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            })
        };
        let r = install_in(&c, &dir, &quick(), &AtomicU64::new(0));
        stop.store(true, Ordering::Relaxed);
        watch.join().unwrap();
        let peak = peak.load(Ordering::Relaxed);
        let left = listing(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(r.as_ref().is_err_and(|e| e.starts_with("SHA-256 mismatch")), "{r:?}");
        assert_eq!(peak, 0, "bytes unpacked before the SHA-256 was compared");
        assert!(left.is_empty(), "{left:?}");
    }

    /// Audit 2.9 (b): a second start of an add-on whose download is under way gives that download
    /// back, and the work runs once; once it has ended, a start is a new download (to try again
    /// after an error). Before, the second one removed the folder the first one was unpacking into.
    #[test]
    fn audit_2_9_one_download_of_an_add_on_at_a_time() {
        use std::sync::atomic::AtomicU32;
        let runs = std::sync::Arc::new(AtomicU32::new(0));
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let counted = |wait: Option<std::sync::mpsc::Receiver<()>>| {
            let runs = runs.clone();
            move |_: &AtomicU64| {
                runs.fetch_add(1, Ordering::SeqCst);
                if let Some(w) = wait {
                    let _ = w.recv();
                }
                Err("stopped by the test".to_string())
            }
        };
        let kind = "audit-2-9-add-on";
        let first = begin(kind, counted(Some(wait)), || {});
        let again = begin(kind, counted(None), || {});
        let other = begin("audit-2-9-other", counted(None), || {});
        let same = std::sync::Arc::ptr_eq(&first, &again);
        go.send(()).unwrap();
        let started = std::time::Instant::now();
        let ended = |d: &Download| d.result.lock().unwrap().is_some();
        while !(ended(&first) && ended(&again) && ended(&other)) {
            assert!(started.elapsed().as_secs() < 20, "the downloads did not end");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let after = begin(kind, counted(None), || {});
        while !ended(&after) {
            assert!(started.elapsed().as_secs() < 20, "the new download did not end");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(same, "a second download of the same add-on started");
        assert!(!std::sync::Arc::ptr_eq(&first, &after), "an ended download given back");
        assert_eq!(runs.load(Ordering::SeqCst), 3, "the first, the other add-on, the one after");
    }
}
