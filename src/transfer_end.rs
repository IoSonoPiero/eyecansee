//! The end of a copy or a move: what its message says (`transfer_notice`, `quiet_copy`), and
//! when it says it (`App::tell_transfer`).

use std::sync::atomic::Ordering;

use crate::jobs_report::Part;
use crate::save_dialogs::{Notice, md, plain};
use crate::{App, source, transfer};

impl App {
    /// The message at the end of a copy or a move into `to` (as the address bar shows it) that
    /// has something to say (`transfer_notice`), in place of the copy box; none when the program
    /// closes after the copy.
    pub(crate) fn tell_transfer(&mut self, p: &transfer::Progress, cut: bool, to: &str, r: &Result<(), source::Error>) {
        if self.quit_after_transfer {
            return;
        }
        // A part of the report of the jobs when another job's is up, or remembered for it (#328);
        // alone it waits behind a message open (#329).
        match transfer_notice(&self.i18n, cut, to, r, p) {
            Some(n) => {
                let n = Notice { job: true, ..n };
                self.job_ended(Part::of_notice(&n), None, crate::jobs_report::Waiting::Notice(n));
            }
            // A long one (a big file the phone moved, minutes) ends with a message, not in silence
            // (Piero, 25/11); a short one quietly, as before.
            None => {
                let part = quiet_copy(&self.i18n, cut, to, r, p);
                if r.is_ok() && p.begun.get().is_some_and(|t| t.elapsed() >= LONG_JOB) {
                    let n = Notice { job: true, ..Notice::new(part.title.clone(), part.text.clone()) };
                    self.job_ended(part, None, crate::jobs_report::Waiting::Notice(n));
                } else {
                    self.quiet_job(part);
                }
            }
        }
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
        // How many files and folders, where; how much, in how long; the items skipped (#356).
        Ok(()) => (if cut { "transfer-moved" } else { "transfer-copied" }, ended_lines(i18n, &Done::of(p, cut), &dir)),
        Err((id, detail)) => (if cut { "transfer-move-stopped" } else { "transfer-copy-stopped" }, format!("{}\n{}", to_line(), md(&i18n.error(id, detail)))),
    };
    Part { title: i18n.t(title), copy: plain(&text), text, groups: Vec::new(), warning: false, help: None }
}

/// What the end of a copy (`cut`: a move) into `to` has to say, all in one window
/// (`notice_list::told`, as the end of an export): the title says the whole ("Copy finished", or
/// "Copy stopped" when it ended at an error, which the error bar says), then where it went, and
/// a part for each thing left out, each under its heading in bold with how many they are, a
/// sentence, and the names in their order. The links to folders left out (#183, only when the
/// copy worked: the log has them otherwise), with the guide's page for F1 (`Notice::help`);
/// the items left out for their name (`transfer::allowed`: #203, #205); what a move left in its
/// folders, arrived while it ran (#194); the files that could not be read (L17 of 2026-11-21),
/// each with why in a few words when it is known, the whole sentence in Copy. It was a long error
/// in the bar (Piero, 20/11: "non si capisce niente"), then a window for each. `None` with
/// nothing to say, or when it was cancelled: the copy box just goes.
pub(crate) fn transfer_notice(i18n: &crate::i18n::I18n, cut: bool, to: &str, r: &Result<(), source::Error>, p: &transfer::Progress) -> Option<Notice> {
    use crate::notice_list::{Entry, Part, told, with_guide};
    if r.as_ref().is_err_and(|(id, _)| *id == "error-cancelled") {
        return None;
    }
    let links = transfer::links_to_say(r, p).unwrap_or_default();
    let (names, left, failed) = (p.refused.lock().unwrap(), p.left.lock().unwrap(), p.failed.lock().unwrap());
    if links.is_empty() && names.is_empty() && left.is_empty() && failed.is_empty() {
        return None;
    }
    let failed = failed.iter().map(|(path, (id, detail))| Entry { name: path.clone(), why: i18n.brief(id, detail).unwrap_or_default(), whole: Some(format!("{path}: {}", i18n.error(id, detail).replace('\n', " "))) });
    let parts = vec![
        Part::named(i18n, "transfer-links-heading", if cut { "transfer-links-why-moved" } else { "transfer-links-why" }, &links),
        Part::named(i18n, "name-refused-heading", "name-refused-why", &names),
        Part::named(i18n, "move-left-heading", "move-left-why", &left),
        Part::of(i18n, "copy-failed-heading", "copy-failed-why", failed.collect()),
    ];
    let title = match (r.is_ok(), cut) {
        (true, false) => "transfer-copied",
        (true, true) => "transfer-moved",
        (false, false) => "transfer-copy-stopped",
        (false, true) => "transfer-move-stopped",
    };
    // On top what a copy that worked says without a list (#356); where it went, after an error.
    let dir = md(&crate::address::without_password(to));
    let to = if r.is_ok() { ended_lines(i18n, &Done::of(p, cut), &dir) } else { i18n.t_args("transfer-to", &[("dir", &dir)]) };
    let told = told(i18n, i18n.t(title), &to, parts);
    Some(if links.is_empty() { told } else { with_guide(i18n, told, "help:copy-move") })
}

/// A copy or a move lasting at least this long says when it ends (`tell_transfer`).
const LONG_JOB: std::time::Duration = std::time::Duration::from_secs(10);

/// What a copy that worked did, for its end message (`ended_lines`).
#[derive(Clone, Copy, Default)]
pub(crate) struct Done {
    pub(crate) cut: bool,
    /// Files copied or moved; not the folders moved whole.
    pub(crate) files: usize,
    /// Folders a move took whole, as a change of place (`transfer::renames`).
    pub(crate) folders: usize,
    /// Items answered Skip, their name already there.
    pub(crate) skipped: usize,
    pub(crate) bytes: u64,
    /// From the start of the paste, looking inside the folders included.
    pub(crate) secs: f64,
}

impl Done {
    pub(crate) fn of(p: &transfer::Progress, cut: bool) -> Self {
        let n = |a: &std::sync::atomic::AtomicUsize| a.load(Ordering::Relaxed);
        let folders = n(&p.folders_moved);
        Done {
            cut,
            files: n(&p.files).saturating_sub(folders),
            folders,
            skipped: n(&p.skipped),
            bytes: p.done.load(Ordering::Relaxed),
            secs: p.begun.get().map_or(0.0, |t| t.elapsed().as_secs_f64()),
        }
    }
}

/// The end of a copy that worked into `dir` (Markdown already), a line each: how many files and
/// folders went there ("To" and the folder when none was counted); how much in how long, how fast
/// on average (Piero, 25/11), from a second and a byte on; the items skipped, when some were.
pub(crate) fn ended_lines(i18n: &crate::i18n::I18n, d: &Done, dir: &str) -> String {
    let n = |v: usize| v.to_string();
    let mut lines = vec![match (d.files, d.folders) {
        (0, 0) => i18n.t_args("transfer-to", &[("dir", dir)]),
        (f, 0) => i18n.t_args(if d.cut { "transfer-files-moved" } else { "transfer-files-copied" }, &[("n", &n(f)), ("dir", dir)]),
        (0, g) => i18n.t_args("transfer-folders-moved", &[("n", &n(g)), ("dir", dir)]),
        (f, g) => i18n.t_args("transfer-files-folders-moved", &[("files", &n(f)), ("folders", &n(g)), ("dir", dir)]),
    }];
    if d.bytes > 0 && d.secs >= 1.0 {
        lines.push(i18n.t_args("transfer-summary", &[
            ("size", &crate::human::size(i18n, d.bytes)),
            ("time", &crate::human::human_time(d.secs.round() as u64)),
            ("speed", &crate::human::size(i18n, (d.bytes as f64 / d.secs) as u64)),
        ]));
    }
    if d.skipped > 0 {
        lines.push(i18n.t_args("transfer-skipped", &[("n", &n(d.skipped))]));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use crate::save_dialogs::{md, plain};
    use crate::source::Error;

    /// The end of a copy (`cut`: a move) that ended as `r`, with links left out, names refused,
    /// items left by a move and files not read.
    fn end(lang: &str, cut: bool, to: &str, r: Result<(), Error>, [links, refused, left]: [&[&str]; 3], failed: &[(&str, Error)]) -> Option<crate::save_dialogs::Notice> {
        let p = crate::transfer::Progress::default();
        let all = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        (*p.links_skipped.lock().unwrap(), *p.refused.lock().unwrap(), *p.left.lock().unwrap()) = (all(links), all(refused), all(left));
        *p.failed.lock().unwrap() = failed.iter().map(|(path, e)| (path.to_string(), e.clone())).collect();
        super::transfer_notice(&crate::i18n::I18n::new(lang), cut, to, &r, &p)
    }

    /// The files a copy could not read (L17 of 2026-11-21): the title says the whole, the heading
    /// counts them, in the singular and the plural; each in its line with why in a few words
    /// when it is known, the name alone otherwise; Copy has the whole sentences, with the
    /// server's answer.
    #[test]
    fn failed_files_told() {
        let cut = |name: &str| (crate::fault::SERVER_CUT, format!("{name}\u{1}550 read error"));
        let one = end("it", false, "/arrivo/", Ok(()), [&[], &[], &[]], &[("album/b.jpg", cut("b.jpg"))]).unwrap();
        assert_eq!(one.title, "Copia finita");
        assert!(one.warning && one.help.is_none());
        assert_eq!(plain(&one.text), "In /arrivo/\n\n1 file non letto:\nNon è stato copiato: è rimasto dove era.\n- album/b.jpg: il server ha smesso di inviarlo prima della fine");
        assert_eq!(one.copy.as_deref(), Some("In /arrivo/\n\n1 file non letto:\nNon è stato copiato: è rimasto dove era.\n- album/b.jpg: Il server ha smesso di inviare \"b.jpg\" prima della fine. (550 read error)"));
        let two = end("en", true, "/in/", Ok(()), [&[], &[], &[]], &[("a/c.jpg", ("error-detail", "timeout".into())), ("a/b.jpg", cut("b.jpg"))]).unwrap();
        assert_eq!(two.title, "Move finished");
        assert_eq!(plain(&two.text), "To /in/\n\n2 files not read:\nThey were not copied: they stayed where they were.\n- a/b.jpg: the server stopped sending it before its end\n- a/c.jpg");
        assert!(two.copy.unwrap().ends_with("\n- a/c.jpg: timeout"));
    }

    /// The end of a copy that left links to folders out (#183; L17 of 2026-11-21: it was the copy
    /// box staying, without Copy): a message as the others, the links counted in the heading and
    /// named in their order, why in a sentence, and at the end that the guide says more, with
    /// the page F1 opens; a move and the plural; the folder as it is, also with what Markdown
    /// would take for its own, and Copy gives it back plain; no password shown.
    #[test]
    fn links_left_out_told() {
        let one = end("it", false, "/home/p/arrivo/", Ok(()), [&["collegamenti/self"], &[], &[]], &[]).unwrap();
        assert_eq!(one.title, "Copia finita");
        let said = "In /home/p/arrivo/\n\n1 collegamento a una cartella saltato:\nUn collegamento a una cartella su un server FTP, o che riporta dentro le cartelle copiate, non viene copiato.\n- collegamenti/self\n\nAltro nella guida (F1).";
        assert_eq!(plain(&one.text), said);
        assert!(one.text.starts_with(&format!("In {}\n\n**1 collegamento a una cartella saltato{}**\n", md("/home/p/arrivo/"), md(":"))), "{}", one.text);
        assert!(one.warning && one.save_as.is_none());
        assert_eq!(one.help, Some("help:copy-move"));
        assert_eq!(one.copy.as_deref(), Some(said));
        let links: Vec<String> = (1..=12).rev().map(|i| format!("a/link_{i}")).collect();
        let links: Vec<&str> = links.iter().map(String::as_str).collect();
        let odd = "/p/a*b `c` **d**\\e/";
        let moved = end("en", true, odd, Ok(()), [&links, &[], &[]], &[]).unwrap();
        assert_eq!(moved.title, "Move finished");
        let start = "12 links to folders skipped:\nA link to a folder on an FTP server, or one that leads back into the folders copied, stays where it was, with the folder that holds it.\n- a/link_1\n- a/link_2\n";
        assert!(plain(&moved.text).starts_with(&format!("To {odd}\n\n{start}")), "{}", moved.text);
        assert!(plain(&moved.text).ends_with("\n- a/link_10\n- and 2 more\n\nMore in the guide (F1).") && moved.text.matches("\n- ").count() == 11, "{}", moved.text);
        let copy = moved.copy.unwrap();
        assert!(copy.starts_with(&format!("To {odd}\n\n{start}")) && copy.ends_with("\n- a/link_11\n- a/link_12\n\nMore in the guide (F1).") && !copy.contains("more\n"), "{copy}");
        let server = end("it", false, "ftp://u:secret@host/in/", Ok(()), [&links, &[], &[]], &[]).unwrap();
        assert!(server.text.starts_with(&format!("In {}\n", md("ftp://u:***@host/in/"))) && !server.copy.unwrap().contains("secret"), "{}", server.text);
        assert!(plain(&end("it", true, "/a/", Ok(()), [&links, &[], &[]], &[]).unwrap().text).contains("\n12 collegamenti a cartelle saltati:\nUn collegamento a una cartella su un server FTP, o che riporta dentro le cartelle copiate, resta dov'era, con la cartella che lo contiene.\n"));
    }

    /// What the end of a copy says (`transfer_notice`). Nothing to say, or cancelled: no window,
    /// the copy box just goes. Links only when the copy worked (the log has them otherwise); the
    /// names left out and the files not read after an error too, under a title that says it
    /// stopped. All in one window, each part under its heading, the links first.
    #[test]
    fn end_of_a_copy_in_one_message() {
        let (cancelled, stopped): (Result<(), Error>, Result<(), Error>) = (Err(("error-cancelled", String::new())), Err(("error-too-deep", "a/".to_string())));
        let nothing: [&[&str]; 3] = [&[], &[], &[]];
        assert!(end("it", false, "/arrivo/", Ok(()), nothing, &[]).is_none() && end("it", true, "/arrivo/", Ok(()), nothing, &[]).is_none());
        assert!(end("it", false, "/arrivo/", stopped.clone(), nothing, &[]).is_none() && end("it", false, "/arrivo/", cancelled.clone(), nothing, &[]).is_none());
        let links: [&[&str]; 3] = [&["c/self"], &[], &[]];
        assert_eq!(end("it", false, "/arrivo/", Ok(()), links, &[]).map(|n| (n.title, n.help)), Some(("Copia finita".to_string(), Some("help:copy-move"))));
        assert_eq!(end("it", true, "/arrivo/", Ok(()), links, &[]).unwrap().title, "Spostamento finito");
        assert!(end("it", false, "/arrivo/", stopped.clone(), links, &[]).is_none() && end("it", false, "/arrivo/", cancelled.clone(), links, &[]).is_none());
        let (everything, failed): ([&[&str]; 3], _) = ([&["c/self"], &["c/a:b.jpg"], &["c/late.jpg"]], [("c/x.jpg", ("error-detail", "timeout".to_string()))]);
        let all = end("it", true, "/arrivo/", Ok(()), everything, &failed).unwrap();
        assert_eq!((all.title.as_str(), all.help, all.warning), ("Spostamento finito", Some("help:copy-move"), true));
        let blocks: Vec<String> = plain(&all.text).split("\n\n").map(str::to_string).collect();
        assert_eq!(blocks.len(), 6, "{}", all.text);
        assert_eq!(blocks[0], "In /arrivo/");
        assert!(blocks[1].starts_with("1 collegamento a una cartella saltato:\n") && blocks[1].ends_with("\n- c/self"), "{}", blocks[1]);
        assert_eq!(blocks[2], "1 file non copiato per il nome:\nIl suo nome non va bene su questo computer: è rimasto dove era.\n- c/a:b.jpg");
        assert_eq!(blocks[3], "1 elemento non spostato:\nÈ arrivato durante lo spostamento, o qui non si vede: è rimasto dove era, con la sua cartella.\n- c/late.jpg");
        assert_eq!(blocks[4], "1 file non letto:\nNon è stato copiato: è rimasto dove era.\n- c/x.jpg");
        assert_eq!(blocks[5], "Altro nella guida (F1).");
        // Every heading in bold, nothing else.
        assert_eq!(all.text.matches("**").count(), 8, "{}", all.text);
        let copy = all.copy.unwrap();
        assert!(copy.starts_with("In /arrivo/\n\n1 collegamento") && copy.contains("\n- c/self\n\n1 file non copiato per il nome:\nIl suo nome") && copy.ends_with("\n- c/x.jpg: timeout\n\nAltro nella guida (F1)."), "{copy}");
        // The copy stopped at an error (said in the error bar): the title says so, the links
        // are not said, the rest is.
        let after_error = end("it", false, "/arrivo/", stopped.clone(), everything, &failed).unwrap();
        assert_eq!((after_error.title.as_str(), after_error.help, after_error.warning), ("Copia interrotta", None, true));
        assert!(!after_error.text.contains("collegament") && !after_error.text.contains("guida") && plain(&after_error.text).contains("\n\n1 file non letto:\n"), "{}", after_error.text);
        assert_eq!(end("en", true, "/in/", stopped, everything, &failed).unwrap().title, "Move stopped");
        assert_eq!(end("en", false, "/in/", Err(("error-detail", "x".into())), everything, &failed).unwrap().title, "Copy stopped");
        assert!(end("it", false, "/arrivo/", cancelled, everything, &failed).is_none());
    }

    /// In the app (no screen): a copy that ends with nothing to say takes its box away and opens
    /// no window; one that left a link out opens the message in place of the box, F1 gives the
    /// page on copying while it is up, and Enter closes it.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn copy_box_goes_and_the_message_comes() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("transfer_end::tests::copy_box_goes_and_the_message_comes") {
            return;
        }
        let mut h = Harness::new(&folder("copy-end", &["a.jpg"]));
        let end = |h: &mut Harness, links: &[&str]| {
            let p = std::sync::Arc::new(crate::transfer::Progress::default());
            p.links_skipped.lock().unwrap().extend(links.iter().map(|l| l.to_string()));
            h.app.transfer = Some((p, false, h.app.base.clone(), h.app.dir.clone()));
            h.frame(Vec::new());
            assert_eq!(h.app.help_spot(&h.ctx), "help:copy-move", "the copy box is up");
            h.app.tx.send(crate::loader::Msg::Pasted(Vec::new(), Ok(()))).unwrap();
            h.frame(Vec::new());
            h.frame(Vec::new());
            assert!(h.app.transfer.is_none(), "the copy box stays");
        };
        end(&mut h, &[]);
        assert!(h.app.notice.is_none() && h.app.error.is_none(), "a window after a copy with nothing to say");
        assert_ne!(h.app.help_spot(&h.ctx), "help:copy-move");
        end(&mut h, &["collegamenti/self"]);
        let n = h.app.notice.clone().expect("no message for the link left out");
        let said = format!("**{}**", md(&h.app.i18n.t_args("transfer-links-heading", &[("n", "1")])));
        assert!(n.warning && n.title == h.app.i18n.t("transfer-copied") && n.text.contains(&said) && n.text.contains(&format!("\n- {}\n", md("collegamenti/self"))) && n.text.ends_with(&h.app.i18n.t("notice-guide")), "{}", n.text);
        assert_eq!(h.app.help_spot(&h.ctx), "help:copy-move");
        h.key(eframe::egui::Key::Enter);
        assert!(h.app.notice.is_none(), "Enter does not close the message");
    }

    /// A message already open (here a stand-in for the end of a batch rotation or an export)
    /// must not vanish when a copy ends with something to say: both are read (Fable, 24/11:
    /// `notice` is one place only, the newer replaced the older).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_copy_ending_keeps_the_message_open() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("transfer_end::tests::a_copy_ending_keeps_the_message_open") {
            return;
        }
        let mut h = Harness::new(&folder("copy-over-message", &["a.jpg"]));
        h.app.notice = Some(crate::save_dialogs::Notice::new("First message".into(), "Read me".into()));
        h.frame(Vec::new());
        let p = std::sync::Arc::new(crate::transfer::Progress::default());
        p.links_skipped.lock().unwrap().push("collegamenti/self".to_string());
        h.app.transfer = Some((p, false, h.app.base.clone(), h.app.dir.clone()));
        h.frame(Vec::new());
        h.app.tx.send(crate::loader::Msg::Pasted(Vec::new(), Ok(()))).unwrap();
        h.frame(Vec::new());
        h.frame(Vec::new());
        let shown = h.app.notice.as_ref().map(|n| n.title.clone());
        assert_eq!(shown.as_deref(), Some("First message"), "the end of the copy replaced the message open before it was read");
    }

    /// The names left out of a copy (#203, #205): counted in the heading, singular and plural,
    /// a sentence under it, the names as a list in their order, cut at ten with "and N more" as
    /// its last item; Copy with every name.
    #[test]
    fn refused_names_told() {
        let names = |n: usize| (1..=n).rev().map(|i| format!("album/{i}:a.jpg")).collect::<Vec<_>>();
        let refused = |lang: &str, names: &[String]| end(lang, false, "/arrivo/", Ok(()), [&[], &names.iter().map(String::as_str).collect::<Vec<_>>(), &[]], &[]).unwrap();
        let one = refused("it", &["album/../victim.txt".to_string()]);
        assert_eq!(one.title, "Copia finita");
        assert_eq!(one.text, format!("In {}\n\n**1 file non copiato per il nome{}**\nIl suo nome non va bene su questo computer: è rimasto dove era.\n- {}", md("/arrivo/"), md(":"), md("album/../victim.txt")));
        assert!(one.warning && one.save_as.is_none() && one.help.is_none());
        let ten = refused("it", &names(10));
        assert!(plain(&ten.text).starts_with("In /arrivo/\n\n10 file non copiati per il nome:\nIl loro nome non va bene su questo computer: sono rimasti dove erano.\n- album/1:a.jpg\n- album/2:a.jpg\n"), "{}", ten.text);
        assert_eq!(ten.text.matches("\n- ").count(), 10);
        assert!(!ten.text.contains("altri") && ten.text.ends_with(&md("album/10:a.jpg")));
        let many = refused("it", &names(15));
        assert!(many.text.ends_with(&format!("- {}\n- e altri 5", md("album/10:a.jpg"))) && many.text.matches("\n- ").count() == 11, "{}", many.text);
        // Copy takes all fifteen names, as they are.
        let copy = many.copy.unwrap();
        assert!(copy.starts_with("In /arrivo/\n\n15 file non copiati per il nome:\nIl loro nome non va bene su questo computer: sono rimasti dove erano.\n- album/1:a.jpg\n"), "{copy}");
        assert!(copy.ends_with("\n- album/14:a.jpg\n- album/15:a.jpg") && copy.matches("\n- ").count() == 15 && !copy.contains("altri"), "{copy}");
        let en = refused("en", &names(12));
        assert_eq!(en.title, "Copy finished");
        assert!(plain(&en.text).starts_with("To /arrivo/\n\n12 files not copied for their names:\nTheir names are not valid on this computer: they stayed where they were.\n- "), "{}", en.text);
        assert!(en.text.ends_with("\n- and 2 more"));
        assert!(plain(&refused("en", &names(1)).text).contains("\n1 file not copied for its name:\nIts name is not valid on this computer: it stayed where it was.\n"));
        assert!(refused("it", &names(11)).text.ends_with("\n- e un altro"));
        let broken = refused("it", &["a\nb.jpg".to_string()]);
        assert!(broken.text.ends_with(&format!("- a\u{FFFD}b{}", md(".jpg"))), "{}", broken.text);
    }

    /// What a move left in its folders (#194): counted and named as the names left out, in one
    /// window with them when both happened.
    #[test]
    fn left_by_a_move_told() {
        let one = end("it", true, "/arrivo/", Ok(()), [&[], &[], &["DCIM/Camera/late.jpg"]], &[]).unwrap();
        assert_eq!(one.title, "Spostamento finito");
        assert_eq!(plain(&one.text), "In /arrivo/\n\n1 elemento non spostato:\nÈ arrivato durante lo spostamento, o qui non si vede: è rimasto dove era, con la sua cartella.\n- DCIM/Camera/late.jpg");
        let en = end("en", true, "/in/", Ok(()), [&[], &["album/a:b.jpg"], &["b.jpg", "a.jpg"]], &[]).unwrap();
        assert_eq!(en.title, "Move finished");
        assert_eq!(plain(&en.text), "To /in/\n\n1 file not copied for its name:\nIts name is not valid on this computer: it stayed where it was.\n- album/a:b.jpg\n\n2 items not moved:\nThey arrived during the move, or they are not shown here: they stayed where they were, with their folders.\n- a.jpg\n- b.jpg");
        assert_eq!(en.copy.as_deref(), Some(plain(&en.text).as_str()));
    }

    /// #356: the lines at the end of a copy that worked, from the numbers: a copy and a move, one
    /// file and many, folders moved whole alone and with files, items skipped, under a second
    /// and with nothing copied; in English and Italian, the counts in bold.
    #[test]
    fn ended_lines_say_what_was_done() {
        use super::{Done, ended_lines};
        let said = |lang: &str, d: Done| plain(&ended_lines(&crate::i18n::I18n::new(lang), &d, "/to/"));
        let copy = Done { files: 3, bytes: 3 << 20, secs: 30.0, ..Done::default() };
        assert_eq!(said("en", copy), "3 files copied to /to/.\n3.0 MB in 30 s, 102 KB/s on average.");
        assert_eq!(said("it", copy), "3 file copiati in /to/.\n3,0 MB in 30 s, in media 102 KB/s.");
        let one = Done { cut: true, files: 1, bytes: 1 << 30, secs: 75.4, ..Done::default() };
        assert_eq!(said("en", one), "1 file moved to /to/.\n1024.0 MB in 1 min 15 s, 13.6 MB/s on average.");
        assert_eq!(said("it", one), "1 file spostato in /to/.\n1024,0 MB in 1 min 15 s, in media 13,6 MB/s.");
        let folders = Done { cut: true, folders: 2, ..Done::default() };
        assert_eq!(said("en", folders), "2 folders moved to /to/.");
        assert_eq!(said("it", folders), "2 cartelle spostate in /to/.");
        assert_eq!(said("it", Done { folders: 1, ..folders }), "1 cartella spostata in /to/.");
        let both = Done { cut: true, files: 1, folders: 1, skipped: 1, ..Done::default() };
        assert_eq!(said("en", both), "1 file and 1 folder moved to /to/.\n1 item skipped: its name was already there.");
        assert_eq!(said("it", both), "1 file e 1 cartella spostati in /to/.\n1 elemento saltato: il nome c'era già.");
        let many = Done { files: 4, folders: 3, skipped: 5, cut: true, ..Done::default() };
        assert_eq!(said("en", many), "4 files and 3 folders moved to /to/.\n5 items skipped: their names were already there.");
        assert_eq!(said("it", many), "4 file e 3 cartelle spostati in /to/.\n5 elementi saltati: i nomi c'erano già.");
        // Under a second, or nothing copied: no size line; nothing counted: where, as before.
        assert_eq!(said("en", Done { secs: 0.9, ..copy }), "3 files copied to /to/.");
        assert_eq!(said("en", Done { skipped: 2, secs: 40.0, ..Done::default() }), "To /to/\n2 items skipped: their names were already there.");
        let bold = ended_lines(&crate::i18n::I18n::new("en"), &both, "/to/");
        assert!(bold.starts_with("**1 file** and **1 folder** moved") && bold.contains("\n**1 item** skipped"), "{bold}");
    }

    /// A copy that worked, with 2 of its 3 files copied (one could not be read), 2 MB, 12 s
    /// from the start of the paste.
    fn two_of_three() -> crate::transfer::Progress {
        let p = crate::transfer::Progress::default();
        p.files.store(2, Ordering::Relaxed);
        p.files_total.store(3, Ordering::Relaxed);
        p.done.store(2 << 20, Ordering::Relaxed);
        let _ = p.begun.set(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(12)).unwrap());
        p
    }

    /// F2: the end of a copy counts the files copied, not those it was to copy: with a file not
    /// read it said one more.
    #[test]
    fn the_files_copied_are_counted() {
        let p = two_of_three();
        for (lang, copied, moved) in [("en", "2 files copied to /to/.", "2 files moved to /to/."), ("it", "2 file copiati in /to/.", "2 file spostati in /to/.")] {
            let i18n = crate::i18n::I18n::new(lang);
            assert!(super::quiet_copy(&i18n, false, "/to/", &Ok(()), &p).copy.starts_with(&format!("{copied}\n")), "{lang}");
            assert!(super::quiet_copy(&i18n, true, "/to/", &Ok(()), &p).copy.starts_with(&format!("{moved}\n")), "{lang}");
        }
    }

    /// F2: the message that lists what a copy left out says on top what the others say: how many
    /// files went where, how much in how long; it said only "To" and the folder.
    #[test]
    fn the_list_has_the_count_on_top() {
        let p = two_of_three();
        *p.failed.lock().unwrap() = vec![("a/c.jpg".to_string(), ("error-detail", "timeout".to_string()))];
        let n = super::transfer_notice(&crate::i18n::I18n::new("en"), false, "/in/", &Ok(()), &p).unwrap();
        assert!(plain(&n.text).starts_with("2 files copied to /in/.\n2.0 MB in 12 s, "), "{}", n.text);
        assert!(plain(&n.text).contains("/s on average.\n\n1 file not read:\n"), "{}", n.text);
        assert!(n.text.starts_with(&format!("**2 files** copied to {}", md("/in/"))), "{}", n.text);
        // Stopped at an error (the error bar says it): where it went, as before.
        let stopped = super::transfer_notice(&crate::i18n::I18n::new("en"), false, "/in/", &Err(("error-detail", "x".into())), &p).unwrap();
        assert!(plain(&stopped.text).starts_with("To /in/\n\n1 file not read:"), "{}", stopped.text);
    }

    /// The part of a quiet copy, in English and Italian: finished, moved, cancelled, stopped.
    #[test]
    fn quiet_copy_says_what_happened() {
        use super::quiet_copy;
        let p = crate::transfer::Progress::default();
        p.files.store(1, Ordering::Relaxed);
        p.files_total.store(3, Ordering::Relaxed);
        for (lang, done, moved, cancelled) in [
            // The files copied, not those to copy (#356).
            ("en", "Copy finished|1 file copied to /to/.", "Move finished|1 file moved to /to/.", "Copy cancelled|1 of 3 files copied to /to/ before Cancel."),
            ("it", "Copia finita|1 file copiato in /to/.", "Spostamento finito|1 file spostato in /to/.", "Copia annullata|1 file su 3 copiati in /to/ prima di Annulla."),
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
