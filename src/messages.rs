//! What the loader and the other threads send back (`Msg`): one method per message type, called by
//! `App::receive`.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui::{self, ColorImage};

use crate::address::{AddressForm, without_password};
use crate::save_dialogs::{Notice, md};
use crate::actions::Act;
use crate::mipmap::{self, Picture};
use crate::{App, RETRY_SECS, devices, encode, loader, meta, source, transform, update, version};
use crate::loader::Msg;
use crate::source::Listing;
use crate::listing::Arrival;

/// A deletion done goes into `deleted`, out of the listings with those after it (audit 1.8);
/// any other message is handled after them.
fn held_back(deleted: &mut Vec<(String, source::Deletion)>, msg: Msg) -> Option<Msg> {
    match msg {
        Msg::Deleted(path, how, Ok(())) => {
            deleted.push((path, how));
            None
        }
        msg => Some(msg),
    }
}

impl App {
    pub(crate) fn receive(&mut self, ctx: &egui::Context) {
        // The deletions done in a row, taken out of their listings together (audit 1.8), before
        // any other message, which comes after them as before.
        let mut deleted = Vec::new();
        while let Ok(msg) = self.rx.try_recv() {
            let Some(msg) = self.of_this_source(msg).and_then(|msg| held_back(&mut deleted, msg)) else { continue };
            self.on_deleted_ok(std::mem::take(&mut deleted));
            match msg {
                Msg::Listed(base, dir, r) => self.on_listed(ctx, &base, dir, r),
                Msg::At(..) => {}
                Msg::Loaded { generation, index, thumb, image, halves } if generation == self.generation => self.on_loaded(ctx, index, thumb, image, halves),
                Msg::Meta { generation, index, meta } if generation == self.generation => self.on_meta(index, meta),
                Msg::Info { generation, file, info } if generation == self.generation => {
                    if let Some(info) = info {
                        self.thumb_infos.insert(crate::thumb_details::version(&file), info);
                    }
                }
                Msg::Mesh { generation, index, mesh } if generation == self.generation => {
                    if let Some(f) = self.files().get(index) {
                        self.model.meshes.insert(f.path.clone(), mesh);
                    }
                }
                Msg::Drawing { generation, index, drawing } if generation == self.generation => {
                    // The first of two drawings compared shows the comparison (`cad_compare_view`).
                    if let Some(f) = self.files().get(index).filter(|f| !self.comparing(&f.path)) {
                        self.model.cad.drawings.insert(f.path.clone(), drawing);
                    }
                }
                Msg::Loaded { .. } | Msg::Meta { .. } | Msg::Info { .. } | Msg::Mesh { .. } | Msg::Drawing { .. } => {} // folder has changed in the meantime
                // Counted from the first event, not the last: a copy into a subfolder sends one for
                // every file, and the folder was read again only when the copy ended.
                Msg::Changed if self.changed_at.is_none() => self.changed_at = Some(ctx.input(|i| i.time) + 0.3),
                Msg::Changed => {}
                Msg::Deleted(path, how, r) => self.on_deleted(path, how, r),
                Msg::Unseen(p, r) => self.on_unseen(&p, r),
                Msg::TrashItems(paths) => self.on_trash_items(paths),
                Msg::Restored(path, r) => self.on_restored(path, r),
                Msg::Renamed(old, r) => self.on_renamed(old, r),
                Msg::MadeFolder(path, r) => self.on_made_folder(path, r),
                Msg::PasteCheck(check, r) => self.on_paste_check(check, r),
                Msg::Pasted(gone, r) => self.on_pasted(ctx, gone, r),
                Msg::Mounted(r) => self.on_mounted(ctx, r),
                Msg::Open(address) => self.on_open(ctx, address),
                Msg::Home(base, r) => self.on_home(base, r),
                // Shown in a window only when asked from the Help menu (the window is open).
                // To the page that asked for it, the editor or Save as… (#200).
                Msg::Original(asked, r) => self.on_original(ctx, asked, r),
                Msg::EditCopy(r) => self.on_edit_copy(ctx, r),
                Msg::EditPicked(r) => self.on_edit_picked(ctx, r),
                Msg::ClipImage(r) => self.on_clip_image(ctx, r),
                Msg::Animated(path) => {
                    self.animated.insert(path);
                }
                Msg::Frames(path, seq) => {
                    self.frames.read.insert(path, seq);
                }
                Msg::Motion(path, video) => {
                    self.motion.read.insert(path, video);
                }
                Msg::Pano(path, Some(pano)) => {
                    log!("panorama {path:?}: {pano:?}");
                    self.pano.read.insert(path, pano);
                }
                Msg::Pano(path, None) => {
                    self.pano.read.remove(&path);
                }
                Msg::EditPrepared(path, r) => self.on_edit_prepared(path, r),
                Msg::EditWritten(path, mark, over) => self.on_edit_written(path, mark, over),
                Msg::Encoded(generation, r) => self.convert_encoded(ctx, generation, r),
                Msg::Saved(asked, r) => self.convert_saved(&asked, r),
                Msg::Exported(r) => self.on_exported(ctx, r),
                Msg::Compared(r) => self.on_compared(r),
                Msg::Prepared(path, turn, r) => self.on_prepared(path, turn, r),
                Msg::Replaced(path, r) => self.on_replaced(ctx, path, r),
                Msg::NotReplaced(kept) => self.on_not_replaced(ctx, kept),
                Msg::KeptSaved(r) => self.on_kept_saved(r),
                Msg::Forgotten(paths) => self.on_forgotten(paths),
                Msg::Purged => {
                    log!("thumbnail cache emptied");
                    self.refresh();
                }
                Msg::Update(r) => self.on_update(r),
                Msg::Devices(d) => self.on_devices(ctx, d),
            }
        }
        self.on_deleted_ok(deleted);
    }

    /// `msg`, out of its `Msg::At`; `None` when it is about paths of a source left meanwhile,
    /// where they are other items (audit 27/11 F.9).
    fn of_this_source(&self, msg: Msg) -> Option<Msg> {
        match msg {
            Msg::At(base, _) if base != self.base => {
                log!("dropped, of a source left: {}", without_password(&base));
                None
            }
            Msg::At(_, m) => Some(*m),
            m => Some(m),
        }
    }

    /// What an error of the listing of `dir` asks for: a confirmation, a password, another
    /// folder. `true`: the listing is dropped (another folder opens instead).
    fn listing_error(&mut self, dir: &str, r: &mut Result<Listing, source::Error>) -> bool {
        // Said in the window that asks about it: not in the bar too, nor after Cancel (Piero, 20/11).
        if let Err((id, detail)) = r
            && dir == self.dir
            && crate::error_bar::said_in_window(id, self.host_question.is_some() || crate::ftp::unknown().is_some() || crate::sftp::unknown().is_some(), self.open_address.is_some())
        {
            self.lasting_closed = Some(self.i18n.error(id, detail));
        }
        match r {
            Err(("error-device-kde" | "error-device-kde-back", _)) => self.kiod_ask = true,
            // The question about the first server refused; asked, it stays until answered.
            Err((id @ ("error-host-unknown" | "error-cert-unknown"), _)) => {
                if self.host_question.is_none() {
                    self.host_question = crate::host_question::Question::of(id);
                }
                self.host_ask = self.host_question.is_some();
            }
            // A server that refused the login: File > Open location asks for the password.
            Err((id @ ("error-login" | "error-login-ssh"), detail)) if dir == self.dir && self.open_address.is_none() => {
                self.open_address = AddressForm::for_password(&format!("{}{}", self.base, self.dir));
                if let Some(f) = &mut self.open_address {
                    f.refused = Some(self.i18n.error(id, detail));
                    f.enter_saves = crate::servers::find(&format!("{}{}", self.base, self.dir)).is_some();
                }
            }
            // A server's folder that is gone (moved, deleted, reopened at startup): the saved
            // server's folder instead, or the server's starting folder.
            Err((crate::source::NOT_FOUND, _)) if dir == self.dir && !dir.is_empty() && self.base.contains("://") => {
                let saved = crate::servers::find(&format!("{}{}", self.base, self.dir)).map(|s| s.folder.trim().trim_matches('/').to_string());
                let back = saved.filter(|f| !f.is_empty()).map(|f| format!("{f}/")).filter(|f| *f != dir);
                log!("folder {dir:?} gone: {back:?}");
                match back {
                    Some(f) => self.set_dir(f),
                    None => {
                        self.set_dir(String::new());
                        if let Some(loader) = &self.loader {
                            loader.home();
                        }
                    }
                }
                return true;
            }
            // A saved key that does not open: the server manager on that server, to fix it, with
            // the message inside it, over the fields of that server. Not in the bar too, as a
            // login refused (Piero, 21/11, the rule of 20/11): closed there while it lasts.
            Err((id, detail)) if *id == "error-key-failed" && dir == self.dir && self.server_manager.is_none() => {
                let address = format!("{}{}", self.base, self.dir);
                let mut f = AddressForm::manager();
                f.choose(f.servers.iter().position(|s| crate::servers::find(&address).is_some_and(|found| found == *s)));
                *id = "error-key-failed-open";
                f.key_problem = f.chosen.map(|i| (i, self.i18n.error(id, detail)));
                self.server_manager = Some(f);
                self.lasting_closed = Some(self.i18n.error(id, detail));
            }
            _ => {}
        }
        false
    }

    fn on_listed(&mut self, ctx: &egui::Context, base: &str, dir: String, r: Result<Listing, source::Error>) {
        // From the source left: the folder of the same path ("" of two servers) of the one open
        // would take it (audit 1.2). Before anything else, as its reading being over.
        if base != self.base {
            log!("listing {dir:?} of {:?}, a source left: dropped", without_password(base));
            return;
        }
        let base = without_password(base);
        match &r {
            Ok(l) => log!("listing {dir:?} of {base:?}: {} folders, {} images", l.dirs.len(), l.files.len()),
            Err(e) => log!("listing {dir:?} of {base:?}: ERROR {e:?}"),
        }
        self.listing_pending.remove(&dir);
        // The mark of this reading goes before another starts, which keeps its own: it went
        // after, and the next listing came in without the files followed (audit 1.3).
        self.relisting.remove(&dir);
        if self.relist_again.remove(&dir) {
            self.relist(dir.clone());
        }
        let mut r = r;
        if let Ok(l) = &mut r {
            l.files.retain(|f| self.settings.show.allows(&f.path));
            l.sort_files(self.settings.sort_by, self.settings.sort_desc);
            #[cfg(feature = "shots")]
            if self.base.starts_with("mtp://") && !dir.is_empty() {
                crate::shots::phone_folders(&mut l.dirs);
            }
        }
        if self.listing_error(&dir, &mut r) {
            return;
        }
        if self.use_mount(ctx, &dir, &r) {
            return;
        }
        if r.is_ok() && crate::mtp::taken_from_gvfs() {
            // Something to know once, not a state: it goes after a while (Piero, 21/11/2026).
            self.status_note = Some(self.i18n.t("status-taken-from-gvfs"));
            self.note_until = Some(ctx.input(|i| i.time) + crate::watch::NOTE_SECS);
        }
        if dir == self.dir {
            let no_storage = match &r {
                Err((id, _)) => matches!(*id, "error-no-storage" | "error-no-device" | "error-not-ready"),
                Ok(l) => l.dirs.is_empty() && l.files.is_empty() && self.phone_root(&dir).is_some(),
            };
            self.retry_at = no_storage.then(|| ctx.input(|i| i.time) + RETRY_SECS);
        }
        let r = r.map_err(|(id, detail)| self.i18n.error(id, &detail));
        let select = if self.editor.is_some() { None } else { self.select_after.as_deref() };
        match r {
            Ok(l) => match crate::listing::arrival(self.listings.get(&dir), &l, select, self.fresh.is_some()) {
                Arrival::Swap => self.swap_listing(&dir, l),
                Arrival::Keep => log!("listing {dir:?} as before"),
                Arrival::Insert => {
                    self.listings.insert(dir.clone(), Ok(l));
                }
            },
            r => {
                self.listings.insert(dir.clone(), r);
            }
        }
        self.let_go_listings(ctx);
        // After the files followed: the viewer and the selection already on their file by then.
        if dir == self.dir {
            self.start_found();
        }
    }

    /// The file asked to be shown at the start (`start`: an image opened, or the one in the viewer
    /// at F5) in the listing of the open folder: the viewer opens on it, and it is selected.
    fn start_found(&mut self) {
        let Some(start) = self.start.take() else { return };
        let found = self.listings.get(&self.dir).and_then(|r| r.as_ref().ok()).and_then(|l| l.files.iter().position(|f| f.path == start).map(|i| (l.dirs.len(), i)));
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
            self.thumb_infos.clear();
            self.info_asked.clear();
            self.thumb_queue_key = None;
        }
        self.viewing = found.map(|(_, i)| i);
        if let Some((ndirs, i)) = found {
            self.selected = ndirs + i;
            self.cursor = true;
            self.scroll_to_selected = true;
        }
    }

    fn on_loaded(&mut self, ctx: &egui::Context, index: usize, thumb: bool, image: Result<ColorImage, String>, halves: Vec<ColorImage>) {
        let image = image.map_err(|e| kept_error(&self.i18n, e, thumb));
        let name = self.files().get(index).map(|f| f.path.clone()).unwrap_or_default();
        match &image {
            Err(e) => log!("{} {index} {name:?}: ERROR {e}", if thumb { "thumbnail" } else { "image" }),
            Ok(img) if !thumb => log!("image {index} {name:?}: {}x{}", img.size[0], img.size[1]),
            Ok(_) => {}
        }
        if thumb {
            self.generated.insert(index);
            // Its details are saved with it: asked again if they were not there before.
            if let Some(v) = self.files().get(index).map(crate::thumb_details::version)
                && !self.thumb_infos.contains_key(&v)
            {
                self.info_asked.remove(&v);
            }
            // Generated in the background far from the screen: it stays on disk only.
            if self.thumb_keep.contains(&index) {
                // Its halves here: small, and drawn down to 64 points (`mipmap`).
                let tex = image.map(|img| Picture::now(ctx, &name, img, mipmap::SMALLEST_THUMB));
                self.thumbs.insert(index, tex);
            }
        } else {
            self.pending.remove(&index);
            match image.as_ref().ok().and_then(crate::decode::own_size) {
                Some(size) => self.own_sizes.insert(name.clone(), size),
                None => self.own_sizes.remove(&name),
            };
            let tex = image.map(|img| Picture::new(ctx, &name, img, halves));
            self.full.insert(index, tex);
        }
    }

    pub(crate) fn on_meta(&mut self, index: usize, meta: Result<Option<meta::Meta>, String>) {
        if let Err(e) = &meta {
            log!("metadata {index}: ERROR {e}");
        }
        // The answers do not come in the order asked (one from the cache overtakes a slow one):
        // that of a file selected before would stay in the panel under the file selected now,
        // which is not asked again. It is in the cache for when its file is selected.
        if self.meta_asked != Some((self.generation, index)) {
            return;
        }
        // A video or an audio file without the engine: why, in words, not libmpv's error.
        let played = self.files().get(index).is_some_and(|f| crate::decode::is_played(&f.path));
        let meta = match meta {
            Err(_) if played && !crate::mpv::available() => Err(self.engine_note()),
            m => m,
        };
        self.meta = Some((index, meta));
    }

    /// Items deleted, out of their listings together (audit 1.8: the listing was copied once for
    /// each).
    fn on_deleted_ok(&mut self, done: Vec<(String, source::Deletion)>) {
        if done.is_empty() {
            return;
        }
        // An emptied trash going away comes as one more: never below zero.
        self.deleting = self.deleting.saturating_sub(done.len());
        let items: Vec<(String, bool)> = done.iter().map(|(path, how)| (path.clone(), *how == source::Deletion::TrashFolder)).collect();
        self.remove_items(&items);
        for (path, _) in done {
            log!("deleted {path:?}");
            self.dup_deleted(&path);
            // An emptied trash: a listing of its folder already on its way would show it again.
            // An Android trashed file gone: its folder may have no trash left.
            if source::is_trash(&path) || source::android_trashed(source::name(&path)).is_some() {
                self.relist(source::parent(&path));
            }
        }
    }

    fn on_deleted(&mut self, path: String, how: source::Deletion, r: Result<(), source::Error>) {
        match (path, how, r) {
            (path, how, Ok(())) => self.on_deleted_ok(vec![(path, how)]),
            (path, _, Err((id, detail))) => {
                log!("delete {path:?}: ERROR {id} {detail}");
                // The loader stops at the first error: the others will not come.
                self.deleting = 0;
                // Gone already (deleted by another program, and the watcher did not tell, #207):
                // said with its name, and its folder read again, which takes it from the grid.
                if crate::fault::gone(&detail) {
                    // Not marked any more: Delete again would ask for it first, and stop there.
                    self.marked.remove(&path);
                    self.error = Some(self.i18n.fault(crate::fault::Kind::NotFound, Some(source::shown(&path))));
                    self.relist(source::listed_in(&path));
                    return;
                }
                self.error = Some(self.i18n.error(id, &detail));
            }
        }
    }

    fn on_trash_items(&mut self, paths: Vec<String>) {
        // A RAW photo's sidecars go back with it (`sidecar_follow::restore`).
        let paths = crate::sidecar_follow::without_sidecars(paths);
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
                self.error = Some(self.i18n.error(id, &detail));
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
                self.error = Some(self.i18n.error(id, &detail));
            }
        }
    }

    fn on_paste_check(&mut self, check: u64, r: Result<Vec<String>, source::Error>) {
        // A check of a paste no longer open, or asked before the one open: not about this one.
        if self.paste_ask.is_none() || !crate::file_dialogs::paste_check_wanted(check) {
            log!("paste: the answer of an older check dropped");
            return;
        }
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
                self.error = Some(self.i18n.error(id, &detail));
            }
        }
    }

    fn on_pasted(&mut self, ctx: &egui::Context, gone: Vec<String>, r: Result<(), source::Error>) {
        if self.quit_after_transfer {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let Some((p, cut, base, dir)) = self.transfer.take() else { return };
        log!(
            "pasted: {} moved away, {r:?}; {} files, {} bytes, {:.1} s looking inside folders, {} links to folders left out",
            gone.len(),
            p.files.load(Ordering::Relaxed),
            p.done.load(Ordering::Relaxed),
            p.looked.lock().unwrap(),
            p.links_skipped.lock().unwrap().len()
        );
        let to = crate::navigation::shown(&base, &dir);
        if base == self.base {
            self.remove_items(&gone.iter().map(|path| (path.clone(), false)).collect::<Vec<_>>());
            // The open folder, if inside the destination, is read again instead of dropped: dropped,
            // nothing asked for it and its spinner never ended (Downloads during a paste into home).
            let open = self.dir.clone();
            self.listings.retain(|k, _| !(k.starts_with(&dir) && *k != dir && *k != open));
            // A folder moved with links left out stays, emptied around them (#183), and so does one
            // that got something new while it moved (#194), or that holds a file not read (L17 of
            // 2026-11-21): the folders that hold them are read again, not shown as cached.
            let failed = p.failed.lock().unwrap().iter().map(|(f, _)| f.clone()).collect::<Vec<_>>();
            let links = if cut { [p.links_skipped.lock().unwrap().clone(), p.left.lock().unwrap().clone(), failed].concat() } else { Vec::new() };
            self.listings.retain(|k, _| *k == open || !links.iter().any(|l| l.starts_with(k.as_str())));
            // (The destination, and a folder inside it, are read again just below.)
            if !open.starts_with(&dir) && links.iter().any(|l| l.starts_with(open.as_str())) {
                self.relist(open.clone());
            }
            if open != dir && open.starts_with(&dir) {
                self.relist(open);
            }
            // What the paste made in the folder open is selected, the last one (#323).
            if dir == self.dir
                && let Some(last) = p.made.lock().unwrap().last()
            {
                self.select_after = Some(last.clone());
            }
            self.relist(dir);
        }
        // The copy box is gone (`transfer` taken): what is left to say, in a message.
        self.tell_transfer(&p, cut, &to, &r);
        if let Err((id, detail)) = r
            && id != "error-cancelled"
        {
            self.error = Some(self.i18n.error(id, &detail));
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

    /// A server opened without a folder: its starting folder, if the user is still at the top.
    fn on_home(&mut self, base: String, r: Result<String, source::Error>) {
        log!("home of {base:?}: {r:?}");
        if let Ok(dir) = r
            && base == self.base
            && self.dir.is_empty()
        {
            self.set_dir(dir);
        }
    }

    fn on_open(&mut self, ctx: &egui::Context, address: String) {
        log!("handed over: {:?}", without_password(&address));
        if let Some(address) = crate::navigation::hold(self.holds_outside(), &mut self.held_open, address)
            && !address.is_empty()
        {
            self.open_outside(ctx, &address);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn on_prepared(&mut self, path: String, turn: transform::Turn, r: Result<(Arc<Vec<u8>>, bool), source::Error>) {
        self.saving.remove(&path);
        self.preparing = None;
        match r {
            // Only for the image still open with the same turn: the confirmation is about it.
            Ok((data, lossy)) if self.viewed(&path) && turn == self.turn => {
                self.save_ready = Some((path, turn, data, lossy));
            }
            Ok(_) => {}
            // Too large for the memory once decoded: asked (`view_memory::save_big_window`).
            Err((crate::replace::TOO_LARGE, need)) if self.viewed(&path) && turn == self.turn => self.save_big = need.parse().ok().map(|need| (path, turn, need)),
            Err((crate::replace::TOO_LARGE, _)) => self.after_save = None,
            Err(e) => {
                let save_as = self.viewed(&path).then_some(Act::Convert(None));
                self.save_failed(e, save_as);
            }
        }
    }

    /// Why an image could not be prepared for saving over itself: what would be lost, or the
    /// error. `save_as`: what the message's Save as… does, if it can be offered.
    pub(crate) fn save_failed(&mut self, e: source::Error, save_as: Option<Act>) {
        // Save chosen on the way out of the image and not possible: the image stays.
        self.after_save = None;
        match e {
            (loader::SKIPPED, name) => {
                // Say exactly what would be lost.
                let id = match encode::Format::of_file(&name) {
                    Some(encode::Format::Tiff) => "save-pages",
                    Some(encode::Format::Jpeg) => "save-extra",
                    Some(_) => "save-animated",
                    None => loader::SKIPPED,
                };
                let text = self.i18n.t_args(id, &[("detail", &md(&name))]);
                self.tell_notice(Notice { save_as, ..Notice::new(self.i18n.t("save-not-possible"), text) });
            }
            (id, detail) => {
                log!("save: ERROR {id} {detail}");
                self.error = Some(self.i18n.save_failed(&(id, detail)));
            }
        }
    }

    pub(crate) fn on_replaced(&mut self, ctx: &egui::Context, path: String, r: Result<(), source::Error>) {
        // The editor's file on its place, which the grid no longer shows: a file of the same
        // path here is another one.
        let elsewhere = self.editor.as_ref().is_some_and(|w| w.saves_elsewhere(&self.base, &path));
        self.saving.remove(&path);
        self.edit_saved(&path, r.is_ok());
        match r {
            Ok(()) if elsewhere => log!("saved {path:?}, in the editor's place"),
            Ok(()) => {
                log!("saved {path:?}");
                if let Some((_, n, videos)) = &mut self.batch_saved {
                    *n += 1;
                    *videos += crate::decode::is_video(&path) as usize;
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
                    // A video saved turned: opened again, with its new rotation (#125).
                    if crate::decode::is_video(&path) {
                        // Where it was, kept for it to open there again (#211).
                        self.keep_place(&path);
                        self.video = None;
                    }
                }
                self.relist(source::parent(&path));
                self.redo_thumbs(ctx, vec![path]);
            }
            Err((loader::SKIPPED, name)) => self.skipped.push(name),
            Err((crate::replace::TOO_LARGE, _)) => self.skipped_large.push(source::name(&path).to_string()),
            Err((id, detail)) => {
                log!("save: ERROR {id} {detail}");
                // With the image kept, its own window says what happened.
                if !self.kept.iter().any(|k| k.path == path) {
                    self.error = Some(self.i18n.save_failed(&(id, detail)));
                }
            }
        }
        if self.saving.is_empty()
            && let Some((op, n, videos)) = self.batch_saved.take()
            // Nothing saved and every failure in its own window (`kept.rs`): "saved: 0" said nothing more.
            && !(n == 0 && self.skipped.is_empty() && self.skipped_large.is_empty() && !self.kept.is_empty())
        {
            // What was done, in the title and in the text ("Images saved: 2." alone
            // did not say what had happened).
            let what = crate::save_dialogs::saved_what(n, videos);
            let args = [("n", n.to_string()), ("op", crate::save_dialogs::op_id(op).to_string()), ("what", what.to_string())];
            let args = args.each_ref().map(|(k, v)| (*k, v.as_str()));
            let title = self.i18n.t_args("batch-done-title", &args);
            let text = self.i18n.t_args("batch-done", &args);
            let (skipped, large) = (std::mem::take(&mut self.skipped), std::mem::take(&mut self.skipped_large));
            self.tell_notice(crate::notice_list::turned(&self.i18n, title, &text, &skipped, &large));
        }
    }

    pub(crate) fn on_forgotten(&mut self, paths: Vec<String>) {
        let paths: HashSet<String> = paths.into_iter().collect();
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        let gone: Vec<usize> = l.files.iter().enumerate().filter(|(_, f)| paths.contains(&f.path)).map(|(i, _)| i).collect();
        for i in gone {
            self.thumbs.remove(&i);
            self.generated.remove(&i);
        }
        self.thumb_queue_key = None;
    }

    /// The answer of the check before deleting folders, if it is still the one waited for.
    fn on_unseen(&mut self, p: &Arc<std::sync::atomic::AtomicBool>, r: Result<Option<(String, String)>, source::Error>) {
        log!("unseen: {r:?}");
        if let Some(crate::file_dialogs::DeleteCheck::Checking(now)) = &self.delete_check
            && Arc::ptr_eq(now, p)
        {
            self.delete_check = Some(crate::file_dialogs::DeleteCheck::Done(r));
        }
    }

    fn on_update(&mut self, r: Result<Option<update::Ready>, String>) {
        log!("update: {r:?}");
        let note = match &r {
            Ok(Some(ready)) => {
                let found = if ready.in_store() { "update-found-store" } else { "update-found" };
                self.i18n.t_args(found, &[("version", &ready.version)])
            }
            Ok(None) => self.i18n.t_args("update-latest", &[("version", version())]),
            Err(e) => self.i18n.t_args("update-check-failed", &[("detail", &update::error_detail(&self.i18n, e))]),
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
        let name = devices::unplugged_name(&self.devices, &d, &self.base, &self.display(&self.dir));
        // The phone of a paste gone: its window, if modal, no longer holds the app (#354).
        if let Some((p, _, base, _)) = &self.transfer
            && base.strip_prefix("mtp://").map(|s| s.trim_end_matches('/')).is_some_and(|s| has(&self.devices, s) && !has(&d, s))
        {
            p.by_phone.unplugged();
        }
        // The last folder of a disk unplugged goes with it: the panel no longer shows it under a
        // disk plugged at the same place (another stick of the same label, audit 27/11 F.12).
        self.last_dirs.retain(|k, _| !k.starts_with('/') || k == "/" || d.iter().any(|e| e.path == *k));
        // A phone unplugged: its session is broken; whoever still holds it (the clip of a copy,
        // the viewer) keeps it, the next opening gets a new one (audit 27/11 T.3, #40).
        for s in self.devices.iter().filter_map(|e| e.serial.as_deref()).filter(|s| !has(&d, s)) {
            log!("phone {s}: unplugged, its session forgotten");
            crate::mtp::Phone::forget(s);
        }
        self.devices = d;
        if unplugged || gone {
            log!("device with the open folder unplugged");
            // Gone with the device: its turns not saved cannot be saved any more.
            self.turn = Default::default();
            let home = std::env::home_dir().map_or(".".into(), |h| h.to_string_lossy().into_owned());
            self.open_source(ctx, &home);
            // Said in the status bar for a while, as a file gone from a phone (opening the home
            // folder clears the bar first).
            if let Some(name) = name {
                self.status_note = Some(self.i18n.t_args("device-unplugged", &[("name", &name)]));
                self.note_until = Some(ctx.input(|i| i.time) + crate::watch::NOTE_SECS);
            }
        }
    }
}

/// The error of a picture or a thumbnail as the program keeps it: `loader::with_id`'s
/// `id\u{1}detail` translated here, where the language is; a thumbnail's error that the grid
/// draws as a sign (`grid::too_large_sign`: a model beyond its cap) kept as it is, for the grid
/// to know it.
fn kept_error(i18n: &crate::i18n::I18n, e: String, thumb: bool) -> String {
    if thumb && crate::grid::too_large_sign(&e).is_some() {
        return e;
    }
    match e.split_once('\u{1}') {
        Some((id, detail)) => i18n.error(id, detail),
        None => e,
    }
}

#[cfg(test)]
mod tests {
    use crate::audit_family_1::{Harness, folder, own_folder};
    use crate::loader::Msg;

    /// #323: when a paste ends in the folder open, what it made there is selected, the last one
    /// when there are several (Piero, 24/11, W2: the new "Sunset (2).jpg" had to be looked for).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_paste_selects_what_it_made() {
        if own_folder("messages::tests::a_paste_selects_what_it_made") {
            return;
        }
        let mut h = Harness::new(&folder("paste-select", &["a.jpg", "b.jpg", "c.jpg"]));
        h.app.selected = 0;
        let made = |name: &str| format!("{}{name}", h.app.dir);
        let p = std::sync::Arc::new(crate::transfer::Progress::default());
        p.made.lock().unwrap().extend([made("c.jpg"), made("b.jpg")]);
        h.app.transfer = Some((p, false, h.app.base.clone(), h.app.dir.clone()));
        h.frame(Vec::new());
        h.app.tx.send(Msg::Pasted(Vec::new(), Ok(()))).unwrap();
        let b = h.app.files().iter().position(|f| f.path.ends_with("/b.jpg")).expect("b.jpg listed");
        h.until(|a| a.selected == b, "the last item made selected");
    }

    /// D1: a model beyond its cap shows "Too large for a thumbnail" in the grid (Piero, 23/11:
    /// it showed the warning, the error was translated before the grid could know it); the
    /// picture of the viewer still says it in words.
    #[test]
    fn thumbnail_beyond_the_cap_keeps_its_sign() {
        let i18n = crate::i18n::I18n::new("it");
        let e = crate::model_limit::OBJ.error();
        assert_eq!(crate::grid::too_large_sign(&super::kept_error(&i18n, e.clone(), true)), Some(crate::icons::Icon::Model));
        assert!(super::kept_error(&i18n, e, false).starts_with("Il modello ha più di 10 milioni di triangoli."));
    }

    /// A saved key that does not open: the server manager opens on that server with the message
    /// inside it, and the bar at the bottom is silent while that error lasts, as for a login
    /// refused (Piero, 21/11, the rule of 20/11).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn key_failed_said_in_its_window_only() {
        if own_folder("messages::tests::key_failed_said_in_its_window_only") {
            return;
        }
        let dir = folder("key-failed", &["a.jpg"]);
        let mut h = Harness::new(&dir);
        let (ctx, base, here) = (h.ctx.clone(), h.app.base.clone(), h.app.dir.clone());
        h.app.on_listed(&ctx, &base, here, Err(("error-key-failed", "nas.local".into())));
        h.frame(Vec::new());
        assert!(h.app.server_manager.is_some(), "the server manager opened");
        let error = h.app.folder_error();
        assert!(error.as_deref().is_some_and(|e| e.contains("nas.local")), "the folder's error: {error:?}");
        assert_eq!(h.app.lasting_closed, error, "the bar is silent on it");
    }

    /// Audit 1.2: a listing that comes late from the source left (a server's root "", a folder
    /// with the same path) does not take the place of the folder of the source open, nor does
    /// it end the reading of one of its folders.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn listing_of_the_source_left_dropped() {
        if own_folder("messages::tests::listing_of_the_source_left_dropped") {
            return;
        }
        let dir = folder("source-left", &["a.png"]);
        let mut h = Harness::new(&dir);
        let here = h.app.dir.clone();
        let fake = |p: &str| crate::source::Listing { files: vec![crate::source::File { path: p.into(), size: 1, modified: String::new() }], ..Default::default() };
        h.app.listing_pending.insert("other/".into());
        let old = "sftp://old.example/".to_string();
        h.app.tx.send(Msg::Listed(old.clone(), here.clone(), Ok(fake(&format!("{here}gone.png"))))).unwrap();
        h.app.tx.send(Msg::Listed(old, "other/".into(), Ok(fake("other/b.png")))).unwrap();
        h.frame(Vec::new());
        let names: Vec<_> = h.app.files().iter().map(|f| f.path.clone()).collect();
        assert_eq!(names, [format!("{here}a.png")], "the old source's listing took the place of the open folder's");
        assert!(h.app.listing_pending.contains("other/") && !h.app.listings.contains_key("other/"), "the old source's listing ended a reading of the new one");
    }

    /// Audit 2.6, scenario 1 (#282): the error of a listing that comes late from the source left,
    /// for a folder with the same path as the one open (the root "" of a phone and of a disk),
    /// does not cover the good listing of the source open.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn error_of_the_source_left_dropped() {
        if own_folder("messages::tests::error_of_the_source_left_dropped") {
            return;
        }
        let dir = folder("source-left-error", &["a.png"]);
        let mut h = Harness::new(&dir);
        let here = h.app.dir.clone();
        h.app.tx.send(Msg::Listed("mtp://OLDPHONE/".into(), here.clone(), Err(("error-device-busy", "OLDPHONE".into())))).unwrap();
        h.frame(Vec::new());
        let names: Vec<_> = h.app.files().iter().map(|f| f.path.clone()).collect();
        assert_eq!(names, [format!("{here}a.png")], "the old source's error took the place of the open folder's listing");
        assert_eq!(h.app.folder_error(), None);
    }

    /// #207 (choice D7): a folder deleted by another program, then sent to the trash from the
    /// grid on Windows: the message says that it is not there any more, and the folder is read
    /// again, so the folder goes from the grid. The error of the `trash` crate is made up here:
    /// on Linux the trash says it otherwise.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn trashed_folder_already_gone() {
        if own_folder("messages::tests::trashed_folder_already_gone") {
            return;
        }
        let dir = folder("trash-gone", &["a.png"]);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let mut h = Harness::new(&dir);
        h.until(|app| app.listing_dirs() == 1 && !app.listing_pending.contains(&app.dir), "the subfolder listed");
        let sub = format!("{}sub/", h.app.dir);
        // Without the watcher, nor a change it told about before: on Linux it would read the
        // folder again 0.3 s after the removal. Only the message is left to ask for it.
        (h.app.watcher, h.app.changed_at) = (None, None);
        std::fs::remove_dir(dir.join("sub")).unwrap();
        let raw = r#"Error during a `trash` operation: Os { code: -2147024894, description: "windows error: Impossibile trovare il file specificato. (0x80070002)" }"#;
        h.app.marked.insert(sub.clone());
        h.app.tx.send(Msg::Deleted(sub.clone(), crate::source::Deletion::SystemTrash, Err(("error-detail", raw.into())))).unwrap();
        h.frame(Vec::new());
        assert_eq!(h.app.error, Some(h.app.i18n.fault(crate::fault::Kind::NotFound, Some("sub"))));
        assert!(!h.app.marked.contains(&sub), "still marked: Delete would ask for it again");
        // Read again by the message itself, in its own frame: the watcher of Linux waits 0.3 s
        // after its event, and Windows' did not tell (#207). Being read, or read already: on a
        // busy computer the loader answers while `receive` is still taking the messages, and
        // the folder is the new one when the frame ends. Asking for the reading still on its
        // way failed one run in ten under load.
        let gone = |app: &crate::App| app.listing_dirs() == 0 && app.files().len() == 1;
        assert!(h.app.listing_pending.contains(&h.app.dir) || gone(&h.app), "the folder not read again");
        h.until(gone, "the folder gone from the grid");
    }

    /// Audit 27/11 F.9 (#282): the end of a deletion asked on a server, come once another source
    /// is open, took a file of the same path out of the grid of the new one (and so a renaming,
    /// a restoring, a new folder). Here the server holds its answer until the computer's folder,
    /// with the same path as the server's, is open.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_end_of_a_deletion_on_the_source_left_dropped() {
        use crate::audit_family_1::fake_ftp;
        if own_folder("messages::tests::the_end_of_a_deletion_on_the_source_left_dropped") {
            return;
        }
        let local = folder("deleted-on-the-server", &["a.png"]);
        let png = std::fs::read(local.join("a.png")).unwrap();
        let abs = local.display().to_string();
        let mut tree = fake_ftp::Tree::default();
        tree.folder(&abs, &[("a.png", &png)]);
        let (target, tree) = fake_ftp::start(tree);
        let mut h = Harness::open(&format!("ftp://tester:x@127.0.0.1:{}{abs}/", target.port));
        assert!(h.app.base.starts_with("ftp://"), "on the server: {}", h.app.base);
        let a = h.app.files()[0].path.clone();
        let held = tree.lock().unwrap();
        h.app.loader.as_ref().unwrap().delete(vec![a.clone()], crate::source::Deletion::Permanent);
        h.app.deleting = 1;
        h.app.tx.send(Msg::Open(format!("{abs}/"))).unwrap();
        h.until(|app| app.base == "/" && !app.files().is_empty(), "the computer's folder listed");
        drop(held);
        let start = std::time::Instant::now();
        while !tree.lock().unwrap().log.iter().any(|l| l.starts_with("DELE")) {
            assert!(start.elapsed().as_secs() < 10, "the server never asked to delete: {:?}", tree.lock().unwrap().log);
            h.frame(Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for _ in 0..20 {
            h.frame(Vec::new());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(local.join("a.png").is_file(), "the computer's a.png deleted");
        assert!(h.app.files().iter().any(|f| f.path == a), "the computer's a.png taken out of its grid by the server's deletion");
        // Its end dropped, the deletion does not stay under way: Delete would wait for it.
        assert_eq!(h.app.deleting, 0, "a deletion of the source left still counted as under way");
    }

    /// Audit 27/11 T.3 (#40): photos of a phone copied (Ctrl+C: the clip holds its session), the
    /// phone unplugged and plugged again: the session of before, broken, was handed out again,
    /// and every folder said the phone was unplugged. Unplugged, the phone is forgotten: whoever
    /// still holds the old session keeps it, a new opening gets a new one.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_phone_unplugged_is_opened_anew() {
        if own_folder("messages::tests::a_phone_unplugged_is_opened_anew") {
            return;
        }
        let mut h = Harness::new(&folder("phone-anew", &["a.png"]));
        let serial = "TESTPHONE0001";
        let held = crate::mtp::Phone::shared(serial);
        let phone = crate::devices::Device { name: "Phone".into(), path: format!("mtp://{serial}/"), phone: true, serial: Some(serial.into()), block: None, internal: false };
        h.app.tx.send(Msg::Devices(vec![phone])).unwrap();
        h.frame(Vec::new());
        h.app.tx.send(Msg::Devices(Vec::new())).unwrap();
        h.frame(Vec::new());
        assert!(!std::sync::Arc::ptr_eq(&held, &crate::mtp::Phone::shared(serial)), "the session of the phone unplugged handed out again");
    }
}
