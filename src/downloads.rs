//! The add-ons' downloads (`component::start`), kept together in `App`: the first-start window
//! and Options > Add-ons (#212) start them too. One slot for each place that starts one, as it was
//! before they were moved here; the next step makes it one per add-on.

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
    /// FFmpeg from the Convert window (`batch_video.rs`).
    pub(crate) convert_ffmpeg: Slot,
    /// FFmpeg asked for to save a turned video (`ffmpeg_ask.rs`).
    pub(crate) ask_ffmpeg: Slot,
}
