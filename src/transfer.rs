//! Copy and move (Ctrl+C or Ctrl+X, then Ctrl+V) between any two sources: the same folder, two
//! folders of a disk, a phone and the computer, two phones. Files travel in chunks from the
//! reader to the writer, so a large video never sits whole in memory.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::source::{Deletion, Error, Store, TRASH_DIR, free_name, name, parent};
use crate::thumbcache;

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

/// Shared with the UI, which shows it while the copy runs.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub files: AtomicUsize,
    pub files_total: AtomicUsize,
    /// Items found while looking inside the folders, before the copy starts.
    pub found: AtomicUsize,
    /// Destination folders that got something new since the UI last looked: it reads them again,
    /// so a copy shows up while it runs.
    pub touched: Mutex<Vec<String>>,
    /// Seconds spent looking, for the log.
    pub looked: Mutex<f64>,
    /// When the copy itself started, after looking: the UI works out speed and time left from it.
    pub started: OnceLock<Instant>,
    /// Name of the file being copied.
    pub current: Mutex<String>,
    pub cancel: AtomicBool,
    /// The speed of the last seconds, as the UI shows it.
    pub recent: Mutex<Recent>,
    /// Items of a phone or a server left out because of their name (`allowed`), as found: an
    /// error at the end of the copy, and a move keeps the folders that hold them.
    pub refused: Mutex<Vec<String>>,
}

/// Bytes copied over the last `SPAN` seconds: the speed of the moment, as a system network meter
/// shows it, which moves with the line; the average since the start gives the time left.
#[derive(Default)]
pub struct Recent(std::collections::VecDeque<(f64, u64)>);

impl Recent {
    const SPAN: f64 = 4.0;

    /// `done` bytes at `now` seconds; bytes per second once a second has been seen.
    pub fn speed(&mut self, now: f64, done: u64) -> Option<f64> {
        self.0.push_back((now, done));
        while self.0.front().is_some_and(|(t, _)| now - t > Self::SPAN) {
            self.0.pop_front();
        }
        let (t0, d0) = *self.0.front()?;
        (now - t0 >= 1.0).then(|| done.saturating_sub(d0) as f64 / (now - t0))
    }
}

/// Items whose name is already in `to_dir`, to ask about. A folder pasted inside itself is an error.
/// Items pasted into their own folder are not conflicts: a copy becomes "name (2)", a move does nothing.
/// A photo whose sidecar (`sidecar_follow`) would land on a file with that name is one too: the
/// sidecar follows the answer for the photo.
pub async fn conflicts(clip: &Clip, to: &Store, to_base: &str, to_dir: &str) -> Result<Vec<String>, Error> {
    let same = clip.base == to_base;
    let taken = to.names(to_dir).await?;
    let sides = sidecars(clip).await;
    let mut found = Vec::new();
    for p in &clip.paths {
        if same && p.ends_with('/') && to_dir.starts_with(p.as_str()) {
            return Err(("error-paste-inside", name(p).to_string()));
        }
        let side_taken = sides.get(p).is_some_and(|s| s.iter().any(|s| taken.contains(name(s))));
        if !(same && parent(p) == to_dir) && (taken.contains(name(p)) || side_taken) {
            found.push(p.clone());
        }
    }
    Ok(found)
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

/// Copies (or moves) the clip into `to_dir`. Returns the source items that went away (moved),
/// also when it stops at an error halfway.
pub async fn run(clip: &Clip, to: &Store, to_base: &str, to_dir: &str, choices: &HashMap<String, Choice>, p: &Progress) -> (Vec<String>, Result<(), Error>) {
    let mut gone = Vec::new();
    let mut clip = clip.clone();
    clip.paths.retain(|src| allowed(&clip.op, to_windows(to), src, name(src), p));
    let r = paste(&clip, to, to_base, to_dir, choices, p, &mut gone).await;
    (gone, r.and_then(|()| refused(p)))
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

/// A copy that left items out (`allowed`) is not one that worked: an error naming them, once the
/// rest has been copied.
fn refused(p: &Progress) -> Result<(), Error> {
    let left = p.refused.lock().unwrap();
    if left.is_empty() { Ok(()) } else { Err(("error-name-refused", left.join(", "))) }
}

/// Whether something below the folder `src` was left out (`allowed`): its original stays.
fn left_below(src: &str, p: &Progress) -> bool {
    src.ends_with('/') && p.refused.lock().unwrap().iter().any(|r| r.starts_with(src))
}

async fn paste(clip: &Clip, to: &Store, to_base: &str, to_dir: &str, choices: &HashMap<String, Choice>, p: &Progress, gone: &mut Vec<String>) -> Result<(), Error> {
    let same = clip.base == to_base;
    let present = to.names(to_dir).await?;
    let plan = planned(clip, to, to_dir, same, choices, &present, &sidecars(clip).await);

    // On the same device a move is a rename, unless a folder has to merge into an existing one;
    // if the rename fails (another disk: different file system) the files are copied. On this
    // computer the rename itself replaces a file: one that is there stays until then, and still
    // there if the copy fails (#90). Phones, servers and gvfs rename only onto a free name.
    let mut copies = Vec::new();
    for (src, dest, exists, sides) in plan {
        if clip.cut && same && !(exists && dest.ends_with('/')) {
            if let Some(entry) = renamed_on_device(clip, to, to_dir, (src, dest, exists, sides), gone).await? {
                copies.push(entry);
            }
            continue;
        }
        copies.push((src, dest, exists, sides));
    }

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
    // Counted as the files chosen: a photo with its sidecars is one.
    let files = || lists.iter().zip(&copies).flat_map(|(l, c)| &l[..l.len() - c.3.len()]).filter(|(s, _, _)| !s.ends_with('/'));
    p.files_total.store(files().count(), Ordering::Relaxed);
    p.total.store(lists.iter().flatten().filter(|(s, _, _)| !s.ends_with('/')).map(|(_, _, size)| size).sum(), Ordering::Relaxed);
    *p.looked.lock().unwrap() = start.elapsed().as_secs_f64();
    let _ = p.started.set(Instant::now());

    // The names in each destination folder of a merge, read once (`Present`).
    let mut there = Present::new(to_dir, present);
    for ((src, _, exists, sides), list) in copies.iter().zip(lists) {
        let merging = src.ends_with('/') && *exists;
        let photos = list.len() - sides.len();
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
                copy_file(&clip.op, &from, (to, &dest, existed), size, p).await?;
                keep_thumbnail(thumbcache::stored(&clip.base, &from), &dest, Some(size));
                if i < photos {
                    p.files.fetch_add(1, Ordering::Relaxed);
                }
            }
            p.touched.lock().unwrap().push(parent(&dest));
        }
        // Moved between devices: the original goes only once all of it has been copied, its
        // sidecars before it (stopped between the two, the photo stays with no sidecar of its
        // own left behind). A folder with an item left out because of its name stays whole.
        if clip.cut && !left_below(src, p) {
            for (s, _, _) in sides {
                clip.op.delete(s, Deletion::Permanent).await?;
            }
            if !src.is_empty() {
                clip.op.delete(src, Deletion::Permanent).await?;
                gone.push(src.clone());
            }
        }
    }
    Ok(())
}

/// What goes where (`Planned`), as the answers to the conflicts (`choices`) say; `present`: the
/// names in `to_dir`; `sidecars`: those of the photos, which follow them under their new name
/// ("a (2).CR2" with "a (2).CR2.xmp"; `sidecar::free_pair`).
fn planned(clip: &Clip, to: &Store, to_dir: &str, same: bool, choices: &HashMap<String, Choice>, present: &HashSet<String>, sidecars: &HashMap<String, Vec<String>>) -> Vec<Planned> {
    let mut taken = present.clone();
    let mut plan = Vec::new();
    for src in &clip.paths {
        let own = name(src);
        let sides: Vec<String> = sidecars.get(src).map_or_else(Vec::new, |s| s.iter().map(|s| name(s).to_string()).collect());
        let choice = match (same && parent(src) == to_dir, clip.cut) {
            (true, true) => continue,
            (true, false) => Choice::KeepBoth,
            _ => choices.get(src).copied().unwrap_or(Choice::Replace),
        };
        // Under gvfs nothing is replaced, not even an item that came after the question: both stay.
        let busy = crate::sidecar::free_pair(&taken, own, &sides) != own;
        let choice = if choice == Choice::Replace && busy && !can_replace(to, to_dir) { Choice::KeepBoth } else { choice };
        let dest_name = match choice {
            Choice::Skip => continue,
            Choice::KeepBoth => crate::sidecar::free_pair(&taken, own, &sides),
            Choice::Replace => own.to_string(),
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

/// A move on the same device, as a rename, the photo's sidecars after it (`paste`). What a rename
/// cannot move is returned to be copied and deleted: the photo with its sidecars, or the sidecars
/// alone once the photo moved.
async fn renamed_on_device(clip: &Clip, to: &Store, to_dir: &str, (src, dest, mut exists, sides): Planned, gone: &mut Vec<String>) -> Result<Option<Planned>, Error> {
    // Phones, servers and gvfs rename only onto a free name: what is there goes first.
    let overwrites = |d: &str| to.local_file(d).is_none() || to.gvfs(d);
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
struct Present(HashMap<String, HashSet<String>>);

impl Present {
    /// Starting with the names of `dir`, read already.
    fn new(dir: &str, names: HashSet<String>) -> Self {
        Present(HashMap::from([(dir.to_string(), names)]))
    }

    /// Is there an item at `path`? `None` if its folder cannot be read (`write_file` asks then).
    async fn has(&mut self, to: &Store, path: &str) -> Option<bool> {
        let dir = parent(path);
        if !self.0.contains_key(&dir) {
            self.0.insert(dir.clone(), to.names(&dir).await.ok()?);
        }
        Some(self.0[&dir].contains(name(path)))
    }

    /// The folder `dir` is about to be made, if it is not there: it starts empty.
    async fn made(&mut self, to: &Store, dir: &str) {
        if self.has(to, dir).await == Some(false) {
            self.0.insert(dir.to_string(), HashSet::new());
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

/// A file of `size` bytes from `src` of `from` to `dest` of `to`; `existed`: whether something is
/// already there (`replace::write_file`): a copy that fails leaves nothing, and a file it was
/// replacing as it was (#90).
async fn copy_file(from: &Store, src: &str, (to, dest, existed): (&Store, &str, Option<bool>), size: u64, p: &Progress) -> Result<(), Error> {
    // Inside the same phone: copied by the phone itself. Through the computer the upload would
    // hold the phone while waiting for the download of the same file from it.
    if let (Store::Mtp(a), Store::Mtp(b)) = (from, to)
        && std::sync::Arc::ptr_eq(a, b)
    {
        a.copy_inside(src, dest).await?;
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
    let (read, write) = futures_util::future::join(from.send_chunks(src, size, tx, &p.done, cancel), crate::replace::write_file(to, dest, size, rx, existed)).await;
    drop(writing);
    if to_phone && read.is_ok() && write.is_ok() && p.cancel.load(Ordering::Relaxed) {
        p.files.fetch_add(1, Ordering::Relaxed);
        return Err(("error-cancelled", String::new()));
    }
    match (read, write) {
        // When the writer stops, the reader sees a closed channel: the writer's error is the real one.
        (Err(e), _) if p.cancel.load(Ordering::Relaxed) => Err(e),
        (_, Err(e)) | (Err(e), Ok(())) => Err(e),
        (Ok(()), Ok(())) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The speed of the last 4 seconds, not of the whole copy.
    #[test]
    fn recent_speed() {
        let mut r = Recent::default();
        assert_eq!(r.speed(0.0, 0), None);
        assert_eq!(r.speed(0.5, 10), None);
        assert_eq!(r.speed(2.0, 40), Some(20.0));
        // Fast at first, then slow: only the slow part counts after 4 seconds.
        assert_eq!(r.speed(10.0, 50), None);
        assert_eq!(r.speed(12.0, 70), Some(10.0));
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
            let (gone, r) = run(&clip(&["a/x.jpg"], true), &op, &base, "b/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(gone, vec!["a/x.jpg"]);
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
    /// a move, whose original stays too.
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
            let p = Progress::default();
            copy_file(&op, "a/x.jpg", (&op, "b/new.jpg", Some(false)), 3 << 20, &p).await.unwrap_err();
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
    /// hold is left out, the copy ends with an error naming it, and a move keeps the folder that
    /// holds it. Names of this computer are not checked. (Copies from servers: `ftp.rs`, `sftp.rs`.)
    #[test]
    fn names_from_devices_checked() {
        let target = crate::ftp::Target { host: "names.invalid".into(), port: 21, user: String::new(), password: String::new(), secure: false };
        let server = Store::Ftp(crate::ftp::Server::shared(target));
        let p = Progress::default();
        for good in ["a.jpg", "sub/", "sub/b.jpg", "12:30.jpg", ".EyeCanSee-trash/x.jpg"] {
            assert!(allowed(&server, false, &format!("album/{good}"), good, &p), "{good}");
        }
        assert_eq!(refused(&p), Ok(()));
        assert!(!left_below("album/", &p));
        for bad in ["../victim.txt", "..", "sub/../../x.jpg", "..\\victim.txt", "C:\\x.jpg", "sub//x", "a\nb.jpg", "/etc/x"] {
            assert!(!allowed(&server, false, &format!("album/{bad}"), bad, &p), "{bad:?}");
        }
        assert_eq!(p.refused.lock().unwrap().len(), 8);
        assert_eq!(refused(&p).unwrap_err().0, "error-name-refused");
        assert!(refused(&p).unwrap_err().1.starts_with("album/../victim.txt, album/.., "));
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
        assert_eq!(refused(&p).unwrap_err().1, "album/Mare 12:30.jpg, album/12:30/, album/12:30/a.jpg, album/sub/a.jpg., album/sub /a.jpg, album/NUL.jpg, album/why?.jpg, album/../x.jpg");
        assert!(left_below("album/", &p));
    }
}
