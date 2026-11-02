//! A 3D model in the viewer: turned with the arrows or by dragging, zoomed with the wheel or + and −,
//! moved with Shift or the middle button; drawn by the graphics card (`render_gpu`) at every
//! frame. Its picture as shown goes to "Save in another format" (`render3d`, in the background).

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Key};

use crate::App;
use crate::model::Mesh;
use crate::render3d::{self, View};
use crate::render_gpu::Gpu;

/// Radians a second while an arrow is held, radians a dragged point.
const TURN_SPEED: f32 = 1.6;
const DRAG_TURN: f32 = 0.008;
/// Long side of the screenshot at least this, whatever the window.
const SHOT_SIDE: f32 = 1920.0;

#[derive(Default)]
pub(crate) struct State {
    /// Models read by the loader for the viewer, by path (`Msg::Mesh`).
    pub(crate) meshes: HashMap<String, Arc<Mesh>>,
    /// The model shown and its camera; a new model starts from the default view.
    of: Option<String>,
    view: View,
    /// The graphics card, from eframe; without it the viewer shows the still picture.
    pub(crate) gpu: Option<Gpu>,
    /// Size of the picture on screen, in pixels, for the screenshot's proportions.
    size: [u32; 2],
    /// CAD drawings, which the viewer also draws itself (`cad_view`).
    pub(crate) cad: crate::cad_view::State,
    /// The CAD engine being downloaded (`cad_engine`).
    pub(crate) engine: Option<Arc<crate::component::Download>>,
    /// When the last frames were drawn (seconds), for the frame rate in the status bar.
    frames: std::collections::VecDeque<f64>,
    /// The presentation was chosen (`pace`), and is mailbox; when the last frame began.
    presented: bool,
    mailbox: bool,
    last_frame: Option<std::time::Instant>,
}

impl App {
    /// With eframe's graphics card, for the 3D viewer.
    pub(crate) fn with_gpu(mut self, cc: &eframe::CreationContext) -> Self {
        if let Some(rs) = &cc.wgpu_render_state {
            let i = rs.adapter.get_info();
            log!("graphics card: {} ({:?}, {:?}, driver {} {})", i.name, i.device_type, i.backend, i.driver, i.driver_info);
        }
        self.model.gpu = cc.wgpu_render_state.clone().map(Gpu::new);
        self
    }

    /// At the first frame, mailbox presentation where the card offers it (`render_gpu::present_mode`);
    /// then, with mailbox, at most 120 frames a second outside the 3D viewer: it never waits for
    /// the screen, and a continuous animation would keep the card busy for nothing. A 3D model
    /// turns with no limit (the user's choice): it is drawn only while it moves.
    pub(crate) fn pace(&mut self, frame: &mut eframe::Frame) {
        let model = self.viewed_mesh().is_some();
        let s = &mut self.model;
        if !s.presented {
            s.presented = true;
            if let Some(config) = crate::render_gpu::present_mode() {
                frame.set_wgpu_surface_config(config);
                s.mailbox = true;
                log!("presentation: mailbox");
            }
        }
        if s.mailbox && !model {
            let min = std::time::Duration::from_secs_f64(1.0 / 120.0);
            if let Some(last) = s.last_frame {
                let spent = last.elapsed();
                if spent < min {
                    std::thread::sleep(min - spent);
                }
            }
            s.last_frame = Some(std::time::Instant::now());
        }
    }

    /// The model open in the viewer, once read.
    pub(crate) fn viewed_mesh(&self) -> Option<(String, Arc<Mesh>)> {
        let path = self.files().get(self.viewing?)?.path.clone();
        let mesh = self.model.meshes.get(&path)?.clone();
        Some((path, mesh))
    }

    /// Keys of the viewer on a model: arrows turn it (held: it keeps turning), + − and 0 zoom,
    /// Ctrl+S saves the picture. The other keys move among the files as for images.
    pub(crate) fn model_keys(&mut self, ctx: &egui::Context) {
        let (dx, dy, zoom, reset) = ctx.input(|i| {
            let held = |k| if i.key_down(k) && !i.modifiers.command { 1.0 } else { 0.0 };
            // The real time since the last frame: a frame the system skips makes a longer step, not a slower turn.
            let dt = i.unstable_dt.min(0.1) * TURN_SPEED;
            let key = |k| i.key_pressed(k) && !i.modifiers.command;
            let zoom = if key(Key::Plus) || key(Key::Equals) { 1.25 } else if key(Key::Minus) { 0.8 } else { 1.0 };
            ((held(Key::ArrowRight) - held(Key::ArrowLeft)) * dt, (held(Key::ArrowUp) - held(Key::ArrowDown)) * dt, zoom, key(Key::Num0))
        });
        let save = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::S) || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::S));
        if dx != 0.0 || dy != 0.0 {
            self.model.view.turn(dx, dy);
            ctx.request_repaint();
        }
        self.model.view.zoom_by(zoom);
        if reset {
            self.model.view = View::default();
        }
        if save {
            self.act = Some(crate::actions::Act::Convert(None));
        }
    }

    /// "Save in another format" from the viewer on a model: the picture as shown, at least
    /// `SHOT_SIDE` on the long side. `false` if no model is open.
    pub(crate) fn convert_model(&mut self, path: &str) -> bool {
        let Some((viewed, mesh)) = self.viewed_mesh().filter(|(p, _)| p == path) else { return false };
        let Some(loader) = &self.loader else { return false };
        let [w, h] = self.model.size.map(|v| v.max(1) as f32);
        let k = (SHOT_SIDE / w.max(h)).max(1.0);
        let size = self.files().iter().find(|f| f.path == viewed).map_or(0, |f| f.size);
        loader.model_original(mesh, self.model.view, [(w * k) as u32, (h * k) as u32], size);
        true
    }

    /// The model drawn by the graphics card in the whole viewer, on the background of `render3d`;
    /// `None` until it is read or without a card (the viewer shows its still picture).
    pub(crate) fn model_view(&mut self, ui: &mut egui::Ui) -> Option<egui::Response> {
        let (path, mesh) = self.viewed_mesh()?;
        self.model.gpu.as_ref()?;
        if self.model.of.as_ref() != Some(&path) {
            (self.model.of, self.model.view) = (Some(path), View::default());
        }
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.model_mouse(ui, &response);
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        self.model.size = size;
        let view = self.model.view;
        let id = self.model.gpu.as_mut()?.draw(&mesh, &view, size);
        let painter = ui.painter_at(rect);
        let [top, bottom] = render3d::BACKGROUND.map(|[r, g, b]| egui::Color32::from_rgb(r, g, b));
        let mut sky = egui::Mesh::default();
        sky.colored_vertex(rect.left_top(), top);
        sky.colored_vertex(rect.right_top(), top);
        sky.colored_vertex(rect.right_bottom(), bottom);
        sky.colored_vertex(rect.left_bottom(), bottom);
        sky.add_triangle(0, 1, 2);
        sky.add_triangle(0, 2, 3);
        painter.add(sky);
        painter.image(id, rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        self.zoom_shown = view.zoom;
        self.count_frame(ui.input(|i| i.time));
        Some(response)
    }

    /// Frames drawn in the last second; with `EYECANSEE_LOG=1` the slow ones (> 50 ms) are logged.
    fn count_frame(&mut self, now: f64) {
        let f = &mut self.model.frames;
        if let Some(&last) = f.back()
            && now - last > 0.05
            && now - last < 1.0
        {
            log!("3D view: slow frame, {:.0} ms", (now - last) * 1000.0);
        }
        f.push_back(now);
        while f.front().is_some_and(|&t| now - t > 1.0) {
            f.pop_front();
        }
    }

    /// Frames per second while the model moves; `None` when still (no frames are drawn then).
    pub(crate) fn model_fps(&self) -> Option<usize> {
        let f = &self.model.frames;
        (f.len() > 2).then_some(f.len())
    }

    /// Dragging turns the model (with Shift or the middle button it moves it), the wheel zooms.
    fn model_mouse(&mut self, ui: &egui::Ui, response: &egui::Response) {
        let short = response.rect.width().min(response.rect.height()).max(1.0);
        let d = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);
        if response.dragged_by(egui::PointerButton::Middle) || (shift && response.dragged_by(egui::PointerButton::Primary)) {
            self.model.view.pan[0] += d.x / short;
            self.model.view.pan[1] += d.y / short;
        } else if response.dragged_by(egui::PointerButton::Primary) {
            self.model.view.turn(d.x * DRAG_TURN, d.y * DRAG_TURN);
        }
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if response.hovered() {
            let f = ui.input(|i| {
                i.events.iter().fold(1.0, |f, e| match e {
                    egui::Event::MouseWheel { unit, delta, .. } => f * 1.15f32.powf(crate::viewer::notches(*unit, *delta)),
                    egui::Event::Zoom(z) => f * z,
                    _ => f,
                })
            });
            self.model.view.zoom_by(f);
        }
    }
}
