//! The originals of a move between devices, deleted once copied (#194): only what was copied, the
//! folders from the bottom, each with its files, and only if nothing else is left in it. Before,
//! a folder was deleted whole at the end, and a photo taken in it while it moved went with it.
//! What stays is named in `Progress::left`, for the message at the end of the copy.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use super::{Clip, Planned, Progress};
use crate::source::{Deletion, Error, NOT_FOUND, Store, name, parent};

/// Before a move writes over an item there or into a folder there (`paste`, once everything to
/// copy is known, before anything is written), for each such item of `copies` (with its `lists`):
/// whether it would write into itself, reached in another way (#195). Where the real places can
/// be compared, by them (`within`); between two sources, and on an FTP server (`places_unknown`),
/// also as `not_the_same` tells. Not an item that stays anyway, with something below it left out
/// for its name.
pub(super) async fn not_onto_itself(clip: &Clip, (to, to_base, same): (&Store, &str, bool), copies: &[Planned], lists: &[Vec<(String, String, u64)>], p: &Progress) -> Result<(), Error> {
    if !clip.cut {
        return Ok(());
    }
    for (item, list) in copies.iter().zip(lists).filter(|(c, _)| c.2 && !c.0.is_empty() && !super::left_below(&c.0, p)) {
        if !same || places_unknown(to) {
            not_the_same(&clip.op, &item.0, to, &item.1).await?;
        }
        within(clip, (to, to_base), &item.0, list).await?;
    }
    Ok(())
}

/// Whether the real place of an item of `to` cannot be asked while the server may hold links: FTP
/// (audit F.1 of 27/11). An item there with the name of one moved may be that one itself, reached
/// through a link to it or to its folder: then `not_the_same` tells. Not this computer nor SFTP,
/// which tell the real places (`within`), nor a phone, which has no links.
pub(super) fn places_unknown(to: &Store) -> bool {
    matches!(to, Store::Ftp(_))
}

/// Whether a place the move of `src` writes into (`list`: its destination and what goes below
/// it) is `src` itself, or lies inside it, through a link (audit F.1 of 27/11): a folder merged into
/// a link that leads to it, or into a real folder holding such a link, a file replacing a link to
/// itself on another disk (the rename fails, so it is copied). Each file was written over itself
/// through the link, then deleted as the original. By the real places where they can be compared
/// (`dest_place`); below the item, files only on this computer, where it costs no request (not
/// under gvfs, where each look is slow): on a server a folder each.
async fn within(clip: &Clip, (to, to_base): (&Store, &str), src: &str, list: &[(String, String, u64)]) -> Result<(), Error> {
    let mut top = None;
    for (i, (from, dest, _)) in list.iter().enumerate() {
        let cheap = to.local_file(dest).is_some() && !to.gvfs(dest);
        if !(i == 0 || from.ends_with('/') || cheap) {
            continue;
        }
        let Some(there) = super::dest_place(clip, to, to_base, dest).await else { continue };
        if top.is_none() {
            top = Some(super::real_place(&clip.op, src).await);
        }
        if top.as_ref().and_then(Option::as_ref).is_some_and(|t| there.starts_with(t)) {
            log!("move: {src:?} would write into itself through {dest:?} ({there:?})");
            return Err(("error-move-same-item", name(src).to_string()));
        }
    }
    Ok(())
}

/// Before the original `src` of a move is copied over `dest` of another source, replacing it or
/// merging into it (`paste`, before anything is written): whether the two are one and the same,
/// reached in two ways where the real places cannot be compared (the same server through SFTP and
/// FTP, a NAS through gvfs and SFTP; #195). The original steps aside under a hidden name for a
/// moment: if `dest` goes with it, they are one, the original gets its name back and the move
/// stops, saying so; copied over itself, it would have been written over and then deleted as the
/// original. A source that cannot rename stops too: it cannot be told. A folder `dest` still
/// listed but no longer to be entered while the original is aside, and again once it is back, is
/// a link to it (audit F.1 of 27/11): a link stays listed when what it leads to is gone.
/// Meanwhile the original put aside is held as one being saved (`SAVING`): a listing of its folder
/// (the grid reads it again) would give it its name back halfway (`Store::recover`, which puts
/// back one left by a move stopped there, audit F.7 of 27/11), and it would be told as not the same.
pub(super) async fn not_the_same(from: &Store, src: &str, to: &Store, dest: &str) -> Result<(), Error> {
    let held = src.trim_end_matches('/').to_string();
    crate::source::SAVING.lock().unwrap().insert(held.clone());
    let r = aside_and_back(from, src, to, dest).await;
    crate::source::SAVING.lock().unwrap().remove(&held);
    r
}

/// `not_the_same`, the original held.
async fn aside_and_back(from: &Store, src: &str, to: &Store, dest: &str) -> Result<(), Error> {
    let own = name(src);
    let aside = from.rename(src, name(&crate::trash::moving_name(src))).await.map_err(|e| {
        log!("move: {src:?} not put aside to tell it from {dest:?}: {e:?}");
        ("error-move-not-told", own.to_string())
    })?;
    let still = to.names(&parent(dest)).await.map(|n| n.contains(name(dest)));
    let shut = matches!(still, Ok(true)) && dest.ends_with('/') && !to.is_dir(dest).await;
    let back = from.rename(&aside, own).await;
    log!("move: {src:?} aside, {dest:?} still there: {still:?}, not to be entered: {shut}; back: {back:?}");
    back?;
    match still {
        Ok(true) if shut && to.is_dir(dest).await => Err(("error-move-same-item", own.to_string())),
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

/// The folder `dir` of a move and the files copied in it, `here` (its folders below went first),
/// one by one, then the folder if nothing else is in it: whether it went. Cancel stops between two
/// files. Never the folder whole with its files: on a phone that took the files written into it
/// shortly before the request too (POCO X6 Pro, 2026-11-21: 300 files went in 1.9 s instead of
/// 5.4 s, but files written in the 0.4 s before went with them). MTP does one request at a time.
async fn remove_folder(from: &Store, dir: &str, here: &[&str], walked: &HashSet<&str>, p: &Progress) -> Result<bool, Error> {
    for file in here {
        if p.cancel.load(Ordering::Relaxed) {
            return Err(("error-cancelled", String::new()));
        }
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
/// (each says its own), nor the items a message says already (names left out, links skipped,
/// files not read). A
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
        walked.contains(folder.as_str()) || named(&p.refused.lock().unwrap()) || named(&p.links_skipped.lock().unwrap()) || super::failed(path, p)
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
