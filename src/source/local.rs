//! Local disks (and gvfs mounts) of a `Store::Dal`: their paths, the reads under gvfs two at a
//! time, the names that are not valid UTF-8, and the folders listed with `std::fs`.

use std::path::{Path, PathBuf};

use opendal::Operator;

use super::{BAD_NAME, Error, Listing, NOT_FOUND, in_gvfs, name};

/// Reads under gvfs (phones, cameras, shares mounted by the desktop), two at a time like the
/// workers of `Store::Mtp`. With too many requests at once gvfs stalls: 12 file opens while a
/// 796-entry folder was being listed gave 12 files out of 30 in 31 s and the listing never ended;
/// with 8 or fewer everything took 3-6 s (script, 2026-10-20; gvfsd-fuse has 10 worker threads
/// [Inference: the limit comes from there]). More parallel reads are not faster: gvfsd-mtp serves
/// one request at a time and in a large folder every file costs ~1.5 s (FUSE_LOOKUP + FUSE_OPEN,
/// kernel trace). Listings stay out, otherwise the tree waits for the thumbnails (Pictures after 14 s).
static GVFS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub(super) async fn gvfs_permit(op: &Operator, path: &str) -> Option<tokio::sync::SemaphorePermit<'static>> {
    if fs_gvfs(op, path) { GVFS.acquire().await.ok() } else { None }
}

/// A path of the local file system under a gvfs mount.
pub(super) fn fs_gvfs(op: &Operator, path: &str) -> bool {
    op.info().scheme() == "fs" && in_gvfs(&op.info().root(), path)
}

/// Absolute path of a local file, pushed one element at a time: Windows' "\\?\" root does not accept "/".
pub(crate) fn local_path(op: &Operator, path: &str) -> PathBuf {
    std::iter::once(op.info().root()).chain(path.split('/').filter(|s| !s.is_empty()).map(String::from)).collect()
}

/// A local name that is not valid UTF-8 on Linux (on Windows, UTF-16 with an unpaired
/// surrogate) is listed with U+FFFD in place of what cannot be read, so its path leads
/// nowhere (issue #88): the app keeps paths as text, and carrying the raw bytes everywhere is not
/// worth it for such rare names. A path of this kind that is not found is such a name, not a file
/// gone.
pub(super) fn bad_name(op: &Operator, path: &str) -> bool {
    op.info().scheme() == "fs" && path.contains(char::REPLACEMENT_CHARACTER)
}

/// One item of a local folder: path (folders end in "/"), name, folder or not, size, date.
type LocalItem = (String, String, bool, u64, String);

/// Everything in a local folder, hidden items too, read without OpenDAL's lister: OpenDAL 0.59.5
/// gives a link to a folder the mode of a folder but a path without "/", and debug builds panic on
/// it (issue #82, apache/opendal#8456). A link counts as what it points to; one that leads nowhere
/// (or to itself) is left out, as OpenDAL did. A folder that is missing or not a folder is empty,
/// unless its own name cannot be read (`bad_name`).
pub(super) async fn local_items(op: &Operator, dir: &str) -> Result<Vec<LocalItem>, Error> {
    let (full, dir, bad) = (local_path(op, dir), dir.to_string(), bad_name(op, dir));
    let io = |e: std::io::Error| (if e.kind() == std::io::ErrorKind::NotFound { NOT_FOUND } else { "error-detail" }, e.to_string());
    tokio::task::spawn_blocking(move || {
        let entries = match std::fs::read_dir(&full) {
            Err(e) if bad && e.kind() == std::io::ErrorKind::NotFound => return Err((BAD_NAME, name(&dir).to_string())),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => return Ok(Vec::new()),
            r => r.map_err(io)?,
        };
        let mut items = Vec::new();
        for e in entries {
            let e = e.map_err(io)?;
            let Ok(m) = std::fs::metadata(e.path()) else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            let path = format!("{dir}{name}{}", if m.is_dir() { "/" } else { "" });
            let modified = m.modified().ok().and_then(|t| jiff::Timestamp::try_from(t).ok()).map(|t| t.to_string());
            items.push((path, name, m.is_dir(), if m.is_file() { m.len() } else { 0 }, modified.unwrap_or_default()));
        }
        Ok(items)
    })
    .await
    .map_err(|e| ("error-detail", e.to_string()))?
}

/// Subfolders (without hidden ones) and images of `dir`, sorted by name.
pub(super) async fn list(op: &Operator, dir: &str) -> Result<Listing, Error> {
    let mut listing = Listing::default();
    for (path, name, is_dir, size, modified) in local_items(op, dir).await? {
        let shown = !hidden(op, &path) && !crate::batch::writing(&local_path(op, &path));
        listing.add(path, &name, is_dir, size, modified, shown);
    }
    listing.sort();
    Ok(listing)
}

/// Renames a local file or folder, over a file already there if any. On Windows that file's
/// creation date goes to the one taking its place (#90): the rename keeps the moved file's own,
/// while up to 0.75.0 the old file was deleted first and NTFS gave its date back (tunneling).
pub(super) fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    let created = std::fs::symlink_metadata(to).ok().filter(|m| m.is_file()).and_then(|m| m.created().ok());
    std::fs::rename(from, to)?;
    #[cfg(windows)]
    if let Some(t) = created {
        // The file is already moved: without its old date it is still whole, so a failure stays silent.
        let _ = set_created(to, t);
    }
    Ok(())
}

/// Sets a file's creation date, opening it only to write its attributes (read-only files too).
#[cfg(windows)]
fn set_created(path: &Path, t: std::time::SystemTime) -> std::io::Result<()> {
    use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::FILE_WRITE_ATTRIBUTES;
    let f = std::fs::OpenOptions::new().access_mode(FILE_WRITE_ATTRIBUTES).open(path)?;
    f.set_times(std::fs::FileTimes::new().set_created(t))
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
    use crate::source::Store;
    use opendal::services;

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

    /// Issue #82: links to folders are folders when listing (debug builds panicked inside
    /// OpenDAL) and when copying; deleting one takes the link away, never what it points to.
    #[cfg(unix)]
    #[test]
    fn links_to_folders() {
        use crate::source::{Deletion, TRASH_DIR};
        let top = std::env::temp_dir().join(format!("eyecansee-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        std::fs::create_dir_all(top.join("real/sub")).unwrap();
        std::fs::create_dir_all(top.join("folder")).unwrap();
        for f in ["real/a.jpg", "real/sub/b.jpg", "folder/c.jpg"] {
            std::fs::write(top.join(f), b"x").unwrap();
        }
        let link = |to: &str, at: &str| std::os::unix::fs::symlink(to, top.join(at)).unwrap();
        link("real", "link");
        link("nowhere", "dangling");
        link("../real", "folder/inner");
        link(".", "folder/self");
        let op = Store::Dal(Operator::new(services::Fs::default().root(&top.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let l = rt.block_on(op.list("")).unwrap();
        assert_eq!(l.dirs, ["folder/", "link/", "real/"]);
        let l = rt.block_on(op.list("link/")).unwrap();
        assert_eq!((l.dirs, l.files.len()), (vec!["link/sub/".to_string()], 1));
        let mut names: Vec<_> = rt.block_on(op.names("")).unwrap().into_iter().collect();
        names.sort();
        assert_eq!(names, ["folder", "link", "real"]);
        // A copy takes what a link points to; a link back to a folder being copied stays empty.
        let walked = rt.block_on(op.walk("folder/", &crate::transfer::Progress::default())).unwrap();
        let walked: Vec<_> = walked.iter().map(|(p, size)| format!("{p} {size}")).collect();
        assert_eq!(walked, ["folder/c.jpg 1", "folder/inner/ 0", "folder/inner/a.jpg 1", "folder/inner/sub/ 0", "folder/inner/sub/b.jpg 1", "folder/self/ 0"]);
        // Deleted for good or into the trash folder: the link goes, the folder it points to stays whole.
        rt.block_on(op.delete("link/", Deletion::Permanent)).unwrap();
        rt.block_on(op.delete("folder/inner/", Deletion::TrashFolder)).unwrap();
        assert!(top.join("link").symlink_metadata().is_err() && top.join("folder/inner").symlink_metadata().is_err());
        assert!(top.join("folder").join(TRASH_DIR).join("inner").symlink_metadata().unwrap().is_symlink());
        assert!(top.join("real/a.jpg").exists() && top.join("real/sub/b.jpg").exists());
        std::fs::remove_dir_all(&top).unwrap();
    }

    /// A file renamed over another takes its place with the new content; on Windows it keeps the
    /// creation date of the one it replaced (#90).
    #[test]
    fn rename_over_a_file() {
        let dir = std::env::temp_dir().join(format!("eyecansee-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (old, new) = (dir.join("a.jpg"), dir.join("b.jpg"));
        std::fs::write(&old, b"old").unwrap();
        #[cfg(windows)]
        let created = {
            let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
            set_created(&old, t).unwrap();
            t
        };
        std::fs::write(&new, b"new").unwrap();
        rename(&new, &old).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"new");
        assert!(!new.exists());
        #[cfg(windows)]
        assert_eq!(std::fs::metadata(&old).unwrap().created().unwrap(), created);
        // Onto a free name nothing changes.
        rename(&old, &new).unwrap();
        assert_eq!(std::fs::read(&new).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_hidden() {
        use crate::source::open;
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
