//! Viewer: the image at full window, zoom and pan, its keys, the images read ahead.

use eframe::egui::{self, Key};

use crate::navigation::Go;
use crate::actions::Act;
use crate::{AROUND, App, browser, paint, source, zoom};

/// The window is at full screen (F11).
pub(crate) fn full_screen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

/// Asks for full screen (#171). On Windows the window is as large as the monitor a moment before
/// its first picture of that size is ready (33 ms on a fast card, 50 to 130 on a laptop's), and
/// until then Windows shows the last picture drawn, stretched over the new size (DirectX 12, chosen
/// for this in `render_gpu::adapter_selector`): the small window with its bars over the whole
/// screen, which flashed. So two frames are drawn first as the full screen will be, squeezed into
/// the window (`ahead`), and stretched they are the full screen's picture, only less sharp. Two, a
/// refresh apart (`pace`): one alone was not always shown before the window changed size (a
/// playing video). Recorded on 18/11. With Vulkan (chosen in the Options, or no DirectX 12) the
/// last picture is not always stretched: at once, as up to 0.76.2. On Linux too, where the
/// desktop waits for the picture of the new size.
pub(crate) fn enter_full(ctx: &egui::Context) {
    ask_full(ctx, crate::render_gpu::stretches());
}

/// `enter_full`: through the frames drawn `ahead`, or at once.
fn ask_full(ctx: &egui::Context, ahead: bool) {
    if ahead {
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("enter-full"), 2u8));
        ctx.request_repaint();
    } else {
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
    }
}

/// Once a frame: the frames left before full screen (`enter_full`); with the last one the window
/// is told to change (eframe runs the commands after presenting the frame).
pub(crate) fn full_step(ctx: &egui::Context) {
    let id = egui::Id::unique("enter-full");
    match ctx.data_mut(|d| d.remove_temp::<u8>(id)) {
        Some(left) if left > 1 => {
            ctx.data_mut(|d| d.insert_temp(id, left - 1));
            ctx.request_repaint();
        }
        // Kept, at 0, for this frame's `bare` and `ahead`; gone at the next.
        Some(1) => {
            ctx.data_mut(|d| d.insert_temp(id, 0u8));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        _ => {}
    }
}

/// In the frames before full screen (`enter_full`).
fn entering(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<u8>(egui::Id::unique("enter-full"))).is_some()
}

/// The viewer without its bars: at full screen, and in the frames before it.
pub(crate) fn bare(ctx: &egui::Context) -> bool {
    full_screen(ctx) || entering(ctx)
}

/// `draw` in `ui`; in the frames before full screen, laid out for the whole monitor and its
/// pictures squeezed into the window (`paint::set_squeeze`).
pub(crate) fn ahead<R>(ui: &mut egui::Ui, draw: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let monitor = ctx.input(|i| i.viewport().monitor_size).filter(|_| entering(&ctx) && !full_screen(&ctx));
    let Some(monitor) = monitor else { return draw(ui) };
    let whole = egui::Rect::from_min_size(egui::Pos2::ZERO, monitor);
    paint::set_squeeze(&ctx, Some(paint::squeeze(ctx.content_rect().size(), monitor)));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(whole));
    child.set_clip_rect(whole);
    let drawn = draw(&mut child);
    paint::set_squeeze(&ctx, None);
    drawn
}

impl App {
    pub(crate) fn viewer_keys(&mut self, ctx: &egui::Context) {
        // A modal window (Rename, Delete…) has the keys: its Esc closed the viewer and was taken,
        // the window stayed open over the grid (Piero, 17/11). Egui drops the text field's focus
        // on Esc before we read it, so the focus alone does not say it.
        if ctx.egui_wants_keyboard_input() || ctx.memory(|m| m.top_modal_layer().is_some()) {
            return;
        }
        // M, and while measuring Esc, Enter, Backspace and D, A, G: taken before the keys below.
        self.measure_keys(ctx);
        // Space on an animation, Page Up and Down on pages: theirs, taken before the keys below.
        self.frames_keys(ctx);
        // Space on a Motion Photo plays its video, which then has the keys of a video.
        self.motion_keys(ctx);
        // On a video Space plays and pauses, as in a player.
        let video = self.viewing_video() || self.playing_motion();
        // On a 3D model the arrows turn it.
        let model = self.viewed_mesh().is_some();
        // On a video Ctrl and the arrows jump back and forward (issue #10), not to another file: taken
        // first, so the arrows left below are those without Ctrl.
        if video {
            let jumps = ctx.input_mut(take_jumps);
            if jumps != 0 {
                self.video_jump(jumps as f64 * crate::video::JUMP);
            }
        }
        let (next, prev, first, last, close, space) = ctx.input(|i| {
            let any = |keys: &[Key]| keys.iter().any(|k| i.key_pressed(*k));
            (
                (i.key_pressed(Key::ArrowRight) && !model) || i.key_pressed(Key::PageDown) || (i.key_pressed(Key::Space) && !video),
                (i.key_pressed(Key::ArrowLeft) && !model) || any(&[Key::Backspace, Key::PageUp]),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Space) && video,
            )
        });
        if space {
            self.toggle_video();
        }
        for (pressed, to) in [(next, Go::Next), (prev, Go::Prev), (first, Go::First), (last, Go::Last)] {
            if pressed {
                self.go(to);
            }
        }
        // Taken: the page under the viewer (Find duplicates' results) read the same Esc in the same
        // pass and closed too (Piero, 17/11).
        if close {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        }
        // Esc in full screen leaves it first, with the bars back; then the viewer.
        if close && full_screen(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        } else if close {
            self.close_viewer();
        }
        if video {
            // A turned video is saved as an image (issue #125): Ctrl+S and Ctrl+Shift+S before
            // the return below, which left them unread on a video.
            self.save_keys(ctx);
            // Its turns not saved yet are taken back and done again as a picture's (#167).
            if let Some(current) = self.viewing {
                self.turn_undo_keys(ctx, current);
            }
            // Up and down the volume, M sound off and on, as in the players.
            let (up, down, mute) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::M) && !i.modifiers.command));
            if up || down {
                self.video_volume(if up { 1 } else { -1 });
            }
            if mute {
                self.toggle_mute();
            }
            return;
        }
        if model {
            return self.model_keys(ctx);
        }
        if let Some(current) = self.viewing {
            self.cad_keys(ctx, current);
            self.turn_undo_keys(ctx, current);
        }
        // Ctrl+R and Ctrl+Shift+R: in `global_keys`, for the grid too.
        self.save_keys(ctx);
    }

    /// Ctrl+S and Ctrl+Shift+S: Ctrl+Shift+S only where Save as… is on (not on a video or an audio
    /// file, which it would read as a picture); Ctrl+S says itself why it cannot save.
    fn save_keys(&mut self, ctx: &egui::Context) {
        match ctx.input_mut(take_save) {
            Some(Act::Convert(_)) if self.save_as_off().is_some() => {}
            Some(act) => self.act = Some(act),
            None => {}
        }
    }

    pub(crate) fn close_viewer(&mut self) {
        log!("viewer: closed");
        if self.hold_for_turn(crate::save_dialogs::After::Close) {
            return;
        }
        if let (Some(i), Some(Ok(l))) = (self.viewing.take(), self.listings.get(&self.dir)) {
            self.selected = l.dirs.len() + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
        self.full.clear();
        self.frames.read.clear();
        self.motion.read.clear();
        self.model.meshes.clear();
        // Opened again, a model starts from the default view, without the section left on.
        self.model.of = None;
        self.end_compare();
        self.model.cad.drawings.clear();
        self.zoom_of = None;
    }

    /// Enter and Open: with several images selected the first of them in the grid (the viewer
    /// then moves only among them), otherwise the item with the cursor.
    pub(crate) fn open_selected(&mut self) {
        match self.viewer_first_marked() {
            Some(i) => self.viewing = Some(i),
            None => self.activate(self.selected),
        }
    }

    fn viewer_first_marked(&self) -> Option<usize> {
        let marked: Vec<usize> = self.files().iter().enumerate().filter(|(_, f)| self.marked.contains(&f.path)).map(|(i, _)| i).collect();
        (marked.len() > 1).then(|| marked[0])
    }

    /// The images the viewer moves among, as positions in `files()`: the marked ones when it
    /// shows one of them and they are more than one (as the slideshow), otherwise the folder.
    pub(crate) fn viewer_set(&self) -> Vec<usize> {
        let files = self.files();
        let marked: Vec<usize> = files.iter().enumerate().filter(|(_, f)| self.marked.contains(&f.path)).map(|(i, _)| i).collect();
        if marked.len() > 1 && self.viewing.is_some_and(|v| marked.contains(&v)) { marked } else { (0..files.len()).collect() }
    }

    /// Requests the open image and its neighbours (among those it moves through), frees the
    /// distant ones.
    pub(crate) fn prefetch_full(&mut self, current: usize) {
        let set = self.viewer_set();
        let Some(at) = set.iter().position(|&i| i == current) else { return };
        let (Some(loader), Some(Ok(listing))) = (&self.loader, self.listings.get(&self.dir)) else { return };
        let n = set.len();
        let near = |r: usize| (0..=r).flat_map(move |d| [at + d, at + n * r - d]).map(|p| set[p % n]).collect::<Vec<_>>();
        for i in near(AROUND) {
            // A video's still is not needed here (the player shows it) and reading it would slow
            // down the player's start on a slow disk (dashcam on USB 2: 8 s, 2026-10-27).
            if crate::decode::is_played(&listing.files[i].path) {
                continue;
            }
            if !self.full.contains_key(&i) && self.pending.insert(i) {
                loader.load_full(self.generation, i, listing.files[i].path.clone(), false);
            }
        }
        let keep = near(AROUND + 1);
        self.full.retain(|i, _| keep.contains(i));
        let paths: Vec<&String> = keep.iter().filter_map(|&i| listing.files.get(i)).map(|f| &f.path).collect();
        self.model.meshes.retain(|p, _| paths.contains(&p));
        self.model.cad.drawings.retain(|p, _| paths.contains(&p));
        self.frames.read.retain(|p, _| paths.contains(&p));
        self.motion.read.retain(|p, _| paths.contains(&p));
    }

    pub(crate) fn viewer(&mut self, ui: &mut egui::Ui, current: usize) {
        if self.viewing_video() || self.playing_motion() {
            return self.video_view(ui, current);
        }
        self.motion_done();
        self.video = None;
        self.compare_left(current);
        let mut chosen = None;
        if let Some(response) = self.model_view(ui) {
            if response.double_clicked() && !self.model.measure.on {
                self.close_viewer();
            }
            response.context_menu(|ui| {
                chosen = browser::context_menu(ui, self.item_menu(None, None));
            });
            if chosen.is_some() {
                self.act = chosen;
            }
            return;
        }
        self.frames_tick(ui.ctx(), current);
        if let Some(Ok(tex)) = self.full.get(&current) {
            let tex = tex.clone();
            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            self.zoom_view(ui, &response, &tex, current);
            self.frames_bar(ui, rect);
            self.motion_bar(ui, rect);
            // While measuring a double click closes an area.
            if response.double_clicked() && !self.model.measure.on {
                self.close_viewer();
            }
            response.context_menu(|ui| {
                chosen = browser::context_menu(ui, self.item_menu(None, None));
            });
            if chosen.is_some() {
                self.act = chosen;
            }
            return;
        }
        ui.centered_and_justified(|ui| match self.full.get(&current) {
            Some(Ok(_)) => {}
            Some(Err(_)) if self.files().get(current).is_some_and(|f| crate::occt::needs_engine(&f.path)) => self.cad_engine_card(ui),
            // Too large for the memory: the question is asked over it (`view_memory`).
            Some(Err(e)) if crate::view_memory::need_of(e).is_some() => {}
            Some(Err(e)) => {
                let name = self.files().get(current).map_or("", |f| source::shown(&f.path));
                // The viewer is always dark: the theme's red was dark and small on it (2026-11-08).
                let text = egui::RichText::new(self.i18n.t_args("cannot-load", &[("file", name), ("error", &self.i18n.load_error(e))]));
                ui.add(egui::Label::new(text.size(18.0).color(egui::Color32::from_rgb(255, 140, 130))).wrap());
            }
            None => crate::theme::loading(ui, self.i18n.t("loading-file"), Some(egui::Color32::from_gray(220))),
        });
        if chosen.is_some() {
            self.act = chosen;
        }
    }

    /// The open image, fitted to the window or zoomed (as GNOME's viewer, Loupe): the wheel ×1.3 a
    /// notch around the pointer (or the next image, see `wheel_zoom`; Ctrl and the wheel always
    /// zoom), + and − ×1.5 around the centre, 0 fitted, 1 at 100%, dragging moves the zoomed image;
    /// the zoom typed, set with the slider or with − and + in the status bar (`zoom_bar`, #152). A picture
    /// goes below fitted, centred, down to `zoom::below_fit` (a drawing stops at fitted); never
    /// larger than `max_zoom`; percentages count the file's own pixels (`zoom`).
    #[allow(clippy::too_many_lines)] // to be split
    fn zoom_view(&mut self, ui: &egui::Ui, response: &egui::Response, tex: &crate::mipmap::Picture, current: usize) {
        let rect = response.rect;
        // A layout of a CAD drawing in place of its model, once drawn.
        let sheet = self.cad_sheet_texture(ui.ctx(), current);
        let tex = match &sheet {
            Some(t) => t,
            None if self.cad_sheet_waiting(current) => {
                egui::Spinner::new().paint_at(ui, egui::Rect::from_center_size(rect.center(), egui::vec2(32.0, 32.0)));
                ui.ctx().request_repaint();
                return;
            }
            None => tex,
        };
        let ppp = ui.ctx().pixels_per_point();
        if self.zoom_of != Some(current) {
            // Reached only once the turns were saved or dropped: leaving the image asks first (`hold_for_turn`).
            (self.zoom, self.zoom_of, self.turn) = (None, Some(current), Default::default());
        }
        // Two drawings compared: N and Shift+N zoom to a change.
        if let Some((zoom, pan)) = self.compare_jump(tex.size_vec2(), rect, ppp) {
            (self.zoom, self.pan) = (Some(zoom), pan);
        }
        let t = paint::turned_size(tex, self.turn);
        // A layout of a drawing has no file pixels: its percentages stay the texture's.
        let own = if sheet.is_some() { None } else { self.files().get(current).and_then(|f| self.own_sizes.get(&f.path).copied()) };
        let per = zoom::file_per_texture(own, tex.size());
        let cad = self.files().get(current).is_some_and(|f| crate::cad::is_cad(&f.path));
        let mut bounds = zoom::bounds((rect.width() * ppp / t.x).min(rect.height() * ppp / t.y), per, self.settings.max_zoom);
        if !cad {
            bounds = zoom::below_fit(bounds, per, t.x.min(t.y));
        }
        let fit = bounds.fit;
        let scale = self.zoom.unwrap_or(fit).clamp(bounds.low, bounds.max);

        let (mut wheel, mut next) = (0.0, 0.0);
        let mut pinch = 1.0;
        if response.hovered() {
            let wheel_zoom = self.settings.wheel_zoom;
            ui.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                            let notches = notches(*unit, *delta);
                            if wheel_zoom || modifiers.command {
                                wheel += notches;
                            } else {
                                next -= notches;
                            }
                        }
                        egui::Event::Zoom(f) => pinch *= f,
                        _ => {}
                    }
                }
            });
        }
        self.wheel_notches += next;
        while self.wheel_notches.abs() >= 1.0 {
            let forward = self.wheel_notches > 0.0;
            self.wheel_notches -= self.wheel_notches.signum();
            self.go(if forward { Go::Next } else { Go::Prev });
        }
        let asked = self.zoom_bar.ask.take();
        let (zoom_in, zoom_out, fitted, actual) = if ui.ctx().egui_wants_keyboard_input() {
            (false, false, false, false)
        } else {
            ui.input(|i| {
                let key = |k| i.key_pressed(k) && !i.modifiers.command;
                (key(Key::Plus) || key(Key::Equals), key(Key::Minus), key(Key::Num0), key(Key::Num1))
            })
        };
        // The − and + of the status bar do as the keys.
        let (zoom_in, zoom_out) = (zoom_in || asked == Some(zoom::Ask::In), zoom_out || asked == Some(zoom::Ask::Out));

        let mut target = scale * 1.3f32.powf(wheel) * pinch;
        let mut anchor = response.hover_pos().filter(|_| wheel != 0.0 || pinch != 1.0).unwrap_or(rect.center());
        if zoom_in || zoom_out {
            target = scale * if zoom_in { 1.5 } else { 1.0 / 1.5 };
            anchor = rect.center();
        }
        if actual {
            target = bounds.actual;
        }
        let target = match asked {
            Some(zoom::Ask::To(asked)) => {
                anchor = rect.center();
                zoom::reach(asked * per, bounds)
            }
            _ => zoom::settle(scale, target, bounds),
        };
        if fitted || target == fit {
            (self.zoom, self.pan) = (None, t / 2.0);
        } else if target != scale {
            // The image point under the anchor stays under it.
            let from_centre = (anchor - rect.center()) * ppp;
            let base = if self.zoom.is_some() { self.pan } else { t / 2.0 };
            self.pan = base + from_centre / scale - from_centre / target;
            self.zoom = Some(target);
        }
        if self.zoom.is_some() && response.dragged() {
            self.pan -= response.drag_delta() * ppp / self.zoom.unwrap_or(fit);
        }

        let painter = ui.painter_at(rect);
        let (dest, uv) = match self.zoom {
            Some(scale) => {
                // The hand only where there is something to move: not on a picture smaller than the window.
                let larger = t * scale / ppp;
                if response.hovered() && (larger.x > rect.width() + 0.5 || larger.y > rect.height() + 0.5) {
                    ui.ctx().set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                }
                paint::paint_zoomed(&painter, rect, tex, scale, &mut self.pan, self.turn)
            }
            None => {
                let dest = egui::Rect::from_center_size(rect.center(), t * fit / ppp);
                let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
                paint::paint_turned(&painter, tex, dest, uv, self.turn);
                (dest, uv)
            }
        };
        // A CAD drawing: drawn again sharp once the view stops.
        self.cad_sharpen(ui, current, tex.size_vec2(), dest, uv);
        self.cad_measure(ui, response, current, dest, uv);
        if cad && self.turn.is_none() {
            crate::cad_view::ucs_corner(&painter, rect);
        }
        self.zoom_shown = zoom::of_file(self.zoom.unwrap_or(fit), per);
        self.zoom_bar.range = [zoom::of_file(bounds.low, per), zoom::of_file(bounds.max, per)];
    }
}

/// Notches of a wheel event, up positive: a mouse gives lines, a touchpad points.
pub(crate) fn notches(unit: egui::MouseWheelUnit, delta: egui::Vec2) -> f32 {
    match unit {
        egui::MouseWheelUnit::Line => delta.y,
        egui::MouseWheelUnit::Point => delta.y / 50.0,
        egui::MouseWheelUnit::Page => delta.y * 3.0,
    }
}

/// Ctrl+S (Save) or Ctrl+Shift+S (Save copy) pressed this pass, taken from the input: the same
/// acts as the toolbar's buttons, for an image and for a video.
fn take_save(i: &mut egui::InputState) -> Option<Act> {
    // With Shift first: Ctrl+S would match Ctrl+Shift+S too.
    let save_copy = i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::S);
    let save = i.consume_key(egui::Modifiers::COMMAND, Key::S);
    match (save, save_copy) {
        (true, _) => Some(Act::Save),
        (false, true) => Some(Act::Convert(None)),
        _ => None,
    }
}

/// Ctrl+arrows pressed this pass, taken from the input: right forward, left back. Each press by
/// its own modifiers, not by those at the end of the pass: Ctrl let go in the same pass as the
/// arrow made Ctrl+right a move to the next file (seen with keys in quick succession, 2026-11-13).
fn take_jumps(i: &mut egui::InputState) -> i32 {
    let on = i.count_and_consume_key(egui::Modifiers::COMMAND, Key::ArrowRight);
    let back = i.count_and_consume_key(egui::Modifiers::COMMAND, Key::ArrowLeft);
    on as i32 - back as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers};

    #[test]
    fn full_screen_after_two_frames_drawn_ahead() {
        // With DirectX 12 two frames without bars first; otherwise the command goes at once.
        for (ahead, frames) in [(true, 2), (false, 0)] {
            let ctx = egui::Context::default();
            assert!(!bare(&ctx));
            ask_full(&ctx, ahead);
            let drawn = (0..5).filter(|_| {
                full_step(&ctx);
                bare(&ctx)
            });
            assert_eq!(drawn.count(), frames);
        }
    }

    fn key(key: Key, pressed: bool, modifiers: Modifiers) -> Event {
        Event::Key { key, physical_key: None, pressed, repeat: false, modifiers }
    }

    #[test]
    fn jump_with_ctrl_let_go_in_the_same_pass() {
        // Ctrl+right, then Ctrl let go before the pass ends: a jump, and no arrow left to change file.
        let mut i = egui::InputState::default();
        i.events = vec![
            key(Key::ArrowRight, true, Modifiers::COMMAND),
            key(Key::ArrowRight, false, Modifiers::COMMAND),
            Event::ModifiersChanged(Modifiers::NONE),
        ];
        assert_eq!(take_jumps(&mut i), 1);
        assert!(!i.key_pressed(Key::ArrowRight));
    }

    #[test]
    fn arrow_without_ctrl_stays_for_the_next_file() {
        // The arrow before Ctrl pressed in the same pass: no jump, the arrow changes file.
        let mut i = egui::InputState::default();
        i.modifiers = Modifiers::COMMAND;
        i.events = vec![key(Key::ArrowRight, true, Modifiers::NONE), Event::ModifiersChanged(Modifiers::COMMAND)];
        assert_eq!(take_jumps(&mut i), 0);
        assert!(i.key_pressed(Key::ArrowRight));
    }

    #[test]
    fn jumps_count_each_press() {
        let mut i = egui::InputState::default();
        i.events = vec![
            key(Key::ArrowRight, true, Modifiers::COMMAND),
            key(Key::ArrowRight, true, Modifiers::COMMAND),
            key(Key::ArrowLeft, true, Modifiers::COMMAND),
            key(Key::ArrowLeft, true, Modifiers::COMMAND | Modifiers::SHIFT),
            key(Key::ArrowLeft, true, Modifiers::COMMAND),
        ];
        assert_eq!(take_jumps(&mut i), -1);
    }

    #[test]
    fn ctrl_s_saves_and_ctrl_shift_s_saves_a_copy() {
        let take = |modifiers| {
            let mut i = egui::InputState::default();
            i.events = vec![key(Key::S, true, modifiers)];
            take_save(&mut i).map(|a| format!("{a:?}"))
        };
        assert_eq!(take(Modifiers::COMMAND).as_deref(), Some("Save"));
        assert_eq!(take(Modifiers::COMMAND | Modifiers::SHIFT).as_deref(), Some("Convert(None)"));
        assert_eq!(take(Modifiers::NONE), None);
    }
}
