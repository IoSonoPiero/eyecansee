//! The boxes of a work under way in a corner (a copy, Convert): the program is used meanwhile, so
//! they take no keys of their own; their controls (Cancel) come in the round of Tab of the main
//! window, after the folders of the tree and before the grid (#311, Piero 24/11). Enter and Space
//! press the one highlighted, as egui does.

use eframe::egui;

use crate::App;

/// Where the controls of the boxes drawn wait, with the frame they were drawn in.
fn key() -> egui::Id {
    egui::Id::unique("box-keys")
}

/// A control of a box under way (`r`), drawn this frame: Tab reaches it.
pub(crate) fn mark(r: &egui::Response) {
    let now = r.ctx.cumulative_frame_nr();
    r.ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<Vec<(egui::Id, u64)>>(key());
        all.retain(|(id, frame)| *frame + 1 >= now && *id != r.id);
        all.push((r.id, now));
    });
}

/// The controls of the boxes on screen, in the order they were drawn (the frame before at most).
pub(crate) fn controls(ctx: &egui::Context) -> Vec<egui::Id> {
    let now = ctx.cumulative_frame_nr();
    let all = ctx.data(|d| d.get_temp::<Vec<(egui::Id, u64)>>(key())).unwrap_or_default();
    all.into_iter().filter(|(id, frame)| *frame + 2 >= now && ctx.read_response(*id).is_some_and(|r| r.enabled())).map(|(id, _)| id).collect()
}

/// The keyboard goes to `id`, a control of a box.
pub(crate) fn focus(ctx: &egui::Context, id: egui::Id) {
    ctx.memory_mut(|m| m.request_focus(id));
}

impl App {
    /// Before the frame: with the keyboard on a control of a box, Tab goes to the next one, or
    /// from the last one to the first item of the grid; Shift+Tab to the one before, or from the
    /// first one to the tree; the arrows do nothing. `true` when the keys were the boxes'.
    pub(crate) fn box_keys(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) -> bool {
        let boxes = controls(ctx);
        let Some(at) = ctx.memory(|m| m.focused()).and_then(|f| boxes.iter().position(|b| *b == f)) else { return false };
        let mut tab = None;
        raw_input.events.retain(|e| match e {
            egui::Event::Key { key: egui::Key::Tab, pressed, modifiers, .. } => {
                if *pressed {
                    tab = Some(modifiers.shift);
                }
                false
            }
            egui::Event::Key { key: egui::Key::ArrowDown | egui::Key::ArrowUp | egui::Key::ArrowLeft | egui::Key::ArrowRight, .. } => false,
            _ => true,
        });
        match tab {
            Some(false) if at + 1 < boxes.len() => focus(ctx, boxes[at + 1]),
            Some(true) if at > 0 => focus(ctx, boxes[at - 1]),
            Some(back) => {
                ctx.memory_mut(|m| m.surrender_focus(boxes[at]));
                self.tab_out_of_boxes(back);
            }
            None => {}
        }
        true
    }
}
