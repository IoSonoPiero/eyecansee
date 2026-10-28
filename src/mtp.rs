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

use crate::decode::is_media;
use crate::source::{self, Error, File, Listing, NOT_FOUND};

/// Device storages with the name shown as a folder.
type Storages = Vec<(String, Arc<Storage>)>;

pub struct Phone {
    serial: String,
    /// Opened on first use: opening is asynchronous and can fail (device busy).
    open: OnceCell<(MtpDevice, Storages)>,
    handles: Mutex<HashMap<String, ObjectHandle>>,
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

/// The call a phone is answering right now (path, start): the status bar says when it takes long.
/// A POCO X7 Pro under Windows took 377 s to delete one file (2026-10-28), with everything else queued.
static WAITING: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

/// The file of a call the phone has not answered for at least `secs` seconds, with the seconds.
pub fn waiting(secs: u64) -> Option<(String, u64)> {
    let w = WAITING.lock().unwrap();
    let (path, since) = w.as_ref()?;
    let s = since.elapsed().as_secs();
    (s >= secs).then(|| (source::shown(path).to_string(), s))
}

fn error(e: mtp_rs::Error) -> Error {
    let id = match e {
        _ if e.is_exclusive_access() => "error-device-busy",
        // Also while the phone reconnects after switching to "File transfer": the UI retries.
        mtp_rs::Error::NoDevice => "error-no-device",
        _ => "error-detail",
    };
    (id, e.to_string())
}

/// On Windows the phone belongs to the WPD driver, which knows it by a serial different from the
/// USB one (POCO X7 Pro: `open_by_serial` gives `NoDevice` from WPD, then `Unsupported` from USB).
/// It is opened by USB location: mtp-rs matches it to the WPD device by VID/PID.
#[cfg(windows)]
async fn open(serial: &str) -> Result<MtpDevice, Error> {
    let found = MtpDevice::list_devices().map_err(error)?.into_iter().find(|d| d.serial_number.as_deref() == Some(serial));
    match found {
        Some(d) => MtpDevice::open_by_location(d.location_id).await.map_err(error),
        None => Err(error(mtp_rs::Error::NoDevice)),
    }
}

/// KDE's kiod may be terminated only after the user said yes (`allow_kiod`, "error-device-kde"):
/// it also forgets the passwords of network folders.
static KIOD_ALLOWED: AtomicBool = AtomicBool::new(false);

pub fn allow_kiod() {
    KIOD_ALLOWED.store(true, Ordering::Relaxed);
}

#[cfg(not(windows))]
async fn open(serial: &str) -> Result<MtpDevice, Error> {
    match MtpDevice::open_by_serial(serial).await {
        Err(e) if e.is_exclusive_access() => match release_from_desktop(serial) {
            Release::Done => {
                log!("phone {serial} released from the desktop");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                MtpDevice::open_by_serial(serial).await.map_err(error)
            }
            Release::AskKiod => Err(("error-device-kde", e.to_string())),
            Release::No => Err(error(e)),
        },
        r => r.map_err(error),
    }
}

#[cfg(not(windows))]
enum Release {
    Done,
    /// Only kiod holds it, and the user has not allowed terminating it yet.
    AskKiod,
    No,
}

/// GNOME: gvfs keeps the phone open and the direct connection finds it busy. Terminating the gvfs
/// process serving it frees it and the phone stays in "File transfer" (verified 2026-10-21;
/// `gio mount -u` instead put it back to "charging only"). gvfs does not take it back by itself:
/// a click in Nautilus or the next connection remounts it. The direct connection is much faster
/// in large folders (Pictures/9GAG: thumbnails in ~20 s instead of over 5 minutes).
/// KDE: kiod6 holds it through its kmtpd module, which D-Bus cannot unload; terminated, kiod6 comes
/// back on demand without kmtpd (verified on Kubuntu 26.04, 2026-10-25). It also forgets the
/// passwords it kept for network folders. Plasma 5: kiod5, the same (Kubuntu 24.04, 2026-10-25;
/// there the phone went back to "charging only" and opened once the user chose "File transfer").
#[cfg(not(windows))]
fn release_from_desktop(serial: &str) -> Release {
    let (mut released, mut ask) = (false, false);
    for (pid, comm) in holders(serial) {
        if comm.starts_with("kiod") && !KIOD_ALLOWED.load(Ordering::Relaxed) {
            ask = true;
            continue;
        }
        released |= kill(&pid);
    }
    // Flatpak: the host's processes are out of sight, gvfs is asked over D-Bus instead.
    if !released && !ask && std::env::var_os("FLATPAK_ID").is_some() {
        released = gvfs_unmount(serial);
    }
    match (released, ask) {
        (true, _) => Release::Done,
        (false, true) => Release::AskKiod,
        _ => Release::No,
    }
}

/// KDE: when Dolphin asks for the phone while EyeCanSee has it, kiod starts again, cannot open it
/// ("device is busy… Could not open MTP device") and never tries again: Dolphin then says the
/// device "does not exist", also after EyeCanSee lets it go (Plasma 5, 2026-10-25). Called as
/// EyeCanSee lets the phone go: terminated, kiod starts again when Dolphin asks, and opens it.
/// Only once the user has allowed terminating kiod (it forgets network passwords).
#[cfg(not(windows))]
fn drop_stale_kiod(serial: &str) {
    if !KIOD_ALLOWED.load(Ordering::Relaxed) {
        return;
    }
    for (pid, comm) in holders(serial) {
        if comm.starts_with("kiod") && kill(&pid) {
            log!("stale {comm} ({pid:?}) terminated for phone {serial}");
        }
    }
}

#[cfg(windows)]
fn drop_stale_kiod(_: &str) {}

/// Desktop processes (gvfs, kiod) with the phone's USB file open: (pid, name).
#[cfg(not(windows))]
fn holders(serial: &str) -> Vec<(std::ffi::OsString, String)> {
    use std::fs::{read_dir, read_link, read_to_string};
    // The phone's USB file (/dev/bus/usb/001/006), from the serial.
    let Some(node) = read_dir("/sys/bus/usb/devices").into_iter().flatten().flatten().find_map(|d| {
        (read_to_string(d.path().join("serial")).ok()?.trim() == serial).then_some(())?;
        let uevent = read_to_string(d.path().join("uevent")).ok()?;
        uevent.lines().find_map(|l| l.strip_prefix("DEVNAME=")).map(|n| format!("/dev/{n}"))
    }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for p in read_dir("/proc").into_iter().flatten().flatten() {
        let comm = read_to_string(p.path().join("comm")).unwrap_or_default().trim().to_string();
        if !matches!(comm.as_str(), "gvfsd-mtp" | "gvfsd-gphoto2" | "kiod6" | "kiod5") {
            continue;
        }
        let holds = read_dir(p.path().join("fd"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|f| read_link(f.path()).is_ok_and(|l| l.to_string_lossy() == node));
        if holds {
            found.push((p.file_name(), comm));
        }
    }
    found
}

/// Unmounts the phone from gvfs over D-Bus (`--talk-name=org.gtk.vfs.*`), like Nautilus's Eject: a
/// Samsung stays in "File transfer" (2026-10-26), a POCO X7 Pro went back to "charging only"
/// (2026-10-21), so only once per phone: after that EyeCanSee uses the gvfs mount.
#[cfg(not(windows))]
fn gvfs_unmount(serial: &str) -> bool {
    static DONE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut done = DONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if done.iter().any(|s| s == serial) {
        return false;
    }
    let gdbus = |args: &[&str]| std::process::Command::new("gdbus").arg("call").arg("--session").args(args).output().ok().filter(|o| o.status.success());
    let tracker = ["--dest", "org.gtk.vfs.Daemon", "--object-path", "/org/gtk/vfs/mounttracker"];
    let Some(list) = gdbus(&[&tracker[..], &["--method", "org.gtk.vfs.MountTracker.ListMounts2", "false"]].concat()) else { return false };
    let Some((dest, path)) = gvfs_mount(&String::from_utf8_lossy(&list.stdout), serial) else { return false };
    done.push(serial.to_string());
    let unmount = ["--dest", &dest, "--object-path", &path, "--method", "org.gtk.vfs.Mount.Unmount", "", "/", "0"];
    gdbus(&unmount).is_some()
}

/// In the reply of gvfs's ListMounts2, the D-Bus name and object of the phone's mount
/// (`mtp:host=<maker>_<model>_<serial>`). ponytail: a display name with a `'` (double quotes in
/// the reply) shifts the fields and the mount is not found: EyeCanSee uses gvfs then.
#[cfg(not(windows))]
fn gvfs_mount(list: &str, serial: &str) -> Option<(String, String)> {
    list.split("('").skip(1).find_map(|m| {
        let f: Vec<&str> = m.split('\'').collect();
        let ours = f.get(6)?.starts_with("mtp:host=") && f[6].ends_with(&format!("_{serial}"));
        (ours && f[2].starts_with("/org/gtk/vfs/mount/")).then(|| (f[0].to_string(), f[2].to_string()))
    })
}

#[cfg(not(windows))]
fn kill(pid: &std::ffi::OsStr) -> bool {
    std::process::Command::new("kill").arg(pid).status().is_ok_and(|s| s.success())
}

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
        Self { serial: serial.to_string(), open: OnceCell::new(), handles: Mutex::default(), complete: Mutex::default(), turn: tokio::sync::Mutex::const_new(()), downloads: tokio::sync::Mutex::const_new(()) }
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

    fn open_failed(&self, e: Error) -> Error {
        log!("phone {}: direct open failed: {} ({})", self.serial, e.0, e.1);
        after_open_failed(e, crate::devices::gvfs_mount(&self.serial).is_some())
    }

    async fn storages(&self) -> Result<&[(String, Arc<Storage>)], Error> {
        let (_, storages) = self
            .open
            .get_or_try_init(|| async {
                let t = std::time::Instant::now();
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
            if !o.is_folder() && source::leftover(&o.filename) {
                listing.leftovers.push(path.clone());
            }
            let modified = || o.modified.and_then(unix_seconds).map(|s| s.to_string()).unwrap_or_default();
            if !o.is_folder() && source::android_trashed(&o.filename).is_some() {
                listing.trashed.push(File { path, size: o.size, modified: modified() });
                continue;
            }
            // Hidden items are not shown, apart from EyeCanSee's trash folders.
            if o.filename.starts_with('.') && !(o.is_folder() && o.filename == source::TRASH_DIR) {
                continue;
            }
            if o.is_folder() {
                listing.dirs.push(path);
            } else if is_media(&o.filename) {
                listing.files.push(File { path, size: o.size, modified: modified() });
            }
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
                let h = self.queued("create_folder", to_dir, storage.create_folder(parent, source::name(to_dir))).await.map_err(error)?;
                self.handles.lock().unwrap().insert(to_dir.to_string(), h);
                h
            }
        };
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

    /// Everything below a folder, hidden items too, folders before what they contain: path and size
    /// (folders end in "/"). EyeCanSee's trash folders stay out.
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
                if o.filename != source::TRASH_DIR {
                    found.push((path.clone(), 0));
                    found.extend(Box::pin(self.walk(&path, p)).await?);
                }
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
        let h = self.queued("create_folder", dir, storage.create_folder(parent, source::name(dir))).await.map_err(error)?;
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
    /// replacing an item with the same name.
    pub async fn copy_inside(&self, path: &str, dest: &str) -> Result<(), Error> {
        if self.taken(dest).await? {
            self.delete(dest).await?;
        }
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        let dir = source::parent(dest);
        let folder = match self.locate(&dir).await? {
            (_, Some(h)) => h,
            (_, None) => ObjectHandle::ROOT,
        };
        let h = self.queued("copy", path, storage.copy_object(handle, folder, None)).await.map_err(error)?;
        if source::name(dest) != source::name(path) {
            self.queued("rename", dest, storage.rename(h, source::name(dest))).await.map_err(error)?;
        }
        self.handles.lock().unwrap().insert(dest.to_string(), h);
        Ok(())
    }

    /// Runs one mtp-rs call in its turn (see `turn`). A call that waits or runs for more than
    /// 2 s goes in the log with `what` and `path`: the phone takes one call at a time, and a
    /// slow one holds every other one back.
    async fn queued<T>(&self, what: &str, path: &str, call: impl std::future::Future<Output = T>) -> T {
        let t = std::time::Instant::now();
        let _turn = self.turn.lock().await;
        let waited = t.elapsed();
        *WAITING.lock().unwrap() = Some((path.to_string(), std::time::Instant::now()));
        let r = call.await;
        *WAITING.lock().unwrap() = None;
        let took = t.elapsed() - waited;
        if waited + took > std::time::Duration::from_secs(2) {
            log!("phone {}: {what} {path} waited {:.1} s, took {:.1} s", self.serial, waited.as_secs_f32(), took.as_secs_f32());
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
        match self.queued("upload", path, storage.upload(parent, NewObjectInfo::file(source::name(path), size), data)).await {
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
    fn waiting_call() {
        let old = WAITING.lock().unwrap().take();
        assert_eq!(waiting(3), None);
        let since = std::time::Instant::now() - std::time::Duration::from_secs(5);
        *WAITING.lock().unwrap() = Some(("DCIM/.trashed-1-a.jpg".to_string(), since));
        assert_eq!(waiting(3).map(|(n, s)| (n, s >= 5)), Some(("a.jpg".to_string(), true)));
        assert_eq!(waiting(30), None);
        *WAITING.lock().unwrap() = old;
    }
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn gvfs_mount_of_phone() {
        let list = "([(':1.919', objectpath '/org/gtk/vfs/mount/1', '', 'http:uri=https%3A%2F%2F127.0.0.1%3A8384', '', '', '', '', false, b'', (b'/', {'type': <b'http'>}), b''), (':1.1546', '/org/gtk/vfs/mount/1', 'mtp', 'mtp:host=SAMSUNG_SAMSUNG_Android_R52W80FSD5D', '', '. GThemedIcon phone', '', '', true, b'/run/user/1000/gvfs/mtp:host=SAMSUNG_SAMSUNG_Android_R52W80FSD5D', (b'/', {'host': <b'SAMSUNG_SAMSUNG_Android_R52W80FSD5D'>, 'type': <b'mtp'>}), b'')],)";
        assert_eq!(gvfs_mount(list, "R52W80FSD5D"), Some((":1.1546".into(), "/org/gtk/vfs/mount/1".into())));
        assert_eq!(gvfs_mount(list, "80FSD5D"), None);
        assert_eq!(gvfs_mount("([],)", "R52W80FSD5D"), None);
    }

    #[test]
    fn open_failure_ids() {
        let e = |id: &'static str| (id, "detail".to_string());
        assert_eq!(after_open_failed(e("error-detail"), true), ("error-use-mount", "detail".into()));
        assert_eq!(after_open_failed(e("error-detail"), false).0, "error-not-ready");
        assert_eq!(after_open_failed(e("error-device-busy"), true).0, "error-device-busy");
        assert_eq!(after_open_failed(e("error-no-device"), true).0, "error-no-device");
        assert_eq!(after_open_failed(e("error-device-kde"), true).0, "error-device-kde");
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
}
