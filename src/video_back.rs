//! Frames back with , from frames kept in memory (#165, Piero 23/11/2026: "buffer what is
//! needed when , is pressed"). mpv's own frame back decodes from the keyframe before at every
//! press: on a 4K video about a second each. Here the first , takes the player back `WINDOW`
//! seconds and steps it forward to where it was, keeping every frame it draws (at the window's
//! size, `Player::record`); the next presses show those frames at once, one before the other.
//! Playing, seeking, Ctrl and the arrows, or . past the last frame kept, go back to mpv at the
//! frame shown. Without the interface but for the texture.

use std::time::{Duration, Instant};

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::mpv;

/// Seconds kept before the frame shown, at each filling.
const WINDOW: f64 = 1.0;
/// At most this much of frames in memory (a frame at the window's size, a few MB each).
const MAX_BYTES: usize = 512 << 20;
/// A step that does not move the video for this long is not waited for (the end of the file).
const STEP_WAIT: Duration = Duration::from_secs(3);
/// A filling that has not ended by then ends with what it has.
const FILL_LIMIT: Duration = Duration::from_secs(20);

/// The frames kept and the one shown, and the filling under way.
#[derive(Default)]
pub(crate) struct Buffer {
    frames: Vec<(f64, ColorImage)>,
    shown: Option<usize>,
    /// The frame shown has changed: the texture takes it at the next paint.
    show_now: bool,
    fill: Option<Fill>,
}

struct Fill {
    /// The time it fills up to, the frame shown when , was pressed.
    target: f64,
    /// The player is back before `target` (the seek has landed).
    landed: bool,
    /// The position when the last step forward was sent, and when. The next waits for the video
    /// to move and for the frame of the new position to be kept (sent on a change of position, or
    /// on a frame drawn again, some frames were never kept: 24-26 of 30, the frames back jumped;
    /// Piero, 23/11/2026).
    sent: Option<(f64, Instant)>,
    started: Instant,
}

impl Buffer {
    /// Frames are being kept: the status bar says so (`App::frames_back`).
    pub(crate) fn busy(&self) -> bool {
        self.fill.is_some()
    }

    /// , pressed: the frame before, kept, or a filling started. Presses while filling do not
    /// count: kept and applied at its end they jumped several frames at once (Piero, 23/11/2026).
    pub(crate) fn back(&mut self, player: &mpv::Player) {
        if self.fill.is_some() {
            return;
        }
        if let Some(i) = self.shown.filter(|&i| i > 0) {
            (self.shown, self.show_now) = (Some(i - 1), true);
            log!("video: frame back to {:.3} s, kept", self.frames[i - 1].0);
            return;
        }
        let target = self.shown.map_or_else(|| player.position(), |i| self.frames[i].0);
        self.frames.clear();
        self.shown = None;
        if target <= 0.001 {
            return;
        }
        player.pause();
        player.record(true);
        player.seek((target - WINDOW).max(0.0), true);
        self.fill = Some(Fill { target, landed: false, sent: None, started: Instant::now() });
    }

    /// . pressed: the frame after, kept; `false` when mpv steps (none kept after it).
    pub(crate) fn forward(&mut self, player: &mpv::Player) -> bool {
        match self.shown {
            Some(i) if i + 1 < self.frames.len() => {
                (self.shown, self.show_now) = (Some(i + 1), true);
                true
            }
            _ => {
                self.leave(player);
                false
            }
        }
    }

    /// Back to mpv at the frame shown (play, Ctrl and an arrow), the frames let go.
    pub(crate) fn leave(&mut self, player: &mpv::Player) {
        if let Some(i) = self.shown {
            player.seek(self.frames[i].0, true);
        }
        self.clear(player);
    }

    /// The frames let go without moving the player (a seek of the timeline goes elsewhere anyway).
    pub(crate) fn clear(&mut self, player: &mpv::Player) {
        if self.fill.take().is_some() {
            player.record(false);
        }
        self.frames.clear();
        (self.shown, self.show_now) = (None, false);
    }

    /// Each frame of the viewer: the filling goes on, a step forward at a time, and ends once the
    /// player is back where it was.
    pub(crate) fn pump(&mut self, player: &mpv::Player) {
        let Some(f) = &mut self.fill else { return };
        let (pos, now) = (player.position(), Instant::now());
        f.landed |= pos < f.target - 0.001;
        let done = (f.landed && pos >= f.target - 0.001) || now.duration_since(f.started) >= FILL_LIMIT;
        if !done {
            if f.landed && step_now(f.sent, player.recorded(), pos, now) {
                player.frame_step(true);
                f.sent = Some((pos, now));
            }
            return;
        }
        let target = f.target;
        self.fill = None;
        self.frames = kept_before(player.record(false), target, MAX_BYTES);
        self.shown = self.frames.len().checked_sub(1);
        self.show_now = self.shown.is_some();
        let times: Vec<String> = self.frames.iter().map(|(at, _)| format!("{at:.3}")).collect();
        log!("video: {} frames kept before {target:.3} s: {}", self.frames.len(), times.join(" "));
    }

    /// Before the frame is painted: while filling or showing a frame kept, the player's own frames
    /// are not shown; a frame kept just chosen goes to the texture.
    pub(crate) fn before_paint(&mut self, ctx: &Context, player: &mpv::Player, texture: &mut Option<TextureHandle>) {
        if self.fill.is_none() && self.shown.is_none() {
            return;
        }
        let _ = player.frame();
        if std::mem::take(&mut self.show_now)
            && let Some(image) = self.shown.map(|i| self.frames[i].1.clone())
        {
            match texture {
                Some(t) => t.set(image, TextureOptions::LINEAR),
                None => *texture = Some(ctx.load_texture("video", image, TextureOptions::LINEAR)),
            }
        }
    }
}

/// Whether the next step forward of a filling can go: the frame of the position now is kept, and
/// the video has moved since the last step (or that one has waited `STEP_WAIT`, at the end).
fn step_now(sent: Option<(f64, Instant)>, kept: Option<f64>, pos: f64, now: Instant) -> bool {
    let this_kept = kept.is_some_and(|at| (at - pos).abs() < 0.0005);
    let moved = sent.is_none_or(|(at, when)| pos != at || now.duration_since(when) >= STEP_WAIT);
    (this_kept && moved) || sent.is_some_and(|(_, when)| now.duration_since(when) >= STEP_WAIT)
}

/// The frames before `target` (the one shown when , was pressed), each time once (the last drawn
/// for it), the oldest let go to stay within `max_bytes`.
fn kept_before(frames: Vec<(f64, ColorImage)>, target: f64, max_bytes: usize) -> Vec<(f64, ColorImage)> {
    let mut kept: Vec<(f64, ColorImage)> = Vec::new();
    for (at, image) in frames.into_iter().filter(|(at, _)| *at < target - 0.001) {
        match kept.last_mut() {
            Some(last) if (last.0 - at).abs() < 0.0005 => *last = (at, image),
            _ => kept.push((at, image)),
        }
    }
    kept.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bytes: usize = kept.iter().map(|(_, i)| i.pixels.len() * 4).sum();
    let mut drop = 0;
    while bytes > max_bytes && drop < kept.len() {
        bytes -= kept[drop].1.pixels.len() * 4;
        drop += 1;
    }
    kept.split_off(drop)
}

#[cfg(test)]
mod tests {
    use eframe::egui::ColorImage;

    fn frame(side: usize) -> ColorImage {
        ColorImage::new([side, side], vec![eframe::egui::Color32::BLACK; side * side])
    }

    /// #165: a step forward goes only once the frame of the position now is kept, and the video
    /// has moved since the step before (a frame drawn again does not count).
    #[test]
    fn a_step_waits_for_its_frame_kept() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        assert!(super::step_now(None, Some(3.7), 3.7, t0), "landed and kept: the first step");
        assert!(!super::step_now(None, Some(3.667), 3.7, t0), "the frame of 3.7 not kept yet");
        let sent = Some((3.7, t0));
        assert!(!super::step_now(sent, Some(3.7), 3.7, t0 + Duration::from_millis(40)), "not moved yet");
        assert!(!super::step_now(sent, Some(3.7), 3.733, t0 + Duration::from_millis(40)), "moved, its frame not kept");
        assert!(super::step_now(sent, Some(3.733), 3.733, t0 + Duration::from_millis(50)));
        assert!(super::step_now(sent, Some(3.7), 3.7, t0 + super::STEP_WAIT), "stuck: not waited for ever");
    }

    /// #165: the frames kept are those before the one shown, once each, in order, within the memory.
    #[test]
    fn frames_kept_before_the_one_shown() {
        let drawn = vec![(9.0, frame(2)), (9.033, frame(2)), (9.033, frame(2)), (9.067, frame(2)), (9.1, frame(2)), (9.2, frame(2))];
        let kept = super::kept_before(drawn, 9.1, usize::MAX);
        assert_eq!(kept.iter().map(|f| f.0).collect::<Vec<_>>(), vec![9.0, 9.033, 9.067]);
        // A frame of 2 × 2 is 16 bytes: room for two, the oldest go.
        let drawn = vec![(1.0, frame(2)), (1.1, frame(2)), (1.2, frame(2))];
        assert_eq!(super::kept_before(drawn, 5.0, 32).iter().map(|f| f.0).collect::<Vec<_>>(), vec![1.1, 1.2]);
    }
}
