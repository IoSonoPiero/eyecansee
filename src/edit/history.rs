//! Undo and redo of the editor: one whole `Doc` per operation done (an object added, moved at the
//! end of the drag, changed or deleted, the crop, the angle, a turn, the fill). Keeping states
//! instead of inverse operations leaves nothing to undo wrongly. The photo is never copied, but a
//! state holds the pixels of its pasted pieces (a turn makes a turned copy of them) and the
//! previews of its filters, highlighters and lenses, shared with the other states while they are
//! the same: the oldest steps go when those pixels pass `MOST_BYTES`, or past `MOST_STEPS` (audit
//! 5.9).

use std::collections::HashSet;

use super::doc::Doc;

/// The pixels the states may hold besides those of the one shown, each buffer counted once:
/// 256 MB.
const MOST_BYTES: usize = 256 << 20;
/// The steps kept at most.
const MOST_STEPS: usize = 1000;

pub struct History {
    states: Vec<Doc>,
    /// The state shown.
    at: usize,
    /// The document as last saved (as opened, at first): closing asks only when it changed.
    /// `None` after a save that failed: closing asks whatever is shown.
    saved: Option<Doc>,
}

impl History {
    pub fn new(doc: Doc) -> History {
        History { saved: Some(doc.clone()), states: vec![doc], at: 0 }
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
        self.trim();
    }

    /// `push` of a copy of `doc`, made only when it is not the document shown: the end of every
    /// frame asks, and the copy holds every point of the strokes drawn by hand (audit 5.13).
    pub fn push_if_changed(&mut self, doc: &Doc) {
        if *doc != *self.doc() {
            self.push(doc.clone());
        }
    }

    /// The oldest steps gone while there are too many or they hold too many pixels; one step back
    /// always stays. Without a word: Undo turns grey when there is none left.
    fn trim(&mut self) {
        while self.at >= 2 && (self.states.len() > MOST_STEPS + 1 || held(&self.states, self.at) > MOST_BYTES) {
            self.states.remove(0);
            self.at -= 1;
        }
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

    /// Something to save: the document shown is not the one opened or last saved.
    pub fn changed(&self) -> bool {
        self.saved.as_ref() != Some(self.doc())
    }

    /// `doc` was just saved (a copy, or over the original): the page stays open, nothing more to
    /// save while it is the one shown. The document written, not the one shown now: a change made
    /// while the save was prepared is still to save (audit 5.2, #199).
    pub fn saved(&mut self, doc: Doc) {
        self.saved = Some(doc);
    }

    /// The save failed: nothing counts as saved, closing asks again.
    pub fn unsaved(&mut self) {
        self.saved = None;
    }
}

/// The bytes of the pixels `states` hold for Undo, each buffer once: not those the state shown
/// (`at`) holds itself, which dropping steps would not give back (counted, a large piece pasted
/// left a single step back for the rest of the page).
fn held(states: &[Doc], at: usize) -> usize {
    fn buffers(d: &Doc) -> impl Iterator<Item = &super::doc::Pixels> {
        d.objects.iter().filter_map(|o| o.shape.held())
    }
    let mut seen: HashSet<usize> = buffers(&states[at]).map(|p| p.id()).collect();
    states.iter().flat_map(buffers).filter(|p| seen.insert(p.id())).map(|p| p.bytes()).sum()
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
            Box::new(|d| d.objects.push(Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "a".into(), size: 20.0, rot: 0.0, font: None, path: None }, style: Style::default() })),
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
            Box::new(|d| d.objects[1].shape = Shape::Text { at: Pt::new(0.0, 0.0), text: "ab".into(), size: 20.0, rot: 0.0, font: None, path: None }),
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

    /// The history holds at most 256 MB of pixels (pasted pieces, previews; a buffer shared by
    /// many steps counted once) and 1000 steps; the oldest go first, one step back always stays.
    /// It had no limit: a large piece pasted and turned ten times was about half a gigabyte kept
    /// until the page closed (audit 5.9). The pieces are not filled in: no memory is used.
    #[test]
    fn history_has_a_ceiling() {
        use crate::edit::doc::Pixels;
        let piece = |side| Obj { shape: Shape::Patch { area: Area { centre: Pt::new(0.0, 0.0), w: 10.0, h: 10.0, rot: 0.0 }, pixels: Pixels::new(image::RgbaImage::new(side, side)), mirrored: false }, style: Style::default() };
        // Each step a new piece of 16 MB: 16 of them behind the one shown.
        let mut h = History::new(Doc::new(100, 80, None));
        for _ in 0..20 {
            h.apply(|d| d.objects = vec![piece(2048)]);
        }
        assert!(h.states.len() <= 17 && h.can_undo(), "{} steps kept", h.states.len());
        // One piece shared by every step: counted once, only the number of steps limits them.
        let mut h = History::new(Doc::new(100, 80, None));
        h.apply(|d| d.objects = vec![piece(2048)]);
        for i in 0..1005 {
            h.apply(|d| d.angle = i as f32 + 1.0);
        }
        assert_eq!(h.states.len(), 1001);
        assert_eq!((h.doc().angle, h.doc().objects.len()), (1005.0, 1));
        while h.can_undo() {
            h.undo();
        }
        assert_eq!(h.doc().angle, 5.0, "the oldest steps gone");
        // A piece larger than all of it alone: one step back still.
        let mut h = History::new(Doc::new(100, 80, None));
        h.apply(|d| d.objects = vec![piece(9000)]);
        h.apply(|d| d.objects = vec![piece(9000)]);
        assert_eq!(h.states.len(), 2);
        h.undo();
        assert!(!h.can_undo() && h.doc().objects.len() == 1);
        // What the image shown holds itself is not the history's: dropping steps would not give
        // it back, and Undo kept only one step for the rest of the page.
        let mut h = History::new(Doc::new(100, 80, None));
        h.apply(|d| d.objects = (0..17).map(|_| piece(2048)).collect());
        for i in 0..10 {
            h.apply(|d| d.angle = i as f32 + 1.0);
        }
        assert_eq!(h.states.len(), 12);
    }

    /// Saved: nothing to save until something changes again; undone past the save, again.
    #[test]
    fn saved_is_unchanged() {
        let mut h = History::new(crate::edit::doc::tests::sample());
        h.apply(|d| d.angle = 7.0);
        assert!(h.changed());
        h.saved(h.doc().clone());
        assert!(!h.changed());
        h.undo();
        assert!(h.changed());
        // A save that failed: something to save, whatever is shown.
        h.redo();
        h.unsaved();
        assert!(h.changed());
    }
}
