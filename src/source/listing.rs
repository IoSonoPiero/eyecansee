//! A folder's listing: its folders, the images and videos shown, the trash folder, the leftovers
//! of an interrupted "Save", Android's trashed files and what the grid leaves out; its sorting.

use super::{TRASH_DIR, android_trashed, is_trash, leftover};
use crate::decode::is_media;
use crate::settings::SortBy;
use std::cmp::Ordering;

/// The order of names wherever they are listed (the grid, the tree, the lists of the messages;
/// Piero, 21/11: "img10" came before "img2"): capitals and small letters together, numbers by
/// their value ("photo2" before "photo10"), and a name before the same name with something
/// added in front of its extension ("model.gltf" before "model-02.gltf"). Paths are compared
/// folder by folder. Two names that differ only in capitals, or in the zeros in front of a
/// number, always come in the same order (that of their characters).
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.split('/'), b.split('/'));
    loop {
        let (p, q) = match (x.next(), y.next()) {
            (Some(p), Some(q)) => (p, q),
            (p, q) => return p.is_some().cmp(&q.is_some()).then_with(|| a.cmp(b)),
        };
        let ((p, p_ext), (q, q_ext)) = (apart(p), apart(q));
        let order = runs(p, q).then_with(|| runs(p_ext, q_ext));
        if order != Ordering::Equal {
            return order;
        }
    }
}

/// A name and its extension; the dot of a hidden name (".git") is not that of an extension.
fn apart(name: &str) -> (&str, &str) {
    name.rfind('.').filter(|&i| i > 0).map_or((name, ""), |i| (&name[..i], &name[i + 1..]))
}

/// `natural` on a piece of a name: character by character without their case, a run of digits
/// as the number it writes.
fn runs(mut a: &str, mut b: &str) -> Ordering {
    loop {
        let (Some(c), Some(d)) = (a.chars().next(), b.chars().next()) else { return a.len().cmp(&b.len()) };
        let order = if c.is_ascii_digit() && d.is_ascii_digit() {
            let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
            let (m, n) = (digits(a), digits(b));
            let (p, q) = (a[..m].trim_start_matches('0'), b[..n].trim_start_matches('0'));
            (a, b) = (&a[m..], &b[n..]);
            p.len().cmp(&q.len()).then_with(|| p.cmp(q))
        } else {
            (a, b) = (&a[c.len_utf8()..], &b[d.len_utf8()..]);
            c.to_lowercase().cmp(d.to_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
}

#[derive(Default, Clone)]
pub struct Listing {
    pub dirs: Vec<String>,
    pub files: Vec<File>,
    /// The trash folder in this folder, if any: kept out of `dirs`, it opens from its own button
    /// in the toolbar (among the folders it was in the way).
    pub trash: Option<String>,
    /// Hidden files of an interrupted "Save" (`save_names`), for `Store::recover`: their size and
    /// date tell whether one is still being written.
    pub leftovers: Vec<File>,
    /// Files in Android's trash (`android_trashed`): shown in the folder's trash, not here.
    pub trashed: Vec<File>,
    /// Names of what the grid leaves out: files that are not media, hidden items (a name starting
    /// with ".", EyeCanSee's trash too, Android's trashed files, Windows' hidden attribute). For
    /// the check before deleting a folder.
    pub unseen: Vec<String>,
}

#[derive(Clone)]
pub struct File {
    pub path: String,
    /// Size and modification date: they identify the file's version for the thumbnail cache.
    pub size: u64,
    /// As the source gives it ("" if it does not): only used for comparison.
    pub modified: String,
}

impl Listing {
    /// One item of a folder being read, put where it belongs: the hidden files of an interrupted
    /// "Save" in `leftovers`, Android's trashed files in `trashed`, then folders and images. Hidden
    /// items are not shown, apart from EyeCanSee's trash folders; `shown` false hides it too
    /// (Windows' hidden attribute, a file being written). Folder paths end in "/".
    pub(crate) fn add(&mut self, path: String, name: &str, is_dir: bool, size: u64, modified: String, shown: bool) {
        // A folder a move put aside too (audit F.7 of 27/11): its name back when free.
        if (!is_dir && leftover(name)) || (is_dir && crate::trash::moved_aside(name)) {
            self.leftovers.push(File { path: path.clone(), size, modified: modified.clone() });
        }
        if !is_dir && android_trashed(name).is_some() {
            self.unseen.push(name.to_string());
            self.trashed.push(File { path, size, modified });
            return;
        }
        if !shown || name.starts_with('.') {
            self.unseen.push(name.to_string());
        }
        if !shown || (name.starts_with('.') && !(is_dir && name == TRASH_DIR)) {
            return;
        }
        if is_dir {
            self.dirs.push(path);
        } else if is_media(name) {
            self.files.push(File { path, size, modified });
        } else {
            self.unseen.push(name.to_string());
        }
    }

    /// Folders and files by name (`natural`). Also takes the trash folder out of `dirs`, into
    /// `trash`.
    pub fn sort(&mut self) {
        if let Some(i) = self.dirs.iter().position(|d| is_trash(d)) {
            self.trash = Some(self.dirs.remove(i));
        }
        self.dirs.sort_by(|a, b| natural(a, b));
        self.files.sort_by(|a, b| natural(&a.path, &b.path));
    }

    /// Images in the order chosen by the user; ties broken by name (`natural`). Those without a
    /// date (or with zero size) go last, in descending order too. The extension and the date of
    /// each file are worked out once, not for each comparison (audit 2.28).
    pub fn sort_files(&mut self, by: SortBy, desc: bool) {
        let tz = jiff::tz::TimeZone::system();
        let key = |f: &File| match by {
            SortBy::Type => Key::Ext(f.path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default()),
            SortBy::Date => Key::Date(crate::human::parse_modified_in(&f.modified, &tz)),
            SortBy::Size => Key::Size(f.size),
            SortBy::Name => Key::Name,
        };
        let mut keyed: Vec<(Key, File)> = std::mem::take(&mut self.files).into_iter().map(|f| (key(&f), f)).collect();
        keyed.sort_by(|(x, a), (y, b)| {
            let flip = |o: Ordering| if desc { o.reverse() } else { o };
            let primary = match (x, y) {
                (Key::Ext(x), Key::Ext(y)) => flip(x.cmp(y)),
                (Key::Size(x), Key::Size(y)) => (*x == 0).cmp(&(*y == 0)).then(flip(x.cmp(y))),
                (Key::Date(Some(x)), Key::Date(Some(y))) => flip(x.cmp(y)),
                (Key::Date(x), Key::Date(y)) => x.is_none().cmp(&y.is_none()),
                _ => flip(natural(&a.path, &b.path)),
            };
            primary.then_with(|| natural(&a.path, &b.path))
        });
        self.files = keyed.into_iter().map(|(_, f)| f).collect();
    }
}

/// What `sort_files` compares the files by, worked out once for each.
enum Key {
    Name,
    Size(u64),
    Ext(String),
    Date(Option<jiff::Timestamp>),
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Audit 2.28: sorted by date, each file's date is read once, not twice for each comparison
    /// (a phone's date without its zone is three `parse` and a change of zone each time).
    #[test]
    fn sort_by_date_reads_each_date_once() {
        const N: usize = 1000;
        let files = (0..N).map(|i| File { path: format!("p/{i:04}.jpg"), size: 1, modified: format!("2026-11-{:02}T10:{:02}:{:02}", 1 + i * 7 % 28, i * 13 % 60, i % 60) }).collect();
        let mut l = Listing { files, ..Default::default() };
        crate::human::PARSED.with(|n| n.set(0));
        l.sort_files(SortBy::Date, false);
        let parsed = crate::human::PARSED.with(|n| n.get());
        let dates: Vec<_> = l.files.iter().map(|f| crate::human::parse_modified(&f.modified).unwrap()).collect();
        assert!(dates.is_sorted(), "not by date");
        assert!(parsed <= N, "{parsed} dates read to sort {N} files");
    }

    /// Names as people read them (Piero, 21/11): numbers by their value, capitals and small
    /// letters together, a name before the same name with something added in front of its
    /// extension; whatever order they come in, also the names that differ only in capitals or in
    /// the zeros in front of a number.
    #[test]
    fn names_in_natural_order() {
        fn sorted<'a>(names: &[&'a str]) -> Vec<&'a str> {
            let mut v: Vec<&str> = names.to_vec();
            v.sort_by(|a, b| natural(a, b));
            let mut back: Vec<&str> = names.iter().rev().copied().collect();
            back.sort_by(|a, b| natural(a, b));
            assert_eq!(v, back, "the order depends on how they came");
            v
        }
        assert_eq!(sorted(&["foto10.jpg", "foto2.jpg", "Foto1.jpg", "foto.jpg"]), ["foto.jpg", "Foto1.jpg", "foto2.jpg", "foto10.jpg"]);
        assert_eq!(sorted(&["nodo-doppio-10.gltf", "nodo-doppio-02.gltf", "nodo-doppio.gltf", "nodo-doppio-3.gltf"]), ["nodo-doppio.gltf", "nodo-doppio-02.gltf", "nodo-doppio-3.gltf", "nodo-doppio-10.gltf"]);
        assert_eq!(sorted(&["foto (2).jpg", "foto.png", "foto.jpg", "B.jpg", "a.JPG"]), ["a.JPG", "B.jpg", "foto.jpg", "foto.png", "foto (2).jpg"]);
        // The same name but for capitals or zeros: always in the same order.
        assert_eq!(sorted(&["a.jpg", "A.jpg", "img02.jpg", "img2.jpg", "IMG2.jpg"]), ["A.jpg", "a.jpg", "IMG2.jpg", "img02.jpg", "img2.jpg"]);
        // Numbers longer than any integer, and those of an extension or of a folder's version.
        assert_eq!(sorted(&["a100000000000000000000000.jpg", "a99999999999999999999999.jpg", "v1.10/", "v1.2/"]), ["a99999999999999999999999.jpg", "a100000000000000000000000.jpg", "v1.2/", "v1.10/"]);
        // Paths folder by folder; a hidden name first, its dot not an extension's.
        assert_eq!(sorted(&["p/10/a.jpg", "p/9/b.jpg", "p/9/a.jpg", "p/.git/", "p/a/"]), ["p/.git/", "p/9/a.jpg", "p/9/b.jpg", "p/10/a.jpg", "p/a/"]);
        assert_eq!(natural("a.jpg", "a.jpg"), Ordering::Equal);
    }

    /// The grid and the tree use it: by name, and by name among files of the same date, size or
    /// type, also in descending order.
    #[test]
    fn listing_in_natural_order() {
        let f = |path: &str, size, modified: &str| File { path: path.into(), size, modified: modified.into() };
        let dirs = ["p/Viaggio 10/", "p/viaggio 2/", "p/Viaggio 1/"].map(String::from).to_vec();
        let files = vec![f("p/img10.jpg", 5, "2026-01-01T00:00:00Z"), f("p/IMG2.jpg", 5, "2026-01-01T00:00:00Z"), f("p/img1.png", 5, "2026-01-01T00:00:00Z"), f("p/a.jpg", 9, "2026-02-01T00:00:00Z")];
        let mut l = Listing { dirs, files, ..Default::default() };
        let names = |l: &Listing| l.files.iter().map(|f| f.path.trim_start_matches("p/").to_string()).collect::<Vec<_>>();
        l.sort();
        assert_eq!(l.dirs, ["p/Viaggio 1/", "p/viaggio 2/", "p/Viaggio 10/"]);
        assert_eq!(names(&l), ["a.jpg", "img1.png", "IMG2.jpg", "img10.jpg"]);
        l.sort_files(SortBy::Name, true);
        assert_eq!(names(&l), ["img10.jpg", "IMG2.jpg", "img1.png", "a.jpg"]);
        for (by, desc, want) in [(SortBy::Date, false, ["img1.png", "IMG2.jpg", "img10.jpg", "a.jpg"]), (SortBy::Date, true, ["a.jpg", "img1.png", "IMG2.jpg", "img10.jpg"]), (SortBy::Size, false, ["img1.png", "IMG2.jpg", "img10.jpg", "a.jpg"]), (SortBy::Size, true, ["a.jpg", "img1.png", "IMG2.jpg", "img10.jpg"]), (SortBy::Type, false, ["a.jpg", "IMG2.jpg", "img10.jpg", "img1.png"])] {
            l.sort_files(by, desc);
            assert_eq!(names(&l), want, "{} desc {desc}", by.label());
        }
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

    /// What the grid leaves out of a folder is noted, for the check before deleting it.
    #[test]
    fn unseen_names() {
        let mut l = Listing::default();
        for (name, is_dir) in [("a.jpg", false), ("contract.pdf", false), ("sub", true), (".git", true), (TRASH_DIR, true), (".trashed-1795803477-b.jpg", false), ("c.stl", false)] {
            l.add(if is_dir { format!("{name}/") } else { name.to_string() }, name, is_dir, 1, String::new(), true);
        }
        l.add("hidden.jpg".into(), "hidden.jpg", false, 1, String::new(), false);
        let mut unseen = l.unseen.clone();
        unseen.sort();
        assert_eq!(unseen, [TRASH_DIR, ".git", ".trashed-1795803477-b.jpg", "contract.pdf", "hidden.jpg"]);
    }
}
