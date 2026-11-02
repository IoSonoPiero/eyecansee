//! What an action works on: the selected item or the one in the viewer, the marked items, and
//! which of them may be changed.

use crate::{App, Focus, source};

/// What a file is, for the menus: each entry works on some kinds, and only on those files of a
/// mixed selection (user, 2026-11-02).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Picture,
    Video,
    Audio,
    Model,
    Drawing,
}

pub(crate) fn kind(path: &str) -> Kind {
    if crate::decode::is_video(path) {
        Kind::Video
    } else if crate::decode::is_audio(path) {
        Kind::Audio
    } else if crate::model::is_model(path) {
        Kind::Model
    } else if crate::cad::is_cad(path) {
        Kind::Drawing
    } else {
        Kind::Picture
    }
}

/// The kinds each entry works on.
pub(crate) const PLAYED: &[Kind] = &[Kind::Picture, Kind::Video];
pub(crate) const THUMBED: &[Kind] = &[Kind::Picture, Kind::Video, Kind::Model, Kind::Drawing];

/// How an entry fits the files chosen: on none of them (`None`: not shown), on all, or on some
/// (`Some(false)`: marked with "*").
pub(crate) fn fits(files: &[String], kinds: &[Kind]) -> Option<bool> {
    let n = files.iter().filter(|p| kinds.contains(&kind(p))).count();
    (n > 0).then_some(n == files.len())
}

/// The label of an entry, with "*" when it works on only some of the files chosen.
pub(crate) fn starred(label: String, all: bool) -> String {
    if all { label } else { format!("{label} *") }
}

impl App {
    /// The files an action works on: the one in the viewer, or `grid_items`.
    pub(crate) fn files_chosen(&self, path: Option<String>) -> Vec<String> {
        if self.viewing.is_some() {
            return path.or_else(|| self.target()).into_iter().collect();
        }
        self.grid_items(path)
    }

    /// The files of `grid_items` of these kinds.
    pub(crate) fn grid_of(&self, path: Option<String>, kinds: &[Kind]) -> Vec<String> {
        self.grid_items(path).into_iter().filter(|p| kinds.contains(&kind(p))).collect()
    }

    /// The images and videos (not folders) an action on `path` in the grid works on, as `chosen`.
    pub(crate) fn grid_items(&self, path: Option<String>) -> Vec<String> {
        if self.viewing.is_some() {
            return Vec::new();
        }
        self.chosen(path).into_iter().filter(|p| !p.ends_with('/') && !source::in_trash(p)).collect()
    }

    /// As `grid_items`, pictures only (no videos, audio, 3D models or CAD drawings): for the image edits.
    pub(crate) fn grid_images(&self, path: Option<String>) -> Vec<String> {
        self.grid_items(path).into_iter().filter(|p| crate::decode::is_image(p)).collect()
    }

    /// The item Delete and Rename act on: the image in the viewer, or the item selected in the grid.
    pub(crate) fn target(&self) -> Option<String> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return None };
        let path = match self.viewing {
            Some(i) => &l.files.get(i)?.path,
            None if self.cursor && self.focus == Focus::Grid => match l.dirs.get(self.selected) {
                Some(d) => d,
                None => &l.files.get(self.selected - l.dirs.len())?.path,
            },
            None => return None,
        };
        Some(path.clone()).filter(|p| self.can_change(p))
    }

    /// Never the root of the source, nor the storages of a phone.
    pub(crate) fn can_change(&self, path: &str) -> bool {
        !path.is_empty() && !(self.base.starts_with("mtp://") && path.ends_with('/') && source::parent(path).is_empty())
    }

    /// Path of a grid item (folders first, then images).
    pub(crate) fn item_path(&self, i: usize) -> Option<String> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return None };
        l.dirs.get(i).or_else(|| l.files.get(i.checked_sub(l.dirs.len())?).map(|f| &f.path)).cloned()
    }

    /// Ctrl+A: every item of the grid marked.
    pub(crate) fn select_all(&mut self) {
        self.marked = (0..self.items()).filter_map(|k| self.item_path(k)).collect();
    }

    /// The marked items still in the folder, in the grid's order.
    pub(crate) fn marked_paths(&self) -> Vec<String> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return Vec::new() };
        l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).filter(|p| self.marked.contains(*p)).cloned().collect()
    }

    /// Total size of the images among `paths` (folders are not counted).
    pub(crate) fn images_size(&self, paths: &[String]) -> u64 {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return 0 };
        l.files.iter().filter(|f| paths.contains(&f.path)).map(|f| f.size).sum()
    }

    /// `path`: `None` = `target`, or all the marked items if there are any (also when `path` is
    /// one of them: the grid's context menu). `permanent`: Shift+Del, for good this time whatever
    /// the options say.
    /// What an action on `path` works on: all the marked items if it is one of them (`None` with
    /// marks in the grid too), otherwise just that one (`None` = the selected one or the one in the viewer).
    pub(crate) fn chosen(&self, path: Option<String>) -> Vec<String> {
        let marked = if self.viewing.is_none() { self.marked_paths() } else { Vec::new() };
        let paths: Vec<String> = match path {
            Some(p) if marked.contains(&p) => marked,
            None if !marked.is_empty() && self.focus == Focus::Grid => marked,
            p => p.or_else(|| self.target()).into_iter().collect(),
        };
        paths.into_iter().filter(|p| self.can_change(p)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_fit_the_kinds_chosen() {
        let files: Vec<String> = ["a.jpg", "b.mp4", "c.stl", "d.dwg", "e.mp3"].map(String::from).to_vec();
        assert_eq!(files.iter().map(|p| kind(p)).collect::<Vec<_>>(), [Kind::Picture, Kind::Video, Kind::Model, Kind::Drawing, Kind::Audio]);
        // A video and a 3D model: Convert and resize on some of them, Export drawing on none.
        let mixed = vec![files[1].clone(), files[2].clone()];
        assert_eq!(fits(&mixed, PLAYED), Some(false));
        assert_eq!(fits(&mixed, &[Kind::Drawing]), None);
        assert_eq!(fits(&files[..2], PLAYED), Some(true));
        assert_eq!(fits(&files[4..], THUMBED), None);
        assert_eq!(starred("Copy".into(), false), "Copy *");
    }
}
