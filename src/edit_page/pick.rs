//! The editor's eyedropper on the canvas, as in the photo editors ("do what the
//! others do"): a box by the pointer with a magnifier (the pixels around it, the one taken
//! framed), the new colour next to the one in use, the codes; the colour of one pixel or the
//! average of 3 × 3 or 5 × 5; what is seen (the photo with the objects drawn); a click copies its
//! hex code; Alt+click with a drawing tool takes it without changing tool. Moved from `canvas.rs`
//! on 2026-11-06.

use eframe::egui::{self, Color32, CursorIcon, Pos2};

use super::canvas::View;
use super::{Tool, Window};
use crate::edit::doc::Pt;
use crate::edit::{geom, render};
use crate::rotate;

/// Pixels a side of the magnifier.
const GRID: usize = 11;

/// The eyedropper on the canvas: the box while the pointer moves, the colour on a click (or where
/// a drag started on the image is let go: on Windows anywhere on the screen).
pub(super) fn mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View) {
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        // By the pointer, kept inside the window; dragged out of it (where nothing can be drawn)
        // where it was last (it jumped to a corner of the canvas).
        let window = ui.ctx().content_rect();
        let pointer = ui.input(|i| i.pointer.latest_pos()).filter(|p| window.contains(*p));
        if let Some(p) = pointer {
            w.pick_box = Some(p);
        }
        let at = pointer.filter(|p| response.rect.contains(*p));
        // Over the image only, as on Linux; past it only while dragging, to take a colour from
        // elsewhere on the screen.
        let on_image = at.is_some_and(|s| photo_at(w, view.doc(s, w.doc.angle)).is_some());
        if let Some(grid) = (on_image || response.dragged()).then(|| around(w, view, at)).flatten() {
            let p = w.pick_box.unwrap_or(response.rect.left_bottom()) + egui::vec2(18.0, 18.0);
            let place = Pos2::new(p.x.min(window.right() - 260.0), p.y.min(window.bottom() - 110.0));
            swatch(ui.ctx(), place, &grid, average(&grid, w.pick_size), current(w));
        }
        if response.dragged() {
            ui.ctx().request_repaint();
        }
    }
    if (response.clicked() || response.drag_stopped())
        && let Some(s) = response.interact_pointer_pos()
    {
        if let Some(c) = around(w, view, Some(s)).and_then(|g| average(&g, w.pick_size)) {
            w.picked = Some(c);
            w.just_picked = Some(c);
            ui.ctx().copy_text(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
        }
        w.picking = false;
    }
}

/// The colour the style shown has now (of the object selected, or of the next one drawn).
fn current(w: &Window) -> Option<[u8; 3]> {
    match (w.selected.and_then(|i| w.doc.objects.get(i)), w.tool) {
        (Some(o), _) => Some(o.style.color),
        (None, Tool::Draw(kind)) => Some(w.style(kind).color),
        _ => None,
    }
}

/// The `GRID` × `GRID` pixels around the pointer, row by row (`None` past the image): on Windows
/// those of the screen, elsewhere those seen on the image at `at`.
fn around(w: &Window, view: &View, at: Option<Pos2>) -> Option<Vec<Option<[u8; 3]>>> {
    if let Some(screen) = crate::eyedropper::around_cursor(GRID) {
        return Some(screen.into_iter().map(Some).collect());
    }
    seen_around(w, view, at?)
}

/// The colour taken from `grid`: its middle pixel, or the average of the `size` × `size` around
/// it (the pixels past the image left out).
pub(super) fn average(grid: &[Option<[u8; 3]>], size: u8) -> Option<[u8; 3]> {
    let (mid, half) = ((GRID / 2) as i32, (size.max(1) / 2) as i32);
    let mut sum = [0u32; 3];
    let mut n = 0;
    for (dy, dx) in (-half..=half).flat_map(|dy| (-half..=half).map(move |dx| (dy, dx))) {
        let i = ((mid + dy) * GRID as i32 + mid + dx) as usize;
        if let Some(Some(c)) = grid.get(i) {
            for k in 0..3 {
                sum[k] += c[k] as u32;
            }
            n += 1;
        }
    }
    (n > 0).then(|| sum.map(|v| ((v + n / 2) / n) as u8))
}

/// The box: the magnifier with the pixel taken framed, the new colour over the one in use, the
/// codes.
fn swatch(ctx: &egui::Context, at: Pos2, grid: &[Option<[u8; 3]>], new: Option<[u8; 3]>, old: Option<[u8; 3]>) {
    const CELL: f32 = 8.0;
    egui::Area::new(egui::Id::unique("edit-pick-swatch")).order(egui::Order::Tooltip).fixed_pos(at).interactable(false).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(CELL * GRID as f32), egui::Sense::hover());
                let p = ui.painter();
                for (i, c) in grid.iter().enumerate() {
                    let cell = egui::Rect::from_min_size(rect.min + egui::vec2((i % GRID) as f32, (i / GRID) as f32) * CELL, egui::Vec2::splat(CELL));
                    p.rect_filled(cell, 0.0, c.map_or(Color32::from_gray(60), |[r, g, b]| Color32::from_rgb(r, g, b)));
                }
                let mid = egui::Rect::from_min_size(rect.min + egui::Vec2::splat((GRID / 2) as f32 * CELL), egui::Vec2::splat(CELL));
                p.rect_stroke(mid, 0.0, egui::Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Outside);
                p.rect_stroke(mid.expand(1.0), 0.0, egui::Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
                let Some([r, g, b]) = new else { return };
                // New over old, as Photoshop's ring.
                let (pair, _) = ui.allocate_exact_size(egui::vec2(40.0, CELL * GRID as f32), egui::Sense::hover());
                let (top, bottom) = pair.split_top_bottom_at_fraction(0.5);
                ui.painter().rect_filled(top, 0.0, Color32::from_rgb(r, g, b));
                if let Some([r, g, b]) = old {
                    ui.painter().rect_filled(bottom, 0.0, Color32::from_rgb(r, g, b));
                }
                ui.painter().rect_stroke(pair, 0.0, egui::Stroke::new(1.0, Color32::from_gray(120)), egui::StrokeKind::Inside);
                ui.label(egui::RichText::new(crate::eyedropper::codes([r, g, b])).monospace());
            });
        });
    });
}

/// The pixels seen around `s` on the image: the photo, and over it the objects drawn (one drawing
/// of them, `GRID` pixels a side, as large as the result's pixels).
fn seen_around(w: &Window, view: &View, s: Pos2) -> Option<Vec<Option<[u8; 3]>>> {
    let (iw, ih) = w.size();
    let (left, top) = rotate::corner(iw, ih, w.doc.crop);
    let p = view.image(s);
    let half = (GRID / 2) as f32;
    let (rx, ry) = ((p.x - left).floor() - half, (p.y - top).floor() - half);
    let drawn = (!w.doc.objects.is_empty()).then(|| render::render(&w.doc, w.upright(), &render::View { x: rx, y: ry, w: GRID as u32, h: GRID as u32, scale: 1.0 }, &crate::decode::usvg_options())).flatten();
    let grid: Vec<Option<[u8; 3]>> = (0..GRID * GRID)
        .map(|i| {
            let (x, y) = (rx + (i % GRID) as f32 + 0.5 + left, ry + (i / GRID) as f32 + 0.5 + top);
            let photo = photo_at(w, geom::turned(Pt::new(x, y), -w.doc.angle))?;
            let Some(px) = drawn.as_ref().and_then(|m| m.pixel((i % GRID) as u32, (i / GRID) as u32)) else { return Some(photo) };
            let a = px.alpha() as f32 / 255.0;
            let mix = |src: u8, under: u8| (src as f32 + under as f32 * (1.0 - a)).round().min(255.0) as u8;
            Some([mix(px.red(), photo[0]), mix(px.green(), photo[1]), mix(px.blue(), photo[2])])
        })
        .collect();
    grid[GRID * GRID / 2].map(|_| grid.clone())
}

/// The colour of the photo at `p` (a point of the document); `None` outside it.
fn photo_at(w: &Window, p: Pt) -> Option<[u8; 3]> {
    use image::GenericImageView;
    let Some(Ok((original, ..))) = &w.original else { return None };
    let q = w.doc.source(p);
    let img = &original.image;
    let (x, y) = (q.x + img.width() as f32 / 2.0, q.y + img.height() as f32 / 2.0);
    (x >= 0.0 && y >= 0.0 && x < img.width() as f32 && y < img.height() as f32).then(|| {
        let c = img.get_pixel(x as u32, y as u32);
        [c[0], c[1], c[2]]
    })
}

#[cfg(test)]
mod tests {
    /// One pixel, or the average of the 3 × 3 around the middle one, the pixels past the image
    /// left out.
    #[test]
    fn averaged() {
        let mut grid = vec![Some([0u8, 0, 0]); super::GRID * super::GRID];
        let mid = grid.len() / 2;
        grid[mid] = Some([90, 90, 90]);
        grid[mid + 1] = None;
        assert_eq!(super::average(&grid, 1), Some([90, 90, 90]));
        assert_eq!(super::average(&grid, 3), Some([11, 11, 11]));
    }
}
