//! Phones and cameras connected directly over MTP/PTP with `mtp-rs` (pure Rust), without going
//! through the desktop's mount: also on GNOME and KDE, where the phone is taken away from gvfs or
//! kiod (`release_from_desktop`), because gvfs is extremely slow in large folders.
//!
//! Paths as for the other sources: the root lists the device's storages
//! ("Internal shared storage/"), folders are below them. MTP works with numeric handles:
//! every listing remembers path → handle, and a path never seen is resolved by listing its parent.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use bytes::Bytes;
use mtp_rs::mtp::{DownloadPacing, FileDownload, MtpDevice, NewObjectInfo, ObjectHandle, Storage};
use tokio::sync::OnceCell;

use crate::source::{self, Error, Listing, NOT_FOUND};

/// Hidden folder where a copy into the file's own folder is made (see `Phone::copy_inside`).
const COPY_DIR: &str = ".EyeCanSee-copy";
/// The phone answered a copy without making it.
const NOT_MADE: &str = "error-copy-not-made";
/// A file the phone may take longer to copy than mtp-rs waits (30 s): its events stop meanwhile
/// (`across::Quiet`); smaller ones, done in a few seconds, keep them.
pub(crate) const LONG_COPY: u64 = 32 << 20;

mod across;
mod aside;
mod cache;
mod events;
mod storages;
pub(crate) mod turn;
mod undo;
pub use events::{Change, on_change, take_changes};

/// Device storages with the name shown as a folder.
type Storages = Vec<(String, Arc<Storage>)>;

pub struct Phone {
    serial: String,
    /// Opened on first use: opening is asynchronous and can fail (device busy).
    open: OnceCell<(MtpDevice, Storages)>,
    /// Shared with the events task (`events.rs`), which turns handles back into paths.
    handles: Arc<Mutex<HashMap<String, ObjectHandle>>>,
    /// Objects this app added or removed: their events are not news (`events::Own`).
    own: Arc<events::Own>,
    /// Dropped with the phone: the events task stops.
    _events: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Folders whose every child is in `handles` (read with `names` or `walk`, or created here):
    /// a copy checks there whether a name is taken, without listing the folder again for each file
    /// (70 files were 70 ever longer listings).
    complete: Arc<Mutex<HashSet<String>>>,
    /// One request at a time, in order of arrival. mtp-rs serialises too, but its lock lets the
    /// task that just released it take it again: during a copy the listing of the destination
    /// waited until the copy ended (4 s for 77 files). Taken around single mtp-rs calls only,
    /// never across our own methods, which call one another.
    turn: tokio::sync::Mutex<()>,
    /// One windowed download at a time. On Windows WPD keeps one open stream per phone and
    /// cannot seek: two downloads taking turns reopened the file at every window and re-read it
    /// from the start, each window slower than the last (POCO X7 Pro, 2026-10-28: 1.2 s to 3.7 s
    /// per 4 MB). Listings still pass between two windows of the same download.
    downloads: tokio::sync::Mutex<()>,
    /// A read inside a file was slow (Windows, #71): the phone or its driver has no partial reads,
    /// so WPD read the file from the start up to there. Its videos are then copied whole.
    ranges_slow: AtomicBool,
}

/// A read from this far into a file that takes longer than `RANGE_SLOW` means no partial reads.
const RANGE_FAR: u64 = 64 << 20;
const RANGE_SLOW: std::time::Duration = std::time::Duration::from_secs(5);

/// A file read in windows, holding the phone's `downloads` turn until it is dropped.
pub struct Download<'a> {
    file: FileDownload,
    path: String,
    /// The phone's `turn`, kept while a window is in flight (`holds_session`): mtp-rs holds the
    /// phone until the window is read, so a call taking the turn then waited for the phone while
    /// the window waited for the turn (copy and listing both stuck, mtp-rs 0.38, Windows 2026-11-07).
    window: Option<tokio::sync::MutexGuard<'a, ()>>,
    _turn: tokio::sync::MutexGuard<'a, ()>,
}

/// A phone date as `File::modified`, part of the key of its thumbnails. Usually the phone does
/// not write the time zone (mtp-rs: `to_unix_seconds` = `None`): its clock time as it is,
/// "2026-11-24T10:30:05", read in the computer's time zone only to be shown and sorted
/// (`human::parse_modified`). Converted here, the key changed with the computer's zone, and every
/// thumbnail of the phone was made again (audit 2.13). With its zone: Unix seconds.
fn phone_modified(m: mtp_rs::DateTime) -> String {
    if let Some(seconds) = m.to_unix_seconds() {
        return seconds.to_string();
    }
    let civil = jiff::civil::DateTime::new(m.year as i16, m.month as i8, m.day as i8, m.hour as i8, m.minute as i8, m.second as i8, 0);
    civil.map(|c| c.to_string()).unwrap_or_default()
}

/// The call a phone is answering right now (what, path, start or last data written): the status
/// bar says when it takes long.
/// A POCO X7 Pro under Windows took 377 s to delete one file (2026-10-28), with everything else queued.
static WAITING: Mutex<Option<(String, String, std::time::Instant)>> = Mutex::new(None);

/// The call in `WAITING` while it lasts, cleared however it ends (also dropped half way).
struct Waiting;

impl Waiting {
    fn start(what: &str, path: &str) -> Self {
        Self::start_at(what, path, std::time::Instant::now())
    }

    /// A call that goes on from `since`: the phone still copying after mtp-rs gave the call up
    /// (`across`), counted from its start, so the status line neither restarts nor disappears.
    fn start_at(what: &str, path: &str, since: std::time::Instant) -> Self {
        *WAITING.lock().unwrap() = Some((what.to_string(), path.to_string(), since));
        Waiting
    }

    /// Data of a write reached the phone: a write that moves is not stalled. Writing 555 MB to a
    /// POCO X6 Pro under Windows was one 17 s call, and the stall message came at 15 s (2026-11-11).
    fn moved() {
        if let Some((_, _, since)) = WAITING.lock().unwrap().as_mut() {
            *since = std::time::Instant::now();
        }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        *WAITING.lock().unwrap() = None;
    }
}

/// Seconds after which a call still waiting on Windows is taken for WPD's stall: ~340 s
/// unless the cable is unplugged. Calls that are only slow end well before.
pub const STALL_SECS: u64 = 15;

/// The file of a call the phone has not answered for at least `secs` seconds (a write: no data
/// taken for that long), the seconds, and what the call is ("upload": a file being written to the
/// phone, which unplugged would stay there half written; "copy" and "move": done by the phone).
pub fn waiting(secs: u64) -> Option<(String, u64, String)> {
    let w = WAITING.lock().unwrap();
    let (what, path, since) = w.as_ref()?;
    let s = since.elapsed().as_secs();
    (s >= secs).then(|| (source::shown(path).to_string(), s, what.clone()))
}

/// The phone copying or moving a file itself (between its storages it can take minutes,
/// `across`): "copy" or "move", the file, the seconds. Busy, not stalled.
pub fn working() -> Option<(String, String, u64)> {
    let (name, secs, what) = waiting(0)?;
    (what == "copy" || what == "move").then_some((what, name, secs))
}

fn error(e: mtp_rs::Error) -> Error {
    let id = match e {
        _ if e.is_exclusive_access() => "error-device-busy",
        // Also while the phone reconnects after switching to "File transfer": the UI retries.
        mtp_rs::Error::NoDevice => "error-no-device",
        // mtp-rs 0.36: refused before anything is sent (upload, new folder, rename); MTP names
        // hold 254 UTF-16 units, an emoji counts as two.
        mtp_rs::Error::NameTooLong { units } => return ("error-name-too-long", units.to_string()),
        // The cable pulled, the phone locked or switched off "File transfer".
        mtp_rs::Error::Disconnected => "error-phone-disconnected",
        mtp_rs::Error::Timeout => "error-phone-timeout",
        // The phone answered DeviceBusy: Windows Photos importing from it (POCO X7 Pro, 2026-11-08).
        mtp_rs::Error::Busy => "error-phone-busy",
        // Windows (WPD) when the phone goes away during a transfer: ERROR_OPERATION_ABORTED, then
        // 0x802a0002 (shut down already) on that session; mtp-rs 0.37.3 leaves both as `Other`
        // (POCO X7 Pro, upload cut off, 2026-11-04).
        mtp_rs::Error::Other { ref detail } if detail.starts_with("HRESULT 0x800703e3") || detail.starts_with("HRESULT 0x802a0002") => "error-phone-disconnected",
        // A copy onto a name the folder has (mtp-rs 0.37.3): `copy_inside` avoids it.
        mtp_rs::Error::AlreadyExists => "error-phone-name-taken",
        _ => "error-detail",
    };
    (id, e.to_string())
}

mod release;
use release::{drop_stale_kiod, open};
pub use release::{allow_kiod, taken_from_gvfs};

/// While opening, an error with no message of its own means the phone is not ready yet: in
/// "charging only" a POCO X7 Pro gave "I/O error: hardware fault or protocol violation"
/// (Kubuntu 24.04 in a VM, 2026-10-25). The UI then says to choose "File transfer" and retries.
fn not_ready((id, detail): Error) -> Error {
    (if id == "error-detail" { "error-not-ready" } else { id }, detail)
}

/// A direct connection that failed for a reason of its own (not busy, not missing) while the
/// desktop has the same phone mounted: "error-use-mount" makes the UI switch to that mount
/// (`App::use_mount`), as the busy case does. Without the mount the phone may just be not ready.
fn after_open_failed((id, detail): Error, mounted: bool) -> Error {
    match id {
        "error-detail" if mounted => ("error-use-mount", detail),
        _ => not_ready((id, detail)),
    }
}

impl Drop for Phone {
    fn drop(&mut self) {
        if self.open.initialized() {
            drop_stale_kiod(&self.serial);
        }
    }
}

/// Phones in use, by serial: the clipboard can keep the source of a copy, and a second opening
/// of the same phone would find it busy.
static PHONES: Mutex<Vec<(String, Weak<Phone>)>> = Mutex::new(Vec::new());

impl Phone {
    pub fn new(serial: &str) -> Self {
        Self {
            serial: serial.to_string(),
            open: OnceCell::new(),
            handles: Arc::default(),
            own: Arc::default(),
            _events: Mutex::new(None),
            complete: Arc::default(),
            turn: tokio::sync::Mutex::const_new(()),
            downloads: tokio::sync::Mutex::const_new(()),
            ranges_slow: Default::default(),
        }
    }

    /// The phones EyeCanSee has open, at exit: their Phone may never be dropped.
    pub fn let_go_all() {
        for (serial, p) in PHONES.lock().unwrap().iter() {
            if p.upgrade().is_some_and(|p| p.open.initialized()) {
                drop_stale_kiod(serial);
            }
        }
    }

    /// The phone already in use with this serial, or a new one.
    pub fn shared(serial: &str) -> Arc<Phone> {
        let mut phones = PHONES.lock().unwrap();
        phones.retain(|(_, p)| p.strong_count() > 0);
        if let Some(p) = phones.iter().find(|(s, _)| s == serial).and_then(|(_, p)| p.upgrade()) {
            return p;
        }
        let p = Arc::new(Phone::new(serial));
        phones.push((serial.to_string(), Arc::downgrade(&p)));
        p
    }

    /// After the phone `serial` was disconnected: the next `shared` opens it anew instead of handing
    /// out the session that broke, which whoever still holds it keeps (on Windows the open viewer
    /// keeps it while the phone list does not lose the phone).
    pub fn forget(serial: &str) {
        PHONES.lock().unwrap().retain(|(s, _)| s != serial);
    }

    fn open_failed(&self, e: Error) -> Error {
        log!("phone {}: direct open failed: {} ({})", self.serial, e.0, e.1);
        after_open_failed(e, crate::devices::gvfs_mount(&self.serial).is_some())
    }

    async fn storages(&self) -> Result<&[(String, Arc<Storage>)], Error> {
        let (_, storages) = self
            .open
            .get_or_try_init(|| async {
                let t = std::time::Instant::now();
                // Opening stalls too while WPD does; no file to name yet.
                let _waiting = Waiting::start("open", "");
                let device = open(&self.serial).await.map_err(|e| self.open_failed(e))?;
                log!("phone {} opened in {:.2} s", self.serial, t.elapsed().as_secs_f32());
                let found = device.storages().await.map_err(|e| not_ready(error(e)))?;
                log!("phone {} storages in {:.2} s", self.serial, t.elapsed().as_secs_f32());
                let names = storages::storage_names(&found.iter().map(|s| (s.info().description.clone(), s.id().0)).collect::<Vec<_>>());
                let storages: Vec<_> = names.into_iter().zip(found.into_iter().map(Arc::new)).collect();
                // Phone locked or in "charging only": it opens but exposes no storage.
                if storages.is_empty() {
                    return Err(("error-no-storage", self.serial.clone()));
                }
                let watch = events::Watch { serial: self.serial.clone(), device: device.clone(), handles: self.handles.clone(), complete: self.complete.clone(), own: self.own.clone() };
                *self._events.lock().unwrap() = Some(events::start(watch));
                Ok::<_, Error>((device, storages))
            })
            .await?;
        Ok(storages)
    }

    /// Storage and handle of a folder or file; `None` = root of the storage.
    async fn locate(&self, path: &str) -> Result<(Arc<Storage>, Option<ObjectHandle>), Error> {
        let trimmed = path.trim_end_matches('/');
        let (storage, rest) = storages::find(self.storages().await?, path).ok_or((NOT_FOUND, path.to_string()))?;
        let storage = storage.clone();
        if rest.is_empty() {
            return Ok((storage, None));
        }
        let cached = |p: &str| self.handles.lock().unwrap().get(p).copied();
        let handle = match cached(path) {
            Some(h) => h,
            None => {
                // Never seen: list the parent, which records it.
                Box::pin(self.list(&source::parent(trimmed))).await?;
                cached(path).ok_or((NOT_FOUND, path.to_string()))?
            }
        };
        Ok((storage, Some(handle)))
    }

    pub async fn list(&self, dir: &str) -> Result<Listing, Error> {
        let mut listing = Listing::default();
        if dir.is_empty() {
            listing.dirs = self.storages().await?.iter().map(|(name, _)| format!("{name}/")).collect();
            return Ok(listing);
        }
        let (storage, handle) = self.locate(dir).await?;
        let objects = self.queued("list", dir, storage.list_objects(handle)).await.map_err(error)?;
        let mut found = Vec::new();
        for o in objects {
            let path = cache::child(dir, &o.filename, o.is_folder());
            // Hidden items too: `locate` finds them here (the files of an interrupted "Save").
            found.push((path.clone(), o.handle));
            let modified = o.modified.map(phone_modified).unwrap_or_default();
            listing.add(path, &o.filename, o.is_folder(), o.size, modified, true);
        }
        let stale = aside::left_behind(dir, found.iter().map(|(p, _)| source::name(p.trim_end_matches('/'))));
        cache::listed(&mut self.handles.lock().unwrap(), dir, found);
        if let Some(tmp) = stale {
            let r = self.delete(&tmp).await;
            log!("phone {}: {tmp:?} left by a copy broken off, deleted: {r:?}", self.serial);
        }
        listing.sort();
        Ok(listing)
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.read_with("read", path, |s, h| async move { s.download_to_vec(h).await }).await
    }

    /// `len` bytes from `offset` (MTP GetPartialObject64).
    pub async fn read_range(&self, path: &str, offset: u64, len: u32) -> Result<Vec<u8>, Error> {
        let start = std::time::Instant::now();
        let r = self.read_with("read_range", path, |s, h| async move { s.read_range(h, offset, len).await }).await;
        // A partial read takes 0.16 s for 4 MB anywhere in a 1 GB file; reading up to the offset
        // took 16.6 s at the middle (POCO X6 Pro, mtp-rs 0.38.1). Waiting behind other calls
        // counts too: a busy phone at worst goes back to whole copies, as before.
        if cfg!(windows) && offset >= RANGE_FAR && start.elapsed() > RANGE_SLOW && !self.ranges_slow.swap(true, Ordering::Relaxed) {
            log!("phone {}: read at {offset} of {path} took {:.1} s: no partial reads, videos copied whole", self.serial, start.elapsed().as_secs_f32());
        }
        r
    }

    /// Whether a read inside a file was slow (`read_range`).
    pub fn ranges_slow(&self) -> bool {
        self.ranges_slow.load(Ordering::Relaxed)
    }

    /// The thumbnail the phone made for the file (MTP GetThumb): a video's still without reading
    /// the video.
    pub async fn thumbnail(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.read_with("thumbnail", path, |s, h| async move { s.thumbnail(h).await }).await
    }

    async fn read_with<F, Fut>(&self, what: &str, path: &str, read: F) -> Result<Vec<u8>, Error>
    where
        F: Fn(Arc<Storage>, ObjectHandle) -> Fut,
        Fut: std::future::Future<Output = Result<Vec<u8>, mtp_rs::Error>>,
    {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        match self.queued(what, path, read(storage, handle)).await {
            // Android can renumber handles during the session: list again and retry once.
            Err(e) if e.is_stale_handle() => {
                self.handles.lock().unwrap().clear();
                self.complete.lock().unwrap().clear();
                let (storage, handle) = self.locate(path).await?;
                let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
                self.queued(what, path, read(storage, handle)).await.map_err(error)
            }
            r => r.map_err(error),
        }
    }

    /// Deletes for good (a folder with what is inside it [Unverified on Android: tried with the trash]).
    pub async fn delete(&self, path: &str) -> Result<(), Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        self.own.mark(handle);
        self.queued("delete", path, storage.delete(handle)).await.map_err(error)?;
        cache::forget(&mut self.handles.lock().unwrap(), path);
        Ok(())
    }

    /// The whole new file `new` (a hidden name next to it) takes the place of `path`: `path` is
    /// deleted first, as MTP would keep two files with the same name. If that fails the new file
    /// goes and `path` stays; if the rename fails, the new file waits under its hidden name and
    /// takes the free name when the folder is read again (`Store::recover`).
    pub async fn put_in_place(&self, new: &str, path: &str) -> Result<(), Error> {
        match self.delete(path).await {
            Err(e) if e.0 != NOT_FOUND => {
                let _ = self.delete(new).await;
                return Err(e);
            }
            _ => {}
        }
        self.rename(new, source::name(path)).await.inspect_err(|e| log!("save {path:?}: the new file stays as {new:?}: {e:?}"))
    }

    /// Names of everything in a folder, of any type and hidden ones too: `list` keeps only images.
    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        if dir.is_empty() {
            return Ok(self.storages().await?.iter().map(|(n, _)| n.clone()).collect());
        }
        let (storage, handle) = self.locate(dir).await?;
        let objects = self.queued("names", dir, storage.list_objects(handle)).await.map_err(error)?;
        let mut handles = self.handles.lock().unwrap();
        cache::forget_children(&mut handles, dir);
        self.complete.lock().unwrap().insert(dir.to_string());
        Ok(objects
            .into_iter()
            .map(|o| {
                handles.insert(cache::child(dir, &o.filename, o.is_folder()), o.handle);
                o.filename
            })
            .collect())
    }

    /// Moves into `to_dir` (created if missing) with the name `new_name`: renamed in place first,
    /// MTP moves keep the name. The name must be free in both folders (`Store::delete`, `restore`).
    pub async fn move_into(&self, path: &str, to_dir: &str, new_name: &str) -> Result<(), Error> {
        // A folder to the other storage is refused at once: the phone would copy it whole inside
        // one call, longer than the 30 s mtp-rs waits for its answer, and the late answer then
        // leaves every next call waiting, Cancel too (Redmi Note 9 Pro, 152 photos, 690 MB, to its
        // SD card, 25/11/2026). The caller copies it file by file and deletes it (`paste`).
        if path.ends_with('/') && across(path, to_dir) {
            return Err((NOT_MADE, path.to_string()));
        }
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        // The destination's storage is given to the phone: without it, the storage the item
        // comes from (a move to the memory card ended up on the internal storage, audit 2.7).
        let (to, folder) = match self.locate(to_dir).await {
            Ok((to, Some(h))) => (to, h),
            // The top of a storage (restoring from its trash): it has no handle of its own.
            Ok((to, None)) => (to, ObjectHandle::ROOT),
            Err(_) => {
                let (to, parent) = self.locate(&source::parent(to_dir)).await?;
                let h = self.queued("create_folder", to_dir, self.marked(to.create_folder(parent, source::name(to_dir)))).await.map_err(error)?;
                self.handles.lock().unwrap().insert(to_dir.to_string(), h);
                (to, h)
            }
        };
        let size = self.size_across(path, (&storage, handle), &to).await?;
        let _quiet = size.map(|_| self.quiet());
        self.own.mark(handle);
        if new_name != source::name(path) {
            self.queued("rename", path, storage.rename(handle, new_name)).await.map_err(error)?;
        }
        let began = std::time::Instant::now();
        match (self.queued_kept("move", path, storage.move_object(handle, folder, Some(to.id()))).await, size) {
            // To the other storage, past the 30 s mtp-rs waits: the phone is still moving it.
            ((Err(mtp_rs::Error::Timeout), turn), Some(size)) => {
                self.finished_by_phone(("move", began, turn), path, (&to, folder), (new_name, size), Some((&storage, handle))).await?;
            }
            ((r, _), _) => r.map_err(error)?,
        }
        cache::forget(&mut self.handles.lock().unwrap(), path);
        let mut complete = self.complete.lock().unwrap();
        complete.remove(to_dir);
        complete.remove(&source::parent(path));
        Ok(())
    }

    /// Never onto a name already in the folder (MTP would keep both): `error-exists`. The folder
    /// is listed for it only the first time (`taken`).
    pub async fn rename(&self, path: &str, new_name: &str) -> Result<(), Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        let to = format!("{}{new_name}", source::parent(path));
        if self.taken(&to).await? || self.taken(&format!("{to}/")).await? {
            return Err(("error-exists", to));
        }
        self.own.mark(handle);
        self.queued("rename", path, storage.rename(handle, new_name)).await.map_err(error)?;
        // The folder's paths below it are stale too: the cache fills again at the next listing.
        // A file leaves its folder's cache complete, nothing else changed there: a paste over
        // many files renames each one (#90).
        let mut handles = self.handles.lock().unwrap();
        cache::forget(&mut handles, path);
        if path.ends_with('/') {
            drop(handles);
            self.complete.lock().unwrap().remove(&source::parent(path));
        } else {
            handles.insert(to, handle);
        }
        Ok(())
    }

    pub async fn size(&self, path: &str) -> Result<u64, Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        Ok(self.queued("size", path, storage.get_object_info(handle)).await.map_err(error)?.size)
    }

    /// Everything below a folder, hidden items and EyeCanSee's trash folders too (`transfer::place`),
    /// folders before what they contain: path and size (folders end in "/").
    pub async fn walk(&self, dir: &str, p: &crate::transfer::Progress) -> Result<Vec<(String, u64)>, Error> {
        if p.cancel.load(Ordering::Relaxed) {
            return Err(("error-cancelled", String::new()));
        }
        // Shown while waiting: a folder of thousands of files can take long to list.
        *p.current.lock().unwrap() = dir.to_string();
        let (storage, handle) = self.locate(dir).await?;
        let objects = self.queued("walk", dir, storage.list_objects(handle)).await.map_err(error)?;
        p.found.fetch_add(objects.len(), Ordering::Relaxed);
        self.complete.lock().unwrap().insert(dir.to_string());
        let mut found = Vec::new();
        for o in objects {
            let path = cache::child(dir, &o.filename, o.is_folder());
            self.handles.lock().unwrap().insert(path.clone(), o.handle);
            if o.is_folder() {
                found.push((path.clone(), 0));
                found.extend(Box::pin(self.walk(&path, p)).await?);
            } else {
                found.push((path, o.size));
            }
        }
        Ok(found)
    }

    /// Is there an item named `path`? An answer that does not come counts as no.
    pub async fn exists(&self, path: &str) -> bool {
        self.taken(path).await.unwrap_or(false)
    }

    /// Is there an item at `path`? Lists its folder only the first time.
    async fn taken(&self, path: &str) -> Result<bool, Error> {
        let dir = source::parent(path);
        if !self.complete.lock().unwrap().contains(&dir) {
            self.names(&dir).await?;
        }
        Ok(self.handles.lock().unwrap().contains_key(path))
    }

    /// Creates a folder whose parent exists; nothing if it is already there.
    pub async fn create_dir(&self, dir: &str) -> Result<(), Error> {
        if self.taken(dir).await? {
            return Ok(());
        }
        let (storage, parent) = self.locate(&source::parent(dir)).await?;
        let h = self.queued("create_folder", dir, self.marked(storage.create_folder(parent, source::name(dir)))).await.map_err(error)?;
        self.handles.lock().unwrap().insert(dir.to_string(), h);
        self.complete.lock().unwrap().insert(dir.to_string());
        Ok(())
    }

    /// A file read in windows of about 0.5 s, sized by mtp-rs from the speed (0.38): the phone stays
    /// free for listings between one and the next. Fixed 4 MB windows (until 0.37) cost ~10% at
    /// 30 MB/s (Galaxy A57, 2026-11-07); mtp-rs's default of 2 s made a listing or a thumbnail
    /// wait up to ~6 s during a copy (POCO X7 Pro, Windows, 2026-11-07).
    pub async fn download(&self, path: &str) -> Result<Download<'_>, Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        let _turn = self.downloads.lock().await;
        let file = self.queued("download", path, storage.download_with_pacing(handle, mtp_rs::mtp::ByteRange::Full, DownloadPacing::MaxCancelDelay(std::time::Duration::from_millis(500)))).await.map_err(error)?;
        Ok(Download { file, path: path.to_string(), window: None, _turn })
    }

    /// A `download` given up before its end (Cancel, or the chunks no longer wanted): mtp-rs reads
    /// the rest of the window in flight (at most about 0.5 s) and gives the phone back clean.
    /// Dropped without it mid-window, the phone answered nothing after it, every listing waiting
    /// for ever (Redmi Note 9 Pro, Cancel while a big file was read, 26/11).
    pub async fn stop_download<'a>(&'a self, download: &mut Download<'a>) {
        // The phone's events stopped meanwhile, as for a call mtp-rs gives up (`Quiet`): waited
        // for, they kept mtp-rs from getting the session back.
        let _quiet = self.quiet();
        if download.file.holds_session() && download.window.is_none() {
            download.window = Some(self.turn.lock().await);
        }
        if let Err(e) = download.file.cancel(mtp_rs::mtp::DEFAULT_CANCEL_TIMEOUT).await {
            log!("phone {}: download {:?} stopped: ERROR {e}", self.serial, download.path);
        }
        download.window = None;
    }

    /// The next chunk (up to 64 KB) of a `download`.
    pub async fn next_chunk<'a>(&'a self, download: &mut Download<'a>) -> Option<Result<Bytes, Error>> {
        if download.window.is_none() {
            download.window = Some(self.turn.lock().await);
        }
        let chunk = {
            let _waiting = Waiting::start("next_chunk", &download.path);
            download.file.next_chunk().await
        };
        if !download.file.holds_session() {
            download.window = None;
        }
        chunk.map(|r| r.map_err(error))
    }

    /// Copies a file inside the phone, without passing through the computer (MTP `CopyObject`),
    /// replacing an item with the same name. A copy keeps the source's name, and Android cannot
    /// copy onto a name the folder already has: in its own folder it answers "done" without making
    /// the copy ("Failed to end copy object", POCO X7 Pro, 2026-10-30), in another folder it writes
    /// the source over the file there without shortening it (its tail stays) and, through WPD,
    /// mtp-rs 0.36 handed back that file, which the rename after it renamed: "Keep both" ruined the
    /// file already there (seen on the POCO X7 Pro on Windows, 2026-11-04; Android's bug, mtp-rs #59,
    /// refused with `AlreadyExists` since 0.37.3). So whenever the name is taken there the copy is
    /// made in a hidden folder next to it and moved in with its new name.
    /// Replacing an item: the copy is made under the hidden name of a "Save" (`save_names`) and
    /// takes the item's place only once whole (`put_in_place`): until then the item stays, and a
    /// copy that fails leaves it as it was (issue #91; it was deleted first up to 0.76.1).
    /// `cancel` set while the phone copied: it ends the file all the same, then the copy made is
    /// deleted where it is and the error is "cancelled" (`undo`, #354).
    pub async fn copy_inside(&self, path: &str, dest: &str, cancel: &AtomicBool) -> Result<(), Error> {
        if self.taken(dest).await? {
            // `Store::recover`, at a listing meanwhile, would delete the new file next to its item.
            source::SAVING.lock().unwrap().insert(dest.to_string());
            let r = self.copy_over(path, dest, cancel).await;
            source::SAVING.lock().unwrap().remove(dest);
            return r;
        }
        let dir = source::parent(dest);
        let name = source::name(path).to_lowercase();
        let clash = dir == source::parent(path) || self.names(&dir).await?.iter().any(|n| n.to_lowercase() == name);
        if !clash {
            let h = self.copy_object(path, &dir).await?;
            let place = async {
                if source::name(dest) != source::name(path) {
                    self.queued("rename", dest, self.storage_of(path).await?.rename(h, source::name(dest))).await.map_err(error)?;
                }
                self.handles.lock().unwrap().insert(dest.to_string(), h);
                Ok(())
            };
            return undo::finish(cancel.load(Ordering::Relaxed), self.remove_copy(&dir, source::name(path), h), place).await;
        }
        self.copy_aside(path, dest, cancel).await
    }

    /// `path` copied over the item `dest`: whole under its hidden name first, then in its place.
    async fn copy_over(&self, path: &str, dest: &str, cancel: &AtomicBool) -> Result<(), Error> {
        let new = source::save_names(dest).0;
        // Left by a copy or a save cut short, next to its item: never the whole one.
        if self.taken(&new).await? {
            self.delete(&new).await?;
        }
        // Cancelled: the copy went with the hidden folder, and the item stays.
        self.copy_aside(path, &new, cancel).await?;
        self.put_in_place(&new, dest).await
    }

    /// `path` copied in the hidden folder next to `dest` and moved in as `dest`, a free name.
    async fn copy_aside(&self, path: &str, dest: &str, cancel: &AtomicBool) -> Result<(), Error> {
        let dir = source::parent(dest);
        // Left over from a copy broken off: what is in it is ours, and a copy is found there by name.
        let tmp = format!("{dir}{COPY_DIR}/");
        let _in_use = aside::InUse::start(&dir);
        if self.taken(&tmp).await? {
            self.delete(&tmp).await?;
        }
        self.create_dir(&tmp).await?;
        let copied = async {
            let h = self.copy_object(path, &tmp).await?;
            // The copy must be there under that handle before it is moved.
            let name = source::name(path);
            if !self.names(&tmp).await?.contains(name) || self.handles.lock().unwrap().get(&format!("{tmp}{name}")) != Some(&h) {
                return Err((NOT_MADE, path.to_string()));
            }
            let made = format!("{tmp}{name}");
            undo::finish(cancel.load(Ordering::Relaxed), self.remove_copy(&tmp, name, h), self.move_into(&made, &dir, source::name(dest))).await
        }
        .await;
        let gone = self.delete(&tmp).await;
        copied.and(gone)
    }

    /// `CopyObject` of `path` into the folder `dir`. A handle equal to the original's means a copy
    /// not made (see `copy_inside`): it is never renamed or moved.
    async fn copy_object(&self, path: &str, dir: &str) -> Result<ObjectHandle, Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        // Into the storage of `dir`, which may not be the one of `path` (audit 2.7).
        let (to, folder) = match self.locate(dir).await? {
            (to, Some(h)) => (to, h),
            (to, None) => (to, ObjectHandle::ROOT),
        };
        // A copy is made by the phone also inside one storage: a big one takes longer than mtp-rs
        // waits there too (6 GB, about a minute). The events stop for a file that may (`across`).
        let size = self.size_of(path, (&storage, handle)).await?;
        let _quiet = (size >= LONG_COPY).then(|| (self.quiet(), turn::Deaf::start(&self.serial, path)));
        let began = std::time::Instant::now();
        let h = match self.queued_kept("copy", path, self.marked(storage.copy_object(handle, folder, Some(to.id())))).await {
            // Given up while the phone is still copying it (`across::still_at_it`).
            (Err(e), turn) if across::still_at_it(&e, size, began.elapsed()) => {
                let h = self.finished_by_phone(("copy", began, turn), path, (&to, folder), (source::name(path), size), None).await?;
                self.own.mark(h);
                h
            }
            (r, _) => r.map_err(|e| across::copy_error(e, path))?,
        };
        log!("phone {}: copy {path} into {dir}: handle {} -> {}", self.serial, handle.0, h.0);
        if h == handle {
            return Err((NOT_MADE, path.to_string()));
        }
        Ok(h)
    }

    async fn storage_of(&self, path: &str) -> Result<Arc<Storage>, Error> {
        Ok(self.locate(path).await?.0)
    }

    /// `call`, which creates an object, with the object marked as ours as soon as it is there:
    /// its ObjectAdded event changes nothing (`events::Own`).
    async fn marked(&self, call: impl std::future::Future<Output = Result<ObjectHandle, mtp_rs::Error>>) -> Result<ObjectHandle, mtp_rs::Error> {
        let r = call.await;
        if let Ok(h) = &r {
            self.own.mark(*h);
        }
        r
    }

    /// Writes a file of `size` bytes from a stream, replacing one with the same name (MTP would
    /// keep both). A partial file left by an error is deleted.
    pub async fn upload<S>(&self, path: &str, size: u64, data: S) -> Result<(), Error>
    where
        S: futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Unpin + Send,
    {
        if self.taken(path).await? {
            self.delete(path).await?;
        }
        let (storage, parent) = self.locate(&source::parent(path)).await?;
        // mtp-rs takes each chunk as the phone takes the one before (back pressure).
        let data = futures_util::StreamExt::inspect(data, |_| Waiting::moved());
        // Marked as soon as it exists: its ObjectAdded event may come at once.
        let upload = async {
            let r = storage.upload(parent, NewObjectInfo::file(source::name(path), size), data).await;
            if let Some(h) = r.as_ref().ok().copied().or(r.as_ref().err().and_then(|e| e.partial)) {
                self.own.mark(h);
            }
            r
        };
        let _deaf = (size >= LONG_COPY).then(|| turn::Deaf::start(&self.serial, path));
        match self.queued("upload", path, upload).await {
            Ok(h) => {
                self.handles.lock().unwrap().insert(path.to_string(), h);
                Ok(())
            }
            Err(e) => {
                if let Some(h) = e.partial {
                    let _ = self.queued("delete", path, storage.delete(h)).await;
                }
                Err(error(e.source))
            }
        }
    }

    /// Does the folder exist? (for cache maintenance).
    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        if dir.is_empty() { Ok(()) } else { self.locate(dir).await.map(|_| ()) }
    }
}

/// Whether `path` and the folder `to_dir` of a phone are on two different storages: the first
/// part of a phone's path is its storage.
fn across(path: &str, to_dir: &str) -> bool {
    path.split('/').next() != to_dir.split('/').next()
}

#[cfg(test)]
mod tests {

    /// Audit 2.13: a phone's date without a time zone (what phones write) is part of the key of
    /// its thumbnails (`File::modified`). Read in the computer's time zone, the same photo had
    /// another key once the zone changed (a laptop on a journey): no thumbnail of the phone
    /// matched any more, and the maintenance read every photo and video of the phone again,
    /// whole. The key must be the same in any zone; a date with its zone stays an instant.
    #[test]
    fn phone_date_key_ignores_the_time_zone() {
        use jiff::tz::{Offset, TimeZone};
        let taken = mtp_rs::DateTime::new(2026, 11, 24, 10, 30, 5).unwrap();
        let rome = TimeZone::fixed(Offset::constant(1));
        let new_york = TimeZone::fixed(Offset::constant(-5));
        let key = super::phone_modified(taken);
        assert_eq!(key, "2026-11-24T10:30:05");
        // Shown and sorted in the computer's zone: the phone's clock time, whatever the zone.
        let at = |tz: &TimeZone| crate::human::parse_modified_in(&key, tz).unwrap().to_zoned(tz.clone()).datetime();
        let clock = jiff::civil::date(2026, 11, 24).at(10, 30, 5, 0);
        assert_eq!((at(&rome), at(&new_york)), (clock, clock));
        assert_eq!(crate::human::parse_modified_in(&key, &rome).unwrap().as_second(), 1795516205 - 3600);
        let utc = taken.with_offset(mtp_rs::UtcOffset::UTC);
        assert_eq!(super::phone_modified(utc), "1795516205");
    }

    #[test]
    fn windows_abort_is_disconnected() {
        let other = |d: &str| super::error(mtp_rs::Error::Other { detail: d.into() }).0;
        assert_eq!(other("HRESULT 0x800703e3: Operazione di I/O terminata"), "error-phone-disconnected");
        assert_eq!(other("HRESULT 0x802a0002: shut down"), "error-phone-disconnected");
        assert_eq!(other("HRESULT 0x80070057: parameter"), "error-detail");
    }

    #[test]
    fn forget_opens_anew() {
        let a = super::Phone::shared("FORGET-TEST");
        assert!(std::sync::Arc::ptr_eq(&a, &super::Phone::shared("FORGET-TEST")));
        super::Phone::forget("FORGET-TEST");
        assert!(!std::sync::Arc::ptr_eq(&a, &super::Phone::shared("FORGET-TEST")));
    }

    #[test]
    fn waiting_call() {
        let old = WAITING.lock().unwrap().take();
        assert_eq!(waiting(3), None);
        let since = std::time::Instant::now() - std::time::Duration::from_secs(5);
        *WAITING.lock().unwrap() = Some(("delete".to_string(), "DCIM/.trashed-1-a.jpg".to_string(), since));
        assert_eq!(waiting(3).map(|(n, s, what)| (n, s >= 5, what)), Some(("a.jpg".to_string(), true, "delete".to_string())));
        assert_eq!(working(), None, "a delete is not a copy");
        assert_eq!(waiting(30), None);
        *WAITING.lock().unwrap() = Some(("upload".to_string(), "DCIM/b.jpg".to_string(), since));
        assert_eq!(waiting(3).map(|(_, _, what)| what).as_deref(), Some("upload"));
        Waiting::moved();
        assert_eq!(waiting(3), None, "a write whose data moves is not stalled");
        *WAITING.lock().unwrap() = Some(("move".to_string(), "SD/DCIM/v.mp4".to_string(), since));
        assert_eq!(working().map(|(w, n, s)| (w, n, s >= 5)), Some(("move".to_string(), "v.mp4".to_string(), true)));
        *WAITING.lock().unwrap() = old;
    }
    use super::*;

    /// A folder moved to the other storage is copied and deleted instead (see `move_into`).
    #[test]
    fn folders_across_storages() {
        assert!(across("Internal storage/DCIM/Bing/", "SD card/DCIM/"));
        assert!(across("Internal storage/DCIM/Bing/", "SD card/"));
        assert!(!across("Internal storage/DCIM/Bing/", "Internal storage/Pictures/"));
        assert!(!across("SD card/a/", "SD card/"));
    }

    #[test]
    fn open_failure_ids() {
        let e = |id: &'static str| (id, "detail".to_string());
        assert_eq!(after_open_failed(e("error-detail"), true), ("error-use-mount", "detail".into()));
        assert_eq!(after_open_failed(e("error-detail"), false).0, "error-not-ready");
        assert_eq!(after_open_failed(e("error-device-busy"), true).0, "error-device-busy");
        assert_eq!(after_open_failed(e("error-no-device"), true).0, "error-no-device");
        assert_eq!(after_open_failed(e("error-device-kde"), true).0, "error-device-kde");
        assert_eq!(error(mtp_rs::Error::NameTooLong { units: 255 }), ("error-name-too-long", "255".into()));
    }

    /// The events of a real phone while something changes on it, with the seconds since the start:
    /// `EYECANSEE_MTP=<serial> [EYECANSEE_MTP_DIR=<folder>] [EYECANSEE_EVENTS_SECS=60] cargo test --release mtp_events_live -- --ignored --nocapture`
    /// (the folder is listed first, so its files have a path; default the first storage's DCIM/Camera/).
    #[test]
    #[ignore]
    fn mtp_events_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        let secs = std::env::var("EYECANSEE_EVENTS_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Phone::new(&serial);
            let root = phone.list("").await.unwrap();
            let dir = std::env::var("EYECANSEE_MTP_DIR").unwrap_or_else(|_| format!("{}DCIM/Camera/", root.dirs[0]));
            let l = phone.list(&dir).await.unwrap();
            println!("{dir}: {} files, waiting {secs} s for events", l.files.len());
            let t = std::time::Instant::now();
            while t.elapsed().as_secs() < secs {
                for (s, c) in take_changes() {
                    println!("{:7.2} s  {s} {c:?}", t.elapsed().as_secs_f32());
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
    }

    /// Test on a real phone, not mounted by the desktop:
    /// `EYECANSEE_MTP=<serial> cargo test mtp_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Phone::new(&serial);
            let root = phone.list("").await.unwrap();
            println!("root: {:?}", root.dirs);
            let first = &root.dirs[0];
            let t = std::time::Instant::now();
            let l = phone.list(first).await.unwrap();
            println!("{first}: {:?} in {:?}", l.dirs, t.elapsed());
            let camera = format!("{first}DCIM/Camera/");
            let t = std::time::Instant::now();
            match phone.list(&camera).await {
                Ok(l) => {
                    println!("{camera}: {} files in {:?}", l.files.len(), t.elapsed());
                    for f in l.files.iter().take(3) {
                        println!("  {} {} bytes, modified {:?}", f.path, f.size, f.modified);
                    }
                    let t = std::time::Instant::now();
                    let mut bytes = 0;
                    for f in &l.files {
                        bytes += phone.read(&f.path).await.unwrap().len();
                    }
                    println!("read {bytes} bytes in {:?}", t.elapsed());
                    // The path of a copy (`Store::send_chunks`): all the files, then the biggest alone.
                    let biggest = l.files.iter().max_by_key(|f| f.size).unwrap();
                    // A listing while a download is half way: it must get in between two windows.
                    let during = async {
                        let mut d = phone.download(&biggest.path).await.unwrap();
                        while let Some(w) = phone.next_chunk(&mut d).await {
                            w.unwrap();
                        }
                    };
                    let listing = async {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        let t = std::time::Instant::now();
                        phone.list(first).await.unwrap();
                        t.elapsed()
                    };
                    let both = tokio::time::timeout(std::time::Duration::from_secs(60), async { tokio::join!(during, listing) });
                    println!("listing during a download: {:?}", both.await.expect("download and listing stuck").1);
                    for (what, files) in [("all", l.files.iter().collect::<Vec<_>>()), ("biggest", vec![biggest])] {
                        let t = std::time::Instant::now();
                        let mut bytes = 0;
                        for f in files {
                            let mut d = phone.download(&f.path).await.unwrap();
                            while let Some(w) = phone.next_chunk(&mut d).await {
                                bytes += w.unwrap().len();
                            }
                        }
                        println!("download {what}: {bytes} bytes in {:?}", t.elapsed());
                    }
                }
                Err(e) => println!("{camera}: {e:?}"),
            }
        });
    }

    /// The objects of a folder on the phone, with handle, name and size.
    /// No Cancel during the copies of these tests.
    static NEVER: AtomicBool = AtomicBool::new(false);

    async fn objects(phone: &Phone, dir: &str) -> Vec<(ObjectHandle, String, u64)> {
        let (storage, h) = phone.locate(dir).await.unwrap();
        storage.list_objects(h).await.unwrap().into_iter().map(|o| (o.handle, o.filename, o.size)).collect()
    }

    /// Ctrl+C and Ctrl+V of a file on a real phone, into its own folder and into a subfolder: the
    /// original keeps its name, handle and content, each copy has the same content, the hidden
    /// folder of `copy_inside` is gone; "Keep both" into a folder that has another file with the
    /// name leaves that file untouched. Works in a new folder `EyeCanSee-copia/` of the first storage,
    /// deleted at the end:
    /// `EYECANSEE_MTP=<serial> cargo test --release mtp_copy_inside_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_copy_inside_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Phone::new(&serial);
            let root = phone.list("").await.unwrap();
            let dir = format!("{}EyeCanSee-copia/", root.dirs[0]);
            assert!(!phone.taken(&dir).await.unwrap(), "{dir} already exists");
            phone.create_dir(&dir).await.unwrap();
            phone.create_dir(&format!("{dir}sub/")).await.unwrap();
            let src = format!("{dir}a.jpg");
            let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
            phone.upload(&src, data.len() as u64, futures_util::stream::iter([Ok(Bytes::from(data.clone()))])).await.unwrap();
            let before = objects(&phone, &dir).await;
            println!("before: {before:?}");
            let t = std::time::Instant::now();
            let same = phone.copy_inside(&src, &format!("{dir}a (2).jpg"), &NEVER).await;
            println!("same folder: {same:?} in {:?}", t.elapsed());
            let other = phone.copy_inside(&src, &format!("{dir}sub/a.jpg"), &NEVER).await;
            println!("subfolder: {other:?}");
            // "Keep both" into a folder that has another, longer a.jpg: it must stay as it was.
            phone.create_dir(&format!("{dir}keep/")).await.unwrap();
            let longer: Vec<u8> = (0..300_000u32).map(|i| (i * 13 % 241) as u8).collect();
            phone.upload(&format!("{dir}keep/a.jpg"), longer.len() as u64, futures_util::stream::iter([Ok(Bytes::from(longer.clone()))])).await.unwrap();
            let both = phone.copy_inside(&src, &format!("{dir}keep/a (2).jpg"), &NEVER).await;
            println!("keep both: {both:?}");
            let kept = objects(&phone, &format!("{dir}keep/")).await;
            println!("keep: {kept:?}");
            let after = objects(&phone, &dir).await;
            let below = objects(&phone, &format!("{dir}sub/")).await;
            println!("after: {after:?}\nsub: {below:?}");
            let (storage, _) = phone.locate(&dir).await.unwrap();
            let content = |h| storage.download_to_vec(h);
            let original = before.iter().find(|o| o.1 == "a.jpg").unwrap();
            let check = async {
                assert!(same.is_ok() && other.is_ok());
                assert!(after.contains(original), "the original keeps name, handle and size");
                assert_eq!(content(original.0).await.unwrap(), data);
                let copy = after.iter().find(|o| o.1 == "a (2).jpg").expect("copy in the same folder");
                assert_eq!(content(copy.0).await.unwrap(), data);
                assert!(!after.iter().any(|o| o.1 == COPY_DIR), "hidden folder left behind");
                let sub = below.iter().find(|o| o.1 == "a.jpg").expect("copy in the subfolder");
                assert_eq!(content(sub.0).await.unwrap(), data);
                assert!(both.is_ok());
                let there = kept.iter().find(|o| o.1 == "a.jpg").expect("the file already there");
                assert_eq!(content(there.0).await.unwrap(), longer, "the file already there untouched");
                let copy = kept.iter().find(|o| o.1 == "a (2).jpg").expect("the copy next to it");
                assert_eq!(content(copy.0).await.unwrap(), data);
                assert!(!kept.iter().any(|o| o.1 == COPY_DIR), "hidden folder left behind");
            };
            let r = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(check)).await;
            phone.delete(&dir).await.unwrap();
            if let Err(e) = r {
                std::panic::resume_unwind(e);
            }
        });
    }

    /// #354, Cancel while a real phone copies by itself: the phone ends the file, then the copy is
    /// gone, wherever it was made. Three copies with Cancel already asked: into another folder (a
    /// free name), into the file's own folder (the hidden folder), over a longer file with the same
    /// name (which must stay as it was). Each ends as cancelled; the original keeps name, handle and
    /// content, and no hidden folder is left. Works in a new folder `EyeCanSee-annulla/` of the
    /// first storage, deleted at the end:
    /// `EYECANSEE_MTP=<serial> cargo test --release mtp_copy_cancel_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_copy_cancel_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Phone::new(&serial);
            let root = phone.list("").await.unwrap();
            let dir = format!("{}EyeCanSee-annulla/", root.dirs[0]);
            assert!(!phone.taken(&dir).await.unwrap(), "{dir} already exists");
            phone.create_dir(&dir).await.unwrap();
            phone.create_dir(&format!("{dir}sub/")).await.unwrap();
            phone.create_dir(&format!("{dir}keep/")).await.unwrap();
            let src = format!("{dir}a.jpg");
            let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
            phone.upload(&src, data.len() as u64, futures_util::stream::iter([Ok(Bytes::from(data.clone()))])).await.unwrap();
            let longer: Vec<u8> = (0..300_000u32).map(|i| (i * 13 % 241) as u8).collect();
            phone.upload(&format!("{dir}keep/a.jpg"), longer.len() as u64, futures_util::stream::iter([Ok(Bytes::from(longer.clone()))])).await.unwrap();
            let before = (objects(&phone, &dir).await, objects(&phone, &format!("{dir}keep/")).await);
            let asked = AtomicBool::new(true);
            let other = phone.copy_inside(&src, &format!("{dir}sub/a.jpg"), &asked).await;
            let same = phone.copy_inside(&src, &format!("{dir}a (2).jpg"), &asked).await;
            let over = phone.copy_inside(&src, &format!("{dir}keep/a.jpg"), &asked).await;
            println!("other folder: {other:?}\nsame folder: {same:?}\nover a file: {over:?}");
            let after = (objects(&phone, &dir).await, objects(&phone, &format!("{dir}keep/")).await);
            let below = objects(&phone, &format!("{dir}sub/")).await;
            println!("before: {before:?}\nafter: {after:?}\nsub: {below:?}");
            let (storage, _) = phone.locate(&dir).await.unwrap();
            let check = async {
                for r in [&other, &same, &over] {
                    assert_eq!(r.as_ref().unwrap_err().0, "error-cancelled");
                }
                assert_eq!(after, before, "the original and the file that was to be replaced as they were, nothing more");
                assert!(below.is_empty(), "a copy left in the other folder: {below:?}");
                let original = after.0.iter().find(|o| o.1 == "a.jpg").unwrap();
                assert_eq!(storage.download_to_vec(original.0).await.unwrap(), data);
                let there = after.1.iter().find(|o| o.1 == "a.jpg").unwrap();
                assert_eq!(storage.download_to_vec(there.0).await.unwrap(), longer);
            };
            let r = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(check)).await;
            phone.delete(&dir).await.unwrap();
            if let Err(e) = r {
                std::panic::resume_unwind(e);
            }
        });
    }

    /// Audit 2.7, copy and move between the two storages of a real phone (one with a memory card):
    /// a copy into a folder of the second storage and a move to the top of it end up there, with
    /// their content, and nothing is left on the first storage. Works in new folders
    /// `EyeCanSee-two/` of both storages, deleted at the end (with the moved file):
    /// `EYECANSEE_MTP=<serial> cargo test --release mtp_two_storages_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_two_storages_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Phone::new(&serial);
            let root = phone.list("").await.unwrap();
            assert!(root.dirs.len() >= 2, "the phone has one storage: {:?}", root.dirs);
            let (first, second) = (root.dirs[0].clone(), root.dirs[1].clone());
            let (a, b) = (format!("{first}EyeCanSee-two/"), format!("{second}EyeCanSee-two/"));
            let moved = format!("{second}EyeCanSee-two-moved.jpg");
            for d in [&a, &b, &moved, &format!("{first}EyeCanSee-two-moved.jpg")] {
                assert!(!phone.taken(d).await.unwrap(), "{d} already exists");
            }
            phone.create_dir(&a).await.unwrap();
            phone.create_dir(&b).await.unwrap();
            let data: Vec<u8> = (0..100_000u32).map(|i| (i * 7 % 251) as u8).collect();
            for name in ["copied.jpg", "moved.jpg"] {
                let up = futures_util::stream::iter([Ok(Bytes::from(data.clone()))]);
                phone.upload(&format!("{a}{name}"), data.len() as u64, up).await.unwrap();
            }
            let copy = phone.copy_inside(&format!("{a}copied.jpg"), &format!("{b}copied.jpg"), &NEVER).await;
            let mv = phone.move_into(&format!("{a}moved.jpg"), &second, "EyeCanSee-two-moved.jpg").await;
            // A folder is refused at once, left where it is: the caller copies it file by file.
            phone.create_dir(&format!("{a}folder/")).await.unwrap();
            let t = std::time::Instant::now();
            let folder = phone.move_into(&format!("{a}folder/"), &b, "folder").await;
            let folder_time = t.elapsed();
            println!("copy: {copy:?}, move: {mv:?}, folder: {folder:?} in {folder_time:?}");
            let in_a = objects(&phone, &a).await;
            let in_b = objects(&phone, &b).await;
            let top_first = objects(&phone, &first).await;
            let top_second = objects(&phone, &second).await;
            println!("first storage, folder: {in_a:?}\nsecond storage, folder: {in_b:?}");
            let (storage_b, _) = phone.locate(&b).await.unwrap();
            let check = async {
                assert!(copy.is_ok() && mv.is_ok(), "copy {copy:?}, move {mv:?}");
                let c = in_b.iter().find(|o| o.1 == "copied.jpg").expect("the copy in the folder of the second storage");
                assert_eq!(storage_b.download_to_vec(c.0).await.unwrap(), data);
                let m = top_second.iter().find(|o| o.1 == "EyeCanSee-two-moved.jpg").expect("the moved file at the top of the second storage");
                assert_eq!(storage_b.download_to_vec(m.0).await.unwrap(), data);
                assert!(!top_first.iter().any(|o| o.1.starts_with("EyeCanSee-two-moved")), "the moved file went to the first storage");
                assert!(in_a.iter().any(|o| o.1 == "copied.jpg") && !in_a.iter().any(|o| o.1 == "moved.jpg"), "first storage, folder: {in_a:?}");
                assert!(folder.is_err() && folder_time.as_secs() < 2, "a folder to the other storage: {folder:?} in {folder_time:?}");
                assert!(in_a.iter().any(|o| o.1 == "folder") && !in_b.iter().any(|o| o.1 == "folder"), "the folder stays where it was");
            };
            let r = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(check)).await;
            // Wherever they ended up: the folders, and the moved file read from the storages
            // themselves (the cache of paths may hold an old listing of their tops).
            phone.delete(&a).await.unwrap();
            phone.delete(&b).await.unwrap();
            for top in [&first, &second] {
                let (storage, _) = phone.locate(top).await.unwrap();
                for (h, name, _) in objects(&phone, top).await {
                    if name.starts_with("EyeCanSee-two-moved") {
                        storage.delete(h).await.unwrap();
                    }
                }
            }
            if let Err(e) = r {
                std::panic::resume_unwind(e);
            }
        });
    }

    /// Issue #91, Ctrl+C and Ctrl+V with Replace on a real phone: over a file of its own folder,
    /// over one in a subfolder and over itself, the replaced file gets the copy's content and no
    /// hidden name or folder is left; a copy that fails (its source gone) leaves the file it would
    /// have replaced as it was. "Keep both" in its own folder too, through the hidden folder. Works in a new folder `Download/EyeCanSee-replace/` of the first
    /// storage, deleted at the end:
    /// `EYECANSEE_MTP=<serial> cargo test --release mtp_copy_replace_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_copy_replace_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Phone::new(&serial);
            let root = phone.list("").await.unwrap();
            let dir = format!("{}Download/EyeCanSee-replace/", root.dirs[0]);
            assert!(!phone.taken(&dir).await.unwrap(), "{dir} already exists");
            phone.create_dir(&dir).await.unwrap();
            phone.create_dir(&format!("{dir}sub/")).await.unwrap();
            let file = |seed: u32, len: u32| (0..len).map(|i| (i * seed % 251) as u8).collect::<Vec<u8>>();
            let (a, b, c) = (file(7, 100_000), file(13, 300_000), file(17, 50_000));
            for (name, data) in [("a.jpg", &a), ("b.jpg", &b), ("sub/a.jpg", &c)] {
                phone.upload(&format!("{dir}{name}"), data.len() as u64, futures_util::stream::iter([Ok(Bytes::from(data.clone()))])).await.unwrap();
            }
            let src = format!("{dir}a.jpg");
            // Not a replace: "Keep both" in its own folder, through the hidden folder.
            let both = phone.copy_inside(&src, &format!("{dir}a (2).jpg"), &NEVER).await;
            println!("keep both: {both:?}");
            let same = phone.copy_inside(&src, &format!("{dir}b.jpg"), &NEVER).await;
            println!("over b.jpg: {same:?}");
            let below = phone.copy_inside(&src, &format!("{dir}sub/a.jpg"), &NEVER).await;
            println!("over sub/a.jpg: {below:?}");
            let itself = phone.copy_inside(&src, &src, &NEVER).await;
            println!("over itself: {itself:?}");
            let failed = phone.copy_inside(&format!("{dir}missing.jpg"), &format!("{dir}b.jpg"), &NEVER).await;
            println!("from a missing file: {failed:?}");
            let after = objects(&phone, &dir).await;
            let sub = objects(&phone, &format!("{dir}sub/")).await;
            println!("after: {after:?}\nsub: {sub:?}");
            let (storage, _) = phone.locate(&dir).await.unwrap();
            let content = |list: &[(ObjectHandle, String, u64)], name: &str| {
                let h = list.iter().find(|o| o.1 == name).unwrap_or_else(|| panic!("{name} missing")).0;
                storage.download_to_vec(h)
            };
            let check = async {
                assert!(both.is_ok() && same.is_ok() && below.is_ok() && itself.is_ok(), "copies failed");
                assert!(failed.is_err(), "a copy from a missing file");
                let mut names: Vec<&str> = after.iter().map(|o| o.1.as_str()).collect();
                names.sort();
                assert_eq!(names, ["a (2).jpg", "a.jpg", "b.jpg", "sub"], "one file per name, nothing hidden left");
                assert_eq!(sub.iter().map(|o| o.1.as_str()).collect::<Vec<_>>(), ["a.jpg"]);
                assert_eq!(content(&after, "a.jpg").await.unwrap(), a);
                assert_eq!(content(&after, "a (2).jpg").await.unwrap(), a);
                assert_eq!(content(&after, "b.jpg").await.unwrap(), a, "b.jpg replaced, and kept by the failed copy");
                assert_eq!(content(&sub, "a.jpg").await.unwrap(), a);
            };
            let r = futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(check)).await;
            phone.delete(&dir).await.unwrap();
            if let Err(e) = r {
                std::panic::resume_unwind(e);
            }
        });
    }
}
