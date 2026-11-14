//! The editor opened: on which image Edit image… and Rotate and crop… act (or why not), the page
//! opened on it or on a new image, and the document started once the image is read.

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui::{self, ColorImage};

use super::{Place, Tool, Window};
use crate::App;
use crate::edit::animated::{self as anim, HEAD};
use crate::edit::doc::Doc;
use crate::edit::history::History;
use crate::encode::Original;
use crate::rotate::Crop;
use crate::transform::Turn;

/// The pictures found animated or with several pages (the editor is off for them, Save as… says
/// it saves only the picture shown), and those already looked at (`probe_animated`).
#[derive(Default)]
pub(crate) struct Animated {
    found: HashSet<String>,
    /// Place and path (marked when read whole if needed): a file is read once.
    probed: HashSet<String>,
}

impl Animated {
    pub(crate) fn insert(&mut self, path: String) -> bool {
        self.found.insert(path)
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        self.found.contains(path)
    }
}

/// Read whole, to tell what its start does not, when on this computer and no larger than this.
const WHOLE_LOCAL: u64 = 64 << 20;

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

    /// In the grid, the one picture chosen is looked at before the editor is asked for: animated or
    /// with several pages, Edit image… is off there too, as in the viewer, which reads it whole.
    pub(crate) fn probe_chosen(&mut self, ctx: &egui::Context) {
        if self.viewing.is_some() || self.editor.is_some() || self.convert.is_some() {
            return;
        }
        if let [path] = self.grid_images(None).as_slice() {
            self.probe_animated(ctx, path, false);
        }
    }

    /// Reads the start of `path`, without decoding it, to know whether it is animated or has
    /// several pages (`animated`, through `Msg::Animated`). When the start does not tell, the
    /// whole file: with `whole`, or on this computer up to `WHOLE_LOCAL`. Once per file.
    pub(crate) fn probe_animated(&mut self, ctx: &egui::Context, path: &str, whole: bool) {
        if !anim::may_be(path) || self.animated.contains(path) || !self.animated.probed.insert(format!("{}{}{path}", if whole { "*" } else { "" }, self.base)) {
            return;
        }
        let Some(loader) = &self.loader else { return };
        let store = loader.store();
        // The size from the listing: a file shorter than `HEAD` is read whole at once.
        let size = self.listings.get(&crate::source::parent(path)).and_then(|l| l.as_ref().ok()).and_then(|l| l.files.iter().find(|f| f.path == path)).map(|f| f.size);
        let whole = whole || (!crate::kept::remote(&store) && size.is_some_and(|s| s <= WHOLE_LOCAL));
        let (tx, ctx, path) = (self.tx.clone(), ctx.clone(), path.to_string());
        self.rt.spawn(async move {
            let found = found_animated(&store, &path, size, whole).await;
            if found && tx.send(crate::loader::Msg::Animated(path)).is_ok() {
                ctx.request_repaint();
            }
        });
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
        // Opened before it was known: off from now on.
        if r.as_ref().is_err_and(|e| e == super::ANIMATED) {
            self.animated.insert(w.path.clone());
        }
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

/// Whether the picture `path` (`size` bytes, if known) is animated or has several pages: told by
/// its start, or by the whole file when short, or with `whole`; `false` when not known.
async fn found_animated(store: &crate::source::Store, path: &str, size: Option<u64>, whole: bool) -> bool {
    let short = size.is_none_or(|s| s <= HEAD);
    let head = if short { store.read(path).await } else { store.read_range(path, 0, HEAD).await };
    let Ok(head) = head else { return false };
    let told = if short { None } else { anim::from_head(&head, path) };
    let found = match told {
        Some(found) => found,
        None if short || whole => {
            let bytes = if short { head } else { store.read(path).await.unwrap_or_default() };
            let name = path.to_string();
            tokio::task::spawn_blocking(move || !bytes.is_empty() && crate::edit::save::animated(&bytes, &name)).await.unwrap_or(false)
        }
        None => false,
    };
    log!("animated {path:?}: {found} ({})", if told.is_some() { "from its start" } else { "read whole" });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Edit image… off from the grid too: an animated or multi-page file is found from its start,
    /// a large one whose start does not tell only when it may be read whole; a still one never.
    #[test]
    fn found_from_the_start_or_whole() {
        let dir = std::env::temp_dir().join(format!("eyecansee-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let small = image::DynamicImage::new_rgb8(8, 8);
        let mut png = Vec::new();
        small.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        // Two pages of noise: the directories come after more than `HEAD` bytes of pixels.
        let big = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(400, 400, |x, y| image::Rgb([(x * 7 + y) as u8, (y * 3) as u8, (x ^ y) as u8])));
        let files = [("still.png", png.clone()), ("moving.png", crate::encode::tests::apng(&png)), ("pages.tif", crate::encode::tests::tiff_pages(&big))];
        for (name, bytes) in &files {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        let store = crate::source::Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let size = |n: usize| Some(files[n].1.len() as u64);
        assert!(files[2].1.len() as u64 > HEAD && anim::from_head(&files[2].1[..HEAD as usize], "pages.tif").is_none());
        assert!(!rt.block_on(found_animated(&store, "still.png", size(0), false)));
        assert!(rt.block_on(found_animated(&store, "moving.png", size(1), false)));
        assert!(rt.block_on(found_animated(&store, "moving.png", None, false)));
        assert!(!rt.block_on(found_animated(&store, "pages.tif", size(2), false)));
        assert!(rt.block_on(found_animated(&store, "pages.tif", size(2), true)));
        assert!(!rt.block_on(found_animated(&store, "gone.png", None, true)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
