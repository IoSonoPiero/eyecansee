//! Videos with libmpv, opened at the first video rather than linked: where it is missing
//! EyeCanSee still starts and videos show an error. Linux: the system's libmpv, in every package
//! (decoders under patents, HEVC above all, stay out of ours); Windows: `libmpv-2.dll` next to
//! the exe. Frames come from mpv's software renderer (render API "sw"),
//! the same with any egui backend; mpv decodes (with the GPU when it can) and plays the sound.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui::{Color32, ColorImage};
use image::RgbaImage;

use crate::mpv_load::{Api, api};
pub use crate::mpv_load::{available, status};
pub(crate) use crate::mpv_stream::OpenFn;
pub use crate::mpv_stream::Reader;
use crate::mpv_stream::stream_open;

// From mpv's client.h and render.h (API 2.x).
pub(crate) const FORMAT_STRING: c_int = 1;
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

pub(crate) type Handle = *mut c_void;
pub(crate) type Render = *mut c_void;
type Update = Box<dyn Fn() + Send + Sync>;

#[repr(C)]
pub(crate) struct Param {
    kind: c_int,
    data: *mut c_void,
}

#[repr(C)]
pub(crate) struct Event {
    id: c_int,
    error: c_int,
    reply: u64,
    data: *mut c_void,
}

fn cstr(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|e| e.to_string())
}

unsafe extern "C" fn on_update(data: *mut c_void) {
    // SAFETY: `data` is the `Update` boxed in `Mpv`, freed only after the render context.
    unsafe { (*(data as *const Update))() }
}

/// What to play: a file mpv opens itself, or a `Reader`.
pub enum Input {
    File(PathBuf),
    Stream(Arc<dyn Reader>),
}

/// Every file is untrusted: a file named as a video can be a playlist that names web addresses
/// or other files (audit 4.6, #185). access-references=no: mpv expands no playlist and FFmpeg
/// opens nothing a file names (HLS segments, MOV references, ordered chapters, archives); the
/// file itself still opens, and EyeCanSee gives mpv only local paths and "eyecansee://" streams.
/// ytdl=no: no yt-dlp run on an address. load-scripts=no: no user scripts (libmpv already reads
/// no configuration). All in mpv's manual since 0.34 (the oldest we take, see `MIN_API`).
const UNTRUSTED: &[(&str, &str)] = &[("access-references", "no"), ("ytdl", "no"), ("load-scripts", "no")];

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
            // video-rotate=no: the software renderer crops with the rotated rectangle but scales the
            // unrotated frame (libmpv_sw.c, mpv 0.41): a phone's portrait video (rotation 90 in
            // the file) aborted on an assertion. The rotation is applied by us: see `rotation`.
            for (k, v) in [("vo", "libmpv"), ("terminal", "no"), ("video-rotate", "no")].iter().chain(UNTRUSTED).chain(options) {
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

    /// Degrees clockwise the file asks to turn its frames by (0, 90, 180, 270), ignored by
    /// mpv because of video-rotate=no.
    fn rotation(&self) -> i64 {
        self.property::<i64>("current-tracks/video/demux-rotation", FORMAT_INT64).unwrap_or(0).rem_euclid(360)
    }

    /// Size of the video as shown (aspect ratio applied, rotation not: see `rotation`).
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
        let mut pixels = zeroed_pixels(w as usize * h as usize);
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

/// `n` pixels to draw into, from memory the system gives already zeroed: no pass to fill them
/// first, which mpv overwrites (2 ms a 4K frame, 0.4 ms at 2560x1440 on IronManL, #17).
fn zeroed_pixels(n: usize) -> Vec<Color32> {
    if n == 0 {
        return Vec::new();
    }
    let layout = std::alloc::Layout::array::<Color32>(n).expect("frame size");
    // SAFETY: zeroed bytes are a valid Color32 (four u8); the Vec gets the pointer with the length
    // and capacity of the layout it was allocated with, so it frees it with the same layout.
    unsafe {
        let ptr = std::alloc::alloc_zeroed(layout) as *mut Color32;
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        Vec::from_raw_parts(ptr, n, n)
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        let this = &*self;
        let end = move || {
            // SAFETY: the render context goes before the core (render.h), each freed once.
            unsafe {
                if !this.render.is_null() {
                    (this.api.render_free)(this.render);
                }
                (this.api.terminate_destroy)(this.handle);
            }
        };
        // On Windows, on a thread of its own. The last handle destroys mpv's core on the thread
        // that frees it, audio output included, and WASAPI's `uninit` (ao_wasapi.c) calls
        // `CoUninitialize` there, though its `CoInitializeEx` was on mpv's own thread. On the
        // window's thread each video with sound closed undid one of the window's own COM
        // initializations: after two or three, the taskbar object winit keeps for full screen was
        // gone, and the next F11 or slideshow crashed in `MarkFullscreenWindow` (access
        // violation; Piero on warmachinew, 2026-11-18: F11 twice, a video opened and closed three
        // times, F11).
        if cfg!(windows) {
            std::thread::scope(|s| {
                s.spawn(end);
            });
        } else {
            end();
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

/// One frame of the video, at most `side` on the longer side. For a thumbnail, at a tenth of the
/// video (the first ones are often black). `side` 0, the slideshow: at full size, and the first
/// frame, the one the video starts playing from (at a tenth, the picture jumped as the video
/// began: Piero, 2026-11-18).
pub fn still(input: Input, side: u32) -> Result<RgbaImage, String> {
    let signal = Arc::new(Signal::default());
    let s = signal.clone();
    // One decoding thread: stills are made many at a time, one per core. A small read buffer:
    // opening a fragmented MP4 (dashcam) reads every fragment header, each with a whole buffer
    // (26 MB read with the default 128k, 8.7 MB with 4k, same frame). autoload-files=no: the
    // folder is not listed for subtitles, audio and covers beside the video.
    let start = if side == 0 { "0%" } else { "10%" };
    let options = [("pause", "yes"), ("start", start), ("aid", "no"), ("sid", "no"), ("hwdec", "no"), ("vd-lavc-threads", "1"), ("stream-buffer-size", "4k"), ("autoload-files", "no")];
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
    let img = mpv.draw(fit(size, side), false);
    Ok(match mpv.rotation() {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    })
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
    "speed_average_kmh",
    "speed_top_kmh",
    "date_recorded",
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
    // Silent, not without audio: an audio file with "aid=no" has nothing to open (2026-10-30).
    // autoload-files=no: as in `still`.
    let options = [("pause", "yes"), ("ao", "null"), ("sid", "no"), ("stream-buffer-size", "4k"), ("autoload-files", "no")];
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
    /// Who asked for the frame shown at the video's own size (`Player::grab`).
    grab: Option<Grab>,
    /// Frames drawn, frames drawn over before the window took them, time spent drawing, the
    /// longest draw (#17, written in the log when the video closes).
    drawn: u64,
    lost: u64,
    drawing: Duration,
    longest: Duration,
}

/// Called with the frame at the video's own size, from the player's thread.
type Grab = Box<dyn FnOnce(RgbaImage) + Send>;

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
    /// `repaint` is called at every new frame, from the player's thread. `volume` (0-100) and
    /// `mute` are set before the first frame plays.
    pub fn open(input: Input, volume: u32, mute: bool, repaint: impl Fn() + Send + 'static) -> Result<Player, String> {
        let shared = Arc::new(Shared::default());
        let s = shared.clone();
        // keep-open: at the end the last frame stays, as in a player.
        // audio-client-name: the name in the system's volume control.
        // sub-auto: no subtitle files from beside the video; a DJI drone's .SRT (telemetry of
        // every frame) covered half of the picture (2026-10-27).
        let volume = volume.min(100).to_string();
        let mute = if mute { "yes" } else { "no" };
        let options = [("hwdec", "auto-copy-safe"), ("keep-open", "yes"), ("audio-client-name", "EyeCanSee"), ("sub-auto", "no"), ("volume", &volume), ("mute", mute)];
        let mut mpv = Mpv::new(&options, move || s.change(|st| st.dirty = true))?;
        mpv.load(input)?;
        let mpv = Arc::new(mpv);
        let (m, s) = (mpv.clone(), shared.clone());
        let thread = crate::logging::spawn_named("mpv-draw", move || draw_loop(&m, &s, repaint));
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

    /// How the playback went, in the log when the video closes (#17): frames drawn and lost on
    /// our side, mpv's own counts, the decoding.
    fn log_stats(&self) {
        let st = self.shared.state.lock().unwrap();
        let (drawn, lost, drawing, longest) = (st.drawn, st.lost, st.drawing, st.longest);
        drop(st);
        let int = |p: &str| self.mpv.property::<i64>(p, FORMAT_INT64).unwrap_or(-1);
        let text = |p: &str| self.mpv.string(p).unwrap_or_default();
        log!(
            "video stats: {drawn} frames drawn up to {:.1} s, {lost} drawn over before shown, a draw {:.1} ms on average with its wait (longest {:.1}), mpv dropped {} (decoder {}), delayed {}, {}x{} {} at {} fps, hwdec {}",
            self.position(),
            drawing.as_secs_f64() * 1000.0 / drawn.max(1) as f64,
            longest.as_secs_f64() * 1000.0,
            int("frame-drop-count"),
            int("decoder-frame-drop-count"),
            int("vo-delayed-frame-count"),
            int("width"),
            int("height"),
            text("video-codec"),
            text("container-fps"),
            text("hwdec-current"),
        );
    }

    pub fn pause(&self) {
        let _ = self.mpv.command(&["set", "pause", "yes"]);
    }

    /// The frame shown, at the video's own size (aspect ratio and the file's rotation applied, a
    /// turn not saved yet not: `video_frame::convert_frame` gives it), to
    /// `done` from the player's thread (#16). The software renderer draws it again from the frame
    /// it keeps; `done` is dropped without a call if there is no picture (an audio file).
    pub fn grab(&self, done: impl FnOnce(RgbaImage) + Send + 'static) {
        self.shared.change(|st| st.grab = Some(Box::new(done)));
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

    /// The file is loaded and plays a sound track; false while loading and for a video without sound.
    pub fn has_audio(&self) -> bool {
        self.mpv.property::<i64>("current-tracks/audio/id", FORMAT_INT64).is_some()
    }

    /// 0-100, as the volume slider.
    pub fn set_volume(&self, volume: u32) {
        let _ = self.mpv.command(&["set", "volume", &volume.min(100).to_string()]);
    }

    pub fn set_mute(&self, mute: bool) {
        let _ = self.mpv.command(&["set", "mute", if mute { "yes" } else { "no" }]);
    }

}

impl Drop for Player {
    fn drop(&mut self) {
        self.log_stats();
        self.shared.change(|st| st.quit = true);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The frames turned by the file's rotation, by a filter. A turn not saved yet (#125) is not
/// mpv's: the window turns the frames it draws (`video::paint_fitted`), at once (#168).
fn turn_frames(mpv: &Mpv) {
    let filter = match mpv.rotation() {
        90 => "transpose=clock",
        180 => "hflip,vflip",
        270 => "transpose=cclock",
        _ => "",
    };
    let _ = if filter.is_empty() { mpv.command(&["vf", "clr", ""]) } else { mpv.command(&["vf", "set", filter]) };
}

fn draw_loop(mpv: &Mpv, shared: &Shared, repaint: impl Fn()) {
    // The rotation is known once the file is loaded; a filter turns every frame from then on
    // (a frame decoded before is filtered again by mpv). `dwidth`/`dheight` follow the filter.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || shared.state.lock().unwrap().quit {
            return;
        }
        match mpv.wait(left.as_secs_f64().min(0.5)) {
            EVENT_FILE_LOADED => break,
            EVENT_END_FILE | EVENT_SHUTDOWN => return,
            _ => {}
        }
    }
    turn_frames(mpv);
    loop {
        let (size, grab) = {
            let mut st = shared.state.lock().unwrap();
            while !st.dirty && !st.quit && st.grab.is_none() {
                st = shared.cv.wait(st).unwrap();
            }
            if st.quit {
                return;
            }
            let dirty = std::mem::take(&mut st.dirty);
            (if dirty { st.size } else { [0, 0] }, st.grab.take())
        };
        // Called at every wake-up, as render.h asks, even when nothing is drawn.
        mpv.update();
        if let (Some(grab), Some(full)) = (grab, mpv.size()) {
            grab(mpv.draw(full, false));
        }
        if size[0] == 0 || size[1] == 0 {
            continue;
        }
        let start = Instant::now();
        let frame = mpv.draw_frame(size, true);
        let took = start.elapsed();
        let mut st = shared.state.lock().unwrap();
        st.lost += u64::from(st.frame.is_some());
        st.frame = Some(frame);
        st.drawn += 1;
        st.drawing += took;
        st.longest = st.longest.max(took);
        drop(st);
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

    /// The pixels a frame is drawn into: as many as asked, all zero, freed as any Vec.
    #[test]
    fn pixels_come_zeroed() {
        let p = zeroed_pixels(1920 * 1080);
        assert_eq!(p.len(), 1920 * 1080);
        assert!(p.iter().all(|c| *c == Color32::TRANSPARENT));
        assert!(zeroed_pixels(0).is_empty());
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
        let player = Player::open(stream(), 100, false, || {}).unwrap();
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

    /// `EYECANSEE_VIDEO=<file> cargo test --release volume_live -- --ignored --nocapture`: volume and
    /// mute given at the opening are those of the first frame, and change while it plays.
    #[test]
    #[ignore]
    fn volume_live() {
        let path = std::path::PathBuf::from(std::env::var("EYECANSEE_VIDEO").expect("EYECANSEE_VIDEO"));
        let player = Player::open(Input::File(path), 35, true, || {}).unwrap();
        player_size(&player);
        let volume = || player.mpv.property::<f64>("volume", FORMAT_DOUBLE);
        let mute = || player.mpv.property::<c_int>("mute", FORMAT_FLAG);
        println!("audio {}, volume {:?}, mute {:?}", player.has_audio(), volume(), mute());
        assert_eq!((volume(), mute()), (Some(35.0), Some(1)));
        player.set_volume(70);
        player.set_mute(false);
        assert_eq!((volume(), mute()), (Some(70.0), Some(0)));
        player.set_volume(250);
        assert_eq!(volume(), Some(100.0));
    }

    /// How fast this computer decodes and draws a video with the software renderer, as fast as it
    /// can (#17): frames a second at the size of the window, `EYECANSEE_VIDEO_SIZE` (default
    /// 2560x1440), for 10 s; `EYECANSEE_HWDEC` as mpv's `hwdec`. `EYECANSEE_VIDEO=<file> cargo test --release render_speed_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn render_speed_live() {
        let file = std::path::PathBuf::from(std::env::var("EYECANSEE_VIDEO").expect("EYECANSEE_VIDEO"));
        let room = std::env::var("EYECANSEE_VIDEO_SIZE").ok().and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]))).unwrap_or([2560u32, 1440]);
        let signal = Arc::new(Signal::default());
        let s = signal.clone();
        // `EYECANSEE_HWDEC=no`: decoded by the processor too, as on a computer without a card that decodes it.
        let hwdec = std::env::var("EYECANSEE_HWDEC").unwrap_or_else(|_| "auto-copy-safe".into());
        // `EYECANSEE_MPV_OPTS=zimg-scaler=bilinear,zimg-dither=no`: more of mpv's options.
        let extra = std::env::var("EYECANSEE_MPV_OPTS").unwrap_or_default();
        let mut options = vec![("hwdec", hwdec.as_str()), ("ao", "null"), ("untimed", "yes"), ("video-sync", "desync")];
        options.extend(extra.split(',').filter_map(|o| o.split_once('=')));
        let mpv = Mpv::new(&options, move || s.raise()).unwrap();
        let mut mpv = mpv;
        mpv.load(Input::File(file)).unwrap();
        let start = Instant::now();
        let (mut frames, mut drawing, mut size) = (0u32, Duration::ZERO, None);
        while start.elapsed() < Duration::from_secs(10) {
            signal.wait(Duration::from_millis(100));
            if mpv.update() & UPDATE_FRAME == 0 {
                continue;
            }
            let Some(video) = size.or_else(|| mpv.size()) else { continue };
            size = Some(video);
            let scale = (room[0] as f64 / video[0] as f64).min(room[1] as f64 / video[1] as f64).min(1.0);
            let drawn = [(video[0] as f64 * scale) as u32, (video[1] as f64 * scale) as u32];
            let t = Instant::now();
            let _ = mpv.draw_frame(drawn, false);
            drawing += t.elapsed();
            frames += 1;
        }
        let secs = start.elapsed().as_secs_f64();
        println!(
            "{size:?} drawn into {room:?}: {frames} frames in {secs:.1} s = {:.1} fps, a draw {:.1} ms, hwdec {:?}, dropped {:?}",
            frames as f64 / secs,
            drawing.as_secs_f64() * 1000.0 / frames.max(1) as f64,
            mpv.string("hwdec-current"),
            mpv.property::<i64>("frame-drop-count", FORMAT_INT64)
        );
    }
}

#[cfg(test)]
mod info_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    /// Audit 4.6: a file named as a video whose content points at a web address: an M3U playlist
    /// and an mpv EDL (both read by mpv itself; FFmpeg does not take HLS from a ".mp4" name).
    /// Making its thumbnail, reading what it says about itself, or playing it must not call that
    /// address. Skipped without libmpv (the CI, Linux and Windows).
    #[test]
    fn audit_4_6_playlist_reference() {
        if !super::available() {
            println!("skipped: no libmpv");
            return;
        }
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        server.set_nonblocking(true).unwrap();
        let (calls, stop) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicBool::new(false)));
        let (c, s) = (calls.clone(), stop.clone());
        // Every call is counted and closed at once: mpv does not wait for an answer.
        let listener = std::thread::spawn(move || {
            while !s.load(Ordering::Relaxed) {
                let Ok((mut socket, _)) = server.accept() else {
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                };
                use std::io::Read;
                let mut request = [0u8; 200];
                socket.set_nonblocking(false).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
                let n = socket.read(&mut request).unwrap_or(0);
                println!("request: {:?}", String::from_utf8_lossy(&request[..n]));
                c.fetch_add(1, Ordering::Relaxed);
            }
        });
        // A call, if any, comes while the file opens; a moment more for the last one to arrive.
        let new_calls = |wait: Duration| {
            std::thread::sleep(wait);
            calls.swap(0, Ordering::Relaxed)
        };
        let dir = std::env::temp_dir().join(format!("eyecansee-audit-4-6-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let url = format!("http://127.0.0.1:{port}/clip.mp4");
        let files = [
            ("holiday.mp4", format!("#EXTM3U\n{url}\n")),
            ("trip.mp4", format!("# mpv EDL v0\n{url}\n")),
        ]
        .map(|(name, text)| {
            let file = dir.join(name);
            std::fs::write(&file, text).unwrap();
            file
        });
        // The thumbnail and the facts of the file, five times each (mpv is closed as soon as the
        // playlist ends: whether the address is reached could depend on who is faster).
        let (mut by_still, mut by_info) = (0, 0);
        for file in &files {
            for _ in 0..5 {
                let still = super::still(super::Input::File(file.clone()), 256).map(|i| i.dimensions());
                println!("still: {still:?}");
            }
            by_still += new_calls(Duration::from_millis(300));
            for _ in 0..5 {
                let info = super::info(super::Input::File(file.clone())).map(|i| i.duration);
                println!("info: {info:?}");
            }
            by_info += new_calls(Duration::from_millis(300));
        }
        // The viewer: the files opened as videos to play.
        let players: Vec<_> = files.iter().map(|f| super::Player::open(super::Input::File(f.clone()), 0, true, || {})).collect();
        let by_player = new_calls(Duration::from_secs(2));
        println!("players open: {:?}", players.iter().map(Result::is_ok).collect::<Vec<_>>());
        drop(players);
        stop.store(true, Ordering::Relaxed);
        listener.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        println!("address called: {by_still} times by the thumbnail, {by_info} by the facts, {by_player} by the player");
        assert_eq!((by_still, by_info, by_player), (0, 0, 0));
    }

    /// `EYECANSEE_VIDEO=<file> cargo test info_tags_live -- --ignored --nocapture`: the tags libmpv reports.
    #[test]
    #[ignore]
    fn info_tags_live() {
        let file = std::path::PathBuf::from(std::env::var("EYECANSEE_VIDEO").expect("EYECANSEE_VIDEO"));
        let info = super::info(super::Input::File(file)).unwrap();
        println!("tags: {:?}\nduration {:?} video {:?}", info.tags, info.duration, info.video);
    }

}
