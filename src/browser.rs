//! Browse mode, the parts shared by the tree (`tree`), the grid (`grid`) and the metadata panel
//! (`meta_panel`): right-click menus, error text, selection colour and focus frame; and what any
//! open menu keeps for itself: the keys and the clicks outside it (`menu_keys`, `menu_clicks`).

use std::cell::Cell;

use eframe::egui::{self, Color32, Ui};

use crate::audio_job::AudioJob;
use crate::actions::Act;
use crate::App;
use crate::selection::{CONVERTED, Kind, META_COPIED, PLAYED, SAVED_AS, THUMBED, fits, starred, why_none};
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
/// Save as… and Convert… look alike: each says, on hover, what it works on (`Act::about`; the
/// texts are put in the context's memory each frame, translated, by `App::ui`).
pub(crate) fn with_about(ui: &Ui, r: egui::Response, act: &Act) -> egui::Response {
    match act.about().and_then(|id| ui.ctx().data(|d| d.get_temp::<String>(egui::Id::unique(id)))) {
        Some(text) => r.on_hover_text(text),
        None => r,
    }
}

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
                let r = ui.add(crate::icons::menu_item(ui, act.icon(), label, keys));
                if with_about(ui, r, &act).clicked() {
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
            None => Some((t("menu-back-to-grid"), self.back_key(), Act::Back)),
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

    /// The right-click menu of the viewer (every kind of file): Full screen second, after Back to
    /// the grid, also where `item_menu` stops early (in a trash, a file that cannot be changed):
    /// at full screen it is the way out with the mouse (#358). `full`: the window is at full screen.
    pub(crate) fn viewer_menu(&self, full: bool) -> Vec<MenuItem> {
        let mut items = self.item_menu(None, None);
        items.insert(1, Some((self.i18n.t(crate::viewer::full_label(full)), "F11".into(), Act::FullScreen)));
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
            let act = match self.turn_op_off(op) {
                Some(why) if path.as_deref().is_none_or(|p| !crate::decode::is_image(p)) => Act::Off(why),
                _ => Act::Transform(op, path.clone()),
            };
            turns.push(Some((t(label), keys.map(t).unwrap_or_default(), act)));
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
    /// `act` is (`Act::Off`) or when none of the `files` is of its `kinds` (`why_none`); "*" when only some
    /// are, which sets `partial` for the line at the bottom of the menu.
    pub(crate) fn kind_entry(&self, files: &[String], partial: &Cell<bool>, kinds: &[Kind], label: &str, keys: String, act: Act) -> MenuItem {
        let why = why_none(label, kinds);
        let label = self.i18n.t(label);
        Some(match (fits(files, kinds), act) {
            (_, off @ Act::Off(_)) => (label, keys, off),
            (None, _) => (label, keys, Act::Off(self.i18n.t(why))),
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
        let files = self.grid_chosen(path.clone());
        let entry = |kinds: &[Kind], label: &str, keys: String, act: Act| self.kind_entry(&files, partial, kinds, label, keys, off.clone().map_or(act, Act::Off));
        vec![
            entry(CONVERTED, "menu-batch", self.i18n.t("key-batch"), Act::Batch(path.clone())),
            entry(&[Kind::Picture], "menu-resize", String::new(), Act::Submenu(self.resize_items(path.clone()))),
            entry(META_COPIED, "menu-copy-meta", String::new(), Act::CopyMeta(path.clone())),
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
        // With every entry inside greyed out (an AVI), Transform is greyed out itself, with their reason.
        let turns = self.turn_items(path.clone());
        items.push(entry(&[Kind::Picture, Kind::Video], "menu-transform", String::new(), all_off(&turns).map_or(Act::Submenu(turns), Act::Off)));
        // In the viewer always there, as in File: greyed out with why when there is nothing to save.
        if in_viewer {
            items.push(Some((t("menu-save"), t("key-save"), self.save_off().map_or(Act::Save, Act::Off))));
        }
        // One file: a picture, or a picture of a 3D model (in the viewer from the side shown) or of
        // a CAD drawing (in the viewer the layout open).
        let save_as = SAVED_AS;
        // One of them: with more the entry is off (with none, `kind_entry` says why).
        let saved = files.iter().filter(|p| save_as.contains(&crate::selection::kind(p))).count();
        let convert = if saved > 1 { Act::Off(t("tb-off-one-picture")) } else { Act::Convert(path.clone()) };
        if in_viewer && self.viewing_video() {
            // The frame shown (#16), on its own: a video is not among `SAVED_AS` in the grid.
            items.push(Some((t("menu-save-frame"), t("key-save-copy"), self.save_as_off().map_or(convert, Act::Off))));
        } else {
            items.push(entry(save_as, "menu-save-copy", if in_viewer { t("key-save-copy") } else { String::new() }, convert));
        }
        // The two exports, each with its window, in the grid and in the viewer (issue #128).
        items.push(entry(&[Kind::Model], "menu-export-3d", String::new(), Act::ExportModel(path.clone())));
        let drawings = self.drawings_off(Some(true)).map_or(Act::ExportDrawings(path.clone()), Act::Off);
        items.push(entry(&[Kind::Drawing], "menu-export-drawings", String::new(), drawings));
        // Two drawings chosen: one over the other (#143).
        if !in_viewer && files.iter().any(|p| crate::cad::is_cad(p)) {
            items.push(Some((t("menu-compare"), String::new(), self.compare_off(&files).map_or(Act::Compare, Act::Off))));
        }
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

/// Why a submenu is greyed out: all its entries are, and the first says why; `None` when one is on.
pub(crate) fn all_off(items: &[MenuItem]) -> Option<String> {
    let mut acts = items.iter().flatten().map(|(_, _, act)| act);
    let why = match acts.next()? {
        Act::Off(why) => why.clone(),
        _ => return None,
    };
    acts.all(|a| matches!(a, Act::Off(_))).then_some(why)
}

/// Where the bar of the menus is, in the frame just drawn (`App::panels`): a press there still
/// reaches it while a menu is open, so File, Edit… go on opening and closing one another as before.
pub(crate) fn menu_bar_at(ctx: &egui::Context, rect: egui::Rect) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(MENU_BAR), Some(rect)));
}

const MENU_BAR: &str = "menu-bar-rect";

/// With a menu open (right click, the bar of the menus, a list to choose from) a click outside
/// it only closes it, as with the menus of GTK (Nautilus): it used to reach what was under the
/// pointer too, and a click on a file to close the menu left that file alone selected (L26 of
/// 2026-11-21). A right click outside it too: it no longer opens the menu of what is under it. On the raw input, before egui sees
/// it: the press is dropped with its release, and the menus are closed here. Tab is dropped
/// too: it would have moved the selection between the tree and the grid under the menu.
pub(crate) fn menu_clicks(ctx: &egui::Context, raw: &mut egui::RawInput) {
    let id = egui::Id::unique("menu-clicks-dropped");
    let mut dropped: Vec<egui::PointerButton> = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let open = egui::Popup::is_any_open(ctx);
    // Taken away: the editor and the full screen have no bar.
    let bar = ctx.data_mut(|d| d.remove_temp::<Option<egui::Rect>>(egui::Id::unique(MENU_BAR))).flatten();
    let mut close = false;
    raw.events.retain(|e| match e {
        egui::Event::PointerButton { pos, button, pressed: true, .. } => {
            // A press let through: an earlier release lost (outside the window) no longer counts.
            dropped.retain(|b| b != button);
            let on_bar = bar.is_some_and(|r| r.contains(*pos));
            if open && outside_menu(ctx.layer_id_at(*pos).map(|l| l.order), on_bar) {
                dropped.push(*button);
                close = true;
                return false;
            }
            true
        }
        egui::Event::PointerButton { button, pressed: false, .. } if dropped.contains(button) => {
            dropped.retain(|b| b != button);
            false
        }
        egui::Event::Key { key: egui::Key::Tab, .. } => !open,
        _ => true,
    });
    if close {
        egui::Popup::close_all(ctx);
    }
    ctx.data_mut(|d| d.insert_temp(id, dropped));
}

/// A press at a place of this layer order, `None` = no layer, is outside the open menu: menus,
/// submenus and lists to choose from are all in the foreground, the window and its dialogs under it.
fn outside_menu(order: Option<egui::Order>, on_bar: bool) -> bool {
    !on_bar && order.is_none_or(|o| o < egui::Order::Foreground)
}

/// With a menu open the keys are its own: they used to reach the grid, the tree, the viewer and
/// the shortcuts under it too, and the first arrow moved the selection of the grid (L26 of
/// 2026-11-21). egui has already taken the arrows to move through the items
/// (`Memory::focus_direction`, at the start of the frame) and reads Enter and Space on the item
/// that has the keyboard: those stay, everything else is taken away here, before the pages read it.
pub(crate) fn menu_keys(ctx: &egui::Context) {
    if !egui::Popup::is_any_open(ctx) || ctx.text_edit_focused() {
        return;
    }
    let item = ctx.egui_wants_keyboard_input();
    let kept = |key: &egui::Key| item && matches!(key, egui::Key::Enter | egui::Key::Space);
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            egui::Event::Key { key, .. } => kept(key),
            egui::Event::Text(_) => false,
            _ => true,
        });
        i.keys_down.retain(|k| kept(k));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Transform on an AVI: every entry greyed out, so the submenu too, with the first reason.
    #[test]
    fn a_submenu_all_greyed_out_is_greyed_out() {
        let off = |why: &str| Some((String::new(), String::new(), Act::Off(why.into())));
        let on = Some((String::new(), String::new(), Act::Back));
        assert_eq!(all_off(&[off("a"), None, off("b")]), Some("a".to_string()));
        assert_eq!(all_off(&[off("a"), None, on.clone()]), None);
        assert_eq!(all_off(&[on, off("a")]), None);
        assert_eq!(all_off(&[None]), None);
        assert_eq!(all_off(&[]), None);
    }

    use crate::audit_family_1::{Harness, folder, own_folder};

    /// The middle of the thumbnail of cell `p` of the grid (`p` = 0 is ".."): cells are at
    /// least `thumb_size + 16` wide, at most a quarter wider with four columns or more.
    fn cell(h: &Harness, p: usize) -> egui::Pos2 {
        let rect = h.app.grid_rect.expect("the grid drawn");
        let side = h.app.thumb_size;
        rect.min + egui::vec2((p as f32 + 0.5) * (side + 16.0), 6.0 + side / 2.0)
    }

    fn click(h: &mut Harness, at: egui::Pos2, button: egui::PointerButton) {
        let ev = |pressed| egui::Event::PointerButton { pos: at, button, pressed, modifiers: egui::Modifiers::NONE };
        h.frame(vec![egui::Event::PointerMoved(at)]);
        h.frame(vec![ev(true)]);
        h.frame(vec![ev(false)]);
        h.frame(Vec::new());
    }

    /// a.png, b.png, c.png all selected with Ctrl+A, then the right-click menu of b.png open.
    fn menu_over_all(name: &str) -> Harness {
        let mut h = Harness::new(&folder(name, &["a.png", "b.png", "c.png"]));
        // As Ctrl+A (the harness gives the keys without the state of Ctrl).
        h.app.focus = crate::Focus::Grid;
        h.app.select_all();
        let b = cell(&h, 2);
        click(&mut h, b, egui::PointerButton::Secondary);
        assert!(egui::Popup::is_any_open(&h.ctx), "the menu of b.png open");
        assert_eq!((h.app.selected, h.app.marked.len()), (1, 3), "the right click on b.png keeps the selection");
        h
    }

    /// L26 of 2026-11-21: with the right-click menu open the arrows moved the selection of the
    /// grid under it, and the files selected with Ctrl+A were left one.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn keys_with_a_menu_open_do_not_reach_the_grid() {
        if own_folder("browser::tests::keys_with_a_menu_open_do_not_reach_the_grid") {
            return;
        }
        let mut h = menu_over_all("menu-keys");
        // Delete, Tab and Ctrl+X stay out, before an arrow gives the keyboard to an item: the
        // selection is neither asked to delete, nor moved to the tree, nor cut.
        h.key(egui::Key::Delete);
        h.key(egui::Key::Tab);
        h.frame(vec![egui::Event::Cut]);
        h.frame(Vec::new());
        assert!(!h.app.asking() && h.app.focus == crate::Focus::Grid && h.app.clip.is_none(), "Delete, Tab or Ctrl+X with the menu open");
        for key in [egui::Key::ArrowRight, egui::Key::ArrowDown, egui::Key::End, egui::Key::Home] {
            h.key(key);
            h.frame(Vec::new());
            assert_eq!((h.app.selected, h.app.marked.len()), (1, 3), "{key:?} with the menu open");
        }
        assert!(egui::Popup::is_any_open(&h.ctx), "the menu still open");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(!egui::Popup::is_any_open(&h.ctx), "Esc closes the menu");
        assert_eq!(h.app.marked.len(), 3, "Esc keeps the selection");
    }

    /// The arrows go through the items of the menu, and Enter runs the one reached: Open.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_arrows_and_enter_choose_in_the_menu() {
        if own_folder("browser::tests::the_arrows_and_enter_choose_in_the_menu") {
            return;
        }
        let mut h = menu_over_all("menu-enter");
        h.key(egui::Key::ArrowDown);
        h.frame(Vec::new());
        assert!(h.ctx.egui_wants_keyboard_input(), "the first item has the keyboard");
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        assert!(!egui::Popup::is_any_open(&h.ctx), "the menu closed by its item");
        assert_eq!(h.app.viewing, Some(0), "Open ran on the selection, from a.png");
    }

    /// L26 of 2026-11-21: a click outside the right-click menu, to close it, also selected the
    /// file under the pointer and dropped the others.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_click_outside_a_menu_only_closes_it() {
        if own_folder("browser::tests::a_click_outside_a_menu_only_closes_it") {
            return;
        }
        let mut h = menu_over_all("menu-click");
        let a = cell(&h, 1);
        click(&mut h, a, egui::PointerButton::Primary);
        assert!(!egui::Popup::is_any_open(&h.ctx), "the click closes the menu");
        assert_eq!((h.app.selected, h.app.marked.len()), (1, 3), "the click outside the menu reached the grid");
        // The next click is the grid's again.
        click(&mut h, a, egui::PointerButton::Primary);
        assert_eq!((h.app.selected, h.app.marked.len()), (0, 0), "a click on a.png with no menu open selects it alone");
    }

    /// A menu of the bar works as the right-click menu: the arrows and a click on the grid stay
    /// out of the grid; a press on the bar still reaches it, so File opens from another menu.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_menu_of_the_bar_keeps_the_keys_and_the_clicks() {
        if own_folder("browser::tests::a_menu_of_the_bar_keeps_the_keys_and_the_clicks") {
            return;
        }
        let mut h = menu_over_all("menu-bar");
        let bar = h.ctx.data(|d| d.get_temp::<Option<egui::Rect>>(egui::Id::unique(MENU_BAR))).flatten().expect("the bar drawn");
        // File, the first menu: its name starts at the left of the bar.
        click(&mut h, bar.left_center() + egui::vec2(16.0, 0.0), egui::PointerButton::Primary);
        assert!(egui::Popup::is_any_open(&h.ctx), "File open, the right-click menu closed");
        h.key(egui::Key::ArrowDown);
        h.frame(Vec::new());
        assert_eq!((h.app.selected, h.app.marked.len()), (1, 3), "the arrow with File open");
        let a = cell(&h, 1);
        click(&mut h, a, egui::PointerButton::Primary);
        assert!(!egui::Popup::is_any_open(&h.ctx), "the click closes File");
        assert_eq!((h.app.selected, h.app.marked.len()), (1, 3), "the click outside File reached the grid");
    }

    /// In the viewer the arrows of the right-click menu no longer go to the next picture.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn arrows_with_the_menu_of_the_viewer_stay_on_the_picture() {
        if own_folder("browser::tests::arrows_with_the_menu_of_the_viewer_stay_on_the_picture") {
            return;
        }
        let mut h = Harness::new(&folder("menu-viewer", &["a.png", "b.png", "c.png"]));
        h.app.viewing = Some(0);
        for _ in 0..5 {
            h.frame(Vec::new());
        }
        click(&mut h, egui::pos2(500.0, 400.0), egui::PointerButton::Secondary);
        assert!(egui::Popup::is_any_open(&h.ctx), "the menu of the viewer open");
        h.key(egui::Key::ArrowRight);
        h.key(egui::Key::ArrowDown);
        h.frame(Vec::new());
        assert_eq!(h.app.viewing, Some(0), "the arrows with the menu open");
        h.key(egui::Key::Escape);
        h.key(egui::Key::ArrowRight);
        h.frame(Vec::new());
        assert_eq!(h.app.viewing, Some(1), "the arrow with no menu open");
    }

    #[test]
    fn what_is_outside_a_menu() {
        use egui::Order;
        assert!(outside_menu(Some(Order::Background), false), "the grid, the tree, the bars");
        assert!(outside_menu(Some(Order::Middle), false), "a dialog");
        assert!(outside_menu(None, false));
        assert!(!outside_menu(Some(Order::Foreground), false), "the menu, a submenu, a list");
        assert!(!outside_menu(Some(Order::Tooltip), false));
        assert!(!outside_menu(Some(Order::Background), true), "the bar of the menus");
    }
}
