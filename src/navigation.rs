//! Moving between sources and folders: opening an address, the current folder and its listing,
//! next and previous, the sort order and the window title.

use eframe::egui;

use crate::address::without_password;
use crate::loader::Loader;
use crate::source::{self, File};
use crate::{App, devices, settings};

/// #206: the folder to open instead of `dir` on the phone or server `base`, entered from the place
/// `from` by the row `row` of the left panel: the last one visited after entering by that row
/// (`last`: row → its address with that folder), when it is inside `dir`. Two saved servers with
/// the same login are one place but two rows: each keeps its own. `None` on the same place, on a
/// disk, or with nothing remembered there.
fn last_folder_in(last: &std::collections::HashMap<String, String>, from: &str, base: &str, row: &str, dir: &str) -> Option<String> {
    if base == from || !base.contains("://") {
        return None;
    }
    let folder = last.get(row)?.strip_prefix(base)?;
    (folder.starts_with(dir) && folder != dir).then(|| folder.to_string())
}

/// #314: the folder last visited inside the device or saved server of the panel's row `row` (its
/// address), shown under its name: that folder's address and the part after `row`. A phone or a
/// server is remembered under its row (`last`, as in `last_folder_in`); a device of this computer
/// (a USB stick) under the computer's own entries (no "://"). `None` at its root or with nothing
/// remembered there.
pub(crate) fn folder_under(last: &std::collections::HashMap<String, String>, row: &str) -> Option<(String, String)> {
    let remote = row.contains("://");
    last.iter().filter(|(k, _)| if remote { *k == row } else { !k.contains("://") }).find_map(|(_, full)| Some((full.clone(), rest_under(full, row)?)))
}

/// #314: the part of the address `full` inside the row `row` ("photos/2024" for
/// "sftp://me@nas/photos/2024/" under "sftp://me@nas/"), `None` outside it or at its root.
pub(crate) fn rest_under(full: &str, row: &str) -> Option<String> {
    let rest = full.strip_prefix(row)?.trim_end_matches('/');
    (!rest.is_empty()).then(|| rest.to_string())
}

impl App {
    /// #314: on this computer, the external disk holding the folder `dir` (its path, as in the
    /// Devices block), if any.
    pub(crate) fn disk_of(&self, dir: &str) -> Option<String> {
        let disks = self.devices.iter().filter(|d| !d.internal && d.block.is_none() && d.path.starts_with('/'));
        disks.map(|d| d.path.trim_start_matches('/')).filter(|p| dir.starts_with(*p)).max_by_key(|p| p.len()).map(|p| format!("/{p}"))
    }

    /// #314: the key of `last_dirs` for the folder `dir` of this computer: its external disk, or
    /// the computer itself ("/").
    fn local_key(&self, dir: &str) -> String {
        self.disk_of(dir).unwrap_or_else(|| "/".to_string())
    }

    /// Opens an address (path or URL). If it fails, stays on the current source. The image in the
    /// viewer with turns not saved is asked about first (audit 1.5, #198: an address typed in the
    /// bar dropped them, and the image opened next in the same place showed them).
    pub(crate) fn open_source(&mut self, ctx: &egui::Context, address: &str) {
        self.open_place(ctx, address, false);
    }

    /// A phone or a saved server from the left panel: back there from another place, it opens at
    /// the folder last visited there while EyeCanSee stays open, if that one is inside the folder
    /// of `address` (a saved server's own folder, or the root); otherwise as `open_source`. A
    /// folder gone meanwhile gives its first folder, as `listing_error` does (#206).
    pub(crate) fn open_back(&mut self, ctx: &egui::Context, address: &str) {
        self.open_place(ctx, address, true);
    }

    fn open_place(&mut self, ctx: &egui::Context, address: &str, back: bool) {
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Open(address.to_string())) {
            return;
        }
        // A server while the saved ones are locked: the master password first (they may hold its
        // password and key).
        if crate::servers::locked() && crate::vault_page::remote(address) {
            self.ask_master(crate::vault_page::Then::Open(address.to_string()));
            return;
        }
        log!("open source {:?}", without_password(address));
        let opened = {
            let _guard = self.rt.enter();
            source::open(address.trim())
        };
        match opened {
            Ok(mut s) => {
                // A phone or a server from its row: its last folder is kept under the row's address.
                let row = (back && s.base.contains("://")).then(|| without_password(address));
                if let Some(row) = &row
                    && let Some(dir) = last_folder_in(&self.last_dirs, &self.base, &s.base, row, &s.dir)
                {
                    log!("back on {row:?}: {dir:?}");
                    (s.dir, s.start, s.home) = (dir, None, false);
                }
                let other = s.base != self.base;
                if other {
                    self.leave_source();
                }
                if !self.base.is_empty() {
                    let key = if self.base == "/" { self.local_key(&self.dir) } else { self.place_key.clone().unwrap_or_else(|| self.base.clone()) };
                    self.last_dirs.insert(key, format!("{}{}", self.base, self.dir));
                }
                if other || row.is_some() {
                    self.place_key = row;
                }
                let handle = self.rt.handle().clone();
                self.loader = Some(Loader::new(handle, s.op, s.base.clone(), self.tx.clone(), ctx.clone()));
                self.base = s.base;
                self.listings.clear();
                // A file to select belongs to the place before.
                self.select_after = None;
                self.listing_pending.clear();
                self.relist_again.clear();
                self.error = None;
                self.status_note = None;
                self.set_dir(s.dir);
                self.start = s.start;
                if s.home && let Some(loader) = &self.loader {
                    loader.home();
                }
                self.maintain();
            }
            Err((id, detail)) => {
                log!("open failed: {id} {detail}");
                self.error = Some(self.i18n.error(id, &detail));
                self.address = self.display(&self.dir);
            }
        }
    }

    /// Another place opens (an address from outside, a device unplugged, a phone through its
    /// mount): the windows that act on paths of the place left close, or their action went to the
    /// new place with the same paths (audit 1.4, #197: a deletion asked on a server sent the
    /// computer's file of the same path to the trash). Save in another format stays on its image,
    /// in the place left, as from the editor; a conversion already started has its own place.
    fn leave_source(&mut self) {
        log!("another place: closing the windows of {:?}", without_password(&self.base));
        self.stop_delete_check();
        self.delete_confirm = None;
        self.restore_conflict = None;
        self.restore_queue.clear();
        // Their ends, about the place left, are dropped (`Msg::At`, audit 27/11 F.9): nothing
        // else would end them here, and Delete and Restore would wait for them.
        (self.deleting, self.restore_busy) = (0, false);
        self.paste_ask = None;
        self.batch_turn = None;
        self.rename = None;
        self.new_folder = None;
        self.model_export = None;
        (self.save_ready, self.after_save, self.after_turn, self.ffmpeg_ask) = (None, None, None, None);
        if let Some(w) = self.duplicates.take() {
            w.stop();
        }
        if self.batch.as_ref().is_some_and(crate::batch_page::Window::asking) {
            self.batch = None;
        }
        if let (Some(w), Some(loader)) = (&mut self.convert, &self.loader)
            && w.place.is_none()
        {
            w.place = Some(crate::edit_page::Place { store: loader.store(), base: self.base.clone() });
        }
    }

    pub(crate) fn maintain(&mut self) {
        if self.settings.thumb_maintenance
            && let Some(loader) = &self.loader
            && self.maintained.insert(self.base.clone())
        {
            loader.maintain(self.settings.thumb_cache_mb as u64 * 1024 * 1024);
        }
    }

    /// Opens an address from outside (command line, another instance); a temporary copy made by
    /// Windows is said so in the status bar (issue #47). Turns not saved in the viewer are asked
    /// about first, as by `open_source`.
    pub(crate) fn open_outside(&mut self, ctx: &egui::Context, address: &str) {
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Outside(address.to_string())) {
            return;
        }
        let address = self.outside_address(address);
        self.open_source(ctx, &address);
        if source::windows_temp_copy(&address) {
            self.status_note = Some(self.i18n.t("status-temp-copy"));
        }
    }

    /// The address from outside held while the editor was open (`hold`), once it is closed.
    pub(crate) fn open_held(&mut self, ctx: &egui::Context) {
        if let Some(address) = release(self.editor.is_some(), &mut self.held_open) {
            log!("editor closed: opening {:?}", without_password(&address));
            self.open_outside(ctx, &address);
        }
    }

    /// An address from outside (command line, the file manager): a phone file mounted by gvfs
    /// opens through the direct connection (`devices::direct_address`).
    fn outside_address(&self, address: &str) -> String {
        match devices::direct_address(address, &self.devices) {
            Some(direct) => {
                log!("{address:?} opened directly as {direct:?}");
                direct
            }
            None => address.to_string(),
        }
    }

    /// For the `mtp://<serial>/` source, the desktop's mount of the same phone.
    pub(crate) fn mounted_twin(&self) -> Option<String> {
        devices::gvfs_mount(self.base.strip_prefix("mtp://")?.trim_end_matches('/'))
    }

    /// A listing that failed because the phone could not be opened directly, while the desktop
    /// has it mounted: the same folder through that mount (slower; the storages have the same
    /// name). Held by another program ("error-device-busy") it switches quietly, as before; failed
    /// for a reason of its own ("error-use-mount") the status bar says so, with that reason.
    pub(crate) fn use_mount(&mut self, ctx: &egui::Context, dir: &str, r: &Result<source::Listing, source::Error>) -> bool {
        let Err((id @ ("error-device-busy" | "error-use-mount"), detail)) = r else { return false };
        let Some(mounted) = self.mounted_twin() else { return false };
        let note = (*id == "error-use-mount").then(|| self.i18n.t_args("status-through-mount", &[("detail", detail)]));
        // The phone cannot be read directly: its turns not saved could not be saved there.
        self.turn = Default::default();
        self.open_source(ctx, &format!("{mounted}{dir}"));
        self.status_note = note;
        true
    }

    pub(crate) fn display(&self, dir: &str) -> String {
        #[cfg(feature = "shots")]
        return crate::shots::made_up_serial(shown(&self.base, dir));
        #[cfg(not(feature = "shots"))]
        shown(&self.base, dir)
    }

    pub(crate) fn set_dir(&mut self, dir: String) {
        log!("folder {dir:?}");
        self.reveal = true;
        // On this computer, the last folder of each part (the computer, each external disk) for
        // the left panel, where the parts not in use show it under their name (#314).
        if self.base == "/" {
            self.last_dirs.insert(self.local_key(&dir), format!("/{dir}"));
        }
        if let Some(loader) = &self.loader {
            self.generation = loader.next_generation();
        }
        self.address = self.display(&dir);
        // A file to select belongs to the folder before (the same one sorted again keeps it).
        if dir != self.dir {
            self.select_after = None;
        }
        self.dir = dir;
        self.thumbs.clear();
        self.generated.clear();
        self.thumb_infos.clear();
        self.info_asked.clear();
        self.thumb_queue_key = None;
        self.full.clear();
        self.pending.clear();
        self.selected = 0;
        self.cursor = false;
        self.up_selected = false;
        self.marked.clear();
        self.anchor = 0;
        self.scroll_top = true;
        self.meta = None;
        self.meta_asked = None;
        self.retry_at = None;
        self.viewing = None;
        // The viewer's turns and zoom belong to the image of the folder left: by position, they
        // went to the image opened next at the same place (audit 1.5, #198).
        self.turn = Default::default();
        self.zoom_of = None;
        self.start = None;
        let dir = self.dir.clone();
        self.request_listing(&dir);
        // A folder read before is shown at once, and read again: its changes made meanwhile were
        // seen only after F5, as only the open folder is watched (audit 1.7). On this computer
        // only, where it costs little (choice D4); the new listing takes its place in
        // `on_listed`, and if it is the same nothing changes.
        if self.listings.contains_key(&dir) && self.on_this_computer(&dir) {
            self.relist(dir);
        }
        self.watch();
    }

    pub(crate) fn go_up(&mut self) {
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Up) {
            return;
        }
        if !self.dir.is_empty() {
            let child = self.dir.clone();
            self.set_dir(source::parent(&child));
            // The folder we came up from is selected.
            if let Some(Ok(l)) = self.listings.get(&self.dir)
                && let Some(i) = l.dirs.iter().position(|d| *d == child)
            {
                self.selected = i;
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
    }

    /// Reads the folders again. The viewer keeps the same image open, if it still exists: until
    /// the new listing arrives the old one is used, so the grid does not flash for a moment.
    pub(crate) fn refresh(&mut self) {
        log!("refresh");
        let viewing = self.viewing;
        let path = viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone());
        let view = (self.zoom_of, self.turn, self.zoom);
        // Only the folder open (Piero, 26/11): every listing was emptied, and the left panel read
        // again each folder it shows open, on a phone one call after the other.
        let old = self.listings.remove(&self.dir);
        let dir = self.dir.clone();
        self.set_dir(dir);
        if let (Some(i), Some(path), Some(old)) = (viewing, path, old) {
            self.listings.insert(self.dir.clone(), old);
            self.viewing = Some(i);
            self.start = Some(path);
            // The same image: its zoom and turns not saved stay.
            (self.zoom_of, self.turn, self.zoom) = view;
        }
    }

    pub(crate) fn request_listing(&mut self, dir: &str) {
        if let Some(loader) = &self.loader
            && !self.listings.contains_key(dir)
            && self.listing_pending.insert(dir.to_string())
        {
            loader.list(dir.to_string());
        }
    }

    pub(crate) fn files(&self) -> &[File] {
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => &l.files,
            _ => &[],
        }
    }

    /// Next, previous, first, last: same action from the keyboard and from the Go menu.
    /// In the viewer it changes image (after the last one back to the first only with `wrap_around`,
    /// otherwise it stops there), in the grid it moves the selection.
    pub(crate) fn go(&mut self, to: Go) {
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Go(to)) {
            return;
        }
        if let Some(current) = self.viewing {
            let set = self.viewer_set();
            if let Some(at) = set.iter().position(|&i| i == current) {
                self.viewing = Some(set[step(at, set.len(), to, self.settings.wrap_around)]);
            }
        } else {
            let n = self.items();
            if n > 0 {
                // With no selection shown, next and previous start from the first item.
                self.selected = match to {
                    Go::Next if self.cursor => (self.selected + 1).min(n - 1),
                    Go::Prev => self.selected.saturating_sub(1),
                    Go::First | Go::Next => 0,
                    Go::Last => n - 1,
                };
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
    }

    /// Grid items: folders plus images.
    pub(crate) fn items(&self) -> usize {
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.dirs.len() + l.files.len(),
            _ => 0,
        }
    }

    pub(crate) fn update_title(&mut self, ctx: &egui::Context) {
        let editing = self.editor.as_ref().map(|w| source::shown(&w.path));
        let viewing = self.viewing.filter(|_| editing.is_none()).map(|i| {
            // Among the images it moves through: "2 of 3" with three marked.
            let set = self.viewer_set();
            let at = set.iter().position(|&k| k == i).unwrap_or(0);
            let counter = self.i18n.t_args("counter", &[("n", &(at + 1).to_string()), ("total", &set.len().to_string())]);
            (self.files().get(i).map_or("", |f| source::shown(&f.path)), counter)
        });
        let title = window_title(editing, viewing, &self.display(&self.dir));
        if title != self.title {
            log!("title {title:?}");
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Changes the sort order: re-sorts the listings already read and restarts from the current
    /// folder (thumbnails and selection are by position), keeping the same image selected. From
    /// the viewer, the viewer stays on its image, with its zoom and turns not saved: they went to
    /// the image sorted into its place, and Save wrote that one turned (audit 1.5, #198).
    pub(crate) fn set_sort(&mut self, by: settings::SortBy, desc: bool) {
        self.settings.sort_by = by;
        self.settings.sort_desc = desc;
        self.settings.save();
        let selected = self.cursor.then(|| self.selected.checked_sub(self.listing_dirs())).flatten();
        let path = selected.and_then(|i| self.files().get(i)).map(|f| f.path.clone());
        let viewed = self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone());
        let view = (self.turn, self.zoom);
        for l in self.listings.values_mut().flatten() {
            l.sort_files(by, desc);
        }
        self.set_dir(self.dir.clone());
        if let Some(path) = path
            && let Some(i) = self.files().iter().position(|f| f.path == path)
        {
            self.selected = self.listing_dirs() + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
        if let Some(path) = viewed
            && let Some(i) = self.files().iter().position(|f| f.path == path)
        {
            self.viewing = Some(i);
            self.zoom_of = Some(i);
            (self.turn, self.zoom) = view;
        }
    }

    /// Images, videos or both: the folders are read again with the new filter.
    pub(crate) fn set_show(&mut self, show: settings::Show) {
        self.settings.show = show;
        self.settings.save();
        self.refresh();
    }

    /// Number of subfolders of the current folder (they come before the images).
    pub(crate) fn listing_dirs(&self) -> usize {
        match self.listings.get(&self.dir) {
            Some(Ok(l)) => l.dirs.len(),
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Go {
    Next,
    Prev,
    First,
    Last,
}

/// The position after `at` among `n` (`n` > 0) going `to`; `wrap`: past the end back to the start.
pub(crate) fn step(at: usize, n: usize, to: Go, wrap: bool) -> usize {
    match to {
        Go::Next if wrap => (at + 1) % n,
        Go::Prev if wrap => (at + n - 1) % n,
        Go::Next => (at + 1).min(n - 1),
        Go::Prev => at.saturating_sub(1),
        Go::First => 0,
        Go::Last => n - 1,
    }
}

/// An address typed or pasted in the bar: spaces around it go, and so do quotes around all of it,
/// as Windows' "Copy as path" puts them (`"C:\Photos\"`). On Windows a drive alone (`C:`) is its
/// root, as in Explorer, not the drive's current folder (the app's working folder, often System32).
/// `~` at the start is the home folder, as in a terminal and in the file managers' address bars.
pub(crate) fn typed_address(a: &str) -> String {
    let a = a.trim();
    let a = a.strip_prefix('"').and_then(|a| a.strip_suffix('"')).map_or(a, str::trim);
    let rest = (a == "~").then_some("").or_else(|| a.strip_prefix("~/")).or_else(|| a.strip_prefix("~\\"));
    if let (Some(rest), Some(home)) = (rest, std::env::home_dir()) {
        return home.join(rest).to_string_lossy().into_owned();
    }
    match a.as_bytes() {
        [d, b':'] if cfg!(windows) && d.is_ascii_alphabetic() => format!("{a}\\"),
        _ => a.to_string(),
    }
}

/// The window's title: with the editor open the name of the file edited (in applications working
/// on documents the title is the document's name, GNOME's guidelines), else in the viewer the image
/// shown and its `counter`, else the `folder`.
pub(crate) fn window_title(editing: Option<&str>, viewing: Option<(&str, String)>, folder: &str) -> String {
    match (editing, viewing) {
        (Some(name), _) => format!("{name} - EyeCanSee"),
        (None, Some((name, counter))) => format!("{name} ({counter}) - EyeCanSee"),
        (None, None) => format!("{folder} - EyeCanSee"),
    }
}

/// A folder of a source as the address bar shows it: `base` plus the folder, with `\` on Windows.
pub(crate) fn shown(base: &str, dir: &str) -> String {
    let sep = if base.contains("://") || !base.contains('\\') { "/" } else { "\\" };
    format!("{base}{}", dir.replace('/', sep))
}

/// An address from outside (`""`: just come forward): `Some` to open now; kept in `held` while
/// the editor is open (the last one if more arrive), so the grid does not change place under the
/// page. Until 0.77.0 it opened at once, and the page's saves went to the new place.
pub(crate) fn hold(editing: bool, held: &mut Option<String>, address: String) -> Option<String> {
    if editing && !address.is_empty() {
        *held = Some(address);
        None
    } else {
        Some(address)
    }
}

/// The address held by `hold`, to open now that the editor is closed.
fn release(editing: bool, held: &mut Option<String>) -> Option<String> {
    if editing { None } else { held.take() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// F5 reads again only the folder open (Piero, 26/11): it emptied every listing read, and the
    /// left panel read again each folder it shows open; on a phone busy with a big file all their
    /// rows spun.
    #[test]
    fn f5_reads_again_only_the_folder_open() {
        if crate::audit_family_1::own_folder("navigation::tests::f5_reads_again_only_the_folder_open") {
            return;
        }
        let dir = crate::audit_family_1::folder("f5-only", &["a.png"]);
        crate::audit_family_1::folder("f5-only/sub", &["b.png"]);
        let mut h = crate::audit_family_1::Harness::new(&dir);
        h.until(|a| a.listings.contains_key(&a.dir), "the folder listed");
        let sub = format!("{}sub/", h.app.dir);
        h.app.request_listing(&sub);
        h.until(|a| a.listings.contains_key(&sub), "the other folder listed");
        h.app.refresh();
        assert!(h.app.listings.contains_key(&sub), "F5 forgot a folder that is not the one open");
        assert!(!h.app.listings.contains_key(&h.app.dir), "F5 kept the folder open as it was");
        h.until(|a| a.listings.contains_key(&a.dir), "the folder listed again");
    }

    /// #206: the folder reopened back on a place: the last one visited from the row clicked,
    /// inside its folder. Two saved servers with the same login are one place: one saved on
    /// `docs/` (or on the root) does not reopen a folder visited from the other.
    #[test]
    fn last_folder_back_on_a_place() {
        let server = "sftp://me@nas/";
        let (photos, home) = ("sftp://me@nas/photos/", "sftp://me@nas/");
        let phone = "mtp://SERIAL1234/";
        let last = std::collections::HashMap::from([(photos.to_string(), format!("{server}photos/2024/")), (phone.to_string(), phone.to_string()), ("/".to_string(), "/home/me/Pictures/".to_string())]);
        assert_eq!(last_folder_in(&last, "/", server, photos, "photos/").as_deref(), Some("photos/2024/"));
        // The row of the same server on its root, never entered: its own folder.
        assert_eq!(last_folder_in(&last, "/", server, home, ""), None);
        assert_eq!(last_folder_in(&last, "/", server, "sftp://me@nas/docs/", "docs/"), None);
        // Already there: the address as it is; nothing remembered, or the root: as before.
        assert_eq!(last_folder_in(&last, server, server, photos, "photos/"), None);
        assert_eq!(last_folder_in(&last, "/", phone, phone, ""), None);
        // A disk of the computer: as before, whatever was open there.
        assert_eq!(last_folder_in(&last, server, "/", "/", "home/me/"), None);
    }

    /// #314: under a device or a saved server of the panel, the folder last visited inside it;
    /// nothing at its root, with nothing remembered, or for another row of the same server.
    #[test]
    fn folder_under_a_row() {
        let (phone, photos, stick) = ("mtp://SERIAL1234/", "sftp://me@nas/photos/", "/run/media/me/STICK/");
        let at = |k: &str, v: &str| (k.to_string(), v.to_string());
        let last = std::collections::HashMap::from([
            at(phone, "mtp://SERIAL1234/Internal storage/DCIM/Camera/"),
            at(photos, "sftp://me@nas/photos/2024/"),
            at("/", "/run/media/me/STICK/DCIM/"),
        ]);
        let rest = |row: &str| folder_under(&last, row).map(|(_, r)| r);
        assert_eq!(folder_under(&last, phone), Some(("mtp://SERIAL1234/Internal storage/DCIM/Camera/".into(), "Internal storage/DCIM/Camera".into())));
        assert_eq!(rest(photos).as_deref(), Some("2024"));
        assert_eq!(rest(stick).as_deref(), Some("DCIM"));
        // Another row of the same server, another stick, a phone never opened: nothing.
        assert_eq!(rest("sftp://me@nas/docs/"), None);
        assert_eq!(rest("sftp://me@nas/"), None);
        assert_eq!(rest("/run/media/me/OTHER/"), None);
        assert_eq!(rest("mtp://OTHER5678/"), None);
        // At its root: nothing under the name.
        let root = std::collections::HashMap::from([at(phone, phone)]);
        assert_eq!(folder_under(&root, phone), None);
        // The folder open on a server, under the row it is in (#314, Piero 25/11).
        assert_eq!(rest_under("sftp://me@nas/photos/2024/", "sftp://me@nas/").as_deref(), Some("photos/2024"));
        assert_eq!(rest_under("sftp://me@nas/docs/", "sftp://me@nas/photos/"), None);
        assert_eq!(rest_under("sftp://me@nas/photos/", "sftp://me@nas/photos/"), None);
    }

    /// #314: on this computer an external disk keeps its last folder apart from the computer's,
    /// so the panel shows each under its own name.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn last_folder_of_each_disk() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("navigation::tests::last_folder_of_each_disk") {
            return;
        }
        let dir = folder("disk-last", &["a.png"]);
        std::fs::create_dir_all(dir.join("disk/DCIM")).unwrap();
        std::fs::create_dir_all(dir.join("home/Pictures")).unwrap();
        let mut h = Harness::new(&dir);
        let root = h.app.dir.clone();
        let disk = format!("/{root}disk/");
        h.app.devices = vec![devices::Device { name: "Disk".into(), path: disk.clone(), phone: false, serial: None, block: None, internal: false }];
        h.app.set_dir(format!("{root}home/Pictures/"));
        h.app.set_dir(format!("{root}disk/DCIM/"));
        assert_eq!(h.app.disk_of(&h.app.dir), Some(disk.clone()));
        assert_eq!(h.app.last_dirs.get(&disk), Some(&format!("/{root}disk/DCIM/")));
        assert_eq!(h.app.last_dirs.get("/"), Some(&format!("/{root}home/Pictures/")), "the computer keeps its own");
        assert_eq!(folder_under(&h.app.last_dirs, &disk).map(|(_, r)| r).as_deref(), Some("DCIM"));
    }

    /// Audit 27/11 F.12 (#314): a disk unplugged and plugged again (or another stick with the same
    /// label, so the same place) opened at the folder last visited on it even when that folder was
    /// gone: an empty grid, no error, and that folder under its name in the panel.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_disk_opens_at_its_root_when_its_last_folder_is_gone() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("navigation::tests::a_disk_opens_at_its_root_when_its_last_folder_is_gone") {
            return;
        }
        let dir = folder("disk-gone", &["a.png"]);
        std::fs::create_dir_all(dir.join("disk/DCIM")).unwrap();
        std::fs::create_dir_all(dir.join("home")).unwrap();
        let mut h = Harness::new(&dir);
        let root = h.app.dir.clone();
        let disk = format!("/{root}disk/");
        let stick = devices::Device { name: "Disk".into(), path: disk.clone(), phone: false, serial: None, block: None, internal: false };
        h.app.devices = vec![stick.clone()];
        h.app.set_dir(format!("{root}disk/DCIM/"));
        h.app.set_dir(format!("{root}home/"));
        std::fs::remove_dir(dir.join("disk/DCIM")).unwrap();
        let ctx = h.ctx.clone();
        h.app.open_device(&ctx, &stick);
        assert_eq!(h.app.dir, format!("{root}disk/"), "opened at a folder no longer on the disk");
        // Unplugged: what was remembered of it goes with it.
        h.app.set_dir(format!("{root}home/"));
        h.app.tx.send(crate::loader::Msg::Devices(Vec::new())).unwrap();
        h.frame(Vec::new());
        assert_eq!(h.app.last_dirs.get(&disk), None, "the last folder of a disk unplugged kept");
    }

    /// An address from outside while the editor is open is kept (the last one) and opened once it
    /// is closed; without the editor it opens at once, and "" (just come forward) is never kept.
    #[test]
    fn address_held_while_editing() {
        let mut held = None;
        assert_eq!(hold(true, &mut held, "/home/u/a.jpg".into()), None);
        assert_eq!(hold(true, &mut held, "/home/u/b.jpg".into()), None);
        assert_eq!(hold(true, &mut held, String::new()), Some(String::new()));
        assert_eq!(release(true, &mut held), None, "still editing");
        assert_eq!(release(false, &mut held).as_deref(), Some("/home/u/b.jpg"));
        assert_eq!(release(false, &mut held), None, "once");
        assert_eq!(hold(false, &mut held, "/home/u/c.jpg".into()).as_deref(), Some("/home/u/c.jpg"));
        assert_eq!(held, None);
    }

    #[test]
    fn typed_address_without_quotes() {
        assert_eq!(typed_address(r#" "C:\Photos\" "#), r"C:\Photos\");
        assert_eq!(typed_address(r#""/home/a b""#), "/home/a b");
        assert_eq!(typed_address("sftp://host/dir"), "sftp://host/dir");
        // Only a pair around all of it.
        assert_eq!(typed_address(r#"C:\a"b"#), r#"C:\a"b"#);
        assert_eq!(typed_address(r#""C:\Photos"#), r#""C:\Photos"#);
        // A drive alone: its root on Windows.
        assert_eq!(typed_address(r#" "d:" "#), if cfg!(windows) { r"d:\" } else { "d:" });
        assert_eq!(typed_address("C:\\"), "C:\\");
    }

    #[test]
    fn typed_address_home() {
        let home = std::env::home_dir().unwrap();
        assert_eq!(typed_address(" ~/Pictures/a.jpg "), home.join("Pictures/a.jpg").to_string_lossy());
        assert_eq!(typed_address("~"), home.join("").to_string_lossy());
        assert_eq!(typed_address(r#""~/a b""#), home.join("a b").to_string_lossy());
        // Only at the start, and not another user's home.
        assert_eq!(typed_address("/tmp/~/a"), "/tmp/~/a");
        assert_eq!(typed_address("~other/a"), "~other/a");
    }

    #[test]
    fn steps() {
        // Three marked images in the viewer: positions among them.
        assert_eq!(step(2, 3, Go::Next, true), 0);
        assert_eq!(step(2, 3, Go::Next, false), 2);
        assert_eq!(step(0, 3, Go::Prev, true), 2);
        assert_eq!(step(0, 3, Go::Prev, false), 0);
        assert_eq!(step(1, 3, Go::First, false), 0);
        assert_eq!(step(1, 3, Go::Last, false), 2);
        assert_eq!(step(0, 1, Go::Next, true), 0);
    }

    #[test]
    fn titles() {
        // The grid: the folder.
        assert_eq!(window_title(None, None, "/home/a/Pictures"), "/home/a/Pictures - EyeCanSee");
        // The viewer: the image and where it is among the others.
        assert_eq!(window_title(None, Some(("b.jpg", "2 of 5".into())), "/home/a/Pictures"), "b.jpg (2 of 5) - EyeCanSee");
        // The editor, opened from the grid, from the viewer or on a new image: the file edited.
        assert_eq!(window_title(Some("b.jpg"), None, "/home/a/Pictures"), "b.jpg - EyeCanSee");
        assert_eq!(window_title(Some("b.jpg"), Some(("c.jpg", "3 of 5".into())), "/home/a/Pictures"), "b.jpg - EyeCanSee");
        assert_eq!(window_title(Some("New image.png"), None, "/home/a/Pictures"), "New image.png - EyeCanSee");
    }
}
