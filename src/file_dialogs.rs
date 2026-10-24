//! File windows: paste and its conflicts, transfer progress, delete, rename, restore from the trash.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use eframe::egui::{self, Key};

use crate::{App, PasteAsk, browser, confirm, devices, human_time, source, theme, transfer};
use crate::navigation::shown;

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
        ctx.copy_text(paths.iter().map(|p| self.display(p)).collect::<Vec<_>>().join("\n"));
        self.clip = Some(transfer::Clip { op: loader.store(), base: self.base.clone(), paths, cut });
    }

    /// Where a paste can go: a folder of the open source, not its root on a phone, not a trash.
    pub(crate) fn can_paste(&self, dir: &str) -> bool {
        self.clip.is_some()
            && self.transfer.is_none()
            && self.paste_ask.is_none()
            && !(self.base.starts_with("mtp://") && dir.is_empty())
            && !source::is_trash(dir)
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
        let Some(ask) = &mut self.paste_ask else { return };
        let Some(conflicts) = &ask.conflicts else { return };
        let Some(path) = conflicts.first().cloned() else {
            let ask = self.paste_ask.take().unwrap();
            self.start_paste(ask.dir, ask.choices);
            return;
        };
        let many = conflicts.len() > 1;
        let t = |id| self.i18n.t(id);
        let text = self.i18n.t_args("paste-taken", &[("name", source::name(&path))]);
        let first = if path.ends_with('/') { t("restore-merge") } else { t("restore-replace") };
        let buttons = [
            (first, transfer::Choice::Replace),
            (t("paste-skip"), transfer::Choice::Skip),
            (t("restore-keep-both"), transfer::Choice::KeepBoth),
        ];
        let (cancel, all_label) = (t("delete-no"), t("paste-all"));
        let mut answer = self.delete_answer.take().and_then(|yes| (!yes).then_some(None));
        egui::Modal::new(egui::Id::unique("paste")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.label(text);
            if many {
                ui.checkbox(&mut ask.all, all_label);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (label, choice) in buttons {
                    if ui.button(label).clicked() {
                        answer = Some(Some(choice));
                    }
                }
                if ui.button(cancel).clicked() {
                    answer = Some(None);
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
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
        let (files, files_total) = (p.files.load(Ordering::Relaxed), p.files_total.load(Ordering::Relaxed));
        let found = p.found.load(Ordering::Relaxed);
        // Until the copy starts it is still looking inside the folders, even with nothing found yet.
        let counts = if p.started.get().is_none() {
            self.i18n.t_args("transfer-looking", &[("n", &found.to_string())])
        } else {
            self.i18n.t_args("transfer-counts", &[
            ("n", &files.to_string()),
            ("total", &files_total.to_string()),
            ("done", &self.human_size(done)),
            ("size", &self.human_size(total)),
        ])
        };
        // Average speed since the copy started; the first second gives no meaningful estimate.
        let left = p.started.get().map(|t| t.elapsed().as_secs_f64()).filter(|&secs| secs >= 1.0 && done > 0).map(|secs| {
            let speed = done as f64 / secs;
            self.i18n.t_args("transfer-left", &[
                ("percent", &(done * 100 / total.max(1)).to_string()),
                ("speed", &self.human_size(speed as u64)),
                ("time", &human_time((total.saturating_sub(done) as f64 / speed) as u64)),
            ])
        });
        let to = self.i18n.t_args("transfer-to", &[("dir", &shown(base, dest))]);
        let current = p.current.lock().unwrap().clone();
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
                ui.label(left.unwrap_or_default());
                // Accent fill on a grey track: the default (selection blue on white) hid the empty part.
                ui.scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = ui.visuals().widgets.inactive.bg_fill;
                    let fill = theme::accent(ui.visuals());
                    let fraction = if total > 0 { done as f32 / total as f32 } else { 0.0 };
                    ui.add(egui::ProgressBar::new(fraction).fill(fill).desired_height(16.0));
                });
                if ui.add_enabled(!cancelling, egui::Button::new(self.i18n.t("delete-no"))).clicked() {
                    p.cancel.store(true, Ordering::Relaxed);
                }
            });
    }

    pub(crate) fn ask_delete(&mut self, path: Option<String>, permanent: bool) {
        // Not while a deletion runs: a click repeated while waiting landed on the next item
        // (Download/Pipe, 2026-10-22).
        if self.deleting > 0 {
            return;
        }
        let paths = self.chosen(path);
        let Some(path) = paths.first().cloned() else { return };
        let Some(loader) = &self.loader else { return };
        // In the trash everything goes for good: there is no trash of the trash.
        let how = match permanent || source::is_trash(&path) || source::in_trash(&path) {
            true => source::Deletion::Permanent,
            false => loader.deletion(&path, self.settings.local_permanent, self.settings.remote_delete),
        };
        log!("delete? {paths:?} {how:?}");
        let size = match paths.len() {
            1 => self.listings.get(&source::parent(&path)).and_then(|l| l.as_ref().ok()).and_then(|l| {
                l.files.iter().find(|f| f.path == path).map(|f| f.size)
            }),
            _ => Some(self.images_size(&paths)),
        };
        let ntfs = how == source::Deletion::SystemTrash && !self.base.contains("://") && devices::on_ntfs(&format!("/{}", path.trim_end_matches('/')));
        self.delete_confirm = Some((paths, size, how, ntfs));
    }

    /// Confirmation of the deletion: what will happen, name and size. Enter confirms, Esc cancels.
    pub(crate) fn delete_window(&mut self, ctx: &egui::Context) {
        let Some((paths, size, how, ntfs)) = self.delete_confirm.clone() else { return };
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
        let mut text = self.i18n.t_args(&id, &[("name", source::name(path)), ("size", &size), ("n", &n)]);
        if ntfs {
            text = format!("{text}\n\n{}", self.i18n.t("delete-ntfs"));
        }
        let (yes, no) = (self.i18n.t("delete-yes"), self.i18n.t("delete-no"));
        match confirm::yes_no(ctx, "delete", 420.0, text, yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                if let Some(loader) = &self.loader {
                    log!("delete confirmed: {} items", paths.len());
                    self.deleting += paths.len();
                    loader.delete(paths, how);
                }
                self.marked.clear();
                self.delete_confirm = None;
            }
            Some(false) => self.delete_confirm = None,
            None => {}
        }
    }

    pub(crate) fn ask_rename(&mut self, path: Option<String>) {
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
        let parent = source::parent(&path);
        let trimmed = name.trim().to_string();
        let taken = self.listings.get(&parent).and_then(|l| l.as_ref().ok()).is_some_and(|l| {
            l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).any(|p| source::name(p) == trimmed)
        });
        let problem = if trimmed.is_empty() || trimmed == "." || trimmed == ".." || trimmed.contains(['/', '\\']) {
            Some(self.i18n.t("rename-bad"))
        } else if trimmed != old && taken {
            Some(self.i18n.t("rename-taken"))
        } else {
            None
        };
        let (title, ok, cancel) = (self.i18n.t("rename-title"), self.i18n.t("rename-ok"), self.i18n.t("delete-no"));
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("rename")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.strong(title);
            ui.add_space(4.0);
            let edit = egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY);
            let mut out = edit.show(ui);
            if first {
                // Focus, and the name selected without the extension, as file managers do.
                out.response.request_focus();
                let end = if path.ends_with('/') { old.chars().count() } else { old.rfind('.').map_or(old.chars().count(), |i| old[..i].chars().count()) };
                let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(end));
                out.state.cursor.set_char_range(Some(range));
                out.state.store(ui.ctx(), out.response.id);
            }
            if let Some(p) = &problem {
                ui.label(browser::error_text(ui, p.as_str()));
            }
            ui.add_space(8.0);
            let enter = out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                if ui.add_enabled(problem.is_none(), egui::Button::new(ok)).clicked() || (enter && problem.is_none()) {
                    answer = Some(true);
                }
                if ui.button(cancel).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
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
        let place = source::name(&source::parent(&source::parent(&path))).to_string();
        let text = self.i18n.t_args("restore-taken", &[("name", source::name(&path)), ("folder", &place)]);
        let t = |id| self.i18n.t(id);
        let first = if folder {
            (t("restore-merge"), source::Conflict::Merge)
        } else {
            (t("restore-replace"), source::Conflict::Replace)
        };
        let buttons = vec![(first.0, true, first.1), (t("restore-keep-both"), true, source::Conflict::KeepBoth)];
        let cancel = t("delete-no");
        let keys = self.delete_answer.take().and_then(|yes| (!yes).then_some(None));
        match confirm::choose(ctx, "restore", 420.0, text, buttons, cancel, keys) {
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
