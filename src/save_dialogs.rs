//! Save windows: confirmation in the viewer, Transform on the grid, and the message windows.

use eframe::egui;

use crate::actions::{Act, transform_items};
use crate::navigation::Go;
use crate::{App, BOLD, confirm, encode, source, transform};

/// What was asked while the image in the viewer had flips or turns not saved: done once the
/// question is answered (`turn_leave_window`).
#[derive(Clone, Debug)]
pub(crate) enum After {
    Go(Go),
    Close,
    Up,
    Quit,
    /// A bookmark, by its place in the list (`App::open_bookmark`).
    Bookmark(usize),
    /// Open the folder of the summary at the end of a conversion (`batch_summary`), or an address
    /// typed or chosen (`App::open_source`).
    Open(String),
    /// An address from outside: another start of EyeCanSee, the file manager (`App::open_outside`).
    Outside(String),
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
        // Quitting: the turn is left on screen, not taken back for the last frames.
        if matches!(after, After::Quit) {
            self.quit_dropped_turn = true;
        } else {
            self.turn = Default::default();
        }
        match after {
            After::Go(to) => self.go(to),
            After::Close => self.close_viewer(),
            After::Up => self.go_up(),
            After::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            After::Bookmark(i) => self.open_bookmark(ctx, i),
            After::Open(address) => self.open_source(ctx, &address),
            After::Outside(address) => self.open_outside(ctx, &address),
        }
    }

    /// Save, Save as…, Don't save, or Cancel (stays on the image). Save goes through the usual
    /// confirmation (`save_window`), which then does what was asked.
    pub(crate) fn turn_leave_window(&mut self, ctx: &egui::Context) {
        let Some(after) = self.after_turn.clone() else { return };
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
        // A video turned: Save as… would save its frame (#16), not the video.
        let choices: Vec<_> = choices.into_iter().filter(|c| !(self.viewing_video() && matches!(c.2, Leave::SaveAs))).collect();
        // Esc: back to the image; Enter does nothing here (no choice is safe to guess).
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, "turn-leave").filter(|yes| !yes).map(|_| None);
        let Some(answer) = confirm::choose(ctx, "turn-leave", 440.0, (self.i18n.t("leave-title"), text), choices, self.i18n.t("delete-no"), keys, 0) else { return };
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
        // Videos are named as videos: "1 image will be changed" on a video (Piero, 2026-11-18).
        let videos = ok.iter().filter(|p| crate::decode::is_video(p)).count();
        let what = saved_what(ok.len(), videos);
        let mut text = format!("{}\n\n{}", self.i18n.t_args("batch-turn-confirm", &[("n", &n), ("what", what)]), self.i18n.t(where_id));
        if ok.iter().any(|p| matches!(encode::Format::of_file(p), Some(encode::Format::Jpeg | encode::Format::Webp | encode::Format::Avif | encode::Format::Gif))) {
            text = format!("{text}\n\n{}", self.i18n.t("batch-lossy"));
        }
        if ok.iter().any(|p| crate::psd::is_psd(p)) {
            text = format!("{text}\n\n{}", self.i18n.t("batch-psd-flatten"));
        }
        // What is skipped among pictures: said only when there are pictures.
        if ok.len() > videos {
            text = format!("{text}\n\n{}", self.i18n.t("batch-animated"));
        }
        if videos > 0 {
            text = format!("{text}\n\n{}", self.i18n.t("batch-turn-videos"));
        }
        if let Some(id) = refused {
            text = format!("{text}\n\n{}", self.i18n.t(id));
        } else if !skipped.is_empty() {
            text = format!("{text}\n\n{}", self.i18n.t_args("batch-skipped", &[("n", &skipped.len().to_string())]));
        }
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        // Only Esc (as `answer`): Enter does not overwrite many images.
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, "batch-turn").and_then(|yes| (!yes).then_some(false));
        match confirm::yes_no(ctx, "batch-turn", 460.0, (label.clone(), text), yes, !ok.is_empty(), no, keys, true) {
            Some(true) => {
                log!("transform {op:?} on {} images, {how:?}", ok.len());
                self.saving.extend(ok.iter().cloned());
                loader.replace(ok, transform::Turn::default().then(op), how);
                self.batch_saved.get_or_insert((op, 0, 0));
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
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, "batch-turn").map(|yes| if yes { single.clone() } else { None });
        match confirm::choose(ctx, "batch-turn", 460.0, (self.op_label(op), text), choices, self.i18n.t("delete-no"), keys, 0) {
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
                // Not on a video, whose Save as… saves its frame (#16).
                let save_as = (!self.viewing_video()).then_some(Act::Convert(None));
                self.tell_notice(Notice { save_as, ..Notice::new(self.i18n.t("save-not-possible"), text) });
            }
            return;
        }
        let Some(path) = self.viewing.and_then(|i| self.files().get(i)).map(|f| f.path.clone()) else { return };
        // A turned video is saved by FFmpeg: without one, offered first (#169).
        if crate::decode::is_video(&path) && self.ffmpeg_missing(crate::ffmpeg_ask::Then::Save) {
            return;
        }
        let Some(loader) = &self.loader else { return };
        log!("prepare {path:?} {:?}", self.turn);
        loader.prepare(path.clone(), self.turn, false);
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
        // Enter and Esc arrive as `answer` (`raw_input_hook`).
        let answer = crate::confirm::answer(ctx, &mut self.key_answer, "save");
        let safe = how == crate::source::Deletion::Permanent;
        match confirm::yes_no(ctx, "save", 420.0, (self.i18n.t("save-title"), text), yes, true, no, answer, safe) {
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
        let Some(Notice { title, mut text, save_as, warning, copy: mut whole, open, .. }) = self.notice.clone() else { return };
        let (copy, close_label) = (self.i18n.t("menu-save-copy"), self.i18n.t("notice-close"));
        let (open_label, covered_hint, covered) = (self.i18n.t("summary-open-folder"), self.i18n.t("summary-open-covered"), self.folder_covered());
        let mut go = None;
        if save_as.is_some() {
            let hint = self.i18n.t("save-as-hint");
            text = format!("{text}\n\n{hint}");
            whole = whole.map(|w| format!("{w}\n\n{}", plain(&hint)));
        }
        // Enter and Esc arrive as `answer` (`raw_input_hook`).
        let id = "notice";
        let mut close = crate::confirm::answer(ctx, &mut self.key_answer, id) == Some(false);
        if close {
            log!("notice: closed with Esc");
        }
        let stalled = self.stall_notice.as_ref() == Some(&text);
        let (why, url) = (self.i18n.t("phone-stalled-why"), self.i18n.t("phone-stalled-url"));
        let tip = self.i18n.t("notice-copy");
        message_copying(ctx, id, Head { title: &title, warning }, &text, whole.as_deref(), &tip, |ui| {
            // A stalled phone (`phone_stall`): our page on why, in the app's language.
            if stalled {
                crate::confirm::join(id, &ui.hyperlink_to(why, url));
            }
            ui.horizontal(|ui| {
                // Close highlighted; Save as… before it when it helps (#311); Open the folder
                // after it, as at the end of Convert (#356).
                let mut labels = Vec::new();
                if save_as.is_some() {
                    labels.push((copy, true));
                }
                labels.push((close_label, true));
                let at = labels.len() - 1;
                if open.is_some() {
                    labels.push((open_label, !covered));
                }
                let mut hints = vec![None; at + 1];
                hints.push(Some(covered_hint));
                if let Some(i) = crate::confirm::buttons_hint(ui, id, &labels, at, &hints) {
                    if i < at
                        && let Some(a) = &save_as
                    {
                        self.act = Some(a.clone());
                    }
                    if i > at {
                        go = open.clone();
                    }
                    log!("notice: closed by its buttons");
                    close = true;
                }
            });
        });
        if let Some(address) = go {
            log!("notice: Open the folder {:?}", crate::address::without_password(&address));
            self.open_folder_after_batch(ctx, address);
        }
        if close {
            self.notice = None;
            self.next_waiting();
        }
    }

    /// A message window titled with the Fluent id `title`; behind the message open, if any
    /// (audit 27/11 A.3: it took its place).
    pub(crate) fn tell(&mut self, title: &str, text: String) {
        self.tell_notice(Notice::new(self.i18n.t(title), text));
    }
}

/// What the grid's flip or turn saved, for the message at the end (`$what` of "batch-done"):
/// `n` files, `videos` of them videos (#125).
pub(crate) fn saved_what(n: usize, videos: usize) -> &'static str {
    match videos {
        0 => "images",
        v if v == n => "videos",
        _ => "mixed",
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
    /// What Copy takes when it is more than the text shows (every name of a list cut short,
    /// `notice_list`); `None`: the text.
    pub(crate) copy: Option<String>,
    /// The page of the guide that says more, when the message names it ("More in the guide
    /// (F1)."): F1 opens it while the message is up (`help_spot`). `None`: the page of what is
    /// under the message.
    pub(crate) help: Option<&'static str>,
    /// The end of a job in a corner box (a copy): another job's part joins it (#328).
    pub(crate) job: bool,
    /// A folder Open the folder opens, after Close (#356: the end of a copy into a folder that is
    /// not the one open).
    pub(crate) open: Option<String>,
}

impl Notice {
    pub(crate) fn new(title: String, text: String) -> Self {
        Notice { title, text, save_as: None, warning: false, copy: None, help: None, job: false, open: None }
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
/// At the right of the last row of buttons, a small Copy: the text without its Markdown (it could
/// not be copied, Piero 20/11).
/// `tip`: Copy's tooltip ("notice-copy").
pub(crate) fn message(ctx: &egui::Context, id: &str, head: Head, text: &str, tip: &str, below: impl FnOnce(&mut egui::Ui)) {
    message_copying(ctx, id, head, text, None, tip, below);
}

/// `message`, with Copy taking `whole` instead of the text, when it is given (a list cut short).
pub(crate) fn message_copying(ctx: &egui::Context, id: &str, head: Head, text: &str, whole: Option<&str>, tip: &str, below: impl FnOnce(&mut egui::Ui)) {
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(460.0);
        // A short message ("Images saved: 1.") made a box so small it went unnoticed.
        ui.set_min_width(320.0);
        head.show(ui);
        notice_text(ui, text);
        ui.add_space(8.0);
        below(ui);
        copy_button(ui, id, tip, || whole.map_or_else(|| plain(text), str::to_string));
    });
}

/// The small Copy at the right of the last row of a message window's buttons: puts `text()` on
/// the clipboard. Tab reaches it after the buttons of the window `id` (#311, Piero 24/11).
pub(crate) fn copy_button(ui: &mut egui::Ui, id: &str, tip: &str, text: impl FnOnce() -> String) {
    let r = ui.min_rect();
    let at = egui::Rect::from_min_max(egui::pos2(r.right() - 22.0, r.bottom() - 22.0), r.right_bottom());
    let copy = ui.put(at, crate::icons::small(ui, crate::icons::Line::Copy)).on_hover_text(tip);
    crate::confirm::join(id, &copy);
    if copy.clicked() {
        ui.ctx().copy_text(text());
    }
}

/// A message's text as it reads, without the Markdown of `notice_text`: for the clipboard.
pub(crate) fn plain(text: &str) -> String {
    let lines = text.lines().map(|l| spans(l).into_iter().map(|(run, _)| run).collect::<String>());
    lines.collect::<Vec<_>>().join("\n")
}

/// Text of a message window: paragraphs apart, `**bold**`, lines starting with "- " as a list,
/// `\` before a punctuation mark to keep it as it is (`md`). Every item of every list has its
/// bullet and its text at the same place, and the lines of an item that wraps stay under its
/// first, right of the bullet (Piero, 21/11: two lists in a window did not look alike).
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
            let item = line.strip_prefix("- ");
            for (run, look) in spans(item.unwrap_or(line)) {
                job.append(&run, 0.0, format(look));
            }
            if item.is_none() {
                ui.add(egui::Label::new(job).wrap());
                continue;
            }
            // A row no taller than its text: the lines of a list as close as those of a paragraph.
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 0.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(8.0);
                ui.add(egui::Label::new(egui::text::LayoutJob::single_section("•".into(), format(Look::Plain))));
                ui.add_space(8.0);
                ui.add(egui::Label::new(job).wrap());
            });
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
pub(crate) mod tests {
    /// The message at the end says what was turned (#125): pictures, videos or both, in every
    /// language and number.
    #[test]
    fn rotated_videos_said_as_videos() {
        assert_eq!((super::saved_what(2, 0), super::saved_what(1, 1), super::saved_what(3, 1)), ("images", "videos", "mixed"));
        let say = |lang: &str, n: usize, videos: usize, op: &str| {
            let args = [("n", n.to_string()), ("op", op.to_string()), ("what", super::saved_what(n, videos).to_string())];
            let i18n = crate::i18n::I18n::new(lang);
            let args = args.each_ref().map(|(k, v)| (*k, v.as_str()));
            (i18n.t_args("batch-done-title", &args), i18n.t_args("batch-done", &args))
        };
        assert_eq!(say("en", 1, 1, "tf-rotate-cw"), ("Video rotated".into(), "**1 video** was rotated 90° clockwise and saved over its original.".into()));
        assert_eq!(say("en", 2, 2, "tf-rotate-180").1, "**2 videos** were rotated 180° and saved over their originals.");
        assert_eq!(say("en", 3, 1, "tf-rotate-ccw"), ("Pictures and videos rotated".into(), "**3 files** (pictures and videos) were rotated 90° anticlockwise and saved over their originals.".into()));
        assert_eq!(say("en", 1, 0, "tf-flip-h"), ("Image flipped".into(), "**1 image** was flipped horizontally and saved over its original.".into()));
        assert_eq!(say("it", 1, 1, "tf-rotate-cw"), ("Video ruotato".into(), "**1 video** è stato ruotato di 90° in senso orario e salvato sopra l'originale.".into()));
        assert_eq!(say("it", 2, 2, "tf-rotate-cw").1, "**2 video** sono stati ruotati di 90° in senso orario e salvati sopra gli originali.");
        assert_eq!(say("it", 3, 1, "tf-rotate-cw").0, "Immagini e video ruotati");
        assert_eq!(say("it", 0, 0, "tf-rotate-cw").1, "Nessuna immagine è stata salvata.");
    }

    /// Bold, code (a fingerprint, its stars and backslashes as they are) and escaped marks.
    #[test]
    fn message_runs() {
        use super::Look::*;
        let runs = super::spans(r"**First** key `SHA256:a*b\c` is \*ok\*");
        assert_eq!(runs, [("First".into(), Bold), (" key ".into(), Plain), (r"SHA256:a*b\c".into(), Code), (" is *ok*".into(), Plain)]);
    }

    /// The question before a turn in the grid names videos as videos ("1 image will be changed"
    /// on a video, Piero, 2026-11-18), in both languages.
    #[test]
    fn turn_question_names_videos() {
        for (lang, video, image, mixed) in [("en", "**1 video** will be turned", "**1 image** will be changed", "images and videos"), ("it", "**1 video** verrà ruotato", "**1 immagine** verrà modificata", "immagini e video")] {
            let i18n = crate::i18n::I18n::detect(Some(lang));
            let text = |n: usize, videos: usize| i18n.t_args("batch-turn-confirm", &[("n", &n.to_string()), ("what", super::saved_what(n, videos))]);
            assert!(text(1, 1).starts_with(video), "{}", text(1, 1));
            assert!(text(1, 0).starts_with(image), "{}", text(1, 0));
            assert!(text(3, 1).contains(mixed), "{}", text(3, 1));
        }
    }

    #[test]
    fn markdown_escape() {
        assert_eq!(super::md("a_b*c.jpg"), r"a\_b\*c\.jpg");
        assert_eq!(super::md("foto è"), "foto è");
    }

    /// Where each line of text of `shapes` was drawn: its words, its left edge, and the left edge
    /// of each of its rows.
    pub(crate) fn drawn(shapes: &[eframe::egui::Shape], out: &mut Vec<(String, f32, Vec<f32>)>) {
        for s in shapes {
            match s {
                eframe::egui::Shape::Vec(v) => drawn(v, out),
                eframe::egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.pos.x, t.galley.rows.iter().map(|r| t.pos.x + r.rect().left()).collect())),
                _ => {}
            }
        }
    }

    /// A message with two lists drawn through egui with the app's fonts (Piero, 21/11: the two
    /// lists of a message were not indented alike): every bullet at the same place, every item's
    /// text at the same place right of it, also the rows of an item that wraps and "and N more";
    /// headings and sentences at the left edge, the heading in bold.
    #[test]
    fn lists_drawn_alike() {
        use eframe::egui;
        let long = "a-name-long-enough.gltf: with a reason that goes on and on until the line of the window ends and it has to wrap";
        let text = format!("In the folder\n\n**3 models not exported:**\n- {}\n- {}\n- and 1 more\n\n**1 model changed:**\nA sentence under it.\n- {}", super::md("b.glb"), super::md(long), super::md("c.gltf"));
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
        let mut lines = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(input.clone(), |ui| super::message(ui.ctx(), "t", super::Head { title: "Export finished", warning: true }, &text, "Copy", |_| {}));
            out.textures_delta.clear();
            lines.clear();
            drawn(&out.shapes.iter().map(|c| c.shape.clone()).collect::<Vec<_>>(), &mut lines);
        }
        let at = |words: &str| lines.iter().find(|(t, ..)| t == words).unwrap_or_else(|| panic!("{words:?} not drawn: {lines:?}")).clone();
        let bullets: Vec<f32> = lines.iter().filter(|(t, ..)| t == "•").map(|(_, x, _)| *x).collect();
        assert_eq!(bullets.len(), 4, "{lines:?}");
        assert!(bullets.iter().all(|x| *x == bullets[0]), "bullets not in line: {bullets:?}");
        let items = [at("b.glb"), at(long), at("and 1 more"), at("c.gltf")];
        assert!(items.iter().all(|(_, x, _)| *x == items[0].1 && *x > bullets[0] + 8.0), "items not in line: {items:?}");
        let rows = &items[1].2;
        assert!(rows.len() > 1 && rows.iter().all(|x| (*x - items[0].1).abs() < 0.5), "the rows of an item that wraps: {rows:?}");
        let (left, heading) = (at("In the folder").1, at("3 models not exported:").1);
        assert!(left < bullets[0] && heading == left && at("1 model changed:").1 == left && at("A sentence under it.").1 == left, "{lines:?}");
    }

    /// Copy takes the text as it reads: no `**`, no `\` before a punctuation mark.
    #[test]
    fn plain_text() {
        assert_eq!(super::plain("**a.jpg** was not saved\\.\n- b"), "a.jpg was not saved.\n- b");
    }
}
