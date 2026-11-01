//! A CAD drawing in the viewer: zoomed and moved as an image (`viewer`), and once the view stops,
//! the visible part drawn again from the drawing at the screen's size (`decode::cad_render`), in
//! the background, and laid over the picture: thin lines and small texts stay sharp at any zoom.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, TextureHandle};

use crate::App;
use crate::cad::Drawing;

/// Seconds without changes before drawing again.
const SETTLE: f64 = 0.2;

/// What is shown: the file, the part of the picture (pixels of `CAD_SIDE`) and the size on screen.
#[derive(Clone, PartialEq)]
struct Key {
    path: String,
    region: [i32; 4],
    out: [u32; 2],
}

/// A view drawn in the background: `None` if it could not be.
type Drawn = (Key, Option<egui::ColorImage>);

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
}

impl App {
    /// Lays the sharp drawing over the picture shown in `dest` (the part `uv` of it), or asks for
    /// it once the view stops. Nothing for a turned picture (Image > Transform).
    pub(crate) fn cad_sharpen(&mut self, ui: &egui::Ui, current: usize, tex_size: egui::Vec2, dest: egui::Rect, uv: egui::Rect) {
        let Some(path) = self.files().get(current).map(|f| f.path.clone()) else { return };
        let Some(drawing) = self.model.cad.drawings.get(&path).cloned() else { return };
        if !self.turn.is_none() {
            return;
        }
        let ppp = ui.ctx().pixels_per_point();
        let out = [(dest.width() * ppp).round() as u32, (dest.height() * ppp).round() as u32];
        let r = [uv.min.x * tex_size.x, uv.min.y * tex_size.y, uv.width() * tex_size.x, uv.height() * tex_size.y];
        let key = Key { path, region: r.map(|v| v.round() as i32), out };
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
        if let Some((k, tex)) = &s.sharp
            && *k == key
        {
            let full = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter_at(dest).image(tex.id(), dest, full, egui::Color32::WHITE);
            return;
        }
        let now = ui.input(|i| i.time);
        let since = match &s.waiting {
            Some((k, t)) if *k == key => *t,
            _ => {
                s.waiting = Some((key.clone(), now));
                now
            }
        };
        if now - since < SETTLE || s.job.is_some() || out[0] == 0 || out[1] == 0 {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SETTLE));
            return;
        }
        s.job = Some(key.clone());
        let (done, ctx) = (s.done.clone(), ui.ctx().clone());
        std::thread::spawn(move || {
            // Not drawn: the picture stays.
            let img = crate::decode::cad_render(&drawing, crate::decode::CAD_SIDE, Some((r, out)))
                .ok()
                .map(|img| egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()));
            *done.lock().unwrap() = Some((key, img));
            ctx.request_repaint();
        });
    }
}
