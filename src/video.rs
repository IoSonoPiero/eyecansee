//! Videos in the viewer: the frames of `mpv::Player`, a bar to play, pause and move along.

use eframe::egui::{self, Color32, Rect, TextureHandle, TextureOptions, pos2, vec2};

use crate::i18n::I18n;
use crate::{App, browser, decode, mpv};

/// Height of the bar under the video.
const BAR: f32 = 40.0;

/// The video open in the viewer; dropped (and so stopped) when the viewer shows something else.
pub(crate) struct Playing {
    path: String,
    player: Result<mpv::Player, String>,
    texture: Option<TextureHandle>,
}

impl App {
    /// The viewer shows a video.
    pub(crate) fn viewing_video(&self) -> bool {
        self.viewing.and_then(|i| self.files().get(i)).is_some_and(|f| decode::is_video(&f.path))
    }

    /// The viewer shows an image.
    pub(crate) fn viewing_image(&self) -> bool {
        self.viewing.is_some() && !self.viewing_video()
    }

    /// Size of the video playing, for the status bar.
    pub(crate) fn video_size(&self) -> Option<[u32; 2]> {
        self.video.as_ref()?.player.as_ref().ok()?.video_size()
    }

    /// Space in the viewer of a video.
    pub(crate) fn toggle_video(&self) {
        if let Some(Playing { player: Ok(p), .. }) = &self.video {
            p.toggle();
        }
    }

    pub(crate) fn video_view(&mut self, ui: &mut egui::Ui, current: usize) {
        let Some((path, size)) = self.files().get(current).map(|f| (f.path.clone(), f.size)) else { return };
        if self.video.as_ref().is_none_or(|v| v.path != path) {
            self.video = None; // the old one stops before the new one starts
            self.video = Some(self.open_video(ui.ctx(), path, size));
        }
        let Some(v) = &mut self.video else { return };
        if let Err(e) = &v.player {
            let e = e.clone();
            ui.centered_and_justified(|ui| ui.label(browser::error_text(ui, e)));
            return;
        }
        let Some(response) = draw(ui, v, &self.i18n) else { return };
        if response.double_clicked() {
            self.close_viewer();
        }
        let mut chosen = None;
        response.context_menu(|ui| chosen = browser::context_menu(ui, self.item_menu(None, None)));
        if chosen.is_some() {
            self.act = chosen;
        }
    }

    /// The slideshow on a video: played whole at full screen (over its still, which the passage
    /// uses). Returns (a frame was painted, the video is over or cannot play).
    pub(crate) fn show_video(&mut self, painter: &egui::Painter, rect: Rect, path: &str, size: u64) -> (bool, bool) {
        if self.video.as_ref().is_none_or(|v| v.path != path) {
            self.video = None;
            self.video = Some(self.open_video(painter.ctx(), path.to_string(), size));
        }
        let Some(v) = &mut self.video else { return (false, true) };
        let Ok(player) = &v.player else { return (false, true) };
        (paint_fitted(painter, rect, player, &mut v.texture), player.ended())
    }

    fn open_video(&self, ctx: &egui::Context, path: String, size: u64) -> Playing {
        let ctx = ctx.clone();
        let player = match &self.loader {
            _ if !mpv::available() => Err(self.i18n.t("video-no-mpv")),
            None => Err(String::new()),
            Some(loader) => mpv::Player::open(loader.video_input(&path, size), move || ctx.request_repaint())
                .map_err(|e| self.i18n.t_args("cannot-load", &[("file", crate::source::name(&path)), ("error", &e)])),
        };
        if let Err(e) = &player {
            log!("video {path:?}: ERROR {e}");
        }
        Playing { path, player, texture: None }
    }
}

/// The video fitted to the area (frames drawn at that size by mpv) and its bar. Click: play or
/// pause. The response of the area, for the double click and the menu, as for images.
fn draw(ui: &mut egui::Ui, v: &mut Playing, i18n: &I18n) -> Option<egui::Response> {
    let Ok(player) = &v.player else { return None };
    let rect = ui.available_rect_before_wrap();
    let area = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - BAR));
    let response = ui.allocate_rect(area, egui::Sense::click());
    if !paint_fitted(ui.painter(), area, player, &mut v.texture) && player.video_size().is_none() {
        ui.put(Rect::from_center_size(area.center(), vec2(32.0, 32.0)), egui::Spinner::new());
    }
    if response.clicked() {
        player.toggle();
    }
    controls(ui, Rect::from_min_max(pos2(rect.min.x, area.max.y), rect.max), player, i18n);
    Some(response)
}

/// The latest frame into `texture`, painted fitted to `area`; the frames are asked at that size.
/// False until there is a frame.
fn paint_fitted(painter: &egui::Painter, area: Rect, player: &mpv::Player, texture: &mut Option<TextureHandle>) -> bool {
    if let Some(image) = player.frame() {
        match texture {
            Some(t) => t.set(image, TextureOptions::LINEAR),
            None => *texture = Some(painter.ctx().load_texture("video", image, TextureOptions::LINEAR)),
        }
    }
    let Some([w, h]) = player.video_size() else { return false };
    let ppp = painter.ctx().pixels_per_point();
    let scale = (area.width() * ppp / w as f32).min(area.height() * ppp / h as f32);
    let pixels = [(w as f32 * scale).round().max(1.0) as u32, (h as f32 * scale).round().max(1.0) as u32];
    player.set_size(pixels);
    let Some(t) = texture else { return false };
    let shown = Rect::from_center_size(area.center(), vec2(pixels[0] as f32, pixels[1] as f32) / ppp);
    painter.image(t.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    true
}

/// Play/pause, time, position. On the viewer's dark background in both themes.
fn controls(ui: &mut egui::Ui, rect: Rect, player: &mpv::Player, i18n: &I18n) {
    let builder = egui::UiBuilder::new().max_rect(rect.shrink2(vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center));
    ui.scope_builder(builder, |ui| {
        let light = Color32::from_gray(225);
        let paused = player.paused();
        let (icon, tip) = if paused { ("▶", "video-play") } else { ("⏸", "video-pause") };
        if ui.button(egui::RichText::new(icon).size(18.0)).on_hover_text(i18n.t(tip)).clicked() {
            player.toggle();
        }
        let (pos, length) = (player.position(), player.duration());
        ui.label(egui::RichText::new(format!("{} / {}", mpv::clock(pos), mpv::clock(length))).color(light).monospace());
        ui.spacing_mut().slider_width = ui.available_width().max(40.0);
        let mut at = pos;
        let r = ui.add(egui::Slider::new(&mut at, 0.0..=length.max(0.001)).show_value(false));
        // While dragging, to the nearest keyframe (fast); exact when released or clicked.
        if r.changed() {
            player.seek(at, !r.dragged());
        }
        if r.drag_stopped() {
            player.seek(at, true);
        }
    });
}
