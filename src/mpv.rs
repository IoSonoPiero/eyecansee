//! Videos with libmpv, opened at the first video rather than linked: where it is missing
//! EyeCanSee still starts and videos show an error. Linux: the system's libmpv, in every package
//! (decoders under patents, HEVC above all, stay out of ours); Windows: `libmpv-2.dll` next to
//! the exe. Frames come from mpv's software renderer (render API "sw"),
//! the same with any egui backend; mpv decodes (with the GPU when it can) and plays the sound.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui::{Color32, ColorImage};
use image::RgbaImage;

// From mpv's client.h and render.h (API 2.x).
const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const PARAM_API_TYPE: c_int = 1;
const PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
const UPDATE_FRAME: u64 = 1;
const ERROR_GENERIC: i64 = -20;

#[cfg(windows)]
const NAMES: &[&str] = &["libmpv-2.dll", "mpv-2.dll"];
#[cfg(not(windows))]
// libmpv.so.1: Ubuntu 22.04 (mpv 0.34, checked in a container on 2026-10-27: same calls work).
const NAMES: &[&str] = &["libmpv.so.2", "libmpv.so.1", "libmpv.so"];

type Handle = *mut c_void;
type Render = *mut c_void;
type Update = Box<dyn Fn() + Send + Sync>;

#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply: u64,
    data: *mut c_void,
}

struct Api {
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    set_option_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, *const *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    wait_event: unsafe extern "C" fn(Handle, f64) -> *const Event,
    terminate_destroy: unsafe extern "C" fn(Handle),
    render_create: unsafe extern "C" fn(*mut Render, Handle, *mut Param) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(Render, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    render_update: unsafe extern "C" fn(Render) -> u64,
    render: unsafe extern "C" fn(Render, *mut Param) -> c_int,
    render_free: unsafe extern "C" fn(Render),
    stream_cb_add_ro: unsafe extern "C" fn(Handle, *const c_char, *mut c_void, OpenFn) -> c_int,
    free: unsafe extern "C" fn(*mut c_void),
    _lib: libloading::Library,
}

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(load).as_ref().map_err(Clone::clone)
}

fn load() -> Result<Api, String> {
    // SAFETY: libmpv runs no code of its own when loaded; the signatures are those of client.h and render.h.
    unsafe {
        let lib = NAMES.iter().find_map(|n| libloading::Library::new(n).ok()).ok_or_else(|| format!("{} not found", NAMES[0]))?;
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

fn cstr(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|e| e.to_string())
}

unsafe extern "C" fn on_update(data: *mut c_void) {
    // SAFETY: `data` is the `Update` boxed in `Mpv`, freed only after the render context.
    unsafe { (*(data as *const Update))() }
}

/// A video that is not a file on this computer (phone, server), read a piece at a time.
pub trait Reader: Send + Sync {
    fn size(&self) -> u64;
    /// `len` bytes from `offset`, inside the file. Called from mpv's threads: it may block.
    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String>;
}

/// What to play: a file mpv opens itself, or a `Reader`.
pub enum Input {
    File(PathBuf),
    Stream(Arc<dyn Reader>),
}

/// mpv_stream_cb_info (stream_cb.h).
#[repr(C)]
struct StreamInfo {
    cookie: *mut c_void,
    read: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, u64) -> i64>,
    seek: Option<unsafe extern "C" fn(*mut c_void, i64) -> i64>,
    size: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    close: Option<unsafe extern "C" fn(*mut c_void)>,
    cancel: Option<unsafe extern "C" fn(*mut c_void)>,
}

type OpenFn = unsafe extern "C" fn(*mut c_void, *mut c_char, *mut StreamInfo) -> c_int;

/// Bytes asked of the `Reader` at a time: mpv reads in small pieces, a phone answers each request slowly.
// ponytail: one block kept, no read-ahead; mpv's own cache covers playback
const BLOCK: u64 = 1 << 20;

/// An open stream: position and the last block read.
struct Cursor {
    reader: Arc<dyn Reader>,
    pos: u64,
    block: (u64, Vec<u8>),
}

impl Cursor {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, String> {
        let size = self.reader.size();
        if self.pos >= size {
            return Ok(0);
        }
        let (start, data) = &self.block;
        if self.pos < *start || self.pos >= start + data.len() as u64 {
            let data = self.reader.read(self.pos, BLOCK.min(size - self.pos))?;
            if data.is_empty() {
                return Err("empty read".into());
            }
            self.block = (self.pos, data);
        }
        let (start, data) = &self.block;
        let from = &data[(self.pos - start) as usize..];
        let n = from.len().min(out.len());
        out[..n].copy_from_slice(&from[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

unsafe extern "C" fn stream_open(user: *mut c_void, _uri: *mut c_char, info: *mut StreamInfo) -> c_int {
    // SAFETY: `user` is the `Arc<dyn Reader>` boxed in `Mpv`, alive as long as the core.
    let reader = unsafe { (*(user as *const Arc<dyn Reader>)).clone() };
    let cursor = Box::new(Cursor { reader, pos: 0, block: (0, Vec::new()) });
    // SAFETY: `info` is mpv's, to be filled here.
    unsafe {
        *info = StreamInfo {
            cookie: Box::into_raw(cursor) as *mut c_void,
            read: Some(stream_read),
            seek: Some(stream_seek),
            size: Some(stream_size),
            close: Some(stream_close),
            cancel: None,
        };
    }
    0
}

unsafe extern "C" fn stream_read(cookie: *mut c_void, buf: *mut c_char, n: u64) -> i64 {
    // SAFETY: the cookie is the `Cursor` of `stream_open`, used by one mpv thread at a time;
    // `buf` holds `n` bytes.
    let (cursor, out) = unsafe { (&mut *(cookie as *mut Cursor), std::slice::from_raw_parts_mut(buf as *mut u8, n as usize)) };
    match cursor.read(out) {
        Ok(n) => n as i64,
        Err(e) => {
            log!("video stream: ERROR {e}");
            -1
        }
    }
}

unsafe extern "C" fn stream_seek(cookie: *mut c_void, offset: i64) -> i64 {
    // SAFETY: as `stream_read`.
    let cursor = unsafe { &mut *(cookie as *mut Cursor) };
    if offset < 0 {
        return ERROR_GENERIC;
    }
    cursor.pos = offset as u64;
    offset
}

unsafe extern "C" fn stream_size(cookie: *mut c_void) -> i64 {
    // SAFETY: as `stream_read`.
    unsafe { (*(cookie as *const Cursor)).reader.size() as i64 }
}

unsafe extern "C" fn stream_close(cookie: *mut c_void) {
    // SAFETY: the box of `stream_open`, closed once.
    drop(unsafe { Box::from_raw(cookie as *mut Cursor) });
}

/// An mpv core with its software renderer. `update` is called (from an mpv thread) when there is
/// a frame to draw: it must only wake someone up, never call mpv.
struct Mpv {
    api: &'static Api,
    handle: Handle,
    render: Render,
    _update: Box<Update>,
    /// The `Reader` of a stream, freed after the core (`Drop` first, then the fields).
    reader: Option<Box<Arc<dyn Reader>>>,
}

// SAFETY: the client API is thread-safe; the render calls are made from one thread at a time
// (the player's thread, or the only thread of `still`).
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Mpv {
    fn new(options: &[(&str, &str)], update: impl Fn() + Send + Sync + 'static) -> Result<Mpv, String> {
        let api = api()?;
        // SAFETY: calls as documented in client.h and render.h; `Drop` frees what was created.
        unsafe {
            let handle = (api.create)();
            if handle.is_null() {
                return Err("mpv_create failed".into());
            }
            let mut mpv = Mpv { api, handle, render: std::ptr::null_mut(), _update: Box::new(Box::new(update)), reader: None };
            // libmpv reads no configuration files and takes no keys unless asked.
            for (k, v) in [("vo", "libmpv"), ("terminal", "no")].iter().chain(options) {
                (api.set_option_string)(handle, cstr(k)?.as_ptr(), cstr(v)?.as_ptr());
            }
            if (api.initialize)(handle) < 0 {
                return Err("mpv_initialize failed".into());
            }
            let mut params = [Param { kind: PARAM_API_TYPE, data: c"sw".as_ptr() as *mut c_void }, Param { kind: 0, data: std::ptr::null_mut() }];
            if (api.render_create)(&mut mpv.render, handle, params.as_mut_ptr()) < 0 {
                return Err("mpv_render_context_create failed".into());
            }
            let data = &*mpv._update as *const Update as *mut c_void;
            (api.render_set_update_callback)(mpv.render, Some(on_update), data);
            Ok(mpv)
        }
    }

    /// Starts loading `input`; a stream through the protocol "eyecansee://".
    fn load(&mut self, input: Input) -> Result<(), String> {
        let url = match input {
            Input::File(path) => path.to_string_lossy().into_owned(),
            Input::Stream(reader) => {
                let reader = self.reader.insert(Box::new(reader));
                let user = &**reader as *const Arc<dyn Reader> as *mut c_void;
                // SAFETY: `user` lives as long as the core, `stream_open` matches `OpenFn`.
                let r = unsafe { (self.api.stream_cb_add_ro)(self.handle, c"eyecansee".as_ptr(), user, stream_open) };
                if r < 0 {
                    return Err(format!("mpv_stream_cb_add_ro: error {r}"));
                }
                "eyecansee://video".into()
            }
        };
        self.command(&["loadfile", &url])
    }

    fn command(&self, args: &[&str]) -> Result<(), String> {
        let args = args.iter().map(|a| cstr(a)).collect::<Result<Vec<_>, _>>()?;
        let mut ptrs: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        // SAFETY: a NULL-terminated array of strings that outlive the call.
        let r = unsafe { (self.api.command)(self.handle, ptrs.as_ptr()) };
        if r < 0 { Err(format!("mpv {}: error {r}", args[0].to_string_lossy())) } else { Ok(()) }
    }

    fn property<T: Default>(&self, name: &str, format: c_int) -> Option<T> {
        let name = cstr(name).ok()?;
        let mut value = T::default();
        // SAFETY: `T` matches `format` at every call (f64 for DOUBLE, i64 for INT64, c_int for FLAG).
        let r = unsafe { (self.api.get_property)(self.handle, name.as_ptr(), format, &mut value as *mut T as *mut c_void) };
        (r >= 0).then_some(value)
    }

    fn string(&self, name: &str) -> Option<String> {
        let mut value: *mut c_char = std::ptr::null_mut();
        let name = cstr(name).ok()?;
        // SAFETY: STRING gives a C string owned by mpv, copied and then freed with mpv_free.
        unsafe {
            if (self.api.get_property)(self.handle, name.as_ptr(), FORMAT_STRING, &mut value as *mut *mut c_char as *mut c_void) < 0 || value.is_null() {
                return None;
            }
            let s = std::ffi::CStr::from_ptr(value).to_string_lossy().into_owned();
            (self.api.free)(value as *mut c_void);
            Some(s)
        }
    }

    fn wait(&self, timeout: f64) -> c_int {
        // SAFETY: the event stays valid until the next call; only its id is read.
        unsafe { (*(self.api.wait_event)(self.handle, timeout)).id }
    }

    fn update(&self) -> u64 {
        // SAFETY: see `Send` above.
        unsafe { (self.api.render_update)(self.render) }
    }

    /// Size of the video as shown (aspect ratio and rotation applied).
    fn size(&self) -> Option<[u32; 2]> {
        let w = self.property::<i64>("dwidth", FORMAT_INT64)?;
        let h = self.property::<i64>("dheight", FORMAT_INT64)?;
        (w > 0 && h > 0).then_some([w as u32, h as u32])
    }

    /// The current frame scaled to `size`, opaque RGBA. `wait`: until the frame's time comes.
    fn draw(&self, size: [u32; 2], wait: bool) -> RgbaImage {
        let [w, h] = size;
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        self.draw_into(size, wait, &mut pixels);
        RgbaImage::from_raw(w, h, pixels).expect("w × h × 4 bytes")
    }

    /// As `draw`, straight into the pixels of a texture: no conversion left for the window
    /// (opaque, so premultiplied or not is the same).
    fn draw_frame(&self, size: [u32; 2], wait: bool) -> ColorImage {
        let [w, h] = size;
        let mut pixels = vec![Color32::BLACK; w as usize * h as usize];
        // SAFETY: Color32 is four u8 in the order r, g, b, a (repr(C)).
        let bytes = unsafe { std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 4) };
        self.draw_into(size, wait, bytes);
        ColorImage::new([w as usize, h as usize], pixels)
    }

    // ponytail: stride w×4, not aligned to 64 as mpv prefers; align it if rendering turns out slow
    fn draw_into(&self, size: [u32; 2], wait: bool, pixels: &mut [u8]) {
        let [w, h] = size;
        let mut dims = [w as c_int, h as c_int];
        let mut stride = w as usize * 4;
        let mut block = c_int::from(wait);
        let mut params = [
            Param { kind: PARAM_SW_SIZE, data: dims.as_mut_ptr() as *mut c_void },
            Param { kind: PARAM_SW_FORMAT, data: c"rgb0".as_ptr() as *mut c_void },
            Param { kind: PARAM_SW_STRIDE, data: &mut stride as *mut usize as *mut c_void },
            Param { kind: PARAM_SW_POINTER, data: pixels.as_mut_ptr() as *mut c_void },
            Param { kind: PARAM_BLOCK_FOR_TARGET_TIME, data: &mut block as *mut c_int as *mut c_void },
            Param { kind: 0, data: std::ptr::null_mut() },
        ];
        // SAFETY: `pixels` holds stride × h bytes, the parameters outlive the call.
        unsafe { (self.api.render)(self.render, params.as_mut_ptr()) };
        // "rgb0": the fourth byte is undefined.
        pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: the render context goes before the core (render.h), each freed once.
        unsafe {
            if !self.render.is_null() {
                (self.api.render_free)(self.render);
            }
            (self.api.terminate_destroy)(self.handle);
        }
    }
}

/// `size` reduced to fit in a `side` × `side` square (0: unchanged), at least 1 × 1.
pub fn fit(size: [u32; 2], side: u32) -> [u32; 2] {
    let [w, h] = size;
    if side == 0 || (w <= side && h <= side) {
        return size;
    }
    let scale = side as f64 / w.max(h) as f64;
    [((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1)]
}

/// A wake-up flag shared with mpv's update callback.
#[derive(Default)]
struct Signal {
    set: Mutex<bool>,
    cv: Condvar,
}

impl Signal {
    fn raise(&self) {
        *self.set.lock().unwrap() = true;
        self.cv.notify_all();
    }

    /// False if `timeout` passed without a signal.
    fn wait(&self, timeout: Duration) -> bool {
        let set = self.set.lock().unwrap();
        let (mut set, r) = self.cv.wait_timeout_while(set, timeout, |s| !*s).unwrap();
        *set = false;
        !r.timed_out()
    }
}

/// One frame at a tenth of the video (the first ones are often black), at most `side` on the
/// longer side (0: full size). For thumbnails and the slideshow.
pub fn still(input: Input, side: u32) -> Result<RgbaImage, String> {
    let signal = Arc::new(Signal::default());
    let s = signal.clone();
    // One decoding thread: stills are made many at a time, one per core. A small read buffer:
    // opening a fragmented MP4 (dashcam) reads every fragment header, each with a whole buffer
    // (26 MB read with the default 128k, 8.7 MB with 4k, same frame).
    let options = [("pause", "yes"), ("start", "10%"), ("aid", "no"), ("sid", "no"), ("hwdec", "no"), ("vd-lavc-threads", "1"), ("stream-buffer-size", "4k")];
    let mut mpv = Mpv::new(&options, move || s.raise())?;
    mpv.load(input)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let left = || deadline.saturating_duration_since(Instant::now());
    loop {
        if left().is_zero() {
            return Err("timeout".into());
        }
        match mpv.wait(left().as_secs_f64()) {
            EVENT_PLAYBACK_RESTART => break,
            EVENT_END_FILE | EVENT_SHUTDOWN => return Err("mpv cannot play the file".into()),
            _ => {}
        }
    }
    // The seek is done; the frame reaches the renderer shortly after.
    while mpv.update() & UPDATE_FRAME == 0 {
        if !signal.wait(left()) {
            return Err("timeout".into());
        }
    }
    let size = mpv.size().ok_or("no video")?;
    Ok(mpv.draw(fit(size, side), false))
}

/// What the file says about itself, read without decoding (container and tracks).
#[derive(Debug, Default)]
pub struct Info {
    /// Container tags found among `TAGS` (key as written there, value).
    pub tags: Vec<(&'static str, String)>,
    pub duration: Option<f64>,
    /// First video track: codec name ("hevc"), stored size, rotation in degrees, frames a second.
    pub video: Option<(String, [i64; 2], i64, Option<f64>)>,
    /// First audio track: codec name, sample rate, channels.
    pub audio: Option<(String, Option<i64>, Option<i64>)>,
}

/// Container tags shown in the metadata panel: phones (Android, iPhone), cameras, drones.
pub const TAGS: &[&str] = &[
    "creation_time",
    "location",
    "com.apple.quicktime.location.ISO6709",
    "com.apple.quicktime.make",
    "com.apple.quicktime.model",
    "com.apple.quicktime.software",
    "com.android.manufacturer",
    "com.android.model",
    "com.android.version",
    "make",
    "model",
    "encoder",
];

/// The tags and tracks of a video; no frame is decoded.
pub fn info(input: Input) -> Result<Info, String> {
    let options = [("pause", "yes"), ("aid", "no"), ("sid", "no"), ("stream-buffer-size", "4k")];
    let mut mpv = Mpv::new(&options, || {})?;
    mpv.load(input)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("timeout".into());
        }
        match mpv.wait(left.as_secs_f64()) {
            EVENT_FILE_LOADED => break,
            EVENT_END_FILE | EVENT_SHUTDOWN => return Err("mpv cannot open the file".into()),
            _ => {}
        }
    }
    let mut info = Info { duration: mpv.property("duration", FORMAT_DOUBLE), ..Info::default() };
    for &key in TAGS {
        if let Some(v) = mpv.string(&format!("metadata/by-key/{key}")).filter(|v| !v.trim().is_empty()) {
            info.tags.push((key, v.trim().to_string()));
        }
    }
    for i in 0..mpv.property::<i64>("track-list/count", FORMAT_INT64).unwrap_or(0) {
        let t = |k: &str| format!("track-list/{i}/{k}");
        let codec = mpv.string(&t("codec")).unwrap_or_default();
        match mpv.string(&t("type")).as_deref() {
            Some("video") if info.video.is_none() && mpv.property::<c_int>(&t("image"), FORMAT_FLAG) != Some(1) => {
                let size = [mpv.property(&t("demux-w"), FORMAT_INT64).unwrap_or(0), mpv.property(&t("demux-h"), FORMAT_INT64).unwrap_or(0)];
                let rotation = mpv.property(&t("demux-rotation"), FORMAT_INT64).unwrap_or(0);
                info.video = Some((codec, size, rotation, mpv.property(&t("demux-fps"), FORMAT_DOUBLE)));
            }
            Some("audio") if info.audio.is_none() => {
                info.audio = Some((codec, mpv.property(&t("demux-samplerate"), FORMAT_INT64), mpv.property(&t("demux-channel-count"), FORMAT_INT64)));
            }
            _ => {}
        }
    }
    Ok(info)
}

#[derive(Default)]
struct State {
    /// mpv has a new frame, or the size changed.
    dirty: bool,
    size: [u32; 2],
    frame: Option<ColorImage>,
    quit: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

impl Shared {
    fn change(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.state.lock().unwrap());
        self.cv.notify_all();
    }
}

/// A video playing (with sound), its frames drawn by a thread of its own at the size the
/// window asks (`set_size`).
pub struct Player {
    mpv: Arc<Mpv>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// `repaint` is called at every new frame, from the player's thread.
    pub fn open(input: Input, repaint: impl Fn() + Send + 'static) -> Result<Player, String> {
        let shared = Arc::new(Shared::default());
        let s = shared.clone();
        // keep-open: at the end the last frame stays, as in a player.
        // audio-client-name: the name in the system's volume control.
        // sub-auto: no subtitle files from beside the video; a DJI drone's .SRT (telemetry of
        // every frame) covered half of the picture (2026-10-27).
        let options = [("hwdec", "auto-copy-safe"), ("keep-open", "yes"), ("audio-client-name", "EyeCanSee"), ("sub-auto", "no")];
        let mut mpv = Mpv::new(&options, move || s.change(|st| st.dirty = true))?;
        mpv.load(input)?;
        let mpv = Arc::new(mpv);
        let (m, s) = (mpv.clone(), shared.clone());
        let thread = std::thread::spawn(move || draw_loop(&m, &s, repaint));
        Ok(Player { mpv, shared, thread: Some(thread) })
    }

    /// The frame drawn since the last call, if any.
    pub fn frame(&self) -> Option<ColorImage> {
        self.shared.state.lock().unwrap().frame.take()
    }

    /// Pixels the frames are drawn at: the video fitted to the window.
    pub fn set_size(&self, size: [u32; 2]) {
        if self.shared.state.lock().unwrap().size != size {
            self.shared.change(|st| (st.size, st.dirty) = (size, true));
        }
    }

    pub fn video_size(&self) -> Option<[u32; 2]> {
        self.mpv.size()
    }

    pub fn position(&self) -> f64 {
        self.mpv.property("time-pos", FORMAT_DOUBLE).unwrap_or(0.0)
    }

    pub fn duration(&self) -> f64 {
        self.mpv.property("duration", FORMAT_DOUBLE).unwrap_or(0.0)
    }

    pub fn paused(&self) -> bool {
        self.mpv.property::<c_int>("pause", FORMAT_FLAG).unwrap_or(0) != 0
    }

    /// Played to the end (the last frame stays, see keep-open).
    pub fn ended(&self) -> bool {
        self.mpv.property::<c_int>("eof-reached", FORMAT_FLAG).unwrap_or(0) != 0
    }

    /// Play or pause; at the end, play from the start.
    pub fn toggle(&self) {
        if self.ended() {
            let _ = self.mpv.command(&["seek", "0", "absolute"]);
            let _ = self.mpv.command(&["set", "pause", "no"]);
        } else {
            let _ = self.mpv.command(&["cycle", "pause"]);
        }
    }

    /// To `seconds` from the start; `exact` false jumps to the nearest keyframe (fast, while dragging).
    pub fn seek(&self, seconds: f64, exact: bool) {
        let how = if exact { "absolute+exact" } else { "absolute+keyframes" };
        let _ = self.mpv.command(&["seek", &format!("{seconds:.3}"), how]);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shared.change(|st| st.quit = true);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn draw_loop(mpv: &Mpv, shared: &Shared, repaint: impl Fn()) {
    loop {
        let size = {
            let mut st = shared.state.lock().unwrap();
            while !st.dirty && !st.quit {
                st = shared.cv.wait(st).unwrap();
            }
            if st.quit {
                return;
            }
            st.dirty = false;
            st.size
        };
        // Called at every wake-up, as render.h asks, even when nothing is drawn.
        mpv.update();
        if size[0] == 0 || size[1] == 0 {
            continue;
        }
        let frame = mpv.draw_frame(size, true);
        shared.state.lock().unwrap().frame = Some(frame);
        repaint();
    }
}

/// "m:ss" or "h:mm:ss".
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file read through `Reader`, as a phone would be.
    struct Bytes(Vec<u8>);

    impl Reader for Bytes {
        fn size(&self) -> u64 {
            self.0.len() as u64
        }
        fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
            Ok(self.0[offset as usize..(offset + len) as usize].to_vec())
        }
    }

    struct Slow(Bytes, bool);

    impl Reader for Slow {
        fn size(&self) -> u64 {
            self.0.size()
        }
        fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
            if self.1 {
                std::thread::sleep(Duration::from_millis(15) + Duration::from_secs_f64(len as f64 / 30e6));
            }
            self.0.read(offset, len)
        }
    }

    #[test]
    fn cursor_reads_across_blocks() {
        let data: Vec<u8> = (0..BLOCK as usize * 2 + 100).map(|i| (i % 251) as u8).collect();
        let mut c = Cursor { reader: Arc::new(Bytes(data.clone())), pos: BLOCK - 10, block: (0, Vec::new()) };
        let mut out = vec![0; 30];
        assert_eq!(c.read(&mut out), Ok(30)); // block read from BLOCK - 10
        assert_eq!(out, data[BLOCK as usize - 10..BLOCK as usize + 20]);
        c.pos = BLOCK * 2 - 15;
        assert_eq!(c.read(&mut out), Ok(5)); // to the end of that block
        assert_eq!(c.read(&mut out), Ok(30)); // the next one
        assert_eq!(out, data[BLOCK as usize * 2 - 10..BLOCK as usize * 2 + 20]);
        c.pos = BLOCK * 2 + 90;
        assert_eq!(c.read(&mut out), Ok(10));
        assert_eq!(out[..10], data[data.len() - 10..]);
        assert_eq!(c.read(&mut out), Ok(0)); // end of file
    }

    #[test]
    fn sizes_and_clock() {
        assert_eq!(fit([2560, 1440], 256), [256, 144]);
        assert_eq!(fit([1080, 1920], 256), [144, 256]);
        assert_eq!(fit([100, 50], 256), [100, 50]);
        assert_eq!(fit([2560, 1440], 0), [2560, 1440]);
        assert_eq!(clock(65.9), "1:05");
        assert_eq!(clock(3725.0), "1:02:05");
    }

    fn player_size(player: &Player) -> [u32; 2] {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) {
            if let Some(s) = player.video_size() {
                return s;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no video size");
    }

    /// `EYECANSEE_VIDEO=<file> [EYECANSEE_VIDEO_SIZE=<longer side drawn>] cargo test --release video_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn video_live() {
        let path = std::path::PathBuf::from(std::env::var("EYECANSEE_VIDEO").expect("EYECANSEE_VIDEO"));
        let t = Instant::now();
        let img = still(Input::File(path.clone()), 256).unwrap();
        println!("still {}x{} in {:?}", img.width(), img.height(), t.elapsed());
        println!("{:?}", info(Input::File(path.clone())).unwrap()); // the metadata panel reads this
        if let Ok(out) = std::env::var("EYECANSEE_VIDEO_STILL") {
            img.save(out).unwrap();
        }
        assert!(img.pixels().any(|p| p.0[..3] != [0, 0, 0]), "black frame");
        let t = Instant::now();
        // As a phone: ~30 MB/s and some milliseconds for every request (EYECANSEE_VIDEO_SLOW=1).
        let slow = std::env::var_os("EYECANSEE_VIDEO_SLOW").is_some();
        let stream = || Input::Stream(Arc::new(Slow(Bytes(std::fs::read(&path).unwrap()), slow)));
        let img = still(stream(), 256).unwrap();
        println!("still through a Reader {}x{} in {:?}", img.width(), img.height(), t.elapsed());
        let player = Player::open(stream(), || {}).unwrap();
        let side = std::env::var("EYECANSEE_VIDEO_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(1280);
        player.set_size(fit(player_size(&player), side));
        let t = Instant::now();
        let mut frames = 0;
        while t.elapsed() < Duration::from_secs(3) {
            if player.frame().is_some() {
                frames += 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        println!("{frames} frames in 3 s, position {:.2} of {:.1} s, fps {:?}", player.position(), player.duration(), player.mpv.property::<f64>("container-fps", FORMAT_DOUBLE));
        assert!(frames > 30, "too few frames");
    }
}
