//! The editor's keys: undo and redo, Del, the tools' letters, the quarter turns, zoom and the saves.

use eframe::egui;

use super::{Ask, Tool, Window, tools};
use crate::App;

impl App {
    /// Without a field taking the keys (text has its own undo there): undo and redo, Del, the
    /// tools' letters, the quarter turns, zoom, Ctrl+S (Save copy) and Ctrl+Shift+S (Save as…).
    pub(super) fn edit_keys(&self, ctx: &egui::Context, w: &mut Window) -> Option<Ask> {
        if ctx.egui_wants_keyboard_input() || w.hold.is_some() || w.drag.is_some() {
            return None;
        }
        use egui::{Key, Modifiers as M};
        let cmd_shift = M::COMMAND | M::SHIFT;
        let (redo, undo, delete) = ctx.input_mut(|i| {
            let redo = i.consume_key(cmd_shift, Key::Z) || i.consume_key(M::COMMAND, Key::Y);
            (redo, i.consume_key(M::COMMAND, Key::Z), i.consume_key(M::NONE, Key::Delete))
        });
        if redo {
            w.redo();
        } else if undo {
            w.undo();
        }
        if delete && let Some(i) = w.selected.take() {
            w.doc.objects.remove(i);
        }
        if w.tool == Tool::Crop && ctx.input_mut(|i| i.consume_key(M::NONE, Key::Enter)) {
            w.apply_crop();
        }
        if w.tool != Tool::Crop {
            let (screen, here) = ctx.input_mut(|i| (i.consume_key(M::SHIFT, Key::I), i.consume_key(M::NONE, Key::I)));
            if screen || here {
                w.ask_pick = Some(screen);
            }
        }
        let tool = ctx.input_mut(|i| tools::TOOLS.iter().find(|t| i.consume_key(M::NONE, t.3)).map(|t| t.0));
        if let Some(tool) = tool {
            w.choose(tool);
        }
        let (ccw, cw) = ctx.input_mut(|i| (i.consume_key(cmd_shift, Key::R), i.consume_key(M::COMMAND, Key::R)));
        if ccw {
            w.turn(crate::transform::Op::Ccw);
        } else if cw {
            w.turn(crate::transform::Op::Cw);
        }
        let (zoom_in, zoom_out, fit) = ctx.input(|i| (i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals), i.key_pressed(Key::Minus), i.key_pressed(Key::Num0)));
        if zoom_in {
            w.zoom(1.25);
        } else if zoom_out {
            w.zoom(0.8);
        } else if fit {
            w.view = None;
        }
        let (save_as, save) = ctx.input_mut(|i| (i.consume_key(cmd_shift, Key::S), i.consume_key(M::COMMAND, Key::S)));
        match () {
            _ if (save_as || save && w.new) && !w.preparing => Some(Ask::SaveAs),
            _ if save && !w.preparing && w.changed() => Some(Ask::Save),
            _ => None,
        }
    }
}
