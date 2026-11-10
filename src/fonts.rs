//! The interface's fonts: egui's plus a bold one for what must stand out in a message.

use std::sync::Arc;

use eframe::egui;

/// Font family of the bold text in message windows (`notice_text`).
pub(crate) const BOLD: &str = "bold";

/// egui's fonts plus Ubuntu Bold, the bold of its Ubuntu Light (same licence), for what must
/// stand out in a message: egui has no bold of its own, `strong` only darkens the colour.
pub(crate) fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(BOLD.into(), Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Ubuntu-Bold.ttf"))));
    let mut family = vec![BOLD.to_string()];
    family.extend(fonts.families[&egui::FontFamily::Proportional].iter().cloned());
    fonts.families.insert(egui::FontFamily::Name(BOLD.into()), family);
    fonts
}
