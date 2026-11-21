//! The originals of a move between devices, deleted once copied (#194): only what was copied, the
//! files one by one, then the folders from the bottom, each only if nothing is left in it. Before,
//! a folder was deleted whole at the end, and a photo taken in it while it moved went with it.
//! What stays is named in `Progress::left`, for the message at the end of the copy.

use std::collections::HashSet;

use super::Progress;
use crate::source::{Deletion, Error, NOT_FOUND, Store, name, parent};

/// Before the original `src` of an item that replaced or merged into `dest` of another source is
/// deleted: whether the two are one and the same, reached in two ways where the real places cannot
/// be compared (the same server through SFTP and FTP, a NAS through gvfs and SFTP; #195). The
/// original steps aside under a hidden name for a moment: if `dest` goes with it, they are one,
/// the original gets its name back and the move stops, saying so; written over itself, it would
/// have been deleted as the original. A source that cannot rename stops too: it cannot be told.
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
/// Whether `src` itself went.
pub(super) async fn delete(from: &Store, src: &str, copied: &[String], p: &Progress) -> Result<bool, Error> {
    if !src.ends_with('/') {
        from.delete(src, Deletion::Permanent).await?;
        return Ok(true);
    }
    let links = Links::find(from, src, copied).await;
    for file in copied.iter().filter(|f| !f.ends_with('/') && !links.through(&parent(f))) {
        match from.delete(file, Deletion::Permanent).await {
            // Gone meanwhile: nothing to keep.
            Ok(()) | Err((NOT_FOUND, _)) => {}
            Err(e) => return Err(e),
        }
    }
    let mut folders: Vec<&String> = copied.iter().filter(|f| f.ends_with('/')).collect();
    // From the bottom: a folder after everything below it.
    folders.sort_by_key(|f| std::cmp::Reverse(f.matches('/').count()));
    let walked: HashSet<&str> = folders.iter().map(|f| f.as_str()).collect();
    let mut went = false;
    for dir in folders {
        let gone = if links.top(dir) {
            remove_link(from, dir, p).await
        } else if links.through(dir) {
            false
        } else {
            remove_if_empty(from, dir, &walked, p).await?
        };
        went |= gone && dir == src;
    }
    Ok(went)
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
