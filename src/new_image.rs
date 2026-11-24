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

/// At most this many pixels (200 megapixels, 800 MB in memory, made at once): 20 000 × 20 000 was
/// 1.6 GB, and a mistake with the units brings there (4.15 a).
const MAX_PIXELS: u64 = 200_000_000;

/// More pixels than a new image may have.
fn too_many((w, h): (u32, u32)) -> bool {
    u64::from(w) * u64::from(h) > MAX_PIXELS
}

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
        // With nothing focused Esc comes as `key_answer` (`asking_keys`, #311).
        let mut answer = (crate::confirm::answer(ctx, &mut self.key_answer, ID) == Some(false)).then_some(Answer::Cancel);
        // A fixed id: the title changes with the language.
        let id = egui::Id::unique(ID);
        let window = egui::Window::new(self.i18n.t("new-title")).id(id).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO);
        crate::app_dialogs::in_ring(ctx, egui::LayerId::new(egui::Order::Middle, id), || {
            window.show(ctx, |ui| answer = new_image_form(ui, &mut f, &self.i18n).or(answer.take()));
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

/// The window's name for the keys of its buttons (`confirm::keys`).
const ID: &str = "new-image";

/// Size (ready-made or one's own, its unit and resolution), background, Cancel and Create.
fn new_image_form(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) -> Option<Answer> {
    let t = |id| i18n.t(id);
    // Enter in a field creates; on a button (Swap, the colour, Cancel) it presses that one only (#311).
    let enter_creates = !ui.ctx().egui_wants_keyboard_input() || ui.ctx().text_edit_focused();
    // The number field Enter left (`lost`): with Create greyed out it keeps the keys, as Rename's name.
    let lost = egui::Grid::new("new-image").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
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
        let sizes = ui.horizontal(|ui| {
            // One's own size once a number or the unit changes.
            let unit = f.unit;
            let speed = if f.unit == Unit::Px { 1.0 } else { 0.1 };
            let (w, _, h) = (ui.add(egui::DragValue::new(&mut f.w).range(0.1..=100_000.0).speed(speed)), ui.label("×"), ui.add(egui::DragValue::new(&mut f.h).range(0.1..=100_000.0).speed(speed)));
            let edited = w.changed() || h.changed();
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
            [w, h]
        })
        .inner;
        ui.end_row();
        ui.label(t("new-dpi"));
        let dpi = ui.add_enabled(f.unit != Unit::Px, egui::DragValue::new(&mut f.dpi).range(10.0..=2400.0).suffix(" dpi")).on_disabled_hover_text(t("new-dpi-off"));
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
        sizes.into_iter().chain([dpi]).find(egui::Response::lost_focus).map(|r| r.id)
    })
    .inner;
    let (w, h) = pixels(f.w, f.h, f.unit, f.dpi);
    let big = too_many((w, h));
    ui.add_space(6.0);
    let (ws, hs) = (w.to_string(), h.to_string());
    let too_big = i18n.t_args("new-too-big", &[("w", &ws), ("h", &hs), ("mp", &(u64::from(w) * u64::from(h) / 1_000_000).to_string()), ("max", &(MAX_PIXELS / 1_000_000).to_string())]);
    if big {
        crate::error_bar::hint(ui, &too_big);
    } else {
        ui.weak(i18n.t_args("new-pixels", &[("w", &ws), ("h", &hs)]));
    }
    if let Some(id) = lost.filter(|_| big && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| form_buttons(ui, if big { Err(too_big.as_str()) } else { Ok(()) }, enter_creates, i18n)).inner
}

/// Create (`Err`: greyed out, and why) and Cancel, as every question's buttons (#311): Create
/// highlighted, Cancel when it is greyed out. `enter_creates`: Enter came in a field.
fn form_buttons(ui: &mut egui::Ui, create: Result<(), &str>, enter_creates: bool, i18n: &I18n) -> Option<Answer> {
    let button = ui.add_enabled(create.is_ok(), crate::theme::main_button(ui, i18n.t("new-create")));
    let button = match create {
        Err(why) => button.on_disabled_hover_text(why),
        Ok(()) => button,
    };
    let cancel = ui.button(i18n.t("delete-no"));
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(Answer::Cancel);
    }
    match crate::confirm::keys(ui, ID, &[button, cancel], 0) {
        Some(0) => Some(Answer::Create),
        Some(_) => Some(Answer::Cancel),
        None if create.is_ok() && enter_creates && ui.input(|i| i.key_pressed(egui::Key::Enter)) => Some(Answer::Create),
        None => None,
    }
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

    /// At most 200 megapixels: 20 000 × 20 000 (1.6 GB, a mistake with the units brings there) is
    /// not made, Enter or no Enter; 20 000 × 10 000 and A4 at 300 dpi are (4.15 a).
    #[test]
    fn at_most_200_megapixels() {
        let enter = egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() };
        let i18n = I18n::new("en");
        let made = |w: f32, h: f32, unit: Unit| {
            // A window opened anew: Create highlighted, Cancel when it is greyed out (#311).
            let ctx = egui::Context::default();
            let mut f = Form { preset: None, w, h, unit, ..Form::new() };
            let mut answer = None;
            for events in [vec![], vec![enter.clone()]] {
                ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| answer = new_image_form(ui, &mut f, &i18n).or(answer.take())).textures_delta.clear();
            }
            matches!(answer, Some(Answer::Create))
        };
        assert!(!made(20_000.0, 20_000.0, Unit::Px));
        assert!(made(20_000.0, 10_000.0, Unit::Px));
        assert!(made(210.0, 297.0, Unit::Mm));
    }

    /// In the grid of a folder open; not in the viewer, not before a folder is open.
    #[test]
    fn available_in_the_grid_only() {
        assert!(available(true, false));
        assert!(!available(true, true));
        assert!(!available(false, false));
    }

    /// #311: New image keeps its keys, also with nothing focused (a click on its body): the grid
    /// and the shortcuts under it get none. Tab goes to Cancel and Enter presses it, no image;
    /// Esc cancels; Enter with nothing focused presses Create.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn new_image_keeps_its_keys() {
        use crate::audit_family_1::{Harness, folder, keys_stay_in_window, own_folder};
        use egui::Key;
        if own_folder("new_image::tests::new_image_keeps_its_keys") {
            return;
        }
        let mut h = Harness::new(&folder("new-image-keys", &["a.png", "b.png", "c.png"]));
        let open = |h: &mut Harness| {
            h.app.new_image = Some(Form::new());
            h.frame(Vec::new());
        };
        open(&mut h);
        keys_stay_in_window(&mut h, |a| a.new_image.is_some());
        h.blur();
        h.key(Key::Tab);
        h.key(Key::Enter);
        assert!(h.app.new_image.is_none() && h.app.editor.is_none(), "Enter on Cancel made an image, or left the window");
        // Shift+Tab from Create to the colour, the button before it: Enter there makes no image.
        open(&mut h);
        h.blur();
        h.frame(Vec::new());
        h.key_with(Key::Tab, egui::Modifiers::SHIFT);
        h.key(Key::Enter);
        h.frame(Vec::new());
        assert!(h.app.new_image.is_some() && h.app.editor.is_none(), "Enter on another button made the image");
        h.app.new_image = None;
        h.frame(Vec::new());
        open(&mut h);
        h.blur();
        h.key(Key::Escape);
        assert!(h.app.new_image.is_none() && h.app.editor.is_none(), "Esc did not cancel");
        open(&mut h);
        h.blur();
        h.key(Key::Enter);
        assert!(h.app.new_image.is_none() && h.app.editor.is_some() && h.app.viewing.is_none(), "Enter did not press Create, or reached the grid");
    }

    /// #311: Tab past Cancel, the last button, went on to the menus under the window, and Enter
    /// pressed what it reached there; Shift+Tab and the arrows too. The keyboard goes round inside.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn tab_goes_round_new_image() {
        use crate::audit_family_1::{Harness, focus_stays_in_window, folder, own_folder};
        if own_folder("new_image::tests::tab_goes_round_new_image") {
            return;
        }
        let mut h = Harness::new(&folder("new-image-round", &["a.png"]));
        h.app.new_image = Some(Form::new());
        focus_stays_in_window(&mut h);
        assert!(h.app.new_image.is_some() && h.app.editor.is_none(), "the window closed");
    }

    /// #311: a size too large greys Create out. Enter in a number field leaves the keys there, as
    /// on a name that does not go in Rename: before, it highlighted Cancel and a second Enter
    /// closed the window.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn enter_on_a_size_too_large_stays_in_the_field() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        use egui::{Key, Modifiers};
        if own_folder("new_image::tests::enter_on_a_size_too_large_stays_in_the_field") {
            return;
        }
        let mut h = Harness::new(&folder("new-image-big", &["a.png"]));
        h.app.new_image = Some(Form { preset: None, w: 100_000.0, h: 100_000.0, unit: Unit::Px, ..Form::new() });
        h.frame(Vec::new());
        h.frame(Vec::new());
        // From Cancel, highlighted, back to a number field with the keyboard.
        for _ in 0..12 {
            if h.ctx.text_edit_focused() {
                break;
            }
            h.key_with(Key::Tab, Modifiers::SHIFT);
        }
        assert!(h.ctx.text_edit_focused(), "no number field reached with Shift+Tab");
        for _ in 0..2 {
            h.key(Key::Enter);
            h.frame(Vec::new());
            assert!(h.app.new_image.is_some() && h.app.editor.is_none(), "Enter in the field closed the window or made the image");
        }
        assert!(h.ctx.text_edit_focused(), "Enter in the field took the keys away from it");
    }
}
