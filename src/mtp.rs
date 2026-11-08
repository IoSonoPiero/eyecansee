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
use mtp_rs::mtp::{MtpDevice, NewObjectInfo, ObjectHandle, Storage, WindowedDownload};
use tokio::sync::OnceCell;

use crate::source::{self, Error, Listing, NOT_FOUND};

/// Hidden folder where a copy into the file's own folder is made (see `Phone::copy_inside`).
const COPY_DIR: &str = ".EyeCanSee-copy";
/// The phone answered a copy without making it.
const NOT_MADE: &str = "error-copy-not-made";

mod events;
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
    complete: Mutex<HashSet<String>>,
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
}

/// A file read in windows, holding the phone's `downloads` turn until it is dropped.
pub struct Download<'a> {
    windows: WindowedDownload,
    path: String,
    _turn: tokio::sync::MutexGuard<'a, ()>,
}

/// Unix seconds of a phone date. Usually the phone does not write the time zone, and then mtp-rs
/// does not convert it (`to_unix_seconds` = `None`): it is the phone's clock time, taken as the
/// computer's local time (almost always the same time zone).
fn unix_seconds(m: mtp_rs::DateTime) -> Option<i64> {
    m.to_unix_seconds().or_else(|| {
        let civil = jiff::civil::DateTime::new(m.year as i16, m.month as i8, m.day as i8, m.hour as i8, m.minute as i8, m.second as i8, 0).ok()?;
        Some(civil.to_zoned(jiff::tz::TimeZone::system()).ok()?.timestamp().as_second())
    })
}

/// The call a phone is answering right now (what, path, start): the status bar says when it takes long.
/// A POCO X7 Pro under Windows took 377 s to delete one file (2026-10-28), with everything else queued.
static WAITING: Mutex<Option<(String, String, std::time::Instant)>> = Mutex::new(None);

/// The call in `WAITING` while it lasts, cleared however it ends (also dropped half way).
struct Waiting;

impl Waiting {
    fn start(what: &str, path: &str) -> Self {
        *WAITING.lock().unwrap() = Some((what.to_string(), path.to_string(), std::time::Instant::now()));
        Waiting
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        *WAITING.lock().unwrap() = None;
    }
}

/// Seconds after which a call still waiting on Windows is taken for WPD's stall (FATTI): ~340 s
/// unless the cable is unplugged. Calls that are only slow end well before.
pub const STALL_SECS: u64 = 15;

/// The last call answered (what, path, end): named in the log line of a slow call, to see what
/// came before it without logging every call.
static LAST: Mutex<Option<(String, String, std::time::Instant)>> = Mutex::new(None);

/// The file of a call the phone has not answered for at least `secs` seconds, the seconds, and
/// whether it is a file being written to the phone (unplugged, it would stay there half written).
pub fn waiting(secs: u64) -> Option<(String, u64, bool)> {
    let w = WAITING.lock().unwrap();
    let (what, path, since) = w.as_ref()?;
    let s = since.elapsed().as_secs();
    (s >= secs).then(|| (source::shown(path).to_string(), s, what == "upload"))
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
            complete: Mutex::default(),
            turn: tokio::sync::Mutex::const_new(()),
            downloads: tokio::sync::Mutex::const_new(()),
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
                // Opening stalls too while WPD does (FATTI); no file to name yet.
                let _waiting = Waiting::start("open", "");
                let device = open(&self.serial).await.map_err(|e| self.open_failed(e))?;
                log!("phone {} opened in {:.2} s", self.serial, t.elapsed().as_secs_f32());
                let mut storages = Vec::new();
                let found = device.storages().await.map_err(|e| not_ready(error(e)))?;
                log!("phone {} storages in {:.2} s", self.serial, t.elapsed().as_secs_f32());
                for s in found {
                    let name = match s.info().description.trim() {
                        "" => format!("{}", s.id().0),
                        d => d.to_string(),
                    };
                    storages.push((name, Arc::new(s)));
                }
                // Phone locked or in "charging only": it opens but exposes no storage.
                if storages.is_empty() {
                    return Err(("error-no-storage", self.serial.clone()));
                }
                let watch = events::Watch { serial: self.serial.clone(), device: device.clone(), handles: self.handles.clone(), own: self.own.clone() };
                *self._events.lock().unwrap() = Some(events::start(watch));
                Ok::<_, Error>((device, storages))
            })
            .await?;
        Ok(storages)
    }

    /// Storage and handle of a folder or file; `None` = root of the storage.
    async fn locate(&self, path: &str) -> Result<(Arc<Storage>, Option<ObjectHandle>), Error> {
        let trimmed = path.trim_end_matches('/');
        let (first, rest) = trimmed.split_once('/').unwrap_or((trimmed, ""));
        let storage = self
            .storages()
            .await?
            .iter()
            .find(|(name, _)| name == first)
            .map(|(_, s)| s.clone())
            .ok_or((NOT_FOUND, path.to_string()))?;
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
        let mut handles = self.handles.lock().unwrap();
        for o in objects {
            let path = format!("{dir}{}{}", o.filename, if o.is_folder() { "/" } else { "" });
            // Hidden items too: `locate` finds them here (the files of an interrupted "Save").
            handles.insert(path.clone(), o.handle);
            let modified = o.modified.and_then(unix_seconds).map(|s| s.to_string()).unwrap_or_default();
            listing.add(path, &o.filename, o.is_folder(), o.size, modified, true);
        }
        listing.sort();
        Ok(listing)
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.read_with("read", path, |s, h| async move { s.download_to_vec(h).await }).await
    }

    /// `len` bytes from `offset` (MTP GetPartialObject64).
    pub async fn read_range(&self, path: &str, offset: u64, len: u32) -> Result<Vec<u8>, Error> {
        self.read_with("read_range", path, |s, h| async move { s.read_range(h, offset, len).await }).await
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
        // A folder takes the paths below it too.
        self.handles.lock().unwrap().retain(|p, _| !p.starts_with(path));
        Ok(())
    }

    /// Names of everything in a folder, of any type and hidden ones too: `list` keeps only images.
    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        if dir.is_empty() {
            return Ok(self.storages().await?.iter().map(|(n, _)| n.clone()).collect());
        }
        let (storage, handle) = self.locate(dir).await?;
        let objects = self.queued("names", dir, storage.list_objects(handle)).await.map_err(error)?;
        let mut handles = self.handles.lock().unwrap();
        handles.retain(|p, _| !(p.starts_with(dir) && source::parent(p) == dir));
        self.complete.lock().unwrap().insert(dir.to_string());
        Ok(objects
            .into_iter()
            .map(|o| {
                let slash = if o.is_folder() { "/" } else { "" };
                handles.insert(format!("{dir}{}{slash}", o.filename), o.handle);
                o.filename
            })
            .collect())
    }

    /// Moves into `to_dir` (created if missing) with the name `new_name`: renamed in place first,
    /// MTP moves keep the name. The name must be free in both folders (`Store::delete`, `restore`).
    pub async fn move_into(&self, path: &str, to_dir: &str, new_name: &str) -> Result<(), Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        let folder = match self.locate(to_dir).await {
            Ok((_, Some(h))) => h,
            // The top of a storage (restoring from its trash): it has no handle of its own.
            Ok((_, None)) => ObjectHandle::ROOT,
            Err(_) => {
                let (_, parent) = self.locate(&source::parent(to_dir)).await?;
                let h = self.queued("create_folder", to_dir, self.marked(storage.create_folder(parent, source::name(to_dir)))).await.map_err(error)?;
                self.handles.lock().unwrap().insert(to_dir.to_string(), h);
                h
            }
        };
        self.own.mark(handle);
        if new_name != source::name(path) {
            self.queued("rename", path, storage.rename(handle, new_name)).await.map_err(error)?;
        }
        self.queued("move", path, storage.move_object(handle, folder, None)).await.map_err(error)?;
        self.handles.lock().unwrap().retain(|p, _| !p.starts_with(path));
        let mut complete = self.complete.lock().unwrap();
        complete.remove(to_dir);
        complete.remove(&source::parent(path));
        Ok(())
    }

    /// The name must not be in the folder already: `App::rename_window` checks it against the listing.
    pub async fn rename(&self, path: &str, new_name: &str) -> Result<(), Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        self.own.mark(handle);
        self.queued("rename", path, storage.rename(handle, new_name)).await.map_err(error)?;
        // The folder's paths below it are stale too: the cache fills again at the next listing.
        let mut handles = self.handles.lock().unwrap();
        handles.retain(|p, _| !p.starts_with(path));
        if !path.ends_with('/') {
            handles.insert(format!("{}{new_name}", source::parent(path)), handle);
        }
        drop(handles);
        self.complete.lock().unwrap().remove(&source::parent(path));
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
            let path = format!("{dir}{}{}", o.filename, if o.is_folder() { "/" } else { "" });
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

    /// A file read in windows of 4 MB: the phone stays free for listings between one and the next.
    pub async fn download(&self, path: &str) -> Result<Download<'_>, Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        let _turn = self.downloads.lock().await;
        let windows = self.queued("download", path, storage.download_windowed(handle, mtp_rs::mtp::ByteRange::Full, 4 << 20)).await.map_err(error)?;
        Ok(Download { windows, path: path.to_string(), _turn })
    }

    /// The next window of a `download`.
    pub async fn next_window(&self, download: &mut Download<'_>) -> Option<Result<Vec<u8>, Error>> {
        self.queued("next_window", &download.path, download.windows.next_window()).await.map(|r| r.map_err(error))
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
    pub async fn copy_inside(&self, path: &str, dest: &str) -> Result<(), Error> {
        if self.taken(dest).await? {
            self.delete(dest).await?;
        }
        let dir = source::parent(dest);
        let name = source::name(path).to_lowercase();
        let clash = dir == source::parent(path) || self.names(&dir).await?.iter().any(|n| n.to_lowercase() == name);
        if !clash {
            let h = self.copy_object(path, &dir).await?;
            if source::name(dest) != source::name(path) {
                self.queued("rename", dest, self.storage_of(path).await?.rename(h, source::name(dest))).await.map_err(error)?;
            }
            self.handles.lock().unwrap().insert(dest.to_string(), h);
            return Ok(());
        }
        // Left over from a copy broken off: what is in it is ours, and a copy is found there by name.
        let tmp = format!("{dir}{COPY_DIR}/");
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
            self.move_into(&format!("{tmp}{name}"), &dir, source::name(dest)).await
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
        let folder = match self.locate(dir).await? {
            (_, Some(h)) => h,
            (_, None) => ObjectHandle::ROOT,
        };
        let h = self.queued("copy", path, self.marked(storage.copy_object(handle, folder, None))).await.map_err(error)?;
        log!("phone {}: copy {path} into {dir}: handle {} -> {}", self.serial, handle.0, h.0);
        if h == handle {
            return Err((NOT_MADE, path.to_string()));
        }
        Ok(h)
    }

    async fn storage_of(&self, path: &str) -> Result<Arc<Storage>, Error> {
        Ok(self.locate(path).await?.0)
    }

    /// Runs one mtp-rs call in its turn (see `turn`). A call that waits or runs for more than
    /// 2 s goes in the log with `what` and `path`: the phone takes one call at a time, and a
    /// slow one holds every other one back.
    async fn queued<T>(&self, what: &str, path: &str, call: impl std::future::Future<Output = T>) -> T {
        let t = std::time::Instant::now();
        let _turn = self.turn.lock().await;
        let waited = t.elapsed();
        let r = {
            let _waiting = Waiting::start(what, path);
            call.await
        };
        let took = t.elapsed() - waited;
        let mut last = LAST.lock().unwrap();
        if waited + took > std::time::Duration::from_secs(2) {
            // The call before this one: a stall came after a delete every time so far (2026-10-28).
            let before = match &*last {
                Some((w, p, end)) => format!(", after {w} {p} ended {:.1} s before", (t + waited).saturating_duration_since(*end).as_secs_f32()),
                None => String::new(),
            };
            log!("phone {}: {what} {path} waited {:.1} s, took {:.1} s{before}", self.serial, waited.as_secs_f32(), took.as_secs_f32());
        }
        *last = Some((what.to_string(), path.to_string(), std::time::Instant::now()));
        r
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
        // Marked as soon as it exists: its ObjectAdded event may come at once.
        let upload = async {
            let r = storage.upload(parent, NewObjectInfo::file(source::name(path), size), data).await;
            if let Some(h) = r.as_ref().ok().copied().or(r.as_ref().err().and_then(|e| e.partial)) {
                self.own.mark(h);
            }
            r
        };
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

#[cfg(test)]
mod tests {

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
        assert_eq!(waiting(3).map(|(n, s, up)| (n, s >= 5, up)), Some(("a.jpg".to_string(), true, false)));
        assert_eq!(waiting(30), None);
        *WAITING.lock().unwrap() = Some(("upload".to_string(), "DCIM/b.jpg".to_string(), since));
        assert_eq!(waiting(3).map(|(_, _, up)| up), Some(true));
        *WAITING.lock().unwrap() = old;
    }
    use super::*;

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
                }
                Err(e) => println!("{camera}: {e:?}"),
            }
        });
    }

    /// The objects of a folder on the phone, with handle, name and size.
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
            let same = phone.copy_inside(&src, &format!("{dir}a (2).jpg")).await;
            println!("same folder: {same:?} in {:?}", t.elapsed());
            let other = phone.copy_inside(&src, &format!("{dir}sub/a.jpg")).await;
            println!("subfolder: {other:?}");
            // "Keep both" into a folder that has another, longer a.jpg: it must stay as it was.
            phone.create_dir(&format!("{dir}keep/")).await.unwrap();
            let longer: Vec<u8> = (0..300_000u32).map(|i| (i * 13 % 241) as u8).collect();
            phone.upload(&format!("{dir}keep/a.jpg"), longer.len() as u64, futures_util::stream::iter([Ok(Bytes::from(longer.clone()))])).await.unwrap();
            let both = phone.copy_inside(&src, &format!("{dir}keep/a (2).jpg")).await;
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
}
