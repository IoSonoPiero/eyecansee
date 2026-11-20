//! A CAD drawing in the viewer: zoomed and moved as an image (`viewer`), and once the view stops,
//! the visible part drawn again from the drawing at the screen's size (`decode::cad_render`), in
//! the background, and laid over the picture: thin lines and small texts stay sharp at any zoom.
//! L and Shift+L go through the model and the layouts (paper space), drawn when first shown.
//! With layers turned on or off in the panel (`cad_layers_panel`), the model too is drawn again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, TextureHandle};

use crate::App;
use crate::cad::Drawing;
use crate::mipmap::Picture;

/// Seconds without changes before drawing again.
const SETTLE: f64 = 0.2;

/// What is shown: the file, the part of the picture (pixels of `CAD_SIDE`) and the size on screen.
#[derive(Clone, PartialEq)]
struct Key {
    path: String,
    /// 0 the model, then the layouts.
    sheet: usize,
    region: [i32; 4],
    out: [u32; 2],
    /// The layers chosen (`State::layers`), by their count of changes.
    layers: u64,
}

/// A view drawn in the background: `None` if it could not be.
type Drawn = (Key, Option<egui::ColorImage>);
/// A whole sheet: path, sheet (0 the model), layers' count of changes.
type SheetKey = (String, usize, u64);
/// A sheet drawn in the background: `None` if it could not be.
type SheetDrawn = (SheetKey, Option<(egui::ColorImage, Vec<egui::ColorImage>)>);

#[derive(Default)]
pub(crate) struct State {
    /// Drawings read by the loader for the viewer, by path (`Msg::Drawing`).
    pub(crate) drawings: HashMap<String, Arc<Drawing>>,
    /// The sharp part on screen.
    sharp: Option<(Key, TextureHandle)>,
    /// The view waiting to be drawn, since when.
    waiting: Option<(Key, f64)>,
    /// The drawing under way and, when done, its result.
    job: Option<Key>,
    done: Arc<Mutex<Option<Drawn>>>,
    /// A view that could not be drawn: not asked for again.
    failed: Option<Key>,
    /// The sheet shown of the open drawing (0 the model), and the pictures of the layouts and of
    /// the model with other layers, drawn when first shown (`None` while being drawn), and the
    /// one that could not be.
    pub(crate) sheet: (String, usize),
    sheets: HashMap<SheetKey, Option<Picture>>,
    sheet_done: Arc<Mutex<Option<SheetDrawn>>>,
    sheet_failed: Option<SheetKey>,
    /// The layers shown, by drawing, as chosen in the panel during the session, with a count of
    /// the changes; drawings not here are shown as their file has them.
    pub(crate) layers: HashMap<String, (u64, Vec<bool>)>,
    /// The layers panel is open, and what it searches.
    pub(crate) panel: bool,
    pub(crate) search: String,
    /// The sheet shown with the layers chosen, kept while they stay so.
    chosen: Option<(SheetKey, Arc<Drawing>)>,
    /// Two drawings compared, shown in place of the first (`cad_compare_view`).
    pub(crate) compare: Option<crate::cad_compare_view::Comparison>,
}

impl State {
    /// The pictures drawn of `path` beyond the loader's (layouts, other layers, a comparison),
    /// forgotten when what it shows changes.
    pub(crate) fn forget(&mut self, path: &str) {
        self.sheets.retain(|k, _| k.0 != path);
        if self.chosen.as_ref().is_some_and(|(k, _)| k.0 == path) {
            self.chosen = None;
        }
        if self.sheet_failed.as_ref().is_some_and(|k| k.0 == path) {
            self.sheet_failed = None;
        }
        self.sharp = None;
        self.waiting = None;
        self.failed = None;
    }
}

impl App {
    /// Lays the sharp drawing over the picture shown in `dest` (the part `uv` of it), or asks for
    /// it once the view stops. Nothing for a turned picture (Image > Transform).
    pub(crate) fn cad_sharpen(&mut self, ui: &egui::Ui, current: usize, tex_size: egui::Vec2, dest: egui::Rect, uv: egui::Rect) {
        let Some(path) = self.files().get(current).map(|f| f.path.clone()) else { return };
        if !self.turn.is_none() {
            return;
        }
        let Some(drawing) = self.cad_drawn(&path) else { return };
        let sheet = self.cad_sheet(&path);
        let ppp = ui.ctx().pixels_per_point();
        let out = [(dest.width() * ppp).round() as u32, (dest.height() * ppp).round() as u32];
        let r = [uv.min.x * tex_size.x, uv.min.y * tex_size.y, uv.width() * tex_size.x, uv.height() * tex_size.y];
        let layers = self.cad_change(&path);
        let key = Key { path, sheet, region: r.map(|v| v.round() as i32), out, layers };
        let s = &mut self.model.cad;
        if let Some((k, img)) = s.done.lock().unwrap().take() {
            s.job = None;
            match img {
                Some(img) if k == key => s.sharp = Some((k, ui.ctx().load_texture("cad-sharp", img, egui::TextureOptions::LINEAR))),
                Some(_) => {}
                None => s.failed = Some(k),
            }
        }
        if s.failed.as_ref() == Some(&key) {
            return;
        }
        // Same view with other layers: the old sharp drawing stays until the new one is ready,
        // otherwise the faded picture underneath flashed for a moment.
        if let Some((k, tex)) = &s.sharp
            && (Key { layers: key.layers, ..k.clone() }) == key
        {
            crate::paint::paint_frame(&ui.painter_at(dest), tex.id(), dest, Default::default());
            if *k == key {
                return;
            }
        }
        let now = ui.input(|i| i.time);
        let since = match &s.waiting {
            Some((k, t)) if *k == key => *t,
            _ => {
                s.waiting = Some((key.clone(), now));
                now
            }
        };
        // The first view of a drawing or layout at once: its picture, 4096 pixels shown much smaller,
        // has thin lines faded or dotted, and the wait made them noticed.
        let first = s.sharp.as_ref().is_none_or(|(k, _)| k.path != key.path || k.sheet != key.sheet || k.layers != key.layers);
        if (now - since < SETTLE && !first) || s.job.is_some() || out[0] == 0 || out[1] == 0 {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SETTLE));
            return;
        }
        s.job = Some(key.clone());
        let (done, ctx) = (s.done.clone(), ui.ctx().clone());
        crate::logging::spawn_named("cad-render", move || {
            // Not drawn: the picture stays.
            let img = crate::decode::cad_render(&drawing, crate::decode::CAD_SIDE, Some((r, out)))
                .ok()
                .map(|img| egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()));
            *done.lock().unwrap() = Some((key, img));
            ctx.request_repaint();
        });
    }

    /// The sheet shown of a drawing: 0 the model, then the layouts.
    pub(crate) fn cad_sheet(&self, path: &str) -> usize {
        let (p, i) = &self.model.cad.sheet;
        if p == path { *i } else { 0 }
    }

    /// The drawing of the sheet shown, with the layers of the file.
    fn cad_shown(&self, path: &str) -> Option<Arc<Drawing>> {
        let d = self.model.cad.drawings.get(path)?;
        match self.cad_sheet(path) {
            0 => Some(d.clone()),
            i => d.sheets.get(i - 1).map(|(_, s)| s.clone()),
        }
    }

    /// How many times the layers of the drawing were changed in the panel: 0 as in the file.
    pub(crate) fn cad_change(&self, path: &str) -> u64 {
        self.model.cad.layers.get(path).map_or(0, |(n, _)| *n)
    }

    /// The drawing of the sheet shown, with the layers chosen in the panel.
    pub(crate) fn cad_drawn(&mut self, path: &str) -> Option<Arc<Drawing>> {
        let shown = self.cad_shown(path)?;
        let Some((n, on)) = self.model.cad.layers.get(path).map(|(n, on)| (*n, on.clone())) else { return Some(shown) };
        let key = (path.to_string(), self.cad_sheet(path), n);
        let s = &mut self.model.cad;
        match &s.chosen {
            Some((k, d)) if *k == key => Some(d.clone()),
            _ => {
                let d = Arc::new(shown.with_layers(&on));
                s.chosen = Some((key, d.clone()));
                Some(d)
            }
        }
    }

    /// For the status bar: the size of the drawing or sheet shown, in its unit.
    pub(crate) fn cad_size_text(&self, current: usize) -> Option<String> {
        let d = self.cad_shown(&self.files().get(current)?.path)?;
        let (w, h) = d.size()?;
        let unit = d.unit.map_or_else(|| self.i18n.t("cad-units"), |u| u.symbol.to_string());
        let (w, h, unit) = crate::cad::readable(w, h, &unit);
        Some(crate::cad::size_text(w, h, unit, &self.i18n.t("decimal-separator")))
    }

    /// For the status bar: the name of the sheet shown, when the drawing has layouts.
    pub(crate) fn cad_sheet_name(&self, current: usize) -> Option<String> {
        let path = &self.files().get(current)?.path;
        let d = self.model.cad.drawings.get(path)?;
        if d.sheets.is_empty() {
            return None;
        }
        let i = self.cad_sheet(path);
        let name = if i == 0 { self.i18n.t("cad-model") } else { d.sheets[i - 1].0.clone() };
        Some(format!("{name} ({}/{})", i + 1, d.sheets.len() + 1))
    }

    /// The drawing has layouts to go through with L.
    pub(crate) fn cad_has_sheets(&self, current: usize) -> bool {
        self.files().get(current).and_then(|f| self.model.cad.drawings.get(&f.path)).is_some_and(|d| !d.sheets.is_empty())
    }

    /// L the next sheet, Shift+L the one before; the view starts fitted. Ctrl+L the layers.
    pub(crate) fn cad_keys(&mut self, ctx: &egui::Context, current: usize) {
        self.compare_keys(ctx);
        // Ctrl+L the layers panel, open and closed.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L)) && self.cad_layers_off().is_none() {
            self.model.cad.panel = !self.model.cad.panel;
        }
        let Some(path) = self.files().get(current).map(|f| f.path.clone()) else { return };
        let Some(count) = self.model.cad.drawings.get(&path).map(|d| d.sheets.len() + 1).filter(|&n| n > 1) else { return };
        let (next, back) = ctx.input(|i| (i.key_pressed(egui::Key::L) && !i.modifiers.command && !i.modifiers.shift, i.key_pressed(egui::Key::L) && i.modifiers.shift));
        if next || back {
            let i = self.cad_sheet(&path);
            self.model.cad.sheet = (path, if next { (i + 1) % count } else { (i + count - 1) % count });
            self.zoom = None;
        }
    }

    /// A layout is chosen and no picture of it is drawn yet.
    pub(crate) fn cad_sheet_waiting(&self, current: usize) -> bool {
        let Some(path) = self.files().get(current).map(|f| f.path.clone()) else { return false };
        let sheet = self.cad_sheet(&path);
        sheet > 0 && self.cad_earlier(&(path, sheet, 0)).is_none()
    }

    /// The picture of the sheet, with any choice of layers, drawn last: shown while the one with
    /// the layers chosen now is drawn.
    fn cad_earlier(&self, key: &SheetKey) -> Option<Picture> {
        let s = &self.model.cad;
        s.sheets.iter().filter(|(k, t)| k.0 == key.0 && k.1 == key.1 && t.is_some()).max_by_key(|(k, _)| k.2).and_then(|(_, t)| t.clone())
    }

    /// The picture of the sheet shown in place of the one the loader read: a layout, or the model
    /// with the layers chosen in the panel; drawn in the background the first time. `None` for
    /// the model as in the file, or while the first picture of a layout is being drawn.
    pub(crate) fn cad_sheet_texture(&mut self, ctx: &egui::Context, current: usize) -> Option<Picture> {
        let path = self.files().get(current)?.path.clone();
        let key = (path.clone(), self.cad_sheet(&path), self.cad_change(&path));
        if (key.1, key.2) == (0, 0) || self.model.cad.sheet_failed.as_ref() == Some(&key) {
            return None;
        }
        let s = &mut self.model.cad;
        if let Some((done, img)) = s.sheet_done.lock().unwrap().take() {
            match img {
                Some((img, halves)) => {
                    let tex = Picture::new(ctx, &format!("cad-sheet-{}", done.1), img, halves);
                    // The pictures with the layers chosen before are no longer needed.
                    s.sheets.retain(|k, _| k.0 != done.0 || k.1 != done.1 || k.2 >= done.2);
                    s.sheets.insert(done, Some(tex));
                }
                // Not drawn: a layout goes back to the model, the model stays as the file has it.
                None => {
                    s.sheets.remove(&done);
                    if done.1 > 0 {
                        s.sheet = (done.0.clone(), 0);
                    }
                    s.sheet_failed = Some(done);
                    return None;
                }
            }
        }
        match s.sheets.get(&key) {
            Some(Some(tex)) => return Some(tex.clone()),
            Some(None) => return self.cad_earlier(&key),
            None => {}
        }
        let drawing = self.cad_drawn(&path)?;
        let s = &mut self.model.cad;
        s.sheets.insert(key.clone(), None);
        let (done, ctx) = (s.sheet_done.clone(), ctx.clone());
        let earlier = self.cad_earlier(&key);
        crate::logging::spawn_named("cad-sheet", move || {
            let img = crate::decode::cad_render(&drawing, crate::decode::CAD_SIDE, None)
                .ok()
                .map(|img| egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
                .map(|img| {
                    let halves = crate::mipmap::picture_halves(&img);
                    (img, halves)
                });
            *done.lock().unwrap() = Some((key, img));
            ctx.request_repaint();
        });
        earlier
    }
}

impl App {
    /// "Save in another format" from the viewer on a layout, or on a drawing with layers chosen in
    /// the panel: what is shown. `false` for the model as the file has it.
    pub(crate) fn convert_sheet(&mut self, path: &str) -> bool {
        if self.viewing.is_none() || (self.cad_sheet(path) == 0 && self.cad_change(path) == 0) {
            return false;
        }
        let Some(drawing) = self.cad_drawn(path) else { return false };
        let Some(loader) = &self.loader else { return false };
        let size = self.files().iter().find(|f| f.path == path).map_or(0, |f| f.size);
        loader.drawing_original(drawing, size);
        true
    }
}

/// The axes of a drawing in the bottom left corner, as AutoCAD's UCS icon: X to the right in red,
/// Y up in green (drawings are seen from above).
pub(crate) fn ucs_corner(painter: &egui::Painter, rect: egui::Rect) {
    const LEN: f32 = 40.0;
    let o = rect.left_bottom() + egui::vec2(28.0, -28.0);
    for (dir, color, label) in [(egui::vec2(1.0, 0.0), egui::Color32::from_rgb(235, 80, 80), "X"), (egui::vec2(0.0, -1.0), egui::Color32::from_rgb(100, 210, 100), "Y")] {
        let tip = o + dir * LEN;
        painter.arrow(o, dir * LEN, egui::Stroke::new(2.5, color));
        painter.text(tip + dir * 10.0, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(14.0), color);
    }
    painter.rect_filled(egui::Rect::from_center_size(o, egui::vec2(7.0, 7.0)), 0.0, egui::Color32::from_gray(220));
}
