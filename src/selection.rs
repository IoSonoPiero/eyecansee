//! What an action works on: the selected item or the one in the viewer, the marked items, and
//! which of them may be changed.

use crate::{App, Focus, source};

/// What a file is, for the menus: each entry works on some kinds, and only on those files of a
/// mixed selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Picture,
    Video,
    Audio,
    Model,
    Drawing,
    /// Chosen together with files: no entry for files works on it, it is left out as the other
    /// kinds are ("*", and named in the message at the end).
    Folder,
}

pub(crate) fn kind(path: &str) -> Kind {
    if path.ends_with('/') {
        Kind::Folder
    } else if crate::decode::is_video(path) {
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
/// Convert…: audio files too (#120).
pub(crate) const CONVERTED: &[Kind] = &[Kind::Picture, Kind::Video, Kind::Audio];
/// Every kind has a thumbnail: an audio file its cover (or the note).
pub(crate) const THUMBED: &[Kind] = &[Kind::Picture, Kind::Video, Kind::Audio, Kind::Model, Kind::Drawing];

/// What Save as… saves from the grid (in the viewer also what a 3D model or a drawing shows).
pub(crate) const SAVED_AS: &[Kind] = &[Kind::Picture];

/// How an entry fits the files chosen: on none of them (`None`: not shown), on all, or on some
/// (`Some(false)`: marked with "*").
pub(crate) fn fits(files: &[String], kinds: &[Kind]) -> Option<bool> {
    let n = files.iter().filter(|p| kinds.contains(&kind(p))).count();
    (n > 0).then_some(n == files.len())
}

/// The `files` of `kinds`, and the others.
pub(crate) fn split(files: Vec<String>, kinds: &[Kind]) -> (Vec<String>, Vec<String>) {
    files.into_iter().partition(|p| kinds.contains(&kind(p)))
}

/// Why an entry is greyed out when none of the files chosen is of its kinds (Fluent id).
pub(crate) fn none_of(kinds: &[Kind]) -> &'static str {
    match kinds {
        [Kind::Picture] => "off-no-picture",
        [Kind::Video] => "off-no-video",
        [Kind::Model] => "off-no-model",
        [Kind::Drawing] => "off-no-drawing",
        [Kind::Picture, Kind::Video] => "off-no-played",
        [Kind::Picture, Kind::Video, Kind::Audio] => "off-no-converted",
        _ => "off-no-kind",
    }
}

/// A file (or folder) left out of a work on `kinds`, in the message at the end (Fluent id,
/// `$detail` = its name).
pub(crate) fn left_out_id(kinds: &[Kind], path: &str) -> &'static str {
    match kinds {
        _ if kind(path) == Kind::Folder => "skip-folder",
        [Kind::Picture] => "skip-not-picture",
        [Kind::Video] => "skip-not-video",
        [Kind::Drawing] => "skip-not-drawing",
        [Kind::Picture, Kind::Video, Kind::Audio] => "skip-not-converted",
        _ => "skip-not-played",
    }
}

/// How the flips and turns of the Image menu (and their keys and toolbar buttons) fit: in the
/// viewer (`viewing_photo` is `Some`) the picture shown or nothing, in the grid the pictures
/// among the files chosen.
pub(crate) fn turn_fit(viewing_photo: Option<bool>, grid: &[String]) -> Option<bool> {
    match viewing_photo {
        Some(photo) => photo.then_some(true),
        None => fits(grid, &[Kind::Picture]),
    }
}

/// The label of an entry, with "*" when it works on only some of the files chosen.
pub(crate) fn starred(label: String, all: bool) -> String {
    if all { label } else { format!("{label} *") }
}

impl App {
    /// The files an action works on: the one in the viewer, or `grid_chosen` (with the folders,
    /// which the entries count as left out).
    pub(crate) fn files_chosen(&self, path: Option<String>) -> Vec<String> {
        if self.viewing.is_some() {
            return path.or_else(|| self.target()).into_iter().collect();
        }
        self.grid_chosen(path)
    }

    /// What Save as… works on: in the viewer the file shown (`path`), in the grid the one file
    /// of `SAVED_AS` among those chosen (a folder or a video next to it is left out, "*").
    pub(crate) fn save_as_target(&self, path: Option<String>) -> Option<String> {
        if self.viewing.is_some() {
            return path;
        }
        let mut files = self.grid_of(path.clone(), SAVED_AS);
        if files.len() == 1 { files.pop() } else { path }
    }

    /// The files of `grid_items` of these kinds.
    pub(crate) fn grid_of(&self, path: Option<String>, kinds: &[Kind]) -> Vec<String> {
        self.grid_items(path).into_iter().filter(|p| kinds.contains(&kind(p))).collect()
    }

    /// `turn_fit` for what is in front: the Image menu, the toolbar and Ctrl+R agree.
    /// Not a file in a trash (a turn there would be saved over it).
    pub(crate) fn turns_fit(&self) -> Option<bool> {
        let photo = || self.viewing_photo() && !self.target().is_some_and(|p| source::in_trash(&p));
        turn_fit(self.viewing.map(|_| photo()), &self.grid_chosen(None))
    }

    /// The files (not folders) an action on `path` in the grid works on, as `chosen`.
    pub(crate) fn grid_items(&self, path: Option<String>) -> Vec<String> {
        self.grid_chosen(path).into_iter().filter(|p| !p.ends_with('/')).collect()
    }

    /// As `grid_items`, with the folders chosen: an entry for some kinds of file counts them among
    /// the items left out ("*"), and a work names them in its message.
    pub(crate) fn grid_chosen(&self, path: Option<String>) -> Vec<String> {
        if self.viewing.is_some() {
            return Vec::new();
        }
        self.chosen(path).into_iter().filter(|p| !source::in_trash(p)).collect()
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
        can_change_in(&self.base, path)
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

/// `App::can_change` for the place `base` (the editor's, which may no longer be the grid's).
pub(crate) fn can_change_in(base: &str, path: &str) -> bool {
    !path.is_empty() && !(base.starts_with("mtp://") && path.ends_with('/') && source::parent(path).is_empty())
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
        // Redo thumbnails also on an audio file, whose cover is its thumbnail.
        assert_eq!(fits(&files[4..], THUMBED), Some(true));
        assert_eq!(fits(&files, THUMBED), Some(true));
        assert_eq!(starred("Copy".into(), false), "Copy *");
        // Why an entry is off, and why a file was left out, said in both languages.
        for code in ["en", "it"] {
            let i18n = crate::i18n::I18n::new(code);
            for kinds in [&[Kind::Picture][..], &[Kind::Video], &[Kind::Model], &[Kind::Drawing], PLAYED, CONVERTED, THUMBED] {
                for id in [none_of(kinds), left_out_id(kinds, "a.wav"), left_out_id(kinds, "dir/")] {
                    assert_ne!(i18n.t(id), id, "{code}: {id}");
                }
            }
        }
        assert_eq!(none_of(PLAYED), "off-no-played");
    }

    #[test]
    fn folders_chosen_with_files_are_left_out() {
        let chosen: Vec<String> = ["photos/", "a.jpg", "b.mp4"].map(String::from).to_vec();
        assert_eq!(kind("photos/"), Kind::Folder);
        // A folder is of no kind an entry works on: "*" with the files, greyed out alone.
        assert_eq!(fits(&chosen, PLAYED), Some(false));
        assert_eq!(fits(&chosen[1..], PLAYED), Some(true));
        assert_eq!(fits(&chosen[..1], THUMBED), None);
        assert_eq!(turn_fit(None, &chosen[..2]), Some(false));
        // Left out of a work, and named so in its message.
        let (files, out) = split(chosen, PLAYED);
        assert_eq!((files.len(), out), (2, vec!["photos/".to_string()]));
        assert_eq!(left_out_id(PLAYED, "photos/"), "skip-folder");
        assert_eq!(left_out_id(PLAYED, "c.stl"), "skip-not-played");
    }

    #[test]
    fn turns_fit_the_viewer_or_the_grid() {
        let grid: Vec<String> = ["a.jpg", "b.mp4"].map(String::from).to_vec();
        // In the viewer only a picture turns, whatever the grid holds.
        assert_eq!(turn_fit(Some(true), &[]), Some(true));
        assert_eq!(turn_fit(Some(false), &grid), None);
        // In the grid: a picture and a video is "*", a video alone is greyed out.
        assert_eq!(turn_fit(None, &grid), Some(false));
        assert_eq!(turn_fit(None, &grid[..1]), Some(true));
        assert_eq!(turn_fit(None, &grid[1..]), None);
        assert_eq!(turn_fit(None, &[]), None);
    }
}
