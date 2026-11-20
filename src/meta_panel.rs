//! Metadata panel under the grid, and the status bar of the viewer.

use eframe::egui::{self, Align2, Color32, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use crate::i18n::I18n;
use crate::meta::Group;
use crate::{App, human, meta};
use crate::source;

impl App {
    pub(crate) fn meta_shown(&self, g: meta::Group) -> bool {
        !self.settings.meta_hidden.iter().any(|k| k == g.key())
    }

    /// Opens or closes a metadata box (X, menu, button) and remembers it.
    pub(crate) fn show_meta(&mut self, g: meta::Group, show: bool) {
        self.settings.meta_hidden.retain(|k| k != g.key());
        if !show {
            self.settings.meta_hidden.push(g.key().to_string());
        }
        self.settings.save();
    }

    /// Whether a metadata box is open: the ℹ button is on then.
    pub(crate) fn any_meta_shown(&self) -> bool {
        meta::Group::ALL.into_iter().any(|g| self.meta_shown(g))
    }

    /// The ℹ button: closes all the boxes if one is open, otherwise reopens them all.
    pub(crate) fn toggle_all_meta(&mut self) {
        let any = self.any_meta_shown();
        for g in meta::Group::ALL {
            self.show_meta(g, !any);
        }
    }

    /// Metadata boxes of the selected image, side by side below the grid; each one closes with
    /// its X and reopens from the View menu or the ℹ button.
    pub(crate) fn meta_panel(&mut self, ui: &mut Ui) {
        self.meta_title_bar(ui);
        if self.settings.meta_collapsed {
            return;
        }
        let ndirs = match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.dirs.len(),
            _ => 0,
        };
        let file = self.selected.checked_sub(ndirs).filter(|_| self.cursor);
        let file = file.and_then(|i| self.files().get(i).map(|f| (i, f.clone())));
        if let (Some((i, f)), Some(loader)) = (&file, &self.loader)
            && self.meta_asked != Some((self.generation, *i))
        {
            loader.load_meta(self.generation, *i, f.clone());
            self.meta_asked = Some((self.generation, *i));
        }
        // While the new data arrives the previous image's stay: the values that do not change
        // (make, model, lens…) stay still instead of blinking through "Reading…".
        let meta = match (&file, &self.meta) {
            (Some(_), Some((_, m))) => Some(m.clone()),
            _ => None,
        };
        let t = |id: &str| self.i18n.t(id);
        let drawing = file.as_ref().is_some_and(|(_, f)| crate::cad_convert::can_convert(&f.path));
        let note = match (&file, &meta) {
            (None, _) => t("meta-none-selected"),
            (Some(_), None) => t("meta-loading"),
            (_, Some(Err(e))) => e.clone(),
            (_, Some(Ok(None))) if drawing => t("cad-props-none"),
            (_, Some(Ok(None))) => t("meta-no-exif"),
            _ => String::new(),
        };
        // A video or an audio file without the video engine: no box but File has anything, so
        // the note once beside it, not repeated in every box.
        let played = file.as_ref().is_some_and(|(_, f)| crate::decode::is_played(&f.path));
        let alone = matches!(meta, Some(Err(_))) && played && !crate::mpv::available();
        let meta = meta.and_then(|m| m.ok().flatten());
        let mut close = None;
        // A fixed height (`meta_size`): a taller box scrolls, a shorter one leaves room below,
        // and the grid above never jumps from one file to the next.
        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for g in boxes(alone, drawing, |g| self.meta_shown(g)) {
                    egui::Frame::group(ui.style()).show(ui, |ui| ui.vertical(|ui| {
                        self.meta_box(ui, g, file.as_ref().map(|(_, f)| f), meta.as_ref(), &note, &mut close);
                    }));
                }
                if alone {
                    egui::Frame::group(ui.style()).show(ui, |ui| ui.vertical(|ui| {
                        ui.set_width(NOTE_WIDTH);
                        ui.add(egui::Label::new(&note).wrap());
                    }));
                }
            });
        });
        if let Some(g) = close {
            self.show_meta(g, false);
        }
    }

    /// The metadata panel under the grid, when a box is open.
    pub(crate) fn meta_bottom(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if !self.any_meta_shown() {
            return;
        }
        // Collapsed, its own id: the open panel's remembered height does not hold the bar.
        let panel = if self.settings.meta_collapsed {
            egui::Panel::bottom("meta-bar")
        } else {
            egui::Panel::bottom("meta").exact_size(self.meta_size(ctx))
        };
        panel.show(ui, |ui| self.meta_panel(ui));
    }

    /// Height of the open metadata panel: its part of the window, between the title bar with
    /// two lines and half the window.
    pub(crate) fn meta_size(&self, ctx: &egui::Context) -> f32 {
        let window = ctx.content_rect().height();
        (self.settings.meta_size * window).clamp(META_MIN.min(window / 2.0), window / 2.0)
    }

    /// The top edge of the open panel, dragged up or down: its height, kept as a part of the window.
    fn meta_resize(&mut self, ui: &Ui) {
        let top = ui.max_rect().top() - ui.spacing().item_spacing.y;
        let edge = Rect::from_x_y_ranges(ui.max_rect().x_range(), top - 4.0..=top + 4.0);
        let drag = ui.interact(edge, egui::Id::unique("meta-resize"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if drag.dragged() {
            let window = ui.ctx().content_rect().height();
            let size = self.meta_size(ui.ctx()) - drag.drag_delta().y;
            self.settings.meta_size = size.clamp(META_MIN.min(window / 2.0), window / 2.0) / window;
        }
        if drag.drag_stopped() {
            self.settings.save();
        }
    }

    /// Title bar of the metadata panel.
    pub(crate) fn meta_title_bar(&mut self, ui: &mut Ui) {
        // Title bar: a click anywhere, or on the button on the right, collapses and reopens the
        // panel; dragging its top edge changes the panel's height (`meta_resize`).
        let collapsed = self.settings.meta_collapsed;
        let (rect, bar) = ui.allocate_exact_size(vec2(ui.available_width(), ui.spacing().interact_size.y), Sense::click());
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, self.i18n.t("meta-title"), font, ui.visuals().strong_text_color());
        let button_rect = Rect::from_min_max(pos2(rect.right() - 26.0, rect.top()), rect.max);
        let hint = self.i18n.t(if collapsed { "meta-expand" } else { "meta-collapse" });
        let button = ui.put(button_rect, crate::icons::small(ui, if collapsed { crate::icons::Line::ChevronUp } else { crate::icons::Line::Down })).on_hover_text(&hint);
        let bar = bar.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand);
        if !collapsed {
            self.meta_resize(ui);
        }
        if bar.clicked() || button.clicked() {
            self.settings.meta_collapsed = !collapsed;
            log!("metadata collapsed: {}", !collapsed);
            self.settings.save();
        }
    }

    /// One metadata box (`g`) with its X (`close`), its fields, or `note` without metadata.
    pub(crate) fn meta_box(&self, ui: &mut Ui, g: Group, file: Option<&source::File>, meta: Option<&meta::Meta>, note: &str, close: &mut Option<Group>) {
        let t = |id: &str| self.i18n.t(id);
        ui.set_width(250.0);
        ui.horizontal(|ui| {
            ui.strong(t(&format!("meta-{}", g.key())));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(crate::icons::small(ui, crate::icons::Line::Close)).on_hover_text(t("meta-close")).clicked() {
                    *close = Some(g);
                }
            });
        });
        ui.separator();
        if g == Group::File {
            self.file_box(ui, file, meta);
            return;
        }
        let Some(m) = meta else {
            ui.weak(note);
            return;
        };
        let fields: Vec<_> = m.fields.iter().filter(|(fg, _, _)| *fg == g).collect();
        let gps = m.gps.filter(|_| g == Group::Gps);
        if fields.is_empty() && gps.is_none() {
            ui.weak(t("meta-empty"));
        }
        egui::Grid::new(("meta", g.key())).num_columns(2).show(ui, |ui| {
            if let Some((lat, lon)) = gps {
                ui.weak(t("meta-coords"));
                ui.label(format!("{lat:.6}, {lon:.6}"));
                ui.end_row();
            }
            for (_, label, value) in fields {
                // A name the drawing gives (`cad_meta`) as it is; a heading in bold.
                match label.strip_prefix('=') {
                    Some(own) => ui.weak(own),
                    None if value.is_empty() => ui.strong(t(label)),
                    None => ui.weak(t(label)),
                };
                ui.label(self.meta_value(label, value));
                ui.end_row();
            }
        });
        if let Some((lat, lon)) = gps {
            let url = format!("https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=16/{lat}/{lon}");
            ui.hyperlink_to(t("meta-map"), url);
        }
    }

    /// The File box: name, size on disk (exact too), file date and, once the metadata are read,
    /// the pixels; shown also for files without metadata.
    fn file_box(&self, ui: &mut Ui, file: Option<&source::File>, meta: Option<&meta::Meta>) {
        let t = |id: &str| self.i18n.t(id);
        let Some(f) = file else {
            ui.weak(t("meta-none-selected"));
            return;
        };
        egui::Grid::new(("meta", "file")).num_columns(2).show(ui, |ui| {
            for (label, value) in file_rows(&self.i18n, f, meta) {
                ui.weak(t(label));
                ui.add(egui::Label::new(value).truncate());
                ui.end_row();
            }
        });
    }

    /// Tooltip of a file of the grid (`file`: index among the images): the File box's lines, the
    /// pixels only if its metadata were already read (no file is read for a tooltip). Two columns
    /// (#118): the labels in bold and right-aligned, so every value starts at the same place.
    pub(crate) fn file_tooltip(&self, ui: &mut Ui, file: usize) {
        let Some(f) = self.files().get(file) else { return };
        let meta = match &self.meta {
            Some((i, Ok(Some(m)))) if *i == file => Some(m),
            _ => None,
        };
        let mut rows = file_hint(&self.i18n, f, meta);
        // What the grid read for the line under the thumbnail (#139): the size in pixels if the
        // metadata are not read yet, and the date inside the file beside the file's own date.
        if let Some(info) = self.thumb_infos.get(&f.path) {
            let pixels = self.i18n.t("meta-pixels");
            if let Some([w, h]) = info.pixels.filter(|_| !rows.iter().any(|(l, _)| *l == pixels)) {
                rows.push((pixels, format!("{w} × {h}")));
            }
            if let Some(taken) = &info.taken {
                let label = if crate::decode::is_played(&f.path) { "tip-recorded" } else { "tip-taken" };
                let at = rows.iter().position(|(l, _)| *l == self.i18n.t("meta-file-date")).map_or(rows.len(), |i| i + 1);
                rows.insert(at, (self.i18n.t(label), self.meta_value("meta-taken", taken)));
            }
        }
        let font = egui::FontId::new(egui::TextStyle::Body.resolve(ui.style()).size, egui::FontFamily::Name(crate::fonts::BOLD.into()));
        let color = ui.visuals().text_color();
        // The labels' column as wide as the longest label, each one right-aligned in it.
        let size = rows.iter().map(|(label, _)| ui.painter().layout_no_wrap(label.clone(), font.clone(), color).size()).fold(egui::Vec2::ZERO, egui::Vec2::max);
        egui::Grid::new("file-tooltip").num_columns(2).show(ui, |ui| {
            for (label, value) in rows {
                ui.allocate_ui_with_layout(size, egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.label(egui::RichText::new(label).font(font.clone()));
                });
                ui.label(value);
                ui.end_row();
            }
        });
    }

    /// Status bar of the viewer, like the grid's: name, position, pixels, size on disk, date
    /// taken (or modified, without EXIF or with the metadata boxes closed).
    /// The Keys button (`keys_window`), on a CAD drawing with layers the Layers button, and while
    /// measuring with results Copy; on a picture the zoom group beside Keys (`zoom_bar`), with
    /// what it asks for.
    pub(crate) fn viewer_status(&self, ui: &mut Ui, current: usize) -> ([Option<egui::Response>; 3], Option<crate::zoom::Ask>) {
        let files = self.files();
        let Some(f) = files.get(current) else { return ([None, None, None], None) };
        let mut zoom_on = false;
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        let top_copy = self.measure_row(ui);
        self.export_said_rows(ui);
        // On more rows in a narrow window (`bar_rows`): the date went under Keys at 520 px.
        let (mut buttons, zoom) = crate::bar_rows::rows(ui, |ui| {
            self.phone_waiting(ui);
            // The same note as the grid's bar: a file gone from the device, say.
            if let Some(note) = &self.status_note {
                ui.label(crate::theme::alert(note.clone()));
                ui.separator();
            }
            ui.label(source::shown(&f.path));
            ui.separator();
            ui.label(format!("{} / {}", current + 1, files.len()));
            if let Some((_, mesh)) = self.viewed_mesh() {
                ui.separator();
                ui.label(self.i18n.t_args("model-triangles", &[("n", &mesh.tris.len().to_string())]));
                self.model_warnings(ui, &mesh);
                if let Some(fps) = self.model_fps() {
                    ui.separator();
                    crate::bar_rows::steady(ui, &format!("{fps} fps"), "000 fps");
                }
                ui.separator();
                crate::bar_rows::steady(ui, &format!("{:.0}%", self.zoom_shown * 100.0), "0000%");
                self.section_bar(ui);
            } else if crate::cad::is_cad(&f.path) {
                // A drawing has no pixels: its own size, in its unit.
                for text in [self.cad_size_text(current), self.cad_sheet_name(current), self.compare_text(current)].into_iter().flatten() {
                    ui.separator();
                    ui.label(text);
                }
                if let Some(warning) = self.compare_units(current) {
                    ui.separator();
                    ui.label(crate::theme::alert(warning));
                }
                ui.separator();
                ui.label(format!("{:.0}%", self.zoom_shown * 100.0));
            } else if let Some(Ok(tex)) = self.full.get(&current) {
                // Its own size, as the metadata panel: not the reduced one beyond 8192 px, nor the
                // 2048 px an SVG is drawn at.
                let [w, h] = self.own_sizes.get(&f.path).copied().unwrap_or_else(|| tex.size().map(|n| n as u32));
                let (w, h) = if self.turn.sideways() { (h, w) } else { (w, h) };
                ui.separator();
                ui.label(self.i18n.t_args("convert-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]));
                zoom_on = true;
                if let Some((id, place)) = self.frames_status(&f.path) {
                    ui.separator();
                    crate::frames_view::counter(ui, &self.i18n, id, place, egui::RichText::new);
                }
            }
            if let Some([w, h]) = self.video_size() {
                ui.separator();
                ui.label(self.i18n.t_args("convert-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]));
            }
            if !self.turn.is_none() {
                ui.separator();
                ui.label(crate::theme::alert(self.i18n.t(if self.preparing.as_ref() == Some(&f.path) {
                    "status-preparing"
                } else if self.saving.contains(&f.path) {
                    "status-saving"
                } else {
                    "status-modified"
                })));
            }
            ui.separator();
            ui.label(self.human_size(f.size));
            let taken = match &self.meta {
                Some((i, Ok(Some(m)))) if *i == current => m.fields.iter().find(|(_, l, _)| *l == "meta-taken").map(|(_, l, v)| self.meta_value(l, v)),
                _ => None,
            };
            if let Some(date) = taken.or_else(|| self.file_date(&f.modified)) {
                ui.separator();
                ui.label(date);
            }
            self.status_buttons(ui, current, zoom_on)
        });
        if top_copy.is_some() {
            buttons[2] = top_copy;
        }
        (buttons, zoom)
    }

    /// What the model's reader did differently from the file (a glTF node with two parents,
    /// #179), in the status bar.
    fn model_warnings(&self, ui: &mut Ui, mesh: &crate::model::Mesh) {
        for id in &mesh.warnings {
            ui.separator();
            ui.label(crate::theme::alert(self.i18n.t(id)));
        }
    }

    /// While measuring, when the results do not fit beside the file's details: on a row of their
    /// own above them, on more lines if needed, with Copy; before, they were cut short with "…".
    fn measure_row(&self, ui: &mut Ui) -> Option<egui::Response> {
        let (text, results) = self.measure_status()?;
        if ui.data(|d| d.get_temp::<bool>(measure_fits_id())).unwrap_or(true) {
            return None;
        }
        let copy = ui.horizontal_top(|ui| {
            let button = egui::Button::new(egui::RichText::new(self.i18n.t("measure-copy")).color(crate::theme::solid(ui.visuals()))).fill(Color32::WHITE);
            let width = ui.available_width() - if results { 80.0 } else { 0.0 };
            ui.scope(|ui| {
                ui.set_max_width(width);
                ui.add(egui::Label::new(measure_text(&text)).wrap());
            });
            results.then(|| ui.add(button).on_hover_text("Ctrl+C"))
        });
        ui.separator();
        copy.inner
    }

    /// The right of the viewer's status bar: Keys, on a picture (`zoom_on`) the zoom group beside
    /// it (#152), Layers on a drawing with layers, and while measuring the results (or how to go
    /// on) with Copy. Together they go on a row of their own when they do not fit (`bar_rows`).
    fn status_buttons(&self, ui: &mut Ui, current: usize, zoom_on: bool) -> ([Option<egui::Response>; 3], Option<crate::zoom::Ask>) {
        // On the right, out of the way: the keys, listed when asked. Blue on
        // white: the bar makes every text white, and the button was white on white.
        let button = |id: &str| egui::Button::new(egui::RichText::new(self.i18n.t(id)).color(crate::theme::solid(ui.visuals()))).fill(Color32::WHITE);
        let (layers, copy) = (button("cad-layers-title"), button("measure-copy"));
        let layers_on = self.cad_layers_off().is_none();
        let measure = self.measure_status();
        crate::bar_rows::right_end(ui, "viewer-status", |ui| {
            let keys = self.keys_hint(current).map(|_| self.keys_button(ui));
            let zoom = zoom_on.then(|| {
                ui.separator();
                self.zoom_group(ui)
            });
            let layers = layers_on.then(|| ui.add(layers).on_hover_text(self.i18n.t("key-layers")));
            // While measuring: the results (or how to go on) take the room left, Copy beside. In
            // the bold font: `strong` only changes the colour, and the bar makes every text white.
            // The results on a row of their own (`measure_row`) when they did not fit here last time.
            let fits = ui.data(|d| d.get_temp::<bool>(measure_fits_id())).unwrap_or(true);
            let copy = measure.as_ref().filter(|_| fits).and_then(|(_, results)| results.then(|| ui.add(copy).on_hover_text("Ctrl+C")));
            // The buttons need their room; the results are cut to what is left.
            let need = ui.min_rect().width();
            if let Some((text, _)) = measure {
                // Measured the same way wherever the results are, so the choice does not swing.
                let font = egui::FontId::new(egui::TextStyle::Body.resolve(ui.style()).size, egui::FontFamily::Name(crate::BOLD.into()));
                let wide = ui.painter().layout_no_wrap(text.clone(), font, Color32::WHITE).size().x;
                let room = ui.available_width() - if fits { 0.0 } else { ui.spacing().interact_size.x + 30.0 };
                let now = wide + 16.0 <= room;
                if now != fits {
                    ui.data_mut(|d| d.insert_temp(measure_fits_id(), now));
                    ui.ctx().request_discard("the measure's row chosen");
                }
                if fits {
                    ui.separator();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| ui.add(egui::Label::new(measure_text(&text)).truncate()).on_hover_text(&text));
                }
            }
            (([keys, layers, copy], zoom.flatten()), need)
        })
    }

    /// The Keys button of a status bar (viewer, grid). Blue on white: the bar makes every text
    /// white, and the button was white on white.
    pub(crate) fn keys_button(&self, ui: &mut Ui) -> egui::Response {
        let keys = egui::Button::new(egui::RichText::new(self.i18n.t("keys-button")).color(crate::theme::solid(ui.visuals()))).fill(Color32::WHITE);
        ui.add(keys)
    }

    /// The keys of the viewer, one a row, over its bottom right corner above the status bar
    /// (`bar`: its height): in place of a band whose letters got smaller to fit.
    pub(crate) fn keys_window(&self, ctx: &egui::Context, id: &str, bar: f32) {
        let text = self.i18n.t(id);
        egui::Area::new(egui::Id::unique("viewer-keys")).order(egui::Order::Foreground).anchor(Align2::RIGHT_BOTTOM, vec2(-8.0, -bar - 8.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(self.i18n.t("keys-button")).strong().size(16.0));
                ui.separator();
                // A row for each key, in alternating stripes, so a key stays with what it does. The
                // dark theme's stripe was the popup's own grey, the light one too faint.
                ui.visuals_mut().faint_bg_color = if ui.visuals().dark_mode { Color32::from_gray(60) } else { Color32::from_gray(228) };
                egui::Grid::new("viewer-keys-grid").num_columns(2).spacing([18.0, 6.0]).striped(true).show(ui, |ui| {
                    for (keys, what) in key_rows(&text) {
                        ui.label(egui::RichText::new(keys).strong());
                        ui.label(what);
                        ui.end_row();
                    }
                });
            });
        });
    }

    /// The keys of the file shown (a 3D model, a CAD drawing, a video or sound, an image): the
    /// Fluent id of their list (`keys_window`). Images too (only the 3D and CAD
    /// viewers had the button).
    pub(crate) fn keys_hint(&self, current: usize) -> Option<&'static str> {
        // While measuring, the keys of Measure: the status bar says only what to click.
        if self.model.measure.on && self.measure_off().is_none() {
            return Some(if self.viewed_mesh().is_some() { "hint-measure-3d" } else { "hint-measure-cad" });
        }
        if self.viewed_mesh().is_some() {
            return Some("hint-model");
        }
        let path = &self.files().get(current)?.path;
        Some(match () {
            _ if crate::cad::is_cad(path) => if self.cad_has_sheets(current) { "hint-cad-layouts" } else { "hint-cad" },
            _ if crate::decode::is_played(path) => "hint-video",
            _ => "hint-image",
        })
    }

    /// Value of a metadata field as shown: Fluent ids translated, dates in the language's format.
    pub(crate) fn meta_value(&self, label: &str, value: &str) -> String {
        if value.starts_with("meta-") || label == "meta-xmp-label" {
            // Colour labels: one id each ("meta-label-red, meta-label-blue"), or Lightroom's own names.
            return value.split(", ").map(|v| if v.starts_with("meta-") { self.i18n.t(v) } else { v.to_string() }).collect::<Vec<_>>().join(", ");
        }
        let formatted = match label {
            "meta-taken" if value.contains(' ') => value.parse::<jiff::civil::DateTime>().ok().map(|d| d.strftime(&self.i18n.t("date-format")).to_string()),
            // Only the day when the IPTC or the XMP says no time.
            "meta-taken" => value.parse::<jiff::civil::Date>().ok().map(|d| d.strftime(&self.i18n.t("date-day-format")).to_string()),
            "meta-gps-date" => value.parse::<jiff::civil::Date>().ok().map(|d| d.strftime(&self.i18n.t("date-day-format")).to_string()),
            _ => None,
        };
        formatted.unwrap_or_else(|| value.to_string())
    }
}

/// Whether the measure's results fit on the status bar's row (`measure_row`).
fn measure_fits_id() -> egui::Id {
    egui::Id::unique("measure-fits")
}

/// The measure's text in the bold font: `strong` only changes the colour, and the bar makes every
/// text white.
fn measure_text(text: &str) -> egui::RichText {
    egui::RichText::new(text).family(egui::FontFamily::Name(crate::BOLD.into()))
}

/// Width of the note that stands for the boxes without data (two boxes and the gap between).
const NOTE_WIDTH: f32 = 512.0;
/// Smallest height of the open metadata panel: the title bar and two lines of a box.
const META_MIN: f32 = 110.0;

/// The boxes drawn, those not closed (`shown`); `alone`: only File, the others have nothing to
/// show and one note says why (the video engine missing).
/// The boxes for a file: with `alone` only File; a CAD `drawing` has File and Drawing, the
/// others have every box but Drawing.
fn boxes(alone: bool, drawing: bool, shown: impl Fn(Group) -> bool) -> Vec<Group> {
    let fits = |g: Group| match g {
        Group::File => true,
        _ if alone => false,
        Group::Drawing => drawing,
        _ => !drawing,
    };
    Group::ALL.into_iter().filter(|&g| shown(g) && fits(g)).collect()
}

/// "Wheel: zoom | Drag: move" → ("Wheel", "zoom"), ("Drag", "move"): the keys and what they do,
/// as the `hint-*` texts write them.
fn key_rows(text: &str) -> Vec<(&str, &str)> {
    text.split('|').filter_map(|part| part.split_once(':')).map(|(k, w)| (k.trim(), w.trim())).collect()
}

/// The File box's lines (label id, value): name, size on disk (exact too), file date and, once
/// the metadata are read (`meta`), the pixels. Lines without a value are left out.
fn file_rows(i18n: &I18n, f: &source::File, meta: Option<&meta::Meta>) -> Vec<(&'static str, String)> {
    let t = |id: &str| i18n.t(id);
    let field = |label: &str| meta.and_then(|m| m.fields.iter().find(|(_, l, _)| *l == label).map(|(_, _, v)| v.clone()));
    let pixels = field("meta-width").zip(field("meta-height")).map(|(w, h)| format!("{} × {h}", w.trim_end_matches(" px")));
    let size = format!("{} ({} {})", human::size(i18n, f.size), grouped(f.size, &t("thousands-separator")), t("meta-bytes"));
    let rows = [
        ("meta-file-name", Some(source::shown(&f.path).trim_end_matches('/').to_string())),
        ("meta-file-size", Some(size)),
        ("meta-file-date", human::date(i18n, &f.modified)),
        ("meta-pixels", pixels),
    ];
    rows.into_iter().filter_map(|(label, value)| value.map(|v| (label, v))).collect()
}

/// The grid's tooltip on a file: the File box's lines, as (label, value) in the interface's language.
fn file_hint(i18n: &I18n, f: &source::File, meta: Option<&meta::Meta>) -> Vec<(String, String)> {
    file_rows(i18n, f, meta).into_iter().map(|(label, v)| (i18n.t(label), v)).collect()
}

/// 245464190 → "245.464.190" with `sep` between the groups of three digits.
fn grouped(n: u64, sep: &str) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    /// Every key list of the viewers splits into rows, in both languages.
    #[test]
    fn key_rows_of_the_hints() {
        assert_eq!(super::key_rows("Wheel, + and −: zoom | Drag: move"), [("Wheel, + and −", "zoom"), ("Drag", "move")]);
        for ftl in [include_str!("../locales/en.ftl"), include_str!("../locales/it.ftl")] {
            for line in ftl.lines().filter(|l| l.starts_with("hint-")) {
                let text = line.split_once(" = ").unwrap().1;
                assert_eq!(super::key_rows(text).len(), text.split('|').count(), "{line}");
            }
        }
    }

    /// The grid's tooltip on a file: full name, size, date, and the pixels only once known.
    #[test]
    fn file_tooltip_lines() {
        use crate::i18n::I18n;
        use crate::meta::{Group, Meta};
        let f = crate::source::File { path: "DCIM/Camera/a rather long holiday name.jpg".into(), size: 4_089_123, modified: String::new() };
        let en = I18n::new("en");
        let row = |label: &str, value: &str| (label.to_string(), value.to_string());
        assert_eq!(super::file_hint(&en, &f, None), [row("Name", "a rather long holiday name.jpg"), row("Size", "3.9 MB (4,089,123 bytes)")]);
        let meta = Meta { fields: vec![(Group::Image, "meta-width", "4000 px".into()), (Group::Image, "meta-height", "3000 px".into())], gps: None };
        let it = I18n::new("it");
        let dated = crate::source::File { modified: "1776333600".into(), ..f };
        let rows = super::file_hint(&it, &dated, Some(&meta));
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert_eq!(rows[0], row("Nome", "a rather long holiday name.jpg"));
        assert_eq!(rows[1], row("Dimensione", "3,9 MB (4.089.123 byte)"));
        assert!(rows[2].0 == "Data del file" && rows[2].1.contains("/2026 "), "{:?}", rows[2]);
        assert_eq!(rows[3], row("Pixel", "4000 × 3000 px"));
    }

    /// Without the video engine only the File box stays, if not closed: the note is said once,
    /// not in every box.
    #[test]
    fn engine_note_once() {
        use crate::meta::Group;
        assert_eq!(super::boxes(false, false, |_| true), [Group::File, Group::Camera, Group::Shot, Group::Image, Group::Gps, Group::Info]);
        assert_eq!(super::boxes(true, false, |_| true), [Group::File]);
        assert!(super::boxes(true, false, |g| g != Group::File).is_empty());
        assert_eq!(super::boxes(false, false, |g| g != Group::Camera).len(), 5);
        // A drawing: its properties beside the File box, nothing of a photo.
        assert_eq!(super::boxes(false, true, |_| true), [Group::File, Group::Drawing]);
    }

    #[test]
    fn digits_in_groups() {
        assert_eq!(super::grouped(245464190, "."), "245.464.190");
        assert_eq!(super::grouped(999, "."), "999");
        assert_eq!(super::grouped(1000, " "), "1 000");
    }
}
