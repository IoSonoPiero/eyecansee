//! Browse mode, the parts shared by the tree (`tree`), the grid (`grid`) and the metadata panel
//! (`meta_panel`): right-click menus, error text, selection colour and focus frame.

use std::cell::Cell;

use eframe::egui::{self, Color32, Ui};

use crate::audio_job::AudioJob;
use crate::actions::Act;
use crate::App;
use crate::selection::{CONVERTED, Kind, PLAYED, THUMBED, fits, none_of, starred};
use crate::source;

/// Selection colour: full in the part receiving the keyboard, faint in the other, so the two
/// selections (folder open on the left, item on the right) do not look in conflict.
pub(crate) fn selection_fill(ui: &Ui, active: bool) -> Color32 {
    let fill = ui.visuals().selection.bg_fill;
    if active { fill } else { fill.gamma_multiply(0.45) }
}

/// Frame around the part that receives arrows and Tab.
pub(crate) fn focus_frame(ui: &Ui) {
    let stroke = egui::Stroke::new(1.0, crate::theme::accent(ui.visuals()));
    ui.painter().rect_stroke(ui.max_rect(), 2.0, stroke, egui::StrokeKind::Outside);
}

/// Text of an error or warning: the former light red was barely readable on the light background.
/// Item of a right-click menu: (label, shortcut, action); `None` = separator.
pub(crate) type MenuItem = Option<(String, String, Act)>;

/// Right-click menu; returns the chosen item, which the caller runs at the end of the frame (`App::act`).
pub(crate) fn context_menu(ui: &mut Ui, items: Vec<MenuItem>) -> Option<Act> {
    let mut chosen = None;
    for item in items {
        match item {
            Some((label, _, Act::Submenu(sub))) => {
                ui.menu_button((crate::icons::menu_icon(ui, None), label), |ui| {
                    if let Some(a) = context_menu(ui, sub) {
                        chosen = Some(a);
                    }
                });
            }
            Some((label, keys, Act::Off(why))) => {
                ui.add_enabled(false, crate::icons::menu_item(ui, None, label, keys)).on_disabled_hover_text(why);
            }
            Some((label, keys, act)) => {
                if ui.add(crate::icons::menu_item(ui, act.icon(), label, keys)).clicked() {
                    chosen = Some(act);
                }
            }
            None => {
                ui.separator();
            }
        }
    }
    chosen
}

impl App {
    /// Right-click menu of an item, as in file managers: open first, the deletions last and
    /// apart, so a slip of the mouse does not hit them. `path`: `None` = the selected item or
    /// the one in the viewer; `open`: the action that opens it (`None` in the viewer).
    pub(crate) fn item_menu(&self, path: Option<String>, open: Option<Act>) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let in_viewer = open.is_none();
        let mut items = vec![match open {
            Some(a @ Act::Open) => Some((t("menu-open-selected"), t("key-enter"), a)),
            Some(a) => Some((t("menu-open-selected"), String::new(), a)),
            None => Some((t("menu-back-to-grid"), "Esc".into(), Act::Back)),
        }];
        let Some(target) = path.clone().or_else(|| self.target()).filter(|p| self.can_change(p)) else {
            // The root of a drive, in the tree: it cannot be cut or renamed, but a folder can be
            // made in it (not in the storages of a phone).
            if path.as_deref() == Some("") && self.can_make_folder_in("") {
                items.push(Some((self.i18n.t("menu-new-folder-in"), String::new(), Act::NewFolder(path.clone()))));
            }
            // It, or a storage of a phone, can be a bookmark all the same.
            if let Some(dir) = path.filter(|p| p.is_empty() || p.ends_with('/')) {
                items.extend([None, self.bookmark_entry(&dir, false)]);
            }
            return items;
        };
        // The trash folder: only put everything back or empty it. What is inside it: look at it,
        // put it back or delete it for good (Del does that too there).
        if source::is_trash(&target) {
            items.push(Some((t("menu-restore-all"), String::new(), Act::Restore(path.clone()))));
            items.push(None);
            items.push(Some((t("menu-empty-trash"), t("key-delete"), Act::Delete(path, true))));
        } else if source::in_trash(&target) {
            items.push(Some((t("menu-restore"), String::new(), Act::Restore(path.clone()))));
            items.push(None);
            items.push(Some((t("menu-delete-permanent"), t("key-delete"), Act::Delete(path, true))));
        } else if target.ends_with('/') {
            items.extend(self.folder_items(path, &target));
        } else {
            items.extend(self.file_items(path, in_viewer));
        }
        items
    }

    /// After Open, in the order of the menu bar: Edit (cut, copy, paste), Tools, then rename
    /// and the deletions apart at the end, as in file managers.
    fn folder_items(&self, path: Option<String>, target: &str) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let mut items = Vec::new();
        // As in File: a new folder, made inside this one.
        if self.can_make_folder_in(target) {
            items.push(Some((t("menu-new-folder-in"), String::new(), Act::NewFolder(Some(target.to_string())))));
        }
        items.extend([None, Some((t("menu-cut"), t("key-cut"), Act::Cut(path.clone()))), Some((t("menu-copy"), t("key-copy"), Act::Copy(path.clone())))]);
        // Not into one of the folders being copied: it would go inside itself.
        let copied = self.clip.as_ref().is_some_and(|c| c.base == self.base && c.paths.iter().any(|p| p == target));
        if !copied && self.can_paste(target) {
            items.push(Some((t("menu-paste-into"), String::new(), Act::Paste(Some(target.to_string())))));
        }
        // Bookmarks, then Tools, as in the menu bar.
        items.push(None);
        items.push(self.bookmark_entry(target, self.chosen(path.clone()).len() > 1));
        items.push(Some((t("menu-duplicates"), String::new(), Act::Duplicates(Some(target.to_string())))));
        items.extend(self.rename_delete(path));
        items
    }

    /// Transform: flips and turns (in the viewer shown until saved, in the grid saved straight
    /// away), then Rotate and crop.
    fn turn_items(&self, path: Option<String>) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let mut turns = Vec::new();
        for (op, label, keys) in crate::actions::transform_items() {
            // Ctrl+R and Ctrl+Shift+R work in the grid too (`turn_keys`).
            turns.push(Some((t(label), keys.map(t).unwrap_or_default(), Act::Transform(op, path.clone()))));
            if op == crate::transform::Op::FlipV {
                turns.push(None);
            }
        }
        turns.push(None);
        // One image at a time: with several marked it would open on one and leave the others.
        let rotate = match self.edit_off(path.clone()) {
            Some(why) => Act::Off(why),
            None => Act::RotateCrop(path),
        };
        turns.push(Some((t("menu-rotate-crop"), String::new(), rotate)));
        turns
    }

    /// An entry that works on some kinds of file, always in its place: greyed out with why when
    /// `act` is (`Act::Off`) or when none of the `files` is of its `kinds`; "*" when only some
    /// are, which sets `partial` for the line at the bottom of the menu.
    pub(crate) fn kind_entry(&self, files: &[String], partial: &Cell<bool>, kinds: &[Kind], label: &str, keys: String, act: Act) -> MenuItem {
        let label = self.i18n.t(label);
        Some(match (fits(files, kinds), act) {
            (_, off @ Act::Off(_)) => (label, keys, off),
            (None, _) => (label, keys, Act::Off(self.i18n.t(none_of(kinds)))),
            (Some(all), act) => {
                partial.set(partial.get() | !all);
                (starred(label, all), keys, act)
            }
        })
    }

    /// The entries of Tools that work on files of some kinds, in the Tools menu and in the
    /// right-click menu of a file of the grid. `off`: why none of them can start now (in the
    /// viewer, or a conversion already under way).
    pub(crate) fn tool_items(&self, path: Option<String>, off: Option<String>, partial: &Cell<bool>) -> Vec<MenuItem> {
        let files = self.grid_items(path.clone());
        let entry = |kinds: &[Kind], label: &str, keys: String, act: Act| self.kind_entry(&files, partial, kinds, label, keys, off.clone().map_or(act, Act::Off));
        vec![
            entry(CONVERTED, "menu-batch", self.i18n.t("key-batch"), Act::Batch(path.clone())),
            entry(&[Kind::Picture], "menu-resize", String::new(), Act::Submenu(self.resize_items(path.clone()))),
            entry(&[Kind::Drawing], "menu-export-drawings", String::new(), Act::ExportDrawings(path.clone())),
            entry(PLAYED, "menu-copy-meta", String::new(), Act::CopyMeta(path.clone())),
            entry(&[Kind::Video], "menu-strip-audio", String::new(), Act::Audio(AudioJob::Strip, path.clone())),
            entry(&[Kind::Video], "menu-extract-audio", String::new(), Act::Audio(AudioJob::Extract, path)),
        ]
    }

    /// After Open (or Back to the grid), in the order of the menu bar: View (slideshow,
    /// thumbnails), Image and saving, Tools, Edit, then rename and the deletions apart. The entries
    /// for some kinds of file (`selection::Kind`) are always there (`kind_entry`), with a line at
    /// the bottom that says what "*" means.
    fn file_items(&self, path: Option<String>, in_viewer: bool) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let files = self.files_chosen(path.clone());
        let partial = Cell::new(false);
        let entry = |kinds: &[Kind], label: &str, keys: String, act: Act| self.kind_entry(&files, &partial, kinds, label, keys, act);
        // A 3D model or a CAD drawing: no slideshow (looked at closely, not in passing); the
        // slideshow stays in View for the whole folder.
        let mut items = vec![
            entry(PLAYED, "menu-slideshow", String::new(), Act::Slideshow(path.clone())),
            entry(THUMBED, "menu-redo-thumbs", String::new(), Act::RedoThumbs(path.clone())),
            None,
        ];
        let edit = match self.edit_off(path.clone()) {
            Some(why) => Act::Off(why),
            None => Act::Edit(path.clone()),
        };
        // Ctrl+E works in the grid too (`global_keys`).
        items.push(entry(&[Kind::Picture], "menu-edit-image", t("key-edit"), edit));
        items.push(entry(&[Kind::Picture], "menu-transform", String::new(), Act::Submenu(self.turn_items(path.clone()))));
        // In the viewer always there, as in File: greyed out with why when there is nothing to save.
        if in_viewer {
            items.push(Some((t("menu-save"), t("key-save"), self.save_off().map_or(Act::Save, Act::Off))));
        }
        // One file: a picture, or in the viewer what is shown (a 3D model from its side, the
        // layout of a drawing open). In the grid a drawing is exported, a model saved as a model.
        let save_as: &[Kind] = if in_viewer { &[Kind::Picture, Kind::Model, Kind::Drawing] } else { &[Kind::Picture] };
        let convert = if files.len() > 1 { Act::Off(t("tb-off-one-picture")) } else { Act::Convert(path.clone()) };
        items.push(entry(save_as, "menu-save-copy", if in_viewer { t("key-save-copy") } else { String::new() }, convert));
        let kinds = crate::model_export::Kind::ALL.iter().map(|&k| Some((k.label().to_string(), String::new(), Act::ExportModel(path.clone(), k)))).collect();
        items.push(entry(&[Kind::Model], "menu-export-3d", String::new(), Act::Submenu(kinds)));
        if !in_viewer {
            items.extend(self.tool_items(path.clone(), self.tools_off(), &partial));
        }
        items.push(None);
        items.push(Some((t("menu-cut"), t("key-cut"), Act::Cut(path.clone()))));
        items.push(Some((t("menu-copy"), t("key-copy"), Act::Copy(path.clone()))));
        items.extend(self.rename_delete(path));
        if partial.get() {
            items.extend([None, Some((t("menu-partial"), String::new(), Act::Off(t("menu-partial-hint"))))]);
        }
        items
    }

    fn rename_delete(&self, path: Option<String>) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let rename = if self.chosen(path.clone()).len() > 1 { Act::Off(t("rename-one-only")) } else { Act::Rename(path.clone()) };
        vec![
            None,
            Some((t("menu-rename"), "F2".into(), rename)),
            None,
            Some((t("menu-delete"), t("key-delete"), Act::Delete(path.clone(), false))),
            Some((t("menu-delete-permanent"), t("key-shift-delete"), Act::Delete(path, true))),
        ]
    }

    /// Name shown for a folder or image: trash folders have their own, in the user's language.
    pub(crate) fn item_name(&self, path: &str) -> String {
        if source::is_trash(path) { self.i18n.t("trash") } else { source::shown(path).to_string() }
    }
}

pub(crate) fn error_text(ui: &Ui, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(ui.visuals().error_fg_color)
}
