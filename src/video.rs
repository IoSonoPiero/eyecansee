//! Videos in the viewer: the frames of `mpv::Player`, a bar to play, pause and move along.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use eframe::egui::{self, Color32, Rect, TextureHandle, TextureOptions, pos2, vec2};

use crate::i18n::I18n;
use crate::transform::Turn;
use crate::{App, browser, decode, mpv};

/// Height of the bar under the video.
const BAR: f32 = 40.0;
/// Width of the volume slider, and the volume a key or a wheel notch adds or takes away.
const VOLUME_WIDTH: f32 = 90.0;
const VOLUME_STEP: i32 = 5;
/// Width of the volume written after its slider ("100 %").
const VOLUME_VALUE: f32 = 44.0;

/// The video open in the viewer; dropped (and so stopped) when the viewer shows something else.
pub(crate) struct Playing {
    path: String,
    player: Result<mpv::Player, String>,
    texture: Option<TextureHandle>,
    /// Not opened because libmpv could not be loaded: opened again once it can.
    no_engine: bool,
    /// Wheel notches not yet turned into volume steps (a touchpad gives fractions).
    wheel: f32,
    /// Bytes copied and the size, while the file is copied whole before playing (a phone on Windows).
    copying: Option<(Arc<AtomicU64>, u64)>,
    /// At full screen, the bar hidden while the mouse stays still (#43).
    hide: crate::video_full::Hide,
}

/// Where the video engine is, for the card shown in its place.
enum Engine {
    Missing,
    Downloading(u64),
    Failed(String),
    /// Downloaded, active after a restart; why the restart could not start, if it could not.
    Ready(Option<String>),
    /// The system's is too old and nothing is downloaded yet (`mpv::MIN_API`).
    TooOld { found: String, required: &'static str },
    /// The system's cannot play what phones record and nothing is downloaded yet (issue #67).
    Lacks { found: String, missing: String },
    /// The Options say "the system's only" and there is none: nothing to download.
    SystemOnly,
}

enum Click {
    Download,
    Restart,
}

/// The Fluent id of the metadata panel's note for a video or an audio file without the engine:
/// never the library's own error.
fn note_id(state: &Engine) -> &'static str {
    match state {
        Engine::Ready(_) => "video-component-restart-needed",
        Engine::SystemOnly => "video-engine-system-only",
        Engine::TooOld { .. } => "video-engine-old",
        Engine::Lacks { .. } => "video-engine-lacks",
        Engine::Missing | Engine::Downloading(_) | Engine::Failed(_) => "meta-video-engine",
    }
}

/// Seconds a jump with Ctrl and an arrow moves a video (issue #10).
pub(crate) const JUMP: f64 = 5.0;

/// Where a jump of `by` seconds from `at` lands: within 0 and the `length` of the video (a little
/// before its end, which would stop it), anywhere when the length is not known.
fn jumped(at: f64, by: f64, length: f64) -> f64 {
    let to = (at + by).max(0.0);
    if length > 0.0 { to.min((length - 0.1).max(0.0)) } else { to }
}

impl App {
    /// The viewer shows a video.
    pub(crate) fn viewing_video(&self) -> bool {
        self.viewing.and_then(|i| self.files().get(i)).is_some_and(|f| decode::is_played(&f.path))
    }

    /// A picture open in the viewer, which turns and flips: not a video, a 3D model (its own
    /// view, a turn would only show in what is saved) or a CAD drawing (turned, it lost its sharpness).
    pub(crate) fn viewing_photo(&self) -> bool {
        self.viewing.and_then(|i| self.files().get(i)).is_some_and(|f| crate::decode::is_image(&f.path))
    }

    /// The player of the video `path` open in the viewer, once it opened and has a picture (not an
    /// audio file): its frame can be saved (#16).
    pub(crate) fn player_of(&self, path: &str) -> Option<&mpv::Player> {
        let v = self.video.as_ref().filter(|v| v.path == path && !decode::is_audio(path))?;
        v.player.as_ref().ok().filter(|p| p.video_size().is_some())
    }

    /// Size of the video playing, turned as it is shown, for the status bar.
    pub(crate) fn video_size(&self) -> Option<[u32; 2]> {
        self.video.as_ref()?.player.as_ref().ok()?.video_size().map(|size| turned(size, self.turn))
    }

    /// Space in the viewer of a video.
    pub(crate) fn toggle_video(&self) {
        if let Some(Playing { player: Ok(p), .. }) = &self.video {
            p.toggle();
        }
    }

    /// Ctrl and an arrow in the viewer of a video: `seconds` forward (or back), within the video.
    pub(crate) fn video_jump(&self, seconds: f64) {
        if let Some(Playing { player: Ok(p), .. }) = &self.video {
            p.seek(jumped(p.position(), seconds, p.duration()), true);
        }
    }

    /// Up and down in the viewer of a video, and the wheel on it: `steps` of `VOLUME_STEP`.
    /// Nothing on a video without sound, whose controls are off.
    pub(crate) fn video_volume(&mut self, steps: i32) {
        let Some(Playing { player: Ok(p), .. }) = &self.video else { return };
        if !p.has_audio() {
            return;
        }
        let volume = (self.settings.video_volume as i32 + steps * VOLUME_STEP).clamp(0, 100) as u32;
        self.settings.video_volume = volume;
        p.set_volume(volume);
    }

    /// M in the viewer of a video.
    pub(crate) fn toggle_mute(&mut self) {
        let Some(Playing { player: Ok(p), .. }) = &self.video else { return };
        if !p.has_audio() {
            return;
        }
        self.settings.video_mute = !self.settings.video_mute;
        p.set_mute(self.settings.video_mute);
    }

    pub(crate) fn video_view(&mut self, ui: &mut egui::Ui, current: usize) {
        let Some((path, size)) = self.files().get(current).map(|f| (f.path.clone(), f.size)) else { return };
        if self.video.as_ref().is_none_or(|v| v.path != path) {
            self.video = None; // the old one stops before the new one starts
            let at = kept_for(self.video_place.take(), &path);
            self.video = Some(self.open_video(ui.ctx(), path, size, at));
        }
        let engine = self.engine();
        let Some(v) = &mut self.video else { return };
        if v.no_engine && engine {
            self.video = None; // the engine is there now: opened at the next frame
            ui.ctx().request_repaint();
            return;
        }
        if let Some(e) = self.video.as_ref().and_then(|v| self.video_failure(v)) {
            match crate::component::VIDEO {
                Some(c) if !engine => self.component_ui(ui, c),
                // The message is in the bar (`viewer_error`): here only its start.
                _ => crate::error_bar::short_on_dark(ui, &e),
            }
            return;
        }
        let wait = self.video.as_ref().and_then(|v| v.copying.as_ref()).map(|(c, size)| (c.load(Ordering::Relaxed), *size)).filter(|(done, size)| done < size).map(|(done, size)| {
            let text = self.i18n.t_args("video-copying", &[("done", &self.human_size(done)), ("total", &self.human_size(size))]);
            (done as f32 / size.max(1) as f32, text)
        });
        let Some(v) = &mut self.video else { return };
        // A turn not saved yet, shown as it will be saved (#125): given to the frames as they are drawn.
        let turn = self.turn.turns_only();
        let sound = (&mut self.settings.video_volume, &mut self.settings.video_mute);
        let bare = crate::viewer::bare(ui.ctx());
        let Some(response) = draw(ui, v, (sound, turn), &self.i18n, wait, bare) else { return };
        if response.hovered() {
            v.wheel += ui.input(|i| i.events.iter().map(|e| if let egui::Event::MouseWheel { unit, delta, .. } = e { crate::viewer::notches(*unit, *delta) } else { 0.0 }).sum::<f32>());
        }
        let steps = v.wheel.trunc();
        if steps != 0.0 {
            v.wheel -= steps;
            self.video_volume(steps as i32);
        }
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
    /// uses). Returns (a frame was painted, the video is over). One that cannot play is not over:
    /// the slideshow passes over it as over a picture that cannot be opened (`show_pass_unreadable`).
    pub(crate) fn show_video(&mut self, painter: &egui::Painter, rect: Rect, path: &str, size: u64) -> (bool, bool) {
        if self.video.as_ref().is_none_or(|v| v.path != path) {
            self.video = None;
            let at = kept_for(self.video_place.take(), path);
            self.video = Some(self.open_video(painter.ctx(), path.to_string(), size, at));
        }
        if self.video_failed(path) {
            return (false, false);
        }
        // Without the engine: over at once, as before (the card is the viewer's).
        let Some(v) = &mut self.video else { return (false, true) };
        let Ok(player) = &v.player else { return (false, true) };
        (paint_fitted(painter, rect, player, &mut v.texture, Turn::default()), player.ended())
    }

    /// libmpv can play videos: also the engine just downloaded, at once, as FFmpeg and the CAD
    /// engine (the restart asked for on 2026-10-27 seemed slow). A restart is
    /// offered only when it cannot be loaded (`Engine::Ready`: the first load right after the
    /// download failed once on Windows, 2026-10-27).
    fn engine(&self) -> bool {
        mpv::available()
    }

    /// The engine just downloaded and loaded: the videos' thumbnails that failed without it are
    /// made again, and the download is done with.
    fn engine_arrived(&mut self) {
        let done = self.downloads.video.as_ref().is_some_and(|d| matches!(*d.result.lock().unwrap(), Some(Ok(()))));
        if !done || !self.engine() {
            return;
        }
        log!("video engine downloaded: used at once");
        self.downloads.video = None;
        let videos = self.files().iter().filter(|f| decode::is_played(&f.path)).map(|f| f.path.clone()).collect();
        self.on_forgotten(videos);
    }

    /// Without libmpv where it can be downloaded: what it is, its size and the button; then the
    /// progress; then the restart that activates it.
    fn component_ui(&mut self, ui: &mut egui::Ui, c: crate::component::Component) {
        let state = self.engine_state(ui.ctx(), c);
        if let Some(click) = self.engine_card(ui, c, &state) {
            self.engine_click(ui.ctx(), c, click);
        }
    }

    fn engine_state(&self, ctx: &egui::Context, c: crate::component::Component) -> Engine {
        let state = self.engine_now(c);
        if matches!(state, Engine::Downloading(_)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
        state
    }

    /// Where the engine is now (`engine_state` also asks for repaints while it downloads).
    fn engine_now(&self, c: crate::component::Component) -> Engine {
        let result = self.downloads.video.as_ref().map(|d| (d.done.load(std::sync::atomic::Ordering::Relaxed), d.result.lock().unwrap().clone()));
        let on_disk = crate::component::path(&c).is_some_and(|p| p.is_file());
        // Downloaded, or on disk but not loadable: a restart activates it (`engine`).
        match &result {
            Some((done, None)) => Engine::Downloading(*done),
            Some((_, Some(Err(e)))) => Engine::Failed(e.clone()),
            _ if on_disk => Engine::Ready(self.restart_error.clone()),
            _ => match mpv::status() {
                crate::engine::Status::TooOld { found, required } => Engine::TooOld { found, required },
                crate::engine::Status::Lacks { found, missing } => Engine::Lacks { found, missing },
                crate::engine::Status::SystemOnly => Engine::SystemOnly,
                _ => Engine::Missing,
            },
        }
    }

    /// The metadata panel of a video or an audio file when the engine is missing: why, in words,
    /// with how to get it, in place of libmpv's raw "libmpv-2.dll not found".
    pub(crate) fn engine_note(&self) -> String {
        let Some(c) = crate::component::VIDEO else { return self.i18n.t("video-no-mpv") };
        let state = self.engine_now(c);
        match note_id(&state) {
            "video-engine-old" | "video-engine-lacks" => self.engine_ask(&state, c),
            id => self.i18n.t_args(id, &[("size", &self.human_size(c.size))]),
        }
    }

    fn engine_click(&mut self, ctx: &egui::Context, c: crate::component::Component, click: Click) {
        match click {
            Click::Download => {
                let ctx = ctx.clone();
                self.downloads.video = Some(crate::component::start(c, move || ctx.request_repaint()));
            }
            Click::Restart => match crate::update::restart() {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => {
                    log!("restart: ERROR {e}");
                    self.restart_error = Some(e);
                }
            },
        }
    }

    /// In the menu bar, as the update button: the engine to download when the open folder has
    /// videos and libmpv is missing, then the restart that activates it. Nothing on a computer
    /// with libmpv or where there is no component to download.
    pub(crate) fn engine_bar(&mut self, ui: &mut egui::Ui) {
        let Some(c) = crate::component::VIDEO else { return };
        self.engine_arrived();
        if self.engine() {
            return;
        }
        let state = self.engine_state(ui.ctx(), c);
        let videos = self.listings.get(&self.dir).and_then(|l| l.as_ref().ok()).is_some_and(|l| l.files.iter().any(|f| decode::is_played(&f.path)));
        if matches!(state, Engine::SystemOnly) || (!videos && matches!(state, Engine::Missing | Engine::TooOld { .. } | Engine::Lacks { .. })) {
            return;
        }
        let t = |id: &str, args: &[(&str, &str)]| self.i18n.t_args(id, args);
        // Red, as the update button (see `menu_bar`).
        let red = crate::theme::alert_button;
        let click = match &state {
            Engine::Missing | Engine::Failed(_) | Engine::TooOld { .. } | Engine::Lacks { .. } => {
                let hover = self.engine_ask(&state, c);
                let button = ui.add(red(t("video-component-button", &[("size", &self.human_size(c.size))]))).on_hover_text(hover);
                button.clicked().then_some(Click::Download)
            }
            Engine::SystemOnly => None,
            Engine::Downloading(done) => {
                let text = t("video-component-progress", &[("done", &self.human_size(*done)), ("total", &self.human_size(c.size))]);
                ui.add_enabled(false, egui::Button::new(text));
                None
            }
            Engine::Ready(_) => ui.add(red(t("video-component-restart", &[]))).clicked().then_some(Click::Restart),
        };
        if let Some(click) = click {
            self.engine_click(ui.ctx(), c, click);
        }
    }

    /// Why the engine is to be downloaded, with its size: for the card and the menu bar's button.
    fn engine_ask(&self, state: &Engine, c: crate::component::Component) -> String {
        let size = self.human_size(c.size);
        match state {
            Engine::TooOld { found, required } => self.i18n.t_args("video-engine-old", &[("found", found), ("required", required), ("size", &size)]),
            Engine::Lacks { found, missing } => self.i18n.t_args("video-engine-lacks", &[("found", found), ("missing", missing), ("size", &size)]),
            _ => self.i18n.t_args("video-component-ask", &[("size", &size)]),
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
                        Engine::SystemOnly => {
                            wrap(ui, egui::RichText::new(t("video-engine-system-only", &[])).color(light));
                        }
                        Engine::Missing | Engine::Failed(_) | Engine::TooOld { .. } | Engine::Lacks { .. } => {
                            wrap(ui, egui::RichText::new(self.engine_ask(state, c)).color(light));
                            if let Engine::Failed(e) = state {
                                ui.add_space(6.0);
                                crate::error_bar::inline_on_dark(ui, &t("video-component-error", &[("error", &crate::update::error_detail(&self.i18n, e))]));
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
                                crate::error_bar::inline_on_dark(ui, e);
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

    /// The video of a Motion Photo (`motion.rs`), played from memory under the photo's path.
    pub(crate) fn open_motion(&self, ctx: &egui::Context, path: String, video: Arc<Vec<u8>>) -> Playing {
        let ctx = ctx.clone();
        let input = mpv::Input::Stream(Arc::new(crate::motion::Video(video)));
        let player = if self.engine() {
            mpv::Player::open(input, self.settings.video_volume, self.settings.video_mute, move || ctx.request_repaint())
                .map_err(|e| self.i18n.cannot_load(crate::source::name(&path), &e))
        } else {
            Err(self.i18n.t("video-no-mpv"))
        };
        if let Err(e) = &player {
            log!("motion photo {path:?}: ERROR {e}");
        }
        Playing { path, no_engine: !self.engine(), player, texture: None, wheel: 0.0, copying: None, hide: Default::default() }
    }

    /// Why the video open for `path` does not play, when no card says it (the engine to
    /// download): for the bar (`viewer_error`). `None` when no video is open for it.
    pub(crate) fn video_error(&self, path: &str) -> Option<Option<String>> {
        let v = self.video.as_ref().filter(|v| v.path == path)?;
        let card = crate::component::VIDEO.is_some() && !self.engine();
        Some(self.video_failure(v).filter(|_| !card))
    }

    /// The video open for `path` cannot be played: mpv could not open it, or found it cannot
    /// play it (#209). Not without the engine (no_engine) or the loader: that is not the file's.
    pub(crate) fn video_failed(&self, path: &str) -> bool {
        self.video.as_ref().filter(|v| v.path == path).is_some_and(|v| match &v.player {
            Ok(p) => p.error().is_some(),
            Err(e) => !v.no_engine && !e.is_empty(),
        })
    }

    /// The path of the video open, if the slideshow passes over it as over a file that cannot be
    /// opened: it cannot be played (`video_failed`), or its phone or server gives no data for a
    /// while (Piero, 22/11: the slideshow never stops on it).
    pub(crate) fn failed_video(&self) -> Option<String> {
        let v = self.video.as_ref()?;
        let stalled = v.player.as_ref().is_ok_and(|p| p.stalled());
        (stalled || self.video_failed(&v.path)).then(|| v.path.clone())
    }

    /// Why `v` does not play, the whole message: as for a picture that cannot be opened, "Cannot
    /// open x" and why under it (#209).
    fn video_failure(&self, v: &Playing) -> Option<String> {
        match &v.player {
            Err(e) => Some(e.clone()),
            Ok(p) => p.error().map(|f| self.i18n.cannot_load(crate::source::name(&v.path), &said(&f))),
        }
    }

    /// The video open for `path` has not reached its end (it plays, it is paused, or it could
    /// not open: its card stays until another image).
    pub(crate) fn video_playing(&self, path: &str) -> bool {
        self.video.as_ref().is_some_and(|v| v.path == path && !v.player.as_ref().is_ok_and(|p| p.ended()))
    }

    /// Before the video `path` is opened again with its new rotation (Save, #211): where it is,
    /// for it to open there (`kept_for`).
    pub(crate) fn keep_place(&mut self, path: &str) {
        let place = self.video.as_ref().filter(|v| v.path == path).and_then(|v| v.player.as_ref().ok()).map(|p| p.place());
        self.video_place = place.map(|p| (path.to_string(), p));
    }

    fn open_video(&self, ctx: &egui::Context, path: String, size: u64, at: Option<mpv::Place>) -> Playing {
        let ctx = ctx.clone();
        let mut copying = None;
        let player = match &self.loader {
            _ if !self.engine() => Err(self.i18n.t("video-no-mpv")),
            None => Err(String::new()),
            Some(loader) => {
                let (input, copied) = loader.video_input(&path, size);
                copying = copied.map(|c| (c, size));
                mpv::Player::open_at(input, self.settings.video_volume, self.settings.video_mute, at, move || ctx.request_repaint())
                    .map_err(|e| self.i18n.cannot_load(crate::source::name(&path), &e))
            }
        };
        if let Err(e) = &player {
            log!("video {path:?}: ERROR {e}");
        }
        if let Some(at) = at {
            log!("video {path:?}: opened at {:.3} s, paused {}, at the end {}", at.time, at.paused, at.at_end);
        }
        Playing { path, no_engine: !self.engine(), player, texture: None, wheel: 0.0, copying, hide: Default::default() }
    }
}

/// The place kept for `path` before Save (#211): used for that path only, and once (the caller
/// takes it out whatever the path).
fn kept_for(kept: Option<(String, mpv::Place)>, path: &str) -> Option<mpv::Place> {
    kept.filter(|(p, _)| p == path).map(|(_, place)| place)
}

/// Why a video does not play, as `I18n::cannot_load` says it: a Fluent id with no detail (`id\u{1}`),
/// or the phone's or the server's own error, already so.
fn said(failure: &mpv::Failure) -> String {
    match failure {
        mpv::Failure::NotVideo => "video-not-video\u{1}".into(),
        mpv::Failure::NoPicture => "video-no-frame\u{1}".into(),
        mpv::Failure::Read(e) => e.clone(),
    }
}

/// The video fitted to the area (frames drawn at that size by mpv) and its bar. Click: play or
/// pause. The response of the area, for the double click, the menu and the wheel, as for images.
/// `wait`: the share and the text of the copy that comes before playing, while it lasts.
/// `bare`: at full screen, the video on the whole screen and the bar over it, shown while the
/// mouse moves (#43).
fn draw(ui: &mut egui::Ui, v: &mut Playing, (sound, turn): ((&mut u32, &mut bool), Turn), i18n: &I18n, wait: Option<(f32, String)>, bare: bool) -> Option<egui::Response> {
    let Ok(player) = &v.player else { return None };
    let copying = wait.is_some();
    let rect = ui.available_rect_before_wrap();
    let area = if bare { rect } else { Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - BAR)) };
    let response = ui.allocate_rect(area, egui::Sense::click());
    if !paint_fitted(ui.painter(), area, player, &mut v.texture, turn) && player.video_size().is_none() {
        if decode::is_audio(&v.path) {
            // An audio file without a cover never gets a picture: a note, not a spinner forever.
            crate::icons::paint(ui.painter(), crate::icons::Icon::Audio, Rect::from_center_size(area.center(), vec2(160.0, 160.0)));
        } else if let Some((share, text)) = wait {
            // No frames come while copying: the bar moves only if asked.
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            let bar = egui::ProgressBar::new(share).text(text).desired_width(360.0);
            ui.put(Rect::from_center_size(area.center(), vec2(360.0, 24.0)), bar);
        } else if !player.stalled() {
            // Light on the viewer's dark ground in both themes, as the spinner of a picture (Piero, 22/11).
            ui.put(Rect::from_center_size(area.center(), vec2(32.0, 32.0)), egui::Spinner::new().color(Color32::from_gray(220)));
        }
    }
    if player.stalled() {
        stalled_note(ui, area, i18n, copying);
    }
    // An audio file sends no frames (a cover is one): the time on the bar moves only if asked.
    if decode::is_audio(&v.path) && !player.paused() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    if response.clicked() {
        player.toggle();
    }
    let bar = Rect::from_min_max(pos2(rect.min.x, rect.max.y - BAR), rect.max);
    if !bare {
        v.hide.reset();
        controls(ui, bar, player, sound, i18n);
    } else if let Some(opacity) = bar_shown(ui, &mut v.hide, bar) {
        // Over the picture: on a dark band, fading in and out.
        ui.painter().rect_filled(bar, 0.0, Color32::from_black_alpha((170.0 * opacity) as u8));
        ui.scope(|ui| {
            ui.set_opacity(opacity);
            controls(ui, bar, player, sound, i18n);
        });
    }
    Some(response)
}

/// The phone or the server gives no data (`mpv::Player::stalled`): said in the middle of the
/// video, on a dark band, above the copy's bar when there is one, until data comes again (#209).
fn stalled_note(ui: &egui::Ui, area: Rect, i18n: &I18n, above_bar: bool) {
    // Looked at again: the note goes as soon as data comes.
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    let painter = ui.painter();
    let galley = painter.layout(i18n.t("video-stalled"), egui::FontId::proportional(16.0), Color32::WHITE, (area.width() - 64.0).max(100.0));
    let center = area.center() - vec2(0.0, if above_bar { 30.0 + galley.size().y / 2.0 } else { 0.0 });
    let back = Rect::from_center_size(center, galley.size() + vec2(32.0, 20.0));
    painter.rect_filled(back, 8.0, Color32::from_black_alpha(190));
    painter.galley(back.min + vec2(16.0, 10.0), galley, Color32::WHITE);
}

/// At full screen, the bar's opacity, `None` once hidden, when the pointer is hidden too. Shown
/// for `HIDE_AFTER` seconds after the mouse moves, and as long as it is on the bar, a slider is
/// dragged or a menu is open.
fn bar_shown(ui: &egui::Ui, hide: &mut crate::video_full::Hide, bar: Rect) -> Option<f32> {
    let ctx = ui.ctx();
    let (now, moved, over) = ctx.input(|i| {
        let moved = i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed() || i.events.iter().any(|e| matches!(e, egui::Event::MouseWheel { .. }));
        (i.time, moved, i.pointer.hover_pos().is_some_and(|p| bar.contains(p)))
    });
    let keep = over || ctx.dragged_id().is_some() || egui::Popup::is_any_open(ctx);
    let left = hide.shown(now, moved, keep);
    if let Some(left) = left {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
    }
    let opacity = ctx.animate_bool_with_time(egui::Id::unique("video-bar"), left.is_some(), crate::video_full::FADE);
    if left.is_none() {
        ctx.set_cursor_icon(egui::CursorIcon::None);
    }
    (opacity > 0.0).then_some(opacity)
}

/// The latest frame into `texture`, painted fitted to `area` and turned by `turn`, the turn not
/// saved yet; the frames are asked at that size. False until there is a frame.
///
/// mpv draws the frames as the file has them, and the turn is given here, on the screen: at once.
/// As a filter of mpv it came about 0.6 s after "Changed, not saved" (#168).
///
/// A frame drawn for another shape is not shown: when the video's size changes (a phone's
/// portrait video as its rotation is applied on opening) the frame at hand was drawn for the old
/// one, and it was stretched into the new rectangle, a narrow picture between black bars, for a
/// quarter of a second (#162). The last frame of the right shape stays, with its own
/// proportions, until the first one drawn for the new size comes.
fn paint_fitted(painter: &egui::Painter, area: Rect, player: &mpv::Player, texture: &mut Option<TextureHandle>, turn: Turn) -> bool {
    let video = player.video_size();
    if let Some(image) = player.frame()
        && (texture.is_none() || video.is_none_or(|v| same_shape(image.size, v)))
    {
        match texture {
            Some(t) => t.set(image, TextureOptions::LINEAR),
            None => *texture = Some(painter.ctx().load_texture("video", image, TextureOptions::LINEAR)),
        }
    }
    let Some(video) = video else { return false };
    let ppp = painter.ctx().pixels_per_point();
    let room = [area.width() * ppp, area.height() * ppp];
    let (drawn, shown) = frame_sizes(turned(video, turn), room);
    player.set_size(turned(drawn, turn));
    let Some(t) = texture else { return false };
    let shown = if same_shape(t.size(), video) { shown } else { frame_sizes(turned(t.size().map(|v| v as u32), turn), room).1 };
    let shown = Rect::from_center_size(area.center(), vec2(shown[0] as f32, shown[1] as f32) / ppp);
    crate::paint::paint_frame(painter, t.id(), shown, turn);
    true
}

/// `size` after `turn`: its sides swapped by a quarter turn or three. The other way round too:
/// the size to ask the frames at, from the size they are shown at.
fn turned(size: [u32; 2], turn: Turn) -> [u32; 2] {
    if turn.sideways() { [size[1], size[0]] } else { size }
}

/// Whether a frame of `frame` pixels was drawn for a video of `video` pixels: the same
/// proportions, within the rounding of `frame_sizes` (half a pixel a side, taken twice over).
fn same_shape(frame: [usize; 2], video: [u32; 2]) -> bool {
    let [fw, fh] = frame.map(|v| v as u64);
    let [w, h] = video.map(u64::from);
    (fw * h).abs_diff(fh * w) <= w + h
}

/// The video of `video` pixels fitted to `room` pixels: the size mpv draws the frames at and the
/// size they are shown at. Never drawn larger than the video: the card enlarges the texture, mpv's
/// software renderer did it on the processor (1080p in a maximized 2560x1440 window: 88% of a core
/// before, 76% after; 116% and 81% on a laptop screen of the other card, 2026-11-09).
fn frame_sizes(video: [u32; 2], room: [f32; 2]) -> ([u32; 2], [u32; 2]) {
    let [w, h] = video.map(|v| v.max(1) as f32);
    let scale = (room[0] / w).min(room[1] / h);
    let shown = [(w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32];
    (if scale > 1.0 { video } else { shown }, shown)
}

/// Play/pause, time, position, then sound off and volume. On the viewer's dark background in both themes.
fn controls(ui: &mut egui::Ui, rect: Rect, player: &mpv::Player, sound: (&mut u32, &mut bool), i18n: &I18n) {
    let builder = egui::UiBuilder::new().max_rect(rect.shrink2(vec2(8.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center));
    ui.scope_builder(builder, |ui| {
        crate::theme::on_dark(ui);
        let light = Color32::from_gray(225);
        let paused = player.paused();
        let (icon, tip) = if paused { (crate::icons::Line::Play, "video-play") } else { (crate::icons::Line::Pause, "video-pause") };
        if ui.add(egui::Button::image(crate::icons::line_in(ui.ctx(), icon, 18.0, light))).on_hover_text(i18n.t(tip)).clicked() {
            player.toggle();
        }
        let length = player.duration();
        let pos = shown_position(player.position(), length, player.ended());
        ui.label(egui::RichText::new(format!("{} / {}", mpv::clock(pos), mpv::clock(length))).color(light).monospace());
        // Room on the right for the sound: the button (as wide as the play one), the slider and
        // the volume.
        let sound_width = VOLUME_WIDTH + VOLUME_VALUE + 4.0 * ui.spacing().item_spacing.x + 34.0;
        ui.spacing_mut().slider_width = (ui.available_width() - sound_width).max(40.0);
        let mut at = pos;
        let r = crate::ticked::styled(ui, |ui| ui.add(egui::Slider::new(&mut at, 0.0..=length.max(0.001)).show_value(false)));
        // While dragging, to the nearest keyframe (fast); exact when released or clicked.
        if r.changed() {
            player.seek(at, !r.dragged());
        }
        if r.drag_stopped() {
            player.seek(at, true);
        }
        sound_controls(ui, player, sound, i18n);
    });
}

/// Where the time and the bar stand: at the end of a video played to its end. mpv keeps the last
/// frame there, whose own time is one frame before the length: the time read 0:09 / 0:10 and the
/// bar stopped short of its end.
fn shown_position(pos: f64, length: f64, ended: bool) -> f64 {
    if ended && length > 0.0 { length } else { pos }
}

/// Sound off and volume; off, not hidden, for a video without sound (and while it loads).
fn sound_controls(ui: &mut egui::Ui, player: &mpv::Player, (volume, mute): (&mut u32, &mut bool), i18n: &I18n) {
    let audio = player.has_audio();
    let (icon, tip) = if *mute { (crate::icons::Line::Mute, "video-unmute") } else { (crate::icons::Line::Volume, "video-mute") };
    let tip = if audio { i18n.t(tip) } else { i18n.t("video-no-audio") };
    let button = ui.add_enabled(audio, egui::Button::image(crate::icons::line_in(ui.ctx(), icon, 18.0, Color32::from_gray(225))));
    if button.on_hover_text(&tip).on_disabled_hover_text(&tip).clicked() {
        *mute = !*mute;
        player.set_mute(*mute);
    }
    ui.spacing_mut().slider_width = VOLUME_WIDTH;
    // A mark every 10, the volume after it; in the light grey of the bar, in both themes.
    let light = Color32::from_gray(225);
    let r = ui
        .add_enabled_ui(audio, |ui| {
            ui.visuals_mut().override_text_color = Some(light);
            let marks = crate::ticked::Marks::new(&(0..=100), Some(f64::from(VOLUME_STEP)));
            let r = crate::ticked::add(ui, egui::Slider::new(volume, 0..=100).show_value(false), &marks);
            let shown = egui::Label::new(egui::RichText::new(format!("{volume} %")).color(light).monospace());
            ui.add_sized([VOLUME_VALUE, ui.spacing().interact_size.y], shown);
            r
        })
        .inner;
    let r = r.on_hover_text(i18n.t_args("video-volume", &[("volume", &volume.to_string())])).on_disabled_hover_text(&tip);
    if r.changed() {
        player.set_volume(*volume);
    }
    // The arrows change the volume through `viewer_keys`: a focused slider would move as well.
    if r.has_focus() {
        r.surrender_focus();
    }
}

#[cfg(test)]
mod tests {

    /// #211: a place kept before Save is for that video only.
    #[test]
    fn place_kept_for_its_video() {
        let place = crate::mpv::Place { time: 3.0, paused: true, at_end: false };
        let kept = || Some(("a/clip.mp4".to_string(), place));
        assert_eq!(super::kept_for(kept(), "a/clip.mp4"), Some(place));
        assert_eq!(super::kept_for(kept(), "a/other.mp4"), None);
        assert_eq!(super::kept_for(None, "a/clip.mp4"), None);
    }

    /// #211: after Save of a turned video the viewer opens it again (its new rotation) where it
    /// was, paused as it was, not from 0:00 playing. Skipped without libmpv (the CI).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn saved_video_stays_where_it_was() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if !crate::mpv::available() {
            println!("skipped: no libmpv");
            return;
        }
        if own_folder("video::tests::saved_video_stays_where_it_was") {
            return;
        }
        let dir = folder("saved-place", &[]);
        std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/site/img/mtp-open.mp4"), dir.join("clip.mp4")).unwrap();
        let mut h = Harness::new(&dir);
        h.app.settings.video_mute = true;
        let path = h.app.files()[0].path.clone();
        h.app.viewing = Some(0);
        h.until(|a| a.player_of(&path).is_some(), "the video open");
        let p = h.app.player_of(&path).unwrap();
        p.pause();
        p.seek(4.5, true);
        h.until(|a| a.player_of(&path).is_some_and(|p| p.paused() && (p.position() - 4.5).abs() < 0.05), "paused at 4.5 s");
        // As after Save: the file replaced, opened again.
        let ctx = h.ctx.clone();
        h.app.on_replaced(&ctx, path.clone(), Ok(()));
        assert!(h.app.video.is_none(), "the old player stays");
        h.until(|a| a.player_of(&path).is_some(), "the video open again");
        // Waited for, 10 s at most (a fixed half second failed under load); what was seen is said.
        let seen = |h: &Harness| h.app.player_of(&path).map(|p| (p.position(), p.paused()));
        let there = |s: Option<(f64, bool)>| s.is_some_and(|(time, paused)| paused && (time - 4.5).abs() < 0.05);
        let start = std::time::Instant::now();
        while !there(seen(&h)) && start.elapsed() < std::time::Duration::from_secs(10) {
            h.frame(Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let (time, paused) = seen(&h).expect("the player after Save");
        println!("after Save: {time:.3} s, paused {paused}, after {:?}", start.elapsed());
        assert!(paused && (time - 4.5).abs() < 0.05, "after Save: {time:.3} s, paused {paused}, in 10 s");
    }

    /// #209: in the viewer a video mpv cannot play says why, as a picture that cannot be opened
    /// (the start in the middle, the whole message in the bar), instead of a spinner forever: one
    /// it does not recognise, one with no frame that decodes. Skipped without libmpv (the CI).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn broken_video_said_in_the_viewer() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if !crate::mpv::available() {
            println!("skipped: no libmpv");
            return;
        }
        if own_folder("video::tests::broken_video_said_in_the_viewer") {
            return;
        }
        let dir = folder("broken-video", &[]);
        std::fs::write(dir.join("noise.mp4"), crate::mpv::tests::noise(20_000)).unwrap();
        std::fs::write(dir.join("no-frame.mp4"), crate::mpv::tests::header_and_noise()).unwrap();
        let mut h = Harness::new(&dir);
        h.app.settings.video_mute = true;
        for (name, id) in [("noise.mp4", "video-not-video"), ("no-frame.mp4", "video-no-frame")] {
            let i = h.app.files().iter().position(|f| f.path.ends_with(name)).unwrap();
            h.app.viewing = Some(i);
            h.until(|a| a.viewer_error().is_some(), name);
            assert_eq!(h.app.viewer_error(), Some(h.app.i18n.cannot_load(name, &format!("{id}\u{1}"))));
        }
        let en = crate::i18n::I18n::new("en");
        assert_eq!(en.cannot_load("a.mp4", "video-not-video\u{1}"), "Cannot open a.mp4\nThe file is not a video, or it is damaged.");
        assert_eq!(en.cannot_load("a.mp4", "video-no-frame\u{1}"), "Cannot open a.mp4\nNo frame can be shown: the video is damaged.");
        let it = crate::i18n::I18n::new("it");
        assert_eq!(it.cannot_load("a.mp4", "video-not-video\u{1}"), "Impossibile aprire a.mp4\nIl file non è un video, o è rovinato.");
        assert_eq!(it.cannot_load("a.mp4", "video-no-frame\u{1}"), "Impossibile aprire a.mp4\nNessun fotogramma si può mostrare: il video è rovinato.");
    }

    #[test]
    fn at_the_end_the_time_and_the_bar_reach_the_length() {
        use super::shown_position;
        assert_eq!(shown_position(9.958, 10.0, true), 10.0);
        assert_eq!(shown_position(4.2, 10.0, false), 4.2);
        assert_eq!(shown_position(0.0, 0.0, true), 0.0);
    }

    /// A jump of 5 seconds back from 3 lands at 0, forward near the end stops just before it.
    #[test]
    fn jumps_stay_in_the_video() {
        assert_eq!((super::jumped(3.0, -5.0, 60.0), super::jumped(10.0, 5.0, 60.0), super::jumped(58.0, 5.0, 60.0), super::jumped(58.0, 5.0, 0.0)), (0.0, 15.0, 59.9, 63.0));
    }

    /// Larger room: drawn at the video's size, shown fitted; smaller room: drawn as shown.
    #[test]
    fn frames_never_drawn_larger_than_the_video() {
        assert_eq!(super::frame_sizes([1920, 1080], [2560.0, 1400.0]), ([1920, 1080], [2489, 1400]));
        assert_eq!(super::frame_sizes([1920, 1080], [960.0, 1000.0]), ([960, 540], [960, 540]));
        assert_eq!(super::frame_sizes([1080, 1920], [2560.0, 1400.0]), ([788, 1400], [788, 1400]));
    }

    /// A frame drawn before a turn of 90° is not one of the turned video (#162: it was stretched
    /// into the new rectangle); the frames fitted to the window are, rounding and all.
    #[test]
    fn frame_of_another_shape() {
        // The sizes of the defect: a 1280x720 video in a 1175x661 room, turned to 720x1280.
        assert!(super::same_shape([1175, 661], [1280, 720]));
        assert!(!super::same_shape([1175, 661], [720, 1280]));
        assert!(super::same_shape([372, 661], [720, 1280]));
        // Every size `frame_sizes` asks for has the shape of its video, down to one pixel.
        for video in [[1920, 1080], [1080, 1920], [720, 576], [4096, 1716], [320, 240]] {
            for room in [[2560.0, 1400.0], [960.0, 1000.0], [333.0, 77.0], [3.0, 400.0], [0.2, 0.2]] {
                let (drawn, shown) = super::frame_sizes(video, room);
                assert!(super::same_shape(drawn.map(|v| v as usize), video), "{video:?} in {room:?}: {drawn:?}");
                assert!(super::same_shape(shown.map(|v| v as usize), video), "{video:?} in {room:?}: {shown:?}");
            }
        }
        // A square video turned keeps its shape: nothing to wait for.
        assert!(super::same_shape([500, 500], [800, 800]));
    }

    /// A video turned a quarter (#168): fitted to the window turned, its frames asked as the file
    /// has them, that is with the sides the other way round.
    #[test]
    fn turned_on_the_screen() {
        use crate::transform::{Op, Turn};
        let (quarter, half) = (Turn::default().then(Op::Cw), Turn::default().then(Op::Cw).then(Op::Cw));
        assert_eq!(super::turned([1280, 720], quarter), [720, 1280]);
        assert_eq!(super::turned([1280, 720], half), [1280, 720]);
        assert_eq!(super::turned([1280, 720], Turn::default()), [1280, 720]);
        // The sizes of the viewer: a 1280x720 video in a 1175x661 room, turned a quarter.
        let (drawn, shown) = super::frame_sizes(super::turned([1280, 720], quarter), [1175.0, 661.0]);
        assert_eq!((shown, super::turned(drawn, quarter)), ([372, 661], [661, 372]));
        assert!(super::same_shape([661, 372], [1280, 720]), "the frame asked has the file's shape");
    }

    /// Without the engine the metadata panel says why in the user's language, never the
    /// library's error ("libmpv-2.dll not found"); every state has its message in both languages.
    #[test]
    fn engine_note_in_words() {
        use super::{Engine, note_id};
        let states = [
            Engine::Missing,
            Engine::Downloading(10),
            Engine::Failed("x".into()),
            Engine::Ready(None),
            Engine::TooOld { found: "mpv 0.32".into(), required: "1.109" },
            Engine::Lacks { found: "mpv 0.36".into(), missing: "HEVC".into() },
            Engine::SystemOnly,
        ];
        assert_eq!(note_id(&Engine::Missing), "meta-video-engine");
        assert_eq!(note_id(&Engine::SystemOnly), "video-engine-system-only");
        assert_eq!(note_id(&Engine::Ready(None)), "video-component-restart-needed");
        for lang in ["en", "it"] {
            let i18n = crate::i18n::I18n::new(lang);
            for state in &states {
                let id = note_id(state);
                let text = i18n.t_args(id, &[("size", "10 MB"), ("found", "f"), ("required", "r"), ("missing", "m")]);
                assert!(text != id && !text.contains("not found") && !text.contains("libmpv-2.dll"), "{lang} {id}: {text}");
            }
        }
        let it = crate::i18n::I18n::new("it").t_args("meta-video-engine", &[("size", "10 MB")]);
        assert!(it.contains("motore video") && it.contains("10 MB"), "{it}");
    }
}
