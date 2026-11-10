//! A folder's listing: its folders, the images and videos shown, the trash folder, the leftovers
//! of an interrupted "Save", Android's trashed files and what the grid leaves out; its sorting.

use super::{TRASH_DIR, android_trashed, is_trash, leftover};
use crate::decode::is_media;
use crate::settings::SortBy;

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
        if !is_dir && leftover(name) {
            self.leftovers.push(path.clone());
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
