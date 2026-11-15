//! File > New image…: an empty image of a size chosen (paper, screen or one's own, in pixels or
//! in a unit of length at a resolution) and a background (a colour or transparent), opened in the
//! editor; its Save is Save as…, the file does not exist yet. The sizes without the interface,
//! then the window.

use eframe::egui;
use image::{DynamicImage, Rgba, RgbaImage};

use crate::App;
use crate::i18n::I18n;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    Px,
    Mm,
    Cm,
    In,
}

impl Unit {
    const ALL: [Unit; 4] = [Unit::Px, Unit::Mm, Unit::Cm, Unit::In];

    fn label(self) -> &'static str {
        match self {
            Unit::Px => "px",
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::In => "in",
        }
    }

    /// Inches in one of it (none for pixels).
    fn inches(self) -> Option<f32> {
        match self {
            Unit::Px => None,
            Unit::Mm => Some(1.0 / 25.4),
            Unit::Cm => Some(1.0 / 2.54),
            Unit::In => Some(1.0),
        }
    }
}

/// The ready-made sizes: name (a Fluent id, or the name itself), width, height, unit.
const PRESETS: [(&str, f32, f32, Unit); 10] = [
    ("A3", 297.0, 420.0, Unit::Mm),
    ("A4", 210.0, 297.0, Unit::Mm),
    ("A5", 148.0, 210.0, Unit::Mm),
    ("new-letter", 8.5, 11.0, Unit::In),
    ("new-legal", 8.5, 14.0, Unit::In),
    ("Full HD", 1920.0, 1080.0, Unit::Px),
    ("4K UHD", 3840.0, 2160.0, Unit::Px),
    ("HD", 1280.0, 720.0, Unit::Px),
    ("new-square", 1080.0, 1080.0, Unit::Px),
    ("new-story", 1080.0, 1920.0, Unit::Px),
];

/// No side longer, as Selection and the decoders allow.
pub(crate) const MAX_SIDE: u32 = 20_000;

/// The size in pixels of `w` × `h` in `unit`, at `dpi` dots per inch for the units of length; at
/// least a pixel, at most `MAX_SIDE`.
pub fn pixels(w: f32, h: f32, unit: Unit, dpi: f32) -> (u32, u32) {
    let k = unit.inches().map_or(1.0, |i| i * dpi);
    let side = |v: f32| ((v * k).round().max(1.0) as u32).min(MAX_SIDE);
    (side(w), side(h))
}

/// The empty image: the colour, or transparent.
pub fn blank((w, h): (u32, u32), background: Option<[u8; 3]>) -> DynamicImage {
    let fill = background.map_or(Rgba([0, 0, 0, 0]), |[r, g, b]| Rgba([r, g, b, 255]));
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, fill))
}

/// The window's choices.
pub struct Form {
    preset: Option<usize>,
    w: f32,
    h: f32,
    unit: Unit,
    dpi: f32,
    /// `None`: transparent.
    background: Option<[u8; 3]>,
    colour: [u8; 3],
}

impl Form {
    pub fn new() -> Form {
        let (_, w, h, unit) = PRESETS[1];
        Form { preset: Some(1), w, h, unit, dpi: 300.0, background: Some([255, 255, 255]), colour: [255, 255, 255] }
    }

    fn name(&self, i18n: &I18n, i: usize) -> String {
        let (id, w, h, unit) = PRESETS[i];
        let name = if id.starts_with("new-") { i18n.t(id) } else { id.to_string() };
        format!("{name} ({w} × {h} {})", unit.label())
    }
}

/// New image… is on with a folder open, in the grid: as New folder, it makes a file in the folder
/// open, and in the viewer the File menu works on the image shown.
fn available(folder_open: bool, viewing: bool) -> bool {
    folder_open && !viewing
}

enum Answer {
    Create,
    Cancel,
}

impl App {
    /// File > New image… and Ctrl+N: on or off together.
    pub(crate) fn can_new_image(&self) -> bool {
        available(self.loader.is_some(), self.viewing.is_some())
    }

    /// The window of File > New image…; Create opens the image in the editor.
    pub(crate) fn new_image_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.new_image.take() else { return };
        let mut answer = None;
        egui::Window::new(self.i18n.t("new-title")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            answer = new_image_form(ui, &mut f, &self.i18n);
        });
        match answer {
            Some(Answer::Create) => {
                let size = pixels(f.w, f.h, f.unit, f.dpi);
                log!("new image {size:?}, background {:?}", f.background);
                self.open_new_image(ctx, blank(size, f.background), &self.i18n.t("new-name"));
            }
            Some(Answer::Cancel) => {}
            None => self.new_image = Some(f),
        }
    }
}

/// Size (ready-made or one's own, its unit and resolution), background, Cancel and Create.
fn new_image_form(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) -> Option<Answer> {
    let t = |id| i18n.t(id);
    egui::Grid::new("new-image").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(t("new-size"));
        let shown = f.preset.map_or_else(|| t("new-custom"), |i| f.name(i18n, i));
        egui::ComboBox::from_id_salt("new-preset").width(260.0).selected_text(shown).show_ui(ui, |ui| {
            for (i, &(_, w, h, unit)) in PRESETS.iter().enumerate() {
                if ui.selectable_label(f.preset == Some(i), f.name(i18n, i)).clicked() {
                    (f.preset, f.w, f.h, f.unit) = (Some(i), w, h, unit);
                }
            }
            if ui.selectable_label(f.preset.is_none(), t("new-custom")).clicked() {
                f.preset = None;
            }
        });
        ui.end_row();
        ui.label(t("new-width-height"));
        ui.horizontal(|ui| {
            // One's own size once a number or the unit changes.
            let unit = f.unit;
            let speed = if f.unit == Unit::Px { 1.0 } else { 0.1 };
            let mut edited = ui.add(egui::DragValue::new(&mut f.w).range(0.1..=100_000.0).speed(speed)).changed();
            ui.label("×");
            edited |= ui.add(egui::DragValue::new(&mut f.h).range(0.1..=100_000.0).speed(speed)).changed();
            egui::ComboBox::from_id_salt("new-unit").width(60.0).selected_text(f.unit.label()).show_ui(ui, |ui| {
                for u in Unit::ALL {
                    ui.selectable_value(&mut f.unit, u, u.label());
                }
            });
            if edited || f.unit != unit {
                f.preset = None;
            }
            if ui.add(crate::icons::button(ui, crate::icons::Line::Swap)).on_hover_text(t("new-swap")).clicked() {
                (f.w, f.h) = (f.h, f.w);
            }
        });
        ui.end_row();
        ui.label(t("new-dpi"));
        ui.add_enabled(f.unit != Unit::Px, egui::DragValue::new(&mut f.dpi).range(10.0..=2400.0).suffix(" dpi")).on_disabled_hover_text(t("new-dpi-off"));
        ui.end_row();
        ui.label(t("new-background"));
        ui.horizontal(|ui| {
            if ui.radio(f.background == Some([255, 255, 255]), t("new-white")).clicked() {
                f.background = Some([255, 255, 255]);
            }
            if ui.radio(f.background.is_none(), t("new-transparent")).clicked() {
                f.background = None;
            }
            let colour = f.background.is_some_and(|c| c != [255, 255, 255]);
            if ui.radio(colour, t("new-colour")).clicked() {
                f.background = Some(f.colour);
            }
            if egui::color_picker::color_edit_button_srgb(ui, &mut f.colour).changed() {
                f.background = Some(f.colour);
            }
        });
        ui.end_row();
    });
    let (w, h) = pixels(f.w, f.h, f.unit, f.dpi);
    ui.add_space(6.0);
    ui.weak(i18n.t_args("new-pixels", &[("w", &w.to_string()), ("h", &h.to_string())]));
    ui.add_space(6.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        if ui.add(crate::theme::main_button(ui, t("new-create"))).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            answer = Some(Answer::Create);
        }
        if ui.button(t("delete-no")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            answer = Some(Answer::Cancel);
        }
    });
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A4 at 300 dpi is 2480 × 3508, as every program makes it; pixels stay pixels; never more
    /// than the largest side.
    #[test]
    fn sizes() {
        assert_eq!(pixels(210.0, 297.0, Unit::Mm, 300.0), (2480, 3508));
        assert_eq!(pixels(21.0, 29.7, Unit::Cm, 300.0), (2480, 3508));
        assert_eq!(pixels(8.5, 11.0, Unit::In, 300.0), (2550, 3300));
        assert_eq!(pixels(1920.0, 1080.0, Unit::Px, 72.0), (1920, 1080));
        assert_eq!(pixels(1000.0, 1000.0, Unit::In, 2400.0), (MAX_SIDE, MAX_SIDE));
        let b = blank((4, 3), None);
        assert!(b.to_rgba8().pixels().all(|p| p[3] == 0));
        assert_eq!(blank((4, 3), Some([1, 2, 3])).to_rgba8().get_pixel(3, 2).0, [1, 2, 3, 255]);
    }

    /// In the grid of a folder open; not in the viewer, not before a folder is open.
    #[test]
    fn available_in_the_grid_only() {
        assert!(available(true, false));
        assert!(!available(true, true));
        assert!(!available(false, false));
    }
}
