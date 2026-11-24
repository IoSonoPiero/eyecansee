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
        // One started from the Convert window and still under way is shown here, not started again.
        crate::downloads::forget_ended(&mut self.downloads.ffmpeg);
        self.ffmpeg_ask = Some(Ask { then });
        true
    }

    /// The window: Esc and Close leave the turn unsaved. Download and Close are one row of
    /// buttons (#311): it opens on Download when there is one to start, on Close otherwise.
    pub(crate) fn ffmpeg_window(&mut self, ctx: &egui::Context) {
        if self.ffmpeg_ask.is_none() {
            return;
        }
        let download = &self.downloads.ffmpeg;
        let (id, offered) = text_of(&crate::ffmpeg::status());
        let component = crate::component::FFMPEG.filter(|_| offered);
        let size = component.map_or(String::new(), |c| format!("{} MB", c.size / 1_000_000));
        let i18n = &self.i18n;
        // Esc arrives as `answer` (`raw_input_hook`); Enter and Space press the highlighted button.
        let modal = "ffmpeg-ask";
        let mut close = crate::confirm::answer(ctx, &mut self.key_answer, modal) == Some(false);
        let (mut done, mut start) = (false, false);
        message(ctx, modal, Head::new(&i18n.t("ffmpeg-ask-title")), &i18n.t_args(id, &[("size", &size)]), &i18n.t("notice-copy"), |ui| {
            let mut rows = Vec::new();
            if let Some(c) = component {
                let (ended, button) = crate::batch_video::download_button(ui, download, i18n, c);
                done = ended;
                rows.extend(button);
            }
            ui.add_space(4.0);
            rows.push(ui.button(i18n.t("notice-close")));
            match crate::confirm::keys(ui, modal, &rows, 0) {
                Some(i) if i + 1 == rows.len() => close = true,
                Some(_) => start = true,
                None => {}
            }
        });
        if let (true, Some(c)) = (start, component) {
            self.downloads.start(c, ctx);
        }
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

    /// #311: Download and Close are one row of buttons. The window opens on Download, which Enter
    /// presses; Tab goes to Close, and Enter there closes the window without downloading (before,
    /// Enter always started the download, whatever was highlighted).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn enter_presses_the_highlighted_button() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("ffmpeg_ask::tests::enter_presses_the_highlighted_button") {
            return;
        }
        let mut h = Harness::new(&folder("ffmpeg-ask", &["a.png"]));
        if crate::component::FFMPEG.is_none() || !text_of(&crate::ffmpeg::status()).1 {
            return;
        }
        // A frame without the window first: it opens anew, not on the button of the last time.
        let open = |h: &mut Harness| {
            h.frame(Vec::new());
            h.app.downloads.ffmpeg = None;
            h.app.ffmpeg_ask = Some(Ask { then: Then::Save });
            h.frame(Vec::new());
            h.frame(Vec::new());
        };
        open(&mut h);
        h.key(egui::Key::Tab);
        h.key(egui::Key::Enter);
        assert!(h.app.ffmpeg_ask.is_none(), "Enter on Close leaves the window open");
        assert!(h.app.downloads.ffmpeg.is_none(), "Enter on Close started the download");
        open(&mut h);
        h.key(egui::Key::Enter);
        assert!(h.app.downloads.ffmpeg.is_some(), "Enter on Download, highlighted at first, did not start it");
        assert!(h.app.ffmpeg_ask.is_some(), "the window stays while it downloads");
        h.key(egui::Key::Escape);
        assert!(h.app.ffmpeg_ask.is_none(), "Esc closes it");
    }
}
