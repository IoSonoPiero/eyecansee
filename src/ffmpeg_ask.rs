//! FFmpeg offered where a turned video is saved and none can be used (issue #169): what is
//! missing, the download with its progress, then the save goes on. Before, the save failed with
//! "it is a video and FFmpeg is missing", and nothing said that EyeCanSee can download it: with
//! "Always the downloaded one" in the Options and FFmpeg not downloaded yet, the turn could not be
//! saved (Piero, 2026-11-18).

use std::sync::Arc;

use eframe::egui;

use crate::App;
use crate::engine::Status;
use crate::save_dialogs::{Head, message};

/// The window open: the download under way, and whether the viewer's Save follows it.
pub struct Ask {
    download: Option<Arc<crate::component::Download>>,
    then_save: bool,
}

/// The text of the window and whether the download is offered, by what `ffmpeg::status` says:
/// with "the system's only" in the Options a downloaded FFmpeg would not be used.
fn text_of(status: &Status) -> (&'static str, bool) {
    match status {
        Status::SystemOnly => ("batch-video-system-only", false),
        _ => ("turn-video-need-ffmpeg", true),
    }
}

impl App {
    /// No FFmpeg that can save a turned video: true, and the window that offers it opens.
    /// `then_save`: the viewer's Save goes on once FFmpeg is there.
    pub(crate) fn ffmpeg_missing(&mut self, then_save: bool) -> bool {
        if crate::ffmpeg::copier().is_some() {
            return false;
        }
        log!("ffmpeg asked for: {:?}", crate::ffmpeg::status());
        self.ffmpeg_ask = Some(Ask { download: None, then_save });
        true
    }

    /// The window: Esc and Close leave the turn unsaved; Enter starts the download.
    pub(crate) fn ffmpeg_window(&mut self, ctx: &egui::Context) {
        let Some(ask) = &mut self.ffmpeg_ask else { return };
        let (id, offered) = text_of(&crate::ffmpeg::status());
        let component = crate::component::FFMPEG.filter(|_| offered);
        let size = component.map_or(String::new(), |c| format!("{} MB", c.size / 1_000_000));
        let i18n = &self.i18n;
        // Enter and Esc arrive as `delete_answer` (`raw_input_hook`).
        let mut close = match (self.delete_answer.take(), component) {
            (Some(true), Some(c)) if ask.download.is_none() => {
                let ctx = ctx.clone();
                ask.download = Some(crate::component::start(c, move || ctx.request_repaint()));
                false
            }
            (Some(true), Some(_)) | (None, _) => false,
            (Some(_), _) => true,
        };
        let mut done = false;
        message(ctx, "ffmpeg-ask", Head::new(&i18n.t("ffmpeg-ask-title")), &i18n.t_args(id, &[("size", &size)]), |ui| {
            if let Some(c) = component {
                done = crate::batch_video::download_row(ui, &mut ask.download, i18n, c);
            }
            ui.add_space(4.0);
            close |= ui.button(i18n.t("notice-close")).clicked();
        });
        if done {
            log!("ffmpeg asked for: downloaded");
            let save = ask.then_save;
            self.ffmpeg_ask = None;
            if save {
                self.ask_save();
            }
        } else if close {
            log!("ffmpeg asked for: closed");
            self.ffmpeg_ask = None;
            // The Save asked on the way out of the video is dropped with it, as its No does.
            self.after_save = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Source;

    /// The download is offered unless the Options say "the system's only", where it would not be
    /// used: there the window says to install FFmpeg or to change the choice.
    #[test]
    fn download_offered_unless_system_only() {
        assert_eq!(text_of(&Status::SystemOnly), ("batch-video-system-only", false));
        let old = Status::TooOld { found: "4.4".into(), required: "6.1" };
        let lacks = Status::Lacks { found: "8.1".into(), missing: "HEVC".into() };
        for status in [Status::Missing, old, lacks, Status::Ready(Source::System, "8.1".into())] {
            assert_eq!(text_of(&status), ("turn-video-need-ffmpeg", true), "{status:?}");
        }
    }
}
