//! The window at the end of Convert… and of the commands that share its work (Resize, Copy with
//! the metadata, Copy without audio, Save the audio, Export drawing): one line per outcome with
//! its count, the time it took and the size before and after, the files left out grouped by
//! reason, then Close and Open the folder, the latter only when saved in another folder (issue #134),
//! and Copy, with every file left out, also those of the groups folded (`notice_list`).

use std::time::Duration;

use eframe::egui;

use crate::App;
use crate::audio_convert::SKIP_AUDIO;
use crate::batch::{SKIP_FORMAT, SKIP_INSIDE, SKIP_NOT_AUTOCAD, SKIP_SAME_SIZE, SKIP_TOO_LARGE, SKIP_VIDEO, SKIP_WOULD_LOSE};
use crate::fault::NOT_OPENED;
use crate::i18n::I18n;
use crate::save_dialogs::{Head, copy_button, md, notice_text};

/// What the work did, as it ends.
pub(crate) struct Outcome<'a> {
    pub(crate) saved: usize,
    /// Files worked on, and those there were: fewer done when it was stopped.
    pub(crate) done: usize,
    pub(crate) total: usize,
    /// Files left out: Fluent id of the reason, and detail (the name, or the error).
    pub(crate) failed: &'a [(&'static str, String)],
    /// Resize with a preset: its own words for a format not written (it has no format to choose).
    pub(crate) preset: bool,
    /// The destination, as the address bar shows it.
    pub(crate) folder: &'a str,
    pub(crate) took: Duration,
    /// Size of the originals of the files saved, and of the new files; `None` when one of them
    /// could not be read.
    pub(crate) sizes: Option<(u64, u64)>,
}

/// The window, ready to draw.
pub(crate) struct Summary {
    pub(crate) title: String,
    /// The outcomes, then the time and the sizes: lines of a message (`notice_text`).
    pub(crate) text: String,
    pub(crate) groups: Vec<Group>,
    /// Where Open the folder goes (an address); `None` when nothing was saved.
    pub(crate) open: Option<String>,
    /// What Copy takes: the text, then every group with all its files.
    pub(crate) copy: String,
}

/// Files left out for the same reason.
#[derive(Debug, PartialEq)]
pub(crate) struct Group {
    pub(crate) heading: String,
    /// The names, or for errors the error of each file, as a message line (`md`).
    pub(crate) items: Vec<String>,
    /// The items as Copy takes them: a file that could not be decoded with the library's text
    /// too, in brackets, for a report.
    pub(crate) copied: Vec<String>,
}

/// A group with more files than this is shown folded.
const FOLD: usize = 5;

/// The heading of the group of a reason: the Fluent id, and whether its items are the names
/// (`false`: an error, said whole).
fn group_of(id: &str, preset: bool) -> (&'static str, bool) {
    match id {
        "skip-folder" | "skip-not-picture" | "skip-not-video" | "skip-not-drawing" | "skip-not-played" | "skip-not-converted" => ("summary-kind", true),
        SKIP_FORMAT if preset => ("summary-format-preset", true),
        SKIP_FORMAT => ("summary-format", true),
        SKIP_SAME_SIZE => ("summary-same-size", true),
        SKIP_INSIDE => ("summary-inside", true),
        SKIP_VIDEO | SKIP_AUDIO => ("summary-no-ffmpeg", true),
        SKIP_NOT_AUTOCAD => ("summary-not-autocad", true),
        SKIP_WOULD_LOSE => ("summary-would-lose", true),
        SKIP_TOO_LARGE => ("summary-too-large", true),
        _ => ("summary-errors", false),
    }
}

/// The files left out by reason, in the order the reasons first came.
fn groups(i18n: &I18n, failed: &[(&'static str, String)], preset: bool) -> Vec<Group> {
    let mut out: Vec<(&'static str, Vec<String>, Vec<String>)> = Vec::new();
    for (id, detail) in failed {
        let (heading, names) = group_of(id, preset);
        let (item, copied) = item(i18n, id, detail, names);
        match out.iter_mut().find(|(h, ..)| *h == heading) {
            Some((_, items, all)) => {
                items.push(item);
                all.push(copied);
            }
            None => out.push((heading, vec![item], vec![copied])),
        }
    }
    out.into_iter().map(|(id, items, copied)| Group { heading: i18n.t_args(id, &[("n", &items.len().to_string())]), items, copied }).collect()
}

/// A file left out as a line of its group (`md`), and as Copy takes it: the name, or the error
/// said plainly; a file that could not be decoded as the viewer says it ("Cannot open x"), the
/// library's text only in Copy (and in the log, `batch_done`).
pub(crate) fn item(i18n: &I18n, id: &str, detail: &str, names: bool) -> (String, String) {
    // One line each in the list: a reason said on several lines is joined again.
    let line = |text: &str| md(text).replace('\n', " ");
    match detail.split_once('\u{1}').filter(|_| id == NOT_OPENED) {
        _ if names => (md(detail), md(detail)),
        Some((name, raw)) => {
            let (sentence, apart) = i18n.not_opened(name, raw);
            // "Cannot open x" and the reason said under it: one sentence on one line.
            let sentence = match sentence.split_once('\n') {
                Some((open, why)) if !open.ends_with(['.', ':', '!', '?']) => format!("{open}: {why}"),
                _ => sentence,
            };
            let copied = apart.map_or_else(|| line(&sentence), |raw| line(&format!("{sentence} ({raw})")));
            (line(&sentence), copied)
        }
        None => {
            let item = i18n.error_md(id, detail).replace('\n', " ");
            (item.clone(), item)
        }
    }
}

/// The window for `o`, titled with the Fluent id `title` (that of the command); `open` the
/// destination's address.
pub(crate) fn summary(i18n: &I18n, title: &str, o: &Outcome, open: String) -> Summary {
    let n = |n: usize| n.to_string();
    let mut lines = Vec::new();
    if o.saved > 0 {
        lines.push(i18n.t_args("summary-saved", &[("n", &n(o.saved)), ("folder", &md(o.folder))]));
    }
    if !o.failed.is_empty() {
        lines.push(i18n.t_args("summary-left-out", &[("n", &n(o.failed.len()))]));
    }
    if o.done < o.total {
        lines.push(i18n.t_args("summary-not-done", &[("n", &n(o.total - o.done))]));
    }
    if lines.is_empty() {
        lines.push(i18n.t("summary-nothing"));
    }
    let mut after = vec![i18n.t_args("summary-took", &[("time", &crate::human::human_time(o.took.as_secs()))])];
    if let Some((before, now)) = o.sizes.filter(|&(before, _)| o.saved > 0 && before > 0) {
        let size = |b| crate::human::size(i18n, b);
        let percent = (now as f64 * 100.0 / before as f64).round().to_string();
        after.push(i18n.t_args("summary-sizes", &[("before", &size(before)), ("after", &size(now)), ("percent", &percent)]));
    }
    let text = format!("{}\n\n{}", lines.join("\n"), after.join("\n"));
    let groups = groups(i18n, o.failed, o.preset);
    let lists: Vec<(&str, &[String])> = groups.iter().map(|g| (g.heading.as_str(), g.copied.as_slice())).collect();
    let copy = crate::notice_list::copied(&text, &lists);
    Summary { title: i18n.t(title), text, groups, open: (o.saved > 0).then_some(open), copy }
}

impl App {
    /// The summary at the end of the work, until closed (Close, Enter or Esc).
    pub(crate) fn batch_summary_window(&mut self, ctx: &egui::Context) {
        let Some(s) = self.batch_summary.take() else { return };
        let t = |id| self.i18n.t(id);
        // Enter and Esc arrive as `answer` (`raw_input_hook`).
        let id = "batch-summary";
        let mut close = crate::confirm::answer(ctx, &mut self.key_answer, id).is_some();
        let mut go = None;
        // A page covers the thumbnails (editor, Save as, slideshow, duplicates): it would hide them.
        let covered = self.show.is_some() || self.convert.is_some() || self.editor.is_some() || self.dup_page_shown();
        let (open_label, covered_hint, close_label) = (t("summary-open-folder"), t("summary-open-covered"), t("notice-close"));
        let tip = t("notice-copy");
        egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
            // As every message window (`save_dialogs::message`).
            ui.set_max_width(460.0);
            ui.set_min_width(320.0);
            Head::new(&s.title).show(ui);
            notice_text(ui, &s.text);
            if !s.groups.is_empty() {
                ui.add_space(8.0);
                // Hundreds of files left out: the window stays within the screen.
                let max = ctx.content_rect().height() * 0.45;
                egui::ScrollArea::vertical().max_height(max).auto_shrink([false, true]).show(ui, |ui| s.groups.iter().for_each(|g| group_ui(ui, g)));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(&close_label).clicked() {
                    close = true;
                }
                if let Some(address) = &s.open {
                    let b = ui.add_enabled(!covered, egui::Button::new(&open_label)).on_disabled_hover_text(&covered_hint);
                    if b.clicked() {
                        go = Some(address.clone());
                    }
                }
            });
            copy_button(ui, &tip, || s.copy.clone());
        });
        if let Some(address) = go {
            log!("batch summary: Open the folder {:?}", crate::address::without_password(&address));
            self.open_folder_after_batch(ctx, address);
            close = true;
        }
        if !close {
            self.batch_summary = Some(s);
        }
    }

    /// The thumbnails of the destination, after asking about turns not saved in the viewer.
    pub(crate) fn open_folder_after_batch(&mut self, ctx: &egui::Context, address: String) {
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Open(address.clone())) {
            return;
        }
        self.open_source(ctx, &address);
    }
}

/// A group: its heading and the files; folded when long.
fn group_ui(ui: &mut egui::Ui, g: &Group) {
    let items: String = g.items.iter().map(|i| format!("- {i}\n")).collect();
    if g.items.len() > FOLD {
        egui::CollapsingHeader::new(&g.heading).default_open(false).show(ui, |ui| notice_text(ui, &items));
    } else {
        ui.label(egui::RichText::new(&g.heading).strong());
        notice_text(ui, &items);
    }
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome<'a>(failed: &'a [(&'static str, String)], saved: usize, done: usize, total: usize) -> Outcome<'a> {
        Outcome { saved, done, total, failed, preset: false, folder: "/p/converted", took: Duration::from_secs(75), sizes: Some((4 << 20, 1 << 20)) }
    }

    /// One line per outcome with its count, none for a count of zero; the time and the sizes.
    #[test]
    fn a_line_per_outcome() {
        let i18n = I18n::new("en");
        let s = summary(&i18n, "batch-title", &outcome(&[], 3, 3, 3), "/p/converted".into());
        assert_eq!(s.title, "Convert");
        assert_eq!(s.text, "**3 files** saved in \\/p\\/converted.\n\nIt took 1 min 15 s.\nSize: 4.0 MB before, 1.0 MB after (25%).");
        assert_eq!(s.open.as_deref(), Some("/p/converted"));
        let failed = [("skip-not-picture", "a.txt".to_string())];
        let s = summary(&i18n, "batch-title", &outcome(&failed, 1, 5, 9), String::new());
        assert!(s.text.starts_with("**1 file** saved in \\/p\\/converted.\n**1 file** left out.\n**4 files** not done: the work was stopped.\n\n"), "{}", s.text);
        // Nothing saved: no folder to open, no sizes.
        let s = summary(&i18n, "batch-title-resize", &outcome(&failed, 0, 1, 1), String::new());
        assert_eq!((s.open, s.text.as_str()), (None, "**1 file** left out.\n\nIt took 1 min 15 s."));
        let s = summary(&i18n, "batch-title", &outcome(&[], 0, 0, 0), String::new());
        assert!(s.text.starts_with("No file was saved.\n\n"), "{}", s.text);
        // Nothing left out: Copy takes the text as it reads.
        assert_eq!(summary(&i18n, "batch-title", &outcome(&[], 3, 3, 3), String::new()).copy, "3 files saved in /p/converted.\n\nIt took 1 min 15 s.\nSize: 4.0 MB before, 1.0 MB after (25%).");
        let it = summary(&I18n::new("it"), "batch-title", &outcome(&[], 2, 2, 2), String::new());
        assert!(it.text.starts_with("**2 file** salvati in \\/p\\/converted.\n\nCi è voluto 1 min 15 s.\nDimensione: 4,0 MB prima, 1,0 MB dopo (25%)."), "{}", it.text);
    }

    /// The files that do not decode, as Convert… leaves them out: a RAW photo full of zeros and
    /// a PNG cut in half, one with a name to escape.
    fn not_decoded() -> Vec<(&'static str, String)> {
        let noise = image::RgbImage::from_fn(64, 64, |x, y| image::Rgb([(x * 7 + y * 13) as u8, (x * y) as u8, (x ^ y) as u8]));
        let mut png = Vec::new();
        noise.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        png.truncate(png.len() / 2);
        let job = crate::batch::Job { format: Some(crate::encode::Format::Jpeg), ..Default::default() };
        [(vec![0; 4096], "huge.cr3"), (png, "a_b*.png")].iter().map(|(bytes, name)| crate::batch::convert(bytes, name, &job).unwrap_err()).collect()
    }

    /// A file that does not decode is said as the viewer says it (`cannot_load`, the same words):
    /// "Cannot open x" in the list, the library's text only in Copy; in both languages.
    #[test]
    fn not_opened_as_the_viewer() {
        let failed = not_decoded();
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            let s = summary(&i18n, "batch-title", &outcome(&failed, 0, 2, 2), String::new());
            assert_eq!((s.groups.len(), s.groups[0].heading.as_str()), (1, if lang == "en" { "2 errors:" } else { "2 errori:" }));
            for ((id, detail), (item, copied)) in failed.iter().zip(s.groups[0].items.iter().zip(&s.groups[0].copied)) {
                assert_eq!(*id, NOT_OPENED);
                let (name, raw) = detail.split_once('\u{1}').unwrap();
                let (sentence, apart) = i18n.not_opened(name, raw);
                assert_eq!(i18n.cannot_load(name, raw), format!("{sentence}\n({})", apart.as_deref().unwrap()), "the viewer's words");
                assert_eq!(crate::save_dialogs::plain(item), sentence);
                assert!(sentence.contains(name) && !sentence.contains(raw.trim()), "{sentence}");
                assert_eq!(crate::save_dialogs::plain(copied), format!("{sentence} ({})", raw.trim()));
                assert!(s.copy.contains(&format!("- {sentence} ({})", raw.trim())), "{}", s.copy);
            }
        }
        assert_eq!(I18n::new("it").not_opened("huge.cr3", "Error: x").0, "Impossibile aprire huge.cr3");
        assert_eq!(I18n::new("en").not_opened("huge.cr3", "Error: x").0, "Cannot open huge.cr3");
    }

    /// What the system said is said plainly, as before (`fault`), with the file's name; a reason
    /// the program translates (the CAD engine missing) stays in the sentence.
    #[test]
    fn not_opened_said_plainly() {
        let en = I18n::new("en");
        #[cfg(unix)]
        assert_eq!(en.not_opened("a.png", "Permission denied (os error 13)"), (en.fault(crate::fault::Kind::Denied, Some("a.png")), None));
        let (sentence, apart) = en.not_opened("p.dwg", crate::occt::MISSING);
        assert_eq!((sentence.as_str(), apart), ("Cannot open p.dwg\nThe CAD engine is missing.", None));
        let g = groups(&en, &[crate::fault::not_opened("p.dwg", crate::occt::MISSING)], false);
        assert_eq!((g[0].items[0].as_str(), g[0].copied[0].as_str()), ("Cannot open p\\.dwg\\: The CAD engine is missing\\.", "Cannot open p\\.dwg\\: The CAD engine is missing\\."));
    }

    /// Copy takes every file left out, also those of a group shown folded, under their headings.
    #[test]
    fn copy_takes_every_file() {
        let i18n = I18n::new("it");
        let mut failed: Vec<(&'static str, String)> = (1..=15).map(|i| (SKIP_FORMAT, format!("foto_{i}.heic"))).collect();
        failed.push(("skip-not-picture", "nota.txt".into()));
        let s = summary(&i18n, "batch-title", &outcome(&failed, 2, 18, 18), String::new());
        assert!(s.groups[0].items.len() > FOLD);
        let files: Vec<String> = (1..=15).map(|i| format!("- foto_{i}.heic")).collect();
        let want = format!("\n\n15 file in un formato che EyeCanSee non sa scrivere; scegli un altro formato:\n{}\n\n1 file di un tipo su cui questo comando non lavora:\n- nota.txt", files.join("\n"));
        assert!(s.copy.starts_with("2 file salvati in /p/converted.\n16 file lasciati fuori.\n\nCi è voluto 1 min 15 s."), "{}", s.copy);
        assert!(s.copy.ends_with(&want), "{}", s.copy);
    }

    /// The files left out by reason, in the order the reasons came; errors said whole; Resize
    /// with a preset says a format not written its own way.
    #[test]
    fn left_out_by_reason() {
        let i18n = I18n::new("en");
        let failed = [
            ("skip-not-picture", "a.txt".to_string()),
            (SKIP_SAME_SIZE, "b.jpg".to_string()),
            ("skip-folder", "c_d".to_string()),
            ("batch-failed", "e.png: broken".to_string()),
            (SKIP_FORMAT, "f.heic".to_string()),
        ];
        let g = groups(&i18n, &failed, false);
        let headings: Vec<_> = g.iter().map(|g| g.heading.as_str()).collect();
        assert_eq!(headings, ["2 files of a kind this command does not work on:", "1 file already of this size, or smaller and not enlarged:", "1 error:", "1 file in a format EyeCanSee cannot write; choose another format:"]);
        assert_eq!(g[0].items, ["a\\.txt", "c\\_d"]);
        assert_eq!(g[2].items, ["e\\.png\\: broken"]);
        // A reason said on several lines stays one item of the list.
        let trash = groups(&i18n, &[("error-trash-failed", "f.jpg: busy".to_string())], false);
        assert!(!trash[0].items[0].contains('\n') && trash[0].items[0].contains("unchanged. (f"), "{:?}", trash[0].items);
        assert_eq!(groups(&i18n, &failed[4..], true)[0].heading, "1 file in a format EyeCanSee cannot write; Resize > Custom… can save it in another one:");
        let large = [(SKIP_TOO_LARGE, "a.cr3".to_string()), (SKIP_TOO_LARGE, "b.tif".to_string())];
        assert_eq!(groups(&i18n, &large, false)[0].heading, "2 pictures too large for the memory of this computer, left out:");
        assert_eq!(groups(&I18n::new("it"), &large[..1], true)[0].heading, "1 immagine troppo grande per la memoria di questo computer, lasciata fuori:");
        for (id, _) in failed.iter().chain(&[(SKIP_INSIDE, String::new()), (SKIP_VIDEO, String::new()), (SKIP_AUDIO, String::new()), (SKIP_NOT_AUTOCAD, String::new()), (SKIP_WOULD_LOSE, String::new()), (SKIP_TOO_LARGE, String::new())]) {
            for lang in ["en", "it"] {
                let g = groups(&I18n::new(lang), &[(id, "x".into())], false);
                assert!(!g[0].heading.contains("summary-"), "{lang} {id}: {}", g[0].heading);
            }
        }
    }
}
