//! Rotate and crop: Apply, Save, Save as… and the questions before saving or closing.

use std::sync::Arc;

use eframe::egui;

use super::Window;
use crate::encode::Original;
use crate::{App, confirm, source};

/// Answers to "save the changes?" when closing.
#[derive(Clone, Copy)]
pub(super) enum Leave {
    Save,
    SaveAs,
    Discard,
}

impl App {
    /// The image changed in memory for Save: the confirmation opens.
    pub(crate) fn on_edited(&mut self, path: String, r: Result<(Arc<Vec<u8>>, bool), source::Error>) {
        self.saving.remove(&path);
        let Some(w) = self.rotate.as_mut().filter(|w| w.path == path) else { return };
        w.preparing = false;
        match r {
            Ok(ready) => w.ready = Some(ready),
            Err(e) => self.save_failed(&path, e),
        }
    }

    /// Why Save cannot write over the original, if it cannot.
    pub(super) fn why_not_save(&self, w: &Window) -> Option<String> {
        if self.loader.as_ref().is_some_and(|l| !l.can_rename()) {
            Some(self.i18n.t("save-no-rename"))
        } else if !crate::encode::saves_back(&w.path) || !self.can_change(&w.path) || self.saving.contains(&w.path) {
            Some(self.i18n.t_args("save-format", &[("name", &crate::save_dialogs::md(source::name(&w.path)))]))
        } else {
            None
        }
    }

    pub(super) fn rotate_save(&mut self, w: &mut Window) {
        let (Some(loader), Some(Ok((original, ..)))) = (&self.loader, &w.original) else { return };
        let pending = w.pending().then(|| w.edit());
        log!("prepare {:?} applied {} {pending:?}", w.path, w.applied);
        loader.prepare_edit(w.path.clone(), original.clone(), pending);
        self.saving.insert(w.path.clone());
        w.preparing = true;
    }

    /// "Save in another format" with the image rotated and cropped, made in the background
    /// (`Msg::Original`); `true` once opened.
    pub(super) fn rotate_save_as(&mut self, ctx: &egui::Context, w: &Window) -> bool {
        let Some(Ok((original, _, size))) = &w.original else { return false };
        let (original, edit, size, tx, ctx) = (original.clone(), w.edit(), *size, self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut o = Original::clone(&original);
            edit.apply(&mut o.image);
            let preview = crate::decode::preview(&o.image);
            if tx.send(crate::loader::Msg::Original(Ok((Arc::new(o), preview, size)))).is_ok() {
                ctx.request_repaint();
            }
        });
        self.convert = Some(crate::convert::Window::new(w.path.clone()));
        true
    }

    /// Apply: the edit done on the full image in the background; it comes back as `Msg::Original`
    /// and becomes the image of the page (`rotate_original`).
    pub(super) fn rotate_apply(&mut self, ctx: &egui::Context, w: &mut Window) {
        let Some(Ok((original, _, size))) = &w.original else { return };
        let (original, edit, size, tx, ctx) = (original.clone(), w.edit(), *size, self.tx.clone(), ctx.clone());
        log!("apply {edit:?}");
        w.applying = Some(edit);
        std::thread::spawn(move || {
            let mut o = Original::clone(&original);
            edit.apply(&mut o.image);
            let preview = crate::decode::preview(&o.image);
            if tx.send(crate::loader::Msg::Original(Ok((Arc::new(o), preview, size)))).is_ok() {
                ctx.request_repaint();
            }
        });
    }

    /// Closing with changes not saved: Save, Save as…, Don't save, or Cancel (stays open).
    pub(super) fn rotate_leave(&mut self, ctx: &egui::Context, w: &mut Window) -> Option<Leave> {
        let i18n = &self.i18n;
        let text = i18n.t_args("rc-leave", &[("name", &crate::save_dialogs::md(source::name(&w.path)))]);
        let choices = vec![
            (i18n.t("rc-save"), self.why_not_save(w).is_none(), Leave::Save),
            (i18n.t("menu-save-copy"), true, Leave::SaveAs),
            (i18n.t("rc-discard"), true, Leave::Discard),
        ];
        // Esc: back to the page; Enter does nothing here (no choice is safe to guess).
        let keys = self.delete_answer.take().filter(|yes| !yes).map(|_| None);
        let answer = confirm::choose(ctx, "rotate-leave", 440.0, text, choices, i18n.t("delete-no"), keys)?;
        w.leaving = false;
        answer
    }

    /// Overwrite? As Save in the viewer. `true` once written (the page closes).
    pub(super) fn rotate_confirm(&mut self, ctx: &egui::Context, w: &mut Window) -> bool {
        let (Some((data, lossy)), Some(loader)) = (w.ready.clone(), &self.loader) else { return false };
        let how = loader.deletion(&w.path, self.settings.local_permanent, self.settings.remote_delete);
        let text = self.save_confirm_text(&w.path, how, lossy);
        let (yes, no) = (self.i18n.t("save-yes"), self.i18n.t("delete-no"));
        match confirm::yes_no(ctx, "rotate-save", 420.0, text, yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                log!("save {:?} {how:?}", w.path);
                loader.replace_with(w.path.clone(), data, how);
                self.saving.insert(w.path.clone());
                w.ready = None;
                true
            }
            Some(false) => {
                w.ready = None;
                false
            }
            None => false,
        }
    }
}
