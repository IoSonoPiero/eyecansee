//! The Measure tool of the viewer (#141), on and off with M: on a CAD drawing, points clicked
//! with snap (`cad_measure`) for a distance, an area with its perimeter, or an angle; on a 3D
//! model (`measure_model`) its overall size, surface and volume, and the distance between two
//! points clicked on it. While measuring, the value is written next to the pointer; the results
//! go to the status bar (`measure_status`), with Copy (also Ctrl+C), until Esc. Nothing covers
//! the drawing or the model (Piero, 15/11).

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
    /// The sizes of the models, measured in the background.
    pub(crate) sizes: crate::model_measure::Memo,
    /// What the status bar shows, set by the viewer at every frame while measuring.
    pub(crate) shown: Option<Shown>,
}

/// The status bar's part of a measure: what is measured, the results (label, value) or how to
/// go on, and whether the file says its unit.
#[derive(Clone, Debug, Default)]
pub(crate) struct Shown {
    pub what: String,
    pub lines: Vec<(String, String)>,
    pub hint: String,
    pub no_unit: bool,
    /// After the results, not copied: how to go on (on a model, the two points to click).
    pub more: String,
}

impl Shown {
    /// The results as copied: one per row.
    pub(crate) fn text(&self) -> String {
        self.lines.iter().map(|(l, v)| format!("{l}: {v}")).collect::<Vec<_>>().join("\n")
    }
}

impl State {
    fn clear(&mut self) {
        self.pts.clear();
        self.pts3.clear();
        self.done = false;
    }
}

impl App {
    /// Why Measure cannot be used: only on a CAD drawing or a 3D model in the viewer.
    pub(crate) fn measure_off(&self) -> Option<String> {
        let path = self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.as_str());
        let ok = self.viewed_mesh().is_some() || path.is_some_and(|p| crate::cad::is_cad(p) && self.model.cad.drawings.contains_key(p));
        (!ok).then(|| self.i18n.t("measure-off"))
    }

    /// The Measure tool on or off (M, View > Measure).
    pub(crate) fn toggle_measure(&mut self) {
        let m = &mut self.model.measure;
        m.on = !m.on;
        m.shown = None;
        m.clear();
    }

    /// For the status bar while measuring: what is measured, then the results or how to go on.
    pub(crate) fn measure_status(&self) -> Option<(String, bool)> {
        let s = self.model.measure.shown.as_ref().filter(|_| self.model.measure.on)?;
        if s.lines.is_empty() {
            return Some((format!("{}: {}", s.what, s.hint), false));
        }
        let body = s.lines.iter().map(|(l, v)| format!("{l} {v}")).collect::<Vec<_>>().join(" · ");
        let unit = if s.no_unit { format!(" ({})", self.i18n.t("measure-no-unit")) } else { String::new() };
        let more = if s.more.is_empty() { String::new() } else { format!(" · {}", s.more) };
        Some((body + &unit + &more, true))
    }

    /// The results of the measure on the clipboard (Copy in the status bar, Ctrl+C).
    pub(crate) fn copy_measure(&self, ctx: &egui::Context) {
        if let Some(s) = self.model.measure.shown.as_ref().filter(|s| !s.lines.is_empty()) {
            ctx.copy_text(s.text());
        }
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
        // Ctrl+C copies the results, not the file, while there are some.
        if m.shown.as_ref().is_some_and(|s| !s.lines.is_empty()) && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::C)) {
            self.copy_measure(ctx);
        }
        let m = &mut self.model.measure;
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
        let unit = d.unit.map(|u| u.symbol);
        let going = !m.done && m.pts.len() < m.mode.points();
        if let Some((p, kind)) = hover {
            let at = to_screen(p);
            if let Some(&last) = m.pts.last().filter(|_| going) {
                painter.line_segment([to_screen(last), at], egui::Stroke::new(1.0, INK.gamma_multiply(0.6)));
            }
            snap_mark(&painter, at, kind);
            // The value as it would be with the point under the pointer, next to it.
            let mut next = m.pts.clone();
            next.push(p);
            if let Some((_, value)) = going.then(|| cad_lines(&self.i18n, measured(m.mode, &next), unit).into_iter().next()).flatten() {
                pointer_label(&painter, at, &value);
            }
        }
        let m = &self.model.measure;
        let (what, hint) = match m.mode {
            Mode::Distance => ("measure-distance", "measure-hint-distance"),
            Mode::Area => ("measure-area", "measure-hint-area"),
            Mode::Angle => ("measure-angle", "measure-hint-angle"),
        };
        let shown = Shown { what: self.i18n.t(what), lines: cad_lines(&self.i18n, measured(m.mode, &m.pts), unit), hint: self.i18n.t(hint), no_unit: unit.is_none(), more: String::new() };
        self.model.measure.shown = Some(shown);
    }
}

/// A value written next to the pointer `at`, on a dark band so it reads on any drawing.
pub(crate) fn pointer_label(painter: &egui::Painter, at: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(14.0), INK);
    let pos = at + egui::vec2(16.0, 14.0);
    let rect = egui::Rect::from_min_size(pos, galley.size()).expand2(egui::vec2(5.0, 2.0));
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(190));
    painter.galley(pos, galley, INK);
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
