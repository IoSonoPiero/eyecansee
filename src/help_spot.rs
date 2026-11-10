//! F1 opens the guide on the page of what is in front of the user: the window on top, else the
//! page open (the editor with its tool, Save in another format, the slideshow, the duplicates),
//! else the viewer by kind of file, else the device or server open, else the grid. A spot is
//! always the id of a page of the guide, written as a `"help:<id>"` literal (the test of the
//! guide finds every one and checks that the page exists); a spot without a page of its own gives
//! the page of its area.

use crate::App;
use crate::devices::Device;
use crate::edit::geom::Kind;
use crate::edit_page::Tool;

/// What the editor shows, for its page (`edit_page::Window::help_view`).
pub(crate) struct EditView {
    pub tool: Tool,
    /// The kind of the object selected, if any.
    pub selected: Option<Kind>,
    /// Saving: the copy's or the overwrite's confirmation, or the question on closing unsaved.
    pub saving: bool,
    /// The eyedropper is on.
    pub picking: bool,
}

impl App {
    /// The page of the guide for what is in front of the user, as `"help:<id>"`.
    pub(crate) fn help_spot(&self) -> &'static str {
        let page = self.window_spot().or_else(|| self.page_spot());
        page.or_else(|| self.viewer_spot()).or_else(|| place_spot(&self.base, &self.dir, &self.devices)).unwrap_or("help:grid")
    }

    /// The window on top, if one is open: the last ones drawn by `windows()` first (the modals
    /// opened from another window come after it, as the unlock from the server manager).
    fn window_spot(&self) -> Option<&'static str> {
        let pasting = self.paste_ask.as_ref().is_some_and(|a| a.conflicts.as_ref().is_some_and(|c| !c.is_empty()));
        let layers = [
            self.restore_conflict.is_some().then_some("help:delete"),
            self.new_image.is_some().then_some("help:new-image"),
            (self.new_folder.is_some() || self.rename.is_some()).then_some("help:rename"),
            (self.transfer.is_some() || pasting).then_some("help:copy-move"),
            self.unlock.is_some().then_some("help:passwords"),
            self.bookmarks.configuring.then_some("help:bookmarks"),
            self.server_manager.is_some().then_some("help:server-manager"),
            self.open_address.is_some().then_some("help:address"),
            self.host_ask.then_some("help:servers"),
            self.kiod_ask.then_some("help:phone"),
            self.kept.first().map(|k| if k.store.is_none() { "help:phone" } else { "help:servers" }),
            // A notice has no page: F1 gives the one of what is under it.
            (self.batch_turn.is_some() || self.save_ready.is_some() || self.after_turn.is_some()).then_some("help:rotate"),
            self.delete_confirm.is_some().then_some("help:delete"),
            self.batch.as_ref().and(self.batch_form.as_ref()).map(|f| batch_spot(f.videos_only(), f.drawings())),
            self.duplicates.as_ref().is_some_and(crate::dup_page::Window::asking).then_some("help:duplicates"),
            self.options.then_some("help:options"),
            self.update_note.is_some().then_some("help:updates"),
            self.about.then_some("help:start"),
        ];
        layers.into_iter().flatten().next()
    }

    /// The page that covers the window, in the order `panels()` draws them.
    fn page_spot(&self) -> Option<&'static str> {
        if self.show.is_some() {
            Some("help:slideshow")
        } else if self.convert.is_some() {
            Some("help:save-as")
        } else if let Some(w) = &self.editor {
            Some(editor_spot(&w.help_view()))
        } else if self.dup_page_shown() {
            Some("help:duplicates")
        } else {
            None
        }
    }

    fn viewer_spot(&self) -> Option<&'static str> {
        self.viewing.and_then(|i| self.files().get(i)).map(|f| file_spot(&f.path))
    }
}

/// Convert and resize: on videos only (or their sound), on CAD drawings, or on pictures.
fn batch_spot(videos_only: bool, drawings: bool) -> &'static str {
    match (videos_only, drawings) {
        (true, _) => "help:convert-video",
        (false, true) => "help:cad",
        (false, false) => "help:convert",
    }
}

/// The editor: saving, the eyedropper, then the tool in use, or with the pointer the kind of the
/// object selected.
fn editor_spot(v: &EditView) -> &'static str {
    if v.saving {
        return "help:editor-save";
    }
    if v.picking {
        return "help:editor-colors";
    }
    match v.tool {
        Tool::Crop => "help:editor-crop",
        Tool::Marquee => "help:editor-marquee",
        Tool::Draw(kind) => kind_spot(kind),
        Tool::Select => v.selected.map_or("help:editor", kind_spot),
    }
}

fn kind_spot(kind: Kind) -> &'static str {
    match kind {
        Kind::Line | Kind::Arrow => "help:editor-lines",
        Kind::Pen | Kind::Highlight | Kind::Rect | Kind::Ellipse => "help:editor-draw",
        Kind::Text | Kind::Bubble | Kind::Step => "help:editor-text",
        Kind::Lens | Kind::Spotlight => "help:editor-lens",
        Kind::Stamp => "help:editor-stamps",
        Kind::Cover => "help:editor-cover",
        Kind::Filter => "help:editor-filters",
        // A part pasted: copied or cut with Selection.
        Kind::Patch => "help:editor-marquee",
    }
}

/// The viewer, by kind of file, in the order `viewer()` tries them.
fn file_spot(path: &str) -> &'static str {
    if crate::decode::is_played(path) {
        "help:video"
    } else if crate::model::is_model(path) {
        "help:model-3d"
    } else if crate::cad::is_cad(path) {
        "help:cad"
    } else {
        "help:viewer"
    }
}

/// The grid of a phone, a server or an external disk; `None` for the computer's folders.
fn place_spot(base: &str, dir: &str, devices: &[Device]) -> Option<&'static str> {
    if base.starts_with("mtp://") {
        return Some("help:phone");
    }
    if ["ftp://", "ftps://", "sftp://"].iter().any(|s| base.starts_with(s)) {
        return Some("help:servers");
    }
    let here = crate::navigation::shown(base, dir);
    let device = devices.iter().find(|d| !d.internal && !d.path.is_empty() && (d.path == base || here.starts_with(&d.path)))?;
    Some(if device.phone { "help:phone" } else { "help:devices" })
}

// placeholder until help.rs
impl App {
    pub(crate) fn toggle_help(&mut self, _spot: &str) {
        self.help = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(tool: Tool) -> EditView {
        EditView { tool, selected: None, saving: false, picking: false }
    }

    #[test]
    fn editor_by_tool() {
        assert_eq!(editor_spot(&view(Tool::Select)), "help:editor");
        assert_eq!(editor_spot(&view(Tool::Crop)), "help:editor-crop");
        assert_eq!(editor_spot(&view(Tool::Marquee)), "help:editor-marquee");
        assert_eq!(editor_spot(&view(Tool::Draw(Kind::Arrow))), "help:editor-lines");
        assert_eq!(editor_spot(&view(Tool::Draw(Kind::Highlight))), "help:editor-draw");
        assert_eq!(editor_spot(&view(Tool::Draw(Kind::Bubble))), "help:editor-text");
        assert_eq!(editor_spot(&view(Tool::Draw(Kind::Spotlight))), "help:editor-lens");
        assert_eq!(editor_spot(&view(Tool::Draw(Kind::Stamp))), "help:editor-stamps");
        assert_eq!(editor_spot(&view(Tool::Draw(Kind::Cover))), "help:editor-cover");
        // With the pointer, the object selected; saving and the eyedropper come first.
        let filter = EditView { selected: Some(Kind::Filter), ..view(Tool::Select) };
        assert_eq!(editor_spot(&filter), "help:editor-filters");
        assert_eq!(editor_spot(&EditView { picking: true, ..view(Tool::Draw(Kind::Pen)) }), "help:editor-colors");
        assert_eq!(editor_spot(&EditView { saving: true, picking: true, ..view(Tool::Crop) }), "help:editor-save");
    }

    #[test]
    fn viewer_by_file() {
        assert_eq!(file_spot("Holiday/beach.JPG"), "help:viewer");
        assert_eq!(file_spot("Holiday/sea.mp4"), "help:video");
        assert_eq!(file_spot("song.flac"), "help:video");
        assert_eq!(file_spot("parts/gear.stl"), "help:model-3d");
        assert_eq!(file_spot("plans/house.dxf"), "help:cad");
    }

    #[test]
    fn places() {
        let disk = Device { name: "USB".into(), path: "/media/USB/".into(), phone: false, serial: None, block: None, internal: false };
        let gvfs = Device { name: "Phone".into(), path: "/run/gvfs/mtp:host=A/".into(), phone: true, ..disk.clone() };
        let devices = [disk, gvfs];
        assert_eq!(place_spot("mtp://ABC/", "DCIM/", &devices), Some("help:phone"));
        assert_eq!(place_spot("sftp://me@host/", "", &devices), Some("help:servers"));
        assert_eq!(place_spot("ftp://host/", "", &devices), Some("help:servers"));
        assert_eq!(place_spot("/", "media/USB/Pictures/", &devices), Some("help:devices"));
        assert_eq!(place_spot("/media/USB/", "", &devices), Some("help:devices"));
        assert_eq!(place_spot("/", "run/gvfs/mtp:host=A/DCIM/", &devices), Some("help:phone"));
        assert_eq!(place_spot("/", "home/Pictures/", &devices), None);
    }

    #[test]
    fn batch_by_files() {
        assert_eq!(batch_spot(true, false), "help:convert-video");
        assert_eq!(batch_spot(false, true), "help:cad");
        assert_eq!(batch_spot(false, false), "help:convert");
    }
}
