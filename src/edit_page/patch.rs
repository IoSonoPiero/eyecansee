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

/// The sliders of a piece (opacity, rotation), shorter than the others' 110 points: with its
/// controls the bar stays on one row in a 1200 px window; the field beside a slider takes a value
/// typed.
pub(super) const SLIDER: f32 = 60.0;

/// The controls of the selected piece `i`, after its opacity.
pub(super) fn bar(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let (angle, keep) = (w.doc.angle, w.keep_patch);
    let Some(Shape::Patch { area, mirrored, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) else { return };
    ui.separator();
    let mut shown = patch::shown(area.rot, angle);
    ui.spacing_mut().slider_width = SLIDER;
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
    // Whole pixels, as Resize has them; the box keeps its fractions until a side is typed. No
    // label, so the bar keeps one row at 1200 px in Italian: the names on hover.
    let max = crate::edit::resize::MAX_SIDE as f32;
    for width in [true, false] {
        let mut side = if width { area.w } else { area.h }.round();
        let id = if width { "edit-patch-width" } else { "edit-patch-height" };
        if ui.add(egui::DragValue::new(&mut side).range(1.0..=max).suffix(" px")).on_hover_text(i18n.t(id)).changed() {
            *area = patch::sized(area, side, width, keep);
        }
        if width {
            ui.label("×");
        }
    }
    ui.checkbox(&mut w.keep_patch, i18n.t("resize-keep")).on_hover_text(i18n.t("edit-patch-keep-hint"));
    // Front, back and the bin after them go to the next row together: none alone there, and
    // front and back not apart (1024 px).
    let gap = ui.spacing().item_spacing.x;
    if ui.available_size_before_wrap().x < 3.0 * (icons::BUTTON.x + gap) + 2.0 * (2.0 * gap + 1.0) {
        ui.end_row();
    } else {
        ui.separator();
    }
    if button(ui, Line::ToFront, "edit-patch-front") {
        w.selected = Some(patch::to_front(&mut w.doc.objects, i));
    }
    if button(ui, Line::ToBack, "edit-patch-back") {
        w.selected = Some(patch::to_back(&mut w.doc.objects, i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bar of a selected piece (four-figure sizes, an angle of three figures and a sign) in
    /// every language, on one row in a 1200 px window: with the sliders of the other bars its bin
    /// went alone to a second row (2026-11-14).
    #[test]
    fn one_row_at_1200() {
        use super::super::{Tool, Window};
        use crate::edit::doc::{Area, Obj, Pixels, Pt, Style};
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        for (lang, _) in crate::i18n::languages() {
            let i18n = I18n::new(lang);
            let mut w = Window::new("p/a.png".into(), Default::default(), Tool::Select);
            let area = Area { centre: Pt::new(0.0, 0.0), w: 1234.0, h: 5678.0, rot: -135.0 };
            w.doc.objects.push(Obj { shape: Shape::Patch { area, pixels: Pixels::new(image::RgbaImage::new(2, 2)), mirrored: true }, style: Style::default() });
            w.selected = Some(0);
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))), ..Default::default() };
            let mut height = 0.0;
            // The fonts are ready from the second pass.
            for _ in 0..3 {
                ctx.run_ui(input.clone(), |ui| {
                    egui::Panel::top("edit-props").show(ui, |ui| {
                        let r = ui.horizontal_wrapped(|ui| {
                            ui.set_min_height(icons::BUTTON.y);
                            super::super::style_bar::style_bar(ui, &mut w, &i18n)
                        });
                        height = r.response.rect.height();
                    });
                })
                .textures_delta
                .clear();
            }
            assert!(height < 1.5 * icons::BUTTON.y, "{lang}: {height} points high");
        }
    }
}
