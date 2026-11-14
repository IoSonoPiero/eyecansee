//! The editor's bars: the tools at the top (drawing tools, Crop, the quarter turns, Resize, Undo and
//! Redo; on the right Copy, Close and Save copy with its menu), the properties under them (the
//! style of the object being drawn or selected, or Rotate and crop's controls) and the status bar
//! at the bottom (name, size, the tool's hint, zoom). Laid out as Snagit, ShareX and Satty (user,
//! 2026-11-05).

use eframe::egui::{self, Color32, Key};

use super::bar_fit::{self, ARROW_ICON, ARROW_MIN, BarParts, SAVE_GAP, SEPARATOR};
use super::{Ask, Tool, Window};
use crate::App;
use crate::actions::Act;
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

impl Window {
    /// What F1 needs to choose the editor's page of the guide.
    pub(crate) fn help_view(&self) -> crate::help_spot::EditView {
        let selected = self.selected.and_then(|i| self.doc.objects.get(i)).map(Kind::of);
        crate::help_spot::EditView { tool: self.tool, selected, saving: self.ready.is_some() || self.leaving, picking: self.picking || self.screen_picking }
    }
}

/// "Name (key)".
pub(super) fn tip(i18n: &I18n, id: &str, key: &str) -> String {
    format!("{} ({key})", i18n.t(id))
}

/// The label of the main save button on the bar: Overwrite the original… is shortened to
/// Overwrite…, so the bar keeps one row in a 1200 px window; its tip has the whole name.
pub(super) fn bar_label(id: &'static str) -> &'static str {
    if id == "edit-overwrite" { "edit-overwrite-short" } else { id }
}

/// A button of a bar: on hover its name and key, when off also why.
pub(super) fn bar_button(ui: &mut egui::Ui, button: egui::Button, tip: String, off: Option<String>) -> bool {
    let r = ui.add_enabled(off.is_none(), button).on_hover_text(&tip);
    match off {
        Some(why) => r.on_disabled_hover_text(format!("{tip}\n{why}")).clicked(),
        None => r.clicked(),
    }
}

impl App {
    /// The bar at the top; what its buttons ask for. When the window is too narrow for one row
    /// the tools wrap, and Copy, Close and Save copy go at the right end of their last row, or of
    /// a row of their own when there is no room there (`bar_fit`).
    pub(super) fn edit_toolbar(&mut self, ui: &mut egui::Ui, w: &mut Window) -> Option<Ask> {
        let (mut ask, mut copy) = (None, false);
        let parts = BarParts::measure(ui, self.i18n.t(bar_label(self.main_save_text(w).0)), w.copying);
        ui.horizontal_wrapped(|ui| {
            tools_row(ui, w, &self.i18n);
            bar_fit::right_end(ui, parts, |ui| (ask, copy) = self.right_row(ui, w));
        });
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
    /// With Overwrite chosen as the main save in the Options (issue #96) the two swap places.
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
        let over_main = self.main_overwrites(w);
        // A new image has no file yet: its main button is Save as….
        // After a first copy the button updates it.
        let copy = copy_text(w);
        let solid = crate::theme::solid(ui.visuals());
        ui.spacing_mut().item_spacing.x = SAVE_GAP;
        let arrow = egui::Button::image(icons::line_in(ui.ctx(), Line::Down, ARROW_ICON, Color32::WHITE)).fill(solid).min_size(egui::vec2(ARROW_MIN, icons::BUTTON.y));
        let (r, _) = egui::containers::menu::MenuButton::from_button(arrow).ui(ui, |ui| {
            if over_main {
                let r = ui.add_enabled(off.is_none(), egui::Button::image_and_text(icons::line(ui, Line::Save, 16.0), i18n.t(copy.0))).on_hover_text(i18n.t(copy.1));
                if r.clicked() {
                    ask = Some(Ask::Save);
                }
            }
            let save_as = egui::Button::image_and_text(icons::line(ui, Line::SaveAs, 16.0), i18n.t("menu-save-copy")).shortcut_text(i18n.t("key-save-copy"));
            let r = ui.add_enabled(!w.preparing, save_as).on_hover_text(i18n.t("rc-save-as-hint"));
            if r.clicked() {
                ask = Some(Ask::SaveAs);
            }
            if over_main {
                return;
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
        let (label, hint) = self.main_save_text(w);
        let (main_ask, off) = match () {
            _ if w.new => (Ask::SaveAs, w.preparing.then(|| i18n.t("edit-off-preparing"))),
            _ if over_main => (Ask::Overwrite, off.or_else(|| self.why_not_overwrite(w))),
            _ => (Ask::Save, off),
        };
        // The tip has the whole name: on the bar Overwrite the original… is only Overwrite….
        let mut tip = format!("{} (Ctrl+S)\n{}", i18n.t(label), i18n.t(hint));
        // Overwrite chosen as the main save but not possible here: why it is Save copy.
        if self.settings.edit_main_overwrite && !w.new && let Some(why) = self.why_never_overwrite(w) {
            tip = format!("{tip}\n\n{}", crate::help_search::plain(&why));
        }
        let main = crate::theme::main_button(ui, i18n.t(bar_label(label))).min_size(egui::vec2(0.0, icons::BUTTON.y));
        if bar_button(ui, main, tip, off) {
            ask = Some(main_ask);
        }
        ui.spacing_mut().item_spacing.x = 8.0;
        ask
    }

    /// The label and hint of the main save button: Save as… for a new image, Overwrite when
    /// chosen as the main save, Save copy or Update copy.
    fn main_save_text(&self, w: &Window) -> (&'static str, &'static str) {
        match () {
            _ if w.new => ("menu-save-copy", "rc-save-as-hint"),
            _ if self.main_overwrites(w) => ("edit-overwrite", "edit-overwrite-hint"),
            _ => copy_text(w),
        }
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
            } else if super::style_bar::style_bar(ui, w, i18n) && !w.screen_picking {
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
        // A copy too large for other programs (X11), once "Copied" is shown.
        if !w.copying
            && let Some(n) = w.too_big.as_ref().and_then(|r| r.try_recv().ok())
        {
            w.too_big = None;
            let size = format!("{:.1}", n as f64 / 1_048_576.0);
            w.note = Some((i18n.t_args("edit-copy-too-large", &[("size", &size)]), ui.input(|i| i.time) + 12.0));
        }
        ui.horizontal(|ui| {
            ui.label(crate::source::name(&w.path));
            let (pw, ph) = w.doc.crop.pixels();
            ui.label(format!("{pw} × {ph}"));
            if let Some(id) = w.remote {
                ui.label(crate::theme::alert(i18n.t(id)));
            }
            if let Some(address) = &self.held_open {
                ui.label(crate::theme::alert(i18n.t_args("edit-open-later", &[("name", crate::source::name(&crate::address::without_password(address)))])));
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

/// Save copy's label and hint; Update copy after a first copy.
fn copy_text(w: &Window) -> (&'static str, &'static str) {
    if w.copy.done.is_some() { ("edit-update-copy", "edit-update-hint") } else { ("edit-save-copy", "rc-save-hint") }
}

/// The tools, the quarter turns, Resize, Filters, Undo and Redo.
pub(super) fn tools_row(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    for (n, &(tool, icon, id, key)) in TOOLS.iter().enumerate() {
        if n == TOOLS.len() - 1 {
            ui.add(egui::Separator::default().spacing(SEPARATOR));
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
    if bar_button(ui, icons::button(ui, Line::Batch), format!("{}\n{}", tip(i18n, "edit-resize", &i18n.t("key-edit-resize")), i18n.t("edit-resize-hint")), None) {
        w.ask_resize();
    }
    filters_menu(ui, w, i18n);
    ui.add(egui::Separator::default().spacing(SEPARATOR));
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
        // Readable at once: where the filter goes (the faint line was not read).
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
