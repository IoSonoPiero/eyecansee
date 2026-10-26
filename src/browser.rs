//! Browse mode, the parts shared by the tree (`tree`), the grid (`grid`) and the metadata panel
//! (`meta_panel`): right-click menus, error text, selection colour and focus frame.

use eframe::egui::{self, Color32, Ui};

use crate::actions::Act;
use crate::App;
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
                ui.menu_button(label, |ui| {
                    if let Some(a) = context_menu(ui, sub) {
                        chosen = Some(a);
                    }
                });
            }
            Some((label, keys, Act::Off(why))) => {
                ui.add_enabled(false, egui::Button::new(label).shortcut_text(keys)).on_disabled_hover_text(why);
            }
            Some((label, keys, act)) => {
                if ui.add(egui::Button::new(label).shortcut_text(keys)).clicked() {
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
        let Some(target) = path.clone().or_else(|| self.target()).filter(|p| self.can_change(p)) else { return items };
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
            items.extend(self.image_items(path, in_viewer));
        }
        items
    }

    /// After Open, in the order of the menu bar: Edit (cut, copy, paste), Tools, then rename
    /// and the deletions apart at the end, as in file managers.
    fn folder_items(&self, path: Option<String>, target: &str) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let mut items = vec![None, Some((t("menu-cut"), t("key-cut"), Act::Cut(path.clone()))), Some((t("menu-copy"), t("key-copy"), Act::Copy(path.clone())))];
        // Not into one of the folders being copied: it would go inside itself.
        let copied = self.clip.as_ref().is_some_and(|c| c.base == self.base && c.paths.iter().any(|p| p == target));
        if !copied && self.can_paste(target) {
            items.push(Some((t("menu-paste-into"), String::new(), Act::Paste(Some(target.to_string())))));
        }
        items.push(None);
        items.push(Some((t("menu-duplicates"), String::new(), Act::Duplicates(Some(target.to_string())))));
        items.extend(self.rename_delete(path));
        items
    }

    /// After Open (or Back to the grid), in the order of the menu bar: View (slideshow,
    /// thumbnails), Image and saving, Tools, Edit, then rename and the deletions apart.
    fn image_items(&self, path: Option<String>, in_viewer: bool) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        let mut items = vec![Some((t("menu-slideshow"), String::new(), Act::Slideshow(path.clone())))];
        items.push(Some((t("menu-redo-thumbs"), String::new(), Act::RedoThumbs(path.clone()))));
        items.push(None);
        // Flips and turns: in the viewer shown until saved, in the grid saved straight away.
        let mut turns = Vec::new();
        for (op, label, keys) in crate::actions::transform_items() {
            turns.push(Some((t(label), if in_viewer { keys.map(t).unwrap_or_default() } else { String::new() }, Act::Transform(op, path.clone()))));
            if op == crate::transform::Op::FlipV {
                turns.push(None);
            }
        }
        turns.push(None);
        // One image at a time: with several marked it would open on one and leave the others.
        let rotate = if self.chosen(path.clone()).len() > 1 { Act::Off(t("rc-one-only")) } else { Act::RotateCrop(path.clone()) };
        turns.push(Some((t("menu-rotate-crop"), String::new(), rotate)));
        items.push(Some((t("menu-transform"), String::new(), Act::Submenu(turns))));
        if in_viewer && self.can_save() {
            items.push(Some((t("menu-save"), t("key-save"), Act::Save)));
        }
        items.push(Some((t("menu-save-copy"), String::new(), Act::Convert(path.clone()))));
        if !in_viewer {
            items.push(Some((t("menu-batch"), String::new(), Act::Batch(path.clone()))));
        }
        items.push(None);
        items.push(Some((t("menu-cut"), t("key-cut"), Act::Cut(path.clone()))));
        items.push(Some((t("menu-copy"), t("key-copy"), Act::Copy(path.clone()))));
        items.extend(self.rename_delete(path));
        items
    }

    fn rename_delete(&self, path: Option<String>) -> Vec<MenuItem> {
        let t = |id| self.i18n.t(id);
        vec![
            None,
            Some((t("menu-rename"), "F2".into(), Act::Rename(path.clone()))),
            None,
            Some((t("menu-delete"), t("key-delete"), Act::Delete(path.clone(), false))),
            Some((t("menu-delete-permanent"), t("key-shift-delete"), Act::Delete(path, true))),
        ]
    }

    /// Name shown for a folder or image: trash folders have their own, in the user's language.
    pub(crate) fn item_name(&self, path: &str) -> String {
        if source::is_trash(path) { self.i18n.t("trash") } else { source::name(path).to_string() }
    }
}

pub(crate) fn error_text(ui: &Ui, text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(ui.visuals().error_fg_color)
}
