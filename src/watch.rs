//! Folders read again by themselves: the open local folder watched for changes made outside
//! EyeCanSee, and a phone without storage tried again.

use std::sync::mpsc::Sender;

use eframe::egui;

use crate::loader::Msg;
use crate::{App, source};

/// How long the "file removed" note stays in the status bar.
const NOTE_SECS: f64 = 10.0;

/// Watcher of the open local folder: every change (not a mere read) that can alter the grid
/// sends `Msg::Changed`.
pub(crate) fn folder_watcher(tx: Sender<Msg>, ctx: egui::Context) -> Option<notify::RecommendedWatcher> {
    notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
        let relevant = e.is_ok_and(|e| !e.kind.is_access() && (e.paths.is_empty() || e.paths.iter().any(|p| shown(p))));
        if relevant && tx.send(Msg::Changed).is_ok() {
            ctx.request_repaint();
        }
    })
    .inspect_err(|e| log!("folder watcher: ERROR {e}"))
    .ok()
}

/// Whether a changed path can be in the grid: anything but an existing file that is not an
/// image (folders, images, and paths already gone, which may have been either). A file written
/// all the time in the open folder, such as the log of `EYECANSEE_LOG=1 … 2> registro.log`,
/// made it read the folder again every 0.3 s, and each reading wrote to the log. Neither a file
/// a conversion is writing: it is read again once closed (`batch_progress`).
fn shown(path: &std::path::Path) -> bool {
    !crate::batch::writing(path) && (!path.is_file() || path.file_name().is_some_and(|n| crate::decode::is_media(&n.to_string_lossy())))
}

impl App {
    /// Watches the open folder if it is on this computer. Phones and servers send nothing (F5);
    /// gvfs mounts neither: changes on the phone do not reach inotify.
    pub(crate) fn watch(&mut self) {
        use notify::Watcher;
        let Some(watcher) = &mut self.watcher else { return };
        if let Some(old) = self.watched.take() {
            let _ = watcher.unwatch(&old);
        }
        self.changed_at = None;
        if self.base.contains("://") || source::in_gvfs(&self.base, &self.dir) {
            return;
        }
        let path: std::path::PathBuf =
            std::iter::once(self.base.as_str()).chain(self.dir.split('/').filter(|s| !s.is_empty())).collect();
        match watcher.watch(&path, notify::RecursiveMode::NonRecursive) {
            Ok(()) => self.watched = Some(path),
            Err(e) => log!("watch {path:?}: ERROR {e}"),
        }
    }


    /// Phone without storage (locked, charging or not yet in "File transfer") or not found (it
    /// reconnects when switched to "File transfer", or was unplugged): the folder is
    /// read again by itself every `RETRY_SECS`, without F5. Until then the message stays.
    /// Reads the open folder again after a change on disk, once the events have calmed down.
    pub(crate) fn reread_changed(&mut self, ctx: &egui::Context) {
        let Some(at) = self.changed_at else { return };
        let now = ctx.input(|i| i.time);
        if now < at {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            return;
        }
        let Some(loader) = &self.loader else { return };
        // A listing already on its way may have been read before the change: try again after it.
        if !self.listing_pending.insert(self.dir.clone()) {
            self.changed_at = Some(now + 0.3);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.3));
            return;
        }
        self.changed_at = None;
        log!("changed on disk {:?}", self.dir);
        self.relisting.insert(self.dir.clone());
        loader.list(self.dir.clone());
    }

    /// A read found a file of the open folder gone (deleted on the phone itself, say): the folder
    /// is read again, which drops it from the grid and moves the viewer to the next file, and the
    /// status bar says so. Phones send no events: this is how the app finds out.
    pub(crate) fn notice_gone(&mut self, ctx: &egui::Context) {
        // The note goes away by itself after a while.
        if let Some(until) = self.note_until {
            let now = ctx.input(|i| i.time);
            if now >= until {
                (self.status_note, self.note_until) = (None, None);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - now));
            }
        }
        let gone = source::take_gone();
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        // A read found it gone, and the folder is not being read again already.
        let Some(path) = gone.iter().find(|p| l.files.iter().any(|f| f.path == **p)) else { return };
        if self.relisting.contains(&self.dir) {
            return;
        }
        log!("gone from the device: {path:?}");
        self.relist(self.dir.clone());
        self.status_note = Some(self.i18n.t_args("status-file-removed", &[("name", source::name(path))]));
        self.note_until = Some(ctx.input(|i| i.time) + NOTE_SECS);
    }

    pub(crate) fn retry(&mut self, ctx: &egui::Context) {
        let Some(at) = self.retry_at else { return };
        let now = ctx.input(|i| i.time);
        if now < at {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(at - now));
            return;
        }
        self.retry_at = None;
        if let Some(loader) = &self.loader
            && self.listing_pending.insert(self.dir.clone())
        {
            log!("retry {:?}", self.dir);
            loader.list(self.dir.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shown() {
        let dir = std::env::temp_dir().join(format!("eyecansee-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("registro.log"), "x").unwrap();
        std::fs::write(dir.join("a.jpg"), "x").unwrap();
        assert!(!super::shown(&dir.join("registro.log")), "a file not shown");
        assert!(super::shown(&dir.join("a.jpg")), "an image");
        assert!(super::shown(&dir), "a folder");
        assert!(super::shown(&dir.join("gone")), "something removed");
        let writing = crate::batch::Writing::start(dir.join("a.jpg"));
        assert!(!super::shown(&dir.join("a.jpg")), "an image a conversion is writing");
        drop(writing);
        assert!(super::shown(&dir.join("a.jpg")), "closed again");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
