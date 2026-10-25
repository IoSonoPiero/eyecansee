//! Menu bar, toolbar and status bar.

use eframe::egui::{self, Color32, Key};

use crate::navigation::Go;
use crate::actions::{Act, transform_items};
use crate::{App, icons, meta, settings, source, transform, version};
use crate::loader::THUMB_SIDE;

impl App {
    /// Every keyboard or double-click action also has a menu item, with the shortcut next to it.
    /// The menus only record the chosen item; the action runs afterwards, with access to the whole app.
    #[allow(clippy::too_many_lines)] // to be split
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let t = |id| self.i18n.t(id);
        let item = |label: String, keys: String| egui::Button::new(label).shortcut_text(keys);
        let viewing = self.viewing.is_some();
        let has_items = self.items() > 0;
        let can_go_up = !viewing && !self.dir.is_empty();
        // A Cell: the plain items, the radio buttons and the check boxes all record into it.
        let act = std::cell::Cell::new(None);
        let pick = |r: egui::Response, a: Act| {
            if r.clicked() {
                act.set(Some(a));
            }
        };
        egui::MenuBar::new().ui(ui, |ui| {
            let entry = |ui: &mut egui::Ui, enabled: bool, button: egui::Button, a: Act| pick(ui.add_enabled(enabled, button), a);
            ui.menu_button(t("menu-file"), |ui| {
                entry(ui, true, egui::Button::new(t("menu-open-location")), Act::Address);
                entry(ui, true, egui::Button::new(t("menu-options")), Act::Options);
                ui.separator();
                let can_clip = !self.chosen(None).is_empty();
                entry(ui, can_clip, item(t("menu-cut"), t("key-cut")), Act::Cut(None));
                entry(ui, can_clip, item(t("menu-copy"), t("key-copy")), Act::Copy(None));
                entry(ui, !viewing && self.can_paste(&self.dir), item(t("menu-paste"), t("key-paste")), Act::Paste(None));
                ui.separator();
                entry(ui, self.can_save(), item(t("menu-save"), t("key-save")), Act::Save);
                entry(ui, viewing, item(t("menu-save-copy"), t("key-save-copy")), Act::Convert(None));
                ui.separator();
                let can_change = self.target().is_some();
                entry(ui, can_change, item(t("menu-rename"), "F2".into()), Act::Rename(None));
                entry(ui, can_change, item(t("menu-delete"), t("key-delete")), Act::Delete(None, false));
                entry(ui, can_change, item(t("menu-delete-permanent"), t("key-shift-delete")), Act::Delete(None, true));
                ui.separator();
                entry(ui, true, egui::Button::new(t("menu-quit")), Act::Quit);
            });
            ui.menu_button(t("menu-image"), |ui| {
                ui.add_enabled_ui(viewing || !self.grid_images(None).is_empty(), |ui| {
                    ui.menu_button(t("menu-transform"), |ui| {
                        for (op, label, keys) in transform_items() {
                            entry(ui, true, item(t(label), keys.map(t).unwrap_or_default()), Act::Transform(op, None));
                            if op == transform::Op::FlipV {
                                ui.separator();
                            }
                        }
                    });
                });
                ui.separator();
                entry(ui, !viewing && self.loader.is_some() && !source::is_trash(&self.dir), egui::Button::new(t("menu-duplicates")), Act::Duplicates(None));
            });
            ui.menu_button(t("menu-view"), |ui| {
                entry(ui, !viewing && has_items && self.cursor, item(t("menu-open-selected"), t("key-enter")), Act::Open);
                entry(ui, viewing, item(t("menu-back-to-grid"), "Esc".into()), Act::Back);
                ui.separator();
                entry(ui, true, item(t("menu-refresh"), "F5".into()), Act::Refresh);
                entry(ui, self.chosen(None).iter().any(|p| !p.ends_with('/')), egui::Button::new(t("menu-redo-thumbs")), Act::RedoThumbs(None));
                entry(ui, !viewing && has_items, item(t("menu-redo-all-thumbs"), t("key-shift-f5")), Act::RedoAllThumbs);
                ui.separator();
                ui.menu_button(t("sort-by"), |ui| {
                    for by in settings::SortBy::ALL {
                        pick(ui.radio(self.settings.sort_by == by, t(by.label())), Act::Sort(by));
                    }
                    ui.separator();
                    pick(ui.checkbox(&mut self.settings.sort_desc.clone(), t("sort-desc")), Act::SortDesc);
                });
                ui.add_enabled_ui(!viewing, |ui| {
                    ui.menu_button(t("menu-meta"), |ui| {
                        for g in meta::Group::ALL {
                            let label = self.i18n.t(&format!("meta-{}", g.key()));
                            pick(ui.checkbox(&mut self.meta_shown(g), label), Act::Meta(g));
                        }
                    });
                });
            });
            ui.menu_button(t("menu-go"), |ui| {
                entry(ui, has_items, item(t("menu-next"), "→".into()), Act::Go(Go::Next));
                entry(ui, has_items, item(t("menu-prev"), "←".into()), Act::Go(Go::Prev));
                entry(ui, has_items, item(t("menu-first"), t("key-home")), Act::Go(Go::First));
                entry(ui, has_items, item(t("menu-last"), t("key-end")), Act::Go(Go::Last));
                ui.separator();
                entry(ui, can_go_up, item(t("menu-parent"), t("key-backspace")), Act::Up);
            });
            ui.menu_button(t("menu-help"), |ui| {
                entry(ui, true, item(t("menu-manual"), "F1".into()), Act::Manual);
                ui.separator();
                entry(ui, true, egui::Button::new(t("menu-check-updates")), Act::CheckUpdates);
                entry(ui, true, egui::Button::new(t("menu-about")), Act::About);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("EyeCanSee {}", version()));
                if let Some(r) = &self.update {
                    // Red with white text (contrast 5.6:1): as a plain button it went unnoticed.
                    let text = egui::RichText::new(self.i18n.t_args("update-install", &[("version", &r.version)])).color(Color32::WHITE).strong();
                    pick(ui.add(egui::Button::new(text).fill(Color32::from_rgb(0xC6, 0x28, 0x28))), Act::Update);
                }
            });
        });
        if let Some(a) = act.take() {
            self.run(ui.ctx(), a);
        }
    }

    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Coloured icons in square buttons with a border (style "B").
        let icon = |icon| egui::Button::image(icons::image(ctx, icon, 18.0)).min_size(egui::vec2(32.0, 28.0));
        ui.horizontal(|ui| {
            if self.viewing.is_some() {
                if ui.add(icon(icons::Icon::Grid)).on_hover_text(self.i18n.t("menu-back-to-grid")).clicked() {
                    self.close_viewer();
                }
            } else if ui
                .add_enabled(!self.dir.is_empty(), icon(icons::Icon::UpButton))
                .on_hover_text(self.i18n.t("tb-up"))
                .clicked()
            {
                self.go_up();
            }
            if ui.add(icon(icons::Icon::Refresh)).on_hover_text(self.i18n.t("menu-refresh")).clicked() {
                self.refresh();
            }
            if self.viewing.is_some() {
                for (i, op, id) in [(icons::Icon::RotateCcw, transform::Op::Ccw, "tf-rotate-ccw"), (icons::Icon::RotateCw, transform::Op::Cw, "tf-rotate-cw")] {
                    if ui.add(icon(i)).on_hover_text(self.i18n.t(id)).clicked() {
                        self.act = Some(Act::Transform(op, None));
                    }
                }
            }
            // Metadata: closes all the boxes if one is open, otherwise reopens them all.
            if self.viewing.is_none() {
                let any = meta::Group::ALL.into_iter().any(|g| self.meta_shown(g));
                let button = egui::Button::selectable(any, icons::image(ctx, icons::Icon::Info, 18.0)).min_size(egui::vec2(32.0, 28.0));
                if ui.add(button).on_hover_text(self.i18n.t("tb-meta")).clicked() {
                    for g in meta::Group::ALL {
                        self.show_meta(g, !any);
                    }
                }
            }
            // Space to the right of the address: sorting (drop-down and direction) and thumbnail slider.
            let slider_width = if self.viewing.is_none() { 370.0 } else { 0.0 };
            let address = egui::TextEdit::singleline(&mut self.address)
                .id(egui::Id::unique("address"))
                .hint_text(self.i18n.t("tb-address-hint"))
                .desired_width(ui.available_width() - slider_width);
            let response = ui.add(address);
            if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let address = self.address.clone();
                self.open_source(ctx, &address);
            }
            if self.viewing.is_none() {
                let (mut by, desc) = (self.settings.sort_by, self.settings.sort_desc);
                egui::ComboBox::from_id_salt("sort")
                    .width(150.0)
                    .selected_text(self.i18n.t(by.label()))
                    .show_ui(ui, |ui| {
                        for b in settings::SortBy::ALL {
                            ui.selectable_value(&mut by, b, self.i18n.t(b.label()));
                        }
                    })
                    .response
                    .on_hover_text(self.i18n.t("sort-by"));
                let arrow = if desc { "▼" } else { "▲" };
                let flip = ui.button(arrow).on_hover_text(self.i18n.t(if desc { "sort-desc" } else { "sort-asc" })).clicked();
                if by != self.settings.sort_by || flip {
                    self.set_sort(by, desc != flip);
                }
                let hint = self.i18n.t("tb-thumb-size");
                ui.label("🖼").on_hover_text(&hint);
                ui.add(egui::Slider::new(&mut self.thumb_size, 64.0..=THUMB_SIDE as f32).show_value(false))
                    .on_hover_text(hint);
            }
        });
    }

    /// How many folders and images there are, and name and size of the selected item.
    /// On the solid accent colour (style "B"), all in white.
    pub(crate) fn status_bar(&self, ui: &mut egui::Ui) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.horizontal(|ui| {
            if self.deleting > 0 {
                ui.spinner();
                ui.label(self.i18n.t_args("status-deleting", &[("n", &self.deleting.to_string())]));
                ui.separator();
            }
            if !self.saving.is_empty() {
                ui.spinner();
                ui.label(self.i18n.t_args("status-saving-n", &[("n", &self.saving.len().to_string())]));
                ui.separator();
            }
            if let Some(clip) = &self.clip {
                let (icon, id) = if clip.cut { (icons::Icon::Cut, "status-cut") } else { (icons::Icon::Copied, "status-copied") };
                ui.add(icons::image(ui.ctx(), icon, 16.0));
                ui.label(self.i18n.t_args(id, &[("n", &clip.paths.len().to_string())]));
                ui.separator();
            }
            ui.label(self.i18n.t_args("status-folders", &[("n", &l.dirs.len().to_string())]));
            ui.label(self.i18n.t_args("status-images", &[("n", &l.files.len().to_string())]));
            let marked = self.marked_paths();
            if !marked.is_empty() {
                ui.separator();
                let size = self.human_size(self.images_size(&marked));
                ui.label(self.i18n.t_args("status-marked", &[("n", &marked.len().to_string()), ("size", &size)]));
                return;
            }
            if !self.cursor {
                return;
            }
            ui.separator();
            match self.selected.checked_sub(l.dirs.len()).and_then(|i| l.files.get(i)) {
                Some(f) => {
                    ui.label(source::name(&f.path));
                    ui.label(self.human_size(f.size));
                    if let Some(date) = self.file_date(&f.modified) {
                        ui.label(date);
                    }
                }
                None => {
                    ui.label(l.dirs.get(self.selected).map_or("", |d| source::name(d)));
                }
            }
        });
    }
}
