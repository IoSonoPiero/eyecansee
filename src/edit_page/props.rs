//! The controls of the properties bar for the kinds of object with more than colour, width and
//! opacity: the bubble's shape and letters, the lens's shape and zoom, the stamp's icon.

use eframe::egui;

use super::tools::colours;
use crate::edit::doc::{Stamp, Style};
use crate::i18n::I18n;
use crate::icons::{self, Line};

/// A lens: round or square, how many times it magnifies.
pub(super) fn lens(ui: &mut egui::Ui, style: &mut Style, i18n: &I18n) {
    ui.separator();
    round_or_square(ui, style, i18n);
    ui.separator();
    ui.label(i18n.t("edit-zoom"));
    for zoom in [2, 3, 4] {
        if ui.add(egui::Button::new(format!("{zoom}×")).selected(style.zoom == zoom)).clicked() {
            style.zoom = zoom;
        }
    }
}

/// The icon of a stamp.
pub(super) fn stamp(ui: &mut egui::Ui, style: &mut Style, i18n: &I18n) {
    ui.separator();
    let all = [
        (Stamp::Check, Line::Check, "edit-stamp-check"),
        (Stamp::Cross, Line::Close, "edit-stamp-cross"),
        (Stamp::Warning, Line::Warning, "edit-stamp-warning"),
        (Stamp::Info, Line::Info, "edit-stamp-info"),
        (Stamp::Question, Line::Question, "edit-stamp-question"),
        (Stamp::Star, Line::Star, "edit-stamp-star"),
        (Stamp::ThumbsUp, Line::ThumbsUp, "edit-stamp-thumbs-up"),
    ];
    for (stamp, icon, id) in all {
        if ui.add(icons::toggle(ui, style.stamp == stamp, icon)).on_hover_text(i18n.t(id)).clicked() {
            style.stamp = stamp;
        }
    }
}

/// Round or square (a lens, a spotlight).
pub(super) fn round_or_square(ui: &mut egui::Ui, style: &mut Style, i18n: &I18n) {
    for (round, icon, id) in [(true, Line::Circle, "edit-round"), (false, Line::Rect, "edit-square")] {
        if ui.add(icons::toggle(ui, style.round == round, icon)).on_hover_text(i18n.t(id)).clicked() {
            style.round = round;
        }
    }
}

/// A bubble: rounded or oval, the colour and size of its letters (`long`: the long side of the
/// image, for the largest size).
pub(super) fn bubble(ui: &mut egui::Ui, style: &mut Style, long: f32, i18n: &I18n) {
    ui.separator();
    for (round, icon, id) in [(false, Line::Bubble, "edit-bubble-rounded"), (true, Line::Ellipse, "edit-bubble-oval")] {
        if ui.add(icons::toggle(ui, style.round == round, icon)).on_hover_text(i18n.t(id)).clicked() {
            style.round = round;
        }
    }
    ui.separator();
    ui.label(i18n.t("edit-letters"));
    colours(ui, &mut style.ink, i18n);
    ui.add(egui::Slider::new(&mut style.letters, 4.0..=(long / 4.0).max(100.0)).integer().suffix(" px"));
}
