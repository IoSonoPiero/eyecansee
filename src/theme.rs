//! Interface colours, light and dark (chosen in the Options). egui's defaults on Windows were
//! flat: buttons, background and bars almost the same grey, light blue selection, pure red
//! errors. Contrasts checked with the WCAG formula (2026-10-21): text at least 4.5:1 on the
//! background, borders and frames at least 3:1 (tile border against the tile).

use eframe::egui::{self, Color32, CornerRadius, Stroke, Theme, ThemePreference, Visuals};

use crate::settings::ThemeChoice;

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// EyeCanSee's own colours, in addition to egui's. Style "B" (2026-10-22): white grid, blue
/// accent also as a solid fill (tree selection, status bar), ochre trash strip.
struct Palette {
    /// Borders of the active part and of the selected tile in the grid.
    accent: Color32,
    /// Solid fill with white text on it: selected folder in the tree, status bar.
    solid: Color32,
    /// Grid background, slightly different from the surrounding panels.
    canvas: Color32,
    /// Tile of each image in the grid, and its fill when selected.
    tile: Color32,
    tile_stroke: Color32,
    tile_selected: Color32,
    /// Trash strip: fill, border, text, border of its buttons.
    strip: [Color32; 4],
}

const LIGHT: Palette = Palette {
    accent: rgb(26, 98, 201),
    solid: rgb(26, 98, 201),
    canvas: rgb(255, 255, 255),
    tile: rgb(244, 246, 250),
    tile_stroke: rgb(128, 135, 145),
    tile_selected: rgb(227, 237, 251),
    strip: [rgb(255, 246, 223), rgb(232, 194, 106), rgb(92, 65, 0), rgb(184, 134, 11)],
};
const DARK: Palette = Palette {
    accent: rgb(91, 155, 255),
    solid: rgb(31, 90, 184),
    canvas: rgb(24, 25, 27),
    tile: rgb(42, 44, 48),
    tile_stroke: rgb(118, 124, 133),
    tile_selected: rgb(35, 55, 90),
    strip: [rgb(56, 45, 18), rgb(150, 115, 40), rgb(240, 214, 150), rgb(190, 150, 60)],
};

fn palette(v: &Visuals) -> &'static Palette {
    if v.dark_mode { &DARK } else { &LIGHT }
}

pub fn accent(v: &Visuals) -> Color32 {
    palette(v).accent
}

/// Solid accent fill; the text on it is white.
pub fn solid(v: &Visuals) -> Color32 {
    palette(v).solid
}

pub fn canvas(v: &Visuals) -> Color32 {
    palette(v).canvas
}

/// Background and border of an image tile.
pub fn tile(v: &Visuals) -> (Color32, Color32) {
    let p = palette(v);
    (p.tile, p.tile_stroke)
}

pub fn tile_selected(v: &Visuals) -> Color32 {
    palette(v).tile_selected
}

/// Trash strip: fill, border, text, border of its buttons.
pub fn strip(v: &Visuals) -> [Color32; 4] {
    palette(v).strip
}

pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_visuals_of(Theme::Light, light());
    ctx.set_visuals_of(Theme::Dark, dark());
    choose(ctx, choice);
}

/// Theme chosen in the Options; with `System` egui follows system changes while the app is open.
pub fn choose(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => ThemePreference::System,
        ThemeChoice::Light => ThemePreference::Light,
        ThemeChoice::Dark => ThemePreference::Dark,
    });
}

/// Buttons with the same rounding as windows and tiles.
fn rounded(v: &mut Visuals) {
    let r = CornerRadius::same(5);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
}

fn light() -> Visuals {
    let mut v = Visuals::light();
    let text = rgb(36, 41, 47);
    v.panel_fill = rgb(244, 246, 250);
    v.window_fill = rgb(255, 255, 255);
    v.extreme_bg_color = rgb(255, 255, 255);
    v.faint_bg_color = rgb(238, 240, 242);
    v.code_bg_color = rgb(236, 238, 241);
    v.window_stroke = Stroke::new(1.0, rgb(208, 212, 218));
    v.weak_text_color = Some(rgb(87, 96, 106));
    v.hyperlink_color = rgb(31, 95, 214);
    v.error_fg_color = rgb(198, 40, 40);
    v.warn_fg_color = rgb(178, 106, 0);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = v.panel_fill;
    w.noninteractive.weak_bg_fill = v.panel_fill;
    w.noninteractive.bg_stroke = Stroke::new(1.0, rgb(216, 219, 224));
    w.noninteractive.fg_stroke = Stroke::new(1.0, text);
    // White buttons with a border, as in the chosen style: on the panel they stand out.
    w.inactive.bg_fill = rgb(226, 229, 234);
    w.inactive.weak_bg_fill = rgb(255, 255, 255);
    w.inactive.bg_stroke = Stroke::new(1.0, rgb(190, 195, 204));
    w.inactive.fg_stroke = Stroke::new(1.0, text);
    w.hovered.bg_fill = rgb(214, 219, 226);
    w.hovered.weak_bg_fill = rgb(234, 240, 250);
    w.hovered.bg_stroke = Stroke::new(1.0, LIGHT.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, rgb(17, 20, 24));
    w.active.bg_fill = rgb(200, 207, 217);
    w.active.weak_bg_fill = rgb(200, 207, 217);
    w.active.bg_stroke = Stroke::new(1.0, LIGHT.accent);
    w.active.fg_stroke = Stroke::new(2.0, rgb(0, 0, 0));
    w.open.weak_bg_fill = rgb(226, 229, 234);
    w.open.bg_fill = rgb(226, 229, 234);
    w.open.fg_stroke = Stroke::new(1.0, text);

    v.selection.bg_fill = rgb(210, 226, 250);
    v.selection.stroke = Stroke::new(1.0, rgb(11, 61, 145));
    rounded(&mut v);
    v
}

fn dark() -> Visuals {
    let mut v = Visuals::dark();
    let text = rgb(215, 218, 223);
    v.panel_fill = rgb(32, 34, 37);
    v.window_fill = rgb(38, 40, 43);
    v.extreme_bg_color = rgb(20, 21, 23);
    v.faint_bg_color = rgb(38, 40, 43);
    v.code_bg_color = rgb(44, 46, 50);
    v.window_stroke = Stroke::new(1.0, rgb(58, 61, 66));
    v.weak_text_color = Some(rgb(139, 145, 153));
    v.hyperlink_color = rgb(121, 168, 255);
    v.error_fg_color = rgb(255, 107, 107);
    v.warn_fg_color = rgb(240, 180, 76);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = v.panel_fill;
    w.noninteractive.weak_bg_fill = v.panel_fill;
    w.noninteractive.bg_stroke = Stroke::new(1.0, rgb(52, 55, 60));
    w.noninteractive.fg_stroke = Stroke::new(1.0, text);
    w.inactive.bg_fill = rgb(46, 49, 53);
    w.inactive.weak_bg_fill = rgb(46, 49, 53);
    w.inactive.bg_stroke = Stroke::new(1.0, rgb(78, 83, 90));
    w.inactive.fg_stroke = Stroke::new(1.0, text);
    w.hovered.bg_fill = rgb(56, 59, 64);
    w.hovered.weak_bg_fill = rgb(56, 59, 64);
    w.hovered.bg_stroke = Stroke::new(1.0, DARK.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, rgb(255, 255, 255));
    w.active.bg_fill = rgb(67, 71, 78);
    w.active.weak_bg_fill = rgb(67, 71, 78);
    w.active.bg_stroke = Stroke::new(1.0, DARK.accent);
    w.active.fg_stroke = Stroke::new(2.0, rgb(255, 255, 255));
    w.open.weak_bg_fill = rgb(46, 49, 53);
    w.open.bg_fill = rgb(46, 49, 53);
    w.open.fg_stroke = Stroke::new(1.0, text);

    v.selection.bg_fill = rgb(43, 74, 126);
    v.selection.stroke = Stroke::new(1.0, rgb(255, 255, 255));
    rounded(&mut v);
    v
}
