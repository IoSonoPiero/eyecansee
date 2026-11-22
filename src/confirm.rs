//! Confirmation windows: a title, a text and a row of buttons, the last one cancelling. Enter and Esc
//! reach them as `App::key_answer` (`raw_input_hook`), taken by the window on top (`App::answer`)
//! and passed in as `answer`.

use eframe::egui;

use crate::App;

/// A modal with `text`: its title, centred, then its text (`**bold**` and paragraphs, as in message windows),
/// one button per choice (greyed out when its flag is false) and `cancel`.
/// `Some(Some(choice))` for a choice, `Some(None)` for cancel; otherwise `answer` (from the keys),
/// `None` while the window stays open.
pub(crate) fn choose<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: (String, String),
    choices: Vec<(String, bool, T)>,
    cancel: String,
    answer: Option<Option<T>>,
) -> Option<Option<T>> {
    choose_with(ctx, id, width, text, choices, cancel, answer, (false, &mut |_| {}))
}

/// `choose`, with `cancel` focused at first when `cancel_first` (it shows as the default), and
/// what `extra` draws between the text and the buttons.
#[allow(clippy::too_many_arguments)]
fn choose_with<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    (title, text): (String, String),
    choices: Vec<(String, bool, T)>,
    cancel: String,
    mut answer: Option<Option<T>>,
    (cancel_first, extra): (bool, &mut dyn FnMut(&mut egui::Ui)),
) -> Option<Option<T>> {
    // As every message window (`save_dialogs::message`), at its own width.
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(width);
        ui.set_min_width(320.0_f32.min(width));
        crate::save_dialogs::Head::new(&title).show(ui);
        crate::save_dialogs::notice_text(ui, &text);
        ui.add_space(8.0);
        extra(ui);
        ui.horizontal(|ui| {
            for (label, enabled, choice) in choices {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    answer = Some(Some(choice));
                }
            }
            let cancel = ui.button(cancel);
            if cancel_first && ui.memory(|m| m.focused().is_none()) {
                cancel.request_focus();
            }
            if cancel.clicked() {
                answer = Some(None);
            }
        });
    });
    answer
}

/// `yes_no` with what `extra` draws between the text and the buttons (a choice for the yes).
pub(crate) fn yes_no_with(ctx: &egui::Context, id: &str, width: f32, text: (String, String), (yes, no): (String, String), answer: Option<bool>, extra: &mut dyn FnMut(&mut egui::Ui)) -> Option<bool> {
    choose_with(ctx, id, width, text, vec![(yes, true, ())], no, answer.map(|yes| yes.then_some(())), (false, extra)).map(|a| a.is_some())
}

/// `choose` with one choice: `Some(true)` yes, `Some(false)` no, `None` still open.
#[allow(clippy::too_many_arguments)]
pub(crate) fn yes_no(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: (String, String),
    yes: String,
    yes_enabled: bool,
    no: String,
    answer: Option<bool>,
) -> Option<bool> {
    choose(ctx, id, width, text, vec![(yes, yes_enabled, ())], no, answer.map(|yes| yes.then_some(()))).map(|a| a.is_some())
}

/// `yes_no` with `no` as the default: focused at first, so it shows (deleting folders).
#[allow(clippy::too_many_arguments)]
pub(crate) fn yes_no_cancel_first(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: (String, String),
    yes: String,
    yes_enabled: bool,
    no: String,
    answer: Option<bool>,
) -> Option<bool> {
    choose_with(ctx, id, width, text, vec![(yes, yes_enabled, ())], no, answer.map(|yes| yes.then_some(())), (true, &mut |_| {})).map(|a| a.is_some())
}

/// The layer of the modal window `id` (`egui::Modal::new(egui::Id::unique(id))`).
fn modal_layer(id: &str) -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(id))
}

/// `id` may take Enter or Esc, with `top` the modal window on top when the key came: only that
/// one, or any window when no modal was on top (About is not a modal).
fn answers(top: Option<egui::LayerId>, id: &str) -> bool {
    top.is_none_or(|top| top == modal_layer(id))
}

/// The modal window `id` is on top, or no modal is (`answers`).
pub(crate) fn on_top(ctx: &egui::Context, id: &str) -> bool {
    answers(ctx.memory(|m| m.top_modal_layer()), id)
}

/// Enter (`Some(true)`) or Esc (`Some(false)`) pressed while a window asks (`App::key_answer`,
/// from `raw_input_hook`), for the window whose modal is `id`: only the window on top takes it,
/// the others under it wait. One answer for all of them went to the first one drawn: Enter on a
/// message confirmed the deletion under it (audit 1.6, #196). Not taken by the end of the frame,
/// it is forgotten (`App::windows`), so it never answers a window that comes on top later.
pub(crate) fn answer(ctx: &egui::Context, key: &mut Option<bool>, id: &str) -> Option<bool> {
    let top = ctx.memory(|m| m.top_modal_layer());
    if !answers(top, id) {
        return None;
    }
    key.take()
}

impl App {
    /// Confirmation open: the keys answer it and reach nothing else (Esc would close the viewer);
    /// F1 still opens the guide, and what is typed in its search field stays (`help::stays`).
    /// Open location, Convert and resize and the editor's Resize have fields to type in: there
    /// the keys go to the field. `true`: the keys were the confirmation's (`raw_input_hook`).
    pub(crate) fn asking_keys(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) -> bool {
        let fields = self.open_address.is_some() || self.server_manager.is_some() || self.bookmarks.configuring || self.unlock.is_some() || self.batch.is_some() || self.editor.as_ref().is_some_and(crate::edit_page::Window::resizing);
        if !self.asking() || (fields && ctx.egui_wants_keyboard_input()) {
            return false;
        }
        let (answer, typing) = (&mut self.key_answer, self.help.in_field());
        // The tip of the day (audit 1.13, D6): ← and → turn it, unless the guide is over it. A key
        // held down from before it came (Enter on Done of the first-start window) does not close it.
        let tip = self.tip.is_some() && !self.help.open;
        let mut turn = None;
        raw_input.events.retain(|e| match e {
            e if crate::help::stays(typing, e) => true,
            egui::Event::Key { key, pressed: true, repeat, .. } => {
                match key {
                    egui::Key::Enter | egui::Key::Escape if *repeat && tip => {}
                    egui::Key::Enter => *answer = Some(true),
                    egui::Key::Escape => *answer = Some(false),
                    egui::Key::ArrowLeft | egui::Key::ArrowRight if tip => turn = Some(*key == egui::Key::ArrowRight),
                    _ => {}
                }
                false
            }
            egui::Event::Key { .. } | egui::Event::Text(_) => false,
            _ => true,
        });
        if let (Some(next), Some(open)) = (turn, &mut self.tip) {
            open.turn(next);
        }
        true
    }

    /// A confirmation or a message is open: deletion, a name already taken when restoring or
    /// pasting, a message to close, the first-start window's question on closing, the tip of the
    /// day… Its keys are its own (`raw_input_hook`).
    pub(crate) fn asking(&self) -> bool {
        let pasting = self.paste_ask.as_ref().is_some_and(|a| a.conflicts.as_ref().is_some_and(|c| !c.is_empty()));
        self.delete_confirm.is_some() || self.restore_conflict.is_some() || pasting || self.save_ready.is_some() || self.ffmpeg_ask.is_some() || self.batch_turn.is_some() || self.notice.is_some() || !self.kept.is_empty() || self.batch_summary.is_some() || self.update_note.is_some() || self.about || self.kiod_ask || self.host_ask || self.open_address.is_some() || self.server_manager.is_some() || self.bookmarks.configuring || self.unlock.is_some() || self.duplicates.as_ref().is_some_and(crate::dup_page::Window::asking) || self.editor.as_ref().is_some_and(crate::edit_page::Window::asking) || self.after_turn.is_some() || self.batch.as_ref().is_some_and(crate::batch_page::Window::asking) || self.convert.as_ref().is_some_and(crate::convert::Window::asking) || self.first_start.as_ref().is_some_and(crate::first_start_ui::Open::asking) || self.tip.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enter and Esc go to the modal on top only, and to any window when none is on top (#196).
    #[test]
    fn answer_only_for_the_window_on_top() {
        assert!(answers(None, "delete"));
        assert!(answers(Some(modal_layer("delete")), "delete"));
        assert!(!answers(Some(modal_layer("notice")), "delete"));
        assert!(!answers(Some(modal_layer("notice")), "about"));
    }
}
