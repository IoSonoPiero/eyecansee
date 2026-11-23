//! File windows: paste and its conflicts, transfer progress, delete, rename, restore from the trash.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui::{self, Key};

use crate::{App, PasteAsk, confirm, devices, source, theme, transfer};
use crate::human::human_time;
use crate::navigation::shown;
use crate::save_dialogs::Notice;

/// The check of the folders about to be deleted: running (its stop flag), or what it found (the
/// first name not shown and its folder).
pub(crate) enum DeleteCheck {
    Checking(Arc<std::sync::atomic::AtomicBool>),
    Done(Result<Option<(String, String)>, source::Error>),
}

impl App {
    /// Ctrl+C / Ctrl+X: remembers the items and their source. The paths also go on the system
    /// clipboard as text: egui turns Ctrl+V into a paste only when the clipboard has something.
    pub(crate) fn clip(&mut self, ctx: &egui::Context, path: Option<String>, cut: bool) {
        let paths = self.chosen(path);
        let Some(loader) = &self.loader else { return };
        if paths.is_empty() || paths.iter().any(|p| source::is_trash(p) || source::in_trash(p)) {
            return;
        }
        log!("clip {paths:?} cut {cut}");
        let text = paths.iter().map(|p| self.display(p)).collect::<Vec<_>>().join("\n");
        ctx.copy_text(text.clone());
        self.clip = Some(transfer::Clip { op: loader.store(), base: self.base.clone(), paths, cut, text });
    }

    /// Where a paste can go: a folder of the open source, not its root on a phone, not a trash.
    pub(crate) fn can_paste(&self, dir: &str) -> bool {
        self.clip.is_some()
            && self.transfer.is_none()
            && self.paste_ask.is_none()
            && !(self.base.starts_with("mtp://") && dir.is_empty())
            && !source::is_trash(dir)
    }

    /// Paste from a menu or the toolbar: as Ctrl+V, only while the clipboard still holds the copy
    /// (#61). egui reads it and gives it as a paste in a later frame (`raw_input_hook`).
    pub(crate) fn ask_paste(&mut self, ctx: &egui::Context, dir: Option<String>) {
        let dir = dir.unwrap_or_else(|| self.dir.clone());
        if self.can_paste(&dir) {
            self.paste_to = Some((dir, std::time::Instant::now()));
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
    }

    /// Ctrl+V: into `dir` (`None` = the folder open). First the loader looks for names already there.
    pub(crate) fn paste(&mut self, dir: Option<String>) {
        let dir = dir.unwrap_or_else(|| self.dir.clone());
        if !self.can_paste(&dir) {
            return;
        }
        let (Some(loader), Some(clip)) = (&self.loader, &self.clip) else { return };
        log!("paste into {dir:?}");
        loader.paste_check(clip.clone(), dir.clone());
        self.paste_ask = Some(PasteAsk { dir, conflicts: None, choices: HashMap::new(), all: false });
    }

    fn start_paste(&mut self, dir: String, choices: HashMap<String, transfer::Choice>) {
        let (Some(loader), Some(clip)) = (&self.loader, &self.clip) else { return };
        let progress = Arc::new(transfer::Progress::default());
        loader.paste(clip.clone(), dir.clone(), choices, progress.clone());
        self.transfer = Some((progress, clip.cut, self.base.clone(), dir));
        // One paste per copy (chosen by the user): the clip, and its badges, go.
        self.clip = None;
    }

    /// Asks about each item whose name is already in the destination; then the copy starts.
    pub(crate) fn paste_window(&mut self, ctx: &egui::Context) {
        let over = match (&self.loader, &self.paste_ask) {
            (Some(loader), Some(ask)) => loader.can_paste_over(&ask.dir),
            _ => true,
        };
        let Some(ask) = &mut self.paste_ask else { return };
        let Some(conflicts) = &ask.conflicts else { return };
        let Some(path) = conflicts.first().cloned() else {
            let ask = self.paste_ask.take().unwrap();
            self.start_paste(ask.dir, ask.choices);
            return;
        };
        let many = conflicts.len() > 1;
        let t = |id| self.i18n.t(id);
        let mut text = self.i18n.t_args("paste-taken", &[("name", &crate::save_dialogs::md(source::name(&path)))]);
        let first = if path.ends_with('/') { t("restore-merge") } else { t("restore-replace") };
        let mut buttons = vec![
            (first, transfer::Choice::Replace),
            (t("paste-skip"), transfer::Choice::Skip),
            (t("restore-keep-both"), transfer::Choice::KeepBoth),
        ];
        // Under gvfs nothing is replaced (issue #91): the reason, and no Replace or Merge.
        if !over {
            text = format!("{text}\n\n{}", t("paste-gvfs"));
            buttons.remove(0);
        }
        let (cancel, all_label, title) = (t("delete-no"), t("paste-all"), t("taken-title"));
        let id = "paste";
        let mut answer = crate::confirm::answer(ctx, &mut self.key_answer, id).and_then(|yes| (!yes).then_some(None));
        egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.set_min_width(320.0);
            crate::save_dialogs::Head::new(&title).show(ui);
            crate::save_dialogs::notice_text(ui, &text);
            if many {
                ui.checkbox(&mut ask.all, all_label);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Keep both highlighted: nothing is lost and the paste goes on (#311).
                let mut labels: Vec<(String, bool)> = buttons.iter().map(|(l, _)| (l.clone(), true)).collect();
                labels.push((cancel, true));
                let keep = labels.len() - 2;
                if let Some(i) = crate::confirm::buttons(ui, id, &labels, keep) {
                    answer = Some(buttons.into_iter().nth(i).map(|(_, c)| c));
                }
            });
        });
        match answer {
            Some(Some(choice)) => {
                let conflicts = ask.conflicts.as_mut().unwrap();
                let n = if ask.all { conflicts.len() } else { 1 };
                for p in conflicts.drain(..n) {
                    ask.choices.insert(p, choice);
                }
            }
            Some(None) => self.paste_ask = None,
            None => {}
        }
    }

    /// Progress of the copy, with Cancel; the window stays until the copy ends.
    pub(crate) fn transfer_window(&mut self, ctx: &egui::Context) {
        let Some((p, _, base, _)) = &self.transfer else { return };
        // Folders being filled, read again once a second if they are on screen. Local ones too: the
        // watcher sees only the folder open when the change happens, so a folder listed before and
        // opened during the copy stayed old. After every file it was ten listings a second, slowing the copy down.
        let now = ctx.input(|i| i.time);
        if now - self.touched_at >= 1.0 {
            self.touched_at = now;
            let touched: HashSet<String> = p.touched.lock().unwrap().drain(..).collect();
            if *base == self.base {
                for dir in touched {
                    if self.listings.contains_key(&dir) {
                        self.relist(dir);
                    }
                }
            }
        }
        let Some((p, cut, base, dest)) = &self.transfer else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let phase = p.phase();
        let (current, counts, left) = self.transfer_lines(p, &phase);
        let to = self.i18n.t_args("transfer-to", &[("dir", &shown(base, dest))]);
        let cancelling = p.cancel.load(Ordering::Relaxed);
        egui::Window::new(self.i18n.t(if *cut { "transfer-moving" } else { "transfer-copying" }))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -40.0))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.add(egui::Label::new(egui::RichText::new(current).strong()).truncate());
                ui.add(egui::Label::new(to).truncate());
                ui.label(counts);
                ui.label(left);
                // Accent fill on a grey track: the default (selection blue on white) hid the empty part.
                ui.scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = ui.visuals().widgets.inactive.bg_fill;
                    let fill = theme::accent(ui.visuals());
                    ui.add(egui::ProgressBar::new(phase.fraction()).fill(fill).desired_height(16.0).show_percentage());
                });
                if ui.add_enabled(!cancelling, egui::Button::new(self.i18n.t("delete-no"))).clicked() {
                    p.cancel.store(true, Ordering::Relaxed);
                }
            });
    }

    /// The lines of the copy box (`transfer_window`) for `phase`: what it is on (the file copied),
    /// the counts, and the speed with the time left, which only a copy has.
    fn transfer_lines(&self, p: &transfer::Progress, phase: &transfer::Phase) -> (String, String, String) {
        match *phase {
            // Until the copy starts it is still looking inside the folders, even with nothing found yet.
            transfer::Phase::Looking(found) => (p.current.lock().unwrap().clone(), self.i18n.t_args("transfer-looking", &[("n", &found.to_string())]), String::new()),
            transfer::Phase::Removing { removed, total } => {
                (self.i18n.t("transfer-removing"), self.i18n.t_args("transfer-removed", &[("n", &removed.to_string()), ("total", &total.to_string())]), String::new())
            }
            transfer::Phase::Copying { files, files_total, done, total } => {
                let counts = self.i18n.t_args("transfer-counts", &[
                    ("n", &files.to_string()),
                    ("total", &files_total.to_string()),
                    ("done", &self.human_size(done)),
                    ("size", &self.human_size(total)),
                ]);
                // The speed of the last seconds, the time left from the average since the copy
                // started; the first second gives no meaningful estimate.
                let left = p.started.get().map(|t| t.elapsed().as_secs_f64()).filter(|&secs| secs >= 1.0 && done > 0).map(|secs| {
                    let average = done as f64 / secs;
                    let speed = p.recent.lock().unwrap().speed(secs, done).unwrap_or(average);
                    self.i18n.t_args("transfer-left", &[
                        ("percent", &(done * 100 / total.max(1)).to_string()),
                        ("speed", &self.human_size(speed as u64)),
                        ("time", &human_time((total.saturating_sub(done) as f64 / average) as u64)),
                    ])
                });
                (p.current.lock().unwrap().clone(), counts, left.unwrap_or_default())
            }
        }
    }

    pub(crate) fn ask_delete(&mut self, path: Option<String>, permanent: bool) {
        // Not while a deletion runs: a click repeated while waiting landed on the next item
        // (Download/Pipe, 2026-10-22).
        if self.deleting > 0 {
            return;
        }
        let paths = self.chosen(path);
        self.ask_delete_paths(paths, permanent, None);
    }

    /// The confirmation for `paths`; `size` of their images, when the open folder cannot tell it
    /// (the duplicates, from any folder).
    pub(crate) fn ask_delete_paths(&mut self, paths: Vec<String>, permanent: bool, size: Option<u64>) {
        let Some(path) = paths.first().cloned() else { return };
        let Some(loader) = &self.loader else { return };
        // In the trash everything goes for good: there is no trash of the trash.
        let how = match permanent || source::is_trash(&path) || source::in_trash(&path) {
            true => source::Deletion::Permanent,
            false => loader.deletion(&path, self.settings.local_permanent, self.settings.remote_delete),
        };
        log!("delete? {paths:?} {how:?}");
        let size = size.or_else(|| match paths.len() {
            1 => self.listings.get(&source::listed_in(&path)).and_then(|l| l.as_ref().ok()).and_then(|l| {
                l.files.iter().find(|f| f.path == path).map(|f| f.size)
            }),
            _ => Some(self.images_size(&paths)),
        });
        let ntfs = how == source::Deletion::SystemTrash && !self.base.contains("://") && devices::on_ntfs(&format!("/{}", path.trim_end_matches('/')));
        // Folders: their first level read at once for what the grid does not show (the warning).
        let dirs: Vec<String> = paths.iter().filter(|p| p.ends_with('/') && !source::is_trash(p)).cloned().collect();
        let check = (!dirs.is_empty()).then(|| {
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            loader.check_unseen(dirs, stop.clone());
            DeleteCheck::Checking(stop)
        });
        self.stop_delete_check();
        self.delete_check = check;
        self.delete_confirm = Some((paths, size, how, ntfs));
    }

    /// Confirmation of the deletion: what will happen, name and size. Enter confirms, Esc cancels;
    /// with a folder chosen Enter cancels too, Delete waits for the reading of the folders' first
    /// level, and a warning appears if they hold something the grid does not show.
    pub(crate) fn delete_window(&mut self, ctx: &egui::Context) {
        let Some((paths, size, how, ntfs)) = self.delete_confirm.clone() else { return };
        let folders = paths.iter().any(|p| p.ends_with('/')) && !source::is_trash(&paths[0]);
        let text = self.delete_text(&paths, size, how, ntfs);
        let reading = matches!(self.delete_check, Some(DeleteCheck::Checking(_)));
        if reading {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // The button says what happens: to the trash, or deleted for good.
        let yes = self.i18n.t(if how == source::Deletion::Permanent { "delete-yes" } else { "delete-move-to-trash" });
        let no = self.i18n.t("delete-no");
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, "delete").map(|enter| enter && !folders);
        // Cancel highlighted when the deletion cannot be undone, and for folders (#311).
        let safe = folders || how == source::Deletion::Permanent || source::is_trash(&paths[0]);
        let title = self.i18n.t(if source::is_trash(&paths[0]) && paths.len() == 1 { "delete-title-empty" } else if how == source::Deletion::Permanent { "delete-title-permanent" } else { "delete-title-trash" });
        match confirm::yes_no(ctx, "delete", 420.0, (title, text), yes, !reading, no, keys, safe) {
            Some(true) => {
                self.stop_delete_check();
                if let Some(loader) = &self.loader {
                    log!("delete confirmed: {} items", paths.len());
                    self.deleting += paths.len();
                    loader.delete(paths, how);
                }
                self.marked.clear();
                self.delete_confirm = None;
            }
            Some(false) => {
                self.stop_delete_check();
                self.delete_confirm = None;
            }
            None => {}
        }
    }

    /// The text of the deletion window: what happens, and the warning about the folders if needed.
    fn delete_text(&self, paths: &[String], size: Option<u64>, how: source::Deletion, ntfs: bool) -> String {
        let path = &paths[0];
        let dir = if path.ends_with('/') { "-dir" } else { "" };
        let id = match how {
            _ if paths.len() > 1 => format!("delete-confirm-many-{}", match how {
                source::Deletion::SystemTrash => "trash",
                source::Deletion::TrashFolder => "folder",
                source::Deletion::Permanent => "permanent",
            }),
            _ if source::is_trash(path) => "delete-confirm-empty-trash".to_string(),
            source::Deletion::SystemTrash => format!("delete-confirm{dir}-trash"),
            source::Deletion::TrashFolder => format!("delete-confirm{dir}-folder"),
            source::Deletion::Permanent => format!("delete-confirm{dir}-permanent"),
        };
        let size = size.map(|s| self.human_size(s)).unwrap_or_default();
        let n = paths.len().to_string();
        let many_folders = match paths.iter().any(|p| p.ends_with('/')) {
            true => format!(" {}", self.i18n.t("delete-many-folders")),
            false => String::new(),
        };
        let args = [("name", &crate::save_dialogs::md(source::shown(path))), ("size", &size), ("n", &n), ("folders", &many_folders)];
        let mut text = self.i18n.t_args(&id, &args.map(|(k, v)| (k, v.as_str())));
        if ntfs {
            text = format!("{text}\n\n{}", self.i18n.t("delete-ntfs"));
        }
        if let Some(warning) = self.unseen_text(paths.iter().filter(|p| p.ends_with('/')).count(), how) {
            text = format!("{text}\n\n{warning}");
        }
        text
    }

    /// The warning when the folders about to be deleted (`folders` of them) hold something the grid
    /// does not show, or could not be read; nothing while reading or when there is nothing.
    fn unseen_text(&self, folders: usize, how: source::Deletion) -> Option<String> {
        match &self.delete_check {
            Some(DeleteCheck::Done(Ok(Some((name, dir))))) => {
                let (name, dir, n) = (crate::save_dialogs::md(name), crate::save_dialogs::md(source::name(dir)), folders.to_string());
                let how = if how == source::Deletion::Permanent { "permanent" } else { "trash" };
                Some(self.i18n.t_args("delete-unseen-found", &[("name", &name), ("folder", &dir), ("n", &n), ("how", how)]))
            }
            Some(DeleteCheck::Done(Err(e))) => {
                log!("delete: first level not read: {e:?}");
                Some(self.i18n.t("delete-unseen-failed"))
            }
            _ => None,
        }
    }

    /// Stops a check of the folders still running, and forgets its answer.
    pub(crate) fn stop_delete_check(&mut self) {
        if let Some(DeleteCheck::Checking(stop)) = &self.delete_check {
            stop.store(true, Ordering::Relaxed);
        }
        self.delete_check = None;
    }

    /// The message at the end of a copy or a move into `to` (as the address bar shows it) that
    /// has something to say (`transfer_notice`), in place of the copy box; none when the program
    /// closes after the copy.
    pub(crate) fn tell_transfer(&mut self, p: &transfer::Progress, cut: bool, to: &str, r: &Result<(), source::Error>) {
        if self.quit_after_transfer {
            return;
        }
        self.notice = transfer_notice(&self.i18n, cut, to, r, p).or(self.notice.take());
    }

    pub(crate) fn ask_rename(&mut self, path: Option<String>) {
        // One at a time, also from F2: with several marked it renamed only the one under the cursor.
        if self.chosen(path.clone()).len() > 1 {
            return;
        }
        let path = path.or_else(|| self.target()).filter(|p| self.can_change(p) && !source::is_trash(p) && !source::in_trash(p));
        if let Some(path) = path {
            log!("rename? {path:?}");
            self.rename = Some((source::name(&path).to_string(), path, true));
        }
    }

    /// New name, in a text field with the old one. Enter confirms, Esc cancels. The name is
    /// checked here: not empty, no separators, not already in the folder.
    pub(crate) fn rename_window(&mut self, ctx: &egui::Context) {
        let Some((mut name, path, first)) = self.rename.clone() else { return };
        let old = source::name(&path).to_string();
        let trimmed = name.trim().to_string();
        let problem = self.name_problem(&source::parent(&path), &trimmed, Some(&old));
        // The name selected without the extension, as file managers do.
        let end = if path.ends_with('/') { old.chars().count() } else { old.rfind('.').map_or(old.chars().count(), |i| old[..i].chars().count()) };
        let texts = (self.i18n.t("rename-title"), self.i18n.t("rename-ok"));
        let answer = name_window(ctx, "rename", texts, &mut name, first.then_some(end), problem, &self.i18n);
        match answer {
            Some(true) => {
                if trimmed != old
                    && let Some(loader) = &self.loader
                {
                    loader.rename(path, trimmed);
                }
                self.rename = None;
            }
            Some(false) => self.rename = None,
            None => self.rename = Some((name, path, false)),
        }
    }

    /// What is wrong with `name` for an item of `dir` (`old`: its present name, when renaming).
    pub(crate) fn name_problem(&self, dir: &str, name: &str, old: Option<&str>) -> Option<String> {
        let taken = self.listings.get(dir).and_then(|l| l.as_ref().ok()).is_some_and(|l| {
            l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).any(|p| source::name(p) == name)
        });
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            Some(self.i18n.t("rename-bad"))
        } else if Some(name) != old && taken {
            Some(self.i18n.t("rename-taken"))
        } else {
            None
        }
    }

    pub(crate) fn ask_restore(&mut self, path: Option<String>) {
        let Some(path) = path.or_else(|| self.target()) else { return };
        log!("restore? {path:?}");
        if source::is_trash(&path) {
            if let Some(loader) = &self.loader {
                loader.trash_items(path);
            }
        } else if source::in_trash(&path) {
            self.restore_queue.push_back(path);
        }
    }

    /// Sends the next item of the queue, when the previous one is done and nothing is being asked.
    pub(crate) fn next_restore(&mut self) {
        if self.restore_busy || self.restore_conflict.is_some() {
            return;
        }
        if let (Some(path), Some(loader)) = (self.restore_queue.pop_front(), &self.loader) {
            self.restore_busy = true;
            loader.restore(path, source::Conflict::Ask);
        }
    }

    /// The name of an item being put back is taken: files can replace or be kept both, folders
    /// can merge or be kept both. Cancel also stops the rest of a Restore all.
    pub(crate) fn restore_window(&mut self, ctx: &egui::Context) {
        let Some(path) = self.restore_conflict.clone() else { return };
        let folder = path.ends_with('/');
        let (dest, own) = source::restore_place(&path);
        let place = source::name(&dest).to_string();
        let text = self.i18n.t_args("restore-taken", &[("name", &crate::save_dialogs::md(own)), ("folder", &crate::save_dialogs::md(&place))]);
        let t = |id| self.i18n.t(id);
        let first = if folder {
            (t("restore-merge"), source::Conflict::Merge)
        } else {
            (t("restore-replace"), source::Conflict::Replace)
        };
        let buttons = vec![(first.0, true, first.1), (t("restore-keep-both"), true, source::Conflict::KeepBoth)];
        let cancel = t("delete-no");
        let keys = crate::confirm::answer(ctx, &mut self.key_answer, "restore").and_then(|yes| (!yes).then_some(None));
        match confirm::choose(ctx, "restore", 420.0, (t("taken-title"), text), buttons, cancel, keys, 1) {
            Some(Some(conflict)) => {
                if let Some(loader) = &self.loader {
                    self.restore_busy = true;
                    loader.restore(path, conflict);
                }
                self.restore_conflict = None;
            }
            Some(None) => {
                self.restore_conflict = None;
                self.restore_queue.clear();
            }
            None => {}
        }
    }
}

/// A name in a text field, as Rename and New folder ask it: `select` = on the first frame, the
/// field takes the focus with the first characters selected. Enter confirms (when there is no
/// `problem`), Esc cancels. `Some(true)` OK, `Some(false)` cancelled.
pub(crate) fn name_window(ctx: &egui::Context, id: &str, (title, ok): (String, String), name: &mut String, select: Option<usize>, problem: Option<String>, i18n: &crate::i18n::I18n) -> Option<bool> {
    let mut answer = None;
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_width(420.0);
        crate::theme::title(ui, title);
        ui.add_space(4.0);
        let mut out = egui::TextEdit::singleline(name).desired_width(f32::INFINITY).show(ui);
        if let Some(end) = select {
            out.response.request_focus();
            let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(end));
            out.state.cursor.set_char_range(Some(range));
            out.state.store(ui.ctx(), out.response.id);
        }
        if let Some(p) = &problem {
            crate::error_bar::hint(ui, p.as_str());
        }
        ui.add_space(8.0);
        let enter = out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        ui.horizontal(|ui| {
            if ui.add_enabled(problem.is_none(), egui::Button::new(ok)).clicked() || (enter && problem.is_none()) {
                answer = Some(true);
            }
            if ui.button(i18n.t("delete-no")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                answer = Some(false);
            }
        });
    });
    answer
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
    let to = i18n.t_args("transfer-to", &[("dir", &crate::save_dialogs::md(&crate::address::without_password(to)))]);
    let told = told(i18n, i18n.t(title), &to, parts);
    Some(if links.is_empty() { told } else { with_guide(i18n, told, "help:copy-move") })
}

#[cfg(test)]
mod tests {
    use crate::save_dialogs::md;

    use crate::save_dialogs::plain;
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
        if own_folder("file_dialogs::tests::copy_box_goes_and_the_message_comes") {
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
}
