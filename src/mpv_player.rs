//! The player of the viewer and the slideshow: an mpv core whose frames a thread of its own
//! draws at the size the window asks, with sound, and what it says when a video cannot be
//! played. Split from `mpv.rs` on 2026-11-22.

use std::ffi::c_int;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui::ColorImage;
use image::RgbaImage;

use crate::mpv::{END_FILE_REASON_ERROR, EVENT_END_FILE, EVENT_FILE_LOADED, EVENT_SHUTDOWN, FORMAT_DOUBLE, FORMAT_FLAG, FORMAT_INT64, Input, Mpv};

#[derive(Default)]
struct State {
    /// mpv has a new frame, or the size changed.
    dirty: bool,
    size: [u32; 2],
    frame: Option<ColorImage>,
    quit: bool,
    /// mpv ended the file before it was loaded: it cannot play it (#209).
    failed: bool,
    /// Who asked for the frame shown at the video's own size (`Player::grab`).
    grab: Option<Grab>,
    /// The frames drawn are kept with their time, for the frames back (`video_back`).
    record: Option<Vec<(f64, ColorImage)>>,
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

/// Why a video cannot be played (`Player::error`, #209).
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// mpv does not recognise the file: not a video, or damaged at its start.
    NotVideo,
    /// It opens, but it reached its end without a frame decoded.
    NoPicture,
    /// The phone or the server failed to give the bytes: their error (`id\u{1}detail`).
    Read(String),
}

/// What a video that reached its end showed: a video track (`video`), that track a cover image
/// (`cover`), a frame that reached the screen (`shown`, mpv's `vo-configured`).
struct Picture {
    video: bool,
    cover: bool,
    shown: bool,
}

/// `Player::error` from what is known: mpv ended the file before loading it (`failed`); the
/// stream's last read failed (`read`); at the end, what it `showed`. A read error says why;
/// a file that ends before loading was not recognised; a video track, not a cover, that reached
/// its end without a frame had none that decodes. Sound alone is no error.
fn judge(failed: bool, read: Option<String>, showed: Option<Picture>) -> Option<Failure> {
    match (failed, read, showed) {
        (true, Some(e), _) => Some(Failure::Read(e)),
        (true, None, _) => Some(Failure::NotVideo),
        (false, Some(e), Some(_)) => Some(Failure::Read(e)),
        (false, None, Some(p)) if p.video && !p.cover && !p.shown => Some(Failure::NoPicture),
        _ => None,
    }
}

/// Where a video was (`Player::place`), for it to start there again (#211).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub time: f64,
    pub paused: bool,
    pub at_end: bool,
}

/// How a video opens at `at`: mpv's `pause` option, and the seek given as its tracks are turned
/// on (`start_tracks`), before any frame is decoded, so that the first frame shown is already that
/// one. mpv's own `start` is lost on a file opened with no track (#173): it opened at 0:00.
/// At the end: to the end, playing, so that mpv ends there at once with the last frame kept, as
/// after playing it (paused there it was one frame before, not at its end).
fn start_options(at: Option<Place>) -> (Vec<(&'static str, String)>, Option<[String; 3]>) {
    let Some(at) = at else { return (Vec::new(), None) };
    let (seek, paused) = if at.at_end { (["seek".into(), "100".into(), "absolute-percent+exact".into()], false) } else { (["seek".into(), format!("{:.3}", at.time.max(0.0)), "absolute+exact".into()], at.paused) };
    (vec![("pause", if paused { "yes" } else { "no" }.to_string())], Some(seek))
}

/// At most this many frames kept by `Player::record` (3 s at 60 per second, a few MB each at the
/// window's size); a filling takes 1 s (`video_back::WINDOW`).
const RECORDED: usize = 180;

/// A video playing (with sound), its frames drawn by a thread of its own at the size the
/// window asks (`set_size`).
pub struct Player {
    pub(crate) mpv: Arc<Mpv>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

/// What the player keeps of the video already played, in memory: a frame back (`,`) and a seek
/// back from there need no new read from a phone or a server, where mpv would read the stretch
/// again and a frame back only paused (Piero, 23/11/2026). mpv keeps its cache only for network
/// streams unless asked; 150 MB is a few minutes of a phone's video.
const BACK_CACHE: [(&str, &str); 3] = [("cache", "yes"), ("demuxer-seekable-cache", "yes"), ("demuxer-max-back-bytes", "150MiB")];

impl Player {
    /// `repaint` is called at every new frame, from the player's thread. `volume` (0-100) and
    /// `mute` are set before the first frame plays.
    pub fn open(input: Input, volume: u32, mute: bool, repaint: impl Fn() + Send + 'static) -> Result<Player, String> {
        Player::open_at(input, volume, mute, None, repaint)
    }

    /// As `open`, from `at` (the place of the video before Save, #211).
    pub fn open_at(input: Input, volume: u32, mute: bool, at: Option<Place>, repaint: impl Fn() + Send + 'static) -> Result<Player, String> {
        let shared = Arc::new(Shared::default());
        let s = shared.clone();
        // keep-open: at the end the last frame stays, as in a player.
        // audio-client-name: the name in the system's volume control.
        // sub-auto: no subtitle files from beside the video; a DJI drone's .SRT (telemetry of
        // every frame) covered half of the picture (2026-10-27).
        let volume = volume.min(100).to_string();
        let mute = if mute { "yes" } else { "no" };
        let (start, seek) = start_options(at);
        // track-auto-selection=no: no track until the rotation is set (`start_tracks`, #173).
        let mut options = vec![("hwdec", "auto-copy-safe"), ("track-auto-selection", "no"), ("keep-open", "yes"), ("audio-client-name", "EyeCanSee"), ("sub-auto", "no"), ("volume", volume.as_str()), ("mute", mute)];
        options.extend(BACK_CACHE);
        options.extend(start.iter().map(|(k, v)| (*k, v.as_str())));
        let mut mpv = Mpv::new(&options, move || s.change(|st| st.dirty = true))?;
        mpv.load(input)?;
        let mpv = Arc::new(mpv);
        let (m, s) = (mpv.clone(), shared.clone());
        let thread = crate::logging::spawn_named("mpv-draw", move || draw_loop(&m, &s, seek, repaint));
        Ok(Player { mpv, shared, thread: Some(thread) })
    }

    /// Where the video is: its time, paused or not, at its end or not (#211).
    pub fn place(&self) -> Place {
        Place { time: self.position(), paused: self.paused(), at_end: self.ended() }
    }

    /// The frame drawn since the last call, if any.
    pub fn frame(&self) -> Option<ColorImage> {
        self.shared.state.lock().unwrap().frame.take()
    }

    /// From now on (`on`) every frame drawn is kept with its time; off, the frames kept are given
    /// back (`video_back`, #165).
    pub fn record(&self, on: bool) -> Vec<(f64, ColorImage)> {
        let mut st = self.shared.state.lock().unwrap();
        if on {
            st.record = Some(Vec::new());
            Vec::new()
        } else {
            st.record.take().unwrap_or_default()
        }
    }

    /// The time of the last frame `record` has kept, if any.
    pub fn recorded(&self) -> Option<f64> {
        self.shared.state.lock().unwrap().record.as_ref().and_then(|k| k.last()).map(|(at, _)| *at)
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

    /// Why the video cannot be played, once mpv has decided it cannot (#209): from mpv's own
    /// state, never a timer, so a slow phone or server only keeps the spinner longer.
    pub fn error(&self) -> Option<Failure> {
        let read = self.mpv.stream.as_ref().and_then(|s| s.last_error());
        let failed = self.shared.state.lock().unwrap().failed;
        let ended = !failed && self.ended();
        let flag = |p: &str| self.mpv.property::<c_int>(p, FORMAT_FLAG).map(|f| f != 0);
        let picture = ended.then(|| {
            let video = self.mpv.property::<i64>("current-tracks/video/id", FORMAT_INT64).is_some();
            Picture { video, cover: flag("current-tracks/video/image") == Some(true), shown: flag("vo-configured") == Some(true) }
        });
        judge(failed, read, picture)
    }

    /// The phone or the server gives no data for a while (`mpv_stream::STALL`) while mpv waits
    /// for some (#209); a file on this computer never.
    pub fn stalled(&self) -> bool {
        self.mpv.stream.as_ref().is_some_and(|s| s.stalled())
    }

    /// Played to the end (the last frame stays, see keep-open).
    pub fn ended(&self) -> bool {
        self.mpv.property::<c_int>("eof-reached", FORMAT_FLAG).unwrap_or(0) != 0
    }

    /// How the playback went, in the log when the video closes (#17): frames drawn and lost on
    /// our side, mpv's own counts, the decoding.
    fn log_stats(mpv: &Mpv, shared: &Shared) {
        let st = shared.state.lock().unwrap();
        let (drawn, lost, drawing, longest) = (st.drawn, st.lost, st.drawing, st.longest);
        drop(st);
        let int = |p: &str| mpv.property::<i64>(p, FORMAT_INT64).unwrap_or(-1);
        let text = |p: &str| mpv.string(p).unwrap_or_default();
        log!(
            "video stats: {drawn} frames drawn up to {:.1} s, {lost} drawn over before shown, a draw {:.1} ms on average with its wait (longest {:.1}), mpv dropped {} (decoder {}), delayed {}, {}x{} {} at {} fps, hwdec {}",
            mpv.property::<f64>("time-pos", FORMAT_DOUBLE).unwrap_or(0.0),
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

    /// One frame forward or back, paused there (mpv's frame-step and frame-back-step, #165).
    pub fn frame_step(&self, forward: bool) {
        log!("video: one frame {} from {:.3} s", if forward { "forward" } else { "back" }, self.position());
        let _ = self.mpv.command(&[if forward { "frame-step" } else { "frame-back-step" }]);
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

/// Players being destroyed (`Drop for Player`), waited for at exit (`wait_closed`).
static CLOSING: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

/// Nothing here waits for mpv, which may be inside a read from a slow phone or server (#276):
/// the sound stops at once (a command mpv runs when it can), the stream lets its read go, and
/// the player's thread, the statistics and mpv's destruction (`Drop for Mpv`) are left to a
/// thread of their own. A new player may open meanwhile: the two are independent cores.
impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.mpv.command_async(&["set", "pause", "yes"]);
        if let Some(stream) = &self.mpv.stream {
            stream.cancel();
        }
        self.shared.change(|st| st.quit = true);
        self.mpv.wake();
        let (mpv, shared, thread) = (self.mpv.clone(), self.shared.clone(), self.thread.take());
        let closing = crate::logging::spawn_named("mpv-close", move || {
            if let Some(t) = thread {
                let _ = t.join();
            }
            Player::log_stats(&mpv, &shared);
            drop(mpv);
        });
        let mut all = CLOSING.lock().unwrap();
        all.retain(|t| !t.is_finished());
        all.push(closing);
    }
}

/// At exit: the players still being destroyed end first, `limit` at most (mpv's threads cut off
/// by the end of the process could crash it). True if all ended.
pub fn wait_closed(limit: Duration) -> bool {
    let start = Instant::now();
    let all = std::mem::take(&mut *CLOSING.lock().unwrap());
    let mut left = 0;
    for t in all {
        while !t.is_finished() && start.elapsed() < limit {
            std::thread::sleep(Duration::from_millis(10));
        }
        if t.is_finished() {
            let _ = t.join();
        } else {
            left += 1;
        }
    }
    if left > 0 {
        log!("video: {left} players still closing at exit");
    }
    left == 0
}

/// The frames turned by the file's rotation, by a filter. A turn not saved yet (#125) is not
/// mpv's: the window turns the frames it draws (`video::paint_fitted`), at once (#168).
fn turn_frames(mpv: &Mpv) {
    turn_by(mpv, mpv.rotation());
}

fn turn_by(mpv: &Mpv, rotation: i64) {
    let filter = match rotation {
        90 => "transpose=clock",
        180 => "hflip,vflip",
        270 => "transpose=cclock",
        _ => "",
    };
    let _ = if filter.is_empty() { mpv.command(&["vf", "clr", ""]) } else { mpv.command(&["vf", "set", filter]) };
}

/// The file was opened with no track (`track-auto-selection=no`): the rotation of the video
/// track mpv is going to choose is read from the track list and its filter set, and only then
/// mpv chooses the tracks, as it does by itself, audio and subtitles too (Piero, 22/11). The first
/// frame decoded is already turned: set after the tracks, the filter came 0.4-0.5 s late and the
/// first frames lay as the file stores them (#173). Opened with `vid=no` instead, a video without
/// sound did not open at all (mpv: "nothing to play").
fn start_tracks(mpv: &Mpv, seek: Option<[String; 3]>) {
    let ahead = rotation_ahead(mpv);
    if ahead != 0 {
        turn_by(mpv, ahead);
    }
    let _ = mpv.command(&["set", "track-auto-selection", "yes"]);
    // Already "auto", it is set again for mpv to choose now.
    for track in ["vid", "aid", "sid"] {
        let _ = mpv.command(&["set", track, "no"]);
        let _ = mpv.command(&["set", track, "auto"]);
    }
    // Measured with the system's libmpv 0.41: in case another one chooses nothing this way, the
    // first video and audio tracks (a file never opened silently without a word).
    if mpv.property::<i64>("current-tracks/video/id", FORMAT_INT64).is_none() && mpv.property::<i64>("current-tracks/audio/id", FORMAT_INT64).is_none() {
        let count = mpv.property::<i64>("track-list/count", FORMAT_INT64).unwrap_or(0);
        let first = |kind: &str| (0..count).find(|i| mpv.string(&format!("track-list/{i}/type")).as_deref() == Some(kind)).and_then(|i| mpv.property::<i64>(&format!("track-list/{i}/id"), FORMAT_INT64));
        for (track, kind) in [("vid", "video"), ("aid", "audio")] {
            if let Some(id) = first(kind) {
                log!("video: mpv chose no track, {track} {id} set");
                let _ = mpv.command(&["set", track, &id.to_string()]);
            }
        }
    }
    // Where the video was before Save (#211), before the first frame is decoded.
    if let Some(seek) = seek {
        let _ = mpv.command(&seek.each_ref().map(String::as_str));
    }
    // Another video track chosen (a file with two), turned another way: as before.
    if mpv.rotation() != ahead {
        turn_frames(mpv);
    }
}

/// The rotation of the video track mpv chooses by itself: the first one marked default, or else
/// the first one (`track-list`, read before any track is chosen).
fn rotation_ahead(mpv: &Mpv) -> i64 {
    let count = mpv.property::<i64>("track-list/count", FORMAT_INT64).unwrap_or(0);
    let video: Vec<i64> = (0..count).filter(|i| mpv.string(&format!("track-list/{i}/type")).as_deref() == Some("video")).collect();
    let default = video.iter().find(|i| mpv.property::<c_int>(&format!("track-list/{i}/default"), FORMAT_FLAG) == Some(1));
    default.or(video.first()).and_then(|i| mpv.property::<i64>(&format!("track-list/{i}/demux-rotation"), FORMAT_INT64)).unwrap_or(0).rem_euclid(360)
}

/// Waits for the file to be loaded, as long as it takes: a phone copying a whole video first, a
/// slow server. There was a limit of 20 s, after which no frame was ever drawn (#209). False when
/// the viewer closed, or when mpv ended the file first: it cannot play it, said in `failed`, and
/// the window is woken to show it.
fn wait_loaded(mpv: &Mpv, shared: &Shared, repaint: &impl Fn()) -> bool {
    loop {
        if shared.state.lock().unwrap().quit {
            return false;
        }
        match mpv.next_event(0.5) {
            (EVENT_FILE_LOADED, _) => return true,
            (EVENT_END_FILE, end) => {
                let (reason, error) = end.unwrap_or_default();
                log!("video: ended before it was loaded, reason {reason}{}, mpv error {error}", if reason == END_FILE_REASON_ERROR { " (error)" } else { "" });
                shared.change(|st| st.failed = true);
                repaint();
                return false;
            }
            (EVENT_SHUTDOWN, _) => return false,
            _ => {}
        }
    }
}

fn draw_loop(mpv: &Mpv, shared: &Shared, seek: Option<[String; 3]>, repaint: impl Fn()) {
    // The rotation is known once the file is loaded; a filter turns every frame from then on,
    // set before any track plays (`start_tracks`). `dwidth`/`dheight` follow the filter.
    if !wait_loaded(mpv, shared, &repaint) {
        return;
    }
    start_tracks(mpv, seek);
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
        let at = mpv.property::<f64>("time-pos", FORMAT_DOUBLE).unwrap_or(0.0);
        let mut st = shared.state.lock().unwrap();
        // ponytail: at most RECORDED frames, for a filling that would never end.
        if let Some(kept) = st.record.as_mut().filter(|k| k.len() < RECORDED) {
            kept.push((at, frame.clone()));
        }
        st.lost += u64::from(st.frame.is_some());
        st.frame = Some(frame);
        st.drawn += 1;
        st.drawing += took;
        st.longest = st.longest.max(took);
        drop(st);
        repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream whose reads wait until `release` (its `cancel` too) or `limit`: a phone or a
    /// server that stops answering, with no data and no error.
    struct Stuck {
        size: u64,
        limit: Duration,
        released: Mutex<bool>,
        cv: Condvar,
        reads: std::sync::atomic::AtomicUsize,
        /// When the last read returned.
        returned: Mutex<Option<Instant>>,
        /// What the reads give once released; none: an error.
        data: Vec<u8>,
    }

    impl Stuck {
        fn new(size: u64, limit: Duration) -> Arc<Stuck> {
            Self::giving(size, limit, Vec::new())
        }
        fn giving(size: u64, limit: Duration, data: Vec<u8>) -> Arc<Stuck> {
            Arc::new(Stuck { size, limit, released: Mutex::new(false), cv: Condvar::new(), reads: Default::default(), returned: Mutex::new(None), data })
        }
        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.cv.notify_all();
        }
    }

    impl crate::mpv::Reader for Stuck {
        fn size(&self) -> u64 {
            self.size
        }
        fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let r = self.cv.wait_timeout_while(self.released.lock().unwrap(), self.limit, |r| !*r).unwrap();
            drop(r);
            *self.returned.lock().unwrap() = Some(Instant::now());
            if self.data.is_empty() { Err("released".into()) } else { Ok(self.data[offset as usize..(offset + len) as usize].to_vec()) }
        }
        fn cancel(&self) {
            self.release();
        }
    }

    /// #209 (Piero, 22/11): a phone that gives no data for 20 s, then answers: said stalled after
    /// 20 s, not before; once data comes it is not, and the video loads.
    /// `cargo test stall_then_answer_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn stall_then_answer_live() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/site/img/mtp-open.mp4")).unwrap();
        let stuck = Stuck::giving(data.len() as u64, Duration::from_secs(3600), data);
        let player = Player::open(Input::Stream(stuck.clone()), 0, true, || {}).unwrap();
        let t = Instant::now();
        std::thread::sleep(Duration::from_secs(19));
        let at_19 = player.stalled();
        while !player.stalled() && t.elapsed() < Duration::from_secs(25) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let said_at = t.elapsed();
        stuck.release();
        let r = Instant::now();
        while player.video_size().is_none() && r.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(20));
        }
        println!("stalled at 19 s: {at_19}; said stalled after {said_at:?}; after the answer: size {:?} in {:?}, stalled {}, error {:?}", player.video_size(), r.elapsed(), player.stalled(), player.error());
        assert!(!at_19 && said_at >= Duration::from_secs(20) && said_at < Duration::from_secs(21));
        assert!(player.video_size().is_some() && !player.stalled() && player.error().is_none());
    }

    /// #211: the options that open a video where it was: the time and pause; at the end, the end
    /// played to (the last frame kept, as after playing it); none for no place.
    #[test]
    fn opened_where_it_was() {
        let at = |time, paused, at_end| Some(Place { time, paused, at_end });
        let paused = |p: &str| vec![("pause", p.to_string())];
        let seek = |to: &str, how: &str| Some(["seek".to_string(), to.into(), how.into()]);
        assert_eq!(start_options(None), (vec![], None));
        assert_eq!(start_options(at(62.345678, true, false)), (paused("yes"), seek("62.346", "absolute+exact")));
        assert_eq!(start_options(at(4.5, false, false)), (paused("no"), seek("4.500", "absolute+exact")));
        assert_eq!(start_options(at(9.958, true, true)), (paused("no"), seek("100", "absolute-percent+exact")));
    }

    /// #211 with the real player: opened at a time, it is there, paused when asked, playing when
    /// not; at the end, at its end with its last frame. Skipped without libmpv (the CI).
    #[test]
    fn player_opened_at_a_place() {
        if !crate::mpv::available() {
            println!("skipped: no libmpv");
            return;
        }
        let file = || Input::File(concat!(env!("CARGO_MANIFEST_DIR"), "/site/img/mtp-open.mp4").into());
        // Each case waits for what it checks, 10 s at most (a fixed wait failed under load); what
        // was seen is said when it does not come.
        let reach = |p: &Player, what: &str, done: &dyn Fn(Place) -> bool| {
            let t = Instant::now();
            let mut seen = p.place();
            while !done(seen) && t.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(20));
                seen = p.place();
            }
            if !done(seen) {
                println!("{what}: not reached in 10 s, seen {seen:?}, size {:?}, error {:?}", p.video_size(), p.error());
            }
            seen
        };
        let open = |at: Place| Player::open_at(file(), 0, true, Some(at), || {}).unwrap();
        let paused = reach(&open(Place { time: 4.5, paused: true, at_end: false }), "paused", &|p| p.paused && (p.time - 4.5).abs() < 0.05);
        let playing = reach(&open(Place { time: 4.5, paused: false, at_end: false }), "playing", &|p| !p.paused && p.time > 4.5);
        let end_player = open(Place { time: 9.958, paused: true, at_end: true });
        let end = reach(&end_player, "end", &|p| p.at_end && p.time > 9.9);
        println!("paused {paused:?}, playing {playing:?}, end {end:?}, end error {:?}", end_player.error());
        assert!(paused.paused && (paused.time - 4.5).abs() < 0.05, "{paused:?}");
        // The frames shown from the opening on, for a second after the first: all at 4.5 s, none
        // from 0:00 first.
        let p = open(Place { time: 4.5, paused: true, at_end: false });
        let (t, mut times, mut first) = (Instant::now(), Vec::new(), None);
        while first.is_none_or(|f: Instant| f.elapsed() < Duration::from_secs(1)) && t.elapsed() < Duration::from_secs(10) {
            if let Some(size) = p.video_size() {
                p.set_size(crate::mpv::fit(size, 200));
            }
            if p.frame().is_some() {
                first.get_or_insert_with(Instant::now);
                times.push(p.position());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        println!("frames at {times:?}");
        assert!(!times.is_empty() && times.iter().all(|t| (t - 4.5).abs() < 0.05), "frames at {times:?}");
        assert!(!playing.paused && playing.time > 4.5 && playing.time < 5.5, "{playing:?}");
        assert!(end.at_end && end.time > 9.9, "{end:?}");
        assert_eq!(end_player.error(), None, "at the end with its last frame: not a damaged video");
    }

    /// #173: a video whose file says to turn it is shown turned from its first frame: the size
    /// the window reads first and every frame drawn in the first second have the turned shape.
    /// The real MP4 of the site turned a quarter in its display matrix (as Save writes it), opened
    /// 12 times (a race: unturned 3 times in 8 before the fix). Skipped without libmpv (the CI).
    #[test]
    fn turned_from_the_first_frame() {
        if !crate::mpv::available() {
            println!("skipped: no libmpv");
            return;
        }
        let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/site/img/mtp-open.mp4")).unwrap();
        let turned = crate::mp4::turned(&original, crate::transform::Turn::default().then(crate::transform::Op::Cw)).unwrap();
        let file = std::env::temp_dir().join(format!("eyecansee-173-{}.mp4", std::process::id()));
        std::fs::write(&file, turned).unwrap();
        let wrong = lying_openings(&file, 12, true);
        let _ = std::fs::remove_file(&file);
        assert!(wrong.is_empty(), "shown unturned: {wrong:?}");
    }

    /// As `turned_from_the_first_frame`, on any video: `EYECANSEE_VIDEO=<file>
    /// EYECANSEE_VIDEO_UPRIGHT=portrait|landscape cargo test turned_from_the_first_frame_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn turned_from_the_first_frame_live() {
        let file = std::path::PathBuf::from(std::env::var("EYECANSEE_VIDEO").expect("EYECANSEE_VIDEO"));
        let portrait = std::env::var("EYECANSEE_VIDEO_UPRIGHT").is_ok_and(|v| v == "portrait");
        let wrong = lying_openings(&file, 8, portrait);
        assert!(wrong.is_empty(), "shown another way: {wrong:?}");
    }

    /// The openings of `file` (out of `times`) whose first size or some frame of the first second
    /// is not `portrait` (or landscape), as the window would see them.
    fn lying_openings(file: &std::path::Path, times: usize, portrait: bool) -> Vec<(usize, Option<[u32; 2]>, usize)> {
        let wrong_way = |[w, h]: [usize; 2]| if portrait { w > h } else { h > w };
        let mut wrong = Vec::new();
        for n in 0..times {
            let player = Player::open(Input::File(file.to_path_buf()), 0, true, || {}).unwrap();
            let (t, mut first_size, mut frames) = (Instant::now(), None, Vec::new());
            while t.elapsed() < Duration::from_secs(1) || first_size.is_none() && t.elapsed() < Duration::from_secs(5) {
                if let Some(size) = player.video_size() {
                    first_size.get_or_insert(size);
                    player.set_size(crate::mpv::fit(size, 400));
                }
                if let Some(f) = player.frame() {
                    frames.push(f.size);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            let lying = frames.iter().filter(|f| wrong_way(**f)).count();
            println!("opening {n}: first size {first_size:?}, {} frames, {lying} the wrong way, first {:?}", frames.len(), frames.first());
            if first_size.is_none_or(|[w, h]| wrong_way([w as usize, h as usize])) || lying > 0 {
                wrong.push((n, first_size, lying));
            }
        }
        wrong
    }

    /// The video, audio and subtitle tracks playing (`current-tracks/*/id`), once loaded.
    fn tracks_chosen(mpv: &Mpv) -> [Option<i64>; 3] {
        let t = Instant::now();
        while mpv.property::<i64>("current-tracks/video/id", FORMAT_INT64).is_none() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(300));
        ["video", "audio", "sub"].map(|k| mpv.property::<i64>(&format!("current-tracks/{k}/id"), FORMAT_INT64))
    }

    /// #173 (Piero, 22/11): opening every video with no track and turning them on after the
    /// rotation must choose the same tracks mpv chose by itself before: audio of several languages,
    /// subtitles default, forced, of other languages. The files are made with ffmpeg in the
    /// temporary folder. `cargo test track_choice_as_before_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn track_choice_as_before_live() {
        let dir = std::env::temp_dir().join(format!("eyecansee-173-tracks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (lang, text) in [("en", "hello"), ("it", "ciao"), ("fr", "salut")] {
            std::fs::write(dir.join(format!("{lang}.srt")), format!("1\n00:00:00,000 --> 00:00:03,000\n{text}\n")).unwrap();
        }
        // Audio: English, Italian (default), French; subtitles: English (default), Italian
        // (forced), French. Then no default anywhere; then only a forced subtitle; then a video
        // whose first track is the default one; then silent, with subtitles.
        let cases: [(&str, &[&str]); 5] = [
            ("all", &["-disposition:a:0", "0", "-disposition:a:1", "default", "-disposition:a:2", "0", "-disposition:s:0", "default", "-disposition:s:1", "forced", "-disposition:s:2", "0"]),
            ("none-default", &["-disposition:a:0", "0", "-disposition:a:1", "0", "-disposition:a:2", "0", "-disposition:s:0", "0", "-disposition:s:1", "0", "-disposition:s:2", "0"]),
            ("forced-only", &["-disposition:a:0", "0", "-disposition:a:1", "0", "-disposition:a:2", "default", "-disposition:s:0", "0", "-disposition:s:1", "0", "-disposition:s:2", "forced"]),
            ("first-default", &["-disposition:a:0", "default", "-disposition:a:1", "0", "-disposition:a:2", "0", "-disposition:s:0", "default", "-disposition:s:1", "0", "-disposition:s:2", "0"]),
            ("silent", &["-an", "-disposition:s:0", "0", "-disposition:s:1", "default", "-disposition:s:2", "0"]),
        ];
        let mut differ = Vec::new();
        for (name, dispositions) in cases {
            let file = dir.join(format!("{name}.mkv"));
            let mut args: Vec<String> = ["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=25:duration=3"].map(String::from).into();
            for f in [440, 550, 660] {
                args.extend(["-f", "lavfi", "-i", &format!("sine=f={f}:d=3")].map(String::from));
            }
            for lang in ["en", "it", "fr"] {
                args.extend(["-i".to_string(), dir.join(format!("{lang}.srt")).to_string_lossy().into_owned()]);
            }
            for i in 0..7 {
                args.extend(["-map".to_string(), i.to_string()]);
            }
            args.extend(["-c:v", "libx264", "-c:a", "libopus", "-c:s", "srt"].map(String::from));
            for (i, l) in ["eng", "ita", "fra"].iter().enumerate() {
                args.extend([format!("-metadata:s:a:{i}"), format!("language={l}"), format!("-metadata:s:s:{i}"), format!("language={l}")]);
            }
            args.extend(dispositions.iter().map(|d| d.to_string()));
            args.push(file.to_string_lossy().into_owned());
            let out = std::process::Command::new("ffmpeg").args(&args).output().expect("ffmpeg");
            assert!(out.status.success(), "ffmpeg: {}", String::from_utf8_lossy(&out.stderr));
            // Before #173: mpv chose by itself, with the player's options of then.
            let options = [("hwdec", "auto-copy-safe"), ("keep-open", "yes"), ("audio-client-name", "EyeCanSee"), ("sub-auto", "no"), ("volume", "0"), ("mute", "yes")];
            let mut before = Mpv::new(&options, || {}).unwrap();
            before.load(Input::File(file.clone())).unwrap();
            let before = tracks_chosen(&before);
            let player = Player::open(Input::File(file.clone()), 0, true, || {}).unwrap();
            let now = tracks_chosen(&player.mpv);
            println!("{name}: video, audio, subtitles before {before:?}, now {now:?}");
            if before != now {
                differ.push(name);
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(differ.is_empty(), "other tracks chosen: {differ:?}");
    }

    /// #276: a video closed while its stream waits 5 s on a slow phone or server: the window's
    /// thread is back at once, and the read is let go at once too. Skipped without libmpv (the CI).
    #[test]
    fn closed_while_a_read_waits() {
        if !crate::mpv::available() {
            println!("skipped: no libmpv");
            return;
        }
        let stuck = Stuck::new(10_000_000, Duration::from_secs(5));
        let player = Player::open(Input::Stream(stuck.clone()), 0, true, || {}).unwrap();
        let t = Instant::now();
        while stuck.reads.load(std::sync::atomic::Ordering::Relaxed) == 0 && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(200));
        let t = Instant::now();
        drop(player);
        let closed = t.elapsed();
        while stuck.returned.lock().unwrap().is_none() && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let read = stuck.returned.lock().unwrap().map(|r| r.duration_since(t));
        println!("closed in {closed:?}, the read returned {read:?} after the close");
        assert!(closed < Duration::from_millis(300), "the window waited {closed:?}");
        assert!(read.is_some_and(|r| r < Duration::from_millis(500)), "the read was let go after {read:?}");
        let t = Instant::now();
        assert!(wait_closed(Duration::from_secs(2)), "mpv not destroyed after {:?}", t.elapsed());
    }

    /// What the viewer has, with no time limit, from a stream whose first read never returns,
    /// and how long closing it takes (Esc or an arrow drop the player on the window's thread).
    /// `cargo test stuck_stream_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn stuck_stream_live() {
        let stuck = Stuck::new(10_000_000, Duration::from_secs(3600));
        let player = Player::open(Input::Stream(stuck.clone()), 0, true, || {}).unwrap();
        for s in [1, 5, 30] {
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(if s == 1 { 1 } else if s == 5 { 4 } else { 25 }) {
                std::thread::sleep(Duration::from_millis(50));
            }
            println!("after {s} s: reads {}, size {:?}, error {:?}, stalled {}, position {:.1}, duration {:.1}", stuck.reads.load(std::sync::atomic::Ordering::Relaxed), player.video_size(), player.error(), player.stalled(), player.position(), player.duration());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let t = Instant::now();
        std::thread::spawn(move || {
            drop(player);
            tx.send(()).unwrap();
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(()) => println!("closed in {:?}", t.elapsed()),
            Err(_) => {
                println!("still closing after {:?}: the window would be frozen; read released now", t.elapsed());
                stuck.release();
                rx.recv().unwrap();
                println!("closed {:?} after the start, once the read returned", t.elapsed());
            }
        }
    }

    /// #209, without mpv: what `Player::error` says from what is known.
    #[test]
    fn judged_from_mpv_state() {
        let at_end = |video, cover, shown| Some(Picture { video, cover, shown });
        let read = || Some("error-device-gone\u{1}USB".to_string());
        assert_eq!(judge(true, None, None), Some(Failure::NotVideo));
        assert_eq!(judge(true, read(), None), Some(Failure::Read(read().unwrap())));
        assert_eq!(judge(false, None, at_end(true, false, false)), Some(Failure::NoPicture));
        assert_eq!(judge(false, read(), at_end(true, false, true)), Some(Failure::Read(read().unwrap())));
        // Playing, or still loading: nothing yet, a read error included (it may come back).
        assert_eq!(judge(false, None, None), None);
        assert_eq!(judge(false, read(), None), None);
        // At the end with a frame shown, sound only, a cover: fine.
        assert_eq!(judge(false, None, at_end(true, false, true)), None);
        assert_eq!(judge(false, None, at_end(false, false, false)), None);
        assert_eq!(judge(false, None, at_end(true, true, false)), None);
    }
}
