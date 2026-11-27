//! The rows of the add-ons (#212), the same on the first-start window and in Options > Add-ons:
//! a card each, with icon, name and size, one line about it, and where it is (downloading,
//! downloaded, on the system); at its right the box to tick or the button, or a green mark when
//! there is nothing to download. Where each one is comes from `component::have`, read when the list opens and when a
//! download ends, not at every frame (it looks at the disk).

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::component::{Addon, Have};
use crate::downloads::Slot;
use crate::i18n::I18n;
use crate::confirm::FollowArea;

/// Side of the icon of a row.
const ICON: f32 = 40.0;
/// Side of the box to tick and of the green mark at the right of a row.
const MARK: f32 = 20.0;
/// Height of the button at the right of a row.
const BUTTON: f32 = 28.0;

/// What the right of a row holds: a box to tick (the first-start window) or a button (Options).
pub(crate) enum Control<'a> {
    Tick(&'a mut bool),
    Button,
}

/// The add-ons of this system and where each one is.
pub(crate) fn list() -> Vec<(Addon, Have)> {
    crate::component::addons().into_iter().map(|a| (a, crate::component::have(&a))).collect()
}

/// Something to download: missing, or an older version.
pub(crate) fn offers(have: &Have) -> bool {
    matches!(have, Have::Missing | Have::Older)
}

/// Where the download of a row is.
enum State {
    Idle,
    Running(u64),
    Done,
    Failed(String),
}

impl State {
    /// No download under way and none arrived: one can start (again, after one that failed).
    fn idle(&self) -> bool {
        matches!(self, State::Idle | State::Failed(_))
    }
}

/// A download of the row was started: under way, arrived, or failed (Piero, 22/11: after a
/// failure the card stays ticked and Done closes without asking; Download tries again).
pub(crate) fn started(dl: &Slot) -> bool {
    dl.is_some()
}

/// Where a download of Download and close is (`first_start::ended`). A slot emptied counts as
/// arrived: the video and CAD engines empty theirs only once theirs has arrived and is in use.
pub(crate) fn download(dl: &Slot) -> crate::first_start::Download {
    use crate::first_start::Download;
    match state(dl) {
        State::Running(_) => Download::Running,
        State::Failed(_) => Download::Failed,
        State::Done | State::Idle => Download::Arrived,
    }
}

fn state(dl: &Slot) -> State {
    let Some(d) = dl else { return State::Idle };
    match d.result.lock().unwrap().clone() {
        None => State::Running(d.done.load(Ordering::Relaxed)),
        Some(Ok(())) => State::Done,
        Some(Err(e)) => State::Failed(e),
    }
}

/// What the right of a row shows.
#[derive(Clone, Copy)]
enum Right {
    /// A box to tick, ticked or not: the whole row is clicked.
    Tick(bool),
    /// Download or Update: the Fluent id of its text.
    Button(&'static str),
    /// Nothing to download, it is here: the green mark.
    Here,
    Nothing,
}

/// The box or the button only where there is something to download and no download under way;
/// the green mark when it is here, downloaded or on the system.
fn right(have: &Have, state: &State, control: &Control) -> Right {
    let can = offers(have) && state.idle();
    match control {
        Control::Tick(ticked) if can => Right::Tick(**ticked),
        Control::Button if can => Right::Button(if *have == Have::Older { "addon-update" } else { "addon-download" }),
        _ if matches!(state, State::Done) || matches!((state, have), (State::Idle, Have::Downloaded | Have::OnSystem(_))) => Right::Here,
        _ => Right::Nothing,
    }
}

/// One row; true when its button (Download or Update) was pressed.
pub(crate) fn row(ui: &mut egui::Ui, (a, have): (&Addon, &Have), dl: &Slot, i18n: &I18n, control: Control) -> bool {
    let state = state(dl);
    let right = right(have, &state, &control);
    let id = ui.scope_id().with(("addon", a.component.kind));
    // The whole card is the box to tick: under the pointer its edge is the accent, as a button's.
    let over = matches!(right, Right::Tick(_)) && ui.ctx().read_response(id).is_some_and(|r| r.hovered() || r.has_focus());
    let v = ui.visuals();
    let frame = match right {
        Right::Tick(true) => crate::theme::card_chosen(v),
        _ if over => crate::theme::card(v).stroke(v.widgets.hovered.bg_stroke),
        _ => crate::theme::card(v),
    };
    let button = if let Right::Button(text) = right { Some(i18n.t(text)) } else { None };
    let room = button.as_ref().map_or(MARK, |text| crate::theme::hint_width(ui, text));
    let card = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.style_mut().interaction.selectable_labels = !matches!(right, Right::Tick(_));
        ui.horizontal_top(|ui| {
            ui.add(crate::icons::image(ui.ctx(), a.icon, ICON));
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - room - 12.0);
                texts(ui, (a, have), &state, i18n);
            });
        });
    });
    // At the right, in the middle of the card's height, known only now.
    let inner = card.response.rect.shrink2(frame.total_margin().sum() / 2.0);
    let at = |size: egui::Vec2| egui::Rect::from_center_size(egui::pos2(inner.right() - size.x / 2.0, inner.center().y), size);
    match (right, control) {
        (Right::Tick(ticked), Control::Tick(value)) => {
            let name = title(i18n, a);
            let response = ui.interact(card.response.rect, id, egui::Sense::click());
            response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, ticked, &name));
            tick(ui, at(egui::Vec2::splat(MARK)), ticked);
            *value = ticked != response.clicked();
        }
        // Here (downloaded or on the system): nothing to tick. Under way the tick stays, so that a
        // download that fails leaves its card ticked, ready for Download again.
        (Right::Here, Control::Tick(value)) => *value = false,
        _ => {}
    }
    match (right, button) {
        (Right::Here, _) => here(ui, at(egui::Vec2::splat(MARK)), frame.fill),
        (_, Some(text)) => {
            // In a child of its own: the rows under this one stay where they are.
            let place = egui::UiBuilder::new().max_rect(at(egui::vec2(room, BUTTON))).layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight));
            return ui.new_child(place).add(egui::Button::new(text)).clicked();
        }
        _ => {}
    }
    false
}

/// The add-on's name with the version people see (`Component::shown`): "Video engine 1.1",
/// "FFmpeg 9.0", "CAD engine 1.1" (Piero, 23/11/2026).
fn title(i18n: &I18n, a: &Addon) -> String {
    format!("{} {}", i18n.t(a.name), a.component.shown)
}

/// Name and size, the line about it, and where it is.
fn texts(ui: &mut egui::Ui, (a, have): (&Addon, &Have), state: &State, i18n: &I18n) {
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.horizontal(|ui| {
        let bold = egui::FontId::new(14.5, egui::FontFamily::Name(crate::BOLD.into()));
        ui.label(egui::RichText::new(title(i18n, a)).font(bold).strong());
        ui.add_space(4.0);
        ui.weak(crate::human::size(i18n, a.component.size));
    });
    ui.add(egui::Label::new(i18n.t(a.about)).wrap());
    status_line(ui, have, state, a.component.size, i18n);
}

/// The box to tick: the accent with a white check when ticked.
fn tick(ui: &egui::Ui, rect: egui::Rect, ticked: bool) {
    let v = ui.visuals();
    if ticked {
        ui.painter().rect_filled(rect, 5, crate::theme::solid(v));
        check(ui.painter(), rect, egui::Color32::WHITE);
    } else {
        ui.painter().rect(rect, 5, v.extreme_bg_color, egui::Stroke::new(1.5, crate::theme::tile(v).1), egui::StrokeKind::Inside);
    }
}

/// The green mark of an add-on that is here; its check in the card's colour.
fn here(ui: &egui::Ui, rect: egui::Rect, card: egui::Color32) {
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, crate::theme::good(ui.visuals()));
    check(ui.painter(), rect, card);
}

/// A check mark in `rect` (square).
pub(crate) fn check(painter: &egui::Painter, rect: egui::Rect, colour: egui::Color32) {
    let at = |x: f32, y: f32| rect.min + egui::vec2(x, y) * rect.width();
    painter.line(vec![at(0.27, 0.52), at(0.43, 0.68), at(0.73, 0.34)], egui::Stroke::new(2.0, colour));
}

/// Under the line about it: the progress, the error of the last try, or where it is.
fn status_line(ui: &mut egui::Ui, have: &Have, state: &State, total: u64, i18n: &I18n) {
    let text = match (state, have) {
        (State::Running(done), _) => {
            // The bar and how far it is on one line: the row as tall as one that is here.
            let text = i18n.t_args("video-component-progress", &[("done", &crate::human::size(i18n, *done)), ("total", &crate::human::size(i18n, total))]);
            let line = egui::vec2(ui.available_width(), ui.text_style_height(&egui::TextStyle::Body));
            ui.allocate_ui_with_layout(line, egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(text);
                ui.add_space(4.0);
                let bar = egui::ProgressBar::new(*done as f32 / total as f32).desired_width(ui.available_width()).desired_height(6.0);
                ui.add(bar.fill(crate::theme::accent(ui.visuals())));
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            return;
        }
        (State::Failed(e), _) => {
            // One line, so the card stays as tall as the others; the whole error under the pointer
            // (a long one, with the address, pushed the next card out of sight).
            let text = i18n.t_args("batch-video-download-failed", &[("detail", &crate::update::error_detail(i18n, e))]);
            // As every message inside a window: the bar's small triangle and the theme's text, never
            // red (Piero, 20/11).
            ui.horizontal(|ui| {
                crate::theme::warning_icon(ui, 14.0);
                ui.add(egui::Label::new(&text).truncate()).on_hover_text(&text);
            });
            return;
        }
        (State::Done, _) | (State::Idle, Have::Downloaded) => i18n.t("addon-downloaded"),
        (State::Idle, Have::Missing) => return,
        (State::Idle, Have::Older) => i18n.t("addon-older"),
        (State::Idle, Have::OnSystem(version)) => i18n.t_args("addon-on-system", &[("version", version)]),
        (State::Idle, Have::SystemOnly) => i18n.t("addon-system-only"),
    };
    ui.add(egui::Label::new(egui::RichText::new(text).weak()).wrap());
}

/// Options > Add-ons: the same rows, each with Download or Update. Where they are is read when
/// the tab is first shown and again when one of their downloads ends.
#[derive(Default)]
pub(crate) struct Tab {
    rows: Option<Vec<(Addon, Have)>>,
    /// The kinds downloading at the last frame.
    running: Vec<&'static str>,
}

impl Tab {
    /// The tab; the add-on whose button was pressed.
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, downloads: &crate::downloads::Downloads, i18n: &I18n) -> Option<crate::component::Component> {
        let running: Vec<_> = crate::component::addons().iter().map(|a| a.component.kind).filter(|k| crate::downloads::running(downloads.get(k))).collect();
        if running.len() < self.running.len() {
            self.rows = None;
        }
        self.running = running;
        let rows = self.rows.get_or_insert_with(list);
        ui.add(egui::Label::new(i18n.t("opt-addons-text")).wrap());
        ui.add_space(8.0);
        let mut pressed = None;
        egui::ScrollArea::vertical().auto_shrink([false, true]).show_follow(ui, |ui| {
            for (i, (a, have)) in rows.iter().enumerate() {
                if i > 0 {
                    ui.add_space(8.0);
                }
                if row(ui, (a, have), downloads.get(a.component.kind), i18n, Control::Button) {
                    pressed = Some(a.component);
                }
            }
        });
        pressed
    }
}

#[cfg(test)]
pub(crate) mod tests {
    /// Each add-on shows its version next to its name (Piero, 23/11/2026).
    #[test]
    fn titles_with_their_version() {
        let it = crate::i18n::I18n::new("it");
        let titles: Vec<String> = crate::component::addons().iter().map(|a| super::title(&it, a)).collect();
        let video = if cfg!(windows) { "Motore video 1.0" } else { "Motore video 1.1" };
        assert_eq!(titles, vec![video.to_string(), "FFmpeg 9.0".to_string(), "Motore CAD 1.1".to_string()]);
    }

    use super::*;

    /// Where a download of Download and close is, from its slot: under way, failed, arrived; a slot
    /// emptied (the engines empty theirs once theirs is in use) counts as arrived. Any slot means
    /// a download was started, a failed one too (Done then closes without asking).
    #[test]
    fn download_state_from_its_slot() {
        use crate::first_start::Download;
        let d = std::sync::Arc::new(crate::component::Download::default());
        let slot: Slot = Some(d.clone());
        assert_eq!(download(&slot), Download::Running);
        assert!(started(&slot));
        *d.result.lock().unwrap() = Some(Err("x".into()));
        assert_eq!(download(&slot), Download::Failed);
        assert!(started(&slot), "a failed download was started");
        *d.result.lock().unwrap() = Some(Ok(()));
        assert_eq!(download(&slot), Download::Arrived);
        assert_eq!(download(&None), Download::Arrived);
        assert!(!started(&None));
    }

    pub(crate) fn frame(ctx: &egui::Context, events: Vec<egui::Event>, mut draw: impl FnMut(&mut egui::Ui)) {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| draw(ui));
        out.textures_delta.clear();
    }

    /// A headless egui with the app's fonts, and where its window draws.
    pub(crate) fn headless() -> (egui::Context, egui::Rect) {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let mut area = egui::Rect::NOTHING;
        frame(&ctx, Vec::new(), |ui| area = ui.max_rect());
        (ctx, area)
    }

    /// A click at `at` on what `draw` draws: the pointer arrives, presses and lets go, a frame
    /// each, and one more frame draws what the click left.
    pub(crate) fn click(ctx: &egui::Context, at: egui::Pos2, mut draw: impl FnMut(&mut egui::Ui)) {
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        for events in [vec![], vec![egui::Event::PointerMoved(at)], vec![button(true)], vec![button(false)], vec![]] {
            frame(ctx, events, &mut draw);
        }
    }

    /// The whole card is the box to tick: a click on its text ticks it, one more unticks it; a
    /// card with nothing to download has no box, and a click there ticks nothing.
    #[test]
    fn a_click_on_the_card_ticks_it() {
        let Some(a) = crate::component::addons().into_iter().next() else { return };
        let (ctx, area) = headless();
        let (i18n, at, mut ticked) = (I18n::new("en"), area.min + egui::vec2(200.0, 30.0), false);
        let draw = |have: &Have, ticked: &mut bool| {
            click(&ctx, at, |ui| {
                row(ui, (&a, have), &None, &i18n, Control::Tick(ticked));
            });
        };
        draw(&Have::Missing, &mut ticked);
        assert!(ticked, "a click on the card");
        draw(&Have::Older, &mut ticked);
        assert!(!ticked, "a second click");
        draw(&Have::Downloaded, &mut ticked);
        assert!(!ticked, "nothing to download");
        ticked = true;
        draw(&Have::OnSystem("1".into()), &mut ticked);
        assert!(!ticked, "a tick left on a card with nothing to download");
    }

    /// The button of Options, at the right of the card: pressed where there is something to
    /// download, and the rows under it stay where they are.
    #[test]
    fn the_button_of_a_row_is_pressed() {
        let Some(a) = crate::component::addons().into_iter().next() else { return };
        let (ctx, area) = headless();
        let (i18n, at) = (I18n::new("en"), egui::pos2(area.right() - 40.0, area.top() + 33.0));
        let (mut pressed, mut under) = (false, 0.0);
        click(&ctx, at, |ui| {
            pressed |= row(ui, (&a, &Have::Missing), &None, &i18n, Control::Button);
            under = ui.cursor().top();
        });
        assert!(pressed);
        assert!(under > at.y + BUTTON / 2.0, "the next row starts under the card: {under}");
        pressed = false;
        click(&ctx, at, |ui| pressed |= row(ui, (&a, &Have::Downloaded), &None, &i18n, Control::Button));
        assert!(!pressed, "nothing to download, no button");
    }
}
