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
