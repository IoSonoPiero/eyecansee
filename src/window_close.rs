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

impl App {
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
}
