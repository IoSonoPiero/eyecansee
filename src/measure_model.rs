//! The Measure tool on a 3D model (#141, `measure_view`): its overall size along X, Y and Z, its
//! surface and, when closed, its volume (`model_measure`, worked out in the background once per
//! model), and the distance between two points clicked on it.

use std::sync::Arc;

use eframe::egui::{self, pos2};

use crate::App;
use crate::cad_measure::number;
use crate::measure_view::{draw_points, lines_and_copy, result_box};
use crate::model::Mesh;
use crate::model_measure::Sizes;

impl App {
    /// Over the model drawn in `rect`: a click (not a drag) takes the point of the model under
    /// the pointer, two give a distance; the points, the line and the results.
    pub(crate) fn model_measure(&mut self, ui: &egui::Ui, response: &egui::Response, path: &str, mesh: &Arc<Mesh>) {
        if !self.model.measure.on {
            return;
        }
        let rect = response.rect;
        let ppp = ui.ctx().pixels_per_point();
        let size = [rect.width() * ppp, rect.height() * ppp];
        let view = self.model.view;
        let m = &mut self.model.measure;
        if response.clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let (origin, dir) = crate::render3d::ray(mesh, &view, size, [(p.x - rect.min.x) * ppp, (p.y - rect.min.y) * ppp]);
            if let Some(hit) = crate::model_measure::hit(mesh, origin, dir) {
                if m.pts3.len() >= 2 {
                    m.pts3.clear();
                }
                m.pts3.push(hit);
            }
        }
        let shown: Vec<egui::Pos2> = m.pts3.iter().filter_map(|&p| crate::render3d::to_screen(mesh, &view, size, p)).map(|[x, y]| pos2(rect.min.x + x / ppp, rect.min.y + y / ppp)).collect();
        draw_points(&ui.painter_at(rect), &shown, false);
        let sizes = self.model_sizes(ui.ctx(), path, mesh);
        let lines = self.model_lines(sizes.as_ref(), mesh.unit);
        let hint = self.i18n.t("measure-hint-3d");
        let measuring = sizes.is_none().then(|| self.i18n.t("measure-measuring"));
        result_box(ui.ctx(), rect, false, &self.i18n.t("measure-title"), |ui| {
            ui.weak(&hint);
            if let Some(text) = &measuring {
                ui.weak(text);
            }
            lines_and_copy(ui, &lines, mesh.unit.is_none(), &self.i18n);
        });
    }

    /// The sizes of the model, asked once and worked out in the background: `None` meanwhile.
    fn model_sizes(&mut self, ctx: &egui::Context, path: &str, mesh: &Arc<Mesh>) -> Option<Sizes> {
        let m = &mut self.model.measure;
        for (p, s) in m.sizes_done.lock().unwrap().drain(..) {
            m.sizes.insert(p, Some(s));
        }
        if let Some(s) = m.sizes.get(path) {
            return s.clone();
        }
        m.sizes.insert(path.to_string(), None);
        let (done, ctx, path, mesh) = (m.sizes_done.clone(), ctx.clone(), path.to_string(), mesh.clone());
        std::thread::spawn(move || {
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
        if let Some(s) = sizes {
            lines.push((t("measure-size"), with(s.size.map(n).join(" × "), "")));
            lines.push((t("measure-surface"), with(n(s.surface), "²")));
            let volume = match s.volume {
                Some(v) => with(n(v), "³"),
                None => self.i18n.t_args("measure-open", &[("n", &s.open_edges.to_string())]),
            };
            lines.push((t("measure-volume"), volume));
        }
        if let [a, b] = &self.model.measure.pts3[..] {
            let d = (0..3).map(|k| f64::from(b[k] - a[k]).powi(2)).sum::<f64>().sqrt();
            lines.push((t("measure-distance"), with(n(d), "")));
        }
        lines
    }
}
