//! The editor opened: on which image Edit image… and Rotate and crop… act (or why not), the page
//! opened on it or on a new image, and the document started once the image is read.

use std::sync::Arc;

use eframe::egui::{self, ColorImage};

use super::{Place, Tool, Window};
use crate::App;
use crate::edit::doc::Doc;
use crate::edit::history::History;
use crate::encode::Original;
use crate::rotate::Crop;
use crate::transform::Turn;

impl App {
    /// The image Edit image… and Rotate and crop… would open: one picture (in the viewer the one
    /// shown); `None` too for one the viewer found animated or of several pages.
    pub(crate) fn edit_target(&self, path: Option<String>) -> Option<String> {
        let picture = if self.viewing.is_some() {
            self.viewing_photo().then(|| self.target()).flatten()
        } else {
            let images = self.grid_images(path);
            if images.len() == 1 { images.into_iter().next() } else { None }
        };
        picture.filter(|p| !self.animated.contains(p))
    }

    pub(crate) fn can_edit(&self, path: Option<String>) -> bool {
        self.edit_target(path).is_some()
    }

    /// Why the editor is off for the item: several images, or one found animated.
    pub(crate) fn edit_off(&self, path: Option<String>) -> Option<String> {
        if self.viewing.is_none() && self.grid_images(path.clone()).len() > 1 {
            Some(self.i18n.t("rc-one-only"))
        } else if path.or_else(|| self.target()).is_some_and(|p| self.animated.contains(&p)) {
            Some(self.i18n.t("edit-animated-off"))
        } else {
            None
        }
    }

    /// Edit image… (`Tool::Select`) or Rotate and crop… (`Tool::Crop`) on one image.
    pub(crate) fn open_editor(&mut self, path: Option<String>, tool: Tool) {
        // In the grid the one picture among the files chosen (a video next to it is left out).
        let picture = if self.viewing.is_some() { None } else { self.grid_images(path.clone()).into_iter().next() };
        let Some(path) = picture.or(path).or_else(|| self.target()).filter(|p| crate::decode::is_image(p)) else { return };
        if self.animated.contains(&path) {
            return;
        }
        let Some(loader) = &self.loader else { return };
        loader.load_original(path.clone(), true);
        // From the viewer: as it is shown there.
        let turn = if self.viewing.is_some() { self.turn } else { Turn::default() };
        let mut w = Window::new(path, turn, tool);
        let store = loader.store();
        w.remote = crate::kept::remote(&store).then_some(if matches!(store, crate::source::Store::Mtp(_)) { "edit-remote-phone" } else { "edit-remote-server" });
        // Every save goes where the image is, whatever the grid opens meanwhile.
        w.place = Some(Place { store, base: self.base.clone() });
        // Fractions of the long side until the image is read (`editor_original`).
        w.styles = self.settings.edit_styles.clone();
        self.editor = Some(w);
    }

    /// File > New image…: the empty image in the editor, named in the folder open; Save as…
    /// writes it.
    pub(crate) fn open_new_image(&mut self, ctx: &egui::Context, image: image::DynamicImage, name: &str) {
        let Some(loader) = &self.loader else { return };
        let mut w = Window::new(format!("{}{name}.png", self.dir), Turn::default(), Tool::Select);
        w.place = Some(Place { store: loader.store(), base: self.base.clone() });
        w.new = true;
        w.styles = self.settings.edit_styles.clone();
        self.editor = Some(w);
        let original = Arc::new(Original::with_meta(image, &[]));
        let preview = crate::decode::preview(&original.image);
        self.editor_original(ctx, Ok((original, preview, 0)));
    }

    pub(crate) fn editor_original(&mut self, ctx: &egui::Context, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        let Some(w) = &mut self.editor else { return };
        // The photo as read; at the document's size from the next frame (`follow_size`).
        w.full = r.as_ref().ok().map(|(original, ..)| original.clone());
        w.scaled = None;
        w.original = Some(r.map(|(original, preview, size)| (original, ctx.load_texture("edit-original", preview, egui::TextureOptions::LINEAR), size)));
        if w.history.is_some() {
            return;
        }
        // The document starts from the image as the viewer showed it.
        let (iw, ih) = w.upright();
        let mut doc = Doc::new(iw, ih, (!w.transparent).then(|| [w.color.r(), w.color.g(), w.color.b()]));
        doc.turn = w.start_turn;
        if doc.turn.sideways() {
            doc.crop = Crop::centred(ih as f32, iw as f32);
        }
        w.doc = doc;
        w.refit();
        w.history = Some(History::new(w.doc.clone()));
        // The widths remembered are fractions of the long side: in pixels of this image now.
        let long = w.long();
        for (_, s) in &mut w.styles {
            s.width = (s.width * long).max(1.0);
            s.letters *= long;
        }
    }
}
