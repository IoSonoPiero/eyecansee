//! Drawing an image turned and zoomed, shared by the viewer and the "Save in another format" page.

use eframe::egui::{self, TextureHandle};

use crate::mipmap::Picture;
use crate::transform::Turn;

/// `tex` drawn in `rect` at `scale` screen pixels per image pixel, around `pan` (the centre of the
/// view, in image pixels). `pan` is kept inside the image; a side smaller than the view stays centred.
/// `turn`: flipped and turned as in the viewer (sizes and `pan` are those of the turned image).
/// Returns where the image went and which part of it (0 to 1).
pub fn paint_zoomed(painter: &egui::Painter, rect: egui::Rect, tex: &Picture, scale: f32, pan: &mut egui::Vec2, turn: Turn) -> (egui::Rect, egui::Rect) {
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
    (dest, uv)
}

pub fn turned_size(tex: &TextureHandle, turn: Turn) -> egui::Vec2 {
    let t = tex.size_vec2();
    if turn.sideways() { egui::vec2(t.y, t.x) } else { t }
}

/// The part `uv` of the turned image (0 to 1 on each side) in `dest`: each corner takes the
/// texture point it comes from, so the GPU turns it.
pub fn paint_turned(painter: &egui::Painter, tex: &Picture, dest: egui::Rect, uv: egui::Rect, turn: Turn) {
    paint_tinted(painter, tex, dest, uv, turn, egui::Color32::WHITE);
}

/// As `paint_turned`, multiplied by `tint` (a picture fading). Smaller than its pixels: filtered
/// on the graphics card at the size on the screen (`smooth_gpu`), or, until the picture is there,
/// from its halved copy of that size (`mipmap`).
pub fn paint_tinted(painter: &egui::Painter, tex: &Picture, dest: egui::Rect, uv: egui::Rect, turn: Turn, tint: egui::Color32) {
    let ppp = painter.ctx().pixels_per_point();
    let scale = dest.width() * ppp / (uv.width() * turned_size(tex, turn).x);
    let corners = [(dest.left_top(), uv.left_top()), (dest.right_top(), uv.right_top()), (dest.right_bottom(), uv.right_bottom()), (dest.left_bottom(), uv.left_bottom())];
    let corners = corners.map(|(pos, shown)| (pos, turn.source_uv(shown)));
    if scale < 1.0
        && crate::mipmap::smooth()
        && let Some((id, corners)) = filtered(tex, scale, ppp, dest, corners, turn)
    {
        quad(painter, id, corners, tint);
        return;
    }
    quad(painter, tex.id_at(scale), corners, tint);
}

/// The corners of `dest` on the pixels of the screen, with the points of the picture filtered
/// at that size: the texture and its corners.
fn filtered(tex: &Picture, scale: f32, ppp: f32, dest: egui::Rect, corners: [(egui::Pos2, egui::Pos2); 4], turn: Turn) -> Option<(egui::TextureId, [(egui::Pos2, egui::Pos2); 4])> {
    let snap = |p: egui::Pos2| (p.to_vec2() * ppp).round().to_pos2() / ppp;
    let dest = egui::Rect::from_min_max(snap(dest.min), snap(dest.max));
    let px = [(dest.width() * ppp).round() as u32, (dest.height() * ppp).round() as u32];
    let out = if turn.sideways() { [px[1], px[0]] } else { px };
    let part = egui::Rect::from_points(&corners.map(|(_, s)| s));
    let (id, extent) = crate::smooth_gpu::resampled(tex, scale, part, out)?;
    let shown = [dest.left_top(), dest.right_top(), dest.right_bottom(), dest.left_bottom()];
    let mut i = 0;
    Some((
        id,
        corners.map(|(_, s)| {
            let at = ((s - part.min) / part.size() * extent).to_pos2();
            i += 1;
            (shown[i - 1], at)
        }),
    ))
}

/// A picture on the four corners (position, point of the texture), clockwise from the top left.
fn quad(painter: &egui::Painter, id: egui::TextureId, corners: [(egui::Pos2, egui::Pos2); 4], tint: egui::Color32) {
    let mut mesh = egui::Mesh::with_texture(id);
    for (pos, uv) in corners {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv, color: tint });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}
