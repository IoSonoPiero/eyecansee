//! STEP, IGES and BREP models through the CAD engine: Open CASCADE Technology with our small C
//! interface (`tools/occt/ecs_occt.cpp`), a library downloaded when needed (`component::OCCT`).
//! The engine reads paths: a local file is given as it is, one from a phone or a server goes to a
//! temporary copy. Its surfaces come back as triangles with their colours and become a
//! `model::Mesh` for the 3D viewer.

use std::ffi::{CStr, CString, c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::model::{Builder, Mesh};

pub(crate) const OCCT: &[&str] = &["step", "stp", "iges", "igs", "brep", "brp"];
/// The interface's version this EyeCanSee speaks (`ECS_OCCT_API`).
const API: c_int = 1;

/// A STEP, IGES or BREP file, when the engine is there or can be downloaded: without it the
/// viewer offers the download (`cad_engine`).
pub fn is_occt(name: &str) -> bool {
    occt_name(name) && (crate::component::OCCT.is_some() || available())
}

/// A STEP, IGES or BREP file that needs the engine, still to be downloaded.
pub fn needs_engine(name: &str) -> bool {
    occt_name(name) && crate::component::OCCT.is_some() && !available()
}

fn occt_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| OCCT.contains(&e.to_lowercase().as_str()))
}

#[repr(C)]
struct EcsMesh {
    pos: *mut f32,
    vertices: u32,
    tris: *mut u32,
    triangles: u32,
    colors: *mut u8,
    error: [c_char; 512],
}

struct Engine {
    read: unsafe extern "C" fn(*const c_char, c_int, *mut EcsMesh) -> c_int,
    free: unsafe extern "C" fn(*mut EcsMesh),
    _lib: libloading::Library,
}

/// Where the engine is: `EYECANSEE_OCCT` (to try a build), otherwise the downloaded component.
fn library() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("EYECANSEE_OCCT") {
        return Some(PathBuf::from(p));
    }
    crate::component::OCCT.and_then(|c| crate::component::path(&c)).filter(|p| p.exists())
}

/// The error without the engine: the grid shows the model's sign waiting for the download.
pub const MISSING: &str = "the CAD engine is missing";

/// The engine is there (downloaded, or given by `EYECANSEE_OCCT`): looked for on disk until found
/// (only for STEP, IGES and BREP files), then remembered.
pub fn available() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FOUND: AtomicBool = AtomicBool::new(false);
    FOUND.load(Ordering::Relaxed) || {
        let found = library().is_some();
        FOUND.store(found, Ordering::Relaxed);
        found
    }
}

/// Loaded once found; until then looked for again (the download may arrive meanwhile).
fn engine() -> Result<&'static Engine, String> {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    if let Some(e) = ENGINE.get() {
        return Ok(e);
    }
    let path = library().ok_or(MISSING)?;
    // SAFETY: our own library, checked by SHA-256 when downloaded; its functions have these types.
    let e = unsafe {
        let lib = libloading::Library::new(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let api: unsafe extern "C" fn() -> c_int = *lib.get(b"ecs_occt_api\0").map_err(|e| e.to_string())?;
        if api() != API {
            return Err(format!("{}: interface {} instead of {API}", path.display(), api()));
        }
        Engine { read: *lib.get(b"ecs_occt_read\0").map_err(|e| e.to_string())?, free: *lib.get(b"ecs_occt_free\0").map_err(|e| e.to_string())?, _lib: lib }
    };
    Ok(ENGINE.get_or_init(|| e))
}

/// A temporary copy of the file, deleted when dropped.
struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The start of the copies' names, by which those left by a copy of the app closed by force are
/// found again (`remove_old_copies`); the test copy has its own.
fn temp_prefix() -> String {
    format!("{}-cad-", crate::test_copy::NAME)
}

/// The copy for a model from a phone or a server, in the temporary folder: a new file that only
/// the user can read, under a name nobody can work out beforehand, never written through a link
/// or over a file already there (`create_new`).
fn temp_copy(bytes: &[u8], ext: &str) -> Result<Temp, String> {
    use std::hash::{BuildHasher, Hasher};
    for _ in 0..8 {
        // Keys seeded from the system's random source, new for each `RandomState`.
        let n = std::hash::RandomState::new().build_hasher().finish();
        let path = std::env::temp_dir().join(format!("{}{}-{n:016x}.{ext}", temp_prefix(), std::process::id()));
        match new_private(&path) {
            Ok(mut f) => {
                let temp = Temp(path);
                std::io::Write::write_all(&mut f, bytes).map_err(|e| e.to_string())?;
                return Ok(temp);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("no new name for the temporary copy".into())
}

/// A new file only the user can read; an error when the name is taken, even by a link.
fn new_private(path: &Path) -> std::io::Result<std::fs::File> {
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
    open.open(path)
}

/// At start (after `single::hand_over`, so no other copy of the app is using them): the copies a
/// copy of the app closed by force or crashed left in the temporary folder. Not with
/// `EYECANSEE_MULTI`: another instance may be using them.
pub(crate) fn remove_old_copies() {
    if !crate::single::multi() {
        remove_old_copies_in(&std::env::temp_dir());
    }
}

fn remove_old_copies_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&temp_prefix()) {
            log!("old CAD copy removed: {name}");
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

thread_local! {
    /// The model's own file on this computer, while it is read (`from_file`).
    static FILE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// The file at `path` on this computer, for `from_file`: not through gvfs, slow and fragile
/// with a phone (FATTI), where the engine keeps reading a copy.
pub(crate) fn local(op: &crate::source::Store, path: &str) -> Option<PathBuf> {
    op.local_file(path).filter(|_| !op.gvfs(path))
}

/// `f` reads one file, whose path on this computer is `file` (`local`): if it is a
/// model for the engine, the engine gets that path, with no copy. Without it (a phone, a server)
/// the engine gets a copy.
pub(crate) fn from_file<T>(file: Option<PathBuf>, f: impl FnOnce() -> T) -> T {
    /// Puts back what was there, also when `f` panics: the thread may read a remote file next.
    struct Back(Option<PathBuf>);
    impl Drop for Back {
        fn drop(&mut self) {
            FILE.set(self.0.take());
        }
    }
    let _back = Back(FILE.replace(file));
    f()
}

/// A text BREP with a number of more than 40 digits: an edge running to infinity (OCCT's test
/// `infinite_line.brep`, ±2·10^103), which Open CASCADE's reader never finishes reading.
// ponytail: a look at the text, not at the geometry; the engine has no way to give up
fn endless(bytes: &[u8]) -> bool {
    let text = bytes.trim_ascii_start().starts_with(b"CASCADE Topology");
    text && bytes.split(|c| !c.is_ascii_digit()).any(|digits| digits.len() > 40)
}

/// The file the engine reads: the local file as it is, when it is the one read (a drawing's
/// picture is read in the same `from_file` as the drawing); a copy for the others, kept with it.
fn engine_file(bytes: &[u8], ext: &str) -> Result<(PathBuf, Option<Temp>), String> {
    let local = FILE.with_borrow(Clone::clone).filter(|f| std::fs::metadata(f).is_ok_and(|m| m.len() == bytes.len() as u64));
    match local {
        Some(file) => Ok((file, None)),
        None => temp_copy(bytes, ext).map(|copy| (copy.0.clone(), Some(copy))),
    }
}

pub fn read(bytes: &[u8], name: &str) -> Result<Mesh, String> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    // A STEP file says so in its first line, whatever its name (one named .iges was seen).
    let kind = match ext.as_str() {
        _ if bytes.trim_ascii_start().starts_with(b"ISO-10303-21") => 0,
        "step" | "stp" => 0,
        "iges" | "igs" => 1,
        _ => 2,
    };
    if kind == 2 && endless(bytes) {
        return Err("the BREP has geometry running to infinity".into());
    }
    let engine = engine()?;
    let (path, _copy) = engine_file(bytes, &ext)?;
    let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    // ponytail: one file at a time; Open CASCADE's document application is shared
    static ONE: Mutex<()> = Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let mut m = EcsMesh { pos: std::ptr::null_mut(), vertices: 0, tris: std::ptr::null_mut(), triangles: 0, colors: std::ptr::null_mut(), error: [0; 512] };
    // SAFETY: `m` is the struct the engine fills; it is freed by the engine below, once copied.
    let r = unsafe { (engine.read)(path.as_ptr(), kind, &mut m) };
    let mesh = if r == 0 {
        // SAFETY: on success the engine gave `vertices` × 3 floats, `triangles` × 3 indices and,
        // when not null, `triangles` × 3 colour bytes.
        unsafe { copy(&m) }
    } else {
        // SAFETY: the engine writes a NUL-terminated message into `error`.
        Err(unsafe { CStr::from_ptr(m.error.as_ptr()) }.to_string_lossy().into_owned())
    };
    // SAFETY: what `read` allocated, freed once.
    unsafe { (engine.free)(&mut m) };
    mesh
}

unsafe fn copy(m: &EcsMesh) -> Result<Mesh, String> {
    let (nv, nt) = (m.vertices as usize, m.triangles as usize);
    // SAFETY: sizes given by the engine with the pointers (see `read`).
    let (pos, tris, colors) = unsafe {
        (
            std::slice::from_raw_parts(m.pos, nv * 3),
            std::slice::from_raw_parts(m.tris, nt * 3),
            (!m.colors.is_null()).then(|| std::slice::from_raw_parts(m.colors, nt * 3)),
        )
    };
    let mut b = Builder::default();
    let pos = pos.as_chunks::<3>().0.to_vec();
    let color = |i: usize| colors.map(|c| [c[i * 3], c[i * 3 + 1], c[i * 3 + 2]]);
    b.add(pos, tris.as_chunks::<3>().0.iter().copied(), color)?;
    // CAD programs keep Z up.
    b.finish(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let huge = format!("\nCASCADE Topology V1\n1 0 -{}.0 {}.0\n", "2".repeat(104), "2".repeat(104));
        assert!(endless(huge.as_bytes()) && !endless(b"\nCASCADE Topology V1\n1 0 -1000.0 1000.0\n"));
        assert!(occt_name("a.STEP") && occt_name("b.stp") && occt_name("c.igs") && occt_name("d.brep") && !occt_name("e.stl"));
    }

    /// Every STEP, IGES and BREP under `EYECANSEE_STEP` read with the engine in `EYECANSEE_OCCT`
    /// and drawn into `EYECANSEE_STEP_OUT` (PNG), with times.
    #[test]
    #[ignore]
    fn occt_live() {
        let (Ok(dir), Ok(out)) = (std::env::var("EYECANSEE_STEP"), std::env::var("EYECANSEE_STEP_OUT")) else { return };
        let mut failed = Vec::new();
        for f in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if !occt_name(&name) {
                continue;
            }
            let bytes = std::fs::read(f.path()).unwrap();
            let t = std::time::Instant::now();
            match read(&bytes, &name) {
                Ok(m) => {
                    let read = t.elapsed();
                    let img = crate::render3d::render(&m, &Default::default(), 800, 600, 2);
                    println!("{name}: {} triangles, {} coloured, read {read:.0?}", m.tris.len(), !m.colors.is_empty());
                    img.save(format!("{out}/{name}.png")).unwrap();
                }
                Err(e) => {
                    println!("{name}: ERROR {e}");
                    failed.push(name);
                }
            }
        }
        println!("failed: {failed:?}");
    }

    /// The tests below make copies under the same names: one at a time.
    static COPIES: Mutex<()> = Mutex::new(());

    /// Audit 6.7: the copy for the engine goes to the temporary folder, which every user of the
    /// computer can list, with the permissions of any new file (readable by all). Only its owner
    /// must be able to read it.
    #[test]
    #[cfg(unix)]
    fn audit_6_7_temp_copy_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let _one = COPIES.lock().unwrap_or_else(|e| e.into_inner());
        let copy = temp_copy(b"ISO-10303-21;", "step").unwrap();
        let mode = std::fs::metadata(&copy.0).unwrap().permissions().mode() & 0o777;
        println!("{}: permissions {mode:o}", copy.0.display());
        assert_eq!(mode & 0o077, 0, "permissions {mode:o}");
    }

    /// Audit 6.7: the name of the copy can be worked out before it is made (the process number
    /// and a count from zero) and it is written with `fs::write`, which follows a link already
    /// there: someone else on the computer who leaves links under those names gets the model
    /// written over the file of ours they point to. The copy must never go through a link.
    #[test]
    #[cfg(unix)]
    fn audit_6_7_temp_copy_follows_no_link() {
        let _one = COPIES.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir();
        let ours = dir.join(format!("eyecansee-audit-ours-{}", std::process::id()));
        std::fs::write(&ours, b"ours").unwrap();
        // The next names, wherever the count has got to in this run of the tests.
        let links: Vec<PathBuf> = (0..16).map(|n| dir.join(format!("eyecansee-cad-{}-{n}.step", std::process::id()))).collect();
        for link in &links {
            std::os::unix::fs::symlink(&ours, link).unwrap();
        }
        let copy = temp_copy(b"the model", "step");
        let after = std::fs::read(&ours).unwrap();
        println!("{:?}: our file now holds {:?}", copy.as_ref().map(|c| c.0.display().to_string()), String::from_utf8_lossy(&after));
        drop(copy);
        for link in &links {
            let _ = std::fs::remove_file(link);
        }
        let _ = std::fs::remove_file(&ours);
        assert_eq!(after, b"ours", "the copy was written through a link, over another file");
    }

    /// The file made for the copy is never one already there: a link under its name is refused,
    /// and the file it points to stays as it was.
    #[test]
    #[cfg(unix)]
    fn new_private_refuses_a_link() {
        let dir = std::env::temp_dir().join(format!("eyecansee-occt-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ours"), b"ours").unwrap();
        std::os::unix::fs::symlink(dir.join("ours"), dir.join("copy.step")).unwrap();
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("dangling.step")).unwrap();
        let link = new_private(&dir.join("copy.step")).map(|_| ()).map_err(|e| e.kind());
        let dangling = new_private(&dir.join("dangling.step")).map(|_| ()).map_err(|e| e.kind());
        let (ours, nowhere) = (std::fs::read(dir.join("ours")).unwrap(), dir.join("nowhere").exists());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(link, Err(std::io::ErrorKind::AlreadyExists));
        assert_eq!(dangling, Err(std::io::ErrorKind::AlreadyExists));
        assert_eq!(ours, b"ours");
        assert!(!nowhere);
    }

    /// The copies left by an earlier run go at start, and nothing else in the folder.
    #[test]
    fn old_copies_removed() {
        let dir = std::env::temp_dir().join(format!("eyecansee-occt-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join(format!("{}123-00ff.step", temp_prefix()));
        let others = [dir.join("eyecansee-video-1-0.tmp"), dir.join("model.step"), dir.join("cad-1.step")];
        for f in others.iter().chain([&old]) {
            std::fs::write(f, b"x").unwrap();
        }
        remove_old_copies_in(&dir);
        let (gone, kept) = (!old.exists(), others.iter().all(|f| f.exists()));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(gone && kept);
    }

    /// A model read with its local file gives the engine that file, with no copy, until the
    /// reading ends (also with a panic): a model from a phone read next on the same thread gets a
    /// copy, and so does another file read meanwhile (not the same size).
    #[test]
    fn local_file_only_while_reading() {
        let _one = COPIES.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("eyecansee-occt-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("model.step");
        std::fs::write(&file, b"ISO-10303-21;").unwrap();
        let given = |bytes: &[u8]| engine_file(bytes, "step").map(|(p, copy)| (p == file, copy.is_some())).unwrap();
        let inside = from_file(Some(file.clone()), || [given(b"ISO-10303-21;"), given(b"another picture")]);
        let after = given(b"ISO-10303-21;");
        let _ = std::panic::catch_unwind(|| from_file(Some(file.clone()), || panic!("broken model")));
        let after_panic = given(b"ISO-10303-21;");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(inside, [(true, false), (false, true)]);
        assert_eq!((after, after_panic), ((false, true), (false, true)));
    }
}
