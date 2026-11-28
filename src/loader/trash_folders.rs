//! The loader's work on trash folders: putting items back, listing a trash, dropping one left
//! empty.

use std::sync::mpsc::Sender;

use super::{Loader, Msg, at};
use crate::source::{self, Store};

impl Loader {
    pub fn restore(&self, path: String, conflict: source::Conflict) {
        let (op, base, tx, ctx) = (self.op.clone(), self.base.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            restore_item(&op, &base, path, conflict, &tx).await;
            ctx.request_repaint();
        });
    }

    pub fn trash_items(&self, trash: String) {
        let (op, base, tx, ctx) = (self.op.clone(), self.base.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let m = match op.children(&trash).await {
                Ok(paths) => Msg::TrashItems(paths),
                Err(e) => Msg::Restored(trash, Err(e)),
            };
            let _ = tx.send(at(&base, m));
            ctx.request_repaint();
        });
    }
}

/// An item of the trash put back (`Loader::restore`); a trash left empty goes.
async fn restore_item(op: &Store, base: &str, path: String, conflict: source::Conflict, tx: &Sender<Msg>) {
    let r = crate::sidecar_follow::restore(op, &path, conflict).await;
    let ok = r.is_ok();
    let trash = source::trash_of(&path);
    let _ = tx.send(at(base, Msg::Restored(path, r)));
    if ok {
        drop_empty_trash(op, base, trash, tx).await;
    }
}

/// A trash folder left empty goes, so its strip does not say there is something to restore.
/// A trash folder left with nothing in it (Android's trashed files of its folder count) goes.
pub(super) async fn drop_empty_trash(op: &Store, base: &str, trash: String, tx: &Sender<Msg>) {
    if op.names(&trash).await.is_ok_and(|n| n.is_empty()) && android_trashed_files(op, &trash).await.is_empty() {
        let how = source::Deletion::Permanent;
        let r = op.delete(&trash, how).await;
        let _ = tx.send(at(base, Msg::Deleted(trash, how, r)));
    }
}

/// Whether the trash folder exists, and Android's trashed files of its folder.
pub(super) async fn trash_contents(op: &Store, trash: &str) -> (bool, Vec<String>) {
    (op.is_dir(trash).await, android_trashed_files(op, trash).await)
}

async fn android_trashed_files(op: &Store, trash: &str) -> Vec<String> {
    op.list(&source::parent(trash)).await.map(|l| l.trashed.into_iter().map(|f| f.path).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::TRASH_DIR;

    /// Emptying and dropping a trash with Android's trashed files in the folder: the folder may not
    /// exist, and it stays while such files are there.
    #[test]
    fn android_trash_folder() {
        let dir = std::env::temp_dir().join(format!("eyecansee-android-trash-{}", std::process::id()));
        let p = dir.join("p");
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(".trashed-1-a.jpg"), b"x").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let trash = format!("p/{TRASH_DIR}/");
        assert_eq!(rt.block_on(trash_contents(&op, &trash)), (false, vec!["p/.trashed-1-a.jpg".to_string()]));
        std::fs::create_dir_all(p.join(TRASH_DIR)).unwrap();
        assert!(rt.block_on(trash_contents(&op, &trash)).0);
        let (tx, rx) = std::sync::mpsc::channel();
        rt.block_on(drop_empty_trash(&op, "test://", trash.clone(), &tx));
        assert!(p.join(TRASH_DIR).is_dir() && rx.try_recv().is_err(), "kept while Android's file is there");
        std::fs::remove_file(p.join(".trashed-1-a.jpg")).unwrap();
        rt.block_on(drop_empty_trash(&op, "test://", trash.clone(), &tx));
        let deleted = match rx.try_recv() {
            Ok(Msg::At(_, m)) => *m,
            m => panic!("not a message of its source: {}", m.is_ok()),
        };
        assert!(!p.join(TRASH_DIR).exists() && matches!(deleted, Msg::Deleted(t, _, Ok(())) if t == trash));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
