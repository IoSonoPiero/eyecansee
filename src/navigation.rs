//! Moving between sources and folders: opening an address, the current folder and its listing,
//! next and previous, the sort order and the window title.

use eframe::egui;

use crate::address::without_password;
use crate::loader::Loader;
use crate::source::{self, File};
use crate::{App, devices, settings};

impl App {
    /// Opens an address (path or URL). If it fails, stays on the current source.
    /// A place of the left panel: the folder where it was left, if it was open before.
    pub(crate) fn back_to(&self, address: &str) -> String {
        let base = source::open(address).map(|s| s.base).unwrap_or_default();
        self.last_dirs.get(&base).cloned().unwrap_or_else(|| address.to_string())
    }

    pub(crate) fn open_source(&mut self, ctx: &egui::Context, address: &str) {
        log!("open source {:?}", without_password(address));
        let opened = {
            let _guard = self.rt.enter();
            source::open(address.trim())
        };
        match opened {
            Ok(s) => {
                if !self.base.is_empty() {
                    self.last_dirs.insert(self.base.clone(), format!("{}{}", self.base, self.dir));
                }
                let handle = self.rt.handle().clone();
                self.loader = Some(Loader::new(handle, s.op, s.base.clone(), self.tx.clone(), ctx.clone()));
                self.base = s.base;
                self.listings.clear();
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
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                self.address = self.display(&self.dir);
            }
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
        self.open_source(ctx, &format!("{mounted}{dir}"));
        self.status_note = note;
        true
    }

    pub(crate) fn display(&self, dir: &str) -> String {
        shown(&self.base, dir)
    }

    pub(crate) fn set_dir(&mut self, dir: String) {
        log!("folder {dir:?}");
        self.reveal = true;
        if let Some(loader) = &self.loader {
            self.generation = loader.next_generation();
        }
        self.address = self.display(&dir);
        self.dir = dir;
        self.thumbs.clear();
        self.generated.clear();
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
        self.start = None;
        let dir = self.dir.clone();
        self.request_listing(&dir);
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
        let old = self.listings.remove(&self.dir);
        self.listings.clear();
        let dir = self.dir.clone();
        self.set_dir(dir);
        if let (Some(i), Some(path), Some(old)) = (viewing, path, old) {
            self.listings.insert(self.dir.clone(), old);
            self.viewing = Some(i);
            self.start = Some(path);
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
        let title = match self.viewing {
            Some(i) => {
                // Among the images it moves through: "2 of 3" with three marked.
                let set = self.viewer_set();
                let at = set.iter().position(|&k| k == i).unwrap_or(0);
                let counter = self.i18n.t_args("counter", &[("n", &(at + 1).to_string()), ("total", &set.len().to_string())]);
                let name = self.files().get(i).map_or("", |f| source::shown(&f.path));
                format!("{name} ({counter}) - EyeCanSee")
            }
            None => format!("{} - EyeCanSee", self.display(&self.dir)),
        };
        if title != self.title {
            log!("title {title:?}");
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    /// Changes the sort order: re-sorts the listings already read and restarts from the current
    /// folder (thumbnails and selection are by position), keeping the same image selected.
    pub(crate) fn set_sort(&mut self, by: settings::SortBy, desc: bool) {
        self.settings.sort_by = by;
        self.settings.sort_desc = desc;
        self.settings.save();
        let selected = self.cursor.then(|| self.selected.checked_sub(self.listing_dirs())).flatten();
        let path = selected.and_then(|i| self.files().get(i)).map(|f| f.path.clone());
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

/// A folder of a source as the address bar shows it: `base` plus the folder, with `\` on Windows.
pub(crate) fn shown(base: &str, dir: &str) -> String {
    let sep = if base.contains("://") || !base.contains('\\') { "/" } else { "\\" };
    format!("{base}{}", dir.replace('/', sep))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
