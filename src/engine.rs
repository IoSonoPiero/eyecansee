//! Which video engine (libmpv) and FFmpeg to use: the system's or the ones downloaded as
//! components (`component.rs`), by the choice in the Options. Read once at startup
//! (`set_choice`): a library already loaded cannot be swapped and the encoder probe is done once,
//! so a change applies at the next start. The minimum versions live with their users:
//! `mpv::MIN_API` and `ffmpeg::MIN_VERSION`.

use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui;

use crate::i18n::I18n;
use crate::settings::{EngineChoice, Settings};

static CHOICE: AtomicU8 = AtomicU8::new(0);

pub fn set_choice(c: EngineChoice) {
    CHOICE.store(c as u8, Ordering::Relaxed);
}

pub fn choice() -> EngineChoice {
    match CHOICE.load(Ordering::Relaxed) {
        1 => EngineChoice::System,
        2 => EngineChoice::Downloaded,
        _ => EngineChoice::SystemIfRecent,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The system's (on Windows, next to the exe).
    System,
    /// The component in the app's data folder.
    Downloaded,
}

/// What an engine is doing, for the viewer, the conversion page and the Options.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    /// In use (or ready to be): where from and which version.
    Ready(Source, String),
    /// Nothing usable: the download is offered where there is a component.
    Missing,
    /// The system's is older than `required` and nothing was downloaded yet.
    TooOld { found: String, required: &'static str },
    /// The options say "the system's only" and there is none.
    SystemOnly,
}

/// Which one to use. `system`: `None` when there is none, else whether it is recent enough;
/// `downloaded`: the component is on disk. `None`: nothing usable (see `status`).
pub fn pick(choice: EngineChoice, system: Option<bool>, downloaded: bool) -> Option<Source> {
    match choice {
        EngineChoice::SystemIfRecent if system == Some(true) => Some(Source::System),
        EngineChoice::SystemIfRecent | EngineChoice::Downloaded => downloaded.then_some(Source::Downloaded),
        EngineChoice::System => system.map(|_| Source::System),
    }
}

/// `pick` and the reason when it picks nothing. `system`: recent enough and the version found.
/// `version`: that of the one picked.
pub fn status(choice: EngineChoice, system: Option<(bool, String)>, downloaded: bool, required: &'static str, version: impl Fn(Source) -> String) -> Status {
    match pick(choice, system.as_ref().map(|s| s.0), downloaded) {
        Some(src) => Status::Ready(src, version(src)),
        None => match (choice, system) {
            (EngineChoice::System, None) => Status::SystemOnly,
            (EngineChoice::SystemIfRecent, Some((false, found))) => Status::TooOld { found, required },
            _ => Status::Missing,
        },
    }
}

/// The Video tab of the Options: the choice, and what is in use now.
pub fn options_ui(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.label(t("opt-engine")).on_hover_text(t("opt-engine-hint"));
    ui.radio_value(&mut s.video_engine, EngineChoice::SystemIfRecent, t("opt-engine-system-recent")).on_hover_text(t("opt-engine-hint"));
    ui.radio_value(&mut s.video_engine, EngineChoice::System, t("opt-engine-system"));
    ui.radio_value(&mut s.video_engine, EngineChoice::Downloaded, t("opt-engine-downloaded"));
    ui.weak(t("opt-engine-restart"));
    ui.add_space(8.0);
    ui.label(i18n.t_args("opt-engine-mpv", &[("what", &status_text(i18n, &crate::mpv::status()))]));
    ui.label(i18n.t_args("opt-engine-ffmpeg", &[("what", &status_text(i18n, &crate::ffmpeg::status()))]));
}

pub fn status_text(i18n: &I18n, status: &Status) -> String {
    match status {
        Status::Ready(Source::System, v) => i18n.t_args("engine-system", &[("version", v)]),
        Status::Ready(Source::Downloaded, v) => i18n.t_args("engine-downloaded", &[("version", v)]),
        Status::Missing => i18n.t("engine-missing"),
        Status::TooOld { found, required } => i18n.t_args("engine-too-old", &[("found", found), ("required", required)]),
        Status::SystemOnly => i18n.t("engine-system-only"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EngineChoice::*;

    #[test]
    fn which_one() {
        // The system's when recent, else the download, else nothing.
        assert_eq!(pick(SystemIfRecent, Some(true), false), Some(Source::System));
        assert_eq!(pick(SystemIfRecent, Some(true), true), Some(Source::System));
        assert_eq!(pick(SystemIfRecent, Some(false), true), Some(Source::Downloaded));
        assert_eq!(pick(SystemIfRecent, Some(false), false), None);
        assert_eq!(pick(SystemIfRecent, None, true), Some(Source::Downloaded));
        assert_eq!(pick(SystemIfRecent, None, false), None);
        // The system's whatever its age, never the download.
        assert_eq!(pick(System, Some(false), true), Some(Source::System));
        assert_eq!(pick(System, None, true), None);
        // The download only.
        assert_eq!(pick(Downloaded, Some(true), true), Some(Source::Downloaded));
        assert_eq!(pick(Downloaded, Some(true), false), None);
    }

    #[test]
    fn why_nothing() {
        let v = |s: Source| format!("{s:?}");
        assert_eq!(status(SystemIfRecent, Some((false, "mpv 0.32".into())), false, "1.109", v), Status::TooOld { found: "mpv 0.32".into(), required: "1.109" });
        assert_eq!(status(SystemIfRecent, Some((false, "mpv 0.32".into())), true, "1.109", v), Status::Ready(Source::Downloaded, "Downloaded".into()));
        assert_eq!(status(SystemIfRecent, None, false, "1.109", v), Status::Missing);
        assert_eq!(status(System, None, true, "1.109", v), Status::SystemOnly);
        assert_eq!(status(System, Some((false, "old".into())), false, "1.109", v), Status::Ready(Source::System, "System".into()));
        assert_eq!(status(Downloaded, Some((true, "new".into())), false, "1.109", v), Status::Missing);
    }

    #[test]
    fn choice_round_trip() {
        for c in [SystemIfRecent, System, Downloaded] {
            set_choice(c);
            assert_eq!(choice(), c);
        }
        set_choice(SystemIfRecent);
    }
}
