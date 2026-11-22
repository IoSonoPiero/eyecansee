//! Menu bar, toolbar and status bar.

use std::cell::Cell;

use eframe::egui::{self, Color32};

use crate::navigation::Go;
use crate::actions::{Act, transform_items};
use crate::{App, icons, meta, settings, source, transform, version};
use crate::selection::{Kind, SAVED_AS, fits, none_of, starred};

/// The address bar's field: F1 while typing in it opens the page on addresses (`help_spot`).
pub(crate) fn address_id() -> egui::Id {
    egui::Id::unique("address")
}

impl App {
    /// Every keyboard or double-click action also has a menu item, with the shortcut next to it.
    /// The menus only record the chosen item; the action runs afterwards, with access to the whole app.
    pub(crate) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        // A Cell: the plain items, the radio buttons and the check boxes all record into it.
        let act = Cell::new(None);
        // As in most programs: XnView MP (Edit, Tools with the settings), Explorer and Dolphin
        // (Rename and Delete in File); no Go menu, as image viewers; Bookmarks as the browsers.
        // On more rows in a narrow window (`bar_rows`): Help was cut at 400 px.
        egui::MenuBar::new().ui(ui, |ui| crate::bar_rows::rows(ui, |ui| {
            ui.menu_button(self.i18n.t("menu-file"), |ui| self.file_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-edit"), |ui| self.edit_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-image"), |ui| self.image_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-view"), |ui| self.view_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-bookmarks"), |ui| {
                if let Some(a) = self.bookmark_items(ui, true) {
                    act.set(Some(a));
                }
            });
            ui.menu_button(self.i18n.t("menu-tools"), |ui| self.tools_menu(ui, &act));
            ui.menu_button(self.i18n.t("menu-help"), |ui| self.help_menu(ui, &act));
            crate::bar_rows::right_end(ui, "menu", |ui| {
                // Left out of the guide's pictures, which it would date.
                #[cfg(feature = "shots")]
                let version_shown = !crate::shots::active();
                #[cfg(not(feature = "shots"))]
                let version_shown = true;
                // Only where it fits: in a narrow window it was drawn over the Help menu.
                let text = format!("EyeCanSee {}", version());
                let font = egui::TextStyle::Body.resolve(ui.style());
                let width = ui.painter().layout_no_wrap(text.clone(), font, ui.visuals().weak_text_color()).size().x;
                // Not counted in what the right end needs: shown only where it fits.
                let mut version_width = 0.0;
                if version_shown && ui.available_width() >= width + ui.spacing().item_spacing.x {
                    version_width = ui.weak(text).rect.width() + ui.spacing().item_spacing.x;
                }
                if let Some(r) = &self.update {
                    // Red: as a plain button it went unnoticed.
                    let id = if r.in_store() { "update-in-store" } else { "update-install" };
                    let text = self.i18n.t_args(id, &[("version", &r.version)]);
                    pick(&act, ui.add(crate::theme::alert_button(text)), Act::Update);
                }
                self.engine_bar(ui);
                self.cad_engine_bar(ui);
                ((), (ui.min_rect().width() - version_width).max(0.0))
            });
        }));
        if let Some(a) = act.take() {
            self.run(ui.ctx(), a);
        }
    }

    fn file_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, None, item(t("menu-open-location"), String::new()), Act::Address);
        entry(ui, act, None, item(t("menu-servers"), String::new()), Act::Servers);
        entry(ui, act, self.new_folder_off(), item(t("menu-new-folder"), t("key-new-folder")), Act::NewFolder(None));
        entry(ui, act, self.new_image_off(), item(t("menu-new-image"), t("key-new-image")), Act::NewImage);
        entry(ui, act, self.new_image_off(), item(t("menu-open-clipboard"), String::new()), Act::OpenClipboard);
        ui.separator();
        // Also when it cannot overwrite (FTP, gvfs, format): `ask_save` then says why and offers Save as….
        entry(ui, act, self.save_off(), item(t("menu-save"), t("key-save")), Act::Save);
        // As in the right-click menu: in the grid one picture, in the viewer also what a 3D model
        // or a CAD drawing shows.
        let files = self.files_chosen(None);
        // Ctrl+Shift+S works in the viewer only.
        let keys = if self.viewing.is_some() { t("key-save-copy") } else { String::new() };
        let saved = if self.viewing.is_some() { Some(true) } else { fits(&files, SAVED_AS) };
        entry(ui, act, self.save_as_off(), item(starred(t(self.save_as_label()), saved != Some(false)), keys), Act::Convert(None));
        // As in the right-click menus: the 3D models and the CAD drawings among the files chosen,
        // each with its window (issue #128).
        let model = fits(&files, &[Kind::Model]);
        let model_off = model.is_none().then(|| t(none_of(&[Kind::Model])));
        entry(ui, act, model_off, item(starred(t("menu-export-3d"), model != Some(false)), String::new()), Act::ExportModel(None));
        let drawing = fits(&files, &[Kind::Drawing]);
        let drawing_off = self.drawings_off(drawing);
        entry(ui, act, drawing_off, item(starred(t("menu-export-drawings"), drawing != Some(false)), String::new()), Act::ExportDrawings(None));
        entry(ui, act, self.compare_off(&files), item(t("menu-compare"), String::new()), Act::Compare);
        partial_note(ui, [model, drawing, saved].contains(&Some(false)), &self.i18n);
        ui.separator();
        self.restore_item(ui, act);
        entry(ui, act, self.rename_off(), item(t("menu-rename"), "F2".into()), Act::Rename(None));
        entry(ui, act, self.delete_off(), item(t("menu-delete"), t("key-delete")), Act::Delete(None, false));
        entry(ui, act, self.delete_off(), item(t("menu-delete-permanent"), t("key-shift-delete")), Act::Delete(None, true));
        ui.separator();
        entry(ui, act, None, item(t("menu-quit"), String::new()), Act::Quit);
    }

    fn edit_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, self.clip_off(), item(t("menu-cut"), t("key-cut")), Act::Cut(None));
        entry(ui, act, self.clip_off(), item(t("menu-copy"), t("key-copy")), Act::Copy(None));
        entry(ui, act, self.paste_off(), item(t("menu-paste"), t("key-paste")), Act::Paste(None));
        self.restore_item(ui, act);
        ui.separator();
        entry(ui, act, self.grid_off(), item(t("menu-select-all"), t("key-select-all")), Act::SelectAll);
    }

    /// Inside a trash, Restore (Restore all on the trash folder), as in the right-click menu (issue #126).
    fn restore_item(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        if let Some((label, off)) = self.restore_entry() {
            entry(ui, act, off, item(label, String::new()), Act::Restore(None));
        }
    }

    /// The flips and turns straight in the menu (in the right-click menu they are under
    /// Transform), then Rotate and crop.
    fn image_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        // In the grid the pictures among the files chosen ("*" with other kinds).
        let fit = self.turns_fit();
        let star = |label| starred(label, fit != Some(false));
        for (op, label, keys) in transform_items() {
            let label = starred(t(label), self.turn_fits(op) != Some(false));
            entry(ui, act, self.turn_op_off(op), item(label, keys.map(t).unwrap_or_default()), Act::Transform(op, None));
            if op == transform::Op::FlipV {
                ui.separator();
            }
        }
        ui.separator();
        let off = self.editor_off();
        for (label, keys, a) in [("menu-edit-image", t("key-edit"), Act::Edit(None)), ("menu-rotate-crop", String::new(), Act::RotateCrop(None))] {
            entry(ui, act, off.clone(), item(star(t(label)), keys), a);
        }
        partial_note(ui, fit == Some(false), &self.i18n);
    }

    fn view_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        entry(ui, act, self.open_off(), item(t("menu-open-selected"), t("key-enter")), Act::Open);
        entry(ui, act, self.back_off(), item(t("menu-back-to-grid"), "Esc".into()), Act::Back);
        // Off without pictures and videos: the rest stays out of the slideshow.
        entry(ui, act, self.slideshow_off(), item(t("menu-slideshow"), String::new()), Act::Slideshow(None));
        ui.separator();
        // Moving around (once a menu of its own, "Go", with only these).
        entry(ui, act, self.go_off(), item(t("menu-next"), "→".into()), Act::Go(Go::Next));
        entry(ui, act, self.go_off(), item(t("menu-prev"), "←".into()), Act::Go(Go::Prev));
        entry(ui, act, self.go_off(), item(t("menu-first"), t("key-home")), Act::Go(Go::First));
        entry(ui, act, self.go_off(), item(t("menu-last"), t("key-end")), Act::Go(Go::Last));
        entry(ui, act, self.parent_off(), item(t("menu-parent"), t("key-backspace")), Act::Up);
        ui.separator();
        entry(ui, act, None, item(t("menu-refresh"), "F5".into()), Act::Refresh);
        entry(ui, act, self.redo_thumbs_off(), item(t("menu-redo-thumbs"), String::new()), Act::RedoThumbs(None));
        entry(ui, act, self.grid_off(), item(t("menu-redo-all-thumbs"), t("key-shift-f5")), Act::RedoAllThumbs);
        ui.separator();
        ui.menu_button((icons::menu_icon(ui, None), t("sort-by")), |ui| {
            for by in settings::SortBy::ALL {
                pick(act, ui.radio(self.settings.sort_by == by, t(by.label())), Act::Sort(by));
            }
            ui.separator();
            pick(act, ui.checkbox(&mut self.settings.sort_desc.clone(), t("sort-desc")), Act::SortDesc);
        });
        ui.menu_button((icons::menu_icon(ui, None), t("show-menu")), |ui| {
            for show in settings::Show::ALL {
                pick(act, ui.radio(self.settings.show == show, t(show.label())), Act::Show(show));
            }
        });
        // A CAD drawing in the viewer: its layers panel.
        let tick = if self.cad_layers_shown() { "✔ " } else { "" };
        entry(ui, act, self.cad_layers_off(), item(format!("{tick}{}", t("menu-layers")), t("key-layers")), Act::Layers);
        let tick = if self.model.measure.on && self.measure_off().is_none() { "✔ " } else { "" };
        entry(ui, act, self.measure_off(), item(format!("{tick}{}", t("menu-measure")), "M".into()), Act::Measure);
        ui.add_enabled_ui(!viewing, |ui| {
            let r = ui.menu_button((icons::menu_icon(ui, None), t("menu-meta")), |ui| {
                for g in meta::Group::ALL {
                    let label = self.i18n.t(&format!("meta-{}", g.key()));
                    pick(act, ui.checkbox(&mut self.meta_shown(g), label), Act::Meta(g));
                }
            });
            r.response.on_disabled_hover_text(t("tb-off-viewer"));
        });
    }

    fn tools_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        // As in the right-click menu: the same entries, greyed out with why or with "*".
        let partial = Cell::new(false);
        if let Some(a) = crate::browser::context_menu(ui, self.tool_items(None, self.tools_off(), &partial)) {
            act.set(Some(a));
        }
        entry(ui, act, self.duplicates_off(), item(t("menu-duplicates"), t("key-duplicates")), Act::Duplicates(None));
        partial_note(ui, partial.get(), &self.i18n);
        ui.separator();
        // Windows only, in the copies that can have one (#210).
        if crate::desktop_icon::me().is_some() {
            entry(ui, act, None, item(t("menu-desktop-icon"), String::new()), Act::DesktopIcon);
        }
        entry(ui, act, None, item(t("menu-options"), String::new()), Act::Options);
    }

    fn help_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, None, item(t("menu-manual"), "F1".into()), Act::Manual);
        ui.separator();
        entry(ui, act, None, item(t("menu-check-updates"), String::new()), Act::CheckUpdates);
        entry(ui, act, None, item(t("menu-about"), String::new()), Act::About);
    }

    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Line icons in square buttons with a border (Lucide).
        let mut below = None;
        ui.horizontal(|ui| {
            // What does not fit goes into the "More" menu (`toolbar::fit`).
            let fit = self.toolbar_fit(ui);
            if self.viewing.is_some() {
                if ui.add(icons::button(ui, icons::Line::Grid)).on_hover_text(self.i18n.t("menu-back-to-grid")).clicked() {
                    self.close_viewer();
                }
            } else if ui
                .add_enabled(!self.dir.is_empty(), icons::button(ui, icons::Line::Up))
                .on_hover_text(self.i18n.t("tb-up"))
                .clicked()
            {
                self.go_up();
            }
            // Not in the viewer: the image shown is read again by itself when it changes (F5 too).
            if fit.refresh() && ui.add(icons::button(ui, icons::Line::Refresh)).on_hover_text(self.i18n.t("menu-refresh")).clicked() {
                self.refresh();
            }
            self.toolbar_ops(ui, fit);
            // Metadata: closes all the boxes if one is open, otherwise reopens them all.
            if fit.meta() && ui.add(icons::toggle(ui, self.any_meta_shown(), icons::Line::Info)).on_hover_text(self.i18n.t("tb-meta")).clicked() {
                self.toggle_all_meta();
            }
            self.bookmark_button(ui, fit.star());
            below = self.address_field(ui, ctx);
        });
        if let Some(tail) = below {
            self.address_tail_row(ui, tail);
        }
    }

    /// A call the phone has not answered for 3 s, in the status bars of the grid and the viewer.
    /// On Windows after `mtp::STALL_SECS` it is taken for WPD's stall: a short line here, what
    /// to do in the window of `phone_stall`.
    pub(crate) fn phone_waiting(&self, ui: &mut egui::Ui) {
        let Some((name, secs, _)) = crate::mtp::waiting(3) else { return };
        let id = match () {
            _ if cfg!(windows) && secs >= crate::mtp::STALL_SECS => "status-phone-stalled",
            _ if name.is_empty() => "status-phone-waiting-open",
            _ => "status-phone-waiting",
        };
        ui.add(egui::Spinner::new().color(Color32::WHITE));
        ui.label(crate::theme::alert(self.i18n.t_args(id, &[("s", &secs.to_string()), ("name", &name)])));
        ui.separator();
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }

    /// How many folders and images there are, and name and size of the selected item.
    /// On the solid accent colour (style "B"), all in white.
    pub(crate) fn status_bar(&self, ui: &mut egui::Ui) {
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        // A stalled phone never sends the listing: the warning is all the bar has then.
        let Some(Ok(l)) = self.listings.get(&self.dir) else {
            ui.horizontal(|ui| self.phone_waiting(ui));
            return;
        };
        ui.horizontal(|ui| {
            self.phone_waiting(ui);
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
            let audio = l.files.iter().filter(|f| crate::decode::is_audio(&f.path)).count();
            if videos + audio < l.files.len() {
                ui.label(self.i18n.t_args("status-images", &[("n", &(l.files.len() - videos - audio).to_string())]));
            }
            if audio > 0 {
                ui.label(self.i18n.t_args("status-audio", &[("n", &audio.to_string())]));
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
                    ui.label(source::shown(&f.path));
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

/// Name and shortcut of a menu item; its icon comes from the action (`Act::icon`).
fn item(label: String, keys: String) -> (String, String) {
    (label, keys)
}

/// Records the item chosen; it runs after the menus are drawn.
fn pick(act: &Cell<Option<Act>>, r: egui::Response, a: Act) {
    if r.clicked() {
        act.set(Some(a));
    }
}

/// An entry of the menu bar; greyed out with why (`off`) on hover, as in the right-click menus.
fn entry(ui: &mut egui::Ui, act: &Cell<Option<Act>>, off: Option<String>, (label, keys): (String, String), a: Act) {
    let button = icons::menu_item(ui, a.icon(), label, keys);
    let r = ui.add_enabled(off.is_none(), button);
    pick(act, match off {
        Some(why) => r.on_disabled_hover_text(why),
        None => crate::browser::with_about(ui, r, &a),
    }, a);
}

/// At the bottom of a menu with an entry marked "*": what the mark means.
fn partial_note(ui: &mut egui::Ui, partial: bool, i18n: &crate::i18n::I18n) {
    if partial {
        ui.separator();
        ui.weak(i18n.t("menu-partial")).on_hover_text(i18n.t("menu-partial-hint"));
    }
}
