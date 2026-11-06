//! The editor's canvas: the image turned by the GPU, the cover boxes, the texture of the other
//! objects, and over them the crop (Rotate and crop) or the selection; the mouse and the wheel on
//! them. Rotate and crop's part moved from `rotate_page.rs` on 2026-11-05.

use eframe::egui::{self, Color32, CursorIcon, Pos2, Rect, TextureHandle, TextureId, Vec2};

use super::{Drawn, Follow, GRAB, Grab, Hold, Tool, Window};
use crate::edit::doc::{Obj, Pt, Shape};
use crate::edit::geom::{self, Kind};
use crate::edit::render;
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

    /// A point of the document (before the free angle) on the screen, and back.
    fn of(&self, p: Pt, angle: f32) -> Pos2 {
        let q = geom::turned(p, angle);
        self.screen(egui::vec2(q.x, q.y))
    }

    fn doc(&self, s: Pos2, angle: f32) -> Pt {
        let v = self.image(s);
        geom::turned(Pt::new(v.x, v.y), -angle)
    }
}

/// The image, the objects and the crop or the selection, and the mouse on them.
pub(super) fn canvas(ui: &mut egui::Ui, w: &mut Window, tex: &TextureHandle, size: Vec2) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let (iw, ih) = w.size();
    let crop_tool = w.tool == Tool::Crop;
    let (at, k) = w.view.unwrap_or_else(|| {
        let c = w.doc.crop;
        if crop_tool {
            // Everything in view: the rotated image and the crop, if it goes further.
            let (bw, bh) = rotate::bounds(iw, ih, w.doc.angle);
            let (vw, vh) = (bw.max(2.0 * c.cx.abs() + c.w), bh.max(2.0 * c.cy.abs() + c.h));
            (Vec2::ZERO, (rect.width() / vw).min(rect.height() / vh) * 0.92)
        } else {
            // The result: what is inside the crop.
            (egui::vec2(c.cx, c.cy), (rect.width() / c.w).min(rect.height() / c.h) * 0.95)
        }
    });
    let view = View { centre: rect.center(), at, k };
    if crop_tool {
        mouse(ui, w, &response, &view, rect);
    } else if w.tool == Tool::Marquee {
        marquee_mouse(ui, w, &response, &view, rect);
    } else {
        objects_mouse(ui, w, &response, &view, rect);
    }
    // Drawn with the view after the mouse moved it.
    let (at, k) = w.view.unwrap_or((at, k));
    w.shown = (at, k);
    paint(ui, w, tex, &View { centre: rect.center(), at, k }, rect);
}

fn paint(ui: &egui::Ui, w: &mut Window, tex: &TextureHandle, view: &View, rect: Rect) {
    let painter = ui.painter_at(rect);
    let stage = crate::theme::stage(ui.visuals());
    painter.rect_filled(rect, 0.0, stage);
    let cr = view.crop(w.doc.crop);
    if w.transparent {
        checker(&painter, cr.intersect(rect));
    } else {
        painter.rect_filled(cr, 0.0, w.color);
    }
    // The four corners of the image, rotated clockwise around its centre; the GPU does the rest.
    let (iw, ih) = w.size();
    let (s, c) = (w.doc.angle as f64).to_radians().sin_cos();
    let (s, c) = (s as f32, c as f32);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        let (qx, qy) = ((x - 0.5) * iw, (y - 0.5) * ih);
        let pos = view.screen(egui::vec2(qx * c - qy * s, qx * s + qy * c));
        mesh.vertices.push(egui::epaint::Vertex { pos, uv: w.doc.turn.source_uv(egui::pos2(x, y)), color: Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    // Every object in its order, the cover boxes among them (`edit::render`).
    if let Some(id) = objects_texture(ui, w, view, rect) {
        painter.image(id, rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
    if w.tool == Tool::Crop {
        crop_overlay(&painter, w, rect, cr);
    } else {
        // Only the result is seen: outside the crop the background.
        for r in outside(rect, cr) {
            painter.rect_filled(r, 0.0, stage);
        }
        if let Some(o) = w.selected.and_then(|i| w.doc.objects.get(i)) {
            selection(&painter, o, view, w.doc.angle);
        }
        if let Some(a) = w.marquee {
            ants(&painter, a, view, w.doc.angle);
        }
    }
}

/// The four bands of `rect` around `cr`.
fn outside(rect: Rect, cr: Rect) -> [Rect; 4] {
    [
        Rect::from_min_max(rect.min, egui::pos2(rect.max.x, cr.min.y)),
        Rect::from_min_max(egui::pos2(rect.min.x, cr.max.y), rect.max),
        Rect::from_min_max(egui::pos2(rect.min.x, cr.min.y), egui::pos2(cr.min.x, cr.max.y)),
        Rect::from_min_max(egui::pos2(cr.max.x, cr.min.y), egui::pos2(rect.max.x, cr.max.y)),
    ]
}

/// Rotate and crop: outside the crop darkened, the thirds, the Straighten line, the handles.
fn crop_overlay(painter: &egui::Painter, w: &Window, rect: Rect, cr: Rect) {
    // Outside the crop darkened, as in the photo editors.
    let shade = Color32::from_black_alpha(150);
    for r in outside(rect, cr) {
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
        handle(painter, p);
    }
}

fn handle(painter: &egui::Painter, p: Pos2) {
    let r = Rect::from_center_size(p, egui::vec2(9.0, 9.0));
    painter.rect_filled(r, 1.0, Color32::WHITE);
    painter.rect_stroke(r, 1.0, egui::Stroke::new(1.0, Color32::from_gray(40)), egui::StrokeKind::Outside);
}

/// The selected object outlined, with its handles.
fn selection(painter: &egui::Painter, o: &Obj, view: &View, angle: f32) {
    let outline: Vec<Pos2> = match &o.shape {
        Shape::Line(a, b, c) | Shape::Arrow(a, b, c) => geom::curve(*a, *b, *c).into_iter().map(|p| view.of(p, angle)).collect(),
        Shape::Text { path: Some((a, b, c)), .. } => geom::curve(*a, *b, *c).into_iter().map(|p| view.of(p, angle)).collect(),
        Shape::Bubble { area, .. } => geom::handles(&Obj { shape: Shape::Rect(*area), style: o.style }).into_iter().map(|p| view.of(p, angle)).collect(),
        Shape::Text { at, text, size, rot, .. } => {
            let area = geom::text_area(*at, text, *size, *rot);
            let box_ = crate::edit::doc::Obj { shape: Shape::Rect(area), style: o.style };
            geom::handles(&box_).into_iter().map(|p| view.of(p, angle)).collect()
        }
        _ => geom::handles(o).into_iter().map(|p| view.of(p, angle)).collect(),
    };
    let closed = outline.len() > 2;
    for stroke in [egui::Stroke::new(3.0, Color32::from_black_alpha(160)), egui::Stroke::new(1.5, Color32::from_rgb(0x4F, 0xC3, 0xF7))] {
        if closed {
            painter.add(egui::Shape::closed_line(outline.clone(), stroke));
        } else {
            painter.add(egui::Shape::line(outline.clone(), stroke));
        }
    }
    for p in geom::handles(o) {
        handle(painter, view.of(p, angle));
    }
}

/// The objects, drawn by `edit::render` for this view at the screen's own
/// pixels; drawn again only when the document or the view changed.
fn objects_texture(ui: &egui::Ui, w: &mut Window, view: &View, rect: Rect) -> Option<TextureId> {
    if w.doc.objects.is_empty() {
        return None;
    }
    let ppp = ui.ctx().pixels_per_point();
    let key = Drawn { doc: w.doc.clone(), made: w.filters_made, at: view.at, k: view.k, rect, ppp };
    if let Some((drawn, tex)) = &w.objects
        && *drawn == key
    {
        return Some(tex.id());
    }
    let (iw, ih) = w.size();
    let (left, top) = rotate::corner(iw, ih, w.doc.crop);
    let p0 = view.image(rect.min);
    let part = render::View { x: p0.x - left, y: p0.y - top, w: (rect.width() * ppp).round().max(1.0) as u32, h: (rect.height() * ppp).round().max(1.0) as u32, scale: view.k * ppp };
    let pixmap = render::render(&w.doc, w.upright(), &part, &crate::decode::usvg_options())?;
    let img = egui::ColorImage::from_rgba_premultiplied([pixmap.width() as usize, pixmap.height() as usize], pixmap.data());
    match &mut w.objects {
        Some((drawn, tex)) => {
            tex.set(img, egui::TextureOptions::LINEAR);
            *drawn = key;
        }
        None => w.objects = Some((key, ui.ctx().load_texture("edit-objects", img, egui::TextureOptions::LINEAR))),
    }
    w.objects.as_ref().map(|(_, t)| t.id())
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
    let cr = view.crop(w.doc.crop);
    for (sx, sy) in sides(w.ratio().is_some()) {
        let h = cr.center() + egui::vec2(sx as f32 * cr.width(), sy as f32 * cr.height()) / 2.0;
        if h.distance(p) <= GRAB {
            return Grab::Side(sx, sy);
        }
    }
    if cr.contains(p) { Grab::Move } else { Grab::Pan }
}

/// Rotate and crop: dragging, the wheel and the double click on the canvas.
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
        w.drag = Some((grab_at(w, view, p), w.doc.crop, (view.at, view.k), p));
    }
    if response.drag_stopped() {
        // The line drawn becomes level (or upright), and Straighten turns itself off.
        if let (Some((Grab::Line, .., from)), Some(to)) = (w.drag, w.line_end.take()) {
            w.doc.angle = rotate::straightened(w.doc.angle, to.x - from.x, to.y - from.y);
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
                w.doc.crop = Crop { cx: start.cx + moved.x, cy: start.cy + moved.y, ..start };
                w.follow = Follow::Manual;
            }
            Grab::Side(sx, sy) => {
                w.doc.crop = resized(start, sx, sy, view.image(p), w.ratio());
                w.follow = Follow::Manual;
            }
        }
    }
    if response.hovered() {
        wheel(ui, w, view, rect, true);
    }
}

/// The other tools: Select picks, moves and resizes objects, the drawing tools draw them, a text
/// is placed with a click; the right or middle button moves the view.
fn objects_mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View, rect: Rect) {
    if w.picking {
        return pick_mouse(ui, w, response, view);
    }
    let angle = w.doc.angle;
    // Within reach of the pointer, in image pixels.
    let tol = GRAB / view.k;
    if let Some(s) = response.hover_pos() {
        let p = view.doc(s, angle);
        let on_handle = handle_at(w, view, s);
        let icon = match (&w.hold, w.tool) {
            (Some(Hold::Pan { .. }), _) => CursorIcon::Grabbing,
            (Some(Hold::Move { .. }), _) => CursorIcon::Move,
            (Some(Hold::Handle { index, handle, .. }), _) => resize_cursor(w, view, *index, *handle),
            (Some(_), _) => CursorIcon::Crosshair,
            // A corner or an end of the selected object, with any tool: it resizes.
            (None, _) if let Some((i, h)) = on_handle => resize_cursor(w, view, i, h),
            (None, Tool::Draw(Kind::Text)) => CursorIcon::Text,
            (None, Tool::Draw(_)) => CursorIcon::Crosshair,
            (None, _) if geom::under(&w.doc.objects, p, tol).is_some() => CursorIcon::Move,
            (None, _) => CursorIcon::Default,
        };
        ui.ctx().set_cursor_icon(icon);
    }
    if response.double_clicked() {
        double_clicked(w, response.interact_pointer_pos().map(|s| view.doc(s, angle)), tol);
    }
    let pan = response.dragged_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Middle);
    if response.drag_started()
        && let Some(s) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
    {
        let p = view.doc(s, angle);
        let on_handle = handle_at(w, view, s);
        w.hold = Some(match w.tool {
            _ if pan => Hold::Pan { at: view.at, k: view.k, from: s },
            // Resizing the selected object needs no change of tool.
            Tool::Draw(_) if let Some((index, handle)) = on_handle => Hold::Handle { index, handle, start: w.doc.objects[index].clone() },
            // Placed with a click: a drag moves the view.
            Tool::Draw(Kind::Text | Kind::Step | Kind::Stamp) => Hold::Pan { at: view.at, k: view.k, from: s },
            Tool::Draw(kind) => Hold::Draw { kind, from: p, index: None },
            _ => picked(w, view, s, p, tol),
        });
    }
    if let (Some(mut hold), Some(s)) = (w.hold.take(), response.interact_pointer_pos()) {
        let p = view.doc(s, angle);
        let square = ui.input(|i| i.modifiers.shift);
        match &mut hold {
            Hold::Move { index, start, from } => w.doc.objects[*index] = geom::moved(start, Pt::new(p.x - from.x, p.y - from.y)),
            Hold::Handle { index, handle, start } => w.doc.objects[*index] = geom::with_handle(start, *handle, p),
            Hold::Pan { at, k, from } => w.view = Some((*at - (s - *from) / *k, *k)),
            Hold::Marquee { .. } => {}
            // By hand: a point more where the pointer went (one every point and a half of the screen).
            Hold::Draw { kind: Kind::Pen, index: Some(i), .. } => geom::pen_to(&mut w.doc.objects[*i], p, 1.5 / view.k),
            Hold::Draw { kind, from, index } => match (geom::drawn(*kind, *from, p, angle, w.style(*kind), square), *index) {
                (Some(o), Some(i)) => w.doc.objects[i] = o,
                (Some(o), None) => {
                    w.doc.objects.push(o);
                    *index = Some(w.doc.objects.len() - 1);
                }
                (None, Some(i)) => {
                    w.doc.objects.remove(i);
                    *index = None;
                }
                (None, None) => {}
            },
        }
        w.hold = Some(hold);
    }
    if response.drag_stopped() {
        if let Some(Hold::Draw { kind, index: Some(i), .. }) = w.hold {
            w.selected = Some(i);
            // A bubble just drawn takes the keys, for its text.
            w.focus_text = kind == Kind::Bubble;
        }
        w.hold = None;
    }
    if response.clicked()
        && let Some(s) = response.interact_pointer_pos()
    {
        clicked(w, view.doc(s, angle), tol);
    }
    if response.hovered() {
        wheel(ui, w, view, rect, false);
    }
}

/// A double click on a text or a bubble: its letters to change, in the bar's field; elsewhere
/// everything in view (but with Text, whose clicks place texts).
fn double_clicked(w: &mut Window, p: Option<Pt>, tol: f32) {
    let text = p.and_then(|p| geom::under(&w.doc.objects, p, tol)).filter(|&i| matches!(Kind::of(&w.doc.objects[i]), Kind::Text | Kind::Bubble));
    match text {
        Some(i) => {
            w.selected = Some(i);
            w.focus_text = true;
        }
        None if w.tool != Tool::Draw(Kind::Text) => w.view = None,
        None => {}
    }
}

/// A drag started with Select (or a drawing tool on nothing to draw): a handle of the selected
/// object, an object, or the view.
fn picked(w: &mut Window, view: &View, s: Pos2, p: Pt, tol: f32) -> Hold {
    if let Some((i, h)) = handle_at(w, view, s) {
        return Hold::Handle { index: i, handle: h, start: w.doc.objects[i].clone() };
    }
    match geom::under(&w.doc.objects, p, tol) {
        Some(i) => {
            w.selected = Some(i);
            Hold::Move { index: i, start: w.doc.objects[i].clone(), from: p }
        }
        None => {
            w.selected = None;
            Hold::Pan { at: view.at, k: view.k, from: s }
        }
    }
}

/// The selection's outline: black dashes on white, seen on any photo.
fn ants(painter: &egui::Painter, a: crate::edit::doc::Area, view: &View, angle: f32) {
    let o = Obj { shape: Shape::Rect(a), style: crate::edit::doc::Style::default() };
    let mut points: Vec<Pos2> = geom::handles(&o).into_iter().map(|p| view.of(p, angle)).collect();
    points.push(points[0]);
    painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(1.5, Color32::WHITE)));
    painter.extend(egui::Shape::dashed_line(&points, egui::Stroke::new(1.5, Color32::BLACK), 6.0, 4.0));
}

/// Selection: a drag chooses a part of the image (upright on the screen), a click lets it go;
/// the right or middle button moves the view.
fn marquee_mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View, rect: Rect) {
    if response.hovered() {
        ui.ctx().set_cursor_icon(if matches!(w.hold, Some(Hold::Pan { .. })) { CursorIcon::Grabbing } else { CursorIcon::Crosshair });
    }
    let pan = response.dragged_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Middle);
    if response.drag_started()
        && let Some(s) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
    {
        w.hold = Some(if pan { Hold::Pan { at: view.at, k: view.k, from: s } } else { Hold::Marquee { from: view.doc(s, w.doc.angle) } });
    }
    if let Some(s) = response.interact_pointer_pos() {
        match &w.hold {
            Some(Hold::Marquee { from }) => {
                let square = ui.input(|i| i.modifiers.shift);
                let drawn = geom::drawn(Kind::Rect, *from, view.doc(s, w.doc.angle), w.doc.angle, crate::edit::doc::Style::default(), square);
                w.marquee = drawn.and_then(|o| if let Shape::Rect(a) = o.shape { Some(a) } else { None });
            }
            Some(Hold::Pan { at, k, from }) => w.view = Some((*at - (s - *from) / *k, *k)),
            _ => {}
        }
    }
    if response.drag_stopped() {
        w.hold = None;
    }
    if response.clicked() {
        w.marquee = None;
    }
    if response.hovered() {
        wheel(ui, w, view, rect, false);
    }
}

/// The eyedropper: a click takes the colour of the image there, nothing else happens.
fn pick_mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View) {
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    }
    if response.clicked()
        && let Some(s) = response.interact_pointer_pos()
    {
        w.picked = colour_at(w, view, s).or(w.picked);
        w.picking = false;
    }
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

/// The selected object and its corner or end within reach of `s`, if any.
fn handle_at(w: &Window, view: &View, s: Pos2) -> Option<(usize, usize)> {
    let i = w.selected?;
    let h = geom::handles(w.doc.objects.get(i)?).iter().position(|h| view.of(*h, w.doc.angle).distance(s) <= GRAB)?;
    Some((i, h))
}

/// The resize pointer for a corner or end, slanted as it lies from the middle of the object.
fn resize_cursor(w: &Window, view: &View, index: usize, handle: usize) -> CursorIcon {
    let Some(o) = w.doc.objects.get(index) else { return CursorIcon::Crosshair };
    let points: Vec<Pos2> = geom::handles(o).into_iter().map(|p| view.of(p, w.doc.angle)).collect();
    let Some(&at) = points.get(handle) else { return CursorIcon::Crosshair };
    let centre = points.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / points.len() as f32;
    let d = at.to_vec2() - centre;
    match () {
        _ if d.x.abs() < d.y.abs() * 0.4 => CursorIcon::ResizeVertical,
        _ if d.y.abs() < d.x.abs() * 0.4 => CursorIcon::ResizeHorizontal,
        _ if d.x * d.y > 0.0 => CursorIcon::ResizeNwSe,
        _ => CursorIcon::ResizeNeSw,
    }
}

/// A click: the Text tool places a text (or picks one to change), the others pick what is under it.
fn clicked(w: &mut Window, p: Pt, tol: f32) {
    let under = geom::under(&w.doc.objects, p, tol);
    // With a drawing tool, an object clicked is selected and its tool chosen: the bar shows its
    // style (user, 2026-11-05). Select stays Select, to go on moving objects. Not a filter or a
    // patch, which have no tool (a filter on the whole image took every click).
    let tool_of = under.map(|i| Tool::Draw(Kind::of(&w.doc.objects[i]))).filter(|t| super::tools::TOOLS.iter().any(|(tool, ..)| tool == t));
    if let (Some(i), Some(tool), Tool::Draw(_)) = (under, tool_of, w.tool) {
        let kind = Kind::of(&w.doc.objects[i]);
        w.tool = tool;
        w.selected = Some(i);
        w.focus_text = matches!(kind, Kind::Text | Kind::Bubble);
        return;
    }
    if w.tool == Tool::Draw(Kind::Text) {
        if let Some(mut o) = geom::drawn(Kind::Text, p, p, w.doc.angle, w.style(Kind::Text), false) {
            // In the font chosen last.
            if let Shape::Text { font, .. } = &mut o.shape {
                font.clone_from(&w.text_font);
            }
            w.doc.objects.push(o);
            w.selected = Some(w.doc.objects.len() - 1);
        }
        w.focus_text = true;
    } else if w.tool == Tool::Draw(Kind::Step) {
        w.doc.objects.push(geom::step(p, w.doc.next_step(), w.doc.angle, w.style(Kind::Step)));
        w.selected = Some(w.doc.objects.len() - 1);
    } else if w.tool == Tool::Draw(Kind::Stamp) {
        w.doc.objects.push(geom::stamp(p, w.doc.angle, w.style(Kind::Stamp)));
        w.selected = Some(w.doc.objects.len() - 1);
    } else {
        w.selected = under;
    }
}

/// Shift + wheel turns by 0.1° a notch (with `turn`, in Rotate and crop); the wheel (or Ctrl +
/// wheel, or the pinch) zooms around the pointer.
fn wheel(ui: &egui::Ui, w: &mut Window, view: &View, rect: Rect, turn: bool) {
    let (notches, zoom, mods, pointer) = ui.input(|i| {
        // Notches, not distance: one notch is 0.1°. Shift turns the wheel sideways on some systems.
        let n: f32 = i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some((delta.y + delta.x).signum()) } else { None }).sum();
        (n, i.zoom_delta(), i.modifiers, i.pointer.hover_pos())
    });
    if mods.shift && turn {
        if notches != 0.0 {
            w.doc.angle = ((w.doc.angle - notches * 0.1) * 10.0).round() / 10.0;
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
