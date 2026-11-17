//! Videos at full screen (F11, issue #43): the picture on the whole screen and the bar over its
//! bottom edge, hidden with the pointer when the mouse stays still, back as soon as it moves.

/// Seconds the bar and the pointer stay after the mouse stops: GNOME Videos waits 2 s
/// (`POPUP_HIDING_TIMEOUT`), VLC 1 s (`mouse-hide-timeout`), mpv 0.5 s for its bar and 1 s for the
/// pointer: 2 s leaves the time to reach the bar.
pub(crate) const HIDE_AFTER: f64 = 2.0;
/// Seconds the bar takes to fade out and in (mpv: 0.2).
pub(crate) const FADE: f32 = 0.2;

/// When the mouse last moved, to hide the bar `HIDE_AFTER` seconds later.
#[derive(Default)]
pub(crate) struct Hide {
    /// The time of the last movement (egui's clock), `None` until full screen is entered.
    moved: Option<f64>,
}

impl Hide {
    /// The bar at `now`: `Some(seconds before it hides)` while shown, `None` hidden. `moved`: the
    /// mouse moved, clicked or turned its wheel in this frame (also the first frame at full
    /// screen, which shows the bar once); `keep`: the pointer is on the bar, a slider is dragged or
    /// a menu is open, which keep it shown.
    pub(crate) fn shown(&mut self, now: f64, moved: bool, keep: bool) -> Option<f64> {
        if moved || keep || self.moved.is_none() {
            self.moved = Some(now);
        }
        let left = self.moved.map_or(0.0, |t| t + HIDE_AFTER - now);
        (left > 0.0).then_some(left)
    }

    /// Out of full screen: shown again at once when it comes back.
    pub(crate) fn reset(&mut self) {
        self.moved = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{HIDE_AFTER, Hide};

    /// Shown on entering, hidden `HIDE_AFTER` seconds after the last movement, back at the next one;
    /// kept while the pointer is on the bar.
    #[test]
    fn hidden_when_the_mouse_stays_still() {
        let mut h = Hide::default();
        assert_eq!(h.shown(10.0, false, false), Some(HIDE_AFTER));
        assert_eq!(h.shown(11.5, false, false), Some(0.5));
        assert_eq!(h.shown(12.0, false, false), None);
        assert_eq!(h.shown(30.0, false, false), None);
        assert_eq!(h.shown(31.0, true, false), Some(HIDE_AFTER));
        assert_eq!(h.shown(40.0, false, true), Some(HIDE_AFTER));
        assert_eq!(h.shown(41.0, false, false), Some(1.0));
        h.reset();
        assert_eq!(h.shown(100.0, false, false), Some(HIDE_AFTER));
    }
}
