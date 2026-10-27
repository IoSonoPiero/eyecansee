//! Listings changed in place: an item deleted or renamed, a folder read again in the background,
//! with thumbnails, selection and viewer following their files.

use std::collections::HashMap;

use crate::source::{self, Listing};
use crate::App;

impl App {
    /// Item deleted: out of the listing, as if read again. A deleted folder also takes the
    /// listings under it, and the current folder if it was inside. `to_trash`: moved into the
    /// trash folder next to it, whose button appears if it was not there (phones and servers have
    /// no watcher to tell).
    pub(crate) fn remove_item(&mut self, path: &str, to_trash: bool) {
        self.marked.remove(path);
        let dir = source::parent(path);
        let trash = format!("{dir}{}/", source::TRASH_DIR);
        // An Android trashed file is listed in its folder's trash, not in the folder.
        let dir = if source::android_trashed(source::name(path)).is_some() { trash.clone() } else { dir };
        self.listings.retain(|k, _| !(path.ends_with('/') && k.starts_with(path)) && !(to_trash && *k == trash));
        if self.dir.starts_with(path) {
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
        };
        self.swap_listing(&dir, new);
    }

    /// Item renamed: its folder is read again (its place in the order may change); if the current
    /// folder was inside a renamed one, it follows the new name.
    pub(crate) fn renamed(&mut self, old: &str, new: &str) {
        self.marked.remove(old);
        let dir = source::parent(old);
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
        let at: HashMap<&str, usize> = new.files.iter().enumerate().map(|(i, f)| (f.path.as_str(), i)).collect();
        let moved: HashMap<usize, usize> = old
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let j = *at.get(f.path.as_str())?;
                (new.files[j].size == f.size && new.files[j].modified == f.modified).then_some((i, j))
            })
            .collect();
        fn remap<T>(map: &mut HashMap<usize, T>, moved: &HashMap<usize, usize>) {
            *map = map.drain().filter_map(|(k, v)| Some((*moved.get(&k)?, v))).collect();
        }
        remap(&mut self.thumbs, &moved);
        remap(&mut self.full, &moved);
        // An image just saved from Rotate and crop: its new file is what it shows already.
        if let Some((path, tex)) = self.fresh.take()
            && let Some(&j) = at.get(path.as_str())
        {
            self.full.insert(j, Ok(tex));
        }
        self.generated = self.generated.iter().filter_map(|k| moved.get(k).copied()).collect();
        let (ndirs, n) = (new.dirs.len(), new.files.len());
        let file_at = |i: usize| old.files.get(i).and_then(|f| at.get(f.path.as_str()).copied());
        let followed = match old.dirs.get(self.selected) {
            Some(d) => new.dirs.iter().position(|x| x == d),
            None => file_at(self.selected - old.dirs.len()).map(|j| ndirs + j),
        };
        // A folder just made is selected instead.
        let made = self.select_after.take().and_then(|p| new.dirs.iter().position(|d| *d == p));
        self.selected = made.or(followed).unwrap_or(self.selected).min((ndirs + n).saturating_sub(1));
        self.cursor = (self.cursor || made.is_some()) && ndirs + n > 0;
        self.scroll_to_selected |= made.is_some();
        self.viewing = self.viewing.filter(|_| n > 0).map(|v| file_at(v).unwrap_or(v.min(n - 1)));
        if let Some(loader) = &self.loader {
            self.generation = loader.next_generation();
        }
        self.thumb_queue_key = None;
        self.pending.clear();
        self.meta_asked = None;
    }
}
