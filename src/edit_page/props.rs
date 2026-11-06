//! The controls of the properties bar for the kinds of object with more than colour, width and
//! opacity: the bubble's shape and letters, the lens's shape and zoom, the stamp's icon, the
//! outline and the box of a text.

use eframe::egui;

use super::tools::colours;
use crate::edit::doc::{Back, Stamp, Style};
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

/// A text: a line round its letters and a box behind them, each turned on by its button, with
/// its colour and sizes; in pixels of the image, kept as fractions of the letters' height
/// (`style.width` for a text), so they grow with them.
pub(super) fn text_look(ui: &mut egui::Ui, style: &mut Style, i18n: &I18n) {
    let size = style.width.max(1.0);
    let px = |ui: &mut egui::Ui, label: String, frac: &mut f32, lo: f32, hi: f32| {
        ui.label(label);
        let mut v = *frac * size;
        ui.add(egui::Slider::new(&mut v, lo..=hi.max(lo + 1.0)).integer().suffix(" px"));
        *frac = v / size;
    };
    ui.separator();
    if ui.add(egui::Button::new(i18n.t("edit-outline")).selected(style.outline.is_some())).on_hover_text(i18n.t("edit-outline-hint")).clicked() {
        style.outline = if style.outline.is_some() { None } else { Some(([255, 255, 255], 0.08)) };
    }
    if let Some((colour, width)) = &mut style.outline {
        colours(ui, colour, i18n);
        px(ui, i18n.t("edit-width"), width, 1.0, size / 3.0);
    }
    ui.separator();
    if ui.add(egui::Button::new(i18n.t("edit-text-back")).selected(style.back.is_some())).on_hover_text(i18n.t("edit-text-back-hint")).clicked() {
        style.back = if style.back.is_some() { None } else { Some(Back::default()) };
    }
    if let Some(back) = &mut style.back {
        colours(ui, &mut back.color, i18n);
        ui.label(i18n.t("edit-opacity"));
        let mut pct = back.opacity * 100.0;
        ui.add(egui::Slider::new(&mut pct, 5.0..=100.0).integer().suffix(" %"));
        back.opacity = pct / 100.0;
        px(ui, i18n.t("edit-margin"), &mut back.margin, 0.0, size);
        px(ui, i18n.t("edit-corners"), &mut back.radius, 0.0, size);
    }
}

/// The icon of a stamp: the first ones on the bar, the others in a grid behind "…" (shown
/// selected when the stamp in use is one of them).
pub(super) fn stamp(ui: &mut egui::Ui, style: &mut Style, i18n: &I18n) {
    ui.separator();
    let (shown, more) = Stamp::ALL.split_at(Stamp::SHOWN);
    for &stamp in shown {
        if ui.add(icons::toggle(ui, style.stamp == stamp, Line::Mark(stamp))).on_hover_text(i18n.t(stamp.label())).clicked() {
            style.stamp = stamp;
        }
    }
    let button = icons::toggle(ui, more.contains(&style.stamp), if more.contains(&style.stamp) { Line::Mark(style.stamp) } else { Line::More });
    let (r, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        egui::Grid::new("edit-stamps").spacing([4.0, 4.0]).show(ui, |ui| {
            for (n, &stamp) in more.iter().enumerate() {
                if ui.add(icons::toggle(ui, style.stamp == stamp, Line::Mark(stamp))).on_hover_text(i18n.t(stamp.label())).clicked() {
                    style.stamp = stamp;
                    ui.close();
                }
                if n % 10 == 9 {
                    ui.end_row();
                }
            }
        });
    });
    r.on_hover_text(i18n.t("edit-stamp-more"));
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
    use crate::edit::bubble::Balloon;
    for (balloon, id) in Balloon::ALL.into_iter().zip(["edit-bubble-rounded", "edit-bubble-oval", "edit-bubble-square", "edit-bubble-cloud", "edit-bubble-shout"]) {
        if ui.add(icons::toggle(ui, style.balloon == balloon, Line::Balloon(balloon))).on_hover_text(i18n.t(id)).clicked() {
            style.balloon = balloon;
        }
    }
    ui.separator();
    ui.label(i18n.t("edit-letters"));
    colours(ui, &mut style.ink, i18n);
    ui.add(egui::Slider::new(&mut style.letters, 4.0..=(long / 4.0).max(100.0)).integer().suffix(" px"));
}
