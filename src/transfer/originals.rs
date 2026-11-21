//! The originals of a move between devices, deleted once copied (#194): only what was copied, the
//! folders from the bottom, each with its files, and only if nothing else is left in it. Before,
//! a folder was deleted whole at the end, and a photo taken in it while it moved went with it.
//! What stays is named in `Progress::left`, for the message at the end of the copy.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use super::Progress;
use crate::source::{Deletion, Error, NOT_FOUND, Store, name, parent};

/// Before the original `src` of a move is copied over `dest` of another source, replacing it or
/// merging into it (`paste`, before anything is written): whether the two are one and the same,
/// reached in two ways where the real places cannot be compared (the same server through SFTP and
/// FTP, a NAS through gvfs and SFTP; #195). The original steps aside under a hidden name for a
/// moment: if `dest` goes with it, they are one, the original gets its name back and the move
/// stops, saying so; copied over itself, it would have been written over and then deleted as the
/// original. A source that cannot rename stops too: it cannot be told.
pub(super) async fn not_the_same(from: &Store, src: &str, to: &Store, dest: &str) -> Result<(), Error> {
    let own = name(src);
    let aside = from.rename(src, &format!(".{own}.eyecansee-moving")).await.map_err(|e| {
        log!("move: {src:?} not put aside to tell it from {dest:?}: {e:?}");
        ("error-move-not-told", own.to_string())
    })?;
    let still = to.names(&parent(dest)).await.map(|n| n.contains(name(dest)));
    let back = from.rename(&aside, own).await;
    log!("move: {src:?} aside, {dest:?} still there: {still:?}; back: {back:?}");
    back?;
    match still {
        Ok(true) => Ok(()),
        Ok(false) => Err(("error-move-same-item", own.to_string())),
        Err(e) => Err(e),
    }
}

/// Deletes the original `src` of an item moved between devices. `copied`: what of it was copied
/// (a folder: itself, then the folders and files found below it, as listed before the copy).
/// Whether `src` itself went. For a folder the copy box says how many files went (`Removing`), and
/// Cancel stops between two of them: what has not gone stays.
pub(super) async fn delete(from: &Store, src: &str, copied: &[String], p: &Progress) -> Result<bool, Error> {
    if !src.ends_with('/') {
        from.delete(src, Deletion::Permanent).await?;
        return Ok(true);
    }
    let links = Links::find(from, src, copied).await;
    let files: Vec<&String> = copied.iter().filter(|f| !f.ends_with('/') && !links.through(&parent(f))).collect();
    let _removing = Removing::start(p, files.len());
    let mut folders: Vec<&String> = copied.iter().filter(|f| f.ends_with('/')).collect();
    // From the bottom: a folder after everything below it. Every file copied is in a folder of the
    // list (the walk gives a folder before what it holds).
    folders.sort_by_key(|f| std::cmp::Reverse(f.matches('/').count()));
    let walked: HashSet<&str> = folders.iter().map(|f| f.as_str()).collect();
    let mut went = false;
    for dir in folders {
        let gone = if links.top(dir) {
            remove_link(from, dir, p).await
        } else if links.through(dir) {
            false
        } else {
            let here: Vec<&str> = files.iter().filter(|f| parent(f) == *dir).map(|f| f.as_str()).collect();
            remove_folder(from, dir, &here, &walked, p).await?
        };
        went |= gone && dir == src;
    }
    Ok(went)
}

/// The phase of removing the originals of a folder, for the copy box: on while it lives.
struct Removing<'a>(&'a Progress);

impl<'a> Removing<'a> {
    fn start(p: &'a Progress, files: usize) -> Self {
        p.removed.store(0, Ordering::Relaxed);
        p.removing.store(files, Ordering::Relaxed);
        Removing(p)
    }
}

impl Drop for Removing<'_> {
    fn drop(&mut self) {
        self.0.removing.store(0, Ordering::Relaxed);
    }
}

/// The folder `dir` of a move and the files copied in it, `here` (its folders below went first):
/// whether it went. A phone takes it whole in one request when it holds just those files: a
/// request for each file was a long wait after the copy (300 files of 250 kB on the POCO X6 Pro,
/// 2026-11-21: copy 6.1 s, then 5.4 s file by file, 1.9 s whole). It is listed just before, as
/// when it was emptied file by file and then deleted (a phone deletes a folder whole), so a file
/// arriving between the two is no more at risk than then. Elsewhere, and when anything else is
/// in it, the files go one by one, then the folder if empty.
async fn remove_folder(from: &Store, dir: &str, here: &[&str], walked: &HashSet<&str>, p: &Progress) -> Result<bool, Error> {
    let stop = || if p.cancel.load(Ordering::Relaxed) { Err(("error-cancelled", String::new())) } else { Ok(()) };
    if let Store::Mtp(phone) = from
        && !here.is_empty()
    {
        let names: HashSet<&str> = here.iter().map(|f| name(f)).collect();
        let only_these = match phone.objects(dir).await {
            // One entry per object: two with one name are not one file copied.
            Ok(objects) => objects.len() <= names.len() && objects.iter().all(|(n, folder)| !folder && names.contains(n.as_str())),
            Err((NOT_FOUND, _)) => return Ok(true),
            Err(e) => return Err(e),
        };
        if only_these {
            stop()?;
            from.delete(dir, Deletion::Permanent).await?;
            log!("move: {dir:?} went whole, {} files copied in it", here.len());
            p.removed.fetch_add(here.len(), Ordering::Relaxed);
            return Ok(true);
        }
    }
    for file in here {
        stop()?;
        match from.delete(file, Deletion::Permanent).await {
            // Gone meanwhile: nothing to keep.
            Ok(()) | Err((NOT_FOUND, _)) => {}
            Err(e) => return Err(e),
        }
        p.removed.fetch_add(1, Ordering::Relaxed);
    }
    remove_if_empty(from, dir, walked, p).await
}

/// A link to a folder copied through: the link goes, what it leads to stays. If it cannot go, it
/// stays and is named.
async fn remove_link(from: &Store, dir: &str, p: &Progress) -> bool {
    let r = from.remove_link(dir).await;
    log!("move: link {dir:?} removed: {r:?}");
    if r.is_err() {
        p.left.lock().unwrap().push(dir.to_string());
    }
    r.is_ok()
}

/// Removes the folder `dir` of a move if nothing is left in it; whether it went. What is left in it
/// and was not copied (it arrived meanwhile) goes into `Progress::left`; not the folders listed
/// (each says its own), nor the items a message says already (names left out, links skipped). A
/// folder that looks empty and still cannot go (it holds what the listing does not show, as a
/// link that leads nowhere) is named itself.
async fn remove_if_empty(from: &Store, dir: &str, walked: &HashSet<&str>, p: &Progress) -> Result<bool, Error> {
    let names = match from.names(dir).await {
        Ok(names) => names,
        Err((NOT_FOUND, _)) => return Ok(true),
        Err(e) => return Err(e),
    };
    if names.is_empty() {
        let r = from.remove_empty_dir(dir).await;
        if let Err(e) = &r {
            log!("move: {dir:?} looks empty but stays: {e:?}");
            p.left.lock().unwrap().push(dir.to_string());
        }
        return Ok(r.is_ok());
    }
    let said = |path: &str| {
        let folder = format!("{path}/");
        let named = |list: &[String]| list.iter().any(|l| *l == path || *l == folder);
        walked.contains(folder.as_str()) || named(&p.refused.lock().unwrap()) || named(&p.links_skipped.lock().unwrap())
    };
    let mut left: Vec<String> = names.iter().map(|n| format!("{dir}{n}")).filter(|path| !said(path)).collect();
    left.sort();
    log!("move: {dir:?} stays, {} items in it not copied", left.len());
    p.left.lock().unwrap().extend(left);
    Ok(false)
}

/// The folders of a moved folder reached through a link to a folder, which `walk` follows on this
/// computer and on SFTP: what is in them is not the moved folder's, it is never deleted. Told by
/// the real paths (`Store::real_path`): a folder whose real path is not the one its path gives.
/// Nothing where real paths are not known (phones, FTP: no links followed).
struct Links(HashSet<String>);

impl Links {
    async fn find(from: &Store, src: &str, copied: &[String]) -> Links {
        let mut through = HashSet::new();
        let Some(top) = from.real_path(src).await else { return Links(through) };
        for dir in copied.iter().filter(|f| f.ends_with('/') && f.len() > src.len() && f.starts_with(src)) {
            let wanted = format!("{top}/{}", dir[src.len()..].trim_end_matches('/'));
            if from.real_path(dir).await.as_deref() != Some(wanted.as_str()) {
                through.insert(dir.clone());
            }
        }
        Links(through)
    }

    fn through(&self, dir: &str) -> bool {
        self.0.contains(dir)
    }

    /// The link itself: reached through a link, its folder is not.
    fn top(&self, dir: &str) -> bool {
        self.through(dir) && !self.through(&parent(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> (std::path::PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("eyecansee-originals-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("m/sub")).unwrap();
        for f in ["m/a.jpg", "m/b.jpg", "m/sub/c.jpg"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        (dir, op)
    }

    fn copied() -> Vec<String> {
        ["m/", "m/a.jpg", "m/b.jpg", "m/sub/", "m/sub/c.jpg"].map(String::from).to_vec()
    }

    /// The originals of a folder go, counted for the copy box while they go; the phase ends with them.
    #[test]
    fn removal_counted() {
        let (dir, op) = folder("counted");
        let p = Progress::default();
        let went = tokio::runtime::Runtime::new().unwrap().block_on(delete(&op, "m/", &copied(), &p)).unwrap();
        assert!(went);
        assert!(!dir.join("m").exists());
        assert_eq!(p.removed.load(Ordering::Relaxed), 3);
        assert_eq!(p.removing.load(Ordering::Relaxed), 0, "no longer removing");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Cancel while the originals go: what has not gone stays, folders and files.
    #[test]
    fn removal_cancelled() {
        let (dir, op) = folder("cancelled");
        let p = Progress::default();
        p.cancel.store(true, Ordering::Relaxed);
        let r = tokio::runtime::Runtime::new().unwrap().block_on(delete(&op, "m/", &copied(), &p));
        assert_eq!(r.unwrap_err().0, "error-cancelled");
        for f in ["m/a.jpg", "m/b.jpg", "m/sub/c.jpg"] {
            assert!(dir.join(f).exists(), "{f} stays");
        }
        assert_eq!(p.removing.load(Ordering::Relaxed), 0, "no longer removing");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
