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
use crate::edit::save::{Depth, Plan};
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

/// A file the editor wrote (the copy, or the original overwritten) and the SHA-256 of what it
/// wrote: the next save writes over it only while it is still that file.
#[derive(Clone, Debug)]
pub(crate) struct Written {
    pub path: String,
    pub sha: [u8; 32],
}

impl Written {
    pub fn of(path: String, data: &[u8]) -> Written {
        Written { path, sha: sha256(data) }
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    ring::digest::digest(&ring::digest::SHA256, data).as_ref().try_into().expect("32 bytes")
}

/// A save planned in the background: what it writes and loses (`Plan`), where it goes and how,
/// the original's bytes (kept for the next saves: they start from it, never from what was
/// written since), and what makes the file once confirmed (`make`).
pub struct Prepared {
    plan: Plan,
    doc: crate::edit::doc::Doc,
    original: Arc<Original>,
    word: String,
    taken: Vec<String>,
    target: String,
    send: Send,
    over: bool,
    /// The copy saved before is gone or was changed by another program: a new one is made.
    gone: bool,
    bytes: Arc<Vec<u8>>,
}

/// The save of `doc` on `original` (its file `path`, its bytes if already read): a copy (`over`
/// false) or over the original. A copy goes over the one written before (`written`) while it is
/// still that file, otherwise is a new one with the first free name; over the original, the first
/// time the original goes to a trash, the next times what was written before is replaced (while
/// it is still that), only the true original in the trash (user, 2026-11-06).
pub(crate) async fn prepare(store: &crate::source::Store, path: &str, (doc, original): (&crate::edit::doc::Doc, Arc<Original>), bytes: Option<Arc<Vec<u8>>>, (word, over, written): (String, bool, Option<Written>)) -> Result<Prepared, source::Error> {
    let bytes = match bytes {
        Some(b) => b,
        None => Arc::new(store.read(path).await?),
    };
    let still = match &written {
        Some(w) => store.read(&w.path).await.is_ok_and(|b| sha256(&b) == w.sha),
        None => false,
    };
    let taken: Vec<String> = store.names(&source::parent(path)).await?.into_iter().collect();
    let (d, o, b, name, (w2, t2)) = (doc.clone(), original.clone(), bytes.clone(), source::name(path).to_string(), (word.clone(), taken.clone()));
    let mut plan = tokio::task::spawn_blocking(move || crate::edit::save::plan(&d, &o, &b, &name, &w2, &t2).map_err(|e| ("error-detail", e)))
        .await
        .map_err(|e| ("error-detail", e.to_string()))??;
    let (target, send, gone) = match (over, written) {
        (true, _) => (path.to_string(), if still { Send::Again } else { Send::Keeping }, false),
        (false, Some(w)) if still => (w.path, Send::Again, false),
        (false, w) => (format!("{}{}", source::parent(path), plan.name), Send::Create, w.is_some()),
    };
    if !over {
        plan.name = source::name(&target).to_string();
    }
    Ok(Prepared { plan, doc: doc.clone(), original, word, taken, target, send, over, gone, bytes })
}

/// The file of a confirmed save: made now (the slow part), or already made by `plan`.
pub(crate) fn make(p: &Prepared, name: &str) -> Result<Vec<u8>, String> {
    if let Some(data) = &p.plan.data {
        return Ok(data.clone());
    }
    crate::edit::save::save(&p.doc, &p.original, &p.bytes, name, &p.word, &p.taken, &crate::decode::usvg_options()).map(|s| s.data)
}

/// A save made, waiting for its confirmation.
pub struct Ready(Arc<Prepared>);

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
        if w.new {
            return Some(self.i18n.t("edit-new-off"));
        }
        if let Some(id) = self.loader.as_ref().and_then(|l| l.no_overwrite(&w.path)) {
            Some(self.i18n.t(id))
        } else if crate::encode::Format::of_file(&w.path).is_none() || !self.can_change(&w.path) || self.saving.contains(&w.path) {
            Some(self.i18n.t_args("save-format", &[("name", &crate::save_dialogs::md(source::name(&w.path)))]))
        } else {
            None
        }
    }

    /// The last save is still being made or sent: the next one waits for it (the button is off;
    /// Ctrl+S too, or it would find the copy not yet written and make another).
    pub(super) fn edit_sending(&self, w: &Window) -> bool {
        [&w.copy, &w.overwritten].into_iter().flatten().any(|m| self.saving.contains(&m.path))
    }

    /// Save (`over` false) or Overwrite: the file made in the background (`Msg::EditPrepared`),
    /// always from the original as read at the start, with the whole document.
    pub(super) fn edit_prepare(&mut self, ctx: &egui::Context, w: &mut Window, over: bool) {
        if self.edit_sending(w) {
            return;
        }
        if !over && self.copy_refused(w) {
            w.note = Some((self.i18n.t("edit-temp-copy"), ctx.input(|i| i.time) + 8.0));
            w.save_as = true;
            return;
        }
        let (Some(loader), Some(Ok((original, ..)))) = (&self.loader, &w.original) else { return };
        let (store, path, doc, original, word) = (loader.store(), w.path.clone(), w.doc.clone(), original.clone(), self.i18n.t("edit-copy-word"));
        let (bytes, written) = (w.bytes.clone(), if over { w.overwritten.clone() } else { w.copy.clone() });
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        log!("edit: prepare {path:?}, over {over}, after {written:?}");
        self.rt.spawn(async move {
            let r = prepare(&store, &path, (&doc, original), bytes, (word, over, written)).await.map(Arc::new);
            let _ = tx.send(crate::loader::Msg::EditPrepared(path, r));
            ctx.request_repaint();
        });
        w.preparing = true;
    }

    /// What a confirmed save wrote: the next one goes over it while it is still that file.
    pub(crate) fn on_edit_written(&mut self, path: String, mark: Written, over: bool) {
        let Some(w) = self.editor.as_mut().filter(|w| w.path == path) else { return };
        if over { w.overwritten = Some(mark) } else { w.copy = Some(mark) }
    }

    pub(crate) fn on_edit_prepared(&mut self, path: String, r: Result<Arc<Prepared>, source::Error>) {
        let Some(w) = self.editor.as_mut().filter(|w| w.path == path) else { return };
        w.preparing = false;
        match r {
            Ok(p) => {
                w.bytes.get_or_insert_with(|| p.bytes.clone());
                w.ready = Some(Ready(p));
            }
            Err(e) => self.save_failed(e, Some(Act::RotateSaveAs)),
        }
    }

    /// What the copy (or the file written over the original, `over`) loses, a line each: encoded
    /// again, fewer bits, highlights cut.
    fn losses(&self, s: &Plan, over: bool) -> String {
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
    /// (always a trash, also with deleting for good chosen in the Options). `true` once an
    /// overwrite is sent: the page closes. After a copy it stays open, the document saved (user,
    /// 2026-11-05: Save copy must not close the image).
    pub(super) fn edit_confirm(&mut self, ctx: &egui::Context, w: &mut Window) -> bool {
        let (Some(Ready(p)), Some(loader)) = (&w.ready, &self.loader) else { return false };
        let p = p.clone();
        let how = loader.deletion(&w.path, false, crate::settings::RemoteDelete::TrashFolder);
        let md = crate::save_dialogs::md;
        let (text, yes) = match (p.over, p.send) {
            (true, Send::Again) => (self.i18n.t_args("edit-overwrite-again", &[("name", &md(source::name(&w.path)))]), self.i18n.t("edit-overwrite-yes")),
            (true, _) => (self.i18n.t_args(trash_id(how), &[("name", &md(source::name(&w.path)))]), self.i18n.t("edit-overwrite-yes")),
            (false, Send::Again) => (self.i18n.t_args("edit-update-confirm", &[("copy", &md(&p.plan.name))]), self.i18n.t("save-yes")),
            (false, _) => (self.i18n.t_args("edit-save-confirm", &[("name", &md(source::name(&w.path))), ("copy", &md(&p.plan.name))]), self.i18n.t("save-yes")),
        };
        let text = format!("{text}{}", self.losses(&p.plan, p.over));
        let no = self.i18n.t("delete-no");
        match confirm::yes_no(ctx, "edit-save", 460.0, text, yes, true, no, self.delete_answer.take()) {
            Some(true) => {
                let (target, send, over, path) = (p.target.clone(), p.send, p.over, w.path.clone());
                log!("edit: {send:?} {target:?}");
                let (store, base, tx, ctx) = (loader.store(), self.base.clone(), self.tx.clone(), ctx.clone());
                self.saving.insert(target.clone());
                // The copy is what the grid selects and the viewer shows once it is there.
                if !p.over {
                    self.select_after = Some(target.clone());
                }
                if p.gone {
                    w.note = Some((self.i18n.t("edit-copy-gone"), ctx.input(|i| i.time) + 8.0));
                }
                // Where it goes, at once (the button says Update copy, and waits while it is
                // sent); what it wrote once the file is made (`Msg::EditWritten`).
                let mark = Written { path: target.clone(), sha: [0; 32] };
                if p.over { w.overwritten = Some(mark) } else { w.copy = Some(mark) }
                let name = source::name(&w.path).to_string();
                self.rt.spawn(async move {
                    let p2 = p.clone();
                    let made = tokio::task::spawn_blocking(move || make(&p2, &name)).await.unwrap_or_else(|e| Err(e.to_string()));
                    let r = match made {
                        Ok(data) => {
                            log!("edit: made {target:?} ({} bytes)", data.len());
                            let _ = tx.send(crate::loader::Msg::EditWritten(path, Written::of(target.clone(), &data), over));
                            crate::kept::send(&store, &base, &target, data, how, send).await
                        }
                        Err(e) => Err((("error-detail", e), None)),
                    };
                    let _ = tx.send(crate::loader::replaced(target, r));
                    ctx.request_repaint();
                });
                w.ready = None;
                // Saved: closing asks nothing until something changes again (the page stays
                // open after an overwrite too, user 2026-11-06).
                if let Some(h) = &mut w.history {
                    h.push(w.doc.clone());
                    h.saved();
                }
                false
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::doc::{Area, Doc, Obj, Pt, Shape, Style};
    use crate::source::Store;

    fn cover(x: f32) -> Obj {
        Obj { shape: Shape::Cover(Area { centre: Pt::new(x, 0.0), w: 8.0, h: 8.0, rot: 0.0 }), style: Style::default() }
    }

    /// A folder `p` with `a.jpg` (quality 80) in a temporary place, as a store.
    fn place(name: &str) -> (std::path::PathBuf, Store, Vec<u8>) {
        let dir = std::env::temp_dir().join(format!("eyecansee-edit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("p")).unwrap();
        let bytes = crate::encode::jpeg_quality::tests::jpeg(80, crate::encode::Chroma::Quarter);
        std::fs::write(dir.join("p/a.jpg"), &bytes).unwrap();
        let store = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        (dir, store, bytes)
    }

    /// One save as the page does it: prepared, sent, and the mark of what was written.
    fn save(rt: &tokio::runtime::Runtime, store: &Store, doc: &Doc, over: bool, bytes: Option<Arc<Vec<u8>>>, written: Option<Written>) -> (Arc<Prepared>, Written) {
        let original = Arc::new(Original::with_meta_of(crate::decode::decode_full(&std::fs::read(store.local_file("p/a.jpg").unwrap()).unwrap(), "a.jpg").unwrap(), &[], "a.jpg"));
        let original = bytes.as_ref().map_or(original, |b| Arc::new(Original::with_meta_of(crate::decode::decode_full(b, "a.jpg").unwrap(), b, "a.jpg")));
        let p = Arc::new(rt.block_on(prepare(store, "p/a.jpg", (doc, original), bytes, ("edited".into(), over, written))).unwrap());
        let data = make(&p, "a.jpg").unwrap();
        rt.block_on(crate::kept::send(store, "file:///x/", &p.target, data.clone(), Deletion::TrashFolder, p.send)).unwrap_or_else(|e| panic!("{:?}", e.0));
        let mark = Written::of(p.target.clone(), &data);
        (p, mark)
    }

    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    /// Two Save copy give one copy, updated, made from the original with all the objects; a copy
    /// deleted between them gives a new one with the first free name, said to be gone.
    #[test]
    fn copy_updated() {
        let (dir, store, original) = place("copy");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut doc = Doc::new(64, 48, None);
        doc.objects.push(cover(-10.0));
        let (first, mark) = save(&rt, &store, &doc, false, None, None);
        assert_eq!((first.send, first.target.as_str()), (Send::Create, "p/a (edited).jpg"));
        doc.objects.push(cover(10.0));
        let (second, mark) = save(&rt, &store, &doc, false, Some(first.bytes.clone()), Some(mark));
        assert_eq!((second.send, second.target.as_str(), second.gone), (Send::Again, "p/a (edited).jpg", false));
        assert_eq!(names(&dir.join("p")), ["a (edited).jpg", "a.jpg"]);
        // The copy is the original with both boxes, as one save from it makes it.
        let direct = crate::edit::save::save(&doc, &Original::with_meta_of(crate::decode::decode_full(&original, "a.jpg").unwrap(), &original, "a.jpg"), &original, "a.jpg", "edited", &[], &crate::decode::usvg_options()).unwrap();
        assert_eq!(std::fs::read(dir.join("p/a (edited).jpg")).unwrap(), direct.data);
        std::fs::remove_file(dir.join("p/a (edited).jpg")).unwrap();
        let (third, _) = save(&rt, &store, &doc, false, Some(second.bytes.clone()), Some(mark));
        assert_eq!((third.send, third.target.as_str(), third.gone), (Send::Create, "p/a (edited).jpg", true));
        // Changed by another program: a new copy beside it, the changed one left alone.
        std::fs::write(dir.join("p/a (edited).jpg"), b"changed").unwrap();
        let (fourth, _) = save(&rt, &store, &doc, false, Some(third.bytes.clone()), Some(Written::of("p/a (edited).jpg".into(), &make(&third, "a.jpg").unwrap())));
        assert_eq!((fourth.target.as_str(), fourth.gone), ("p/a (edited 2).jpg", true));
        assert_eq!(std::fs::read(dir.join("p/a (edited).jpg")).unwrap(), b"changed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Two Overwrite in a row on a JPEG: the file is the original with all the changes,
    /// compressed once from it (as one save from the original makes it, at its quality 80), and
    /// in the trash only the original, once.
    #[test]
    fn overwritten_twice() {
        let (dir, store, original) = place("over");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut doc = Doc::new(64, 48, None);
        doc.objects.push(cover(-10.0));
        let (first, mark) = save(&rt, &store, &doc, true, None, None);
        assert_eq!(first.send, Send::Keeping);
        doc.objects.push(cover(10.0));
        let (second, _) = save(&rt, &store, &doc, true, Some(first.bytes.clone()), Some(mark));
        assert_eq!(second.send, Send::Again);
        let direct = crate::edit::save::save(&doc, &Original::with_meta_of(crate::decode::decode_full(&original, "a.jpg").unwrap(), &original, "a.jpg"), &original, "a.jpg", "edited", &[], &crate::decode::usvg_options()).unwrap();
        let now = std::fs::read(dir.join("p/a.jpg")).unwrap();
        assert_eq!(now, direct.data);
        assert_eq!(crate::encode::jpeg_quality::of(&now).map(|q| q.0), Some(80));
        let trash = dir.join("p").join(crate::source::TRASH_DIR);
        let trashed: Vec<Vec<u8>> = walk(&trash).iter().filter(|p| p.extension().is_some_and(|e| e == "jpg")).map(|p| std::fs::read(p).unwrap()).collect();
        assert_eq!(trashed, [original]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir).map(|r| r.flat_map(|e| { let p = e.unwrap().path(); if p.is_dir() { walk(&p) } else { vec![p] } }).collect()).unwrap_or_default()
    }
}
