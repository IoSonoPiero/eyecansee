//! Slideshow: the chosen images one after the other at full screen, on black, with a passage
//! between them chosen in the options. Esc ends it, Space pauses, the arrows change image.

use eframe::egui::{self, Color32, Key, Rect, TextureHandle, Vec2};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::i18n::I18n;
use crate::settings::Settings;

/// How one image gives way to the next.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum Fade {
    /// Straight cut.
    None,
    #[default]
    Cross,
    /// The old one fades to black, then the new one comes up.
    Black,
    /// The new one pushes the old one out to the left.
    Slide,
    /// The new one fades in growing from a little smaller.
    Zoom,
}

impl Fade {
    pub const ALL: [Fade; 5] = [Fade::None, Fade::Cross, Fade::Black, Fade::Slide, Fade::Zoom];

    /// Fluent id of the name.
    pub fn label(self) -> &'static str {
        match self {
            Fade::None => "fade-none",
            Fade::Cross => "fade-cross",
            Fade::Black => "fade-black",
            Fade::Slide => "fade-slide",
            Fade::Zoom => "fade-zoom",
        }
    }
}

/// Seconds a passage lasts (never more than half the time of an image).
const FADE_SECS: f64 = 0.8;

/// The slideshow under way. Images by path: a folder read again meanwhile changes the positions.
pub struct Show {
    paths: Vec<String>,
    at: usize,
    /// When the image at `at` started to come in.
    since: f64,
    /// The image it comes from, during the passage.
    from: Option<String>,
    /// Gone back with the arrows: the slide runs the other way.
    back: bool,
    paused: bool,
    /// The window was at full screen before.
    was_full: bool,
    /// The viewer was open: it comes back on the last image shown.
    from_viewer: bool,
    /// When it started: the keys are shown for the first seconds.
    started: f64,
}

/// The next position after `at` among `n` (backwards when `!forward`); `None` past the end
/// without `again`.
pub fn step(at: usize, n: usize, forward: bool, again: bool) -> Option<usize> {
    match (forward, at) {
        (true, a) if a + 1 < n => Some(a + 1),
        (false, a) if a > 0 => Some(a - 1),
        _ if !again || n == 0 => None,
        (true, _) => Some(0),
        (false, _) => Some(n - 1),
    }
}

/// How far the passage is, 0 to 1.
pub fn passage(now: f64, since: f64, secs: f64) -> f32 {
    ((now - since) / FADE_SECS.min(secs / 2.0)).clamp(0.0, 1.0) as f32
}

impl App {
    /// From the menus: the marked images from the first, or every image of the folder from the
    /// chosen one.
    pub(crate) fn start_show(&mut self, ctx: &egui::Context, path: Option<String>) {
        let files: Vec<String> = self.files().iter().map(|f| f.path.clone()).collect();
        let chosen = self.grid_images(path.clone());
        let current = path.or_else(|| self.viewing.and_then(|i| files.get(i).cloned())).or_else(|| chosen.first().cloned());
        let paths: Vec<String> = if chosen.len() > 1 { files.iter().filter(|p| chosen.contains(p)).cloned().collect() } else { files };
        if paths.is_empty() {
            return;
        }
        // With several selected always from the first of them, as Open (also from the right
        // click on another one); otherwise from the image chosen or open.
        let at = if chosen.len() > 1 { 0 } else { current.and_then(|c| paths.iter().position(|p| *p == c)).unwrap_or(0) };
        log!("slideshow: {} images from {at}", paths.len());
        let was_full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        let now = ctx.input(|i| i.time);
        self.show = Some(Show { paths, at, since: now, from: None, back: false, paused: false, was_full, from_viewer: self.viewing.is_some(), started: now });
        self.viewing = None;
    }

    fn end_show(&mut self, ctx: &egui::Context) {
        let Some(s) = self.show.take() else { return };
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(s.was_full));
        let index = self.files().iter().position(|f| f.path == s.paths[s.at]);
        if let (Some(i), Some(Ok(l))) = (index, self.listings.get(&self.dir)) {
            if s.from_viewer {
                self.viewing = Some(i);
            } else {
                self.selected = l.dirs.len() + i;
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
        self.full.clear();
        self.pending.clear();
    }

    /// Drawn in place of the whole interface.
    pub(crate) fn show_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.show_keys(&ctx, now);
        let Some(s) = &self.show else { return };
        let secs = f64::from(self.settings.slideshow_secs.max(1.0));
        let index = |p: &String| self.files().iter().position(|f| f.path == *p);
        let (cur, from) = (index(&s.paths[s.at]), s.from.as_ref().and_then(index));
        let near = |forward| step(s.at, s.paths.len(), forward, self.settings.slideshow_loop).and_then(|n| index(&s.paths[n]));
        // The previous one too: going back with the arrows found it not loaded, a black flash.
        let (next, prev) = (near(true), near(false));
        let since = s.since;
        self.show_load([cur, next, prev, from].into_iter().flatten().collect());
        let tex = |i: Option<usize>| i.and_then(|i| self.full.get(&i)).and_then(|r| r.as_ref().ok()).cloned();
        let (cur_tex, from_tex) = (tex(cur), tex(from));
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        let t = passage(now, since, secs);
        if let Some(cur_tex) = &cur_tex {
            let back = self.show.as_ref().is_some_and(|s| s.back);
            paint_passage(ui.painter(), rect, from_tex.as_ref(), cur_tex, (self.settings.slideshow_fade, back), t);
        } else if let Some(from_tex) = &from_tex {
            // Arrows pressed faster than the images load: the old one stays meanwhile.
            paint_passage(ui.painter(), rect, None, from_tex, (Fade::None, false), 1.0);
        } else if cur.is_some_and(|i| matches!(self.full.get(&i), Some(Err(_)))) {
            // Cannot be read: the next one comes at its time.
        } else {
            ui.put(rect, egui::Spinner::new().color(Color32::WHITE));
        }
        self.show_hints(ui, rect, now);
        let next_ready = next.is_some_and(|i| self.full.contains_key(&i));
        let s = self.show.as_mut().unwrap();
        if t >= 1.0 {
            s.from = None;
        }
        if !s.paused && now - s.since >= secs && (next_ready || next.is_none()) {
            self.show_go(&ctx, true, now);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(if t < 1.0 { 16 } else { 100 }));
    }

    /// The images needed now are asked for; the others are let go.
    fn show_load(&mut self, keep: Vec<usize>) {
        let (Some(loader), Some(Ok(listing))) = (&self.loader, self.listings.get(&self.dir)) else { return };
        for &i in &keep {
            if !self.full.contains_key(&i) && self.pending.insert(i) {
                loader.load_full(self.generation, i, listing.files[i].path.clone());
            }
        }
        self.full.retain(|i, _| keep.contains(i));
    }

    fn show_keys(&mut self, ctx: &egui::Context, now: f64) {
        let (esc, space, next, prev, double) = ctx.input(|i| {
            let any = |keys: &[Key]| keys.iter().any(|k| i.key_pressed(*k));
            (
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Space),
                any(&[Key::ArrowRight, Key::PageDown]),
                any(&[Key::ArrowLeft, Key::PageUp]),
                i.pointer.button_double_clicked(egui::PointerButton::Primary),
            )
        });
        if esc || double {
            self.end_show(ctx);
            return;
        }
        if let Some(s) = &mut self.show
            && space
        {
            s.paused = !s.paused;
        }
        if next || prev {
            self.show_go(ctx, next, now);
        }
    }

    /// To the next image (or the previous one), with the passage; past the last one without
    /// "start again" the slideshow ends.
    fn show_go(&mut self, ctx: &egui::Context, forward: bool, now: f64) {
        let again = self.settings.slideshow_loop;
        let Some(s) = &mut self.show else { return };
        match step(s.at, s.paths.len(), forward, again) {
            Some(at) => {
                s.from = Some(s.paths[s.at].clone());
                s.back = !forward;
                s.at = at;
                s.since = now;
            }
            None if forward => self.end_show(ctx),
            None => {}
        }
    }

    /// The keys, for the first seconds; "Paused" while paused.
    fn show_hints(&self, ui: &egui::Ui, rect: Rect, now: f64) {
        let Some(s) = &self.show else { return };
        let text = if s.paused {
            Some(self.i18n.t("show-paused"))
        } else {
            (now - s.started < 4.0).then(|| self.i18n.t("show-hint"))
        };
        if let Some(text) = text {
            let pos = rect.center_bottom() - Vec2::new(0.0, 40.0);
            let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(16.0), Color32::WHITE);
            let back = Rect::from_center_size(pos, galley.size() + Vec2::new(24.0, 12.0));
            ui.painter().rect_filled(back, 6.0, Color32::from_black_alpha(170));
            ui.painter().galley(back.min + Vec2::new(12.0, 6.0), galley, Color32::WHITE);
        }
    }
}

/// `tex` fitted into `rect`, never larger than its pixels, scaled by `scale` around the centre
/// and moved by `dx` points.
fn fitted(rect: Rect, tex: &TextureHandle, ppp: f32, scale: f32, dx: f32) -> Rect {
    let size = tex.size_vec2() / ppp;
    let fit = (rect.width() / size.x).min(rect.height() / size.y).min(1.0) * scale;
    Rect::from_center_size(rect.center() + Vec2::new(dx, 0.0), size * fit)
}

/// The passage at `t` (0 to 1) from `from` to `to`; `back`: going to the previous image.
fn paint_passage(p: &egui::Painter, rect: Rect, from: Option<&TextureHandle>, to: &TextureHandle, (fade, back): (Fade, bool), t: f32) {
    let ppp = p.ctx().pixels_per_point();
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let draw = |tex: &TextureHandle, scale: f32, dx: f32, alpha: f32| {
        p.image(tex.id(), fitted(rect, tex, ppp, scale, dx), uv, Color32::WHITE.gamma_multiply(alpha.clamp(0.0, 1.0)));
    };
    let Some(from) = from.filter(|_| t < 1.0 && fade != Fade::None) else {
        draw(to, 1.0, 0.0, 1.0);
        return;
    };
    // The old one under the new one stays almost whole: two half transparent images over black
    // made the middle of the passage darker. It goes only at the end, where the new one does not
    // cover it (images of other proportions).
    let under = 1.0 - t.powi(4);
    match fade {
        Fade::Cross => {
            draw(from, 1.0, 0.0, under);
            draw(to, 1.0, 0.0, t);
        }
        Fade::Black if t < 0.5 => draw(from, 1.0, 0.0, 1.0 - 2.0 * t),
        Fade::Black => draw(to, 1.0, 0.0, 2.0 * t - 1.0),
        Fade::Slide => {
            // Eased: fast at first, gentle at the end.
            // Forward the new one comes from the right, back from the left.
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            let w = if back { -rect.width() } else { rect.width() };
            draw(from, 1.0, -e * w, 1.0);
            draw(to, 1.0, (1.0 - e) * w, 1.0);
        }
        Fade::Zoom => {
            draw(from, 1.0, 0.0, under);
            draw(to, 0.9 + 0.1 * t, 0.0, t);
        }
        Fade::None => draw(to, 1.0, 0.0, 1.0),
    }
}

/// The tab of the options.
pub fn options_ui(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.horizontal(|ui| {
        ui.label(t("opt-show-secs"));
        ui.add(egui::DragValue::new(&mut s.slideshow_secs).range(1.0..=600.0).speed(0.2).fixed_decimals(0).suffix(" s"));
    });
    ui.add_space(6.0);
    ui.label(t("opt-show-fade"));
    for fade in Fade::ALL {
        ui.radio_value(&mut s.slideshow_fade, fade, t(fade.label()));
    }
    ui.add_space(6.0);
    ui.checkbox(&mut s.slideshow_loop, t("opt-show-loop"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_passage() {
        assert_eq!(step(0, 3, true, false), Some(1));
        assert_eq!(step(2, 3, true, false), None);
        assert_eq!(step(2, 3, true, true), Some(0));
        assert_eq!(step(0, 3, false, false), None);
        assert_eq!(step(0, 3, false, true), Some(2));
        assert_eq!(step(1, 3, false, true), Some(0));
        assert_eq!(step(0, 1, true, true), Some(0));
        assert_eq!(step(0, 0, true, true), None);
        assert_eq!(passage(10.0, 10.0, 5.0), 0.0);
        assert_eq!(passage(10.4, 10.0, 5.0), 0.5);
        assert_eq!(passage(12.0, 10.0, 5.0), 1.0);
        // A short time per image: the passage takes half of it at most.
        assert_eq!(passage(10.5, 10.0, 1.0), 1.0);
    }
}
