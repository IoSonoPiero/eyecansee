//! What an action works on: the selected item or the one in the viewer, the marked items, and
//! which of them may be changed.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;

use crate::source::File;
use crate::{App, Focus, source};

/// The marked items (`App::marked`, paths), with the positions of the marked files in the open
/// folder's listing found last: the viewer asks for them at every frame (`viewer_set`, the
/// title), and finding them looked at every file of the folder twice a frame (audit 4.11). Every
/// change of the marks lets the positions go; a listing changed has another generation or another
/// number of files.
#[derive(Default)]
pub(crate) struct Marked {
    set: HashSet<String>,
    found: RefCell<Option<Found>>,
}

/// The positions of the marked files, and the listing they are of: its generation, its files.
type Found = ((u64, usize), Arc<[usize]>);

impl Marked {
    pub(crate) fn insert(&mut self, path: String) -> bool {
        *self.found.get_mut() = None;
        self.set.insert(path)
    }

    pub(crate) fn remove<Q: Hash + Eq + ?Sized>(&mut self, path: &Q) -> bool
    where
        String: Borrow<Q>,
    {
        *self.found.get_mut() = None;
        self.set.remove(path)
    }

    pub(crate) fn clear(&mut self) {
        *self.found.get_mut() = None;
        self.set.clear();
    }

    /// The positions in `files` (a listing of `generation`) of the marked ones, in order.
    pub(crate) fn positions(&self, files: &[File], generation: u64) -> Arc<[usize]> {
        let key = (generation, files.len());
        let mut found = self.found.borrow_mut();
        match &*found {
            Some((k, p)) if *k == key => p.clone(),
            _ => {
                let p: Arc<[usize]> = files.iter().enumerate().filter(|(_, f)| self.set.contains(&f.path)).map(|(i, _)| i).collect();
                *found = Some((key, p.clone()));
                p
            }
        }
    }
}

impl std::ops::Deref for Marked {
    type Target = HashSet<String>;
    fn deref(&self) -> &HashSet<String> {
        &self.set
    }
}

impl From<HashSet<String>> for Marked {
    fn from(set: HashSet<String>) -> Self {
        Self { set, found: RefCell::default() }
    }
}

impl FromIterator<String> for Marked {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        HashSet::from_iter(iter).into()
    }
}

#[cfg(test)]
thread_local! {
    /// Paths copied by `marked_paths` on this thread (audit 1.1: thirty copies in a frame).
    pub(crate) static MARKED_COPIED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Files of the folder compared with the paths weighed by `images_size`, at most, on this
    /// thread (audit 1.1: each file with every path marked).
    pub(crate) static SIZE_COMPARED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

thread_local! {
    /// `chosen(None)` and `grid_chosen(None)` while the toolbar weighs its entries
    /// (`App::weighing`): made once for all of them, not once for each (audit 1.1). Kept only
    /// during a call on `&App`, in which nothing they are made from can change.
    static WEIGHED: RefCell<Option<Weighed>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Weighed {
    chosen: Option<Rc<Vec<String>>>,
    grid: Option<Rc<Vec<String>>>,
}

/// Ends `App::weighing`, also when what it ran panicked.
struct EndWeighing;

impl Drop for EndWeighing {
    fn drop(&mut self) {
        WEIGHED.with(|w| *w.borrow_mut() = None);
    }
}

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

/// Copy with the metadata…: videos only, a picture already keeps its metadata in its own file.
pub(crate) const META_COPIED: &[Kind] = &[Kind::Video];

/// What Save as… saves from the grid (in the viewer also what a 3D model or a drawing shows).
pub(crate) const SAVED_AS: &[Kind] = &[Kind::Picture, Kind::Model, Kind::Drawing];

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

/// Why the entry `label` (Fluent id) is greyed out when none of the files chosen is of its
/// `kinds`: its own reason when it has one, otherwise `none_of`.
pub(crate) fn why_none(label: &str, kinds: &[Kind]) -> &'static str {
    match label {
        "menu-copy-meta" => "off-copy-meta-no-video",
        _ => none_of(kinds),
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

/// What a quarter turn (`op`) works on: a picture, or a video whose container keeps a rotation.
pub(crate) fn turned(path: &str, op: crate::transform::Op) -> bool {
    crate::decode::is_image(path) || crate::video_turn::turned_by(path, op)
}

/// How a quarter turn fits the files chosen in the grid, as `fits`: only on what it turns, so an
/// AVI alone greys it out instead of leaving it on with nothing to do.
pub(crate) fn turned_fit(files: &[String], op: crate::transform::Op) -> Option<bool> {
    let n = files.iter().filter(|p| turned(p, op)).count();
    (n > 0).then_some(n == files.len())
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

    /// As `turns_fit`, for one flip or turn: the quarter turns work on videos too (#125).
    pub(crate) fn turn_fits(&self, op: crate::transform::Op) -> Option<bool> {
        if !crate::video_turn::turns_videos(op) {
            return self.turns_fit();
        }
        let shown = self.target().filter(|p| !source::in_trash(p));
        match self.viewing {
            Some(_) => shown.filter(|p| turned(p, op)).map(|_| true),
            None => turned_fit(&self.grid_chosen_now(), op),
        }
    }

    /// The files a flip or a turn in the grid works on: pictures, and videos for the quarter turns.
    pub(crate) fn grid_turned(&self, path: Option<String>, op: crate::transform::Op) -> Vec<String> {
        self.grid_items(path).into_iter().filter(|p| turned(p, op)).collect()
    }

    /// `turn_fit` for what is in front: the Image menu, the toolbar and Ctrl+R agree.
    /// Not a file in a trash (a turn there would be saved over it).
    pub(crate) fn turns_fit(&self) -> Option<bool> {
        let photo = || self.viewing_photo() && !self.target().is_some_and(|p| source::in_trash(&p));
        turn_fit(self.viewing.map(|_| photo()), &self.grid_chosen_now())
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
        let paths: Vec<String> = l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).filter(|p| self.marked.contains(*p)).cloned().collect();
        #[cfg(test)]
        MARKED_COPIED.with(|n| n.set(n.get() + paths.len()));
        paths
    }

    /// Total size of the images among `paths` (folders are not counted).
    pub(crate) fn images_size(&self, paths: &[String]) -> u64 {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return 0 };
        let paths: HashSet<&str> = paths.iter().map(String::as_str).collect();
        #[cfg(test)]
        SIZE_COMPARED.with(|n| n.set(n.get() + l.files.len()));
        l.files.iter().filter(|f| paths.contains(f.path.as_str())).map(|f| f.size).sum()
    }

    /// How many of the items of the open folder are marked, and the size of the images among
    /// them: the status bar's, without a copy of their list (audit 1.1).
    pub(crate) fn marked_weight(&self) -> (usize, u64) {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return (0, 0) };
        #[cfg(test)]
        SIZE_COMPARED.with(|n| n.set(n.get() + l.files.len()));
        let files = l.files.iter().filter(|f| self.marked.contains(&f.path));
        let (n, size) = files.fold((0, 0), |(n, size), f| (n + 1, size + f.size));
        (n + l.dirs.iter().filter(|d| self.marked.contains(*d)).count(), size)
    }

    /// `f` with `chosen_now` and `grid_chosen_now` made once for all it asks: the toolbar
    /// weighing each of its entries asked for the list of the marked items eighty times a frame
    /// (audit 1.1).
    pub(crate) fn weighing<R>(&self, f: impl FnOnce(&Self) -> R) -> R {
        if WEIGHED.with(|w| w.borrow().is_some()) {
            return f(self);
        }
        WEIGHED.with(|w| *w.borrow_mut() = Some(Weighed::default()));
        let _end = EndWeighing;
        f(self)
    }

    /// `chosen(None)`, made once inside `weighing`.
    pub(crate) fn chosen_now(&self) -> Rc<Vec<String>> {
        let kept = WEIGHED.with(|w| w.borrow().as_ref().map(|w| w.chosen.clone()));
        match kept {
            Some(Some(chosen)) => chosen,
            Some(None) => {
                let chosen = Rc::new(self.chosen(None));
                WEIGHED.with(|w| w.borrow_mut().as_mut().map(|w| w.chosen = Some(chosen.clone())));
                chosen
            }
            None => Rc::new(self.chosen(None)),
        }
    }

    /// `grid_chosen(None)`, made once inside `weighing`: the same list as `chosen_now` when none
    /// of it is in a trash.
    pub(crate) fn grid_chosen_now(&self) -> Rc<Vec<String>> {
        if let Some(Some(grid)) = WEIGHED.with(|w| w.borrow().as_ref().map(|w| w.grid.clone())) {
            return grid;
        }
        let chosen = self.chosen_now();
        let grid = match () {
            _ if self.viewing.is_some() => Rc::new(Vec::new()),
            _ if !chosen.iter().any(|p| source::in_trash(p)) => chosen,
            _ => Rc::new(chosen.iter().filter(|p| !source::in_trash(p)).cloned().collect()),
        };
        WEIGHED.with(|w| w.borrow_mut().as_mut().map(|w| w.grid = Some(grid.clone())));
        grid
    }

    /// `files_chosen(None)`: in the grid `grid_chosen_now`.
    pub(crate) fn files_chosen_now(&self) -> Rc<Vec<String>> {
        if self.viewing.is_some() {
            return Rc::new(self.files_chosen(None));
        }
        self.grid_chosen_now()
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

    /// Audit 1.1: Ctrl+A in a large folder. A frame of the grid copies the list of the marked
    /// items once or twice, not once for each entry of the toolbar and the status bar, and the
    /// status bar weighs them with one look at each file, not one comparison with every marked path.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn select_all_in_a_large_folder() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("selection::tests::select_all_in_a_large_folder") {
            return;
        }
        const N: usize = 2000;
        let names: Vec<String> = (0..N).map(|i| format!("p{i:05}.png")).collect();
        let dir = folder("select-all-large", &names.iter().map(String::as_str).collect::<Vec<_>>());
        let mut h = Harness::new(&dir);
        h.until(|app| app.files().len() == N && app.listing_pending.is_empty(), "the folder listed");
        (h.app.cursor, h.app.focus) = (true, Focus::Grid);
        h.app.select_all();
        assert_eq!(h.app.marked.len(), N, "Ctrl+A");
        h.frame(Vec::new());
        MARKED_COPIED.with(|n| n.set(0));
        SIZE_COMPARED.with(|n| n.set(0));
        // Counted by pass: egui runs a frame twice when a pass asks for it (a busy computer).
        let pass = h.ctx.cumulative_pass_nr();
        h.frame(Vec::new());
        let passes = (h.ctx.cumulative_pass_nr() - pass) as usize;
        let (copied, compared) = (MARKED_COPIED.with(|n| n.get()) / passes, SIZE_COMPARED.with(|n| n.get()) / passes);
        eprintln!("one pass with {N} files marked: {copied} paths copied, {compared} files compared");
        // One for the toolbar's entries, one for the editor's look at the picture chosen.
        assert!(copied <= 2 * N, "{copied} paths copied in a pass: the marked list made {} times", copied / N);
        assert!(compared <= N, "{compared} comparisons to weigh {N} files marked");
    }

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

    /// Copy with the metadata: greyed out with its own reason on pictures only (also a motion
    /// photo, a JPEG) and on audio files, "*" with a picture next to a video.
    #[test]
    fn copy_meta_fits_the_videos() {
        let files: Vec<String> = ["a.jpg", "b.MP4", "c.mov", "d.mp3", "e/", "f.MP.jpg"].map(String::from).to_vec();
        assert_eq!(fits(&files[..1], META_COPIED), None);
        assert_eq!(fits(&files[5..], META_COPIED), None);
        assert_eq!(fits(&files[3..4], META_COPIED), None);
        assert_eq!(fits(&files[1..3], META_COPIED), Some(true));
        assert_eq!(fits(&files[..2], META_COPIED), Some(false));
        assert_eq!(fits(&files[2..5], META_COPIED), Some(false));
        assert_eq!(why_none("menu-copy-meta", META_COPIED), "off-copy-meta-no-video");
        // The other video entries keep the common reason.
        assert_eq!(why_none("menu-strip-audio", &[Kind::Video]), "off-no-video");
        assert_eq!(why_none("menu-batch", CONVERTED), "off-no-converted");
        // The pictures and the audio files chosen with videos are left out, and named so.
        let (videos, out) = split(files.clone(), META_COPIED);
        assert_eq!((videos.len(), out.len()), (2, 4));
        assert_eq!(left_out_id(META_COPIED, "a.jpg"), "skip-not-video");
        for code in ["en", "it"] {
            let i18n = crate::i18n::I18n::new(code);
            assert_ne!(i18n.t("off-copy-meta-no-video"), "off-copy-meta-no-video", "{code}");
        }
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
        // A quarter turn: on the videos that keep a rotation, not on an AVI (#125).
        let cw = crate::transform::Op::Cw;
        let files: Vec<String> = ["a.jpg", "b.mp4", "c.avi", "d/"].map(String::from).to_vec();
        assert_eq!(turned_fit(&files[1..2], cw), Some(true));
        assert_eq!(turned_fit(&files[..2], cw), Some(true));
        assert_eq!(turned_fit(&files[..3], cw), Some(false));
        assert_eq!(turned_fit(&files[2..3], cw), None);
        assert_eq!(turned_fit(&files[2..], cw), None);
    }
}
