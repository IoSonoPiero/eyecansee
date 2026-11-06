//! Typing a text or a bubble's letters on the image itself (user, 2026-11-06: "I want to select
//! the text on the image too"): while it is being typed, a field without a frame lies over it,
//! where the pointer selects letters as in any field; the letters drawn by `edit::render` are
//! hidden meanwhile (the field shows them, in the interface's font), and are back when it ends.
//! A text on a curve is typed in the bar only.

use eframe::egui::{self, Color32, Pos2};

use super::Window;
use crate::edit::doc::{Doc, Shape};

/// The text being typed on the image, if `i` is one that can be: a text not on a curve, or a
/// bubble.
pub(super) fn can(doc: &Doc, i: usize) -> bool {
    matches!(doc.objects.get(i).map(|o| &o.shape), Some(Shape::Text { path: None, .. } | Shape::Bubble { .. }))
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
/// image. Ends when it loses the keys or the object is no longer the selected one.
pub(super) fn overlay(ui: &mut egui::Ui, w: &mut Window, i: usize, centre: Pos2, k: f32) {
    if w.selected != Some(i) || !can(&w.doc, i) {
        w.editing = None;
        return;
    }
    let o = &mut w.doc.objects[i];
    let style = o.style;
    let (size, room, [r, g, b], text) = match &mut o.shape {
        Shape::Text { text, size, at, rot, .. } => {
            let room = crate::edit::geom::text_area(*at, text, *size, *rot).w + *size;
            (*size, room, style.color, text)
        }
        Shape::Bubble { text, area, .. } => (style.letters.max(1.0), crate::edit::bubble::text_width(area.w, &style), style.ink, text),
        _ => return,
    };
    let pts = (size * k).max(6.0);
    let rows = text.lines().count().max(1) + usize::from(text.ends_with('\n'));
    let rect = egui::Rect::from_center_size(centre, egui::vec2((room * k).max(2.0 * pts), rows as f32 * 1.25 * pts + 0.4 * pts));
    let field = egui::TextEdit::multiline(text)
        .font(egui::FontId::proportional(pts))
        .text_color(Color32::from_rgb(r, g, b))
        .align(egui::Align2::CENTER_CENTER)
        .frame(egui::Frame::NONE)
        .background_color(Color32::TRANSPARENT)
        .desired_width(rect.width())
        .desired_rows(rows);
    let response = ui.put(rect, field);
    if std::mem::take(&mut w.focus_text) {
        response.request_focus();
    }
    if response.lost_focus() {
        w.editing = None;
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
}
