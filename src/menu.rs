//! Menu bar, toolbar and status bar.

use std::cell::Cell;

use eframe::egui::{self, Color32, Key};

use crate::navigation::Go;
use crate::actions::{Act, transform_items};
use crate::{App, icons, meta, settings, source, transform, version};
use crate::loader::THUMB_SIDE;
use crate::selection::{Kind, PLAYED, fits, starred};

/// Below this width of the address bar the copy and paste buttons are left out.
const ADDRESS_WITH_CLIP: f32 = 420.0;
/// The room the two buttons take at the end of the address bar.
const CLIP_BUTTONS: f32 = 64.0;

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
        egui::MenuBar::new().ui(ui, |ui| {
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Left out of the manual's pictures, which it would date.
                #[cfg(feature = "shots")]
                let version_shown = !crate::shots::active();
                #[cfg(not(feature = "shots"))]
                let version_shown = true;
                if version_shown {
                    ui.weak(format!("EyeCanSee {}", version()));
                }
                if let Some(r) = &self.update {
                    // Red: as a plain button it went unnoticed.
                    let text = self.i18n.t_args("update-install", &[("version", &r.version)]);
                    pick(&act, ui.add(crate::theme::alert_button(text)), Act::Update);
                }
                self.engine_bar(ui);
                self.cad_engine_bar(ui);
            });
        });
        if let Some(a) = act.take() {
            self.run(ui.ctx(), a);
        }
    }

    fn file_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, true, item(t("menu-open-location"), String::new()), Act::Address);
        entry(ui, act, true, item(t("menu-servers"), String::new()), Act::Servers);
        entry(ui, act, self.can_make_folder(), item(t("menu-new-folder"), t("key-new-folder")), Act::NewFolder(None));
        entry(ui, act, self.can_new_image(), item(t("menu-new-image"), t("key-new-image")), Act::NewImage);
        ui.separator();
        // Also when it cannot overwrite (FTP, gvfs, format): `ask_save` then says why and offers Save as….
        let changed = self.viewing.is_some() && !self.turn.is_none() && self.save_ready.is_none();
        entry(ui, act, self.can_save() || changed, item(t("menu-save"), t("key-save")), Act::Save);
        // As in the right-click menu: in the grid one picture, in the viewer also what a 3D model
        // or a CAD drawing shows.
        let files = self.files_chosen(None);
        let one_picture = self.viewing.is_none() && files.len() == 1 && fits(&files, &[Kind::Picture]).is_some();
        // Ctrl+Shift+S works in the viewer only.
        let keys = if self.viewing.is_some() { t("key-save-copy") } else { String::new() };
        entry(ui, act, self.viewing_image() || one_picture, item(t("menu-save-copy"), keys), Act::Convert(None));
        // As in the right-click menu: the 3D models among the files chosen.
        let model = fits(&files, &[Kind::Model]);
        ui.add_enabled_ui(model.is_some(), |ui| {
            ui.menu_button((icons::menu_icon(ui, None), starred(t("menu-export-3d"), model != Some(false))), |ui| {
                for k in crate::model_export::Kind::ALL {
                    entry(ui, act, true, item(k.label().into(), String::new()), Act::ExportModel(None, k));
                }
            });
        });
        partial_note(ui, model == Some(false), &self.i18n);
        ui.separator();
        let can_change = self.target().is_some();
        let one = self.chosen(None).len() <= 1;
        let rename = icons::menu_item(ui, Some(icons::Line::Rename), t("menu-rename"), "F2".into());
        let r = ui.add_enabled(can_change && one, rename);
        pick(act, if one { r } else { r.on_disabled_hover_text(t("rename-one-only")) }, Act::Rename(None));
        entry(ui, act, can_change, item(t("menu-delete"), t("key-delete")), Act::Delete(None, false));
        entry(ui, act, can_change, item(t("menu-delete-permanent"), t("key-shift-delete")), Act::Delete(None, true));
        ui.separator();
        entry(ui, act, true, item(t("menu-quit"), String::new()), Act::Quit);
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
        // In the grid the pictures among the files chosen ("*" with other kinds).
        let fit = self.turns_fit();
        let star = |label| starred(label, fit != Some(false));
        for (op, label, keys) in transform_items() {
            entry(ui, act, fit.is_some(), item(star(t(label)), keys.map(t).unwrap_or_default()), Act::Transform(op, None));
            if op == transform::Op::FlipV {
                ui.separator();
            }
        }
        ui.separator();
        let off = self.edit_off(None);
        let ok = fit.is_some() && off.is_none();
        for (label, keys, a) in [("menu-edit-image", t("key-edit"), Act::Edit(None)), ("menu-rotate-crop", String::new(), Act::RotateCrop(None))] {
            let button = icons::menu_item(ui, a.icon(), star(t(label)), keys);
            let r = ui.add_enabled(ok, button);
            pick(act, match &off { Some(why) => r.on_disabled_hover_text(why), None => r }, a);
        }
        partial_note(ui, fit == Some(false), &self.i18n);
    }

    fn view_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        let has_items = self.items() > 0;
        entry(ui, act, !viewing && has_items && self.cursor, item(t("menu-open-selected"), t("key-enter")), Act::Open);
        entry(ui, act, viewing, item(t("menu-back-to-grid"), "Esc".into()), Act::Back);
        // Off without pictures and videos: the rest stays out of the slideshow.
        let show = self.files().iter().any(|f| crate::slideshow::in_show(&f.path));
        entry(ui, act, show, item(t("menu-slideshow"), String::new()), Act::Slideshow(None));
        ui.separator();
        // Moving around (once a menu of its own, "Go", with only these).
        entry(ui, act, has_items, item(t("menu-next"), "→".into()), Act::Go(Go::Next));
        entry(ui, act, has_items, item(t("menu-prev"), "←".into()), Act::Go(Go::Prev));
        entry(ui, act, has_items, item(t("menu-first"), t("key-home")), Act::Go(Go::First));
        entry(ui, act, has_items, item(t("menu-last"), t("key-end")), Act::Go(Go::Last));
        entry(ui, act, !viewing && !self.dir.is_empty(), item(t("menu-parent"), t("key-backspace")), Act::Up);
        ui.separator();
        entry(ui, act, true, item(t("menu-refresh"), "F5".into()), Act::Refresh);
        entry(ui, act, self.chosen(None).iter().any(|p| !p.ends_with('/')), item(t("menu-redo-thumbs"), String::new()), Act::RedoThumbs(None));
        entry(ui, act, !viewing && has_items, item(t("menu-redo-all-thumbs"), t("key-shift-f5")), Act::RedoAllThumbs);
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
        ui.add_enabled_ui(!viewing, |ui| {
            ui.menu_button((icons::menu_icon(ui, None), t("menu-meta")), |ui| {
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
        // As in the right-click menu: each entry off when none of the files chosen is of a kind
        // it works on, with "*" when only some are.
        let chosen = self.grid_items(None);
        let mut partial = false;
        let mut kinds_entry = |ui: &mut egui::Ui, kinds: &[Kind], label: &str, keys: String, a: Act| {
            let fit = fits(&chosen, kinds);
            partial |= free && fit == Some(false);
            entry(ui, act, free && fit.is_some(), item(starred(self.i18n.t(label), fit != Some(false)), keys), a);
        };
        kinds_entry(ui, PLAYED, "menu-batch", t("key-batch"), Act::Batch(None));
        kinds_entry(ui, &[Kind::Drawing], "menu-export-drawings", String::new(), Act::ExportDrawings(None));
        kinds_entry(ui, &[Kind::Video], "menu-copy-meta", String::new(), Act::CopyMeta(None));
        kinds_entry(ui, &[Kind::Video], "menu-strip-audio", String::new(), Act::Audio(crate::audio_job::AudioJob::Strip, None));
        kinds_entry(ui, &[Kind::Video], "menu-extract-audio", String::new(), Act::Audio(crate::audio_job::AudioJob::Extract, None));
        entry(ui, act, self.duplicates_off().is_none(), item(t("menu-duplicates"), t("key-duplicates")), Act::Duplicates(None));
        partial_note(ui, partial, &self.i18n);
        ui.separator();
        entry(ui, act, true, item(t("menu-options"), String::new()), Act::Options);
    }

    fn help_menu(&self, ui: &mut egui::Ui, act: &Cell<Option<Act>>) {
        let t = |id| self.i18n.t(id);
        entry(ui, act, true, item(t("menu-manual"), "F1".into()), Act::Manual);
        ui.separator();
        entry(ui, act, true, item(t("menu-check-updates"), String::new()), Act::CheckUpdates);
        entry(ui, act, true, item(t("menu-about"), String::new()), Act::About);
    }

    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Line icons in square buttons with a border (Lucide, user's choice of 2026-11-05).
        ui.horizontal(|ui| {
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
            if self.viewing.is_none() && ui.add(icons::button(ui, icons::Line::Refresh)).on_hover_text(self.i18n.t("menu-refresh")).clicked() {
                self.refresh();
            }
            self.toolbar_ops(ui);
            // Metadata: closes all the boxes if one is open, otherwise reopens them all.
            if self.viewing.is_none() {
                let any = meta::Group::ALL.into_iter().any(|g| self.meta_shown(g));
                if ui.add(icons::toggle(ui, any, icons::Line::Info)).on_hover_text(self.i18n.t("tb-meta")).clicked() {
                    for g in meta::Group::ALL {
                        self.show_meta(g, !any);
                    }
                }
            }
            self.bookmark_button(ui);
            // Space to the right of the address: sorting (drop-down and direction) and thumbnail slider.
            let slider_width = if self.viewing.is_none() { 370.0 } else { 0.0 };
            // Copy and paste of the address at its end, when the bar has room for them.
            let room = ui.available_width() - slider_width;
            let clip = room > ADDRESS_WITH_CLIP;
            let address = egui::TextEdit::singleline(&mut self.address)
                .id(address_id())
                .hint_text(self.i18n.t("tb-address-hint"))
                .desired_width(if clip { room - CLIP_BUTTONS } else { room });
            let response = ui.add(address);
            if clip {
                self.address_clip(ui);
            }
            // The Enter is taken here: left to the grid drawn after, it opened the selected file
            // (an address that does not open left the viewer on it: user, 2026-11-10).
            if response.lost_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
                let address = crate::navigation::typed_address(&self.address);
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
                let arrow = icons::button(ui, if desc { icons::Line::SortDesc } else { icons::Line::SortAsc });
                let flip = ui.add(arrow).on_hover_text(self.i18n.t(if desc { "sort-desc" } else { "sort-asc" })).clicked();
                if by != self.settings.sort_by || flip {
                    self.set_sort(by, desc != flip);
                }
                let hint = self.i18n.t("tb-thumb-size");
                ui.add(icons::line(ui, icons::Line::Image, 18.0)).on_hover_text(&hint);
                ui.add(egui::Slider::new(&mut self.thumb_size, 64.0..=THUMB_SIDE as f32).show_value(false))
                    .on_hover_text(hint);
            }
        });
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

    /// Copy the address shown (never with a password), or paste one in its place: the field
    /// takes the keyboard and gets what the clipboard holds, ready for Enter.
    fn address_clip(&mut self, ui: &mut egui::Ui) {
        if ui.add(icons::small(ui, icons::Line::Copy)).on_hover_text(self.i18n.t("tb-address-copy")).clicked() {
            ui.ctx().copy_text(crate::address::without_password(&self.address));
        }
        if ui.add(icons::small(ui, icons::Line::Paste)).on_hover_text(self.i18n.t("tb-address-paste")).clicked() {
            self.address.clear();
            ui.ctx().memory_mut(|m| m.request_focus(address_id()));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
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

fn entry(ui: &mut egui::Ui, act: &Cell<Option<Act>>, enabled: bool, (label, keys): (String, String), a: Act) {
    let button = icons::menu_item(ui, a.icon(), label, keys);
    pick(act, ui.add_enabled(enabled, button), a);
}

/// At the bottom of a menu with an entry marked "*": what the mark means.
fn partial_note(ui: &mut egui::Ui, partial: bool, i18n: &crate::i18n::I18n) {
    if partial {
        ui.separator();
        ui.weak(i18n.t("menu-partial")).on_hover_text(i18n.t("menu-partial-hint"));
    }
}
