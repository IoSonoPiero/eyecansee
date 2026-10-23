//! Copy and move (Ctrl+C or Ctrl+X, then Ctrl+V) between any two sources: the same folder, two
//! folders of a disk, a phone and the computer, two phones. Files travel in chunks from the
//! reader to the writer, so a large video never sits whole in memory.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::source::{Deletion, Error, Store, free_name, name, parent};

/// What Ctrl+C or Ctrl+X took: the source it came from, which can be different from the one
/// open when pasting.
#[derive(Clone)]
pub struct Clip {
    pub op: Store,
    /// `base` of the source: the same `base` means the same device.
    pub base: String,
    pub paths: Vec<String>,
    pub cut: bool,
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
}

/// Items whose name is already in `to_dir`, to ask about. A folder pasted inside itself is an error.
/// Items pasted into their own folder are not conflicts: a copy becomes "name (2)", a move does nothing.
pub async fn conflicts(clip: &Clip, to: &Store, to_base: &str, to_dir: &str) -> Result<Vec<String>, Error> {
    let same = clip.base == to_base;
    let taken = to.names(to_dir).await?;
    let mut found = Vec::new();
    for p in &clip.paths {
        if same && p.ends_with('/') && to_dir.starts_with(p.as_str()) {
            return Err(("error-paste-inside", name(p).to_string()));
        }
        if !(same && parent(p) == to_dir) && taken.contains(name(p)) {
            found.push(p.clone());
        }
    }
    Ok(found)
}

/// Copies (or moves) the clip into `to_dir`. Returns the source items that went away (moved),
/// also when it stops at an error halfway.
pub async fn run(clip: &Clip, to: &Store, to_base: &str, to_dir: &str, choices: &HashMap<String, Choice>, p: &Progress) -> (Vec<String>, Result<(), Error>) {
    let mut gone = Vec::new();
    let r = paste(clip, to, to_base, to_dir, choices, p, &mut gone).await;
    (gone, r)
}

async fn paste(clip: &Clip, to: &Store, to_base: &str, to_dir: &str, choices: &HashMap<String, Choice>, p: &Progress, gone: &mut Vec<String>) -> Result<(), Error> {
    let same = clip.base == to_base;
    let present = to.names(to_dir).await?;
    let mut taken = present.clone();
    // (source, destination, there is already an item with that name)
    let mut plan = Vec::new();
    for src in &clip.paths {
        let own = name(src);
        let choice = match (same && parent(src) == to_dir, clip.cut) {
            (true, true) => continue,
            (true, false) => Choice::KeepBoth,
            _ => choices.get(src).copied().unwrap_or(Choice::Replace),
        };
        let dest_name = match choice {
            Choice::Skip => continue,
            Choice::KeepBoth => free_name(&taken, own),
            Choice::Replace => own.to_string(),
        };
        taken.insert(dest_name.clone());
        let slash = if src.ends_with('/') { "/" } else { "" };
        plan.push((src.clone(), format!("{to_dir}{dest_name}{slash}"), present.contains(&dest_name)));
    }

    // On the same device a move is a rename, unless a folder has to merge into an existing one;
    // if the rename fails (another disk: different file system) the files are copied.
    let mut copies = Vec::new();
    for (src, dest, exists) in plan {
        if clip.cut && same && !(exists && dest.ends_with('/')) {
            if exists {
                to.delete(&dest, Deletion::Permanent).await?;
            }
            if clip.op.move_into(&src, to_dir, name(&dest)).await.is_ok() {
                gone.push(src);
                continue;
            }
        }
        copies.push((src, dest));
    }

    // Everything to copy, with sizes for the progress bar.
    let start = Instant::now();
    let mut lists = Vec::new();
    for (src, dest) in &copies {
        let list = if src.ends_with('/') {
            let below = clip.op.walk(src, &p.found).await?;
            std::iter::once((src.clone(), dest.clone(), 0))
                .chain(below.into_iter().map(|(path, size)| {
                    let to_path = format!("{dest}{}", &path[src.len()..]);
                    (path, to_path, size)
                }))
                .collect()
        } else {
            vec![(src.clone(), dest.clone(), clip.op.size(src).await?)]
        };
        lists.push(list);
    }
    let files = lists.iter().flatten().filter(|(s, _, _)| !s.ends_with('/'));
    p.files_total.store(files.clone().count(), Ordering::Relaxed);
    p.total.store(files.map(|(_, _, size)| size).sum(), Ordering::Relaxed);
    *p.looked.lock().unwrap() = start.elapsed().as_secs_f64();
    let _ = p.started.set(Instant::now());

    for ((src, _), list) in copies.iter().zip(lists) {
        for (from, dest, size) in list {
            if from.ends_with('/') {
                to.create_dir(&dest).await?;
            } else {
                if p.cancel.load(Ordering::Relaxed) {
                    return Err(("error-cancelled", String::new()));
                }
                *p.current.lock().unwrap() = name(&from).to_string();
                copy_file(&clip.op, &from, to, &dest, size, p).await?;
                p.files.fetch_add(1, Ordering::Relaxed);
            }
            p.touched.lock().unwrap().push(parent(&dest));
        }
        // Moved between devices: the original goes only once all of it has been copied.
        if clip.cut {
            clip.op.delete(src, Deletion::Permanent).await?;
            gone.push(src.clone());
        }
    }
    Ok(())
}

async fn copy_file(from: &Store, src: &str, to: &Store, dest: &str, size: u64, p: &Progress) -> Result<(), Error> {
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
    let (read, write) = futures_util::future::join(from.send_chunks(src, size, tx, &p.done, cancel), to.write_chunks(dest, size, rx)).await;
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
        let clip = |paths: &[&str], cut| Clip { op: op.clone(), base: "/".into(), paths: paths.iter().map(|s| s.to_string()).collect(), cut };
        rt.block_on(async {
            // Copy into another folder: x.jpg is a conflict, kept both.
            let c = clip(&["a/x.jpg", "a/sub/"], false);
            assert_eq!(conflicts(&c, &op, "/", "b/").await.unwrap(), vec!["a/x.jpg"]);
            assert_eq!(conflicts(&c, &op, "/", "a/sub/").await.unwrap_err().0, "error-paste-inside");
            let choices = HashMap::from([("a/x.jpg".to_string(), Choice::KeepBoth)]);
            let p = Progress::default();
            let (gone, r) = run(&c, &op, "/", "b/", &choices, &p).await;
            r.unwrap();
            assert!(gone.is_empty());
            assert_eq!(std::fs::read(dir.join("b/x (2).jpg")).unwrap().len(), 3 << 20);
            assert_eq!(std::fs::read(dir.join("b/x.jpg")).unwrap(), b"old");
            assert_eq!(std::fs::read(dir.join("b/sub/y.mp4")).unwrap(), b"video");
            assert_eq!(p.files.load(Ordering::Relaxed), 2);
            assert_eq!(p.done.load(Ordering::Relaxed), (3 << 20) + 5);

            // Copy into its own folder: "name (2)".
            let (_, r) = run(&clip(&["a/sub/"], false), &op, "/", "a/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert!(dir.join("a/sub (2)/y.mp4").exists());

            // Move replacing: the old x.jpg is gone, the source too.
            let (gone, r) = run(&clip(&["a/x.jpg"], true), &op, "/", "b/", &HashMap::new(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(gone, vec!["a/x.jpg"]);
            assert_eq!(std::fs::read(dir.join("b/x.jpg")).unwrap().len(), 3 << 20);
            assert!(!dir.join("a/x.jpg").exists());

            // Cancelled: no half file left.
            let p = Progress::default();
            p.cancel.store(true, Ordering::Relaxed);
            let (_, r) = run(&clip(&["b/x.jpg"], false), &op, "/", "a/", &HashMap::new(), &p).await;
            assert_eq!(r.unwrap_err().0, "error-cancelled");
            assert!(!dir.join("a/x.jpg").exists());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
