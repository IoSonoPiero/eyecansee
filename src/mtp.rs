//! Phones and cameras connected directly over MTP/PTP with `mtp-rs` (pure Rust), without going
//! through the desktop's mount: also on GNOME, where the phone is taken away from gvfs
//! (`release_from_gvfs`), because gvfs is extremely slow in large folders.
//!
//! Paths as for the other sources: the root lists the device's storages
//! ("Internal shared storage/"), folders are below them. MTP works with numeric handles:
//! every listing remembers path → handle, and a path never seen is resolved by listing its parent.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use mtp_rs::mtp::{MtpDevice, ObjectHandle, Storage};
use tokio::sync::OnceCell;

use crate::decode::is_image;
use crate::source::{self, Error, File, Listing, NOT_FOUND};

/// Device storages with the name shown as a folder.
type Storages = Vec<(String, Arc<Storage>)>;

pub struct Phone {
    serial: String,
    /// Opened on first use: opening is asynchronous and can fail (device busy).
    open: OnceCell<(MtpDevice, Storages)>,
    handles: Mutex<HashMap<String, ObjectHandle>>,
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
async fn open(serial: &str) -> Result<MtpDevice, mtp_rs::Error> {
    let found = MtpDevice::list_devices()?.into_iter().find(|d| d.serial_number.as_deref() == Some(serial));
    match found {
        Some(d) => MtpDevice::open_by_location(d.location_id).await,
        None => Err(mtp_rs::Error::NoDevice),
    }
}

#[cfg(not(windows))]
async fn open(serial: &str) -> Result<MtpDevice, mtp_rs::Error> {
    match MtpDevice::open_by_serial(serial).await {
        Err(e) if e.is_exclusive_access() && release_from_gvfs(serial) => {
            log!("phone {serial} released from gvfs");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            MtpDevice::open_by_serial(serial).await
        }
        r => r,
    }
}

/// GNOME: gvfs keeps the phone open and the direct connection finds it busy. Terminating the gvfs
/// process serving it frees it and the phone stays in "File transfer" (verified 2026-10-21;
/// `gio mount -u` instead put it back to "charging only"). gvfs does not take it back by itself:
/// a click in Nautilus or the next connection remounts it. The direct connection is much faster
/// in large folders (Pictures/9GAG: thumbnails in ~20 s instead of over 5 minutes).
#[cfg(not(windows))]
fn release_from_gvfs(serial: &str) -> bool {
    use std::fs::{read_dir, read_link, read_to_string};
    // The phone's USB file (/dev/bus/usb/001/006), from the serial.
    let Some(node) = read_dir("/sys/bus/usb/devices").into_iter().flatten().flatten().find_map(|d| {
        (read_to_string(d.path().join("serial")).ok()?.trim() == serial).then_some(())?;
        let uevent = read_to_string(d.path().join("uevent")).ok()?;
        uevent.lines().find_map(|l| l.strip_prefix("DEVNAME=")).map(|n| format!("/dev/{n}"))
    }) else {
        return false;
    };
    let mut released = false;
    for p in read_dir("/proc").into_iter().flatten().flatten() {
        let comm = read_to_string(p.path().join("comm")).unwrap_or_default();
        if !matches!(comm.trim(), "gvfsd-mtp" | "gvfsd-gphoto2") {
            continue;
        }
        let holds = read_dir(p.path().join("fd"))
            .into_iter()
            .flatten()
            .flatten()
            .any(|f| read_link(f.path()).is_ok_and(|l| l.to_string_lossy() == node));
        if holds {
            released |= std::process::Command::new("kill").arg(p.file_name()).status().is_ok_and(|s| s.success());
        }
    }
    released
}

impl Phone {
    pub fn new(serial: &str) -> Self {
        Self { serial: serial.to_string(), open: OnceCell::new(), handles: Mutex::default() }
    }

    async fn storages(&self) -> Result<&[(String, Arc<Storage>)], Error> {
        let (_, storages) = self
            .open
            .get_or_try_init(|| async {
                let device = open(&self.serial).await.map_err(error)?;
                let mut storages = Vec::new();
                for s in device.storages().await.map_err(error)? {
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
        let objects = storage.list_objects(handle).await.map_err(error)?;
        let mut handles = self.handles.lock().unwrap();
        for o in objects {
            // Hidden items are not shown, apart from EyeCanSee's trash folders.
            if o.filename.starts_with('.') && !(o.is_folder() && o.filename == source::TRASH_DIR) {
                continue;
            }
            if o.is_folder() {
                let path = format!("{dir}{}/", o.filename);
                handles.insert(path.clone(), o.handle);
                listing.dirs.push(path);
            } else if is_image(&o.filename) {
                let path = format!("{dir}{}", o.filename);
                handles.insert(path.clone(), o.handle);
                let modified = o.modified.and_then(unix_seconds).map(|s| s.to_string()).unwrap_or_default();
                listing.files.push(File { path, size: o.size, modified });
            }
        }
        listing.sort();
        Ok(listing)
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        match storage.download_to_vec(handle).await {
            // Android can renumber handles during the session: list again and retry once.
            Err(e) if e.is_stale_handle() => {
                self.handles.lock().unwrap().clear();
                let (storage, handle) = self.locate(path).await?;
                let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
                storage.download_to_vec(handle).await.map_err(error)
            }
            r => r.map_err(error),
        }
    }

    /// Deletes for good (a folder with what is inside it [Unverified on Android: tried with the trash]).
    pub async fn delete(&self, path: &str) -> Result<(), Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        storage.delete(handle).await.map_err(error)?;
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
        let objects = storage.list_objects(handle).await.map_err(error)?;
        let mut handles = self.handles.lock().unwrap();
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
                let h = storage.create_folder(parent, source::name(to_dir)).await.map_err(error)?;
                self.handles.lock().unwrap().insert(to_dir.to_string(), h);
                h
            }
        };
        if new_name != source::name(path) {
            storage.rename(handle, new_name).await.map_err(error)?;
        }
        storage.move_object(handle, folder, None).await.map_err(error)?;
        self.handles.lock().unwrap().retain(|p, _| !p.starts_with(path));
        Ok(())
    }

    /// The name must not be in the folder already: `App::rename_window` checks it against the listing.
    pub async fn rename(&self, path: &str, new_name: &str) -> Result<(), Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        storage.rename(handle, new_name).await.map_err(error)?;
        // The folder's paths below it are stale too: the cache fills again at the next listing.
        self.handles.lock().unwrap().retain(|p, _| !p.starts_with(path));
        Ok(())
    }

    /// Does the folder exist? (for cache maintenance).
    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        if dir.is_empty() { Ok(()) } else { self.locate(dir).await.map(|_| ()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
