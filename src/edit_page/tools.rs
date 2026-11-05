//! The editor's bars: the tools at the top (drawing tools, Crop and the quarter turns, Undo and
//! Redo; on the right Copy, Close and Save copy with its menu), the properties under them (the
//! style of the object being drawn or selected, or Rotate and crop's controls) and the status bar
//! at the bottom (name, size, the tool's hint, zoom). Laid out as Snagit, ShareX and Satty (user,
//! 2026-11-05).

use eframe::egui::{self, Color32, Key};

use super::{Ask, Tool, Window};
use crate::App;
use crate::actions::Act;
use crate::edit::doc::{Dash, Head, Shape, Style};
use crate::edit::geom::Kind;
use crate::i18n::I18n;
use crate::icons::{self, Line};
use crate::transform::Op;

/// The tools: icon, name, key (alone, no Ctrl).
pub(super) const TOOLS: [(Tool, Line, &str, Key); 8] = [
    (Tool::Select, Line::Select, "edit-tool-select", Key::V),
    (Tool::Draw(Kind::Arrow), Line::Arrow, "edit-tool-arrow", Key::A),
    (Tool::Draw(Kind::Line), Line::Stroke, "edit-tool-line", Key::L),
    (Tool::Draw(Kind::Rect), Line::Rect, "edit-tool-rect", Key::R),
    (Tool::Draw(Kind::Highlight), Line::Highlight, "edit-tool-highlight", Key::H),
    (Tool::Draw(Kind::Text), Line::Text, "edit-tool-text", Key::T),
    (Tool::Draw(Kind::Cover), Line::Cover, "edit-tool-cover", Key::B),
    (Tool::Crop, Line::Crop, "edit-tool-crop", Key::C),
];

/// Ready-made colours: red, orange, yellow, green, blue, purple, black, white.
const COLOURS: [([u8; 3], &str); 8] = [
    ([255, 0, 0], "edit-col-red"),
    ([255, 140, 0], "edit-col-orange"),
    ([255, 230, 0], "edit-col-yellow"),
    ([46, 160, 67], "edit-col-green"),
    ([30, 110, 230], "edit-col-blue"),
    ([142, 36, 170], "edit-col-purple"),
    ([0, 0, 0], "edit-col-black"),
    ([255, 255, 255], "edit-col-white"),
];

/// "Name (key)".
fn tip(i18n: &I18n, id: &str, key: &str) -> String {
    format!("{} ({key})", i18n.t(id))
}

/// A button of a bar: on hover its name and key, when off also why.
fn bar_button(ui: &mut egui::Ui, button: egui::Button, tip: String, off: Option<String>) -> bool {
    let r = ui.add_enabled(off.is_none(), button).on_hover_text(&tip);
    match off {
        Some(why) => r.on_disabled_hover_text(format!("{tip}\n{why}")).clicked(),
        None => r.clicked(),
    }
}

impl App {
    /// The bar at the top; what its buttons ask for.
    pub(super) fn edit_toolbar(&mut self, ui: &mut egui::Ui, w: &mut Window) -> Option<Ask> {
        let i18n = &self.i18n;
        let (mut ask, mut copy) = (None, false);
        ui.horizontal(|ui| {
            for (n, &(tool, icon, id, key)) in TOOLS.iter().enumerate() {
                if n == TOOLS.len() - 1 {
                    ui.separator();
                }
                if bar_button(ui, icons::toggle(ui, w.tool == tool, icon), tip(i18n, id, key.name()), None) {
                    w.choose(tool);
                }
            }
            for (op, icon, id, key) in [(Op::Ccw, Line::RotateCcw, "tf-rotate-ccw", "key-rotate-ccw"), (Op::Cw, Line::RotateCw, "tf-rotate-cw", "key-rotate-cw")] {
                if bar_button(ui, icons::button(ui, icon), tip(i18n, id, &i18n.t(key)), None) {
                    w.turn(op);
                }
            }
            ui.separator();
            let (can_undo, can_redo) = w.history.as_ref().map_or((false, false), |h| (h.can_undo() || w.doc != *h.doc(), h.can_redo()));
            if bar_button(ui, icons::button(ui, Line::Undo), tip(i18n, "edit-undo", "Ctrl+Z"), (!can_undo).then(|| i18n.t("edit-off-undo"))) {
                w.undo();
            }
            if bar_button(ui, icons::button(ui, Line::Redo), tip(i18n, "edit-redo", "Ctrl+Y"), (!can_redo).then(|| i18n.t("edit-off-redo"))) {
                w.redo();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ask = self.save_buttons(ui, w);
                if bar_button(ui, icons::button(ui, Line::Close), tip(i18n, "rc-close", "Esc"), None) {
                    ask = Some(Ask::Close);
                }
                if w.copying {
                    ui.spinner();
                }
                copy = bar_button(ui, icons::button(ui, Line::Copy), tip(i18n, "edit-copy", "Ctrl+C"), w.copying.then(|| i18n.t("edit-off-copying")));
            });
        });
        if copy {
            self.act = Some(Act::EditCopy);
        }
        ask
    }

    /// Save copy, and its arrow with Save as… and Overwrite the original…: drawn right to left.
    fn save_buttons(&self, ui: &mut egui::Ui, w: &Window) -> Option<Ask> {
        let i18n = &self.i18n;
        let mut ask = None;
        let off = match () {
            _ if w.preparing => Some(i18n.t("edit-off-preparing")),
            _ if !w.changed() => Some(i18n.t("edit-off-unchanged")),
            _ => None,
        };
        let solid = crate::theme::solid(ui.visuals());
        ui.spacing_mut().item_spacing.x = 1.0;
        let arrow = egui::Button::image(icons::line_in(ui.ctx(), Line::Down, 16.0, Color32::WHITE)).fill(solid).min_size(egui::vec2(24.0, icons::BUTTON.y));
        let (r, _) = egui::containers::menu::MenuButton::from_button(arrow).ui(ui, |ui| {
            let save_as = egui::Button::image_and_text(icons::line(ui, Line::SaveAs, 16.0), i18n.t("menu-save-copy")).shortcut_text(i18n.t("key-save-copy"));
            let r = ui.add_enabled(!w.preparing, save_as).on_hover_text(i18n.t("rc-save-as-hint"));
            if r.clicked() {
                ask = Some(Ask::SaveAs);
            }
            let why = off.clone().or_else(|| self.why_not_overwrite(w));
            let r = ui.add_enabled(why.is_none(), egui::Button::image_and_text(icons::line(ui, Line::Save, 16.0), i18n.t("edit-overwrite"))).on_hover_text(i18n.t("edit-overwrite-hint"));
            if let Some(why) = why {
                // Written for the message windows (`**bold**`, escaped names): drawn the same way.
                r.on_disabled_hover_ui(|ui| {
                    ui.set_max_width(320.0);
                    crate::save_dialogs::notice_text(ui, &why);
                });
            } else if r.clicked() {
                ask = Some(Ask::Overwrite);
            }
        });
        r.on_hover_text(i18n.t("edit-save-more"));
        let main = crate::theme::main_button(ui, i18n.t("edit-save-copy")).min_size(egui::vec2(0.0, icons::BUTTON.y));
        let hint = format!("{} (Ctrl+S)\n{}", i18n.t("edit-save-copy"), i18n.t("rc-save-hint"));
        if bar_button(ui, main, hint, off) {
            ask = Some(Ask::Save);
        }
        ui.spacing_mut().item_spacing.x = 8.0;
        ask
    }

    /// The bar under the tools: Rotate and crop's controls, or the style of the object.
    pub(super) fn props_bar(&self, ui: &mut egui::Ui, w: &mut Window) {
        let i18n = &self.i18n;
        ui.horizontal(|ui| {
            ui.set_min_height(icons::BUTTON.y);
            if w.tool == Tool::Crop {
                if super::crop::controls(ui, w, i18n) {
                    w.refit();
                }
                w.doc.fill = (!w.transparent).then(|| [w.color.r(), w.color.g(), w.color.b()]);
                ui.separator();
                let apply = crate::theme::main_button(ui, i18n.t("rc-apply"));
                if bar_button(ui, apply, format!("{} ({})\n{}", i18n.t("rc-apply"), i18n.t("key-enter"), i18n.t("rc-apply-hint")), None) {
                    w.apply_crop();
                }
            } else if style_bar(ui, w, i18n) && !w.screen_picking {
                // The desktop's picker, in the background: its answer comes as `Msg::EditPicked`.
                w.screen_picking = true;
                let (tx, ctx) = (self.tx.clone(), ui.ctx().clone());
                self.rt.spawn(async move {
                    let r = crate::eyedropper::pick().await;
                    log!("eyedropper: {r:?}");
                    let _ = tx.send(crate::loader::Msg::EditPicked(r));
                    ctx.request_repaint();
                });
            }
        });
    }

    /// At the bottom: name and size of the result, the remote line, notes; the tool's hint; zoom.
    pub(super) fn edit_status(&self, ui: &mut egui::Ui, w: &mut Window) {
        let i18n = &self.i18n;
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        ui.horizontal(|ui| {
            ui.label(crate::source::name(&w.path));
            let (pw, ph) = w.doc.crop.pixels();
            ui.label(format!("{pw} × {ph}"));
            if let Some(id) = w.remote {
                ui.label(crate::theme::alert(i18n.t(id)));
            }
            if w.preparing {
                ui.add(egui::Spinner::new().color(Color32::WHITE));
                ui.label(i18n.t("status-preparing"));
            }
            if let Some((note, until)) = &w.note {
                if ui.input(|i| i.time) < *until {
                    ui.label(crate::theme::alert(note.clone()));
                } else {
                    w.note = None;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ctx = ui.ctx().clone();
                let white = |icon| egui::Button::image(icons::line_in(&ctx, icon, 16.0, Color32::WHITE)).frame(false);
                if ui.add(white(Line::ZoomIn)).on_hover_text(tip(i18n, "edit-zoom-in", "+")).clicked() {
                    w.zoom(1.25);
                }
                let pct = format!("{:.0}%", w.shown.1 * ui.ctx().pixels_per_point() * 100.0);
                if ui.add(egui::Button::new(pct).frame(false)).on_hover_text(tip(i18n, "edit-zoom-fit", "0")).clicked() {
                    w.view = None;
                }
                if ui.add(white(Line::ZoomOut)).on_hover_text(tip(i18n, "edit-zoom-out", "−")).clicked() {
                    w.zoom(0.8);
                }
                ui.separator();
                let hint = if w.picking { "edit-tip-pick" } else { hint_id(w.tool) };
                ui.add(egui::Label::new(i18n.t(hint)).truncate());
            });
        });
    }
}

/// The short hint of the status bar for the tool.
fn hint_id(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "edit-tip-select",
        Tool::Crop => "edit-tip-crop",
        Tool::Draw(Kind::Text) => "edit-tip-text",
        Tool::Draw(Kind::Cover) => "edit-tip-cover",
        Tool::Draw(_) => "edit-tip-draw",
    }
}

/// The style of the selected object, or of the next one the tool draws; the last one used for
/// each kind is kept. `true` when the screen's eyedropper is asked for.
fn style_bar(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let selected = w.selected.filter(|&i| i < w.doc.objects.len());
    let kind = match (selected, w.tool) {
        (Some(i), _) => Kind::of(&w.doc.objects[i]),
        (None, Tool::Draw(kind)) => kind,
        _ => {
            ui.weak(i18n.t("edit-select-hint"));
            return false;
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
    if let Some(c) = w.picked.take() {
        style.color = c;
    }
    colours(ui, &mut style.color, i18n);
    let screen = eyedroppers(ui, w, i18n);
    style_controls(ui, &mut style, kind, w.long(), i18n);
    if kind == Kind::Text {
        ui.separator();
        if let Some(i) = selected {
            font_choice(ui, w, i, i18n);
        }
        text_field(ui, w, selected, i18n);
    }
    if let Some(i) = selected {
        let o = &mut w.doc.objects[i];
        o.style = style;
        if let Shape::Text { size, .. } = &mut o.shape {
            *size = style.width;
        }
        ui.separator();
        if bar_button(ui, icons::button(ui, Line::Delete), format!("{} ({})", i18n.t("edit-delete"), i18n.t("key-delete")), None) {
            w.doc.objects.remove(i);
            w.selected = None;
        }
    }
    w.remember(kind, style);
    screen
}

/// The eyedropper on the image (I), and on the whole screen where the system allows it; `true`
/// when the latter is clicked.
fn eyedroppers(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    if bar_button(ui, icons::toggle(ui, w.picking, Line::Pipette), tip(i18n, "edit-pick", "I"), None) {
        w.picking = !w.picking;
    }
    let off = (!crate::eyedropper::on_screen()).then(|| i18n.t("edit-pick-screen-off")).or_else(|| w.screen_picking.then(|| i18n.t("edit-pick-screen-busy")));
    bar_button(ui, icons::button(ui, Line::Monitor), i18n.t("edit-pick-screen"), off)
}

/// Colours, width (size of a text), opacity, dashes, as each kind has them.
fn style_controls(ui: &mut egui::Ui, style: &mut Style, kind: Kind, long: f32, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let max = (long / 10.0).max(50.0);
    let slider = |ui: &mut egui::Ui, label: String, value: &mut f32, range, suffix| {
        ui.separator();
        ui.label(label);
        ui.add(egui::Slider::new(value, range).integer().suffix(suffix));
    };
    ui.spacing_mut().slider_width = 110.0;
    match kind {
        Kind::Line | Kind::Arrow | Kind::Rect => slider(ui, t("edit-width"), &mut style.width, 1.0..=max, " px"),
        Kind::Text => slider(ui, t("edit-size"), &mut style.width, 4.0..=(long / 4.0).max(100.0), " px"),
        Kind::Highlight | Kind::Cover => {}
    }
    if kind != Kind::Cover {
        let mut pct = style.opacity * 100.0;
        slider(ui, t("edit-opacity"), &mut pct, 5.0..=100.0, " %");
        style.opacity = (pct / 100.0).clamp(0.05, 1.0);
    }
    if kind == Kind::Arrow {
        ui.separator();
        for (head, icon, id) in [(Head::Filled, Line::HeadFilled, "edit-head-filled"), (Head::Open, Line::HeadOpen, "edit-head-open"), (Head::Double, Line::HeadDouble, "edit-head-double"), (Head::Dot, Line::HeadDot, "edit-head-dot")] {
            if ui.add(icons::toggle(ui, style.head == head, icon)).on_hover_text(t(id)).clicked() {
                style.head = head;
            }
        }
    }
    if matches!(kind, Kind::Line | Kind::Arrow | Kind::Rect) {
        ui.separator();
        for (dash, icon, id) in [(Dash::Solid, Line::Solid, "edit-solid"), (Dash::Dashed, Line::Dashed, "edit-dashed"), (Dash::Dotted, Line::Dotted, "edit-dotted")] {
            if ui.add(icons::toggle(ui, style.dash == dash, icon)).on_hover_text(t(id)).clicked() {
                style.dash = dash;
            }
        }
    }
}

/// The ready-made colours as swatches, the one in use ringed, then any other colour.
fn colours(ui: &mut egui::Ui, color: &mut [u8; 3], i18n: &I18n) {
    ui.spacing_mut().item_spacing.x = 4.0;
    let ring = ui.visuals().selection.stroke.color;
    for (c, id) in COLOURS {
        let (rect, r) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
        let fill = Color32::from_rgb(c[0], c[1], c[2]);
        let p = ui.painter();
        p.rect_filled(rect.shrink(2.0), 3.0, fill);
        p.rect_stroke(rect.shrink(2.0), 3.0, egui::Stroke::new(1.0, Color32::from_gray(120)), egui::StrokeKind::Inside);
        if *color == c {
            p.rect_stroke(rect, 4.0, egui::Stroke::new(2.0, ring), egui::StrokeKind::Inside);
        }
        if r.on_hover_text(i18n.t(id)).clicked() {
            *color = c;
        }
    }
    egui::color_picker::color_edit_button_srgb(ui, color).on_hover_text(i18n.t("edit-col-other"));
    ui.spacing_mut().item_spacing.x = 8.0;
}

/// The font of the selected text: pointing at one in the list shows the text in it, choosing one
/// keeps it, closing the list without choosing puts the text's own back.
fn font_choice(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let Shape::Text { font, .. } = &w.doc.objects[i].shape else { return };
    let own = w.font_preview.as_ref().filter(|(j, _)| *j == i).map_or_else(|| font.clone(), |(_, f)| f.clone());
    let default = i18n.t("edit-font-default");
    let (mut pointed, mut chosen) = (None, None);
    let families = crate::decode::font_families();
    let r = egui::ComboBox::from_id_salt("edit-font").width(180.0).height(420.0).selected_text(own.clone().unwrap_or_else(|| default.clone())).show_ui(ui, |ui| {
        for f in std::iter::once(None).chain(families.iter().cloned().map(Some)) {
            let r = ui.selectable_label(own == f, f.clone().unwrap_or_else(|| default.clone()));
            if r.hovered() {
                pointed = Some(f.clone());
            }
            if r.clicked() {
                chosen = Some(f);
            }
        }
    });
    r.response.on_hover_text(i18n.t("edit-font-hint"));
    let set = |w: &mut Window, f: Option<String>| {
        if let Some(Shape::Text { font, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) {
            *font = f;
        }
    };
    match (chosen, r.inner.is_some()) {
        (Some(f), _) => {
            w.font_preview = None;
            w.text_font.clone_from(&f);
            set(w, f);
        }
        (None, true) => {
            w.font_preview.get_or_insert((i, own));
            if let Some(f) = pointed {
                set(w, f);
            }
        }
        (None, false) => {
            if let Some((j, f)) = w.font_preview.take()
                && j == i
            {
                set(w, f);
            }
        }
    }
}

/// The letters of the selected text, typed here; a text just placed takes the keys at once.
fn text_field(ui: &mut egui::Ui, w: &mut Window, selected: Option<usize>, i18n: &I18n) {
    let Some(Shape::Text { text, .. }) = selected.map(|i| &mut w.doc.objects[i].shape) else {
        w.focus_text = false;
        ui.weak(i18n.t("edit-text-hint"));
        return;
    };
    let r = ui.add(egui::TextEdit::multiline(text).desired_rows(1).desired_width(280.0).hint_text(i18n.t("edit-text-hint")));
    if std::mem::take(&mut w.focus_text) {
        r.request_focus();
    }
}
