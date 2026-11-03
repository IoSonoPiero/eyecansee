//! Confirmation windows: a text and a row of buttons, the last one cancelling. Enter and Esc
//! reach them as `App::delete_answer` (`raw_input_hook`), passed in as `answer`.

use eframe::egui;

use crate::App;

/// A modal with `text` (`**bold**` and paragraphs, as in message windows), one button per choice (greyed out when its flag is false) and `cancel`.
/// `Some(Some(choice))` for a choice, `Some(None)` for cancel; otherwise `answer` (from the keys),
/// `None` while the window stays open.
pub(crate) fn choose<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: String,
    choices: Vec<(String, bool, T)>,
    cancel: String,
    answer: Option<Option<T>>,
) -> Option<Option<T>> {
    choose_with(ctx, id, width, text, choices, cancel, answer, false)
}

/// `choose`, with `cancel` focused at first when `cancel_first` (it shows as the default).
#[allow(clippy::too_many_arguments)]
fn choose_with<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: String,
    choices: Vec<(String, bool, T)>,
    cancel: String,
    mut answer: Option<Option<T>>,
    cancel_first: bool,
) -> Option<Option<T>> {
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(width);
        crate::save_dialogs::notice_text(ui, &text);
        ui.add_space(8.0);
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

/// `choose` with one choice: `Some(true)` yes, `Some(false)` no, `None` still open.
#[allow(clippy::too_many_arguments)]
pub(crate) fn yes_no(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: String,
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
    text: String,
    yes: String,
    yes_enabled: bool,
    no: String,
    answer: Option<bool>,
) -> Option<bool> {
    choose_with(ctx, id, width, text, vec![(yes, yes_enabled, ())], no, answer.map(|yes| yes.then_some(())), true).map(|a| a.is_some())
}

impl App {
    /// A confirmation is open: deletion, or a name already taken when restoring or pasting.
    pub(crate) fn asking(&self) -> bool {
        let pasting = self.paste_ask.as_ref().is_some_and(|a| a.conflicts.as_ref().is_some_and(|c| !c.is_empty()));
        self.delete_confirm.is_some() || self.restore_conflict.is_some() || pasting || self.save_ready.is_some() || self.batch_turn.is_some() || self.notice.is_some() || self.kiod_ask || self.host_ask || self.open_address.is_some() || self.server_manager.is_some() || self.unlock.is_some() || self.duplicates.as_ref().is_some_and(crate::dup_page::Window::asking) || self.rotate.as_ref().is_some_and(|w| w.ready.is_some() || w.leaving) || self.after_turn.is_some() || self.batch.as_ref().is_some_and(crate::batch_page::Window::asking)
    }
}
