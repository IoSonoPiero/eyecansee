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
                        self.meta_box(ui, g, meta.as_ref(), &note, &mut close);
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
    pub(crate) fn meta_box(&self, ui: &mut Ui, g: Group, meta: Option<&meta::Meta>, note: &str, close: &mut Option<Group>) {
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

    /// Status bar of the viewer, like the grid's: name, position, pixels, size on disk, date
    /// taken (or modified, without EXIF or with the metadata boxes closed).
    pub(crate) fn viewer_status(&self, ui: &mut Ui, current: usize) {
        let files = self.files();
        let Some(f) = files.get(current) else { return };
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.horizontal(|ui| {
            ui.label(source::shown(&f.path));
            ui.separator();
            ui.label(format!("{} / {}", current + 1, files.len()));
            // shortcut: images beyond 8192 px show the size of their scaled-down texture; the decoder would have to report the real one.
            if let Some(Ok(tex)) = self.full.get(&current) {
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
        });
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
