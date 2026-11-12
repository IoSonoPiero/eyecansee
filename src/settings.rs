//! User settings in a JSON file, readable and editable by hand.
//! Linux: `$XDG_CONFIG_HOME/eyecansee/settings.json` (usually `~/.config/...`),
//! Windows: `%APPDATA%\eyecansee\settings.json`.
//! Kept apart from the thumbnail cache, which can be deleted, and from the future edit catalogue.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Keys missing from the file take the default value: options can be added without breaking
/// files already written.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    /// When started without arguments, the last folder viewed is reopened.
    pub remember_location: bool,
    /// Location as shown in the bar (never the password).
    pub last_location: Option<String>,
    /// Every folder reopens at the scroll position it was left at; otherwise from the top.
    pub remember_scroll: bool,
    /// The warning about saved servers in plain text (no master password) was shown once.
    pub servers_plain_warned: bool,
    /// In the viewer, after the last image the first one follows (and the other way round).
    pub wrap_around: bool,
    /// In the viewer the window's ✕ goes back to the grid, as Esc; otherwise it closes the app.
    pub close_to_grid: bool,
    /// The mouse wheel zooms the viewer; otherwise it changes image (Ctrl and the wheel always zoom).
    pub wheel_zoom: bool,
    /// Largest zoom in the viewer, in percent of the image's pixels.
    pub max_zoom: u32,
    /// Metadata boxes closed with the X (`meta::Group::key`); empty = all open.
    pub meta_hidden: Vec<String>,
    /// Metadata panel collapsed: only its bar remains.
    pub meta_collapsed: bool,
    pub sort_by: SortBy,
    pub sort_desc: bool,
    /// Which files the grid shows.
    pub show: Show,
    pub thumb_maintenance: bool,
    pub thumb_cache_mb: u32,
    pub window_start: WindowStart,
    /// Size in logical points and state at closing, used with `WindowStart::Remember`.
    pub window_size: Option<[f32; 2]>,
    pub window_maximized: bool,
    pub theme: ThemeChoice,
    /// Code of the chosen language (`en`, `it`); `None` = the system's.
    pub language: Option<String>,
    /// Local files deleted for good instead of going to the system trash (dangerous, off by default).
    pub local_permanent: bool,
    /// Deleting on phones and servers, which have no trash EyeCanSee can reach.
    pub remote_delete: RemoteDelete,
    /// At startup, ask GitHub for a newer release and download it (`update`).
    pub check_updates: bool,
    /// Which libmpv and FFmpeg to use (`engine.rs`).
    pub video_engine: EngineChoice,
    /// Volume of the videos, 0-100, and sound off: as left in the viewer.
    pub video_volume: u32,
    pub video_mute: bool,
    /// Slideshow: seconds each image stays, the passage between images, start again after the last.
    pub slideshow_secs: f32,
    pub slideshow_fade: crate::slideshow::Fade,
    pub slideshow_loop: bool,
    /// The editor's last style for each kind of object (`edit::geom::Kind`), the width (the size
    /// of a text) as a fraction of the long side of the image, so it looks the same on any photo.
    pub edit_styles: Vec<(crate::edit::geom::Kind, crate::edit::doc::Style)>,
    /// The highlighters remembered before it multiplied (at 40%, for a plain fill) are at full
    /// opacity: done once (2026-11-06).
    pub edit_multiply: bool,
    /// What the editor does after Save copy, Update copy or Overwrite (issue #95).
    pub edit_after_save: AfterSave,
    /// The editor's main button and Ctrl+S overwrite the original instead of saving a copy, where
    /// overwriting is possible (issue #96).
    pub edit_main_overwrite: bool,
    /// Bookmarks > places to go back to (`bookmarks.rs`), in the menu's order.
    pub bookmarks: Vec<crate::bookmarks::Bookmark>,
}

/// After Save copy, Update copy or Overwrite in the editor (issue #95): the page stays open (as
/// in 0.76.0), or it closes once the file is written and goes back where it was opened from (the
/// grid selecting the saved file, the viewer showing it) or to the thumbnails in any case.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum AfterSave {
    #[default]
    Stay,
    Back,
    Thumbnails,
}

/// For phones, FTP, SFTP; local files go by `local_permanent`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum RemoteDelete {
    /// Moved into a hidden `.EyeCanSee-trash` folder next to the image.
    #[default]
    TrashFolder,
    Permanent,
}

/// The system's libmpv and FFmpeg, or the components EyeCanSee downloads (`engine.rs`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum EngineChoice {
    /// "Automatic": the system's when at least `mpv::MIN_API` / `ffmpeg::MIN_VERSION` and with
    /// the phones' decoders (`engine::PHONE_CODECS`, issue #67), else the downloaded one; for FFmpeg also the downloaded one when it does more on the card
    /// (`ffmpeg::start_probe`). The old name stays: it is what `settings.json` holds.
    #[default]
    SystemIfRecent,
    /// The system's whatever its version; nothing is downloaded.
    System,
    /// Always the downloaded one, even with a recent one on the system.
    Downloaded,
}

/// Light colours, dark ones or like the system (changes by itself when the system's changes).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Sort order of the images in the grid (folders stay sorted by name).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    Name,
    Date,
    Size,
    Type,
}

impl SortBy {
    pub const ALL: [SortBy; 4] = [SortBy::Name, SortBy::Date, SortBy::Size, SortBy::Type];

    /// Fluent id of the menu item.
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Name => "sort-name",
            SortBy::Date => "sort-date",
            SortBy::Size => "sort-size",
            SortBy::Type => "sort-type",
        }
    }
}

/// Images, videos and audio files, or one kind only.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Show {
    #[default]
    All,
    Images,
    Videos,
    Audio,
}

impl Show {
    pub const ALL: [Show; 4] = [Show::All, Show::Images, Show::Videos, Show::Audio];

    /// Fluent id of the menu item.
    pub fn label(self) -> &'static str {
        match self {
            Show::All => "show-all",
            Show::Images => "show-images",
            Show::Videos => "show-videos",
            Show::Audio => "show-audio",
        }
    }

    pub fn allows(self, path: &str) -> bool {
        match self {
            Show::All => true,
            Show::Images => !crate::decode::is_played(path),
            Show::Videos => crate::decode::is_video(path),
            Show::Audio => crate::decode::is_audio(path),
        }
    }
}

/// How the window opens: one choice at a time.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowStart {
    #[default]
    Normal,
    Maximized,
    Remember,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            remember_location: true,
            last_location: None,
            remember_scroll: false,
            servers_plain_warned: false,
            wrap_around: false,
            close_to_grid: true,
            wheel_zoom: true,
            // As GNOME's image viewer (Loupe).
            max_zoom: 2000,
            meta_hidden: Vec::new(),
            meta_collapsed: false,
            sort_by: SortBy::Name,
            sort_desc: false,
            show: Show::All,
            thumb_maintenance: false,
            thumb_cache_mb: 1024,
            window_start: WindowStart::Normal,
            window_size: None,
            window_maximized: false,
            theme: ThemeChoice::System,
            language: None,
            local_permanent: false,
            remote_delete: RemoteDelete::TrashFolder,
            check_updates: true,
            video_engine: EngineChoice::SystemIfRecent,
            video_volume: 100,
            video_mute: false,
            slideshow_secs: 5.0,
            slideshow_fade: crate::slideshow::Fade::Cross,
            slideshow_loop: true,
            edit_styles: Vec::new(),
            edit_multiply: false,
            // Off: Save copy and Overwrite leave the page open, as up to 0.76.0 (user, 2026-11-05
            // and 2026-11-06); who updates does not find the editor closing without choosing it
            // (user, 2026-11-11).
            edit_after_save: AfterSave::Stay,
            // Overwriting an original is the risky choice: a copy unless asked.
            edit_main_overwrite: false,
            bookmarks: Vec::new(),
        }
    }
}

fn path() -> Option<PathBuf> {
    config_path("settings.json")
}

/// A file in EyeCanSee's configuration folder.
pub(crate) fn config_path(name: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
    };
    Some(base?.join(crate::test_copy::NAME).join(name))
}

impl Settings {
    /// Missing file: default values. Unreadable file: set aside as `.bad`, so a wrong manual
    /// edit is not silently overwritten.
    pub fn load() -> Self {
        let Some(path) = path() else { return Self::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        // A BOM (written by PowerShell and some editors) is not JSON: without this the file was set aside.
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map(Self::upgraded).unwrap_or_else(|_| {
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            Self::default()
        })
    }

    /// Settings of an older EyeCanSee brought up to date: the highlighter remembered at 40%
    /// (a plain fill then) multiplies in full.
    fn upgraded(mut self) -> Self {
        if !self.edit_multiply {
            for (kind, style) in &mut self.edit_styles {
                if *kind == crate::edit::geom::Kind::Highlight {
                    style.opacity = 1.0;
                }
            }
            self.edit_multiply = true;
        }
        self
    }

    /// Written to a temporary file and renamed: never a half-written file. Errors are ignored.
    pub fn save(&self) {
        let Some(path) = path() else { return };
        let Some(dir) = path.parent() else { return };
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A highlighter remembered before the multiply goes to full opacity, once: one chosen after
    /// it is kept.
    #[test]
    fn highlighter_brought_up_to_date() {
        let old = r#"{"edit_styles": [["Highlight", {"color": [255, 230, 0], "width": 0.004, "opacity": 0.4, "dash": "Solid"}], ["Rect", {"color": [255, 0, 0], "width": 0.004, "opacity": 0.4, "dash": "Solid"}]]}"#;
        let s = serde_json::from_str::<Settings>(old).unwrap().upgraded();
        assert_eq!((s.edit_styles[0].1.opacity, s.edit_styles[1].1.opacity, s.edit_multiply), (1.0, 0.4, true));
        let mut again = s.clone();
        again.edit_styles[0].1.opacity = 0.4;
        assert_eq!(again.upgraded().edit_styles[0].1.opacity, 0.4);
    }

    /// A settings.json written by 0.76.0 (no editor keys): the editor behaves as in 0.76.0, the
    /// page open after saving and Save copy as the main save (issues #95, #96).
    #[test]
    fn settings_of_0_76_0_keep_the_editor_as_it_was() {
        // Every key 0.76.0 wrote, none of the two added after it.
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        let keys = old.as_object_mut().unwrap();
        keys.retain(|k, _| k != "edit_after_save" && k != "edit_main_overwrite");
        assert_eq!(keys.len(), 32, "the keys of 0.76.0's settings.json");
        let s: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(s.edit_after_save, AfterSave::Stay, "the page stays open unless chosen");
        assert!(!s.edit_main_overwrite, "the main save: Save copy unless chosen");
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn missing_keys_get_defaults() {
        let s: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50}"#).unwrap();
        assert_eq!(s, Settings { thumb_cache_mb: 50, ..Settings::default() });
        assert!(s.close_to_grid, "the ✕ goes back to the grid unless turned off");
        assert!(s.edit_after_save == AfterSave::Stay && !s.edit_main_overwrite, "the editor: the page stays open, Save copy first");
        let s: Settings = serde_json::from_str(r#"{"edit_after_save": "thumbnails"}"#).unwrap();
        assert_eq!(s.edit_after_save, AfterSave::Thumbnails);
        let s: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50, "window_start": "maximized"}"#).unwrap();
        assert_eq!(s.window_start, WindowStart::Maximized);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
