//! The piece of image pasted (`edit::patch`) in the properties bar: its angle, quarter turns,
//! mirrors, size with its proportions, to the front or to the back. Each button is one step of
//! the history (`Window::settle`), and every change is to its box alone: the pixels pasted stay.

use eframe::egui;

use super::Window;
use super::tools::bar_button;
use crate::edit::doc::Shape;
use crate::edit::patch;
use crate::i18n::I18n;
use crate::icons::{self, Line};

/// How far above the box its turn handle is, in points of the screen.
pub(super) const REACH: f32 = 24.0;

/// The controls of the selected piece `i`, after its opacity.
pub(super) fn bar(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let (angle, keep) = (w.doc.angle, w.keep_patch);
    let Some(Shape::Patch { area, mirrored, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) else { return };
    ui.separator();
    let mut shown = patch::shown(area.rot, angle);
    ui.spacing_mut().slider_width = 110.0;
    let slider = egui::Slider::new(&mut shown, -180.0..=180.0).integer().suffix("°");
    if super::props::slider(ui, "rc-rotation", i18n.t("rc-rotation"), slider).on_hover_text(i18n.t("edit-patch-turn-hint")).changed() {
        area.rot = patch::rot_for(shown, angle);
    }
    let button = |ui: &mut egui::Ui, icon, id| bar_button(ui, icons::button(ui, icon), i18n.t(id), None);
    if button(ui, Line::RotateCcw, "edit-patch-ccw") {
        *area = patch::quarter(area, true);
    }
    if button(ui, Line::RotateCw, "edit-patch-cw") {
        *area = patch::quarter(area, false);
    }
    for (horizontal, icon, id) in [(true, Line::FlipH, "edit-patch-flip-h"), (false, Line::FlipV, "edit-patch-flip-v")] {
        if button(ui, icon, id) {
            (*area, *mirrored) = patch::flipped(area, *mirrored, horizontal, angle);
        }
    }
    ui.separator();
    // Whole pixels, as Resize has them; the box keeps its fractions until a side is typed.
    ui.label(i18n.t("edit-patch-size"));
    let max = crate::edit::resize::MAX_SIDE as f32;
    for width in [true, false] {
        let mut side = if width { area.w } else { area.h }.round();
        if ui.add(egui::DragValue::new(&mut side).range(1.0..=max).suffix(" px")).changed() {
            *area = patch::sized(area, side, width, keep);
        }
        if width {
            ui.label("×");
        }
    }
    ui.checkbox(&mut w.keep_patch, i18n.t("resize-keep")).on_hover_text(i18n.t("edit-patch-keep-hint"));
    ui.separator();
    if button(ui, Line::ToFront, "edit-patch-front") {
        w.selected = Some(patch::to_front(&mut w.doc.objects, i));
    }
    if button(ui, Line::ToBack, "edit-patch-back") {
        w.selected = Some(patch::to_back(&mut w.doc.objects, i));
    }
}
