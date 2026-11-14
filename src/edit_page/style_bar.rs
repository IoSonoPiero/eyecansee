//! The properties bar of the editor (`tools::props_bar`): the style of the selected object, or of
//! the next one the tool draws (colours, width, opacity, dashes, the font and the letters of a
//! text), the eyedroppers, a filter's amount. Moved from `tools.rs` on 2026-11-14.

use eframe::egui::{self, Color32};

use super::tools::{bar_button, tip};
use super::{Tool, Window};
use crate::edit::doc::{Dash, Head, Pt, Shape, Style};
use crate::edit::geom::Kind;
use crate::i18n::I18n;
use crate::icons::{self, Line};

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

/// A filter selected: its name, its amount, delete.
fn filter_bar(ui: &mut egui::Ui, w: &mut Window, i: usize, i18n: &I18n) {
    let long = w.long();
    let Some(Shape::Filter { filter, amount, .. }) = w.doc.objects.get_mut(i).map(|o| &mut o.shape) else { return };
    ui.strong(i18n.t(filter.label()));
    if let Some((lo, hi, _)) = filter.amount(long) {
        ui.spacing_mut().slider_width = 160.0;
        super::props::slider(ui, "edit-filter-amount", i18n.t("edit-filter-amount"), egui::Slider::new(amount, lo..=hi).integer());
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

/// The style of the selected object, or of the next one the tool draws; the last one used for
/// each kind is kept. `true` when the screen's eyedropper is asked for.
pub(super) fn style_bar(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) -> bool {
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
    // What the controls change: the object selected, or the next one drawn
    // ("what is the opacity for, if nothing is selected?").
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
    // A piece brought to the front or sent to the back is at its new place.
    if let Some(i) = selected.and(w.selected) {
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
    if let (Kind::Patch, Some(i)) = (kind, selected) {
        super::patch::bar(ui, w, i, i18n);
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

/// One eyedropper ("only the one that takes any colour"), its button or I:
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
    let slider = |ui: &mut egui::Ui, id: &'static str, value: &mut f32, range, suffix| {
        ui.separator();
        super::props::slider(ui, id, t(id), egui::Slider::new(value, range).integer().suffix(suffix));
    };
    ui.spacing_mut().slider_width = 110.0;
    match kind {
        Kind::Line | Kind::Arrow | Kind::Pen | Kind::Rect | Kind::Ellipse | Kind::Bubble => slider(ui, "edit-width", &mut style.width, 1.0..=max, " px"),
        // A lens may have no border.
        Kind::Lens => slider(ui, "edit-width", &mut style.width, 0.0..=max, " px"),
        Kind::Text | Kind::Step | Kind::Stamp => slider(ui, "edit-size", &mut style.width, 4.0..=(long / 4.0).max(100.0), " px"),
        Kind::Highlight | Kind::Cover | Kind::Patch | Kind::Filter | Kind::Spotlight => {}
    }
    if kind != Kind::Cover {
        let mut pct = style.opacity * 100.0;
        // A spotlight's opacity is how dark the rest gets.
        slider(ui, if kind == Kind::Spotlight { "edit-intensity" } else { "edit-opacity" }, &mut pct, 5.0..=100.0, " %");
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
    ui.spacing_mut().slider_width = 110.0;
    if super::props::slider(ui, "rc-rotation", i18n.t("rc-rotation"), egui::Slider::new(&mut shown, -180.0..=180.0).integer().suffix("°")).changed() {
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
        crate::theme::bar_hint(ui, i18n.t("edit-text-hint"));
        return;
    };
    // As wide as the hint, so the empty box shows it on one line.
    let hint = i18n.t("edit-text-hint");
    let r = ui.add(egui::TextEdit::multiline(text).desired_rows(1).desired_width(crate::theme::hint_width(ui, &hint).max(280.0)).hint_text(hint));
    // A text typed on the image takes the keys there (`typing`).
    if w.editing.is_none() && std::mem::take(&mut w.focus_text) {
        r.request_focus();
    }
}

#[cfg(test)]
mod tests {
    /// The properties bar of a text, nothing selected and selected with its outline and box on,
    /// in every language: nothing past the right edge of the window down to 700 px (a slider at
    /// the end of a row was cut: "Opacity 100 %" in a 900 px window).
    #[test]
    fn text_bar_not_cut() {
        use super::*;
        use crate::edit::doc::{Back, Obj};
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        for (lang, _) in crate::i18n::languages() {
            let i18n = I18n::new(lang);
            for (width, selected) in [1200.0, 900.0, 700.0].into_iter().flat_map(|w| [(w, false), (w, true)]) {
                let mut w = Window::new("p/a.png".into(), Default::default(), Tool::Draw(Kind::Text));
                if selected {
                    let style = Style { outline: Some(([255, 255, 255], 0.08)), back: Some(Back::default()), ..Style::default() };
                    let shape = Shape::Text { at: Pt::new(0.0, 0.0), text: "Text".into(), size: 40.0, rot: 0.0, font: None, path: None };
                    w.doc.objects.push(Obj { shape, style });
                    w.selected = Some(0);
                }
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))), ..Default::default() };
                let mut edge = 0.0;
                // The fonts are ready from the second pass.
                for _ in 0..3 {
                    // Debug builds check that the texture changes were taken: nobody draws them here.
                    ctx.run_ui(input.clone(), |ui| {
                        egui::Panel::top("edit-props").show(ui, |ui| {
                            let r = ui.horizontal_wrapped(|ui| style_bar(ui, &mut w, &i18n));
                            edge = r.response.rect.right();
                        });
                    })
                    .textures_delta
                    .clear();
                }
                assert!(edge <= width, "{lang}, {width} px, selected {selected}: drawn to {edge}");
            }
        }
    }
}
