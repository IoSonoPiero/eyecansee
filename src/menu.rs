//! Menu bar, toolbar and status bar.

use std::cell::Cell;

use eframe::egui::{self, Color32, Key};

use crate::navigation::Go;
use crate::actions::{Act, transform_items};
use crate::{App, icons, meta, settings, source, transform, version};
use crate::loader::THUMB_SIDE;

impl App {
    /// Every keyboard or double-click action also has a menu item, with the shortcut next to it.
    /// The menus only record the chosen item; the action runs afterwards, with access to the whole app.
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        // A Cell: the plain items, the radio buttons and the check boxes all record into it.
        let act = Cell::new(None);
        // As in most programs: XnView MP (Edit, Tools with the settings), Explorer and Dolphin
        // (Rename and Delete in File); no Go menu, as image viewers.
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(self.i18n.t("menu-file"), |ui| self.file_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-edit"), |ui| self.edit_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-image"), |ui| self.image_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-view"), |ui| self.view_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-tools"), |ui| self.tools_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-help"), |ui| self.help_menu(ui, &act));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("EyeCanSee {}", version()));
                if let Some(r) = &self.update {
                    // Red with white text (contrast 5.6:1): as a plain button it went unnoticed.
                    let text = egui::RichText::new(self.i18n.t_args("update-install", &[("version", &r.version)])).color(Color32::WHITE).strong();
                    pick(&act, ui.add(egui::Button::new(text).fill(Color32::from_rgb(0xC6, 0x28, 0x28))), Act::Update);
                }
                self.engine_bar(ui);
            });
        });
        if let Some(a) = act.take() {
            self.run(ui.ctx(), a);
        }
    }

    fn file_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, true, egui::Button::new(t("menu-open-location")), Act::Address);
        entry(ui, act, self.can_make_folder(), item(t("menu-new-folder"), t("key-new-folder")), Act::NewFolder(None));
        ui.separator();
        entry(ui, act, self.can_save(), item(t("menu-save"), t("key-save")), Act::Save);
        entry(ui, act, self.viewing_image(), item(t("menu-save-copy"), t("key-save-copy")), Act::Convert(None));
        ui.separator();
        let can_change = self.target().is_some();
        entry(ui, act, can_change, item(t("menu-rename"), "F2".into()), Act::Rename(None));
        entry(ui, act, can_change, item(t("menu-delete"), t("key-delete")), Act::Delete(None, false));
        entry(ui, act, can_change, item(t("menu-delete-permanent"), t("key-shift-delete")), Act::Delete(None, true));
        ui.separator();
        entry(ui, act, true, egui::Button::new(t("menu-quit")), Act::Quit);
    }

    fn edit_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        let can_clip = !self.chosen(None).is_empty();
        entry(ui, act, can_clip, item(t("menu-cut"), t("key-cut")), Act::Cut(None));
        entry(ui, act, can_clip, item(t("menu-copy"), t("key-copy")), Act::Copy(None));
        entry(ui, act, !viewing && self.can_paste(&self.dir), item(t("menu-paste"), t("key-paste")), Act::Paste(None));
        ui.separator();
        entry(ui, act, !viewing && self.items() > 0, item(t("menu-select-all"), t("key-select-all")), Act::SelectAll);
    }

    /// The flips and turns straight in the menu (in the right-click menu they are under
    /// Transform), then Rotate and crop.
    fn image_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        let some = self.viewing_image() || !self.grid_images(None).is_empty();
        for (op, label, keys) in transform_items() {
            entry(ui, act, some, item(t(label), keys.map(t).unwrap_or_default()), Act::Transform(op, None));
            if op == transform::Op::FlipV {
                ui.separator();
            }
        }
        ui.separator();
        let one = self.chosen(None).len() <= 1;
        let r = ui.add_enabled(some && one && self.target().is_some_and(|p| !p.ends_with('/')), egui::Button::new(t("menu-rotate-crop")));
        pick(act, if one { r } else { r.on_disabled_hover_text(t("rc-one-only")) }, Act::RotateCrop(None));
    }

    fn view_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        let has_items = self.items() > 0;
        entry(ui, act, !viewing && has_items && self.cursor, item(t("menu-open-selected"), t("key-enter")), Act::Open);
        entry(ui, act, viewing, item(t("menu-back-to-grid"), "Esc".into()), Act::Back);
        entry(ui, act, !self.files().is_empty(), egui::Button::new(t("menu-slideshow")), Act::Slideshow(None));
        ui.separator();
        // Moving around (once a menu of its own, "Go", with only these).
        entry(ui, act, has_items, item(t("menu-next"), "→".into()), Act::Go(Go::Next));
        entry(ui, act, has_items, item(t("menu-prev"), "←".into()), Act::Go(Go::Prev));
        entry(ui, act, has_items, item(t("menu-first"), t("key-home")), Act::Go(Go::First));
        entry(ui, act, has_items, item(t("menu-last"), t("key-end")), Act::Go(Go::Last));
        entry(ui, act, !viewing && !self.dir.is_empty(), item(t("menu-parent"), t("key-backspace")), Act::Up);
        ui.separator();
        entry(ui, act, true, item(t("menu-refresh"), "F5".into()), Act::Refresh);
        entry(ui, act, self.chosen(None).iter().any(|p| !p.ends_with('/')), egui::Button::new(t("menu-redo-thumbs")), Act::RedoThumbs(None));
        entry(ui, act, !viewing && has_items, item(t("menu-redo-all-thumbs"), t("key-shift-f5")), Act::RedoAllThumbs);
        ui.separator();
        ui.menu_button(t("sort-by"), |ui| {
            for by in settings::SortBy::ALL {
                pick(act, ui.radio(self.settings.sort_by == by, t(by.label())), Act::Sort(by));
            }
            ui.separator();
            pick(act, ui.checkbox(&mut self.settings.sort_desc.clone(), t("sort-desc")), Act::SortDesc);
        });
        ui.menu_button(t("show-menu"), |ui| {
            for show in settings::Show::ALL {
                pick(act, ui.radio(self.settings.show == show, t(show.label())), Act::Show(show));
            }
        });
        ui.add_enabled_ui(!viewing, |ui| {
            ui.menu_button(t("menu-meta"), |ui| {
                for g in meta::Group::ALL {
                    let label = self.i18n.t(&format!("meta-{}", g.key()));
                    pick(act, ui.checkbox(&mut self.meta_shown(g), label), Act::Meta(g));
                }
            });
        });
    }

    /// Work on many images at once, and the settings.
    fn tools_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        // One conversion at a time: the entries wait for the one running.
        let free = !viewing && self.batch.is_none();
        entry(ui, act, free && !self.grid_items(None).is_empty(), egui::Button::new(t("menu-batch")), Act::Batch(None));
        entry(ui, act, free && self.grid_items(None).iter().any(|p| crate::decode::is_video(p)), egui::Button::new(t("menu-copy-meta")), Act::CopyMeta(None));
        entry(ui, act, !viewing && self.loader.is_some() && !source::is_trash(&self.dir), egui::Button::new(t("menu-duplicates")), Act::Duplicates(None));
        ui.separator();
        entry(ui, act, true, egui::Button::new(t("menu-options")), Act::Options);
    }

    fn help_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, true, item(t("menu-manual"), "F1".into()), Act::Manual);
        ui.separator();
        entry(ui, act, true, egui::Button::new(t("menu-check-updates")), Act::CheckUpdates);
        entry(ui, act, true, egui::Button::new(t("menu-about")), Act::About);
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
            if self.viewing_image() {
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
            if let Some((name, secs)) = crate::mtp::waiting(3) {
                ui.add(egui::Spinner::new().color(Color32::WHITE));
                ui.label(crate::theme::alert(self.i18n.t_args("status-phone-waiting", &[("s", &secs.to_string()), ("name", &name)])));
                ui.separator();
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            if self.deleting > 0 {
                ui.add(egui::Spinner::new().color(Color32::WHITE));
                ui.label(crate::theme::alert(self.i18n.t_args("status-deleting", &[("n", &self.deleting.to_string())])));
                ui.separator();
            }
            if !self.saving.is_empty() {
                ui.add(egui::Spinner::new().color(Color32::WHITE));
                ui.label(crate::theme::alert(self.i18n.t_args("status-saving-n", &[("n", &self.saving.len().to_string())])));
                ui.separator();
            }
            if let Some(note) = &self.status_note {
                ui.label(crate::theme::alert(note.clone()));
                ui.separator();
            }
            if let Some(clip) = &self.clip {
                let (icon, id) = if clip.cut { (icons::Icon::Cut, "status-cut") } else { (icons::Icon::Copied, "status-copied") };
                ui.add(icons::image(ui.ctx(), icon, 16.0));
                ui.label(self.i18n.t_args(id, &[("n", &clip.paths.len().to_string())]));
                ui.separator();
            }
            ui.label(self.i18n.t_args("status-folders", &[("n", &l.dirs.len().to_string())]));
            let videos = l.files.iter().filter(|f| crate::decode::is_video(&f.path)).count();
            if videos < l.files.len() {
                ui.label(self.i18n.t_args("status-images", &[("n", &(l.files.len() - videos).to_string())]));
            }
            if videos > 0 {
                ui.label(self.i18n.t_args("status-videos", &[("n", &videos.to_string())]));
            }
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

/// A menu item with its shortcut on the right.
fn item(label: String, keys: String) -> egui::Button<'static> {
    egui::Button::new(label).shortcut_text(keys)
}

/// Records the item chosen; it runs after the menus are drawn.
fn pick(act: &Cell<Option<Act>>, r: egui::Response, a: Act) {
    if r.clicked() {
        act.set(Some(a));
    }
}

fn entry(ui: &mut egui::Ui, act: &Cell<Option<Act>>, enabled: bool, button: egui::Button, a: Act) {
    pick(act, ui.add_enabled(enabled, button), a);
}
