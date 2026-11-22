//! The section of a 3D model in the viewer (#145): S cuts the model with a plane across X, Y or
//! Z (`render3d::Cut`, drawn by the graphics card and by `render3d` for Save as… and without a
//! card), moved with Alt and the wheel or the keys , and .; F keeps the other side. The face of
//! the cut is filled where it crosses the inside of the model, so walls and thicknesses show; the
//! status bar says where the plane is.

use eframe::egui::{self, Key, Modifiers};

use crate::App;
use crate::model::Mesh;
use crate::render3d::{self, Cut, View};

/// How far a key or a notch of the wheel moves the plane, across the model.
const STEP: f32 = 0.01;
/// Seconds the plane stays drawn after it was moved, the last of them fading.
const PLANE_SHOWN: f64 = 1.0;

impl App {
    /// S on and off (through the middle; across Z for a model with Z up: a plan of a building),
    /// X, Y, Z its axis, F the other side, , and . move it. Taken from the input.
    pub(crate) fn section_keys(&mut self, ctx: &egui::Context, z_up: bool) {
        let before = self.model.view.cut;
        self.section_keys_taken(ctx, z_up);
        if self.model.view.cut.is_some() && self.model.view.cut != before {
            self.model.section_moved = ctx.input(|i| i.time);
        }
    }

    fn section_keys_taken(&mut self, ctx: &egui::Context, z_up: bool) {
        let view = &mut self.model.view;
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::S)) {
            view.cut = if view.cut.is_some() { None } else { Some(Cut::middle(z_up)) };
        }
        let Some(cut) = &mut view.cut else { return };
        ctx.input_mut(|i| {
            for (key, axis) in [(Key::X, 0), (Key::Y, 1), (Key::Z, 2)] {
                if i.consume_key(Modifiers::NONE, key) {
                    cut.axis = axis;
                }
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                cut.flip = !cut.flip;
            }
            let steps = i.count_and_consume_key(Modifiers::NONE, Key::Period) as f32 - i.count_and_consume_key(Modifiers::NONE, Key::Comma) as f32;
            cut.at = (cut.at + steps * STEP).clamp(0.0, 1.0);
        });
    }

    /// Alt and the wheel over the model move the plane; `true` if they did (the wheel then does
    /// not zoom).
    pub(crate) fn section_wheel(&mut self, ui: &egui::Ui, hovered: bool) -> bool {
        let Some(cut) = &mut self.model.view.cut else { return false };
        if !hovered {
            return false;
        }
        let notches: f32 = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.alt => Some(crate::viewer::notches(*unit, *delta)),
                    _ => None,
                })
                .sum()
        });
        cut.at = (cut.at + notches * STEP).clamp(0.0, 1.0);
        if notches != 0.0 {
            self.model.section_moved = ui.input(|i| i.time);
        }
        ui.input(|i| i.modifiers.alt)
    }

    /// In the status bar while a section is on: its axis, where the plane is, the side kept, why
    /// the cut is not filled, a slider that moves the plane (taken by `section_slider`), the keys.
    pub(crate) fn section_bar(&self, ui: &mut egui::Ui) {
        let Some(((_, mesh), cut)) = self.viewed_mesh().zip(self.model.view.cut) else { return };
        let t = |id: &str| self.i18n.t(id);
        let axis = ["X", "Y", "Z"][cut.axis.min(2)];
        let side = if cut.flip { format!(" · {}", t("section-flip")) } else { String::new() };
        // Also where the plane is, from the model's first face, as Fusion and FreeCAD say it; as
        // wide as at the far end, so the bar does not shake while it moves.
        let length = 2.0 * mesh.half[cut.axis.min(2)];
        let dec = t("decimal-separator");
        let text = |at: f32| {
            let from = fixed(at * length, length, &dec);
            let from = mesh.unit.map_or(from.clone(), |u| format!("{from} {u}"));
            format!("{}: {axis} {:.0}% · {from}{side}", t("section-title"), at * 100.0)
        };
        ui.separator();
        crate::bar_rows::steady(ui, &text(cut.at), &text(1.0));
        let mut at = cut.at;
        ui.spacing_mut().slider_width = 120.0;
        // Where the plane is, in tenths of the model: the label before says the percentage.
        let slider = egui::Slider::new(&mut at, 0.0..=1.0).show_value(false);
        if crate::ticked::add(ui, slider, &crate::ticked::Marks::new(&(0.0..=1.0), None)).on_hover_text(t("section-keys")).changed() {
            ui.ctx().data_mut(|d| d.insert_temp(slider_id(), at));
        }
        // A model that is not closed has no known inside: the cut is left empty, and said so.
        let open = self.viewed_mesh().and_then(|(p, m)| self.model.measure.sizes.known(&p, &m)).is_some_and(|s| s.open_edges > 0);
        if open {
            ui.label(crate::theme::alert(t("section-open")));
        }
        ui.add(egui::Label::new(t("section-keys")).truncate());
    }

    /// The plane where the status bar's slider put it.
    pub(crate) fn section_slider(&mut self, ctx: &egui::Context) {
        if let Some(at) = ctx.data_mut(|d| d.remove_temp::<f32>(slider_id()))
            && let Some(cut) = &mut self.model.view.cut
        {
            cut.at = at.clamp(0.0, 1.0);
            self.model.section_moved = ctx.input(|i| i.time);
        }
    }

    /// The plane, a square a little larger than the model, drawn see-through over it for a moment
    /// after it moved, so it shows where it cuts; then it fades out and leaves the cut in view.
    pub(crate) fn section_plane(&self, painter: &egui::Painter, rect: egui::Rect, mesh: &Mesh, view: &View) {
        let Some(cut) = view.cut else { return };
        let since = painter.ctx().input(|i| i.time) - self.model.section_moved;
        if !(0.0..PLANE_SHOWN).contains(&since) {
            return;
        }
        painter.ctx().request_repaint();
        let ppp = painter.ctx().pixels_per_point();
        let size = [rect.width() * ppp, rect.height() * ppp];
        let corners = plane_corners(&cut, mesh).map(|p| render3d::to_screen(mesh, view, size, p).map(|[x, y]| egui::pos2(rect.min.x + x / ppp, rect.min.y + y / ppp)));
        let Some(points) = corners.into_iter().collect::<Option<Vec<_>>>() else { return };
        // Full for most of the time, then fading over the last third.
        let fade = (((PLANE_SHOWN - since) / (PLANE_SHOWN / 3.0)).min(1.0)) as f32;
        let fill = egui::Color32::from_rgba_unmultiplied(120, 180, 255, (60.0 * fade) as u8);
        let edge = egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(170, 210, 255, (200.0 * fade) as u8));
        painter.add(egui::Shape::convex_polygon(points, fill, edge));
    }
}

/// The corners of the square drawn for the plane: on the plane, around the model's centre, a
/// little larger than the model across the other two axes.
fn plane_corners(cut: &Cut, mesh: &Mesh) -> [[f32; 3]; 4] {
    let axis = cut.axis.min(2);
    let plane = cut.plane(mesh);
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let (hu, hv) = (mesh.half[u] * 1.15 + mesh.radius * 0.02, mesh.half[v] * 1.15 + mesh.radius * 0.02);
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(a, b)| {
        let mut p = mesh.center;
        p[axis] = plane[3] / plane[axis];
        p[u] += a * hu;
        p[v] += b * hv;
        p
    })
}

/// `v` with as many decimals for every position of the plane (those of `number` for `max`),
/// so the text keeps its width.
fn fixed(v: f32, max: f32, decimal: &str) -> String {
    let places = match max.abs() {
        a if a >= 10_000.0 => 0,
        a if a >= 100.0 => 1,
        a if a >= 1.0 => 2,
        _ => 4,
    };
    format!("{v:.places$}").replace('.', decimal)
}

fn slider_id() -> egui::Id {
    egui::Id::unique("section-slider")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The square lies on the plane and goes beyond the model on the other two axes.
    #[test]
    fn plane_square_on_the_cut() {
        let obj = "v 0 0 0
v 4 0 0
v 0 2 0
v 0 0 1
f 1 2 3
f 1 2 4
f 1 3 4
f 2 3 4
";
        let mesh = crate::model::parse(obj.as_bytes(), "t.obj").unwrap();
        let cut = Cut { axis: 0, at: 0.25, flip: true, fill: true };
        for p in plane_corners(&cut, &mesh) {
            assert!((p[0] - 1.0).abs() < 1e-2, "{p:?}");
            assert!(!(0.0..=2.0).contains(&p[1]) && !(0.0..=1.0).contains(&p[2]), "{p:?}");
        }
    }
}
