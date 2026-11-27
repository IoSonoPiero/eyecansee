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
use crate::zoom::Ask as ZoomAsk;

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
        let parts = BarParts::measure(ui, self.i18n.t(bar_label(self.main_save_of(w).1)), w.copying);
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
        let copy = bar_button(ui, icons::button(ui, Line::Copy), tip(i18n, "edit-copy", &i18n.t("key-copy")), w.copying.then(|| i18n.t("edit-off-copying")));
        (ask, copy)
    }

    /// Save copy, and its arrow with Save as… and Overwrite the original…: drawn right to left.
    /// With Overwrite chosen as the main save in the Options (issue #96) the two swap places.
    fn save_buttons(&self, ui: &mut egui::Ui, w: &Window) -> Option<Ask> {
        let i18n = &self.i18n;
        let mut ask = None;
        // While the last save is still being sent (a phone, a server): the next one waits for it.
        let off = save_off(w.preparing, self.edit_sending(w), w.changed()).map(|id| i18n.t(id));
        let over_main = self.main_overwrites(w);
        let copy = copy_text(w.copy.done.is_some());
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
        let (main_ask, label, hint) = self.main_save_of(w);
        let off = match main_ask {
            Ask::SaveAs => w.preparing.then(|| i18n.t("edit-off-preparing")),
            Ask::Overwrite => off.or_else(|| self.why_not_overwrite(w)),
            _ => off,
        };
        // The tip has the whole name: on the bar Overwrite the original… is only Overwrite….
        let mut tip = format!("{} ({})\n{}", i18n.t(label), i18n.t("key-save"), i18n.t(hint));
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

    /// What the main save button asks, its label and its hint, for this page (`main_save`).
    fn main_save_of(&self, w: &Window) -> (Ask, &'static str, &'static str) {
        main_save(w.new, self.main_overwrites(w), w.copy.done.is_some())
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
        // The crop stopped at its largest size.
        if std::mem::take(&mut w.crop_capped) {
            let m = w.crop_most();
            let (mw, mh, mp) = ((m.w as u32).to_string(), (m.h as u32).to_string(), ((m.pixels / 1e6) as u32).to_string());
            w.note = Some((i18n.t_args("edit-crop-max", &[("w", &mw), ("h", &mh), ("mp", &mp)]), ui.input(|i| i.time) + 6.0));
        }
        // A copy too large for other programs (X11), once "Copied" is shown.
        if !w.copying
            && let Some(n) = w.too_big.as_ref().and_then(|r| r.try_recv().ok())
        {
            w.too_big = None;
            w.note = Some((i18n.t_args("edit-copy-too-large", &[("size", &mib(n))]), ui.input(|i| i.time) + 12.0));
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
                // −, the percentage to type, the slider and +, as in the viewer (#152).
                let ppp = ui.ctx().pixels_per_point();
                let range = [super::ZOOM_LEAST * ppp, super::ZOOM_MOST * ppp];
                match crate::zoom_bar::group(ui, i18n, super::ZOOM_SALT, shown_zoom(w.shown.1, ppp), range) {
                    Some(ZoomAsk::In) => w.zoom(1.25),
                    Some(ZoomAsk::Out) => w.zoom(0.8),
                    Some(ZoomAsk::To(zoom)) => w.zoom_to(zoom, ppp),
                    None => {}
                }
                ui.separator();
                ui.add(egui::Label::new(i18n.t(status_hint(w.picking, w.tool))).truncate());
                keys
            })
            .inner
        })
        .inner
    }
}

/// Save copy's label and hint; Update copy after a first copy (`copied`).
fn copy_text(copied: bool) -> (&'static str, &'static str) {
    if copied { ("edit-update-copy", "edit-update-hint") } else { ("edit-save-copy", "rc-save-hint") }
}

/// The main save button: what it asks, its label and its hint. A new image has no file yet: Save
/// as…; Overwrite when chosen as the main save in the Options and possible here (`over_main`);
/// else Save copy, or Update copy after a first copy (`copied`).
fn main_save(new: bool, over_main: bool, copied: bool) -> (Ask, &'static str, &'static str) {
    match () {
        _ if new => (Ask::SaveAs, "menu-save-copy", "rc-save-as-hint"),
        _ if over_main => (Ask::Overwrite, "edit-overwrite", "edit-overwrite-hint"),
        _ => {
            let (label, hint) = copy_text(copied);
            (Ask::Save, label, hint)
        }
    }
}

/// Why Save copy (and Overwrite in the menu) is off, as a Fluent id: a save being prepared, the
/// last one still being sent (a phone, a server: the next one waits for it), nothing changed.
pub(super) fn save_off(preparing: bool, sending: bool, changed: bool) -> Option<&'static str> {
    match () {
        _ if preparing => Some("edit-off-preparing"),
        _ if sending => Some("edit-off-sending"),
        _ if !changed => Some("edit-off-unchanged"),
        _ => None,
    }
}

/// Undo and Redo on: a step to go back to (or changes not yet a step), a step to go forward to.
fn undo_redo(w: &Window) -> (bool, bool) {
    w.history.as_ref().map_or((false, false), |h| (h.can_undo() || w.doc != *h.doc(), h.can_redo()))
}

/// Why Selection's Copy and Cut are off, and why its Paste is off, as Fluent ids: no part chosen,
/// nothing copied.
fn marquee_off(marquee: bool, clip: bool) -> (Option<&'static str>, Option<&'static str>) {
    ((!marquee).then_some("edit-off-no-marquee"), (!clip).then_some("edit-off-no-clip"))
}

/// The heading of the Filters menu: where the filter goes, the part chosen or the whole image.
fn filters_heading(marquee: bool) -> &'static str {
    if marquee { "edit-filters-part" } else { "edit-filters-whole" }
}

/// The zoom in the status bar, in pixels of the screen per pixel of the image (1 = 100%): `scale`
/// points per pixel at `ppp` pixels per point.
fn shown_zoom(scale: f32, ppp: f32) -> f32 {
    scale * ppp
}

/// Bytes in MiB, one decimal.
fn mib(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
}

/// The hint of the status bar: the eyedropper's while picking, else the tool's.
fn status_hint(picking: bool, tool: Tool) -> &'static str {
    if picking { "edit-tip-pick" } else { hint_id(tool) }
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
    let (can_undo, can_redo) = undo_redo(w);
    if bar_button(ui, icons::button(ui, Line::Undo), tip(i18n, "edit-undo", &i18n.t("key-undo")), (!can_undo).then(|| i18n.t("edit-off-undo"))) {
        w.undo();
    }
    if bar_button(ui, icons::button(ui, Line::Redo), tip(i18n, "edit-redo", &i18n.t("key-redo")), (!can_redo).then(|| i18n.t("edit-off-redo"))) {
        w.redo();
    }
}

/// The Filters menu: on the part chosen with Selection, or on the whole image.
fn filters_menu(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    let (r, _) = egui::containers::menu::MenuButton::from_button(icons::button(ui, Line::Filters)).ui(ui, |ui| {
        // Readable at once: where the filter goes (the faint line was not read).
        ui.set_max_width(300.0);
        ui.label(egui::RichText::new(i18n.t(filters_heading(w.marquee.is_some()))).strong());
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
    let (none, empty) = marquee_off(w.marquee.is_some(), w.clip.is_some());
    let mut act = None;
    if bar_button(ui, icons::button(ui, Line::Copy), tip(i18n, "edit-copy-part", &i18n.t("key-copy")), none.map(|id| i18n.t(id))) {
        act = Some(Act::EditCopy);
    }
    if bar_button(ui, icons::button(ui, Line::Cut), tip(i18n, "edit-cut-part", &i18n.t("key-cut")), none.map(|id| i18n.t(id))) {
        act = Some(Act::EditCut);
    }
    if bar_button(ui, icons::button(ui, Line::Paste), tip(i18n, "edit-paste-part", &i18n.t("key-paste")), empty.map(|id| i18n.t(id))) {
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

/// The Fluent ids of the keys listed for `tool`: its own, then those of every tool (#331), which
/// its own list does not repeat.
pub(super) fn keys_ids(tool: Tool) -> [&'static str; 2] {
    [keys_id(tool), "hint-edit-common"]
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
    use super::*;

    /// Each id has its message in both languages.
    fn in_both_languages(ids: &[&str]) {
        for ftl in [include_str!("../../locales/en.ftl"), include_str!("../../locales/it.ftl")] {
            for id in ids {
                assert!(ftl.lines().any(|l| l.starts_with(&format!("{id} = "))), "{id}");
            }
        }
    }

    /// Save copy is off while a save is prepared, then while the last one is sent, then when
    /// nothing changed; the first reason that holds is the one shown.
    #[test]
    fn save_off_reasons() {
        assert_eq!(save_off(true, true, false), Some("edit-off-preparing"));
        assert_eq!(save_off(false, true, false), Some("edit-off-sending"));
        assert_eq!(save_off(false, false, false), Some("edit-off-unchanged"));
        assert_eq!(save_off(false, false, true), None);
        in_both_languages(&["edit-off-preparing", "edit-off-sending", "edit-off-unchanged"]);
    }

    /// The main save button: Save as… for a new image (also with Overwrite chosen), Overwrite when
    /// chosen and possible, Save copy, Update copy after a first copy.
    #[test]
    fn main_save_button() {
        for over in [false, true] {
            for copied in [false, true] {
                assert_eq!(main_save(true, over, copied), (Ask::SaveAs, "menu-save-copy", "rc-save-as-hint"));
                assert_eq!(main_save(false, true, copied), (Ask::Overwrite, "edit-overwrite", "edit-overwrite-hint"));
            }
        }
        assert_eq!(main_save(false, false, false), (Ask::Save, "edit-save-copy", "rc-save-hint"));
        assert_eq!(main_save(false, false, true), (Ask::Save, "edit-update-copy", "edit-update-hint"));
        in_both_languages(&["menu-save-copy", "rc-save-as-hint", "edit-overwrite", "edit-overwrite-hint", "edit-save-copy", "rc-save-hint", "edit-update-copy", "edit-update-hint"]);
    }

    /// On the bar only Overwrite the original… is shortened, to Overwrite….
    #[test]
    fn short_labels() {
        assert_eq!(bar_label("edit-overwrite"), "edit-overwrite-short");
        for id in ["edit-save-copy", "edit-update-copy", "menu-save-copy"] {
            assert_eq!(bar_label(id), id);
        }
        in_both_languages(&["edit-overwrite-short"]);
    }

    /// The tools: Select first, Crop last after its separator (as `bar_fit` sums the bar), each
    /// with its own key, none of them I (the eyedropper), each named in both languages.
    #[test]
    fn tools_listed() {
        assert_eq!(TOOLS[0].0, Tool::Select);
        assert_eq!(TOOLS[TOOLS.len() - 1].0, Tool::Crop);
        for (n, a) in TOOLS.iter().enumerate() {
            assert!(TOOLS[n + 1..].iter().all(|b| a.0 != b.0 && a.2 != b.2 && a.3 != b.3), "{}", a.2);
            assert_ne!(a.3, Key::I, "{}", a.2);
        }
        in_both_languages(&TOOLS.map(|t| t.2));
    }

    /// Undo and Redo: off with no history; Undo on with a change not yet a step; after Undo only
    /// Redo, after Redo only Undo.
    #[test]
    fn undo_and_redo_on() {
        use crate::edit::doc::{Doc, Obj, Pt, Shape, Style};
        let mut w = Window::new("p/a.png".into(), Default::default(), Tool::Select);
        assert_eq!(undo_redo(&w), (false, false));
        w.doc = Doc::new(64, 48, None);
        w.history = Some(crate::edit::history::History::new(w.doc.clone()));
        assert_eq!(undo_redo(&w), (false, false));
        w.doc.objects.push(Obj { shape: Shape::Line(Pt::new(0.0, 0.0), Pt::new(9.0, 9.0), None), style: Style::default() });
        assert_eq!(undo_redo(&w), (true, false));
        w.undo();
        assert!(w.doc.objects.is_empty());
        assert_eq!(undo_redo(&w), (false, true));
        w.redo();
        assert_eq!(undo_redo(&w), (true, false));
        in_both_languages(&["edit-off-undo", "edit-off-redo"]);
    }

    /// Selection's Copy and Cut need a part chosen, its Paste something copied.
    #[test]
    fn marquee_buttons() {
        assert_eq!(marquee_off(false, false), (Some("edit-off-no-marquee"), Some("edit-off-no-clip")));
        assert_eq!(marquee_off(true, false), (None, Some("edit-off-no-clip")));
        assert_eq!(marquee_off(false, true), (Some("edit-off-no-marquee"), None));
        assert_eq!(marquee_off(true, true), (None, None));
        assert_eq!((filters_heading(true), filters_heading(false)), ("edit-filters-part", "edit-filters-whole"));
        in_both_languages(&["edit-off-no-marquee", "edit-off-no-clip", "edit-filters-part", "edit-filters-whole"]);
    }

    /// The status bar: the zoom in pixels of the screen (a point is 2 pixels at 200 %), sizes in
    /// MiB, the hint of every tool and of the eyedropper in both languages.
    #[test]
    fn status_bar_texts() {
        let label = |scale, ppp| crate::zoom::percent_text(shown_zoom(scale, ppp), ".");
        assert_eq!(label(1.0, 1.0), "100%");
        assert_eq!(label(0.5, 2.0), "100%");
        assert_eq!(label(0.25, 1.0), "25%");
        assert_eq!(label(2.0, 1.1), "220%");
        assert_eq!(label(0.015, 1.0), "1.5%");
        assert_eq!(mib(0), "0.0");
        assert_eq!(mib(1_048_576), "1.0");
        assert_eq!(mib(25 * 1_048_576 / 2), "12.5");
        for (tool, ..) in TOOLS {
            assert_eq!(status_hint(true, tool), "edit-tip-pick");
            in_both_languages(&[status_hint(false, tool)]);
        }
        in_both_languages(&["edit-tip-pick"]);
    }

    /// The warning that Blur and Pixelate are no cover is in both languages.
    #[test]
    fn blur_warning() {
        for ftl in [include_str!("../../locales/en.ftl"), include_str!("../../locales/it.ftl")] {
            assert!(ftl.lines().any(|l| l.starts_with("edit-filter-not-cover = ")));
        }
        use crate::edit::filter::Filter;
        assert!(Filter::Blur.hides() && Filter::Pixelate.hides() && !Filter::Gray.hides());
    }

    /// Every tool lists its own keys, then those of every tool (#331); its own list does not
    /// repeat a row of those.
    #[test]
    fn keys_with_those_of_every_tool() {
        for (tool, ..) in super::TOOLS {
            assert_eq!(super::keys_ids(tool), [super::keys_id(tool), "hint-edit-common"]);
        }
        for kind in [super::Kind::Patch, super::Kind::Filter] {
            assert_eq!(super::keys_ids(Tool::Draw(kind)), ["hint-edit-select", "hint-edit-common"]);
        }
        for ftl in [include_str!("../../locales/en.ftl"), include_str!("../../locales/it.ftl")] {
            let keys = |id: &str| -> Vec<String> {
                let text = ftl.lines().find_map(|l| l.strip_prefix(&format!("{id} = "))).unwrap_or_else(|| panic!("{id}"));
                text.split('|').filter_map(|row| row.split_once(':')).map(|(k, _)| k.trim().to_string()).collect()
            };
            let common = keys("hint-edit-common");
            for (tool, ..) in super::TOOLS {
                let own = keys(super::keys_id(tool));
                assert!(own.iter().all(|k| !common.contains(k)), "{tool:?} repeats a row of every tool: {own:?}");
            }
        }
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
