//! The window's ✕: what it closes. With "The window's ✕ goes back to the grid" on in the Options,
//! one step at a time (the editor, then the viewer, then EyeCanSee); off, EyeCanSee at once, from
//! any page, asking once about the changes not saved (decided on 2026-11-14).

use eframe::egui;

use crate::App;

/// What is in front when the ✕ is clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Grid,
    Viewer,
    Editor,
}

/// What the ✕ closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Only the editor: the viewer (or the grid) under it stays.
    Editor,
    /// The viewer, back to the grid.
    Viewer,
    /// EyeCanSee.
    Quit,
}

/// The answer to the ✕: what closes, and whether a question about the changes not saved comes
/// first (one only: the editor's, or the viewer's for a turned image).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Close {
    pub step: Step,
    pub ask: bool,
}

/// The ✕ over `page`; `step_back`: the option in the Options; `changes`: changes not saved on that
/// page (the editor's drawing, or the viewer's turned image).
pub(crate) fn on_close(step_back: bool, page: Page, changes: bool) -> Close {
    let step = match (step_back, page) {
        (true, Page::Editor) => Step::Editor,
        (true, Page::Viewer) => Step::Viewer,
        _ => Step::Quit,
    };
    Close { step, ask: changes }
}

/// What Esc does in the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Esc {
    /// Leaves full screen, the viewer stays.
    LeaveFullScreen,
    /// Back to the grid.
    Viewer,
    /// EyeCanSee closes, as with the ✕ (which asks about a turned image not saved first).
    Quit,
    /// Nothing: Esc held down after leaving full screen does not close EyeCanSee too.
    Nothing,
}

/// Esc in the viewer (#320): `step_back`, the option in the Options, as the ✕; `full`: in full
/// screen, which it leaves first; `repeat`: the key held down.
pub(crate) fn on_esc(step_back: bool, full: bool, repeat: bool) -> Esc {
    match (full, step_back, repeat) {
        (true, ..) => Esc::LeaveFullScreen,
        (false, true, _) => Esc::Viewer,
        (false, false, false) => Esc::Quit,
        (false, false, true) => Esc::Nothing,
    }
}

/// What a double click does in the viewer (#349).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Double {
    /// Back to the grid (at full screen the window stays so, as up to 0.77.0).
    Grid,
    /// To full screen, as F11.
    EnterFullScreen,
    /// Out of full screen, the viewer stays.
    LeaveFullScreen,
    /// Nothing here: the double click is Measure's.
    Nothing,
}

/// A double click in the viewer: `to_full`, "Goes to full screen" chosen in the Options;
/// `full`: at full screen; `measuring`: Measure on, which keeps the double click (it closes an
/// area on a drawing).
pub(crate) fn on_double_click(to_full: bool, full: bool, measuring: bool) -> Double {
    match (measuring, to_full, full) {
        (true, ..) => Double::Nothing,
        (false, false, _) => Double::Grid,
        (false, true, false) => Double::EnterFullScreen,
        (false, true, true) => Double::LeaveFullScreen,
    }
}

impl App {
    /// The key shown beside "Back to the grid": Esc, unless Esc closes EyeCanSee from the viewer
    /// (the ✕ set to close it, #320); then Enter, which goes back in both cases.
    pub(crate) fn back_key(&self) -> String {
        if self.settings.close_to_grid { "Esc".into() } else { self.i18n.t("key-enter") }
    }

    /// What the ✕ does now, over the page in front; `changes`: on that page.
    pub(crate) fn on_close(&self, changes: bool) -> Close {
        let page = if self.editor.is_some() {
            Page::Editor
        } else if self.viewing.is_some() {
            Page::Viewer
        } else {
            Page::Grid
        };
        on_close(self.settings.close_to_grid, page, changes)
    }

    /// The ✕ clicked over the editor with the option off (`quit_after_editor`): EyeCanSee closes
    /// once the editor has, after its question if it had changes; that question also covered the
    /// viewer's turned image (the editor started from it), so it is not asked again. Cancelled, or
    /// a save that failed, and the editor stays open: nothing more.
    pub(crate) fn quit_after_editor(&mut self, ctx: &egui::Context) {
        let Some(asked) = self.quit_after_editor else { return };
        match &self.editor {
            None => {
                self.quit_after_editor = None;
                if asked {
                    self.turn = Default::default();
                }
                log!("close: editor closed, now EyeCanSee (asked {asked})");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(w) if !w.on_the_way_out() => {
                log!("close: the editor stays open, EyeCanSee too");
                self.quit_after_editor = None;
            }
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every page with the option on and off, with and without changes not saved.
    #[test]
    fn the_cross_closes_a_step_or_all() {
        for changes in [false, true] {
            let close = |step_back, page| on_close(step_back, page, changes);
            // On: one step at a time.
            assert_eq!(close(true, Page::Editor), Close { step: Step::Editor, ask: changes });
            assert_eq!(close(true, Page::Viewer), Close { step: Step::Viewer, ask: changes });
            assert_eq!(close(true, Page::Grid), Close { step: Step::Quit, ask: changes });
            // Off: EyeCanSee from every page, asking once when something is not saved.
            assert_eq!(close(false, Page::Editor), Close { step: Step::Quit, ask: changes });
            assert_eq!(close(false, Page::Viewer), Close { step: Step::Quit, ask: changes });
            assert_eq!(close(false, Page::Grid), Close { step: Step::Quit, ask: changes });
        }
    }

    /// Esc in the viewer: full screen first, then as the ✕; held down it never closes EyeCanSee.
    #[test]
    fn esc_in_the_viewer_follows_the_cross() {
        for step_back in [true, false] {
            for repeat in [false, true] {
                assert_eq!(on_esc(step_back, true, repeat), Esc::LeaveFullScreen);
            }
        }
        assert_eq!(on_esc(true, false, false), Esc::Viewer);
        assert_eq!(on_esc(true, false, true), Esc::Viewer);
        assert_eq!(on_esc(false, false, false), Esc::Quit);
        assert_eq!(on_esc(false, false, true), Esc::Nothing);
    }
}
