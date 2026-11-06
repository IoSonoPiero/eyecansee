//! The editor's eyedropper on the canvas: the colour under the pointer, the box showing it, the
//! colour seen (the photo with the objects drawn). Moved from `canvas.rs` on 2026-11-06,
//! unchanged.

use eframe::egui::{self, Color32, CursorIcon, Pos2};

use super::Window;
use super::canvas::View;
use crate::edit::render;
use crate::rotate;

/// The eyedropper: a click takes the colour there, nothing else happens. On Windows the screen's
/// pixel (the objects drawn too), also where a drag started on the image is let go, anywhere on
/// the screen; elsewhere the image's.
pub(super) fn mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View) {
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        // The colour under the pointer, live: by the pointer, or (dragged out of the window,
        // where nothing can be drawn) in the bottom left corner of the canvas (user, 2026-11-06).
        let at = ui.input(|i| i.pointer.latest_pos()).filter(|p| response.rect.contains(*p));
        let under = crate::eyedropper::at_cursor().or_else(|| at.and_then(|s| colour_seen(w, view, s)));
        if let Some(c) = under {
            let place = at.map_or(response.rect.left_bottom() + egui::vec2(12.0, -96.0), |p| p + egui::vec2(18.0, 18.0));
            swatch(ui.ctx(), place, c);
        }
        if response.dragged() {
            ui.ctx().request_repaint();
        }
    }
    if (response.clicked() || response.drag_stopped())
        && let Some(s) = response.interact_pointer_pos()
    {
        w.picked = crate::eyedropper::at_cursor().or_else(|| colour_seen(w, view, s)).or(w.picked);
        w.picking = false;
        if let Some(c) = w.picked {
            w.note = Some((crate::eyedropper::codes(c).replace('\n', " · "), ui.input(|i| i.time) + 8.0));
        }
    }
}

/// The box by the eyedropper: the colour and its codes.
fn swatch(ctx: &egui::Context, at: Pos2, [r, g, b]: [u8; 3]) {
    egui::Area::new(egui::Id::unique("edit-pick-swatch")).order(egui::Order::Tooltip).fixed_pos(at).interactable(false).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(r, g, b));
                ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(120)), egui::StrokeKind::Inside);
                ui.label(egui::RichText::new(crate::eyedropper::codes([r, g, b])).monospace());
            });
        });
    });
}

/// The colour seen at `s`: the photo with the objects drawn over it (a pixel of them drawn there).
fn colour_seen(w: &Window, view: &View, s: Pos2) -> Option<[u8; 3]> {
    let photo = colour_at(w, view, s)?;
    if w.doc.objects.is_empty() {
        return Some(photo);
    }
    let (iw, ih) = w.size();
    let (left, top) = rotate::corner(iw, ih, w.doc.crop);
    let p = view.image(s);
    let one = render::View { x: p.x - left, y: p.y - top, w: 1, h: 1, scale: 1.0 };
    let Some(px) = render::render(&w.doc, w.upright(), &one, &crate::decode::usvg_options()).and_then(|m| m.pixel(0, 0)) else { return Some(photo) };
    let a = px.alpha() as f32 / 255.0;
    let mix = |src: u8, under: u8| (src as f32 + under as f32 * (1.0 - a)).round().min(255.0) as u8;
    Some([mix(px.red(), photo[0]), mix(px.green(), photo[1]), mix(px.blue(), photo[2])])
}

/// The colour of the image under `s` (the photo, without the objects); `None` outside it.
fn colour_at(w: &Window, view: &View, s: Pos2) -> Option<[u8; 3]> {
    use image::GenericImageView;
    let Some(Ok((original, ..))) = &w.original else { return None };
    let q = w.doc.source(view.doc(s, w.doc.angle));
    let img = &original.image;
    let (x, y) = (q.x + img.width() as f32 / 2.0, q.y + img.height() as f32 / 2.0);
    (x >= 0.0 && y >= 0.0 && x < img.width() as f32 && y < img.height() as f32).then(|| {
        let c = img.get_pixel(x as u32, y as u32);
        [c[0], c[1], c[2]]
    })
}

