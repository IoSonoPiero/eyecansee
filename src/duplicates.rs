//! Duplicates: images with exactly the same bytes in a folder (and its subfolders if asked), on any
//! source. Only images of the same size are read, and compared by SHA-256 of their content.
//! The page that shows them is `dup_page`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::source::{Error, File, Store};

/// How far a search has got, read by the window while it runs; `result` when it ends.
#[derive(Default)]
pub struct Progress {
    /// Images found so far.
    pub found: AtomicU64,
    /// Images done: those with a size of their own count at once, the others once read.
    pub examined: AtomicU64,
    pub read: AtomicU64,
    pub cancel: AtomicBool,
    pub result: Mutex<Option<Result<Vec<Group>, Error>>>,
}

/// Order of the copies inside every group, and so which one is "the first".
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Order {
    /// Oldest first, to the minute as shown; with the same date (or none) the shorter path first.
    /// Copies made together differ by a few seconds: by the second the copy "(2)" made a moment
    /// earlier stayed instead of the original (seen on Windows).
    #[default]
    Date,
    /// Shorter file name first ("photo.jpg" before "photo copy.jpg", "photo (2).jpg"): copies
    /// usually get something added to the name. Then as `Date`.
    Name,
    Path,
}

/// Which copy to keep in every group, the others marked for deletion.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Keep {
    First,
    Last,
    /// Nothing marked.
    All,
}

/// Identical images, and which of them to delete.
#[derive(Clone)]
pub struct Group {
    pub files: Vec<File>,
    pub delete: Vec<bool>,
}

impl Group {
    fn new(files: Vec<File>) -> Self {
        let mut g = Group { delete: vec![false; files.len()], files };
        g.sort(Order::default());
        g.keep(Keep::First);
        g
    }

    /// Sorts the copies, each keeping its mark.
    pub fn sort(&mut self, by: Order) {
        let mut both: Vec<(File, bool)> = self.files.drain(..).zip(self.delete.drain(..)).collect();
        match by {
            Order::Date => both.sort_by_key(|(f, _)| date_key(f)),
            Order::Name => both.sort_by_key(|(f, _)| (crate::source::name(&f.path).chars().count(), date_key(f))),
            Order::Path => both.sort_by(|a, b| a.0.path.cmp(&b.0.path)),
        }
        (self.files, self.delete) = both.into_iter().unzip();
    }

    pub fn keep(&mut self, which: Keep) {
        let last = self.files.len() - 1;
        for (i, d) in self.delete.iter_mut().enumerate() {
            *d = match which {
                Keep::First => i != 0,
                Keep::Last => i != last,
                Keep::All => false,
            };
        }
    }

    /// Can copy `i` change its mark? Not when it is the only one left: one copy always stays.
    pub fn can_toggle(&self, i: usize) -> bool {
        self.delete[i] || self.delete.iter().filter(|d| !**d).count() > 1
    }

    pub fn toggle(&mut self, i: usize) {
        if self.can_toggle(i) {
            self.delete[i] = !self.delete[i];
        }
    }

    /// A copy deleted: gone from the group. `false` when the group has no duplicates left.
    pub fn remove(&mut self, path: &str) -> bool {
        if let Some(i) = self.files.iter().position(|f| f.path == path) {
            self.files.remove(i);
            self.delete.remove(i);
        }
        // The copy left on its own is not a duplicate any more; and one must always stay unmarked.
        if self.delete.iter().all(|d| *d) && let Some(d) = self.delete.first_mut() {
            *d = false;
        }
        self.files.len() > 1
    }
}

/// Oldest first (no date last), then the shorter path, then the path.
fn date_key(f: &File) -> (i64, usize, String) {
    let minute = crate::human::parse_modified(&f.modified).map_or(i64::MAX, |t| t.as_second().div_euclid(60));
    (minute, f.path.chars().count(), f.path.clone())
}

/// The images of `dir` (and of its subfolders with `sub`) grouped by identical content, the
/// groups in path order of their first image. Unreadable subfolders and images are left out.
pub async fn find(op: &Store, dir: &str, sub: bool, p: &Progress) -> Result<Vec<Group>, Error> {
    let files = collect(op, dir, sub, p).await?;
    let mut by_size: HashMap<u64, Vec<File>> = HashMap::new();
    for f in files {
        by_size.entry(f.size).or_default().push(f);
    }
    let mut groups = Vec::new();
    for same in by_size.into_values() {
        if same.len() < 2 {
            p.examined.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let mut by_hash: HashMap<Vec<u8>, Vec<File>> = HashMap::new();
        // ponytail: one file at a time; in parallel (local disks only, phones serve one) if it is too slow
        for f in same {
            if p.cancel.load(Ordering::Relaxed) {
                return Err(("error-cancelled", String::new()));
            }
            match op.read(&f.path).await {
                Ok(bytes) => {
                    p.read.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    let hash = ring::digest::digest(&ring::digest::SHA256, &bytes).as_ref().to_vec();
                    by_hash.entry(hash).or_default().push(f);
                }
                Err(e) => log!("duplicates: {:?} not read: {e:?}", f.path),
            }
            p.examined.fetch_add(1, Ordering::Relaxed);
        }
        groups.extend(by_hash.into_values().filter(|g| g.len() > 1).map(Group::new));
    }
    groups.sort_by(|a, b| a.files[0].path.cmp(&b.files[0].path));
    Ok(groups)
}

/// Every image of `dir`, and below it with `sub` (trash folders and hidden folders stay out,
/// as in the listings). Only an error on `dir` itself stops the search.
async fn collect(op: &Store, dir: &str, sub: bool, p: &Progress) -> Result<Vec<File>, Error> {
    let (mut dirs, mut files) = (vec![dir.to_string()], Vec::new());
    while let Some(d) = dirs.pop() {
        if p.cancel.load(Ordering::Relaxed) {
            return Err(("error-cancelled", String::new()));
        }
        let listing = match op.list(&d).await {
            Ok(l) => l,
            Err(e) if d == dir => return Err(e),
            Err(e) => {
                log!("duplicates: {d:?} not listed: {e:?}");
                continue;
            }
        };
        p.found.fetch_add(listing.files.len() as u64, Ordering::Relaxed);
        files.extend(listing.files);
        if sub {
            dirs.extend(listing.dirs);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, modified: &str) -> File {
        File { path: path.into(), size: 1, modified: modified.into() }
    }

    #[test]
    fn group_marks() {
        let mut g = Group::new(vec![file("b/new.jpg", "2026-02-01T00:00:00Z"), file("a/longer/old.jpg", "2026-01-01T00:00:00Z"), file("old.jpg", "2026-01-01T00:00:00Z")]);
        // Oldest first, the shorter path first at the same date; all but the first marked.
        let paths: Vec<_> = g.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["old.jpg", "a/longer/old.jpg", "b/new.jpg"]);
        assert_eq!(g.delete, [false, true, true]);
        // The only copy kept cannot be marked.
        assert!(!g.can_toggle(0));
        g.toggle(0);
        assert_eq!(g.delete, [false, true, true]);
        g.toggle(1);
        assert!(g.can_toggle(0));
        g.sort(Order::Name);
        assert_eq!(g.files[0].path, "old.jpg");
        assert_eq!(g.files[1].path, "a/longer/old.jpg");
        g.sort(Order::Path);
        assert_eq!(g.files[0].path, "a/longer/old.jpg");
        assert_eq!(g.delete, [false, true, false]);
        g.keep(Keep::Last);
        assert_eq!(g.delete, [true, true, false]);
        // Copied together: "(2)" a few seconds older, the original still stays.
        let together = Group::new(vec![file("p (2).jpg", "2026-01-01T10:00:01Z"), file("p.jpg", "2026-01-01T10:00:03Z")]);
        assert_eq!(together.files[0].path, "p.jpg");
        g.keep(Keep::All);
        assert_eq!(g.delete, [false, false, false]);
        // Deleting the copy kept leaves an unmarked one; one copy alone is no group.
        g.keep(Keep::First);
        assert!(g.remove("a/longer/old.jpg"));
        assert_eq!(g.delete, [false, true]);
        assert!(!g.remove("b/new.jpg"));
    }

    #[test]
    fn find_identical() {
        let dir = std::env::temp_dir().join(format!("eyecansee-dup-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let write = |name: &str, data: &[u8]| std::fs::write(dir.join(name), data).unwrap();
        write("a.jpg", b"same");
        write("b.png", b"same");
        write("c.jpg", b"diff"); // same size, other bytes
        write("d.jpg", b"other size");
        write("sub/e.jpg", b"same");
        write("notes.txt", b"same"); // not an image
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let s = crate::source::open(&dir.to_string_lossy()).unwrap();
        let groups = |sub| {
            let p = Progress::default();
            let g = rt.block_on(find(&s.op, &s.dir, sub, &p)).unwrap();
            let names: Vec<Vec<&str>> = g.iter().map(|g| g.files.iter().map(|f| crate::source::name(&f.path)).collect()).collect();
            (names.iter().map(|n| n.join(" ")).collect::<Vec<_>>(), p.found.into_inner(), p.examined.into_inner())
        };
        assert_eq!(groups(false), (vec!["a.jpg b.png".to_string()], 4, 4));
        let (g, found, _) = groups(true);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].split(' ').count(), 3);
        assert_eq!(found, 5);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
