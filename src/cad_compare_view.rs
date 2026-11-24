//! Two versions of a CAD drawing compared in the viewer (#143): chosen in the grid, read and
//! compared in the background (`Loader::compare_drawings`, `cad_compare`), then shown in place of
//! the first as a drawing of its own, so zoom, layouts, the layers panel (in both, only in the
//! first, only in the second: also the legend), Measure and Save as… work on it. N and Shift+N
//! go to the next and previous change. Moving to another file or leaving the viewer ends it.

use std::sync::Arc;

use eframe::egui;

use crate::App;
use crate::cad::Drawing;
use crate::cad_compare::Compared;

/// The comparison shown: the paths, the first drawing as read (back in its place at the end),
/// the places that changed and the one shown last.
pub(crate) struct Comparison {
    paths: [String; 2],
    first: Arc<Drawing>,
    units: [Option<&'static str>; 2],
    changes: Vec<[f64; 4]>,
    at: Option<usize>,
    /// A change to zoom to, asked by N or Shift+N, done by the viewer (`compare_jump`).
    jump: bool,
}

impl App {
    /// Why Compare drawings cannot start on the files chosen: two CAD drawings are needed.
    pub(crate) fn compare_off(&self, files: &[String]) -> Option<String> {
        let ok = files.len() == 2 && files.iter().all(|p| crate::cad::is_cad(p)) && self.loader.is_some();
        (!ok).then(|| self.i18n.t("compare-off"))
    }

    /// Compare drawings: the two drawings chosen read and compared in the background.
    pub(crate) fn start_compare(&mut self, ctx: &egui::Context) {
        let files = self.files_chosen(None);
        if self.compare_off(&files).is_some() {
            return;
        }
        let Some(loader) = &self.loader else { return };
        let name = |p: &String| crate::source::name(p).to_string();
        let names = [self.i18n.t("compare-both"), self.i18n.t_args("compare-only", &[("name", &name(&files[0]))]), self.i18n.t_args("compare-only", &[("name", &name(&files[1]))])];
        loader.compare_drawings([files[0].clone(), files[1].clone()], names);
        self.status_note = Some(self.i18n.t("compare-reading"));
        self.note_until = Some(ctx.input(|i| i.time) + 30.0);
    }

    /// The comparison read: shown in the viewer in place of the first drawing, with the layers
    /// panel open as its legend.
    pub(crate) fn on_compared(&mut self, r: Result<Compared, String>) {
        let c = match r {
            Ok(c) => c,
            // In the error bar, to close and to copy, as every error: it was a line in the status
            // bar that went after eight seconds.
            // The drawing that did not open is said as the viewer says it, the library's text
            // under it as the detail (`I18n::compare_failed`); the log has it as before.
            Err(e) => {
                log!("compare: ERROR {}", e.replace('\u{1}', ": "));
                (self.status_note, self.note_until) = (None, None);
                self.error = Some(self.i18n.compare_failed(&e));
                return;
            }
        };
        self.status_note = None;
        self.end_compare();
        let Some(index) = self.files().iter().position(|f| f.path == c.paths[0]) else { return };
        let path = c.paths[0].clone();
        self.forget_pictures(&path);
        let s = &mut self.model.cad;
        s.drawings.insert(path.clone(), c.drawing.clone());
        // A choice of layers of its own (its three), which also has the viewer draw it again.
        s.layers.insert(path.clone(), (1, c.drawing.layers_shown()));
        s.sheet = (path, 0);
        s.panel = true;
        s.compare = Some(Comparison { units: [c.first.unit.map(|u| u.symbol), c.second.unit.map(|u| u.symbol)], paths: c.paths, first: c.first, changes: c.changes, at: None, jump: false });
        self.viewing = Some(index);
        self.zoom = None;
    }

    /// The comparison ended: the first drawing back in its place.
    pub(crate) fn end_compare(&mut self) {
        let Some(c) = self.model.cad.compare.take() else { return };
        let path = c.paths[0].clone();
        self.forget_pictures(&path);
        let s = &mut self.model.cad;
        s.layers.remove(&path);
        s.drawings.insert(path, c.first);
    }

    /// `path` shows a comparison.
    pub(crate) fn comparing(&self, path: &str) -> bool {
        self.model.cad.compare.as_ref().is_some_and(|c| c.paths[0] == path)
    }

    /// Each frame of the viewer: the comparison ends once another file is shown.
    pub(crate) fn compare_left(&mut self, current: usize) {
        if let Some(c) = &self.model.cad.compare
            && self.files().get(current).is_none_or(|f| f.path != c.paths[0])
        {
            self.end_compare();
        }
    }

    /// N and Shift+N ask for the next or previous change.
    pub(crate) fn compare_keys(&mut self, ctx: &egui::Context) {
        if self.model.cad.compare.is_none() {
            return;
        }
        let (next, back) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::N), i.consume_key(egui::Modifiers::SHIFT, egui::Key::N)));
        let Some(c) = &mut self.model.cad.compare else { return };
        let n = c.changes.len();
        if n > 0 && (next || back) {
            c.at = Some(match (c.at, next) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(i), true) => (i + 1) % n,
                (Some(i), false) => (i + n - 1) % n,
            });
            c.jump = true;
        }
    }

    /// The zoom and the centre (in pixels of the picture `tex` × of the drawing) that show the
    /// change asked for, in the viewer's `rect`; taken once.
    pub(crate) fn compare_jump(&mut self, tex: egui::Vec2, rect: egui::Rect, ppp: f32) -> Option<(f32, egui::Vec2)> {
        let c = self.model.cad.compare.as_mut().filter(|c| c.jump)?;
        c.jump = false;
        let b = *c.changes.get(c.at?)?;
        let d = self.model.cad.drawings.get(&c.paths[0])?;
        let place = d.placement(crate::decode::CAD_SIDE)?;
        // At least a tenth of the drawing, so a small change is seen with what is around it.
        let least = place.size[0].max(place.size[1]) / place.scale / 10.0;
        let mid = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        let (w, h) = ((b[2] - b[0]).max(least), (b[3] - b[1]).max(least));
        let (lo, hi) = (place.uv([mid[0] - w / 2.0, mid[1] + h / 2.0]), place.uv([mid[0] + w / 2.0, mid[1] - h / 2.0]));
        let px = |uv: [f64; 2]| egui::vec2(uv[0] as f32 * tex.x, uv[1] as f32 * tex.y);
        let (lo, hi) = (px(lo), px(hi));
        let zoom = (rect.width() * ppp / (hi.x - lo.x) / 2.0).min(rect.height() * ppp / (hi.y - lo.y) / 2.0);
        Some((zoom, (lo + hi) / 2.0))
    }

    /// For the status bar: what the drawing is compared with and its changes.
    pub(crate) fn compare_text(&self, current: usize) -> Option<String> {
        let c = self.compared(current)?;
        let name = crate::source::name(&c.paths[1]);
        let at = c.at.map_or(String::new(), |i| format!(" {}/{}", i + 1, c.changes.len()));
        Some(self.i18n.t_args("compare-status", &[("name", name), ("n", &c.changes.len().to_string())]) + &at)
    }

    /// For the status bar, as a warning: the two drawings have different units (or one has
    /// none), and are compared as they are, not converted.
    pub(crate) fn compare_units(&self, current: usize) -> Option<String> {
        let c = self.compared(current).filter(|c| c.units[0] != c.units[1])?;
        let unit = |u: Option<&str>| u.map_or_else(|| self.i18n.t("compare-no-unit"), str::to_string);
        let name = |i: usize| crate::source::name(&c.paths[i]).to_string();
        Some(self.i18n.t_args("compare-units", &[("first", &name(0)), ("a", &unit(c.units[0])), ("second", &name(1)), ("b", &unit(c.units[1]))]))
    }

    /// The comparison shown on `current`.
    fn compared(&self, current: usize) -> Option<&Comparison> {
        let c = self.model.cad.compare.as_ref()?;
        (self.files().get(current)?.path == c.paths[0]).then_some(c)
    }

    /// The pictures of `path` drawn with other layers or another drawing, forgotten.
    fn forget_pictures(&mut self, path: &str) {
        self.model.cad.forget(path);
    }
}

#[cfg(test)]
mod keys {
    use std::sync::Arc;

    use eframe::egui::{self, Modifiers};

    use crate::audit_family_1::{Harness, folder, own_folder};

    /// N pressed with `modifiers` held, read by the comparison's keys only; the change shown after.
    fn press(h: &mut Harness, modifiers: Modifiers) -> Option<usize> {
        let ev = |pressed| egui::Event::Key { key: egui::Key::N, physical_key: None, pressed, repeat: false, modifiers };
        let raw = egui::RawInput { events: vec![egui::Event::ModifiersChanged(modifiers), ev(true), ev(false)], ..Default::default() };
        let (ctx, app) = (h.ctx.clone(), &mut h.app);
        let _ = ctx.run_ui(raw, |ui| app.compare_keys(ui.ctx()));
        h.app.model.cad.compare.as_ref().and_then(|c| c.at)
    }

    /// #341: Shift+N in a comparison goes to the change before, N to the one after.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn shift_n_goes_back() {
        if own_folder("cad_compare_view::keys::shift_n_goes_back") {
            return;
        }
        let mut h = Harness::new(&folder("keys-341", &["a.png"]));
        let changes = vec![[0.0; 4]; 3];
        h.app.model.cad.compare = Some(super::Comparison { paths: ["a.dxf".into(), "b.dxf".into()], first: Arc::new(crate::cad::Drawing::default()), units: [None, None], changes, at: Some(1), jump: false });
        assert_eq!(press(&mut h, Modifiers::SHIFT), Some(0), "Shift+N did not go to the change before");
        assert_eq!(press(&mut h, Modifiers::NONE), Some(1), "N did not go to the change after");
    }
}
