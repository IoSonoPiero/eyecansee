//! The controls of the properties bar for the kinds of object with more than colour, width and
//! opacity: the bubble's shape and letters.

use eframe::egui;

use super::tools::colours;
use crate::edit::doc::Style;
use crate::i18n::I18n;
use crate::icons::{self, Line};

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
