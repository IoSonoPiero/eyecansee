//! The Measure tool on a 3D model (#141, `measure_view`): its overall size along X, Y and Z, its
//! surface and, when closed, its volume (`model_measure`, worked out in the background once per
//! model), and the distance between two points clicked on it.

use std::sync::Arc;

use eframe::egui::{self, pos2};

use crate::App;
use crate::cad_measure::number;
use crate::measure_view::{INK, Shown, draw_points, pointer_label};
use crate::model::Mesh;
use crate::model_measure::Sizes;

/// Above this many triangles the point under the pointer is not looked for at every move.
const LIVE_MAX: usize = 2_000_000;

fn distance3(a: [f32; 3], b: [f32; 3]) -> f64 {
    (0..3).map(|k| f64::from(b[k] - a[k]).powi(2)).sum::<f64>().sqrt()
}

impl App {
    /// Over the model drawn in `rect`: a click (not a drag) takes the point of the model under
    /// the pointer, two give a distance, written next to the pointer while choosing the second;
    /// the points and the line; the results go to the status bar (`measure_status`).
    pub(crate) fn model_measure(&mut self, ui: &egui::Ui, response: &egui::Response, path: &str, mesh: &Arc<Mesh>) {
        if !self.model.measure.on {
            return;
        }
        let rect = response.rect;
        let ppp = ui.ctx().pixels_per_point();
        let size = [rect.width() * ppp, rect.height() * ppp];
        let view = self.model.view;
        let under = |p: egui::Pos2| {
            let (origin, dir) = crate::render3d::ray(mesh, &view, size, [(p.x - rect.min.x) * ppp, (p.y - rect.min.y) * ppp]);
            crate::model_measure::hit(mesh, origin, dir)
        };
        let m = &mut self.model.measure;
        if response.clicked()
            && let Some(hit) = response.interact_pointer_pos().and_then(under)
        {
            if m.pts3.len() >= 2 {
                m.pts3.clear();
            }
            m.pts3.push(hit);
        }
        let screen = |p: [f32; 3]| crate::render3d::to_screen(mesh, &view, size, p).map(|[x, y]| pos2(rect.min.x + x / ppp, rect.min.y + y / ppp));
        let painter = ui.painter_at(rect);
        draw_points(&painter, &m.pts3.iter().filter_map(|&p| screen(p)).collect::<Vec<_>>(), false);
        let dec = self.i18n.t("decimal-separator");
        let with_unit = |d: f64| mesh.unit.map_or_else(|| number(d, &dec), |u| format!("{} {u}", number(d, &dec)));
        // With one point: the line and the distance to the point under the pointer (not on very
        // large models, where finding it at every move would slow the view).
        if let ([a], Some(p)) = (&m.pts3[..], response.hover_pos().filter(|_| mesh.tris.len() <= LIVE_MAX))
            && let Some(b) = under(p)
            && let (Some(sa), Some(sb)) = (screen(*a), screen(b))
        {
            painter.line_segment([sa, sb], egui::Stroke::new(1.0, INK.gamma_multiply(0.6)));
            pointer_label(&painter, p, &with_unit(distance3(*a, b)));
        }
        if let ([a, b], Some(sb)) = (&m.pts3[..], m.pts3.get(1).copied().and_then(screen)) {
            pointer_label(&painter, sb, &with_unit(distance3(*a, *b)));
        }
        let sizes = self.model_sizes(ui.ctx(), path, mesh);
        let lines = self.model_lines(sizes.as_ref(), mesh.unit);
        let hint = if sizes.is_none() { self.i18n.t("measure-measuring") } else { self.i18n.t("measure-hint-3d") };
        let more = if self.model.measure.pts3.len() < 2 { hint.clone() } else { String::new() };
        self.model.measure.shown = Some(Shown { what: self.i18n.t("measure-title"), lines, hint, no_unit: mesh.unit.is_none(), more });
    }

    /// The sizes of the model, asked once and worked out in the background: `None` meanwhile.
    pub(crate) fn model_sizes(&mut self, ctx: &egui::Context, path: &str, mesh: &Arc<Mesh>) -> Option<Sizes> {
        let m = &mut self.model.measure;
        for (p, s) in m.sizes_done.lock().unwrap().drain(..) {
            m.sizes.insert(p, Some(s));
        }
        if let Some(s) = m.sizes.get(path) {
            return s.clone();
        }
        m.sizes.insert(path.to_string(), None);
        let (done, ctx, path, mesh) = (m.sizes_done.clone(), ctx.clone(), path.to_string(), mesh.clone());
        crate::logging::spawn_named("model-measure", move || {
            let s = crate::model_measure::sizes(&mesh);
            done.lock().unwrap().push((path, s));
            ctx.request_repaint();
        });
        None
    }

    /// The lines of a model's measures: size, surface, volume or why there is none, and the
    /// distance between the two points clicked.
    fn model_lines(&self, sizes: Option<&Sizes>, unit: Option<&str>) -> Vec<(String, String)> {
        let dec = self.i18n.t("decimal-separator");
        let n = |v: f64| number(v, &dec);
        let with = |text: String, power: &str| match unit {
            Some(u) => format!("{text} {u}{power}"),
            None => text,
        };
        let t = |id: &str| self.i18n.t(id);
        let mut lines = Vec::new();
        // The distance first: the status bar may cut what follows.
        if let [a, b] = &self.model.measure.pts3[..] {
            lines.push((t("measure-distance"), with(n(distance3(*a, *b)), "")));
        }
        if let Some(s) = sizes {
            lines.push((t("measure-size"), with(s.size.map(n).join(" × "), "")));
            lines.push((t("measure-surface"), with(n(s.surface), "²")));
            let volume = match s.volume {
                Some(v) => with(n(v), "³"),
                None => self.i18n.t_args("measure-open", &[("n", &s.open_edges.to_string())]),
            };
            lines.push((t("measure-volume"), volume));
        }
        lines
    }
}
