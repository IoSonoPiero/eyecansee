//! Typing a text or a bubble's letters on the image itself ("I want to select
//! the text on the image too"): while it is being typed, a field without a frame lies over it,
//! where the pointer selects letters as in any field; the letters drawn by `edit::render` are
//! hidden meanwhile (the field shows them, in the text's own font: `field_font`, #310), and are
//! back when it ends.
//! A text on a curve is typed in the bar only.

use eframe::egui::{self, Color32, Pos2};

use super::Window;
use crate::edit::doc::{Doc, Shape};

/// The text being typed on the image, if `i` is one that can be: a text not on a curve, or a
/// bubble.
pub(super) fn can(doc: &Doc, i: usize) -> bool {
    matches!(doc.objects.get(i).map(|o| &o.shape), Some(Shape::Text { path: None, .. } | Shape::Bubble { .. }))
}

/// The fields where a text's letters are typed: on the image, in the properties bar.
pub(super) const ON_IMAGE: &str = "edit-letters-on-image";
pub(super) const IN_BAR: &str = "edit-letters-in-bar";

/// A field's own undo (Ctrl+Z while typing) is egui's, kept by the field's id: when the text in
/// them changes (another one selected, or one starting to be typed), both fields start without
/// it, so Ctrl+Z never brings back the letters of another text (a bubble's "Ciao" came back in a
/// text just placed, seen on 2026-11-15). The document's own undo is not touched (`edit_keys`
/// leaves the keys to a field that has them).
pub(super) fn fresh_fields(ctx: &egui::Context, w: &mut Window) {
    if w.selected != w.letters_of || w.focus_text {
        ctx.data_mut(|d| {
            for id in [ON_IMAGE, IN_BAR] {
                d.remove::<egui::text_edit::TextEditState>(egui::Id::unique(id));
            }
        });
        w.letters_of = w.selected;
    }
}

/// The document as drawn under the field: the letters being typed hidden.
pub(super) fn hidden(doc: &Doc, editing: Option<usize>) -> Doc {
    let mut doc = doc.clone();
    if let Some(text) = editing.and_then(|i| doc.objects.get_mut(i)).and_then(|o| o.shape.text_mut()) {
        text.clear();
    }
    doc
}

/// The field over the text `i`, centred on `centre` (on the screen), `k` points a pixel of the
/// image; `on_image`: the image was pressed or clicked in this frame (off the field). Ends when
/// the object is no longer the selected one, and when it loses the keys (Esc, a press on the
/// image), but to a control of the bars (`paused`).
pub(super) fn overlay(ui: &mut egui::Ui, w: &mut Window, i: usize, (centre, on_image): (Pos2, bool), k: f32) {
    if w.selected != Some(i) || !can(&w.doc, i) {
        w.editing = None;
        w.typing_paused = false;
        return;
    }
    let o = &mut w.doc.objects[i];
    let style = o.style;
    // A bubble's letters go to a new line at its width; a text's only where typed.
    let (size, wrap, [r, g, b]) = match &o.shape {
        Shape::Text { size, .. } => (*size, None, style.color),
        Shape::Bubble { area, .. } => (style.letters.max(1.0), Some(crate::edit::bubble::text_width(area.w, &style)), style.ink),
        _ => return,
    };
    let pts = (size * k).max(6.0);
    let face = o.shape.font_mut().and_then(|f| f.clone());
    let font = egui::FontId::new(pts, super::field_font::family(ui.ctx(), face.as_deref()));
    let Some(now) = o.shape.text_mut().map(|t| format!("{t} ")) else { return };
    // The lines as the field will show them (in the text's font, laid out by egui: they may break
    // a little elsewhere than the drawn ones), for its size and the bubble's height.
    let wrap_at = wrap.map_or(f32::INFINITY, |w| w * k);
    let shown = ui.painter().layout(now, font.clone(), Color32::WHITE, wrap_at).size();
    let rect = egui::Rect::from_center_size(centre, egui::vec2(wrap.map_or(shown.x + pts, |w| w * k).max(2.0 * pts), shown.y + 0.2 * pts));
    // The bubble as tall as the lines shown, with its margins: they stayed out of it.
    if let Shape::Bubble { area, .. } = &mut o.shape {
        let needed = crate::edit::bubble::height_for(shown.y / k, &style);
        if area.h < needed {
            area.h = needed;
        }
    }
    fresh_fields(ui.ctx(), w);
    let Some(text) = w.doc.objects[i].shape.text_mut() else { return };
    let field = egui::TextEdit::multiline(text)
        .id(egui::Id::unique(ON_IMAGE))
        .font(font)
        .text_color(Color32::from_rgb(r, g, b))
        .align(egui::Align2::CENTER_CENTER)
        .frame(egui::Frame::NONE)
        .background_color(Color32::TRANSPARENT)
        .desired_width(rect.width())
        .desired_rows(1);
    let response = ui.put(rect, field);
    // A paste does not go through the printable keys.
    if response.changed() {
        crate::edit::render::xml_text(text);
    }
    if std::mem::take(&mut w.focus_text) {
        response.request_focus();
    }
    paused(ui.ctx(), w, &response, on_image);
}

/// The field lost the keys to a control of the bars (the list of fonts, a size, a colour): the
/// typing goes on, and the field takes the keys back, with its cursor where it was, once that
/// control is done with (no button held, no list open, no other field typed in). Until 0.77.0
/// it ended there, and choosing a font meant a click on the text again (#310). Lost otherwise
/// (Esc, a press on the image off the field: `on_image`), or the image pressed while paused: it
/// ends.
fn paused(ctx: &egui::Context, w: &mut Window, field: &egui::Response, on_image: bool) {
    // egui takes the keys from a field at a press or a click elsewhere.
    let pressed = ctx.input(|i| i.pointer.any_pressed() || i.pointer.any_released());
    // `lost_focus` holds for two frames: only the first one decides.
    if field.lost_focus() && !w.typing_paused {
        w.typing_paused = pressed && !on_image;
        if !w.typing_paused {
            w.editing = None;
        }
    } else if w.typing_paused {
        if on_image {
            w.editing = None;
            w.typing_paused = false;
        } else if field.has_focus() {
            w.typing_paused = false;
        } else if !ctx.input(|i| i.pointer.any_down()) && !egui::Popup::is_any_open(ctx) && !ctx.text_edit_focused() {
            field.request_focus();
            w.typing_paused = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::doc::{Area, Obj, Pt, Style};

    /// Texts not on a curve and bubbles are typed on the image; their letters are hidden under
    /// the field, the others' are not.
    #[test]
    fn typed_on_the_image() {
        let text = |path| Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "Ab".into(), size: 10.0, rot: 0.0, font: None, path }, style: Style::default() };
        let bubble = Obj { shape: Shape::Bubble { area: Area { centre: Pt::new(0.0, 0.0), w: 40.0, h: 20.0, rot: 0.0 }, tail: Pt::new(0.0, 30.0), text: "Hi".into(), font: None }, style: Style::default() };
        let doc = Doc { objects: vec![text(None), text(Some((Pt::new(0.0, 0.0), Pt::new(9.0, 0.0), None))), bubble], ..Doc::new(10, 10, None) };
        assert_eq!((can(&doc, 0), can(&doc, 1), can(&doc, 2), can(&doc, 3)), (true, false, true, false));
        let shown = hidden(&doc, Some(2));
        let letters = |d: &Doc, i: usize| d.objects[i].shape.clone().text_mut().cloned();
        assert_eq!((letters(&shown, 0), letters(&shown, 2)), (Some("Ab".into()), Some(String::new())));
        assert_eq!(hidden(&doc, None), doc);
    }

    /// Ctrl+Z in the field of a text just placed undoes only its own letters: egui keeps a
    /// field's undo by its id, and the letters of the text typed before ("Ciao", a bubble's)
    /// came back in the new one (seen on 2026-11-15).
    #[test]
    fn undo_in_the_field_is_its_own() {
        let ctx = egui::Context::default();
        let text = Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: String::new(), size: 10.0, rot: 0.0, font: None, path: None }, style: Style::default() };
        let mut w = Window::new("p/a.png".into(), Default::default(), super::super::Tool::Select);
        w.doc = Doc { objects: vec![text.clone()], ..Doc::new(64, 48, None) };
        let mut time = 0.0;
        let mut frame = |w: &mut Window, events: Vec<egui::Event>| {
            time += 2.0;
            let input = egui::RawInput { time: Some(time), events, screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))), ..Default::default() };
            let i = w.selected.unwrap();
            w.editing = Some(i);
            ctx.run_ui(input, |ui| overlay(ui, w, i, (Pos2::new(200.0, 150.0), false), 1.0)).textures_delta.clear();
        };
        let typed = |s: &str| vec![egui::Event::Text(s.into())];
        let undo = vec![egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }];
        // The first text, typed and left.
        (w.selected, w.focus_text) = (Some(0), true);
        for e in [vec![], typed("Ciao"), vec![], vec![]] {
            frame(&mut w, e);
        }
        // A second one placed, typed, Ctrl+Z.
        w.doc.objects.push(text);
        (w.selected, w.focus_text) = (Some(1), true);
        for e in [vec![], typed("Te"), vec![], vec![], undo.clone(), vec![], undo, vec![]] {
            frame(&mut w, e);
        }
        let letters = |w: &mut Window, i: usize| w.doc.objects[i].shape.text_mut().cloned().unwrap();
        assert_eq!((letters(&mut w, 0), letters(&mut w, 1)), ("Ciao".to_string(), String::new()));
    }
}
