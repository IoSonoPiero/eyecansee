//! The controls of the properties bar for the kinds of object with more than colour, width and
//! opacity: the bubble's shape and letters, the lens's shape and zoom, the stamp's icon, the
//! outline and the box of a text. And how a label with its slider stays whole on the bar.

use eframe::egui;

use super::tools::colours;
use crate::edit::doc::{Back, Stamp, Style};
use crate::i18n::I18n;
use crate::icons::{self, Line};

/// A label and its slider on a wrapped bar, kept together on one row: a slider is a group of its
/// own, which the row does not move to the next line, so at the end of a narrow row it was cut at
/// the right edge ("Opacity 100 %" of a text in a 900 px window). The group goes to the next row
/// when the width it had in the last pass does not fit; measured for the first time, the pass is
/// drawn again, so it is never shown cut. `salt`: which slider of the bar it is; `label` may be
/// empty.
pub(super) fn slider(ui: &mut egui::Ui, salt: &str, label: String, slider: egui::Slider) -> egui::Response {
    let id = ui.scope_id().with(("bar-slider", salt));
    let last = ui.data(|d| d.get_temp::<f32>(id));
    let row_start = ui.cursor().min.x <= ui.max_rect().min.x + 0.5;
    if let Some(width) = last
        && !row_start
        && ui.available_size_before_wrap().x < width
    {
        ui.end_row();
    }
    let r = ui.horizontal(|ui| {
        if !label.is_empty() {
            ui.label(label);
        }
        ui.add(slider)
    });
    let width = r.response.rect.width();
    ui.data_mut(|d| d.insert_temp(id, width));
    if last.is_none() {
        ui.ctx().request_discard("a slider of the bar measured");
    }
    r.inner
}

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
    ui.separator();
    if ui.add(egui::Button::new(i18n.t("edit-lens-objects")).selected(style.with_objects)).on_hover_text(i18n.t("edit-lens-objects-hint")).clicked() {
        style.with_objects = !style.with_objects;
    }
}

/// A text: a line round its letters and a box behind them, each turned on by its button, with
/// its colour and sizes; in pixels of the image, kept as fractions of the letters' height
/// (`style.width` for a text), so they grow with them.
pub(super) fn text_look(ui: &mut egui::Ui, style: &mut Style, i18n: &I18n) {
    let size = style.width.max(1.0);
    let px = |ui: &mut egui::Ui, (salt, label): (&str, &str), frac: &mut f32, lo: f32, hi: f32| {
        let mut v = *frac * size;
        slider(ui, salt, i18n.t(label), egui::Slider::new(&mut v, lo..=hi.max(lo + 1.0)).integer().suffix(" px"));
        *frac = v / size;
    };
    ui.separator();
    if ui.add(egui::Button::new(i18n.t("edit-outline")).selected(style.outline.is_some())).on_hover_text(i18n.t("edit-outline-hint")).clicked() {
        style.outline = if style.outline.is_some() { None } else { Some(([255, 255, 255], 0.08)) };
    }
    if let Some((colour, width)) = &mut style.outline {
        colours(ui, colour, i18n);
        px(ui, ("edit-outline", "edit-width"), width, 1.0, size / 3.0);
    }
    ui.separator();
    if ui.add(egui::Button::new(i18n.t("edit-text-back")).selected(style.back.is_some())).on_hover_text(i18n.t("edit-text-back-hint")).clicked() {
        style.back = if style.back.is_some() { None } else { Some(Back::default()) };
    }
    if let Some(back) = &mut style.back {
        colours(ui, &mut back.color, i18n);
        let mut pct = back.opacity * 100.0;
        slider(ui, "edit-text-back", i18n.t("edit-opacity"), egui::Slider::new(&mut pct, 5.0..=100.0).integer().suffix(" %"));
        back.opacity = pct / 100.0;
        px(ui, ("edit-margin", "edit-margin"), &mut back.margin, 0.0, size);
        px(ui, ("edit-corners", "edit-corners"), &mut back.radius, 0.0, size);
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
    slider(ui, "edit-letters", String::new(), egui::Slider::new(&mut style.letters, 4.0..=(long / 4.0).max(100.0)).integer().suffix(" px"));
}
