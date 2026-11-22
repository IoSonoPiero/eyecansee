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

/// A video playing (with sound), its frames drawn by a thread of its own at the size the
/// window asks (`set_size`).
pub struct Player {
    pub(crate) mpv: Arc<Mpv>,
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
    let filter = match mpv.rotation() {
        90 => "transpose=clock",
        180 => "hflip,vflip",
        270 => "transpose=cclock",
        _ => "",
    };
    let _ = if filter.is_empty() { mpv.command(&["vf", "clr", ""]) } else { mpv.command(&["vf", "set", filter]) };
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

fn draw_loop(mpv: &Mpv, shared: &Shared, repaint: impl Fn()) {
    // The rotation is known once the file is loaded; a filter turns every frame from then on
    // (a frame decoded before is filtered again by mpv). `dwidth`/`dheight` follow the filter.
    if !wait_loaded(mpv, shared, &repaint) {
        return;
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
    }

    impl Stuck {
        fn new(size: u64, limit: Duration) -> Arc<Stuck> {
            Arc::new(Stuck { size, limit, released: Mutex::new(false), cv: Condvar::new(), reads: Default::default(), returned: Mutex::new(None) })
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
        fn read(&self, _offset: u64, _len: u64) -> Result<Vec<u8>, String> {
            self.reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let r = self.cv.wait_timeout_while(self.released.lock().unwrap(), self.limit, |r| !*r).unwrap();
            drop(r);
            *self.returned.lock().unwrap() = Some(Instant::now());
            Err("released".into())
        }
        fn cancel(&self) {
            self.release();
        }
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
            println!("after {s} s: reads {}, size {:?}, error {:?}, position {:.1}, duration {:.1} (no size and no error: the viewer shows its spinner)", stuck.reads.load(std::sync::atomic::Ordering::Relaxed), player.video_size(), player.error(), player.position(), player.duration());
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
