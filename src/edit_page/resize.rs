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
    /// the original each time (never twice). The next objects' widths and the view follow it.
    pub(super) fn follow_size(&mut self, ctx: &egui::Context) {
        if self.doc.scale == self.scaled {
            return;
        }
        let (Some(full), Some(Ok((_, _, size)))) = (self.full.clone(), &self.original) else { return };
        let (size, start) = (*size, std::time::Instant::now());
        let photo = match self.doc.scale {
            Some(s) => Arc::new(resize::resample(&full, s)),
            None => full,
        };
        let tex = crate::mipmap::Picture::now(ctx, "edit-original", crate::decode::preview(&photo.image), crate::mipmap::SMALLEST_PICTURE);
        log!("edit: photo at {:?} in {:?}", self.doc.scale, start.elapsed());
        let before = self.long();
        self.original = Some(Ok((photo, tex, size)));
        self.scaled = self.doc.scale;
        let k = self.long() / before;
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
    }
}
