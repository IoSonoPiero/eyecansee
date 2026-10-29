//! Trash folders, deletion, putting items back, and the hidden files of a "Save" over an image
//! (`source::Store` methods and the name helpers they share). Local files go to the system trash
//! (the `trash` crate); phones and servers use `.EyeCanSee-trash/` inside the folder.

use std::collections::{BTreeSet, HashSet};
use std::sync::Mutex;

use crate::decode::is_media;
use crate::settings::RemoteDelete;
use crate::source::{dal_error, in_gvfs, local_path, name, parent, Error, Store};

/// Hidden folder, next to the image, where images deleted on phones and servers go.
pub const TRASH_DIR: &str = ".EyeCanSee-trash";

/// Endings of the hidden names an image has while it is saved over itself (`loader::replace`):
/// the new file is written as ".name.eyecansee-new", the original waits as ".name.eyecansee-old"
/// until the new one has taken its name.
const NEW: &str = ".eyecansee-new";
const OLD: &str = ".eyecansee-old";

/// Images being saved over themselves: `Store::recover` leaves their hidden files alone.
pub static SAVING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// The two hidden paths of `path` while it is saved over itself: new file, original.
pub fn save_names(path: &str) -> (String, String) {
    let (dir, own) = (parent(path), name(path));
    (format!("{dir}.{own}{NEW}"), format!("{dir}.{own}{OLD}"))
}

/// Is this name one of `save_names`?
pub fn leftover(name: &str) -> bool {
    name.starts_with('.') && (name.ends_with(NEW) || name.ends_with(OLD))
}

/// What to do when an item put back from the trash finds its name taken (`Store::restore`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Conflict {
    Ask,
    Replace,
    KeepBoth,
    Merge,
}

/// Is this the trash folder of some folder?
pub fn is_trash(path: &str) -> bool {
    path.ends_with('/') && name(path) == TRASH_DIR
}

/// Is this item inside a trash folder? Android's trashed files count too.
pub fn in_trash(path: &str) -> bool {
    is_trash(&parent(path)) || android_trashed(name(path)).is_some()
}

/// The original name of a file Android's gallery put in its trash (30 days, then deleted):
/// ".trashed-<expiry seconds>-<name>", hidden, still in its folder. Only images and videos.
pub fn android_trashed(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(".trashed-")?;
    let (expiry, own) = rest.split_once('-')?;
    (!expiry.is_empty() && expiry.bytes().all(|b| b.is_ascii_digit()) && is_media(own)).then_some(own)
}

/// The trash an item belongs to: the folder of an item of a trash folder, the folder's trash
/// for an Android trashed file (which lies in the folder itself).
pub fn trash_of(path: &str) -> String {
    let dir = parent(path);
    if android_trashed(name(path)).is_some() { format!("{dir}{TRASH_DIR}/") } else { dir }
}

/// The name to show for an item: a trashed Android file by its original name.
pub fn shown(path: &str) -> &str {
    android_trashed(name(path)).unwrap_or(name(path))
}

/// `name` if free, otherwise "name (2).ext", "name (3).ext"… as file managers do.
pub fn free_name(taken: &HashSet<String>, name: &str) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    (2..).map(|n| format!("{stem} ({n}){ext}")).find(|n| !taken.contains(n)).unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Deletion {
    SystemTrash,
    TrashFolder,
    Permanent,
}

impl Store {
    /// Files left by a "Save" cut short (the app closed): a new file goes, it may be incomplete; an
    /// original whose name is free takes it back. An original next to its new version stays: the
    /// save had worked, and the next save of that image sends it where a deletion would.
    pub(crate) async fn recover(&self, dir: &str, leftovers: &[String]) {
        let Ok(names) = self.names(dir).await else { return };
        for path in leftovers {
            let own = name(path);
            let Some(original) = own.strip_prefix('.').and_then(|n| n.strip_suffix(NEW).or_else(|| n.strip_suffix(OLD))) else { continue };
            if SAVING.lock().unwrap().contains(&format!("{dir}{original}")) {
                continue;
            }
            let r = if own.ends_with(NEW) {
                self.delete(path, Deletion::Permanent).await
            } else if !names.contains(original) {
                self.rename(path, original).await.map(|_| ())
            } else {
                continue;
            };
            log!("recover {path:?}: {r:?}");
        }
    }

    /// What deleting `path` does: local files go to the system trash unless `local_permanent`;
    /// phones and servers (also when mounted by gvfs, which has no trash) follow `remote`.
    pub fn deletion(&self, path: &str, local_permanent: bool, remote: RemoteDelete) -> Deletion {
        match self {
            Store::Dal(op) if op.info().scheme() == "fs" && !in_gvfs(&op.info().root(), path) => match () {
                _ if local_permanent => Deletion::Permanent,
                _ if crate::devices::has_recycle_bin(&local_path(op, path)) => Deletion::SystemTrash,
                // Windows USB stick, card or network drive: no Recycle Bin, our own trash folder.
                _ => Deletion::TrashFolder,
            },
            _ if remote == RemoteDelete::Permanent => Deletion::Permanent,
            _ => Deletion::TrashFolder,
        }
    }

    /// Deletes an image, or a folder (path ending in "/") with everything inside it.
    pub async fn delete(&self, path: &str, how: Deletion) -> Result<(), Error> {
        let detail = |e: &dyn std::fmt::Display| ("error-detail", e.to_string());
        match (self, how) {
            (Store::Dal(op), Deletion::SystemTrash) => {
                let full = local_path(op, path);
                tokio::task::spawn_blocking(move || ::trash::delete(&full)).await.map_err(|e| detail(&e))?.map_err(|e| detail(&e))
            }
            // Recursive only for folders: it works by prefix, "a.jpg" would also take "a.jpg.bak".
            (Store::Dal(op), Deletion::Permanent) if path.ends_with('/') => {
                op.delete_with(path).recursive(true).await.map_err(dal_error)
            }
            (Store::Dal(op), Deletion::Permanent) => op.delete(path).await.map_err(dal_error),
            (_, Deletion::TrashFolder) => self.to_trash_folder(path, name(path)).await,
            (Store::Mtp(phone), _) => phone.delete(path).await,
            (Store::Sftp(server), _) => server.delete(path).await,
        }
    }

    /// Moves an item into the trash folder of its folder, named `own`, or "own (2)"… if taken.
    pub async fn to_trash_folder(&self, path: &str, own: &str) -> Result<(), Error> {
        let dir = parent(path);
        let trash = format!("{dir}{TRASH_DIR}/");
        let mut taken = self.names(&trash).await.unwrap_or_default();
        // On a phone the item is renamed before being moved: free in its folder too, apart from itself.
        if let Store::Mtp(_) = self {
            taken.extend(self.names(&dir).await?.into_iter().filter(|n| n != name(path)));
            // A new name (the original of a "Save", whose name its new version holds) is given
            // once inside the trash, where only the trash counts: not "name (2)" for nothing.
            if own != name(path) {
                let inside = self.move_into(path, &trash, &free_name(&taken, name(path))).await?;
                let there = self.names(&trash).await?;
                return self.rename(&inside, &free_name(&there, own)).await.map(|_| ());
            }
        }
        self.move_into(path, &trash, &free_name(&taken, own)).await.map(|_| ())
    }

    /// Puts an item of a trash folder back in the folder containing the trash. With a name
    /// already there: `Conflict::Ask` gives `error-exists` (the app asks what to do), the others
    /// replace (files), keep both ("name (2)") or merge (folders: their contents, keeping both
    /// files with the same name). Returns the path where it went.
    pub async fn restore(&self, path: &str, conflict: Conflict) -> Result<String, Error> {
        let trash = parent(path);
        let (own, dest) = match android_trashed(name(path)) {
            Some(own) => (own, trash.clone()),
            None => (name(path), parent(&trash)),
        };
        let taken = self.names(&dest).await?;
        if !taken.contains(own) {
            return self.put_back(path, &dest, own).await;
        }
        match conflict {
            Conflict::Ask => Err(("error-exists", own.to_string())),
            Conflict::KeepBoth => {
                let mut all = taken;
                all.extend(self.names(&trash).await?.into_iter().filter(|n| n != own));
                self.put_back(path, &dest, &free_name(&all, own)).await
            }
            Conflict::Replace => {
                let target = format!("{dest}{own}");
                self.delete(&target, Deletion::Permanent).await?;
                self.put_back(path, &dest, own).await
            }
            Conflict::Merge => {
                let target = format!("{dest}{own}/");
                Box::pin(self.merge(path, &target)).await?;
                Ok(target)
            }
        }
    }

    /// An item back from the trash as `dest/name`: a plain rename when it already lies in `dest`
    /// (Android's trashed files stay in their folder). Moving an object into the folder it is in
    /// made the POCO X7 Pro answer `GeneralError` after the rename had already happened (2026-10-28).
    async fn put_back(&self, path: &str, dest: &str, name: &str) -> Result<String, Error> {
        if parent(path) == dest { self.rename(path, name).await } else { self.move_into(path, dest, name).await }
    }

    /// Moves the contents of folder `from` into folder `to`: folders on both sides merge, files
    /// on both sides are both kept. Then `from`, now empty, goes.
    async fn merge(&self, from: &str, to: &str) -> Result<(), Error> {
        let present = self.names(to).await?;
        for src in self.children(from).await? {
            let (child, is_dir) = (name(&src).to_string(), src.ends_with('/'));
            if !present.contains(&child) {
                self.move_into(&src, to, &child).await?;
            } else if is_dir && self.is_dir(&format!("{to}{child}/")).await {
                Box::pin(self.merge(&src, &format!("{to}{child}/"))).await?;
            } else {
                let mut all = self.names(to).await?;
                all.extend(self.names(from).await?.into_iter().filter(|n| *n != child));
                self.move_into(&src, to, &free_name(&all, &child)).await?;
            }
        }
        self.delete(from, Deletion::Permanent).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendal::{services, Operator};

    /// Android's trash: hidden ".trashed-<seconds>-<name>" files, shown in the folder's trash
    /// and put back by renaming.
    #[test]
    fn android_trash() {
        assert_eq!(android_trashed(".trashed-1794410026-IMG_1.jpg"), Some("IMG_1.jpg"));
        assert_eq!(android_trashed(".trashed-1794410026-clip.mp4"), Some("clip.mp4"));
        assert_eq!(android_trashed(".trashed-1794410026-notes.txt"), None);
        assert_eq!(android_trashed(".trashed-x-IMG_1.jpg"), None);
        assert_eq!(android_trashed("IMG_1.jpg"), None);
        assert!(in_trash("photos/.trashed-1-a.jpg"));
        assert_eq!(shown("photos/.trashed-1-a.jpg"), "a.jpg");
        assert_eq!(shown("photos/a.jpg"), "a.jpg");

        let dir = std::env::temp_dir().join(format!("eyecansee-android-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("photos")).unwrap();
        std::fs::write(dir.join("photos/.trashed-1-a.jpg"), b"x").unwrap();
        std::fs::write(dir.join("photos/b.jpg"), b"x").unwrap();
        let op = Store::Dal(Operator::new(services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let t = format!("photos/{TRASH_DIR}/");
        // The folder: one image, a trash although no trash folder exists.
        let l = rt.block_on(op.list("photos/")).unwrap();
        assert_eq!(l.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["photos/b.jpg"]);
        assert_eq!(l.trash.as_deref(), Some(t.as_str()));
        // The trash: the Android file; Restore all sees it; back by renaming.
        let l = rt.block_on(op.list(&t)).unwrap();
        assert_eq!(l.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["photos/.trashed-1-a.jpg"]);
        assert_eq!(rt.block_on(op.children(&t)).unwrap(), ["photos/.trashed-1-a.jpg"]);
        assert_eq!(rt.block_on(op.restore("photos/.trashed-1-a.jpg", Conflict::Ask)).unwrap(), "photos/a.jpg");
        assert!(dir.join("photos/a.jpg").exists() && !dir.join("photos/.trashed-1-a.jpg").exists());
        // Taken name: kept both.
        std::fs::write(dir.join("photos/.trashed-2-a.jpg"), b"y").unwrap();
        assert_eq!(rt.block_on(op.restore("photos/.trashed-2-a.jpg", Conflict::Ask)).unwrap_err().0, "error-exists");
        assert_eq!(rt.block_on(op.restore("photos/.trashed-2-a.jpg", Conflict::KeepBoth)).unwrap(), "photos/a (2).jpg");
        // No Android files left and an empty trash folder: no trash shown.
        std::fs::create_dir_all(dir.join(&t)).unwrap();
        assert_eq!(rt.block_on(op.list("photos/")).unwrap().trash, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn free_names() {
        let taken: HashSet<String> = ["a.jpg", "a (2).jpg", "dir", ".hidden"].map(String::from).into();
        assert_eq!(free_name(&taken, "b.jpg"), "b.jpg");
        assert_eq!(free_name(&taken, "a.jpg"), "a (3).jpg");
        assert_eq!(free_name(&taken, "dir"), "dir (2)");
        assert_eq!(free_name(&taken, ".hidden"), ".hidden (2)");
    }

    /// What an interrupted "Save" left is tidied up when the folder is read: an original alone
    /// takes its name back, a new file goes; an original next to its new version, and the files
    /// of a save still running, stay.
    #[test]
    fn recover_leftovers() {
        let dir = std::env::temp_dir().join(format!("eyecansee-recover-{}", std::process::id()));
        let p = dir.join("p");
        std::fs::create_dir_all(&p).unwrap();
        let op = Store::Dal(Operator::new(services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        std::fs::write(p.join(".a.jpg.eyecansee-old"), b"a").unwrap();
        std::fs::write(p.join("b.jpg"), b"b").unwrap();
        std::fs::write(p.join(".b.jpg.eyecansee-new"), b"half").unwrap();
        std::fs::write(p.join("c.jpg"), b"c").unwrap();
        std::fs::write(p.join(".c.jpg.eyecansee-old"), b"c old").unwrap();
        std::fs::write(p.join(".d.jpg.eyecansee-new"), b"writing").unwrap();
        SAVING.lock().unwrap().insert("p/d.jpg".into());
        let l = tokio::runtime::Runtime::new().unwrap().block_on(op.list("p/")).unwrap();
        SAVING.lock().unwrap().remove("p/d.jpg");
        assert_eq!(l.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["p/a.jpg", "p/b.jpg", "p/c.jpg"]);
        assert_eq!(std::fs::read(p.join("a.jpg")).unwrap(), b"a");
        assert!(!p.join(".b.jpg.eyecansee-new").exists() && !p.join(".a.jpg.eyecansee-old").exists());
        assert!(p.join(".c.jpg.eyecansee-old").exists() && p.join(".d.jpg.eyecansee-new").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
