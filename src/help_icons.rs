//! The icons inside the text of the illustrated manual: `![Save](icon://save)` in a page draws,
//! in the line of text, the same icon as the button. A texture loader for the `icon://` scheme,
//! installed at start: the names are the Lucide files of `assets/icons/lucide/` (all of them,
//! also those only used by stamps, filters and bubbles), our own line icons and the coloured ones
//! with a `color-` prefix. Line icons take the colour of the text of the theme in use, coloured
//! ones keep theirs; the height is that of the body text.
//!
//! A texture loader, not an image loader: egui keeps the textures made from an image loader by
//! address only, so a change of theme (also the system's, while the program runs) would leave the
//! icons in the old colour. Here the colour is part of the key, read at every frame.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions, vec2};
use egui::load::{LoadError, SizeHint, SizedTexture, TextureLoadResult, TexturePoll};

use crate::edit::bubble::Balloon;
use crate::icons::{Icon, Line};

/// The scheme of the icon addresses.
pub(crate) const SCHEME: &str = "icon://";

/// Name and SVG of every file of `assets/icons/lucide/`, as the file is.
macro_rules! lucide_table {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../assets/icons/lucide/", $name, ".svg")))),*]
    };
}

const LUCIDE: &[(&str, &str)] = lucide_table![
    "arrow-big-down", "arrow-big-left", "arrow-big-right", "arrow-big-up", "arrow-down-wide-narrow", "arrow-left-right",
    "arrow-up-narrow-wide", "arrow-up", "award", "badge-check", "ban", "bell",
    "bookmark", "book-open", "calendar", "camera", "car", "check",
    "chevron-down", "chevron-up", "circle-check", "circle-exclamation-point", "circle-minus", "circle-plus",
    "circle-question-mark", "circle", "circle-x", "clipboard-paste", "clock", "cloud",
    "coffee", "contrast", "copy", "crop", "crown", "download",
    "droplets", "eclipse", "ellipse", "ellipsis", "eye-off", "eye",
    "face-grinning", "face-slightly-frowning", "face-slightly-smiling", "file-output", "flag", "flame",
    "focus", "folder-plus", "gift", "grid-3x3", "hand", "heart",
    "highlighter", "house", "image-plus", "images", "image", "info",
    "layout-grid", "lightbulb", "link", "list-ordered", "lock-open", "lock",
    "log-out", "mail", "map-pin", "message-circle", "message-square", "moon",
    "mouse-pointer-2", "move-up-right", "party-popper", "pause", "pencil", "phone",
    "pipette", "plane", "play", "presentation", "redo-2", "refresh-cw",
    "rocket", "rotate-ccw-square", "rotate-cw-square", "save", "scaling", "scan-search",
    "scissors", "search", "server", "settings", "shield-check", "slash",
    "sparkles", "spotlight", "square-dashed", "square-pen", "square-split-horizontal", "square-split-vertical",
    "square", "stamp", "star", "sun", "target", "text-cursor-input",
    "thumbs-down", "thumbs-up", "trash", "triangle-exclamation-point", "trophy", "type",
    "undo-2", "users", "user", "volume-2", "volume-x", "wand-sparkles",
    "x", "zap", "zoom-in", "zoom-out",
];

/// Our line icons (`icons::line_svg`): the heads and kinds of line, the bubbles not from Lucide.
const OWN: [(&str, Line); 10] = [
    ("head-filled", Line::HeadFilled),
    ("head-open", Line::HeadOpen),
    ("head-double", Line::HeadDouble),
    ("head-dot", Line::HeadDot),
    ("line-solid", Line::Solid),
    ("line-dashed", Line::Dashed),
    ("line-dotted", Line::Dotted),
    ("balloon-square", Line::Balloon(Balloon::Square)),
    ("balloon-cloud", Line::Balloon(Balloon::Cloud)),
    ("balloon-shout", Line::Balloon(Balloon::Shout)),
];

/// The coloured icons, with the `color-` prefix.
const COLOR: [(&str, Icon); 14] = [
    ("color-folder", Icon::Folder),
    ("color-up", Icon::Up),
    ("color-phone", Icon::Phone),
    ("color-disk", Icon::Disk),
    ("color-internal-disk", Icon::InternalDisk),
    ("color-trash", Icon::Trash),
    ("color-home", Icon::Home),
    ("color-copied", Icon::Copied),
    ("color-cut", Icon::Cut),
    ("color-video", Icon::Video),
    ("color-audio", Icon::Audio),
    ("color-server", Icon::Server),
    ("color-model", Icon::Model),
    ("color-download", Icon::Download),
];

/// Every name valid after `icon://` (for the checks of the pages, `help_check`).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn icon_names() -> impl Iterator<Item = &'static str> {
    LUCIDE.iter().map(|(n, _)| *n).chain(OWN.iter().map(|(n, _)| *n)).chain(COLOR.iter().map(|(n, _)| *n))
}

/// Whether `icon://<name>` draws an icon.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn icon_exists(name: &str) -> bool {
    icon_names().any(|n| n == name)
}

/// The SVG of an icon, and whether it is a line icon (its `currentColor` to be replaced).
fn icon_svg(name: &str) -> Option<(std::borrow::Cow<'static, str>, bool)> {
    if let Some((_, svg)) = LUCIDE.iter().find(|(n, _)| *n == name) {
        return Some(((*svg).into(), true));
    }
    if let Some((_, line)) = OWN.iter().find(|(n, _)| *n == name) {
        return Some((crate::icons::line_svg(*line), true));
    }
    COLOR.iter().find(|(n, _)| *n == name).map(|(_, icon)| (crate::icons::svg(*icon).into(), false))
}

/// The icon `name` drawn `px` pixels square, a line icon in `color` (`[r, g, b]`).
pub(crate) fn icon_image(name: &str, px: u32, color: [u8; 3]) -> Option<ColorImage> {
    let (svg, line) = icon_svg(name)?;
    let svg = if line {
        let [r, g, b] = color;
        svg.replace("currentColor", &format!("#{r:02x}{g:02x}{b:02x}")).into()
    } else {
        svg
    };
    Some(crate::icons::render(&svg, px))
}

/// How much an icon is raised, as a share of its height. egui puts an image of a line of text on
/// the bottom of the line, under the baseline: raised so, its middle is that of the capitals
/// (measured on screenshots in both themes, 2026-11-10).
const RAISE: f32 = 0.08;

/// The image with `rows` transparent rows added below: the line puts that bottom on its own.
fn with_room_below(image: ColorImage, rows: usize) -> ColorImage {
    let [w, h] = image.size;
    let mut pixels = image.pixels;
    pixels.resize(w * (h + rows), egui::Color32::TRANSPARENT);
    ColorImage::new([w, h + rows], pixels)
}

type Key = (String, u32, [u8; 4]);

/// Answers the `icon://` addresses; the textures by name, size in pixels and colour.
#[derive(Default)]
struct IconLoader {
    textures: Mutex<HashMap<Key, TextureHandle>>,
}

impl egui::load::TextureLoader for IconLoader {
    fn id(&self) -> &str {
        concat!(module_path!(), "::IconLoader")
    }

    fn load(&self, ctx: &egui::Context, uri: &str, _: TextureOptions, _: SizeHint) -> TextureLoadResult {
        let Some(name) = uri.strip_prefix(SCHEME) else { return Err(LoadError::NotSupported) };
        let style = ctx.global_style();
        let side = style.text_styles.get(&egui::TextStyle::Body).map_or(14.0, |f| f.size);
        let color = style.visuals.text_color();
        let px = (side * ctx.pixels_per_point()).round().max(1.0) as u32;
        let room = (px as f32 * RAISE).round() as usize;
        let key = (name.to_owned(), px, color.to_array());
        let mut textures = self.textures.lock().unwrap();
        let tex = match textures.get(&key) {
            Some(tex) => tex.clone(),
            None => {
                let [r, g, b, _] = key.2;
                let image = icon_image(name, px, [r, g, b]).ok_or_else(|| LoadError::Loading(format!("no icon {name:?}")))?;
                let image = with_room_below(image, room);
                let tex = ctx.load_texture(uri, image, TextureOptions::LINEAR);
                textures.insert(key, tex.clone());
                tex
            }
        };
        Ok(TexturePoll::Ready { texture: SizedTexture::new(tex.id(), vec2(side, side * (px as usize + room) as f32 / px as f32)) })
    }

    fn forget(&self, uri: &str) {
        if let Some(name) = uri.strip_prefix(SCHEME) {
            self.textures.lock().unwrap().retain(|(n, _, _), _| n != name);
        }
    }

    fn forget_all(&self) {
        self.textures.lock().unwrap().clear();
    }

    fn byte_size(&self) -> usize {
        self.textures.lock().unwrap().values().map(|t| t.byte_size()).sum()
    }
}

/// At start: `icon://` addresses draw the icons, in every `egui::Image`.
pub(crate) fn install(ctx: &egui::Context) {
    ctx.add_texture_loader(Arc::new(IconLoader::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Lucide table holds exactly the files of the folder: a new icon is added to it too.
    #[test]
    fn lucide_table_matches_folder() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons/lucide");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.unwrap().file_name().into_string().ok())
            .filter_map(|f| f.strip_suffix(".svg").map(str::to_owned))
            .collect();
        files.sort();
        let mut table: Vec<String> = LUCIDE.iter().map(|(n, _)| (*n).to_owned()).collect();
        table.sort();
        assert_eq!(table, files);
    }

    /// Names unique, lower case with hyphens, and found by `icon_exists`.
    #[test]
    fn names_unique_and_plain() {
        let names: Vec<&str> = icon_names().collect();
        assert_eq!(names.len(), LUCIDE.len() + OWN.len() + COLOR.len());
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len());
        for n in names {
            assert!(n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{n}");
            assert!(icon_exists(n));
        }
        assert!(!icon_exists("no-such-icon"));
        assert!(icon_image("no-such-icon", 16, [0; 3]).is_none());
    }

    /// The room below is transparent and the icon above it unchanged.
    #[test]
    fn room_below_is_transparent() {
        let image = icon_image("color-folder", 20, [0; 3]).unwrap();
        let raised = with_room_below(image.clone(), 2);
        assert_eq!(raised.size, [20, 22]);
        assert_eq!(raised.pixels[..400], image.pixels[..]);
        assert!(raised.pixels[400..].iter().all(|p| *p == egui::Color32::TRANSPARENT));
    }

    /// Through egui: the size is that of the body text (a little taller, the room below), another
    /// scheme is left to the other loaders, and a change of theme gives the icon a new texture.
    #[test]
    fn loader_follows_theme() {
        let ctx = egui::Context::default();
        install(&ctx);
        let load = |uri: &str| ctx.try_load_texture(uri, TextureOptions::LINEAR, SizeHint::Scale(1.0.into()));
        ctx.set_visuals(egui::Visuals::dark());
        let Ok(TexturePoll::Ready { texture: dark }) = load("icon://save") else { panic!("no icon") };
        let body = ctx.global_style().text_styles[&egui::TextStyle::Body].size;
        assert_eq!(dark.size.x, body);
        assert!(dark.size.y > body && dark.size.y < body * 1.2, "{:?}", dark.size);
        assert!(matches!(load("icon://no-such-icon"), Err(LoadError::Loading(_))));
        let other = egui::load::TextureLoader::load(&IconLoader::default(), &ctx, "bytes://help/x.webp", TextureOptions::LINEAR, SizeHint::Scale(1.0.into()));
        assert!(matches!(other, Err(LoadError::NotSupported)));
        ctx.set_visuals(egui::Visuals::light());
        let Ok(TexturePoll::Ready { texture: light }) = load("icon://save") else { panic!("no icon") };
        assert_ne!(dark.id, light.id);
    }

    /// Every icon is an SVG resvg reads, with something drawn: line icons in the colour asked.
    #[test]
    fn every_icon_draws() {
        for name in icon_names() {
            let img = icon_image(name, 24, [255, 0, 0]).unwrap();
            assert_eq!(img.size, [24, 24]);
            let drawn = img.pixels.iter().filter(|p| p.a() > 128).count();
            assert!(drawn > 10, "{name}: {drawn} pixels");
            if !name.starts_with("color-") {
                let red = img.pixels.iter().filter(|p| p.a() > 128 && p.r() > 200 && p.g() < 60).count();
                assert!(red > 10, "{name}: {red} red pixels");
            }
        }
    }
}
