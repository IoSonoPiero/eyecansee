//! Undo and redo of the editor: one whole `Doc` per operation done (an object added, moved at the
//! end of the drag, changed or deleted, the crop, the angle, a turn, the fill). The document is a
//! few kilobytes and the image is never copied; keeping states instead of inverse operations
//! leaves nothing to undo wrongly.

use super::doc::Doc;

pub struct History {
    states: Vec<Doc>,
    /// The state shown.
    at: usize,
}

impl History {
    pub fn new(doc: Doc) -> History {
        History { states: vec![doc], at: 0 }
    }

    pub fn doc(&self) -> &Doc {
        &self.states[self.at]
    }

    /// An operation done on the document shown: what was undone after it is gone. A change that
    /// leaves the document as it was is no operation.
    pub fn push(&mut self, doc: Doc) {
        if doc == *self.doc() {
            return;
        }
        self.states.truncate(self.at + 1);
        self.states.push(doc);
        self.at += 1;
    }

    /// `change` done on a copy of the document shown, kept as one operation.
    #[cfg(test)]
    pub fn apply(&mut self, change: impl FnOnce(&mut Doc)) {
        let mut doc = self.doc().clone();
        change(&mut doc);
        self.push(doc);
    }

    pub fn can_undo(&self) -> bool {
        self.at > 0
    }

    pub fn can_redo(&self) -> bool {
        self.at + 1 < self.states.len()
    }

    pub fn undo(&mut self) {
        if self.can_undo() {
            self.at -= 1;
        }
    }

    pub fn redo(&mut self) {
        if self.can_redo() {
            self.at += 1;
        }
    }

    /// Something to save: the document shown is not the one opened.
    pub fn changed(&self) -> bool {
        *self.doc() != self.states[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::doc::{Area, Dash, Obj, Pt, Shape, Style};
    use crate::rotate::Crop;
    use crate::transform::Op;

    fn rect() -> Obj {
        Obj { shape: Shape::Rect(Area { centre: Pt::new(1.0, 2.0), w: 10.0, h: 5.0, rot: 0.0 }), style: Style::default() }
    }

    /// Each kind of operation, undone and redone one at a time: the states come back in order.
    #[test]
    fn undo_and_redo_every_operation() {
        type Change = Box<dyn Fn(&mut Doc)>;
        let ops: Vec<Change> = vec![
            Box::new(|d| d.objects.push(rect())),
            Box::new(|d| d.objects.push(Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "a".into(), size: 20.0, rot: 0.0 }, style: Style::default() })),
            // Moved.
            Box::new(|d| {
                if let Shape::Rect(r) = &mut d.objects[0].shape {
                    r.centre = Pt::new(-4.0, 3.0);
                }
            }),
            // Resized.
            Box::new(|d| {
                if let Shape::Rect(r) = &mut d.objects[0].shape {
                    r.w = 30.0;
                }
            }),
            Box::new(|d| d.objects[0].style = Style { color: [0, 0, 255], width: 9.0, opacity: 0.5, dash: Dash::Dotted, ..Style::default() }),
            Box::new(|d| d.objects[1].shape = Shape::Text { at: Pt::new(0.0, 0.0), text: "ab".into(), size: 20.0, rot: 0.0 }),
            Box::new(|d| {
                d.objects.remove(1);
            }),
            Box::new(|d| d.crop = Crop { cx: 3.0, cy: -2.0, w: 50.0, h: 40.0 }),
            Box::new(|d| d.angle = 12.5),
            Box::new(|d| d.turned(Op::Cw)),
            Box::new(|d| d.turned(Op::FlipV)),
            Box::new(|d| d.fill = Some([255, 255, 255])),
            Box::new(|d| d.objects.push(Obj { shape: Shape::Cover(Area { centre: Pt::new(0.0, 0.0), w: 8.0, h: 8.0, rot: 0.0 }), style: Style::default() })),
        ];
        let mut h = History::new(Doc::new(100, 80, None));
        let mut states = vec![h.doc().clone()];
        for op in &ops {
            h.apply(|d| op(d));
            assert_ne!(h.doc(), states.last().unwrap(), "each operation is a new state");
            states.push(h.doc().clone());
        }
        assert!(h.changed() && !h.can_redo());
        for i in (0..ops.len()).rev() {
            h.undo();
            assert_eq!(h.doc(), &states[i]);
        }
        assert!(!h.can_undo() && !h.changed());
        h.undo();
        assert_eq!(h.doc(), &states[0], "nothing before the first state");
        for state in &states[1..] {
            h.redo();
            assert_eq!(h.doc(), state);
        }
        h.redo();
        assert_eq!(h.doc(), states.last().unwrap(), "nothing after the last state");
    }

    /// A new operation after an undo cuts what was undone; a change to nothing adds no step.
    #[test]
    fn new_operation_cuts_the_tail() {
        let mut h = History::new(Doc::new(100, 80, None));
        h.apply(|d| d.angle = 1.0);
        h.apply(|d| d.angle = 2.0);
        h.apply(|d| d.angle = 3.0);
        h.undo();
        h.undo();
        h.apply(|d| d.objects.push(rect()));
        assert!(!h.can_redo());
        assert_eq!((h.doc().angle, h.doc().objects.len()), (1.0, 1));
        h.undo();
        assert_eq!((h.doc().angle, h.doc().objects.len()), (1.0, 0));
        h.apply(|d| d.angle = 1.0);
        assert!(h.can_redo(), "the same document: no operation, the redo is still there");
    }

    /// Changing the crop leaves the objects where they were on the image.
    #[test]
    fn crop_leaves_objects() {
        let mut h = History::new(crate::edit::doc::tests::sample());
        let before = h.doc().objects.clone();
        h.apply(|d| d.crop = Crop { cx: -8.0, cy: 5.0, w: 12.0, h: 9.0 });
        h.apply(|d| d.crop = Crop::centred(40.0, 30.0));
        assert_eq!(h.doc().objects, before);
        h.undo();
        assert_eq!(h.doc().objects, before);
    }
}
