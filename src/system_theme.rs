//! The desktop's light or dark preference, for Colors > Same as the system (#100).
//! winit 0.30 never reports it on Linux (`system_theme()` is `None`, no `ThemeChanged`), so egui
//! fell back to its dark colours on every desktop: there it comes from the desktop portal
//! (`portal`). On Windows winit follows a change one step late, so it comes from the registry
//! (`registry`). Each frame `fill` hands it to egui as the system's theme
//! (`RawInput::system_theme`), so `ThemePreference::System` follows it, and Light and Dark chosen
//! in the Options are untouched. `sync_window` gives it to the title bar too (Windows, and the
//! frame winit draws itself on Wayland).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{self, Theme};

#[cfg(target_os = "linux")]
mod portal;
#[cfg(windows)]
mod registry;

/// 0 while not read yet, then 1 dark, 2 light.
static READ: AtomicU8 = AtomicU8::new(0);
/// The window's context, known from the first frame: a change asks it for a new frame.
static CTX: OnceLock<egui::Context> = OnceLock::new();
/// The title bar's theme last sent to the window: 0 none yet, 1 dark, 2 light, 3 the system's as
/// winit sees it.
#[cfg(any(target_os = "linux", windows))]
static SENT: AtomicU8 = AtomicU8::new(0);

/// Starts reading and following the preference, in the background.
pub fn start() {
    #[cfg(target_os = "linux")]
    portal::start();
    #[cfg(windows)]
    registry::start();
}

/// Each frame: the theme read as the system's; until it is read, what winit gave (on Linux
/// nothing, so egui's own fallback).
pub fn fill(ctx: &egui::Context, raw_input: &mut egui::RawInput) {
    CTX.get_or_init(|| ctx.clone());
    if let Some(theme) = read() {
        raw_input.system_theme = Some(theme);
    }
}

fn read() -> Option<Theme> {
    match READ.load(Ordering::SeqCst) {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

/// Each frame: the title bar's theme. egui's own sync leaves it to winit with "the system's"
/// (`SetTheme(SystemDefault)`): on Windows winit follows that one step late, like it did the
/// colours; on Wayland the frame winit draws (sctk-adwaita, `FrameConfig::auto()`) picks light or
/// dark once, when it is made, and never follows a change (#108). So egui's sync is off and the
/// title bar gets the theme chosen, or the one read here. Where the desktop draws the title bar
/// (KDE on Wayland), winit only keeps the value.
#[cfg(any(target_os = "linux", windows))]
pub fn sync_window(ctx: &egui::Context) {
    let theme = window_theme(ctx.options(|o| o.theme_preference), read());
    let code = match theme {
        egui::SystemTheme::Dark => 1,
        egui::SystemTheme::Light => 2,
        egui::SystemTheme::SystemDefault => 3,
    };
    if SENT.swap(code, Ordering::SeqCst) != code {
        log!("system theme: title bar {theme:?}");
        ctx.options_mut(|o| o.sync_window_theme = false);
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(theme));
    }
}

/// Windows, a moment after a change: the title bar's theme sent again in the next frame, over the
/// one Windows put back.
#[cfg(windows)]
fn resend_window() {
    SENT.store(0, Ordering::SeqCst);
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// The title bar's theme for the preference in the Options and the system's theme read (`None`
/// until read: winit's own, right at start).
#[cfg(any(target_os = "linux", windows))]
fn window_theme(preference: egui::ThemePreference, system: Option<Theme>) -> egui::SystemTheme {
    use egui::{SystemTheme, ThemePreference};
    match (preference, system) {
        (ThemePreference::Dark, _) | (ThemePreference::System, Some(Theme::Dark)) => SystemTheme::Dark,
        (ThemePreference::Light, _) | (ThemePreference::System, Some(Theme::Light)) => SystemTheme::Light,
        (ThemePreference::System, None) => SystemTheme::SystemDefault,
    }
}

fn store(theme: Theme) {
    let code = if theme == Theme::Dark { 1 } else { 2 };
    if READ.swap(code, Ordering::SeqCst) != code
        && let Some(ctx) = CTX.get()
    {
        ctx.request_repaint();
    }
}

#[cfg(all(test, any(target_os = "linux", windows)))]
mod tests {
    use super::*;
    use egui::{SystemTheme, ThemePreference};

    #[test]
    fn title_bar_theme() {
        assert_eq!(window_theme(ThemePreference::System, Some(Theme::Dark)), SystemTheme::Dark);
        assert_eq!(window_theme(ThemePreference::System, Some(Theme::Light)), SystemTheme::Light);
        assert_eq!(window_theme(ThemePreference::System, None), SystemTheme::SystemDefault);
        // Chosen in the Options: the system's does not count.
        assert_eq!(window_theme(ThemePreference::Light, Some(Theme::Dark)), SystemTheme::Light);
        assert_eq!(window_theme(ThemePreference::Dark, None), SystemTheme::Dark);
    }
}
