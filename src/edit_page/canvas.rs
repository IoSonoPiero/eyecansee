//! Rotate and crop, the canvas: the image rotated, the crop over it, the mouse and the wheel on
//! them. Moved from `rotate_page.rs` on 2026-11-05.

use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, TextureHandle, Vec2};

use super::{Follow, GRAB, Grab, Window};
use crate::rotate::{self, Crop};

/// From image pixels (rotated, from the centre of the image) to the screen and back.
struct View {
    centre: Pos2,
    at: Vec2,
    k: f32,
}

impl View {
    fn screen(&self, p: Vec2) -> Pos2 {
        self.centre + (p - self.at) * self.k
    }

    fn image(&self, s: Pos2) -> Vec2 {
        (s - self.centre) / self.k + self.at
    }

    fn crop(&self, c: Crop) -> Rect {
        Rect::from_center_size(self.screen(egui::vec2(c.cx, c.cy)), egui::vec2(c.w, c.h) * self.k)
    }
}

/// The image rotated, the crop over it, and the mouse on them.
pub(super) fn canvas(ui: &mut egui::Ui, w: &mut Window, tex: &TextureHandle, size: Vec2) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let (iw, ih) = w.size();
    let (at, k) = w.view.unwrap_or_else(|| {
        // Everything in view: the rotated image and the crop, if it goes further.
        let (bw, bh) = rotate::bounds(iw, ih, w.angle);
        let c = w.crop;
        let (vw, vh) = (bw.max(2.0 * c.cx.abs() + c.w), bh.max(2.0 * c.cy.abs() + c.h));
        (Vec2::ZERO, (rect.width() / vw).min(rect.height() / vh) * 0.92)
    });
    let view = View { centre: rect.center(), at, k };
    mouse(ui, w, &response, &view, rect);
    // Drawn with the view after the mouse moved it.
    let (at, k) = w.view.unwrap_or((at, k));
    paint(ui, w, tex, &View { centre: rect.center(), at, k }, rect);
}

fn paint(ui: &egui::Ui, w: &Window, tex: &TextureHandle, view: &View, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::from_gray(18));
    let cr = view.crop(w.crop);
    if w.transparent {
        checker(&painter, cr.intersect(rect));
    } else {
        painter.rect_filled(cr, 0.0, w.color);
    }
    // The four corners of the image, rotated clockwise around its centre; the GPU does the rest.
    let (iw, ih) = w.size();
    let (s, c) = (w.angle as f64).to_radians().sin_cos();
    let (s, c) = (s as f32, c as f32);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        let (qx, qy) = ((x - 0.5) * iw, (y - 0.5) * ih);
        let pos = view.screen(egui::vec2(qx * c - qy * s, qx * s + qy * c));
        mesh.vertices.push(egui::epaint::Vertex { pos, uv: w.turn.source_uv(egui::pos2(x, y)), color: Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    // Outside the crop darkened, as in the photo editors.
    let shade = Color32::from_black_alpha(150);
    for r in [
        Rect::from_min_max(rect.min, egui::pos2(rect.max.x, cr.min.y)),
        Rect::from_min_max(egui::pos2(rect.min.x, cr.max.y), rect.max),
        Rect::from_min_max(egui::pos2(rect.min.x, cr.min.y), egui::pos2(cr.min.x, cr.max.y)),
        Rect::from_min_max(egui::pos2(cr.max.x, cr.min.y), egui::pos2(rect.max.x, cr.max.y)),
    ] {
        painter.rect_filled(r, 0.0, shade);
    }
    // Thirds, to line up the horizon or the subject.
    let line = egui::Stroke::new(1.0, Color32::from_white_alpha(90));
    for i in 1..3 {
        let f = i as f32 / 3.0;
        painter.line_segment([egui::pos2(cr.min.x + cr.width() * f, cr.min.y), egui::pos2(cr.min.x + cr.width() * f, cr.max.y)], line);
        painter.line_segment([egui::pos2(cr.min.x, cr.min.y + cr.height() * f), egui::pos2(cr.max.x, cr.min.y + cr.height() * f)], line);
    }
    painter.rect_stroke(cr, 0.0, egui::Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Middle);
    if let (Some((Grab::Line, .., from)), Some(to)) = (w.drag, w.line_end) {
        painter.line_segment([from, to], egui::Stroke::new(3.0, Color32::from_black_alpha(160)));
        painter.line_segment([from, to], egui::Stroke::new(1.5, Color32::from_rgb(0xFF, 0xD5, 0x4F)));
    }
    for (sx, sy) in sides(w.ratio().is_some()) {
        let p = cr.center() + egui::vec2(sx as f32 * cr.width(), sy as f32 * cr.height()) / 2.0;
        let handle = Rect::from_center_size(p, egui::vec2(9.0, 9.0));
        painter.rect_filled(handle, 1.0, Color32::WHITE);
        painter.rect_stroke(handle, 1.0, egui::Stroke::new(1.0, Color32::from_gray(40)), egui::StrokeKind::Outside);
    }
}

/// Grey squares where the result will be transparent.
fn checker(painter: &egui::Painter, area: Rect) {
    const SQ: f32 = 10.0;
    painter.rect_filled(area, 0.0, Color32::from_gray(190));
    let mut y = area.min.y;
    while y < area.max.y {
        let mut x = area.min.x + if ((y - area.min.y) / SQ) as i32 % 2 == 0 { 0.0 } else { SQ };
        while x < area.max.x {
            painter.rect_filled(Rect::from_min_max(egui::pos2(x, y), egui::pos2((x + SQ).min(area.max.x), (y + SQ).min(area.max.y))), 0.0, Color32::from_gray(140));
            x += 2.0 * SQ;
        }
        y += SQ;
    }
}

/// The corners, and the middle of the sides when the proportions are free.
fn sides(fixed: bool) -> Vec<(i8, i8)> {
    let mut v = vec![(-1, -1), (1, -1), (1, 1), (-1, 1)];
    if !fixed {
        v.extend([(0, -1), (1, 0), (0, 1), (-1, 0)]);
    }
    v
}

/// What is under the pointer at `p`.
fn grab_at(w: &Window, view: &View, p: Pos2) -> Grab {
    if w.straighten {
        return Grab::Line;
    }
    let cr = view.crop(w.crop);
    for (sx, sy) in sides(w.ratio().is_some()) {
        let h = cr.center() + egui::vec2(sx as f32 * cr.width(), sy as f32 * cr.height()) / 2.0;
        if h.distance(p) <= GRAB {
            return Grab::Side(sx, sy);
        }
    }
    if cr.contains(p) { Grab::Move } else { Grab::Pan }
}

/// Dragging, the wheel and the double click on the canvas.
fn mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View, rect: Rect) {
    if let Some(p) = response.hover_pos() {
        let icon = match w.drag.map_or_else(|| grab_at(w, view, p), |d| d.0) {
            Grab::Side(sx, sy) if sx == sy => CursorIcon::ResizeNwSe,
            Grab::Side(sx, sy) if sx == -sy => CursorIcon::ResizeNeSw,
            Grab::Side(0, _) => CursorIcon::ResizeVertical,
            Grab::Side(..) => CursorIcon::ResizeHorizontal,
            Grab::Move => CursorIcon::Move,
            Grab::Line => CursorIcon::Crosshair,
            Grab::Pan if w.drag.is_some() => CursorIcon::Grabbing,
            Grab::Pan => CursorIcon::Grab,
        };
        ui.ctx().set_cursor_icon(icon);
    }
    if response.double_clicked() {
        w.view = None;
    }
    // Where the button went down: egui starts a drag only once the pointer has moved a few
    // pixels, and taken there a corner pulled inwards was already "inside" and moved the crop.
    if response.drag_started()
        && let Some(p) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
    {
        w.drag = Some((grab_at(w, view, p), w.crop, (view.at, view.k), p));
    }
    if response.drag_stopped() {
        // The line drawn becomes level (or upright), and Straighten turns itself off.
        if let (Some((Grab::Line, .., from)), Some(to)) = (w.drag, w.line_end.take()) {
            w.angle = rotate::straightened(w.angle, to.x - from.x, to.y - from.y);
            w.straighten = false;
            w.refit();
        }
        w.drag = None;
    }
    if let (Some((grab, start, (at, k), from)), Some(p)) = (w.drag, response.interact_pointer_pos()) {
        let moved = (p - from) / k;
        match grab {
            Grab::Pan => w.view = Some((at - moved, k)),
            Grab::Line => w.line_end = Some(p),
            Grab::Move => {
                w.crop = Crop { cx: start.cx + moved.x, cy: start.cy + moved.y, ..start };
                w.follow = Follow::Manual;
            }
            Grab::Side(sx, sy) => {
                w.crop = resized(start, sx, sy, view.image(p), w.ratio());
                w.follow = Follow::Manual;
            }
        }
    }
    if response.hovered() {
        wheel(ui, w, view, rect);
    }
}

/// Shift + wheel turns by 0.1° a notch; the wheel (or Ctrl + wheel, or the pinch) zooms around the pointer.
fn wheel(ui: &egui::Ui, w: &mut Window, view: &View, rect: Rect) {
    let (notches, zoom, mods, pointer) = ui.input(|i| {
        // Notches, not distance: one notch is 0.1°. Shift turns the wheel sideways on some systems.
        let n: f32 = i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some((delta.y + delta.x).signum()) } else { None }).sum();
        (n, i.zoom_delta(), i.modifiers, i.pointer.hover_pos())
    });
    if mods.shift {
        if notches != 0.0 {
            w.angle = ((w.angle - notches * 0.1) * 10.0).round() / 10.0;
            w.refit();
        }
        return;
    }
    // Ctrl + wheel already comes as `zoom`.
    let factor = zoom * if mods.command { 1.0 } else { 1.15f32.powf(notches) };
    if factor != 1.0
        && let Some(p) = pointer
    {
        // The point under the pointer stays under it.
        let fixed = view.image(p);
        let k = (view.k * factor).clamp(0.01, 64.0);
        w.view = Some((fixed - (p - rect.center()) / k, k));
    }
}

/// `start` with the side or corner (`sx`, `sy`) moved to `p`, the opposite one staying where it
/// was; with `ratio`, the proportions kept (only corners then). At least a pixel.
fn resized(start: Crop, sx: i8, sy: i8, p: Vec2, ratio: Option<f32>) -> Crop {
    let (mut l, mut r) = (start.cx - start.w / 2.0, start.cx + start.w / 2.0);
    let (mut t, mut b) = (start.cy - start.h / 2.0, start.cy + start.h / 2.0);
    match sx {
        1 => r = p.x.max(l + 1.0),
        -1 => l = p.x.min(r - 1.0),
        _ => {}
    }
    match sy {
        1 => b = p.y.max(t + 1.0),
        -1 => t = p.y.min(b - 1.0),
        _ => {}
    }
    if let Some(ratio) = ratio {
        let (mut w, mut h) = (r - l, b - t);
        if w / h > ratio { h = w / ratio } else { w = h * ratio }
        if sx == 1 { r = l + w } else { l = r - w }
        if sy == 1 { b = t + h } else { t = b - h }
    }
    Crop { cx: (l + r) / 2.0, cy: (t + b) / 2.0, w: r - l, h: b - t }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize() {
        let start = Crop::centred(100.0, 50.0);
        // Right side out by 20: the left one stays.
        assert_eq!(resized(start, 1, 0, egui::vec2(70.0, 0.0), None), Crop { cx: 10.0, cy: 0.0, w: 120.0, h: 50.0 });
        // Top left corner with 1:1: square, the bottom right corner stays.
        let sq = resized(start, -1, -1, egui::vec2(-40.0, -40.0), Some(1.0));
        assert_eq!((sq.w, sq.h, sq.cx + sq.w / 2.0, sq.cy + sq.h / 2.0), (90.0, 90.0, 50.0, 25.0));
        // Past the opposite side: a pixel wide, not turned inside out.
        assert_eq!(resized(start, 1, 0, egui::vec2(-500.0, 0.0), None).w, 1.0);
    }
}
