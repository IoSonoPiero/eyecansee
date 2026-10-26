//! "Rotate and crop…" (Image > Transform): a page of its own, like "Save in another format". The
//! image is drawn rotated by the GPU with the crop over it; Save writes it over the original after a
//! confirmation, "Save as…" hands it to "Save in another format". The pixels: `rotate`.

use std::sync::Arc;

use eframe::egui::{self, Color32, ColorImage, CursorIcon, Pos2, Rect, TextureHandle, Vec2};

use crate::encode::{Format, Original};
use crate::i18n::I18n;
use crate::rotate::{self, Crop, Edit};
use crate::transform::Turn;
use crate::{App, source};

mod save;
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
    /// How many edits were applied: the image shown has them, Save encodes it as it is.
    applied: usize,
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
            applied: 0,
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
        self.applied > 0 || self.pending()
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
        let Some(path) = path.or_else(|| self.target()).filter(|p| !p.ends_with('/')) else { return };
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
            w.applied += 1;
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
        if angle_controls(ui, w, i18n) | crop_controls(ui, w, i18n) {
            w.refit();
        }
        fill_controls(ui, w, i18n);
        ui.add_space(12.0);
        let (pw, ph) = w.crop.pixels();
        ui.strong(i18n.t_args("rc-size", &[("w", &pw.to_string()), ("h", &ph.to_string())]));
        ui.add_space(4.0);
        let mut ask = apply_row(ui, w, i18n);
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

/// Apply (also Enter), with what is applied and what is not saved yet.
fn apply_row(ui: &mut egui::Ui, w: &Window, i18n: &I18n) -> Option<Ask> {
    let mut ask = None;
    ui.horizontal(|ui| {
        let ready = w.pending() && w.applying.is_none() && !w.preparing;
        let r = ui.add_enabled(ready, crate::theme::main_button(ui, i18n.t("rc-apply"))).on_hover_text(i18n.t("rc-apply-hint"));
        // Enter applies too, unless a field is being typed in.
        if r.clicked() || (ready && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
            ask = Some(Ask::Apply);
        }
        if w.applying.is_some() {
            ui.spinner();
        }
    });
    // Nothing is written until Save: said in so many words (the user took Apply for Save).
    let note = if w.pending() {
        Some("rc-pending")
    } else if w.applied > 0 {
        Some("rc-unsaved")
    } else {
        None
    };
    if let Some(id) = note {
        ui.label(i18n.t(id));
    }
    ui.add_space(8.0);
    ask
}

/// The angle, with buttons for the quarter turns and back to zero. `true` if it changed.
fn angle_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let before = w.angle;
    ui.strong(i18n.t("rc-rotation"));
    ui.spacing_mut().slider_width = SIDE - 80.0;
    // The language's decimal separator, both shown and typed (a dot works too).
    let sep = i18n.t("decimal-separator");
    ui.add(
        egui::Slider::new(&mut w.angle, -180.0..=180.0)
            .step_by(0.1)
            .suffix("°")
            .custom_formatter(move |v, _| format!("{v:.1}").replace('.', &sep))
            .custom_parser(|s| s.trim().replace(',', ".").parse().ok()),
    );
    ui.horizontal(|ui| {
        if ui.button("−90°").clicked() {
            w.angle -= 90.0;
        }
        if ui.button("+90°").clicked() {
            w.angle += 90.0;
        }
        if ui.add_enabled(w.angle != 0.0, egui::Button::new(i18n.t("rc-reset"))).clicked() {
            w.angle = 0.0;
        }
    });
    // A framed button even when off (`toggle_value` looked like plain text then).
    if ui.add(egui::Button::new(i18n.t("rc-straighten")).selected(w.straighten)).on_hover_text(i18n.t("rc-straighten-hint")).clicked() {
        w.straighten = !w.straighten;
    }
    ui.weak(i18n.t("rc-rotation-hint"));
    // Kept between -180 and 180, the range of the slider.
    w.angle = (w.angle + 180.0).rem_euclid(360.0) - 180.0;
    ui.add_space(12.0);
    w.angle != before
}

/// Proportions and the two ready-made crops. `true` if the crop must be made again.
fn crop_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let t = |id| i18n.t(id);
    let label = |a: Aspect| match a {
        Aspect::Free => t("rc-free"),
        Aspect::Original => t("rc-original"),
        Aspect::Ratio(x, y) => format!("{x}:{y}"),
        Aspect::Custom => t("rc-custom"),
    };
    let before = (w.aspect, w.custom, w.swapped, w.follow);
    ui.strong(t("rc-crop"));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("rc-aspect").selected_text(label(w.aspect)).show_ui(ui, |ui| {
            for a in ASPECTS {
                ui.selectable_value(&mut w.aspect, a, label(a));
            }
        });
        let square = matches!(w.aspect, Aspect::Free | Aspect::Ratio(1, 1));
        ui.add_enabled(!square, egui::Button::new("⇄")).on_hover_text(t("rc-swap")).clicked().then(|| w.swapped = !w.swapped);
    });
    if w.aspect == Aspect::Custom {
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut w.custom[0]).range(1.0..=100.0).speed(0.05));
            ui.label(":");
            ui.add(egui::DragValue::new(&mut w.custom[1]).range(1.0..=100.0).speed(0.05));
        });
    }
    ui.horizontal(|ui| {
        if ui.button(t("rc-inside")).on_hover_text(t("rc-inside-hint")).clicked() {
            w.follow = Follow::Inside;
        }
        if ui.button(t("rc-whole")).on_hover_text(t("rc-whole-hint")).clicked() {
            w.follow = Follow::Whole;
        }
    });
    ui.add_space(12.0);
    (w.aspect, w.custom, w.swapped, w.follow) != before
}

/// Transparent or a colour where the crop goes past the image.
fn fill_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    ui.strong(i18n.t("rc-empty"));
    ui.horizontal(|ui| {
        ui.radio_value(&mut w.transparent, true, i18n.t("rc-transparent"));
        ui.radio_value(&mut w.transparent, false, i18n.t("rc-color"));
        ui.add_enabled_ui(!w.transparent, |ui| egui::color_picker::color_edit_button_srgba(ui, &mut w.color, egui::color_picker::Alpha::Opaque));
    });
    if w.transparent && Format::of_file(&w.path) == Some(Format::Jpeg) {
        ui.weak(i18n.t("rc-jpeg-white"));
    }
}

/// From image pixels (rotated, from the centre of the image) to the screen and back.
struct View {
    centre: Pos2,
    at: Vec2,
    k: f32,
}

impl View {
    fn screen(&self, p: Vec2) -> Pos2 {
        self.centre + (p - self.at) * self.k
    }

    fn image(&self, s: Pos2) -> Vec2 {
        (s - self.centre) / self.k + self.at
    }

    fn crop(&self, c: Crop) -> Rect {
        Rect::from_center_size(self.screen(egui::vec2(c.cx, c.cy)), egui::vec2(c.w, c.h) * self.k)
    }
}

/// The image rotated, the crop over it, and the mouse on them.
fn canvas(ui: &mut egui::Ui, w: &mut Window, tex: &TextureHandle, size: Vec2) {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let (iw, ih) = w.size();
    let (at, k) = w.view.unwrap_or_else(|| {
        // Everything in view: the rotated image and the crop, if it goes further.
        let (bw, bh) = rotate::bounds(iw, ih, w.angle);
        let c = w.crop;
        let (vw, vh) = (bw.max(2.0 * c.cx.abs() + c.w), bh.max(2.0 * c.cy.abs() + c.h));
        (Vec2::ZERO, (rect.width() / vw).min(rect.height() / vh) * 0.92)
    });
    let view = View { centre: rect.center(), at, k };
    mouse(ui, w, &response, &view, rect);
    // Drawn with the view after the mouse moved it.
    let (at, k) = w.view.unwrap_or((at, k));
    paint(ui, w, tex, &View { centre: rect.center(), at, k }, rect);
}

fn paint(ui: &egui::Ui, w: &Window, tex: &TextureHandle, view: &View, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::from_gray(18));
    let cr = view.crop(w.crop);
    if w.transparent {
        checker(&painter, cr.intersect(rect));
    } else {
        painter.rect_filled(cr, 0.0, w.color);
    }
    // The four corners of the image, rotated clockwise around its centre; the GPU does the rest.
    let (iw, ih) = w.size();
    let (s, c) = (w.angle as f64).to_radians().sin_cos();
    let (s, c) = (s as f32, c as f32);
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        let (qx, qy) = ((x - 0.5) * iw, (y - 0.5) * ih);
        let pos = view.screen(egui::vec2(qx * c - qy * s, qx * s + qy * c));
        mesh.vertices.push(egui::epaint::Vertex { pos, uv: w.turn.source_uv(egui::pos2(x, y)), color: Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
    // Outside the crop darkened, as in the photo editors.
    let shade = Color32::from_black_alpha(150);
    for r in [
        Rect::from_min_max(rect.min, egui::pos2(rect.max.x, cr.min.y)),
        Rect::from_min_max(egui::pos2(rect.min.x, cr.max.y), rect.max),
        Rect::from_min_max(egui::pos2(rect.min.x, cr.min.y), egui::pos2(cr.min.x, cr.max.y)),
        Rect::from_min_max(egui::pos2(cr.max.x, cr.min.y), egui::pos2(rect.max.x, cr.max.y)),
    ] {
        painter.rect_filled(r, 0.0, shade);
    }
    // Thirds, to line up the horizon or the subject.
    let line = egui::Stroke::new(1.0, Color32::from_white_alpha(90));
    for i in 1..3 {
        let f = i as f32 / 3.0;
        painter.line_segment([egui::pos2(cr.min.x + cr.width() * f, cr.min.y), egui::pos2(cr.min.x + cr.width() * f, cr.max.y)], line);
        painter.line_segment([egui::pos2(cr.min.x, cr.min.y + cr.height() * f), egui::pos2(cr.max.x, cr.min.y + cr.height() * f)], line);
    }
    painter.rect_stroke(cr, 0.0, egui::Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Middle);
    if let (Some((Grab::Line, .., from)), Some(to)) = (w.drag, w.line_end) {
        painter.line_segment([from, to], egui::Stroke::new(3.0, Color32::from_black_alpha(160)));
        painter.line_segment([from, to], egui::Stroke::new(1.5, Color32::from_rgb(0xFF, 0xD5, 0x4F)));
    }
    for (sx, sy) in sides(w.ratio().is_some()) {
        let p = cr.center() + egui::vec2(sx as f32 * cr.width(), sy as f32 * cr.height()) / 2.0;
        let handle = Rect::from_center_size(p, egui::vec2(9.0, 9.0));
        painter.rect_filled(handle, 1.0, Color32::WHITE);
        painter.rect_stroke(handle, 1.0, egui::Stroke::new(1.0, Color32::from_gray(40)), egui::StrokeKind::Outside);
    }
}

/// Grey squares where the result will be transparent.
fn checker(painter: &egui::Painter, area: Rect) {
    const SQ: f32 = 10.0;
    painter.rect_filled(area, 0.0, Color32::from_gray(190));
    let mut y = area.min.y;
    while y < area.max.y {
        let mut x = area.min.x + if ((y - area.min.y) / SQ) as i32 % 2 == 0 { 0.0 } else { SQ };
        while x < area.max.x {
            painter.rect_filled(Rect::from_min_max(egui::pos2(x, y), egui::pos2((x + SQ).min(area.max.x), (y + SQ).min(area.max.y))), 0.0, Color32::from_gray(140));
            x += 2.0 * SQ;
        }
        y += SQ;
    }
}

/// The corners, and the middle of the sides when the proportions are free.
fn sides(fixed: bool) -> Vec<(i8, i8)> {
    let mut v = vec![(-1, -1), (1, -1), (1, 1), (-1, 1)];
    if !fixed {
        v.extend([(0, -1), (1, 0), (0, 1), (-1, 0)]);
    }
    v
}

/// What is under the pointer at `p`.
fn grab_at(w: &Window, view: &View, p: Pos2) -> Grab {
    if w.straighten {
        return Grab::Line;
    }
    let cr = view.crop(w.crop);
    for (sx, sy) in sides(w.ratio().is_some()) {
        let h = cr.center() + egui::vec2(sx as f32 * cr.width(), sy as f32 * cr.height()) / 2.0;
        if h.distance(p) <= GRAB {
            return Grab::Side(sx, sy);
        }
    }
    if cr.contains(p) { Grab::Move } else { Grab::Pan }
}

/// Dragging, the wheel and the double click on the canvas.
fn mouse(ui: &egui::Ui, w: &mut Window, response: &egui::Response, view: &View, rect: Rect) {
    if let Some(p) = response.hover_pos() {
        let icon = match w.drag.map_or_else(|| grab_at(w, view, p), |d| d.0) {
            Grab::Side(sx, sy) if sx == sy => CursorIcon::ResizeNwSe,
            Grab::Side(sx, sy) if sx == -sy => CursorIcon::ResizeNeSw,
            Grab::Side(0, _) => CursorIcon::ResizeVertical,
            Grab::Side(..) => CursorIcon::ResizeHorizontal,
            Grab::Move => CursorIcon::Move,
            Grab::Line => CursorIcon::Crosshair,
            Grab::Pan if w.drag.is_some() => CursorIcon::Grabbing,
            Grab::Pan => CursorIcon::Grab,
        };
        ui.ctx().set_cursor_icon(icon);
    }
    if response.double_clicked() {
        w.view = None;
    }
    // Where the button went down: egui starts a drag only once the pointer has moved a few
    // pixels, and taken there a corner pulled inwards was already "inside" and moved the crop.
    if response.drag_started()
        && let Some(p) = ui.input(|i| i.pointer.press_origin()).or(response.interact_pointer_pos())
    {
        w.drag = Some((grab_at(w, view, p), w.crop, (view.at, view.k), p));
    }
    if response.drag_stopped() {
        // The line drawn becomes level (or upright), and Straighten turns itself off.
        if let (Some((Grab::Line, .., from)), Some(to)) = (w.drag, w.line_end.take()) {
            w.angle = rotate::straightened(w.angle, to.x - from.x, to.y - from.y);
            w.straighten = false;
            w.refit();
        }
        w.drag = None;
    }
    if let (Some((grab, start, (at, k), from)), Some(p)) = (w.drag, response.interact_pointer_pos()) {
        let moved = (p - from) / k;
        match grab {
            Grab::Pan => w.view = Some((at - moved, k)),
            Grab::Line => w.line_end = Some(p),
            Grab::Move => {
                w.crop = Crop { cx: start.cx + moved.x, cy: start.cy + moved.y, ..start };
                w.follow = Follow::Manual;
            }
            Grab::Side(sx, sy) => {
                w.crop = resized(start, sx, sy, view.image(p), w.ratio());
                w.follow = Follow::Manual;
            }
        }
    }
    if response.hovered() {
        wheel(ui, w, view, rect);
    }
}

/// Shift + wheel turns by 0.1° a notch; the wheel (or Ctrl + wheel, or the pinch) zooms around the pointer.
fn wheel(ui: &egui::Ui, w: &mut Window, view: &View, rect: Rect) {
    let (notches, zoom, mods, pointer) = ui.input(|i| {
        // Notches, not distance: one notch is 0.1°. Shift turns the wheel sideways on some systems.
        let n: f32 = i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some((delta.y + delta.x).signum()) } else { None }).sum();
        (n, i.zoom_delta(), i.modifiers, i.pointer.hover_pos())
    });
    if mods.shift {
        if notches != 0.0 {
            w.angle = ((w.angle - notches * 0.1) * 10.0).round() / 10.0;
            w.refit();
        }
        return;
    }
    // Ctrl + wheel already comes as `zoom`.
    let factor = zoom * if mods.command { 1.0 } else { 1.15f32.powf(notches) };
    if factor != 1.0
        && let Some(p) = pointer
    {
        // The point under the pointer stays under it.
        let fixed = view.image(p);
        let k = (view.k * factor).clamp(0.01, 64.0);
        w.view = Some((fixed - (p - rect.center()) / k, k));
    }
}

/// `start` with the side or corner (`sx`, `sy`) moved to `p`, the opposite one staying where it
/// was; with `ratio`, the proportions kept (only corners then). At least a pixel.
fn resized(start: Crop, sx: i8, sy: i8, p: Vec2, ratio: Option<f32>) -> Crop {
    let (mut l, mut r) = (start.cx - start.w / 2.0, start.cx + start.w / 2.0);
    let (mut t, mut b) = (start.cy - start.h / 2.0, start.cy + start.h / 2.0);
    match sx {
        1 => r = p.x.max(l + 1.0),
        -1 => l = p.x.min(r - 1.0),
        _ => {}
    }
    match sy {
        1 => b = p.y.max(t + 1.0),
        -1 => t = p.y.min(b - 1.0),
        _ => {}
    }
    if let Some(ratio) = ratio {
        let (mut w, mut h) = (r - l, b - t);
        if w / h > ratio { h = w / ratio } else { w = h * ratio }
        if sx == 1 { r = l + w } else { l = r - w }
        if sy == 1 { b = t + h } else { t = b - h }
    }
    Crop { cx: (l + r) / 2.0, cy: (t + b) / 2.0, w: r - l, h: b - t }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize() {
        let start = Crop::centred(100.0, 50.0);
        // Right side out by 20: the left one stays.
        assert_eq!(resized(start, 1, 0, egui::vec2(70.0, 0.0), None), Crop { cx: 10.0, cy: 0.0, w: 120.0, h: 50.0 });
        // Top left corner with 1:1: square, the bottom right corner stays.
        let sq = resized(start, -1, -1, egui::vec2(-40.0, -40.0), Some(1.0));
        assert_eq!((sq.w, sq.h, sq.cx + sq.w / 2.0, sq.cy + sq.h / 2.0), (90.0, 90.0, 50.0, 25.0));
        // Past the opposite side: a pixel wide, not turned inside out.
        assert_eq!(resized(start, 1, 0, egui::vec2(-500.0, 0.0), None).w, 1.0);
    }
}
