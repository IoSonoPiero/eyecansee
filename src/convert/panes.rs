//! The two images of Save as…: the original and, decoded back, the file it will write; fitted, or
//! both at 100% around the same point, moved together by dragging either.

use eframe::egui::{self, TextureHandle};

use super::{GAP, Window};
use crate::i18n::I18n;
use crate::paint::{paint_turned, paint_zoomed};
use crate::transform::Turn;

/// The original and the new image side by side, each under its title.
pub(super) fn convert_images(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n, tex: &TextureHandle, half: f32, pane: egui::Vec2) {
    let t = |id| i18n.t(id);
    let result_tex = match &w.result {
        Some(Ok((_, Some(tex)))) => Some(tex),
        _ => None,
    };
    // Titles in rows of the same height (the spinner is taller than the text), so the
    // two images start at the same height.
    let title = |ui: &mut egui::Ui, text: String, busy: bool| {
        ui.allocate_ui_with_layout(egui::vec2(half, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(egui::vec2(half, 22.0));
            ui.strong(text);
            if busy {
                ui.spinner();
                ui.weak(t("convert-working"));
            }
        });
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.vertical(|ui| {
            title(ui, t("convert-original"), false);
            let r = preview(ui, pane, Some(tex), None, w.actual, &mut w.pan);
            drag(&r, w.actual, &mut w.pan);
        });
        ui.vertical(|ui| {
            title(ui, t("convert-result"), w.busy);
            let note = match &w.result {
                _ if w.busy && result_tex.is_none() => Some(t("convert-working")),
                Some(Ok((_, None))) => Some(t("convert-no-preview")),
                _ => None,
            };
            let r = preview(ui, pane, result_tex, note.as_deref(), w.actual, &mut w.pan);
            drag(&r, w.actual, &mut w.pan);
        });
    });
}

/// The image fitted into `size` (never enlarged), or at 100% around `pan`; grey when none.
/// `note` is written in the middle when there is no image (converting, no preview for the format).
fn preview(ui: &mut egui::Ui, size: egui::Vec2, tex: Option<&TextureHandle>, note: Option<&str>, actual: bool, pan: &mut egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let painter = ui.painter_at(rect);
    // Grey ground and a border: on the window's own colour the two images ran into each other.
    let ground = if ui.visuals().dark_mode { egui::Color32::from_gray(24) } else { egui::Color32::from_gray(200) };
    painter.rect_filled(rect, 2.0, ground);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
    let Some(tex) = tex else {
        if let Some(note) = note {
            let font = egui::TextStyle::Body.resolve(ui.style());
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, note, font, ui.visuals().text_color());
        }
        return response;
    };
    let ppp = ui.ctx().pixels_per_point();
    let t = tex.size_vec2();
    if actual {
        paint_zoomed(&painter, rect, tex, 1.0, pan, Turn::default());
    } else {
        let scale = (rect.width() / t.x).min(rect.height() / t.y).min(1.0 / ppp);
        let dest = egui::Rect::from_center_size(rect.center(), t * scale);
        paint_turned(&painter, tex, dest, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Turn::default());
    }
    response
}

/// Dragging either preview moves both, at 100%.
fn drag(response: &egui::Response, actual: bool, pan: &mut egui::Vec2) {
    if actual && response.dragged() {
        *pan -= response.drag_delta() * response.ctx.pixels_per_point();
    }
}
