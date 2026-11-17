//! Ctrl+Z and Ctrl+Y in the viewer (issue #157): the turns and flips of the picture shown, not
//! saved yet, taken back and done again. Each step is kept as the turn before and after it; a
//! step whose "after" is not the turn shown any more (the picture saved, another picture, the
//! turn reset) is forgotten, so nothing has to be told when that happens.

use eframe::egui::{self, Key, Modifiers};

use crate::App;
use crate::transform::Turn;

/// The steps of one picture.
#[derive(Default)]
pub struct Steps {
    path: String,
    back: Vec<(Turn, Turn)>,
    ahead: Vec<(Turn, Turn)>,
}

impl Steps {
    /// A turn made on `path`: from `before` to `after`.
    pub fn done(&mut self, path: &str, before: Turn, after: Turn) {
        if path != self.path || self.back.last().is_some_and(|s| s.1 != before) {
            *self = Steps { path: path.to_string(), ..Default::default() };
        }
        self.back.push((before, after));
        self.ahead.clear();
    }

    /// The turn to show after Ctrl+Z on `path`, now turned `now`.
    pub fn undo(&mut self, path: &str, now: Turn) -> Option<Turn> {
        let step = self.back.last().copied().filter(|s| path == self.path && s.1 == now);
        let Some(step) = step else {
            *self = Steps::default();
            return None;
        };
        self.back.pop();
        self.ahead.push(step);
        Some(step.0)
    }

    /// The turn to show after Ctrl+Y on `path`, now turned `now`.
    pub fn redo(&mut self, path: &str, now: Turn) -> Option<Turn> {
        let step = self.ahead.last().copied().filter(|s| path == self.path && s.0 == now)?;
        self.ahead.pop();
        self.back.push(step);
        Some(step.1)
    }
}

impl App {
    /// Ctrl+Z, Ctrl+Y and Ctrl+Shift+Z on the picture shown.
    pub(crate) fn turn_undo_keys(&mut self, ctx: &egui::Context, current: usize) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (redo, undo) = ctx.input_mut(|i| {
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y);
            (redo, i.consume_key(Modifiers::COMMAND, Key::Z))
        });
        let Some(path) = self.files().get(current).map(|f| f.path.clone()) else { return };
        let turned = if redo {
            self.turn_steps.redo(&path, self.turn)
        } else if undo {
            self.turn_steps.undo(&path, self.turn)
        } else {
            None
        };
        if let Some(t) = turned {
            log!("viewer: turn {} to {t:?}", if redo { "redone" } else { "undone" });
            self.turn = t;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Op;

    #[test]
    fn back_and_again() {
        let mut s = Steps::default();
        let (a, b) = (Turn::default().then(Op::Cw), Turn::default().then(Op::Cw).then(Op::FlipH));
        s.done("p", Turn::default(), a);
        s.done("p", a, b);
        assert_eq!(s.undo("p", b), Some(a));
        assert_eq!(s.undo("p", a), Some(Turn::default()));
        assert_eq!(s.undo("p", Turn::default()), None, "nothing more");
        assert_eq!(s.redo("p", Turn::default()), None, "forgotten with the undo that found nothing");
        s.done("p", Turn::default(), a);
        s.done("p", a, b);
        assert_eq!(s.undo("p", b), Some(a));
        assert_eq!(s.redo("p", a), Some(b));
        // A new turn after an undo: what was ahead is gone.
        assert_eq!(s.undo("p", b), Some(a));
        s.done("p", a, Turn::default());
        assert_eq!(s.redo("p", Turn::default()), None);
    }

    /// Saved (the turn back to none) or another picture: the steps are not used.
    #[test]
    fn forgotten_when_the_turn_changed_elsewhere() {
        let mut s = Steps::default();
        let a = Turn::default().then(Op::Cw);
        s.done("p", Turn::default(), a);
        assert_eq!(s.undo("p", Turn::default()), None, "saved: shown as none");
        s.done("p", Turn::default(), a);
        assert_eq!(s.undo("q", a), None, "another picture");
        // A turn on another picture starts its own steps.
        s.done("p", Turn::default(), a);
        s.done("q", Turn::default(), a);
        assert_eq!(s.undo("q", a), Some(Turn::default()));
        assert_eq!(s.undo("p", a), None);
    }
}
