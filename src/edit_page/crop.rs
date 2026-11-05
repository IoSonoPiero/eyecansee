//! Rotate and crop, the controls on the right: angle, proportions, fill. Moved from
//! `rotate_page.rs` on 2026-11-05.

use eframe::egui;

use super::{ASPECTS, Aspect, Follow, SIDE, Window};
use crate::encode::Format;
use crate::i18n::I18n;
use crate::transform::Op;

/// The angle, with buttons for the quarter turns and back to zero. `true` if it changed. The
/// quarter turns turn the document (`Doc::turned`): the objects and the crop go with the image,
/// and a document with only quarter turns is saved as the viewer's Save does it.
pub(super) fn angle_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let before = w.doc.angle;
    let mut turned = false;
    ui.strong(i18n.t("rc-rotation"));
    ui.spacing_mut().slider_width = SIDE - 80.0;
    // The language's decimal separator, both shown and typed (a dot works too).
    let sep = i18n.t("decimal-separator");
    ui.add(
        egui::Slider::new(&mut w.doc.angle, -180.0..=180.0)
            .step_by(0.1)
            .suffix("°")
            .custom_formatter(move |v, _| format!("{v:.1}").replace('.', &sep))
            .custom_parser(|s| s.trim().replace(',', ".").parse().ok()),
    );
    ui.horizontal(|ui| {
        for (label, op) in [("−90°", Op::Ccw), ("+90°", Op::Cw)] {
            if ui.button(label).clicked() {
                w.doc.turned(op);
                w.view = None;
                turned = true;
            }
        }
        if ui.add_enabled(w.doc.angle != 0.0, egui::Button::new(i18n.t("rc-reset"))).clicked() {
            w.doc.angle = 0.0;
        }
    });
    // A framed button even when off (`toggle_value` looked like plain text then).
    if ui.add(egui::Button::new(i18n.t("rc-straighten")).selected(w.straighten)).on_hover_text(i18n.t("rc-straighten-hint")).clicked() {
        w.straighten = !w.straighten;
    }
    ui.weak(i18n.t("rc-rotation-hint"));
    // Kept between -180 and 180, the range of the slider.
    w.doc.angle = (w.doc.angle + 180.0).rem_euclid(360.0) - 180.0;
    ui.add_space(12.0);
    turned || w.doc.angle != before
}

/// Proportions and the two ready-made crops. `true` if the crop must be made again.
pub(super) fn crop_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let t = |id| i18n.t(id);
    let label = |a: Aspect| match a {
        Aspect::Free => t("rc-free"),
        Aspect::Original => t("rc-original"),
        Aspect::Ratio(x, y) => format!("{x}:{y}"),
        Aspect::Custom => t("rc-custom"),
    };
    let before = (w.aspect, w.custom, w.swapped, w.follow);
    ui.strong(t("rc-crop"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("rc-aspect").selected_text(label(w.aspect)).show_ui(ui, |ui| {
            for a in ASPECTS {
                ui.selectable_value(&mut w.aspect, a, label(a));
            }
        });
        let square = matches!(w.aspect, Aspect::Free | Aspect::Ratio(1, 1));
        ui.add_enabled(!square, egui::Button::new("⇄")).on_hover_text(t("rc-swap")).clicked().then(|| w.swapped = !w.swapped);
    });
    if w.aspect == Aspect::Custom {
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut w.custom[0]).range(1.0..=100.0).speed(0.05));
            ui.label(":");
            ui.add(egui::DragValue::new(&mut w.custom[1]).range(1.0..=100.0).speed(0.05));
        });
    }
    ui.horizontal(|ui| {
        if ui.button(t("rc-inside")).on_hover_text(t("rc-inside-hint")).clicked() {
            w.follow = Follow::Inside;
        }
        if ui.button(t("rc-whole")).on_hover_text(t("rc-whole-hint")).clicked() {
            w.follow = Follow::Whole;
        }
    });
    ui.add_space(12.0);
    (w.aspect, w.custom, w.swapped, w.follow) != before
}

/// Transparent or a colour where the crop goes past the image.
pub(super) fn fill_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    ui.strong(i18n.t("rc-empty"));
    ui.horizontal(|ui| {
        ui.radio_value(&mut w.transparent, true, i18n.t("rc-transparent"));
        ui.radio_value(&mut w.transparent, false, i18n.t("rc-color"));
        ui.add_enabled_ui(!w.transparent, |ui| egui::color_picker::color_edit_button_srgba(ui, &mut w.color, egui::color_picker::Alpha::Opaque));
    });
    if w.transparent && Format::of_file(&w.path) == Some(Format::Jpeg) {
        ui.weak(i18n.t("rc-jpeg-white"));
    }
}
