//! Drawing an image turned and zoomed, shared by the viewer and the "Save in another format" page.

use eframe::egui::{self, TextureHandle};

use crate::transform::Turn;

/// `tex` drawn in `rect` at `scale` screen pixels per image pixel, around `pan` (the centre of the
/// view, in image pixels). `pan` is kept inside the image; a side smaller than the view stays centred.
/// `turn`: flipped and turned as in the viewer (sizes and `pan` are those of the turned image).
pub fn paint_zoomed(painter: &egui::Painter, rect: egui::Rect, tex: &TextureHandle, scale: f32, pan: &mut egui::Vec2, turn: Turn) {
    let ppp = painter.ctx().pixels_per_point();
    let t = turned_size(tex, turn);
    let half = rect.size() * ppp / scale / 2.0;
    let clamp = |c: f32, h: f32, full: f32| if full <= 2.0 * h { full / 2.0 } else { c.clamp(h, full - h) };
    *pan = egui::vec2(clamp(pan.x, half.x, t.x), clamp(pan.y, half.y, t.y));
    let min = (*pan - half).max(egui::Vec2::ZERO);
    let max = (*pan + half).min(t);
    let dest = egui::Rect::from_center_size(rect.center() + ((min + max) / 2.0 - *pan) * scale / ppp, (max - min) * scale / ppp);
    let uv = egui::Rect::from_min_max((min / t).to_pos2(), (max / t).to_pos2());
    paint_turned(painter, tex, dest, uv, turn);
}

pub fn turned_size(tex: &TextureHandle, turn: Turn) -> egui::Vec2 {
    let t = tex.size_vec2();
    if turn.sideways() { egui::vec2(t.y, t.x) } else { t }
}

/// The part `uv` of the turned image (0 to 1 on each side) in `dest`: each corner takes the
/// texture point it comes from, so the GPU turns it.
pub fn paint_turned(painter: &egui::Painter, tex: &TextureHandle, dest: egui::Rect, uv: egui::Rect, turn: Turn) {
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (pos, shown) in [(dest.left_top(), uv.left_top()), (dest.right_top(), uv.right_top()), (dest.right_bottom(), uv.right_bottom()), (dest.left_bottom(), uv.left_bottom())] {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv: turn.source_uv(shown), color: egui::Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}
