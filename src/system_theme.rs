//! The desktop's light or dark preference, for Colors > Same as the system (#100).
//! winit 0.30 never reports it on Linux (`system_theme()` is `None`, no `ThemeChanged`), so egui
//! fell back to its dark colours on every desktop: there it comes from the desktop portal
//! (`portal`). Each frame `fill` hands it to egui as the system's theme
//! (`RawInput::system_theme`), so `ThemePreference::System` follows it, and Light and Dark chosen
//! in the Options are untouched.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{self, Theme};

mod portal;

/// 0 while not read yet, then 1 dark, 2 light.
static READ: AtomicU8 = AtomicU8::new(0);
/// The window's context, known from the first frame: a change asks it for a new frame.
static CTX: OnceLock<egui::Context> = OnceLock::new();

/// Starts reading and following the preference, in the background.
pub fn start() {
    portal::start();
}

/// Each frame: the portal's theme as the system's, once read; until then egui's own fallback.
pub fn fill(ctx: &egui::Context, raw_input: &mut egui::RawInput) {
    CTX.get_or_init(|| ctx.clone());
    raw_input.system_theme = match READ.load(Ordering::SeqCst) {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    };
}

fn store(theme: Theme) {
    let code = if theme == Theme::Dark { 1 } else { 2 };
    if READ.swap(code, Ordering::SeqCst) != code
        && let Some(ctx) = CTX.get()
    {
        ctx.request_repaint();
    }
}
