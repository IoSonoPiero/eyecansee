//! libmpv found and loaded: the system's library or the downloaded component (`component.rs`),
//! chosen by the option in `engine.rs` and by the version, and the functions we call from it
//! (`Api`). Split from `mpv.rs` on 2026-10-29.

use std::ffi::{c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::engine::{self, Source, Status};
use crate::mpv::{Event, FORMAT_STRING, Handle, OpenFn, Param, Render};

#[cfg(windows)]
const NAMES: &[&str] = &["libmpv-2.dll", "mpv-2.dll"];
#[cfg(not(windows))]
// libmpv.so.1: Ubuntu 22.04 (mpv 0.34, checked in a container on 2026-10-27: same calls work).
const NAMES: &[&str] = &["libmpv.so.2", "libmpv.so.1", "libmpv.so"];

/// The oldest libmpv EyeCanSee works with: client API 1.109 (mpv 0.33.0, November 2020), which
/// added the software renderer (`MPV_RENDER_API_TYPE_SW`, the `sw` of `Mpv::new`) that gives us
/// the frames. Everything else we call is older: `mpv_stream_cb_add_ro` is API 1.106 (mpv 0.30),
/// the properties and commands date from the 0.2x days. libmpv 2.x (mpv 0.35 on) only removed
/// APIs we never used. Oldest one tried: mpv 0.34 (API 1.109) of Ubuntu 22.04, in a container.
pub const MIN_API: (u32, u32) = (1, 109);
pub const MIN_TEXT: &str = "1.109 (mpv 0.33)";

pub(crate) struct Api {
    pub(crate) create: unsafe extern "C" fn() -> Handle,
    pub(crate) initialize: unsafe extern "C" fn(Handle) -> c_int,
    pub(crate) set_option_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    pub(crate) command: unsafe extern "C" fn(Handle, *const *const c_char) -> c_int,
    pub(crate) get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    pub(crate) wait_event: unsafe extern "C" fn(Handle, f64) -> *const Event,
    pub(crate) terminate_destroy: unsafe extern "C" fn(Handle),
    pub(crate) render_create: unsafe extern "C" fn(*mut Render, Handle, *mut Param) -> c_int,
    pub(crate) render_set_update_callback: unsafe extern "C" fn(Render, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    pub(crate) render_update: unsafe extern "C" fn(Render) -> u64,
    pub(crate) render: unsafe extern "C" fn(Render, *mut Param) -> c_int,
    pub(crate) render_free: unsafe extern "C" fn(Render),
    pub(crate) stream_cb_add_ro: unsafe extern "C" fn(Handle, *const c_char, *mut c_void, OpenFn) -> c_int,
    pub(crate) free: unsafe extern "C" fn(*mut c_void),
    pub(crate) _lib: std::mem::ManuallyDrop<libloading::Library>,
}

/// Loaded once found; until then looked for again at every call (the component may arrive).
pub(crate) fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Api> = OnceLock::new();
    if let Some(api) = API.get() {
        return Ok(api);
    }
    let api = load()?;
    Ok(API.get_or_init(|| api))
}

/// What a libmpv is: its client API (`mpv_client_api_version`, major and minor) and its
/// `mpv-version` ("mpv 0.41.0"), read through a handle of its own that shows nothing.
pub struct Version {
    pub api: (u32, u32),
    pub mpv: String,
}

impl Version {
    fn text(&self) -> String {
        format!("{} (API {}.{})", self.mpv, self.api.0, self.api.1)
    }
}

/// `mpv_client_api_version` packs the version as `(major << 16) | minor`.
pub fn api_pair(v: u64) -> (u32, u32) {
    ((v >> 16) as u32, (v & 0xffff) as u32)
}

fn version_of(lib: &libloading::Library) -> Option<Version> {
    // SAFETY: signatures from client.h; the handle is destroyed before returning.
    unsafe {
        let api_version: libloading::Symbol<unsafe extern "C" fn() -> std::ffi::c_ulong> = lib.get(b"mpv_client_api_version\0").ok()?;
        // `c_ulong` is 32 bits on Windows and 64 on Linux.
        #[allow(clippy::unnecessary_cast)]
        let api = api_pair((api_version)() as u64);
        let create: libloading::Symbol<unsafe extern "C" fn() -> Handle> = lib.get(b"mpv_create\0").ok()?;
        let set: libloading::Symbol<unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int> = lib.get(b"mpv_set_option_string\0").ok()?;
        let initialize: libloading::Symbol<unsafe extern "C" fn(Handle) -> c_int> = lib.get(b"mpv_initialize\0").ok()?;
        let get: libloading::Symbol<unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int> = lib.get(b"mpv_get_property\0").ok()?;
        let free: libloading::Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"mpv_free\0").ok()?;
        let destroy: libloading::Symbol<unsafe extern "C" fn(Handle)> = lib.get(b"mpv_terminate_destroy\0").ok()?;
        let handle = create();
        if handle.is_null() {
            return Some(Version { api, mpv: String::new() });
        }
        set(handle, c"terminal".as_ptr(), c"no".as_ptr());
        let mut mpv = String::new();
        if initialize(handle) >= 0 {
            let mut value: *mut c_char = std::ptr::null_mut();
            if get(handle, c"mpv-version".as_ptr(), FORMAT_STRING, &mut value as *mut *mut c_char as *mut c_void) >= 0 && !value.is_null() {
                mpv = std::ffi::CStr::from_ptr(value).to_string_lossy().into_owned();
                free(value as *mut c_void);
            }
        }
        destroy(handle);
        Some(Version { api, mpv })
    }
}

/// The system's libmpv (next to the exe on Windows), looked at once and kept loaded: a libmpv
/// that was initialized must not be unloaded (`dlclose` after `version_of` left a thread of it
/// running, which crashed when the test ended, 2026-10-29).
fn system() -> Option<&'static Version> {
    static SYSTEM: OnceLock<Option<(libloading::Library, Version)>> = OnceLock::new();
    // SAFETY: libmpv runs no code of its own when loaded.
    let found = || unsafe { NAMES.iter().find_map(|n| libloading::Library::new(n).ok()) };
    SYSTEM.get_or_init(|| found().and_then(|lib| version_of(&lib).map(|v| (lib, v)))).as_ref().map(|(_, v)| v)
}

fn downloaded() -> Option<PathBuf> {
    crate::component::VIDEO.as_ref().and_then(crate::component::path).filter(|p| p.is_file())
}

/// Where the loaded libmpv came from and its version, once loaded.
static USED: OnceLock<(Source, String)> = OnceLock::new();

/// For the viewer, the menu bar and the Options.
pub fn status() -> Status {
    if let Some((src, v)) = USED.get() {
        return Status::Ready(*src, v.clone());
    }
    let found = system().map(|v| (v.api >= MIN_API, v.text()));
    engine::status(engine::choice(), found, downloaded().is_some(), MIN_TEXT, |src| match src {
        Source::System => system().map(Version::text).unwrap_or_default(),
        Source::Downloaded => crate::component::VIDEO.map(|c| format!("mpv {}", c.version)).unwrap_or_default(),
    })
}

fn load() -> Result<Api, String> {
    // Debug only: a computer without libmpv, to see the download (EYECANSEE_NO_MPV=1).
    if cfg!(debug_assertions) && std::env::var_os("EYECANSEE_NO_MPV").is_some() {
        return Err("EYECANSEE_NO_MPV".into());
    }
    // The system's when the option allows it and it is recent enough, else the downloaded component.
    let downloaded = downloaded();
    let src = engine::pick(engine::choice(), system().map(|v| v.api >= MIN_API), downloaded.is_some()).ok_or_else(|| format!("{} not found", NAMES[0]))?;
    // SAFETY: libmpv runs no code of its own when loaded.
    let lib = unsafe {
        match src {
            Source::System => NAMES.iter().find_map(|n| libloading::Library::new(n).ok()),
            Source::Downloaded => downloaded.and_then(|p| libloading::Library::new(p).ok()),
        }
    }
    .ok_or_else(|| format!("{} not found", NAMES[0]))?;
    let version = version_of(&lib).map(|v| v.text()).unwrap_or_default();
    log!("libmpv: {src:?}, {version}");
    let _ = USED.set((src, version));
    open(lib)
}

/// The functions we call, from the library found. `ManuallyDrop`: a library we could not use
/// stays loaded (see `system`); the one in use lives in a static anyway.
fn open(lib: libloading::Library) -> Result<Api, String> {
    let lib = std::mem::ManuallyDrop::new(lib);
    // SAFETY: the signatures are those of client.h and render.h.
    unsafe {
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| e.to_string())?
            };
        }
        Ok(Api {
            create: sym!("mpv_create"),
            initialize: sym!("mpv_initialize"),
            set_option_string: sym!("mpv_set_option_string"),
            command: sym!("mpv_command"),
            get_property: sym!("mpv_get_property"),
            wait_event: sym!("mpv_wait_event"),
            terminate_destroy: sym!("mpv_terminate_destroy"),
            render_create: sym!("mpv_render_context_create"),
            render_set_update_callback: sym!("mpv_render_context_set_update_callback"),
            render_update: sym!("mpv_render_context_update"),
            render: sym!("mpv_render_context_render"),
            render_free: sym!("mpv_render_context_free"),
            stream_cb_add_ro: sym!("mpv_stream_cb_add_ro"),
            free: sym!("mpv_free"),
            _lib: lib,
        })
    }
}

/// libmpv could be loaded.
pub fn available() -> bool {
    api().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_versions() {
        assert_eq!(api_pair(0x0001_006D), (1, 109));
        assert_eq!(api_pair(0x0002_0006), (2, 6));
        assert!((1, 109) >= MIN_API);
        assert!((1, 108) < MIN_API);
    }
}
