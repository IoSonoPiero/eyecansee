//! Confirmation windows: a text and a row of buttons, the last one cancelling. Enter and Esc
//! reach them as `App::delete_answer` (`raw_input_hook`), passed in as `answer`.

use eframe::egui;

/// A modal with `text`, one button per choice (greyed out when its flag is false) and `cancel`.
/// `Some(Some(choice))` for a choice, `Some(None)` for cancel; otherwise `answer` (from the keys),
/// `None` while the window stays open.
pub(crate) fn choose<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: String,
    choices: Vec<(String, bool, T)>,
    cancel: String,
    mut answer: Option<Option<T>>,
) -> Option<Option<T>> {
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(width);
        ui.label(text);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (label, enabled, choice) in choices {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    answer = Some(Some(choice));
                }
            }
            if ui.button(cancel).clicked() {
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
