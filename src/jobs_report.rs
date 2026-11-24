//! One report when several jobs in the corner boxes end (#328), and messages that wait for the
//! one open instead of taking its place (#329).

use std::sync::atomic::Ordering;

use eframe::egui;

use crate::App;
use crate::batch_summary::{Group, Summary, group_ui};
use crate::save_dialogs::{Head, Notice, copy_button, md, notice_text, plain};

/// What a job said at its end, as a part of the report: its title in bold, its text and its
/// lists, what Copy takes, and the guide's page it names.
#[derive(Clone)]
pub(crate) struct Part {
    pub(crate) title: String,
    pub(crate) text: String,
    pub(crate) groups: Vec<Group>,
    pub(crate) copy: String,
    pub(crate) warning: bool,
    pub(crate) help: Option<&'static str>,
}

impl Part {
    pub(crate) fn of_notice(n: &Notice) -> Self {
        let copy = n.copy.clone().unwrap_or_else(|| plain(&n.text));
        Part { title: n.title.clone(), text: n.text.clone(), groups: Vec::new(), copy, warning: n.warning, help: n.help }
    }

    pub(crate) fn of_summary(s: &Summary) -> Self {
        Part { title: s.title.clone(), text: s.text.clone(), groups: s.groups.clone(), copy: s.copy.clone(), warning: false, help: Some("help:convert") }
    }
}

/// The report of several jobs, until closed; `open`: Convert's folder, for Open the folder.
#[derive(Clone)]
pub(crate) struct Report {
    pub(crate) parts: Vec<Part>,
    pub(crate) open: Option<String>,
}

impl Report {
    /// What Copy takes: each part under its title, every name included.
    pub(crate) fn copied(&self) -> String {
        self.parts.iter().map(|p| format!("{}\n{}", p.title, p.copy)).collect::<Vec<_>>().join("\n\n")
    }

    /// The guide's page for F1: the copy's when a part names it, else the first a part gives.
    pub(crate) fn help(&self) -> &'static str {
        let named = self.parts.iter().filter_map(|p| p.help);
        named.clone().find(|h| *h == "help:copy-move").or(named.clone().next()).unwrap_or("help:copy-move")
    }
}

/// A window that waits for the one open to close (#329).
pub(crate) enum Waiting {
    Notice(Notice),
    Summary(Summary),
    Report(Report),
}

impl Waiting {
    /// A job's report, which another job's part joins.
    fn join(&mut self, part: Part, open: Option<String>) -> bool {
        let report = match self {
            Waiting::Report(r) => r,
            Waiting::Summary(s) => {
                *self = Waiting::Report(Report { parts: vec![Part::of_summary(s)], open: s.open.clone() });
                let Waiting::Report(r) = self else { unreachable!() };
                r
            }
            Waiting::Notice(n) if n.job => {
                *self = Waiting::Report(Report { parts: vec![Part::of_notice(n)], open: None });
                let Waiting::Report(r) = self else { unreachable!() };
                r
            }
            Waiting::Notice(_) => return false,
        };
        report.parts.push(part);
        report.open = report.open.take().or(open);
        true
    }
}

/// The part of a copy that ended with nothing to say alone, for a report with more jobs:
/// cancelled, finished, or stopped by an error the red bar says (`r`); `cut`: a move.
pub(crate) fn quiet_copy(i18n: &crate::i18n::I18n, cut: bool, to: &str, r: &Result<(), crate::source::Error>, p: &crate::transfer::Progress) -> Part {
    let (files, total) = (p.files.load(Ordering::Relaxed), p.files_total.load(Ordering::Relaxed));
    let dir = md(&crate::address::without_password(to));
    let to_line = || i18n.t_args("transfer-to", &[("dir", &dir)]);
    // One line, as Convert's "3 files saved in …" (Piero, 24/11): the count and the folder.
    let counted = |id: &str, n: usize| i18n.t_args(id, &[("n", &n.to_string()), ("total", &total.to_string()), ("dir", &dir)]);
    let (title, text) = match r {
        Err((id, _)) if *id == "error-cancelled" => {
            let text = if total > 0 { counted(if cut { "transfer-moved-before-cancel" } else { "transfer-done-before-cancel" }, files) } else { to_line() };
            (if cut { "transfer-move-cancelled" } else { "transfer-copy-cancelled" }, text)
        }
        Ok(()) => {
            let text = if total > 0 { counted(if cut { "transfer-files-moved" } else { "transfer-files-copied" }, total) } else { to_line() };
            (if cut { "transfer-moved" } else { "transfer-copied" }, text)
        }
        Err((id, detail)) => (if cut { "transfer-move-stopped" } else { "transfer-copy-stopped" }, format!("{}\n{}", to_line(), md(&i18n.error(id, detail)))),
    };
    Part { title: i18n.t(title), copy: plain(&text), text, groups: Vec::new(), warning: false, help: None }
}

impl App {
    /// A message, a job's report or a report, or the window open (a message, a report) is
    /// already up: it waits (#329).
    fn reporting(&self) -> bool {
        self.notice.is_some() || self.batch_summary.is_some() || self.jobs_report.is_some()
    }

    /// A message from something that ended in the background (a batch rotation, an export): it
    /// waits while another message is open, instead of taking its place (#329).
    pub(crate) fn tell_notice(&mut self, n: Notice) {
        self.show_or_wait(Waiting::Notice(n));
    }

    fn show_or_wait(&mut self, w: Waiting) {
        if self.reporting() {
            self.waiting.push_back(w);
            return;
        }
        match w {
            Waiting::Notice(n) => self.notice = Some(n),
            Waiting::Summary(s) => self.batch_summary = Some(s),
            Waiting::Report(r) => self.jobs_report = Some(r),
        }
    }

    /// The window closed: the next one waiting comes.
    pub(crate) fn next_waiting(&mut self) {
        if !self.reporting()
            && let Some(w) = self.waiting.pop_front()
        {
            self.show_or_wait(w);
        }
    }

    /// A job ended with `part` to say (`alone`: its window when no other job's report is up):
    /// it joins the job report open or waiting; else the copies remembered come with it.
    pub(crate) fn job_ended(&mut self, part: Part, open: Option<String>, alone: Waiting) {
        if self.join_report(part.clone(), open.clone()) {
            return;
        }
        if self.jobs_quiet.is_empty() {
            self.show_or_wait(alone);
        } else {
            let mut parts = std::mem::take(&mut self.jobs_quiet);
            parts.push(part);
            self.show_or_wait(Waiting::Report(Report { parts, open }));
        }
    }

    /// A copy ended with nothing to say alone (`part`): in the job report up, else remembered
    /// while another job runs, else nothing, as before.
    pub(crate) fn quiet_job(&mut self, part: Part) {
        if self.join_report(part.clone(), None) {
            return;
        }
        if self.batch.as_ref().is_some_and(crate::batch_page::Window::running) {
            self.jobs_quiet.push(part);
        } else {
            self.jobs_quiet.clear();
        }
    }

    /// `part` added to the job report open (or to a job's report open, which becomes one), or
    /// to one waiting behind a message.
    fn join_report(&mut self, part: Part, open: Option<String>) -> bool {
        let up = if let Some(r) = self.jobs_report.take() {
            Some(Waiting::Report(r))
        } else if let Some(s) = self.batch_summary.take() {
            Some(Waiting::Summary(s))
        } else if self.notice.as_ref().is_some_and(|n| n.job) {
            self.notice.take().map(Waiting::Notice)
        } else {
            None
        };
        if let Some(mut w) = up {
            w.join(part, open);
            let Waiting::Report(r) = w else { unreachable!("joined is a report") };
            self.jobs_report = Some(r);
            return true;
        }
        self.waiting.iter_mut().any(|w| w.join(part.clone(), open.clone()))
    }

    /// The report of several jobs, until closed (Close, Enter or Esc close it all).
    pub(crate) fn jobs_report_window(&mut self, ctx: &egui::Context) {
        let Some(r) = self.jobs_report.take() else { return };
        let id = "jobs-report";
        let mut close = crate::confirm::answer(ctx, &mut self.key_answer, id) == Some(false);
        let mut go = None;
        let covered = self.show.is_some() || self.convert.is_some() || self.editor.is_some() || self.dup_page_shown();
        let t = |id| self.i18n.t(id);
        let (title, open_label, covered_hint, close_label, tip) = (t("jobs-report-title"), t("summary-open-folder"), t("summary-open-covered"), t("notice-close"), t("notice-copy"));
        let warning = r.parts.iter().any(|p| p.warning);
        egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.set_min_width(320.0);
            Head { title: &title, warning }.show(ui);
            let max = ctx.content_rect().height() * 0.55;
            use crate::confirm::FollowArea;
            egui::ScrollArea::vertical().max_height(max).auto_shrink([false, true]).show_follow(ui, |ui| {
                for (i, p) in r.parts.iter().enumerate() {
                    // A line between the parts, and each title in bold as the headings of the
                    // lists (Piero, 24/11: `strong` only darkened it).
                    if i > 0 {
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(6.0);
                    }
                    notice_text(ui, &format!("**{}**", md(&p.title)));
                    ui.add_space(2.0);
                    notice_text(ui, &p.text);
                    p.groups.iter().for_each(|g| group_ui(ui, g, None));
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let mut labels = vec![(close_label.clone(), true)];
                if r.open.is_some() {
                    labels.push((open_label.clone(), !covered));
                }
                match crate::confirm::buttons_hint(ui, id, &labels, 0, &[None, Some(covered_hint.clone())]) {
                    Some(0) => close = true,
                    Some(_) => go = r.open.clone(),
                    None => {}
                }
            });
            copy_button(ui, id, &tip, || r.copied());
        });
        if let Some(address) = go {
            log!("jobs report: Open the folder {:?}", crate::address::without_password(&address));
            self.open_folder_after_batch(ctx, address);
            close = true;
        }
        if close {
            log!("jobs report: closed");
            self.next_waiting();
        } else {
            self.jobs_report = Some(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use crate::audit_family_1::{Harness, folder, own_folder};
    use crate::loader::Msg;

    fn message(title: &str) -> crate::save_dialogs::Notice {
        crate::save_dialogs::Notice::new(title.into(), "Read me".into())
    }

    /// A copy into the folder open ends as `r`, with a link left out when `say` (something to
    /// say alone).
    pub(crate) fn copy_ends(h: &mut Harness, say: bool, r: Result<(), crate::source::Error>) -> Arc<crate::transfer::Progress> {
        let p = Arc::new(crate::transfer::Progress::default());
        if say {
            p.links_skipped.lock().unwrap().push("collegamenti/self".to_string());
        }
        h.app.transfer = Some((p.clone(), false, h.app.base.clone(), h.app.dir.clone()));
        h.frame(Vec::new());
        h.app.tx.send(Msg::Pasted(Vec::new(), r)).unwrap();
        h.frame(Vec::new());
        h.frame(Vec::new());
        p
    }

    /// A Convert starts on the first file; `end` ends it.
    pub(crate) fn convert_runs(h: &mut Harness) -> Arc<crate::batch::Progress> {
        let (w, p) = crate::batch_page::running(&h.app.dir.clone(), vec![format!("{}a.png", h.app.dir)]);
        h.app.batch = Some(w);
        h.frame(Vec::new());
        p
    }

    pub(crate) fn convert_ends(h: &mut Harness, p: &crate::batch::Progress) {
        p.finished.store(true, Ordering::Relaxed);
        h.frame(Vec::new());
        h.frame(Vec::new());
    }

    /// #329: the end of a batch rotation does not take the place of a message open.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_rotation_ending_keeps_the_message_open() {
        if own_folder("jobs_report::tests::a_rotation_ending_keeps_the_message_open") {
            return;
        }
        let mut h = Harness::new(&folder("rotation-over-message", &["a.png"]));
        h.app.notice = Some(message("First message"));
        h.app.batch_saved = Some((crate::transform::Op::Cw, 0, 0));
        h.app.tx.send(Msg::Replaced(format!("{}a.png", h.app.dir), Err((crate::loader::SKIPPED, "a.png".into())))).unwrap();
        h.frame(Vec::new());
        h.frame(Vec::new());
        assert_eq!(h.app.notice.as_ref().map(|n| n.title.as_str()), Some("First message"), "the end of the rotation replaced the message open");
    }

    /// #329: the end of a model export does not take the place of a message open.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn an_export_ending_keeps_the_message_open() {
        if own_folder("jobs_report::tests::an_export_ending_keeps_the_message_open") {
            return;
        }
        let mut h = Harness::new(&folder("export-over-message", &["a.png"]));
        h.app.notice = Some(message("First message"));
        h.app.export_said.start(1);
        h.app.tx.send(Msg::Exported(Err(("a.obj".into(), "boom".into())))).unwrap();
        h.frame(Vec::new());
        h.frame(Vec::new());
        assert_eq!(h.app.notice.as_ref().map(|n| n.title.as_str()), Some("First message"), "the end of the export replaced the message open");
    }

    /// #328: a copy with something to say and a Convert that end together give one window, in
    /// both orders, not one over the other.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_copy_and_a_convert_end_in_one_window() {
        if own_folder("jobs_report::tests::a_copy_and_a_convert_end_in_one_window") {
            return;
        }
        for convert_first in [false, true] {
            let mut h = Harness::new(&folder(if convert_first { "jobs-convert-copy" } else { "jobs-copy-convert" }, &["a.png"]));
            let p = convert_runs(&mut h);
            if convert_first {
                convert_ends(&mut h, &p);
                copy_ends(&mut h, true, Ok(()));
            } else {
                copy_ends(&mut h, true, Ok(()));
                convert_ends(&mut h, &p);
            }
            let open = [h.app.notice.is_some(), h.app.batch_summary.is_some()];
            assert!(open != [true, true], "convert first {convert_first}: two windows, the copy's and Convert's");
        }
    }

    /// #328: Enter closes the whole report of two jobs, and nothing is left under it; the
    /// report has a part for each, in the order they ended, under "Jobs finished".
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn one_report_for_two_jobs_closed_at_once() {
        if own_folder("jobs_report::tests::one_report_for_two_jobs_closed_at_once") {
            return;
        }
        let mut h = Harness::new(&folder("jobs-enter", &["a.png"]));
        let p = convert_runs(&mut h);
        convert_ends(&mut h, &p);
        copy_ends(&mut h, true, Ok(()));
        let r = h.app.jobs_report.clone().expect("no jobs report");
        let titles: Vec<&str> = r.parts.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["Converti", "Copia finita"], "the parts and their order");
        assert!(r.copied().contains("Converti\n") && r.copied().contains("collegamenti/self"), "{}", r.copied());
        assert_eq!(h.app.help_spot(&h.ctx), "help:copy-move", "F1: the copy's page, which a part names");
        crate::window_keys::tab_goes_through(&mut h, crate::window_keys::modal("jobs-report"), |a| a.jobs_report.is_some(), 1, true);
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        assert!(h.app.jobs_report.is_none() && h.app.notice.is_none() && h.app.batch_summary.is_none(), "Enter left a window");
    }

    /// #328: a copy that ends quietly while a Convert runs shows nothing, then is at the top of
    /// Convert's report; cancelled, it says so with what was done before Cancel.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_quiet_copy_joins_the_next_report() {
        if own_folder("jobs_report::tests::a_quiet_copy_joins_the_next_report") {
            return;
        }
        let mut h = Harness::new(&folder("jobs-quiet", &["a.png"]));
        let p = convert_runs(&mut h);
        let copy = copy_ends(&mut h, false, Ok(()));
        assert!(h.app.notice.is_none() && h.app.jobs_report.is_none(), "a quiet copy opened a window");
        drop(copy);
        let cancelled = {
            let p = Arc::new(crate::transfer::Progress::default());
            p.files.store(2, Ordering::Relaxed);
            p.files_total.store(5, Ordering::Relaxed);
            p
        };
        h.app.transfer = Some((cancelled, false, h.app.base.clone(), h.app.dir.clone()));
        h.frame(Vec::new());
        h.app.tx.send(Msg::Pasted(Vec::new(), Err(("error-cancelled", String::new())))).unwrap();
        h.frame(Vec::new());
        p.cancel.store(true, Ordering::Relaxed);
        convert_ends(&mut h, &p);
        let r = h.app.jobs_report.clone().expect("no jobs report");
        let titles: Vec<&str> = r.parts.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["Copia finita", "Copia annullata", "Converti"], "the quiet copies at the top");
        assert!(r.parts[1].text.starts_with("**2 file su 5** copiati in ") && r.parts[1].text.ends_with(" prima di Annulla."), "{}", r.parts[1].text);
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        assert!(h.app.jobs_quiet.is_empty() && h.app.jobs_report.is_none());
    }

    /// #328: with the first report closed, the job that ends later has its own, as before; a
    /// quiet copy with no other job running says nothing and is not kept.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_report_closed_leaves_the_next_job_alone() {
        if own_folder("jobs_report::tests::a_report_closed_leaves_the_next_job_alone") {
            return;
        }
        let mut h = Harness::new(&folder("jobs-alone", &["a.png"]));
        let p = convert_runs(&mut h);
        convert_ends(&mut h, &p);
        assert!(h.app.batch_summary.is_some() && h.app.jobs_report.is_none(), "one job: Convert's summary as before");
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        copy_ends(&mut h, true, Ok(()));
        assert!(h.app.notice.as_ref().is_some_and(|n| n.title == "Copia finita") && h.app.jobs_report.is_none(), "the copy's message alone");
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        copy_ends(&mut h, false, Ok(()));
        assert!(h.app.notice.is_none() && h.app.jobs_quiet.is_empty(), "a quiet copy alone said something, or was kept");
    }

    /// #329: a message that comes while another is open waits and shows when the first is
    /// closed; Enter closes one at a time.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_message_waits_for_the_one_open() {
        if own_folder("jobs_report::tests::a_message_waits_for_the_one_open") {
            return;
        }
        let mut h = Harness::new(&folder("jobs-wait", &["a.png"]));
        h.app.notice = Some(message("First message"));
        copy_ends(&mut h, true, Ok(()));
        h.app.tell_notice(message("Third message"));
        h.frame(Vec::new());
        let shown = |h: &Harness| h.app.notice.as_ref().map(|n| n.title.clone());
        assert_eq!(shown(&h).as_deref(), Some("First message"));
        for next in ["Copia finita", "Third message"] {
            h.key(egui::Key::Enter);
            h.frame(Vec::new());
            assert_eq!(shown(&h).as_deref(), Some(next), "after Enter");
        }
        h.key(egui::Key::Enter);
        h.frame(Vec::new());
        assert!(h.app.notice.is_none() && h.app.waiting.is_empty());
    }

    /// The part of a quiet copy, in English and Italian: finished, moved, cancelled, stopped.
    #[test]
    fn quiet_copy_says_what_happened() {
        use super::quiet_copy;
        let p = crate::transfer::Progress::default();
        p.files.store(1, Ordering::Relaxed);
        p.files_total.store(3, Ordering::Relaxed);
        for (lang, done, moved, cancelled) in [
            ("en", "Copy finished|3 files copied to /to/.", "Move finished|3 files moved to /to/.", "Copy cancelled|1 of 3 files copied to /to/ before Cancel."),
            ("it", "Copia finita|3 file copiati in /to/.", "Spostamento finito|3 file spostati in /to/.", "Copia annullata|1 file su 3 copiati in /to/ prima di Annulla."),
        ] {
            let i18n = crate::i18n::I18n::new(lang);
            let said = |cut, r: Result<(), crate::source::Error>| {
                let part = quiet_copy(&i18n, cut, "/to/", &r, &p);
                format!("{}|{}", part.title, part.copy)
            };
            assert_eq!(said(false, Ok(())), done);
            assert_eq!(said(true, Ok(())), moved);
            assert_eq!(said(false, Err(("error-cancelled", String::new()))), cancelled);
            assert!(said(false, Err(("error-no-host", "nas".into()))).starts_with(&i18n.t("transfer-copy-stopped")));
        }
    }
}
