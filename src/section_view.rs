//! The section of a 3D model in the viewer (#145): S cuts the model with a plane across X, Y or
//! Z (`render3d::Cut`, drawn by the graphics card and by `render3d` for Save as… and without a
//! card), moved with Alt and the wheel or the keys , and .; F keeps the other side. The face of
//! the cut is filled where it crosses the inside of the model, so walls and thicknesses show; the
//! status bar says where the plane is.

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

    /// In the status bar while a section is on: its axis, where the plane is, the side kept, why
    /// the cut is not filled, a slider that moves the plane (taken by `section_slider`), the keys.
    pub(crate) fn section_bar(&self, ui: &mut egui::Ui) {
        let Some(cut) = self.viewed_mesh().and(self.model.view.cut) else { return };
        let t = |id: &str| self.i18n.t(id);
        let axis = ["X", "Y", "Z"][cut.axis.min(2)];
        let side = if cut.flip { format!(" · {}", t("section-flip")) } else { String::new() };
        ui.separator();
        ui.label(format!("{}: {axis} {:.0}%{side}", t("section-title"), cut.at * 100.0));
        let mut at = cut.at;
        ui.spacing_mut().slider_width = 120.0;
        // Where the plane is, in tenths of the model: the label before says the percentage.
        let slider = egui::Slider::new(&mut at, 0.0..=1.0).show_value(false);
        if crate::ticked::add(ui, slider, &crate::ticked::Marks::new(&(0.0..=1.0), None)).on_hover_text(t("section-keys")).changed() {
            ui.ctx().data_mut(|d| d.insert_temp(slider_id(), at));
        }
        // A model that is not closed has no known inside: the cut is left empty, and said so.
        let open = self.viewed_mesh().and_then(|(p, _)| self.model.measure.sizes.get(&p).cloned().flatten()).is_some_and(|s| s.open_edges > 0);
        if open {
            ui.label(crate::theme::alert(t("section-open")));
        }
        ui.add(egui::Label::new(t("section-keys")).truncate());
    }

    /// The plane where the status bar's slider put it.
    pub(crate) fn section_slider(&mut self, ctx: &egui::Context) {
        if let Some(at) = ctx.data_mut(|d| d.remove_temp::<f32>(slider_id()))
            && let Some(cut) = &mut self.model.view.cut
        {
            cut.at = at.clamp(0.0, 1.0);
        }
    }
}

fn slider_id() -> egui::Id {
    egui::Id::unique("section-slider")
}
