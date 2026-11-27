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
    choose_with(ctx, id, width, text, (choices, &[]), cancel, answer, (default, &mut |_| {}))
}

/// The choices of a question (label, usable, answer), and over a greyed one why it is greyed.
type Choices<'a, T> = (Vec<(String, bool, T)>, &'a [Option<String>]);

/// `choose` with, over a choice greyed out, why it cannot be used (`hints`, by choice).
#[allow(clippy::too_many_arguments)]
pub(crate) fn choose_hint<T>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    text: (String, String),
    choices: Choices<T>,
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
    (choices, hints): Choices<T>,
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
            if let Some(i) = buttons_hint(ui, id, &labels, default, hints) {
                answer = Some(choices.into_iter().nth(i).map(|(_, _, c)| c));
            }
        });
    });
    answer
}

/// `yes_no` with what `extra` draws between the text and the buttons (a choice for the yes).
pub(crate) fn yes_no_with(ctx: &egui::Context, id: &str, width: f32, text: (String, String), (yes, no): (String, String), answer: Option<bool>, extra: &mut dyn FnMut(&mut egui::Ui)) -> Option<bool> {
    choose_with(ctx, id, width, text, (vec![(yes, true, ())], &[]), no, answer.map(|yes| yes.then_some(())), (0, extra)).map(|a| a.is_some())
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
/// opens, unless a text field has the keyboard (beyond the rows: none is). The index pressed, by the mouse or the keys.
pub(crate) fn keys(ui: &egui::Ui, id: &str, rows: &[egui::Response], default: usize) -> Option<usize> {
    let clicked = rows.iter().position(egui::Response::clicked);
    let opened = just_opened(ui.ctx(), id);
    if !on_top(ui.ctx(), id) {
        return clicked;
    }
    // The controls joined to the row (`join`), drawn before it or after it in the frame before:
    // the keys go through them too; what they do when pressed is theirs.
    let n = rows.len();
    let rows: Vec<egui::Response> = rows.iter().cloned().chain(joined(ui.ctx(), id).into_iter().filter_map(|j| ui.ctx().read_response(j))).collect();
    let nav = ui.ctx().data_mut(|d| {
        let n = d.get_temp::<Nav>(nav_id());
        d.remove::<Nav>(nav_id());
        n
    });
    let order = on_screen(&rows.iter().map(|r| r.rect).collect::<Vec<_>>());
    let place = |i: usize| order.iter().position(|&o| o == i);
    let on: Vec<bool> = order.iter().map(|&i| rows[i].enabled()).collect();
    let focused = rows.iter().position(egui::Response::has_focus);
    let Some(start) = place(default).or(focused.and_then(place)) else {
        // No button highlighted at first (`default` beyond the rows: About): Tab goes to the
        // first control, Shift+Tab to the last one.
        let mut usable = (0..on.len()).filter(|&i| on[i]);
        let to = match nav {
            Some(Nav::Next) => usable.next(),
            Some(Nav::Prev) => usable.next_back(),
            _ => None,
        };
        if let Some(to) = to {
            rows[order[to]].request_focus();
        }
        return clicked;
    };
    let (to, pressed) = step(&on, start, focused.and_then(place), nav)?;
    let (to, pressed) = (order[to], pressed.map(|p| order[p]));
    // Another widget of the window (a check box) may hold the focus: left to it, unless a key moved
    // or the window just opened (the button that opened a question inside a form had it, #311).
    let take = nav.is_some() || ui.memory(|m| m.focused().is_none()) || (opened && !ui.ctx().text_edit_focused());
    if focused != Some(to) && take {
        rows[to].request_focus();
    }
    clicked.or(pressed.filter(|&p| p < n))
}

/// The controls joined to rows of buttons (`join`): for each, its window and the frame it was
/// drawn in.
fn joined_id() -> egui::Id {
    egui::Id::unique("confirm-joined")
}

/// A control of the window `id` that is not one of its buttons (a check box, a list, a link, the
/// small Copy): Tab, Shift+Tab and the arrows go through it with the row of buttons (`keys`), in
/// the order they are on screen; Enter and Space on it are its own, as a click (`chosen`), and
/// never answer the window (#311). Called every frame it is drawn, before or after the row.
pub(crate) fn join(id: &str, r: &egui::Response) {
    let (window, now) = (egui::Id::unique(id), r.ctx.cumulative_frame_nr());
    r.ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<std::collections::HashMap<egui::Id, (egui::Id, u64)>>(joined_id());
        all.retain(|_, (_, frame)| *frame + 1 >= now);
        all.insert(r.id, (window, now));
    });
    // A list closed (Esc, or a choice made in it) with the keyboard in it: the keyboard comes back
    // to the list, not lost (Space then pressed the highlighted button of the window).
    let (open, was) = (egui::Popup::is_id_open(&r.ctx, r.id.with("popup")), r.id.with("list-open"));
    if open {
        r.ctx.data_mut(|d| d.insert_temp(was, ()));
    } else if r.ctx.data_mut(|d| d.remove_temp::<()>(was)).is_some() && r.ctx.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
}

/// The controls joined to the window `id` in this frame or the one before (`join`).
fn joined(ctx: &egui::Context, id: &str) -> Vec<egui::Id> {
    let (window, now) = (egui::Id::unique(id), ctx.cumulative_frame_nr());
    let all = ctx.data(|d| d.get_temp::<std::collections::HashMap<egui::Id, (egui::Id, u64)>>(joined_id())).unwrap_or_default();
    let mut ids: Vec<egui::Id> = all.into_iter().filter(|(_, (w, frame))| *w == window && *frame + 1 >= now).map(|(j, _)| j).collect();
    ids.sort_by_key(|j| j.value());
    ids
}

/// The keyboard is on a control joined to a row of buttons (`join`): Enter and Space are its own.
fn chosen(ctx: &egui::Context) -> bool {
    let Some(focused) = ctx.memory(|m| m.focused()) else { return false };
    let now = ctx.cumulative_frame_nr();
    ctx.data(|d| d.get_temp::<std::collections::HashMap<egui::Id, (egui::Id, u64)>>(joined_id())).is_some_and(|all| all.get(&focused).is_some_and(|(_, frame)| *frame + 2 >= now))
}

/// The order of the widgets on screen (`rects`, as drawn): by rows from the top, a new row when one
/// starts under the bottom of the row before; in a row from the left, by the left edge of each.
fn on_screen(rects: &[egui::Rect]) -> Vec<usize> {
    let mut by_top: Vec<usize> = (0..rects.len()).collect();
    by_top.sort_by(|&a, &b| rects[a].top().total_cmp(&rects[b].top()));
    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut bottom = f32::NEG_INFINITY;
    for i in by_top {
        match rows.last_mut() {
            Some(row) if rects[i].top() < bottom => {
                row.push(i);
                bottom = bottom.max(rects[i].bottom());
            }
            _ => {
                rows.push(vec![i]);
                bottom = rects[i].bottom();
            }
        }
    }
    for row in &mut rows {
        row.sort_by(|&a, &b| rects[a].left().total_cmp(&rects[b].left()));
    }
    rows.concat()
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
        // The folder chooser of Save as and Convert, a modal: its folders and buttons take Tab,
        // the arrows and Enter as egui gives them (Piero, 24/11, W3.4).
        let picking = self.convert.as_ref().is_some_and(crate::convert::Window::picking) || (self.batch.is_some() && self.batch_form.as_ref().is_some_and(crate::batch_page::Form::picking));
        // Modals with fields and no row of buttons (`keys`): Tab with nothing focused goes to their
        // controls too, as in Server manager (Piero, 24/11).
        let own_ring = picking || self.open_address.is_some() || self.server_manager.is_some() || self.bookmarks.configuring || self.unlock.is_some() || self.batch.as_ref().is_some_and(crate::batch_page::Window::asking) || self.editor.as_ref().is_some_and(crate::edit_page::Window::resizing);
        let fields = own_ring || self.forms();
        // A question inside a form has the keys of a question, unless a text field has them (#311).
        let fields = fields && !(self.form_asks() && !ctx.text_edit_focused());
        // An open drop-down list (Language, Unit) has its own keys too (`browser::menu_keys`). Under
        // the guide only a text field keeps them: Enter on a highlighted button answered the window.
        let own = ctx.text_edit_focused() || (!self.help.open && (ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx)));
        // A list of a question opened from the keyboard (`join`) takes its keys too: ↓ and Enter
        // chose and pressed a button of the window, Esc cancelled it (#311).
        let list = !self.help.open && egui::Popup::is_any_open(ctx);
        if !self.asking() || (fields && own) || list {
            return false;
        }
        // The tip's first frame: egui keeps what is under a modal from the mouse only from the frame
        // after it is drawn, so a click now would reach the grid, the tree or the toolbar behind it.
        // The phone's copy window too, which comes by itself while the user may be clicking (#354).
        if (self.tip.is_some() || self.phone_copy_modal()) && ctx.memory(|m| m.top_modal_layer()).is_none() {
            raw_input.events.retain(|e| !matches!(e, egui::Event::PointerButton { .. }));
        }
        // Options read no `Nav` (no row of buttons): Tab goes to egui, which gives the keyboard to the
        // window's first control (`app_dialogs::in_ring`); not under the guide, which has it first.
        // Server manager, a modal, the same (Piero, 24/11): egui keeps the keyboard inside it.
        let options = self.options && ctx.memory(|m| m.top_modal_layer()).is_none();
        // The modals with fields too (`own_ring`, above).
        let free_tab = (options || own_ring) && self.notice.is_none() && !self.form_asks() && !self.help.open;
        let (answer, typing) = (&mut self.key_answer, self.help.in_field());
        // The tip of the day (audit 1.13, D6): ← and → turn it, unless the guide is over it. A key
        // held down from before it came (Enter on Done of the first-start window) does not close it.
        // Only with the tip on top: a message over it keeps its arrows for its buttons (#311).
        let tip = self.tip.is_some() && !self.help.open && on_top(ctx, "tip");
        let mut turn = None;
        let mut nav = None;
        let chosen = chosen(ctx);
        raw_input.events.retain(|e| match e {
            e if crate::help::stays(typing, e) => true,
            // The guide over the window has Tab: it goes round inside the guide (`help_window`),
            // also from its search field; taken for the window's buttons it was lost (W1.4).
            egui::Event::Key { key: egui::Key::Tab, .. } if self.help.open => true,
            // A check box, a list or a link of the window (`join`): pressed as with a click.
            egui::Event::Key { key: egui::Key::Enter | egui::Key::Space, .. } if chosen => true,
            egui::Event::Key { key: egui::Key::Tab, pressed: true, .. } if free_tab => true,
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
                    // ← and → turn the tip: ↑ and ↓ move nothing there (#311).
                    ArrowUp | ArrowDown if tip => {}
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
        self.delete_confirm.is_some() || self.restore_conflict.is_some() || pasting || self.save_ready.is_some() || self.ffmpeg_ask.is_some() || self.batch_turn.is_some() || self.notice.is_some() || !self.kept.is_empty() || self.batch_summary.is_some() || self.jobs_report.is_some() || self.update_note.is_some() || self.about || self.kiod_ask || self.host_ask || self.open_address.is_some() || self.server_manager.is_some() || self.bookmarks.configuring || self.unlock.is_some() || self.duplicates.as_ref().is_some_and(crate::dup_page::Window::asking) || self.editor.as_ref().is_some_and(crate::edit_page::Window::asking) || self.after_turn.is_some() || self.batch.as_ref().is_some_and(crate::batch_page::Window::asking) || self.convert.as_ref().is_some_and(crate::convert::Window::asking) || self.first_start.as_ref().is_some_and(crate::first_start_ui::Open::asking) || self.tip.is_some() || self.model_export.is_some() || self.phone_copy_modal() || self.quit_phone.asking.is_some() || self.forms()
    }

    /// Rename, New folder, Options and New image: windows with fields, whose keys reach nothing
    /// under them either (#311).
    fn forms(&self) -> bool {
        self.rename.is_some() || self.new_folder.is_some() || self.options || self.new_image.is_some() || self.convert.as_ref().is_some_and(crate::convert::Window::naming)
    }

    /// A question inside a form, in place of its buttons: deleting a server, removing the master
    /// password, emptying the thumbnail cache (#311).
    fn form_asks(&self) -> bool {
        let purge = self.options && self.options_tab == crate::OptTab::Thumbnails && self.purge_confirm.is_some();
        purge || self.server_manager.as_ref().is_some_and(|f| f.deleting || f.master == crate::vault_page::Change::Remove)
    }
}

/// A combo box whose list follows the keyboard (`follow_focus`): `show_list` in place of egui's
/// `show_ui`, in every combo box of the app.
pub(crate) trait FollowList {
    fn show_list<R>(self, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>>;
}

impl FollowList for egui::ComboBox<'_> {
    fn show_list<R>(self, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
        self.show_ui(ui, |ui| {
            // Just opened: no frame of this list right before this one (`choice`).
            let (seen, now) = (egui::Id::unique(LIST_ID).with(ui.layer_id()), ui.ctx().cumulative_frame_nr());
            let fresh = ui.data(|d| d.get_temp::<u64>(seen)).is_none_or(|f| f.saturating_add(1) < now);
            ui.data_mut(|d| {
                d.insert_temp(seen, now);
                d.insert_temp(egui::Id::unique(LIST_ID), (ui.layer_id(), fresh));
            });
            let r = contents(ui);
            follow_focus(ui);
            r
        })
    }
}

/// A scroll area that follows the keyboard (`follow_focus`): `show_follow` in place of `show`, in
/// the lists and pages of the windows (the grid and the tree move with their own keys).
pub(crate) trait FollowArea {
    fn show_follow<R>(self, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::scroll_area::ScrollAreaOutput<R>;
}

impl FollowArea for egui::ScrollArea {
    fn show_follow<R>(self, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::scroll_area::ScrollAreaOutput<R> {
        self.show(ui, |ui| {
            let r = contents(ui);
            follow_focus(ui);
            r
        })
    }
}

/// The list of `show_list` drawn now: its layer, and whether it has just opened.
const LIST_ID: &str = "list-open";

/// An entry of a drop-down list opened with `show_list`, as `selectable_value` (#311, Piero,
/// 24/11): the list opens with the keyboard on the entry chosen, so the arrows go on from it and
/// not from the first; Enter or Space on an entry chooses it and closes the list, as a click.
pub(crate) fn choice<V: PartialEq>(ui: &mut egui::Ui, current: &mut V, value: V, text: impl Into<egui::WidgetText>) -> egui::Response {
    let selected = *current == value;
    let mut r = ui.selectable_label(selected, text);
    if r.clicked() && !selected {
        *current = value;
        r.mark_changed();
    }
    list_entry(ui, &r, selected);
    r
}

/// What `choice` does for an entry drawn otherwise (`selectable_label` with its own logic).
pub(crate) fn list_entry(ui: &egui::Ui, r: &egui::Response, selected: bool) {
    let Some((layer, fresh)) = ui.data(|d| d.get_temp::<(egui::LayerId, bool)>(egui::Id::unique(LIST_ID))) else { return };
    if layer != ui.layer_id() {
        return;
    }
    if fresh && selected {
        r.request_focus();
    }
    if r.clicked() {
        ui.close();
    }
}

/// A list or a page that scrolls follows the keyboard (#311): when the focus moves (Tab, the
/// arrows) to a widget of `ui` out of sight, `ui` scrolls to show it. egui moves the focus but
/// does not scroll (a long list of a combo box, opened with the keyboard, stayed still below its
/// last visible row: Piero, 24/11, the sizes of New image). Called at the end of the contents of a
/// scroll area: a combo box's list, a page, a list in a window. Only when the focus has just moved,
/// so a scroll made with the mouse stays.
pub(crate) fn follow_focus(ui: &egui::Ui) {
    let ctx = ui.ctx();
    let Some(id) = ctx.memory(|m| m.focused()) else { return };
    let seen = ui.scope_id().with("follow-focus");
    if ctx.data(|d| d.get_temp::<egui::Id>(seen)) == Some(id) {
        return;
    }
    let Some(r) = ctx.read_response(id) else { return };
    if r.layer_id != ui.layer_id() || !ui.min_rect().contains(r.rect.center()) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(seen, id));
    if !ui.clip_rect().contains_rect(r.rect) {
        ui.scroll_to_rect(r.rect, None);
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
        let at = |x: f32, y: f32| egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(80.0, 30.0));
        assert_eq!(on_screen(&[at(300.0, 100.0), at(200.0, 100.0), at(100.0, 100.0)]), [2, 1, 0]);
        // The tip: the link above the buttons, the check box under them, at the left; a smaller
        // widget in the middle of the row stays in it.
        let small = egui::Rect::from_min_size(egui::pos2(150.0, 110.0), egui::vec2(20.0, 10.0));
        let rects = [at(0.0, 50.0), at(300.0, 100.0), at(200.0, 100.0), at(0.0, 140.0), small];
        assert_eq!(on_screen(&rects), [0, 4, 2, 1, 3]);
    }

    /// #311: the arrows move the focus down a long list in a scroll area, past its last visible
    /// row: the list scrolls to show the row focused (before, it stayed still: Piero, 24/11, the
    /// sizes of New image needed the mouse).
    #[test]
    fn a_list_follows_the_keyboard() {
        let ctx = egui::Context::default();
        let mut ids = Vec::new();
        let mut offset = 0.0;
        let draw = |ui: &mut egui::Ui, ids: &mut Vec<egui::Id>, offset: &mut f32| {
            let out = egui::ScrollArea::vertical().max_height(100.0).show(ui, |ui| {
                ids.clear();
                for i in 0..40 {
                    ids.push(ui.selectable_label(false, format!("row {i}")).id);
                }
                follow_focus(ui);
            });
            *offset = out.state.clamped_offset().y;
        };
        let key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        crate::addons_ui::tests::frame(&ctx, Vec::new(), |ui| draw(ui, &mut ids, &mut offset));
        ctx.memory_mut(|m| m.request_focus(ids[0]));
        for _ in 0..20 {
            crate::addons_ui::tests::frame(&ctx, vec![key(egui::Key::ArrowDown)], |ui| draw(ui, &mut ids, &mut offset));
        }
        crate::addons_ui::tests::frame(&ctx, Vec::new(), |ui| draw(ui, &mut ids, &mut offset));
        let focused = ctx.memory(|m| m.focused()).and_then(|f| ids.iter().position(|i| *i == f));
        assert!(focused.is_some_and(|i| i >= 10), "the arrows moved the focus down: {focused:?}");
        assert!(offset > 100.0, "the list scrolled to the row focused: offset {offset}");
    }

    /// #311 (Piero, 24/11): a drop-down list opened from the keyboard starts on the entry chosen,
    /// not on the first; the arrows go on from there; Enter chooses the entry and closes the list.
    #[test]
    fn a_list_opens_on_its_choice_and_enter_closes_it() {
        let ctx = egui::Context::default();
        let mut chosen = 6usize;
        let draw = |ui: &mut egui::Ui, chosen: &mut usize| {
            egui::ComboBox::from_id_salt("list-test").selected_text(chosen.to_string()).show_list(ui, |ui| {
                for i in 0..10 {
                    choice(ui, chosen, i, format!("entry {i}"));
                }
            });
        };
        let key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        let frame = |ctx: &egui::Context, events, chosen: &mut usize| crate::addons_ui::tests::frame(ctx, events, |ui| draw(ui, chosen));
        frame(&ctx, Vec::new(), &mut chosen);
        frame(&ctx, vec![key(egui::Key::Tab)], &mut chosen);
        frame(&ctx, vec![key(egui::Key::Space)], &mut chosen);
        frame(&ctx, Vec::new(), &mut chosen);
        frame(&ctx, Vec::new(), &mut chosen);
        assert!(egui::Popup::is_any_open(&ctx), "Space did not open the list");
        frame(&ctx, vec![key(egui::Key::ArrowDown)], &mut chosen);
        frame(&ctx, Vec::new(), &mut chosen);
        frame(&ctx, vec![key(egui::Key::Enter)], &mut chosen);
        frame(&ctx, Vec::new(), &mut chosen);
        frame(&ctx, Vec::new(), &mut chosen);
        assert_eq!(chosen, 7, "the arrow did not go on from the entry chosen (6), or Enter did not choose");
        assert!(!egui::Popup::is_any_open(&ctx), "Enter chose but left the list open");
    }
}
