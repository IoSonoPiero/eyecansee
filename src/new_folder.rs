//! New folder: a name asked as Rename asks it, the folder made in the folder open (or in the
//! folder right-clicked), then selected.

use eframe::egui;

use crate::{App, source};

impl App {
    /// Can a folder be made in the folder open? Not in a trash, not among the storages of a phone.
    pub(crate) fn can_make_folder(&self) -> bool {
        self.viewing.is_none() && self.can_make_folder_in(&self.dir)
    }

    pub(crate) fn can_make_folder_in(&self, dir: &str) -> bool {
        self.loader.is_some() && !source::is_trash(dir) && !source::in_trash(dir) && !(self.base.starts_with("mtp://") && dir.is_empty())
    }

    /// In `dir` (a folder right-clicked), or in the folder open.
    pub(crate) fn ask_new_folder(&mut self, dir: Option<String>) {
        let dir = dir.unwrap_or_else(|| self.dir.clone());
        if !(self.can_make_folder_in(&dir) && (dir != self.dir || self.can_make_folder())) || self.new_folder.is_some() {
            return;
        }
        // A folder not opened yet: its names arrive while the name is asked, and the window
        // checks them as they come.
        self.request_listing(&dir);
        let taken = self.listings.get(&dir).and_then(|l| l.as_ref().ok()).map(|l| l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).map(|p| source::name(p).to_string()).collect()).unwrap_or_default();
        let name = source::free_name(&taken, &self.i18n.t("new-folder-name"));
        log!("new folder? in {dir:?}");
        self.new_folder = Some((name, dir, true));
    }

    pub(crate) fn new_folder_window(&mut self, ctx: &egui::Context) {
        let Some((mut name, dir, first)) = self.new_folder.clone() else { return };
        let trimmed = name.trim().to_string();
        let problem = self.name_problem(&dir, &trimmed, None);
        let texts = (self.i18n.t("new-folder-title"), self.i18n.t("new-folder-ok"));
        let select = first.then(|| name.chars().count());
        match crate::file_dialogs::name_window(ctx, "new-folder", texts, &mut name, select, problem, &self.i18n) {
            Some(true) => {
                if let Some(loader) = &self.loader {
                    loader.make_folder(format!("{dir}{trimmed}/"));
                }
                self.new_folder = None;
            }
            Some(false) => self.new_folder = None,
            None => self.new_folder = Some((name, dir, false)),
        }
    }

    /// Made: the folder is read again and the new one selected; or the error.
    pub(crate) fn on_made_folder(&mut self, path: String, r: Result<(), source::Error>) {
        match r {
            Ok(()) => {
                log!("folder made {path:?}");
                self.select_after = Some(path.clone());
                self.relist(source::parent(&path));
            }
            Err((id, detail)) => {
                log!("new folder {path:?}: ERROR {id} {detail}");
                self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
            }
        }
    }
}
