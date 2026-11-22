//! Videos with libmpv, opened at the first video rather than linked: where it is missing
//! EyeCanSee still starts and videos show an error. Linux: the system's libmpv, in every package
//! (decoders under patents, HEVC above all, stay out of ours); Windows: `libmpv-2.dll` next to
//! the exe. Frames come from mpv's software renderer (render API "sw"),
//! the same with any egui backend; mpv decodes (with the GPU when it can) and plays the sound.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{Color32, ColorImage};
use image::RgbaImage;

use crate::mpv_load::{Api, api};
pub use crate::mpv_load::{available, status};
pub(crate) use crate::mpv_stream::OpenFn;
pub use crate::mpv_stream::Reader;
pub use crate::mpv_player::{Failure, Player};
use crate::mpv_stream::{Stream, stream_open};

// From mpv's client.h and render.h (API 2.x).
pub(crate) const FORMAT_STRING: c_int = 1;
pub(crate) const FORMAT_FLAG: c_int = 3;
pub(crate) const FORMAT_INT64: c_int = 4;
pub(crate) const FORMAT_DOUBLE: c_int = 5;
pub(crate) const EVENT_SHUTDOWN: c_int = 1;
pub(crate) const EVENT_END_FILE: c_int = 7;
pub(crate) const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PLAYBACK_RESTART: c_int = 21;
/// `mpv_end_file_reason`: the file could not be played.
pub(crate) const END_FILE_REASON_ERROR: c_int = 4;
const PARAM_API_TYPE: c_int = 1;
const PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
pub(crate) const UPDATE_FRAME: u64 = 1;

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
pub(crate) struct Mpv {
    api: &'static Api,
    handle: Handle,
    render: Render,
    _update: Box<Update>,
    /// The stream of a `Reader`, freed after the core (`Drop` first, then the fields).
    pub(crate) stream: Option<Arc<Stream>>,
}

// SAFETY: the client API is thread-safe; the render calls are made from one thread at a time
// (the player's thread, or the only thread of `still`).
unsafe impl Send for Mpv {}
unsafe impl Sync for Mpv {}

impl Mpv {
    pub(crate) fn new(options: &[(&str, &str)], update: impl Fn() + Send + Sync + 'static) -> Result<Mpv, String> {
        let api = api()?;
        // SAFETY: calls as documented in client.h and render.h; `Drop` frees what was created.
        unsafe {
            let handle = (api.create)();
            if handle.is_null() {
                return Err("mpv_create failed".into());
            }
            let mut mpv = Mpv { api, handle, render: std::ptr::null_mut(), _update: Box::new(Box::new(update)), stream: None };
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
    pub(crate) fn load(&mut self, input: Input) -> Result<(), String> {
        let url = match input {
            Input::File(path) => path.to_string_lossy().into_owned(),
            Input::Stream(reader) => {
                let stream = self.stream.insert(Arc::new(Stream::new(reader)));
                let user = Arc::as_ptr(stream) as *mut c_void;
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

    pub(crate) fn command(&self, args: &[&str]) -> Result<(), String> {
        let args = args.iter().map(|a| cstr(a)).collect::<Result<Vec<_>, _>>()?;
        let mut ptrs: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        // SAFETY: a NULL-terminated array of strings that outlive the call.
        let r = unsafe { (self.api.command)(self.handle, ptrs.as_ptr()) };
        if r < 0 { Err(format!("mpv {}: error {r}", args[0].to_string_lossy())) } else { Ok(()) }
    }

    pub(crate) fn property<T: Default>(&self, name: &str, format: c_int) -> Option<T> {
        let name = cstr(name).ok()?;
        let mut value = T::default();
        // SAFETY: `T` matches `format` at every call (f64 for DOUBLE, i64 for INT64, c_int for FLAG).
        let r = unsafe { (self.api.get_property)(self.handle, name.as_ptr(), format, &mut value as *mut T as *mut c_void) };
        (r >= 0).then_some(value)
    }

    pub(crate) fn string(&self, name: &str) -> Option<String> {
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
        self.next_event(timeout).0
    }

    /// The next event's id and, for `END_FILE`, its reason and mpv's error (`mpv_event_end_file`,
    /// whose first two fields are the same in every API we take).
    pub(crate) fn next_event(&self, timeout: f64) -> (c_int, Option<(c_int, c_int)>) {
        // SAFETY: the event and its data stay valid until the next call; read here only.
        unsafe {
            let event = &*(self.api.wait_event)(self.handle, timeout);
            let end = (event.id == EVENT_END_FILE && !event.data.is_null()).then(|| {
                let data = event.data as *const c_int;
                (*data, *data.add(1))
            });
            (event.id, end)
        }
    }

    pub(crate) fn update(&self) -> u64 {
        // SAFETY: see `Send` above.
        unsafe { (self.api.render_update)(self.render) }
    }

    /// Degrees clockwise the file asks to turn its frames by (0, 90, 180, 270), ignored by
    /// mpv because of video-rotate=no.
    pub(crate) fn rotation(&self) -> i64 {
        self.property::<i64>("current-tracks/video/demux-rotation", FORMAT_INT64).unwrap_or(0).rem_euclid(360)
    }

    /// Size of the video as shown (aspect ratio applied, rotation not: see `rotation`).
    pub(crate) fn size(&self) -> Option<[u32; 2]> {
        let w = self.property::<i64>("dwidth", FORMAT_INT64)?;
        let h = self.property::<i64>("dheight", FORMAT_INT64)?;
        (w > 0 && h > 0).then_some([w as u32, h as u32])
    }

    /// The current frame scaled to `size`, opaque RGBA. `wait`: until the frame's time comes.
    pub(crate) fn draw(&self, size: [u32; 2], wait: bool) -> RgbaImage {
        let [w, h] = size;
        let mut pixels = vec![0u8; w as usize * h as usize * 4];
        self.draw_into(size, wait, &mut pixels);
        RgbaImage::from_raw(w, h, pixels).expect("w × h × 4 bytes")
    }

    /// As `draw`, straight into the pixels of a texture: no conversion left for the window
    /// (opaque, so premultiplied or not is the same).
    pub(crate) fn draw_frame(&self, size: [u32; 2], wait: bool) -> ColorImage {
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

/// "m:ss" or "h:mm:ss".
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[cfg(test)]
pub(crate) mod tests {
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

    /// The bytes of `Bytes`, failing from `from` on, as a phone unplugged while it plays.
    struct Failing(Bytes, u64);

    impl Reader for Failing {
        fn size(&self) -> u64 {
            self.0.size()
        }
        fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
            if offset + len > self.1 { Err("error-device-gone\u{1}USB 3-2".into()) } else { self.0.read(offset, len) }
        }
    }

    /// `n` bytes that look random, always the same.
    pub(crate) fn noise(n: usize) -> Vec<u8> {
        let mut x = 0x2545_f491_4f6c_dd1du64;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    /// A real MP4 with its index at the start (`moov` before `mdat`): its boxes up to the data
    /// kept, the data replaced with noise. Its length and size are read, no frame decodes.
    pub(crate) fn header_and_noise() -> Vec<u8> {
        let mut v = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/site/img/mtp-open.mp4")).unwrap();
        let mut at = 0;
        while let Some((kind, size)) = crate::mp4::box_header(&v[at..]) {
            if &kind == b"mdat" {
                break;
            }
            at += size as usize;
        }
        let keep = at + 16;
        let len = v.len();
        v.truncate(keep);
        v.extend(noise(len - keep));
        v
    }

    /// Half a second of silence, as a WAV: a file with sound and no picture.
    fn silence() -> Vec<u8> {
        let (rate, data) = (8000u32, 4000u32);
        let mut v = b"RIFF".to_vec();
        v.extend((36 + data).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes()); // PCM
        v.extend(1u16.to_le_bytes()); // mono
        v.extend(rate.to_le_bytes());
        v.extend(rate.to_le_bytes()); // bytes a second, 8 bits
        v.extend(1u16.to_le_bytes());
        v.extend(8u16.to_le_bytes());
        v.extend(b"data");
        v.extend(data.to_le_bytes());
        v.extend(std::iter::repeat_n(128u8, data as usize));
        v
    }

    /// What `Player::error` says within `secs`; `None` if nothing in that time.
    fn error_within(player: &Player, secs: u64) -> Option<Failure> {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(secs) {
            if let Some(e) = player.error() {
                return Some(e);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    fn stream(bytes: Vec<u8>) -> Input {
        Input::Stream(Arc::new(Bytes(bytes)))
    }

    /// #209: a file mpv cannot play says why instead of a spinner forever: one it does not
    /// recognise (noise, as a file and as a stream), one that opens but has no frame that decodes,
    /// one whose stream fails (its own error); a file with sound only is not an error. Skipped
    /// without libmpv (the CI).
    #[test]
    fn broken_videos_say_why() {
        if !available() {
            println!("skipped: no libmpv");
            return;
        }
        let open = |input| Player::open(input, 0, true, || {}).unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-209-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("broken.mp4");
        std::fs::write(&file, noise(20_000)).unwrap();
        let found = [
            ("noise, file", error_within(&open(Input::File(file)), 10)),
            ("noise, stream", error_within(&open(stream(noise(20_000))), 10)),
            ("header and noise", error_within(&open(stream(header_and_noise())), 10)),
        ];
        let _ = std::fs::remove_dir_all(&dir);
        let good = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/site/img/mtp-open.mp4")).unwrap();
        let cut = open(Input::Stream(Arc::new(Failing(Bytes(good), 10_000))));
        let read = error_within(&cut, 10);
        let audio = open(stream(silence()));
        let t = Instant::now();
        while !audio.ended() && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        println!("{found:?}, read {read:?}, audio ended {} with {:?}", audio.ended(), audio.error());
        assert_eq!(found.clone().map(|f| f.1), [Some(Failure::NotVideo), Some(Failure::NotVideo), Some(Failure::NoPicture)], "{found:?}");
        assert_eq!(read, Some(Failure::Read("error-device-gone\u{1}USB 3-2".into())));
        assert!(audio.ended() && audio.error().is_none(), "sound only: ended {}, {:?}", audio.ended(), audio.error());
    }

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
