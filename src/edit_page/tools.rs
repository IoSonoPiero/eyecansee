//! The editor's panel on the right: the tools, undo, redo and copy, the style of the object being
//! drawn or selected (or Rotate and crop's controls), then Save, Save as… and Close.

use eframe::egui;

use super::crop::{angle_controls, crop_controls, fill_controls};
use super::{Ask, Tool, Window};
use crate::actions::Act;
use crate::App;
use crate::edit::doc::{Dash, Shape, Style};
use crate::edit::geom::Kind;
use crate::i18n::I18n;

const TOOLS: [(Tool, &str); 8] = [
    (Tool::Select, "edit-tool-select"),
    (Tool::Draw(Kind::Arrow), "edit-tool-arrow"),
    (Tool::Draw(Kind::Line), "edit-tool-line"),
    (Tool::Draw(Kind::Rect), "edit-tool-rect"),
    (Tool::Draw(Kind::Highlight), "edit-tool-highlight"),
    (Tool::Draw(Kind::Text), "edit-tool-text"),
    (Tool::Draw(Kind::Cover), "edit-tool-cover"),
    (Tool::Crop, "edit-tool-crop"),
];

impl App {
    /// The whole panel; what its buttons at the bottom ask for.
    pub(super) fn side_panel(&mut self, ui: &mut egui::Ui, w: &mut Window) -> Option<Ask> {
        let i18n = &self.i18n;
        tool_buttons(ui, w, i18n);
        if history_row(ui, w, i18n) {
            self.act = Some(Act::EditCopy);
        }
        ui.separator();
        if w.tool == Tool::Crop {
            if angle_controls(ui, w, i18n) | crop_controls(ui, w, i18n) {
                w.refit();
            }
            fill_controls(ui, w, i18n);
            w.doc.fill = (!w.transparent).then(|| [w.color.r(), w.color.g(), w.color.b()]);
        } else {
            style_panel(ui, w, i18n);
        }
        ui.add_space(12.0);
        let (pw, ph) = w.doc.crop.pixels();
        ui.strong(i18n.t_args("rc-size", &[("w", &pw.to_string()), ("h", &ph.to_string())]));
        ui.separator();
        let ask = self.save_buttons(ui, w);
        if let Some((note, until)) = &w.note {
            if ui.input(|i| i.time) < *until {
                ui.label(note);
            } else {
                w.note = None;
            }
        }
        ui.add_space(12.0);
        ui.weak(i18n.t(match w.tool {
            Tool::Crop => "rc-help",
            Tool::Select => "edit-help-select",
            Tool::Draw(_) => "edit-help-draw",
        }));
        ask
    }

    /// Save, Save as…, Close. Until the objects can be saved (next step of the editor) Save and
    /// Save as take only the rotation and the crop, and are off with objects.
    fn save_buttons(&self, ui: &mut egui::Ui, w: &mut Window) -> Option<Ask> {
        let i18n = &self.i18n;
        let mut ask = None;
        let why_not = self.why_not_save(w);
        let objects = w.has_objects();
        ui.horizontal(|ui| {
            let ready = w.changed() && why_not.is_none() && !w.preparing && !objects;
            let r = ui.add_enabled(ready, egui::Button::new(i18n.t("rc-save"))).on_hover_text(i18n.t("rc-save-hint"));
            if r.clicked() {
                ask = Some(Ask::Save);
            }
            let r = if objects { r.on_disabled_hover_text(i18n.t("edit-save-later")) } else { r };
            if let Some(why) = why_not {
                // Written for the message windows (`**bold**`, escaped names): drawn the same way.
                r.on_disabled_hover_ui(|ui| {
                    ui.set_max_width(320.0);
                    crate::save_dialogs::notice_text(ui, &why);
                });
            }
            let r = ui.add_enabled(!w.preparing && !objects, egui::Button::new(i18n.t("menu-save-copy"))).on_hover_text(i18n.t("rc-save-as-hint"));
            if r.clicked() {
                ask = Some(Ask::SaveAs);
            }
            if objects {
                r.on_disabled_hover_text(i18n.t("edit-save-later"));
            }
            if ui.button(i18n.t("rc-close")).on_hover_text("Esc").clicked() {
                ask = Some(Ask::Close);
            }
        });
        // A row of its own: next to the buttons it was cut off by the edge of the panel.
        if w.preparing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(i18n.t("status-preparing"));
            });
        }
        ask
    }
}

/// The tools, as many to a row as the panel holds. Changing tool lets go of the selection (but
/// for Select).
fn tool_buttons(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    ui.horizontal_wrapped(|ui| {
        for &(tool, id) in &TOOLS {
            if ui.add(egui::Button::new(i18n.t(id)).selected(w.tool == tool)).clicked() && w.tool != tool {
                // Rotate and crop shows all of the image, the others the result.
                if (tool == Tool::Crop) != (w.tool == Tool::Crop) {
                    w.view = None;
                }
                w.tool = tool;
                if tool != Tool::Select {
                    w.selected = None;
                }
                w.hold = None;
            }
        }
    });
    ui.add_space(6.0);
}

/// Undo, Redo, Copy; `true` when Copy is asked.
fn history_row(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let (can_undo, can_redo) = w.history.as_ref().map_or((false, false), |h| (h.can_undo() || w.doc != *h.doc(), h.can_redo()));
    let mut copy = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(can_undo, egui::Button::new(i18n.t("edit-undo"))).on_hover_text("Ctrl+Z").clicked() {
            w.undo();
        }
        if ui.add_enabled(can_redo, egui::Button::new(i18n.t("edit-redo"))).on_hover_text("Ctrl+Y, Ctrl+Shift+Z").clicked() {
            w.redo();
        }
        copy = ui.add_enabled(!w.copying, egui::Button::new(i18n.t("edit-copy"))).on_hover_text("Ctrl+C").clicked();
        if w.copying {
            ui.spinner();
        }
    });
    copy
}

/// The style of the selected object, or of the next one the tool draws; the last one used for
/// each kind is kept.
fn style_panel(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    let selected = w.selected.filter(|&i| i < w.doc.objects.len());
    let kind = match (selected, w.tool) {
        (Some(i), _) => Kind::of(&w.doc.objects[i]),
        (None, Tool::Draw(kind)) => kind,
        _ => {
            ui.weak(i18n.t("edit-select-hint"));
            return;
        }
    };
    let mut style = match selected {
        Some(i) => w.doc.objects[i].style,
        None => w.style(kind),
    };
    // A text's size lives in the text; the style keeps it for the next one.
    if let Some(Shape::Text { size, .. }) = selected.map(|i| &w.doc.objects[i].shape) {
        style.width = *size;
    }
    style_controls(ui, &mut style, kind, w.long(), i18n);
    if kind == Kind::Text {
        text_field(ui, w, selected, i18n);
    }
    if kind == Kind::Cover {
        ui.weak(i18n.t("edit-cover-hint"));
    }
    if let Some(i) = selected {
        let o = &mut w.doc.objects[i];
        o.style = style;
        if let Shape::Text { size, .. } = &mut o.shape {
            *size = style.width;
        }
        ui.add_space(8.0);
        if ui.button(i18n.t("edit-delete")).on_hover_text(i18n.t("key-delete")).clicked() {
            w.doc.objects.remove(i);
            w.selected = None;
        }
    }
    w.remember(kind, style);
}

/// Colour, width (size of a text), opacity, dashes, as each kind has them.
fn style_controls(ui: &mut egui::Ui, style: &mut Style, kind: Kind, long: f32, i18n: &I18n) {
    let t = |id| i18n.t(id);
    egui::Grid::new("edit-style").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(t("edit-color"));
        egui::color_picker::color_edit_button_srgb(ui, &mut style.color);
        ui.end_row();
        let max = (long / 10.0).max(50.0);
        match kind {
            Kind::Line | Kind::Arrow | Kind::Rect => {
                ui.label(t("edit-width"));
                ui.add(egui::Slider::new(&mut style.width, 1.0..=max).integer().suffix(" px"));
                ui.end_row();
            }
            Kind::Text => {
                ui.label(t("edit-size"));
                ui.add(egui::Slider::new(&mut style.width, 4.0..=(long / 4.0).max(100.0)).integer().suffix(" px"));
                ui.end_row();
            }
            Kind::Highlight | Kind::Cover => {}
        }
        if kind != Kind::Cover {
            ui.label(t("edit-opacity"));
            let mut pct = style.opacity * 100.0;
            ui.add(egui::Slider::new(&mut pct, 5.0..=100.0).integer().suffix(" %"));
            style.opacity = (pct / 100.0).clamp(0.05, 1.0);
            ui.end_row();
        }
        if matches!(kind, Kind::Line | Kind::Arrow | Kind::Rect) {
            ui.label(t("edit-dash"));
            ui.vertical(|ui| {
                for (dash, id) in [(Dash::Solid, "edit-solid"), (Dash::Dashed, "edit-dashed"), (Dash::Dotted, "edit-dotted")] {
                    ui.radio_value(&mut style.dash, dash, t(id));
                }
            });
            ui.end_row();
        }
    });
}

/// The letters of the selected text, typed here; a text just placed takes the keys at once.
fn text_field(ui: &mut egui::Ui, w: &mut Window, selected: Option<usize>, i18n: &I18n) {
    ui.add_space(6.0);
    let Some(Shape::Text { text, .. }) = selected.map(|i| &mut w.doc.objects[i].shape) else {
        w.focus_text = false;
        ui.weak(i18n.t("edit-text-hint"));
        return;
    };
    let r = ui.add(egui::TextEdit::multiline(text).desired_rows(3).desired_width(f32::INFINITY).hint_text(i18n.t("edit-text-hint")));
    if std::mem::take(&mut w.focus_text) {
        r.request_focus();
    }
}
