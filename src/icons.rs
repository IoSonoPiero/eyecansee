//! Coloured icons (style "B", chosen by the user on 2026-10-22), the same on Windows and Linux.
//! Drawn from SVG with resvg, like SVG images, and kept as textures, one per icon and size in
//! pixels: sharp at every thumbnail size. The font glyphs used before had one colour only.

use std::collections::HashMap;
use std::sync::Mutex;

use eframe::egui::{self, ColorImage, Rect, TextureHandle, TextureOptions, vec2};
use resvg::{tiny_skia, usvg};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Folder,
    /// The ".." cell of the grid.
    Up,
    /// Toolbar: parent folder.
    UpButton,
    Refresh,
    Info,
    /// Toolbar in the viewer: back to the grid.
    Grid,
    Phone,
    /// External disk, USB stick, SD card.
    Disk,
    /// Disk inside the computer.
    InternalDisk,
    Trash,
    Home,
    /// Badge of the items copied with Ctrl+C.
    Copied,
    /// Badge of the items cut with Ctrl+X.
    Cut,
    /// Viewer: turn a quarter clockwise and anticlockwise.
    RotateCw,
    RotateCcw,
    /// Badge of the videos in the grid.
    Video,
    /// Audio files in the grid: badge on a cover, or the whole cell without one.
    Audio,
    /// A saved server in the left panel.
    Server,
}

const VIDEO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11" fill="#000000" fill-opacity="0.6"/><path d="M9.5 7.2v9.6l7.6-4.8z" fill="#ffffff"/></svg>"##;
const AUDIO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11" fill="#7b5cc4"/><path d="M10 16.5V7.5l7-1.5v8.5" fill="none" stroke="#ffffff" stroke-width="1.8" stroke-linejoin="round"/><circle cx="8.3" cy="16.5" r="2.1" fill="#ffffff"/><circle cx="15.3" cy="14.5" r="2.1" fill="#ffffff"/></svg>"##;
const FOLDER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2.5 6A1.5 1.5 0 0 1 4 4.5h5.2l2 2H20A1.5 1.5 0 0 1 21.5 8v10A1.5 1.5 0 0 1 20 19.5H4A1.5 1.5 0 0 1 2.5 18z" fill="#d99a1e"/><path d="M2.5 9A1.5 1.5 0 0 1 4 7.5h16A1.5 1.5 0 0 1 21.5 9v9a1.5 1.5 0 0 1-1.5 1.5H4A1.5 1.5 0 0 1 2.5 18z" fill="#f4b63f"/></svg>"##;
const UP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#e3edfb"/><path d="M12 17V7.5M7.5 11.5l4.5-4.5 4.5 4.5" fill="none" stroke="#1a62c9" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const UP_BUTTON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#1a62c9"/><path d="M12 17V8M8 11.5l4-4 4 4" fill="none" stroke="#ffffff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const REFRESH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" fill="none" stroke="#1f9d55" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const INFO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#1a62c9"/><path d="M12 11v6M12 7.5v.5" fill="none" stroke="#ffffff" stroke-width="2.4" stroke-linecap="round"/></svg>"##;
const GRID: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3" y="3" width="8" height="8" rx="2" fill="#1a62c9"/><rect x="13" y="3" width="8" height="8" rx="2" fill="#f4b63f"/><rect x="3" y="13" width="8" height="8" rx="2" fill="#1f9d55"/><rect x="13" y="13" width="8" height="8" rx="2" fill="#e05a4b"/></svg>"##;
const PHONE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="1.5" width="12" height="21" rx="2.5" fill="#1f9d55"/><rect x="8" y="4" width="8" height="14" rx="1" fill="#d7f5e3"/></svg>"##;
const DISK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2" y="6" width="20" height="12" rx="3" fill="#5b6b86"/><circle cx="6.5" cy="12" r="1.5" fill="#7ee2a8"/></svg>"##;
const INTERNAL_DISK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2" y="6" width="20" height="12" rx="3" fill="#1a62c9"/><circle cx="6.5" cy="12" r="1.5" fill="#cfe0fa"/></svg>"##;
const TRASH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="4" y="5" width="16" height="3" rx="1" fill="#c0392b"/><path d="M9 5V3.5h6V5" fill="none" stroke="#c0392b" stroke-width="1.8"/><path d="M6 9h12l-1 12H7z" fill="#e05a4b"/><path d="M10 12v6M14 12v6" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round"/></svg>"##;
const SERVER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3" y="3.5" width="18" height="7.5" rx="2" fill="#7b5cc4"/><rect x="3" y="13" width="18" height="7.5" rx="2" fill="#7b5cc4"/><circle cx="7" cy="7.25" r="1.3" fill="#7ee2a8"/><circle cx="7" cy="16.75" r="1.3" fill="#7ee2a8"/><path d="M11 7.25h6M11 16.75h6" stroke="#e4dcf7" stroke-width="1.6" stroke-linecap="round"/></svg>"##;
const HOME: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3 11 12 3.5 21 11v9.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" fill="#1a62c9"/><rect x="9.5" y="14" width="5" height="7.5" rx="1" fill="#f4b63f"/></svg>"##;

const COPIED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11.5" fill="#1a62c9"/><rect x="6" y="5" width="8.5" height="10.5" rx="1.5" fill="none" stroke="#ffffff" stroke-width="1.8"/><rect x="9.5" y="8.5" width="8.5" height="10.5" rx="1.5" fill="#1a62c9" stroke="#ffffff" stroke-width="1.8"/></svg>"##;
const CUT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11.5" fill="#d9541e"/><circle cx="8" cy="16.5" r="2.4" fill="none" stroke="#ffffff" stroke-width="1.8"/><circle cx="16" cy="16.5" r="2.4" fill="none" stroke="#ffffff" stroke-width="1.8"/><path d="M9.6 14.6 16.5 5M14.4 14.6 7.5 5" stroke="#ffffff" stroke-width="1.8" stroke-linecap="round"/></svg>"##;

const ROTATE_CW: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3" y="10" width="11" height="11" rx="2" fill="#f4b63f"/><path d="M8 6.5h5a5 5 0 0 1 5 5V14" fill="none" stroke="#1a62c9" stroke-width="2.4" stroke-linecap="round"/><path d="M15 12.5l3 3 3-3" fill="none" stroke="#1a62c9" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const ROTATE_CCW: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="10" y="10" width="11" height="11" rx="2" fill="#f4b63f"/><path d="M16 6.5h-5a5 5 0 0 0-5 5V14" fill="none" stroke="#1a62c9" stroke-width="2.4" stroke-linecap="round"/><path d="M9 12.5l-3 3-3-3" fill="none" stroke="#1a62c9" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;

fn svg(icon: Icon) -> &'static str {
    match icon {
        Icon::Folder => FOLDER,
        Icon::Up => UP,
        Icon::UpButton => UP_BUTTON,
        Icon::Refresh => REFRESH,
        Icon::Info => INFO,
        Icon::Grid => GRID,
        Icon::Phone => PHONE,
        Icon::Disk => DISK,
        Icon::InternalDisk => INTERNAL_DISK,
        Icon::Trash => TRASH,
        Icon::Home => HOME,
        Icon::Copied => COPIED,
        Icon::Cut => CUT,
        Icon::RotateCw => ROTATE_CW,
        Icon::RotateCcw => ROTATE_CCW,
        Icon::Video => VIDEO,
        Icon::Audio => AUDIO,
        Icon::Server => SERVER,
    }
}

static TEXTURES: Mutex<Option<HashMap<(Icon, u32), TextureHandle>>> = Mutex::new(None);

/// The icon `side` points wide, drawn for the screen's pixel density. Sizes are rounded to 4
/// pixels: the thumbnail slider would otherwise make a texture for every step.
fn texture(ctx: &egui::Context, icon: Icon, side: f32) -> TextureHandle {
    let px = ((side * ctx.pixels_per_point() / 4.0).ceil() as u32 * 4).clamp(8, 1024);
    let mut cache = TEXTURES.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    cache
        .entry((icon, px))
        .or_insert_with(|| ctx.load_texture(format!("icon-{icon:?}-{px}"), render(icon, px), TextureOptions::LINEAR))
        .clone()
}

fn render(icon: Icon, px: u32) -> ColorImage {
    // The SVGs are ours and valid: a failure would be a bug, shown as an empty square.
    let Ok(tree) = usvg::Tree::from_str(svg(icon), &usvg::Options::default()) else {
        return ColorImage::new([px as usize; 2], vec![egui::Color32::TRANSPARENT; (px * px) as usize]);
    };
    let scale = px as f32 / tree.size().width().max(tree.size().height());
    let mut pixmap = tiny_skia::Pixmap::new(px, px).expect("icon size is not zero");
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let data: Vec<u8> = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect();
    ColorImage::from_rgba_unmultiplied([px as usize; 2], &data)
}

/// The icon as an image for buttons, labels and tree rows, `side` points wide.
pub fn image(ctx: &egui::Context, icon: Icon, side: f32) -> egui::Image<'static> {
    let tex = texture(ctx, icon, side);
    egui::Image::new(egui::load::SizedTexture::new(tex.id(), vec2(side, side)))
}

/// Draws the icon in `rect` (square), e.g. in the middle of a grid cell.
pub fn paint(painter: &egui::Painter, icon: Icon, rect: Rect) {
    let tex = texture(painter.ctx(), icon, rect.width());
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(tex.id(), rect, uv, egui::Color32::WHITE);
}
