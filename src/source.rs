//! Where the images come from: local disk and FTP through OpenDAL, SFTP servers (`sftp.rs`) and
//! phones connected directly over MTP (`mtp://<serial>/`).
//! Every source has its root at the top, so the folder tree can climb all the way up.
//! Folders are paths relative to the root ending in `/` ("" is the root).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use opendal::{Operator, services};
use percent_encoding::percent_decode_str;
use url::Url;

use crate::decode::is_media;
use crate::settings::SortBy;

/// Translatable error: Fluent message id and technical detail.
pub type Error = (&'static str, String);

/// Id of the "not found" error, recognised by cache maintenance.
pub const NOT_FOUND: &str = "error-not-found";

/// Chunks of a file being copied, from the reader to the writer (`Store::send_chunks`).
pub type Chunks = tokio::sync::mpsc::Sender<std::io::Result<Bytes>>;

/// Size of the chunks read from disks and servers.
const CHUNK: u64 = 1 << 20;

pub use crate::trash::{android_trashed, free_name, in_trash, is_trash, leftover, save_names, shown, trash_of, Conflict, Deletion, SAVING, TRASH_DIR};

/// The operations the app needs, the same for all sources.
#[derive(Clone)]
pub enum Store {
    Dal(Operator),
    Mtp(Arc<crate::mtp::Phone>),
    Sftp(Arc<crate::sftp::Server>),
}

/// Reads under gvfs (phones, cameras, shares mounted by the desktop), two at a time like the
/// workers of `Store::Mtp`. With too many requests at once gvfs stalls: 12 file opens while a
/// 796-entry folder was being listed gave 12 files out of 30 in 31 s and the listing never ended;
/// with 8 or fewer everything took 3-6 s (script, 2026-10-20; gvfsd-fuse has 10 worker threads
/// [Inference: the limit comes from there]). More parallel reads are not faster: gvfsd-mtp serves
/// one request at a time and in a large folder every file costs ~1.5 s (FUSE_LOOKUP + FUSE_OPEN,
/// kernel trace). Listings stay out, otherwise the tree waits for the thumbnails (Pictures after 14 s).
static GVFS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

async fn gvfs_permit(op: &Operator, path: &str) -> Option<tokio::sync::SemaphorePermit<'static>> {
    if op.info().scheme() == "fs" && in_gvfs(&op.info().root(), path) { GVFS.acquire().await.ok() } else { None }
}

pub fn in_gvfs(root: &str, path: &str) -> bool {
    format!("{root}/{path}").contains("/gvfs/")
}

/// Absolute path of a local file, pushed one element at a time: Windows' "\\?\" root does not accept "/".
pub(crate) fn local_path(op: &Operator, path: &str) -> PathBuf {
    std::iter::once(op.info().root()).chain(path.split('/').filter(|s| !s.is_empty()).map(String::from)).collect()
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

fn noticed<T>(r: Result<T, Error>, path: &str) -> Result<T, Error> {
    if let Err(e) = &r
        && is_gone(e)
    {
        mark_gone(path.to_string());
    }
    r
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
    /// The file on this computer (gvfs mounts too), for libraries that open it themselves (libmpv).
    pub fn local_file(&self, path: &str) -> Option<PathBuf> {
        match self {
            Store::Dal(op) if op.info().scheme() == "fs" => Some(local_path(op, path)),
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
            Store::Dal(op) => list(op, dir).await.map_err(dal_error),
            Store::Mtp(phone) => phone.list(dir).await,
            Store::Sftp(server) => server.list(dir).await,
        }
    }

    /// Can it rename files? OpenDAL's FTP cannot, and "Save" needs it.
    pub fn can_rename(&self) -> bool {
        match self {
            Store::Dal(op) => op.info().capability().rename,
            Store::Mtp(_) | Store::Sftp(_) => true,
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
        };
        noticed(r, path)
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
        };
        noticed(r, path)
    }

    /// Names of everything in a folder, of any type, hidden ones too (folders without the "/").
    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        match self {
            Store::Dal(op) => {
                let entries = op.list(if dir.is_empty() { "/" } else { dir }).await.map_err(dal_error)?;
                Ok(entries
                    .iter()
                    .filter(|e| e.path().trim_start_matches('/') != dir)
                    .map(|e| e.name().trim_end_matches('/').to_string())
                    .collect())
            }
            Store::Mtp(phone) => phone.names(dir).await,
            Store::Sftp(server) => server.names(dir).await,
        }
    }

    /// Moves a file or folder (path ending in "/") into `to_dir`, created if missing, with the
    /// name `new_name`; returns the new path. The name must be free (`free_name`).
    pub async fn move_into(&self, path: &str, to_dir: &str, new_name: &str) -> Result<String, Error> {
        let slash = if path.ends_with('/') { "/" } else { "" };
        let to = format!("{to_dir}{new_name}{slash}");
        match self {
            // Local and gvfs: by hand, folders too (OpenDAL renames files only).
            Store::Dal(op) if op.info().scheme() == "fs" => {
                let (from, dir, dest) = (local_path(op, path), local_path(op, to_dir), local_path(op, &to));
                tokio::task::spawn_blocking(move || {
                    std::fs::create_dir_all(&dir)?;
                    std::fs::rename(&from, &dest)
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
        }
        Ok(to)
    }

    pub async fn size(&self, path: &str) -> Result<u64, Error> {
        let r = match self {
            Store::Dal(op) => op.stat(path).await.map(|m| m.content_length()).map_err(dal_error),
            Store::Mtp(phone) => phone.size(path).await,
            Store::Sftp(server) => server.size(path).await,
        };
        noticed(r, path)
    }

    /// Everything below a folder, hidden items too, folders before what they contain: path and
    /// size (folders end in "/", size 0). EyeCanSee's trash folders stay out.
    /// `counted` grows as items are found, for the progress window.
    pub async fn walk(&self, dir: &str, p: &crate::transfer::Progress) -> Result<Vec<(String, u64)>, Error> {
        match self {
            Store::Dal(op) => {
                *p.current.lock().unwrap() = dir.to_string();
                let entries = op.list_with(dir).recursive(true).await.map_err(dal_error)?;
                let mut found = Vec::new();
                for e in entries {
                    if p.cancel.load(Ordering::Relaxed) {
                        return Err(("error-cancelled", String::new()));
                    }
                    let path = e.path().trim_start_matches('/').to_string();
                    if path == dir || path.split('/').any(|p| p == TRASH_DIR) {
                        continue;
                    }
                    // The size is not always in the listing (local files): asked one by one.
                    let size = if path.ends_with('/') { 0 } else { self.size(&path).await? };
                    found.push((path, size));
                    p.found.fetch_add(1, Ordering::Relaxed);
                }
                // A folder before its contents, whatever order the service lists them in.
                found.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(found)
            }
            Store::Mtp(phone) => phone.walk(dir, p).await,
            Store::Sftp(server) => server.walk(dir, p).await,
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
        }
    }

    /// Reads a file of `size` bytes in chunks into `out`, adding them to `done`; stops with
    /// `error-cancelled` when `cancel` is set. An error also goes into `out`, so the writer
    /// knows the file is incomplete.
    pub async fn send_chunks(&self, path: &str, size: u64, out: Chunks, done: &AtomicU64, cancel: &AtomicBool) -> Result<(), Error> {
        // Also what a phone on Windows answers after a stale handle (the whole copy of a video).
        let r = noticed(self.send(path, size, &out, done, cancel).await, path);
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
            Store::Sftp(server) => server.send(path, size, CHUNK, cancel, push).await?,
            Store::Mtp(phone) => {
                let mut download = phone.download(path).await?;
                while let Some(window) = phone.next_window(&mut download).await {
                    push(Bytes::from(window?)).await?;
                }
            }
        }
        Ok(())
    }

    /// Writes a file of `size` bytes from the chunks of `send_chunks`, replacing one with the same
    /// name; deleted if the chunks end with an error.
    pub async fn write_chunks(&self, path: &str, size: u64, chunks: tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Error> {
        let detail = |e: std::io::Error| ("error-detail", e.to_string());
        match self {
            Store::Dal(op) => {
                let _permit = gvfs_permit(op, path).await;
                let mut writer = op.writer(path).await.map_err(dal_error)?;
                let mut chunks = chunks;
                let mut failed = None;
                while let Some(chunk) = chunks.recv().await {
                    match chunk {
                        Ok(c) => {
                            if let Err(e) = writer.write(c).await {
                                failed = Some(dal_error(e));
                                break;
                            }
                        }
                        Err(e) => {
                            failed = Some(detail(e));
                            break;
                        }
                    }
                }
                match failed {
                    None => writer.close().await.map(|_| ()).map_err(dal_error),
                    Some(e) => {
                        let _ = writer.abort().await;
                        let _ = op.delete(path).await;
                        Err(e)
                    }
                }
            }
            Store::Sftp(server) => server.write_chunks(path, chunks).await,
            Store::Mtp(phone) => {
                let stream = futures_util::stream::unfold(chunks, |mut c| async { c.recv().await.map(|x| (x, c)) });
                phone.upload(path, size, Box::pin(stream)).await
            }
        }
    }

    /// Does the folder exist? OpenDAL (at least `fs`) lists a non-existent folder as empty.
    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        match self {
            Store::Dal(_) if dir.is_empty() => Ok(()),
            Store::Dal(op) => op.stat(dir).await.map(|_| ()).map_err(dal_error),
            Store::Mtp(phone) => phone.check_dir(dir).await,
            Store::Sftp(server) => server.check_dir(dir).await,
        }
    }
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

#[derive(Default, Clone)]
pub struct Listing {
    pub dirs: Vec<String>,
    pub files: Vec<File>,
    /// The trash folder in this folder, if any: kept out of `dirs`, it opens from its own button
    /// in the toolbar (among the folders it was in the way).
    pub trash: Option<String>,
    /// Hidden files of an interrupted "Save" (`save_names`), for `Store::recover`.
    pub leftovers: Vec<String>,
    /// Files in Android's trash (`android_trashed`): shown in the folder's trash, not here.
    pub trashed: Vec<File>,
}

#[derive(Clone)]
pub struct File {
    pub path: String,
    /// Size and modification date: they identify the file's version for the thumbnail cache.
    pub size: u64,
    /// As the source gives it ("" if it does not): only used for comparison.
    pub modified: String,
}

/// Splits a relative path into (folder, file) if the last element is an image or a video.
fn split(path: &str) -> (String, Option<String>) {
    let path = path.trim_matches('/');
    let dir = |d: &str| if d.is_empty() { String::new() } else { format!("{d}/") };
    match path.rsplit_once('/').unwrap_or(("", path)) {
        (d, name) if is_media(name) => (dir(d), Some(path.to_string())),
        _ => (dir(path), None),
    }
}

fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// Last element of a relative path, for labels and titles.
pub fn name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

/// Folder containing `dir` ("" if `dir` is already the root).
pub fn parent(dir: &str) -> String {
    match dir.trim_end_matches('/').rsplit_once('/') {
        Some((p, _)) => format!("{p}/"),
        None => String::new(),
    }
}

/// Accepts a local path or `file://`, `ftp://`, `ftps://`, `sftp://`, `mtp://`.
pub fn open(arg: &str) -> Result<Source, Error> {
    // "C:\photos" would be read as a URL with scheme "c": one-letter schemes are paths.
    let url = Url::parse(arg).ok().filter(|u| u.scheme().len() > 1);
    let Some(url) = url else { return local(Path::new(arg)) };
    if url.scheme() == "file" {
        let path = url.to_file_path().map_err(|_| ("error-path", arg.to_string()))?;
        return local(&path);
    }

    let host = url.host_str().ok_or(("error-no-host", arg.to_string()))?;
    let (dir, start) = split(&decode(url.path()));
    if url.scheme() == "mtp" {
        let serial = decode(host);
        let base = format!("mtp://{serial}/");
        let op = Store::Mtp(crate::mtp::Phone::shared(&serial));
        return Ok(Source { op, base, dir, start, home: false });
    }
    let user = match url.username() {
        "" => String::new(),
        u => format!("{}@", decode(u)),
    };
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    // The password never appears in the address bar.
    let base = format!("{}://{user}{host}{port}/", url.scheme());
    if url.scheme() == "sftp" {
        let password = url.password().map(decode);
        let target = crate::sftp::Target { user: decode(url.username()), host: host.to_string(), port: url.port(), password };
        let home = dir.is_empty() && start.is_none();
        return Ok(Source { op: Store::Sftp(crate::sftp::Server::shared(target)), base, dir, start, home });
    }
    let op = match url.scheme() {
        scheme @ ("ftp" | "ftps") => {
            let port = url.port_or_known_default().unwrap_or(21);
            let mut b = services::Ftp::default().endpoint(&format!("{scheme}://{host}:{port}")).root("/");
            // Without a user OpenDAL does not log in at all; FTP clients log in as "anonymous".
            b = match url.username() {
                "" => b.user("anonymous").password("anonymous@"),
                u => b.user(&decode(u)),
            };
            if let Some(p) = url.password() {
                b = b.password(&decode(p));
            }
            Operator::new(b)
        }
        s => return Err(("error-unsupported", s.to_string())),
    };
    Ok(Source { op: Store::Dal(op.map_err(|e| ("error-source", e.to_string()))?), base, dir, start, home: false })
}

fn local(path: &Path) -> Result<Source, Error> {
    let path = path.canonicalize().map_err(|e| ("error-path", format!("{}: {e}", path.display())))?;
    // "/" on Linux, "\\?\C:\" on Windows.
    let root = path.ancestors().last().unwrap_or(&path);
    let rel = path.strip_prefix(root).unwrap_or(&path);
    let rel: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let (dir, start) = split(&rel.join("/"));
    let op = Operator::new(services::Fs::default().root(&root.to_string_lossy()))
        .map_err(|e| ("error-source", e.to_string()))?;
    // ponytail: Windows' "\\?\" prefix is hidden only in the bar; it is really needed only for paths over 260 characters
    let base = root.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    Ok(Source { op: Store::Dal(op), base, dir, start, home: false })
}

impl Listing {
    /// One item of a folder being read, put where it belongs: the hidden files of an interrupted
    /// "Save" in `leftovers`, Android's trashed files in `trashed`, then folders and images. Hidden
    /// items are not shown, apart from EyeCanSee's trash folders; `shown` false hides it too
    /// (Windows' hidden attribute, a file being written). Folder paths end in "/".
    pub(crate) fn add(&mut self, path: String, name: &str, is_dir: bool, size: u64, modified: String, shown: bool) {
        if !is_dir && leftover(name) {
            self.leftovers.push(path.clone());
        }
        if !is_dir && android_trashed(name).is_some() {
            self.trashed.push(File { path, size, modified });
            return;
        }
        if !shown || (name.starts_with('.') && !(is_dir && name == TRASH_DIR)) {
            return;
        }
        if is_dir {
            self.dirs.push(path);
        } else if is_media(name) {
            self.files.push(File { path, size, modified });
        }
    }

    // ponytail: plain alphabetical order, "img10" before "img2"; natural sort if it bothers anyone
    /// Also takes the trash folder out of `dirs`, into `trash`.
    pub fn sort(&mut self) {
        if let Some(i) = self.dirs.iter().position(|d| is_trash(d)) {
            self.trash = Some(self.dirs.remove(i));
        }
        self.dirs.sort_by_key(|n| n.to_lowercase());
        self.files.sort_by_key(|f| f.path.to_lowercase());
    }

    /// Images in the order chosen by the user; ties broken by name. Those without a date (or with
    /// zero size) go last, in descending order too.
    pub fn sort_files(&mut self, by: SortBy, desc: bool) {
        use std::cmp::Ordering;
        let name = |f: &File| f.path.to_lowercase();
        let ext = |f: &File| f.path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
        let date = |f: &File| crate::human::parse_modified(&f.modified);
        self.files.sort_by(|a, b| {
            let flip = |o: Ordering| if desc { o.reverse() } else { o };
            let primary = match by {
                SortBy::Name => flip(name(a).cmp(&name(b))),
                SortBy::Type => flip(ext(a).cmp(&ext(b))),
                SortBy::Size => (a.size == 0).cmp(&(b.size == 0)).then(flip(a.size.cmp(&b.size))),
                SortBy::Date => match (date(a), date(b)) {
                    (Some(x), Some(y)) => flip(x.cmp(&y)),
                    (x, y) => x.is_none().cmp(&y.is_none()),
                },
            };
            primary.then_with(|| name(a).cmp(&name(b)))
        });
    }
}

/// Subfolders (without hidden ones) and images of `dir`, sorted by name.
async fn list(op: &Operator, dir: &str) -> opendal::Result<Listing> {
    let mut listing = Listing::default();
    for e in op.list(if dir.is_empty() { "/" } else { dir }).await? {
        let path = e.path().trim_start_matches('/');
        if path == dir {
            continue;
        }
        let m = e.metadata();
        // Symlinks to folders arrive without a trailing "/" (see Cargo.toml).
        let path = if m.is_dir() && !path.ends_with('/') { format!("{path}/") } else { path.to_string() };
        let modified = m.last_modified().map(|t| t.to_string()).unwrap_or_default();
        let shown = !hidden(op, &path) && !crate::batch::writing(&local_path(op, &path));
        listing.add(path, e.name().trim_end_matches('/'), m.is_dir(), m.content_length(), modified, shown);
    }
    listing.sort();
    Ok(listing)
}

/// On Windows, local files with the "hidden" attribute, which File Explorer does not show. Among
/// them the compatibility junctions such as `Documents\My Pictures`, which deny listing to everyone.
#[cfg(windows)]
fn hidden(op: &Operator, path: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    op.info().scheme() == "fs"
        && std::fs::symlink_metadata(Path::new(&op.info().root()).join(path.trim_end_matches('/')))
            .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn hidden(_: &Operator, _: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read of a file that is not there any more is noticed, so the folder gets read again.
    #[test]
    fn gone_noticed() {
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

    /// A file a conversion is writing is left out of the listing until it is closed, however its
    /// path is written (OpenDAL canonicalizes the root: on Windows `\\?\` and long names, while the
    /// temp folder has short ones; on Linux a symlink in the path does the same).
    #[test]
    fn writing_left_out() {
        let top = std::env::temp_dir().join(format!("eyecansee-writing-{}", std::process::id()));
        let real = top.join("real");
        std::fs::create_dir_all(&real).unwrap();
        // On Linux the folder is reached through a symlink, which OpenDAL resolves as Windows
        // resolves the short names of the temp folder.
        #[cfg(unix)]
        let dir = {
            std::os::unix::fs::symlink(&real, top.join("link")).unwrap();
            top.join("link")
        };
        #[cfg(not(unix))]
        let dir = real.clone();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        std::fs::write(dir.join("b.mp4"), b"x").unwrap();
        let op = Store::Dal(Operator::new(services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let names = |l: Listing| l.files.into_iter().map(|f| f.path).collect::<Vec<_>>();
        let writing = crate::batch::Writing::start(dir.join("b.mp4"));
        assert_eq!(names(rt.block_on(op.list("")).unwrap()), ["a.jpg"]);
        drop(writing);
        assert_eq!(names(rt.block_on(op.list("")).unwrap()), ["a.jpg", "b.mp4"]);
        std::fs::remove_dir_all(&top).unwrap();
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

    #[test]
    fn sort_files() {
        let f = |path: &str, size, modified: &str| File { path: path.into(), size, modified: modified.into() };
        let mut l = Listing {
            files: vec![
                f("b.png", 300, "1776333600"),
                f("A.jpg", 100, ""),
                f("c.jpg", 200, "2026-01-01T00:00:00Z"),
            ],
            ..Default::default()
        };
        let names = |l: &Listing| l.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        l.sort_files(SortBy::Name, false);
        assert_eq!(names(&l), ["A.jpg", "b.png", "c.jpg"]);
        l.sort_files(SortBy::Size, true);
        assert_eq!(names(&l), ["b.png", "c.jpg", "A.jpg"]);
        // Without a date last, in reverse order too.
        l.sort_files(SortBy::Date, false);
        assert_eq!(names(&l), ["c.jpg", "b.png", "A.jpg"]);
        l.sort_files(SortBy::Date, true);
        assert_eq!(names(&l), ["b.png", "c.jpg", "A.jpg"]);
        l.sort_files(SortBy::Type, false);
        assert_eq!(names(&l), ["A.jpg", "c.jpg", "b.png"]);
    }

    #[test]
    fn trash_apart() {
        let dirs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let trash = format!("p/{TRASH_DIR}/");
        let mut l = Listing { dirs: dirs(&["p/b/", &trash, "p/.a/", "p/A/"]), ..Default::default() };
        l.sort();
        assert_eq!(l.dirs, dirs(&["p/.a/", "p/A/", "p/b/"]));
        assert_eq!(l.trash, Some(trash));
    }

    #[test]
    fn paths_and_open() {
        assert_eq!(split("/photos/sea/"), ("photos/sea/".into(), None));
        assert_eq!(split("/photos/a.JPG"), ("photos/".into(), Some("photos/a.JPG".into())));
        assert_eq!(split("/a.png"), ("".into(), Some("a.png".into())));
        assert_eq!(split("/"), ("".into(), None));
        assert_eq!((parent("a/b/"), parent("a/"), parent("")), ("a/".into(), "".into(), "".into()));
        assert_eq!((name("a/b/"), name("a/c.jpg")), ("b", "c.jpg"));
        // Building the FTP operator does not open connections.
        let s = open("ftp://me:p%40ss@nas.lan/Sea%20photos/a.png").unwrap();
        assert_eq!((s.base.as_str(), s.dir.as_str()), ("ftp://me@nas.lan/", "Sea photos/"));
        assert_eq!(s.start.as_deref(), Some("Sea photos/a.png"));
        assert_eq!(open("gopher://x/").err().map(|e| e.0), Some("error-unsupported"));
        // The checkout folder can have any name (/src in the AppImage container).
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).file_name().unwrap().to_string_lossy();
        let l = open(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(l.dir.ends_with(&format!("{here}/")) && l.start.is_none());
        assert!(in_gvfs("/", "run/user/1000/gvfs/mtp:host=X/DCIM/") && in_gvfs("/run/user/1000/gvfs/", "a.jpg"));
        assert!(!in_gvfs("/", "home/me/a.jpg"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_hidden() {
        let dir = std::env::temp_dir().join("eyecansee-hidden-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("visible")).unwrap();
        std::fs::create_dir_all(dir.join("hidden")).unwrap();
        let ok = std::process::Command::new("attrib").arg("+h").arg(dir.join("hidden")).status().unwrap();
        assert!(ok.success());
        let s = open(&dir.to_string_lossy()).unwrap();
        let Store::Dal(op) = &s.op else { unreachable!() };
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let l = rt.block_on(list(op, &s.dir)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(l.dirs, [format!("{}visible/", s.dir)]);
    }
}
