//! Listings changed in place: an item deleted or renamed, a folder read again in the background,
//! with thumbnails, selection and viewer following their files.

use std::collections::{HashMap, HashSet};

use crate::source::{self, Listing};
use crate::App;

impl App {
    /// Item deleted: out of the listing, as if read again. A deleted folder also takes the
    /// listings under it, and the current folder if it was inside. `to_trash`: moved into the
    /// trash folder next to it, whose button appears if it was not there (phones and servers have
    /// no watcher to tell).
    pub(crate) fn remove_item(&mut self, path: &str, to_trash: bool) {
        self.marked.remove(path);
        let trash = format!("{}{}/", source::parent(path), source::TRASH_DIR);
        let dir = source::listed_in(path);
        self.listings.retain(|k, _| !(path.ends_with('/') && k.starts_with(path)) && !(to_trash && *k == trash));
        if left_inside(&self.dir, path) {
            self.listings.remove(&dir);
            self.set_dir(dir);
            return;
        }
        let Some(Ok(l)) = self.listings.get(&dir) else { return };
        let new = Listing {
            dirs: l.dirs.iter().filter(|d| *d != path).cloned().collect(),
            files: l.files.iter().filter(|f| f.path != path).cloned().collect(),
            trash: if to_trash { Some(trash) } else { l.trash.clone().filter(|t| t != path) },
            leftovers: Vec::new(),
            trashed: l.trashed.iter().filter(|f| f.path != path).cloned().collect(),
            unseen: l.unseen.clone(),
        };
        self.swap_listing(&dir, new);
    }

    /// Item renamed: its folder is read again (its place in the order may change) and the item is
    /// selected under its new name, not what took its old place; if the current folder was inside
    /// a renamed one, it follows the new name.
    pub(crate) fn renamed(&mut self, old: &str, new: &str) {
        self.marked.remove(old);
        let dir = source::parent(old);
        if dir == self.dir {
            self.select_after = Some(new.to_string());
        }
        // The listing has the new name until it is read again: the viewer on the file renamed (F2)
        // follows it there with its zoom and turns not saved (`swap_listing`), as do the thumbnail
        // and Ctrl+Z. They were dropped, or went to the image that took its place (audit 1.5, #198).
        if let Some(Ok(l)) = self.listings.get_mut(&dir)
            && let Some(f) = l.files.iter_mut().find(|f| f.path == old)
        {
            f.path = new.to_string();
        }
        self.turn_steps.renamed(old, new);
        if old.ends_with('/') {
            self.listings.retain(|k, _| !k.starts_with(old));
            if self.dir.starts_with(old) {
                let current = format!("{new}{}", &self.dir[old.len()..]);
                self.listings.remove(&dir);
                self.set_dir(current);
                return;
            }
        }
        self.relist(dir);
    }

    /// Reads a folder again in the background; the new listing replaces the old one in place.
    pub(crate) fn relist(&mut self, dir: String) {
        let Some(loader) = &self.loader else { return };
        if self.listing_pending.insert(dir.clone()) {
            self.relisting.insert(dir.clone());
            loader.list(dir);
        } else {
            // The one on its way may be from before: a trash emptied while a restore read the
            // folder came back in the grid, empty (seen on USB sticks on Windows).
            self.relist_again.insert(dir);
        }
    }

    /// New listing of a folder already shown (image deleted, folder changed on disk). Thumbnails
    /// and viewer images are kept by position: they follow their file to its new place, so the
    /// next image in the viewer is already there; those of files gone or changed are dropped.
    /// Selection and viewer follow their item; if it is gone, what took its place.
    /// Requests in flight carry the old positions: a new generation drops them and they are asked again.
    pub(crate) fn swap_listing(&mut self, dir: &str, new: Listing) {
        let Some(Ok(old)) = self.listings.insert(dir.to_string(), Ok(new)) else { return };
        if dir != self.dir {
            return;
        }
        let Some(Ok(new)) = self.listings.get(dir) else { return };
        // A folder just made, or the file the editor saved last, is selected instead; the
        // editor's only once it is closed (`select_saved`): until then the viewer stays on the
        // image being edited, and so does the window's title (issue #98). Kept until a listing
        // has it: one read before it was written must not drop it (Save as… from the viewer).
        let select = if self.editor.is_some() { None } else { self.select_after.as_deref() };
        let state = Follow {
            selected: self.selected,
            anchor: self.anchor,
            cursor: self.cursor,
            viewing: self.viewing,
            zoom_of: self.zoom_of,
            generated: std::mem::take(&mut self.generated),
        };
        let f = follow(&old, new, state, select);
        fn remap<T>(map: &mut HashMap<usize, T>, moved: &HashMap<usize, usize>) {
            *map = map.drain().filter_map(|(k, v)| Some((*moved.get(&k)?, v))).collect();
        }
        remap(&mut self.thumbs, &f.moved);
        remap(&mut self.full, &f.moved);
        // An image just saved from Rotate and crop: its new file is what it shows already.
        if let Some((path, tex)) = self.fresh.take()
            && let Some(j) = new.files.iter().position(|f| f.path == path)
        {
            self.full.insert(j, Ok(tex));
        }
        if f.made {
            self.select_after = None;
        }
        self.scroll_to_selected |= f.made;
        if f.drop_turn {
            self.turn = Default::default();
        }
        let s = f.state;
        (self.selected, self.anchor, self.cursor, self.viewing, self.zoom_of, self.generated) =
            (s.selected, s.anchor, s.cursor, s.viewing, s.zoom_of, s.generated);
        self.meta = self.meta.take().and_then(|(i, m)| Some((*f.moved.get(&i)?, m)));
        if let Some(loader) = &self.loader {
            self.generation = loader.next_generation();
        }
        self.thumb_queue_key = None;
        self.pending.clear();
        self.meta_asked = None;
    }

    /// The editor closed: the file it saved last (`select_after`), if already listed, is selected
    /// and shown by an open viewer (issue #98); if not yet, the next listing selects it.
    pub(crate) fn select_saved(&mut self) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return };
        let Some(i) = self.select_after.as_deref().and_then(|p| place(l, p)) else { return };
        let file = i.checked_sub(l.dirs.len());
        self.select_after = None;
        (self.selected, self.cursor, self.scroll_to_selected) = (i, true, true);
        if self.viewing.is_some() && file.is_some() && self.viewing != file {
            self.viewing = file;
            self.turn = Default::default();
        }
    }
}

/// What follows its item by position when a listing takes the place of another (`swap_listing`):
/// grid items (folders, then files) for `selected` and `anchor`, files for the others.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Follow {
    pub(crate) selected: usize,
    /// Where a Shift range starts (`mark_range`).
    pub(crate) anchor: usize,
    pub(crate) cursor: bool,
    pub(crate) viewing: Option<usize>,
    pub(crate) zoom_of: Option<usize>,
    /// Files whose thumbnail is done.
    pub(crate) generated: HashSet<usize>,
}

/// `Follow` once `old` is replaced by `new`, and what else changes with it.
#[derive(Debug)]
pub(crate) struct Followed {
    pub(crate) state: Follow,
    /// Files unchanged, old position to new: their images are still good.
    pub(crate) moved: HashMap<usize, usize>,
    /// `select` was found: selected (and shown by the viewer), to scroll to.
    pub(crate) made: bool,
    /// The viewer goes to another file saved: its turns not saved go.
    pub(crate) drop_turn: bool,
}

/// Selection and viewer follow their item from `old` to `new`; if it is gone, what took its place.
/// `select`: an item to select instead (a folder just made, a file saved), if `new` has it.
pub(crate) fn follow(old: &Listing, new: &Listing, s: Follow, select: Option<&str>) -> Followed {
    let (to, moved) = moves(old, new);
    let generated = s.generated.iter().filter_map(|k| moved.get(k).copied()).collect();
    let (ndirs, n) = (new.dirs.len(), new.files.len());
    let file_at = |i: usize| to.get(i).copied().flatten();
    let followed = match old.dirs.get(s.selected) {
        Some(d) => new.dirs.iter().position(|x| x == d),
        None => file_at(s.selected - old.dirs.len()).map(|j| ndirs + j),
    };
    let made = select.and_then(|p| place(new, p));
    let selected = made.or(followed).unwrap_or(s.selected).min((ndirs + n).saturating_sub(1));
    let cursor = (s.cursor || made.is_some()) && ndirs + n > 0;
    // The viewer goes to that file: what was just saved is what is shown.
    let copy = made.and_then(|m| m.checked_sub(ndirs));
    // A file saved (a copy, or the one shown written again) is shown as it is; the file shown,
    // renamed and otherwise unchanged (same size and date), keeps its turns.
    let unchanged = s.viewing.and_then(|v| moved.get(&v).copied());
    let drop_turn = s.viewing.is_some() && copy.is_some() && copy != unchanged;
    let viewing = s.viewing.filter(|_| n > 0).map(|v| copy.unwrap_or_else(|| file_at(v).unwrap_or(v.min(n - 1))));
    // The viewer's zoom and turns, and the metadata read, follow their file too: with a new
    // file before it the image looked like another one, its turns not saved were dropped
    // and the date taken became the file's.
    let zoom_of = s.zoom_of.and_then(file_at);
    let state = Follow { selected, anchor: s.anchor, cursor, viewing, zoom_of, generated };
    Followed { state, moved, made: made.is_some(), drop_turn }
}

/// Where the files of `old` are in `new` (positions among the images): every file still there,
/// by its path, and those unchanged too (same size and date), whose images are still good.
fn moves(old: &Listing, new: &Listing) -> (Vec<Option<usize>>, HashMap<usize, usize>) {
    let at: HashMap<&str, usize> = new.files.iter().enumerate().map(|(i, f)| (f.path.as_str(), i)).collect();
    let to: Vec<Option<usize>> = old.files.iter().map(|f| at.get(f.path.as_str()).copied()).collect();
    let moved = (old.files.iter().zip(&to).enumerate())
        .filter_map(|(i, (f, j))| j.filter(|&j| new.files[j].size == f.size && new.files[j].modified == f.modified).map(|j| (i, j)))
        .collect();
    (to, moved)
}

/// Whether the folder open `dir` goes with the item `path` removed (it was inside it).
fn left_inside(dir: &str, path: &str) -> bool {
    dir.starts_with(path)
}

/// The item of `path` in a listing: folders first, then files.
fn place(l: &Listing, path: &str) -> Option<usize> {
    l.dirs.iter().position(|d| d == path).or_else(|| l.files.iter().position(|f| f.path == path).map(|j| l.dirs.len() + j))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::File;

    /// Folders come before files: a file's item counts the folders first.
    #[test]
    fn place_of_folders_and_files() {
        let file = |p: &str| File { path: p.into(), size: 1, modified: String::new() };
        let l = Listing { dirs: vec!["p/sub/".into()], files: vec![file("p/a.jpg"), file("p/a (edited).jpg")], ..Default::default() };
        assert_eq!(place(&l, "p/sub/"), Some(0));
        assert_eq!(place(&l, "p/a (edited).jpg"), Some(2));
        assert_eq!(place(&l, "p/gone.jpg"), None);
    }

    /// A file new before the one viewed moves it one place on, a file gone one place back: it
    /// is followed by its path (zoom, turns not saved, metadata), its images only if unchanged.
    #[test]
    fn files_followed_to_their_new_place() {
        let file = |p: &str, size| File { path: p.into(), size, modified: "2026-11-14".into() };
        let listing = |files: Vec<File>| Listing { files, ..Default::default() };
        let old = listing(vec![file("p/b.jpg", 1), file("p/q.jpg", 2), file("p/z.jpg", 3)]);
        let added = listing(vec![file("p/a.jpg", 9), file("p/b.jpg", 1), file("p/q.jpg", 2), file("p/z.jpg", 4)]);
        let (to, moved) = moves(&old, &added);
        assert_eq!(to, [Some(1), Some(2), Some(3)]);
        assert_eq!(moved, HashMap::from([(0, 1), (1, 2)]), "z.jpg changed: its images are dropped");
        let removed = listing(vec![file("p/q.jpg", 2), file("p/z.jpg", 3)]);
        let (to, moved) = moves(&old, &removed);
        assert_eq!(to, [None, Some(0), Some(1)]);
        assert_eq!(moved, HashMap::from([(1, 0), (2, 1)]));
        let after = listing(vec![file("p/b.jpg", 1), file("p/q.jpg", 2), file("p/z.jpg", 3), file("p/zeta.jpg", 5)]);
        assert_eq!(moves(&old, &after).0, [Some(0), Some(1), Some(2)]);
    }

    /// Audit 1.15 (a): a file removed whose name starts the open folder's (`a/IMG_1.jpg` and
    /// `a/IMG_1.jpg.bak/`) is not a folder the open one was inside: the grid stays where it is.
    #[test]
    fn file_removed_is_not_the_folder_open() {
        assert!(!left_inside("a/IMG_1.jpg.bak/", "a/IMG_1.jpg"), "a file is not a folder");
        assert!(left_inside("a/IMG_1.jpg.bak/", "a/IMG_1.jpg.bak/"));
        assert!(left_inside("a/b/c/", "a/b/"));
        assert!(!left_inside("a/bc/", "a/b/"));
    }

    /// Audit 1.15 (b): the anchor of Shift+click follows its item, as the selection: with a file
    /// new before it, or one gone, the range marked from it is not shifted by one.
    #[test]
    fn anchor_follows_its_item() {
        let file = |p: &str| File { path: p.into(), size: 1, modified: "2026-11-22".into() };
        let listing = |files: Vec<File>| Listing { dirs: vec!["p/sub/".into()], files, ..Default::default() };
        let old = listing(vec![file("p/b.jpg"), file("p/c.jpg"), file("p/d.jpg")]);
        let added = listing(vec![file("p/a.jpg"), file("p/b.jpg"), file("p/c.jpg"), file("p/d.jpg")]);
        let state = Follow { selected: 3, anchor: 2, cursor: true, ..Default::default() };
        let f = follow(&old, &added, state.clone(), None);
        assert_eq!((f.state.selected, f.state.anchor), (4, 3), "d.jpg selected and the anchor on c.jpg, one on");
        let removed = listing(vec![file("p/c.jpg"), file("p/d.jpg")]);
        let f = follow(&old, &removed, state.clone(), None);
        assert_eq!((f.state.selected, f.state.anchor), (2, 1), "b.jpg gone: both one back");
        let f = follow(&old, &removed, Follow { anchor: 1, ..state.clone() }, None);
        assert_eq!(f.state.anchor, 1, "the anchor's b.jpg gone: what took its place");
        let f = follow(&old, &listing(vec![file("p/b.jpg")]), Follow { anchor: 3, ..state.clone() }, None);
        assert_eq!(f.state.anchor, 1, "past the end: the last item");
        let f = follow(&old, &listing(vec![]), Follow { anchor: 0, ..state }, None);
        assert_eq!(f.state.anchor, 0, "on the folder, still there");
    }
}
