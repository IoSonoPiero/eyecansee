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
    /// The system's cannot decode `missing` (`PHONE_CODECS`) and nothing was downloaded yet.
    Lacks { found: String, missing: String },
    /// The options say "the system's only" and there is none.
    SystemOnly,
}

/// Whether the system's engine is the one "Automatic" takes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fit {
    Yes,
    /// Older than the version EyeCanSee needs.
    TooOld,
    /// Cannot decode these codecs (`lacking`): "H.264, HEVC".
    Lacks(String),
}

/// What phones record: a system engine without one of them leaves their videos black (issue #67,
/// the libavcodec-free of Fedora has neither). FFmpeg's decoder names, with the name to show.
pub const PHONE_CODECS: [(&str, &str); 2] = [("h264", "H.264"), ("hevc", "HEVC")];

/// The `PHONE_CODECS` that none of `decoders` (FFmpeg's decoder names, `driver` of mpv's
/// `decoder-list`) decodes, as shown. FFmpeg's own decoders only, named as the codec: the wrappers
/// are listed whether or not they work, `libopenh264` of Fedora is a stub until Cisco's library is
/// installed (noopenh264) and `h264_cuvid`, `_qsv`, `_v4l2m2m`, `_amf` need that hardware.
pub fn lacking<S: AsRef<str>>(decoders: &[S]) -> Vec<&'static str> {
    PHONE_CODECS.iter().filter(|(name, _)| !decoders.iter().any(|d| d.as_ref() == *name)).map(|(_, shown)| *shown).collect()
}

/// `Fit` of a system engine: recent enough, then (when its list could be read) its decoders.
pub fn fit<S: AsRef<str>>(recent: bool, decoders: Option<&[S]>) -> Fit {
    let missing = decoders.map(lacking).unwrap_or_default();
    if !recent {
        Fit::TooOld
    } else if !missing.is_empty() {
        Fit::Lacks(missing.join(", "))
    } else {
        Fit::Yes
    }
}

/// Which one to use. `system`: `None` when there is none, else whether "Automatic" takes it (`Fit::Yes`);
/// `downloaded`: the component is on disk. `None`: nothing usable (see `status`).
pub fn pick(choice: EngineChoice, system: Option<bool>, downloaded: bool) -> Option<Source> {
    match choice {
        EngineChoice::SystemIfRecent if system == Some(true) => Some(Source::System),
        EngineChoice::SystemIfRecent | EngineChoice::Downloaded => downloaded.then_some(Source::Downloaded),
        EngineChoice::System => system.map(|_| Source::System),
    }
}

/// `pick` and the reason when it picks nothing. `system`: whether it fits and the version found.
/// `version`: that of the one picked.
pub fn status(choice: EngineChoice, system: Option<(Fit, String)>, downloaded: bool, required: &'static str, version: impl Fn(Source) -> String) -> Status {
    match pick(choice, system.as_ref().map(|s| s.0 == Fit::Yes), downloaded) {
        Some(src) => Status::Ready(src, version(src)),
        None => match (choice, system) {
            (EngineChoice::System, None) => Status::SystemOnly,
            (EngineChoice::SystemIfRecent, Some((Fit::TooOld, found))) => Status::TooOld { found, required },
            (EngineChoice::SystemIfRecent, Some((Fit::Lacks(missing), found))) => Status::Lacks { found, missing },
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
        Status::Lacks { found, missing } => i18n.t_args("engine-lacks", &[("found", found), ("missing", missing)]),
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
        assert_eq!(status(SystemIfRecent, Some((Fit::TooOld, "mpv 0.32".into())), false, "1.109", v), Status::TooOld { found: "mpv 0.32".into(), required: "1.109" });
        assert_eq!(status(SystemIfRecent, Some((Fit::TooOld, "mpv 0.32".into())), true, "1.109", v), Status::Ready(Source::Downloaded, "Downloaded".into()));
        assert_eq!(status(SystemIfRecent, None, false, "1.109", v), Status::Missing);
        assert_eq!(status(System, None, true, "1.109", v), Status::SystemOnly);
        assert_eq!(status(System, Some((Fit::TooOld, "old".into())), false, "1.109", v), Status::Ready(Source::System, "System".into()));
        assert_eq!(status(Downloaded, Some((Fit::Yes, "new".into())), false, "1.109", v), Status::Missing);
        // Without the phones' decoders (issue #67): as too old for "Automatic" only.
        let lacks = || Some((Fit::Lacks("H.264, HEVC".into()), "mpv v0.41.0".into()));
        assert_eq!(status(SystemIfRecent, lacks(), false, "1.109", v), Status::Lacks { found: "mpv v0.41.0".into(), missing: "H.264, HEVC".into() });
        assert_eq!(status(SystemIfRecent, lacks(), true, "1.109", v), Status::Ready(Source::Downloaded, "Downloaded".into()));
        assert_eq!(status(System, lacks(), true, "1.109", v), Status::Ready(Source::System, "System".into()));
        assert_eq!(status(Downloaded, lacks(), true, "1.109", v), Status::Ready(Source::Downloaded, "Downloaded".into()));
        assert_eq!(status(SystemIfRecent, Some((Fit::Yes, "mpv v0.41.0".into())), true, "1.109", v), Status::Ready(Source::System, "System".into()));
    }

    #[test]
    fn phone_decoders() {
        // Fedora's libavcodec-free: H.264 only through the openh264 wrapper, no HEVC.
        let fedora = ["vp9", "av1", "libdav1d", "libopenh264", "aac"];
        assert_eq!(lacking(&fedora), ["H.264", "HEVC"]);
        assert_eq!(fit(true, Some(&fedora[..])), Fit::Lacks("H.264, HEVC".into()));
        // Wrappers that need hardware do not count; FFmpeg's own decoders do.
        assert_eq!(lacking(&["h264_cuvid", "hevc_qsv", "hevc"]), ["H.264"]);
        let full = ["h264", "h264_v4l2m2m", "hevc", "libopenh264"];
        assert!(lacking(&full).is_empty());
        assert_eq!(fit(true, Some(&full[..])), Fit::Yes);
        // Too old comes first; a list that could not be read changes nothing.
        assert_eq!(fit(false, Some(&fedora[..])), Fit::TooOld);
        assert_eq!(fit::<&str>(true, None), Fit::Yes);
        assert_eq!(fit::<&str>(false, None), Fit::TooOld);
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
