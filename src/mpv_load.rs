//! libmpv found and loaded: the system's library or the downloaded component (`component.rs`),
//! and the functions we call from it (`Api`). Split from `mpv.rs` on 2026-10-29.

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use crate::mpv::{Event, Handle, OpenFn, Param, Render};

#[cfg(windows)]
const NAMES: &[&str] = &["libmpv-2.dll", "mpv-2.dll"];
#[cfg(not(windows))]
// libmpv.so.1: Ubuntu 22.04 (mpv 0.34, checked in a container on 2026-10-27: same calls work).
const NAMES: &[&str] = &["libmpv.so.2", "libmpv.so.1", "libmpv.so"];

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
    pub(crate) _lib: libloading::Library,
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

fn load() -> Result<Api, String> {
    // Debug only: a computer without libmpv, to see the download (EYECANSEE_NO_MPV=1).
    if cfg!(debug_assertions) && std::env::var_os("EYECANSEE_NO_MPV").is_some() {
        return Err("EYECANSEE_NO_MPV".into());
    }
    // SAFETY: libmpv runs no code of its own when loaded; the signatures are those of client.h and render.h.
    unsafe {
        // The system's (or next to the exe), then the downloaded component.
        let component = crate::component::VIDEO.as_ref().and_then(crate::component::path).filter(|p| p.is_file());
        let lib = NAMES
            .iter()
            .find_map(|n| libloading::Library::new(n).ok())
            .or_else(|| component.and_then(|p| libloading::Library::new(p).ok()))
            .ok_or_else(|| format!("{} not found", NAMES[0]))?;
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
