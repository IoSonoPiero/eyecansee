//! Where the images come from: local disk and FTP through OpenDAL, SFTP servers (`sftp.rs`) and
//! phones connected directly over MTP (`mtp://<serial>/`).
//! Every source has its root at the top, so the folder tree can climb all the way up.
//! Folders are paths relative to the root ending in `/` ("" is the root).
//! Here the operations of every source (`Store`); local disks in `local`, addresses and paths in
//! `paths`, a folder's listing in `listing`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use opendal::Operator;

mod listing;
mod local;
mod paths;
pub use listing::{File, Listing};
pub(crate) use local::local_path;
use local::{bad_name, fs_gvfs, gvfs_permit, list, local_items};
pub use paths::{in_gvfs, name, open, parent, windows_temp_copy};

/// Translatable error: Fluent message id and technical detail.
pub type Error = (&'static str, String);

/// Id of the "not found" error, recognised by cache maintenance.
pub const NOT_FOUND: &str = "error-not-found";
/// A local name that is not valid UTF-8 (`bad_name`).
pub const BAD_NAME: &str = "error-bad-name";

/// Id of the error of a server that does not answer in time (connection, login, start), with
/// the server as detail (#75).
pub const SILENT: &str = "error-server-silent";

/// Chunks of a file being copied, from the reader to the writer (`Store::send_chunks`).
pub type Chunks = tokio::sync::mpsc::Sender<std::io::Result<Bytes>>;

/// Size of the chunks read from disks and servers.
const CHUNK: u64 = 1 << 20;

pub use crate::trash::{android_trashed, free_name, in_trash, is_trash, leftover, listed_in, restore_place, save_names, saved_image, shown, trash_of, Conflict, Deletion, SAVING, TRASH_DIR};

#[cfg(test)]
thread_local! {
    /// Folders read by `Store::names` on this thread: a paste must not read one per file (#90).
    pub static FOLDERS_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The operations the app needs, the same for all sources.
#[derive(Clone)]
pub enum Store {
    Dal(Operator),
    Mtp(Arc<crate::mtp::Phone>),
    Sftp(Arc<crate::sftp::Server>),
    Ftp(Arc<crate::ftp::Server>),
}

/// Files a read found gone: not there any more, or a stale MTP handle (the phone re-keyed the
/// object after a change made on the phone itself), or removed by a phone's event while the
/// viewer showed them (`mark_gone`). The app reads the folder again (`App::notice_gone`).
static GONE: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn mark_gone(path: String) {
    GONE.lock().unwrap().push(path);
}

pub fn is_gone((id, detail): &Error) -> bool {
    *id == NOT_FOUND || detail.contains("stale object handle")
}


/// The paths found gone since the last call.
pub fn take_gone() -> Vec<String> {
    std::mem::take(&mut *GONE.lock().unwrap())
}

pub(crate) fn dal_error(e: opendal::Error) -> Error {
    let id = match e.kind() {
        opendal::ErrorKind::NotFound => NOT_FOUND,
        // FTP "530 Not logged in" comes as an unexpected error.
        _ if e.to_string().contains("[530]") => "error-login",
        _ => "error-detail",
    };
    (id, e.to_string())
}

impl Store {
    /// A read's result, with a file gone noticed (`GONE`) and a local name that cannot be read
    /// (`bad_name`) said as such.
    fn noticed<T>(&self, r: Result<T, Error>, path: &str) -> Result<T, Error> {
        match r {
            Err((NOT_FOUND, _)) if matches!(self, Store::Dal(op) if bad_name(op, path)) => Err((BAD_NAME, name(path).to_string())),
            Err(e) if is_gone(&e) => {
                mark_gone(path.to_string());
                Err(e)
            }
            r => r,
        }
    }

    /// The file on this computer (gvfs mounts too), for libraries that open it themselves (libmpv).
    pub fn local_file(&self, path: &str) -> Option<PathBuf> {
        match self {
            Store::Dal(op) if op.info().scheme() == "fs" => Some(local_path(op, path)),
            _ => None,
        }
    }

    /// Where a path really leads, links resolved, to tell the same file or folder reached by two
    /// paths: on this computer and on SFTP servers; `None` where it is not known or not needed.
    /// Written as the paths of the source (without the final "/"), so a path that goes through
    /// no link is its own real path.
    pub async fn real_path(&self, path: &str) -> Option<String> {
        match self {
            Store::Dal(op) if op.info().scheme() == "fs" => {
                let (root, real) = (Path::new(&op.info().root()).canonicalize().ok()?, local_path(op, path).canonicalize().ok()?);
                let real = real.strip_prefix(&root).unwrap_or(&real);
                Some(real.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join("/"))
            }
            Store::Sftp(server) => server.real_path(path).await.ok().map(|p| p.trim_start_matches('/').to_string()),
            _ => None,
        }
    }

    /// Images and folders of `dir`, after tidying up what an interrupted "Save" left there.
    pub async fn list(&self, dir: &str) -> Result<Listing, Error> {
        if is_trash(dir) {
            // Android's trashed files of the folder show up in its trash, with the trash folder's
            // own contents (if that folder exists at all).
            let trashed = self.list_once(&parent(dir)).await?.trashed;
            let mut listing = match self.list_once(dir).await {
                Ok(listing) => listing,
                Err(e) if trashed.is_empty() => return Err(e),
                Err(_) => Listing::default(),
            };
            listing.files.extend(trashed);
            listing.sort();
            return Ok(listing);
        }
        let mut listing = self.list_once(dir).await?;
        if !listing.leftovers.is_empty() {
            self.recover(dir, &listing.leftovers).await;
            listing = self.list_once(dir).await?;
        }
        if !listing.trashed.is_empty() {
            listing.trash.get_or_insert_with(|| format!("{dir}{TRASH_DIR}/"));
        } else if let Some(trash) = &listing.trash
            && self.names(trash).await.is_ok_and(|n| n.is_empty())
        {
            listing.trash = None; // a trash folder left empty: no strip for it
        }
        Ok(listing)
    }

    async fn list_once(&self, dir: &str) -> Result<Listing, Error> {
        match self {
            Store::Dal(op) => list(op, dir).await,
            Store::Mtp(phone) => phone.list(dir).await,
            Store::Sftp(server) => server.list(dir).await,
            Store::Ftp(server) => server.list(dir).await,
        }
    }

    /// A path under a gvfs mount: a phone, camera or network share mounted by the desktop.
    pub fn gvfs(&self, path: &str) -> bool {
        matches!(self, Store::Dal(op) if fs_gvfs(op, path))
    }

    /// A local file that is a temporary copy made by Windows (`windows_temp_copy`).
    pub fn temp_copy(&self, path: &str) -> bool {
        matches!(self, Store::Dal(op) if op.info().scheme() == "fs" && windows_temp_copy(&format!("{}/{path}", op.info().root())))
    }

    /// Can it rename files? OpenDAL's FTP cannot, and "Save" needs it.
    pub fn can_rename(&self) -> bool {
        match self {
            Store::Dal(op) => op.info().capability().rename,
            Store::Mtp(_) | Store::Sftp(_) | Store::Ftp(_) => true,
        }
    }

    /// A second name `to` (free) for a local file: a hard link, or a copy where the disk has none
    /// (FAT, exFAT).
    pub async fn link_or_copy(&self, from: &str, to: &str) -> Result<(), Error> {
        let Store::Dal(op) = self else { return Err(("error-detail", from.to_string())) };
        let (from, to) = (local_path(op, from), local_path(op, to));
        tokio::task::spawn_blocking(move || {
            std::fs::hard_link(&from, &to).or_else(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => Err(e),
                _ => std::fs::copy(&from, &to).map(|_| ()),
            })
        })
        .await
        .map_err(|e| ("error-detail", e.to_string()))?
        .map_err(|e| ("error-detail", e.to_string()))
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let r = match self {
            Store::Dal(op) => {
                let _permit = gvfs_permit(op, path).await;
                op.read(path).await.map(|b| b.to_vec()).map_err(dal_error)
            }
            Store::Mtp(phone) => phone.read(path).await,
            Store::Sftp(server) => server.read(path).await,
            Store::Ftp(server) => server.read(path).await,
        };
        self.noticed(r, path)
    }

    /// `len` bytes from `offset`: videos are read a piece at a time (`mpv::Reader`).
    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let r = match self {
            Store::Dal(op) => {
                let _permit = gvfs_permit(op, path).await;
                op.read_with(path).range(offset..offset + len).await.map(|b| b.to_vec()).map_err(dal_error)
            }
            Store::Mtp(phone) => phone.read_range(path, offset, len as u32).await,
            Store::Sftp(server) => server.read_range(path, offset, len).await,
            Store::Ftp(server) => server.read_range(path, offset, len).await,
        };
        self.noticed(r, path)
    }

    /// Names of everything in a folder, of any type, hidden ones too (folders without the "/").
    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        #[cfg(test)]
        FOLDERS_READ.with(|n| n.set(n.get() + 1));
        match self {
            Store::Dal(op) => Ok(local_items(op, dir).await?.into_iter().map(|i| i.1).collect()),
            Store::Mtp(phone) => phone.names(dir).await,
            Store::Sftp(server) => server.names(dir).await,
            Store::Ftp(server) => server.names(dir).await,
        }
    }

    /// Moves a file or folder (path ending in "/") into `to_dir`, created if missing, with the
    /// name `new_name`; returns the new path. The name must be free (`free_name`), except on this
    /// computer, where a file there is replaced (`local::rename`).
    pub async fn move_into(&self, path: &str, to_dir: &str, new_name: &str) -> Result<String, Error> {
        let slash = if path.ends_with('/') { "/" } else { "" };
        let to = format!("{to_dir}{new_name}{slash}");
        match self {
            // Local and gvfs: by hand, folders too (OpenDAL renames files only).
            Store::Dal(op) if op.info().scheme() == "fs" => {
                let (from, dir, dest) = (local_path(op, path), local_path(op, to_dir), local_path(op, &to));
                tokio::task::spawn_blocking(move || {
                    std::fs::create_dir_all(&dir)?;
                    local::rename(&from, &dest)
                })
                .await
                .map_err(|e| ("error-detail", e.to_string()))?
                .map_err(|e| ("error-detail", e.to_string()))?;
            }
            Store::Dal(_) if path.ends_with('/') => return Err(("error-no-trash-folder", path.to_string())),
            Store::Dal(op) => {
                let unsupported = |e: opendal::Error| match e.kind() {
                    opendal::ErrorKind::Unsupported => ("error-no-trash-folder", e.to_string()),
                    _ => dal_error(e),
                };
                op.create_dir(to_dir).await.map_err(unsupported)?;
                op.rename(path, &to).await.map_err(unsupported)?;
            }
            Store::Mtp(phone) => phone.move_into(path, to_dir, new_name).await?,
            // On the server itself, folders too.
            Store::Sftp(server) => {
                server.create_dir_all(to_dir).await?;
                server.rename(path, &to).await?;
            }
            Store::Ftp(server) => {
                server.create_dir_all(to_dir).await?;
                server.rename(path, &to).await?;
            }
        }
        Ok(to)
    }

    /// Paths of everything in a folder, of any type (folders ending in "/").
    pub async fn children(&self, dir: &str) -> Result<Vec<String>, Error> {
        let mut paths = Vec::new();
        if is_trash(dir) {
            paths.extend(self.list_once(&parent(dir)).await?.trashed.into_iter().map(|f| f.path));
        }
        let names = match self.names(dir).await {
            Ok(names) => names,
            Err(e) if paths.is_empty() => return Err(e),
            Err(_) => HashSet::new(), // only Android's trash, no folder of ours
        };
        for n in names {
            let slash = if self.is_dir(&format!("{dir}{n}/")).await { "/" } else { "" };
            paths.push(format!("{dir}{n}{slash}"));
        }
        Ok(paths)
    }

    pub(crate) async fn is_dir(&self, dir: &str) -> bool {
        match self {
            Store::Dal(op) => op.stat(dir).await.is_ok_and(|m| m.is_dir()),
            Store::Mtp(phone) => phone.check_dir(dir).await.is_ok(),
            Store::Sftp(server) => server.is_dir(dir).await,
            Store::Ftp(server) => server.is_dir(dir).await,
        }
    }

    /// Renames an image or a folder (path ending in "/") in its folder; returns the new path.
    /// Never over an existing item.
    pub async fn rename(&self, path: &str, new_name: &str) -> Result<String, Error> {
        let slash = if path.ends_with('/') { "/" } else { "" };
        let to = format!("{}{new_name}{slash}", parent(path));
        match self {
            // Local and gvfs: by hand, folders too (OpenDAL renames files only).
            Store::Dal(op) if op.info().scheme() == "fs" => {
                let (from, dest) = (local_path(op, path), local_path(op, &to));
                tokio::task::spawn_blocking(move || {
                    if dest.symlink_metadata().is_ok() {
                        return Err(("error-exists", dest.display().to_string()));
                    }
                    std::fs::rename(&from, &dest).map_err(|e| ("error-detail", e.to_string()))
                })
                .await
                .map_err(|e| ("error-detail", e.to_string()))??;
            }
            Store::Dal(_) if path.ends_with('/') => return Err(("error-rename-unsupported", path.to_string())),
            Store::Dal(op) => {
                if op.exists(&to).await.unwrap_or(false) {
                    return Err(("error-exists", to));
                }
                op.rename(path, &to).await.map_err(|e| match e.kind() {
                    opendal::ErrorKind::Unsupported => ("error-rename-unsupported", e.to_string()),
                    _ => dal_error(e),
                })?;
            }
            Store::Mtp(phone) => phone.rename(path, new_name).await?,
            Store::Sftp(server) => {
                if server.exists(&to).await {
                    return Err(("error-exists", to));
                }
                server.rename(path, &to).await?;
            }
            Store::Ftp(server) => {
                if server.exists(&to).await {
                    return Err(("error-exists", to));
                }
                server.rename(path, &to).await?;
            }
        }
        Ok(to)
    }

    pub async fn size(&self, path: &str) -> Result<u64, Error> {
        let r = match self {
            Store::Dal(op) => op.stat(path).await.map(|m| m.content_length()).map_err(dal_error),
            Store::Mtp(phone) => phone.size(path).await,
            Store::Sftp(server) => server.size(path).await,
            Store::Ftp(server) => server.size(path).await,
        };
        self.noticed(r, path)
    }

    /// Everything below a folder, hidden items and EyeCanSee's trash folders too (a copy leaves
    /// them out, a move takes them along: `transfer::place`), folders before what they contain:
    /// path and size (folders end in "/", size 0).
    /// `counted` grows as items are found, for the progress window.
    pub async fn walk(&self, dir: &str, p: &crate::transfer::Progress) -> Result<Vec<(String, u64)>, Error> {
        match self {
            Store::Dal(op) => {
                *p.current.lock().unwrap() = dir.to_string();
                let (mut found, mut todo, mut seen) = (Vec::new(), vec![dir.to_string()], HashSet::new());
                while let Some(dir) = todo.pop() {
                    // Links to folders are followed, but a folder already walked (a link to a
                    // folder above) is not walked again.
                    if !seen.insert(local_path(op, &dir).canonicalize().unwrap_or_default()) {
                        continue;
                    }
                    for (path, _, is_dir, size, _) in local_items(op, &dir).await? {
                        if p.cancel.load(Ordering::Relaxed) {
                            return Err(("error-cancelled", String::new()));
                        }
                        if is_dir {
                            todo.push(path.clone());
                        }
                        found.push((path, size));
                        p.found.fetch_add(1, Ordering::Relaxed);
                    }
                }
                // A folder before its contents, whatever order the service lists them in.
                found.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(found)
            }
            Store::Mtp(phone) => phone.walk(dir, p).await,
            Store::Sftp(server) => server.walk(dir, p).await,
            Store::Ftp(server) => server.walk(dir, p).await,
        }
    }

    /// Creates a folder whose parent exists; nothing if it is already there.
    pub async fn create_dir(&self, dir: &str) -> Result<(), Error> {
        match self {
            Store::Dal(op) => {
                let _permit = gvfs_permit(op, dir).await;
                op.create_dir(dir).await.map_err(dal_error)
            }
            Store::Mtp(phone) => phone.create_dir(dir).await,
            Store::Sftp(server) => server.create_dir(dir).await,
            Store::Ftp(server) => server.create_dir(dir).await,
        }
    }

    /// Reads a file of `size` bytes in chunks into `out`, adding them to `done`; stops with
    /// `error-cancelled` when `cancel` is set. An error also goes into `out`, so the writer
    /// knows the file is incomplete.
    pub async fn send_chunks(&self, path: &str, size: u64, out: Chunks, done: &AtomicU64, cancel: &AtomicBool) -> Result<(), Error> {
        // Also what a phone on Windows answers after a stale handle (the whole copy of a video).
        let r = self.noticed(self.send(path, size, &out, done, cancel).await, path);
        if let Err((_, detail)) = &r {
            let _ = out.send(Err(std::io::Error::other(detail.clone()))).await;
        }
        r
    }

    async fn send(&self, path: &str, size: u64, out: &Chunks, done: &AtomicU64, cancel: &AtomicBool) -> Result<(), Error> {
        let push = |chunk: Bytes| async move {
            if cancel.load(Ordering::Relaxed) {
                return Err(("error-cancelled", String::new()));
            }
            done.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            out.send(Ok(chunk)).await.map_err(|_| ("error-cancelled", String::new()))
        };
        match self {
            Store::Dal(op) => {
                let _permit = gvfs_permit(op, path).await;
                let reader = op.reader(path).await.map_err(dal_error)?;
                let mut at = 0;
                while at < size {
                    let chunk = reader.read(at..(at + CHUNK).min(size)).await.map_err(dal_error)?.to_bytes();
                    if chunk.is_empty() {
                        return Err(("error-detail", format!("{path}: file shorter than expected")));
                    }
                    at += chunk.len() as u64;
                    push(chunk).await?;
                }
            }
            Store::Sftp(server) => server.send(path, 0, size, CHUNK, cancel, push).await?,
            Store::Ftp(server) => server.send(path, 0, size, CHUNK, cancel, push).await?,
            Store::Mtp(phone) => {
                let mut download = phone.download(path).await?;
                while let Some(window) = phone.next_window(&mut download).await {
                    push(Bytes::from(window?)).await?;
                }
            }
        }
        Ok(())
    }

    /// Writes a new file of `size` bytes from the chunks of `send_chunks`. At any failure (an error
    /// in the chunks, a write or the closing that fails, for example for lack of space) what was
    /// written is deleted: `path` must be a name this write owns, a file already there is replaced
    /// only through `replace::write_file` (issue #90).
    pub async fn write_chunks(&self, path: &str, size: u64, chunks: tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Error> {
        match self {
            Store::Dal(op) => write_dal(op, path, chunks).await,
            Store::Sftp(server) => server.write_chunks(path, chunks).await,
            Store::Ftp(server) => server.write_chunks(path, chunks).await,
            Store::Mtp(phone) => {
                let stream = futures_util::stream::unfold(chunks, |mut c| async { c.recv().await.map(|x| (x, c)) });
                phone.upload(path, size, Box::pin(stream)).await
            }
        }
    }

    /// Is there an item named `path`? An answer that does not come counts as no.
    pub async fn exists(&self, path: &str) -> bool {
        match self {
            Store::Dal(op) => op.exists(path).await.unwrap_or(false),
            Store::Mtp(phone) => phone.exists(path).await,
            Store::Sftp(server) => server.exists(path).await,
            Store::Ftp(server) => server.exists(path).await,
        }
    }

    /// Does the folder exist? OpenDAL (at least `fs`) lists a non-existent folder as empty.
    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        match self {
            Store::Dal(_) if dir.is_empty() => Ok(()),
            Store::Dal(op) => op.stat(dir).await.map(|_| ()).map_err(dal_error),
            Store::Mtp(phone) => phone.check_dir(dir).await,
            Store::Sftp(server) => server.check_dir(dir).await,
            Store::Ftp(server) => server.check_dir(dir).await,
        }
    }
}

/// `Store::write_chunks` on this computer. The writer is closed before the file is deleted
/// (Windows does not delete an open file). The error of a full disk often comes only when the file
/// is closed: tokio writes in the background, the closing flushes (#90).
async fn write_dal(op: &Operator, path: &str, mut chunks: tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Error> {
    let _permit = gvfs_permit(op, path).await;
    let r = async {
        let mut writer = op.writer(path).await.map_err(dal_error)?;
        while let Some(chunk) = chunks.recv().await {
            let written = match chunk {
                Ok(c) => writer.write(c).await.map_err(dal_error),
                Err(e) => Err(("error-detail", e.to_string())),
            };
            if let Err(e) = written {
                let _ = writer.abort().await;
                return Err(e);
            }
        }
        writer.close().await.map(|_| ()).map_err(dal_error)
    }
    .await;
    if let Err(e) = &r {
        let gone = op.delete(path).await;
        log!("write {path:?} failed ({e:?}), what was written deleted: {gone:?}");
    }
    r
}

pub struct Source {
    pub op: Store,
    /// Prefix shown in the address bar: "/", "C:\", "ftp://user@host/".
    pub base: String,
    /// Initial folder.
    pub dir: String,
    /// Image to open straight away, if the address pointed to a file.
    pub start: Option<String>,
    /// A server address without a folder: it opens in the folder the server starts in
    /// (`Loader::home`), which is known only once connected.
    pub home: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendal::services;

    /// The tests that take what `GONE` holds, one at a time.
    static GONE_TESTS: Mutex<()> = Mutex::new(());

    /// A read of a file that is not there any more is noticed, so the folder gets read again.
    #[test]
    fn gone_noticed() {
        let _one = GONE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        assert!(is_gone(&(NOT_FOUND, "a.jpg".into())));
        assert!(is_gone(&("error-detail", "stale object handle (device re-keyed it; re-list and retry)".into())));
        assert!(!is_gone(&("error-detail", "timeout".into())));
        let dir = std::env::temp_dir().join(format!("eyecansee-gone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        let op = Store::Dal(Operator::new(services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _ = take_gone();
        assert!(rt.block_on(op.read("a.jpg")).is_ok());
        assert_eq!(rt.block_on(op.read("b.jpg")).unwrap_err().0, NOT_FOUND);
        assert!(rt.block_on(op.read_range("b.jpg", 0, 1)).is_err());
        assert!(rt.block_on(op.size("b.jpg")).is_err());
        assert_eq!(take_gone(), ["b.jpg", "b.jpg", "b.jpg"]);
        assert!(take_gone().is_empty(), "taken once");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A name that is not valid UTF-8 is listed with U+FFFD; reading it says the name cannot be
    /// read, not that the file is gone (issue #88). A folder with such a name says the same.
    #[cfg(unix)]
    #[test]
    fn name_not_utf8() {
        use std::os::unix::ffi::OsStrExt;
        let _one = GONE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("eyecansee-not-utf8-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(std::ffi::OsStr::from_bytes(b"dir\xff"))).unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        std::fs::write(dir.join(std::ffi::OsStr::from_bytes(b"bad\xff\xfe.jpg")), b"x").unwrap();
        let op = Store::Dal(Operator::new(services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let listing = rt.block_on(op.list("")).unwrap();
        let files: Vec<_> = listing.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["a.jpg", "bad\u{FFFD}\u{FFFD}.jpg"], "still listed, marked");
        assert_eq!(listing.dirs, ["dir\u{FFFD}/"]);
        let _ = take_gone();
        let bad = (BAD_NAME, "bad\u{FFFD}\u{FFFD}.jpg".to_string());
        assert_eq!(rt.block_on(op.read(files[1])).unwrap_err(), bad);
        assert_eq!(rt.block_on(op.read_range(files[1], 0, 1)).unwrap_err(), bad);
        assert_eq!(rt.block_on(op.size(files[1])).unwrap_err(), bad);
        assert!(!take_gone().iter().any(|p| p.contains(char::REPLACEMENT_CHARACTER)), "not taken for a file gone");
        assert_eq!(rt.block_on(op.list("dir\u{FFFD}/")).err(), Some((BAD_NAME, "dir\u{FFFD}".to_string())));
        let arg = dir.join(std::ffi::OsStr::from_bytes(b"bad\xff\xfe.jpg"));
        assert_eq!(open(&arg.to_string_lossy()).err().map(|e| e.0), Some(BAD_NAME), "the same name given to the app");
        assert!(rt.block_on(op.read("nothing.jpg")).is_err());
        assert!(take_gone().contains(&"nothing.jpg".to_string()), "a file really gone is still noticed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Trash folder, permanent deletion and renaming, on a temporary folder.
    #[test]
    fn delete_and_rename() {
        let dir = std::env::temp_dir().join(format!("eyecansee-delete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("photos")).unwrap();
        let op = Store::Dal(Operator::new(services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let trash = dir.join("photos").join(TRASH_DIR);
        for _ in 0..2 {
            std::fs::write(dir.join("photos/a.jpg"), b"x").unwrap();
            rt.block_on(op.delete("photos/a.jpg", Deletion::TrashFolder)).unwrap();
        }
        // The second one with the same name gets a prefix instead of replacing the first.
        assert!(!dir.join("photos/a.jpg").exists());
        assert!(trash.join("a.jpg").exists());
        assert_eq!(std::fs::read_dir(&trash).unwrap().count(), 2);
        std::fs::write(dir.join("photos/b.jpg"), b"x").unwrap();
        rt.block_on(op.delete("photos/b.jpg", Deletion::Permanent)).unwrap();
        assert!(!dir.join("photos/b.jpg").exists());
        assert_eq!(std::fs::read_dir(&trash).unwrap().count(), 2);
        // Folders: renamed, never over an existing name, and deleted with what is inside.
        std::fs::create_dir_all(dir.join("photos/sub")).unwrap();
        std::fs::write(dir.join("photos/sub/c.jpg"), b"x").unwrap();
        assert_eq!(rt.block_on(op.rename("photos/sub/", "renamed")).unwrap(), "photos/renamed/");
        assert!(dir.join("photos/renamed/c.jpg").exists());
        std::fs::write(dir.join("photos/d.jpg"), b"x").unwrap();
        assert_eq!(rt.block_on(op.rename("photos/d.jpg", "renamed")).unwrap_err().0, "error-exists");
        rt.block_on(op.delete("photos/renamed/", Deletion::Permanent)).unwrap();
        assert!(!dir.join("photos/renamed").exists());

        // Back from the trash: a taken name is asked about, then kept both or replaced.
        assert!(trash.join("a (2).jpg").exists());
        std::fs::write(dir.join("photos/a.jpg"), b"new").unwrap();
        let t = format!("photos/{TRASH_DIR}/");
        assert_eq!(rt.block_on(op.restore(&format!("{t}a.jpg"), Conflict::Ask)).unwrap_err().0, "error-exists");
        assert_eq!(rt.block_on(op.restore(&format!("{t}a.jpg"), Conflict::KeepBoth)).unwrap(), "photos/a (3).jpg");
        assert_eq!(rt.block_on(op.restore(&format!("{t}a (2).jpg"), Conflict::Ask)).unwrap(), "photos/a (2).jpg");
        // Folders merge: what is only in the trash moves, the same name is kept twice.
        std::fs::create_dir_all(dir.join("photos/f/inner")).unwrap();
        std::fs::write(dir.join("photos/f/x.jpg"), b"1").unwrap();
        rt.block_on(op.delete("photos/f/", Deletion::TrashFolder)).unwrap();
        std::fs::create_dir_all(dir.join("photos/f")).unwrap();
        std::fs::write(dir.join("photos/f/x.jpg"), b"2").unwrap();
        assert_eq!(rt.block_on(op.restore(&format!("{t}f/"), Conflict::Merge)).unwrap(), "photos/f/");
        assert!(dir.join("photos/f/inner").is_dir() && dir.join("photos/f/x (2).jpg").exists());
        assert!(!trash.join("f").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

