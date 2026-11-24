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

/// Where the boxes stand, oldest first, with their height and the frame they were drawn in.
fn stack_key() -> egui::Id {
    egui::Id::unique("box-stack")
}

/// The offset from the bottom right corner for the box `name`: above the boxes that came before
/// it, so two at once are both seen (#324; they were drawn at the same place).
pub(crate) fn offset(ctx: &egui::Context, name: &'static str) -> egui::Vec2 {
    let now = ctx.cumulative_frame_nr();
    let stack = ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<Vec<(&'static str, f32, u64)>>(stack_key());
        all.retain(|(_, _, frame)| *frame + 2 >= now);
        if !all.iter().any(|(n, _, _)| *n == name) {
            all.push((name, 0.0, now));
        }
        all.clone()
    });
    let below: f32 = stack.iter().take_while(|(n, _, _)| *n != name).map(|(_, h, _)| h + 8.0).sum();
    egui::vec2(-16.0, -40.0 - below)
}

/// The box `name` was drawn this frame, `height` tall.
pub(crate) fn drawn(ctx: &egui::Context, name: &'static str, height: f32) {
    let now = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        if let Some(b) = d.get_temp_mut_or_default::<Vec<(&'static str, f32, u64)>>(stack_key()).iter_mut().find(|(n, _, _)| *n == name) {
            (b.1, b.2) = (height, now);
        }
    });
}

/// F6, from anywhere: the keyboard to Cancel of a job under way, the next one at every press
/// (#324). With none, F6 does nothing. The key is taken out of the input.
pub(crate) fn f6(ctx: &egui::Context, raw_input: &mut egui::RawInput) {
    // Only the press counts: the release moved it on again (Piero, 24/11).
    let pressed = raw_input.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::F6, pressed: true, .. }));
    raw_input.events.retain(|e| !matches!(e, egui::Event::Key { key: egui::Key::F6, .. }));
    let boxes = controls(ctx);
    if !pressed || boxes.is_empty() {
        return;
    }
    let at = ctx.memory(|m| m.focused()).and_then(|f| boxes.iter().position(|b| *b == f));
    focus(ctx, boxes[at.map_or(0, |i| (i + 1) % boxes.len())]);
}

impl App {
    /// Before the frame, on the grid: with the keyboard on a control of a box, Tab goes to the next one, or
    /// from the last one to the first item of the grid; Shift+Tab to the one before, or from the
    /// first one to the tree; the arrows do nothing. `true` when the keys were the boxes'.
    pub(crate) fn box_keys(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) -> bool {
        // Over a page (the viewer, the editor, Save as, the duplicates, the slideshow) Tab goes
        // through the box as egui gives it: the round of the grid moved what the page hides.
        let page = self.viewing.is_some() || self.convert.is_some() || self.editor.is_some() || self.show.is_some() || self.dup_page_shown();
        if page {
            return false;
        }
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
