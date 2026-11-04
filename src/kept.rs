//! An image changed on a phone or a server (Save, Rotate and crop) is first written to the app's
//! data folder (`~/.local/share/eyecansee/edits`, `%LOCALAPPDATA%\eyecansee\edits`), one folder
//! per file, then sent back over the original. If sending fails the original there stays as it was
//! (`replace::replace`) and the changed image is still on the computer: a window says so, where it
//! is, and offers to try again, save it elsewhere or open its folder. The copy goes away only when
//! sending works.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::App;
use crate::source::{self, Store};

/// A changed image that could not be sent: where it was going, where it is kept, and why.
#[derive(Clone)]
pub struct Kept {
    pub store: Store,
    /// The address of the place (`Loader`'s base), which names the kept folder.
    pub base: String,
    pub path: String,
    pub file: PathBuf,
    pub how: source::Deletion,
    pub error: source::Error,
}

/// The app's data folder for kept images.
pub fn dir() -> Option<PathBuf> {
    Some(crate::component::dir()?.parent()?.join("edits"))
}

/// Phones and servers: an upload there can break off half way.
pub fn remote(store: &Store) -> bool {
    !matches!(store, Store::Dal(_))
}

/// The folder for the image `path` of the place `base`, in `root`: its name and a hash of the
/// whole address, so the same image always gets the same folder and two images with the same name
/// do not mix.
fn folder(root: &Path, base: &str, path: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    (base, path).hash(&mut h);
    root.join(format!("{}-{:016x}", local_name(path), h.finish()))
}

/// The name of `path` as a file on this computer: the characters Windows does not allow in a name
/// (a phone or a server allows them) become "_".
fn local_name(path: &str) -> String {
    source::name(path).chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect()
}

/// Writes `data` as the image `path` of `base` in its folder in `root`, with a text saying where
/// it was going; gives the file.
fn keep_in(root: &Path, base: &str, path: &str, data: &[u8]) -> std::io::Result<PathBuf> {
    let folder = folder(root, base, path);
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(local_name(path));
    std::fs::write(&file, data)?;
    std::fs::write(folder.join("from.txt"), format!("{}\n{path}\n", crate::address::without_password(base)))?;
    Ok(file)
}

/// The folder of a kept image in `root`, gone once the image has been sent. Only a folder right
/// inside `root`: nothing else is ever removed.
fn forget_in(root: &Path, file: &Path) {
    if let Some(folder) = file.parent().filter(|f| f.parent() == Some(root)) {
        let r = std::fs::remove_dir_all(folder);
        log!("kept copy {folder:?} removed: {r:?}");
    }
}

/// `replace::replace`, for a phone or a server with the image kept on the computer first: on
/// success the copy goes, on failure it stays and comes back with the error. On the computer's
/// own disks, or if the copy cannot be written, as `replace` alone.
pub(crate) async fn replace(store: &Store, base: &str, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), (source::Error, Option<Box<Kept>>)> {
    replace_in(dir().filter(|_| remote(store)), store, base, path, data, how).await
}

/// `replace`, keeping the image in `root` when there is one.
async fn replace_in(root: Option<PathBuf>, store: &Store, base: &str, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), (source::Error, Option<Box<Kept>>)> {
    let file = match root.clone() {
        Some(root) => {
            let (base, to, data) = (base.to_string(), path.to_string(), data.clone());
            tokio::task::spawn_blocking(move || keep_in(&root, &base, &to, &data)).await.ok().and_then(|r| r.inspect_err(|e| log!("keep {path:?}: {e}")).ok())
        }
        None => None,
    };
    match (crate::replace::replace(store, path, data, how).await, file) {
        (Ok(()), Some(file)) => {
            if let Some(root) = &root {
                forget_in(root, &file);
            }
            Ok(())
        }
        (Ok(()), None) => Ok(()),
        (Err(e), file) => {
            log!("save {path:?} failed, kept as {file:?}");
            let kept = file.map(|file| Box::new(Kept { store: store.clone(), base: base.to_string(), path: path.to_string(), file, how, error: e.clone() }));
            Err((e, kept))
        }
    }
}

#[derive(Clone, Copy)]
enum Choice {
    Retry,
    SaveAs,
    Folder,
}

impl App {
    /// A save that did not reach the phone or server, its image kept: the window, then the usual
    /// end of a save.
    pub(crate) fn on_not_replaced(&mut self, ctx: &egui::Context, kept: Box<Kept>) {
        let (path, error) = (kept.path.clone(), kept.error.clone());
        self.kept.push(*kept);
        self.on_replaced(ctx, path, Err(error));
    }

    /// The first kept image: what happened, where it is, and what can be done.
    pub(crate) fn kept_window(&mut self, ctx: &egui::Context) {
        let Some(k) = self.kept.first().cloned() else { return };
        let place = if matches!(k.store, Store::Mtp(_)) { "kept-failed-phone" } else { "kept-failed-server" };
        let (id, detail) = &k.error;
        let error = self.i18n.t_args(id, &[("detail", detail)]);
        let folder = k.file.parent().unwrap_or(&k.file).display().to_string();
        let text = self.i18n.t_args(place, &[("name", &crate::save_dialogs::md(source::name(&k.path))), ("error", &error), ("folder", &crate::save_dialogs::md(&folder))]);
        let t = |id| self.i18n.t(id);
        // Esc closes; Enter does nothing here (it must not choose one of the actions).
        let choices = vec![(t("kept-retry"), self.loader.is_some(), Choice::Retry), (t("menu-save-copy"), true, Choice::SaveAs), (t("kept-open-folder"), true, Choice::Folder)];
        match crate::confirm::choose(ctx, "kept", 480.0, text, choices, t("notice-close"), self.delete_answer.take().and_then(|enter| (!enter).then_some(None))) {
            Some(Some(Choice::Retry)) => {
                self.kept.remove(0);
                if let Some(loader) = &self.loader {
                    log!("save {:?}: again from {:?}", k.path, k.file);
                    self.saving.insert(k.path.clone());
                    loader.replace_kept(k);
                }
            }
            Some(Some(Choice::SaveAs)) => {
                self.kept.remove(0);
                self.save_kept_as(ctx, k.file);
            }
            Some(Some(Choice::Folder)) => open_folder(Path::new(&folder)),
            Some(None) => {
                self.kept.remove(0);
            }
            None => {}
        }
    }

    /// The system window to choose where a kept image goes; the copy stays where it was.
    fn save_kept_as(&self, ctx: &egui::Context, file: PathBuf) {
        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let dialog = rfd::AsyncFileDialog::new().set_title(self.i18n.t("menu-save-copy")).set_file_name(name);
        let dialog = match std::env::home_dir() {
            Some(home) => dialog.set_directory(home),
            None => dialog,
        };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let Some(to) = dialog.save_file().await else { return };
            let to = to.path().to_path_buf();
            let r = std::fs::copy(&file, &to).map(|_| to.display().to_string()).map_err(|e| ("error-detail", e.to_string()));
            log!("kept copy {file:?} saved as {to:?}: {r:?}");
            let _ = tx.send(crate::loader::Msg::KeptSaved(r));
            ctx.request_repaint();
        });
    }

    pub(crate) fn on_kept_saved(&mut self, r: Result<String, source::Error>) {
        match r {
            Ok(to) => self.notice = Some((self.i18n.t_args("kept-saved", &[("path", &crate::save_dialogs::md(&to))]), None)),
            Err((id, detail)) => self.error = Some(self.i18n.t_args("save-failed", &[("detail", &self.i18n.t_args(id, &[("detail", &detail)]))])),
        }
    }
}

/// The folder in the system's file manager.
fn open_folder(folder: &Path) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(folder).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(folder).spawn();
    log!("open folder {folder:?}: {:?}", r.map(|_| ()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kept image: in its own folder under the data folder, with where it was going; the same
    /// image gets the same folder, another image another; `forget` removes it, and nothing outside.
    #[test]
    fn kept_and_forgotten() {
        let data = std::env::temp_dir().join(format!("eyecansee-kept-{}", std::process::id()));
        let root = data.join("edits");
        let file = keep_in(&root, "mtp://phone/", "DCIM/a:b.jpg", b"new").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        assert_eq!(file.file_name().unwrap(), "a_b.jpg");
        let folder = file.parent().unwrap();
        assert_eq!(folder.parent(), Some(root.as_path()));
        assert!(folder.file_name().unwrap().to_string_lossy().starts_with("a_b.jpg-"));
        assert_eq!(std::fs::read_to_string(folder.join("from.txt")).unwrap(), "mtp://phone/\nDCIM/a:b.jpg\n");
        assert_eq!(keep_in(&root, "mtp://phone/", "DCIM/a:b.jpg", b"newer").unwrap(), file);
        assert_ne!(keep_in(&root, "mtp://other/", "DCIM/a:b.jpg", b"x").unwrap(), file);
        forget_in(&root, &file);
        assert!(!folder.exists());
        // Not a kept image: left alone.
        let outside = data.join("x.jpg");
        std::fs::write(&outside, b"x").unwrap();
        forget_in(&root, &outside);
        assert!(outside.exists());
        std::fs::remove_dir_all(&data).unwrap();
    }

    /// A save that fails (a folder that cannot be written): the original as it was, the new image
    /// kept with where it was going; once the folder can be written, trying again saves it and the
    /// kept copy goes.
    #[cfg(unix)]
    #[test]
    fn failed_save_kept_then_retried() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("eyecansee-kept-save-{}", std::process::id()));
        let (p, root) = (dir.join("p"), dir.join("edits"));
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("a.jpg"), b"old").unwrap();
        let store = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o555)).unwrap();
        let save = || rt.block_on(replace_in(Some(root.clone()), &store, "file:///x/", "p/a.jpg", b"new".to_vec(), source::Deletion::Permanent));
        let Err((_, Some(k))) = save() else { panic!("not kept") };
        assert_eq!(std::fs::read(p.join("a.jpg")).unwrap(), b"old");
        assert_eq!(std::fs::read(&k.file).unwrap(), b"new");
        assert_eq!((k.base.as_str(), k.path.as_str()), ("file:///x/", "p/a.jpg"));
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        save().unwrap_or_else(|_| panic!("not saved"));
        assert_eq!(std::fs::read(p.join("a.jpg")).unwrap(), b"new");
        assert!(!k.file.parent().unwrap().exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
