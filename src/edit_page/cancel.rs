//! Esc during a drag (audit 5.6): only what the drag did goes back. Until 0.77.0 Esc put back the
//! last step of the history, which lacks what is not a step yet (letters typed while the field
//! had the keys): a text just typed went with a drag that only moved the view, and its index,
//! still selected, made Del remove an object that was not there (panic).

use super::{Hold, Window};

impl Window {
    /// The drag under way stopped and undone, `true` if there was one: an object moved, resized
    /// or turned back as it was, one being drawn gone, the view as before; a selection being
    /// drawn goes. Rotate and crop: the crop and the view as before, no line to straighten.
    pub(super) fn cancel_drag(&mut self) -> bool {
        if let Some((_, crop, view, _)) = self.drag.take() {
            self.doc.crop = crop;
            self.view = Some(view);
            self.line_end = None;
            return true;
        }
        let Some(hold) = self.hold.take() else { return false };
        match hold {
            Hold::Move { index, start, .. } | Hold::Handle { index, start, .. } | Hold::Turn { index, start } => {
                if let Some(o) = self.doc.objects.get_mut(index) {
                    *o = start;
                }
            }
            Hold::Draw { index: Some(i), .. } if i < self.doc.objects.len() => {
                self.doc.objects.remove(i);
            }
            Hold::Draw { .. } => {}
            Hold::Pan { at, k, .. } => self.view = Some((at, k)),
            Hold::Marquee { .. } => self.marquee = None,
        }
        let n = self.doc.objects.len();
        self.selected = self.selected.filter(|&i| i < n);
        self.editing = self.editing.filter(|&i| i < n);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::doc::{Area, Doc, Obj, Pt, Shape, Style};
    use crate::edit::geom::Kind;

    fn text(s: &str, x: f32) -> Obj {
        Obj { shape: Shape::Text { at: Pt::new(x, 0.0), text: s.into(), size: 10.0, rot: 0.0, font: None, path: None }, style: Style::default() }
    }

    /// A page with two texts, the second one's letters typed since the last step, selected.
    fn page() -> Window {
        let mut w = Window::new("p/a.png".into(), Default::default(), super::super::Tool::Select);
        w.doc = Doc { objects: vec![text("a", -10.0), text("b", 10.0)], ..Doc::new(64, 48, None) };
        w.history = Some(crate::edit::history::History::new(w.doc.clone()));
        w.doc.objects[1] = text("bcd", 10.0);
        w.selected = Some(1);
        w
    }

    /// Esc during each kind of drag: what the drag did goes back, what was done before it (not
    /// yet a step) stays, the selection stays on an object that is there.
    #[test]
    fn esc_undoes_the_drag_only() {
        let moved = |w: &mut Window| w.doc.objects[1] = text("bcd", 30.0);
        let start = text("bcd", 10.0);
        for hold in [
            Hold::Move { index: 1, start: start.clone(), from: Pt::new(10.0, 0.0) },
            Hold::Handle { index: 1, handle: 0, start: start.clone() },
            Hold::Turn { index: 1, start: start.clone() },
        ] {
            let mut w = page();
            moved(&mut w);
            w.hold = Some(hold);
            assert!(w.cancel_drag());
            assert_eq!(w.doc.objects[1], start, "the text back where it was, its letters kept");
            assert_eq!((w.selected, w.hold.is_none()), (Some(1), true));
        }
        // An object being drawn goes; the others stay as they are.
        let mut w = page();
        w.doc.objects.push(Obj { shape: Shape::Rect(Area { centre: Pt::new(0.0, 0.0), w: 9.0, h: 9.0, rot: 0.0 }), style: Style::default() });
        w.hold = Some(Hold::Draw { kind: Kind::Rect, from: Pt::new(0.0, 0.0), index: Some(2) });
        assert!(w.cancel_drag());
        assert_eq!(w.doc.objects, vec![text("a", -10.0), text("bcd", 10.0)]);
        // The view moved goes back; the document is not touched.
        let mut w = page();
        w.view = Some((eframe::egui::vec2(50.0, 0.0), 2.0));
        w.hold = Some(Hold::Pan { at: eframe::egui::vec2(0.0, 0.0), k: 1.0, from: eframe::egui::pos2(0.0, 0.0) });
        assert!(w.cancel_drag());
        assert_eq!((w.view, w.doc.objects[1].clone()), (Some((eframe::egui::vec2(0.0, 0.0), 1.0)), text("bcd", 10.0)));
        // A selection being drawn goes.
        let mut w = page();
        w.marquee = Some(Area { centre: Pt::new(0.0, 0.0), w: 9.0, h: 9.0, rot: 0.0 });
        w.hold = Some(Hold::Marquee { from: Pt::new(0.0, 0.0) });
        assert!(w.cancel_drag() && w.marquee.is_none());
        // Nothing held: nothing to stop.
        assert!(!page().cancel_drag());
    }

    /// Rotate and crop: Esc during a drag of the crop puts back the crop and the view as they
    /// were when it started (it went on to close the page).
    #[test]
    fn esc_undoes_a_drag_of_the_crop() {
        let mut w = page();
        let before = w.doc.crop;
        w.drag = Some((super::super::Grab::Move, before, (eframe::egui::vec2(1.0, 2.0), 3.0), eframe::egui::pos2(0.0, 0.0)));
        w.doc.crop.cx += 20.0;
        assert!(w.cancel_drag());
        assert_eq!((w.doc.crop, w.view, w.drag.is_none()), (before, Some((eframe::egui::vec2(1.0, 2.0), 3.0)), true));
    }
}
