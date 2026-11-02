//! Metadata panel under the grid, and the status bar of the viewer.

use eframe::egui::{self, Align2, Color32, Rect, ScrollArea, Sense, Ui, pos2, vec2};

use crate::meta::Group;
use crate::{App, meta};
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
        self.meta_height = 0.0;
        self.settings.save();
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
        let note = match (&file, &meta) {
            (None, _) => t("meta-none-selected"),
            (Some(_), None) => t("meta-loading"),
            (_, Some(Err(e))) => e.clone(),
            (_, Some(Ok(None))) => t("meta-no-exif"),
            _ => String::new(),
        };
        // The panel never shrinks: with a photo without EXIF, or while the data arrives (only
        // one line), it shrank and grew back at every click and the grid above jumped.
        // It starts again from its content when a box is opened or closed (`show_meta`).
        // Only the boxes are measured: the whole panel includes the title bar and grew at every frame.
        let meta = meta.and_then(|m| m.ok().flatten());
        let mut close = None;
        let min_height = self.meta_height;
        let boxes = ScrollArea::horizontal().show(ui, |ui| {
            ui.set_min_height(min_height);
            ui.horizontal_top(|ui| {
                for g in Group::ALL.into_iter().filter(|&g| self.meta_shown(g)) {
                    egui::Frame::group(ui.style()).show(ui, |ui| ui.vertical(|ui| {
                        self.meta_box(ui, g, file.as_ref().map(|(_, f)| f), meta.as_ref(), &note, &mut close);
                    }));
                }
            });
        });
        self.meta_height = self.meta_height.max(boxes.content_size.y);
        if let Some(g) = close {
            self.show_meta(g, false);
        }
    }

    /// Title bar of the metadata panel.
    pub(crate) fn meta_title_bar(&mut self, ui: &mut Ui) {
        // Title bar: a click anywhere, or on the button on the right, collapses and reopens the
        // panel. Dragging its edge was useless: it always went back to the content's height.
        let collapsed = self.settings.meta_collapsed;
        let (rect, bar) = ui.allocate_exact_size(vec2(ui.available_width(), ui.spacing().interact_size.y), Sense::click());
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, self.i18n.t("meta-title"), font, ui.visuals().strong_text_color());
        let button_rect = Rect::from_min_max(pos2(rect.right() - 26.0, rect.top()), rect.max);
        let hint = self.i18n.t(if collapsed { "meta-expand" } else { "meta-collapse" });
        let button = ui.put(button_rect, egui::Button::new(if collapsed { "▲" } else { "▼" }).small()).on_hover_text(&hint);
        let bar = bar.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand);
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
                if ui.small_button("✖").on_hover_text(t("meta-close")).clicked() {
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
                ui.weak(t(label));
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
        let field = |label: &str| meta.and_then(|m| m.fields.iter().find(|(_, l, _)| *l == label).map(|(_, _, v)| v.clone()));
        let pixels = field("meta-width").zip(field("meta-height")).map(|(w, h)| format!("{} × {h}", w.trim_end_matches(" px")));
        let size = format!("{} ({} {})", self.human_size(f.size), grouped(f.size, &t("thousands-separator")), t("meta-bytes"));
        let rows = [
            ("meta-file-name", Some(source::name(&f.path).trim_end_matches('/').to_string())),
            ("meta-file-size", Some(size)),
            ("meta-file-date", self.file_date(&f.modified)),
            ("meta-pixels", pixels),
        ];
        egui::Grid::new(("meta", "file")).num_columns(2).show(ui, |ui| {
            for (label, value) in rows {
                if let Some(v) = value {
                    ui.weak(t(label));
                    ui.add(egui::Label::new(v).truncate());
                    ui.end_row();
                }
            }
        });
    }

    /// Status bar of the viewer, like the grid's: name, position, pixels, size on disk, date
    /// taken (or modified, without EXIF or with the metadata boxes closed).
    /// `Some`: the Keys button, shown for 3D models and CAD drawings (`keys_window`).
    pub(crate) fn viewer_status(&self, ui: &mut Ui, current: usize) -> Option<egui::Response> {
        let files = self.files();
        let f = files.get(current)?;
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.horizontal(|ui| {
            self.phone_waiting(ui);
            // The same note as the grid's bar: a file gone from the device, say.
            if let Some(note) = &self.status_note {
                ui.label(crate::theme::alert(note.clone()));
                ui.separator();
            }
            ui.label(source::shown(&f.path));
            ui.separator();
            ui.label(format!("{} / {}", current + 1, files.len()));
            // shortcut: images beyond 8192 px show the size of their scaled-down texture; the decoder would have to report the real one.
            if let Some((_, mesh)) = self.viewed_mesh() {
                ui.separator();
                ui.label(self.i18n.t_args("model-triangles", &[("n", &mesh.tris.len().to_string())]));
                if let Some(fps) = self.model_fps() {
                    ui.separator();
                    ui.label(format!("{fps} fps"));
                }
                ui.separator();
                ui.label(format!("{:.0}%", self.zoom_shown * 100.0));
            } else if crate::cad::is_cad(&f.path) {
                // A drawing has no pixels: its own size, in its unit (user, 2026-11-02).
                for text in [self.cad_size_text(current), self.cad_sheet_name(current)].into_iter().flatten() {
                    ui.separator();
                    ui.label(text);
                }
                ui.separator();
                ui.label(format!("{:.0}%", self.zoom_shown * 100.0));
            } else if let Some(Ok(tex)) = self.full.get(&current) {
                let [w, h] = tex.size();
                let (w, h) = if self.turn.sideways() { (h, w) } else { (w, h) };
                ui.separator();
                ui.label(self.i18n.t_args("convert-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]));
                ui.separator();
                ui.label(format!("{:.0}%", self.zoom_shown * 100.0));
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
            // On the right, out of the way: the keys, listed when asked (user, 2026-11-02). Blue on
            // white: the bar makes every text white, and the button was white on white.
            let keys = egui::Button::new(egui::RichText::new(self.i18n.t("keys-button")).color(crate::theme::solid(ui.visuals()))).fill(Color32::WHITE);
            self.keys_hint(current).map(|_| ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add(keys)).inner)
        })
        .inner
    }

    /// The keys of the viewer, one a row, over its bottom right corner above the status bar
    /// (`bar`: its height): in place of a band whose letters got smaller to fit (user, 2026-11-02).
    pub(crate) fn keys_window(&self, ctx: &egui::Context, id: &str, bar: f32) {
        let text = self.i18n.t(id);
        egui::Area::new(egui::Id::unique("viewer-keys")).order(egui::Order::Foreground).anchor(Align2::RIGHT_BOTTOM, vec2(-8.0, -bar - 8.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new(self.i18n.t("keys-button")).strong().size(16.0));
                ui.separator();
                // A row for each key, in alternating stripes, so a key stays with what it does.
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

    /// The keys of 3D models and CAD drawings, which differ from those of images: the Fluent id
    /// of their list (`keys_window`), if the file is one of them.
    pub(crate) fn keys_hint(&self, current: usize) -> Option<&'static str> {
        if self.viewed_mesh().is_some() {
            return Some("hint-model");
        }
        let cad = self.files().get(current).is_some_and(|f| crate::cad::is_cad(&f.path));
        cad.then(|| if self.cad_has_sheets(current) { "hint-cad-layouts" } else { "hint-cad" })
    }

    /// Value of a metadata field as shown: Fluent ids translated, dates in the language's format.
    pub(crate) fn meta_value(&self, label: &str, value: &str) -> String {
        if value.starts_with("meta-") {
            return self.i18n.t(value);
        }
        let formatted = match label {
            "meta-taken" => value.parse::<jiff::civil::DateTime>().ok().map(|d| d.strftime(&self.i18n.t("date-format")).to_string()),
            "meta-gps-date" => value.parse::<jiff::civil::Date>().ok().map(|d| d.strftime(&self.i18n.t("date-day-format")).to_string()),
            _ => None,
        };
        formatted.unwrap_or_else(|| value.to_string())
    }
}

/// "Wheel: zoom | Drag: move" → ("Wheel", "zoom"), ("Drag", "move"): the keys and what they do,
/// as the `hint-*` texts write them.
fn key_rows(text: &str) -> Vec<(&str, &str)> {
    text.split('|').filter_map(|part| part.split_once(':')).map(|(k, w)| (k.trim(), w.trim())).collect()
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

    #[test]
    fn digits_in_groups() {
        assert_eq!(super::grouped(245464190, "."), "245.464.190");
        assert_eq!(super::grouped(999, "."), "999");
        assert_eq!(super::grouped(1000, " "), "1 000");
    }
}
