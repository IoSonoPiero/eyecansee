//! "Rotate and crop…" (Image > Transform): a page of its own, like "Save in another format". The
//! image is drawn rotated by the GPU with the crop over it; Save writes it over the original after a
//! confirmation, "Save as…" hands it to "Save in another format". The pixels: `rotate`.

use std::sync::Arc;

use eframe::egui::{self, Color32, ColorImage, CursorIcon, Pos2, Rect, TextureHandle, Vec2};

use crate::encode::{Format, Original};
use crate::i18n::I18n;
use crate::rotate::{self, Crop, Edit};
use crate::transform::Turn;
use crate::{App, confirm, source};

type Loaded = Result<(Arc<Original>, TextureHandle, u64), String>;

/// Width of the controls on the right.
const SIDE: f32 = 300.0;
const GAP: f32 = 16.0;
/// Distance in points at which a side or corner of the crop can be grabbed.
const GRAB: f32 = 10.0;

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
    preparing: bool,
    /// The file to write and whether it lost quality: the confirmation is open.
    pub ready: Option<(Arc<Vec<u8>>, bool)>,
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
            preparing: false,
            ready: None,
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
        self.crop = match self.follow {
            Follow::Inside => rotate::inside(iw, ih, self.angle, self.ratio().unwrap_or(iw / ih)),
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

    /// Does the result differ from the original?
    fn changed(&self) -> bool {
        let (iw, ih) = self.size();
        !self.turn.is_none() || self.angle.rem_euclid(360.0) != 0.0 || self.crop.pixels() != (iw as u32, ih as u32) || self.crop.cx.abs() >= 0.5 || self.crop.cy.abs() >= 0.5
    }
}

/// What the buttons at the bottom of the controls asked for.
enum Ask {
    Save,
    SaveAs,
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
        w.original = Some(r.map(|(original, preview, size)| (original, ctx.load_texture("rotate-original", preview, egui::TextureOptions::LINEAR), size)));
        w.refit();
    }

    /// The image changed in memory for Save: the confirmation opens.
    pub(crate) fn on_edited(&mut self, path: String, r: Result<(Arc<Vec<u8>>, bool), source::Error>) {
        self.saving.remove(&path);
        let Some(w) = self.rotate.as_mut().filter(|w| w.path == path) else { return };
        w.preparing = false;
        match r {
            Ok(ready) => w.ready = Some(ready),
            Err(e) => self.save_failed(&path, e),
        }
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
        if let Some(tex) = tex {
            let size = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                canvas(ui, &mut w, &tex, egui::vec2(size.x - SIDE - GAP, size.y));
                ui.vertical(|ui| {
                    ui.set_width(SIDE);
                    match self.rotate_controls(ui, &mut w) {
                        Some(Ask::Save) => self.rotate_save(&mut w),
                        Some(Ask::SaveAs) => open = !self.rotate_save_as(ui.ctx(), &w),
                        None => {}
                    }
                });
            });
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
        let why_not = self.why_not_save(w);
        let mut ask = None;
        ui.horizontal(|ui| {
            let ready = w.changed() && why_not.is_none() && !w.preparing;
            let r = ui.add_enabled(ready, egui::Button::new(i18n.t("menu-save")));
            if r.clicked() {
                ask = Some(Ask::Save);
            }
            if let Some(why) = why_not {
                r.on_disabled_hover_text(why);
            }
            if ui.add_enabled(!w.preparing, egui::Button::new(i18n.t("menu-save-copy"))).clicked() {
                ask = Some(Ask::SaveAs);
            }
            if w.preparing {
                ui.spinner();
            }
        });
        ui.add_space(12.0);
        ui.weak(i18n.t("rc-help"));
        ask
    }

    /// Why Save cannot write over the original, if it cannot.
    fn why_not_save(&self, w: &Window) -> Option<String> {
        if self.loader.as_ref().is_some_and(|l| !l.can_rename()) {
            Some(self.i18n.t("save-no-rename"))
        } else if Format::of_file(&w.path).is_none() || !self.can_change(&w.path) || self.saving.contains(&w.path) {
            Some(self.i18n.t_args("save-format", &[("name", &crate::save_dialogs::md(source::name(&w.path)))]))
        } else {
            None
        }
    }

    fn rotate_save(&mut self, w: &mut Window) {
        let Some(loader) = &self.loader else { return };
        log!("prepare {:?} {:?}", w.path, w.edit());
        loader.prepare_edit(w.path.clone(), w.edit());
        self.saving.insert(w.path.clone());
        w.preparing = true;
    }

    /// "Save in another format" with the image rotated and cropped, made in the background
    /// (`Msg::Original`); `true` once opened.
    fn rotate_save_as(&mut self, ctx: &egui::Context, w: &Window) -> bool {
        let Some(Ok((original, _, size))) = &w.original else { return false };
        let (original, edit, size, tx, ctx) = (original.clone(), w.edit(), *size, self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut o = Original::clone(&original);
            edit.apply(&mut o.image);
            let preview = crate::decode::preview(&o.image);
            if tx.send(crate::loader::Msg::Original(Ok((Arc::new(o), preview, size)))).is_ok() {
                ctx.request_repaint();
            }
        });
        self.convert = Some(crate::convert::Window::new(w.path.clone()));
        true
    }

    /// Overwrite? As Save in the viewer. `true` once written (the page closes).
    fn rotate_confirm(&mut self, ctx: &egui::Context, w: &mut Window) -> bool {
        let (Some((data, lossy)), Some(loader)) = (w.ready.clone(), &self.loader) else { return false };
        let how = loader.deletion(&w.path, self.settings.local_permanent, self.settings.remote_delete);
        let text = self.save_confirm_text(&w.path, how, lossy);
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        match confirm::yes_no(ctx, "rotate-save", 420.0, text, yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                log!("save {:?} {how:?}", w.path);
                loader.replace_with(w.path.clone(), data, how);
                self.saving.insert(w.path.clone());
                w.ready = None;
                true
            }
            Some(false) => {
                w.ready = None;
                false
            }
            None => false,
        }
    }
}

/// The angle, with buttons for the quarter turns and back to zero. `true` if it changed.
fn angle_controls(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let before = w.angle;
    ui.strong(i18n.t("rc-rotation"));
    ui.spacing_mut().slider_width = SIDE - 80.0;
    ui.add(egui::Slider::new(&mut w.angle, -180.0..=180.0).step_by(0.1).fixed_decimals(1).suffix("°"));
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
            Grab::Pan if w.drag.is_some() => CursorIcon::Grabbing,
            Grab::Pan => CursorIcon::Grab,
        };
        ui.ctx().set_cursor_icon(icon);
    }
    if response.double_clicked() {
        w.view = None;
    }
    if response.drag_started()
        && let Some(p) = response.interact_pointer_pos()
    {
        w.drag = Some((grab_at(w, view, p), w.crop, (view.at, view.k), p));
    }
    if response.drag_stopped() {
        w.drag = None;
    }
    if let (Some((grab, start, (at, k), from)), Some(p)) = (w.drag, response.interact_pointer_pos()) {
        let moved = (p - from) / k;
        match grab {
            Grab::Pan => w.view = Some((at - moved, k)),
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
