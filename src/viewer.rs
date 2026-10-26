//! Viewer: the image at full window, zoom and pan, its keys, the images read ahead.

use eframe::egui::{self, Key, TextureHandle};

use crate::navigation::Go;
use crate::actions::Act;
use crate::{AROUND, App, browser, paint, source, transform};

impl App {
    pub(crate) fn viewer_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (next, prev, first, last, close) = ctx.input(|i| {
            let any = |keys: &[Key]| keys.iter().any(|k| i.key_pressed(*k));
            (
                any(&[Key::ArrowRight, Key::Space, Key::PageDown]),
                any(&[Key::ArrowLeft, Key::Backspace, Key::PageUp]),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::Escape),
            )
        });
        for (pressed, to) in [(next, Go::Next), (prev, Go::Prev), (first, Go::First), (last, Go::Last)] {
            if pressed {
                self.go(to);
            }
        }
        if close {
            self.close_viewer();
        }
        let (cw, ccw, save, save_copy) = ctx.input_mut(|i| {
            let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
            // With Shift first: Ctrl+R would match Ctrl+Shift+R too.
            let ccw = i.consume_key(shift, Key::R);
            let save_copy = i.consume_key(shift, Key::S);
            (i.consume_key(egui::Modifiers::COMMAND, Key::R), ccw, i.consume_key(egui::Modifiers::COMMAND, Key::S), save_copy)
        });
        if cw {
            self.act = Some(Act::Transform(transform::Op::Cw, None));
        } else if ccw {
            self.act = Some(Act::Transform(transform::Op::Ccw, None));
        } else if save {
            self.act = Some(Act::Save);
        } else if save_copy {
            self.act = Some(Act::Convert(None));
        }
    }

    pub(crate) fn close_viewer(&mut self) {
        if self.hold_for_turn(crate::save_dialogs::After::Close) {
            return;
        }
        if let (Some(i), Some(Ok(l))) = (self.viewing.take(), self.listings.get(&self.dir)) {
            self.selected = l.dirs.len() + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
        self.full.clear();
        self.zoom_of = None;
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
            if !self.full.contains_key(&i) && self.pending.insert(i) {
                loader.load_full(self.generation, i, listing.files[i].path.clone());
            }
        }
        let keep = near(AROUND + 1);
        self.full.retain(|i, _| keep.contains(i));
    }

    pub(crate) fn viewer(&mut self, ui: &mut egui::Ui, current: usize) {
        let mut chosen = None;
        if let Some(Ok(tex)) = self.full.get(&current) {
            let tex = tex.clone();
            let rect = ui.available_rect_before_wrap();
            let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
            self.zoom_view(ui, &response, &tex, current);
            if response.double_clicked() {
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
            Some(Err(e)) => {
                let name = self.files().get(current).map_or("", |f| source::name(&f.path));
                ui.label(browser::error_text(ui, self.i18n.t_args("cannot-load", &[("file", name), ("error", e)])));
            }
            None => {
                ui.spinner();
            }
        });
        if chosen.is_some() {
            self.act = chosen;
        }
    }

    /// The open image, fitted to the window or zoomed (as GNOME's viewer, Loupe): the wheel ×1.3 a
    /// notch around the pointer (or the next image, see `wheel_zoom`; Ctrl and the wheel always
    /// zoom), + and − ×1.5 around the centre, 0 fitted, 1 at 100%, dragging moves the zoomed image.
    /// Never smaller than fitted, never larger than `max_zoom`.
    #[allow(clippy::too_many_lines)] // to be split
    fn zoom_view(&mut self, ui: &egui::Ui, response: &egui::Response, tex: &TextureHandle, current: usize) {
        let rect = response.rect;
        let ppp = ui.ctx().pixels_per_point();
        if self.zoom_of != Some(current) {
            // Reached only once the turns were saved or dropped: leaving the image asks first (`hold_for_turn`).
            (self.zoom, self.zoom_of, self.turn) = (None, Some(current), Default::default());
        }
        let t = paint::turned_size(tex, self.turn);
        let fit = (rect.width() * ppp / t.x).min(rect.height() * ppp / t.y).min(1.0);
        let max = (self.settings.max_zoom as f32 / 100.0).max(fit);
        let scale = self.zoom.unwrap_or(fit).clamp(fit, max);

        let (mut wheel, mut next) = (0.0, 0.0);
        let mut pinch = 1.0;
        if response.hovered() {
            let wheel_zoom = self.settings.wheel_zoom;
            ui.input(|i| {
                for e in &i.events {
                    match e {
                        egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                            // Notches: a mouse gives lines, a touchpad points.
                            let notches = match unit {
                                egui::MouseWheelUnit::Line => delta.y,
                                egui::MouseWheelUnit::Point => delta.y / 50.0,
                                egui::MouseWheelUnit::Page => delta.y * 3.0,
                            };
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
        let (zoom_in, zoom_out, fitted, actual) = if ui.ctx().egui_wants_keyboard_input() {
            (false, false, false, false)
        } else {
            ui.input(|i| {
                let key = |k| i.key_pressed(k) && !i.modifiers.command;
                (key(Key::Plus) || key(Key::Equals), key(Key::Minus), key(Key::Num0), key(Key::Num1))
            })
        };

        let mut target = scale * 1.3f32.powf(wheel) * pinch;
        let mut anchor = response.hover_pos().filter(|_| wheel != 0.0 || pinch != 1.0).unwrap_or(rect.center());
        if zoom_in || zoom_out {
            target = scale * if zoom_in { 1.5 } else { 1.0 / 1.5 };
            anchor = rect.center();
        }
        if actual {
            target = 1.0;
        }
        let target = target.clamp(fit, max);
        if fitted || target <= fit {
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
        match self.zoom {
            Some(scale) => {
                if response.hovered() {
                    ui.ctx().set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                }
                paint::paint_zoomed(&painter, rect, tex, scale, &mut self.pan, self.turn);
            }
            None => {
                let dest = egui::Rect::from_center_size(rect.center(), t * fit / ppp);
                paint::paint_turned(&painter, tex, dest, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), self.turn);
            }
        }
        self.zoom_shown = self.zoom.unwrap_or(fit);
    }
}
