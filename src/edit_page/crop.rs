//! Rotate and crop, the controls in the properties bar: angle, Straighten, proportions, the two
//! ready-made crops, fill. The quarter turns are in the toolbar. Moved from `rotate_page.rs` on
//! 2026-11-05, in a row since the bars of 2026-11-05.

use eframe::egui;

use super::{ASPECTS, Aspect, Follow, Window};
use crate::encode::Format;
use crate::i18n::I18n;
use crate::icons::{self, Line};

/// Everything in a row; `true` if the crop must be made again.
pub(super) fn controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let changed = angle_controls(ui, w, i18n);
    ui.separator();
    let changed = crop_controls(ui, w, i18n) | changed;
    ui.separator();
    fill_controls(ui, w, i18n);
    changed
}

/// The angle and back to zero, Straighten. `true` if it changed.
fn angle_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let before = w.doc.angle;
    ui.label(i18n.t("rc-rotation"));
    ui.spacing_mut().slider_width = 160.0;
    // The language's decimal separator, both shown and typed (a dot works too).
    let sep = i18n.t("decimal-separator");
    ui.add(
        egui::Slider::new(&mut w.doc.angle, -180.0..=180.0)
            .step_by(0.1)
            .suffix("°")
            .custom_formatter(move |v, _| format!("{v:.1}").replace('.', &sep))
            .custom_parser(|s| s.trim().replace(',', ".").parse().ok()),
    )
    .on_hover_text(i18n.t("rc-rotation-hint"));
    let reset = ui.add_enabled(w.doc.angle != 0.0, egui::Button::new(i18n.t("rc-reset"))).on_disabled_hover_text(i18n.t("rc-reset-off"));
    if reset.clicked() {
        w.doc.angle = 0.0;
    }
    // A framed button even when off (`toggle_value` looked like plain text then).
    if ui.add(egui::Button::new(i18n.t("rc-straighten")).selected(w.straighten)).on_hover_text(i18n.t("rc-straighten-hint")).clicked() {
        w.straighten = !w.straighten;
    }
    // Kept between -180 and 180, the range of the slider.
    w.doc.angle = (w.doc.angle + 180.0).rem_euclid(360.0) - 180.0;
    w.doc.angle != before
}

/// Proportions and the two ready-made crops. `true` if the crop must be made again.
fn crop_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let t = |id| i18n.t(id);
    let label = |a: Aspect| match a {
        Aspect::Free => t("rc-free"),
        Aspect::Original => t("rc-original"),
        Aspect::Ratio(x, y) => format!("{x}:{y}"),
        Aspect::Custom => t("rc-custom"),
    };
    let before = (w.aspect, w.custom, w.swapped, w.follow);
    ui.label(t("rc-crop"));
    egui::ComboBox::from_id_salt("rc-aspect").selected_text(label(w.aspect)).show_ui(ui, |ui| {
        for a in ASPECTS {
            ui.selectable_value(&mut w.aspect, a, label(a));
        }
    });
    if w.aspect == Aspect::Custom {
        ui.add(egui::DragValue::new(&mut w.custom[0]).range(1.0..=100.0).speed(0.05));
        ui.label(":");
        ui.add(egui::DragValue::new(&mut w.custom[1]).range(1.0..=100.0).speed(0.05));
    }
    let square = matches!(w.aspect, Aspect::Free | Aspect::Ratio(1, 1));
    let swap = ui.add_enabled(!square, icons::button(ui, Line::Swap)).on_hover_text(t("rc-swap")).on_disabled_hover_text(format!("{}\n{}", t("rc-swap"), t("rc-swap-off")));
    if swap.clicked() {
        w.swapped = !w.swapped;
    }
    if ui.button(t("rc-inside")).on_hover_text(t("rc-inside-hint")).clicked() {
        w.follow = Follow::Inside;
    }
    if ui.button(t("rc-whole")).on_hover_text(t("rc-whole-hint")).clicked() {
        w.follow = Follow::Whole;
    }
    (w.aspect, w.custom, w.swapped, w.follow) != before
}

/// Transparent or a colour where the crop goes past the image.
fn fill_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    ui.label(i18n.t("rc-empty"));
    let jpeg = Format::of_file(&w.path) == Some(Format::Jpeg);
    let r = ui.radio_value(&mut w.transparent, true, i18n.t("rc-transparent"));
    if jpeg {
        r.on_hover_text(i18n.t("rc-jpeg-white"));
    }
    ui.radio_value(&mut w.transparent, false, i18n.t("rc-color"));
    ui.add_enabled_ui(!w.transparent, |ui| egui::color_picker::color_edit_button_srgba(ui, &mut w.color, egui::color_picker::Alpha::Opaque))
        .response
        .on_disabled_hover_text(i18n.t("rc-color-off"));
}
