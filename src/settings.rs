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
    /// Metadata boxes closed with the X (`meta::Group::key`); empty = all open.
    pub meta_hidden: Vec<String>,
    /// Metadata panel collapsed: only its bar remains.
    pub meta_collapsed: bool,
    pub sort_by: SortBy,
    pub sort_desc: bool,
    pub thumb_maintenance: bool,
    pub thumb_cache_mb: u32,
    pub window_start: WindowStart,
    /// Size in logical points and state at closing, used with `WindowStart::Remember`.
    pub window_size: Option<[f32; 2]>,
    pub window_maximized: bool,
    pub theme: ThemeChoice,
    /// Code of the chosen language (`en`, `it`); `None` = the system's.
    pub language: Option<String>,
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
            meta_hidden: Vec::new(),
            meta_collapsed: false,
            sort_by: SortBy::Name,
            sort_desc: false,
            thumb_maintenance: false,
            thumb_cache_mb: 1024,
            window_start: WindowStart::Normal,
            window_size: None,
            window_maximized: false,
            theme: ThemeChoice::System,
            language: None,
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
    };
    Some(base?.join("eyecansee").join("settings.json"))
}

impl Settings {
    /// Missing file: default values. Unreadable file: set aside as `.bad`, so a wrong manual
    /// edit is not silently overwritten.
    pub fn load() -> Self {
        let Some(path) = path() else { return Self::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        serde_json::from_str(&text).unwrap_or_else(|_| {
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            Self::default()
        })
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

    #[test]
    fn missing_keys_get_defaults() {
        let s: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50}"#).unwrap();
        assert_eq!(s, Settings { thumb_cache_mb: 50, ..Settings::default() });
        let s: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50, "window_start": "maximized"}"#).unwrap();
        assert_eq!(s.window_start, WindowStart::Maximized);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
