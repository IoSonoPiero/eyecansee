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

/// The sheet of a 3D model in `State::of`: models and drawings share it.
pub(crate) const MODEL_SHEET: usize = usize::MAX;

impl State {
    fn clear(&mut self) {
        self.pts.clear();
        self.pts3.clear();
        self.done = false;
    }

    /// The points are of `of`, a file and its sheet (`MODEL_SHEET` for a model): when it changes
    /// they all go, those of a drawing and those of a model (audit 27/11, D.7).
    pub(crate) fn measuring(&mut self, of: (String, usize)) {
        if self.of.as_ref() != Some(&of) {
            self.of = Some(of);
            self.clear();
        }
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
        // Not while the file is read: Measure stays on then (audit 27/11, D.7), its results are another file's.
        let s = self.model.measure.shown.as_ref().filter(|_| self.model.measure.on && self.measure_off().is_none())?;
        // Measure said first, so that turned on it is seen (#367): a model's line is already "Measure".
        let title = self.i18n.t("measure-title");
        if s.lines.is_empty() {
            let head = if s.what == title { title } else { format!("{title} · {}", s.what) };
            return Some((format!("{head}: {}", s.hint), false));
        }
        let body = s.lines.iter().map(|(l, v)| format!("{l} {v}")).collect::<Vec<_>>().join(" · ");
        let unit = if s.no_unit { format!(" ({})", self.i18n.t("measure-no-unit")) } else { String::new() };
        let more = if s.more.is_empty() { String::new() } else { format!(" · {}", s.more) };
        Some((format!("{title} · {body}{unit}{more}"), true))
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
            // A drawing or a model still being read keeps Measure on (audit 27/11, D.7).
            let path = self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.as_str());
            if !path.is_some_and(|p| crate::cad::is_cad(p) || crate::model::is_model(p)) {
                self.model.measure.on = false;
            }
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
        // A model has no areas and no modes: Enter goes back to the grid as without Measure (#345),
        // D, A and G do nothing (#342). On a drawing Enter stays Measure's, also with no area to close.
        let model = self.viewed_mesh().is_some();
        let m = &mut self.model.measure;
        let (esc, enter, back) = ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::Escape), !model && i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::NONE, Key::Backspace)));
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
            if !model && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) && m.mode != mode {
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
        m.measuring((path, sheet));
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

#[cfg(test)]
mod tests {
    use eframe::egui::Key;

    use crate::audit_family_1::{Harness, folder, own_folder};

    /// The viewer on `name` in a folder with a small 3D model (OBJ) and a drawing (DXF), Measure on.
    fn measuring(dir: &str, name: &str) -> Harness {
        let dir = folder(dir, &["b.png"]);
        std::fs::write(dir.join("a.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        std::fs::write(dir.join("c.dxf"), "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n100\n21\n50\n31\n0\n0\nENDSEC\n0\nEOF\n").unwrap();
        let mut h = Harness::new(&dir);
        open(&mut h, name);
        h.key(Key::M);
        h.frame(Vec::new());
        assert!(h.app.model.measure.on, "M did not start Measure");
        h
    }

    fn open(h: &mut Harness, name: &str) {
        let i = h.app.files().iter().position(|f| f.path.ends_with(name)).unwrap();
        h.app.activate(i);
        h.until(|a| a.measure_off().is_none() && (a.viewed_mesh().is_some() || a.zoom_of == a.viewing), "the file shown");
        h.frame(Vec::new());
    }

    /// Audit 27/11, D.7 (a): a point taken on a drawing stayed when the viewer went on to a model,
    /// where it is not seen, so that the first Esc did nothing; and the points of a model stayed
    /// back on the drawing measured before.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn audit_d7_points_of_another_file() {
        if own_folder("measure_view::tests::audit_d7_points_of_another_file") {
            return;
        }
        let mut h = measuring("measure-d7a", "c.dxf");
        h.app.model.measure.pts.push([1.0, 1.0]);
        open(&mut h, "a.obj");
        assert!(h.app.model.measure.pts.is_empty(), "the drawing's point stayed on the model");
        h.key(Key::Escape);
        h.frame(Vec::new());
        assert!(!h.app.model.measure.on, "the first Esc on the model did not close Measure");
        // Back on the drawing measured before, the model's points are gone.
        h.key(Key::M);
        h.frame(Vec::new());
        h.app.model.measure.pts3 = vec![[0.0; 3], [1.0, 0.0, 0.0]];
        open(&mut h, "c.dxf");
        assert!(h.app.model.measure.pts3.is_empty(), "the model's points stayed on the drawing");
    }

    /// Audit 27/11, D.7 (b): on a drawing not read yet Measure went off by itself, and was off
    /// once it was read.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn audit_d7_stays_on_while_read() {
        if own_folder("measure_view::tests::audit_d7_stays_on_while_read") {
            return;
        }
        let mut h = measuring("measure-d7b", "c.dxf");
        // As before the drawing is read: not in memory yet.
        let path = h.app.files().iter().find(|f| f.path.ends_with("c.dxf")).unwrap().path.clone();
        h.app.model.cad.drawings.remove(&path);
        h.frame(Vec::new());
        assert!(h.app.model.measure.on, "Measure went off while the drawing was read");
    }
    /// #367: Measure turned on could hardly be seen, a line of the status bar like the others.
    /// Its line says Measure first (and is a yellow label), on a drawing and on a model; M again
    /// turns it off and the line goes.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn measure_says_itself_in_the_status_bar() {
        if own_folder("measure_view::tests::measure_says_itself_in_the_status_bar") {
            return;
        }
        let mut h = measuring("measure-367", "c.dxf");
        let line = |h: &Harness| h.app.measure_status().map(|(t, _)| t);
        let (title, distance) = (h.app.i18n.t("measure-title"), h.app.i18n.t("measure-distance"));
        assert!(line(&h).is_some_and(|t| t.starts_with(&format!("{title} · {distance}: "))), "the drawing's line: {:?}", line(&h));
        open(&mut h, "a.obj");
        assert!(line(&h).is_some_and(|t| t.starts_with(&title)), "the model's line: {:?}", line(&h));
        h.key(Key::M);
        h.frame(Vec::new());
        assert_eq!(line(&h), None, "M again left the line");
    }
}
