//! What the loader and the other threads send back (`Msg`): one method per message type, called by
//! `App::receive`.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui::{self, ColorImage, TextureOptions};

use crate::address::without_password;
use crate::save_dialogs::md;
use crate::{App, RETRY_SECS, devices, encode, loader, meta, source, transform, update, version};
use crate::loader::Msg;
use crate::source::Listing;

impl App {
    pub(crate) fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Listed(dir, r) => self.on_listed(ctx, dir, r),
                Msg::Loaded { generation, index, thumb, image } if generation == self.generation => self.on_loaded(ctx, index, thumb, image),
                Msg::Meta { generation, index, meta } if generation == self.generation => self.on_meta(index, meta),
                Msg::Loaded { .. } | Msg::Meta { .. } => {} // folder has changed in the meantime
                // Counted from the first event, not the last: a copy into a subfolder sends one for
                // every file, and the folder was read again only when the copy ended.
                Msg::Changed if self.changed_at.is_none() => self.changed_at = Some(ctx.input(|i| i.time) + 0.3),
                Msg::Changed => {}
                Msg::Deleted(path, how, r) => self.on_deleted(path, how, r),
                Msg::TrashItems(paths) => self.on_trash_items(paths),
                Msg::Restored(path, r) => self.on_restored(path, r),
                Msg::Renamed(old, r) => self.on_renamed(old, r),
                Msg::PasteCheck(r) => self.on_paste_check(r),
                Msg::Pasted(gone, r) => self.on_pasted(ctx, gone, r),
                Msg::Mounted(r) => self.on_mounted(ctx, r),
                Msg::Open(address) => self.on_open(ctx, address),
                // Shown in a window only when asked from the Help menu (the window is open).
                Msg::Original(r) if self.rotate.is_some() => self.rotate_original(ctx, r),
                Msg::Original(r) => self.convert_original(ctx, r),
                Msg::Edited(path, r) => self.on_edited(path, r),
                Msg::Encoded(generation, r) => self.convert_encoded(ctx, generation, r),
                Msg::Saved(r) => self.convert_saved(r),
                Msg::Prepared(path, turn, r) => self.on_prepared(path, turn, r),
                Msg::Replaced(path, r) => self.on_replaced(ctx, path, r),
                Msg::Forgotten(paths) => self.on_forgotten(paths),
                Msg::Update(r) => self.on_update(r),
                Msg::Devices(d) => self.on_devices(ctx, d),
            }
        }
    }

    fn on_listed(&mut self, ctx: &egui::Context, dir: String, r: Result<Listing, source::Error>) {
        match &r {
            Ok(l) => log!("listing {dir:?}: {} folders, {} images", l.dirs.len(), l.files.len()),
            Err(e) => log!("listing {dir:?}: ERROR {e:?}"),
        }
        self.listing_pending.remove(&dir);
        let mut r = r;
        if let Ok(l) = &mut r {
            l.sort_files(self.settings.sort_by, self.settings.sort_desc);
        }
        if let Err(("error-device-kde", _)) = &r {
            self.kiod_ask = true;
        }
        // Direct connection impossible because the desktop holds the phone: switch to
        // its mount, in the same folder (the storages have the same name).
        if let Err(("error-device-busy", _)) = &r
            && let Some(mounted) = self.mounted_twin()
        {
            self.open_source(ctx, &format!("{mounted}{dir}"));
            return;
        }
        if dir == self.dir
            && let Some(start) = self.start.take()
        {
            let found = r.as_ref().ok().and_then(|l| {
                l.files.iter().position(|f| f.path == start).map(|i| (l.dirs.len(), i))
            });
            // After Refresh the image may have changed position or be gone:
            // images already loaded by position are no longer valid.
            if self.viewing.is_some() && self.viewing != found.map(|(_, i)| i) {
                if let Some(loader) = &self.loader {
                    self.generation = loader.next_generation();
                }
                self.full.clear();
                self.pending.clear();
                self.thumbs.clear();
                self.generated.clear();
                self.thumb_queue_key = None;
            }
            self.viewing = found.map(|(_, i)| i);
            if let Some((ndirs, i)) = found {
                self.selected = ndirs + i;
                self.cursor = true;
                self.scroll_to_selected = true;
            }
        }
        if dir == self.dir {
            let no_storage = match &r {
                Err((id, _)) => matches!(*id, "error-no-storage" | "error-no-device" | "error-not-ready"),
                Ok(l) => l.dirs.is_empty() && l.files.is_empty() && self.phone_root(&dir).is_some(),
            };
            self.retry_at = no_storage.then(|| ctx.input(|i| i.time) + RETRY_SECS);
        }
        let r = r.map_err(|(id, detail)| self.i18n.t_args(id, &[("detail", &detail)]));
        match r {
            Ok(l) if self.relisting.remove(&dir) => self.swap_listing(&dir, l),
            r => {
                self.relisting.remove(&dir);
                self.listings.insert(dir, r);
            }
        }
    }

    fn on_loaded(&mut self, ctx: &egui::Context, index: usize, thumb: bool, image: Result<ColorImage, String>) {
        let name = self.files().get(index).map(|f| f.path.clone()).unwrap_or_default();
        match &image {
            Err(e) => log!("{} {index} {name:?}: ERROR {e}", if thumb { "thumbnail" } else { "image" }),
            Ok(img) if !thumb => log!("image {index} {name:?}: {}x{}", img.size[0], img.size[1]),
            Ok(_) => {}
        }
        if thumb {
            self.generated.insert(index);
            // Generated in the background far from the screen: it stays on disk only.
            if self.thumb_keep.contains(&index) {
                let tex = image.map(|img| ctx.load_texture(name, img, TextureOptions::LINEAR));
                self.thumbs.insert(index, tex);
            }
        } else {
            self.pending.remove(&index);
            let tex = image.map(|img| ctx.load_texture(name, img, TextureOptions::LINEAR));
            self.full.insert(index, tex);
        }
    }

    fn on_meta(&mut self, index: usize, meta: Result<Option<meta::Meta>, String>) {
        if let Err(e) = &meta {
            log!("metadata {index}: ERROR {e}");
        }
        self.meta = Some((index, meta));
    }

    fn on_deleted(&mut self, path: String, how: source::Deletion, r: Result<(), source::Error>) {
        match (path, how, r) {
            (path, how, Ok(())) => {
                log!("deleted {path:?}");
                // An emptied trash going away comes as one more: never below zero.
                self.deleting = self.deleting.saturating_sub(1);
                self.remove_item(&path, how == source::Deletion::TrashFolder);
                self.dup_deleted(&path);
            }
            (path, _, Err((id, detail))) => {
                log!("delete {path:?}: ERROR {id} {detail}");
                // The loader stops at the first error: the others will not come.
                self.deleting = 0;
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
            }
        }
    }

    fn on_trash_items(&mut self, paths: Vec<String>) {
        log!("restore all: {} items", paths.len());
        self.restore_queue.extend(paths);
    }

    fn on_restored(&mut self, path: String, r: Result<String, source::Error>) {
        match (path, r) {
            (path, Ok(to)) => {
                log!("restored {path:?} -> {to:?}");
                self.restore_busy = false;
                self.listings.retain(|k, _| !(to.ends_with('/') && k.starts_with(&to)));
                self.remove_item(&path, false);
                self.relist(source::parent(&to));
            }
            (path, Err(("error-exists", _))) => {
                log!("restore {path:?}: name taken");
                self.restore_busy = false;
                self.restore_conflict = Some(path);
            }
            (path, Err((id, detail))) => {
                log!("restore {path:?}: ERROR {id} {detail}");
                self.restore_busy = false;
                self.restore_queue.clear();
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
            }
        }
    }

    fn on_renamed(&mut self, old: String, r: Result<String, source::Error>) {
        match (old, r) {
            (old, Ok(new)) => {
                log!("renamed {old:?} -> {new:?}");
                self.renamed(&old, &new);
            }
            (old, Err((id, detail))) => {
                log!("rename {old:?}: ERROR {id} {detail}");
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
            }
        }
    }

    fn on_paste_check(&mut self, r: Result<Vec<String>, source::Error>) {
        match r {
            Ok(conflicts) => {
                log!("paste: {} names taken", conflicts.len());
                if let Some(ask) = &mut self.paste_ask {
                    ask.conflicts = Some(conflicts);
                }
            }
            Err((id, detail)) => {
                log!("paste: ERROR {id} {detail}");
                self.paste_ask = None;
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
            }
        }
    }

    fn on_pasted(&mut self, ctx: &egui::Context, gone: Vec<String>, r: Result<(), source::Error>) {
        if self.quit_after_transfer {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let Some((p, _, base, dir)) = self.transfer.take() else { return };
        log!(
            "pasted: {} moved away, {r:?}; {} files, {} bytes, {:.1} s looking inside folders",
            gone.len(),
            p.files.load(Ordering::Relaxed),
            p.done.load(Ordering::Relaxed),
            p.looked.lock().unwrap()
        );
        if base == self.base {
            for path in &gone {
                self.remove_item(path, false);
            }
            // The open folder, if inside the destination, is read again instead of dropped: dropped,
            // nothing asked for it and its spinner never ended (Downloads during a paste into home).
            let open = self.dir.clone();
            self.listings.retain(|k, _| !(k.starts_with(&dir) && *k != dir && *k != open));
            if open != dir && open.starts_with(&dir) {
                self.relist(open);
            }
            self.relist(dir);
        }
        if let Err((id, detail)) = r
            && id != "error-cancelled"
        {
            self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
        }
    }

    fn on_mounted(&mut self, ctx: &egui::Context, r: Result<String, String>) {
        match r {
            Ok(path) => {
                log!("mounted at {path:?}");
                // Opened straight away: as a local folder, or reopening the disk if the source is remote.
                if self.base == "/" {
                    self.set_dir(format!("{}/", path.trim_matches('/')));
                } else {
                    self.open_source(ctx, &path);
                }
            }
            Err(e) => {
                log!("mount failed: {e}");
                self.error = Some(self.i18n.t_args("error-mount", &[("detail", &e)]));
            }
        }
    }

    fn on_open(&mut self, ctx: &egui::Context, address: String) {
        log!("handed over: {:?}", without_password(&address));
        if !address.is_empty() {
            self.open_source(ctx, &address);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn on_prepared(&mut self, path: String, turn: transform::Turn, r: Result<(Arc<Vec<u8>>, bool), source::Error>) {
        self.saving.remove(&path);
        match r {
            // Only for the image still open with the same turn: the confirmation is about it.
            Ok((data, lossy)) if self.viewed(&path) && turn == self.turn => {
                self.save_ready = Some((path, turn, data, lossy));
            }
            Ok(_) => {}
            Err(e) => self.save_failed(&path, e),
        }
    }

    /// Why an image could not be prepared for saving over itself: what would be lost, or the error.
    pub(crate) fn save_failed(&mut self, path: &str, e: source::Error) {
        match e {
            (loader::SKIPPED, name) => {
                // Say exactly what would be lost.
                let id = match encode::Format::of_file(&name) {
                    Some(encode::Format::Tiff) => "save-pages",
                    Some(encode::Format::Jpeg) => "save-extra",
                    Some(_) => "save-animated",
                    None => loader::SKIPPED,
                };
                self.notice = Some((self.i18n.t_args(id, &[("detail", &md(&name))]), self.viewed(path)));
            }
            (id, detail) => {
                log!("save: ERROR {id} {detail}");
                self.error = Some(self.i18n.t_args("save-failed", &[("detail", &self.i18n.t_args(id, &[("detail", &detail)]))]));
            }
        }
    }

    fn on_replaced(&mut self, ctx: &egui::Context, path: String, r: Result<(), source::Error>) {
        self.saving.remove(&path);
        match r {
            Ok(()) => {
                log!("saved {path:?}");
                if let Some(n) = &mut self.batch_saved {
                    *n += 1;
                }
                // Same name, maybe the same size: what is in memory is the old image.
                if let Some(i) = self.files().iter().position(|f| f.path == path) {
                    match self.fresh.as_ref().filter(|(p, _)| *p == path) {
                        Some((_, tex)) => self.full.insert(i, Ok(tex.clone())),
                        None => self.full.remove(&i),
                    };
                }
                if self.viewed(&path) {
                    self.turn = Default::default();
                }
                self.relist(source::parent(&path));
                self.redo_thumbs(ctx, vec![path]);
            }
            Err((loader::SKIPPED, name)) => self.skipped.push(name),
            Err((id, detail)) => {
                log!("save: ERROR {id} {detail}");
                self.error = Some(self.i18n.t_args("save-failed", &[("detail", &self.i18n.t_args(id, &[("detail", &detail)]))]));
            }
        }
        if self.saving.is_empty()
            && let Some(n) = self.batch_saved.take()
        {
            let mut text = self.i18n.t_args("batch-done", &[("n", &n.to_string())]);
            if !self.skipped.is_empty() {
                let names: String = std::mem::take(&mut self.skipped).iter().map(|n| format!("\n- {}", md(n))).collect();
                text = format!("{text}\n\n{}\n{names}", self.i18n.t("batch-skipped-names"));
            }
            self.notice = Some((text, false));
        }
    }

    fn on_forgotten(&mut self, paths: Vec<String>) {
        let paths: HashSet<String> = paths.into_iter().collect();
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        let gone: Vec<usize> = l.files.iter().enumerate().filter(|(_, f)| paths.contains(&f.path)).map(|(i, _)| i).collect();
        for i in gone {
            self.thumbs.remove(&i);
            self.generated.remove(&i);
        }
        self.thumb_queue_key = None;
    }

    fn on_update(&mut self, r: Result<Option<update::Ready>, String>) {
        log!("update: {r:?}");
        let note = match &r {
            Ok(Some(ready)) => self.i18n.t_args("update-found", &[("version", &ready.version)]),
            Ok(None) => self.i18n.t_args("update-latest", &[("version", version())]),
            Err(e) => self.i18n.t_args("update-check-failed", &[("detail", e)]),
        };
        if self.update_note.is_some() {
            self.update_note = Some(note);
        }
        if let Ok(Some(ready)) = r {
            self.update = Some(ready);
        }
    }

    fn on_devices(&mut self, ctx: &egui::Context, d: Vec<devices::Device>) {
        log!("devices: {:?}", d.iter().map(|d| &d.path).collect::<Vec<_>>());
        // Open phone unplugged: it disappears altogether, back to home as at startup
        // (otherwise it stayed in the tree with the folders read before).
        let serial = self.base.strip_prefix("mtp://").map(|s| s.trim_end_matches('/').to_string());
        let has = |devices: &[devices::Device], s: &str| devices.iter().any(|d| d.serial.as_deref() == Some(s));
        let unplugged = serial.is_some_and(|s| has(&self.devices, &s) && !has(&d, &s));
        // The same for a disk, a stick or a mounted phone with one of its folders open: otherwise the
        // grid stayed on the files read before.
        let gone = self.loader.is_some() && devices::gone_under(&self.devices, &d, &self.display(&self.dir));
        self.devices = d;
        if unplugged || gone {
            log!("device with the open folder unplugged");
            let home = std::env::home_dir().map_or(".".into(), |h| h.to_string_lossy().into_owned());
            self.open_source(ctx, &home);
        }
    }
}
