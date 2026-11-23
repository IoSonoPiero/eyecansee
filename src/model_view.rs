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
    pub(crate) of: Option<String>,
    pub(crate) view: View,
    /// The graphics card, from eframe; without it the processor draws the model (`model_cpu`).
    pub(crate) gpu: Option<Gpu>,
    pub(crate) cpu: crate::model_cpu::CpuView,
    /// Size of the picture on screen, in pixels, for the screenshot's proportions.
    size: [u32; 2],
    /// CAD drawings, which the viewer also draws itself (`cad_view`).
    pub(crate) cad: crate::cad_view::State,
    /// The Measure tool, on drawings and models (`measure_view`).
    pub(crate) measure: crate::measure_view::State,
    /// When the last frames were drawn (seconds), for the frame rate in the status bar.
    frames: std::collections::VecDeque<f64>,
    /// When a section's plane was last moved (egui time): it is drawn for a moment after.
    pub(crate) section_moved: f64,
    /// The presentation was chosen (`pace`), and is mailbox; when the last frame began.
    presented: bool,
    mailbox: bool,
    last_frame: Option<std::time::Instant>,
    /// The refresh rate of the screen under the window, last seen (for the log).
    screen_hz: f64,
}

impl App {
    /// With eframe's graphics card, for the 3D viewer.
    pub(crate) fn with_gpu(mut self, cc: &eframe::CreationContext) -> Self {
        if let Some(rs) = &cc.wgpu_render_state {
            let i = rs.adapter.get_info();
            log!("graphics card: {} ({:?}, {:?}, driver {} {})", i.name, i.device_type, i.backend, i.driver, i.driver_info);
        }
        self.model.gpu = cc.wgpu_render_state.clone().map(Gpu::new);
        // Pictures shown smaller than their pixels, filtered on the card.
        if let Some(rs) = &cc.wgpu_render_state {
            crate::smooth_gpu::init(rs, &cc.egui_ctx);
        }
        // To try the viewer of a computer without a usable graphics card.
        if std::env::var_os("EYECANSEE_NO_GPU_3D").is_some() {
            log!("graphics card: not used for 3D (EYECANSEE_NO_GPU_3D)");
            self.model.gpu = None;
        }
        self
    }

    /// At the first frame, mailbox presentation where the card offers it (`render_gpu::present_mode`);
    /// then, with mailbox, no more frames a second than the screen under the window shows (it
    /// never waits for the screen, and frames it cannot show keep the card busy for nothing), and
    /// at most 120 outside the 3D viewer. A 3D model turns as smoothly as the screen allows:
    /// 60, 144, 240...
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
        if s.mailbox {
            let screen = frame.winit_window().and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).map_or(120.0, |mhz| f64::from(mhz) / 1000.0).clamp(30.0, 1000.0);
            if screen != s.screen_hz {
                s.screen_hz = screen;
                log!("screen: {screen:.0} Hz");
            }
            let rate = if model { screen } else { screen.min(120.0) };
            let min = std::time::Duration::from_secs_f64(1.0 / rate);
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
    /// Ctrl+Shift+S saves the picture. The other keys move among the files as for images.
    pub(crate) fn model_keys(&mut self, ctx: &egui::Context) {
        if let Some((_, mesh)) = self.viewed_mesh() {
            self.section_keys(ctx, mesh.z_up);
        }
        let (dx, dy, zoom, reset) = ctx.input(|i| {
            let held = |k| if i.key_down(k) && !i.modifiers.command { 1.0 } else { 0.0 };
            // The real time since the last frame: a frame the system skips makes a longer step, not a slower turn.
            // Not at the press: the frame before may be long ago (nothing to draw), and the model jumped.
            let arrows = [Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown];
            let pressed = i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, repeat: false, .. } if arrows.contains(key)));
            let dt = if pressed { i.predicted_dt } else { i.unstable_dt.min(0.1) } * TURN_SPEED;
            let key = |k| i.key_pressed(k) && !i.modifiers.command;
            let zoom = if key(Key::Plus) || key(Key::Equals) { 1.25 } else if key(Key::Minus) { 0.8 } else { 1.0 };
            ((held(Key::ArrowRight) - held(Key::ArrowLeft)) * dt, (held(Key::ArrowUp) - held(Key::ArrowDown)) * dt, zoom, key(Key::Num0))
        });
        // Ctrl+S is "over the original", which a model is not (File > Save says why): Ctrl+Shift+S only.
        let save = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::S));
        if dx != 0.0 || dy != 0.0 {
            self.model.view.turn(dx, dy);
            ctx.request_repaint();
        }
        self.model.view.zoom_by(zoom);
        if reset {
            // The starting view, the section kept.
            self.model.view = View { cut: self.model.view.cut, ..View::default() };
        }
        if save {
            self.act = Some(crate::actions::Act::Convert(None));
        }
    }

    /// "Save in another format" from the viewer on a model: the picture as shown, at least
    /// `SHOT_SIDE` on the long side. `false` if no model is open.
    pub(crate) fn convert_model(&mut self, path: &str, asked: u64) -> bool {
        let Some((viewed, mesh)) = self.viewed_mesh().filter(|(p, _)| p == path) else { return false };
        let Some(loader) = &self.loader else { return false };
        let [w, h] = self.model.size.map(|v| v.max(1) as f32);
        let k = (SHOT_SIDE / w.max(h)).max(1.0);
        let size = self.files().iter().find(|f| f.path == viewed).map_or(0, |f| f.size);
        loader.model_original(asked, mesh, self.model.view, [(w * k) as u32, (h * k) as u32], size);
        true
    }

    /// The model drawn by the graphics card in the whole viewer, on the background of `render3d`;
    /// `None` until it is read or without a card (the viewer shows its still picture).
    pub(crate) fn model_view(&mut self, ui: &mut egui::Ui) -> Option<egui::Response> {
        let (path, mesh) = self.viewed_mesh()?;
        if self.model.of.as_ref() != Some(&path) {
            (self.model.of, self.model.view) = (Some(path.clone()), View::default());
            self.model.measure.pts3.clear();
        }
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        self.model_mouse(ui, &response);
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
        self.model.size = size;
        // The cut filled only on a closed model, as the measure of its volume finds it.
        if self.model.view.cut.is_some() {
            let closed = self.model_sizes(ui.ctx(), &path, &mesh).is_some_and(|s| s.open_edges == 0);
            if let Some(c) = &mut self.model.view.cut {
                c.fill = closed;
            }
        }
        let view = self.model.view;
        // Without a graphics card, drawn on the processor (`model_cpu`).
        let id = match self.model.gpu.as_mut() {
            Some(gpu) => Some(gpu.draw(&mesh, &view, size)),
            None => self.model_cpu_picture(ui.ctx(), &path, &mesh, view, size),
        };
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
        if let Some(id) = id {
            crate::paint::paint_frame(&painter, id, rect, Default::default());
        }
        axes_corner(&painter, rect, render3d::axes(&view, mesh.z_up));
        self.section_plane(&painter, rect, &mesh, &view);
        self.model_measure(ui, &response, &path, &mesh);
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
        // Not in the guide's pictures: the number is the computer's, not the program's.
        #[cfg(feature = "shots")]
        if crate::shots::active() {
            return None;
        }
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
        // Alt and the wheel move a section's plane instead.
        if !self.section_wheel(ui, response.hovered()) && response.hovered() {
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

impl App {
    /// "Export as a 3D model": the models among the files chosen, each next to itself, with its
    /// name and a free one if taken (also by another of them: `a.stl` and `a.obj` to GLB).
    pub(crate) fn export_model(&mut self, path: Option<String>, kind: crate::model_export::Kind) {
        let paths = self.files_chosen(path).into_iter().filter(|p| crate::model::is_model(p));
        let Some(loader) = &self.loader else { return };
        let mut taken: std::collections::HashMap<String, std::collections::HashSet<String>> = Default::default();
        let mut jobs = Vec::new();
        for path in paths {
            let dir = crate::source::parent(&path);
            let names = taken.entry(dir.clone()).or_insert_with(|| match self.listings.get(&dir) {
                Some(Ok(l)) => l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).map(|p| crate::source::name(p).trim_end_matches('/').to_string()).collect(),
                _ => Default::default(),
            });
            let name = crate::source::name(&path);
            let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
            let free = crate::source::free_name(names, &format!("{stem}.{}", kind.ext()));
            names.insert(free.clone());
            let to = format!("{dir}{free}");
            log!("export {path:?} as {to:?}");
            jobs.push((path, to));
        }
        // Told in the window already (`model_export_page::beyond_line`), not again at the end.
        let limit = crate::model_limit::of_export(kind);
        let read = |p: &String| self.model.meshes.get(p).is_some_and(|m| limit.check(m.tris.len()).is_err());
        self.export_said.told.extend(jobs.iter().map(|(p, _)| p).filter(|p| read(p)).cloned());
        self.export_said.start(jobs.len());
        loader.export_models(jobs, kind);
    }

    /// The exported model: its new name in the status bar for a while, the folder read again. What
    /// needs reading is said in one message once the whole export is done (`ExportSaid`): a model
    /// not exported (it was a line in the status bar that went after six seconds, and with
    /// several models only the last one's), and a glTF that placed a part more than once (#179:
    /// a line in the status bar stayed on the other files, Piero, 21/11).
    pub(crate) fn on_exported(&mut self, ctx: &egui::Context, r: Result<(String, String, Vec<&'static str>), (String, String)>) {
        let (mut again, mut failed) = (None, None);
        match &r {
            Ok((to, from, notes)) => {
                self.relist(crate::source::parent(to));
                if !notes.is_empty() {
                    log!("export {from:?}: said of the new file {notes:?}");
                }
                again = notes.contains(&crate::model_gltf::NODE_AGAIN).then(|| crate::source::name(from).to_string());
                if notes.contains(&crate::model_limit::BEYOND_EXPORT) {
                    self.export_said.beyond(from, to);
                }
                self.status_note = Some(self.i18n.t_args("status-exported", &[("name", crate::source::name(to))]));
                self.note_until = Some(ctx.input(|i| i.time) + 6.0);
            }
            Err((from, e)) => {
                log!("export {from:?}: ERROR {e}");
                failed = Some((crate::source::name(from).to_string(), e.clone()));
            }
        }
        if let Some(said) = self.export_said.done(again, failed) {
            self.notice = export_notice(&self.i18n, &said).or(self.notice.take());
        }
    }
}

/// The models of an export still to come, and what there is to say of those done (`Said`): said
/// all together when the last one is done; the models already told in the window to be beyond
/// the cap of their new format (`told`).
#[derive(Default)]
pub(crate) struct ExportSaid {
    left: usize,
    said: Said,
    told: std::collections::HashSet<String>,
}

/// What the end of an export says: the models not exported, each with its error as the loader
/// gave it, the names of those whose glTF placed a part more than once in its hierarchy (#179),
/// and those exported into a file EyeCanSee will not open, with the cap of its format
/// (`model_limit::Limit::error`).
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Said {
    pub(crate) failed: Vec<(String, String)>,
    pub(crate) again: Vec<String>,
    pub(crate) beyond: Vec<(String, String)>,
}

impl ExportSaid {
    /// `n` models more to export; with an export still running, one message at the end of both.
    pub(crate) fn start(&mut self, n: usize) {
        self.left += n;
    }

    /// The model `from` exported into `to`, a file beyond the cap of its format: said at the end,
    /// unless the window said it before (`told`).
    pub(crate) fn beyond(&mut self, from: &str, to: &str) {
        let kind = to.rsplit_once('.').and_then(|(_, ext)| crate::model_export::Kind::of_ext(ext));
        if let Some(kind) = kind.filter(|_| !self.told.remove(from)) {
            self.said.beyond.push((crate::source::name(from).to_string(), crate::model_limit::of_export(kind).error()));
        }
    }

    /// A model exported (`again`: its name, if it placed a part more than once) or not (`failed`:
    /// its name and why); what to say once it was the last, if anything.
    pub(crate) fn done(&mut self, again: Option<String>, failed: Option<(String, String)>) -> Option<Said> {
        self.left = self.left.saturating_sub(1);
        self.said.again.extend(again);
        self.said.failed.extend(failed);
        (self.left == 0 && self.said != Said::default()).then(|| std::mem::take(&mut self.said))
    }
}

/// The message at the end of an export with something to say (`notice_list::told`, as the end
/// of a copy): the models not exported, each with why in a few words when it is known (`brief_load`;
/// the whole sentence, as the viewer says a file that does not open, and the library's text
/// only in Copy: `batch_summary::item`), and the glTF models that placed a part more than once
/// (#179); both in one window, each under its heading.
pub(crate) fn export_notice(i18n: &crate::i18n::I18n, said: &Said) -> Option<crate::save_dialogs::Notice> {
    use crate::notice_list::{Entry, Part};
    let failed = said.failed.iter().map(|(name, e)| {
        let whole = crate::batch_summary::item(i18n, crate::fault::NOT_OPENED, &format!("{name}\u{1}{e}"), false).1;
        Entry { name: name.clone(), why: i18n.brief_load(e).unwrap_or_default(), whole: Some(crate::save_dialogs::plain(&whole)) }
    });
    let beyond = said.beyond.iter().map(|(name, e)| Entry { name: name.clone(), why: i18n.brief_load(e).unwrap_or_default(), whole: None });
    let parts = vec![
        Part::of(i18n, "model-export-failed-heading", "", failed.collect()),
        Part::named(i18n, "model-export-again-heading", "model-export-again-why", &said.again),
        Part::of(i18n, "model-export-beyond-heading", "model-export-beyond-why", beyond.collect()),
    ];
    (*said != Said::default()).then(|| crate::notice_list::told(i18n, i18n.t("model-export-done-title"), "", parts))
}

/// The axes of the file in the top right corner, as CAD programs show them: X red, Y green, Z
/// blue, turning with the model; those pointing away are fainter and drawn first.
fn axes_corner(painter: &egui::Painter, rect: egui::Rect, axes: [[f32; 3]; 3]) {
    const LEN: f32 = 38.0;
    let centre = rect.right_top() + egui::vec2(-62.0, 62.0);
    painter.circle_filled(centre, LEN + 16.0, egui::Color32::from_black_alpha(70));
    let colors = [egui::Color32::from_rgb(235, 80, 80), egui::Color32::from_rgb(100, 210, 100), egui::Color32::from_rgb(90, 150, 255)];
    let mut order = [0, 1, 2];
    order.sort_by(|&a, &b| axes[a][2].total_cmp(&axes[b][2]));
    for i in order {
        let [x, y, z] = axes[i];
        let tip = centre + egui::vec2(x, y) * LEN;
        let color = if z < -0.05 { colors[i].gamma_multiply(0.55) } else { colors[i] };
        painter.line_segment([centre, tip], egui::Stroke::new(3.0, color));
        painter.circle_filled(tip, 3.0, color);
        let label = centre + egui::vec2(x, y) * (LEN + 10.0);
        painter.text(label, egui::Align2::CENTER_CENTER, ["X", "Y", "Z"][i], egui::FontId::proportional(14.0), color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    /// Three models, the second failed: the names of the first and the third, in their order, and
    /// the second with why, only once the last is done; an export with nothing to say says
    /// nothing; a second export started while the first runs is said with it, at the end of both.
    #[test]
    fn said_at_the_end() {
        let again = |names: &[&str]| Some(Said { failed: Vec::new(), again: names.iter().map(|n| n.to_string()).collect(), ..Default::default() });
        let mut s = ExportSaid::default();
        s.start(3);
        assert_eq!(s.done(Some("a.gltf".into()), None), None);
        assert_eq!(s.done(None, Some(("b.gltf".into(), "cut short".into()))), None);
        assert_eq!(s.done(Some("c.gltf".into()), None), Some(Said { failed: vec![("b.gltf".into(), "cut short".into())], again: vec!["a.gltf".into(), "c.gltf".into()], ..Default::default() }));
        s.start(2);
        assert_eq!((s.done(None, None), s.done(None, None)), (None, None));
        s.start(1);
        assert_eq!(s.done(Some("x.gltf".into()), None), again(&["x.gltf"]));
        s.start(2);
        assert_eq!(s.done(Some("a.gltf".into()), None), None);
        s.start(1);
        assert_eq!(s.done(None, None), None);
        assert_eq!(s.done(Some("b.gltf".into()), None), again(&["a.gltf", "b.gltf"]));
        // A message more than was waited for leaves nothing behind.
        assert_eq!(s.done(None, None), None);
        // The only model, not exported: said at once.
        s.start(1);
        assert_eq!(s.done(None, Some(("m.step".into(), "no".into()))), Some(Said { failed: vec![("m.step".into(), "no".into())], again: Vec::new(), ..Default::default() }));
    }

    /// One model exported with a change: the title says the whole, the heading counts in the
    /// singular; twelve: ten names in their order, then "and 2 more" as the last item, all in
    /// Copy; in both languages.
    #[test]
    fn one_window_for_all() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let md = crate::save_dialogs::md;
        let again = |names: &[String]| Said { failed: Vec::new(), again: names.to_vec(), ..Default::default() };
        let one = export_notice(&it, &again(&["nodo_doppio.gltf".to_string()])).unwrap();
        assert!(one.warning);
        assert_eq!(one.title, "Esportazione finita");
        let why = "Metteva una sua parte più volte nella gerarchia, cosa che glTF non permette: nel file nuovo c'è una volta sola.";
        assert_eq!(one.text, format!("**1 modello esportato con una modifica{}**\n{why}\n- {}", md(":"), crate::notice_list::item("nodo_doppio.gltf")));
        assert_eq!(one.copy.as_deref(), Some(format!("1 modello esportato con una modifica:\n{why}\n- nodo_doppio.gltf").as_str()));
        assert_eq!(export_notice(&en, &again(&["a.gltf".to_string()])).unwrap().title, "Export finished");
        let names: Vec<String> = (1..=12).rev().map(|i| format!("m{i}.gltf")).collect();
        for (i18n, heading, more) in [(&en, "12 models exported with a change:", "and 2 more"), (&it, "12 modelli esportati con una modifica:", "e altri 2")] {
            let many = export_notice(i18n, &again(&names)).unwrap();
            assert!(many.text.starts_with(&format!("**{}**\n", md(heading))), "{}", many.text);
            assert!(many.text.ends_with(&format!("\n- {}\n- {}\n- {more}", md("m9.gltf"), md("m10.gltf"))) && !many.text.contains("m11"), "{}", many.text);
            let copy = many.copy.unwrap();
            assert!(copy.starts_with(heading) && copy.ends_with("- m10.gltf\n- m11.gltf\n- m12.gltf") && !copy.contains(more), "{copy}");
        }
        assert!(export_notice(&en, &again(&names)).unwrap().text.contains("\nThey placed one of their parts more than once in their hierarchy, which glTF does not allow: the new files have it only once.\n"));
        assert_eq!(export_notice(&en, &Said::default()).map(|n| n.title), None);
    }

    /// Exported from the grid into files beyond the cap of their format (Piero, 22/11/2026): said
    /// at the end with the cap, in a few words; not the one the window told already.
    #[test]
    fn beyond_told_at_the_end_d1() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let plain = crate::save_dialogs::plain;
        let mut s = ExportSaid::default();
        s.told.insert("d/seen.ply".into());
        s.start(3);
        s.beyond("d/big.ply", "d/big.obj");
        assert_eq!(s.done(None, None), None);
        s.beyond("d/seen.ply", "d/seen.obj");
        assert_eq!(s.done(None, None), None);
        s.beyond("d/huge.ply", "d/huge (2).glb");
        let said = s.done(None, None).unwrap();
        assert_eq!(said.beyond, [("big.ply".to_string(), crate::model_limit::OBJ.error()), ("huge.ply".to_string(), crate::model_limit::GLB.error())]);
        assert!(s.told.is_empty());
        let n = export_notice(&en, &said).unwrap();
        assert!(n.warning);
        assert_eq!(plain(&n.text), "2 models exported to files that EyeCanSee will not be able to open:\nThey have more triangles than EyeCanSee can open in that format.\n- big.ply: more than 10 million triangles\n- huge.ply: more than 20 million triangles");
        let one = Said { beyond: vec![("big.ply".into(), crate::model_limit::OBJ.error())], ..Default::default() };
        assert_eq!(plain(&export_notice(&it, &one).unwrap().text), "1 modello esportato in un file che EyeCanSee non potrà aprire:\nHa più triangoli di quanti EyeCanSee ne possa aprire in quel formato.\n- big.ply: più di 10 milioni di triangoli");
    }

    /// The models not exported, as Piero had them (21/11): a glTF without its data file, one with
    /// data damaged inside, one with data outside its folder, a GLB without its block, with the
    /// errors as the loader gives them. Each is a line of the list, in the order of the names:
    /// the name and why in a few words, not "Cannot open …" on every line; Copy has the whole
    /// sentences. An engine that is missing and a folder that cannot be written have their few
    /// words too; the text of a library is not shown (the name alone), and Copy has it. With
    /// models exported with a change, one window, each part under its heading.
    #[test]
    fn not_exported_told() {
        use crate::model_gltf::{DAMAGED_INSIDE, INCOMPLETE, MISSING_BIN, OUTSIDE_BIN};
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let plain = crate::save_dialogs::plain;
        let said = |errors: &[(&str, String)]| Said { failed: errors.iter().map(|(n, e)| (n.to_string(), e.clone())).collect(), again: Vec::new(), ..Default::default() };
        let four = [("senza-bin.gltf", format!("{MISSING_BIN}\u{1}senza-bin.bin")), ("dati-rovinati.gltf", format!("{DAMAGED_INSIDE}\u{1}")), ("fuori.gltf", format!("{OUTSIDE_BIN}\u{1}../dati.bin")), ("incompleto.glb", format!("{INCOMPLETE}\u{1}"))];
        let n = export_notice(&it, &said(&four)).unwrap();
        assert!(n.warning);
        assert_eq!(n.title, "Esportazione finita");
        assert_eq!(plain(&n.text), "4 modelli non esportati:\n- dati-rovinati.gltf: i dati scritti dentro il file sono rovinati\n- fuori.gltf: i dati sono fuori dalla cartella del modello (../dati.bin)\n- incompleto.glb: manca il blocco con i dati\n- senza-bin.gltf: manca il file senza-bin.bin");
        assert!(n.text.starts_with("**4 modelli non esportati\\:**\n- dati\\-rovinati"), "{}", n.text);
        assert_eq!(n.copy.as_deref(), Some("4 modelli non esportati:\n- Impossibile aprire dati-rovinati.gltf: I dati del modello scritti dentro il file sono rovinati\n- Impossibile aprire fuori.gltf: Il file con i dati del modello non viene letto: EyeCanSee non legge file fuori dalla cartella del modello né da internet. ../dati.bin\n- Impossibile aprire incompleto.glb: Il file è incompleto: manca il blocco con i dati del modello\n- Impossibile aprire senza-bin.gltf: Manca il file con i dati del modello: senza-bin.bin"));
        let en_four = export_notice(&en, &said(&four)).unwrap();
        assert_eq!(plain(&en_four.text), "4 models not exported:\n- dati-rovinati.gltf: the data written inside the file is damaged\n- fuori.gltf: the data is outside the model's folder (../dati.bin)\n- incompleto.glb: the block with the data is missing\n- senza-bin.gltf: the file senza-bin.bin is missing");
        let one = export_notice(&it, &said(&[("a*.gltf", format!("{MISSING_BIN}\u{1}dati.bin"))])).unwrap();
        assert_eq!(plain(&one.text), "1 modello non esportato:\n- a*.gltf: manca il file dati.bin");
        assert!(one.text.contains("a\\*"), "{}", one.text);
        let denied = "PermissionDenied (permanent) at write, context: { service_operation: OpenForWrite, service: fs, path: m/c.stl, size: 84, written: 0 } => permission denied, source: Permission denied (os error 13)".to_string();
        let other = export_notice(&en, &said(&[("d.step", crate::occt::MISSING.into()), ("b.glb", "unexpected end of file".into()), ("c.step", denied)])).unwrap();
        assert_eq!(plain(&other.text), "3 models not exported:\n- b.glb\n- c.step: access denied\n- d.step: the CAD engine is missing");
        let copy = other.copy.unwrap();
        assert!(copy.contains("\n- Cannot open b.glb (unexpected end of file)\n") && copy.contains("c.stl") && !copy.contains("PermissionDenied") && copy.ends_with(&format!("\n- Cannot open d.step: {}", en.t("cad-engine-missing"))), "{copy}");
        let many: Vec<(String, String)> = (1..=12).map(|i| (format!("m{i}.glb"), "cut".to_string())).collect();
        let both = export_notice(&en, &Said { failed: many, again: vec!["a.gltf".into()], ..Default::default() }).unwrap();
        assert_eq!(both.title, "Export finished");
        assert!(plain(&both.text).contains("\n- m10.glb\n- and 2 more\n\n1 model exported with a change:\nIt placed one of its parts") && !both.text.contains("m11"), "{}", both.text);
        let copy = both.copy.unwrap();
        assert!(copy.contains("\n- Cannot open m12.glb (cut)\n\n1 model exported with a change:\n") && copy.ends_with("\n- a.gltf"), "{copy}");
    }
}
