//! Resize (#114): its window (the size of the result in pixels or percent, the proportions, the
//! filter) and the photo shown following the document's size. The sizes and the document without
//! the interface: `edit::resize`.

use std::sync::Arc;

use eframe::egui;

use super::Window;
use crate::App;
use crate::batch::Filter;
use crate::edit::resize::{self, Form, MAX_PERCENT, MAX_SIDE};
use crate::i18n::I18n;
use crate::confirm::FollowList;

impl Window {
    /// Resize (its button or Ctrl+Alt+I): the window opens on the size of the result now.
    pub(super) fn ask_resize(&mut self) {
        self.resizing = Some(Form::new(self.doc.crop.pixels(), &self.doc.crop, self.size(), self.doc.angle));
    }

    /// The result made `out` pixels with `filter`: one step of the history, as any other.
    fn resize_to(&mut self, out: (u32, u32), filter: Filter) {
        let Some(full) = &self.full else { return };
        let original = (full.image.width(), full.image.height());
        log!("edit: resize {:?} to {out:?}, {filter:?}", self.doc.crop.pixels());
        self.doc = resize::resized(&self.doc, self.upright(), original, out, filter);
    }

    /// The photo shown follows the document's size, after Resize, Undo and Redo: resampled from
    /// the original each time (never twice), in a thread aside (audit 5.14: the window stood
    /// still meanwhile); until it comes the photo before is drawn at the new size.
    pub(super) fn follow_size(&mut self, ctx: &egui::Context) {
        let scale = self.doc.scale;
        if scale == self.scaled || self.aside.resampling == Some(scale) {
            return;
        }
        let (Some(full), Some(Ok(_))) = (self.full.clone(), &self.original) else { return };
        self.aside.resampling = Some(scale);
        self.aside(ctx, "edit-resample", move || {
            let start = std::time::Instant::now();
            let photo = match scale {
                Some(s) => Arc::new(resize::resample(&full, s)),
                None => full,
            };
            let preview = crate::decode::preview(&photo.image);
            let halves = crate::mipmap::picture_halves(&preview);
            log!("edit: photo at {scale:?} in {:?}", start.elapsed());
            super::work::Done::Photo(scale, photo, preview, halves)
        });
    }

    /// The photo the document is drawn on, for what is made from it (Save, Copy, Save as…): the
    /// one shown, or while that is being resampled the original, which `resize::at_size` takes
    /// to the document's size (the one shown would give the size before: a Save right after Undo
    /// of a Resize wrote the file at the size undone).
    pub(super) fn photo(&self) -> Option<Arc<crate::encode::Original>> {
        let Some(Ok((shown, ..))) = &self.original else { return None };
        Some(if self.doc.scale == self.scaled { shown.clone() } else { self.full.clone().unwrap_or_else(|| shown.clone()) })
    }

    /// The photo at `scale` resampled (`follow_size`): shown, unless the document went to another
    /// size meanwhile. The next objects' widths and the view follow it.
    pub(super) fn photo_arrived(&mut self, ctx: &egui::Context, scale: Option<resize::Scale>, photo: Arc<crate::encode::Original>, (preview, halves): (egui::ColorImage, Vec<egui::ColorImage>)) {
        if self.aside.resampling == Some(scale) {
            self.aside.resampling = None;
        }
        let Some(Ok((before, _, size))) = &self.original else { return };
        if scale != self.doc.scale || scale == self.scaled {
            return;
        }
        let long = |o: &crate::encode::Original| o.image.width().max(o.image.height()) as f32;
        let (k, size) = (long(&photo) / long(before), *size);
        let tex = crate::mipmap::Picture::new(ctx, "edit-original", preview, halves);
        self.original = Some(Ok((photo, tex, size)));
        self.scaled = scale;
        // In whole pixels, as the properties bar has them.
        for (_, s) in &mut self.styles {
            s.width = (s.width * k).round().max(1.0);
            s.letters = (s.letters * k).round();
        }
        self.view = None;
        self.marquee = None;
    }
}

impl App {
    /// The Resize window over the page, while open: Resize changes the document, Cancel and Esc
    /// close it. Enter and Esc outside its fields come as `answer` (`confirm::asking`).
    pub(super) fn resize_window(&mut self, ctx: &egui::Context, w: &mut Window) {
        let Some(mut f) = w.resizing.take() else { return };
        let id = "edit-resize";
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, id);
        let mut answer = None;
        egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| answer = resize_form(ui, &mut f, &self.i18n, keys));
        // Closed: none of its fields keeps the keys.
        if answer.is_some() {
            ctx.memory_mut(|m| m.focused().map(|id| m.surrender_focus(id)));
        }
        match answer {
            Some(true) => w.resize_to(f.size, f.filter()),
            Some(false) => {}
            None => w.resizing = Some(f),
        }
    }
}

/// The window: the size now and the new one, the proportions, the filter; the result or why it
/// cannot be; Resize and Cancel. `Some(true)`: Resize.
fn resize_form(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n, keys: Option<bool>) -> Option<bool> {
    let t = |id| i18n.t(id);
    ui.set_width(420.0);
    crate::theme::title(ui, t("resize-title"));
    ui.add_space(6.0);
    egui::Grid::new("resize-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| size_rows(ui, f, i18n));
    ui.add_space(8.0);
    let why = f.why_not();
    let pixels = |(w, h): (u32, u32)| i18n.t_args("resize-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]);
    match why {
        Some("resize-too-big") => crate::error_bar::hint(ui, &i18n.t_args("resize-too-big", &[("max", &MAX_SIDE.to_string())])),
        Some(id) => ui.weak(t(id)),
        None => ui.weak(format!("{} {}", t("resize-result"), pixels(f.size))),
    };
    ui.add_space(8.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        let enter = keys == Some(true) || ui.input(|i| i.key_pressed(egui::Key::Enter));
        let resize = ui.add_enabled(why.is_none(), crate::theme::main_button(ui, t("resize-apply")));
        if why.is_none() && (resize.clicked() || enter) {
            answer = Some(true);
        }
        if ui.button(t("delete-no")).clicked() || keys == Some(false) || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            answer = Some(false);
        }
    });
    answer
}

/// The size now; the new one in pixels or percent, with the unit; the proportions; the filter.
fn size_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.label(t("resize-now"));
    ui.label(i18n.t_args("resize-pixels", &[("w", &f.from.0.to_string()), ("h", &f.from.1.to_string())]));
    ui.end_row();
    ui.label(t("resize-size"));
    ui.horizontal(|ui| {
        if f.percent {
            let (mut pw, mut ph) = f.percents();
            if ui.add(egui::DragValue::new(&mut pw).range(1..=MAX_PERCENT).suffix(" %")).changed() {
                f.set_percent(true, pw);
            }
            ui.label("×");
            if ui.add(egui::DragValue::new(&mut ph).range(1..=MAX_PERCENT).suffix(" %")).changed() {
                f.set_percent(false, ph);
            }
        } else {
            let (mut w, mut h) = f.size;
            if ui.add(egui::DragValue::new(&mut w).range(1..=MAX_SIDE)).changed() {
                f.set_width(w);
            }
            ui.label("×");
            if ui.add(egui::DragValue::new(&mut h).range(1..=MAX_SIDE)).changed() {
                f.set_height(h);
            }
        }
        let unit = |percent: bool| if percent { t("resize-percent") } else { t("resize-unit-pixels") };
        egui::ComboBox::from_id_salt("resize-unit").selected_text(unit(f.percent)).show_list(ui, |ui| {
            for percent in [false, true] {
                crate::confirm::choice(ui, &mut f.percent, percent, unit(percent));
            }
        });
    });
    ui.end_row();
    ui.label("");
    let mut locked = f.locked;
    let r = ui.add_enabled(f.unlockable, egui::Checkbox::new(&mut locked, t("resize-keep"))).on_disabled_hover_text(t("resize-keep-off"));
    if r.changed() {
        f.lock(locked);
    }
    ui.end_row();
    ui.label(t("batch-filter"));
    let shown = f.filter();
    egui::ComboBox::from_id_salt("resize-filter").width(220.0).selected_text(t(shown.label())).show_list(ui, |ui| {
        for filter in Filter::ALL {
            let r = ui.selectable_label(shown == filter, t(filter.label()));
            crate::confirm::list_entry(ui, &r, shown == filter);
            if r.clicked() {
                f.chosen = Some(filter);
            }
        }
    });
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::{Tool, Window};
    use crate::edit::resize::{RESAMPLES, Scale};
    use eframe::egui;

    /// A page on a photo of 400 × 300, read.
    pub(in crate::edit_page) fn read(ctx: &egui::Context) -> Window {
        let mut w = Window::new("p/a.png".into(), Default::default(), Tool::Select);
        let photo = Arc::new(crate::encode::Original::with_meta(image::DynamicImage::new_rgb8(400, 300), &[]));
        let tex = crate::mipmap::Picture::now(ctx, "test", crate::decode::preview(&photo.image), crate::mipmap::SMALLEST_PICTURE);
        w.full = Some(photo.clone());
        w.original = Some(Ok((photo, tex, 0)));
        w.doc = crate::edit::doc::Doc::new(400, 300, None);
        w
    }

    /// Audit 5.14 (#295): after Resize, and Undo or Redo across it, the frame resampled the whole
    /// photo (up to 20 000 pixels a side) and made its preview: the window stood still meanwhile.
    /// The frame only asks; a thread aside resamples.
    #[test]
    fn the_photo_is_resampled_aside() {
        let ctx = egui::Context::default();
        let mut w = read(&ctx);
        w.doc.scale = Some(Scale { w: 200, h: 150, filter: crate::batch::Filter::Lanczos3 });
        RESAMPLES.with(|n| n.set(0));
        w.follow_size(&ctx);
        let n = RESAMPLES.with(|n| n.get());
        eprintln!("photos resampled in the frame: {n}");
        assert_eq!(n, 0, "the photo resampled in the frame");
        assert!(w.working(), "the status bar says Preparing");
        // Meanwhile the page is at the new size, drawn from the photo before.
        assert_eq!(w.upright(), (200, 150));
        w.follow_size(&ctx);
        let start = std::time::Instant::now();
        while w.working() {
            assert!(start.elapsed().as_secs() < 10, "the photo never came");
            std::thread::sleep(std::time::Duration::from_millis(5));
            w.arrived(&ctx);
        }
        let Some(Ok((photo, ..))) = &w.original else { panic!() };
        assert_eq!((photo.image.width(), photo.image.height(), w.scaled), (200, 150, w.doc.scale));
    }

    /// While the photo is resampled, what is made from it starts from the original (taken to the
    /// document's size by `at_size`), not from the photo shown, of the size before.
    #[test]
    fn made_from_the_original_while_resampled() {
        let ctx = egui::Context::default();
        let mut w = read(&ctx);
        let shown = |w: &Window| if let Some(Ok((o, ..))) = &w.original { o.clone() } else { panic!() };
        assert!(Arc::ptr_eq(&w.photo().unwrap(), &shown(&w)));
        w.doc.scale = Some(Scale { w: 200, h: 150, filter: crate::batch::Filter::Nearest });
        w.follow_size(&ctx);
        assert!(Arc::ptr_eq(&w.photo().unwrap(), w.full.as_ref().unwrap()));
        let start = std::time::Instant::now();
        while w.working() {
            assert!(start.elapsed().as_secs() < 10, "the photo never came");
            std::thread::sleep(std::time::Duration::from_millis(5));
            w.arrived(&ctx);
        }
        assert!(Arc::ptr_eq(&w.photo().unwrap(), &shown(&w)));
        // Undo: the original's size, the photo shown still of the other.
        w.doc.scale = None;
        let made = w.photo().unwrap();
        assert_eq!((made.image.width(), made.image.height()), (400, 300));
    }

    /// Resize, then Undo before the photo comes, then Redo: one photo made, the one asked last
    /// shown; a photo for a size the document left is dropped.
    #[test]
    fn only_the_last_size_is_shown() {
        let ctx = egui::Context::default();
        let mut w = read(&ctx);
        let half = Some(Scale { w: 200, h: 150, filter: crate::batch::Filter::Lanczos3 });
        w.doc.scale = half;
        w.follow_size(&ctx);
        w.doc.scale = None;
        w.follow_size(&ctx);
        w.doc.scale = half;
        w.follow_size(&ctx);
        assert_eq!(w.jobs_under_way(), 1, "asked twice for the same size");
        let wait = |w: &mut Window| {
            let start = std::time::Instant::now();
            while w.working() {
                assert!(start.elapsed().as_secs() < 10, "the photo never came");
                std::thread::sleep(std::time::Duration::from_millis(5));
                w.arrived(&ctx);
            }
        };
        wait(&mut w);
        assert_eq!((w.upright(), w.scaled), ((200, 150), half));
        // Undo before the photo comes back: the one at the original's size.
        w.doc.scale = None;
        w.follow_size(&ctx);
        w.doc.scale = Some(Scale { w: 100, h: 75, filter: crate::batch::Filter::Nearest });
        w.follow_size(&ctx);
        wait(&mut w);
        let Some(Ok((photo, ..))) = &w.original else { panic!() };
        assert_eq!((photo.image.width(), w.scaled), (100, w.doc.scale));
    }
}
