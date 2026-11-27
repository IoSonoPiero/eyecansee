//! The copy window while the phone copies by itself inside one storage (#354): a modal in the
//! middle of the app for the whole paste, since the phone answers nothing meanwhile (Piero, 26/11
//! night). Everything under it waits, the computer's folders too. Only what is known is shown
//! (`transfer::view`): the file under way in bold, its size and seconds, a bar (a strip that
//! moves while the phone is on a file), with several files the counts, and a short explanation.
//! Its size never changes while it is open: the width is fixed, every row keeps its place even
//! when empty, and a long name is cut on one line (Piero, 26/11: "for a second it gets bigger").
//! Cancel: the phone ends the file all the same, then its copy goes (`mtp::undo`); once pressed
//! the button gives way to what is happening and to "Stop waiting", the way out of a wait that
//! lasts (`mtp::leave`: what the phone makes then stays). Esc does nothing (the window cannot close before the
//! phone is done); Enter and Space press Cancel only once Tab has reached it.

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::human::{human_time, size_gb};
use crate::transfer::{Bar, Data, Leave, Note};
use crate::{App, theme, transfer};

/// The id of its modal.
pub(crate) const ID: &str = "phone-copy";
/// The width of what is in it.
const WIDTH: f32 = 440.0;
/// The height of the bar.
const BAR: f32 = 16.0;
/// The lines the explanation always takes, whatever it says.
const NOTE_LINES: f32 = 3.0;

/// The words of the window, ready to draw.
struct Shown {
    title: String,
    /// The file under way, and with several files which one it is.
    name: String,
    nth: Option<String>,
    /// Its size and its seconds.
    detail: String,
    bar: Bar,
    /// With several files, the rows of the bytes and of the speed (empty until known).
    counts: Option<[String; 2]>,
    /// The explanation, and whether it says what is happening now (after Cancel, after 3
    /// minutes): in the colour of the text then, fainter otherwise.
    note: (String, bool),
    /// Cancel and its tooltip, or once pressed what is happening.
    cancel: Result<(String, String), String>,
    why: String,
}

impl App {
    /// The paste under way is one the phone copies by itself inside one storage, and its phone
    /// is still among the devices: the copy window is this modal, from the start of the paste
    /// (while its files are counted too: the copy box in the corner showed first, for a second).
    pub(crate) fn phone_copy_modal(&self) -> bool {
        self.transfer.as_ref().is_some_and(|(p, ..)| p.by_phone.modal())
    }

    /// What the window knows now.
    fn phone_copy_seen(p: &transfer::Progress) -> transfer::Seen {
        transfer::Seen {
            file: p.by_phone.file(),
            current: p.current.lock().unwrap().clone(),
            files: p.files.load(Ordering::Relaxed),
            files_total: p.files_total.load(Ordering::Relaxed),
            done: p.done.load(Ordering::Relaxed),
            total: p.total.load(Ordering::Relaxed),
            big: p.by_phone.big(),
            cancelling: p.cancel.load(Ordering::Relaxed),
            through_pc: p.staging.lock().unwrap().is_some(),
            many: p.by_phone.many(),
            counting: match p.phase() {
                transfer::Phase::Looking(found) => Some(found),
                _ => None,
            },
            leaving: crate::mtp::leaving(),
            windows_gave_up: crate::mtp::windows_gave_up(),
            stays: p.by_phone.stays(),
        }
    }

    /// The words of the window for what it knows now (`transfer::view`).
    fn phone_copy_shown(&self, p: &transfer::Progress) -> Shown {
        let seen = Self::phone_copy_seen(p);
        let v = transfer::view(&seen);
        let (t, size) = (|id: &str| self.i18n.t(id), |b: u64| size_gb(&self.i18n, b));
        let detail = match v.file {
            // The items found in the folders, once there are some.
            None if v.counting.is_some_and(|found| found > 0) => self.i18n.t_args("transfer-looking", &[("n", &v.counting.unwrap_or(0).to_string())]),
            Some((s, Some(secs))) => self.i18n.t_args("phone-copy-for", &[("size", &size(s)), ("time", &human_time(secs))]),
            Some((s, None)) => size(s),
            // A big file of the other storage, through this computer: the line of the copy box.
            None if seen.through_pc => self.phone_line(p, seen.cancelling).unwrap_or_default(),
            None => String::new(),
        };
        let data = v.data.map_or_else(String::new, |d| match d {
            Data::Total(total) => self.i18n.t_args("phone-copy-total", &[("size", &size(total))]),
            Data::Done { done, total } => self.i18n.t_args("phone-copy-done", &[("done", &size(done)), ("size", &size(total))]),
        });
        let speed = v.speed.map_or_else(String::new, |(speed, left)| self.i18n.t_args("phone-copy-speed", &[("speed", &size(speed)), ("time", &human_time(left))]));
        let now = match v.note {
            Note::Waiting => t("phone-copy-notice"),
            Note::Long => t("phone-copy-long"),
            Note::Cancelled { others: 0 } => t("phone-copy-cancelled-note"),
            Note::Cancelled { others } => format!("{} {}", t("phone-copy-cancelled-note"), self.i18n.t_args("phone-copy-others", &[("n", &others.to_string())])),
            Note::Left => t("phone-copy-leaving-note"),
            Note::ThroughPc => t("phone-copy-through-pc"),
        };
        Shown {
            title: t("phone-copy-title"),
            nth: v.nth.map(|(n, total)| self.i18n.t_args("phone-copy-nth", &[("n", &n.to_string()), ("total", &total.to_string())])),
            detail,
            bar: v.bar,
            counts: v.rows.then_some([data, speed]),
            note: (if v.keep { format!("{now} {}", t("phone-copy-keep")) } else { now }, v.note != Note::Waiting),
            cancel: match (v.cancel, v.leave) {
                (true, _) => Ok((t("delete-no"), t("phone-copy-cancel-tip"))),
                (false, Leave::Asked) => Err(t("phone-copy-leaving")),
                (false, _) => Err(t("phone-copy-cancelled")),
            },
            why: t("phone-copy-why"),
            name: if v.counting.is_some() { t("phone-copy-counting") } else { v.name },
        }
    }

    /// The modal (`phone_copy_modal`), drawn in place of the copy box.
    pub(crate) fn phone_copy_window(&mut self, ctx: &egui::Context) {
        let Some((p, ..)) = &self.transfer else { return };
        let p = p.clone();
        let shown = self.phone_copy_shown(&p);
        // Esc closes nothing here: taken, so it answers no window under this one later.
        let _ = crate::confirm::answer(ctx, &mut self.key_answer, ID);
        let (mut pressed, mut help) = (false, false);
        egui::Modal::new(egui::Id::unique(ID)).show(ctx, |ui| {
            ui.set_width(WIDTH);
            (pressed, help) = draw(ui, &shown);
        });
        // The button of the window: Cancel.
        if pressed {
            p.cancel.store(true, Ordering::Relaxed);
        }
        if help {
            let title = self.i18n.t("phone-copy-why-section");
            self.open_help_at("phone", &title);
        }
    }
}

/// The window's content; whether its button (Cancel, or once that is pressed "Stop waiting") and
/// the link to the guide were pressed.
fn draw(ui: &mut egui::Ui, s: &Shown) -> (bool, bool) {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let bold = |size: f32| egui::FontId::new(size, egui::FontFamily::Name(crate::BOLD.into()));
    let line = ui.text_style_height(&egui::TextStyle::Body);
    crate::save_dialogs::Head::new(&s.title).show(ui);
    // The file under way: what the eye finds first. Cut on one line, whole in its tooltip.
    let main = bold(body.size + 2.0);
    row(ui, ui.fonts_mut(|f| f.row_height(&main)), |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(nth) = &s.nth {
                ui.label(nth);
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| ui.add(egui::Label::new(egui::RichText::new(&s.name).font(main.clone())).truncate()));
        });
    });
    row(ui, line, |ui| {
        ui.add(egui::Label::new(&s.detail).truncate());
    });
    ui.add_space(2.0);
    bar(ui, s.bar);
    for text in s.counts.iter().flatten() {
        row(ui, line, |ui| {
            ui.add(egui::Label::new(text).truncate());
        });
    }
    ui.add_space(8.0);
    // The explanation: at the size of the text (Piero, 27/11: small, it could not be read), its lines always there.
    let small = egui::TextStyle::Body.resolve(ui.style());
    let colour = if s.note.1 { ui.visuals().text_color() } else { theme::faint(ui.visuals()) };
    let height = (ui.fonts_mut(|f| f.row_height(&small)) * NOTE_LINES).ceil();
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_height(height);
        ui.add(egui::Label::new(egui::RichText::new(&s.note.0).font(small.clone()).color(colour)).wrap());
    });
    ui.add_space(8.0);
    let (mut cancel, mut help) = (false, false);
    row(ui, ui.spacing().interact_size.y, |ui| {
        let stop = match &s.cancel {
            Ok((label, tip)) => Some(ui.button(label).on_hover_text(tip)),
            // Once pressed, what is happening; no "Stop waiting" (Piero, 27/11): it read as
            // cancelling at once, while the phone ends its file and the copy stayed.
            Err(state) => {
                ui.add(egui::Label::new(egui::RichText::new(state).font(bold(body.size))).truncate());
                None
            }
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let link = ui.link(egui::RichText::new(&s.why).font(small.clone()));
            crate::confirm::join(ID, &link);
            help = link.clicked();
        });
        // No button highlighted at first: a habitual Enter cancels nothing; Tab reaches them.
        cancel = crate::confirm::keys(ui, ID, stop.as_slice(), usize::MAX) == Some(0);
    });
    (cancel, help)
}

/// A row `height` tall whatever is in it, even nothing: the window keeps its size.
fn row(ui: &mut egui::Ui, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(egui::vec2(ui.available_width(), height));
        add(ui);
    });
}

/// The bar. Several files: the part done as in the copy box, and while the phone is on a file a
/// pale strip that moves along the rest. One file: nothing is known of how far it is, so only
/// the strip, along the whole bar: the phone works, the window is not stuck.
fn bar(ui: &mut egui::Ui, bar: Bar) {
    let (track, accent) = (ui.visuals().widgets.inactive.bg_fill, theme::accent(ui.visuals()));
    match bar {
        Bar::Busy => {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), BAR), egui::Sense::hover());
            ui.painter().rect_filled(rect, BAR / 2.0, track);
            strip(ui, rect, accent);
        }
        Bar::Part { part, busy } => {
            // Accent fill on a grey track, as the copy box.
            let rect = ui
                .scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = track;
                    ui.add(egui::ProgressBar::new(part).fill(accent).desired_width(ui.available_width()).desired_height(BAR).show_percentage()).rect
                })
                .inner;
            // egui's bar is never shorter than it is tall.
            let rest = rect.with_min_x(rect.left() + (rect.width() * part).max(BAR));
            if busy && rest.width() > 2.0 * BAR {
                strip(ui, rest, accent.gamma_multiply(0.4));
            }
        }
    }
}

/// A strip that crosses `track` from left to right, again and again, by the clock: as wide as a
/// quarter of it, coming in and going out at the ends.
fn strip(ui: &egui::Ui, track: egui::Rect, colour: egui::Color32) {
    const SECS: f64 = 1.6;
    let turn = (ui.input(|i| i.time) % SECS / SECS) as f32;
    let wide = track.width() * 0.25;
    let left = track.left() - wide + (track.width() + wide) * turn;
    let rect = egui::Rect::from_min_max(egui::pos2(left, track.top()), egui::pos2(left + wide, track.bottom())).intersect(track);
    if rect.width() > 0.0 {
        ui.painter().rect_filled(rect, (rect.width() / 2.0).min(BAR / 2.0), colour);
    }
    ui.ctx().request_repaint();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use eframe::egui::{self, Key, Modifiers};

    use crate::App;
    use crate::audit_family_1::{Harness, folder, own_folder};

    const PHONE: &str = "mtp://TESTPHONE/";
    const GB: u64 = 1 << 30;

    /// The app on a folder of three pictures, the grid on the last one, and a paste the phone
    /// copies inside one storage set on it directly (no phone), as it is when just asked: of one
    /// file, or of several (`files`), nothing counted yet. The phone is not among the devices: a
    /// list from the devices' watcher, arriving late, would take it away.
    fn asked(name: &str, files: usize) -> (Harness, Arc<crate::transfer::Progress>) {
        let mut h = Harness::new(&folder(name, &["a.png", "b.png", "c.png"]));
        (h.app.cursor, h.app.selected, h.app.focus) = (true, 2, crate::Focus::Grid);
        let p = Arc::new(crate::transfer::Progress::default());
        p.by_phone.decide(true, files > 1);
        h.app.transfer = Some((p.clone(), false, PHONE.into(), "Internal storage/Download/".into()));
        frames(&mut h);
        (h, p)
    }

    /// The files of the paste counted, `files` of 1 GB, and the copy started: the second one
    /// under way (the only one, with one).
    fn start(p: &crate::transfer::Progress, files: usize) {
        p.files.store(files.min(2) - 1, Ordering::Relaxed);
        p.files_total.store(files, Ordering::Relaxed);
        p.total.store(files as u64 * GB, Ordering::Relaxed);
        p.by_phone.begin("big.mp4", GB);
        let _ = p.started.set(std::time::Instant::now());
    }

    /// `asked`, then `start`.
    fn copying(name: &str, files: usize) -> (Harness, Arc<crate::transfer::Progress>) {
        let (mut h, p) = asked(name, files);
        start(&p, files);
        frames(&mut h);
        (h, p)
    }

    fn frames(h: &mut Harness) {
        for _ in 0..3 {
            h.frame(Vec::new());
        }
    }

    fn open(a: &App) -> bool {
        a.phone_copy_modal() && a.transfer.is_some()
    }

    /// A double click at `at`.
    fn double_click(h: &mut Harness, at: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.frame(vec![egui::Event::PointerMoved(at)]);
        for _ in 0..2 {
            h.frame(vec![button(true)]);
            h.frame(vec![button(false)]);
        }
        h.frame(Vec::new());
    }

    /// What is under the window, as words.
    fn under(a: &App) -> String {
        format!("{:?}", (a.cursor, a.selected, a.focus, a.viewing, &a.dir, a.clip.is_some(), a.delete_confirm.is_some(), a.rename.is_some(), a.editor.is_some()))
    }

    /// With nothing highlighted no key cancels the copy, closes the window or reaches what is
    /// under it: Enter, Space and Esc among them.
    fn keys_do_nothing(h: &mut Harness, p: &crate::transfer::Progress) {
        let before = under(&h.app);
        for (key, modifiers) in [(Key::Enter, Modifiers::NONE), (Key::Space, Modifiers::NONE), (Key::Escape, Modifiers::NONE), (Key::ArrowLeft, Modifiers::NONE), (Key::Home, Modifiers::NONE), (Key::Delete, Modifiers::NONE), (Key::F2, Modifiers::NONE), (Key::Backspace, Modifiers::NONE), (Key::A, Modifiers::COMMAND), (Key::E, Modifiers::COMMAND)] {
            h.blur();
            h.key_with(key, modifiers);
            h.frame(Vec::new());
            assert!(open(&h.app), "{key:?} {modifiers:?} closed the window");
            assert!(!p.cancel.load(Ordering::Relaxed), "{key:?} {modifiers:?} with nothing highlighted cancelled the copy");
            assert_eq!(under(&h.app), before, "{key:?} {modifiers:?} reached what is under the window");
        }
    }

    /// Tab to Cancel, the first control on screen, then Enter: Cancel is asked and the window
    /// stays until the phone ends the file. Its button is gone then: the keyboard is not left on
    /// it, and Tab still reaches what is left, the link.
    fn cancel_with_the_keys(h: &mut Harness, p: &crate::transfer::Progress) {
        h.blur();
        h.frame(Vec::new());
        h.key(Key::Tab);
        h.key(Key::Enter);
        h.frame(Vec::new());
        assert!(p.cancel.load(Ordering::Relaxed) && open(&h.app), "Cancel pressed with the keys, the window staying until the file ends");
        frames(h);
        assert_eq!(h.ctx.memory(|m| m.focused()), None, "the keyboard left on a button that is gone");
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(super::ID));
        for _ in 0..2 {
            h.key(Key::Tab);
            h.frame(Vec::new());
            let focused = h.ctx.memory(|m| m.focused()).and_then(|id| h.ctx.read_response(id));
            assert!(focused.is_some_and(|r| r.layer_id == layer) && open(&h.app), "Tab after Cancel: the keyboard is not on the link");
        }
        h.key(Key::Escape);
        frames(h);
        assert!(open(&h.app), "Esc after Cancel closed the window");
    }

    /// #354: while the phone copies by itself the window is a modal on top: the mouse and the keys
    /// reach nothing under it (the grid, the viewer, the menus' shortcuts). Tab goes through Cancel
    /// and the link; Enter, Space and Esc with nothing highlighted cancel nothing, and Esc does not
    /// close it. Once the transfer is gone, the same double click reaches the grid.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_window_holds_everything_under_it() {
        if own_folder("phone_copy_window::tests::the_window_holds_everything_under_it") {
            return;
        }
        let (mut h, p) = copying("phone-copy-modal", 3);
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(super::ID));
        assert_eq!(h.ctx.memory(|m| m.top_modal_layer()), Some(layer), "the window is the modal on top");
        let before = under(&h.app);
        let grid = h.app.grid_rect.expect("the grid drawn");
        let first = grid.min + egui::vec2(40.0, 40.0);
        double_click(&mut h, first);
        assert_eq!(under(&h.app), before, "a double click reached the grid under the window");
        keys_do_nothing(&mut h, &p);
        crate::window_keys::tab_goes_through(&mut h, crate::window_keys::modal(super::ID), open, 2, true);
        cancel_with_the_keys(&mut h, &p);
        // The control: without the window the same double click reaches the grid.
        h.app.transfer = None;
        frames(&mut h);
        double_click(&mut h, first);
        assert_ne!(under(&h.app), before, "the double click was not on the grid");
    }

    /// #354, one file (the 6 GB copy of Piero's test): the same window, with Cancel too, which
    /// removes the copy once the phone has made it. No key cancels by itself; Tab reaches Cancel
    /// and the link.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn one_file_has_cancel_too() {
        if own_folder("phone_copy_window::tests::one_file_has_cancel_too") {
            return;
        }
        let (mut h, p) = copying("phone-copy-one", 1);
        keys_do_nothing(&mut h, &p);
        crate::window_keys::tab_goes_through(&mut h, crate::window_keys::modal(super::ID), open, 2, true);
        cancel_with_the_keys(&mut h, &p);
    }

    /// Where the control that has the keyboard is.
    fn focused_rect(h: &Harness) -> egui::Rect {
        h.ctx.memory(|m| m.focused()).and_then(|id| h.ctx.read_response(id)).map(|r| r.rect).expect("a control with the keyboard")
    }

    /// Tab, or Shift+Tab, from nothing focused: where the control it reaches is.
    fn tab_to(h: &mut Harness, modifiers: Modifiers) -> egui::Rect {
        h.key_with(Key::Tab, modifiers);
        h.frame(Vec::new());
        focused_rect(h)
    }

    /// #354 (prova W5 on Windows, 27/11): once Cancel was pressed nothing could be done until the
    /// phone had ended its file, and the wait for it could last for ever. After Cancel the window
    /// has a way out, "Stop waiting", next to what is happening and not where Cancel was: a double
    /// click on Cancel, or Enter twice, only cancels. It fits in the row with the link, in both
    /// languages. Pressed, the wait for the phone is left (`mtp::leave`) and the button goes; the
    /// window stays until the copy lets go.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn after_cancel_the_window_waits_for_the_phone() {
        if own_folder("phone_copy_window::tests::after_cancel_the_window_waits_for_the_phone") {
            return;
        }
        for lang in ["en", "it"] {
            crate::mtp::stay();
            let (mut h, p) = copying(&format!("phone-copy-leave-{lang}"), 1);
            p.by_phone.begin_ago("big.mp4", GB, 40);
            h.app.i18n = crate::i18n::I18n::new(lang);
            frames(&mut h);
            h.blur();
            let cancel = tab_to(&mut h, Modifiers::NONE);
            h.blur();
            double_click(&mut h, cancel.center());
            frames(&mut h);
            assert!(p.cancel.load(Ordering::Relaxed) && open(&h.app), "{lang}: Cancel not pressed by the click");
            assert!(!crate::mtp::leaving(), "{lang}: a double click on Cancel left the wait");
            assert_eq!(h.ctx.memory(|m| m.focused()), None, "{lang}: the keyboard on a button after the click");
            h.key(Key::Enter);
            frames(&mut h);
            assert!(!crate::mtp::leaving(), "{lang}: Enter with nothing highlighted left the wait");
            // No "Stop waiting" (Piero, 27/11): Cancel waits for the phone; only the link is left.
            let link = tab_to(&mut h, Modifiers::NONE);
            assert_eq!(tab_to(&mut h, Modifiers::NONE), link, "{lang}: a button besides the link after Cancel");
            assert!(!crate::mtp::leaving() && open(&h.app), "{lang}: the wait left without asking");
        }
        crate::mtp::stay();
    }

    /// #354 (prova W5 on Windows, 27/11): the phone unplugged, the copy window was the box in the
    /// corner with Cancel greyed out (the ✕ had cancelled the copy), "The phone is copying…
    /// (177 s)", and nothing to press. Cancelled while the phone copies alone, the box has the way
    /// out in place of the greyed Cancel; pressed, the wait is left. With no phone at work the
    /// greyed Cancel is all there is, as before.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_corner_box_has_no_stop_waiting() {
        if own_folder("phone_copy_window::tests::the_corner_box_has_no_stop_waiting") {
            return;
        }
        crate::mtp::stay();
        let (mut h, p) = copying("phone-copy-corner-leave", 1);
        p.by_phone.unplugged();
        p.cancel.store(true, Ordering::Relaxed);
        frames(&mut h);
        let controls = |h: &Harness| crate::box_keys::controls(&h.ctx);
        assert_eq!(controls(&h).len(), 0, "cancelled, no phone at work: only the greyed Cancel");
        let working = crate::mtp::copying_for_tests("Internal storage/DCIM/big.mp4", 40);
        frames(&mut h);
        assert_eq!(controls(&h).len(), 0, "cancelled while the phone copies alone: no \"Stop waiting\" (Piero, 27/11)");
        assert!(!crate::mtp::leaving(), "the wait left without asking");
        drop(working);
        crate::mtp::stay();
    }

    /// The size of the window on screen.
    fn size(h: &mut Harness, what: &str) -> egui::Vec2 {
        frames(h);
        let size = h.ctx.memory(|m| m.area_rect(egui::Id::unique(super::ID))).unwrap_or_else(|| panic!("no window: {what}")).size();
        (size * 100.0).round() / 100.0
    }

    /// Piero, 26/11: "as soon as the copy starts the window goes down for a second, then appears
    /// in the centre": while the files were counted the copy box in the corner showed. The window
    /// in the middle is there from the first frame of the paste, modal, before anything is
    /// counted: no copy box in the corner, the keys cancel nothing and reach nothing under it,
    /// Cancel works. (Unplugged, the copy box is the one in the corner: the check of the check.)
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_window_is_there_from_the_start() {
        if own_folder("phone_copy_window::tests::the_window_is_there_from_the_start") {
            return;
        }
        for files in [1, 3] {
            let (mut h, p) = asked(&format!("phone-copy-start-{files}"), files);
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(super::ID));
            // The controls of the boxes in the corner drawn in the last frame: Cancel of the copy box.
            let corner = |h: &Harness| crate::box_keys::controls(&h.ctx).len();
            assert!(open(&h.app) && h.ctx.memory(|m| m.top_modal_layer()) == Some(layer), "{files}: no window in the middle while the files are counted");
            assert_eq!(corner(&h), 0, "{files}: the copy box in the corner showed first");
            keys_do_nothing(&mut h, &p);
            cancel_with_the_keys(&mut h, &p);
            // Another paste, its phone unplugged: there the copy box is the one in the corner.
            let (mut h, p) = asked(&format!("phone-copy-start-unplugged-{files}"), files);
            p.by_phone.unplugged();
            frames(&mut h);
            assert!(corner(&h) > 0 && h.ctx.memory(|m| m.top_modal_layer()).is_none(), "{files}: unplugged, the copy box in the corner");
        }
    }

    /// Piero, 26/11: "while it runs, the window for a second gets bigger and then goes back": a
    /// long name wrapped. The window has one size from when it opens (its files not counted yet)
    /// to when it closes, whatever it says, in both languages: the items found, the copy started,
    /// a name longer than the window, the bytes done and the speed
    /// once known, 3 minutes on a file, after Cancel (with files left out, and on the last file), after Stop waiting,
    /// a file through the computer. With one file and with several (which has the rows of the
    /// counts: taller, and as wide).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_window_keeps_its_size() {
        if own_folder("phone_copy_window::tests::the_window_keeps_its_size") {
            return;
        }
        let long = format!("{}.mp4", "a very long name of a video ".repeat(12));
        for lang in ["en", "it"] {
            let mut sizes = Vec::new();
            for files in [1, 5] {
                let (mut h, p) = asked(&format!("phone-copy-size-{lang}-{files}"), files);
                h.app.i18n = crate::i18n::I18n::new(lang);
                let first = size(&mut h, "as it opens, the files not counted yet");
                let same = |h: &mut Harness, what: &str| assert_eq!(size(h, what), first, "{lang}, {files} files: the window changed size: {what}");
                p.found.store(1234, Ordering::Relaxed);
                same(&mut h, "items found in the folders");
                start(&p, files);
                same(&mut h, "the copy started");
                p.by_phone.begin(&long, 6 * GB);
                *p.current.lock().unwrap() = long.clone();
                same(&mut h, "a long name");
                p.done.store(GB, Ordering::Relaxed);
                p.by_phone.set_big(GB, 10.0);
                same(&mut h, "bytes done and a speed");
                p.by_phone.begin_ago(&long, 6 * GB, crate::transfer::LONG_SECS + 20);
                same(&mut h, "3 minutes on a file");
                *p.staging.lock().unwrap() = Some(crate::transfer::Staging::Reading);
                same(&mut h, "a file through the computer");
                *p.staging.lock().unwrap() = None;
                p.cancel.store(true, Ordering::Relaxed);
                same(&mut h, "after Cancel");
                p.files.store(files - 1, Ordering::Relaxed);
                same(&mut h, "after Cancel on the last file");
                crate::mtp::leave();
                same(&mut h, "after Stop waiting");
                crate::mtp::stay();
                sizes.push(first);
            }
            assert!(sizes[0].x == sizes[1].x && sizes[0].y < sizes[1].y, "{lang}: one file {:?}, several {:?}", sizes[0], sizes[1]);
        }
    }

    /// Audit T.6 (27/11): Cancel's tooltip said that the phone ends the file and then its copy
    /// goes, also for a big file going through this computer, where Cancel stops the reading at
    /// once and a file being written to the phone is finished and stays. And for a small file of
    /// the other storage that the phone copies (a paste of items of both storages), Cancel's
    /// tooltip and the line after Cancel said that the copy just made goes: it stays, only a
    /// copy inside one storage is removed. In both languages.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn cancel_says_what_becomes_of_the_file_under_way() {
        if own_folder("phone_copy_window::tests::cancel_says_what_becomes_of_the_file_under_way") {
            return;
        }
        let mut wrong = Vec::new();
        for lang in ["en", "it"] {
            let (mut h, p) = copying(&format!("phone-copy-cancel-says-{lang}"), 2);
            h.app.i18n = crate::i18n::I18n::new(lang);
            let (removed_tip, removed_note) = (h.app.i18n.t("phone-copy-cancel-tip"), h.app.i18n.t("phone-copy-cancelled-note"));
            let tip = |h: &Harness| h.app.phone_copy_shown(&p).cancel.map(|(_, tip)| tip);
            assert_eq!(tip(&h), Ok(removed_tip.clone()), "{lang}: a copy inside one storage, removed");
            *p.staging.lock().unwrap() = Some(crate::transfer::Staging::Reading);
            if tip(&h) == Ok(removed_tip.clone()) {
                wrong.push(format!("{lang}: through this computer, the tooltip: {removed_tip}"));
            }
            *p.staging.lock().unwrap() = None;
            p.by_phone.given("SD card/DCIM/a.jpg", "Internal storage/Download/a.jpg");
            if tip(&h) == Ok(removed_tip.clone()) {
                wrong.push(format!("{lang}: a file of the other storage, the tooltip: {removed_tip}"));
            }
            p.cancel.store(true, Ordering::Relaxed);
            let note = h.app.phone_copy_shown(&p).note.0;
            if note.contains(&removed_note) {
                wrong.push(format!("{lang}: a file of the other storage, after Cancel: {note}"));
            }
            frames(&mut h);
        }
        assert!(wrong.is_empty(), "Cancel promises to remove a copy that stays:\n{}", wrong.join("\n"));
    }

    /// #354: the phone gone from the devices during the copy: the window is no longer modal (back to
    /// the corner box until the paste ends), so a phone that is not there never holds the app.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn an_unplugged_phone_lets_the_app_go() {
        if own_folder("phone_copy_window::tests::an_unplugged_phone_lets_the_app_go") {
            return;
        }
        let (mut h, _p) = copying("phone-copy-unplugged", 3);
        assert!(open(&h.app));
        h.app.devices.push(crate::devices::Device { name: "Test phone".into(), path: PHONE.into(), phone: true, serial: Some("TESTPHONE".into()), block: None, internal: false });
        let left: Vec<_> = h.app.devices.iter().filter(|d| d.serial.as_deref() != Some("TESTPHONE")).cloned().collect();
        h.app.tx.send(crate::loader::Msg::Devices(left)).unwrap();
        h.until(|a| !a.phone_copy_modal(), "the window no longer modal");
        frames(&mut h);
        assert!(h.app.transfer.is_some() && h.ctx.memory(|m| m.top_modal_layer()).is_none(), "the copy box in the corner, no modal");
    }
}
