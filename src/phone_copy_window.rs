//! The copy window while the phone copies by itself inside one storage (#354): a modal in the
//! middle of the app for the whole paste, since the phone answers nothing meanwhile (Piero, 26/11
//! night). Everything under it waits, the computer's folders too. Only what is known is shown
//! (`transfer::lines`). Cancel stops between files; Esc does nothing (the window cannot close
//! before the phone is done); Enter and Space press Cancel only once Tab has reached it.

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::human::human_time;
use crate::{App, theme, transfer};

/// The id of its modal.
pub(crate) const ID: &str = "phone-copy";

impl App {
    /// The paste under way is one the phone copies by itself inside one storage, its files
    /// counted, and its phone still among the devices: the copy window is this modal.
    pub(crate) fn phone_copy_modal(&self) -> bool {
        self.transfer.as_ref().is_some_and(|(p, ..)| p.by_phone.modal() && matches!(p.phase(), transfer::Phase::Copying { .. }))
    }

    /// What the window knows now.
    fn phone_copy_seen(p: &transfer::Progress) -> transfer::Seen {
        transfer::Seen {
            file: p.by_phone.file(),
            files: p.files.load(Ordering::Relaxed),
            files_total: p.files_total.load(Ordering::Relaxed),
            done: p.done.load(Ordering::Relaxed),
            total: p.total.load(Ordering::Relaxed),
            big: p.by_phone.big(),
            cancelling: p.cancel.load(Ordering::Relaxed),
        }
    }

    /// The words of a line, or the part of the bar.
    fn phone_copy_text(&self, line: &transfer::Line) -> Result<String, f32> {
        let size = |b: u64| self.human_size(b);
        Ok(match line {
            transfer::Line::File { name, size: s, secs } => self.i18n.t_args("phone-copy-file", &[("name", name), ("size", &size(*s)), ("s", &secs.to_string())]),
            transfer::Line::Counts { n, total, done, size: s } => {
                self.i18n.t_args("phone-copy-counts", &[("n", &n.to_string()), ("total", &total.to_string()), ("done", &size(*done)), ("size", &size(*s))])
            }
            transfer::Line::Bar(part) => return Err(*part),
            transfer::Line::NoBar => self.i18n.t("phone-copy-one"),
            transfer::Line::Speed { speed, left } => self.i18n.t_args("phone-copy-speed", &[("speed", &size(*speed)), ("time", &human_time(*left))]),
            transfer::Line::Long => self.i18n.t("phone-copy-long"),
            transfer::Line::Stopping(name) => self.i18n.t_args("phone-copy-stopping", &[("name", name)]),
        })
    }

    /// The modal (`phone_copy_modal`), drawn in place of the copy box.
    pub(crate) fn phone_copy_window(&mut self, ctx: &egui::Context) {
        let Some((p, ..)) = &self.transfer else { return };
        let p = p.clone();
        let lines: Vec<Result<String, f32>> = transfer::lines(&Self::phone_copy_seen(&p)).iter().map(|l| self.phone_copy_text(l)).collect();
        let (title, notice, cancel, why) = (self.i18n.t("phone-copy-title"), self.i18n.t("phone-copy-notice"), self.i18n.t("delete-no"), self.i18n.t("phone-copy-why"));
        let cancelling = p.cancel.load(Ordering::Relaxed);
        // Esc closes nothing here: taken, so it answers no window under this one later.
        let _ = crate::confirm::answer(ctx, &mut self.key_answer, ID);
        let mut help = false;
        egui::Modal::new(egui::Id::unique(ID)).show(ctx, |ui| {
            ui.set_width(440.0);
            crate::save_dialogs::Head::new(&title).show(ui);
            for line in lines {
                match line {
                    Ok(text) => {
                        ui.label(text);
                    }
                    Err(part) => bar(ui, part),
                }
            }
            ui.add_space(6.0);
            ui.label(notice);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let stop = ui.add_enabled(!cancelling, egui::Button::new(cancel));
                let link = ui.link(why);
                crate::confirm::join(ID, &link);
                // No button highlighted at first: a habitual Enter cancels nothing; Tab reaches them.
                if crate::confirm::keys(ui, ID, std::slice::from_ref(&stop), usize::MAX) == Some(0) {
                    p.cancel.store(true, Ordering::Relaxed);
                }
                help = link.clicked();
            });
        });
        if help {
            let title = self.i18n.t("phone-copy-why-section");
            self.open_help_at("phone", &title);
        }
    }
}

/// The bar of the files done, as in the copy box.
fn bar(ui: &mut egui::Ui, part: f32) {
    ui.scope(|ui| {
        ui.visuals_mut().extreme_bg_color = ui.visuals().widgets.inactive.bg_fill;
        let fill = theme::accent(ui.visuals());
        ui.add(egui::ProgressBar::new(part).fill(fill).desired_height(16.0).show_percentage());
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use eframe::egui::{self, Key, Modifiers};

    use crate::App;
    use crate::audit_family_1::{Harness, folder, own_folder};

    const PHONE: &str = "mtp://TESTPHONE/";

    /// The app on a folder of three pictures, the grid on the last one, and a paste the phone
    /// copies inside one storage set on it directly (no phone): its files counted, the second of
    /// three under way. The phone is not among the devices: a list from the devices' watcher,
    /// arriving late, would take it away.
    fn copying(name: &str) -> (Harness, Arc<crate::transfer::Progress>) {
        let mut h = Harness::new(&folder(name, &["a.png", "b.png", "c.png"]));
        (h.app.cursor, h.app.selected, h.app.focus) = (true, 2, crate::Focus::Grid);
        let p = Arc::new(crate::transfer::Progress::default());
        p.by_phone.decide(true);
        p.files.store(1, Ordering::Relaxed);
        p.files_total.store(3, Ordering::Relaxed);
        p.total.store(3 << 30, Ordering::Relaxed);
        p.by_phone.begin("big.mp4", 1 << 30);
        let _ = p.started.set(std::time::Instant::now());
        h.app.transfer = Some((p.clone(), false, PHONE.into(), "Internal storage/Download/".into()));
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        (h, p)
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
        let (mut h, p) = copying("phone-copy-modal");
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(super::ID));
        assert_eq!(h.ctx.memory(|m| m.top_modal_layer()), Some(layer), "the window is the modal on top");
        let under = |a: &App| format!("{:?}", (a.cursor, a.selected, a.focus, a.viewing, &a.dir, a.clip.is_some(), a.delete_confirm.is_some(), a.rename.is_some(), a.editor.is_some()));
        let before = under(&h.app);
        let grid = h.app.grid_rect.expect("the grid drawn");
        let first = grid.min + egui::vec2(40.0, 40.0);
        double_click(&mut h, first);
        assert_eq!(under(&h.app), before, "a double click reached the grid under the window");
        for (key, modifiers) in [(Key::Enter, Modifiers::NONE), (Key::Space, Modifiers::NONE), (Key::Escape, Modifiers::NONE), (Key::ArrowLeft, Modifiers::NONE), (Key::Home, Modifiers::NONE), (Key::Delete, Modifiers::NONE), (Key::F2, Modifiers::NONE), (Key::Backspace, Modifiers::NONE), (Key::A, Modifiers::COMMAND), (Key::E, Modifiers::COMMAND)] {
            h.blur();
            h.key_with(key, modifiers);
            h.frame(Vec::new());
            assert!(open(&h.app), "{key:?} {modifiers:?} closed the window");
            assert!(!p.cancel.load(Ordering::Relaxed), "{key:?} {modifiers:?} with nothing highlighted cancelled the copy");
            assert_eq!(under(&h.app), before, "{key:?} {modifiers:?} reached what is under the window");
        }
        crate::window_keys::tab_goes_through(&mut h, crate::window_keys::modal(super::ID), open, 2, true);
        // Tab to Cancel, the first control on screen, then Enter: the copy stops after the file
        // under way, the window stays.
        h.blur();
        h.frame(Vec::new());
        h.key(Key::Tab);
        h.key(Key::Enter);
        h.frame(Vec::new());
        assert!(p.cancel.load(Ordering::Relaxed) && open(&h.app), "Cancel pressed with the keys, the window staying until the file ends");
        // The control: without the window the same double click reaches the grid.
        h.app.transfer = None;
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        double_click(&mut h, first);
        assert_ne!(under(&h.app), before, "the double click was not on the grid");
    }

    /// #354: the phone gone from the devices during the copy: the window is no longer modal (back to
    /// the corner box until the paste ends), so a phone that is not there never holds the app.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn an_unplugged_phone_lets_the_app_go() {
        if own_folder("phone_copy_window::tests::an_unplugged_phone_lets_the_app_go") {
            return;
        }
        let (mut h, _p) = copying("phone-copy-unplugged");
        assert!(open(&h.app));
        h.app.devices.push(crate::devices::Device { name: "Test phone".into(), path: PHONE.into(), phone: true, serial: Some("TESTPHONE".into()), block: None, internal: false });
        let left: Vec<_> = h.app.devices.iter().filter(|d| d.serial.as_deref() != Some("TESTPHONE")).cloned().collect();
        h.app.tx.send(crate::loader::Msg::Devices(left)).unwrap();
        h.until(|a| !a.phone_copy_modal(), "the window no longer modal");
        for _ in 0..3 {
            h.frame(Vec::new());
        }
        assert!(h.app.transfer.is_some() && h.ctx.memory(|m| m.top_modal_layer()).is_none(), "the copy box in the corner, no modal");
    }
}
