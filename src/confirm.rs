//! Confirmation windows: a title, a text and a row of buttons, the last one cancelling. Esc
//! reaches them as `App::key_answer` (`raw_input_hook`), taken by the window on top (`App::answer`)
//! and passed in as `answer`. Tab, Shift+Tab and the arrows move between the buttons, Enter and
//! Space press the one highlighted (#311): they reach the row of buttons (`buttons`) as a `Nav`.
//! Each window opens on its default button: the main choice, or the safe one when the main choice
//! cannot be undone (decided with Piero on 24/11: `bozze/issue-311-pulsanti-fable.md`).

use eframe::egui;

use crate::App;

/// A modal with `text`: its title, centred, then its text (`**bold**` and paragraphs, as in message windows),
/// one button per choice (greyed out when its flag is false) and `cancel`, highlighted at first on
/// `default` (an index of `choices`; `choices.len()` is `cancel`).
/// `Some(Some(choice))` for a choice, `Some(None)` for cancel; otherwise `answer` (Esc),
/// `None` while the window stays open.
#[allow(clippy::too_many_arguments)]
pub(crate) fn choose<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: (String, String),
    choices: Vec<(String, bool, T)>,
    cancel: String,
    answer: Option<Option<T>>,
    default: usize,
) -> Option<Option<T>> {
    choose_with(ctx, id, width, text, choices, cancel, answer, (default, &mut |_| {}))
}

/// `choose`, with what `extra` draws between the text and the buttons.
#[allow(clippy::too_many_arguments)]
fn choose_with<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    (title, text): (String, String),
    choices: Vec<(String, bool, T)>,
    cancel: String,
    mut answer: Option<Option<T>>,
    (default, extra): (usize, &mut dyn FnMut(&mut egui::Ui)),
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
            let mut labels: Vec<(String, bool)> = choices.iter().map(|(l, on, _)| (l.clone(), *on)).collect();
            labels.push((cancel, true));
            if let Some(i) = buttons(ui, id, &labels, default) {
                answer = Some(choices.into_iter().nth(i).map(|(_, _, c)| c));
            }
        });
    });
    answer
}

/// `yes_no` with what `extra` draws between the text and the buttons (a choice for the yes).
pub(crate) fn yes_no_with(ctx: &egui::Context, id: &str, width: f32, text: (String, String), (yes, no): (String, String), answer: Option<bool>, extra: &mut dyn FnMut(&mut egui::Ui)) -> Option<bool> {
    choose_with(ctx, id, width, text, vec![(yes, true, ())], no, answer.map(|yes| yes.then_some(())), (0, extra)).map(|a| a.is_some())
}

/// `choose` with one choice: `Some(true)` yes, `Some(false)` no, `None` still open. `safe`: the
/// window opens on `no`, for a yes that cannot be undone (deleting for good, folders…).
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
    safe: bool,
) -> Option<bool> {
    choose(ctx, id, width, text, vec![(yes, yes_enabled, ())], no, answer.map(|yes| yes.then_some(())), usize::from(safe)).map(|a| a.is_some())
}

/// A key that moves between the buttons of the window on top or presses one (#311).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nav {
    Next,
    Prev,
    Press,
}

/// Where the `Nav` of this frame waits for the row of buttons of the window on top.
fn nav_id() -> egui::Id {
    egui::Id::unique("confirm-nav")
}

/// The keys of the buttons are not for the windows of this frame (the guide over them, #196).
pub(crate) fn drop_nav(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Nav>(nav_id()));
}

/// The button highlighted after `nav`, and the one pressed: `on`, which buttons can be used;
/// `focused`, the one highlighted now (none: the window just opened); `default`, the one it opens
/// on, or the first usable after it (the last one, cancel, at worst). `None` with no usable button.
pub(crate) fn step(on: &[bool], default: usize, focused: Option<usize>, nav: Option<Nav>) -> Option<(usize, Option<usize>)> {
    let usable: Vec<usize> = (0..on.len()).filter(|&i| on[i]).collect();
    let start = usable.iter().copied().find(|&i| i >= default).or_else(|| usable.last().copied())?;
    let now = focused.filter(|f| on.get(*f) == Some(&true)).unwrap_or(start);
    let k = usable.iter().position(|&i| i == now).unwrap_or(0);
    Some(match nav {
        Some(Nav::Next) => (usable[(k + 1) % usable.len()], None),
        Some(Nav::Prev) => (usable[(k + usable.len() - 1) % usable.len()], None),
        Some(Nav::Press) => (now, Some(now)),
        None => (now, None),
    })
}

/// A row of buttons of the window `id` (in the `ui` the caller lays out): `labels` with whether
/// each can be used; the one highlighted (focused) is `default` when the window opens. The
/// window on top takes the keys of the frame (`Nav`); the index of the button pressed, by the
/// mouse or the keys.
pub(crate) fn buttons(ui: &mut egui::Ui, id: &str, labels: &[(String, bool)], default: usize) -> Option<usize> {
    buttons_hint(ui, id, labels, default, &[])
}

/// `buttons`, with `hints[i]` shown over button `i` when it is greyed out (why it cannot be used).
pub(crate) fn buttons_hint(ui: &mut egui::Ui, id: &str, labels: &[(String, bool)], default: usize, hints: &[Option<String>]) -> Option<usize> {
    let rows: Vec<egui::Response> = labels
        .iter()
        .enumerate()
        .map(|(i, (l, on))| {
            let r = ui.add_enabled(*on, egui::Button::new(l.as_str()));
            match hints.get(i) {
                Some(Some(h)) => r.on_disabled_hover_text(h),
                _ => r,
            }
        })
        .collect();
    keys(ui, id, &rows, default)
}

/// The keys of the frame for a row of buttons the caller drew itself (`rows`, greyed ones too, in
/// any order: Tab goes from the left, as they are on screen, also when drawn from the right). The
/// window on top takes them (`Nav`); `default`, an index of `rows`, is highlighted when the window
/// opens, unless a text field has the keyboard. The index pressed, by the mouse or the keys.
pub(crate) fn keys(ui: &egui::Ui, id: &str, rows: &[egui::Response], default: usize) -> Option<usize> {
    let clicked = rows.iter().position(egui::Response::clicked);
    let opened = just_opened(ui.ctx(), id);
    if !on_top(ui.ctx(), id) {
        return clicked;
    }
    let nav = ui.ctx().data_mut(|d| {
        let n = d.get_temp::<Nav>(nav_id());
        d.remove::<Nav>(nav_id());
        n
    });
    let order = on_screen(&rows.iter().map(|r| r.rect.left()).collect::<Vec<_>>());
    let place = |i: usize| order.iter().position(|&o| o == i);
    let on: Vec<bool> = order.iter().map(|&i| rows[i].enabled()).collect();
    let focused = rows.iter().position(egui::Response::has_focus);
    let (to, pressed) = step(&on, place(default)?, focused.and_then(place), nav)?;
    let (to, pressed) = (order[to], pressed.map(|p| order[p]));
    // Another widget of the window (a check box) may hold the focus: left to it, unless a key moved
    // or the window just opened (the button that opened a question inside a form had it, #311).
    let take = nav.is_some() || ui.memory(|m| m.focused().is_none()) || (opened && !ui.ctx().text_edit_focused());
    if focused != Some(to) && take {
        rows[to].request_focus();
    }
    clicked.or(pressed)
}

/// The order of buttons on screen, from the left, by the left edge of each (`lefts`, as drawn).
fn on_screen(lefts: &[f32]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..lefts.len()).collect();
    order.sort_by(|&a, &b| lefts[a].total_cmp(&lefts[b]));
    order
}

/// The row of buttons of `id` was not drawn in the frame before: its window has just opened.
fn just_opened(ctx: &egui::Context, id: &str) -> bool {
    let (key, now) = (egui::Id::unique(id).with("confirm-drawn"), ctx.cumulative_frame_nr());
    let before = ctx.data_mut(|d| {
        let b = d.get_temp::<u64>(key);
        d.insert_temp(key, now);
        b
    });
    before.is_none_or(|b| b + 1 < now)
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
        ctx.data_mut(|d| d.remove::<Nav>(nav_id()));
        let fields = self.open_address.is_some() || self.server_manager.is_some() || self.bookmarks.configuring || self.unlock.is_some() || self.batch.is_some() || self.editor.as_ref().is_some_and(crate::edit_page::Window::resizing);
        if !self.asking() || (fields && ctx.egui_wants_keyboard_input()) {
            return false;
        }
        // The tip's first frame: egui keeps what is under a modal from the mouse only from the frame
        // after it is drawn, so a click now would reach the grid, the tree or the toolbar behind it.
        if self.tip.is_some() && ctx.memory(|m| m.top_modal_layer()).is_none() {
            raw_input.events.retain(|e| !matches!(e, egui::Event::PointerButton { .. }));
        }
        let (answer, typing) = (&mut self.key_answer, self.help.in_field());
        // The tip of the day (audit 1.13, D6): ← and → turn it, unless the guide is over it. A key
        // held down from before it came (Enter on Done of the first-start window) does not close it.
        let tip = self.tip.is_some() && !self.help.open;
        let mut turn = None;
        let mut nav = None;
        raw_input.events.retain(|e| match e {
            e if crate::help::stays(typing, e) => true,
            egui::Event::Key { key, pressed: true, repeat, modifiers, .. } => {
                use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Enter, Escape, Space, Tab};
                match key {
                    Enter | Escape | Space if *repeat => {}
                    // Enter still says "yes" to the windows that read `answer` only; the rows of
                    // buttons (`buttons`) take Press instead, the highlighted button.
                    Enter => {
                        *answer = Some(true);
                        nav = Some(Nav::Press);
                    }
                    Space => nav = Some(Nav::Press),
                    Escape => *answer = Some(false),
                    ArrowLeft | ArrowRight if tip => turn = Some(*key == ArrowRight),
                    Tab => nav = Some(if modifiers.shift { Nav::Prev } else { Nav::Next }),
                    ArrowRight | ArrowDown => nav = Some(Nav::Next),
                    ArrowLeft | ArrowUp => nav = Some(Nav::Prev),
                    _ => {}
                }
                false
            }
            egui::Event::Key { .. } | egui::Event::Text(_) => false,
            _ => true,
        });
        if let Some(n) = nav {
            ctx.data_mut(|d| d.insert_temp(nav_id(), n));
        }
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
        self.delete_confirm.is_some() || self.restore_conflict.is_some() || pasting || self.save_ready.is_some() || self.ffmpeg_ask.is_some() || self.batch_turn.is_some() || self.notice.is_some() || !self.kept.is_empty() || self.batch_summary.is_some() || self.update_note.is_some() || self.about || self.kiod_ask || self.host_ask || self.open_address.is_some() || self.server_manager.is_some() || self.bookmarks.configuring || self.unlock.is_some() || self.duplicates.as_ref().is_some_and(crate::dup_page::Window::asking) || self.editor.as_ref().is_some_and(crate::edit_page::Window::asking) || self.after_turn.is_some() || self.batch.as_ref().is_some_and(crate::batch_page::Window::asking) || self.convert.as_ref().is_some_and(crate::convert::Window::asking) || self.first_start.as_ref().is_some_and(crate::first_start_ui::Open::asking) || self.tip.is_some() || self.model_export.is_some()
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

    /// The default button at first, the first usable after it when it is greyed out; Tab and the
    /// arrows go round the usable ones; Enter and Space press the one highlighted (#311).
    #[test]
    fn keys_move_between_the_buttons() {
        let all = [true, true, true];
        assert_eq!(step(&all, 0, None, None), Some((0, None)));
        assert_eq!(step(&all, 2, None, None), Some((2, None)));
        assert_eq!(step(&all, 2, None, Some(Nav::Press)), Some((2, Some(2))));
        assert_eq!(step(&all, 0, Some(1), Some(Nav::Next)), Some((2, None)));
        assert_eq!(step(&all, 0, Some(2), Some(Nav::Next)), Some((0, None)));
        assert_eq!(step(&all, 0, Some(0), Some(Nav::Prev)), Some((2, None)));
        // Delete while the folders are read: the default greyed out, Cancel instead.
        let reading = [false, true];
        assert_eq!(step(&reading, 0, None, Some(Nav::Press)), Some((1, Some(1))));
        assert_eq!(step(&reading, 0, Some(1), Some(Nav::Next)), Some((1, None)));
        // Save, Save as (off), Don't save, Cancel: Tab skips the greyed one.
        let off = [true, false, true, true];
        assert_eq!(step(&off, 0, Some(0), Some(Nav::Next)), Some((2, None)));
        assert_eq!(step(&[false, false], 0, None, None), None);
        // Drawn from the right (Close, Next, Back): Tab still goes from the left.
        assert_eq!(on_screen(&[300.0, 200.0, 100.0]), [2, 1, 0]);
    }
}
