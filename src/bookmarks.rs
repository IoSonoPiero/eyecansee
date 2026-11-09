//! Bookmarks, as the "Directory hotlist" of Total Commander (issue #25): places to go back to with
//! one click, from the star button next to the address bar, the Bookmarks menu or Ctrl+Shift+D
//! (Ctrl+D is Find duplicates). A bookmark is any address the address bar opens (a folder, a
//! disk, a phone's folder, a server's folder, an image), kept as the bar shows it (never with a
//! password) in `settings.json`, and opened as if typed there. One that cannot be opened (a
//! folder gone, a disk or phone not connected) is said so in a message, never removed.

use std::path::PathBuf;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::actions::Act;
use crate::devices::{self, Device};
use crate::icons::{self, Line};
use crate::App;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Bookmark {
    /// As the menu shows it ("" = the address).
    pub name: String,
    /// As the address bar shows it.
    pub address: String,
}

impl Bookmark {
    pub fn label(&self) -> &str {
        if self.name.trim().is_empty() { &self.address } else { self.name.trim() }
    }
}

/// The star button's menu and the Configure window.
#[derive(Default)]
pub(crate) struct State {
    /// Bookmarks > Configure… is open.
    pub(crate) configuring: bool,
    /// The star button and its menu, to open it from the keyboard.
    menu: Option<(egui::Id, egui::Id)>,
    /// The menu was opened from the keyboard: the button has the focus until it closes.
    keyed: bool,
}

/// The same place, with or without the separator at the end.
pub(crate) fn same(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\']) == b.trim_end_matches(['/', '\\'])
}

/// Adds a bookmark at the end; `false` (and nothing added) when the place is already there.
pub(crate) fn add(list: &mut Vec<Bookmark>, name: &str, address: &str) -> bool {
    if list.iter().any(|b| same(&b.address, address)) {
        return false;
    }
    list.push(Bookmark { name: name.to_string(), address: address.to_string() });
    true
}

/// Moves the bookmark at `i` one place up or down; `false` at the ends.
pub(crate) fn shift(list: &mut [Bookmark], i: usize, up: bool) -> bool {
    let to = if up { i.checked_sub(1) } else { i.checked_add(1) };
    match to.filter(|&j| j < list.len() && i < list.len()) {
        Some(j) => {
            list.swap(i, j);
            true
        }
        None => false,
    }
}

/// The name a new bookmark gets: the device's name for the top of a disk or phone, otherwise
/// the last part of the address ("Camera", "beach.jpg"); the address itself for "/".
pub(crate) fn default_name(address: &str, devices: &[Device]) -> String {
    if let Some(d) = devices.iter().find(|d| !d.path.is_empty() && same(&d.path, address)) {
        return d.name.clone();
    }
    let last = address.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default();
    if last.is_empty() { address.to_string() } else { last.to_string() }
}

/// Why a bookmark cannot be opened now (Fluent id): a phone that is not connected, a place on
/// this computer that does not exist (deleted, or on a disk that is not connected). `None` when it
/// looks fine, and for servers, which say what is wrong only once connected.
pub(crate) fn missing(address: &str, devices: &[Device]) -> Option<&'static str> {
    if devices::phone_missing(address, devices) {
        return Some("bm-missing-phone");
    }
    // As `source::open`: one-letter schemes are Windows drives.
    let path = match url::Url::parse(address).ok().filter(|u| u.scheme().len() > 1) {
        None => PathBuf::from(address),
        Some(u) if u.scheme() == "file" => u.to_file_path().ok()?,
        Some(_) => return None,
    };
    (!path.exists()).then_some("bm-missing-place")
}

impl App {
    /// The address of what is open: the image in the viewer, otherwise the folder.
    fn here(&self) -> String {
        match self.viewing.and_then(|i| self.files().get(i)) {
            Some(f) => self.display(&f.path),
            None => self.display(&self.dir),
        }
    }

    /// The items of the star menu and of the Bookmarks menu: the bookmarks by name (the address
    /// on hover), then Add and Configure. The one chosen.
    pub(crate) fn bookmark_items(&self, ui: &mut egui::Ui) -> Option<Act> {
        let t = |id| self.i18n.t(id);
        let mut chosen = None;
        let list = &self.settings.bookmarks;
        if list.is_empty() {
            ui.weak(t("bm-empty"));
        }
        for (i, b) in list.iter().enumerate() {
            let r = ui.add(icons::menu_item(ui, None, b.label().to_string(), String::new())).on_hover_text(&b.address);
            if r.clicked() {
                chosen = Some(Act::Bookmark(i));
            }
        }
        ui.separator();
        let here = self.here();
        let have = list.iter().any(|b| same(&b.address, &here));
        let add = t(if self.viewing.is_some() { "bm-add-image" } else { "bm-add-folder" });
        let r = ui.add_enabled(self.loader.is_some() && !have, icons::menu_item(ui, None, add, String::new()));
        if (if have { r.on_disabled_hover_text(t("bm-already")) } else { r }).clicked() {
            chosen = Some(Act::AddBookmark);
        }
        if ui.add(icons::menu_item(ui, None, t("bm-configure"), String::new())).clicked() {
            chosen = Some(Act::Bookmarks);
        }
        chosen
    }

    /// The star button next to the address bar, with the menu of the bookmarks.
    pub(crate) fn bookmark_button(&mut self, ui: &mut egui::Ui) {
        let button = icons::button(ui, Line::Star);
        let (r, inner) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| self.bookmark_items(ui));
        let menu = egui::Popup::default_response_id(&r);
        self.bookmarks.menu = Some((r.id, menu));
        // Opened with the keys: once closed, the keys go back to the grid.
        if self.bookmarks.keyed && !egui::Popup::is_id_open(ui.ctx(), menu) {
            self.bookmarks.keyed = false;
            r.surrender_focus();
        }
        r.on_hover_text(format!("{} ({})", self.i18n.t("bm-title"), self.i18n.t("key-bookmarks")));
        if let Some(a) = inner.and_then(|i| i.inner) {
            self.act = Some(a);
        }
    }

    /// Ctrl+Shift+D: the star menu, with the focus on its button (Down enters the menu, Enter
    /// chooses). Before Ctrl+D is read, which would take it too. Not in the viewer at full
    /// screen, which has no toolbar.
    pub(crate) fn bookmark_keys(&mut self, ctx: &egui::Context) {
        let pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::D));
        let bare = self.viewing.is_some() && ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if pressed && !bare && !ctx.egui_wants_keyboard_input()
            && let Some((button, menu)) = self.bookmarks.menu
        {
            egui::Popup::open_id(ctx, menu);
            ctx.memory_mut(|m| m.request_focus(button));
            self.bookmarks.keyed = true;
        }
    }

    /// The items of the menus: open, add, Configure….
    pub(crate) fn bookmark_act(&mut self, ctx: &egui::Context, act: Act) {
        match act {
            Act::Bookmark(i) => self.open_bookmark(ctx, i),
            Act::AddBookmark => self.add_bookmark(ctx),
            Act::Bookmarks => self.bookmarks.configuring = true,
            _ => {}
        }
    }

    /// Opens a bookmark as the address bar would. A place that is gone or a device that is not
    /// connected is said in a message, and the bookmark stays.
    pub(crate) fn open_bookmark(&mut self, ctx: &egui::Context, i: usize) {
        let Some(b) = self.settings.bookmarks.get(i).cloned() else { return };
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Bookmark(i)) {
            return;
        }
        if let Some(id) = missing(&b.address, &self.devices) {
            log!("bookmark {:?} cannot be opened: {id}", b.address);
            let md = crate::save_dialogs::md;
            self.notice = Some((self.i18n.t_args(id, &[("name", &md(b.label())), ("address", &md(&b.address))]), None));
            return;
        }
        self.open_source(ctx, &b.address);
    }

    /// "+ Add current folder" (in the viewer "+ Add this image"), with the name of its last part;
    /// the status bar says so for a while.
    pub(crate) fn add_bookmark(&mut self, ctx: &egui::Context) {
        if self.loader.is_none() {
            return;
        }
        let address = self.here();
        let name = default_name(&address, &self.devices);
        if add(&mut self.settings.bookmarks, &name, &address) {
            log!("bookmark added: {address:?}");
            self.settings.save();
            self.status_note = Some(self.i18n.t_args("bm-added", &[("name", &name)]));
            self.note_until = Some(ctx.input(|i| i.time) + 6.0);
        }
    }

    /// Bookmarks > Configure…: rename, move up and down, remove. Saved when the window closes
    /// (Close, Enter or Esc).
    pub(crate) fn bookmarks_window(&mut self, ctx: &egui::Context) {
        if !self.bookmarks.configuring {
            return;
        }
        // In a field Esc reaches the window; elsewhere Enter and Esc arrive as `delete_answer`.
        let mut close = self.delete_answer.take().is_some() || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut list = std::mem::take(&mut self.settings.bookmarks);
        let file = crate::settings::config_path("settings.json").map(|p| p.display().to_string()).unwrap_or_default();
        egui::Modal::new(egui::Id::unique("bookmarks")).show(ctx, |ui| {
            ui.set_width(640.0);
            crate::theme::title(ui, self.i18n.t("bm-title"));
            ui.add_space(4.0);
            self.bookmark_rows(ui, &mut list);
            ui.add_space(6.0);
            ui.label(egui::RichText::new(self.i18n.t_args("bm-file", &[("path", &file)])).weak().small());
            ui.add_space(8.0);
            close |= ui.button(self.i18n.t("srv-close")).clicked();
        });
        self.settings.bookmarks = list;
        if close {
            self.bookmarks.configuring = false;
            self.settings.save();
        }
    }

    /// One row per bookmark: its name to change, the address, up, down and remove.
    fn bookmark_rows(&self, ui: &mut egui::Ui, list: &mut Vec<Bookmark>) {
        let t = |id| self.i18n.t(id);
        if list.is_empty() {
            ui.label(egui::RichText::new(t("bm-empty-configure")).weak());
            return;
        }
        let n = list.len();
        // (row, -1 up, 1 down, 0 remove), done after the rows are drawn.
        let mut change = None;
        egui::ScrollArea::vertical().max_height(360.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("bm-rows").num_columns(5).spacing([6.0, 4.0]).show(ui, |ui| {
                for (i, b) in list.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut b.name).hint_text(t("bm-name-hint")).desired_width(200.0));
                    let address = egui::Label::new(egui::RichText::new(&b.address).weak()).truncate();
                    ui.add_sized([300.0, 20.0], address).on_hover_text(&b.address);
                    for (on, icon, tip, what) in [(i > 0, Line::ChevronUp, "bm-up", -1), (i + 1 < n, Line::Down, "bm-down", 1), (true, Line::Delete, "bm-remove", 0)] {
                        if ui.add_enabled(on, icons::small(ui, icon)).on_hover_text(t(tip)).clicked() {
                            change = Some((i, what));
                        }
                    }
                    ui.end_row();
                }
            });
        });
        match change {
            Some((i, 0)) => {
                list.remove(i);
            }
            Some((i, step)) => {
                shift(list, i, step < 0);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[Bookmark]) -> Vec<&str> {
        list.iter().map(Bookmark::label).collect()
    }

    #[test]
    fn add_move_remove() {
        let mut list = Vec::new();
        assert!(add(&mut list, "Photos", "/home/me/Photos/"));
        assert!(add(&mut list, "Camera", "mtp://ABCD1234EFGH/Internal storage/DCIM/Camera/"));
        assert!(add(&mut list, "NAS", "sftp://me@nas.lan/srv/photos/"));
        // The same place again, with or without the separator at the end: not added.
        assert!(!add(&mut list, "Again", "/home/me/Photos"));
        assert!(!add(&mut list, "Again", "sftp://me@nas.lan/srv/photos/"));
        assert_eq!(names(&list), ["Photos", "Camera", "NAS"]);
        assert!(shift(&mut list, 2, true));
        assert_eq!(names(&list), ["Photos", "NAS", "Camera"]);
        assert!(shift(&mut list, 0, false));
        assert_eq!(names(&list), ["NAS", "Photos", "Camera"]);
        // At the ends, and past them: nothing moves.
        assert!(!shift(&mut list, 0, true));
        assert!(!shift(&mut list, 2, false));
        assert!(!shift(&mut list, 7, true));
        assert_eq!(names(&list), ["NAS", "Photos", "Camera"]);
        // A name emptied: the address is shown; a name with spaces around is shown without them.
        list[0].name = "  ".into();
        list[1].name = " Holidays ".into();
        assert_eq!(names(&list), ["sftp://me@nas.lan/srv/photos/", "Holidays", "Camera"]);
    }

    #[test]
    fn names_of_new_bookmarks() {
        let phone = Device { name: "Maker Phone".into(), path: "mtp://ABCD1234EFGH/".into(), phone: true, serial: Some("ABCD1234EFGH".into()), block: None, internal: false };
        let devices = [phone];
        assert_eq!(default_name("/home/me/Photos/", &devices), "Photos");
        assert_eq!(default_name("C:\\Users\\me\\Pictures\\", &devices), "Pictures");
        assert_eq!(default_name("/home/me/Photos/beach.jpg", &devices), "beach.jpg");
        assert_eq!(default_name("mtp://ABCD1234EFGH/", &devices), "Maker Phone");
        assert_eq!(default_name("mtp://ABCD1234EFGH/Internal storage/DCIM/Camera/", &devices), "Camera");
        assert_eq!(default_name("sftp://me@nas.lan/", &devices), "me@nas.lan");
        assert_eq!(default_name("/", &devices), "/");
        assert_eq!(default_name("E:\\", &devices), "E:");
    }

    #[test]
    fn missing_places() {
        let dir = std::env::temp_dir().join(format!("eyecansee-bookmarks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let here = format!("{}/", dir.display());
        assert_eq!(missing(&here, &[]), None);
        assert_eq!(missing(&format!("{here}gone/"), &[]), Some("bm-missing-place"));
        std::fs::remove_dir(&dir).unwrap();
        assert_eq!(missing(&here, &[]), Some("bm-missing-place"));
        // A phone not connected; a server is known only once connected.
        assert_eq!(missing("mtp://ABCD1234EFGH/Internal storage/", &[]), Some("bm-missing-phone"));
        assert_eq!(missing("sftp://me@nas.lan/srv/", &[]), None);
        assert_eq!(missing("ftp://nas.lan/Photos/", &[]), None);
    }

    /// Settings written before bookmarks existed read with none; bookmarks are written and read back.
    #[test]
    fn kept_in_settings() {
        let s: crate::settings::Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50}"#).unwrap();
        assert!(s.bookmarks.is_empty());
        let mut s = s;
        add(&mut s.bookmarks, "Camera", "mtp://ABCD1234EFGH/Internal storage/DCIM/Camera/");
        let back: crate::settings::Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.bookmarks, s.bookmarks);
    }
}
