//! Why an entry of the menu bar or a button of the toolbar cannot be used now: the same reason in
//! both places, shown on hover over the greyed-out entry (as in the right-click menus).
//! The choices are plain functions on what is in front (testable); `App` gives them its state.

use crate::App;
use crate::selection::{Kind, SAVED_AS, kind};

/// Why an entry that needs one item chosen is off (Fluent id): nothing chosen.
pub(crate) fn nothing_id(chosen: usize) -> Option<&'static str> {
    (chosen == 0).then_some("tb-off-nothing")
}

/// Why an entry for the grid is off in the viewer, or on an empty folder (Fluent id).
pub(crate) fn grid_id(viewing: bool, items: usize) -> Option<&'static str> {
    match () {
        _ if viewing => Some("tb-off-viewer"),
        _ if items == 0 => Some("off-empty"),
        _ => None,
    }
}

/// Why Paste is off (Fluent id): in the viewer, nothing copied, or not into this folder.
pub(crate) fn paste_id(viewing: bool, copied: bool, can_paste: bool) -> Option<&'static str> {
    match () {
        _ if viewing => Some("tb-off-viewer"),
        _ if !copied => Some("tb-off-paste"),
        _ if !can_paste => Some("tb-off-paste-here"),
        _ => None,
    }
}

/// Why Rename is off (Fluent id): one item at a time, and one that can be changed.
pub(crate) fn rename_id(chosen: usize, can_change: bool) -> Option<&'static str> {
    match () {
        _ if chosen > 1 => Some("rename-one-only"),
        _ if !can_change => Some("tb-off-nothing"),
        _ => None,
    }
}

/// Why Up is off (Fluent id): in the viewer, or at the top already.
pub(crate) fn parent_id(viewing: bool, top: bool) -> Option<&'static str> {
    match () {
        _ if viewing => Some("tb-off-viewer"),
        _ if top => Some("off-top"),
        _ => None,
    }
}

/// Why Save is off (Fluent id). `shown`: what the viewer shows (`None` in the grid); `changed`: a
/// picture turned there. Ctrl+S always means "over the original": where that cannot be, it says
/// what does the job instead.
pub(crate) fn save_id(shown: Option<Kind>, changed: bool) -> Option<&'static str> {
    match shown {
        Some(Kind::Model) => Some("off-save-model"),
        Some(Kind::Drawing) => Some("off-save-drawing"),
        Some(Kind::Video | Kind::Audio) => Some("off-save-played"),
        _ if changed => None,
        _ => Some("tb-off-save"),
    }
}

/// Why Save as… is off (Fluent id): in the viewer on a video or an audio file, in the grid
/// unless one picture is among the files chosen (`saved`: how many of `SAVED_AS`).
pub(crate) fn save_as_id(shown: Option<Kind>, saved: usize) -> Option<&'static str> {
    match shown {
        Some(Kind::Video | Kind::Audio) => Some("off-save-as-played"),
        Some(_) => None,
        None if saved == 1 => None,
        None => Some("tb-off-one-picture"),
    }
}

/// Restore in the File and Edit menus, inside a trash only: label and why it is off (Fluent ids).
/// `target`: the item selected or in the viewer is a trash folder (`Some(true)`) or inside one
/// (`Some(false)`); `here`: the folder open is a trash.
pub(crate) fn restore_ids(target: Option<bool>, here: bool) -> Option<(&'static str, Option<&'static str>)> {
    match target {
        Some(true) => Some(("menu-restore-all", None)),
        Some(false) => Some(("menu-restore", None)),
        None => here.then_some(("menu-restore", Some("off-restore"))),
    }
}

impl App {
    fn off(&self, id: Option<&str>) -> Option<String> {
        id.map(|id| self.i18n.t(id))
    }

    pub(crate) fn new_folder_off(&self) -> Option<String> {
        let id = match () {
            _ if self.can_make_folder() => None,
            _ if self.viewing.is_some() => Some("tb-off-viewer"),
            _ => Some("tb-off-folder"),
        };
        self.off(id)
    }

    pub(crate) fn new_image_off(&self) -> Option<String> {
        let id = match () {
            _ if self.can_new_image() => None,
            _ if self.viewing.is_some() => Some("tb-off-viewer"),
            _ => Some("tb-off-no-folder"),
        };
        self.off(id)
    }

    /// What the viewer shows (`None` in the grid).
    fn shown(&self) -> Option<Kind> {
        let i = self.viewing?;
        self.files().get(i).map(|f| kind(&f.path))
    }

    /// Save: a picture turned in the viewer (also when it cannot be written over: `ask_save` then
    /// says why and offers Save as…).
    pub(crate) fn save_off(&self) -> Option<String> {
        let changed = self.viewing.is_some() && !self.turn.is_none() && self.save_ready.is_none();
        self.trash_off().or_else(|| self.off(save_id(self.shown(), changed)))
    }

    /// Save as…: in the grid one picture, in the viewer what is shown.
    pub(crate) fn save_as_off(&self) -> Option<String> {
        let files = self.files_chosen(None);
        let saved = files.iter().filter(|p| SAVED_AS.contains(&kind(p))).count();
        self.trash_off().or_else(|| self.off(save_as_id(self.shown(), saved)))
    }

    /// Export drawing…, on how the CAD drawings fit the files chosen (`selection::fits`): one
    /// conversion at a time.
    pub(crate) fn drawings_off(&self, fit: Option<bool>) -> Option<String> {
        let id = match () {
            _ if fit.is_none() => Some(crate::selection::none_of(&[Kind::Drawing])),
            _ if self.batch.is_some() => Some("tb-off-busy"),
            _ => None,
        };
        self.off(id)
    }

    pub(crate) fn rename_off(&self) -> Option<String> {
        let trash = self.chosen_in_trash().then_some("off-trash-rename");
        self.off(trash.or_else(|| rename_id(self.chosen(None).len(), self.target().is_some())))
    }

    /// Something chosen (or the file in the viewer) is inside a trash: it can be looked at, put
    /// back or deleted for good, nothing else.
    pub(crate) fn chosen_in_trash(&self) -> bool {
        self.chosen(None).iter().any(|p| crate::source::in_trash(p))
    }

    /// Why what changes the file chosen (turns, the editor, Save, Save as…) is off in a trash.
    fn trash_off(&self) -> Option<String> {
        self.off(self.chosen_in_trash().then_some("off-trash-change"))
    }

    /// Restore of the File and Edit menus, inside a trash: its label, and why it is off.
    pub(crate) fn restore_entry(&self) -> Option<(String, Option<String>)> {
        use crate::source::{in_trash, is_trash};
        let target = self.target().filter(|p| is_trash(p) || in_trash(p)).map(|p| is_trash(&p));
        let (label, off) = restore_ids(target, is_trash(&self.dir))?;
        Some((self.i18n.t(label), self.off(off)))
    }

    pub(crate) fn delete_off(&self) -> Option<String> {
        self.off(nothing_id(usize::from(self.target().is_some())))
    }

    /// Cut and Copy.
    pub(crate) fn clip_off(&self) -> Option<String> {
        let trash = self.chosen_in_trash().then_some("off-trash-clip");
        self.off(trash.or_else(|| nothing_id(self.chosen(None).len())))
    }

    pub(crate) fn paste_off(&self) -> Option<String> {
        self.off(paste_id(self.viewing.is_some(), self.clip.is_some(), self.can_paste(&self.dir)))
    }

    /// Select all, Redo all thumbnails: the grid, with something in it.
    pub(crate) fn grid_off(&self) -> Option<String> {
        self.off(grid_id(self.viewing.is_some(), self.items()))
    }

    /// Open: the item with the cursor, in the grid.
    pub(crate) fn open_off(&self) -> Option<String> {
        self.grid_off().or_else(|| self.off(nothing_id(usize::from(self.cursor))))
    }

    /// Next, Previous, First, Last: something to move among.
    pub(crate) fn go_off(&self) -> Option<String> {
        self.off((self.items() == 0).then_some("off-empty"))
    }

    pub(crate) fn parent_off(&self) -> Option<String> {
        self.off(parent_id(self.viewing.is_some(), self.dir.is_empty()))
    }

    pub(crate) fn back_off(&self) -> Option<String> {
        self.off(self.viewing.is_none().then_some("off-not-viewer"))
    }

    /// The slideshow shows the pictures and videos of the folder.
    pub(crate) fn slideshow_off(&self) -> Option<String> {
        let show = self.files().iter().any(|f| crate::slideshow::in_show(&f.path));
        self.off((!show).then_some("off-no-show"))
    }

    /// Redo thumbnails: the files chosen (not the folders).
    pub(crate) fn redo_thumbs_off(&self) -> Option<String> {
        let files = self.chosen(None).iter().filter(|p| !p.ends_with('/')).count();
        self.off((files == 0).then_some("off-no-file"))
    }

    /// The flips and turns: in the viewer the picture shown, in the grid the pictures chosen.
    pub(crate) fn turn_off(&self) -> Option<String> {
        let id = if self.viewing.is_some() { "tb-off-not-picture" } else { "tb-off-no-picture" };
        self.trash_off().or_else(|| self.off(self.turns_fit().is_none().then_some(id)))
    }

    /// One flip or turn (`turn_fits`): a flip of a video says why it cannot be done.
    pub(crate) fn turn_op_off(&self, op: crate::transform::Op) -> Option<String> {
        if crate::video_turn::turns_videos(op) {
            let id = if self.viewing.is_some() { "tb-off-not-picture" } else { "tb-off-no-picture" };
            return self.trash_off().or_else(|| self.off(self.turn_fits(op).is_none().then_some(id)));
        }
        let video = self.target().is_some_and(|p| crate::decode::is_video(&p)) && (self.viewing.is_some() || self.turns_fit().is_none());
        if video { self.off(Some("off-video-flip")) } else { self.turn_off() }
    }

    /// Edit image… and Rotate and crop…: as the turns, then one image that is not animated.
    pub(crate) fn editor_off(&self) -> Option<String> {
        self.turn_off().or_else(|| self.edit_off(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_greyed_entry_says_why() {
        assert_eq!(nothing_id(0), Some("tb-off-nothing"));
        assert_eq!(nothing_id(2), None);
        assert_eq!(grid_id(true, 5), Some("tb-off-viewer"));
        assert_eq!(grid_id(false, 0), Some("off-empty"));
        assert_eq!(grid_id(false, 1), None);
        assert_eq!(paste_id(false, false, false), Some("tb-off-paste"));
        assert_eq!(paste_id(false, true, false), Some("tb-off-paste-here"));
        assert_eq!(paste_id(false, true, true), None);
        assert_eq!(rename_id(2, true), Some("rename-one-only"));
        assert_eq!(rename_id(1, false), Some("tb-off-nothing"));
        assert_eq!(rename_id(1, true), None);
        assert_eq!(parent_id(false, true), Some("off-top"));
        assert_eq!(parent_id(false, false), None);
        // Restore in the menus: only inside a trash, greyed out there with nothing chosen.
        assert_eq!(restore_ids(Some(true), false), Some(("menu-restore-all", None)));
        assert_eq!(restore_ids(Some(false), true), Some(("menu-restore", None)));
        assert_eq!(restore_ids(None, true), Some(("menu-restore", Some("off-restore"))));
        assert_eq!(restore_ids(None, false), None);
        // Save over the original: a turned picture only, and why not on the other kinds.
        assert_eq!(save_id(Some(Kind::Picture), true), None);
        assert_eq!(save_id(Some(Kind::Picture), false), Some("tb-off-save"));
        assert_eq!(save_id(Some(Kind::Model), false), Some("off-save-model"));
        assert_eq!(save_id(Some(Kind::Drawing), false), Some("off-save-drawing"));
        assert_eq!(save_id(Some(Kind::Video), false), Some("off-save-played"));
        assert_eq!(save_id(Some(Kind::Audio), false), Some("off-save-played"));
        assert_eq!(save_id(None, false), Some("tb-off-save"));
        // Save as…: what the viewer shows, but not a video or an audio file; one picture in the grid.
        assert_eq!(save_as_id(Some(Kind::Model), 0), None);
        assert_eq!(save_as_id(Some(Kind::Audio), 0), Some("off-save-as-played"));
        assert_eq!(save_as_id(None, 1), None);
        assert_eq!(save_as_id(None, 0), Some("tb-off-one-picture"));
        assert_eq!(save_as_id(None, 2), Some("tb-off-one-picture"));
        // Each reason said in both languages.
        let ids = ["off-trash-clip", "off-trash-rename", "off-trash-change", "off-restore", "off-save-model", "off-save-drawing", "off-save-played", "off-save-as-played", "tb-off-nothing", "tb-off-viewer", "off-empty", "tb-off-paste", "tb-off-paste-here", "rename-one-only", "off-top", "off-not-viewer", "off-no-show", "off-no-file", "tb-off-save", "tb-off-folder", "tb-off-no-folder", "tb-off-not-picture", "tb-off-no-picture"];
        for code in ["en", "it"] {
            let i18n = crate::i18n::I18n::new(code);
            for id in ids {
                assert_ne!(i18n.t(id), id, "{code}: {id}");
            }
        }
    }
}
