//! The window's ✕ while a copy runs. As always it cancels the copy and EyeCanSee closes once the
//! file under way is finished (closed in the middle, a phone kept half an operation). But a phone
//! that copies or moves a file by itself cannot be stopped, and the wait for it can last minutes,
//! or never end when Windows loses the phone: the ✕ closed nothing then, and the program had to
//! be closed by force (#354, prova W5 on Windows, 27/11). So while the phone works alone the ✕
//! asks: "Close EyeCanSee" stops the wait (`mtp::leave`) and closes at once (Piero, 27/11; the
//! phone finishes its file by itself); `AT_MOST` stays as the last resort.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::{App, confirm, mtp};

/// The id of the question's modal.
pub(crate) const ID: &str = "quit-phone";

/// After "Close EyeCanSee" the program closes even with the copy still there: a look at the
/// phone under way is given up after 30 s on Linux, and on Windows a call can stay for minutes.
const AT_MOST: Duration = Duration::from_secs(40);

/// What the ✕ does while a copy runs.
#[derive(Debug, PartialEq)]
pub(crate) enum Close {
    /// The copy is cancelled, and EyeCanSee closes at its end.
    AtEnd,
    /// The phone works alone: asked first.
    Ask,
    /// Asked and answered already: the window closes now.
    Now,
}

/// The ✕ during a copy: `working`, a phone has been copying or moving a file by itself for some
/// seconds (`mtp::working_long`: a small file is done before, and the copy stops at its end);
/// `confirmed`, "Close EyeCanSee" was answered already.
pub(crate) fn on_close(working: bool, confirmed: bool) -> Close {
    match (confirmed, working) {
        (true, _) => Close::Now,
        (false, true) => Close::Ask,
        (false, false) => Close::AtEnd,
    }
}

/// The question of the ✕ during a copy the phone makes alone, and its answer.
#[derive(Default)]
pub(crate) struct Quit {
    /// The question is open: what the phone was doing ("copy" or "move") and the file.
    pub asking: Option<(String, String)>,
    /// "Close EyeCanSee" answered: when.
    pub confirmed: Option<Instant>,
}

impl App {
    /// Every frame, before anything else looks at the ✕ (`before_frame`): the ✕ during a copy
    /// (`on_close`), and the close that comes `AT_MOST` after the answer.
    pub(crate) fn close_during_copy(&mut self, ctx: &egui::Context) {
        let Some((p, ..)) = &self.transfer else {
            self.quit_phone = Quit::default();
            return;
        };
        let asked = ctx.input(|i| i.viewport().close_requested());
        if !asked {
            if self.quit_phone.confirmed.is_some_and(|at| at.elapsed() >= AT_MOST) {
                log!("close during a copy: the copy has not let go in {} s, closing now", AT_MOST.as_secs());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        let working = mtp::working_long();
        match on_close(working.is_some(), self.quit_phone.confirmed.is_some()) {
            Close::Now => log!("close during a copy: asked again after the answer, closing now"),
            Close::Ask => {
                log!("close during a copy the phone makes alone: asking first");
                self.quit_phone.asking = working;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            // Closed in the middle, a phone kept half an operation and the next session got its
            // leftovers ("Transaction ID mismatch", until it was plugged in again).
            Close::AtEnd => {
                log!("close during a copy: cancelling it first");
                p.cancel.store(true, Ordering::Relaxed);
                self.quit_after_transfer = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
    }

    /// The question of the ✕ while the phone works alone. "Close EyeCanSee": the copy is
    /// cancelled, the wait for the phone left, and the program closes when the copy lets go.
    /// "Keep waiting" (the one highlighted, and Esc): nothing changes.
    pub(crate) fn quit_phone_window(&mut self, ctx: &egui::Context) {
        let (Some((p, ..)), Some((what, name))) = (&self.transfer, &self.quit_phone.asking) else { return };
        let text = self.i18n.t_args("quit-phone-ask", &[("what", what), ("name", &crate::save_dialogs::md(name))]);
        let (yes, no) = (self.i18n.t("quit-phone-yes"), self.i18n.t("quit-phone-no"));
        let answer = confirm::answer(ctx, &mut self.key_answer, ID);
        match confirm::yes_no(ctx, ID, 460.0, (self.i18n.t("quit-phone-title"), text), yes, true, no, answer, true) {
            Some(true) => {
                log!("close during a copy the phone makes alone: closing, the wait for the phone left");
                p.cancel.store(true, Ordering::Relaxed);
                mtp::leave();
                self.quit_after_transfer = true;
                self.quit_phone = Quit { asking: None, confirmed: Some(Instant::now()) };
                // At once, as the question says (Piero, 27/11: it went on copying up to `AT_MOST`).
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(false) => self.quit_phone.asking = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use eframe::egui::{Key, ViewportCommand};

    use super::*;
    use crate::audit_family_1::{Harness, folder, own_folder};

    #[test]
    fn what_the_x_does_during_a_copy() {
        assert_eq!(on_close(false, false), Close::AtEnd, "a copy as any other, or a small file the phone ends at once: cancelled, closed at its end");
        assert_eq!(on_close(true, false), Close::Ask, "the phone works alone");
        assert_eq!(on_close(true, true), Close::Now, "asked and answered already");
        assert_eq!(on_close(false, true), Close::Now);
    }

    /// The app with a paste the phone copies inside one storage, one file of 6 GB under way.
    fn pasting(name: &str) -> (Harness, Arc<crate::transfer::Progress>) {
        let mut h = Harness::new(&folder(name, &["a.png"]));
        let p = Arc::new(crate::transfer::Progress::default());
        p.by_phone.decide(true, false);
        p.files_total.store(1, Ordering::Relaxed);
        p.total.store(6 << 30, Ordering::Relaxed);
        p.by_phone.begin("big.mp4", 6 << 30);
        let _ = p.started.set(Instant::now());
        h.app.transfer = Some((p.clone(), false, "mtp://TESTPHONE/".into(), "Internal storage/Download/".into()));
        h.frame(Vec::new());
        (h, p)
    }

    /// #354 (prova W5 on Windows, 27/11): while EyeCanSee waited for the phone to end a copy the
    /// ✕ only cancelled the copy and waited for its end, which did not come: the program was
    /// closed by force. While the phone works alone the ✕ asks. "Keep waiting" (highlighted) and
    /// Esc change nothing; "Close EyeCanSee" cancels the copy, leaves the wait for the phone and
    /// closes at once, without waiting for the copy (a second ✕ too). With a copy as any other the
    /// ✕ cancels and waits, as before.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn the_x_asks_while_the_phone_copies_alone() {
        if own_folder("quit_phone::tests::the_x_asks_while_the_phone_copies_alone") {
            return;
        }
        crate::mtp::stay();
        let (mut h, p) = pasting("quit-phone-other");
        assert!(!h.close() && h.app.quit_phone.asking.is_none(), "no phone at work: no question, and the window stays");
        assert!(p.cancel.load(Ordering::Relaxed) && h.app.quit_after_transfer, "the copy cancelled, closing at its end");

        let (mut h, p) = pasting("quit-phone");
        // A file the phone has just been given (a photo is done in less): no question either.
        let just = crate::mtp::copying_for_tests("Internal storage/DCIM/small.jpg", 0);
        assert!(!h.close() && h.app.quit_phone.asking.is_none(), "a file just given to the phone: no question");
        assert!(p.cancel.load(Ordering::Relaxed) && h.app.quit_after_transfer, "the copy cancelled, closing at its end");
        drop(just);
        p.cancel.store(false, Ordering::Relaxed);
        h.app.quit_after_transfer = false;
        let _working = crate::mtp::copying_for_tests("Internal storage/DCIM/big.mp4", 40);
        let state = |h: &Harness, p: &crate::transfer::Progress| (h.app.quit_phone.asking.is_some(), p.cancel.load(Ordering::Relaxed), crate::mtp::leaving(), h.app.quit_after_transfer);
        assert!(!h.close(), "the window closed with the phone at work");
        assert_eq!(state(&h, &p), (true, false, false, false), "the ✕ asks, and nothing else yet");
        h.frame(Vec::new());
        assert!(h.ctx.memory(|m| m.top_modal_layer()).is_some_and(crate::window_keys::modal(ID)), "the question is not on top of the copy window");
        // Enter presses the button highlighted, Keep waiting; Esc the same.
        for key in [Key::Enter, Key::Escape] {
            h.key(key);
            h.frame(Vec::new());
            assert_eq!(state(&h, &p), (false, false, false, false), "{key:?}: keep waiting");
            assert!(!h.close());
            h.frame(Vec::new());
        }
        // Close EyeCanSee, the other button: closed at once (Piero, 27/11), in the frame of the
        // answer or the one after it (`Harness::commands` holds a frame's only; audit voce 34 of
        // 27/11: the test looked at the frame after and passed proving nothing).
        h.key(Key::ArrowLeft);
        h.key(Key::Enter);
        let mut sent = h.commands.clone();
        h.frame(Vec::new());
        sent.extend(h.commands.iter().cloned());
        assert_eq!(state(&h, &p), (false, true, true, true), "Close EyeCanSee: the copy cancelled, the wait left, closing");
        assert!(sent.contains(&ViewportCommand::Close), "Close EyeCanSee did not close at once: {sent:?}");
        // The close that follows is not held back for the copy.
        assert!(h.close(), "the window did not close after Close EyeCanSee");
        // The time is up with the copy still there: the window is told to close.
        h.app.quit_phone.confirmed = Instant::now().checked_sub(AT_MOST);
        h.frame(Vec::new());
        assert!(h.commands.contains(&ViewportCommand::Close), "not closed {} s after the answer", AT_MOST.as_secs());
        assert!(h.close(), "the ✕ after the answer did not close the window");
        crate::mtp::stay();
    }
}
