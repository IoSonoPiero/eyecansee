//! The image editor's page (issue #53), today "Rotate and crop…" (Image > Transform): a page of
//! its own, like "Save in another format". The image is drawn rotated by the GPU with the crop over
//! it; Save writes it over the original after a confirmation, "Save as…" hands it to "Save in
//! another format". The pixels: `rotate`. Moved from `rotate_page.rs` on 2026-11-05: the crop
//! controls in `crop`, the canvas in `canvas`.

use std::sync::Arc;

use eframe::egui::{self, Color32, ColorImage, Pos2, TextureHandle, Vec2};

use crate::encode::{Format, Original};
use crate::rotate::{self, Crop, Edit};
use crate::transform::Turn;
use crate::{App, source};

mod canvas;
mod crop;
mod save;
use canvas::canvas;
use crop::{angle_controls, apply_row, crop_controls, fill_controls};
use save::Leave;

type Loaded = Result<(Arc<Original>, TextureHandle, u64), String>;

/// Width of the controls on the right.
const SIDE: f32 = 300.0;
const GAP: f32 = 16.0;
/// Distance in points at which a side or corner of the crop can be grabbed.
const GRAB: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Aspect {
    Free,
    Original,
    Ratio(u8, u8),
    Custom,
}

const ASPECTS: [Aspect; 8] = [Aspect::Free, Aspect::Original, Aspect::Ratio(1, 1), Aspect::Ratio(5, 4), Aspect::Ratio(4, 3), Aspect::Ratio(3, 2), Aspect::Ratio(16, 9), Aspect::Custom];

/// What the crop does when the angle or the proportions change.
#[derive(Clone, Copy, PartialEq)]
enum Follow {
    /// The largest crop with no empty corner.
    Inside,
    /// All the rotated image.
    Whole,
    /// Where the user put it.
    Manual,
}

/// What a drag started on: a side or corner of the crop (signs of x and y), its inside, or the view.
#[derive(Clone, Copy, PartialEq)]
enum Grab {
    Side(i8, i8),
    Move,
    Pan,
    /// A line along the horizon (or something upright), for Straighten.
    Line,
}

pub struct Window {
    pub path: String,
    /// Flips and quarter turns of the viewer, done first.
    turn: Turn,
    original: Option<Loaded>,
    angle: f32,
    crop: Crop,
    follow: Follow,
    aspect: Aspect,
    custom: [f32; 2],
    /// Portrait instead of landscape.
    swapped: bool,
    transparent: bool,
    color: Color32,
    /// Centre of the view (image pixels, from the centre of the image) and points per pixel;
    /// `None`: everything in view.
    view: Option<(Vec2, f32)>,
    /// The drag under way: what, the crop and the view when it started, where the pointer was.
    drag: Option<(Grab, Crop, (Vec2, f32), Pos2)>,
    /// Straighten is on: a drag draws a line, which becomes level when let go.
    straighten: bool,
    /// Where the line being drawn ends.
    line_end: Option<Pos2>,
    preparing: bool,
    /// The file to write and whether it lost quality: the confirmation is open.
    pub ready: Option<(Arc<Vec<u8>>, bool, ColorImage)>,
    /// The edits applied: the image shown has them, Save encodes it as it is (an Ultra HDR
    /// photo's gain map gets them too).
    applied: Vec<Edit>,
    /// Apply under way (in the background, `Msg::Original`): the edit being applied.
    applying: Option<Edit>,
    /// Closing with changes not saved: the question is open.
    pub leaving: bool,
    /// Save as… chosen in the message saying Save cannot write over the original.
    pub save_as: bool,
}

impl Window {
    fn new(path: String, turn: Turn) -> Window {
        // JPEG has no transparency: white there, transparent elsewhere.
        let transparent = Format::of_file(&path) != Some(Format::Jpeg);
        Window {
            path,
            turn,
            original: None,
            angle: 0.0,
            crop: Crop::centred(1.0, 1.0),
            follow: Follow::Inside,
            aspect: Aspect::Free,
            custom: [4.0, 3.0],
            swapped: false,
            transparent,
            color: Color32::WHITE,
            view: None,
            drag: None,
            straighten: false,
            line_end: None,
            preparing: false,
            ready: None,
            applied: Vec::new(),
            applying: None,
            leaving: false,
            save_as: false,
        }
    }

    /// Size of the image once flipped and turned as in the viewer.
    fn size(&self) -> (f32, f32) {
        match &self.original {
            Some(Ok((o, ..))) if self.turn.sideways() => (o.image.height() as f32, o.image.width() as f32),
            Some(Ok((o, ..))) => (o.image.width() as f32, o.image.height() as f32),
            _ => (1.0, 1.0),
        }
    }

    /// Width / height the crop keeps, if any.
    fn ratio(&self) -> Option<f32> {
        let (iw, ih) = self.size();
        let r = match self.aspect {
            Aspect::Free => return None,
            Aspect::Original => iw / ih,
            Aspect::Ratio(a, b) => a as f32 / b as f32,
            Aspect::Custom => self.custom[0].max(1.0) / self.custom[1].max(1.0),
        };
        Some(if self.swapped { 1.0 / r } else { r })
    }

    /// The crop again after a change of angle or proportions.
    fn refit(&mut self) {
        let (iw, ih) = self.size();
        // Free proportions: those of the image as it stands after the nearest quarter turn (at 90°
        // the image's own ones cut a strip off it).
        let upright = if (self.angle / 90.0).round() as i32 % 2 != 0 { ih / iw } else { iw / ih };
        self.crop = match self.follow {
            Follow::Inside => rotate::inside(iw, ih, self.angle, self.ratio().unwrap_or(upright)),
            Follow::Whole => {
                let (bw, bh) = rotate::bounds(iw, ih, self.angle);
                let all = Crop::centred(bw, bh);
                self.ratio().map_or(all, |r| all.fit(r))
            }
            Follow::Manual => self.ratio().map_or(self.crop, |r| self.crop.fit(r)),
        };
    }

    fn edit(&self) -> Edit {
        Edit { turn: self.turn, angle: self.angle, crop: self.crop, fill: (!self.transparent).then(|| [self.color.r(), self.color.g(), self.color.b()]) }
    }

    /// Anything not saved yet: applied, or still to apply.
    fn changed(&self) -> bool {
        !self.applied.is_empty() || self.pending()
    }

    /// Rotation or crop set and not applied yet?
    fn pending(&self) -> bool {
        let (iw, ih) = self.size();
        !self.turn.is_none() || self.angle.rem_euclid(360.0) != 0.0 || self.crop.pixels() != (iw as u32, ih as u32) || self.crop.cx.abs() >= 0.5 || self.crop.cy.abs() >= 0.5
    }
}

/// What the buttons at the bottom of the controls asked for.
enum Ask {
    Apply,
    Save,
    SaveAs,
    Close,
}

impl App {
    pub(crate) fn open_rotate(&mut self, path: Option<String>) {
        // In the grid the one picture among the files chosen (a video next to it is left out).
        let picture = if self.viewing.is_some() { None } else { self.grid_images(path.clone()).into_iter().next() };
        let Some(path) = picture.or(path).or_else(|| self.target()).filter(|p| crate::decode::is_image(p)) else { return };
        let Some(loader) = &self.loader else { return };
        loader.load_original(path.clone());
        // From the viewer: as it is shown there.
        let turn = if self.viewing.is_some() { self.turn } else { Turn::default() };
        self.rotate = Some(Window::new(path, turn));
    }

    pub(crate) fn rotate_original(&mut self, ctx: &egui::Context, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        let Some(w) = &mut self.rotate else { return };
        // Apply done: the image now has the edit, the controls start again from it.
        if let Some(e) = w.applying.take() {
            log!("applied {e:?}");
            w.applied.push(e);
            w.angle = 0.0;
            w.turn = Turn::default();
            w.follow = Follow::Inside;
            w.view = None;
        }
        w.original = Some(r.map(|(original, preview, size)| (original, ctx.load_texture("rotate-original", preview, egui::TextureOptions::LINEAR), size)));
        w.refit();
    }

    /// Drawn in place of the whole interface, as "Save in another format".
    pub(crate) fn rotate_page(&mut self, ui: &mut egui::Ui) {
        let Some(mut w) = self.rotate.take() else { return };
        let mut open = crate::convert::page_header(ui, &self.i18n.t("rc-title"));
        let tex = match &w.original {
            None => {
                ui.centered_and_justified(|ui| ui.spinner());
                None
            }
            Some(Err(e)) => {
                ui.label(crate::browser::error_text(ui, self.i18n.t_args("cannot-load", &[("file", source::name(&w.path)), ("error", e)])));
                None
            }
            Some(Ok((_, tex, _))) => Some(tex.clone()),
        };
        let mut handed = false;
        if let Some(tex) = tex {
            let size = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                canvas(ui, &mut w, &tex, egui::vec2(size.x - SIDE - GAP, size.y));
                ui.vertical(|ui| {
                    ui.set_width(SIDE);
                    let ask = self.rotate_controls(ui, &mut w);
                    match if std::mem::take(&mut w.save_as) { Some(Ask::SaveAs) } else { ask } {
                        Some(Ask::Apply) => self.rotate_apply(ui.ctx(), &mut w),
                        Some(Ask::Save) => self.rotate_save(&mut w),
                        Some(Ask::SaveAs) => {
                            handed = self.rotate_save_as(ui.ctx(), &w);
                            open = !handed;
                        }
                        Some(Ask::Close) => open = false,
                        None => {}
                    }
                });
            });
        }
        // Closing (Esc, ✕, Close) with changes not saved: asked first. Not after Save as…: the
        // page stayed open under it and took the image meant for it (spinner for ever).
        if !open && !handed && w.changed() && w.ready.is_none() {
            w.leaving = true;
            open = true;
        }
        if w.leaving {
            match self.rotate_leave(ui.ctx(), &mut w) {
                Some(Leave::Save) => self.rotate_save(&mut w),
                Some(Leave::SaveAs) => open = !self.rotate_save_as(ui.ctx(), &w),
                Some(Leave::Discard) => open = false,
                None => {}
            }
        }
        if w.ready.is_some() && self.rotate_confirm(ui.ctx(), &mut w) {
            open = false;
        }
        if open {
            self.rotate = Some(w);
        }
    }

    /// The controls on the right; what the buttons at the bottom ask for.
    fn rotate_controls(&self, ui: &mut egui::Ui, w: &mut Window) -> Option<Ask> {
        let i18n = &self.i18n;
        // Before the fields are drawn: Enter in the degrees field leaves it in this same frame,
        // and Apply took it as its own (the user only wanted to see 45°).
        let typing = ui.ctx().egui_wants_keyboard_input();
        if angle_controls(ui, w, i18n) | crop_controls(ui, w, i18n) {
            w.refit();
        }
        fill_controls(ui, w, i18n);
        ui.add_space(12.0);
        let (pw, ph) = w.crop.pixels();
        ui.strong(i18n.t_args("rc-size", &[("w", &pw.to_string()), ("h", &ph.to_string())]));
        ui.add_space(4.0);
        let mut ask = apply_row(ui, w, i18n, typing);
        ui.separator();
        ui.add_space(4.0);
        let why_not = self.why_not_save(w);
        ui.horizontal(|ui| {
            let ready = w.changed() && why_not.is_none() && !w.preparing && w.applying.is_none();
            let r = ui.add_enabled(ready, egui::Button::new(i18n.t("rc-save"))).on_hover_text(i18n.t("rc-save-hint"));
            if r.clicked() {
                ask = Some(Ask::Save);
            }
            if let Some(why) = why_not {
                // Written for the message windows (`**bold**`, escaped names): drawn the same way.
                r.on_disabled_hover_ui(|ui| {
                    ui.set_max_width(320.0);
                    crate::save_dialogs::notice_text(ui, &why);
                });
            }
            if ui.add_enabled(!w.preparing && w.applying.is_none(), egui::Button::new(i18n.t("menu-save-copy"))).on_hover_text(i18n.t("rc-save-as-hint")).clicked() {
                ask = Some(Ask::SaveAs);
            }
            if ui.button(i18n.t("rc-close")).on_hover_text("Esc").clicked() {
                ask = Some(Ask::Close);
            }
        });
        // A row of its own: next to the buttons it was cut off by the edge of the panel.
        if w.preparing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(i18n.t("status-preparing"));
            });
        }
        ui.add_space(12.0);
        ui.weak(i18n.t("rc-help"));
        ask
    }

}
