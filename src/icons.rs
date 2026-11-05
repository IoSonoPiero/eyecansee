//! Coloured icons (style "B", chosen by the user on 2026-10-22) for folders, devices and badges,
//! and line icons for every button (`Line`, from Lucide, chosen by the user on 2026-11-05), the
//! same on Windows and Linux. Drawn from SVG with resvg, like SVG images, and kept as textures,
//! one per icon, size in pixels (and colour): sharp at every size. The font glyphs used before had
//! one colour only.

use std::collections::HashMap;
use std::sync::Mutex;

use eframe::egui::{self, ColorImage, Rect, TextureHandle, TextureOptions, vec2};
use resvg::{tiny_skia, usvg};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Folder,
    /// The ".." cell of the grid.
    Up,
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
const PHONE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="1.5" width="12" height="21" rx="2.5" fill="#1f9d55"/><rect x="8" y="4" width="8" height="14" rx="1" fill="#d7f5e3"/></svg>"##;
const DISK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2" y="6" width="20" height="12" rx="3" fill="#5b6b86"/><circle cx="6.5" cy="12" r="1.5" fill="#7ee2a8"/></svg>"##;
const INTERNAL_DISK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2" y="6" width="20" height="12" rx="3" fill="#1a62c9"/><circle cx="6.5" cy="12" r="1.5" fill="#cfe0fa"/></svg>"##;
const TRASH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="4" y="5" width="16" height="3" rx="1" fill="#c0392b"/><path d="M9 5V3.5h6V5" fill="none" stroke="#c0392b" stroke-width="1.8"/><path d="M6 9h12l-1 12H7z" fill="#e05a4b"/><path d="M10 12v6M14 12v6" stroke="#ffffff" stroke-width="1.6" stroke-linecap="round"/></svg>"##;
const SERVER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3" y="3.5" width="18" height="7.5" rx="2" fill="#7b5cc4"/><rect x="3" y="13" width="18" height="7.5" rx="2" fill="#7b5cc4"/><circle cx="7" cy="7.25" r="1.3" fill="#7ee2a8"/><circle cx="7" cy="16.75" r="1.3" fill="#7ee2a8"/><path d="M11 7.25h6M11 16.75h6" stroke="#e4dcf7" stroke-width="1.6" stroke-linecap="round"/></svg>"##;
const HOME: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M3 11 12 3.5 21 11v9.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" fill="#1a62c9"/><rect x="9.5" y="14" width="5" height="7.5" rx="1" fill="#f4b63f"/></svg>"##;

const COPIED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11.5" fill="#1a62c9"/><rect x="6" y="5" width="8.5" height="10.5" rx="1.5" fill="none" stroke="#ffffff" stroke-width="1.8"/><rect x="9.5" y="8.5" width="8.5" height="10.5" rx="1.5" fill="#1a62c9" stroke="#ffffff" stroke-width="1.8"/></svg>"##;
const CUT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="11.5" fill="#d9541e"/><circle cx="8" cy="16.5" r="2.4" fill="none" stroke="#ffffff" stroke-width="1.8"/><circle cx="16" cy="16.5" r="2.4" fill="none" stroke="#ffffff" stroke-width="1.8"/><path d="M9.6 14.6 16.5 5M14.4 14.6 7.5 5" stroke="#ffffff" stroke-width="1.8" stroke-linecap="round"/></svg>"##;


fn svg(icon: Icon) -> &'static str {
    match icon {
        Icon::Folder => FOLDER,
        Icon::Up => UP,
        Icon::Phone => PHONE,
        Icon::Disk => DISK,
        Icon::InternalDisk => INTERNAL_DISK,
        Icon::Trash => TRASH,
        Icon::Home => HOME,
        Icon::Copied => COPIED,
        Icon::Cut => CUT,
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
        .or_insert_with(|| ctx.load_texture(format!("icon-{icon:?}-{px}"), render(svg(icon), px), TextureOptions::LINEAR))
        .clone()
}

fn render(svg: &str, px: u32) -> ColorImage {
    // The SVGs are ours or Lucide's, and valid: a failure would be a bug, shown as an empty square.
    let Ok(tree) = usvg::Tree::from_str(svg, &usvg::Options::default()) else {
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

/// The line icons of the buttons: Lucide's (ISC, some from Feather, MIT; the notices in
/// `assets/icons/lucide/LICENSE-lucide.txt` and in THIRD-PARTY-LICENSES.txt), only those used, as
/// Lucide gives them (commit ee8e10d of 2026-11-05); the three kinds of line are ours, same style.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Line {
    Up,
    Refresh,
    Info,
    Grid,
    RotateCcw,
    RotateCw,
    NewFolder,
    Copy,
    Cut,
    Paste,
    Rename,
    Delete,
    Save,
    SaveAs,
    Batch,
    Duplicates,
    More,
    Edit,
    Select,
    Arrow,
    Stroke,
    Rect,
    Highlight,
    Text,
    Cover,
    Crop,
    Undo,
    Redo,
    Close,
    Down,
    ZoomIn,
    ZoomOut,
    Swap,
    ChevronUp,
    SortAsc,
    SortDesc,
    Image,
    Play,
    Pause,
    Volume,
    Mute,
    Settings,
    Manual,
    Quit,
    Server,
    Address,
    Slideshow,
    FlipH,
    FlipV,
    Updates,
    Pipette,
    Marquee,
    HeadFilled,
    HeadOpen,
    HeadDouble,
    HeadDot,
    Solid,
    Dashed,
    Dotted,
}

macro_rules! lucide {
    ($name:literal) => {
        include_str!(concat!("../assets/icons/lucide/", $name, ".svg"))
    };
}

const LINE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">"#;

fn line_svg(icon: Line) -> std::borrow::Cow<'static, str> {
    let own = |path: &str| format!("{LINE}<path d=\"{path}\"/></svg>").into();
    match icon {
        Line::Up => lucide!("arrow-up").into(),
        Line::Refresh => lucide!("refresh-cw").into(),
        Line::Info => lucide!("info").into(),
        Line::Grid => lucide!("layout-grid").into(),
        // With a square: the image turns; the bare arrows looked like Undo and Redo (user, 2026-11-05).
        Line::RotateCcw => lucide!("rotate-ccw-square").into(),
        Line::RotateCw => lucide!("rotate-cw-square").into(),
        Line::NewFolder => lucide!("folder-plus").into(),
        Line::Copy => lucide!("copy").into(),
        Line::Cut => lucide!("scissors").into(),
        Line::Paste => lucide!("clipboard-paste").into(),
        Line::Rename => lucide!("text-cursor-input").into(),
        Line::Delete => lucide!("trash").into(),
        Line::Save => lucide!("save").into(),
        Line::SaveAs => lucide!("file-output").into(),
        Line::Batch => lucide!("scaling").into(),
        Line::Duplicates => lucide!("scan-search").into(),
        Line::More => lucide!("ellipsis").into(),
        Line::Edit => lucide!("square-pen").into(),
        Line::Select => lucide!("mouse-pointer-2").into(),
        Line::Arrow => lucide!("move-up-right").into(),
        Line::Stroke => lucide!("slash").into(),
        Line::Rect => lucide!("square").into(),
        Line::Highlight => lucide!("highlighter").into(),
        Line::Text => lucide!("type").into(),
        Line::Cover => lucide!("eye-off").into(),
        Line::Crop => lucide!("crop").into(),
        Line::Undo => lucide!("undo-2").into(),
        Line::Redo => lucide!("redo-2").into(),
        Line::Close => lucide!("x").into(),
        Line::Down => lucide!("chevron-down").into(),
        Line::ZoomIn => lucide!("zoom-in").into(),
        Line::ZoomOut => lucide!("zoom-out").into(),
        Line::Swap => lucide!("arrow-left-right").into(),
        Line::ChevronUp => lucide!("chevron-up").into(),
        Line::SortAsc => lucide!("arrow-up-narrow-wide").into(),
        Line::SortDesc => lucide!("arrow-down-wide-narrow").into(),
        Line::Image => lucide!("image").into(),
        Line::Play => lucide!("play").into(),
        Line::Pause => lucide!("pause").into(),
        Line::Volume => lucide!("volume-2").into(),
        Line::Mute => lucide!("volume-x").into(),
        Line::Settings => lucide!("settings").into(),
        Line::Manual => lucide!("book-open").into(),
        Line::Quit => lucide!("log-out").into(),
        Line::Server => lucide!("server").into(),
        Line::Address => lucide!("link").into(),
        Line::Slideshow => lucide!("presentation").into(),
        Line::FlipH => lucide!("square-split-horizontal").into(),
        Line::FlipV => lucide!("square-split-vertical").into(),
        Line::Updates => lucide!("download").into(),
        Line::Pipette => lucide!("pipette").into(),
        Line::Marquee => lucide!("square-dashed").into(),
        Line::HeadFilled => format!(r#"{LINE}<path d="M3 12h11"/><path d="M13 7l8 5-8 5z" fill="currentColor"/></svg>"#).into(),
        Line::HeadOpen => own("M3 12h17M15 7l5 5-5 5"),
        Line::HeadDouble => format!(r#"{LINE}<path d="M9 12h6"/><path d="M10 7l-7 5 7 5zM14 7l7 5-7 5z" fill="currentColor"/></svg>"#).into(),
        Line::HeadDot => format!(r#"{LINE}<path d="M3 12h13"/><circle cx="18" cy="12" r="3.5" fill="currentColor"/></svg>"#).into(),
        Line::Solid => own("M3 12h18"),
        Line::Dashed => own("M3 12h4M10 12h4M17 12h4"),
        // Dots wider than the line, or they look like specks.
        Line::Dotted => format!("{}<path d=\"M4.5 12h.01M9.5 12h.01M14.5 12h.01M19.5 12h.01\"/></svg>", LINE.replace("stroke-width=\"2\"", "stroke-width=\"3.5\"")).into(),
    }
}

type LineKey = (Line, u32, [u8; 4]);
static LINES: Mutex<Option<HashMap<LineKey, TextureHandle>>> = Mutex::new(None);

/// A line icon `side` points wide in `color` (Lucide's `currentColor`).
pub fn line_in(ctx: &egui::Context, icon: Line, side: f32, color: egui::Color32) -> egui::Image<'static> {
    let px = ((side * ctx.pixels_per_point() / 4.0).ceil() as u32 * 4).clamp(8, 256);
    let key = (icon, px, color.to_array());
    let tex = LINES.lock().unwrap().get_or_insert_with(HashMap::new).entry(key).or_insert_with(|| {
        let hex = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
        ctx.load_texture(format!("line-{icon:?}-{px}-{hex}"), render(&line_svg(icon).replace("currentColor", &hex), px), TextureOptions::LINEAR)
    }).clone();
    egui::Image::new(egui::load::SizedTexture::new(tex.id(), vec2(side, side)))
}

/// A line icon in the colour of the text.
pub fn line(ui: &egui::Ui, icon: Line, side: f32) -> egui::Image<'static> {
    line_in(ui.ctx(), icon, side, ui.visuals().text_color())
}

/// Size of the square icon buttons of the bars.
pub const BUTTON: egui::Vec2 = vec2(32.0, 28.0);

/// A square button of a bar with a line icon.
pub fn button(ui: &egui::Ui, icon: Line) -> egui::Button<'static> {
    egui::Button::image(line(ui, icon, 18.0)).min_size(BUTTON)
}

/// The same, shown on or off (the tool chosen, a box shown); framed also when off, as the others.
pub fn toggle(ui: &egui::Ui, on: bool, icon: Line) -> egui::Button<'static> {
    egui::Button::image(line(ui, icon, 18.0)).selected(on).min_size(BUTTON)
}

/// The icon of a menu item, or room as wide as one (the labels stay in line).
pub fn menu_icon(ui: &egui::Ui, icon: Option<Line>) -> egui::Image<'static> {
    match icon {
        Some(icon) => line(ui, icon, 16.0),
        // ponytail: a transparent icon as the blank, one texture.
        None => line_in(ui.ctx(), Line::Close, 16.0, egui::Color32::TRANSPARENT),
    }
}

/// A menu item: icon, name, shortcut on the right.
pub fn menu_item(ui: &egui::Ui, icon: Option<Line>, label: String, keys: String) -> egui::Button<'static> {
    egui::Button::image_and_text(menu_icon(ui, icon), label).shortcut_text(keys)
}

/// A small button with a line icon, inside a row of text (close a box, fold a panel).
pub fn small(ui: &egui::Ui, icon: Line) -> egui::Button<'static> {
    egui::Button::image(line(ui, icon, 13.0)).small()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line icon is a valid SVG with something drawn, in the colour asked.
    #[test]
    fn line_icons_draw() {
        use Line::*;
        for icon in [Up, Refresh, Info, Grid, RotateCcw, RotateCw, NewFolder, Copy, Cut, Paste, Rename, Delete, Save, SaveAs, Batch, Duplicates, More, Edit, Select, Arrow, Stroke, Rect, Highlight, Text, Cover, Crop, Undo, Redo, Close, Down, ZoomIn, ZoomOut, Swap, ChevronUp, SortAsc, SortDesc, Image, Play, Pause, Volume, Mute, Settings, Manual, Quit, Server, Address, Slideshow, FlipH, FlipV, Updates, Pipette, Marquee, HeadFilled, HeadOpen, HeadDouble, HeadDot, Solid, Dashed, Dotted] {
            let img = render(&line_svg(icon).replace("currentColor", "#ff0000"), 24);
            let red = img.pixels.iter().filter(|p| p.a() > 128 && p.r() > 200 && p.g() < 60).count();
            assert!(red > 10, "{icon:?}: {red} pixels");
        }
    }
}
