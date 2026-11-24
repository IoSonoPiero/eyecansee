//! The layers panel of the CAD viewer (#142), on the right: the layers of the drawing with their
//! colour and a box each, All on and All off, and a search for drawings with many layers. It
//! changes only what is shown (`cad_layers`), in the model and the layouts; the file is not
//! changed. The choice is kept for each drawing during the session, and Save as… and Export
//! drawing use it.

use eframe::egui;

use crate::App;
use crate::cad_layers::{Layer, listed};
use crate::confirm::FollowArea;

/// What the panel shows: the drawing's path, its layers, which are used and which are shown now.
struct Rows {
    path: String,
    layers: Vec<Layer>,
    used: Vec<bool>,
    shown: Vec<bool>,
}

/// The layers shown of a drawing: the ones chosen (`saved`), if they are still as many as its
/// layers (`all`, as the file says). A drawing saved again with other layers since the choice was
/// made starts again from its own: the old choice would point past the list.
fn chosen(saved: Option<&Vec<bool>>, all: Vec<bool>) -> Vec<bool> {
    match saved {
        Some(on) if on.len() == all.len() => on.clone(),
        _ => all,
    }
}

impl App {
    /// Why the layers panel cannot open: no DWG or DXF drawing in the viewer, or nothing drawn on
    /// its layers.
    pub(crate) fn cad_layers_off(&self) -> Option<String> {
        let ok = self.cad_layer_rows().is_some_and(|r| r.used.iter().any(|&u| u));
        (!ok).then(|| self.i18n.t("cad-layers-off"))
    }

    /// The panel is open on a drawing that has layers.
    pub(crate) fn cad_layers_shown(&self) -> bool {
        self.model.cad.panel && self.cad_layers_off().is_none()
    }

    fn cad_layer_rows(&self) -> Option<Rows> {
        let path = self.files().get(self.viewing?)?.path.clone();
        let d = self.model.cad.drawings.get(&path)?;
        if d.layers.is_empty() {
            return None;
        }
        let shown = chosen(self.model.cad.layers.get(&path).map(|(_, on)| on), d.layers_shown());
        Some(Rows { used: d.used_layers(), layers: d.layers.clone(), shown, path })
    }

    /// The layers of `path` shown as `on`: the pictures are drawn again.
    fn cad_set_layers(&mut self, path: String, on: Vec<bool>) {
        let entry = self.model.cad.layers.entry(path).or_insert((0, Vec::new()));
        *entry = (entry.0 + 1, on);
    }

    /// The layers chosen in the panel for each drawing, as Export drawing applies them.
    pub(crate) fn cad_layer_choices(&self) -> std::collections::HashMap<String, Vec<bool>> {
        self.model.cad.layers.iter().map(|(p, (_, on))| (p.clone(), on.clone())).collect()
    }

    /// The panel, on the right of the viewer.
    pub(crate) fn cad_layers_panel(&mut self, ui: &mut egui::Ui) {
        let Some(rows) = self.cad_layer_rows() else { return };
        let mut on = rows.shown.clone();
        let mut close = false;
        egui::Panel::right("cad-layers").resizable(true).default_size(240.0).show(ui, |ui| {
            close = self.cad_layers_ui(ui, &rows, &mut on);
        });
        if on != rows.shown {
            self.cad_set_layers(rows.path, on);
        }
        if close {
            self.model.cad.panel = false;
        }
    }

    /// Title, search, All on and All off, the list; `true`: closed with its ✕.
    fn cad_layers_ui(&mut self, ui: &mut egui::Ui, rows: &Rows, on: &mut [bool]) -> bool {
        let t = |id| self.i18n.t(id);
        let mut close = false;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong(t("cad-layers-title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = ui.small_button("✕").on_hover_text(t("cad-layers-close")).clicked();
            });
        });
        let list = listed(&rows.layers, &rows.used, &self.model.cad.search);
        let total = rows.used.iter().filter(|&&u| u).count();
        let shown = (0..on.len()).filter(|&i| rows.used[i] && on[i]).count();
        ui.weak(self.i18n.t_args("cad-layers-count", &[("shown", &shown.to_string()), ("total", &total.to_string())]));
        ui.add(egui::TextEdit::singleline(&mut self.model.cad.search).hint_text(t("cad-layers-search")).desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            for (label, value) in [("cad-layers-all-on", true), ("cad-layers-all-off", false)] {
                if ui.button(t(label)).on_hover_text(t("cad-layers-all-hint")).clicked() {
                    list.iter().for_each(|&i| on[i] = value);
                }
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_follow(ui, |ui| {
            for &i in &list {
                let l = &rows.layers[i];
                ui.horizontal(|ui| {
                    ui.checkbox(&mut on[i], "");
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    let [r, g, b] = l.color;
                    ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
                    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
                    if ui.add(egui::Label::new(&l.name).truncate().sense(egui::Sense::click())).clicked() {
                        on[i] = !on[i];
                    }
                });
            }
            if list.is_empty() {
                ui.weak(t("cad-layers-none"));
            }
        });
        close
    }
}


#[cfg(test)]
mod tests {
    use super::chosen;

    /// A choice made for a drawing that now has other layers is not used.
    #[test]
    fn a_choice_for_other_layers_is_left() {
        let all = vec![true, false, true];
        assert_eq!(chosen(None, all.clone()), all);
        assert_eq!(chosen(Some(&vec![false, false, false]), all.clone()), [false, false, false]);
        assert_eq!(chosen(Some(&vec![false, false]), all.clone()), all);
        assert_eq!(chosen(Some(&vec![false; 5]), all.clone()), all);
    }
}
