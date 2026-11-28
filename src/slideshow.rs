//! Slideshow: the chosen images one after the other at full screen, on black, with a passage
//! between them chosen in the options. Esc ends it, Space pauses, the arrows change image.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Key, Rect, TextureHandle, Vec2};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::i18n::I18n;
use crate::mipmap::Picture;
use crate::selection::{PLAYED, kind};
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

/// Seconds the notice of a picture passed over stays in its corner.
const SKIPPED_SECS: f64 = 4.0;

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
    /// The pictures that cannot be read: passed over, also by the look-ahead (`next_shown`).
    unreadable: HashSet<String>,
    /// A picture or a video was shown: with none, the end is "none of them opens".
    shown: bool,
    /// The last picture passed over, and when: said small in a corner for a few seconds
    /// (`SKIPPED_SECS`). The error bar over the pictures covered them (Piero, 20/11).
    skipped: Option<(String, f64)>,
    /// Where its images are in the folder's listing (`Places`).
    places: Places,
}

/// Where the images of the slideshow are in the folder's listing, looked for at every frame.
#[derive(Default)]
pub(crate) struct Places;

impl Places {
    /// Where `path` is among `n` files, `at(i)` the path of the `i`-th.
    pub(crate) fn find<'a>(&self, path: &str, n: usize, at: impl Fn(usize) -> &'a str) -> Option<usize> {
        (0..n).position(|i| at(i) == path)
    }
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

/// The next position after `at` that is not known to be unreadable (`bad`), as `step` goes;
/// `None` past the end without `again`, or when every one is.
pub fn next_shown(at: usize, n: usize, forward: bool, again: bool, bad: impl Fn(usize) -> bool) -> Option<usize> {
    let mut i = at;
    for _ in 0..n {
        i = step(i, n, forward, again)?;
        if !bad(i) {
            return Some(i);
        }
    }
    None
}

/// Where the slideshow goes from a picture that cannot be read.
#[derive(Debug, PartialEq)]
pub enum Skip {
    /// To this position, going forward (`true`) or back.
    To(usize, bool),
    /// Past the last one without "start again": it ends, as after a picture shown.
    End,
    /// None of them can be read: it stops, and says so.
    NoneReadable,
}

/// From the unreadable picture at `at` (`bad`: those known not to open) to the next one the same
/// way; going back past the first without `again`, forward instead (the one it came from). Every
/// one unreadable, or the end without any shown (`shown`): `NoneReadable`, never round and round.
pub fn past_unreadable(at: usize, n: usize, forward: bool, again: bool, bad: impl Fn(usize) -> bool, shown: bool) -> Skip {
    if (0..n).all(&bad) {
        return Skip::NoneReadable;
    }
    if let Some(i) = next_shown(at, n, forward, again, &bad) {
        return Skip::To(i, forward);
    }
    match (forward, next_shown(at, n, true, again, &bad)) {
        (false, Some(i)) => Skip::To(i, true),
        _ if shown => Skip::End,
        _ => Skip::NoneReadable,
    }
}

/// How far the passage is, 0 to 1.
pub fn passage(now: f64, since: f64, secs: f64) -> f32 {
    ((now - since) / FADE_SECS.min(secs / 2.0)).clamp(0.0, 1.0) as f32
}

/// When the picture shown started (`since`), now that it is `reading` or not: its time and its
/// passage start once it is there. A picture opened anyway before is read only once shown (never
/// prepared in advance, `view_memory::Wanted`): it was passed over while still being read.
pub fn started(since: f64, now: f64, reading: bool) -> f64 {
    if reading { now } else { since }
}

/// A picture or a video: 3D models, CAD drawings and audio files stay out of the slideshow
/// (looked at closely, not in passing).
pub fn in_show(path: &str) -> bool {
    PLAYED.contains(&kind(path))
}

/// The slideshow's files and where it starts: the pictures and videos among `chosen` when there
/// are several, from the first; otherwise those of the whole folder (`files`), from `current`,
/// or from the first when `current` is not one of them.
pub fn show_list(files: &[String], chosen: &[String], current: Option<&str>) -> (Vec<String>, usize) {
    let several = chosen.iter().filter(|p| in_show(p)).count() > 1;
    let paths: Vec<String> = files.iter().filter(|p| in_show(p) && (!several || chosen.contains(p))).cloned().collect();
    let at = if several { 0 } else { current.and_then(|c| paths.iter().position(|p| p == c)).unwrap_or(0) };
    (paths, at)
}

/// Does a thumbnail's `error` say that its picture cannot be opened? Only the decoder's own
/// verdict does: not a picture too large for the memory (#177, the viewer may open it anyway), one
/// not tried (`thumb_queue::NOT_TRIED`: a file not read, maybe later), one no longer wanted, an
/// engine to download.
pub fn thumb_says_unreadable(error: &str) -> bool {
    let not_said = [crate::thumb_memory::TOO_LARGE, crate::thumb_queue::NOT_WANTED, crate::video_read::NO_ENGINE, crate::occt::MISSING];
    !not_said.contains(&error) && !error.starts_with(crate::thumb_queue::NOT_TRIED)
}

/// Is the slideshow's picture at `path` already known not to open, from the viewer (`full`) or its
/// thumbnail (`thumb`)? What the viewer read wins: opened, it opens; an error counts as in the
/// slideshow (`show_pass_unreadable`), not one too large for the memory. A video only when mpv
/// could not play its still at all (Piero, 22/11: a slideshow of broken videos does not start
/// either); not a still that ran out of time (a slow phone), not one waiting for the video engine,
/// not a sound file (its picture is its cover). Nothing read yet is not known.
pub fn known_unreadable(path: &str, full: Option<Result<(), &str>>, thumb: Option<Result<(), &str>>) -> bool {
    if crate::decode::is_played(path) {
        return crate::decode::is_video(path) && matches!(thumb, Some(Err(e)) if e == crate::mpv::CANNOT_PLAY);
    }
    match full {
        Some(Ok(())) => false,
        Some(Err(e)) if crate::view_memory::need_of(e).is_none() => true,
        _ => matches!(thumb, Some(Err(e)) if thumb_says_unreadable(e)),
    }
}

/// Every one of `paths` already known not to open (`bad`): the slideshow does not start (Piero,
/// 21/11/2026: it went to full screen for an instant, only to stop).
pub fn none_opens(paths: &[String], bad: impl Fn(&str) -> bool) -> bool {
    !paths.is_empty() && paths.iter().all(|p| bad(p))
}

/// How a picture was read (`App::full`, `App::thumbs`), without the picture.
fn outcome(read: Option<&Result<Picture, String>>) -> Option<Result<(), &str>> {
    read.map(|r| r.as_ref().map(|_| ()).map_err(String::as_str))
}

impl App {
    /// The picture at `path` (one of `files`) already known not to open (`known_unreadable`).
    fn known_unreadable_in(&self, files: &[String], path: &str) -> bool {
        let Some(i) = files.iter().position(|f| f == path) else { return false };
        known_unreadable(path, outcome(self.full.get(&i)), outcome(self.thumbs.get(&i)))
    }

    /// From the menus: the marked pictures and videos from the first, or every picture and video
    /// of the folder from the chosen one.
    pub(crate) fn start_show(&mut self, ctx: &egui::Context, path: Option<String>) {
        let files: Vec<String> = self.files().iter().map(|f| f.path.clone()).collect();
        let chosen = self.grid_of(path.clone(), PLAYED);
        let current = path.or_else(|| self.viewing.and_then(|i| files.get(i).cloned())).or_else(|| chosen.first().cloned());
        let (paths, at) = show_list(&files, &chosen, current.as_deref());
        if paths.is_empty() {
            // View > Slideshow is off then; from elsewhere nothing happens.
            log!("slideshow: no pictures or videos");
            return;
        }
        // The grid shows them all broken: said at once, without going to full screen.
        if none_opens(&paths, |p| self.known_unreadable_in(&files, p)) {
            log!("slideshow: none of its {} images opens, not started", paths.len());
            self.error = Some(self.i18n.t("show-none-readable"));
            return;
        }
        log!("slideshow: {} images from {at}", paths.len());
        let was_full = crate::viewer::full_screen(ctx);
        if !was_full {
            crate::viewer::enter_full(ctx);
        }
        let now = ctx.input(|i| i.time);
        let from_viewer = self.viewing.is_some();
        self.show = Some(Show { paths, at, since: now, from: None, back: false, paused: false, was_full, from_viewer, started: now, unreadable: HashSet::new(), shown: false, skipped: None, places: Places::default() });
        self.viewing = None;
    }

    fn end_show(&mut self, ctx: &egui::Context) {
        let Some(s) = self.show.take() else { return };
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(s.was_full));
        let files = self.files();
        let index = s.places.find(&s.paths[s.at], files.len(), |i| &files[i].path);
        if let (Some(i), Some(Ok(l))) = (index, self.listings.get(&self.dir)) {
            if s.from_viewer {
                self.viewing = Some(i);
            } else {
                self.selected = l.dirs.len() + i;
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
        self.video = None;
        self.full.clear();
        self.pending.clear();
    }

    /// Drawn in place of the whole interface.
    pub(crate) fn show_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.show_keys(&ctx, now);
        self.show_pass_unreadable(&ctx, now);
        let Some(s) = &self.show else { return };
        let secs = f64::from(self.settings.slideshow_secs.max(1.0));
        let files = self.files();
        let index = |p: &String| s.places.find(p, files.len(), |i| &files[i].path);
        let (cur, from) = (index(&s.paths[s.at]), s.from.as_ref().and_then(index));
        let bad = |i: usize| s.unreadable.contains(&s.paths[i]);
        let near = |forward| next_shown(s.at, s.paths.len(), forward, self.settings.slideshow_loop, bad).and_then(|n| index(&s.paths[n]));
        // The previous one too: going back with the arrows found it not loaded, a black flash.
        let (next, prev) = (near(true), near(false));
        let since = s.since;
        self.show_load(cur, [cur, next, prev, from].into_iter().flatten().collect());
        let tex = |i: Option<usize>| i.and_then(|i| self.full.get(&i)).and_then(|r| r.as_ref().ok()).cloned();
        let (cur_tex, from_tex) = (tex(cur), tex(from));
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        let t = passage(now, since, secs);
        // A video plays whole, after the passage that brought its still in.
        let video = cur.and_then(|i| self.files().get(i)).filter(|f| crate::decode::is_played(&f.path)).map(|f| (f.path.clone(), f.size));
        // The still of a video fills the screen as the video does: at its own pixels, a 720p one
        // came in small on a larger screen and jumped to the whole screen as it started to play.
        let from_video = from.and_then(|i| self.files().get(i)).is_some_and(|f| crate::decode::is_played(&f.path));
        let (mut played, mut ended) = (false, false);
        match &video {
            Some((path, size)) if t >= 1.0 => (played, ended) = self.show_video(ui.painter(), rect, path, *size),
            Some(_) => {}
            None => self.video = None,
        }
        if played {
            // the video's frame is on screen
        } else if let Some(cur_tex) = &cur_tex {
            let back = self.show.as_ref().is_some_and(|s| s.back);
            paint_passage(ui.painter(), rect, from_tex.as_ref().map(|f| (f, from_video)), (cur_tex, video.is_some()), (self.settings.slideshow_fade, back), t);
        } else if let Some(from_tex) = &from_tex {
            // Arrows pressed faster than the images load: the old one stays meanwhile.
            paint_passage(ui.painter(), rect, None, (from_tex, from_video), (Fade::None, false), 1.0);
        } else if cur.is_some_and(|i| matches!(self.full.get(&i), Some(Err(_)))) {
            // The still of a video that cannot be read: the video comes after the passage.
        } else {
            ui.put(rect, egui::Spinner::new().color(Color32::WHITE));
        }
        self.show_hints(ui, rect, now);
        self.show_skipped(ui, rect, now);
        let next_ready = next.is_some_and(|i| self.full.contains_key(&i));
        let reading = video.is_none() && cur.is_some_and(|i| !self.full.contains_key(&i));
        let s = self.show.as_mut().unwrap();
        s.since = started(s.since, now, reading);
        s.shown |= played || cur_tex.is_some();
        // Until the new one is there the old one stays (after a picture passed over too).
        if t >= 1.0 && (played || cur_tex.is_some()) {
            s.from = None;
        }
        let over = if video.is_some() { ended } else { now - s.since >= secs };
        if !s.paused && over && (next_ready || next.is_none()) {
            self.show_go(&ctx, true, now);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(if t < 1.0 { 16 } else { 100 }));
    }

    /// The pictures found unreadable (among those loaded: the current one and its neighbours) are
    /// remembered; the current one, if it is one, is passed over at once, with a notice in a corner
    /// (`show_skipped`). A video whose still cannot be read is left to play; one that cannot be
    /// played is passed over as a picture (Piero, 20/11, #209).
    fn show_pass_unreadable(&mut self, ctx: &egui::Context, now: f64) {
        // One opened anyway before is not: as a neighbour it is not read (`view_memory::Wanted`).
        let mut failed: Vec<String> = self.full.iter().filter(|&(i, r)| r.is_err() && !self.remembered_too_large(*i)).filter_map(|(i, _)| self.files().get(*i)).filter(|f| !crate::decode::is_played(&f.path)).map(|f| f.path.clone()).collect();
        failed.extend(self.failed_video());
        let again = self.settings.slideshow_loop;
        let Some(s) = &mut self.show else { return };
        s.unreadable.extend(failed);
        if !s.unreadable.contains(&s.paths[s.at]) {
            return;
        }
        let skip = past_unreadable(s.at, s.paths.len(), !s.back, again, |i| s.unreadable.contains(&s.paths[i]), s.shown);
        log!("slideshow: {:?} cannot be read: {skip:?}", s.paths[s.at]);
        match skip {
            Skip::To(at, forward) => {
                s.skipped = Some((crate::source::name(&s.paths[s.at]).to_string(), now));
                s.at = at;
                s.back = !forward;
                s.since = now;
            }
            Skip::End => self.end_show(ctx),
            Skip::NoneReadable => {
                self.end_show(ctx);
                self.error = Some(self.i18n.t("show-none-readable"));
            }
        }
    }

    /// The images needed now are asked for; the others are let go. The one shown (`cur`), opened
    /// anyway before and found too large as a neighbour, is read again, now opened.
    fn show_load(&mut self, cur: Option<usize>, keep: Vec<usize>) {
        if let Some(i) = cur.filter(|&i| self.remembered_too_large(i)) {
            self.full.remove(&i);
        }
        let (Some(loader), Some(Ok(listing))) = (&self.loader, self.listings.get(&self.dir)) else { return };
        for &i in &keep {
            if !self.full.contains_key(&i) && self.pending.insert(i) {
                loader.load_full(self.generation, i, listing.files[i].path.clone(), Some(listing.files[i].size), crate::view_memory::Wanted::of(Some(i) == cur));
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
            self.toggle_video();
        }
        if next || prev {
            self.show_go(ctx, next, now);
        }
    }

    /// To the next image (or the previous one), with the passage, over those that cannot be read;
    /// past the last one without "start again" the slideshow ends.
    fn show_go(&mut self, ctx: &egui::Context, forward: bool, now: f64) {
        let again = self.settings.slideshow_loop;
        let Some(s) = &mut self.show else { return };
        let n = s.paths.len();
        match next_shown(s.at, n, forward, again, |i| s.unreadable.contains(&s.paths[i])) {
            Some(at) => {
                // Unreadable ones passed over on the way (found by the look-ahead): the first said.
                if let Some(over) = step(s.at, n, forward, again).filter(|&p| p != at) {
                    s.skipped = Some((crate::source::name(&s.paths[over]).to_string(), now));
                }
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

impl App {
    /// The picture just passed over, in the top left corner for a few seconds: small, with the
    /// warning triangle, on the hints' dark background; the pictures after it stay in view.
    fn show_skipped(&self, ui: &egui::Ui, rect: Rect, now: f64) {
        let Some((name, since)) = self.show.as_ref().and_then(|s| s.skipped.as_ref()) else { return };
        if now - since >= SKIPPED_SECS {
            return;
        }
        let text = self.i18n.t_args("show-skipped", &[("name", name)]);
        let galley = ui.painter().layout(text, egui::FontId::proportional(15.0), Color32::WHITE, rect.width() * 0.4);
        let icon = 18.0;
        let back = Rect::from_min_size(rect.min + Vec2::splat(24.0), Vec2::new(icon + 8.0 + galley.size().x + 24.0, galley.size().y.max(icon) + 12.0));
        ui.painter().rect_filled(back, 6.0, Color32::from_black_alpha(170));
        let top = back.min + Vec2::new(12.0, 6.0);
        crate::theme::paint_warning(ui.painter(), Rect::from_min_size(top, Vec2::splat(icon)));
        ui.painter().galley(top + Vec2::new(icon + 8.0, 0.0), galley, Color32::WHITE);
    }
}

/// How much a picture of `size` is scaled to fit `room`: never larger than its pixels, but for
/// the still of a video (`enlarge`), which fills the room as the video playing after it does.
fn fit(room: Vec2, size: Vec2, enlarge: bool) -> f32 {
    let fit = (room.x / size.x).min(room.y / size.y);
    if enlarge { fit } else { fit.min(1.0) }
}

/// `tex` fitted into `rect` (`fit`), scaled by `scale` around the centre and moved by `dx` points.
fn fitted(rect: Rect, (tex, enlarge): (&TextureHandle, bool), ppp: f32, scale: f32, dx: f32) -> Rect {
    let size = tex.size_vec2() / ppp;
    Rect::from_center_size(rect.center() + Vec2::new(dx, 0.0), size * fit(rect.size(), size, enlarge) * scale)
}

/// The passage at `t` (0 to 1) from `from` to `to`, each with whether it is the still of a video;
/// `back`: going to the previous image.
fn paint_passage(p: &egui::Painter, rect: Rect, from: Option<(&Picture, bool)>, to: (&Picture, bool), (fade, back): (Fade, bool), t: f32) {
    let ppp = p.ctx().pixels_per_point();
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let draw = |(tex, enlarge): (&Picture, bool), scale: f32, dx: f32, alpha: f32| {
        // Smaller than its pixels: filtered as in the viewer (`paint`).
        crate::paint::paint_tinted(p, tex, fitted(rect, (tex, enlarge), ppp, scale, dx), uv, Default::default(), Color32::WHITE.gamma_multiply(alpha.clamp(0.0, 1.0)));
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

    /// #209: in the slideshow a video that cannot be opened is passed over as a picture that
    /// cannot be opened (Piero, 20/11): its name said small in the corner, and when no file of
    /// the slideshow opens it stops with the message in the bar. Skipped without libmpv (the CI).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn broken_video_skipped_as_a_picture() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if !crate::mpv::available() {
            println!("skipped: no libmpv");
            return;
        }
        if own_folder("slideshow::tests::broken_video_skipped_as_a_picture") {
            return;
        }
        let dir = folder("show-broken-video", &["a.png", "c.png"]);
        std::fs::write(dir.join("b.mp4"), crate::mpv::tests::noise(20_000)).unwrap();
        let mut h = Harness::new(&dir);
        h.app.settings.video_mute = true;
        let path = |h: &Harness, name: &str| h.app.files().iter().find(|f| f.path.ends_with(name)).unwrap().path.clone();
        let (b, c) = (path(&h, "b.mp4"), path(&h, "c.png"));
        let ctx = h.ctx.clone();
        h.app.start_show(&ctx, Some(b));
        h.until(|a| a.show.as_ref().is_some_and(|s| s.skipped.as_ref().is_some_and(|k| k.0 == "b.mp4")), "b.mp4 passed over");
        let s = h.app.show.as_ref().unwrap();
        assert_eq!(s.paths[s.at], c);
        let en = crate::i18n::I18n::new("en");
        assert_eq!(en.t_args("show-skipped", &[("name", "b.mp4")]), "File skipped: b.mp4 (it cannot be opened)");
        assert_eq!(en.t("show-none-readable"), "Slideshow stopped: none of its files can be opened.");

        let dir = folder("show-only-broken-videos", &[]);
        std::fs::write(dir.join("x.mp4"), crate::mpv::tests::noise(20_000)).unwrap();
        std::fs::write(dir.join("y.mp4"), crate::mpv::tests::header_and_noise()).unwrap();
        let mut h = Harness::new(&dir);
        h.app.settings.video_mute = true;
        let ctx = h.ctx.clone();
        h.app.start_show(&ctx, None);
        assert!(h.app.show.is_some(), "a video whose still failed may still play: the slideshow starts");
        h.until(|a| a.show.is_none(), "the slideshow stopped");
        assert_eq!(h.app.error, Some(h.app.i18n.t("show-none-readable")));
        let it = crate::i18n::I18n::new("it");
        assert_eq!(it.t_args("show-skipped", &[("name", "b.mp4")]), "File saltato: b.mp4 (non si apre)");
        assert_eq!(it.t("show-none-readable"), "Carosello fermato: nessuno dei suoi file si apre.");
    }
    use super::*;

    /// The slideshow covers the whole window: Ctrl+C, Ctrl+X, Ctrl+V and Tab, which reach the app
    /// as events before the frame, are not the grid's under it. Ctrl+V pasted the files copied
    /// into the folder open, Tab moved the grid's cursor once the slideshow ended. The same
    /// events in the grid do copy, cut, paste and move, so the test sees them arrive.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn clipboard_and_tab_not_under_the_slideshow() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("slideshow::tests::clipboard_and_tab_not_under_the_slideshow") {
            return;
        }
        let mut h = Harness::new(&folder("show-clip-tab", &["a.png", "b.png", "c.png"]));
        (h.app.cursor, h.app.selected, h.app.focus) = (true, 0, crate::Focus::Grid);
        let tab = egui::Event::Key { key: Key::Tab, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        // In the grid: Ctrl+X cuts and Ctrl+C copies the file with the cursor.
        h.frame(vec![egui::Event::Cut]);
        assert!(h.app.clip.as_ref().is_some_and(|c| c.cut), "Ctrl+X in the grid did not cut");
        h.frame(vec![egui::Event::Copy]);
        let text = h.app.clip.as_ref().map(|c| c.text.clone()).expect("Ctrl+C in the grid did not copy");

        let ctx = h.ctx.clone();
        h.app.start_show(&ctx, None);
        assert!(h.app.show.is_some(), "the slideshow did not start");
        // Each of the four, seen before the assert: what reached the grid.
        let mut reached = Vec::new();
        h.frame(vec![egui::Event::Paste(text.clone())]);
        if h.app.paste_ask.is_some() || h.app.transfer.is_some() {
            reached.push("Ctrl+V pasted into the folder");
        }
        for (event, what) in [(egui::Event::Copy, "Ctrl+C copied the grid's file"), (egui::Event::Cut, "Ctrl+X cut the grid's file")] {
            h.app.clip = None;
            h.frame(vec![event]);
            if h.app.clip.is_some() {
                reached.push(what);
            }
        }
        h.frame(vec![tab.clone()]);
        if h.app.tab.is_some() {
            reached.push("Tab was kept for the grid");
        }
        assert!(reached.is_empty(), "in the slideshow: {reached:?}");
        h.key(Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.show.is_none(), "Esc did not end the slideshow");

        // In the grid again: Tab moves the cursor, Ctrl+V pastes what is copied.
        let at = h.app.selected;
        h.frame(vec![tab]);
        assert_eq!(h.app.selected, at + 1, "Tab in the grid did not move the cursor");
        h.frame(vec![egui::Event::Copy]);
        let text = h.app.clip.as_ref().map(|c| c.text.clone()).expect("Ctrl+C in the grid did not copy");
        h.frame(vec![egui::Event::Paste(text)]);
        assert!(h.app.paste_ask.is_some(), "Ctrl+V in the grid did not paste");
    }

    /// A picture is never shown larger than its pixels; the still of a video fills the screen as
    /// the video does (Piero, 2026-11-18: small for a moment, then the whole screen).
    #[test]
    fn still_of_a_video_fills_the_screen() {
        let (room, hd) = (Vec2::new(2560.0, 1440.0), Vec2::new(1280.0, 720.0));
        assert_eq!(fit(room, hd, false), 1.0);
        assert_eq!(fit(room, hd, true), 2.0);
        // Larger than the screen, both are reduced the same.
        let big = Vec2::new(5120.0, 1440.0);
        assert_eq!((fit(room, big, false), fit(room, big, true)), (0.5, 0.5));
    }

    /// The time of a picture still being read does not run: it starts when the picture is there.
    #[test]
    fn time_from_the_picture_read() {
        assert_eq!(started(10.0, 25.0, true), 25.0);
        assert_eq!(started(10.0, 25.0, false), 10.0);
    }

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

    /// Unreadable pictures passed over the way it goes, in the look-ahead too; back past the first
    /// one without "start again", forward; none readable or none shown at the end: it stops.
    #[test]
    fn unreadable_passed_over() {
        let bad = |list: &'static [usize]| move |i: usize| list.contains(&i);
        // Look-ahead: the next one that opens, the bad ones in a row passed over.
        assert_eq!(next_shown(0, 5, true, false, bad(&[1, 2])), Some(3));
        assert_eq!(next_shown(4, 5, false, false, bad(&[3])), Some(2));
        assert_eq!(next_shown(3, 5, true, false, bad(&[4])), None);
        assert_eq!(next_shown(3, 5, true, true, bad(&[4, 0])), Some(1));
        assert_eq!(next_shown(0, 3, true, true, bad(&[0, 1, 2])), None);
        // On a bad one: on the same way.
        assert_eq!(past_unreadable(1, 4, true, false, bad(&[1, 2]), true), Skip::To(3, true));
        assert_eq!(past_unreadable(2, 4, false, false, bad(&[1, 2]), true), Skip::To(0, false));
        // The first one, going back without "start again": forward, to the one it came from.
        assert_eq!(past_unreadable(0, 3, false, false, bad(&[0]), true), Skip::To(1, true));
        // A bad first one at the start.
        assert_eq!(past_unreadable(0, 3, true, false, bad(&[0]), false), Skip::To(1, true));
        // A bad last one without "start again": the end, as after a picture shown; round with it.
        assert_eq!(past_unreadable(3, 4, true, false, bad(&[3]), true), Skip::End);
        assert_eq!(past_unreadable(3, 4, true, true, bad(&[3]), true), Skip::To(0, true));
        // From a bad one to the end without any shown (started from one near the end).
        assert_eq!(past_unreadable(3, 4, true, false, bad(&[2, 3]), false), Skip::NoneReadable);
        // All bad: it stops, with and without "start again", one picture alone too.
        assert_eq!(past_unreadable(1, 3, true, true, bad(&[0, 1, 2]), false), Skip::NoneReadable);
        assert_eq!(past_unreadable(0, 3, true, false, bad(&[0, 1, 2]), false), Skip::NoneReadable);
        assert_eq!(past_unreadable(0, 1, true, true, bad(&[0]), false), Skip::NoneReadable);
        assert_eq!(past_unreadable(0, 1, false, false, bad(&[0]), false), Skip::NoneReadable);
    }

    /// Only what the decoder said, or the viewer, makes a picture known not to open; the slideshow
    /// does not start only when every one of its files is known so (Piero, 21/11/2026).
    #[test]
    fn known_not_to_open_before_the_start() {
        let damaged = "The image format could not be determined";
        let busy = format!("{}device busy", crate::thumb_queue::NOT_TRIED);
        // Thumbnails: the decoder's verdict counts; too large, not tried, not wanted, no engine do not.
        assert!(thumb_says_unreadable(damaged));
        for e in [crate::thumb_memory::TOO_LARGE, &busy, crate::thumb_queue::NOT_WANTED, crate::video_read::NO_ENGINE, crate::occt::MISSING] {
            assert!(!thumb_says_unreadable(e), "{e}");
        }
        let photo = "a/broken.jpg";
        assert!(known_unreadable(photo, None, Some(Err(damaged))));
        assert!(!known_unreadable(photo, None, Some(Err(crate::thumb_memory::TOO_LARGE))));
        assert!(!known_unreadable(photo, None, Some(Err(&busy))));
        assert!(!known_unreadable(photo, None, Some(Ok(()))));
        // No thumbnail yet: not known.
        assert!(!known_unreadable(photo, None, None));
        // The viewer: opened wins over a failed thumbnail; its error counts, unless too large.
        assert!(!known_unreadable(photo, Some(Ok(())), Some(Err(damaged))));
        assert!(known_unreadable(photo, Some(Err("damaged")), None));
        let too_large = crate::view_memory::error(6 << 30);
        assert!(!known_unreadable(photo, Some(Err(&too_large)), None));
        // A video whose still failed for another reason may still play; one mpv cannot play at
        // all is known (Piero, 22/11); a sound file never, nor a still that ran out of time.
        assert!(!known_unreadable("a/clip.mp4", Some(Err("damaged")), Some(Err(damaged))));
        assert!(!known_unreadable("a/clip.mp4", None, Some(Err("timeout"))));
        assert!(!known_unreadable("a/clip.mp4", None, Some(Err(crate::video_read::NO_ENGINE))));
        assert!(!known_unreadable("a/clip.mp4", None, None));
        assert!(known_unreadable("a/clip.mp4", None, Some(Err(crate::mpv::CANNOT_PLAY))));
        assert!(!known_unreadable("a/song.mp3", None, Some(Err(crate::mpv::CANNOT_PLAY))));
        let videos: Vec<String> = ["a/1.mp4", "a/2.mp4"].map(String::from).into();
        assert!(none_opens(&videos, |p| known_unreadable(p, None, Some(Err(crate::mpv::CANNOT_PLAY)))), "broken videos only: no start");

        let paths: Vec<String> = ["a/1.jpg", "a/2.jpg", "a/3.jpg"].map(String::from).into();
        assert!(none_opens(&paths, |_| true));
        // One not known yet (its thumbnail to come): it starts, and passes over the broken ones.
        assert!(!none_opens(&paths, |p| p != "a/2.jpg"));
        assert!(!none_opens(&[], |_| true));
    }

    #[test]
    fn only_pictures_and_videos() {
        let files: Vec<String> = ["a.jpg", "b.glb", "c.dxf", "d.mp3", "e.mp4", "f.png"].map(String::from).to_vec();
        let all = || vec!["a.jpg".to_string(), "e.mp4".into(), "f.png".into()];
        // Nothing selected: the whole folder without the 3D model, the drawing and the audio.
        assert_eq!(show_list(&files, &[], None), (all(), 0));
        // One picture or video: the folder, from it.
        assert_eq!(show_list(&files, &["e.mp4".into()], Some("e.mp4")), (all(), 1));
        // A 3D model alone (chosen or open in the viewer): from the first picture.
        assert_eq!(show_list(&files, &["b.glb".into()], Some("b.glb")), (all(), 0));
        // Several: only those that are pictures or videos, from the first.
        let chosen: Vec<String> = ["f.png", "b.glb", "a.jpg"].map(String::from).to_vec();
        assert_eq!(show_list(&files, &chosen, Some("f.png")), (vec!["a.jpg".to_string(), "f.png".into()], 0));
        // No picture nor video: nothing to show.
        let none: Vec<String> = ["b.glb", "c.dxf", "d.mp3"].map(String::from).to_vec();
        assert_eq!(show_list(&none, &[], None), (vec![], 0));
        assert!(!none.iter().any(|p| in_show(p)));
    }
}
