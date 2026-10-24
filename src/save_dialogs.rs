//! Save windows: confirmation in the viewer, Transform on the grid, and the message windows.

use eframe::egui;

use crate::actions::{Act, transform_items};
use crate::{App, BOLD, confirm, convert, source, transform};

impl App {
    /// Confirmation of a transform on images of the grid: they are overwritten (originals where a
    /// deletion sends them), lossy formats may lose a little quality, the others are left out.
    /// Once saved, their thumbnails are made again.
    pub(crate) fn batch_turn_window(&mut self, ctx: &egui::Context) {
        let Some((paths, op)) = self.batch_turn.clone() else { return };
        let Some(loader) = &self.loader else { return };
        // Images still being saved stay out: they would be turned twice.
        let paths = paths.into_iter().filter(|p| !self.saving.contains(p));
        let rename = loader.can_rename();
        let (ok, skipped): (Vec<String>, Vec<String>) = paths.partition(|p| rename && convert::Format::of_file(p).is_some());
        let how = ok.first().map_or(source::Deletion::Permanent, |p| loader.deletion(p, self.settings.local_permanent, self.settings.remote_delete));
        let label = transform_items().into_iter().find(|(o, ..)| *o == op).map_or("", |(_, l, _)| l);
        let n = ok.len().to_string();
        let where_id = match how {
            source::Deletion::SystemTrash => "batch-originals-trash",
            source::Deletion::TrashFolder => "batch-originals-folder",
            source::Deletion::Permanent => "batch-originals-permanent",
        };
        let mut text = format!("{}\n\n{}", self.i18n.t_args("batch-turn-confirm", &[("op", &self.i18n.t(label)), ("n", &n)]), self.i18n.t(where_id));
        if ok.iter().any(|p| matches!(convert::Format::of_file(p), Some(convert::Format::Jpeg | convert::Format::Webp | convert::Format::Avif | convert::Format::Gif))) {
            text = format!("{text}\n\n{}", self.i18n.t("batch-lossy"));
        }
        if !ok.is_empty() {
            text = format!("{text}\n\n{}", self.i18n.t("batch-animated"));
        }
        if !rename {
            text = format!("{text}\n\n{}", self.i18n.t("save-no-rename"));
        } else if !skipped.is_empty() {
            text = format!("{text}\n\n{}", self.i18n.t_args("batch-skipped", &[("n", &skipped.len().to_string())]));
        }
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        // Only Esc (as `delete_answer`): Enter does not overwrite many images.
        let keys = self.delete_answer.take().and_then(|yes| (!yes).then_some(false));
        match confirm::yes_no(ctx, "batch-turn", 460.0, text, yes, !ok.is_empty(), no, keys) {
            Some(true) => {
                log!("transform {op:?} on {} images, {how:?}", ok.len());
                self.saving.extend(ok.iter().cloned());
                loader.replace(ok, transform::Turn::default().then(op), how);
                self.batch_saved.get_or_insert(0);
                self.batch_turn = None;
            }
            Some(false) => self.batch_turn = None,
            None => {}
        }
    }

    /// "Save" overwrites only what changed and what can be written back in its own format, where
    /// renaming is possible, and not while that image is being saved already. What is inside the
    /// file (animated, several pages…) is looked at by `Loader::prepare`.
    pub(crate) fn can_save(&self) -> bool {
        let Some(current) = self.viewing else { return false };
        !self.turn.is_none()
            && self.save_ready.is_none()
            && self.loader.as_ref().is_some_and(|l| l.can_rename())
            && self.files().get(current).is_some_and(|f| {
                self.can_change(&f.path) && !self.saving.contains(&f.path) && convert::Format::of_file(&f.path).is_some()
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
            } else if self.loader.as_ref().is_some_and(|l| !l.can_rename()) {
                Some(self.i18n.t("save-no-rename"))
            } else {
                self.viewing
                    .and_then(|i| self.files().get(i))
                    .filter(|f| convert::Format::of_file(&f.path).is_none())
                    .map(|f| self.i18n.t_args("save-format", &[("name", &md(source::name(&f.path)))]))
            };
            if let Some(text) = text {
                self.notice = Some((text, true));
            }
            return;
        }
        let (Some(loader), Some(path)) = (&self.loader, self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone())) else { return };
        log!("prepare {path:?} {:?}", self.turn);
        loader.prepare(path.clone(), self.turn);
        self.saving.insert(path);
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
        let id = match how {
            source::Deletion::SystemTrash => "save-confirm-trash",
            source::Deletion::TrashFolder => "save-confirm-folder",
            source::Deletion::Permanent => "save-confirm-permanent",
        };
        let mut text = self.i18n.t_args(id, &[("name", source::name(&path))]);
        if lossy {
            text = format!("{text}\n\n{}", self.i18n.t("save-lossy"));
        }
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        // Enter and Esc arrive as `delete_answer` (`raw_input_hook`).
        match confirm::yes_no(ctx, "save", 420.0, text, yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                log!("save {path:?} {turn:?} {how:?}");
                loader.replace_with(path.clone(), data, how);
                self.saving.insert(path);
                self.save_ready = None;
            }
            Some(false) => self.save_ready = None,
            None => {}
        }
    }

    /// A message that needs reading (Markdown: names in bold, lists), with "Save as…" for the
    /// open image when it helps.
    pub(crate) fn notice_window(&mut self, ctx: &egui::Context) {
        let Some((mut text, save_as)) = self.notice.clone() else { return };
        let (copy, close_label) = (self.i18n.t("menu-save-copy"), self.i18n.t("notice-close"));
        if save_as {
            text = format!("{text}\n\n{}", self.i18n.t("save-as-hint"));
        }
        // Enter and Esc arrive as `delete_answer` (`raw_input_hook`).
        let mut close = self.delete_answer.take().is_some();
        egui::Modal::new(egui::Id::unique("notice")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            notice_text(ui, &text);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if save_as && ui.button(copy).clicked() {
                    self.act = Some(Act::Convert(None));
                    close = true;
                }
                if ui.button(close_label).clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.notice = None;
        }
    }
}

/// Text of a message window: paragraphs apart, `**bold**`, lines starting with "- " as a list,
/// `\` before a punctuation mark to keep it as it is (`md`).
fn notice_text(ui: &mut egui::Ui, text: &str) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let bold = egui::FontId::new(font.size, egui::FontFamily::Name(BOLD.into()));
    let color = ui.visuals().text_color();
    let format = |strong: bool| egui::TextFormat::simple(if strong { bold.clone() } else { font.clone() }, color);
    for (i, paragraph) in text.split("\n\n").enumerate() {
        if i > 0 {
            ui.add_space(6.0);
        }
        for line in paragraph.lines() {
            let mut job = egui::text::LayoutJob::default();
            let line = match line.strip_prefix("- ") {
                Some(item) => {
                    job.append("•  ", 8.0, format(false));
                    item
                }
                None => line,
            };
            let (mut strong, mut run, mut chars) = (false, String::new(), line.chars().peekable());
            while let Some(c) = chars.next() {
                match c {
                    '\\' if chars.peek().is_some_and(|n| n.is_ascii_punctuation()) => run.extend(chars.next()),
                    '*' if chars.peek() == Some(&'*') => {
                        chars.next();
                        job.append(&std::mem::take(&mut run), 0.0, format(strong));
                        strong = !strong;
                    }
                    c => run.push(c),
                }
            }
            job.append(&run, 0.0, format(strong));
            ui.add(egui::Label::new(job).wrap());
        }
    }
}

/// `s` shown as it is in Markdown: every ASCII punctuation mark escaped (a name with `*` or `_`).
pub(crate) fn md(s: &str) -> String {
    s.chars().flat_map(|c| c.is_ascii_punctuation().then_some('\\').into_iter().chain([c])).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn markdown_escape() {
        assert_eq!(super::md("a_b*c.jpg"), r"a\_b\*c\.jpg");
        assert_eq!(super::md("foto è"), "foto è");
    }
}
