//! Save windows: confirmation in the viewer, Transform on the grid, and the message windows.

use eframe::egui;

use crate::actions::{Act, transform_items};
use crate::navigation::Go;
use crate::{App, BOLD, confirm, encode, source, transform};

/// What was asked while the image in the viewer had flips or turns not saved: done once the
/// question is answered (`turn_leave_window`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum After {
    Go(Go),
    Close,
    Up,
    Quit,
    /// A bookmark, by its place in the list (`App::open_bookmark`).
    Bookmark(usize),
}

/// Answers to "save the changes?" before leaving the image.
#[derive(Clone, Copy)]
enum Leave {
    Save,
    SaveAs,
    Discard,
}

impl App {
    /// The image in the viewer has flips or turns not saved, and not being saved.
    pub(crate) fn turn_unsaved(&self) -> bool {
        !self.turn.is_none() && self.viewing.and_then(|i| self.files().get(i)).is_some_and(|f| !self.saving.contains(&f.path))
    }

    /// Leaving the image would lose its changes: `after` waits and the question opens (`true`).
    /// Changes are never dropped without asking, as in Rotate and crop (asked by the user).
    pub(crate) fn hold_for_turn(&mut self, after: After) -> bool {
        let hold = self.turn_unsaved() && self.save_ready.is_none();
        if hold {
            log!("changes not saved: asking before {after:?}");
            self.after_turn = Some(after);
        }
        hold
    }

    /// What was asked, now that the changes are saved or dropped.
    fn run_after(&mut self, ctx: &egui::Context, after: After) {
        self.turn = Default::default();
        match after {
            After::Go(to) => self.go(to),
            After::Close => self.close_viewer(),
            After::Up => self.go_up(),
            After::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            After::Bookmark(i) => self.open_bookmark(ctx, i),
        }
    }

    /// Save, Save as…, Don't save, or Cancel (stays on the image). Save goes through the usual
    /// confirmation (`save_window`), which then does what was asked.
    pub(crate) fn turn_leave_window(&mut self, ctx: &egui::Context) {
        let Some(after) = self.after_turn else { return };
        let Some(path) = self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone()) else {
            self.after_turn = None;
            return;
        };
        let text = self.i18n.t_args("rc-leave", &[("name", &md(source::name(&path)))]);
        let choices = vec![
            (self.i18n.t("rc-save"), self.can_save(), Leave::Save),
            (self.i18n.t("menu-save-copy"), true, Leave::SaveAs),
            (self.i18n.t("rc-discard"), true, Leave::Discard),
        ];
        // Esc: back to the image; Enter does nothing here (no choice is safe to guess).
        let keys = self.delete_answer.take().filter(|yes| !yes).map(|_| None);
        let Some(answer) = confirm::choose(ctx, "turn-leave", 440.0, (self.i18n.t("leave-title"), text), choices, self.i18n.t("delete-no"), keys) else { return };
        self.after_turn = None;
        match answer {
            Some(Leave::Save) => {
                self.after_save = Some(after);
                self.ask_save();
            }
            Some(Leave::SaveAs) => self.act = Some(Act::Convert(None)),
            Some(Leave::Discard) => self.run_after(ctx, after),
            None => {}
        }
    }

    /// Confirmation of a transform on images of the grid: they are overwritten (originals where a
    /// deletion sends them), lossy formats may lose a little quality, the others are left out.
    /// Once saved, their thumbnails are made again.
    pub(crate) fn batch_turn_window(&mut self, ctx: &egui::Context) {
        let Some((paths, op)) = self.batch_turn.clone() else { return };
        let Some(loader) = &self.loader else { return };
        // Images still being saved stay out: they would be turned twice.
        let paths = paths.into_iter().filter(|p| !self.saving.contains(p));
        let paths: Vec<String> = paths.collect();
        // One folder: the reason, if any, is the same for all.
        let refused = paths.first().and_then(|p| loader.no_overwrite(p));
        let (ok, skipped): (Vec<String>, Vec<String>) = paths.into_iter().partition(|p| refused.is_none() && (encode::saves_back(p) || crate::video_turn::turned_by(p, op)));
        if ok.is_empty() {
            self.batch_turn_none(ctx, &skipped, refused, op);
            return;
        }
        let how = ok.first().map_or(source::Deletion::Permanent, |p| loader.deletion(p, self.settings.local_permanent, self.settings.remote_delete));
        let label = self.op_label(op);
        let n = ok.len().to_string();
        let where_id = match how {
            source::Deletion::SystemTrash => "batch-originals-trash",
            source::Deletion::TrashFolder => "batch-originals-folder",
            source::Deletion::Permanent => "batch-originals-permanent",
        };
        let mut text = format!("{}\n\n{}", self.i18n.t_args("batch-turn-confirm", &[("n", &n)]), self.i18n.t(where_id));
        if ok.iter().any(|p| matches!(encode::Format::of_file(p), Some(encode::Format::Jpeg | encode::Format::Webp | encode::Format::Avif | encode::Format::Gif))) {
            text = format!("{text}\n\n{}", self.i18n.t("batch-lossy"));
        }
        if ok.iter().any(|p| crate::psd::is_psd(p)) {
            text = format!("{text}\n\n{}", self.i18n.t("batch-psd-flatten"));
        }
        if !ok.is_empty() {
            text = format!("{text}\n\n{}", self.i18n.t("batch-animated"));
        }
        if ok.iter().any(|p| crate::decode::is_video(p)) {
            text = format!("{text}\n\n{}", self.i18n.t("batch-turn-videos"));
        }
        if let Some(id) = refused {
            text = format!("{text}\n\n{}", self.i18n.t(id));
        } else if !skipped.is_empty() {
            text = format!("{text}\n\n{}", self.i18n.t_args("batch-skipped", &[("n", &skipped.len().to_string())]));
        }
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        // Only Esc (as `delete_answer`): Enter does not overwrite many images.
        let keys = self.delete_answer.take().and_then(|yes| (!yes).then_some(false));
        match confirm::yes_no(ctx, "batch-turn", 460.0, (label.clone(), text), yes, !ok.is_empty(), no, keys) {
            Some(true) => {
                log!("transform {op:?} on {} images, {how:?}", ok.len());
                self.saving.extend(ok.iter().cloned());
                loader.replace(ok, transform::Turn::default().then(op), how);
                self.batch_saved.get_or_insert((op, 0));
                self.batch_turn = None;
            }
            Some(false) => self.batch_turn = None,
            None => {}
        }
    }

    /// The name of a transform as its menu item says it: the title of its windows.
    fn op_label(&self, op: transform::Op) -> String {
        self.i18n.t(op_id(op))
    }

    /// Not one of the images can be saved over itself: say why instead of "up to 0 images will be
    /// overwritten", and for a single image offer "Save as…", already turned.
    fn batch_turn_none(&mut self, ctx: &egui::Context, skipped: &[String], refused: Option<&str>, op: transform::Op) {
        let single = (skipped.len() == 1).then(|| skipped[0].clone());
        let mut text = if let Some(id) = refused {
            self.i18n.t(id)
        } else if let Some(p) = &single {
            self.i18n.t_args("save-format", &[("name", &md(source::name(p)))])
        } else {
            self.i18n.t_args("batch-none", &[("n", &skipped.len().to_string())])
        };
        if single.is_some() {
            text = format!("{text}\n\n{}", self.i18n.t("batch-save-as-hint"));
        }
        let choices = single.iter().map(|p| (self.i18n.t("menu-save-copy"), true, p.clone())).collect();
        // Enter: Save as… (when there is one); Esc: cancel.
        let keys = self.delete_answer.take().map(|yes| if yes { single.clone() } else { None });
        match confirm::choose(ctx, "batch-turn", 460.0, (self.op_label(op), text), choices, self.i18n.t("delete-no"), keys) {
            Some(Some(path)) => {
                self.batch_turn = None;
                self.open_convert(Some(path));
                if let Some(w) = &mut self.convert {
                    w.options.turn = transform::Turn::default().then(op);
                }
            }
            Some(None) => self.batch_turn = None,
            None => {}
        }
    }

    /// "Save" overwrites only what changed and what can be written back in its own format, where
    /// renaming is possible, not in a trash, and not while that image is being saved already. What is inside the
    /// file (animated, several pages…) is looked at by `Loader::prepare`.
    pub(crate) fn can_save(&self) -> bool {
        let Some(current) = self.viewing else { return false };
        !self.turn.is_none()
            && self.save_ready.is_none()
            && self.files().get(current).is_some_and(|f| {
                self.loader.as_ref().is_some_and(|l| l.can_overwrite(&f.path))
                    && self.can_change(&f.path)
                    && !self.saving.contains(&f.path)
                    && (encode::saves_back(&f.path) || crate::video_turn::turned_by(&f.path, transform::Op::Cw))
                    && !source::in_trash(&f.path)
            })
    }

    /// Is `path` the image open in the viewer?
    pub(crate) fn viewed(&self, path: &str) -> bool {
        self.viewing.and_then(|i| self.files().get(i)).is_some_and(|f| f.path == path)
    }

    /// Ctrl+S: the image is turned in the background, then the confirmation opens (`Msg::Prepared`).
    pub(crate) fn ask_save(&mut self) {
        if !self.can_save() {
            // Changed but not saveable over itself: say why and offer "Save as…".
            let text = if self.turn.is_none() || self.save_ready.is_some() {
                None
            } else if let Some(id) = self.viewing.and_then(|i| self.files().get(i)).and_then(|f| self.loader.as_ref()?.no_overwrite(&f.path)) {
                Some(self.i18n.t(id))
            } else {
                self.viewing
                    .and_then(|i| self.files().get(i))
                    .filter(|f| !encode::saves_back(&f.path))
                    .map(|f| self.i18n.t_args("save-format", &[("name", &md(source::name(&f.path)))]))
            };
            if let Some(text) = text {
                self.notice = Some(Notice { save_as: Some(Act::Convert(None)), ..Notice::new(self.i18n.t("save-not-possible"), text) });
            }
            return;
        }
        let (Some(loader), Some(path)) = (&self.loader, self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone())) else { return };
        log!("prepare {path:?} {:?}", self.turn);
        loader.prepare(path.clone(), self.turn);
        self.saving.insert(path.clone());
        self.preparing = Some(path);
    }

    /// Overwrite? The original goes where a deletion would send it (the trash, usually). What is
    /// written is the file prepared for this image and turn; if either changed, it is dropped.
    pub(crate) fn save_window(&mut self, ctx: &egui::Context) {
        let Some((path, turn, data, lossy)) = self.save_ready.clone() else { return };
        if !self.viewed(&path) || turn != self.turn {
            self.save_ready = None;
            return;
        }
        let Some(loader) = &self.loader else { return };
        let how = loader.deletion(&path, self.settings.local_permanent, self.settings.remote_delete);
        let text = self.save_confirm_text(&path, how, lossy);
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        // Enter and Esc arrive as `delete_answer` (`raw_input_hook`).
        match confirm::yes_no(ctx, "save", 420.0, (self.i18n.t("save-title"), text), yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                log!("save {path:?} {turn:?} {how:?}");
                // A video: its rotation written when it is saved, nothing prepared (#125).
                if crate::decode::is_video(&path) {
                    loader.replace(vec![path.clone()], turn, how);
                } else {
                    loader.replace_with(path.clone(), data, how);
                }
                self.saving.insert(path);
                self.save_ready = None;
                // Saved on the way out of the image: now what was asked.
                if let Some(after) = self.after_save.take() {
                    self.run_after(ctx, after);
                }
            }
            Some(false) => {
                self.save_ready = None;
                self.after_save = None;
            }
            None => {}
        }
    }

    /// Overwrite `path`? Where the original goes, and whether the new file lost quality.
    pub(crate) fn save_confirm_text(&self, path: &str, how: source::Deletion, lossy: bool) -> String {
        let id = match how {
            source::Deletion::SystemTrash => "save-confirm-trash",
            source::Deletion::TrashFolder => "save-confirm-folder",
            source::Deletion::Permanent => "save-confirm-permanent",
        };
        let text = self.i18n.t_args(id, &[("name", &md(source::name(path)))]);
        // A PSD is "lossy" when it had layers, text or effects: it is flattened.
        let warning = if crate::psd::is_psd(path) { "save-psd-flatten" } else { "save-lossy" };
        if lossy { format!("{text}\n\n{}", self.i18n.t(warning)) } else { text }
    }

    /// A message that needs reading (Markdown: names in bold, lists), with "Save as…" for the
    /// open image when it helps.
    pub(crate) fn notice_window(&mut self, ctx: &egui::Context) {
        let Some(Notice { title, mut text, save_as, warning }) = self.notice.clone() else { return };
        let (copy, close_label) = (self.i18n.t("menu-save-copy"), self.i18n.t("notice-close"));
        if save_as.is_some() {
            text = format!("{text}\n\n{}", self.i18n.t("save-as-hint"));
        }
        // Enter and Esc arrive as `delete_answer` (`raw_input_hook`).
        let mut close = self.delete_answer.take().is_some();
        if close {
            log!("notice: closed with Enter or Esc");
        }
        let stalled = self.stall_notice.as_ref() == Some(&text);
        let (why, url) = (self.i18n.t("phone-stalled-why"), self.i18n.t("phone-stalled-url"));
        message(ctx, "notice", Head { title: &title, warning }, &text, |ui| {
            // A stalled phone (`phone_stall`): our page on why, in the app's language.
            if stalled {
                ui.hyperlink_to(why, url);
            }
            ui.horizontal(|ui| {
                if let Some(a) = &save_as
                    && ui.button(copy).clicked()
                {
                    self.act = Some(a.clone());
                    close = true;
                }
                if ui.button(close_label).clicked() {
                    log!("notice: Close");
                    close = true;
                }
            });
        });
        if close {
            self.notice = None;
        }
    }

    /// A message window titled with the Fluent id `title`.
    pub(crate) fn tell(&mut self, title: &str, text: String) {
        self.notice = Some(Notice::new(self.i18n.t(title), text));
    }
}

/// The Fluent id of a transform's menu item; also the key of its variants in "batch-done".
pub(crate) fn op_id(op: transform::Op) -> &'static str {
    transform_items().into_iter().find(|(o, ..)| *o == op).map_or("", |(_, l, _)| l)
}

/// A message to read (`App::notice`): what it is about, what happened, and "Save as…" for the
/// open image when that helps. A box with only "Images saved: 2." left the user wondering what
/// had happened (2026-11-10): every message has its title.
#[derive(Clone)]
pub(crate) struct Notice {
    pub(crate) title: String,
    pub(crate) text: String,
    pub(crate) save_as: Option<Act>,
    /// A warning to take in: the yellow triangle over the title (`theme::warning_icon`).
    pub(crate) warning: bool,
}

impl Notice {
    pub(crate) fn new(title: String, text: String) -> Self {
        Notice { title, text, save_as: None, warning: false }
    }
}

/// The top of a message or confirmation window: its title, centred, under the warning triangle
/// when `warning`.
#[derive(Clone, Copy)]
pub(crate) struct Head<'a> {
    pub(crate) title: &'a str,
    pub(crate) warning: bool,
}

impl<'a> Head<'a> {
    pub(crate) fn new(title: &'a str) -> Self {
        Head { title, warning: false }
    }

    pub(crate) fn show(self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            if self.warning {
                crate::theme::warning_icon(ui, 30.0);
                ui.add_space(2.0);
            }
            ui.heading(self.title);
        });
        ui.add_space(6.0);
    }
}

/// A message window, as all of them look (issue #11): modal, 320 to 460 points wide, the title
/// (`Head`), the text (`notice_text`), then what `below` adds (its buttons).
pub(crate) fn message(ctx: &egui::Context, id: &str, head: Head, text: &str, below: impl FnOnce(&mut egui::Ui)) {
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(460.0);
        // A short message ("Images saved: 1.") made a box so small it went unnoticed.
        ui.set_min_width(320.0);
        head.show(ui);
        notice_text(ui, text);
        ui.add_space(8.0);
        below(ui);
    });
}

/// Text of a message window: paragraphs apart, `**bold**`, lines starting with "- " as a list,
/// `\` before a punctuation mark to keep it as it is (`md`).
pub(crate) fn notice_text(ui: &mut egui::Ui, text: &str) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let bold = egui::FontId::new(font.size, egui::FontFamily::Name(BOLD.into()));
    let mono = egui::FontId::monospace(font.size + 1.0);
    let (color, code_bg) = (ui.visuals().text_color(), ui.visuals().code_bg_color);
    let strong_color = ui.visuals().strong_text_color();
    let format = |look: Look| match look {
        Look::Plain => egui::TextFormat::simple(font.clone(), color),
        Look::Bold => egui::TextFormat::simple(bold.clone(), color),
        // What is to be compared (a server's fingerprint): apart, on a field of its own (issue #11).
        Look::Code => egui::TextFormat { font_id: mono.clone(), color: strong_color, background: code_bg, ..Default::default() },
    };
    for (i, paragraph) in text.split("\n\n").enumerate() {
        if i > 0 {
            ui.add_space(6.0);
        }
        for line in paragraph.lines() {
            let mut job = egui::text::LayoutJob::default();
            let line = match line.strip_prefix("- ") {
                Some(item) => {
                    job.append("•  ", 8.0, format(Look::Plain));
                    item
                }
                None => line,
            };
            for (run, look) in spans(line) {
                job.append(&run, 0.0, format(look));
            }
            ui.add(egui::Label::new(job).wrap());
        }
    }
}

/// How a run of a message is shown.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Look {
    Plain,
    Bold,
    /// Between backquotes: to be compared, letter by letter.
    Code,
}

/// The runs of a line of a message: `**bold**`, `` `code` ``, `\` before a punctuation mark to
/// keep it as it is.
fn spans(line: &str) -> Vec<(String, Look)> {
    let (mut out, mut run, mut look, mut chars) = (Vec::new(), String::new(), Look::Plain, line.chars().peekable());
    let flush = |out: &mut Vec<(String, Look)>, run: &mut String, look: Look| {
        if !run.is_empty() {
            out.push((std::mem::take(run), look));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' if look != Look::Code && chars.peek().is_some_and(|n| n.is_ascii_punctuation()) => run.extend(chars.next()),
            '*' if look != Look::Code && chars.peek() == Some(&'*') => {
                chars.next();
                flush(&mut out, &mut run, look);
                look = if look == Look::Bold { Look::Plain } else { Look::Bold };
            }
            '`' => {
                flush(&mut out, &mut run, look);
                look = if look == Look::Code { Look::Plain } else { Look::Code };
            }
            c => run.push(c),
        }
    }
    flush(&mut out, &mut run, look);
    out
}

/// `s` shown as it is in Markdown: every ASCII punctuation mark escaped (a name with `*` or `_`).
pub(crate) fn md(s: &str) -> String {
    s.chars().flat_map(|c| c.is_ascii_punctuation().then_some('\\').into_iter().chain([c])).collect()
}

#[cfg(test)]
mod tests {
    /// Bold, code (a fingerprint, its stars and backslashes as they are) and escaped marks.
    #[test]
    fn message_runs() {
        use super::Look::*;
        let runs = super::spans(r"**First** key `SHA256:a*b\c` is \*ok\*");
        assert_eq!(runs, [("First".into(), Bold), (" key ".into(), Plain), (r"SHA256:a*b\c".into(), Code), (" is *ok*".into(), Plain)]);
    }

    #[test]
    fn markdown_escape() {
        assert_eq!(super::md("a_b*c.jpg"), r"a\_b\*c\.jpg");
        assert_eq!(super::md("foto è"), "foto è");
    }
}
