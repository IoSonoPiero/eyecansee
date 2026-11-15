//! The section of a 3D model in the viewer (#145): S cuts the model with a plane across X, Y or
//! Z (`render3d::Cut`, drawn by the graphics card and by `render3d` for Save as…), moved with Alt
//! and the wheel, the keys , and . or the slider of its box; F keeps the other side. The inside
//! seen through the cut takes the colour of the cut, so walls and thicknesses show.

use eframe::egui::{self, Key, Modifiers};

use crate::App;
use crate::render3d::Cut;

/// How far a key or a notch of the wheel moves the plane, across the model.
const STEP: f32 = 0.01;

impl App {
    /// S on and off (through the middle; across Z for a model with Z up: a plan of a building),
    /// X, Y, Z its axis, F the other side, , and . move it. Taken from the input.
    pub(crate) fn section_keys(&mut self, ctx: &egui::Context, z_up: bool) {
        let view = &mut self.model.view;
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::S)) {
            view.cut = if view.cut.is_some() { None } else { Some(Cut::middle(z_up)) };
        }
        let Some(cut) = &mut view.cut else { return };
        ctx.input_mut(|i| {
            for (key, axis) in [(Key::X, 0), (Key::Y, 1), (Key::Z, 2)] {
                if i.consume_key(Modifiers::NONE, key) {
                    cut.axis = axis;
                }
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                cut.flip = !cut.flip;
            }
            let steps = i.count_and_consume_key(Modifiers::NONE, Key::Period) as f32 - i.count_and_consume_key(Modifiers::NONE, Key::Comma) as f32;
            cut.at = (cut.at + steps * STEP).clamp(0.0, 1.0);
        });
    }

    /// Alt and the wheel over the model move the plane; `true` if they did (the wheel then does
    /// not zoom).
    pub(crate) fn section_wheel(&mut self, ui: &egui::Ui, hovered: bool) -> bool {
        let Some(cut) = &mut self.model.view.cut else { return false };
        if !hovered {
            return false;
        }
        let notches: f32 = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.alt => Some(crate::viewer::notches(*unit, *delta)),
                    _ => None,
                })
                .sum()
        });
        cut.at = (cut.at + notches * STEP).clamp(0.0, 1.0);
        ui.input(|i| i.modifiers.alt)
    }

    /// The box of the section, bottom left: the axis, where the plane is, the side kept.
    pub(crate) fn section_box(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        let Some(mut cut) = self.model.view.cut else { return };
        let t = |id: &str| self.i18n.t(id);
        crate::measure_view::result_box(ctx, rect, egui::Align2::LEFT_BOTTOM, &t("section-title"), |ui| {
            ui.horizontal(|ui| {
                for (axis, name) in ["X", "Y", "Z"].into_iter().enumerate() {
                    ui.selectable_value(&mut cut.axis, axis, name).on_hover_text(name);
                }
                ui.checkbox(&mut cut.flip, t("section-flip")).on_hover_text("F");
            });
            ui.add(egui::Slider::new(&mut cut.at, 0.0..=1.0).show_value(false).text(t("section-where")));
            ui.weak(t("section-keys"));
        });
        self.model.view.cut = Some(cut);
    }
}
