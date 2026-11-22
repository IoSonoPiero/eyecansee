//! Save as…: the image decoded in full, encoded again with the chosen format and options
//! (`encode`), saved in the original's folder or another one (`folder`). The encoded bytes are
//! both what gets saved and, decoded back, the preview: what the window shows is what the file
//! will contain.

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui::{self, ColorImage};

use crate::i18n::I18n;
use crate::mipmap::Picture;
use crate::batch::with_ext;
use crate::encode::{Format, Options, Original};

mod folder;
mod format_ui;
mod panes;

pub(crate) use format_ui::{format_options, meta_choice};
use format_ui::{format_choice, meta_options};
use panes::convert_images;

/// The original decoded, its texture and file size.
type Loaded = Result<(Arc<Original>, Picture, u64), String>;
/// The encoded file and, decoded back, its texture.
type Encoded = Result<(Arc<Vec<u8>>, Option<Picture>), String>;

/// The open Save as… page.
pub struct Window {
    /// The original, relative to the source.
    pub path: String,
    original: Option<Loaded>,
    pub(crate) options: Options,
    /// Options of the encoding under way or shown; a change starts a new one when it ends.
    encoded_with: Option<Options>,
    pub generation: u64,
    busy: bool,
    result: Option<Encoded>,
    /// What the file shown leaves out of the original's metadata, too large for a JPEG (#217).
    left_out: crate::encoders::LeftOut,
    name: String,
    /// 100%: one image pixel per screen pixel, moved by dragging (`pan` = centre, in image pixels).
    actual: bool,
    pan: egui::Vec2,
    saving: bool,
    /// Height of the part under the images, measured at the last frame.
    bottom_h: f32,
    error: Option<String>,
    /// A new image from the editor: no file yet, so no size on disk to show or compare with.
    unsaved: bool,
    /// Opened from the editor: where its image is, and so where this file goes, whatever the grid
    /// shows (`edit_page::Place`).
    pub(crate) place: Option<crate::edit_page::Place>,
    /// Another folder chosen for the new file (issue #127); `None`: the original's.
    folder: Option<folder::Folder>,
    /// Browse… was pressed: the folder chooser opens after drawing.
    browse: bool,
    /// The folder chooser, open over the page.
    picker: Option<crate::folder_pick::Picker>,
    /// Opened from the viewer, or from the editor opened from it: once saved, the viewer shows
    /// the new file, or stays and says where it is (`viewer_after`).
    pub(crate) from_viewer: bool,
    /// The frame of a video (#16): its time in seconds. Like a new image, no file of its own to
    /// compare sizes with; the viewer stays on the video once it is saved.
    frame: Option<f64>,
    /// The number of the picture this page asked for (`ask`): another is not its own (#200).
    pub(crate) asked: u64,
}

/// A number for a picture asked for a page (the editor, Save as…), sent back with it in
/// `Msg::Original`: a picture asked for a page closed meanwhile goes to no other (#200; until
/// 0.77.0 the result of Save as… cancelled became the editor's photo).
pub(crate) fn ask() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Pictures dropped because no page open asked for them: the tests wait for one.
#[cfg(test)]
static DROPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
pub(crate) fn dropped() -> u64 {
    DROPPED.load(std::sync::atomic::Ordering::Relaxed)
}

impl Window {
    pub fn new(path: String) -> Self {
        Self::with(path, Options::default())
    }

    /// For a new image from the editor (File > New image…): PNG, as the editor names it
    /// (issue #104), not the JPEG proposed for a photo.
    pub fn for_new_image(path: String) -> Self {
        Window { unsaved: true, ..Self::with(path, Options { format: Format::Png, ..Options::default() }) }
    }

    fn with(path: String, options: Options) -> Self {
        let name = with_ext(crate::source::name(&path), options.format);
        Window {
            path,
            original: None,
            options,
            encoded_with: None,
            generation: 0,
            busy: false,
            result: None,
            left_out: Default::default(),
            name,
            actual: false,
            pan: egui::Vec2::ZERO,
            saving: false,
            bottom_h: 300.0,
            error: None,
            unsaved: false,
            place: None,
            folder: None,
            browse: false,
            picker: None,
            from_viewer: false,
            frame: None,
            asked: ask(),
        }
    }

    /// The picture read, for the tests.
    #[cfg(test)]
    pub(crate) fn read(&self) -> Option<&Arc<Original>> {
        self.original.as_ref().and_then(|o| o.as_ref().ok()).map(|(o, ..)| o)
    }
}

impl crate::App {
    pub(crate) fn open_convert(&mut self, path: Option<String>) {
        let Some(path) = path.or_else(|| self.target()).filter(|p| !p.ends_with('/')) else { return };
        // A video in the viewer: its frame, paused (already turned as shown).
        let mut w = Window::new(path.clone());
        let frame = if crate::decode::is_played(&path) {
            let Some(at) = self.convert_frame(&path, w.asked) else { return };
            Some(at)
        } else {
            None
        };
        // A 3D model in the viewer: its picture as shown.
        if frame.is_none() && !self.convert_model(&path, w.asked) && !self.convert_sheet(&path, w.asked) {
            let Some(loader) = &self.loader else { return };
            loader.load_original(w.asked, path, false, false);
        }
        if let Some(at) = frame {
            w.frame = Some(at);
            w.name = crate::video_frame::frame_name(crate::source::name(&w.path), at, w.options.format);
        } else if self.viewing.is_some() {
            // From the viewer: saved as it is shown there.
            w.options.turn = self.turn;
            w.from_viewer = true;
        }
        self.convert = Some(w);
    }

    pub(crate) fn convert_original(&mut self, ctx: &egui::Context, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        let Some(w) = &mut self.convert else { return };
        if let Err(e) = &r {
            log!("{}", failed_line("save as: read", &w.path, "", e));
        }
        // A JPEG: its own quality and subsampling proposed, 90 and 4:2:0 when they cannot be read.
        if let Some((q, chroma)) = r.as_ref().ok().and_then(|(o, ..)| o.jpeg) {
            (w.options.jpeg_quality, w.options.chroma) = (q, chroma);
        }
        // The format of the last Save as…, once the image's size is known (nothing can be chosen
        // before it arrives); a new image keeps the PNG it is named with (issue #104).
        let last = self.save_as_format.filter(|_| !w.unsaved);
        if let (None, Ok((o, ..))) = (&w.original, &r) {
            w.options.format = proposed(last, w.options.format, o.image.width().max(o.image.height()));
            w.name = with_ext(&w.name, w.options.format);
        }
        w.original = Some(r.map(|(original, preview, size)| {
            w.pan = egui::vec2(preview.size[0] as f32, preview.size[1] as f32) / 2.0;
            // Its halves here, once: shown fitted, smaller than its pixels (`mipmap`).
            let halves = crate::mipmap::picture_halves(&preview);
            (original, Picture::new(ctx, "convert-original", preview, halves), size)
        }));
    }

    /// A picture read for a page (`Msg::Original`): to the one that asked for it (`asked`), Save
    /// as… or the editor; one asked for a page closed since goes to none (#200).
    pub(crate) fn on_original(&mut self, ctx: &egui::Context, asked: u64, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        if self.convert.as_ref().is_some_and(|w| w.asked == asked) {
            self.convert_original(ctx, r);
        } else if self.editor.as_ref().is_some_and(|w| w.asked == asked) {
            self.editor_original(ctx, r);
        } else {
            log!("picture {asked} dropped: its page is closed");
            #[cfg(test)]
            DROPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub(crate) fn convert_encoded(&mut self, ctx: &egui::Context, generation: u64, r: Result<(Arc<Vec<u8>>, Option<ColorImage>), String>) {
        let Some(w) = &mut self.convert else { return };
        if generation != w.generation {
            return;
        }
        w.busy = false;
        if let Err(e) = &r {
            log!("{}", failed_line(&format!("save as: {} encoding", w.options.format.ext()), &w.path, "", e));
        }
        w.left_out = match (&w.original, w.encoded_with) {
            (Some(Ok((original, ..))), Some(o)) => left_out(original, &o),
            _ => Default::default(),
        };
        w.result = Some(r.map(|(bytes, preview)| (bytes, preview.map(|p| {
            let halves = crate::mipmap::picture_halves(&p);
            Picture::new(ctx, "convert-result", p, halves)
        }))));
    }

    pub(crate) fn convert_saved(&mut self, asked: &str, r: Result<String, crate::source::Error>) {
        match r {
            Ok(path) => {
                log!("saved as {path:?}");
                // From the editor, on a place the grid no longer shows, or into a folder of another
                // device: nothing there to select or list again.
                let here = self.convert.as_ref().is_none_or(|w| w.dest_base(&self.base) == self.base);
                let w = self.convert.take();
                let from_viewer = w.as_ref().is_some_and(|w| w.from_viewer);
                let shown = w.as_ref().map_or_else(|| path.clone(), |w| crate::navigation::shown(w.dest_base(&self.base), &path));
                self.save_as_format = w.map(|w| w.options.format).or(self.save_as_format);
                // Opened from the editor: the page under it closes now that the file is written,
                // before the viewer under it changes and says where the file went.
                if self.editor.as_mut().is_some_and(|w| w.save_as_ended(true)) {
                    self.editor = None;
                    log!("edit: page closed after Save as…, viewing {:?}", self.viewing);
                    // The file just saved is selected, or shown by the viewer, as after Save copy:
                    // only in the open folder, the only one listed for it.
                    // Saved elsewhere, a copy saved before it is not selected either (issue #98):
                    // the viewer stays on the image edited.
                    self.select_after = (here && crate::source::parent(&path) == self.dir).then(|| path.clone());
                }
                if let Some(after) = viewer_after(from_viewer, Some(&path), here, &self.dir) {
                    self.viewer_saved_as(after, &path, &shown);
                }
                if here {
                    self.relist(crate::source::parent(&path));
                }
            }
            Err((id, detail)) => {
                log!("{}", failed_line("save as: write", asked, id, &detail));
                if let Some(w) = &mut self.convert {
                    w.saving = false;
                    w.error = Some(self.i18n.error(id, &detail));
                }
            }
        }
    }

    /// Save as… from the viewer went well: the original is as it was on disk, so its flips and
    /// turns not saved are dropped (no "Changed, not saved", no question on closing). The new
    /// file, which has them in its pixels, is shown once listed (`select_saved` if it already
    /// is, `swap_listing` otherwise); saved elsewhere, a message says where.
    fn viewer_saved_as(&mut self, after: ViewerAfter, path: &str, shown: &str) {
        self.turn = Default::default();
        log!("save as: viewer {after:?}");
        match after {
            ViewerAfter::Show => {
                self.select_after = Some(path.to_string());
                self.select_saved();
            }
            ViewerAfter::Stay => {
                let shown = crate::address::without_password(shown);
                self.tell("convert-saved-elsewhere-title", self.i18n.t_args("convert-saved-elsewhere", &[("path", &crate::save_dialogs::md(&shown))]));
            }
        }
    }

    /// Drawn in place of the whole interface, as a page of its own: a window could not cover the
    /// menu, tool and status bars, and the two images need all the room there is.
    pub(crate) fn convert_page(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let Some(mut w) = self.convert.take() else { return };
        // A picture that cannot be read: no page, the message in the bar, as in the viewer (a
        // whole page for one line, Piero 20/11). Nothing is lost: nothing was done on it.
        if let Some(Err(e)) = &w.original
            && crate::view_memory::need_of(e).is_none()
        {
            self.error = Some(self.i18n.cannot_load(crate::source::name(&w.path), e));
            return;
        }
        // A file of the grid or the viewer, not the editor's result: animated or with pages?
        if w.place.is_none() {
            self.probe_animated(ctx, &w.path, true);
        }
        self.start_encoding(&mut w, ctx);
        let dir = w.dest_dir();
        let final_name = crate::source::free_name(&self.convert_taken(&w, &dir), &crate::batch::typed_name(&w.name, w.options.format));
        let mut open = page_header(ui, &self.i18n.t("convert-title"));
        let shown = crate::navigation::shown(w.dest_base(&self.base), &dir);
        let press = self.convert_body(ui, &mut w, &final_name, &shown);
        // Cancel does what Esc does: from the editor, back to the drawing (issue #106).
        if press == Some(Press::Cancel) {
            open = false;
        }
        if press == Some(Press::Save) && let Some(Ok((bytes, _))) = &w.result {
            let (path, bytes) = (format!("{dir}{final_name}"), bytes.clone());
            let sent = match (w.dest_store(), &self.loader) {
                (Some(store), _) => self.save_in_place(ctx, store.clone(), path, bytes),
                (None, Some(loader)) => {
                    loader.save(path, bytes);
                    true
                }
                (None, None) => false,
            };
            if sent {
                w.saving = true;
                w.error = None;
            }
        }
        self.convert_pick(ctx, &mut w);
        open &= self.big_picture_page(ctx, &mut w.original, (&w.path, w.asked), false);
        // Taken out while drawn (it needs the rest of the app), put back unless closed.
        if open {
            self.convert = Some(w);
        }
    }

    /// The names taken in `dir`, where the file goes: the grid's for its own place (`save_in_place`
    /// checks them again elsewhere), those read in a folder chosen (`folder`).
    fn convert_taken(&self, w: &Window, dir: &str) -> HashSet<String> {
        if let Some(f) = &w.folder {
            return f.taken();
        }
        let here = w.dest_base(&self.base) == self.base;
        // What the grid does not show is taken too: a hidden "e (2).jpg" was overwritten.
        match self.listings.get(dir).filter(|_| here) {
            Some(Ok(l)) => l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).map(|p| crate::source::name(p).trim_end_matches('/').to_string()).chain(l.unseen.iter().cloned()).collect(),
            _ => HashSet::new(),
        }
    }

    /// Save as… from the editor, or into a folder chosen: written there (`place`, `folder`),
    /// under a name free there: the grid may show another place, whose names say nothing of that
    /// folder. `true`: sent.
    fn save_in_place(&self, ctx: &egui::Context, store: crate::source::Store, path: String, data: Arc<Vec<u8>>) -> bool {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let r = write_free(&store, &path, data.to_vec()).await;
            let _ = tx.send(crate::loader::Msg::Saved(path, r));
            ctx.request_repaint();
        });
        true
    }

    /// One encoding at a time (AVIF takes seconds): a change made meanwhile starts when it ends.
    fn start_encoding(&self, w: &mut Window, ctx: &egui::Context) {
        if !w.busy
            && w.encoded_with != Some(w.options)
            && let Some(Ok((original, _, _))) = &w.original
        {
            w.generation += 1;
            w.busy = true;
            // Another format: the old preview would pass for the new one while it is being made.
            if w.encoded_with.map(|o| o.format) != Some(w.options.format) {
                w.result = None;
            }
            w.encoded_with = Some(w.options);
            let (original, options, generation, tx, ctx2) = (original.clone(), w.options, w.generation, self.tx.clone(), ctx.clone());
            crate::logging::spawn_named("convert-preview", move || {
                let r = encoded(&original, &options).map(|bytes| {
                    let preview = crate::decode::decode(&bytes, &format!("x.{}", options.format.ext())).ok();
                    (Arc::new(bytes), preview)
                });
                if tx.send(crate::loader::Msg::Encoded(generation, r)).is_ok() {
                    ctx2.request_repaint();
                }
            });
        }
    }

    /// Everything under the title: the two images, then what each one is, with the options and
    /// the name on the side of the new one, and the button pressed.
    fn convert_body(&self, ui: &mut egui::Ui, w: &mut Window, final_name: &str, folder: &str) -> Option<Press> {
        let i18n = &self.i18n;
        let t = |id| i18n.t(id);
        let mut save = None;
        let original = match &w.original {
            None => {
                ui.centered_and_justified(|ui| ui.spinner());
                return save;
            }
            // Too large for the memory: the question is over the page (`convert_page`).
            Some(Err(e)) if crate::view_memory::need_of(e).is_some() => return save,
            Some(Err(e)) => {
                crate::error_bar::inline(ui, &i18n.cannot_load(crate::source::name(&w.path), e));
                return save;
            }
            Some(Ok(o)) => o.clone(),
        };
        // A new image has no file: no size on disk, no percentage of it (issue #99).
        let (image, tex, size) = (&original.0.image, &original.1, (!w.unsaved && w.frame.is_none()).then_some(original.2));
        let half = (ui.available_width() - GAP) / 2.0;
        // The images get what the options below leave (their height measured at the last frame).
        let pane = egui::vec2(half, (ui.available_height() - 22.0 - 16.0 - w.bottom_h).max(120.0));
        convert_images(ui, w, i18n, tex, half, pane);
        ui.add_space(4.0);
        let size_text = |(w, h): (u32, u32)| i18n.t_args("convert-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]);
        let pixels = size_text((image.width(), image.height()));
        // The new file's own: swapped by a quarter turn made in the viewer, reduced for an ICO.
        let saved = size_text(saved_size((image.width(), image.height()), &w.options));
        let bottom = ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.vertical(|ui| {
                ui.set_width(half);
                let ext = crate::source::name(&w.path).rsplit_once('.').map_or(String::new(), |(_, e)| e.to_uppercase());
                // The file as it is on disk, and what the viewer did to it (the image above is shown so).
                match (panes::turn_note(w.options.turn), w.frame) {
                    (Some(turn), _) => ui.label(format!("{ext} · {pixels} · {}", i18n.t_args("convert-turned", &[("turn", turn)]))),
                    (None, Some(at)) => ui.label(format!("{ext} · {pixels} · {}", i18n.t_args("convert-frame-at", &[("time", &frame_clock(at))]))),
                    (None, None) => ui.label(format!("{ext} · {pixels}")),
                };
                if let Some(size) = size {
                    ui.label(i18n.t_args("convert-file-size", &[("size", &self.human_size(size))]));
                }
                ui.add_space(6.0);
                ui.checkbox(&mut w.actual, t("convert-actual"));
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.set_width(half);
                self.result_info(ui, w, &saved, size);
                ui.add_space(6.0);
                format_choice(ui, w, i18n);
                let image = &original.0.image;
                format_options(ui, &mut w.options, image.color().has_alpha(), image.width().max(image.height()) > crate::encode::ICO_SIDE, i18n);
                meta_options(ui, &mut w.options, &original.0, i18n);
                // The other frames or pages are not in the new file (as Convert… says, leaving them out).
                if w.place.is_none() && self.animated.contains(&w.path) {
                    ui.label(crate::theme::alert(t("convert-first-only")));
                }
                // A 3D model read otherwise than its file says (#179): the new file has it so.
                for line in crate::model::saved_lines(i18n, crate::source::name(&w.path), &original.0.notes) {
                    ui.label(crate::theme::alert(line));
                }
                // Metadata too large for a JPEG (#217), for the file made with the options shown.
                if w.left_out.any() && !w.busy && w.encoded_with == Some(w.options) {
                    ui.label(crate::theme::alert(i18n.t_args("convert-meta-too-large", &[("parts", &w.left_out.parts(&i18n.t("list-and")))])));
                }
                // The folder and the name belong to the new image, so they sit on its side.
                ui.add_space(6.0);
                folder::folder_row(ui, w, i18n, folder);
                save = name_row(ui, w, i18n, half, final_name);
            });
        })
        .response;
        // Measured, for the next frame: the options change with the format.
        let used = bottom.rect.height() + 8.0;
        if (used - w.bottom_h).abs() > 1.0 {
            w.bottom_h = used;
            ui.ctx().request_repaint();
        }
        save
    }

    /// Format and size of the new file, against the original's; its error if it failed.
    fn result_info(&self, ui: &mut egui::Ui, w: &Window, pixels: &str, size: Option<u64>) {
        let i18n = &self.i18n;
        match &w.result {
            // Options changed, new file not made yet: the old size would pass for the new one
            // (the user saw 100 KB, saved, and got 150).
            Some(Ok(_)) if w.busy || w.encoded_with != Some(w.options) => {
                ui.label(format!("{} · {pixels}", w.options.format.label()));
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(i18n.t("convert-calculating"));
                });
            }
            Some(Ok((bytes, _))) => {
                ui.label(format!("{} · {pixels}", w.options.format.label()));
                let on_disk = i18n.t_args("convert-file-size", &[("size", &self.human_size(bytes.len() as u64))]);
                match ratio(bytes.len() as u64, size) {
                    Some(percent) => ui.label(format!("{on_disk} ({})", i18n.t_args("convert-ratio", &[("percent", &percent.to_string())]))),
                    None => ui.label(on_disk),
                };
            }
            Some(Err(e)) => {
                crate::error_bar::inline(ui, &crate::error_bar::with_detail(&i18n.t_args("convert-error", &[("detail", e)]), e));
            }
            None => {
                ui.label(" ");
                ui.label(" ");
            }
        }
    }
}

/// What the viewer does after Save as… opened from it.
#[derive(Debug, PartialEq)]
enum ViewerAfter {
    /// The new file is in the open folder: the viewer goes to it.
    Show,
    /// It is in another folder or place: the viewer stays on the original, and says where.
    Stay,
}

/// The viewer after Save as…, opened from it or from the editor opened from it (`from_viewer`):
/// `None` when opened elsewhere (the grid, the editor opened from the grid) or nothing was saved
/// (`saved` none: cancelled or failed), which leaves it as it was.
fn viewer_after(from_viewer: bool, saved: Option<&str>, here: bool, open_dir: &str) -> Option<ViewerAfter> {
    let path = saved.filter(|_| from_viewer)?;
    Some(if here && crate::source::parent(path) == open_dir { ViewerAfter::Show } else { ViewerAfter::Stay })
}

/// The time of a frame with its thousandths: "1:02.345".
fn frame_clock(at: f64) -> String {
    let ms = (at.max(0.0) * 1000.0).round() as u64;
    format!("{}.{:03}", crate::mpv::clock((ms / 1000) as f64), ms % 1000)
}

/// The new file's size in percent of the original's; none without an original file (a new image,
/// issue #99).
fn ratio(new: u64, original: Option<u64>) -> Option<u64> {
    original.filter(|&s| s > 0).map(|s| new * 100 / s)
}

/// The format Save as… opens on: that of the last one, unless this image is larger than it can
/// hold; then `default`, as before (JPEG, PNG for a new image from the editor).
fn proposed(last: Option<Format>, default: Format, side: u32) -> Format {
    last.filter(|&f| max_side(f).is_none_or(|max| side <= max)).unwrap_or(default)
}

/// The longest side the format's encoder writes: libwebp 16383, libjpeg-turbo 65500, GIF and TGA
/// 65535 (sizes in 16 bits); the others have no limit an image reaches (ICO is reduced).
fn max_side(f: Format) -> Option<u32> {
    match f {
        Format::Webp => Some(16_383),
        Format::Jpeg => Some(65_500),
        Format::Gif | Format::Tga => Some(65_535),
        _ => None,
    }
}

/// Space between the two halves of the page.
const GAP: f32 = 16.0;

/// Title of a page covering the window; `false` once closed (the window's ✕ or Esc).
pub(crate) fn page_header(ui: &mut egui::Ui, title: &str) -> bool {
    let mut open = !crate::theme::page_title(ui, title);
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    ui.separator();
    open
}

/// The file Save writes: an Ultra HDR photo saved as JPEG with its metadata stays Ultra HDR, as
/// in Convert and resize (#66); otherwise the picture alone.
fn encoded(original: &Original, o: &Options) -> Result<Vec<u8>, String> {
    match &original.hdr {
        Some(hdr) if keeps_hdr(o) => hdr.write(&original.image, o),
        _ => original.encode(o),
    }
}

/// Width and height of the file Save writes, from the original's: a quarter turn made in the
/// viewer swaps them, an ICO is reduced into 256 × 256 as `encode` does (with
/// `image::DynamicImage::resize`). Nothing else in Save as… changes them: the editor hands over
/// its result already cropped and resized.
fn saved_size((w, h): (u32, u32), o: &Options) -> (u32, u32) {
    let (w, h) = if o.turn.sideways() { (h, w) } else { (w, h) };
    let side = f64::from(crate::encode::ICO_SIDE);
    if o.format != Format::Ico || w.max(h) <= crate::encode::ICO_SIDE {
        return (w, h);
    }
    let ratio = f64::min(side / f64::from(w), side / f64::from(h));
    let fit = |s: u32| ((f64::from(s) * ratio).round() as u32).max(1);
    (fit(w), fit(h))
}

/// What the file Save writes leaves out of the original's metadata, too large for a JPEG (#217);
/// nothing for an Ultra HDR photo kept so, which gets back the original's own segments.
fn left_out(original: &Original, o: &Options) -> crate::encoders::LeftOut {
    match &original.hdr {
        Some(_) if keeps_hdr(o) => Default::default(),
        _ => original.left_out(o),
    }
}

/// Only a JPEG with its metadata holds the gain map and the XMP that describes it.
fn keeps_hdr(o: &Options) -> bool {
    o.format == Format::Jpeg && o.meta != crate::meta_strip::Keep::Nothing
}

/// A button of the page clicked.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Press {
    Save,
    /// Closes the page without saving, as Esc and the window's ✕ (issue #106).
    Cancel,
}

/// Name of the new file, Save and Cancel (the one clicked); the name it will really get if taken.
fn name_row(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n, half: f32, final_name: &str) -> Option<Press> {
    let t = |id| i18n.t(id);
    let mut press = None;
    ui.horizontal(|ui| {
        ui.label(t("convert-name"));
        ui.add(egui::TextEdit::singleline(&mut w.name).desired_width((half - 200.0).clamp(160.0, 420.0)));
        let ready = matches!(w.result, Some(Ok(_))) && !w.busy && !w.saving && !w.name.trim().is_empty() && !w.name.contains(['/', '\\']);
        if ui.add_enabled(ready, egui::Button::new(t("convert-save"))).clicked() {
            press = Some(Press::Save);
        }
        // Not while the file is being written: it would be written all the same.
        if ui.add_enabled(!w.saving, egui::Button::new(t("convert-cancel"))).clicked() {
            press = Some(Press::Cancel);
        }
        if w.saving {
            ui.spinner();
        }
    });
    if final_name != crate::batch::typed_name(&w.name, w.options.format) && !w.name.trim().is_empty() {
        ui.weak(i18n.t_args("convert-exists", &[("name", final_name)]));
    }
    if let Some(e) = &w.error {
        crate::error_bar::inline(ui, e.as_str());
    }
    press
}


/// The log line of a step of Save as… that failed: what, on which file, and why (the text id,
/// when there is one, and the detail).
fn failed_line(what: &str, path: &str, id: &str, detail: &str) -> String {
    match id {
        "" => format!("{what} {path:?} failed: {detail}"),
        _ => format!("{what} {path:?} failed: {id} {detail}"),
    }
}

/// Writes `data` as `path` in `store`, or under the first free name beside it; the path written.
async fn write_free(store: &crate::source::Store, path: &str, data: Vec<u8>) -> Result<String, crate::source::Error> {
    let dir = crate::source::parent(path);
    let taken = store.names(&dir).await?;
    let path = format!("{dir}{}", crate::source::free_name(&taken, crate::source::name(path)));
    crate::replace::write_whole(store, &path, data).await.map(|()| path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Turn;

    /// Save as… from the editor writes in the editor's place under a name free there, checked in
    /// that folder (the grid may list another place): a file already there is never replaced.
    #[test]
    fn saved_as_under_a_free_name_of_its_place() {
        let dir = std::env::temp_dir().join(format!("eyecansee-convert-free-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("p")).unwrap();
        std::fs::write(dir.join("p/a.png"), b"there before").unwrap();
        let store = crate::source::Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert_eq!(rt.block_on(write_free(&store, "p/b.png", b"b".to_vec())).unwrap(), "p/b.png");
        assert_eq!(rt.block_on(write_free(&store, "p/a.png", b"new".to_vec())).unwrap(), "p/a (2).png");
        assert_eq!(std::fs::read(dir.join("p/a.png")).unwrap(), b"there before");
        assert_eq!(std::fs::read(dir.join("p/a (2).png")).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A Save as… that cannot write leaves a line in the log naming the file and the reason.
    #[test]
    fn failed_save_said_in_the_log() {
        let dir = std::env::temp_dir().join(format!("eyecansee-convert-failed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A file where the folder should be: nothing can be written under it.
        std::fs::write(dir.join("p"), b"a file").unwrap();
        let store = crate::source::Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (id, detail) = rt.block_on(write_free(&store, "p/b.png", b"b".to_vec())).unwrap_err();
        let line = failed_line("save as: write", "p/b.png", id, &detail);
        assert!(line.starts_with("save as: write \"p/b.png\" failed: "), "{line}");
        assert!(line.contains(id) && line.ends_with(&detail) && !detail.is_empty(), "{line}");
        assert_eq!(failed_line("save as: avif encoding", "a.jpg", "", "too big"), "save as: avif encoding \"a.jpg\" failed: too big");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Save in another format keeps an Ultra HDR photo Ultra HDR in JPEG with its metadata (#66);
    /// in another format, or without the metadata, the picture alone.
    #[test]
    fn ultra_hdr_only_in_jpeg_with_meta() {
        let file = crate::ultrahdr::tests::sample(64, 32);
        let mut original = Original::with_meta(crate::decode::decode_full(&file, "a.jpg").unwrap(), &file).alone();
        original.hdr = crate::ultrahdr::Source::of(Arc::new(file), Vec::new(), Vec::new()).map(Arc::new);
        let jpeg = Options { format: Format::Jpeg, ..Options::default() };
        assert!(crate::ultrahdr::is(&encoded(&original, &jpeg).unwrap()));
        let bare = encoded(&original, &Options { meta: crate::meta_strip::Keep::Nothing, ..jpeg }).unwrap();
        assert!(!crate::ultrahdr::is(&bare) && bare.starts_with(&[0xFF, 0xD8]));
        let png = encoded(&original, &Options { format: Format::Png, ..Options::default() }).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
    }

    /// The real Ultra HDR photos of the test files (`notes/test-files/online/telefono`, or
    /// `EYECANSEE_PHONE_SAMPLES`) saved as JPEG turned, as from the viewer: each stays Ultra HDR,
    /// photo and gain map turned, nothing after the gain map (#66).
    #[test]
    #[ignore]
    fn ultra_hdr_saved_live() {
        use image::GenericImageView;
        let dir = std::env::var_os("EYECANSEE_PHONE_SAMPLES").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("notes/test-files/online/telefono"), std::path::PathBuf::from);
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            let Some(hdr) = crate::ultrahdr::Source::of(Arc::new(bytes.clone()), Vec::new(), Vec::new()) else { continue };
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let mut original = Original::with_meta_of(crate::decode::decode_full(&bytes, &name).unwrap(), &bytes, &name).alone();
            original.hdr = Some(Arc::new(hdr));
            let (w, h) = original.image.dimensions();
            let o = Options { format: Format::Jpeg, turn: Turn::default().then(crate::transform::Op::Cw), ..Options::default() };
            let d = encoded(&original, &o).unwrap();
            let (photo, gain) = crate::ultrahdr::split(&d).unwrap_or_else(|| panic!("{name}: no longer Ultra HDR"));
            assert_eq!(photo.len() + gain.len(), d.len(), "{name}: nothing after the gain map");
            let p = image::load_from_memory(photo).unwrap().dimensions();
            let g = image::load_from_memory(gain).unwrap().dimensions();
            assert_eq!(p, (h, w), "{name}");
            assert_eq!(g.0 > g.1, p.0 > p.1, "{name}: gain map {g:?} turned with the photo {p:?}");
            println!("{name}: {w}x{h} -> {p:?}, gain map {g:?}, {} bytes", d.len());
            seen += 1;
        }
        assert!(seen > 0, "no Ultra HDR photo in {}", dir.display());
    }

    /// A photo turned by a quarter in the viewer: Save as… says the size of the file it writes (a
    /// 1920 × 613 photo said 1920 × 613 under the new image), and the file has its pixels turned
    /// once, clockwise, with no EXIF orientation left to turn it again.
    #[test]
    fn turned_photo_saved_with_its_size() {
        use image::{GenericImageView, ImageDecoder};
        let cw = Turn::default().then(crate::transform::Op::Cw);
        let half = Turn::default().then(crate::transform::Op::Half);
        let jpeg = Options::default();
        assert_eq!(saved_size((1920, 613), &jpeg), (1920, 613));
        assert_eq!(saved_size((1920, 613), &Options { turn: cw, ..jpeg }), (613, 1920));
        assert_eq!(saved_size((1920, 613), &Options { turn: half, ..jpeg }), (1920, 613));
        // An ICO is reduced into 256 × 256, after the turn.
        let ico = Options { format: Format::Ico, ..jpeg };
        assert_eq!(saved_size((1920, 613), &ico), (256, 82));
        assert_eq!(saved_size((1920, 613), &Options { turn: cw, ..ico }), (82, 256));
        assert_eq!(saved_size((200, 100), &Options { turn: cw, ..ico }), (100, 200));
        // A JPEG whose EXIF says "turn by a quarter" (done when decoded), red at the top left, in
        // blocks of 16 pixels so that the JPEG keeps it red.
        let red = |p: image::Rgba<u8>| p[0] > 200 && p[1] < 80 && p[2] < 80;
        let img = image::RgbImage::from_fn(64, 32, |x, y| image::Rgb(if x < 16 && y < 16 { [255, 0, 0] } else { [255, 255, 255] }));
        let mut file = Vec::new();
        image::DynamicImage::ImageRgb8(img.clone()).write_to(&mut std::io::Cursor::new(&mut file), image::ImageFormat::Jpeg).unwrap();
        let exif = b"Exif\0\0II*\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00";
        let app1 = [&[0xFF, 0xE1][..], &(exif.len() as u16 + 2).to_be_bytes(), exif].concat();
        file.splice(2..2, app1);
        let original = Original::with_meta(image::DynamicImage::ImageRgb8(img), &file);
        for format in [Format::Jpeg, Format::Png, Format::Ico] {
            for turn in [Turn::default(), cw, half] {
                let o = Options { format, turn, ..Options::default() };
                let bytes = encoded(&original, &o).unwrap();
                let mut d = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().unwrap().into_decoder().unwrap();
                assert_eq!(d.orientation().unwrap(), image::metadata::Orientation::NoTransforms, "{format:?} {turn:?}");
                let back = image::DynamicImage::from_decoder(d).unwrap();
                assert_eq!(back.dimensions(), saved_size((64, 32), &o), "{format:?} {turn:?}");
                if turn == cw && format != Format::Ico {
                    // Turned clockwise: the top left corner is now at the top right.
                    assert!(red(back.get_pixel(24, 8)) && !red(back.get_pixel(8, 8)) && !red(back.get_pixel(8, 56)), "{format:?}");
                }
            }
        }
    }

    /// Issue #104: a new image is proposed as the PNG the editor names; a photo as a JPEG.
    #[test]
    fn new_image_proposed_as_png() {
        let w = Window::for_new_image("p/New image.png".into());
        assert_eq!((w.options.format, w.name.as_str()), (Format::Png, "New image.png"));
        let w = Window::new("p/beach.webp".into());
        assert_eq!((w.options.format, w.name.as_str()), (Format::Jpeg, "beach.jpg"));
    }

    /// Save as… opens on the format of the last one; an image larger than that format holds, on
    /// the format proposed before (JPEG, or PNG for a new image).
    #[test]
    fn save_as_proposes_the_last_format() {
        assert_eq!(proposed(None, Format::Jpeg, 4000), Format::Jpeg);
        assert_eq!(proposed(None, Format::Png, 800), Format::Png);
        assert_eq!(proposed(Some(Format::Avif), Format::Jpeg, 4000), Format::Avif);
        assert_eq!(proposed(Some(Format::Webp), Format::Png, 800), Format::Webp);
        assert_eq!(proposed(Some(Format::Webp), Format::Jpeg, 16_383), Format::Webp);
        assert_eq!(proposed(Some(Format::Webp), Format::Jpeg, 16_384), Format::Jpeg);
        assert_eq!(proposed(Some(Format::Gif), Format::Jpeg, 70_000), Format::Jpeg);
        assert_eq!(proposed(Some(Format::Png), Format::Jpeg, 70_000), Format::Png);
    }

    /// The limits of `max_side` are those of the encoders: one pixel more fails, the limit itself
    /// is written.
    #[test]
    fn max_side_is_the_encoders() {
        for f in Format::ALL {
            let Some(max) = max_side(f) else { continue };
            let o = Options { format: f, ..Options::default() };
            let at = |w| Original::with_meta(image::DynamicImage::new_rgb8(w, 1), &[]).alone().encode(&o);
            assert!(at(max).is_ok(), "{f:?} at {max}");
            assert!(at(max + 1).is_err(), "{f:?} at {}", max + 1);
        }
    }

    /// Save as… from the viewer, or from the editor opened from it: beside the original the viewer
    /// goes to the new file; in another folder or place it stays, and says where; cancelled or
    /// failed, or opened from the grid or the editor opened from the grid, nothing changes.
    #[test]
    fn viewer_after_save_as() {
        assert_eq!(viewer_after(true, Some("p/a.jpg"), true, "p/"), Some(ViewerAfter::Show));
        assert_eq!(viewer_after(true, Some("p/q/a.jpg"), true, "p/"), Some(ViewerAfter::Stay));
        assert_eq!(viewer_after(true, Some("a.jpg"), true, "p/"), Some(ViewerAfter::Stay));
        assert_eq!(viewer_after(true, Some("p/a.jpg"), false, "p/"), Some(ViewerAfter::Stay));
        assert_eq!(viewer_after(true, None, true, "p/"), None);
        assert_eq!(viewer_after(false, Some("p/a.jpg"), true, "p/"), None);
    }

    /// Issue #99: a new image has no file, so no size on disk and no percentage of it.
    #[test]
    fn no_ratio_without_a_file() {
        assert!(Window::for_new_image("p/New image.png".into()).unsaved);
        assert!(!Window::new("p/beach.jpg".into()).unsaved);
        assert_eq!(ratio(42_159, None), None);
        assert_eq!(ratio(42_159, Some(0)), None);
        assert_eq!(ratio(50, Some(200)), Some(25));
    }
}
