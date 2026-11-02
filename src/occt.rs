//! STEP, IGES and BREP models through the CAD engine: Open CASCADE Technology with our small C
//! interface (`tools/occt/ecs_occt.cpp`), a library downloaded when needed (`component::OCCT`).
//! The file goes to a temporary copy (the engine reads paths), its surfaces come back as
//! triangles with their colours and become a `model::Mesh` for the 3D viewer.

use std::ffi::{CStr, CString, c_char, c_int};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::model::{Builder, Mesh};

const OCCT: &[&str] = &["step", "stp", "iges", "igs", "brep", "brp"];
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
    let path = library().ok_or("the CAD engine is missing")?;
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

fn temp_copy(bytes: &[u8], ext: &str) -> Result<Temp, String> {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("eyecansee-cad-{}-{n}.{ext}", std::process::id()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(Temp(path))
}

pub fn read(bytes: &[u8], name: &str) -> Result<Mesh, String> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    let kind = match ext.as_str() {
        "step" | "stp" => 0,
        "iges" | "igs" => 1,
        _ => 2,
    };
    let engine = engine()?;
    let file = temp_copy(bytes, &ext)?;
    let path = CString::new(file.0.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
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
}
