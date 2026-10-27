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
    /// Not opened because libmpv could not be loaded: opened again once it can.
    no_engine: bool,
}

/// Where the video engine is, for the card shown in its place.
enum Engine {
    Missing,
    Downloading(u64),
    Failed(String),
    /// Downloaded, active after a restart; why the restart could not start, if it could not.
    Ready(Option<String>),
}

enum Click {
    Download,
    Restart,
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
        if v.no_engine && mpv::available() {
            self.video = None; // the engine is there now: opened at the next frame
            ui.ctx().request_repaint();
            return;
        }
        if let Err(e) = &v.player {
            let e = e.clone();
            match crate::component::VIDEO {
                Some(c) if !mpv::available() => self.component_ui(ui, c),
                _ => {
                    ui.centered_and_justified(|ui| ui.label(browser::error_text(ui, e)));
                }
            }
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

    /// Without libmpv where it can be downloaded: what it is, its size and the button; then the
    /// progress; then the restart that activates it. The video opens by itself if the engine
    /// loads meanwhile (`video_view`).
    fn component_ui(&mut self, ui: &mut egui::Ui, c: crate::component::Component) {
        let result = self.download.as_ref().map(|d| (d.done.load(std::sync::atomic::Ordering::Relaxed), d.result.lock().unwrap().clone()));
        let on_disk = crate::component::path(&c).is_some_and(|p| p.is_file());
        // Downloaded: a restart activates it (on Windows the first load right after the download
        // failed until EyeCanSee was started again, 2026-10-27). The same once on disk but not
        // loadable. If it loads meanwhile, the video opens by itself (`video_view`).
        let state = match &result {
            Some((done, None)) => Engine::Downloading(*done),
            Some((_, Some(Err(e)))) => Engine::Failed(e.clone()),
            _ if on_disk => Engine::Ready(self.restart_error.clone()),
            _ => Engine::Missing,
        };
        if matches!(state, Engine::Downloading(_)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        match self.engine_card(ui, c, &state) {
            Some(Click::Download) => {
                let ctx = ui.ctx().clone();
                self.download = Some(crate::component::start(c, move || ctx.request_repaint()));
            }
            Some(Click::Restart) => match crate::update::restart() {
                Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => {
                    log!("restart: ERROR {e}");
                    self.restart_error = Some(e);
                }
            },
            None => {}
        }
    }

    /// The card of `component_ui`, in the middle of the viewer, and what was clicked.
    fn engine_card(&self, ui: &mut egui::Ui, c: crate::component::Component, state: &Engine) -> Option<Click> {
        let t = |id: &str, args: &[(&str, &str)]| self.i18n.t_args(id, args);
        let light = Color32::from_gray(230);
        let mut click = None;
        let big = |text: String| egui::Button::new(egui::RichText::new(text).size(15.0)).min_size(egui::vec2(220.0, 34.0));
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 110.0).max(0.0));
            let card = egui::Frame::new().fill(Color32::from_gray(34)).stroke(egui::Stroke::new(1.0, Color32::from_gray(60))).corner_radius(10.0).inner_margin(24.0);
            card.show(ui, |ui| {
                ui.set_max_width(460.0);
                ui.vertical_centered(|ui| {
                    let title = egui::FontId::new(20.0, egui::FontFamily::Name(crate::BOLD.into()));
                    ui.label(egui::RichText::new(t("video-component-title", &[])).font(title).color(light));
                    ui.add_space(10.0);
                    let wrap = |ui: &mut egui::Ui, text: egui::RichText| ui.add(egui::Label::new(text).wrap());
                    match state {
                        Engine::Missing | Engine::Failed(_) => {
                            wrap(ui, egui::RichText::new(t("video-component-ask", &[("size", &self.human_size(c.size))])).color(light));
                            if let Engine::Failed(e) = state {
                                ui.add_space(6.0);
                                wrap(ui, browser::error_text(ui, t("video-component-error", &[("error", e)])));
                            }
                            ui.add_space(14.0);
                            if ui.add(big(t("video-component-download", &[]))).clicked() {
                                click = Some(Click::Download);
                            }
                        }
                        Engine::Downloading(done) => {
                            let (d, total) = (self.human_size(*done), self.human_size(c.size));
                            wrap(ui, egui::RichText::new(t("video-component-progress", &[("done", &d), ("total", &total)])).color(light));
                            ui.add_space(10.0);
                            ui.add(egui::ProgressBar::new(*done as f32 / c.size as f32).desired_width(360.0));
                        }
                        Engine::Ready(error) => {
                            wrap(ui, egui::RichText::new(t("video-component-restart-needed", &[])).color(light));
                            if let Some(e) = error {
                                ui.add_space(6.0);
                                wrap(ui, browser::error_text(ui, e.clone()));
                            }
                            ui.add_space(14.0);
                            if ui.add(big(t("video-component-restart", &[]))).clicked() {
                                click = Some(Click::Restart);
                            }
                        }
                    }
                });
            });
        });
        click
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
        Playing { path, no_engine: !mpv::available(), player, texture: None }
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
