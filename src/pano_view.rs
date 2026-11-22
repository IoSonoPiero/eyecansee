//! 360° panoramas in the viewer (#188): a picture whose XMP declares an equirectangular panorama
//! (`pano::detect`, read with the picture by the loader) is shown as the inside of a sphere,
//! drawn by the graphics card as a grid of small triangles with the picture on them
//! (`pano::grid`). Dragging turns the view, the wheel zooms as on a flat picture (or changes
//! image, as Options > Viewer says; with Ctrl it always zooms), + and − zoom, 0 goes back to the
//! first look and 1 to 100% in the middle. P, or the button over the bottom left corner, shows the
//! flat picture instead, and back. A turned picture (Rotate), or measuring, is shown flat; the
//! thumbnails, the slideshow, Save, Copy and the editor always have the flat picture.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Key};

use crate::App;
use crate::pano::{NARROWEST, Pano, View, WIDEST};

#[derive(Default)]
pub(crate) struct State {
    /// The panoramas found in the pictures read for the viewer (and around it), by path.
    pub(crate) read: HashMap<String, Pano>,
    /// P or the button: the panoramas shown flat, until asked again (as long as EyeCanSee is open).
    flat: bool,
    /// The look at the open panorama, and its path.
    view: Option<(String, View)>,
}

impl App {
    /// The open picture's path and panorama, when it has one and the picture has the shape its
    /// tags say: a panorama turned on its side and saved, or cropped, keeps the tags of the whole
    /// one (`xmp.rs` keeps what it does not know), and as a sphere it would look broken.
    fn open_pano(&self) -> Option<(String, Pano)> {
        let current = self.viewing?;
        let path = &self.files().get(current)?.path;
        let pano = self.pano.read.get(path)?;
        let Some(Ok(tex)) = self.full.get(&current) else { return None };
        pano.fits([tex.size()[0] as f32, tex.size()[1] as f32]).then(|| (path.clone(), *pano))
    }

    /// The open panorama is shown as a sphere now: not chosen flat, not turned, not measured.
    pub(crate) fn sphere_shown(&self) -> bool {
        self.open_pano().is_some() && !self.pano.flat && self.turn.is_none() && !self.model.measure.on
    }

    /// P on a panorama: the flat picture, or the sphere again.
    pub(crate) fn pano_keys(&mut self, ctx: &egui::Context) {
        if self.open_pano().is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::P)) {
            self.toggle_pano();
        }
    }

    fn toggle_pano(&mut self) {
        self.pano.flat = !self.pano.flat;
        log!("panorama: {}", if self.pano.flat { "flat" } else { "360° view" });
    }

    /// The open picture as a sphere, when it is a panorama to show so: `true` when drawn.
    pub(crate) fn pano_view(&mut self, ui: &egui::Ui, response: &egui::Response, tex: &crate::mipmap::Picture, current: usize) -> bool {
        if !self.sphere_shown() {
            return false;
        }
        let Some((path, pano)) = self.open_pano() else { return false };
        // As `zoom_view`: another picture starts unzoomed and unturned.
        if self.zoom_of != Some(current) {
            (self.zoom, self.zoom_of, self.turn) = (None, Some(current), Default::default());
        }
        let (rect, ppp) = (response.rect, ui.ctx().pixels_per_point());
        let size = [rect.width() * ppp, rect.height() * ppp];
        let width = tex.size()[0] as f32;
        let own = self.files().get(current).and_then(|f| self.own_sizes.get(&f.path).copied());
        let per = crate::zoom::file_per_texture(own, tex.size());
        let before = match &self.pano.view {
            Some((p, v)) if *p == path => *v,
            _ => pano.first,
        };
        let view = pano.clamped(self.pano_moved(ui, response, &pano, before, [size, [width, per]]), size);
        self.pano.view = Some((path, view));
        let scale = pano.scale(view, size, width);
        draw(&ui.painter_at(rect), rect, tex, &pano, view, scale);
        if response.hovered() {
            ui.ctx().set_cursor_icon(if response.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        let at = |fov| crate::zoom::of_file(pano.scale(View { fov, ..view }, size, width), per);
        self.zoom_shown = crate::zoom::of_file(scale, per);
        self.zoom_bar.range = [at(pano.clamped(View { fov: WIDEST, ..view }, size).fov), at(NARROWEST)];
        true
    }

    /// `view` after this pass's wheel, pinch, keys, the status bar's zoom and the drag. `screen`:
    /// the size of the view in pixels, and the picture's width in pixels with the file's pixels
    /// for each of them.
    fn pano_moved(&mut self, ui: &egui::Ui, response: &egui::Response, pano: &Pano, mut view: View, [size, [width, per]]: [[f32; 2]; 2]) -> View {
        let (wheel, next, pinch) = crate::viewer::wheel_input(ui, response, self.settings.wheel_zoom);
        self.wheel_go(next);
        view = view.zoomed(1.3f32.powf(wheel) * pinch);
        let keys = !ui.ctx().egui_wants_keyboard_input();
        let key = |k| keys && ui.input(|i| i.key_pressed(k) && !i.modifiers.command);
        let asked = self.zoom_bar.ask.take();
        if key(Key::Plus) || key(Key::Equals) || asked == Some(crate::zoom::Ask::In) {
            view = view.zoomed(1.5);
        }
        if key(Key::Minus) || asked == Some(crate::zoom::Ask::Out) {
            view = view.zoomed(1.0 / 1.5);
        }
        if key(Key::Num0) {
            view = pano.first;
        }
        if key(Key::Num1) {
            view.fov = pano.fov_for(per, size, width);
        }
        if let Some(crate::zoom::Ask::To(z)) = asked {
            view.fov = pano.fov_for(z.max(1e-3) * per, size, width);
        }
        if response.dragged() {
            let d = response.drag_delta() * ui.ctx().pixels_per_point();
            view = view.dragged([d.x, d.y], size);
        }
        view
    }

    /// The button over the bottom left corner of a panorama: "Flat picture", or "360° view".
    pub(crate) fn pano_bar(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if self.open_pano().is_none() || !self.turn.is_none() || self.model.measure.on {
            return;
        }
        let light = Color32::from_gray(225);
        let area = egui::Area::new(egui::Id::unique("pano-bar")).pivot(egui::Align2::LEFT_BOTTOM).fixed_pos(egui::pos2(rect.left() + 12.0, rect.bottom() - 12.0));
        let frame = egui::Frame::new().fill(Color32::from_black_alpha(170)).corner_radius(6.0).inner_margin(6.0);
        let (id, hint) = if self.pano.flat { ("pano-sphere", "pano-sphere-hint") } else { ("pano-flat", "pano-flat-hint") };
        let (text, hint) = (egui::RichText::new(self.i18n.t(id)).color(light), self.i18n.t(hint));
        let clicked = area
            .show(ui.ctx(), |ui| {
                frame
                    .show(ui, |ui| {
                        crate::theme::on_dark(ui);
                        // An area near the corner gives little room: the label went to a new line at
                        // every few letters (seen by Piero, 22/11). One line, whatever its width.
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        ui.add(egui::Button::new(text).fill(Color32::TRANSPARENT)).on_hover_text(hint).clicked()
                    })
                    .inner
            })
            .inner;
        if clicked {
            self.toggle_pano();
        }
    }
}

/// The grid of the panorama on the screen, as one mesh: the picture's copy for `scale` (screen
/// pixels for each of its pixels in the middle, `mipmap`), on the triangles that can be drawn.
fn draw(painter: &egui::Painter, rect: egui::Rect, tex: &crate::mipmap::Picture, pano: &Pano, view: View, scale: f32) {
    let (ppp, by) = (painter.ctx().pixels_per_point(), crate::paint::squeeze_now(painter.ctx()));
    let grid = crate::pano::grid(pano, view, [rect.width() * ppp, rect.height() * ppp]);
    let mut mesh = egui::Mesh::with_texture(tex.id_at(scale));
    mesh.vertices = grid
        .uv
        .iter()
        .zip(&grid.screen)
        .map(|(uv, at)| {
            // A vertex not drawn is in no triangle: anywhere will do. Squeezed as a flat picture in
            // the frames before full screen (`viewer::ahead`).
            let pos = at.map_or(rect.center(), |p| ((rect.min + egui::vec2(p[0], p[1]) / ppp).to_vec2() * by).to_pos2());
            egui::epaint::Vertex { pos, uv: egui::pos2(uv[0], uv[1]), color: Color32::WHITE }
        })
        .collect();
    for t in grid.triangles() {
        mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}
