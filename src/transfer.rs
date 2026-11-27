//! Copy and move (Ctrl+C or Ctrl+X, then Ctrl+V) between any two sources: the same folder, two
//! folders of a disk, a phone and the computer, two phones. Files travel in chunks from the
//! reader to the writer, so a large video never sits whole in memory.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::source::{Deletion, Error, Store, TRASH_DIR, free_name, name, parent};
use crate::thumbcache;

mod by_phone;
mod names;
mod originals;
mod progress;
mod through_pc;
#[cfg(test)]
pub use by_phone::LONG_SECS;
pub use by_phone::{Bar, Data, Leave, Note, Seen, view};
pub use progress::{Phase, Progress};
pub use through_pc::Staging;
pub(crate) use through_pc::remove_left;

use names::Names;

/// What Ctrl+C or Ctrl+X took: the source it came from, which can be different from the one
/// open when pasting.
#[derive(Clone)]
pub struct Clip {
    pub op: Store,
    /// `base` of the source: the same `base` means the same device.
    pub base: String,
    pub paths: Vec<String>,
    pub cut: bool,
    /// The text put on the system clipboard with the copy (the paths, one per line).
    pub text: String,
}

/// Ctrl+V pastes the items only while the system clipboard still holds `copied`, the text of
/// their copy: after a copy in another program it holds something else (#61). Line ends compared
/// as one (Windows gives `\r\n` back).
pub fn on_clipboard(copied: &str, clipboard: &str) -> bool {
    copied.lines().eq(clipboard.trim_end_matches(['\r', '\n']).lines())
}

/// Whether a paste given by egui (Ctrl+V, or Paste chosen in a menu) pastes the items copied
/// (`copied`: the text of their copy): not in the viewer, where Edit > Paste is greyed out too,
/// and only while the clipboard still holds that copy.
pub fn pastes_items(viewing: bool, copied: Option<&str>, clipboard: &str) -> bool {
    !viewing && copied.is_some_and(|c| on_clipboard(c, clipboard))
}

/// Answer for an item whose name is already in the destination.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Choice {
    /// A file replaces the one there; a folder merges into it (same-name files inside are replaced).
    Replace,
    Skip,
    /// "name (2)".
    KeepBoth,
}

/// Can a paste into `to_dir` replace an item already there (or merge a folder into one)? Not in
/// a folder opened through gvfs (issue #91): a file cannot be renamed into place there safely
/// (#45), so it would be written over, and a copy cut short would leave it damaged. "Keep both"
/// and "Skip" stay; a move there keeps both too.
pub fn can_replace(to: &Store, to_dir: &str) -> bool {
    !to.gvfs(to_dir)
}

/// Items whose name is already in `to_dir`, to ask about. A folder pasted inside itself is an error.
/// Items pasted into their own folder are not conflicts: a copy becomes "name (2)", a move does nothing.
/// A photo whose sidecar (`sidecar_follow`) would land on a file with that name is one too: the
/// sidecar follows the answer for the photo.
pub async fn conflicts(clip: &Clip, to: &Store, to_base: &str, to_dir: &str) -> Result<Vec<String>, Error> {
    let taken = Names::read(to, to_dir).await?;
    let sides = sidecars(clip).await;
    let dest = dest_place(clip, to, to_base, to_dir).await;
    for p in clip.paths.iter().filter(|p| p.ends_with('/')) {
        if inside(clip, to_base, to_dir, dest.as_ref(), p).await {
            return Err(("error-paste-inside", name(p).to_string()));
        }
    }
    let here = in_own_folder(clip, to_base, to_dir, dest.as_ref()).await;
    let mut found = Vec::new();
    for p in &clip.paths {
        let side_taken = sides.get(p).is_some_and(|s| s.iter().any(|s| taken.contains(name(s))));
        if !here.contains(p) && (taken.contains(name(p)) || side_taken) {
            found.push(p.clone());
        }
    }
    Ok(found)
}

/// Where `path` of `op` really is, links resolved, to tell a folder reached by two paths or from
/// two sources (#195): on this computer its absolute path, the same for any two local sources; on
/// an SFTP server its path there. `None` elsewhere (phones, FTP), or when it cannot be read.
async fn real_place(op: &Store, path: &str) -> Option<PathBuf> {
    match op {
        Store::Sftp(_) => op.real_path(path).await.map(|p| PathBuf::from(format!("/{p}"))),
        _ => op.local_file(path)?.canonicalize().ok(),
    }
}

/// The real place of `to_dir` (`real_place`), when it can be compared with those of the clip: both
/// sources on this computer, or both the same SFTP server. Where it cannot (a server opened by SFTP
/// and by FTP, a NAS through gvfs and SFTP), a move deletes only what it copied (#194).
async fn dest_place(clip: &Clip, to: &Store, to_base: &str, to_dir: &str) -> Option<PathBuf> {
    let comparable = match (&clip.op, to) {
        (Store::Dal(_), Store::Dal(_)) => true,
        (Store::Sftp(_), Store::Sftp(_)) => clip.base == to_base,
        _ => false,
    };
    if comparable && COMPARE_PLACES.with(|c| c.get()) { real_place(to, to_dir).await } else { None }
}

thread_local! {
    /// Off only in tests: as between two sources whose real places cannot be compared (a server
    /// through SFTP and through FTP), to test what keeps the files then (`originals::not_the_same`).
    static COMPARE_PLACES: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Whether `to_dir` is the folder `p` of the clip or lies inside it: by the paths on one source,
/// and by the real places (`dest`: that of `to_dir`), through links and between sources (#195).
async fn inside(clip: &Clip, to_base: &str, to_dir: &str, dest: Option<&PathBuf>, p: &str) -> bool {
    if clip.base == to_base && to_dir.starts_with(p) {
        return true;
    }
    match dest {
        Some(d) => real_place(&clip.op, p).await.is_some_and(|r| d.starts_with(r)),
        None => false,
    }
}

/// The items of the clip whose own folder is `to_dir`: by the paths on one source, and by the real
/// places (`dest`) through links and between sources (#195). Pasted there, a copy becomes
/// "name (2)" and a move does nothing: moved onto itself from another source, the file was
/// written over itself and then deleted as the original.
async fn in_own_folder(clip: &Clip, to_base: &str, to_dir: &str, dest: Option<&PathBuf>) -> HashSet<String> {
    let mut folders: HashMap<String, bool> = HashMap::new();
    let mut here = HashSet::new();
    for p in &clip.paths {
        let dir = parent(p);
        let own = match folders.get(&dir) {
            Some(own) => *own,
            None => {
                let own = (clip.base == to_base && dir == to_dir) || (dest.is_some() && real_place(&clip.op, &dir).await.as_ref() == dest);
                folders.insert(dir, own);
                own
            }
        };
        if own {
            here.insert(p.clone());
        }
    }
    here
}

/// The sidecars of the photos of the clip (`sidecar_follow::following`), by photo; each folder
/// read once.
async fn sidecars(clip: &Clip) -> HashMap<String, Vec<String>> {
    let mut folders = crate::sidecar_follow::Folders::new();
    let mut out = HashMap::new();
    for src in clip.paths.iter().filter(|p| !crate::sidecar::names(p).is_empty()) {
        let dir = parent(src);
        if !folders.contains_key(&dir) {
            folders.insert(dir.clone(), clip.op.names(&dir).await.unwrap_or_default());
        }
        let found = crate::sidecar_follow::following(&clip.op, src, &folders[&dir]).await;
        if !found.is_empty() {
            out.insert(src.clone(), found);
        }
    }
    out
}

/// What goes where: source, destination, whether there is already an item with that name, and
/// the same for the photo's sidecars.
type Planned = (String, String, bool, Vec<(String, String, bool)>);

/// The original's thumbnail (`thumb`: size and image, if it had one) waits for the listing of
/// its copy or its new name `dest`: the same content, nothing is read back from a phone or a
/// slow disk. `size`: the copy's, which must match.
fn keep_thumbnail(thumb: Option<(u64, Vec<u8>)>, dest: &str, size: Option<u64>) {
    if let Some((s, data)) = thumb
        && size.is_none_or(|size| size == s)
    {
        thumbcache::keep(dest, s, data);
    }
}

/// Links to folders left out of a copy that worked, to say at its end (#183); `None` when there
/// are none, or when the copy failed or was cancelled (the log has them then).
pub fn links_to_say(r: &Result<(), Error>, p: &Progress) -> Option<Vec<String>> {
    let links = p.links_skipped.lock().unwrap();
    (r.is_ok() && !links.is_empty()).then(|| links.clone())
}

/// Copies (or moves) the clip into `to_dir`. Returns the source items that went away (moved),
/// also when it stops at an error halfway. Items left out for their name are in `p.refused`.
pub async fn run(clip: &Clip, to: &Store, to_base: &str, to_dir: &str, choices: &HashMap<String, Choice>, p: &Progress) -> (Vec<String>, Result<(), Error>) {
    let mut gone = Vec::new();
    let mut clip = clip.clone();
    clip.paths.retain(|src| allowed(&clip.op, to_windows(to), src, name(src), p));
    let r = paste(&clip, to, to_base, to_dir, choices, p, &mut gone).await;
    (gone, r)
}

/// Whether the item `path` of `from` may be copied as `rel` (its path below the folder pasted, or
/// its own name). A name sent by a phone or a server that is not one item of a path at the
/// destination (`source::safe_name`: "../x" would land outside the folder) is left out, with a
/// line in the log, in `Progress::refused`; so is one Windows cannot write, when the destination
/// is a folder of Windows (`windows`; `source::windows_name`: "12:30.jpg" would write into a
/// hidden stream of an empty file "12"). Names of this computer are what its disks hold.
fn allowed(from: &Store, windows: bool, path: &str, rel: &str, p: &Progress) -> bool {
    let fits = |n: &str| crate::source::safe_name(n) && (!windows || crate::source::windows_name(n));
    if matches!(from, Store::Dal(_)) || rel.trim_end_matches('/').split('/').all(fits) {
        return true;
    }
    log!("paste: {path:?} left out: a name sent by the device that is not one a path can hold here");
    p.refused.lock().unwrap().push(path.to_string());
    false
}

/// Whether `to` is a folder of Windows, where some names of a phone or a server cannot be written
/// (`allowed`); a server or a phone there takes them.
fn to_windows(to: &Store) -> bool {
    cfg!(windows) && matches!(to, Store::Dal(_))
}

/// Whether something below the folder `src` was left out (`allowed`): its original stays.
fn left_below(src: &str, p: &Progress) -> bool {
    src.ends_with('/') && p.refused.lock().unwrap().iter().any(|r| r.starts_with(src))
}

/// The items of a paste on the same device that a move makes as renames (`renamed_on_device`),
/// counted in the window as files and bytes (their sizes asked only on a phone, where between its
/// storages each one is done by the phone and can take minutes, `mtp::across`); the folder they go
/// to is read again after each, so what arrived shows (Piero, 25/11: it showed only at the end).
/// What is left to copy.
async fn renames(clip: &Clip, (to, to_dir, same): (&Store, &str, bool), plan: Vec<Planned>, p: &Progress, gone: &mut Vec<String>) -> Result<Vec<Planned>, Error> {
    let mut copies = Vec::new();
    let renamed = |e: &Planned| clip.cut && same && !(e.2 && e.1.ends_with('/'));
    let phone = matches!(to, Store::Mtp(_));
    // All the sizes first, so the total is right from the start (it grew file by file). A big
    // file to the phone's other storage is copied through this computer and then deleted
    // (`through_pc`), decided here: a rename refused later would come after the item it replaces
    // was deleted.
    let mut sizes = Vec::new();
    for e in &plan {
        let size = if phone && renamed(e) && !e.0.is_empty() && !e.0.ends_with('/') { clip.op.size(&e.0).await.unwrap_or(0) } else { 0 };
        sizes.push((renamed(e) && !(phone && through_pc::by_computer(size, &e.0, &e.1, None))).then_some(size));
    }
    if sizes.iter().any(Option::is_some) {
        p.files_total.store(sizes.iter().flatten().count(), Ordering::Relaxed);
        p.total.store(sizes.iter().flatten().sum(), Ordering::Relaxed);
        let _ = p.started.set(Instant::now());
    }
    for ((src, dest, exists, sides), size) in plan.into_iter().zip(sizes) {
        if let Some(size) = size {
            *p.current.lock().unwrap() = name(&src).to_string();
            let made = (!src.is_empty()).then(|| dest.clone());
            let folder = src.ends_with('/');
            match renamed_on_device(clip, to, to_dir, (src, dest, exists, sides), gone).await? {
                Some(entry) => {
                    p.total.fetch_sub(size, Ordering::Relaxed);
                    p.files_total.fetch_sub(1, Ordering::Relaxed);
                    copies.push(entry);
                }
                None => {
                    p.made.lock().unwrap().extend(made);
                    p.files.fetch_add(1, Ordering::Relaxed);
                    p.folders_moved.fetch_add(usize::from(folder), Ordering::Relaxed);
                    p.done.fetch_add(size, Ordering::Relaxed);
                    p.touched.lock().unwrap().push(to_dir.to_string());
                }
            }
            continue;
        }
        copies.push((src, dest, exists, sides));
    }
    Ok(copies)
}

/// Whether the paste of `clip` into `to_dir` of `to` is one the phone copies by itself inside
/// one storage (`by_phone`), told to `p` as soon as the paste is asked, before `run` starts: its
/// window is the one in the middle from its first frame (the copy box in the corner showed for a
/// second while the files were counted; Piero, 26/11). Nothing is asked of the phone for it.
/// The same session as `copy_file` tells it: a phone plugged in again between copy and paste is another.
pub fn announce(clip: &Clip, to: &Store, to_dir: &str, p: &Progress) {
    let same_phone = matches!((&clip.op, to), (Store::Mtp(a), Store::Mtp(b)) if std::sync::Arc::ptr_eq(a, b));
    p.by_phone.decide(by_phone::by_phone(clip.cut, same_phone, &clip.paths, to_dir), by_phone::many(&clip.paths));
}

async fn paste(clip: &Clip, to: &Store, to_base: &str, to_dir: &str, choices: &HashMap<String, Choice>, p: &Progress, gone: &mut Vec<String>) -> Result<(), Error> {
    let _ = p.begun.set(Instant::now());
    // A wait for the phone left during the paste before is not this one's.
    crate::mtp::stay();
    let same = clip.base == to_base;
    announce(clip, to, to_dir, p);
    let present = Names::read(to, to_dir).await?;
    let here = in_own_folder(clip, to_base, to_dir, dest_place(clip, to, to_base, to_dir).await.as_ref()).await;
    let plan = planned(clip, to, to_dir, &here, choices, &present, &sidecars(clip).await);
    p.skipped.store(skips(clip, &here, choices), Ordering::Relaxed);

    // On the same device a move is a rename, unless a folder has to merge into an existing one;
    // if the rename fails (another disk: different file system) the files are copied. On this
    // computer the rename itself replaces a file: one that is there stays until then, and still
    // there if the copy fails (#90). Phones, servers and gvfs rename only onto a free name.
    let copies = renames(clip, (to, to_dir, same), plan, p, gone).await?;

    // Everything to copy, with sizes for the progress bar; a photo's sidecars after it.
    let start = Instant::now();
    let mut lists = Vec::new();
    for (src, dest, _, sides) in &copies {
        // No photo: sidecars whose photo moved already (`renamed_on_device`).
        let mut list = if src.is_empty() {
            Vec::new()
        } else if src.ends_with('/') {
            folder_items(clip, to, src, dest, p).await?
        } else {
            vec![(src.clone(), dest.clone(), clip.op.size(src).await?)]
        };
        for (s, d, _) in sides {
            list.push((s.clone(), d.clone(), clip.op.size(s).await?));
        }
        lists.push(list);
    }
    // A move between sources that would replace an item, or merge a folder into one, first tells
    // whether that item is the original itself, reached in another way (#195): nothing is written
    // over it, not even once (L8 of 2026-11-21: told only after the copy, the file was written
    // over itself). All of them before anything is copied; not a folder that stays anyway, with
    // an item left out for its name.
    if clip.cut && !same {
        for (src, dest, _, _) in copies.iter().filter(|c| c.2 && !c.0.is_empty() && !left_below(&c.0, p)) {
            originals::not_the_same(&clip.op, src, to, dest).await?;
        }
    }
    // Counted as the files chosen: a photo with its sidecars is one.
    let files = || lists.iter().zip(&copies).flat_map(|(l, c)| &l[..l.len() - c.3.len()]).filter(|(s, _, _)| !s.ends_with('/'));
    // Added to the renames counted above, if any.
    p.files_total.fetch_add(files().count(), Ordering::Relaxed);
    p.total.fetch_add(lists.iter().flatten().filter(|(s, _, _)| !s.ends_with('/')).map(|(_, _, size)| size).sum(), Ordering::Relaxed);
    *p.looked.lock().unwrap() = start.elapsed().as_secs_f64();
    let _ = p.started.set(Instant::now());

    // The names in each destination folder of a merge, read once (`Present`).
    let mut there = Present::new(to_dir, present);
    for (item, list) in copies.iter().zip(lists) {
        let copied = copy_item(clip, to, item, list, &mut there, p).await?;
        let (src, dest, _, sides) = item;
        if !src.is_empty() && copied.contains(src) {
            p.made.lock().unwrap().push(dest.clone());
        }
        // Moved between devices: the original goes only once all of it has been copied, its
        // sidecars before it (stopped between the two, the photo stays with no sidecar of its
        // own left behind); of a folder only what was copied (`originals`). A folder with an item
        // left out because of its name stays whole.
        // A photo not copied (`failed`) stays, with its sidecars.
        let photo_failed = !src.is_empty() && !src.ends_with('/') && !copied.contains(src);
        if clip.cut && !left_below(src, p) && !photo_failed {
            for (s, _, _) in sides.iter().filter(|(s, _, _)| !failed(s, p)) {
                clip.op.delete(s, Deletion::Permanent).await?;
            }
            if !src.is_empty() && originals::delete(&clip.op, src, &copied, p).await? {
                gone.push(src.clone());
            }
        }
    }
    Ok(())
}

/// Copies one item of the plan, `list`: everything of it (`paste`), the photo's sidecars last.
/// Returns what of it was copied, its sidecars apart: the item, and below a folder each folder and
/// file found, as `originals::delete` wants them.
async fn copy_item(clip: &Clip, to: &Store, (src, _, exists, sides): &Planned, list: Vec<(String, String, u64)>, there: &mut Present, p: &Progress) -> Result<Vec<String>, Error> {
    let merging = src.ends_with('/') && *exists;
    let photos = list.len() - sides.len();
    let mut copied = Vec::new();
    for (i, (from, dest, size)) in list.into_iter().enumerate() {
        if from.ends_with('/') {
            if merging {
                there.made(to, &dest).await;
            }
            to.create_dir(&dest).await?;
        } else {
            if p.cancel.load(Ordering::Relaxed) {
                return Err(("error-cancelled", String::new()));
            }
            *p.current.lock().unwrap() = name(&from).to_string();
            // Whether a file is already where it goes: a file pasted over one, or one of a
            // folder that merges into another; in a new folder nothing is there.
            let existed = match i.checked_sub(photos) {
                Some(side) => Some(sides[side].2),
                None if merging => there.has(to, &dest).await,
                None => Some(*exists),
            };
            match copy_file(&clip.op, &from, (to, &dest, existed), size, p).await {
                Ok(()) => {}
                Err(Fail::Stop(e)) => return Err(e),
                Err(Fail::Source(e)) => {
                    left_out(&clip.op, &from, e, p).await?;
                    // A photo not copied: its sidecars stay with it, not copied either.
                    if i < photos && !sides.is_empty() {
                        break;
                    }
                    continue;
                }
            }
            keep_thumbnail(thumbcache::stored(&clip.base, &from), &dest, Some(size));
            if i < photos {
                p.files.fetch_add(1, Ordering::Relaxed);
            }
        }
        p.touched.lock().unwrap().push(parent(&dest));
        if i < photos {
            copied.push(from);
        }
    }
    Ok(copied)
}

/// What goes where (`Planned`), as the answers to the conflicts (`choices`) say; `here`: the items
/// pasted into their own folder (`in_own_folder`); `present`: the names in `to_dir`; `sidecars`:
/// those of the photos, which follow them under their new name ("a (2).CR2" with "a (2).CR2.xmp";
/// `sidecar::free_pair`). Replace takes the name of the item there, as it is there: "img_001.jpg"
/// for "IMG_001.JPG" where capitals do not count (#204).
fn planned(clip: &Clip, to: &Store, to_dir: &str, here: &HashSet<String>, choices: &HashMap<String, Choice>, present: &Names, sidecars: &HashMap<String, Vec<String>>) -> Vec<Planned> {
    let mut taken = present.clone();
    let mut plan = Vec::new();
    for src in &clip.paths {
        let own = name(src);
        let sides: Vec<String> = sidecars.get(src).map_or_else(Vec::new, |s| s.iter().map(|s| name(s).to_string()).collect());
        let choice = match (here.contains(src), clip.cut) {
            (true, true) => continue,
            (true, false) => Choice::KeepBoth,
            _ => choices.get(src).copied().unwrap_or(Choice::Replace),
        };
        // Under gvfs nothing is replaced, not even an item that came after the question: both stay.
        let busy = crate::sidecar::free_pair(|n| taken.taken(n), own, &sides) != own;
        let choice = if choice == Choice::Replace && busy && !can_replace(to, to_dir) { Choice::KeepBoth } else { choice };
        let dest_name = match choice {
            Choice::Skip => continue,
            Choice::KeepBoth => crate::sidecar::free_pair(|n| taken.taken(n), own, &sides),
            Choice::Replace => present.get(own).unwrap_or(own).to_string(),
        };
        let side_plan: Vec<(String, String, bool)> = sides
            .iter()
            .map(|s| {
                let n = crate::sidecar::renamed(own, s, &dest_name);
                taken.insert(n.clone());
                (format!("{}{s}", parent(src)), format!("{to_dir}{n}"), present.contains(&n))
            })
            .collect();
        taken.insert(dest_name.clone());
        plan.push((src.clone(), format!("{to_dir}{dest_name}{}", if src.ends_with('/') { "/" } else { "" }), present.contains(&dest_name), side_plan));
    }
    plan
}

/// The items left out by the answer Skip (`planned`); not those pasted into their own folder.
fn skips(clip: &Clip, here: &HashSet<String>, choices: &HashMap<String, Choice>) -> usize {
    clip.paths.iter().filter(|s| !here.contains(*s) && choices.get(*s) == Some(&Choice::Skip)).count()
}

/// A move on the same device, as a rename, the photo's sidecars after it (`paste`). What a rename
/// cannot move is returned to be copied and deleted: the photo with its sidecars, or the sidecars
/// alone once the photo moved.
async fn renamed_on_device(clip: &Clip, to: &Store, to_dir: &str, (src, dest, mut exists, sides): Planned, gone: &mut Vec<String>) -> Result<Option<Planned>, Error> {
    // Phones, servers and gvfs rename only onto a free name: what is there goes first.
    let overwrites = |d: &str| to.local_file(d).is_none() || to.gvfs(d);
    // On this computer a rename replaces a file: one not seen as there, as one with other capitals
    // where they do not count, is never renamed over (#204).
    let unseen = |d: &str, there: bool| !there && to.local_file(d).is_some_and(|f| f.symlink_metadata().is_ok());
    if unseen(&dest, exists) || sides.iter().any(|(_, d, there)| unseen(d, *there)) {
        return Err(("error-exists", name(&dest).to_string()));
    }
    if exists && overwrites(&dest) {
        to.delete(&dest, Deletion::Permanent).await?;
        exists = false;
    }
    let thumb = (!src.ends_with('/')).then(|| thumbcache::stored(&clip.base, &src)).flatten();
    if clip.op.move_into(&src, to_dir, name(&dest)).await.is_err() {
        return Ok(Some((src, dest, exists, sides)));
    }
    keep_thumbnail(thumb, &dest, None);
    gone.push(src.clone());
    let mut left = Vec::new();
    for (s, d, mut there) in sides {
        if there && overwrites(&d) {
            to.delete(&d, Deletion::Permanent).await?;
            there = false;
        }
        if clip.op.move_into(&s, to_dir, name(&d)).await.is_err() {
            left.push((s, d, there));
        }
    }
    // The photo moved already: its sidecars are copied alone, as files of no photo, after it.
    Ok((!left.is_empty()).then(|| (String::new(), String::new(), false, left)))
}

/// The names already in the destination folders of a merge, each folder read once, when its first
/// file is copied: asking for every file whether it is there cost a request each (#90 follow-up).
/// A folder the merge makes is empty.
struct Present(HashMap<String, Names>);

impl Present {
    /// Starting with the names of `dir`, read already.
    fn new(dir: &str, names: Names) -> Self {
        Present(HashMap::from([(dir.to_string(), names)]))
    }

    /// Is there an item at `path`, as the destination tells names (`Names`)? `None` if its folder
    /// cannot be read (`write_file` asks then).
    async fn has(&mut self, to: &Store, path: &str) -> Option<bool> {
        let dir = parent(path);
        if !self.0.contains_key(&dir) {
            self.0.insert(dir.clone(), Names::read(to, &dir).await.ok()?);
        }
        Some(self.0[&dir].contains(name(path)))
    }

    /// The folder `dir` is about to be made, if it is not there: it starts empty.
    async fn made(&mut self, to: &Store, dir: &str) {
        if self.has(to, dir).await == Some(false) {
            let empty = self.0[&parent(dir)].empty();
            self.0.insert(dir.to_string(), empty);
        }
    }
}

/// A folder and everything to copy from below it: (source, destination, size).
async fn folder_items(clip: &Clip, to: &Store, src: &str, dest: &str, p: &Progress) -> Result<Vec<(String, String, u64)>, Error> {
    // Made before looking inside, which can take seconds on a phone: the folder shows up at once.
    to.create_dir(dest).await?;
    p.touched.lock().unwrap().push(parent(dest));
    let below = clip.op.walk(src, p).await?;
    let mut list = vec![(src.to_string(), dest.to_string(), 0)];
    let mut trashes = HashMap::new();
    for (path, size) in below {
        if !allowed(&clip.op, to_windows(to), &path, &path[src.len()..], p) {
            continue;
        }
        if let Some(rel) = place(to, dest, &path[src.len()..], clip.cut, &mut trashes).await {
            list.push((path, format!("{dest}{rel}"), size));
        }
    }
    Ok(list)
}

/// Names taken in a trash folder of the destination, and what each item of the source's trash became.
type Trashes = HashMap<String, (HashSet<String>, HashMap<String, String>)>;

/// Where an item found below a pasted folder goes, relative to the folder's destination `dest`.
/// Items in EyeCanSee's trash folders are left out of a copy (`None`) but come along in a move:
/// the original folder is deleted afterwards, and its trash with it (lost before 0.73.1). In a
/// trash that is already at the destination (merging folders) nothing is replaced: an item whose
/// name is taken becomes "name (2)", as when keeping both, and what it contains follows it.
async fn place(to: &Store, dest: &str, rel: &str, cut: bool, trashes: &mut Trashes) -> Option<String> {
    let marker = format!("{TRASH_DIR}/");
    let Some((at, _)) = rel.match_indices(&marker).find(|(i, _)| *i == 0 || rel.as_bytes()[i - 1] == b'/') else {
        return Some(rel.to_string());
    };
    if !cut {
        return None;
    }
    let (trash, rest) = rel.split_at(at + marker.len());
    if rest.is_empty() {
        return Some(rel.to_string());
    }
    if !trashes.contains_key(trash) {
        let taken = to.names(&format!("{dest}{trash}")).await.unwrap_or_default();
        trashes.insert(trash.to_string(), (taken, HashMap::new()));
    }
    let (taken, renamed) = trashes.get_mut(trash)?;
    let (top, below) = rest.split_once('/').map_or((rest, None), |(t, b)| (t, Some(b)));
    let new = renamed
        .entry(top.to_string())
        .or_insert_with(|| {
            let n = free_name(taken, top);
            taken.insert(n.clone());
            n
        })
        .clone();
    Some(match below {
        Some(b) => format!("{trash}{new}/{b}"),
        None => format!("{trash}{new}"),
    })
}

/// A file of this computer being written: kept out of the listing and of the folder watcher
/// until it is whole, as for conversions (`batch::Writing`); half written, its thumbnail was tried
/// every 0.3 s and showed the error mark (Windows, phone to computer, 2026-10-31). The folder is
/// read again once a second during the copy (`Progress::touched`), so it shows up then.
fn writing_guard(to: &Store, dest: &str) -> Option<crate::batch::Writing> {
    to.local_file(dest).map(crate::batch::Writing::start)
}

/// A new file of this computer takes its name only while it is free, as the disk tells names
/// (`create_new`): a file there that the folder's names did not show, as one with other capitals
/// where they do not count, is never written over (#204). Not under gvfs.
fn claim(to: &Store, dest: &str) -> Result<(), Error> {
    let Some(file) = to.local_file(dest).filter(|_| !to.gvfs(dest)) else { return Ok(()) };
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&file) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(("error-exists", name(dest).to_string())),
        Err(e) => Err(("error-detail", e.to_string())),
    }
}

/// Why a file was not copied (`copy_file`).
enum Fail {
    /// Its source could not be read: the copy goes on without it (`left_out`).
    Source(Error),
    /// Anything else (the destination, Cancel): the copy stops.
    Stop(Error),
}

/// Whether the file `path` of the source was left out of the copy, not read (`Progress::failed`).
fn failed(path: &str, p: &Progress) -> bool {
    p.failed.lock().unwrap().iter().any(|(f, _)| f == path)
}

/// The file `src` that could not be read, `e`: left out, and the copy goes on, unless its folder
/// cannot be reached either (a server gone, a phone unplugged): then the copy stops with `e`, as
/// waiting for every file left would take long and copy nothing.
async fn left_out(from: &Store, src: &str, e: Error, p: &Progress) -> Result<(), Error> {
    if let Err(gone) = from.check_dir(&parent(src)).await {
        log!("paste: {src:?} not read ({e:?}), and its folder cannot be reached ({gone:?}): stopping");
        return Err(e);
    }
    log!("paste: {src:?} left out, not read: {e:?}");
    p.failed.lock().unwrap().push((src.to_string(), e));
    Ok(())
}

/// A file of `size` bytes from `src` of `from` to `dest` of `to`; `existed`: whether something is
/// already there (`replace::write_file`): a copy that fails leaves nothing, and a file it was
/// replacing as it was (#90).
async fn copy_file(from: &Store, src: &str, (to, dest, existed): (&Store, &str, Option<bool>), size: u64, p: &Progress) -> Result<(), Fail> {
    // Inside the same phone: copied by the phone itself. Through the computer the upload would
    // hold the phone while waiting for the download of the same file from it.
    if let (Store::Mtp(a), Store::Mtp(b)) = (from, to)
        && std::sync::Arc::ptr_eq(a, b)
    {
        // A big one through the computer, with a bar and Cancel (`through_pc`).
        if through_pc::by_computer(size, src, dest, a.alone_limit(dest)) {
            return through_pc::copy(from, src, (to, dest, existed), size, p).await;
        }
        p.by_phone.begin(name(src), size);
        // Cancel while the phone copies inside one storage: the phone ends the file, then the copy
        // it made goes and the paste stops there (`by_phone::undone`, #354).
        let never = AtomicBool::new(false);
        let cancel = if p.by_phone.undone(src, dest) { &p.cancel } else { &never };
        a.copy_inside(src, dest, cancel).await.map_err(Fail::Stop)?;
        p.by_phone.end(size);
        p.done.fetch_add(size, Ordering::Relaxed);
        return Ok(());
    }
    // A phone must receive a file whole: an upload broken off halfway left it answering nothing
    // ("operation timed out" for every listing, POCO X7 Pro, 2026-10-22). There Cancel waits for
    // the end of the file being copied, which stays.
    let to_phone = matches!(to, Store::Mtp(_));
    let never = AtomicBool::new(false);
    let cancel = if to_phone { &never } else { &p.cancel };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let writing = writing_guard(to, dest);
    if existed == Some(false) {
        claim(to, dest).map_err(Fail::Stop)?;
    }
    let (read, write) = futures_util::future::join(from.send_chunks(src, size, tx, &p.done, cancel), crate::replace::write_file(to, dest, size, rx, existed)).await;
    drop(writing);
    if to_phone && read.is_ok() && write.is_ok() && p.cancel.load(Ordering::Relaxed) {
        p.files.fetch_add(1, Ordering::Relaxed);
        return Err(Fail::Stop(("error-cancelled", String::new())));
    }
    outcome(read, write, &p.cancel)
}

/// What a copy of chunks came to, from its reading end and its writing end.
fn outcome(read: Result<(), Error>, write: Result<(), Error>, cancel: &AtomicBool) -> Result<(), Fail> {
    match (read, write) {
        (Err(e), _) if cancel.load(Ordering::Relaxed) => Err(Fail::Stop(e)),
        // When the writer stops, the reader sees a closed channel ("error-cancelled"): the
        // writer's error is the real one. Otherwise the reader stopped first, and the writer
        // only says so: the source could not be read.
        (Err(e), _) if e.0 != "error-cancelled" => Err(Fail::Source(e)),
        (_, Err(e)) | (Err(e), Ok(())) => Err(Fail::Stop(e)),
        (Ok(()), Ok(())) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Links left out are said at the end of a copy that worked, not of one stopped (#183).
    #[test]
    fn links_said_at_the_end() {
        let p = Progress::default();
        assert_eq!(links_to_say(&Ok(()), &p), None);
        p.links_skipped.lock().unwrap().extend(["a/l1", "a/l2", "a/b/l3"].map(String::from));
        assert_eq!(links_to_say(&Ok(()), &p), Some(["a/l1", "a/l2", "a/b/l3"].map(String::from).to_vec()));
        assert_eq!(links_to_say(&Err(("error-cancelled", String::new())), &p), None);
        assert_eq!(links_to_say(&Err(("error-too-deep", "a/".into())), &p), None);
    }

    /// The paste of the files only with the text of their copy still on the clipboard.
    #[test]
    fn paste_only_with_own_text() {
        let copied = "/home/a/x.jpg\n/home/a/y.png";
        assert!(on_clipboard(copied, copied));
        assert!(on_clipboard(copied, "/home/a/x.jpg\r\n/home/a/y.png\r\n"));
        assert!(!on_clipboard(copied, "some text copied elsewhere"));
        assert!(!on_clipboard(copied, "/home/a/x.jpg"));
        assert!(!on_clipboard(copied, ""));
    }

    /// Ctrl+V in the viewer pastes nothing, as Edit > Paste is greyed out there; in the grid the items.
    #[test]
    fn no_paste_in_the_viewer() {
        let copied = "/home/a/x.jpg";
        assert!(pastes_items(false, Some(copied), copied));
        assert!(!pastes_items(true, Some(copied), copied));
        assert!(!pastes_items(false, None, copied));
        assert!(!pastes_items(false, Some(copied), "some text copied elsewhere"));
    }

    /// The copy box: looking, then copying (bytes on the bar), then removing the originals of a
    /// folder moved (files on the bar, no bytes), then copying again (the next item of the move).
    #[test]
    fn phases_of_the_box() {
        let p = Progress::default();
        p.found.store(7, Ordering::Relaxed);
        assert_eq!(p.phase(), Phase::Looking(7));
        assert_eq!(p.phase().fraction(), 0.0);
        p.files_total.store(4, Ordering::Relaxed);
        p.total.store(400, Ordering::Relaxed);
        let _ = p.started.set(Instant::now());
        p.files.store(4, Ordering::Relaxed);
        p.done.store(400, Ordering::Relaxed);
        assert_eq!(p.phase(), Phase::Copying { files: 4, files_total: 4, done: 400, total: 400 });
        assert_eq!(p.phase().fraction(), 1.0);
        p.removing.store(4, Ordering::Relaxed);
        p.removed.store(1, Ordering::Relaxed);
        assert_eq!(p.phase(), Phase::Removing { removed: 1, total: 4 });
        assert_eq!(p.phase().fraction(), 0.25);
        p.removing.store(0, Ordering::Relaxed);
        assert!(matches!(p.phase(), Phase::Copying { .. }));
    }

    /// The speed of the last 4 seconds, not of the whole copy.
    #[test]
    fn recent_speed() {
        let mut r = progress::Recent::default();
        assert_eq!(r.speed(0.0, 0), None);
        assert_eq!(r.speed(0.5, 10), None);
        assert_eq!(r.speed(2.0, 40), Some(20.0));
        // Fast at first, then slow: only the slow part counts after 4 seconds.
        assert_eq!(r.speed(10.0, 50), None);
        assert_eq!(r.speed(12.0, 70), Some(10.0));
    }

    /// #356: what the end message counts. A move on the same disk takes a folder whole: a
    /// folder, not a file; the item answered Skip is counted; a copy into its own folder (kept
    /// both, never asked) is not a skip.
    #[test]
    fn skips_and_folders_moved_counted() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-transfer-end-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/f")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        for f in ["a/x.jpg", "a/f/y.jpg", "a/f/z.jpg", "b/x.jpg"] {
            std::fs::write(dir.join(f), b"data").unwrap();
        }
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let base = format!("test-transfer-end-{}://", std::process::id());
        let clip = |paths: &[&str], cut| Clip { op: op.clone(), base: base.clone(), paths: paths.iter().map(|s| s.to_string()).collect(), cut, text: String::new() };
        rt.block_on(async {
            let choices = HashMap::from([("a/x.jpg".to_string(), Choice::Skip)]);
            let p = Progress::default();
            run(&clip(&["a/x.jpg", "a/f/"], true), &op, &base, "b/", &choices, &p).await.1.unwrap();
            assert!(dir.join("b/f/y.jpg").exists() && dir.join("a/x.jpg").exists());
            let counted = |p: &Progress| [&p.files, &p.folders_moved, &p.skipped].map(|n| n.load(Ordering::Relaxed));
            assert_eq!(counted(&p), [1, 1, 1], "files (the folder among them), folders moved whole, skipped");
            let p = Progress::default();
            run(&clip(&["b/x.jpg"], false), &op, &base, "b/", &HashMap::new(), &p).await.1.unwrap();
            assert_eq!(counted(&p), [1, 0, 0], "a copy in its own folder");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Copy, copy in place, move with a conflict, cancel, between two local folders.
    #[test]
    fn local_copy_and_move() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-transfer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/sub")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(dir.join("a/x.jpg"), vec![7u8; 3 << 20]).unwrap();
        std::fs::write(dir.join("a/sub/y.mp4"), b"video").unwrap();
        std::fs::write(dir.join("b/x.jpg"), b"old").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let base = format!("test-transfer-{}://", std::process::id());
        let clip = |paths: &[&str], cut| Clip { op: op.clone(), base: base.clone(), paths: paths.iter().map(|s| s.to_string()).collect(), cut, text: String::new() };
        // The thumbnail of x.jpg, saved (asynchronously) before the copies.
        let x = crate::source::File { path: "a/x.jpg".into(), size: 3 << 20, modified: "then".into() };
        thumbcache::save(&base, &x, &image::DynamicImage::new_rgb8(4, 3));
        for _ in 0..100 {
            if thumbcache::stored(&base, "a/x.jpg").is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let thumb = thumbcache::stored(&base, "a/x.jpg").expect("thumbnail saved").1;
        rt.block_on(async {
            // Copy into another folder: x.jpg is a conflict, kept both.
            let c = clip(&["a/x.jpg", "a/sub/"], false);
            assert_eq!(conflicts(&c, &op, &base, "b/").await.unwrap(), vec!["a/x.jpg"]);
            assert_eq!(conflicts(&c, &op, &base, "a/sub/").await.unwrap_err().0, "error-paste-inside");
            let choices = HashMap::from([("a/x.jpg".to_string(), Choice::KeepBoth)]);
            let p = Progress::default();
            let (gone, r) = run(&c, &op, &base, "b/", &choices, &p).await;
            r.unwrap();
            assert!(gone.is_empty());
            assert_eq!(*p.made.lock().unwrap(), ["b/x (2).jpg", "b/sub/"], "what the paste made, in order (#323)");
            assert_eq!(std::fs::read(dir.join("b/x (2).jpg")).unwrap().len(), 3 << 20);
            assert_eq!(std::fs::read(dir.join("b/x.jpg")).unwrap(), b"old");
            assert_eq!(std::fs::read(dir.join("b/sub/y.mp4")).unwrap(), b"video");
            assert_eq!(p.files.load(Ordering::Relaxed), 2);
            assert_eq!(p.done.load(Ordering::Relaxed), (3 << 20) + 5);
            assert_eq!(thumbcache::take("b/x (2).jpg", 3 << 20).as_ref(), Some(&thumb), "the copy has the original's thumbnail waiting");
            assert!(thumbcache::take("b/sub/y.mp4", 5).is_none(), "never had one");
            assert!(!crate::batch::writing(&dir.join("b/x (2).jpg")), "no longer being written once copied");
            let guard = writing_guard(&op, "b/z.mp4");
            assert!(crate::batch::writing(&dir.join("b/z.mp4")), "a local copy is marked while written");
            drop(guard);
            assert!(!crate::batch::writing(&dir.join("b/z.mp4")));

            // Copy into its own folder: "name (2)".
            let (_, r) = run(&clip(&["a/sub/"], false), &op, &base, "a/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert!(dir.join("a/sub (2)/y.mp4").exists());

            // Move replacing: the old x.jpg is gone, the source too.
            let p = Progress::default();
            let (gone, r) = run(&clip(&["a/x.jpg"], true), &op, &base, "b/", &HashMap::new(), &p).await;
            r.unwrap();
            assert_eq!(gone, vec!["a/x.jpg"]);
            assert_eq!(*p.made.lock().unwrap(), ["b/x.jpg"], "a move by a rename made x.jpg there (#323)");
            assert_eq!(std::fs::read(dir.join("b/x.jpg")).unwrap().len(), 3 << 20);
            assert!(!dir.join("a/x.jpg").exists());
            assert_eq!(thumbcache::take("b/x.jpg", 3 << 20).as_ref(), Some(&thumb), "moved by a rename: the thumbnail waits too");

            // Cancelled: no half file left.
            let p = Progress::default();
            p.cancel.store(true, Ordering::Relaxed);
            let (_, r) = run(&clip(&["b/x.jpg"], false), &op, &base, "a/", &HashMap::new(), &p).await;
            assert_eq!(r.unwrap_err().0, "error-cancelled");
            assert!(!dir.join("a/x.jpg").exists());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A photo's sidecars (#24) follow it: a conflict of a sidecar's name is one of the photo, Keep
    /// both gives them the same new name, in its own folder too; moved to another device (copied
    /// and deleted) or on the same one (renamed), replacing a stale sidecar there.
    #[test]
    fn sidecars_follow() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-transfer-xmp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["a", "b", "c", "d"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let w = |p: &str, t: &str| std::fs::write(dir.join(p), t).unwrap();
        w("a/p.CR2", "raw");
        w("a/p.CR2.xmp", "dt");
        w("a/p.xmp", "lr");
        w("a/q.jpg", "jpg");
        w("a/q.jpg.xmp", "dk");
        w("b/p.CR2", "old raw");
        w("b/q.jpg.xmp", "stray");
        w("d/p.CR2.xmp", "stale");
        let files = |d: &str| {
            let mut v: Vec<(String, String)> = std::fs::read_dir(dir.join(d)).unwrap().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap())).collect();
            v.sort();
            v
        };
        let pairs = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let base = format!("test-xmp-{}://", std::process::id());
        let clip = |paths: &[&str], cut, base: &str| Clip { op: op.clone(), base: base.to_string(), paths: paths.iter().map(|s| s.to_string()).collect(), cut, text: String::new() };
        rt.block_on(async {
            let c = clip(&["a/p.CR2", "a/q.jpg"], false, &base);
            assert_eq!(conflicts(&c, &op, &base, "b/").await.unwrap(), vec!["a/p.CR2", "a/q.jpg"]);
            let both = HashMap::from([("a/p.CR2".to_string(), Choice::KeepBoth), ("a/q.jpg".to_string(), Choice::KeepBoth)]);
            let p = Progress::default();
            run(&c, &op, &base, "b/", &both, &p).await.1.unwrap();
            assert_eq!(files("b"), pairs(&[("p (2).CR2", "raw"), ("p (2).CR2.xmp", "dt"), ("p (2).xmp", "lr"), ("p.CR2", "old raw"), ("q (2).jpg", "jpg"), ("q (2).jpg.xmp", "dk"), ("q.jpg.xmp", "stray")]));
            assert_eq!((p.files.load(Ordering::Relaxed), p.files_total.load(Ordering::Relaxed)), (2, 2), "a photo with its sidecars counts once");
            // In its own folder.
            run(&clip(&["a/q.jpg"], false, &base), &op, &base, "a/", &HashMap::new(), &Progress::default()).await.1.unwrap();
            assert!(files("a").contains(&("q (2).jpg.xmp".to_string(), "dk".to_string())));
            // Moved to another device: copied, then the originals deleted.
            let (gone, r) = run(&clip(&["a/p.CR2"], true, "elsewhere://"), &op, &base, "c/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(gone, vec!["a/p.CR2"]);
            assert_eq!(files("c"), pairs(&[("p.CR2", "raw"), ("p.CR2.xmp", "dt"), ("p.xmp", "lr")]));
            assert!(!files("a").iter().any(|(n, _)| n.starts_with("p.")), "{:?}", files("a"));
            // Moved on the same device over a stale sidecar: replaced.
            let c = clip(&["c/p.CR2"], true, &base);
            assert_eq!(conflicts(&c, &op, &base, "d/").await.unwrap(), vec!["c/p.CR2"]);
            run(&c, &op, &base, "d/", &HashMap::new(), &Progress::default()).await.1.unwrap();
            assert_eq!(files("d"), pairs(&[("p.CR2", "raw"), ("p.CR2.xmp", "dt"), ("p.xmp", "lr")]));
            assert!(files("c").is_empty());
            // Cancelled: nothing of the pair arrives.
            let p = Progress::default();
            p.cancel.store(true, Ordering::Relaxed);
            assert_eq!(run(&clip(&["d/p.CR2"], false, "elsewhere://"), &op, &base, "c/", &HashMap::new(), &p).await.1.unwrap_err().0, "error-cancelled");
            assert!(files("c").is_empty());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A folder merged into one of the same name replaces its files reading each destination
    /// folder once, not once per file (a paste of 300 files over SFTP took 241 s instead of 44);
    /// a folder the merge makes is not read at all.
    #[test]
    fn merge_reads_each_folder_once() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["a/f/g", "a/f/h", "b/f/h"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let files = ["f/1.jpg", "f/2.jpg", "f/3.jpg", "f/4.jpg", "f/g/5.jpg", "f/g/6.jpg", "f/h/7.jpg", "f/h/8.jpg"];
        for f in files {
            std::fs::write(dir.join("a").join(f), f.as_bytes()).unwrap();
            if !f.contains("/g/") {
                std::fs::write(dir.join("b").join(f), b"old").unwrap();
            }
        }
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-merge-a://".into(), paths: vec!["a/f/".into()], cut: false, text: String::new() };
        crate::source::FOLDERS_READ.with(|n| n.set(0));
        let (_, r) = rt.block_on(run(&clip, &op, "test-merge-b://", "b/", &HashMap::new(), &Progress::default()));
        r.unwrap();
        // "b/" for the plan, then "b/f/" and "b/f/h/" once each.
        assert_eq!(crate::source::FOLDERS_READ.with(|n| n.get()), 3);
        for f in files {
            assert_eq!(std::fs::read(dir.join("b").join(f)).unwrap(), f.as_bytes(), "{f}");
        }
        let hidden = |d: &str| std::fs::read_dir(dir.join(d)).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with('.')).count();
        assert_eq!(hidden("b/f") + hidden("b/f/g") + hidden("b/f/h"), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A folder's own trash comes along when it moves (to another device, or merged into a folder
    /// of the same name) and stays out of a copy; in a trash already at the destination nothing is
    /// replaced. The files in it were lost before 0.73.1.
    #[test]
    fn move_takes_the_trash_along() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-trash-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let put = |path: &str, data: &[u8]| {
            let f = dir.join(path);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, data).unwrap();
        };
        let trash = TRASH_DIR;
        for v in ["one/Holiday", "one/Copied", "two/A/Holiday", "two/C/Holiday"] {
            put(&format!("{v}/Denali.jpg"), b"denali");
            put(&format!("{v}/Iceberg.jpg"), b"iceberg");
            put(&format!("{v}/{trash}/Moth.jpg"), b"moth");
        }
        put(&format!("two/C/Holiday/{trash}/Album/p.jpg"), b"p");
        put("two/B/Holiday/Sunset.jpg", b"sunset");
        put(&format!("two/D/Holiday/{trash}/Moth.jpg"), b"old moth");
        put(&format!("two/D/Holiday/{trash}/Album/q.jpg"), b"q");
        let store = |sub: &str| Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.join(sub).to_str().unwrap())).unwrap());
        let (one, two) = (store("one"), store("two"));
        let (base1, base2) = (format!("test-trash1-{}://", std::process::id()), format!("test-trash2-{}://", std::process::id()));
        let clip = |op: &Store, base: &str, path: &str, cut| Clip { op: op.clone(), base: base.to_string(), paths: vec![path.to_string()], cut, text: String::new() };
        let read = |path: &str| std::fs::read(dir.join(path)).ok();
        rt.block_on(async {
            // A copy to another device: the trash stays behind.
            let (_, r) = run(&clip(&one, &base1, "Copied/", false), &two, &base2, "", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(read("two/Copied/Denali.jpg").as_deref(), Some(&b"denali"[..]));
            assert!(!dir.join(format!("two/Copied/{trash}")).exists(), "a copy leaves the trash out");

            // A move to another device: copied, then the original deleted, its trash came along.
            let p = Progress::default();
            let (gone, r) = run(&clip(&one, &base1, "Holiday/", true), &two, &base2, "E/", &HashMap::new(), &p).await;
            r.unwrap();
            assert_eq!(gone, vec!["Holiday/"]);
            assert_eq!(p.files_total.load(Ordering::Relaxed), 3);
            assert_eq!(read(&format!("two/E/Holiday/{trash}/Moth.jpg")).as_deref(), Some(&b"moth"[..]));
            assert!(!dir.join("one/Holiday").exists());

            // Merged into a folder of the same name on the same device: copied, not renamed.
            let (_, r) = run(&clip(&two, &base2, "A/Holiday/", true), &two, &base2, "B/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(read("two/B/Holiday/Sunset.jpg").as_deref(), Some(&b"sunset"[..]));
            assert_eq!(read(&format!("two/B/Holiday/{trash}/Moth.jpg")).as_deref(), Some(&b"moth"[..]));
            assert!(!dir.join("two/A/Holiday").exists());

            // Merged where a trash is already there: nothing in it replaced, "name (2)" instead.
            let (_, r) = run(&clip(&two, &base2, "C/Holiday/", true), &two, &base2, "D/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(read(&format!("two/D/Holiday/{trash}/Moth.jpg")).as_deref(), Some(&b"old moth"[..]));
            assert_eq!(read(&format!("two/D/Holiday/{trash}/Moth (2).jpg")).as_deref(), Some(&b"moth"[..]));
            assert_eq!(read(&format!("two/D/Holiday/{trash}/Album/q.jpg")).as_deref(), Some(&b"q"[..]));
            assert_eq!(read(&format!("two/D/Holiday/{trash}/Album (2)/p.jpg")).as_deref(), Some(&b"p"[..]));
            assert!(!dir.join("two/C/Holiday").exists());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A paste that does not fit (`/dev/full` behind the name written, as a full disk answers)
    /// leaves nothing (#90): a new file is gone; a file it was replacing stays as it was, also in
    /// a move, whose original stays too. (The new file written by `write_file` itself: a paste
    /// first claims its name, `claim`, and a link there is a name taken.)
    #[cfg(target_os = "linux")]
    #[test]
    fn failed_paste_leaves_nothing() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-paste-full-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(dir.join("a/x.jpg"), vec![7u8; 3 << 20]).unwrap();
        std::fs::write(dir.join("b/x.jpg"), b"old").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let names = |sub: &str| {
            let mut v: Vec<String> = std::fs::read_dir(dir.join(sub)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        };
        rt.block_on(async {
            // A new file.
            std::os::unix::fs::symlink("/dev/full", dir.join("b/new.jpg")).unwrap();
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tx.try_send(Ok(bytes::Bytes::from(vec![7u8; 3 << 20]))).unwrap();
            drop(tx);
            crate::replace::write_file(&op, "b/new.jpg", 3 << 20, rx, Some(false)).await.unwrap_err();
            assert_eq!(names("b"), ["x.jpg"]);
            // Replacing x.jpg, in a copy and in a move (another device: copied, then deleted).
            std::os::unix::fs::symlink("/dev/full", dir.join("b/.x.jpg.eyecansee-new")).unwrap();
            let clip = |cut| Clip { op: op.clone(), base: "test-full-a://".into(), paths: vec!["a/x.jpg".into()], cut, text: String::new() };
            for cut in [false, true] {
                let (gone, r) = run(&clip(cut), &op, "test-full-b://", "b/", &HashMap::new(), &Progress::default()).await;
                assert!(r.is_err() && gone.is_empty(), "{r:?} {gone:?}");
                assert_eq!(std::fs::read(dir.join("b/x.jpg")).unwrap(), b"old");
                assert_eq!(names("b"), ["x.jpg"], "the hidden new version gone (the link to /dev/full)");
                assert_eq!(std::fs::read(dir.join("a/x.jpg")).unwrap().len(), 3 << 20);
                std::os::unix::fs::symlink("/dev/full", dir.join("b/.x.jpg.eyecansee-new")).unwrap();
            }
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Issue #91: in a folder opened through gvfs a paste never replaces. A file answered
    /// "Replace", or one whose name came after the question, is kept both; a folder is not merged
    /// but kept both; a move on the same device too. Elsewhere Replace stays.
    // gvfs mounts are Linux's; the mount's name has a ':' no Windows path may hold.
    #[cfg(unix)]
    #[test]
    fn gvfs_paste_never_replaces() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-paste-gvfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dcim = "gvfs/mtp:host=Phone_ABCD1234EFGH/DCIM/";
        for d in ["src/f", &format!("{dcim}f")] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        for (f, data) in [("a.jpg", "new a"), ("b.jpg", "new b"), ("c.jpg", "new c"), ("f/1.jpg", "new 1")] {
            std::fs::write(dir.join("src").join(f), data).unwrap();
            std::fs::write(dir.join(dcim).join(f), "original").unwrap();
        }
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        assert!(!can_replace(&op, dcim));
        assert!(can_replace(&op, "src/"));
        let clip = |paths: &[&str], cut, base: &str| Clip { op: op.clone(), base: base.into(), paths: paths.iter().map(|s| s.to_string()).collect(), cut, text: String::new() };
        let replace = |paths: &[&str]| paths.iter().map(|p| (p.to_string(), Choice::Replace)).collect::<HashMap<_, _>>();
        let read = |f: &str| std::fs::read_to_string(dir.join(dcim).join(f)).unwrap();
        rt.block_on(async {
            // A copy from another device: a.jpg and the folder answered Replace, c.jpg not asked.
            let c = clip(&["src/a.jpg", "src/f/", "src/c.jpg"], false, "test-gvfs-other://");
            let (_, r) = run(&c, &op, "test-gvfs://", dcim, &replace(&["src/a.jpg", "src/f/"]), &Progress::default()).await;
            r.unwrap();
            assert_eq!((read("a.jpg"), read("a (2).jpg")), ("original".into(), "new a".into()));
            assert_eq!((read("f/1.jpg"), read("f (2)/1.jpg")), ("original".into(), "new 1".into()));
            assert_eq!((read("c.jpg"), read("c (2).jpg")), ("original".into(), "new c".into()));
            // A move on the same device: the file there stays, the one moved is kept beside it.
            let (gone, r) = run(&clip(&["src/b.jpg"], true, "test-gvfs://"), &op, "test-gvfs://", dcim, &replace(&["src/b.jpg"]), &Progress::default()).await;
            r.unwrap();
            assert_eq!(gone, vec!["src/b.jpg"]);
            assert_eq!((read("b.jpg"), read("b (2).jpg")), ("original".into(), "new b".into()));
            // Outside gvfs Replace replaces.
            let from = format!("{dcim}a.jpg");
            let (_, r) = run(&clip(&[&from], false, "test-gvfs://"), &op, "test-gvfs://", "src/", &replace(&[&from]), &Progress::default()).await;
            r.unwrap();
            assert_eq!(std::fs::read_to_string(dir.join("src/a.jpg")).unwrap(), "original");
            assert!(!dir.join("src/a (2).jpg").exists());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Names sent by a phone or a server are checked before they become paths at the destination
    /// (audit 2.3): an item whose path below the folder pasted is not made of names a path can
    /// hold is left out, `Progress::refused` names it for the message at the end, and a move keeps
    /// the folder that holds it. Names of this computer are not checked. (Copies from servers: `ftp.rs`, `sftp.rs`.)
    #[test]
    fn names_from_devices_checked() {
        let target = crate::ftp::Target { host: "names.invalid".into(), port: 21, user: String::new(), password: String::new(), secure: false };
        let server = Store::Ftp(crate::ftp::Server::shared(target));
        let p = Progress::default();
        for good in ["a.jpg", "sub/", "sub/b.jpg", "12:30.jpg", ".EyeCanSee-trash/x.jpg"] {
            assert!(allowed(&server, false, &format!("album/{good}"), good, &p), "{good}");
        }
        assert!(p.refused.lock().unwrap().is_empty());
        assert!(!left_below("album/", &p));
        for bad in ["../victim.txt", "..", "sub/../../x.jpg", "..\\victim.txt", "C:\\x.jpg", "sub//x", "a\nb.jpg", "/etc/x"] {
            assert!(!allowed(&server, false, &format!("album/{bad}"), bad, &p), "{bad:?}");
        }
        assert_eq!(p.refused.lock().unwrap().len(), 8);
        assert_eq!(p.refused.lock().unwrap()[..2], ["album/../victim.txt", "album/.."]);
        assert!(left_below("album/", &p) && !left_below("other/", &p) && !left_below("album/a.jpg", &p) && !left_below("", &p));
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&std::env::temp_dir().to_string_lossy())).unwrap());
        assert!(allowed(&local, false, "a\\b.jpg", "a\\b.jpg", &Progress::default()), "a name of this computer");
        assert!(allowed(&local, true, "a:b.jpg", "a:b.jpg", &Progress::default()), "a name of this computer");

        // A folder of Windows as the destination (audit 2.4): also the names Windows cannot write.
        let p = Progress::default();
        for good in ["a.jpg", "sub/", "sub/b.jpg", "CONSOLE.jpg"] {
            assert!(allowed(&server, true, &format!("album/{good}"), good, &p), "{good}");
        }
        for bad in ["Mare 12:30.jpg", "12:30/", "12:30/a.jpg", "sub/a.jpg.", "sub /a.jpg", "NUL.jpg", "why?.jpg", "../x.jpg"] {
            assert!(!allowed(&server, true, &format!("album/{bad}"), bad, &p), "{bad:?}");
        }
        assert_eq!(p.refused.lock().unwrap().join(", "), "album/Mare 12:30.jpg, album/12:30/, album/12:30/a.jpg, album/sub/a.jpg., album/sub /a.jpg, album/NUL.jpg, album/why?.jpg, album/../x.jpg");
        assert!(left_below("album/", &p));
    }

    /// Every file under `dir` with the name `n`, at any depth.
    fn audit_find(dir: &std::path::Path, n: &str) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else { return found };
        for e in entries.flatten() {
            let path = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                found.extend(audit_find(&path, n));
            } else if e.file_name() == n {
                found.push(path);
            }
        }
        found
    }

    /// Audit 2.1 (#194): a folder moved to another device (here: another `base`, as from a phone
    /// to the computer). A file that arrives in the folder after it was looked through (a photo
    /// taken while `DCIM/Camera/` moves) is not in the list, so it is not copied; it is not deleted
    /// either: only what was copied goes, and the folder stays with it, named at the end
    /// (`Progress::left`). Before, the folder was deleted whole, and the file with it.
    #[test]
    fn audit_2_1_move_deletes_a_file_that_arrived_meanwhile() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-audit-2-1-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("phone/DCIM/Camera")).unwrap();
        std::fs::create_dir_all(dir.join("pc")).unwrap();
        // Large enough that the copy is still running when the late photo arrives.
        std::fs::write(dir.join("phone/DCIM/Camera/a.jpg"), vec![1u8; 32 << 20]).unwrap();
        std::fs::write(dir.join("phone/DCIM/Camera/b.jpg"), b"b").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-phone://".into(), paths: vec!["phone/DCIM/Camera/".into()], cut: true, text: String::new() };
        let p = Progress::default();
        let late = dir.join("phone/DCIM/Camera/late.jpg");
        let (written, r) = std::thread::scope(|s| {
            // `started` is set once the folder has been looked through, before the first file.
            let writer = s.spawn(|| {
                while p.started.get().is_none() {
                    std::thread::yield_now();
                }
                std::fs::write(&late, b"taken while moving").is_ok()
            });
            let (_, r) = rt.block_on(run(&clip, &op, "test-pc://", "pc/", &HashMap::new(), &p));
            (writer.join().unwrap(), r)
        });
        assert!(written, "the late photo was written before the folder went (race lost: run again)");
        r.unwrap();
        assert!(dir.join("pc/Camera/a.jpg").exists() && dir.join("pc/Camera/b.jpg").exists(), "the files listed were copied");
        let left = audit_find(&dir, "late.jpg");
        let copied_gone = !dir.join("phone/DCIM/Camera/a.jpg").exists() && !dir.join("phone/DCIM/Camera/b.jpg").exists();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!left.is_empty(), "the photo that arrived during the move is gone: neither copied nor left in the source");
        assert!(copied_gone, "the files copied went from the source");
        assert_eq!(*p.left.lock().unwrap(), ["phone/DCIM/Camera/late.jpg"], "named at the end");
    }

    /// Audit 2.2, scenario A (#195): a folder cut and pasted into a folder reached through a link
    /// that leads inside it (`shortcut` → `Photos/album/sub`). Refused, as by the paths: before,
    /// the check compared only the text of the paths, the rename failed, the files were copied
    /// into `album/sub/album/`, then `album/` was deleted whole, the copy with it.
    #[cfg(unix)]
    #[test]
    fn audit_2_2_a_move_into_itself_through_a_link() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-audit-2-2a-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/album/sub")).unwrap();
        std::fs::write(dir.join("Photos/album/x.jpg"), b"the only copy").unwrap();
        std::os::unix::fs::symlink(dir.join("Photos/album/sub"), dir.join("shortcut")).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let base = "test-link://";
        let clip = Clip { op: op.clone(), base: base.into(), paths: vec!["Photos/album/".into()], cut: true, text: String::new() };
        let (refused, r) = rt.block_on(async {
            match conflicts(&clip, &op, base, "shortcut/").await {
                Err(e) => (Some(e.0), None),
                Ok(_) => (None, Some(run(&clip, &op, base, "shortcut/", &HashMap::new(), &Progress::default()).await.1)),
            }
        });
        let left = audit_find(&dir, "x.jpg");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!left.is_empty(), "x.jpg is gone (refused: {refused:?}, paste: {r:?})");
        assert_eq!(refused, Some("error-paste-inside"), "paste: {r:?}");
    }

    /// Audit 2.2, scenario B (#195): the same folder reached as two sources (here two local ones;
    /// the same server opened by SFTP and by FTP, or a NAS through gvfs and through SFTP, are
    /// the same case where the real paths cannot be compared): two `base`s. Moving `album/` into
    /// `album/sub/` is refused by the real paths; before, it was copied into itself, then
    /// `album/` was deleted whole, the copy with it.
    #[test]
    fn audit_2_2_b_move_into_itself_from_another_source() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-audit-2-2b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/album/sub")).unwrap();
        std::fs::write(dir.join("Photos/album/x.jpg"), b"the only copy").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-sftp://box/".into(), paths: vec!["Photos/album/".into()], cut: true, text: String::new() };
        let to_base = "test-ftp://box/";
        let (refused, r) = rt.block_on(async {
            match conflicts(&clip, &op, to_base, "Photos/album/sub/").await {
                Err(e) => (Some(e.0), None),
                Ok(_) => (None, Some(run(&clip, &op, to_base, "Photos/album/sub/", &HashMap::new(), &Progress::default()).await.1)),
            }
        });
        let left = audit_find(&dir, "x.jpg");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!left.is_empty(), "x.jpg is gone (refused: {refused:?}, paste: {r:?})");
        assert_eq!(refused, Some("error-paste-inside"), "paste: {r:?}");
    }

    /// #195: files pasted into their own folder reached from another source (two `base`s, the
    /// same real folder): not conflicts; a copy keeps both, a move does nothing. Before, a move
    /// answered Replace wrote the file over itself and then deleted it as the original.
    #[test]
    fn own_folder_from_another_source() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-own-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/sub")).unwrap();
        std::fs::write(dir.join("Photos/a.jpg"), b"the only copy").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = |cut| Clip { op: op.clone(), base: "test-one://".into(), paths: vec!["Photos/a.jpg".into(), "Photos/sub/".into()], cut, text: String::new() };
        let replace = HashMap::from([("Photos/a.jpg".to_string(), Choice::Replace), ("Photos/sub/".to_string(), Choice::Replace)]);
        rt.block_on(async {
            for cut in [true, false] {
                assert!(conflicts(&clip(cut), &op, "test-two://", "Photos/").await.unwrap().is_empty());
                let (gone, r) = run(&clip(cut), &op, "test-two://", "Photos/", &replace, &Progress::default()).await;
                r.unwrap();
                assert!(gone.is_empty());
                assert_eq!(std::fs::read(dir.join("Photos/a.jpg")).unwrap(), b"the only copy");
                assert!(dir.join("Photos/sub").is_dir());
            }
            assert_eq!(std::fs::read(dir.join("Photos/a (2).jpg")).unwrap(), b"the only copy", "a copy keeps both");
            assert!(dir.join("Photos/sub (2)").is_dir());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #194: a folder moved to another device that holds a link to a folder outside it (followed by
    /// the walk on this computer): what the link leads to is copied, the link goes, the folder it
    /// leads to stays whole. Before, the folder was deleted whole at the end.
    #[cfg(unix)]
    #[test]
    fn move_removes_a_link_not_what_it_leads_to() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-move-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["album/sub", "elsewhere", "pc"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("album/a.jpg"), b"a").unwrap();
        std::fs::write(dir.join("album/sub/b.jpg"), b"b").unwrap();
        std::fs::write(dir.join("elsewhere/c.jpg"), b"c").unwrap();
        std::os::unix::fs::symlink(dir.join("elsewhere"), dir.join("album/link")).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-a://".into(), paths: vec!["album/".into()], cut: true, text: String::new() };
        let p = Progress::default();
        let (gone, r) = rt.block_on(run(&clip, &op, "test-b://", "pc/", &HashMap::new(), &p));
        r.unwrap();
        assert_eq!(gone, ["album/"]);
        assert!(!dir.join("album").exists());
        assert_eq!(std::fs::read(dir.join("elsewhere/c.jpg")).unwrap(), b"c", "what the link leads to stays");
        for f in ["a.jpg", "sub/b.jpg", "link/c.jpg"] {
            assert!(dir.join("pc/album").join(f).exists(), "{f}");
        }
        assert!(p.left.lock().unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #204: a new file of this computer is written only onto a free name: one there that the
    /// paste did not see (here a file told as not there, as one with other capitals on Windows)
    /// is not written over, nor renamed over by a move on the same disk.
    #[test]
    fn new_file_never_written_over() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-claim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::fs::write(dir.join("a/x.jpg"), b"new").unwrap();
        std::fs::write(dir.join("b/x.jpg"), b"there").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        rt.block_on(async {
            let Err(Fail::Stop(e)) = copy_file(&op, "a/x.jpg", (&op, "b/x.jpg", Some(false)), 3, &Progress::default()).await else { panic!("the copy must stop") };
            assert_eq!(e.0, "error-exists");
            let clip = Clip { op: op.clone(), base: "test-claim://".into(), paths: vec!["a/x.jpg".into()], cut: true, text: String::new() };
            let planned = ("a/x.jpg".to_string(), "b/x.jpg".to_string(), false, Vec::new());
            let e = renamed_on_device(&clip, &op, "b/", planned, &mut Vec::new()).await.unwrap_err();
            assert_eq!(e.0, "error-exists");
        });
        assert_eq!(std::fs::read(dir.join("b/x.jpg")).unwrap(), b"there");
        assert_eq!(std::fs::read(dir.join("a/x.jpg")).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #204: where the destination ignores capitals, a name with other capitals is the item there:
    /// Replace writes over it under its own name, Keep both takes a free name; a new name is never
    /// one there with other capitals, also where capitals count.
    #[test]
    fn plan_with_other_capitals() {
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&std::env::temp_dir().to_string_lossy())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-a://".into(), paths: vec!["c/IMG_001.JPG".into(), "c/Album/".into()], cut: false, text: String::new() };
        let present = |folds| Names::new(["img_001.jpg", "album"].map(String::from).into(), folds);
        let plan = |choice, folds| {
            let choices = HashMap::from([("c/IMG_001.JPG".to_string(), choice), ("c/Album/".to_string(), choice)]);
            planned(&clip, &op, "p/", &HashSet::new(), &choices, &present(folds), &HashMap::new()).into_iter().map(|(_, d, e, _)| (d, e)).collect::<Vec<_>>()
        };
        assert_eq!(plan(Choice::Replace, true), [("p/img_001.jpg".to_string(), true), ("p/album/".to_string(), true)]);
        assert_eq!(plan(Choice::KeepBoth, true), [("p/IMG_001 (2).JPG".to_string(), false), ("p/Album (2)/".to_string(), false)]);
        assert_eq!(plan(Choice::Replace, false), [("p/IMG_001.JPG".to_string(), false), ("p/Album/".to_string(), false)]);
        assert_eq!(plan(Choice::KeepBoth, false), [("p/IMG_001 (2).JPG".to_string(), false), ("p/Album (2)/".to_string(), false)]);
    }

    /// #195: a move onto itself between two sources whose real places cannot be compared (here two
    /// local ones with the comparison off, as a server through SFTP and through FTP), answered
    /// Replace or Merge: the original must not be deleted as such, nor written over itself first
    /// (L8 of 2026-11-21: content intact, its time changed). The move stops before the copy,
    /// saying so; everything stays as it was, the same files with the same times.
    #[test]
    fn move_onto_itself_where_places_are_not_known() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = std::env::temp_dir().join(format!("eyecansee-same-item-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/sub")).unwrap();
        std::fs::write(dir.join("Photos/a.jpg"), b"the only copy").unwrap();
        std::fs::write(dir.join("Photos/sub/b.jpg"), b"the only b").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let replace = HashMap::from([("Photos/a.jpg".to_string(), Choice::Replace), ("Photos/sub/".to_string(), Choice::Replace)]);
        // Each file as it is: the same file (inode) and the same time.
        let stamp = |f: &str| {
            let m = std::fs::metadata(dir.join(f)).unwrap();
            #[cfg(unix)]
            let id = std::os::unix::fs::MetadataExt::ino(&m);
            #[cfg(not(unix))]
            let id = 0;
            (id, m.modified().unwrap())
        };
        let before = [stamp("Photos/a.jpg"), stamp("Photos/sub/b.jpg")];
        std::thread::sleep(std::time::Duration::from_millis(20));
        COMPARE_PLACES.with(|c| c.set(false));
        rt.block_on(async {
            for item in ["Photos/a.jpg", "Photos/sub/"] {
                let clip = Clip { op: op.clone(), base: "test-sftp://box/".into(), paths: vec![item.into()], cut: true, text: String::new() };
                assert_eq!(conflicts(&clip, &op, "test-ftp://box/", "Photos/").await.unwrap(), [item], "asked, as a name there");
                let (gone, r) = run(&clip, &op, "test-ftp://box/", "Photos/", &replace, &Progress::default()).await;
                assert_eq!(r.unwrap_err(), ("error-move-same-item", crate::source::name(item).to_string()));
                assert!(gone.is_empty());
            }
        });
        COMPARE_PLACES.with(|c| c.set(true));
        assert_eq!(std::fs::read(dir.join("Photos/a.jpg")).unwrap(), b"the only copy");
        assert_eq!(std::fs::read(dir.join("Photos/sub/b.jpg")).unwrap(), b"the only b");
        assert_eq!([stamp("Photos/a.jpg"), stamp("Photos/sub/b.jpg")], before, "not written over itself");
        let mut names: Vec<String> = std::fs::read_dir(dir.join("Photos")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["a.jpg", "sub"], "nothing left aside");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Audit F.1 of 27/11 (#195): a folder cut and merged into a link of the same name that leads
    /// to it (`Desk/album` → `Photos/album`). Each file was written over itself through the link
    /// and then deleted as the original: the folder went whole, the link was left leading nowhere.
    #[cfg(unix)]
    #[test]
    fn f1_folder_merged_into_a_link_to_itself() {
        let dir = std::env::temp_dir().join(format!("eyecansee-f1-top-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/album")).unwrap();
        std::fs::create_dir_all(dir.join("Desk")).unwrap();
        std::fs::write(dir.join("Photos/album/x.jpg"), b"the only copy").unwrap();
        std::os::unix::fs::symlink("../Photos/album", dir.join("Desk/album")).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-f1://".into(), paths: vec!["Photos/album/".into()], cut: true, text: String::new() };
        let replace = HashMap::from([("Photos/album/".to_string(), Choice::Replace)]);
        let (gone, r) = rt.block_on(run(&clip, &op, "test-f1://", "Desk/", &replace, &Progress::default()));
        let x = std::fs::read(dir.join("Photos/album/x.jpg")).ok();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(x.as_deref(), Some(&b"the only copy"[..]), "x.jpg is gone (paste: {r:?}, gone: {gone:?})");
        assert_eq!(r.map_err(|e| e.0), Err("error-move-same-item"));
        assert!(gone.is_empty(), "{gone:?}");
    }

    /// Audit F.1 of 27/11 (#195), a level below: the folder of arrival is a real one, but in it
    /// `2024` is a link to the `2024` of the folder moved. Its files were written over themselves
    /// and deleted as the originals.
    #[cfg(unix)]
    #[test]
    fn f1_folder_below_merged_into_a_link_to_itself() {
        let dir = std::env::temp_dir().join(format!("eyecansee-f1-below-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/album/2024")).unwrap();
        std::fs::create_dir_all(dir.join("Desk/album")).unwrap();
        std::fs::write(dir.join("Photos/album/a.jpg"), b"a").unwrap();
        std::fs::write(dir.join("Photos/album/2024/y.jpg"), b"the only y").unwrap();
        std::os::unix::fs::symlink("../../Photos/album/2024", dir.join("Desk/album/2024")).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-f1://".into(), paths: vec!["Photos/album/".into()], cut: true, text: String::new() };
        let replace = HashMap::from([("Photos/album/".to_string(), Choice::Replace)]);
        let (gone, r) = rt.block_on(run(&clip, &op, "test-f1://", "Desk/", &replace, &Progress::default()));
        let y = std::fs::read(dir.join("Photos/album/2024/y.jpg")).ok();
        let a = std::fs::read(dir.join("Photos/album/a.jpg")).ok();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(y.as_deref(), Some(&b"the only y"[..]), "y.jpg is gone (paste: {r:?}, gone: {gone:?})");
        assert_eq!(a.as_deref(), Some(&b"a"[..]), "the original of a.jpg went with the move stopped");
        assert_eq!(r.map_err(|e| e.0), Err("error-move-same-item"));
        assert!(gone.is_empty(), "{gone:?}");
    }

    /// Audit F.1 of 27/11 (#195), a file a level below: in the real folder of arrival, `x.jpg` is a
    /// link to the `x.jpg` of the folder moved. Written over itself through the link, then deleted
    /// as the original.
    #[cfg(unix)]
    #[test]
    fn f1_file_below_merged_into_a_link_to_itself() {
        let dir = std::env::temp_dir().join(format!("eyecansee-f1-file-below-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/album")).unwrap();
        std::fs::create_dir_all(dir.join("Desk/album")).unwrap();
        std::fs::write(dir.join("Photos/album/x.jpg"), b"the only copy").unwrap();
        std::os::unix::fs::symlink("../../Photos/album/x.jpg", dir.join("Desk/album/x.jpg")).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let clip = Clip { op: op.clone(), base: "test-f1://".into(), paths: vec!["Photos/album/".into()], cut: true, text: String::new() };
        let replace = HashMap::from([("Photos/album/".to_string(), Choice::Replace)]);
        let (gone, r) = rt.block_on(run(&clip, &op, "test-f1://", "Desk/", &replace, &Progress::default()));
        let x = std::fs::read(dir.join("Photos/album/x.jpg")).ok();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(x.as_deref(), Some(&b"the only copy"[..]), "x.jpg is gone (paste: {r:?}, gone: {gone:?})");
        assert_eq!(r.map_err(|e| e.0), Err("error-move-same-item"));
        assert!(gone.is_empty(), "{gone:?}");
    }

    /// Audit F.1 of 27/11 (#195): a file cut on one disk and pasted, Replace, onto a link of the
    /// same name on another disk that leads to it. The rename fails (two file systems), so the file
    /// was copied: written over itself through the link, then deleted as the original. Here the
    /// second disk is `/dev/shm` (Linux), with a store rooted at `/` that holds both.
    #[cfg(target_os = "linux")]
    #[test]
    fn f1_file_on_two_disks_replacing_a_link_to_itself() {
        use std::os::unix::fs::MetadataExt;
        let shm = std::path::PathBuf::from(format!("/dev/shm/eyecansee-f1-file-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("eyecansee-f1-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&shm);
        if std::fs::create_dir_all(&shm).is_err() {
            // No /dev/shm here: no second disk to try.
            return;
        }
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dev = |p: &std::path::Path| std::fs::metadata(p).unwrap().dev();
        if dev(&shm) == dev(&dir) {
            // One disk: the rename itself replaces the link, nothing to try.
            let _ = std::fs::remove_dir_all(&shm);
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        std::fs::write(shm.join("x.jpg"), b"the only copy").unwrap();
        std::os::unix::fs::symlink(shm.join("x.jpg"), dir.join("x.jpg")).unwrap();
        let rel = |p: &std::path::Path| p.to_str().unwrap().trim_start_matches('/').to_string();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root("/")).unwrap());
        let src = format!("{}/x.jpg", rel(&shm));
        let clip = Clip { op: op.clone(), base: "test-f1://".into(), paths: vec![src.clone()], cut: true, text: String::new() };
        let replace = HashMap::from([(src, Choice::Replace)]);
        let (gone, r) = rt.block_on(run(&clip, &op, "test-f1://", &format!("{}/", rel(&dir)), &replace, &Progress::default()));
        let x = std::fs::read(shm.join("x.jpg")).ok();
        let _ = std::fs::remove_dir_all(&shm);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(x.as_deref(), Some(&b"the only copy"[..]), "x.jpg is gone (paste: {r:?}, gone: {gone:?})");
        assert_eq!(r.map_err(|e| e.0), Err("error-move-same-item"));
        assert!(gone.is_empty(), "{gone:?}");
    }
}
