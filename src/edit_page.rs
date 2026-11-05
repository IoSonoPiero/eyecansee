//! The image editor's page (issue #53, Image > Edit image…, also Rotate and crop…): a page of its
//! own, like "Save in another format". The image is drawn turned by the GPU from its preview, the
//! objects over it from a texture of their own (`edit::render`), redrawn only when the document or
//! the view change. Every change goes into the document (`edit::doc`); once an operation is over
//! (no button held, no field being typed in) it becomes a step of the history (`edit::history`).
//! The tools are in `tools`, Rotate and crop's controls in `crop`, the canvas in `canvas`, the
//! questions before saving or closing in `save`.

use std::sync::Arc;

use eframe::egui::{self, Color32, ColorImage, Pos2, TextureHandle, Vec2};

use crate::edit::doc::{Doc, Obj, Pt, Shape, Style};
use crate::edit::geom::Kind;
use crate::edit::history::History;
use crate::encode::{Format, Original};
use crate::rotate::{self, Crop};
use crate::transform::Turn;
use crate::{App, source};

mod canvas;
mod crop;
mod save;
mod tools;
use canvas::canvas;
use save::Leave;

type Loaded = Result<(Arc<Original>, TextureHandle, u64), String>;

/// Width of the controls on the right.
const SIDE: f32 = 300.0;
const GAP: f32 = 16.0;
/// Distance in points at which a side or corner (of the crop or of an object) can be grabbed.
const GRAB: f32 = 12.0;
/// The error of `Loader::load_original` for an image the editor does not take.
pub(crate) const ANIMATED: &str = "edit-animated";

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

/// Rotate and crop: what a drag started on: a side or corner of the crop (signs of x and y), its
/// inside, or the view.
#[derive(Clone, Copy, PartialEq)]
enum Grab {
    Side(i8, i8),
    Move,
    Pan,
    /// A line along the horizon (or something upright), for Straighten.
    Line,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    Select,
    Draw(Kind),
    Crop,
}

/// The other tools: an object being moved, resized or drawn, or the view moved.
enum Hold {
    Move { index: usize, start: Obj, from: Pt },
    Handle { index: usize, handle: usize, start: Obj },
    /// The object drawn is at `index` once the drag is long enough.
    Draw { kind: Kind, from: Pt, index: Option<usize> },
    Pan { at: Vec2, k: f32, from: Pos2 },
}

/// What the texture of the objects was drawn for.
#[derive(PartialEq)]
struct Drawn {
    doc: Doc,
    at: Vec2,
    k: f32,
    rect: egui::Rect,
    ppp: f32,
}

pub struct Window {
    pub path: String,
    original: Option<Loaded>,
    /// The document shown, changed by the controls and the mouse.
    doc: Doc,
    /// The operations done; `None` until the image is read.
    history: Option<History>,
    /// Flips and quarter turns of the viewer, the document's at the start.
    start_turn: Turn,
    tool: Tool,
    selected: Option<usize>,
    hold: Option<Hold>,
    objects: Option<(Drawn, TextureHandle)>,
    /// The last style of each kind of object, the widths in pixels of this image.
    styles: Vec<(Kind, Style)>,
    styles_changed: bool,
    /// The text field takes the keys in the next frame (a text just placed).
    focus_text: bool,
    /// A line under the buttons ("Copied…") and until when (seconds of `egui::InputState::time`).
    note: Option<(String, f64)>,
    copying: bool,
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
    /// Rotate and crop: the drag under way: what, the crop and the view when it started, where the pointer was.
    drag: Option<(Grab, Crop, (Vec2, f32), Pos2)>,
    /// Straighten is on: a drag draws a line, which becomes level when let go.
    straighten: bool,
    /// Where the line being drawn ends.
    line_end: Option<Pos2>,
    preparing: bool,
    /// The copy (or the image for an overwrite) made: its confirmation is open.
    pub ready: Option<save::Ready>,
    /// On a phone or a server: the image is edited in a copy on this computer (a line at the top).
    remote: Option<&'static str>,
    /// Closing with changes not saved: the question is open.
    pub leaving: bool,
    /// Save as… chosen in the message saying Save cannot write over the original.
    pub save_as: bool,
}

impl Window {
    fn new(path: String, turn: Turn, tool: Tool) -> Window {
        // JPEG has no transparency: white there, transparent elsewhere.
        let transparent = Format::of_file(&path) != Some(Format::Jpeg);
        Window {
            path,
            original: None,
            doc: Doc::new(1, 1, None),
            history: None,
            start_turn: turn,
            tool,
            selected: None,
            hold: None,
            objects: None,
            styles: Vec::new(),
            styles_changed: false,
            focus_text: false,
            note: None,
            copying: false,
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
            remote: None,
            leaving: false,
            save_as: false,
        }
    }

    /// Size of the image as read, upright.
    fn upright(&self) -> (u32, u32) {
        match &self.original {
            Some(Ok((o, ..))) => (o.image.width(), o.image.height()),
            _ => (1, 1),
        }
    }

    /// Size of the image once flipped and turned as in the document.
    fn size(&self) -> (f32, f32) {
        let (w, h) = self.upright();
        if self.doc.turn.sideways() { (h as f32, w as f32) } else { (w as f32, h as f32) }
    }

    /// The long side, for the widths that look the same on any photo.
    fn long(&self) -> f32 {
        let (w, h) = self.upright();
        w.max(h) as f32
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

    fn refit(&mut self) {
        let (iw, ih) = self.size();
        // Free proportions: those of the image as it stands after the nearest quarter turn (at 90°
        // the image's own ones cut a strip off it).
        let angle = self.doc.angle;
        let upright = if (angle / 90.0).round() as i32 % 2 != 0 { ih / iw } else { iw / ih };
        self.doc.crop = match self.follow {
            Follow::Inside => rotate::inside(iw, ih, angle, self.ratio().unwrap_or(upright)),
            Follow::Whole => {
                let (bw, bh) = rotate::bounds(iw, ih, angle);
                let all = Crop::centred(bw, bh);
                self.ratio().map_or(all, |r| all.fit(r))
            }
            Follow::Manual => self.ratio().map_or(self.doc.crop, |r| self.doc.crop.fit(r)),
        };
    }

    /// Anything not saved yet.
    fn changed(&self) -> bool {
        self.history.as_ref().is_some_and(|h| h.changed() || self.doc != *h.doc())
    }

    /// The style a new object of `kind` starts with.
    fn style(&self, kind: Kind) -> Style {
        self.styles.iter().find(|(k, _)| *k == kind).map_or_else(|| default_style(kind, self.long()), |(_, s)| *s)
    }

    fn remember(&mut self, kind: Kind, style: Style) {
        if self.style(kind) != style {
            self.styles.retain(|(k, _)| *k != kind);
            self.styles.push((kind, style));
            self.styles_changed = true;
        }
    }

    /// After undo or redo: the controls that mirror the document follow it.
    fn jumped(&mut self) {
        self.hold = None;
        self.selected = self.selected.filter(|&i| i < self.doc.objects.len());
        self.transparent = self.doc.fill.is_none();
        if let Some([r, g, b]) = self.doc.fill {
            self.color = Color32::from_rgb(r, g, b);
        }
    }

    fn undo(&mut self) {
        let Some(h) = &mut self.history else { return };
        h.push(self.doc.clone());
        h.undo();
        self.doc = h.doc().clone();
        self.jumped();
    }

    fn redo(&mut self) {
        let Some(h) = &mut self.history else { return };
        h.push(self.doc.clone());
        h.redo();
        self.doc = h.doc().clone();
        self.jumped();
    }
}

/// Where each kind of object starts the first time: red lines a 250th of the long side (at least
/// 2 pixels), letters a 25th, the highlighter yellow at 40%, the cover box black.
fn default_style(kind: Kind, long: f32) -> Style {
    let width = (long / 250.0).round().max(2.0);
    match kind {
        Kind::Highlight => Style::highlighter(),
        Kind::Cover => Style { color: [0, 0, 0], width, ..Style::default() },
        Kind::Text => Style { width: (long / 25.0).round().max(8.0), ..Style::default() },
        _ => Style { width, ..Style::default() },
    }
}

/// What the buttons of the panel asked for.
enum Ask {
    Save,
    SaveAs,
    Overwrite,
    Close,
}

impl App {
    /// The image Edit image… and Rotate and crop… would open: one picture (in the viewer the one
    /// shown); `None` too for one the viewer found animated or of several pages.
    pub(crate) fn edit_target(&self, path: Option<String>) -> Option<String> {
        let picture = if self.viewing.is_some() {
            self.viewing_photo().then(|| self.target()).flatten()
        } else {
            let images = self.grid_images(path);
            if images.len() == 1 { images.into_iter().next() } else { None }
        };
        picture.filter(|p| !self.animated.contains(p))
    }

    pub(crate) fn can_edit(&self, path: Option<String>) -> bool {
        self.edit_target(path).is_some()
    }

    /// Why the editor is off for the item: several images, or one found animated.
    pub(crate) fn edit_off(&self, path: Option<String>) -> Option<String> {
        if self.viewing.is_none() && self.grid_images(path.clone()).len() > 1 {
            Some(self.i18n.t("rc-one-only"))
        } else if path.or_else(|| self.target()).is_some_and(|p| self.animated.contains(&p)) {
            Some(self.i18n.t("edit-animated-off"))
        } else {
            None
        }
    }

    /// Edit image… (`Tool::Select`) or Rotate and crop… (`Tool::Crop`) on one image.
    pub(crate) fn open_editor(&mut self, path: Option<String>, tool: Tool) {
        // In the grid the one picture among the files chosen (a video next to it is left out).
        let picture = if self.viewing.is_some() { None } else { self.grid_images(path.clone()).into_iter().next() };
        let Some(path) = picture.or(path).or_else(|| self.target()).filter(|p| crate::decode::is_image(p)) else { return };
        if self.animated.contains(&path) {
            return;
        }
        let Some(loader) = &self.loader else { return };
        loader.load_original(path.clone(), true);
        // From the viewer: as it is shown there.
        let turn = if self.viewing.is_some() { self.turn } else { Turn::default() };
        let mut w = Window::new(path, turn, tool);
        let store = loader.store();
        w.remote = crate::kept::remote(&store).then_some(if matches!(store, crate::source::Store::Mtp(_)) { "edit-remote-phone" } else { "edit-remote-server" });
        // Fractions of the long side until the image is read (`editor_original`).
        w.styles = self.settings.edit_styles.clone();
        self.editor = Some(w);
    }

    pub(crate) fn editor_original(&mut self, ctx: &egui::Context, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        let Some(w) = &mut self.editor else { return };
        w.original = Some(r.map(|(original, preview, size)| (original, ctx.load_texture("edit-original", preview, egui::TextureOptions::LINEAR), size)));
        if w.history.is_some() {
            return;
        }
        // The document starts from the image as the viewer showed it.
        let (iw, ih) = w.upright();
        let mut doc = Doc::new(iw, ih, (!w.transparent).then(|| [w.color.r(), w.color.g(), w.color.b()]));
        doc.turn = w.start_turn;
        if doc.turn.sideways() {
            doc.crop = Crop::centred(ih as f32, iw as f32);
        }
        w.doc = doc;
        w.refit();
        w.history = Some(History::new(w.doc.clone()));
        // The widths remembered are fractions of the long side: in pixels of this image now.
        let long = w.long();
        for (_, s) in &mut w.styles {
            s.width = (s.width * long).max(1.0);
        }
    }

    /// Drawn in place of the whole interface, as "Save in another format".
    pub(crate) fn edit_page(&mut self, ui: &mut egui::Ui) {
        let Some(mut w) = self.editor.take() else { return };
        // Before the fields are drawn: Esc in a field or during a drag stops only that.
        let busy = ui.ctx().egui_wants_keyboard_input() || w.hold.is_some();
        let mut open = crate::convert::page_header(ui, &self.i18n.t("edit-title"));
        if !open && busy {
            open = true;
            if w.hold.take().is_some()
                && let Some(h) = &w.history
            {
                w.doc = h.doc().clone();
            }
        }
        let tex = match &w.original {
            None => {
                ui.centered_and_justified(|ui| ui.spinner());
                None
            }
            Some(Err(e)) if e == ANIMATED => {
                ui.label(crate::browser::error_text(ui, self.i18n.t_args("edit-animated", &[("name", source::name(&w.path))])));
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
            self.edit_keys(ui.ctx(), &mut w);
            let size = ui.available_size();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                canvas(ui, &mut w, &tex, egui::vec2(size.x - SIDE - GAP, size.y));
                ui.vertical(|ui| {
                    ui.set_width(SIDE);
                    let ask = self.side_panel(ui, &mut w);
                    match if std::mem::take(&mut w.save_as) { Some(Ask::SaveAs) } else { ask } {
                        Some(Ask::Save) => self.edit_prepare(ui.ctx(), &mut w, false),
                        Some(Ask::Overwrite) => self.edit_prepare(ui.ctx(), &mut w, true),
                        Some(Ask::SaveAs) => {
                            handed = self.rotate_save_as(ui.ctx(), &w);
                            open = !handed;
                        }
                        Some(Ask::Close) => open = false,
                        None => {}
                    }
                });
            });
            self.settle(ui.ctx(), &mut w);
        }
        // Closing (Esc, ✕, Close) with changes not saved: asked first. Not after Save as…: the
        // page stayed open under it and took the image meant for it (spinner for ever).
        if !open && !handed && w.changed() && w.ready.is_none() {
            w.leaving = true;
            open = true;
        }
        if w.leaving {
            match self.rotate_leave(ui.ctx(), &mut w) {
                Some(Leave::Save) => self.edit_prepare(ui.ctx(), &mut w, false),
                Some(Leave::SaveAs) => open = !self.rotate_save_as(ui.ctx(), &w),
                Some(Leave::Discard) => open = false,
                None => {}
            }
        }
        if w.ready.is_some() && self.edit_confirm(ui.ctx(), &mut w) {
            open = false;
        }
        if open {
            self.editor = Some(w);
        }
    }

    /// Undo, redo and Del, unless a field takes the keys (text has its own undo there).
    fn edit_keys(&self, ctx: &egui::Context, w: &mut Window) {
        if ctx.egui_wants_keyboard_input() || w.hold.is_some() || w.drag.is_some() {
            return;
        }
        use egui::{Key, Modifiers};
        let (redo, undo, delete) = ctx.input_mut(|i| {
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y);
            (redo, i.consume_key(Modifiers::COMMAND, Key::Z), i.consume_key(Modifiers::NONE, Key::Delete))
        });
        if redo {
            w.redo();
        } else if undo {
            w.undo();
        }
        if delete && let Some(i) = w.selected.take() {
            w.doc.objects.remove(i);
        }
    }

    /// End of the frame: an operation over (no button held, no field typed in, no drag) becomes a
    /// step of the history; a text left empty goes; the styles changed are kept in the settings.
    fn settle(&mut self, ctx: &egui::Context, w: &mut Window) {
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input() || w.focus_text || w.hold.is_some() || w.drag.is_some();
        if busy {
            return;
        }
        if let Some(i) = w.doc.objects.iter().position(|o| matches!(&o.shape, Shape::Text { text, .. } if text.trim().is_empty())) {
            w.doc.objects.remove(i);
            w.selected = w.selected.filter(|&s| s != i).map(|s| if s > i { s - 1 } else { s });
        }
        if let Some(h) = &mut w.history {
            h.push(w.doc.clone());
        }
        if std::mem::take(&mut w.styles_changed) {
            let long = w.long();
            self.settings.edit_styles = w.styles.iter().map(|(k, s)| (*k, Style { width: s.width / long, ..*s })).collect();
            self.settings.save();
        }
    }

    /// Ctrl+C or Copy: the result at full size, made in the background (`Msg::EditCopy`).
    pub(crate) fn edit_copy(&mut self, ctx: &egui::Context) {
        let Some(w) = &mut self.editor else { return };
        let Some(Ok((original, ..))) = &w.original else { return };
        if w.copying {
            return;
        }
        w.copying = true;
        let (doc, original, tx, ctx) = (w.doc.clone(), original.clone(), self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let img = crate::edit::save::result(&doc, &original, &crate::decode::usvg_options());
            if tx.send(crate::loader::Msg::EditCopy(crate::decode::to_color_image(img))).is_ok() {
                ctx.request_repaint();
            }
        });
    }

    pub(crate) fn on_edit_copy(&mut self, ctx: &egui::Context, img: ColorImage) {
        let Some(w) = &mut self.editor else { return };
        w.copying = false;
        ctx.copy_image(img);
        w.note = Some((self.i18n.t("edit-copied"), ctx.input(|i| i.time) + 4.0));
    }
}
