//! The metadata panel while the selected file's data are on their way. The previous file's data
//! stay on the screen for a moment (`KEPT`), so a quick answer changes only the values that
//! differ, without a blink; after that the boxes are emptied and say "Reading…", so the data of
//! one file never stand for long under the name of another.

use std::time::{Duration, Instant};

use crate::{App, meta};

/// How long the previous file's data stay once another file is selected. Long enough for an
/// answer from the cache or from a disk (a few frames), short enough not to be read as the new
/// file's: 0.1 s is felt as at once, and a sign of work is expected within a second.
pub const KEPT: Duration = Duration::from_millis(250);

/// What the boxes show for the selected file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shown {
    /// Its own data.
    Own,
    /// The data that were on the screen the frame before, the previous file's, for this long still.
    Kept(Duration),
    /// Nothing, for this long still: no data were on the screen to keep, and "Reading…" would
    /// blink before a quick answer.
    Empty(Duration),
    /// "Reading…": the answer is taking longer than `KEPT`.
    Reading,
}

/// Since when the panel waits for the selected file's data.
#[derive(Default)]
pub struct Wait {
    /// When the selected file's data began to be missing; `None` while they are shown.
    since: Option<Instant>,
    /// Whether data were on the screen at that moment.
    kept: bool,
    /// The last frame the boxes were drawn for a file.
    frame: Option<u64>,
}

impl Wait {
    /// What to show in `frame` at `now`; `own`: the data held are the selected file's.
    /// The wait starts when the data on the screen stop being the selected file's, and does not
    /// start again at every file selected after that: with an arrow key held down on a phone
    /// the first file's data would stay for as long as the key. Data not drawn the frame before
    /// (no file selected, the panel collapsed or hidden, the viewer open) do not come back.
    pub fn shown(&mut self, own: bool, frame: u64, now: Instant) -> Shown {
        // Twice in one frame when egui draws it again: still the same frame.
        let drawn = self.frame.is_some_and(|f| frame <= f + 1);
        self.frame = Some(frame);
        if own {
            self.since = None;
            return Shown::Own;
        }
        if !drawn || self.since.is_none() {
            self.kept = drawn;
        }
        let since = *self.since.filter(|_| drawn).get_or_insert(now);
        self.since = Some(since);
        match KEPT.checked_sub(now.saturating_duration_since(since)).filter(|left| !left.is_zero()) {
            Some(left) if self.kept => Shown::Kept(left),
            Some(left) => Shown::Empty(left),
            None => Shown::Reading,
        }
    }
}

/// The metadata read, as the loader gives them: none in the file, or why they were not read.
type Read = Result<Option<meta::Meta>, String>;

impl App {
    /// What the boxes show for the selected file (`file`: its index among the images).
    pub(crate) fn meta_state(&mut self, file: usize, frame: u64, now: Instant) -> Shown {
        let own = self.meta.as_ref().is_some_and(|(i, _)| *i == file);
        self.meta_wait.shown(own, frame, now)
    }

    /// The data for the panel's boxes (`file`: the selected one's index among the images, if
    /// any), and whether the boxes without data say "Reading…". The frame is drawn again when
    /// the data kept are to go.
    pub(crate) fn meta_for_boxes(&mut self, file: Option<usize>, ctx: &eframe::egui::Context) -> (Option<Read>, bool) {
        let Some(file) = file else { return (None, false) };
        let shown = self.meta_state(file, ctx.cumulative_frame_nr(), Instant::now());
        if let Shown::Kept(left) | Shown::Empty(left) = shown {
            ctx.request_repaint_after(left);
        }
        (self.meta_held(shown), shown == Shown::Reading)
    }

    /// The data held, when the boxes show data (`shown`): the selected file's, or those kept
    /// from the file before. Otherwise none, whatever is held.
    fn meta_held(&self, shown: Shown) -> Option<Read> {
        let data = matches!(shown, Shown::Own | Shown::Kept(_));
        self.meta.as_ref().filter(|_| data).map(|(_, m)| m.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{KEPT, Shown, Wait};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A quick answer (the cache, a disk): the previous file's data stay until it comes, and
    /// "Reading…" is never shown.
    #[test]
    fn a_quick_answer_never_says_reading() {
        let (mut w, t) = (Wait::default(), Instant::now());
        assert_eq!(w.shown(true, 1, t), Shown::Own);
        assert_eq!(w.shown(false, 2, t + ms(1000)), Shown::Kept(KEPT));
        assert_eq!(w.shown(false, 3, t + ms(1040)), Shown::Kept(KEPT - ms(40)));
        assert_eq!(w.shown(true, 4, t + ms(1060)), Shown::Own);
        // The next file starts its own wait.
        assert_eq!(w.shown(false, 5, t + ms(5000)), Shown::Kept(KEPT));
    }

    /// A slow answer (a large file on a phone or a server): after `KEPT` the previous file's
    /// data go and the boxes say "Reading…", until the answer.
    #[test]
    fn a_slow_answer_empties_the_boxes() {
        let (mut w, t) = (Wait::default(), Instant::now());
        assert_eq!(w.shown(true, 1, t), Shown::Own);
        assert_eq!(w.shown(false, 2, t), Shown::Kept(KEPT));
        assert_eq!(w.shown(false, 3, t + KEPT - ms(1)), Shown::Kept(ms(1)));
        assert_eq!(w.shown(false, 4, t + KEPT), Shown::Reading);
        assert_eq!(w.shown(false, 5, t + ms(900)), Shown::Reading);
        assert_eq!(w.shown(true, 6, t + ms(950)), Shown::Own);
    }

    /// An arrow key held down over slow files: every frame another file, none with its data. The
    /// wait is one, from the first of them, not a new one for each.
    #[test]
    fn files_passed_over_do_not_start_the_wait_again() {
        let (mut w, t) = (Wait::default(), Instant::now());
        assert_eq!(w.shown(true, 1, t), Shown::Own);
        let shown: Vec<_> = (1..=10).map(|n| w.shown(false, 1 + n, t + ms(40 * n))).collect();
        assert!(shown[..6].iter().all(|s| matches!(s, Shown::Kept(_))), "{shown:?}");
        assert!(shown[7..].iter().all(|s| *s == Shown::Reading), "{shown:?}");
    }

    /// Data that were not on the screen the frame before are not shown under another file: the
    /// first file selected, or the panel opened again on another one. The boxes stay empty for
    /// the moment a quick answer takes, then say "Reading…".
    #[test]
    fn data_not_on_the_screen_do_not_come_back() {
        let (mut w, t) = (Wait::default(), Instant::now());
        assert_eq!(w.shown(false, 1, t), Shown::Empty(KEPT));
        assert_eq!(w.shown(false, 2, t + ms(100)), Shown::Empty(KEPT - ms(100)));
        assert_eq!(w.shown(false, 3, t + KEPT), Shown::Reading);
        assert_eq!(w.shown(true, 4, t + ms(400)), Shown::Own);
        // Frames without the boxes (the panel collapsed), then another file.
        assert_eq!(w.shown(false, 9, t + ms(2000)), Shown::Empty(KEPT));
        // The same frame drawn twice is one frame.
        assert_eq!(w.shown(false, 9, t + ms(2000)), Shown::Empty(KEPT));
        // Kept data that went do not come back either, after frames without the boxes.
        assert_eq!(w.shown(true, 10, t + ms(2100)), Shown::Own);
        assert_eq!(w.shown(false, 11, t + ms(3000)), Shown::Kept(KEPT));
        assert_eq!(w.shown(false, 20, t + ms(3050)), Shown::Empty(KEPT));
    }

    /// The panel on a folder of two files, with the app without a screen: the data of a.png
    /// stay under b.png for a moment only, an answer for a file no longer selected is dropped
    /// (it stayed under the other file's name, which was not asked again), and the data of
    /// b.png are shown as they come.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_data_of_one_file_do_not_stay_under_another() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("meta_wait::tests::the_data_of_one_file_do_not_stay_under_another") {
            return;
        }
        let mut h = Harness::new(&folder("meta-wait", &["a.png", "b.png"]));
        // a.png selected, as with a click: the folder has no folders before it.
        (h.app.selected, h.app.cursor) = (0, true);
        h.until(|app| matches!(app.meta, Some((0, Ok(_)))), "the metadata of a.png read");
        let (generation, t) = (h.app.generation, Instant::now());
        let frame = h.ctx.cumulative_frame_nr();
        assert_eq!(h.app.meta_state(0, frame, t), Shown::Own);
        // b.png selected and asked for, its answer not there yet.
        h.app.meta_asked = Some((generation, 1));
        assert_eq!(h.app.meta_state(1, frame + 1, t), Shown::Kept(KEPT));
        assert_eq!(h.app.meta_state(1, frame + 2, t + KEPT), Shown::Reading);
        // The data held are still a.png's: given to the boxes only while kept.
        assert!(h.app.meta_held(Shown::Kept(KEPT)).is_some());
        assert!(h.app.meta_held(Shown::Empty(KEPT)).is_none() && h.app.meta_held(Shown::Reading).is_none());
        // A late answer for a.png, selected before: not b.png's.
        h.app.on_meta(0, Err("late".into()));
        assert!(matches!(h.app.meta, Some((0, Ok(_)))), "the late answer was taken");
        assert_eq!(h.app.meta_state(1, frame + 3, t + KEPT), Shown::Reading);
        h.app.on_meta(1, Ok(None));
        assert!(matches!(h.app.meta, Some((1, Ok(None)))));
        assert_eq!(h.app.meta_state(1, frame + 4, t + ms(600)), Shown::Own);
    }
}
