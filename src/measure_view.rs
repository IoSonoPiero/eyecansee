//! The Measure tool of the viewer (#141), on and off with M: on a CAD drawing, points clicked
//! with snap (`cad_measure`) for a distance, an area with its perimeter, or an angle; on a 3D
//! model (`measure_model`) its overall size, surface and volume, and the distance between two
//! points clicked on it. The results stay on screen until Esc, with a button that copies them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Key, Modifiers, Pos2, pos2};

use crate::App;
use crate::cad_geom::P;
use crate::cad_measure::{Measured, Mode, Snap, measured, number};

/// Within this many points of the pointer a click snaps.
const SNAP: f32 = 12.0;
/// The lines and points of a measure: yellow, seen on the dark drawing and on a model.
pub(crate) const INK: Color32 = Color32::from_rgb(255, 214, 10);

#[derive(Default)]
pub(crate) struct State {
    pub(crate) on: bool,
    pub(crate) mode: Mode,
    /// What the points are on: the file and its sheet (0 the model); they go when it changes.
    of: Option<(String, usize)>,
    pts: Vec<P>,
    /// An area closed (Enter or a double click): the next click starts a new one.
    done: bool,
    /// The points clicked on a 3D model, in its coordinates.
    pub(crate) pts3: Vec<[f32; 3]>,
    /// The sizes of the models, measured in the background (`None` while being measured).
    pub(crate) sizes: HashMap<String, Option<crate::model_measure::Sizes>>,
    pub(crate) sizes_done: Arc<Mutex<Vec<(String, crate::model_measure::Sizes)>>>,
}

impl State {
    fn clear(&mut self) {
        self.pts.clear();
        self.pts3.clear();
        self.done = false;
    }
}

impl App {
    /// Why Measure cannot be used: only on a CAD drawing or a 3D model in the viewer (a model
    /// drawn by the graphics card: without one it is a still picture).
    pub(crate) fn measure_off(&self) -> Option<String> {
        let path = self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.as_str());
        let ok = (self.viewed_mesh().is_some() && self.model.gpu.is_some()) || path.is_some_and(|p| crate::cad::is_cad(p) && self.model.cad.drawings.contains_key(p));
        (!ok).then(|| self.i18n.t("measure-off"))
    }

    /// The Measure tool on or off (M, View > Measure).
    pub(crate) fn toggle_measure(&mut self) {
        let m = &mut self.model.measure;
        m.on = !m.on;
        m.clear();
    }

    /// M turns Measure on and off; while on, Esc clears the points (and with none closes it),
    /// Enter closes an area, Backspace takes back the last point, D, A and G choose what a drawing
    /// measures. Taken from the input: Esc does not close the viewer, Backspace does not go back.
    pub(crate) fn measure_keys(&mut self, ctx: &egui::Context) {
        if self.measure_off().is_some() {
            self.model.measure.on = false;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::M)) {
            self.toggle_measure();
        }
        let m = &mut self.model.measure;
        if !m.on {
            return;
        }
        let (esc, enter, back) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Escape), i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Backspace)));
        if esc {
            if m.pts.is_empty() && m.pts3.is_empty() { m.on = false } else { m.clear() }
        }
        if enter && m.mode == Mode::Area && m.pts.len() >= 3 {
            m.done = true;
        }
        if back {
            m.done = false;
            m.pts.pop();
            m.pts3.pop();
        }
        for (key, mode) in [(Key::D, Mode::Distance), (Key::A, Mode::Area), (Key::G, Mode::Angle)] {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) && m.mode != mode {
                m.mode = mode;
                m.clear();
            }
        }
    }

    /// On a drawing in the viewer, shown in `dest` (the part `uv` of its picture): the points
    /// clicked and their lines, the point the pointer snaps to, and the results.
    pub(crate) fn cad_measure(&mut self, ui: &egui::Ui, response: &egui::Response, current: usize, dest: egui::Rect, uv: egui::Rect) {
        let Some(path) = self.files().get(current).map(|f| f.path.clone()) else { return };
        if !self.model.measure.on || !self.turn.is_none() {
            return;
        }
        let sheet = self.cad_sheet(&path);
        let Some(d) = self.cad_drawn(&path) else { return };
        let Some(place) = d.placement(crate::decode::CAD_SIDE) else { return };
        let m = &mut self.model.measure;
        if m.of.as_ref() != Some(&(path.clone(), sheet)) {
            m.of = Some((path, sheet));
            m.clear();
        }
        let to_drawing = |p: Pos2| place.point([f64::from(uv.min.x + (p.x - dest.min.x) / dest.width() * uv.width()), f64::from(uv.min.y + (p.y - dest.min.y) / dest.height() * uv.height())]);
        let to_screen = |p: P| {
            let [u, v] = place.uv(p);
            pos2(dest.min.x + (u as f32 - uv.min.x) / uv.width() * dest.width(), dest.min.y + (v as f32 - uv.min.y) / uv.height() * dest.height())
        };
        // Drawing units in a point of the screen.
        let per_point = f64::from(uv.width()) * place.size[0] / place.scale / f64::from(dest.width().max(1.0));
        let hover = response.hover_pos().map(|p| crate::cad_measure::snap(&d, to_drawing(p), f64::from(SNAP) * per_point));
        if response.clicked()
            && let Some((p, _)) = hover
        {
            if m.done || m.pts.len() >= m.mode.points() {
                m.clear();
            }
            m.pts.push(p);
        }
        if response.double_clicked() && m.mode == Mode::Area && m.pts.len() >= 3 {
            m.done = true;
        }
        let painter = ui.painter_at(response.rect);
        draw_points(&painter, &m.pts.iter().map(|&p| to_screen(p)).collect::<Vec<_>>(), m.mode == Mode::Area && (m.done || m.pts.len() >= 3));
        if let Some((p, kind)) = hover {
            let at = to_screen(p);
            if let Some(&last) = m.pts.last().filter(|_| !m.done && m.pts.len() < m.mode.points()) {
                painter.line_segment([to_screen(last), at], egui::Stroke::new(1.0, INK.gamma_multiply(0.6)));
            }
            snap_mark(&painter, at, kind);
        }
        let unit = d.unit.map(|u| u.symbol);
        self.measure_box(ui.ctx(), response.rect, unit);
    }

    /// The box of the results, at the top right of the viewer: what is measured (D, A, G), how
    /// to go on, the results, Copy.
    fn measure_box(&mut self, ctx: &egui::Context, rect: egui::Rect, unit: Option<&str>) {
        let t = |id: &str| self.i18n.t(id);
        let m = &self.model.measure;
        let lines = cad_lines(&self.i18n, measured(m.mode, &m.pts), unit);
        let hint = t(match m.mode {
            Mode::Distance => "measure-hint-distance",
            Mode::Area => "measure-hint-area",
            Mode::Angle => "measure-hint-angle",
        });
        let modes = [(Mode::Distance, "measure-distance", "D"), (Mode::Area, "measure-area", "A"), (Mode::Angle, "measure-angle", "G")];
        let mut chosen = m.mode;
        result_box(ctx, rect, true, &t("measure-title"), |ui| {
            ui.horizontal(|ui| {
                for (mode, label, key) in modes {
                    ui.selectable_value(&mut chosen, mode, t(label)).on_hover_text(key);
                }
            });
            ui.weak(&hint);
            lines_and_copy(ui, &lines, unit.is_none(), &self.i18n);
        });
        if chosen != self.model.measure.mode {
            self.model.measure.mode = chosen;
            self.model.measure.clear();
        }
    }
}

/// The lines of a drawing's measure, as shown and copied.
fn cad_lines(i18n: &crate::i18n::I18n, r: Option<Measured>, unit: Option<&str>) -> Vec<(String, String)> {
    let dec = i18n.t("decimal-separator");
    let n = |v: f64| number(v, &dec);
    let with = |v: f64, power: &str| match unit {
        Some(u) => format!("{} {u}{power}", n(v)),
        None => n(v),
    };
    match r {
        None => Vec::new(),
        Some(Measured::Distance { length, dx, dy }) => vec![(i18n.t("measure-distance"), with(length, "")), ("ΔX".into(), with(dx, "")), ("ΔY".into(), with(dy, ""))],
        Some(Measured::Area { area, perimeter }) => vec![(i18n.t("measure-area"), with(area, "²")), (i18n.t("measure-perimeter"), with(perimeter, ""))],
        Some(Measured::Angle(a)) => vec![(i18n.t("measure-angle"), format!("{}°", number(a, &dec)))],
    }
}

/// A box of results over a top corner of `rect` (the right one on a drawing, whose corner on
/// the left is often its title; on a model the axes are there), which can be moved.
pub(crate) fn result_box(ctx: &egui::Context, rect: egui::Rect, right: bool, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    let (pivot, at) = if right { (egui::Align2::RIGHT_TOP, rect.right_top() + egui::vec2(-12.0, 12.0)) } else { (egui::Align2::LEFT_TOP, rect.left_top() + egui::vec2(12.0, 12.0)) };
    egui::Area::new(egui::Id::unique(if right { "measure-box-drawing" } else { "measure-box-model" })).order(egui::Order::Foreground).pivot(pivot).default_pos(at).movable(true).constrain_to(rect).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_max_width(320.0);
            ui.strong(title);
            add(ui);
        });
    });
}

/// The results in two columns, the note on a missing unit, Copy and the keys.
pub(crate) fn lines_and_copy(ui: &mut egui::Ui, lines: &[(String, String)], no_unit: bool, i18n: &crate::i18n::I18n) {
    if lines.is_empty() {
        return;
    }
    ui.separator();
    egui::Grid::new("measure-lines").num_columns(2).show(ui, |ui| {
        for (label, value) in lines {
            ui.weak(label);
            ui.strong(value);
            ui.end_row();
        }
    });
    if no_unit {
        ui.weak(i18n.t("measure-no-unit"));
    }
    ui.horizontal(|ui| {
        if ui.button(i18n.t("measure-copy")).clicked() {
            let text: Vec<String> = lines.iter().map(|(l, v)| format!("{l}: {v}")).collect();
            ui.ctx().copy_text(text.join("\n"));
        }
        ui.weak(i18n.t("measure-keys"));
    });
}

/// The points clicked and the lines between them, closed for an area.
pub(crate) fn draw_points(painter: &egui::Painter, pts: &[Pos2], closed: bool) {
    let stroke = egui::Stroke::new(2.0, INK);
    for w in pts.windows(2) {
        painter.line_segment([w[0], w[1]], stroke);
    }
    if closed && let (Some(&a), Some(&b)) = (pts.last(), pts.first()) {
        painter.line_segment([a, b], stroke);
    }
    for &p in pts {
        painter.circle_filled(p, 3.5, INK);
    }
}

/// The mark of what the pointer snaps to, as CAD programs draw them: a square on an end, a
/// triangle on a middle, a circle on a centre, a cross where two lines cross, a small cross alone.
fn snap_mark(painter: &egui::Painter, at: Pos2, kind: Snap) {
    let s = egui::Stroke::new(2.0, Color32::from_rgb(80, 220, 120));
    let r = 7.0;
    match kind {
        Snap::End => {
            painter.rect_stroke(egui::Rect::from_center_size(at, egui::vec2(2.0 * r, 2.0 * r)), 0.0, s, egui::StrokeKind::Middle);
        }
        Snap::Middle => {
            let pts = vec![at + egui::vec2(0.0, -r), at + egui::vec2(r, r * 0.8), at + egui::vec2(-r, r * 0.8)];
            painter.add(egui::Shape::closed_line(pts, s));
        }
        Snap::Centre => {
            painter.circle_stroke(at, r, s);
        }
        Snap::Cross => {
            painter.line_segment([at + egui::vec2(-r, -r), at + egui::vec2(r, r)], s);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, -r)], s);
        }
        Snap::Free => {
            let s = egui::Stroke::new(1.0, INK);
            painter.line_segment([at + egui::vec2(-r, 0.0), at + egui::vec2(r, 0.0)], s);
            painter.line_segment([at + egui::vec2(0.0, -r), at + egui::vec2(0.0, r)], s);
        }
    }
}
