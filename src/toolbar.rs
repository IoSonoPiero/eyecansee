//! The operations of the toolbar: File, Image and Tools, each a group of line icons with the
//! name and the shortcut on hover and, when off, why. On as their menu items are. When the address
//! would be left narrower than `ADDRESS` points, Image and Tools go into the "More" menu.

use eframe::egui;

use crate::actions::Act;
use crate::i18n::I18n;
use crate::icons::{self, Line};
use crate::selection::{CONVERTED, fits};
use crate::{App, source, transform};

/// The address is never narrower than this because of the icons: Image and Tools go into the
/// "More" menu first (at 1400 points the address was 290 wide, user 2026-11-05).
const ADDRESS: f32 = 360.0;

struct Op {
    icon: Line,
    /// Fluent id of its name, the one of its menu item.
    label: &'static str,
    keys: String,
    act: Act,
    /// Why it cannot be used now.
    off: Option<String>,
}

impl App {
    fn file_ops(&self) -> Vec<Op> {
        let t = |id| self.i18n.t(id);
        let (rename, delete) = (
            Op { icon: Line::Rename, label: "menu-rename", keys: "F2".into(), act: Act::Rename(None), off: self.rename_off() },
            Op { icon: Line::Delete, label: "menu-delete", keys: t("key-delete"), act: Act::Delete(None, false), off: self.delete_off() },
        );
        // In the viewer only what works on the image shown: making folders, cutting, copying and
        // pasting files are for the grid; they stay in the menus.
        if self.viewing.is_some() {
            return vec![rename, delete];
        }
        vec![
            Op { icon: Line::NewFolder, label: "menu-new-folder", keys: t("key-new-folder"), act: Act::NewFolder(None), off: self.new_folder_off() },
            // Cut before Copy, as Explorer and the Edit menu.
            Op { icon: Line::Cut, label: "menu-cut", keys: t("key-cut"), act: Act::Cut(None), off: self.clip_off() },
            Op { icon: Line::Copy, label: "menu-copy", keys: t("key-copy"), act: Act::Copy(None), off: self.clip_off() },
            Op { icon: Line::Paste, label: "menu-paste", keys: t("key-paste"), act: Act::Paste(None), off: self.paste_off() },
            rename,
            delete,
        ]
    }

    fn image_ops(&self) -> Vec<Op> {
        let t = |id| self.i18n.t(id);
        let viewing = self.viewing.is_some();
        // As the Image menu: in the viewer the picture shown, in the grid the pictures chosen.
        let rotate = |op, icon, label, keys| Op { icon, label, keys: t(keys), act: Act::Transform(op, None), off: self.turn_off() };
        vec![
            rotate(transform::Op::Ccw, Line::RotateCcw, "tf-rotate-ccw", "key-rotate-ccw"),
            rotate(transform::Op::Cw, Line::RotateCw, "tf-rotate-cw", "key-rotate-cw"),
            Op { icon: Line::Edit, label: "menu-edit-image", keys: t("key-edit"), act: Act::Edit(None), off: self.editor_off() },
            Op { icon: Line::Save, label: "menu-save", keys: t("key-save"), act: Act::Save, off: self.save_off() },
            // Ctrl+Shift+S works in the viewer only.
            Op { icon: Line::SaveAs, label: "menu-save-copy", keys: if viewing { t("key-save-copy") } else { String::new() }, act: Act::Convert(None), off: self.save_as_off() },
        ]
    }

    /// None in the viewer: they work on folders and on the grid's choice.
    fn tool_ops(&self) -> Vec<Op> {
        if self.viewing.is_some() {
            return Vec::new();
        }
        let t = |id| self.i18n.t(id);
        vec![
            Op { icon: Line::Batch, label: "menu-batch", keys: t("key-batch"), act: Act::Batch(None), off: self.batch_off() },
            Op { icon: Line::Duplicates, label: "menu-duplicates", keys: t("key-duplicates"), act: Act::Duplicates(None), off: self.duplicates_off() },
        ]
    }

    /// Why Convert… cannot start (also for Ctrl+B): as the Tools menu.
    pub(crate) fn batch_off(&self) -> Option<String> {
        self.tools_off().or_else(|| fits(&self.grid_chosen(None), CONVERTED).is_none().then(|| self.i18n.t("tb-off-batch")))
    }

    /// Why no entry of Tools on the files chosen can start now: in the viewer, one conversion
    /// at a time (the entries wait for the one running), or in a trash.
    pub(crate) fn tools_off(&self) -> Option<String> {
        let t = |id| self.i18n.t(id);
        match () {
            _ if self.viewing.is_some() => Some(t("tb-off-viewer")),
            _ if self.batch.is_some() => Some(t("tb-off-busy")),
            // The files in a trash are left out of every work (`grid_items`).
            _ if self.chosen_in_trash() => Some(t("tb-off-trash")),
            _ => None,
        }
    }

    /// Why Find duplicates cannot start (also for Ctrl+D): as the Tools menu.
    pub(crate) fn duplicates_off(&self) -> Option<String> {
        let t = |id| self.i18n.t(id);
        match () {
            _ if self.viewing.is_some() => Some(t("tb-off-viewer")),
            _ if self.loader.is_none() => Some(t("tb-off-no-folder")),
            _ if source::is_trash(&self.dir) => Some(t("tb-off-trash")),
            _ => None,
        }
    }

    /// The three groups, after Up (or Grid) and Refresh; the one chosen runs after the frame.
    pub(crate) fn toolbar_ops(&mut self, ui: &mut egui::Ui) {
        let mut chosen = None;
        group(ui, &self.file_ops(), &self.i18n, &mut chosen);
        let (image, tools) = (self.image_ops(), self.tool_ops());
        // What follows the groups: the metadata and bookmarks buttons, then in the grid sorting and
        // the slider (the 370 points `toolbar` keeps for them).
        let step = icons::BUTTON.x + ui.spacing().item_spacing.x;
        let groups = (image.len() + tools.len()) as f32 * step + 2.0 * (1.0 + 2.0 * ui.spacing().item_spacing.x);
        let after = 2.0 * step + if self.viewing.is_none() { 370.0 } else { 0.0 };
        let wide = ui.available_width() - groups - after >= ADDRESS;
        if wide {
            group(ui, &image, &self.i18n, &mut chosen);
            if !tools.is_empty() {
                group(ui, &tools, &self.i18n, &mut chosen);
            }
        } else {
            ui.separator();
            more(ui, [&image, &tools], &self.i18n, &mut chosen);
        }
        ui.separator();
        if chosen.is_some() {
            self.act = chosen;
        }
    }
}

/// "Name (keys)".
fn tip(i18n: &I18n, op: &Op) -> String {
    let name = i18n.t(op.label);
    if op.keys.is_empty() { name } else { format!("{name} ({})", op.keys) }
}

fn group(ui: &mut egui::Ui, ops: &[Op], i18n: &I18n, chosen: &mut Option<Act>) {
    ui.separator();
    for op in ops {
        let tip = tip(i18n, op);
        let r = ui.add_enabled(op.off.is_none(), icons::button(ui, op.icon)).on_hover_text(&tip);
        let r = match &op.off {
            Some(why) => r.on_disabled_hover_text(format!("{tip}\n{why}")),
            None => r,
        };
        if r.clicked() {
            *chosen = Some(op.act.clone());
        }
    }
}

/// Image and Tools in a menu, as menu items: icon, name, shortcut.
fn more(ui: &mut egui::Ui, groups: [&[Op]; 2], i18n: &I18n, chosen: &mut Option<Act>) {
    let button = icons::button(ui, Line::More);
    let (r, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        for (n, ops) in groups.iter().enumerate() {
            if n > 0 {
                ui.separator();
            }
            for op in *ops {
                let item = egui::Button::image_and_text(icons::line(ui, op.icon, 16.0), i18n.t(op.label)).shortcut_text(op.keys.clone());
                let r = ui.add_enabled(op.off.is_none(), item);
                let r = match &op.off {
                    Some(why) => r.on_disabled_hover_text(why),
                    None => r,
                };
                if r.clicked() {
                    *chosen = Some(op.act.clone());
                }
            }
        }
    });
    r.on_hover_text(i18n.t("tb-more"));
}
