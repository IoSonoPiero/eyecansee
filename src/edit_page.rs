//! The image editor's page (issue #53, Image > Edit image…, also Rotate and crop…): a page of its
//! own, like "Save in another format". The image is drawn turned by the GPU from its preview, the
//! objects over it from a texture of their own (`edit::render`), redrawn only when the document or
//! the view change. Every change goes into the document (`edit::doc`); once an operation is over
//! (no button held, no field being typed in) it becomes a step of the history (`edit::history`).
//! The bars (tools, properties, status) are in `tools`, Rotate and crop's controls in `crop`, the
//! canvas in `canvas`, the questions before saving or closing in `save`.

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
mod clip;
mod crop;
mod pick;
mod props;
mod save;
mod tools;
mod typing;
use canvas::canvas;
pub(crate) use save::Prepared;
use save::Leave;

type Loaded = Result<(Arc<Original>, TextureHandle, u64), String>;

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
    /// A part of the image chosen, to copy, cut or (later) filter.
    Marquee,
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
    /// The selection being drawn, from where the drag started.
    Marquee { from: Pt },
}

/// What the texture of the objects was drawn for.
#[derive(PartialEq)]
struct Drawn {
    doc: Doc,
    /// Previews of filters made since (`Window::filters_made`): not in the document's equality.
    made: u64,
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
    /// The tool before Rotate and crop: Apply goes back to it.
    before_crop: Tool,
    selected: Option<usize>,
    hold: Option<Hold>,
    objects: Option<(Drawn, TextureHandle)>,
    /// The last style of each kind of object, the widths in pixels of this image.
    styles: Vec<(Kind, Style)>,
    styles_changed: bool,
    /// The text field takes the keys in the next frame (a text just placed).
    focus_text: bool,
    /// The font of the next text: the last one chosen.
    text_font: Option<String>,
    /// The list of fonts is open: the text shows the font pointed at; its own font, to put back
    /// if none is chosen.
    font_preview: Option<(usize, Option<String>)>,
    /// The eyedropper is on: a click on the image takes its colour.
    picking: bool,
    /// The screen's eyedropper is open (the desktop's picker).
    screen_picking: bool,
    /// A colour taken, for the style shown in the properties bar.
    picked: Option<[u8; 3]>,
    /// The colour just taken, for its codes in the status bar.
    just_picked: Option<[u8; 3]>,
    /// The eyedropper takes one pixel, or the average of 3 × 3 or 5 × 5.
    pick_size: u8,
    /// I pressed: the eyedropper as if its button were clicked.
    /// I or Shift+I pressed: the eyedropper as if its button were clicked; `Some(true)` for the
    /// desktop's picker on the whole screen (Shift+I, Linux).
    ask_pick: Option<bool>,
    /// The part of the image chosen with Selection (upright on the screen when drawn).
    marquee: Option<crate::edit::doc::Area>,
    /// A part copied or cut: Paste puts it back as a patch where it was (a little aside).
    clip: Option<(crate::edit::doc::Area, crate::edit::doc::Pixels)>,
    /// How many times the previews of the filters were made: the objects are drawn again then.
    filters_made: u64,
    /// What the lenses' previews were made with (`photo::hiders_key`).
    hiders: u64,
    /// The list of the keys stays open (the Keys button clicked).
    keys_open: bool,
    /// The text (or bubble) being typed on the image itself (`typing`).
    editing: Option<usize>,
    /// The original's bytes, read at the first save: every save starts from them.
    bytes: Option<Arc<Vec<u8>>>,
    /// The copy written last, updated by the next Save while it is still that file.
    copy: Option<save::Written>,
    /// What Overwrite wrote last over the original: the next Overwrite replaces it without the
    /// trash while it is still that file.
    overwritten: Option<save::Written>,
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
    /// The view drawn last (that of `view`, or everything in view): for the zoom buttons.
    shown: (Vec2, f32),
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
    /// A new image (File > New image…): no file yet, its Save is Save as….
    new: bool,
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
            before_crop: Tool::Select,
            selected: None,
            hold: None,
            objects: None,
            styles: Vec::new(),
            styles_changed: false,
            focus_text: false,
            text_font: None,
            font_preview: None,
            picking: false,
            screen_picking: false,
            picked: None,
            just_picked: None,
            pick_size: 1,
            ask_pick: None,
            marquee: None,
            clip: None,
            filters_made: 0,
            hiders: 0,
            keys_open: false,
            editing: None,
            bytes: None,
            copy: None,
            overwritten: None,
            note: None,
            copying: false,
            follow: Follow::Inside,
            aspect: Aspect::Free,
            custom: [4.0, 3.0],
            swapped: false,
            transparent,
            color: Color32::WHITE,
            view: None,
            shown: (Vec2::ZERO, 1.0),
            drag: None,
            straighten: false,
            line_end: None,
            preparing: false,
            ready: None,
            remote: None,
            leaving: false,
            save_as: false,
            new: false,
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
        let first = default_style(kind, self.long());
        // Letters of no size: a style kept before they had one.
        self.styles.iter().find(|(k, _)| *k == kind).map_or(first, |(_, s)| Style { letters: if s.letters > 0.0 { s.letters } else { first.letters }, ..*s })
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

    /// Esc, one thing at a time (user, 2026-11-05: Esc after placing a text closed the page): the
    /// eyedropper or a drag stops, a text being typed or just placed is done (and goes if empty),
    /// the selection goes. `false` when there was nothing of it: Esc closes the page then.
    fn escape(&mut self) -> bool {
        if std::mem::take(&mut self.picking) {
            return true;
        }
        if self.hold.take().is_some() {
            if let Some(h) = &self.history {
                self.doc = h.doc().clone();
            }
            return true;
        }
        if self.marquee.take().is_some() {
            return true;
        }
        let Some(i) = self.selected.take() else { return false };
        self.focus_text = false;
        if let Some(Shape::Text { text, .. }) = self.doc.objects.get(i).map(|o| &o.shape)
            && text.trim().is_empty()
        {
            self.doc.objects.remove(i);
        }
        true
    }

    /// A tool of the toolbar (or its key): changing tool lets go of the selection (but for Select).
    fn choose(&mut self, tool: Tool) {
        if tool == self.tool {
            return;
        }
        // Rotate and crop shows all of the image, the others the result.
        if (tool == Tool::Crop) != (self.tool == Tool::Crop) {
            self.view = None;
        }
        if tool == Tool::Crop {
            self.before_crop = self.tool;
        }
        if tool != Tool::Marquee {
            self.marquee = None;
        }
        self.tool = tool;
        if tool != Tool::Select {
            self.selected = None;
        }
        self.hold = None;
    }

    /// Rotate and crop's Apply (or Enter): back to the tool before, which shows the result.
    fn apply_crop(&mut self) {
        let back = self.before_crop;
        self.choose(back);
    }

    /// A quarter turn: the document turns, objects and crop with it (`Doc::turned`).
    fn turn(&mut self, op: crate::transform::Op) {
        self.doc.turned(op);
        self.view = None;
        self.refit();
    }

    /// The filters, highlighters and lenses whose preview is missing or was made for another box
    /// or amount (colour, zoom): made again from the photo, at most 1200 pixels a side; while a
    /// drag is under way at most 400, to follow the pointer (the old one, stretched over the new
    /// place, showed the part it came from: user, 2026-11-06), and sharp again once let go. The
    /// lenses last, with the filters' previews and the cover boxes on them, and again whenever
    /// those changed.
    fn refresh_previews(&mut self) {
        use crate::edit::doc::{Pixels, Preview};
        use crate::edit::photo;
        let Some(Ok((original, ..))) = &self.original else { return };
        let still = self.hold.is_none();
        for lenses in [false, true] {
            if lenses && still && std::mem::replace(&mut self.hiders, photo::hiders_key(&self.doc)) != self.hiders {
                for o in &mut self.doc.objects {
                    if let Shape::Lens { preview, .. } = &mut o.shape {
                        *preview = Preview::default();
                    }
                }
            }
            for i in 0..self.doc.objects.len() {
                let o = &self.doc.objects[i];
                let (area, amount, preview) = match &o.shape {
                    Shape::Filter { area, amount, preview, .. } if !lenses => (*area, *amount, preview),
                    Shape::Highlight { area, preview } if !lenses => (*area, photo::colour_key(o.style.color), preview),
                    // Its key: the zoom, a half more when it shows the objects.
                    Shape::Lens { area, preview } if lenses => (*area, o.style.zoom.max(1) as f32 + if o.style.with_objects { 0.5 } else { 0.0 }, preview),
                    _ => continue,
                };
                // A highlighter is quick to make and is over text, read closely: sharper.
                let side = match (still, &o.shape) {
                    (false, _) => 400.0,
                    (true, Shape::Highlight { .. }) => 3000.0,
                    _ => 1200.0,
                };
                let scale = (side / area.w.max(area.h)).min(1.0);
                // Made for this box and amount, and as sharp as it is to be now.
                let sharp = |p: &Pixels| p.image().width() + 1 >= (area.w * scale).round() as u32;
                if preview.0.as_ref().is_some_and(|(a, m, p)| *a == area && *m == amount && sharp(p)) {
                    continue;
                }
                let pixels = match &o.shape {
                    Shape::Filter { filter, .. } => {
                        let under = image::DynamicImage::ImageRgba8(self.doc.pixels_under_at(&original.image, &area, scale));
                        crate::edit::filter::apply(&under, *filter, amount, scale).to_rgba8()
                    }
                    Shape::Highlight { .. } => {
                        let mut under = self.doc.pixels_under_at(&original.image, &area, scale);
                        photo::highlighted(&mut under, o.style.color);
                        under
                    }
                    _ => {
                        let filtered: Vec<photo::Filtered> = self.doc.objects.iter().filter_map(|f| if let Shape::Filter { preview: Preview(Some((a, _, p))), .. } = &f.shape { Some((*a, p.image())) } else { None }).collect();
                        let fonts = crate::decode::usvg_options();
                        let before = o.style.with_objects.then_some((&self.doc.objects[..i], &fonts));
                        photo::lens(&self.doc, &original.image, &area, (amount.floor(), side as u32), &filtered, before)
                    }
                };
                if let Shape::Filter { preview, .. } | Shape::Lens { preview, .. } | Shape::Highlight { preview, .. } = &mut self.doc.objects[i].shape {
                    *preview = Preview(Some((area, amount, Pixels::new(pixels))));
                }
                self.filters_made += 1;
            }
        }
    }

    /// A filter from the menu: on the part chosen with Selection, or on the whole image; selected,
    /// with Select chosen to move it or change its amount.
    fn add_filter(&mut self, filter: crate::edit::filter::Filter) {
        let (iw, ih) = self.size();
        let area = self.marquee.unwrap_or(crate::edit::doc::Area { centre: Pt::new(0.0, 0.0), w: iw, h: ih, rot: 0.0 });
        let amount = filter.amount(self.long()).map_or(0.0, |(_, _, start)| start);
        self.doc.objects.push(Obj { shape: Shape::Filter { area, filter, amount, preview: Default::default() }, style: Style::default() });
        self.selected = Some(self.doc.objects.len() - 1);
        self.choose(Tool::Select);
        // `choose` keeps it when Select was already the tool.
        self.marquee = None;
    }

    /// Zoom in or out around the middle of the view.
    fn zoom(&mut self, factor: f32) {
        let (at, k) = self.shown;
        self.view = Some((at, (k * factor).clamp(0.01, 64.0)));
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
        Kind::Step => Style { width: (long / 20.0).round().max(16.0), ..Style::default() },
        Kind::Stamp => Style { color: [46, 160, 67], width: (long / 12.0).round().max(24.0), ..Style::default() },
        Kind::Spotlight => Style { opacity: 0.6, ..Style::default() },
        Kind::Lens => Style { color: [255, 255, 255], width, round: true, zoom: 2, ..Style::default() },
        Kind::Bubble => Style { color: [0, 0, 0], width, fill: Some([255, 255, 255]), letters: (long / 30.0).round().max(8.0), ..Style::default() },
        _ => Style { width, ..Style::default() },
    }
}

/// What the buttons of the bars (or their keys) asked for.
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

    /// File > New image…: the empty image in the editor, named in the folder open; Save as…
    /// writes it.
    pub(crate) fn open_new_image(&mut self, ctx: &egui::Context, image: image::DynamicImage, name: &str) {
        if self.loader.is_none() {
            return;
        }
        let mut w = Window::new(format!("{}{name}.png", self.dir), Turn::default(), Tool::Select);
        w.new = true;
        w.styles = self.settings.edit_styles.clone();
        self.editor = Some(w);
        let original = Arc::new(Original::with_meta(image, &[]));
        let preview = crate::decode::preview(&original.image);
        self.editor_original(ctx, Ok((original, preview, 0)));
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
            s.letters *= long;
        }
    }

    /// Drawn in place of the whole interface, as "Save in another format": the tools at the top,
    /// the properties under them, the status at the bottom, the image in the middle.
    pub(crate) fn edit_page(&mut self, ui: &mut egui::Ui) {
        let Some(mut w) = self.editor.take() else { return };
        let busy = ui.ctx().egui_wants_keyboard_input() || w.hold.is_some() || w.picking;
        let mut open = !crate::theme::close_asked(ui);
        // The window's ✕ during a drag or while typing: it stops only that.
        if !open && busy {
            open = true;
            w.escape();
        }
        // With a question open (save, leave) Esc is its Cancel, not the page's.
        let asking = w.ready.is_some() || w.leaving;
        if !asking && ui.input(|i| i.key_pressed(egui::Key::Escape)) && !w.escape() {
            open = false;
        }
        let tex = match &w.original {
            Some(Ok((_, tex, _))) => Some(tex.clone()),
            _ => None,
        };
        let mut handed = false;
        if let Some(tex) = tex {
            w.refresh_previews();
            let mut ask = self.edit_keys(ui.ctx(), &mut w);
            egui::Panel::top("edit-tools").show(ui, |ui| ask = self.edit_toolbar(ui, &mut w).or(ask.take()));
            egui::Panel::top("edit-props").show(ui, |ui| self.props_bar(ui, &mut w));
            let bar = egui::Frame::side_top_panel(ui.style()).fill(crate::theme::solid(ui.visuals()));
            let status = egui::Panel::bottom("edit-status").frame(bar).show(ui, |ui| self.edit_status(ui, &mut w));
            // The keys of the tool in use: listed while the button is pointed at, or until it is
            // clicked again.
            if status.inner.clicked() {
                w.keys_open = !w.keys_open;
            }
            if w.keys_open || status.inner.hovered() {
                self.keys_window(ui.ctx(), tools::keys_id(w.tool), status.response.rect.height());
            }
            let stage = egui::Frame::NONE.fill(crate::theme::stage(ui.visuals()));
            egui::CentralPanel::default().frame(stage).show(ui, |ui| canvas(ui, &mut w, &tex, ui.available_size()));
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
            // Bubbles as tall as their text needs (it goes to a new line at their width).
            let fonts = crate::decode::usvg_options();
            for o in &mut w.doc.objects {
                crate::edit::bubble::fit(o, &fonts);
            }
            self.settle(ui.ctx(), &mut w);
        } else {
            open &= self.edit_not_ready(ui, &w);
        }
        // Closing (Esc, ✕, Close) with changes not saved: asked first. Not after Save as…: the
        // page stayed open under it and took the image meant for it (spinner for ever).
        if !open && !handed && w.changed() && w.ready.is_none() {
            w.leaving = true;
            open = true;
        }
        if w.leaving {
            match self.rotate_leave(ui.ctx(), &mut w) {
                Some(Leave::Save) if !w.new => self.edit_prepare(ui.ctx(), &mut w, false),
                Some(Leave::Save | Leave::SaveAs) => open = !self.rotate_save_as(ui.ctx(), &w),
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

    /// The image being read, or why it cannot be edited, with Close; `false` once Close is clicked.
    fn edit_not_ready(&self, ui: &mut egui::Ui, w: &Window) -> bool {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| self.not_ready_inside(ui, w)).inner
    }

    fn not_ready_inside(&self, ui: &mut egui::Ui, w: &Window) -> bool {
        let mut open = true;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                open = !ui.add(crate::icons::button(ui, crate::icons::Line::Close)).on_hover_text(format!("{} (Esc)", self.i18n.t("rc-close"))).clicked();
            });
        });
        match &w.original {
            Some(Err(e)) if e == ANIMATED => {
                ui.label(crate::browser::error_text(ui, self.i18n.t_args("edit-animated", &[("name", source::name(&w.path))])));
            }
            Some(Err(e)) => {
                ui.label(crate::browser::error_text(ui, self.i18n.t_args("cannot-load", &[("file", source::name(&w.path)), ("error", e)])));
            }
            _ => {
                crate::theme::loading(ui, self.i18n.t("loading-file"));
            }
        }
        open
    }

    /// Without a field taking the keys (text has its own undo there): undo and redo, Del, the
    /// tools' letters, the quarter turns, zoom, Ctrl+S (Save copy) and Ctrl+Shift+S (Save as…).
    fn edit_keys(&self, ctx: &egui::Context, w: &mut Window) -> Option<Ask> {
        if ctx.egui_wants_keyboard_input() || w.hold.is_some() || w.drag.is_some() {
            return None;
        }
        use egui::{Key, Modifiers as M};
        let cmd_shift = M::COMMAND | M::SHIFT;
        let (redo, undo, delete) = ctx.input_mut(|i| {
            let redo = i.consume_key(cmd_shift, Key::Z) || i.consume_key(M::COMMAND, Key::Y);
            (redo, i.consume_key(M::COMMAND, Key::Z), i.consume_key(M::NONE, Key::Delete))
        });
        if redo {
            w.redo();
        } else if undo {
            w.undo();
        }
        if delete && let Some(i) = w.selected.take() {
            w.doc.objects.remove(i);
        }
        if w.tool == Tool::Crop && ctx.input_mut(|i| i.consume_key(M::NONE, Key::Enter)) {
            w.apply_crop();
        }
        if w.tool != Tool::Crop {
            let (screen, here) = ctx.input_mut(|i| (i.consume_key(M::SHIFT, Key::I), i.consume_key(M::NONE, Key::I)));
            if screen || here {
                w.ask_pick = Some(screen);
            }
        }
        let tool = ctx.input_mut(|i| tools::TOOLS.iter().find(|t| i.consume_key(M::NONE, t.3)).map(|t| t.0));
        if let Some(tool) = tool {
            w.choose(tool);
        }
        let (ccw, cw) = ctx.input_mut(|i| (i.consume_key(cmd_shift, Key::R), i.consume_key(M::COMMAND, Key::R)));
        if ccw {
            w.turn(crate::transform::Op::Ccw);
        } else if cw {
            w.turn(crate::transform::Op::Cw);
        }
        let (zoom_in, zoom_out, fit) = ctx.input(|i| (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals), i.key_pressed(Key::Minus), i.key_pressed(Key::Num0)));
        if zoom_in {
            w.zoom(1.25);
        } else if zoom_out {
            w.zoom(0.8);
        } else if fit {
            w.view = None;
        }
        let (save_as, save) = ctx.input_mut(|i| (i.consume_key(cmd_shift, Key::S), i.consume_key(M::COMMAND, Key::S)));
        match () {
            _ if (save_as || save && w.new) && !w.preparing => Some(Ask::SaveAs),
            _ if save && !w.preparing && w.changed() => Some(Ask::Save),
            _ => None,
        }
    }

    /// End of the frame: an operation over (no button held, no field typed in, no drag) becomes a
    /// step of the history; a text left empty goes; the styles changed are kept in the settings.
    fn settle(&mut self, ctx: &egui::Context, w: &mut Window) {
        // The fonts tried while the list is open are not steps of the history.
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input() || w.focus_text || w.hold.is_some() || w.drag.is_some() || w.font_preview.is_some();
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
            self.settings.edit_styles = w.styles.iter().map(|(k, s)| (*k, Style { width: s.width / long, letters: s.letters / long, ..*s })).collect();
            self.settings.save();
        }
    }
}
