//! FFmpeg offered where a turned video is saved and none can be used (issue #169): what is
//! missing, the download with its progress, then the save goes on. Before, the save failed with
//! "it is a video and FFmpeg is missing", and nothing said that EyeCanSee can download it: with
//! "Always the downloaded one" in the Options and FFmpeg not downloaded yet, the turn could not be
//! saved (Piero, 2026-11-18).

use eframe::egui;

use crate::App;
use crate::engine::Status;
use crate::save_dialogs::{Head, message};

/// The window open: what goes on once FFmpeg is there (the download: `App::downloads`).
pub struct Ask {
    then: Then,
}

/// What was asked when FFmpeg was found missing.
pub enum Then {
    /// The viewer's Save, with its question.
    Save,
    /// The turn of these files in the grid, with its question.
    Turn(Vec<String>, crate::transform::Op),
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
    /// `then` goes on once FFmpeg is there: left to the user to repeat, nothing seemed to have
    /// happened after the download (Piero, 2026-11-18).
    pub(crate) fn ffmpeg_missing(&mut self, then: Then) -> bool {
        if crate::ffmpeg::copier().is_some() {
            return false;
        }
        log!("ffmpeg asked for: {:?}", crate::ffmpeg::status());
        self.downloads.ask_ffmpeg = None;
        self.ffmpeg_ask = Some(Ask { then });
        true
    }

    /// The window: Esc and Close leave the turn unsaved; Enter starts the download.
    pub(crate) fn ffmpeg_window(&mut self, ctx: &egui::Context) {
        if self.ffmpeg_ask.is_none() {
            return;
        }
        let download = &mut self.downloads.ask_ffmpeg;
        let (id, offered) = text_of(&crate::ffmpeg::status());
        let component = crate::component::FFMPEG.filter(|_| offered);
        let size = component.map_or(String::new(), |c| format!("{} MB", c.size / 1_000_000));
        let i18n = &self.i18n;
        // Enter and Esc arrive as `answer` (`raw_input_hook`).
        let modal = "ffmpeg-ask";
        let mut close = match (crate::confirm::answer(ctx, &mut self.key_answer, modal), component) {
            (Some(true), Some(c)) if download.is_none() => {
                let ctx = ctx.clone();
                *download = Some(crate::component::start(c, move || ctx.request_repaint()));
                false
            }
            (Some(true), Some(_)) | (None, _) => false,
            (Some(_), _) => true,
        };
        let mut done = false;
        message(ctx, modal, Head::new(&i18n.t("ffmpeg-ask-title")), &i18n.t_args(id, &[("size", &size)]), &i18n.t("notice-copy"), |ui| {
            if let Some(c) = component {
                done = crate::batch_video::download_row(ui, download, i18n, c);
            }
            ui.add_space(4.0);
            close |= ui.button(i18n.t("notice-close")).clicked();
        });
        if done {
            log!("ffmpeg asked for: downloaded");
            match self.ffmpeg_ask.take().map(|a| a.then) {
                Some(Then::Save) => self.ask_save(),
                Some(Then::Turn(paths, op)) => self.batch_turn = Some((paths, op)),
                None => {}
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
