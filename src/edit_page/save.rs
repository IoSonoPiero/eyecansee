//! The editor: Save (a copy next to the original), Overwrite the original…, Save as… and the
//! questions before saving or closing. The file is made in the background (`edit::save`, from the
//! file read again for its format and metadata), shown in a confirmation with what it loses, then
//! sent through `kept`: on a phone or a server it is kept on the computer first, and stays there
//! with Try again, Save as… and Open the folder if sending fails. Moved from `rotate_page/save.rs`
//! on 2026-11-05.

use std::sync::Arc;

use eframe::egui;

use super::Window;
use crate::actions::Act;
use crate::edit::save::{Depth, Saved};
use crate::encode::Original;
use crate::kept::Send;
use crate::source::{self, Deletion};
use crate::{App, confirm};

/// Answers to "save the changes?" when closing.
#[derive(Clone, Copy)]
pub(super) enum Leave {
    Save,
    SaveAs,
    Discard,
}

/// A copy, or the image to write over the original (`over`), waiting for its confirmation.
pub struct Ready {
    saved: Arc<Saved>,
    over: bool,
}

impl App {
    /// The copy cannot go next to the original: a temporary copy Windows made of a phone's file
    /// (issue #47), which Windows can delete. Save opens Save as… instead.
    fn copy_refused(&self, w: &Window) -> bool {
        self.loader.as_ref().and_then(|l| l.no_overwrite(&w.path)) == Some("save-temp-copy")
    }

    /// Why Overwrite cannot write over the original, if it cannot: the places Save never writes
    /// over (gvfs, temporary copies, no renaming), a format EyeCanSee does not write back (the copy
    /// of a HEIC or a RAW is a JPEG or a PNG).
    pub(super) fn why_not_overwrite(&self, w: &Window) -> Option<String> {
        if let Some(id) = self.loader.as_ref().and_then(|l| l.no_overwrite(&w.path)) {
            Some(self.i18n.t(id))
        } else if crate::encode::Format::of_file(&w.path).is_none() || !self.can_change(&w.path) || self.saving.contains(&w.path) {
            Some(self.i18n.t_args("save-format", &[("name", &crate::save_dialogs::md(source::name(&w.path)))]))
        } else {
            None
        }
    }

    /// Save (`over` false) or Overwrite: the file made in the background (`Msg::EditPrepared`).
    pub(super) fn edit_prepare(&mut self, ctx: &egui::Context, w: &mut Window, over: bool) {
        if !over && self.copy_refused(w) {
            w.note = Some((self.i18n.t("edit-temp-copy"), ctx.input(|i| i.time) + 8.0));
            w.save_as = true;
            return;
        }
        let (Some(loader), Some(Ok((original, ..)))) = (&self.loader, &w.original) else { return };
        let (store, path, doc, original, word) = (loader.store(), w.path.clone(), w.doc.clone(), original.clone(), self.i18n.t("edit-copy-word"));
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        log!("edit: prepare {path:?}, over {over}");
        self.rt.spawn(async move {
            let made = async {
                let bytes = store.read(&path).await?;
                let taken: Vec<String> = store.names(&source::parent(&path)).await?.into_iter().collect();
                let name = source::name(&path).to_string();
                tokio::task::spawn_blocking(move || crate::edit::save::save(&doc, &original, &bytes, &name, &word, &taken, &crate::decode::usvg_options()).map_err(|e| ("error-detail", e)))
                    .await
                    .map_err(|e| ("error-detail", e.to_string()))?
            };
            let r = made.await.map(|s| (Arc::new(s), over));
            let _ = tx.send(crate::loader::Msg::EditPrepared(path, r));
            ctx.request_repaint();
        });
        w.preparing = true;
    }

    pub(crate) fn on_edit_prepared(&mut self, path: String, r: Result<(Arc<Saved>, bool), source::Error>) {
        let Some(w) = self.editor.as_mut().filter(|w| w.path == path) else { return };
        w.preparing = false;
        match r {
            Ok((saved, over)) => w.ready = Some(Ready { saved, over }),
            Err(e) => self.save_failed(e, Some(Act::RotateSaveAs)),
        }
    }

    /// What the copy (or the file written over the original, `over`) loses, a line each: encoded
    /// again, fewer bits, highlights cut.
    fn losses(&self, s: &Saved, over: bool) -> String {
        let mut ids = Vec::new();
        if s.lossy {
            ids.push(if over { "edit-over-loss-lossy" } else { "edit-loss-lossy" });
        }
        match s.depth {
            Depth::To8Bits => ids.push(if over { "edit-over-loss-8bit" } else { "edit-loss-8bit" }),
            Depth::HdrCut => ids.push(if over { "edit-over-loss-hdr" } else { "edit-loss-hdr" }),
            Depth::Kept => {}
        }
        ids.iter().map(|id| format!("\n\n{}", self.i18n.t(id))).collect()
    }

    /// The confirmation: the copy's name, or over the original with where the original goes
    /// (always a trash, also with deleting for good chosen in the Options). `true` once sent: the
    /// page closes.
    pub(super) fn edit_confirm(&mut self, ctx: &egui::Context, w: &mut Window) -> bool {
        let (Some(ready), Some(loader)) = (&w.ready, &self.loader) else { return false };
        let (saved, over) = (ready.saved.clone(), ready.over);
        let how = loader.deletion(&w.path, false, crate::settings::RemoteDelete::TrashFolder);
        let md = crate::save_dialogs::md;
        let (text, yes) = if over {
            (self.i18n.t_args(trash_id(how), &[("name", &md(source::name(&w.path)))]), self.i18n.t("edit-overwrite-yes"))
        } else {
            (self.i18n.t_args("edit-save-confirm", &[("name", &md(source::name(&w.path))), ("copy", &md(&saved.name))]), self.i18n.t("save-yes"))
        };
        let text = format!("{text}{}", self.losses(&saved, over));
        let no = self.i18n.t("delete-no");
        match confirm::yes_no(ctx, "edit-save", 460.0, text, yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                let (target, send) = if over { (w.path.clone(), Send::Keeping) } else { (format!("{}{}", source::parent(&w.path), saved.name), Send::Create) };
                log!("edit: {send:?} {target:?} ({} bytes)", saved.data.len());
                let (store, base, tx, ctx) = (loader.store(), self.base.clone(), self.tx.clone(), ctx.clone());
                self.saving.insert(target.clone());
                self.rt.spawn(async move {
                    let r = crate::kept::send(&store, &base, &target, saved.data.clone(), how, send).await;
                    let _ = tx.send(crate::loader::replaced(target, r));
                    ctx.request_repaint();
                });
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

    /// "Save in another format" with the result (objects too), made in the background
    /// (`Msg::Original`); `true` once opened. Without the gain map, so without its description.
    pub(super) fn rotate_save_as(&mut self, ctx: &egui::Context, w: &Window) -> bool {
        let Some(Ok((original, _, size))) = &w.original else { return false };
        let (original, doc, size, tx, ctx) = (original.clone(), w.doc.clone(), *size, self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let image = crate::edit::save::result(&doc, &original, &crate::decode::usvg_options());
            let mut o = Original::clone(&original);
            o.image = image;
            let o = o.alone();
            let preview = crate::decode::preview(&o.image);
            if tx.send(crate::loader::Msg::Original(Ok((Arc::new(o), preview, size)))).is_ok() {
                ctx.request_repaint();
            }
        });
        self.convert = Some(crate::convert::Window::new(w.path.clone()));
        true
    }

    /// Closing with changes not saved: Save (a copy), Save as…, Don't save, or Cancel (stays open).
    pub(super) fn rotate_leave(&mut self, ctx: &egui::Context, w: &mut Window) -> Option<Leave> {
        let i18n = &self.i18n;
        let text = i18n.t_args("edit-leave", &[("name", &crate::save_dialogs::md(source::name(&w.path)))]);
        let choices = vec![(i18n.t("rc-save"), !w.preparing, Leave::Save), (i18n.t("menu-save-copy"), true, Leave::SaveAs), (i18n.t("rc-discard"), true, Leave::Discard)];
        // Esc: back to the page; Enter does nothing here (no choice is safe to guess).
        let keys = self.delete_answer.take().filter(|yes| !yes).map(|_| None);
        let answer = confirm::choose(ctx, "rotate-leave", 440.0, text, choices, i18n.t("delete-no"), keys)?;
        w.leaving = false;
        answer
    }
}

/// The question of an overwrite by where the original goes: always a trash (`edit_confirm` asks
/// for one), never deleted for good.
fn trash_id(how: Deletion) -> &'static str {
    match how {
        Deletion::SystemTrash => "edit-overwrite-trash",
        Deletion::TrashFolder | Deletion::Permanent => "edit-overwrite-folder",
    }
}
