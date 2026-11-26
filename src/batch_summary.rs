//! The window at the end of Convert… and of the commands that share its work (Resize, Copy with
//! the metadata, Copy without audio, Save the audio, Export drawing): one line per outcome with
//! its count, the time it took and the size before and after, the files left out grouped by
//! reason, then Close and Open the folder, the latter only when saved in another folder (issue #134),
//! and Copy, with every file left out, also those of the groups folded (`notice_list`).

use std::time::Duration;

use eframe::egui;

use crate::App;
use crate::audio_convert::SKIP_AUDIO;
use crate::batch::{META_TOO_LARGE, SKIP_FORMAT, SKIP_INSIDE, SKIP_NOT_AUTOCAD, SKIP_SAME_SIZE, SKIP_TOO_LARGE, SKIP_VIDEO, SKIP_WOULD_LOSE};
use crate::fault::NOT_OPENED;
use crate::i18n::I18n;
use crate::save_dialogs::{Head, copy_button, md, notice_text};
use crate::confirm::FollowArea;

/// What the work did, as it ends.
pub(crate) struct Outcome<'a> {
    pub(crate) saved: usize,
    /// Files worked on, and those there were: fewer done when it was stopped.
    pub(crate) done: usize,
    pub(crate) total: usize,
    /// Files left out: Fluent id of the reason, and detail (the name, or the error).
    pub(crate) failed: &'a [(&'static str, String)],
    /// Copies without audio saved without the camera's preview, which could not be written into
    /// them: their names (#216).
    pub(crate) no_preview: &'a [String],
    /// Files saved as JPEG without a part of their metadata, too large for it (#217), as `failed`.
    pub(crate) left_out_meta: &'a [(&'static str, String)],
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
#[derive(Clone)]
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
#[derive(Debug, PartialEq, Clone)]
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
        META_TOO_LARGE => ("summary-meta-group", true),
        _ => ("summary-errors", false),
    }
}

/// A file left out: the name it is ordered by, its line in the list, and the line Copy takes.
type Left<'a> = (&'a str, String, String);

/// The files left out by reason, in the order the reasons first came; in each group in the
/// order of their names (`source::natural`, as the grid; an error without a name by its words).
fn groups(i18n: &I18n, failed: &[(&'static str, String)], preset: bool) -> Vec<Group> {
    let mut out: Vec<(&'static str, Vec<Left>)> = Vec::new();
    for (id, detail) in failed {
        let (heading, names) = group_of(id, preset);
        let (item, copied) = item(i18n, id, detail, names);
        let file = (detail.split('\u{1}').next().unwrap_or_default(), item, copied);
        match out.iter_mut().find(|(h, _)| *h == heading) {
            Some((_, files)) => files.push(file),
            None => out.push((heading, vec![file])),
        }
    }
    let group = |(id, mut files): (&str, Vec<Left>)| {
        files.sort_by(|a, b| crate::source::natural(a.0, b.0));
        let (items, copied): (Vec<String>, Vec<String>) = files.into_iter().map(|(_, item, copied)| (item, copied)).unzip();
        Group { heading: i18n.t_args(id, &[("n", &items.len().to_string())]), items, copied }
    };
    out.into_iter().map(group).collect()
}

/// A file left out as a line of its group (`md`), and as Copy takes it: the name, or the error
/// said plainly; a file that could not be decoded as the viewer says it ("Cannot open x"), the
/// library's text only in Copy (and in the log, `batch_done`).
pub(crate) fn item(i18n: &I18n, id: &str, detail: &str, names: bool) -> (String, String) {
    // One line each in the list: a reason said on several lines is joined again.
    let line = |text: &str| md(text).replace('\n', " ");
    // "EXIF and XMP" in the language of the summary (#217).
    let detail = &detail.replace(crate::encoders::AND, &i18n.t("list-and"));
    // What a drawing would lose in the other format, in the language of the summary (#340).
    let detail = &if id == SKIP_WOULD_LOSE { crate::cad_convert::said(i18n, detail) } else { detail.clone() };
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
    if !o.left_out_meta.is_empty() {
        lines.push(i18n.t_args("summary-meta-left-out", &[("n", &n(o.left_out_meta.len()))]));
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
    // The files saved first, then those left out, then the copies without the preview.
    let mut groups: Vec<Group> = [o.left_out_meta, o.failed].iter().flat_map(|list| groups(i18n, list, o.preset)).collect();
    groups.extend(no_preview(i18n, o.no_preview));
    let lists: Vec<(&str, &[String])> = groups.iter().map(|g| (g.heading.as_str(), g.copied.as_slice())).collect();
    let copy = crate::notice_list::copied(&text, &lists);
    Summary { title: i18n.t(title), text, groups, open: (o.saved > 0).then_some(open), copy }
}

/// The copies saved without the camera's preview, as a group after those left out: they are
/// saved, but the user should know their thumbnail is a frame of the video (#216).
fn no_preview(i18n: &I18n, names: &[String]) -> Option<Group> {
    let mut names: Vec<&String> = names.iter().collect();
    names.sort_by(|a, b| crate::source::natural(a, b));
    let items: Vec<String> = names.iter().map(|n| md(n)).collect();
    (!items.is_empty()).then(|| Group { heading: i18n.t_args("summary-no-preview", &[("n", &items.len().to_string())]), copied: items.clone(), items })
}

impl App {
    /// The summary at the end of the work, until closed (Close, Enter or Esc).
    pub(crate) fn batch_summary_window(&mut self, ctx: &egui::Context) {
        let Some(s) = self.batch_summary.take() else { return };
        let t = |id| self.i18n.t(id);
        // Enter and Esc arrive as `answer` (`raw_input_hook`).
        let id = "batch-summary";
        let mut close = crate::confirm::answer(ctx, &mut self.key_answer, id) == Some(false);
        let mut go = None;
        let covered = self.folder_covered();
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
                egui::ScrollArea::vertical().max_height(max).auto_shrink([false, true]).show_follow(ui, |ui| s.groups.iter().for_each(|g| group_ui(ui, g, None)));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Close highlighted, then Open the folder when there is one (#311).
                let mut labels = vec![(close_label.clone(), true)];
                if s.open.is_some() {
                    labels.push((open_label.clone(), !covered));
                }
                match crate::confirm::buttons_hint(ui, id, &labels, 0, &[None, Some(covered_hint.clone())]) {
                    Some(0) => close = true,
                    Some(_) => go = s.open.clone(),
                    None => {}
                }
            });
            copy_button(ui, id, &tip, || s.copy.clone());
        });
        if let Some(address) = go {
            log!("batch summary: Open the folder {:?}", crate::address::without_password(&address));
            self.open_folder_after_batch(ctx, address);
            close = true;
        }
        if close {
            self.next_waiting();
        } else {
            self.batch_summary = Some(s);
        }
    }

    /// A page covers the thumbnails (editor, Save as, slideshow, duplicates): Open the folder
    /// would open it under them, so it is greyed out (`summary-open-covered`).
    pub(crate) fn folder_covered(&self) -> bool {
        self.show.is_some() || self.convert.is_some() || self.editor.is_some() || self.dup_page_shown()
    }

    /// The thumbnails of the destination, after asking about turns not saved in the viewer.
    pub(crate) fn open_folder_after_batch(&mut self, ctx: &egui::Context, address: String) {
        if self.viewing.is_some() && self.hold_for_turn(crate::save_dialogs::After::Open(address.clone())) {
            return;
        }
        self.open_source(ctx, &address);
    }
}

/// A group: its heading in bold, as the headings of the other messages (`notice_list`), and the
/// files; folded when long, its files then in line with those of the other groups. `open`: a
/// folded group held open or closed, whatever was clicked (`None` in the window: as clicked).
pub(crate) fn group_ui(ui: &mut egui::Ui, g: &Group, open: Option<bool>) {
    let items: String = g.items.iter().map(|i| format!("- {i}\n")).collect();
    let heading = egui::RichText::new(&g.heading).family(egui::FontFamily::Name(crate::BOLD.into()));
    if g.items.len() > FOLD {
        egui::CollapsingHeader::new(heading).default_open(false).open(open).show_unindented(ui, |ui| notice_text(ui, &items));
    } else {
        ui.add(egui::Label::new(heading).wrap());
        notice_text(ui, &items);
    }
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome<'a>(failed: &'a [(&'static str, String)], saved: usize, done: usize, total: usize) -> Outcome<'a> {
        Outcome { saved, done, total, failed, no_preview: &[], left_out_meta: &[], preset: false, folder: "/p/converted", took: Duration::from_secs(75), sizes: Some((4 << 20, 1 << 20)) }
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
        assert!(it.text.starts_with("**2 file** salvati in \\/p\\/converted.\n\nCi sono voluti 1 min 15 s.\nDimensione: 4,0 MB prima, 1,0 MB dopo (25%)."), "{}", it.text);
    }

    /// The files that do not decode, as Convert… leaves them out, in the order of their names: a
    /// PNG cut in half, with a name to escape, and a RAW photo full of zeros.
    fn not_decoded() -> Vec<(&'static str, String)> {
        let noise = image::RgbImage::from_fn(64, 64, |x, y| image::Rgb([(x * 7 + y * 13) as u8, (x * y) as u8, (x ^ y) as u8]));
        let mut png = Vec::new();
        noise.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        png.truncate(png.len() / 2);
        let job = crate::batch::Job { format: Some(crate::encode::Format::Jpeg), ..Default::default() };
        [(png, "a_b*.png"), (vec![0; 4096], "huge.cr3")].iter().map(|(bytes, name)| crate::batch::convert(bytes, name, &job).unwrap_err()).collect()
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

    /// Two groups drawn through egui with the app's fonts, one folded (more than five files) and
    /// opened: the files of both in line, bullets and names, as in the other messages; the
    /// headings in the bold of the messages.
    #[test]
    fn groups_drawn_alike() {
        let group = |heading: &str, n: usize| Group { heading: heading.into(), items: (1..=n).map(|i| md(&format!("{heading}-{i}.jpg"))).collect(), copied: Vec::new() };
        let (short, long) = (group("short", 2), group("long", FOLD + 2));
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let mut lines = Vec::new();
        for frame in 0..6 {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), time: Some(frame as f64), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                group_ui(ui, &short, None);
                group_ui(ui, &long, Some(true));
            });
            out.textures_delta.clear();
            lines.clear();
            crate::save_dialogs::tests::drawn(&out.shapes.iter().map(|c| c.shape.clone()).collect::<Vec<_>>(), &mut lines);
        }
        let at = |words: &str| lines.iter().find(|(t, ..)| t == words).unwrap_or_else(|| panic!("{words:?} not drawn: {lines:?}")).1;
        let bullets: Vec<f32> = lines.iter().filter(|(t, ..)| t == "•").map(|(_, x, _)| *x).collect();
        assert_eq!(bullets.len(), FOLD + 4, "{lines:?}");
        assert!(bullets.iter().all(|x| *x == bullets[0]), "bullets not in line: {bullets:?}");
        assert!(at("short-1.jpg") == at("long-1.jpg") && at("long-7.jpg") == at("short-2.jpg") && at("short-1.jpg") > bullets[0], "{lines:?}");
        assert!(at("short") < bullets[0], "{lines:?}");
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
        assert!(s.copy.starts_with("2 file salvati in /p/converted.\n16 file lasciati fuori.\n\nCi sono voluti 1 min 15 s."), "{}", s.copy);
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
        // In each group in the order of the names, as the grid has them; Copy too.
        let mixed = [("skip-not-picture", "img10.txt".to_string()), (SKIP_SAME_SIZE, "z.jpg".to_string()), ("skip-folder", "Img2".to_string()), ("skip-not-picture", "a.txt".to_string()), crate::fault::not_opened("b.png", "cut"), crate::fault::not_opened("A.png", "cut")];
        let m = groups(&i18n, &mixed, false);
        assert_eq!(m.iter().map(|g| g.items.len()).collect::<Vec<_>>(), [3, 1, 2]);
        assert_eq!((&m[0].items, &m[0].copied), (&vec!["a\\.txt".to_string(), "Img2".into(), "img10\\.txt".into()], &m[0].items));
        assert_eq!(m[2].copied, ["Cannot open A\\.png \\(cut\\)", "Cannot open b\\.png \\(cut\\)"]);
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

    /// Copies without audio saved without the camera's preview (#216): a group of their own after
    /// those left out, in the order of their names, and in what Copy takes; none without them.
    #[test]
    fn copies_without_the_preview() {
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            let names = ["v10 (2).mp4".to_string(), "v9 (2).mp4".to_string()];
            let failed = [("skip-not-video", "p.jpg".to_string())];
            let s = summary(&i18n, "batch-title", &Outcome { no_preview: &names, ..outcome(&failed, 2, 3, 3) }, String::new());
            assert_eq!(s.groups.len(), 2);
            let g = &s.groups[1];
            assert_eq!(g.items, [md("v9 (2).mp4"), md("v10 (2).mp4")], "{lang}");
            assert!(!g.heading.contains("summary-") && g.heading.starts_with('2'), "{lang}: {}", g.heading);
            assert!(s.copy.contains(&g.heading) && s.copy.contains("v10 (2).mp4"), "{lang}: {}", s.copy);
            assert_eq!(summary(&i18n, "batch-title", &outcome(&failed, 2, 3, 3), String::new()).groups.len(), 1);
        }
        let one = summary(&I18n::new("en"), "batch-title", &Outcome { no_preview: &["v (2).mp4".into()], ..outcome(&[], 1, 1, 1) }, String::new());
        assert_eq!(one.groups[0].heading, "1 copy saved without the camera preview, because it could not be written:");
        let two = summary(&I18n::new("it"), "batch-title", &Outcome { no_preview: &["a.mp4".into(), "b.mp4".into()], ..outcome(&[], 2, 2, 2) }, String::new());
        assert_eq!(two.groups[0].heading, "2 copie salvate senza le anteprime della videocamera, perché non è stato possibile scriverle:");
    }

    /// Files saved without metadata too large for a JPEG (#217): a line after the files saved,
    /// and their group first, each with what it lost.
    #[test]
    fn meta_too_large_said() {
        let i18n = I18n::new("en");
        let thin = [(META_TOO_LARGE, "b.png (EXIF)".to_string()), (META_TOO_LARGE, format!("a.tif (EXIF {} XMP)", crate::encoders::AND))];
        let failed = [("skip-not-picture", "c.txt".to_string())];
        let s = summary(&i18n, "batch-title", &Outcome { left_out_meta: &thin, ..outcome(&failed, 2, 3, 3) }, String::new());
        assert!(s.text.starts_with("**2 files** saved in \\/p\\/converted.\n**2 files** saved without part of their metadata, because it is too large for a JPEG file.\n**1 file** left out."), "{}", s.text);
        assert_eq!(s.groups[0].heading, "2 files saved without the metadata in brackets, because it is too large for a JPEG file:");
        assert_eq!(s.groups[0].items, ["a\\.tif \\(EXIF and XMP\\)", "b\\.png \\(EXIF\\)"]);
        let it = summary(&I18n::new("it"), "batch-title", &Outcome { left_out_meta: &thin[..1], ..outcome(&[], 1, 1, 1) }, String::new());
        assert!(it.text.contains("**1 file** salvato senza una parte dei suoi metadati, perché sono troppo grandi per un file JPEG."), "{}", it.text);
    }
}
