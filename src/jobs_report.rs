//! One report when several jobs in the corner boxes end (#328), and messages that wait for the
//! one open instead of taking its place (#329).

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
}
