//! The editor's bars: the tools at the top (drawing tools, Crop and the quarter turns, Undo and
//! Redo; on the right Copy, Close and Save copy with its menu), the properties under them (the
//! style of the object being drawn or selected, or Rotate and crop's controls) and the status bar
//! at the bottom (name, size, the tool's hint, zoom). Laid out as Snagit, ShareX and Satty (user,
//! 2026-11-05).

use eframe::egui::{self, Color32, Key};

use super::{Ask, Tool, Window};
use crate::App;
use crate::actions::Act;
use crate::edit::doc::{Dash, Head, Pt, Shape, Style};
use crate::edit::geom::Kind;
use crate::i18n::I18n;
use crate::icons::{self, Line};
use crate::transform::Op;

/// The tools: icon, name, key (alone, no Ctrl).
pub(super) const TOOLS: [(Tool, Line, &str, Key); 16] = [
    (Tool::Select, Line::Select, "edit-tool-select", Key::V),
    (Tool::Marquee, Line::Marquee, "edit-tool-marquee", Key::M),
    (Tool::Draw(Kind::Arrow), Line::Arrow, "edit-tool-arrow", Key::A),
    (Tool::Draw(Kind::Line), Line::Stroke, "edit-tool-line", Key::L),
    (Tool::Draw(Kind::Pen), Line::Pen, "edit-tool-pen", Key::P),
    (Tool::Draw(Kind::Rect), Line::Rect, "edit-tool-rect", Key::R),
    (Tool::Draw(Kind::Ellipse), Line::Ellipse, "edit-tool-ellipse", Key::E),
    (Tool::Draw(Kind::Highlight), Line::Highlight, "edit-tool-highlight", Key::H),
    (Tool::Draw(Kind::Spotlight), Line::Spotlight, "edit-tool-spotlight", Key::S),
    (Tool::Draw(Kind::Text), Line::Text, "edit-tool-text", Key::T),
    (Tool::Draw(Kind::Bubble), Line::Bubble, "edit-tool-bubble", Key::U),
    (Tool::Draw(Kind::Step), Line::Step, "edit-tool-step", Key::N),
    (Tool::Draw(Kind::Stamp), Line::Stamp, "edit-tool-stamp", Key::K),
    (Tool::Draw(Kind::Lens), Line::Lens, "edit-tool-lens", Key::Z),
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
    /// The bar at the top; what its buttons ask for. When the window is too narrow for one row
    /// the tools go to a new line and Copy, Close and Save copy get a row of their own, on the
    /// right (they were cut off: user, 2026-11-06).
    pub(super) fn edit_toolbar(&mut self, ui: &mut egui::Ui, w: &mut Window) -> Option<Ask> {
        let (mut ask, mut copy) = (None, false);
        let step = icons::BUTTON.x + ui.spacing().item_spacing.x;
        // The tools, the turns, Filters, Undo and Redo, three separators; Copy, Close, Save copy.
        let need = (TOOLS.len() + 5) as f32 * step + 3.0 * 12.0 + 2.0 * step + 200.0;
        if ui.available_width() >= need {
            ui.horizontal(|ui| {
                tools_row(ui, w, &self.i18n);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| (ask, copy) = self.right_row(ui, w));
            });
        } else {
            ui.horizontal_wrapped(|ui| tools_row(ui, w, &self.i18n));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| (ask, copy) = self.right_row(ui, w));
        }
        if copy {
            self.act = Some(Act::EditCopy);
        }
        ask
    }

    /// Save copy (with its arrow), Close, Copy: drawn right to left; what they ask, and Copy.
    fn right_row(&self, ui: &mut egui::Ui, w: &mut Window) -> (Option<Ask>, bool) {
        let i18n = &self.i18n;
        let mut ask = self.save_buttons(ui, w);
        if bar_button(ui, icons::button(ui, Line::Close), tip(i18n, "rc-close", "Esc"), None) {
            ask = Some(Ask::Close);
        }
        if w.copying {
            ui.spinner();
        }
        let copy = bar_button(ui, icons::button(ui, Line::Copy), tip(i18n, "edit-copy", "Ctrl+C"), w.copying.then(|| i18n.t("edit-off-copying")));
        (ask, copy)
    }

    /// Save copy, and its arrow with Save as… and Overwrite the original…: drawn right to left.
    fn save_buttons(&self, ui: &mut egui::Ui, w: &Window) -> Option<Ask> {
        let i18n = &self.i18n;
        let mut ask = None;
        // While the last save is still being sent (a phone, a server): the next one waits for it.
        let sending = self.edit_sending(w);
        let off = match () {
            _ if w.preparing => Some(i18n.t("edit-off-preparing")),
            _ if sending => Some(i18n.t("edit-off-sending")),
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
        // A new image has no file yet: its main button is Save as….
        // After a first copy the button updates it (user, 2026-11-06).
        let (label, hint, main_ask, off) = match () {
            _ if w.new => ("menu-save-copy", "rc-save-as-hint", Ask::SaveAs, w.preparing.then(|| i18n.t("edit-off-preparing"))),
            _ if w.copy.is_some() => ("edit-update-copy", "edit-update-hint", Ask::Save, off),
            _ => ("edit-save-copy", "rc-save-hint", Ask::Save, off),
        };
        let main = crate::theme::main_button(ui, i18n.t(label)).min_size(egui::vec2(0.0, icons::BUTTON.y));
        if bar_button(ui, main, format!("{} (Ctrl+S)\n{}", i18n.t(label), i18n.t(hint)), off) {
            ask = Some(main_ask);
        }
        ui.spacing_mut().item_spacing.x = 8.0;
        ask
    }

    /// The bar under the tools: Rotate and crop's controls, or the style of the object.
    pub(super) fn props_bar(&mut self, ui: &mut egui::Ui, w: &mut Window) {
        let i18n = &self.i18n;
        // Wrapped: with the border and the fill of a rectangle it may not fit in one row.
        ui.horizontal_wrapped(|ui| {
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
            } else if w.tool == Tool::Marquee {
                if let Some(act) = marquee_bar(ui, w, i18n) {
                    self.act = Some(act);
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
    /// The Keys button, as the viewer's (`keys_window`).
    pub(super) fn edit_status(&self, ui: &mut egui::Ui, w: &mut Window) -> egui::Response {
        let i18n = &self.i18n;
        ui.visuals_mut().override_text_color = Some(Color32::WHITE);
        // A colour just taken: its codes, and that its hex code is on the clipboard.
        if let Some(c) = w.just_picked.take() {
            w.note = Some((i18n.t_args("edit-picked", &[("codes", &crate::eyedropper::codes(c).replace('\n', " · "))]), ui.input(|i| i.time) + 8.0));
        }
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
                // Blue on white, as the viewer's: the bar makes every text white.
                let keys = ui.add(egui::Button::new(egui::RichText::new(i18n.t("keys-button")).color(crate::theme::solid(ui.visuals()))).fill(Color32::WHITE));
                ui.separator();
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
                keys
            })
            .inner
        })
        .inner
    }
}

/// The tools, the quarter turns, Filters, Undo and Redo.
fn tools_row(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
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
    filters_menu(ui, w, i18n);
    ui.separator();
    let (can_undo, can_redo) = w.history.as_ref().map_or((false, false), |h| (h.can_undo() || w.doc != *h.doc(), h.can_redo()));
    if bar_button(ui, icons::button(ui, Line::Undo), tip(i18n, "edit-undo", "Ctrl+Z"), (!can_undo).then(|| i18n.t("edit-off-undo"))) {
        w.undo();
    }
    if bar_button(ui, icons::button(ui, Line::Redo), tip(i18n, "edit-redo", "Ctrl+Y"), (!can_redo).then(|| i18n.t("edit-off-redo"))) {
        w.redo();
    }
}

/// The Filters menu: on the part chosen with Selection, or on the whole image.
fn filters_menu(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    let (r, _) = egui::containers::menu::MenuButton::from_button(icons::button(ui, Line::Filters)).ui(ui, |ui| {
        // Readable at once: where the filter goes (user, 2026-11-06: the faint line was not read).
        ui.set_max_width(300.0);
        ui.label(egui::RichText::new(i18n.t(if w.marquee.is_some() { "edit-filters-part" } else { "edit-filters-whole" })).strong());
        ui.separator();
        for f in crate::edit::filter::Filter::ALL {
            let r = ui.add(icons::menu_item(ui, Some(Line::Filter(f)), i18n.t(f.label()), String::new()));
            // Blur and Pixelate look like hiding, and are not enough for private text.
            let r = if f.hides() { r.on_hover_text(i18n.t("edit-filter-not-cover")) } else { r };
            if r.clicked() {
                w.add_filter(f);
            }
        }
    });
    r.on_hover_text(i18n.t("edit-filters"));
}

/// A filter selected: its name, its amount, delete.
fn filter_bar(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let long = w.long();
    let Some(Shape::Filter { filter, amount, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) else { return };
    ui.strong(i18n.t(filter.label()));
    if let Some((lo, hi, _)) = filter.amount(long) {
        ui.label(i18n.t("edit-filter-amount"));
        ui.spacing_mut().slider_width = 160.0;
        ui.add(egui::Slider::new(amount, lo..=hi).integer());
    }
    if filter.hides() {
        ui.label(crate::theme::alert(i18n.t("edit-filter-not-cover")));
    }
    ui.separator();
    if bar_button(ui, icons::button(ui, Line::Delete), format!("{} ({})", i18n.t("edit-delete"), i18n.t("key-delete")), None) {
        w.doc.objects.remove(i);
        w.selected = None;
        return;
    }
    // Done: as Esc, the filter let go of as it is; it was not clear it is already applied (user,
    // 2026-11-06, as Paint.NET's and Photoshop's OK).
    if bar_button(ui, crate::theme::main_button(ui, i18n.t("edit-filter-done")), format!("{} (Esc)\n{}", i18n.t("edit-filter-done"), i18n.t("edit-filter-done-hint")), None) {
        w.escape();
    }
}

/// Selection's bar: copy, cut, paste, with their keys and why they are off.
fn marquee_bar(ui: &mut egui::Ui, w: &Window, i18n: &I18n) -> Option<Act> {
    let none = w.marquee.is_none().then(|| i18n.t("edit-off-no-marquee"));
    let mut act = None;
    if bar_button(ui, icons::button(ui, Line::Copy), tip(i18n, "edit-copy-part", "Ctrl+C"), none.clone()) {
        act = Some(Act::EditCopy);
    }
    if bar_button(ui, icons::button(ui, Line::Cut), tip(i18n, "edit-cut-part", "Ctrl+X"), none) {
        act = Some(Act::EditCut);
    }
    if bar_button(ui, icons::button(ui, Line::Paste), tip(i18n, "edit-paste-part", "Ctrl+V"), w.clip.is_none().then(|| i18n.t("edit-off-no-clip"))) {
        act = Some(Act::EditPaste(None));
    }
    act
}

/// The short hint of the status bar for the tool.
fn hint_id(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "edit-tip-select",
        Tool::Marquee => "edit-tip-marquee",
        Tool::Crop => "edit-tip-crop",
        Tool::Draw(Kind::Text) => "edit-tip-text",
        Tool::Draw(Kind::Cover) => "edit-tip-cover",
        Tool::Draw(Kind::Step) => "edit-tip-step",
        Tool::Draw(Kind::Stamp) => "edit-tip-stamp",
        Tool::Draw(Kind::Bubble) => "edit-tip-bubble",
        Tool::Draw(Kind::Lens) => "edit-tip-lens",
        Tool::Draw(Kind::Spotlight) => "edit-tip-spotlight",
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
    if let (Kind::Filter, Some(i)) = (kind, selected) {
        filter_bar(ui, w, i, i18n);
        return false;
    }
    let mut style = match selected {
        Some(i) => w.doc.objects[i].style,
        None => w.style(kind),
    };
    // A text's size lives in the text, a step's in its box; the style keeps it for the next one.
    match selected.map(|i| &w.doc.objects[i].shape) {
        Some(Shape::Text { size, .. }) => style.width = *size,
        Some(Shape::Step { area, .. } | Shape::Stamp(area)) => style.width = area.w,
        _ => {}
    }
    if let Some(c) = w.picked.take() {
        style.color = c;
    }
    // What the controls change: the object selected, or the next one drawn (user, 2026-11-05:
    // "what is the opacity for, if nothing is selected?").
    let whose = if selected.is_some() { ("edit-props-selected", "edit-props-selected-hint") } else { ("edit-props-next", "edit-props-next-hint") };
    ui.weak(i18n.t(whose.0)).on_hover_text(i18n.t(whose.1));
    let boxed = matches!(kind, Kind::Rect | Kind::Ellipse | Kind::Bubble);
    if boxed {
        ui.label(i18n.t("edit-border"));
    }
    // A piece of image has its own colours, a spotlight none.
    let screen = !matches!(kind, Kind::Patch | Kind::Spotlight) && {
        colours(ui, &mut style.color, i18n);
        let screen = eyedroppers(ui, w, i18n);
        if w.picking {
            pick_sizes(ui, w, i18n);
        }
        screen
    };
    if boxed {
        ui.separator();
        ui.label(i18n.t("edit-fill"));
        fill_colours(ui, &mut style.fill, i18n);
    }
    style_controls(ui, &mut style, kind, w.long(), i18n);
    kind_controls(ui, w, &mut style, (kind, selected), i18n);
    if let Some(i) = selected {
        let o = &mut w.doc.objects[i];
        o.style = style;
        match &mut o.shape {
            Shape::Text { size, .. } => *size = style.width,
            Shape::Stamp(area) => (area.w, area.h) = (style.width, style.width),
            Shape::Step { area, n } => {
                (area.w, area.h) = (style.width, style.width);
                ui.separator();
                ui.label(i18n.t("edit-step-number"));
                ui.add(egui::DragValue::new(n).range(0..=999));
            }
            _ => {}
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

/// What only some kinds have: the shape and letters of a bubble, a lens's zoom, a spotlight's
/// shape, the font and the letters of a text or a bubble.
fn kind_controls(ui: &mut egui::Ui, w: &mut Window, style: &mut Style, (kind, selected): (Kind, Option<usize>), i18n: &I18n) {
    match kind {
        Kind::Bubble => super::props::bubble(ui, style, w.long(), i18n),
        Kind::Lens => super::props::lens(ui, style, i18n),
        Kind::Stamp => super::props::stamp(ui, style, i18n),
        Kind::Text => super::props::text_look(ui, style, i18n),
        Kind::Spotlight => {
            ui.separator();
            super::props::round_or_square(ui, style, i18n);
        }
        _ => {}
    }
    if matches!(kind, Kind::Text | Kind::Bubble) {
        ui.separator();
        if let Some(i) = selected {
            font_choice(ui, w, i, i18n);
            if kind == Kind::Text {
                text_curve(ui, w, i, i18n);
                text_angle(ui, w, i, i18n);
            }
        }
        text_field(ui, w, selected, i18n);
    }
}

/// One eyedropper (user, 2026-11-05: "only the one that takes any colour"), its button or I:
/// anywhere on the screen where the system allows it, `true` then; elsewhere a click on the image.
/// I picks on the image (what is drawn on it too, with a live box of the colour); the button,
/// and Shift+I, the desktop's picker on the whole screen where there is one (Linux: user,
/// 2026-11-06, the picker shows no codes).
fn eyedroppers(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
    let screen = crate::eyedropper::on_screen();
    let (id, keys) = if screen { ("edit-pick", "I, Shift+I") } else { ("edit-pick-image", "I") };
    let off = w.screen_picking.then(|| i18n.t("edit-pick-screen-busy"));
    let clicked = bar_button(ui, icons::toggle(ui, w.picking || w.screen_picking, Line::Pipette), tip(i18n, id, keys), off);
    let whole = match (clicked, w.ask_pick.take()) {
        (true, _) => true,
        (false, Some(whole)) => whole,
        (false, None) => return false,
    };
    if whole && screen {
        return true;
    }
    w.picking = !w.picking;
    false
}

/// While picking: one pixel, or the average of 3 × 3 or 5 × 5 (as in the photo editors).
fn pick_sizes(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    for (size, label) in [(1, "1 px"), (3, "3×3"), (5, "5×5")] {
        if ui.add(egui::Button::new(label).selected(w.pick_size == size)).on_hover_text(i18n.t("edit-pick-size")).clicked() {
            w.pick_size = size;
        }
    }
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
        Kind::Line | Kind::Arrow | Kind::Pen | Kind::Rect | Kind::Ellipse | Kind::Bubble => slider(ui, t("edit-width"), &mut style.width, 1.0..=max, " px"),
        // A lens may have no border.
        Kind::Lens => slider(ui, t("edit-width"), &mut style.width, 0.0..=max, " px"),
        Kind::Text | Kind::Step | Kind::Stamp => slider(ui, t("edit-size"), &mut style.width, 4.0..=(long / 4.0).max(100.0), " px"),
        Kind::Highlight | Kind::Cover | Kind::Patch | Kind::Filter | Kind::Spotlight => {}
    }
    if kind != Kind::Cover {
        let mut pct = style.opacity * 100.0;
        // A spotlight's opacity is how dark the rest gets.
        slider(ui, t(if kind == Kind::Spotlight { "edit-intensity" } else { "edit-opacity" }), &mut pct, 5.0..=100.0, " %");
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
    if matches!(kind, Kind::Line | Kind::Arrow | Kind::Pen | Kind::Rect | Kind::Ellipse) {
        ui.separator();
        for (dash, icon, id) in [(Dash::Solid, Line::Solid, "edit-solid"), (Dash::Dashed, Line::Dashed, "edit-dashed"), (Dash::Dotted, Line::Dotted, "edit-dotted")] {
            if ui.add(icons::toggle(ui, style.dash == dash, icon)).on_hover_text(t(id)).clicked() {
                style.dash = dash;
            }
        }
    }
}

/// The ready-made colours as swatches, the one in use ringed, then any other colour; `true` when
/// one is chosen.
pub(super) fn colours(ui: &mut egui::Ui, color: &mut [u8; 3], i18n: &I18n) -> bool {
    let mut chosen = false;
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
            chosen = true;
        }
    }
    chosen |= egui::color_picker::color_edit_button_srgb(ui, color).on_hover_text(i18n.t("edit-col-other")).changed();
    ui.spacing_mut().item_spacing.x = 8.0;
    chosen
}

/// The font of the selected text: pointing at one in the list shows the text in it, choosing one
/// keeps it, closing the list without choosing puts the text's own back.
fn font_choice(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let Some(font) = w.doc.objects[i].shape.font_mut() else { return };
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
        if let Some(font) = w.doc.objects.get_mut(i).and_then(|o| o.shape.font_mut()) {
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

/// On a curve or not: on, the text gets a line as long as it is, along its angle, to bend by its
/// middle handle; off, it is upright again where the middle of the line was.
fn text_curve(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let Some(Shape::Text { at, text, size, rot, path, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) else { return };
    if !ui.add(egui::Button::new(i18n.t("edit-text-curve")).selected(path.is_some())).on_hover_text(i18n.t("edit-text-curve-hint")).clicked() {
        return;
    }
    *path = match path.take() {
        Some((a, b, c)) => {
            *at = crate::edit::geom::bend_handle(a, b, c);
            None
        }
        None => {
            let half = crate::edit::geom::text_area(*at, text, *size, *rot).w / 2.0 + *size;
            let (s, c) = rot.to_radians().sin_cos();
            Some((Pt::new(at.x - c * half, at.y - s * half), Pt::new(at.x + c * half, at.y + s * half), None))
        }
    };
}

/// The angle of the selected text on the screen, -180° to 180° (its own angle in the document is
/// measured on the image, which may be turned by Rotate and crop).
fn text_angle(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let angle = w.doc.angle;
    let Some(Shape::Text { rot, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) else { return };
    let mut shown = (*rot + angle + 180.0).rem_euclid(360.0) - 180.0;
    ui.label(i18n.t("rc-rotation"));
    ui.spacing_mut().slider_width = 110.0;
    if ui.add(egui::Slider::new(&mut shown, -180.0..=180.0).integer().suffix("°")).changed() {
        *rot = shown - angle;
    }
}

/// `colours` with none ringed (a colour no swatch has).
fn colours_unringed(ui: &mut egui::Ui, colour: &mut [u8; 3], i18n: &I18n) -> bool {
    // ponytail: almost white, no swatch has it, and the button of any other colour looks empty.
    let mut probe = [254, 254, 254];
    let chosen = colours(ui, &mut probe, i18n);
    if chosen {
        *colour = probe;
    }
    chosen
}

/// The inside of a rectangle: none (a white square crossed in red), the ready-made colours, any
/// other colour.
fn fill_colours(ui: &mut egui::Ui, fill: &mut Option<[u8; 3]>, i18n: &I18n) {
    let (rect, r) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let p = ui.painter();
    let inner = rect.shrink(2.0);
    p.rect_filled(inner, 3.0, Color32::WHITE);
    p.rect_stroke(inner, 3.0, egui::Stroke::new(1.0, Color32::from_gray(120)), egui::StrokeKind::Inside);
    p.line_segment([inner.left_bottom(), inner.right_top()], egui::Stroke::new(2.0, Color32::from_rgb(220, 30, 30)));
    if fill.is_none() {
        p.rect_stroke(rect, 4.0, egui::Stroke::new(2.0, ui.visuals().selection.stroke.color), egui::StrokeKind::Inside);
    }
    if r.on_hover_text(i18n.t("edit-fill-none")).clicked() {
        *fill = None;
    }
    // With no fill no swatch is ringed: the colours only show what a click would give.
    let mut colour = fill.unwrap_or([255, 255, 255]);
    let none = fill.is_none();
    let chosen = if none { colours_unringed(ui, &mut colour, i18n) } else { colours(ui, &mut colour, i18n) };
    if chosen || !none {
        *fill = Some(colour);
    }
}

/// The letters of the selected text, typed here; a text just placed takes the keys at once.
fn text_field(ui: &mut egui::Ui, w: &mut Window, selected: Option<usize>, i18n: &I18n) {
    let Some(text) = selected.and_then(|i| w.doc.objects[i].shape.text_mut()) else {
        w.focus_text = false;
        ui.weak(i18n.t("edit-text-hint"));
        return;
    };
    let r = ui.add(egui::TextEdit::multiline(text).desired_rows(1).desired_width(280.0).hint_text(i18n.t("edit-text-hint")));
    // A text typed on the image takes the keys there (`typing`).
    if w.editing.is_none() && std::mem::take(&mut w.focus_text) {
        r.request_focus();
    }
}

/// The Fluent id of the keys of `tool`, for the Keys button: only those of the tool in use (user,
/// 2026-11-06: "only the commands of the tool selected, not all").
pub(super) fn keys_id(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "hint-edit-select",
        Tool::Marquee => "hint-edit-marquee",
        Tool::Crop => "hint-edit-crop",
        Tool::Draw(kind) => match kind {
            Kind::Arrow => "hint-edit-arrow",
            Kind::Line => "hint-edit-line",
            Kind::Pen => "hint-edit-pen",
            Kind::Rect => "hint-edit-rect",
            Kind::Ellipse => "hint-edit-ellipse",
            Kind::Highlight => "hint-edit-highlight",
            Kind::Spotlight => "hint-edit-spotlight",
            Kind::Text => "hint-edit-text",
            Kind::Bubble => "hint-edit-bubble",
            Kind::Step => "hint-edit-step",
            Kind::Stamp => "hint-edit-stamp",
            Kind::Lens => "hint-edit-lens",
            Kind::Cover => "hint-edit-cover",
            Kind::Patch | Kind::Filter => "hint-edit-select",
        },
    }
}

#[cfg(test)]
mod tests {
    /// The warning that Blur and Pixelate are no cover is in both languages.
    #[test]
    fn blur_warning() {
        for ftl in [include_str!("../../locales/en.ftl"), include_str!("../../locales/it.ftl")] {
            assert!(ftl.lines().any(|l| l.starts_with("edit-filter-not-cover = ")));
        }
        use crate::edit::filter::Filter;
        assert!(Filter::Blur.hides() && Filter::Pixelate.hides() && !Filter::Gray.hides());
    }

    /// Every tool has its list of keys, in both languages.
    #[test]
    fn keys_listed() {
        for ftl in [include_str!("../../locales/en.ftl"), include_str!("../../locales/it.ftl")] {
            for (tool, ..) in super::TOOLS {
                let id = super::keys_id(tool);
                assert!(ftl.lines().any(|l| l.starts_with(&format!("{id} = "))), "{id}");
            }
        }
    }
}
