//! The window at the end of Convert… and of the commands that share its work (Resize, Copy with
//! the metadata, Copy without audio, Save the audio, Export drawing): one line per outcome with
//! its count, the time it took and the size before and after, the files left out grouped by
//! reason, then Open the folder and Close (issue #134).

use std::time::Duration;

use eframe::egui;

use crate::App;
use crate::audio_convert::SKIP_AUDIO;
use crate::batch::{SKIP_FORMAT, SKIP_INSIDE, SKIP_NOT_AUTOCAD, SKIP_SAME_SIZE, SKIP_VIDEO, SKIP_WOULD_LOSE};
use crate::i18n::I18n;
use crate::save_dialogs::{Head, md, notice_text};

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
}

/// Files left out for the same reason.
#[derive(Debug, PartialEq)]
pub(crate) struct Group {
    pub(crate) heading: String,
    /// The names, or for errors the error of each file, as a message line (`md`).
    pub(crate) items: Vec<String>,
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
        _ => ("summary-errors", false),
    }
}

/// The files left out by reason, in the order the reasons first came.
fn groups(i18n: &I18n, failed: &[(&'static str, String)], preset: bool) -> Vec<Group> {
    let mut out: Vec<(&'static str, Vec<String>)> = Vec::new();
    for (id, detail) in failed {
        let (heading, names) = group_of(id, preset);
        let item = if names { md(detail) } else { i18n.error_md(id, detail) };
        match out.iter_mut().find(|(h, _)| *h == heading) {
            Some((_, items)) => items.push(item),
            None => out.push((heading, vec![item])),
        }
    }
    out.into_iter().map(|(id, items)| Group { heading: i18n.t_args(id, &[("n", &items.len().to_string())]), items }).collect()
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
    Summary {
        title: i18n.t(title),
        text: format!("{}\n\n{}", lines.join("\n"), after.join("\n")),
        groups: groups(i18n, o.failed, o.preset),
        open: (o.saved > 0).then_some(open),
    }
}

impl App {
    /// The summary at the end of the work, until closed (Close, Enter or Esc).
    pub(crate) fn batch_summary_window(&mut self, ctx: &egui::Context) {
        let Some(s) = self.batch_summary.take() else { return };
        let t = |id| self.i18n.t(id);
        // Enter and Esc arrive as `delete_answer` (`raw_input_hook`).
        let mut close = self.delete_answer.take().is_some();
        let mut go = None;
        // A page covers the thumbnails (editor, Save as, slideshow, duplicates): it would hide them.
        let covered = self.show.is_some() || self.convert.is_some() || self.editor.is_some() || self.dup_page_shown();
        let (open_label, covered_hint, close_label) = (t("summary-open-folder"), t("summary-open-covered"), t("notice-close"));
        egui::Modal::new(egui::Id::unique("batch-summary")).show(ctx, |ui| {
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
                if let Some(address) = &s.open {
                    let b = ui.add_enabled(!covered, egui::Button::new(&open_label)).on_disabled_hover_text(&covered_hint);
                    if b.clicked() {
                        go = Some(address.clone());
                    }
                }
                if ui.button(&close_label).clicked() {
                    close = true;
                }
            });
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
        let it = summary(&I18n::new("it"), "batch-title", &outcome(&[], 2, 2, 2), String::new());
        assert!(it.text.starts_with("**2 file** salvati in \\/p\\/converted.\n\nCi è voluto 1 min 15 s.\nDimensione: 4,0 MB prima, 1,0 MB dopo (25%)."), "{}", it.text);
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
        assert_eq!(groups(&i18n, &failed[4..], true)[0].heading, "1 file in a format EyeCanSee cannot write; Resize > Custom… can save it in another one:");
        for (id, _) in failed.iter().chain(&[(SKIP_INSIDE, String::new()), (SKIP_VIDEO, String::new()), (SKIP_AUDIO, String::new()), (SKIP_NOT_AUTOCAD, String::new()), (SKIP_WOULD_LOSE, String::new())]) {
            for lang in ["en", "it"] {
                let g = groups(&I18n::new(lang), &[(id, "x".into())], false);
                assert!(!g[0].heading.contains("summary-"), "{lang} {id}: {}", g[0].heading);
            }
        }
    }
}
