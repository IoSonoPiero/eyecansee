//! The add-ons' downloads (`component::start`), kept together in `App`, one per add-on: the
//! Convert window, the FFmpeg question, the first-start window and Options > Add-ons (#212) all
//! show the same download of FFmpeg, and none of them starts a second one while it runs.

use std::sync::Arc;

use crate::component::Download;

/// A download under way or ended, or none.
pub(crate) type Slot = Option<Arc<Download>>;

#[derive(Default)]
pub(crate) struct Downloads {
    /// The video engine (`video.rs`).
    pub(crate) video: Slot,
    /// The CAD engine (`cad_engine.rs`).
    pub(crate) cad: Slot,
    /// FFmpeg (`batch_video.rs`, `ffmpeg_ask.rs`).
    pub(crate) ffmpeg: Slot,
}

impl Downloads {
    /// The download of the add-on of this kind (`Component::kind`): the first-start window and
    /// Options > Add-ons start and follow them by kind. A new add-on gets its place here.
    pub(crate) fn slot(&mut self, kind: &str) -> Option<&mut Slot> {
        match kind {
            "libmpv" => Some(&mut self.video),
            "ffmpeg" => Some(&mut self.ffmpeg),
            "occt" => Some(&mut self.cad),
            _ => None,
        }
    }

    /// The same, to read.
    pub(crate) fn get(&self, kind: &str) -> &Slot {
        const NONE: &Slot = &None;
        match kind {
            "libmpv" => &self.video,
            "ffmpeg" => &self.ffmpeg,
            "occt" => &self.cad,
            _ => NONE,
        }
    }

    /// Starts the download of `c` unless one is under way; true when it started.
    pub(crate) fn start(&mut self, c: crate::component::Component, ctx: &eframe::egui::Context) -> bool {
        let Some(slot) = self.slot(c.kind) else { return false };
        if running(slot) {
            return false;
        }
        let ctx = ctx.clone();
        *slot = Some(crate::component::start(c, move || ctx.request_repaint()));
        true
    }
}

/// Still under way.
pub(crate) fn running(slot: &Slot) -> bool {
    slot.as_ref().is_some_and(|d| d.result.lock().unwrap().is_none())
}

/// An ended download forgotten (its error included), one under way kept: a window that opens
/// shows a running download started elsewhere, and a fresh button otherwise.
pub(crate) fn forget_ended(slot: &mut Slot) {
    if !running(slot) {
        *slot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every add-on of this system has its place, so the window and Options can start it.
    #[test]
    fn every_addon_has_a_slot() {
        let mut d = Downloads::default();
        for a in crate::component::addons() {
            assert!(d.slot(a.component.kind).is_some(), "{}", a.component.kind);
        }
    }

    #[test]
    fn ended_forgotten_running_kept() {
        let mut slot: Slot = Some(Arc::new(Download::default()));
        forget_ended(&mut slot);
        assert!(running(&slot), "under way: kept");
        *slot.as_ref().unwrap().result.lock().unwrap() = Some(Err("x".into()));
        assert!(!running(&slot));
        forget_ended(&mut slot);
        assert!(slot.is_none(), "ended: forgotten");
    }
}
