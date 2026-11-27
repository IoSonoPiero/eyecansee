//! User settings in a JSON file, readable and editable by hand.
//! Linux: `$XDG_CONFIG_HOME/eyecansee/settings.json` (usually `~/.config/...`),
//! Windows: `%APPDATA%\eyecansee\settings.json`.
//! Kept apart from the thumbnail cache, which can be deleted, and from the future edit catalogue.

use std::path::{Path, PathBuf};

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
    /// What goes under each thumbnail (#139): at first only the name.
    pub thumb_details: crate::thumb_details::Details,
    /// The mouse wheel zooms the viewer; otherwise it changes image (Ctrl and the wheel always zoom).
    pub wheel_zoom: bool,
    /// A double click in the viewer goes to full screen and back (#349); otherwise back to the grid.
    pub double_click_full: bool,
    /// Largest zoom in the viewer, in percent of the image's pixels.
    pub max_zoom: u32,
    /// Pictures shown smaller than 100% are filtered (`mipmap`, `smooth_gpu`, #151); off: the
    /// graphics card's linear filter alone, as up to 0.76.2.
    pub smooth: bool,
    /// Which filter, and from where (`smooth_gpu`).
    pub smooth_filter: SmoothFilter,
    /// On Windows, what draws on the graphics card (`render_gpu::adapter_selector`, #171).
    pub graphics: Graphics,
    /// Metadata boxes closed with the X (`meta::Group::key`); empty = all open.
    pub meta_hidden: Vec<String>,
    /// Metadata panel collapsed: only its bar remains.
    pub meta_collapsed: bool,
    /// Height of the open metadata panel, as a part of the window's height (dragged by its top edge).
    pub meta_size: f32,
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
    /// The sizes of Resize (`presets`); `None`: the first ones, named in the language shown.
    pub resize_presets: Option<Vec<crate::presets::Preset>>,
    /// A tip at the first start of the day (`tips`, issue #113); off from its box or Options.
    pub tips: bool,
    /// The tip the next start shows, and the day (since 1970, UTC) one was last shown.
    pub tip_next: usize,
    pub tips_day: Option<u64>,
    /// The first-start window was closed with its welcome page (#212): it is not shown again.
    pub welcome_seen: bool,
    /// The add-ons (`Component::kind`) the first-start window already offered: a later version
    /// brings it back only for a new one.
    pub addons_offered: Vec<String>,
    /// Options > Diagnostics: each start writes a log file in the data folder (`logging`, #189).
    /// Read from the next start, before the rest of the settings (`log_switch`).
    pub diagnostic_log: bool,
    /// The file is there but could not be read (no permission, held by another program): this
    /// start leaves it alone and does not save over it. Never in the file.
    #[serde(skip)]
    pub(crate) not_read: bool,
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

/// The filter of pictures shown below 100% (`smooth_gpu`): Lanczos from the halved copy just larger
/// than the screen, Lanczos from the whole picture, or Mitchell (softer) from the whole picture.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SmoothFilter {
    Halves,
    // The fewest false patterns (`smooth_gpu`'s zone plate test).
    #[default]
    Whole,
    Mitchell,
}

/// On Windows, what EyeCanSee draws on the graphics card with. DirectX 12 stretches the last
/// picture drawn while the window changes size, which entering full screen counts on
/// (`viewer::enter_full`, #171); Vulkan is what every version up to 0.76.2 used, kept as a way
/// back for a card that draws badly with DirectX 12. Read at startup.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Graphics {
    #[default]
    Directx12,
    Vulkan,
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
    Normal,
    Maximized,
    /// The default since 0.78.0 (#221): only a settings.json without the key gets it.
    #[default]
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
            thumb_details: Default::default(),
            wheel_zoom: true,
            double_click_full: false,
            // As GNOME's image viewer (Loupe).
            max_zoom: 2000,
            smooth: true,
            smooth_filter: SmoothFilter::default(),
            graphics: Graphics::default(),
            meta_hidden: Vec::new(),
            meta_collapsed: false,
            meta_size: 0.25,
            sort_by: SortBy::Name,
            sort_desc: false,
            show: Show::All,
            thumb_maintenance: false,
            thumb_cache_mb: 1024,
            window_start: WindowStart::Remember,
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
            // Off: Save copy and Overwrite leave the page open, as up to 0.76.0; who updates
            // does not find the editor closing without choosing it.
            edit_after_save: AfterSave::Stay,
            // Overwriting an original is the risky choice: a copy unless asked.
            edit_main_overwrite: false,
            bookmarks: Vec::new(),
            resize_presets: None,
            // On at the very first start, and for who updates (the key missing): issue #113.
            tips: true,
            tip_next: 0,
            tips_day: None,
            // Also for who updates from a version without the window (the keys missing): #212.
            welcome_seen: false,
            addons_offered: Vec::new(),
            diagnostic_log: false,
            not_read: false,
        }
    }
}

fn path() -> Option<PathBuf> {
    config_path("settings.json")
}

/// The switch of the diagnostic log (#189), read at the start of `main`, before the log knows where
/// it goes: so without writing to it, and without setting a file aside (`Settings::load` does that
/// later). As `Settings::load` reads it: false where that reads the default.
pub(crate) fn log_switch() -> bool {
    path().is_some_and(|p| log_switch_in(&p))
}

fn log_switch_in(path: &Path) -> bool {
    let Some(text) = std::fs::read(path).ok().and_then(|b| String::from_utf8(b).ok()) else { return false };
    serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v.get("diagnostic_log")?.as_bool()).unwrap_or(false)
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
    /// Missing file: default values. A file that is not UTF-8 (UTF-16 or ANSI as PowerShell and
    /// Notepad may save it) or is not JSON: set aside as `.bad`, so the save at exit does not
    /// overwrite what the user wrote. A file that cannot be read now (no permission, held open by
    /// another program, a folder in its place): default values, the file left alone and never
    /// saved over during this start (`not_read`; audit 27/11, doubt of area 6). A value this version does not know (a
    /// file written by a newer one, a wrong edit): only that key takes its default (`tolerant`).
    pub fn load() -> Self {
        path().map_or_else(Self::default, |p| Self::load_from(&p))
    }

    /// `load` from a given file.
    pub(crate) fn load_from(path: &Path) -> Self {
        let bad = path.with_extension("json.bad");
        let text = match std::fs::read(path).map(String::from_utf8) {
            Ok(Ok(text)) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                log!("settings: {path:?} not read ({e}), left alone and not saved over");
                return Self { not_read: true, ..Self::default() };
            }
            Ok(Err(e)) => {
                log!("settings: {path:?} not UTF-8 ({e}), set aside");
                let _ = std::fs::rename(path, &bad);
                return Self::default();
            }
        };
        // A BOM (written by PowerShell and some editors) is not JSON: without this the file was set aside.
        let text = text.trim_start_matches('\u{feff}');
        if let Ok(s) = serde_json::from_str::<Self>(text) {
            return s.upgraded();
        }
        if let Some(s) = Self::tolerant(text) {
            // Copied, not moved: the file stays the settings until the next save.
            log!("settings: {path:?} read key by key, copied aside");
            let _ = std::fs::copy(path, &bad);
            return s.upgraded();
        }
        log!("settings: {path:?} is not JSON, set aside");
        let _ = std::fs::rename(path, &bad);
        Self::default()
    }

    /// The keys of a JSON object one at a time: a key whose value does not fit keeps its default,
    /// the others are taken. `None` when the text is not a JSON object.
    fn tolerant(text: &str) -> Option<Self> {
        let serde_json::Value::Object(keys) = serde_json::from_str(text).ok()? else { return None };
        let mut good = serde_json::Map::new();
        for (key, value) in keys {
            let one = serde_json::Value::Object(serde_json::Map::from_iter([(key.clone(), value.clone())]));
            match serde_json::from_value::<Self>(one) {
                Ok(_) => {
                    good.insert(key, value);
                }
                Err(e) => log!("settings: {key:?} not understood ({e}), default kept"),
            }
        }
        serde_json::from_value(serde_json::Value::Object(good)).ok()
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

    /// The window as it is now, for `WindowStart::Remember`: its size only when not maximised.
    /// At full screen nothing: what is remembered is the window it came from (the desktop may
    /// not call a full screen maximised).
    pub(crate) fn note_window(&mut self, size: egui::Vec2, viewport: &egui::ViewportInfo) {
        if viewport.fullscreen == Some(true) {
            return;
        }
        self.window_maximized = viewport.maximized.unwrap_or(false);
        if !self.window_maximized {
            self.window_size = Some([size.x, size.y]);
        }
    }

    /// Written to a temporary file, flushed to the disk and renamed: never a half-written file.
    /// Errors are ignored.
    pub fn save(&self) {
        if let Some(path) = path() {
            self.save_to(&path);
        }
    }

    /// `save` to a given file.
    pub(crate) fn save_to(&self, path: &Path) {
        if self.not_read {
            log!("settings: not saved, {path:?} could not be read at the start");
            return;
        }
        let Some(dir) = path.parent() else { return };
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        // On the disk before it replaces the old file: after a power cut the renamed file could
        // otherwise be empty on some file systems.
        let written = std::fs::create_dir_all(dir).and_then(|()| {
            let mut f = std::fs::File::create(&tmp)?;
            std::io::Write::write_all(&mut f, text.as_bytes())?;
            f.sync_all()
        });
        match written {
            Ok(()) => {
                let _ = std::fs::rename(&tmp, path);
            }
            Err(_) => {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

/// What changes while EyeCanSee runs without passing from the Options: the folder open, the
/// size of the window, the volume. Saved also before the exit (audit 1.11 d): a program that ends
/// badly (a crash, the session closed, the computer switched off) starts again where it was.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Moving {
    pub(crate) location: Option<String>,
    pub(crate) size: Option<[f32; 2]>,
    pub(crate) maximized: bool,
    pub(crate) volume: u32,
}

/// How long the size and the volume stay the same before they are saved: dragging the window's
/// border or the volume is many changes, one save.
pub(crate) const QUIET: std::time::Duration = std::time::Duration::from_secs(3);

/// When `Moving` is saved. `saved`: what the file holds; `seen`: a change not saved yet, and
/// since when it has stayed the same.
pub(crate) struct Autosave {
    saved: Moving,
    seen: Option<(Moving, std::time::Instant)>,
}

impl Autosave {
    pub(crate) fn new(saved: Moving) -> Self {
        Autosave { saved, seen: None }
    }

    /// Whether `now` is the time to save `m`; then it counts as saved. A new folder at once, the
    /// rest once it has stayed the same for `QUIET`; a change taken back is nothing to save.
    pub(crate) fn due(&mut self, m: &Moving, now: std::time::Instant) -> bool {
        if *m == self.saved {
            self.seen = None;
            return false;
        }
        let quiet = match &self.seen {
            Some((seen, since)) if seen == m => now.duration_since(*since) >= QUIET,
            _ => {
                self.seen = Some((m.clone(), now));
                false
            }
        };
        if !quiet && m.location == self.saved.location {
            return false;
        }
        self.saved = m.clone();
        self.seen = None;
        true
    }

    /// A change waits for `QUIET`: the caller asks for a pass after it.
    pub(crate) fn waiting(&self) -> bool {
        self.seen.is_some()
    }
}

impl crate::App {
    /// Every pass, also with the window hidden (`App::logic`): the folder, the size of the window
    /// and the volume saved when `Autosave` says so.
    pub(crate) fn save_when_moved(&mut self, ctx: &eframe::egui::Context) {
        static AUTOSAVE: std::sync::Mutex<Option<Autosave>> = std::sync::Mutex::new(None);
        let s = &self.settings;
        let location = if self.loader.is_some() { Some(self.display(&self.dir)) } else { s.last_location.clone() };
        let now = Moving { location, size: s.window_size, maximized: s.window_maximized, volume: s.video_volume };
        let mut autosave = AUTOSAVE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let autosave = autosave.get_or_insert_with(|| Autosave::new(Moving { location: s.last_location.clone(), ..now.clone() }));
        if autosave.due(&now, std::time::Instant::now()) {
            self.settings.last_location = now.location;
            self.settings.save();
        } else if autosave.waiting() {
            ctx.request_repaint_after(QUIET);
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
        // Every key 0.76.0 wrote, none of those added after it.
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        let keys = old.as_object_mut().unwrap();
        keys.retain(|k, _| !["edit_after_save", "edit_main_overwrite", "resize_presets", "thumb_details", "tips", "tip_next", "tips_day", "smooth", "smooth_filter", "meta_size", "graphics", "welcome_seen", "addons_offered", "diagnostic_log", "double_click_full"].contains(&k.as_str()));
        assert_eq!(keys.len(), 32, "the keys of 0.76.0's settings.json");
        let s: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(s.edit_after_save, AfterSave::Stay, "the page stays open unless chosen");
        assert!(!s.edit_main_overwrite, "the main save: Save copy unless chosen");
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    /// "Size it had when closed" is the default (#221), but a settings.json that holds a choice
    /// keeps it: every file written by a published version has the key.
    #[test]
    fn window_start_remember_by_default_saved_choice_kept() {
        assert_eq!(Settings::default().window_start, WindowStart::Remember);
        let old: Settings = serde_json::from_str(r#"{"window_start": "normal"}"#).unwrap();
        assert_eq!(old.window_start, WindowStart::Normal);
        let without: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50}"#).unwrap();
        assert_eq!(without.window_start, WindowStart::Remember);
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

    /// A folder of its own for a test's settings.json, empty.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A settings.json that cannot be read as text (UTF-16, as PowerShell and Notepad may save it)
    /// is set aside as `.bad`: the save at exit does not overwrite what the user wrote (audit 1.11).
    #[test]
    fn unreadable_file_set_aside_not_overwritten() {
        let dir = test_dir("utf16");
        let path = dir.join("settings.json");
        let text = r#"{"bookmarks": [], "thumb_cache_mb": 50}"#;
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain(text.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        std::fs::write(&path, &utf16).unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
        Settings::default().save_to(&path);
        assert_eq!(std::fs::read(path.with_extension("json.bad")).ok(), Some(utf16), "the file is set aside as it was");
        // An error other than "not found" (here a folder where the file should be): left alone
        // (audit 27/11), and not saved over.
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("json.bad")).unwrap();
        std::fs::create_dir(&path).unwrap();
        let s = Settings::load_from(&path);
        assert_eq!(s, Settings { not_read: true, ..Settings::default() });
        s.save_to(&path);
        assert!(path.is_dir() && !path.with_extension("json.bad").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Audit 27/11 (doubt of area 6, #285): a file that cannot be read for a moment (here without
    /// read permission; on Windows one held open by another program) is left alone: not set
    /// aside, and the settings of this start are not saved over it.
    #[cfg(unix)]
    #[test]
    fn file_not_read_for_a_moment_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = test_dir("no-read");
        let path = dir.join("settings.json");
        let text = r#"{"thumb_cache_mb": 50}"#;
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
        let s = Settings::load_from(&path);
        s.save_to(&path);
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        let left = (std::fs::read_to_string(&path).ok(), path.with_extension("json.bad").exists());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(left, (Some(text.to_string()), false));
    }

    /// Audit 1.11 (d): a new folder is saved at once; the size of the window and the volume once
    /// they have stayed the same for `QUIET`, one save for a whole drag; nothing when nothing
    /// changed.
    #[test]
    fn moving_settings_saved_before_the_exit() {
        use std::time::{Duration, Instant};
        let m = |location: &str, width: f32, volume: u32| Moving { location: Some(location.into()), size: Some([width, 600.0]), maximized: false, volume };
        let t = Instant::now();
        let at = |ms: u64| t + Duration::from_millis(ms);
        let mut a = Autosave::new(m("/a", 900.0, 100));
        assert!(!a.due(&m("/a", 900.0, 100), at(0)), "nothing changed");
        assert!(a.due(&m("/b", 900.0, 100), at(10)), "a new folder at once");
        assert!(!a.due(&m("/b", 900.0, 100), at(20)), "saved already");
        // The border dragged for a second, then left.
        for (i, width) in [910.0, 950.0, 1000.0].into_iter().enumerate() {
            assert!(!a.due(&m("/b", width, 100), at(100 + 500 * i as u64)), "while it moves");
        }
        assert!(a.waiting());
        assert!(!a.due(&m("/b", 1000.0, 100), at(1100 + 2900)), "not quiet long enough");
        assert!(a.due(&m("/b", 1000.0, 100), at(1100 + 3000)), "quiet for QUIET");
        assert!(!a.waiting() && !a.due(&m("/b", 1000.0, 100), at(5000)));
        assert!(!a.due(&m("/b", 1000.0, 40), at(6000)));
        assert!(a.due(&m("/b", 1000.0, 40), at(9000)), "the volume too");
        // A change taken back before it is saved: nothing to save.
        assert!(!a.due(&m("/b", 1000.0, 60), at(10_000)));
        assert!(!a.due(&m("/b", 1000.0, 40), at(14_000)) && !a.waiting());
    }

    /// A value this version does not know (a file written by a newer one) loses only its key: the
    /// others are kept, and the file is copied aside as `.bad` (audit 1.11).
    #[test]
    fn unknown_value_loses_only_its_key() {
        let dir = test_dir("unknown");
        let path = dir.join("settings.json");
        let text = r#"{"theme": "sepia", "thumb_cache_mb": 50, "sort_by": "date", "max_zoom": "much"}"#;
        std::fs::write(&path, text).unwrap();
        let s = Settings::load_from(&path);
        assert_eq!(s, Settings { thumb_cache_mb: 50, sort_by: SortBy::Date, edit_multiply: true, ..Settings::default() });
        assert_eq!(std::fs::read_to_string(path.with_extension("json.bad")).ok().as_deref(), Some(text));
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some(text), "the file stays until the next save");
        // Not JSON at all: set aside, the defaults.
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #189: the switch of the log, read before the rest without a log, is what `load_from` reads:
    /// on with an unknown value in another key, off where its own value is not understood, the
    /// file is not JSON or is missing; and the peek sets nothing aside.
    #[test]
    fn the_log_switch_read_before_the_rest_as_the_rest() {
        let dir = test_dir("log-switch");
        let path = dir.join("settings.json");
        for text in [
            r#"{"diagnostic_log": true}"#,
            r#"{"theme": "sepia", "diagnostic_log": true}"#,
            r#"{"diagnostic_log": "yes", "theme": "dark"}"#,
            r#"{"diagnostic_log": false}"#,
            "{ not json, \"diagnostic_log\": true",
            "{}",
        ] {
            std::fs::write(&path, text).unwrap();
            let peek = log_switch_in(&path);
            assert!(path.exists() && !path.with_extension("json.bad").exists(), "{text}");
            assert_eq!(peek, Settings::load_from(&path).diagnostic_log, "{text}");
            let _ = std::fs::remove_file(path.with_extension("json.bad"));
        }
        assert!(!log_switch_in(&dir.join("missing.json")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// At full screen (F11, the slideshow) the window's size and state stay as they were: closed
    /// from there, with "remember" the window reopened as large as the screen (audit 1.11).
    #[test]
    fn full_screen_not_remembered() {
        let mut s = Settings::default();
        let normal = egui::ViewportInfo { maximized: Some(false), fullscreen: Some(false), ..Default::default() };
        s.note_window(egui::vec2(900.0, 600.0), &normal);
        assert_eq!((s.window_size, s.window_maximized), (Some([900.0, 600.0]), false));
        let full = egui::ViewportInfo { maximized: Some(false), fullscreen: Some(true), ..Default::default() };
        s.note_window(egui::vec2(1920.0, 1080.0), &full);
        assert_eq!((s.window_size, s.window_maximized), (Some([900.0, 600.0]), false));
        // Full screen from a maximised window: still maximised, whatever the desktop says then.
        s.note_window(egui::vec2(1920.0, 1040.0), &egui::ViewportInfo { maximized: Some(true), ..Default::default() });
        s.note_window(egui::vec2(1920.0, 1080.0), &full);
        assert_eq!((s.window_size, s.window_maximized), (Some([900.0, 600.0]), true));
    }

    /// A settings file from before #151 has no `smooth`: the pictures are smoothed.
    #[test]
    fn smooth_is_on_in_an_old_file() {
        let s: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50, "wheel_zoom": false}"#).unwrap();
        assert!(s.smooth);
        assert!(Settings::default().smooth);
        let off: Settings = serde_json::from_str(r#"{"smooth": false}"#).unwrap();
        assert!(!off.smooth);
    }

    /// A settings file from before #349 has no `double_click_full`: a double click goes back to
    /// the grid; "Goes to full screen" chosen is read and written back as it is.
    #[test]
    fn double_click_goes_back_to_the_grid_in_an_old_file() {
        let s: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50, "wheel_zoom": false}"#).unwrap();
        assert!(!s.double_click_full);
        assert!(!Settings::default().double_click_full);
        let full: Settings = serde_json::from_str(r#"{"double_click_full": true}"#).unwrap();
        assert!(full.double_click_full);
        let written = serde_json::to_value(&full).unwrap();
        assert_eq!(written["double_click_full"], serde_json::Value::Bool(true));
        let again: Settings = serde_json::from_value(written).unwrap();
        assert!(again.double_click_full);
    }
}
