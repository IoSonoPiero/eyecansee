//! An image changed on a phone or a server (Save, the editor's copy and overwrite) is first
//! written to the app's data folder (`~/.local/share/eyecansee/edits`,
//! `%LOCALAPPDATA%\eyecansee\edits`), one folder per file, then sent: over the original, or as a
//! new file next to it (`Send`). If sending fails the original there stays as it was
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
    /// The server, which connects again by itself. `None` for a phone: holding it would keep the
    /// session that broke (`Phone::shared` would hand it out again after the phone is plugged back
    /// in), so "Try again" opens it anew from `base`.
    pub store: Option<Store>,
    /// The address of the place (`Loader`'s base), which names the kept folder.
    pub base: String,
    pub path: String,
    pub file: PathBuf,
    pub how: source::Deletion,
    /// What sending it does, again with "Try again".
    pub send: Send,
    pub error: source::Error,
}

/// How a kept image goes where it belongs.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Send {
    /// Over the original (`replace::replace`): Save in the viewer and on the grid.
    Replace,
    /// Over the original only with the original in its trash (`replace::replace_keeping`): the
    /// editor's "Overwrite the original".
    Keeping,
    /// A new file, never over another one: the editor's copy.
    Create,
    /// Over a file EyeCanSee itself wrote in this editing (the copy updated, the original
    /// overwritten again): the version replaced is deleted, not trashed (it can be made again).
    Again,
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
    send(store, base, path, data, how, Send::Replace).await
}

/// As `replace`, sent as `send` says.
pub(crate) async fn send(store: &Store, base: &str, path: &str, data: Vec<u8>, how: source::Deletion, send: Send) -> Result<(), (source::Error, Option<Box<Kept>>)> {
    send_in(dir().filter(|_| remote(store)), store, base, path, data, how, send).await
}

/// A new file `path`, never over an item already there.
async fn create(store: &Store, path: &str, data: Vec<u8>) -> Result<(), source::Error> {
    if store.names(&source::parent(path)).await?.contains(source::name(path)) {
        return Err(("error-exists", source::name(path).to_string()));
    }
    crate::replace::write_whole(store, path, data).await
}

/// `send`, keeping the image in `root` when there is one.
async fn send_in(root: Option<PathBuf>, store: &Store, base: &str, path: &str, data: Vec<u8>, how: source::Deletion, send: Send) -> Result<(), (source::Error, Option<Box<Kept>>)> {
    let file = match root.clone() {
        Some(root) => {
            let (base, to, data) = (base.to_string(), path.to_string(), data.clone());
            tokio::task::spawn_blocking(move || keep_in(&root, &base, &to, &data)).await.ok().and_then(|r| r.inspect_err(|e| log!("keep {path:?}: {e}")).ok())
        }
        None => None,
    };
    let sent = match send {
        Send::Replace => crate::replace::replace(store, path, data, how).await,
        Send::Keeping => crate::replace::replace_keeping(store, path, data, how).await,
        Send::Create => create(store, path, data).await,
        Send::Again => crate::replace::replace(store, path, data, source::Deletion::Permanent).await,
    };
    match (sent, file) {
        (Ok(()), Some(file)) => {
            if let Some(root) = &root {
                forget_in(root, &file);
            }
            Ok(())
        }
        (Ok(()), None) => Ok(()),
        (Err(e), file) => {
            log!("save {path:?} failed, kept as {file:?}");
            let kept = file.map(|file| Box::new(Kept { store: (!matches!(store, Store::Mtp(_))).then(|| store.clone()), base: base.to_string(), path: path.to_string(), file, how, send, error: e.clone() }));
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
        let place = if k.store.is_none() { "kept-failed-phone" } else { "kept-failed-server" };
        let (id, detail) = &k.error;
        let error = self.i18n.error_md(id, detail);
        let folder = k.file.parent().unwrap_or(&k.file).display().to_string();
        let text = self.i18n.t_args(place, &[("name", &crate::save_dialogs::md(source::name(&k.path))), ("error", &error), ("folder", &format!("**{}**", crate::save_dialogs::md(&folder)))]);
        let t = |id| self.i18n.t(id);
        // Esc closes; Enter does nothing here (it must not choose one of the actions).
        let choices = vec![(t("kept-retry"), self.loader.is_some(), Choice::Retry), (t("menu-save-copy"), true, Choice::SaveAs), (t("kept-open-folder"), true, Choice::Folder)];
        match crate::confirm::choose(ctx, "kept", 480.0, (t("kept-title"), text), choices, t("notice-close"), self.delete_answer.take().and_then(|enter| (!enter).then_some(None))) {
            Some(Some(Choice::Retry)) => {
                self.kept.remove(0);
                if let Some(loader) = &self.loader {
                    log!("save {:?}: again from {:?}", k.path, k.file);
                    self.saving.insert(k.path.clone());
                    // The editor's copy: shown once it is there, as after a Save that went well.
                    if matches!(k.send, Send::Create | Send::Again) {
                        self.select_after = Some(k.path.clone());
                    }
                    loader.replace_kept(k);
                }
            }
            // The window stays: the image still has to reach the phone or server ("Try again").
            Some(Some(Choice::SaveAs)) => self.save_kept_as(ctx, k.file),
            Some(Some(Choice::Folder)) => open_folder(Path::new(&folder)),
            Some(None) => {
                self.kept.remove(0);
            }
            None => {}
        }
    }

    /// The system window to choose where a kept image goes; the copy stays where it was, and so does
/// its window.
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
            let r = copy_whole(&file, &to).map(|()| to.display().to_string()).map_err(|e| ("error-detail", e.to_string()));
            log!("kept copy {file:?} saved as {to:?}: {r:?}");
            let _ = tx.send(crate::loader::Msg::KeptSaved(r));
            ctx.request_repaint();
        });
    }

    pub(crate) fn on_kept_saved(&mut self, r: Result<String, source::Error>) {
        match r {
            Ok(to) => self.tell("kept-saved-title", self.i18n.t_args("kept-saved", &[("path", &crate::save_dialogs::md(&to))])),
            Err((id, detail)) => self.error = Some(self.i18n.save_failed(&(id, detail))),
        }
    }
}

/// `file` copied to `to` on this computer under the hidden name of a "Save" next to it, renamed at
/// the end: a copy that fails (a full disk) leaves nothing, and a file it replaces stays as it was
/// (#90).
fn copy_whole(file: &Path, to: &Path) -> std::io::Result<()> {
    let own = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = to.with_file_name(source::save_names(&own).0);
    let r = std::fs::copy(file, &tmp).and_then(|_| std::fs::rename(&tmp, to));
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
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
        // As root (the CI container) a read-only folder can still be written: nothing to try.
        if std::fs::write(p.join("probe"), b"").is_ok() {
            eprintln!("failed_save_kept_then_retried: skipped, the folder stays writable (root)");
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        }
        let save = || rt.block_on(send_in(Some(root.clone()), &store, "file:///x/", "p/a.jpg", b"new".to_vec(), source::Deletion::Permanent, Send::Replace));
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

    /// The editor's copy: a new file, kept on the computer until it is there; never over an item
    /// of the same name (the copy stays kept, sent as a copy again). Its overwrite, with a trash
    /// that cannot be made: the original as it was, the new image kept, to be sent the same way.
    #[test]
    fn copy_and_overwrite_kept() {
        let dir = std::env::temp_dir().join(format!("eyecansee-kept-copy-{}", std::process::id()));
        let (p, root) = (dir.join("p"), dir.join("edits"));
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("a.jpg"), b"old").unwrap();
        let store = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let go = |path: &str, send| rt.block_on(send_in(Some(root.clone()), &store, "sftp://x/", path, b"new".to_vec(), source::Deletion::TrashFolder, send));
        go("p/a (edited).jpg", Send::Create).unwrap_or_else(|_| panic!("copy not written"));
        assert_eq!(std::fs::read(p.join("a (edited).jpg")).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "kept copy gone once sent");
        let Err((e, Some(k))) = go("p/a.jpg", Send::Create) else { panic!("written over a.jpg") };
        assert_eq!((e.0, k.send), ("error-exists", Send::Create));
        assert_eq!(std::fs::read(p.join("a.jpg")).unwrap(), b"old");
        std::fs::write(p.join(crate::trash::TRASH_DIR), b"a file").unwrap();
        let Err((_, Some(k))) = go("p/a.jpg", Send::Keeping) else { panic!("overwritten without a trash") };
        assert_eq!((std::fs::read(p.join("a.jpg")).unwrap(), std::fs::read(&k.file).unwrap(), k.send), (b"old".to_vec(), b"new".to_vec(), Send::Keeping));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The editor's copy and overwrite on a real server, in a new folder deleted at the end; the
    /// kept copies in a temporary folder. `EYECANSEE_SFTP=sftp://host/folder/` (or `EYECANSEE_FTP`)
    /// `cargo test --release … kept::tests::edit_server_live -- --exact --ignored --nocapture`.
    /// A copy written, never over a name already there; an overwrite with the original in the
    /// folder's trash; the copy updated and the original overwritten again (only the true original in
    /// the trash); with a trash that cannot be made (a file has its name) nothing changes.
    #[test]
    #[ignore]
    fn edit_server_live() {
        let root = std::env::temp_dir().join(format!("eyecansee-kept-live-{}", std::process::id()));
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // A phone (EYECANSEE_MTP=<serial>, EyeCanSee closed): in its first storage.
            let (store, dir, url) = if let Ok(serial) = std::env::var("EYECANSEE_MTP") {
                let op = Store::Mtp(std::sync::Arc::new(crate::mtp::Phone::new(&serial)));
                let first = op.list("").await.unwrap().dirs[0].clone();
                (op, format!("{first}eyecansee-edit-test-{secs}/"), format!("mtp://{serial}/"))
            } else {
                server_dir(secs).await
            };
            edit_live(&root, &store, &dir, &url).await;
        });
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The server of EYECANSEE_SFTP or EYECANSEE_FTP, the test folder in its home, its address
    /// without the password.
    async fn server_dir(secs: u64) -> (Store, String, String) {
        let url = std::env::var("EYECANSEE_SFTP").or_else(|_| std::env::var("EYECANSEE_FTP")).expect("EYECANSEE_SFTP, EYECANSEE_FTP or EYECANSEE_MTP");
        {
            let mut s = source::open(&url).unwrap();
            // A key with a passphrase, as a saved server has it (never the system's agent):
            // EYECANSEE_SSH_KEY and EYECANSEE_SSH_PASSPHRASE, never written to a file.
            if let (Ok(key), Ok(passphrase)) = (std::env::var("EYECANSEE_SSH_KEY"), std::env::var("EYECANSEE_SSH_PASSPHRASE")) {
                let u = url::Url::parse(&url).unwrap();
                let target = crate::sftp::Target { user: u.username().into(), host: u.host_str().unwrap().into(), port: u.port(), password: None, key: Some((key.into(), passphrase)), saved: true };
                s.op = Store::Sftp(crate::sftp::Server::shared(target));
            }
            let home = match &s.op {
                Store::Sftp(server) => server.home().await.unwrap(),
                Store::Ftp(server) => server.home().await.unwrap(),
                _ => panic!("not a server"),
            };
            let dir = format!("{}eyecansee-edit-test-{secs}/", if s.home { &home } else { &s.dir });
            // The address without its password, as the app keeps it (`Source::base`).
            (s.op, dir, s.base)
        }
    }

    /// The steps of `edit_server_live` in `dir` (made, then deleted) of `op`.
    async fn edit_live(root: &std::path::Path, op: &Store, dir: &str, url: &str) {
        {
            println!("{url}: {dir}");
            op.create_dir(dir).await.unwrap();
            let put = |path: String, data: &'static [u8]| async move { crate::replace::write_whole(op, &path, data.to_vec()).await.unwrap() };
            put(format!("{dir}a.jpg"), b"original").await;
            let send = |path: String, data: &'static [u8], how| {
                let (root, url) = (root.to_path_buf(), url.to_string());
                async move { send_in(Some(root), op, &url, &path, data.to_vec(), source::Deletion::TrashFolder, how).await }
            };
            // The copy, then the same name again: refused, kept.
            send(format!("{dir}a (edited).jpg"), b"copy", Send::Create).await.unwrap_or_else(|_| panic!("copy not written"));
            assert_eq!(op.read(&format!("{dir}a (edited).jpg")).await.unwrap(), b"copy");
            let Err((e, Some(k))) = send(format!("{dir}a (edited).jpg"), b"again", Send::Create).await else { panic!("written over the copy") };
            assert_eq!((e.0, k.send), ("error-exists", Send::Create));
            assert_eq!(op.read(&format!("{dir}a (edited).jpg")).await.unwrap(), b"copy");
            // Update copy: the same copy written again, the old one not trashed.
            send(format!("{dir}a (edited).jpg"), b"copy 2", Send::Again).await.unwrap_or_else(|_| panic!("copy not updated"));
            assert_eq!(op.read(&format!("{dir}a (edited).jpg")).await.unwrap(), b"copy 2");
            // Overwrite: the original into the trash of the folder.
            send(format!("{dir}a.jpg"), b"new", Send::Keeping).await.unwrap_or_else(|_| panic!("not overwritten"));
            assert_eq!(op.read(&format!("{dir}a.jpg")).await.unwrap(), b"new");
            assert_eq!(op.read(&format!("{dir}{}/a.jpg", crate::trash::TRASH_DIR)).await.unwrap(), b"original");
            // Overwrite again: over what was written, the trash keeps only the true original.
            send(format!("{dir}a.jpg"), b"newer", Send::Again).await.unwrap_or_else(|_| panic!("not overwritten again"));
            assert_eq!(op.read(&format!("{dir}a.jpg")).await.unwrap(), b"newer");
            assert_eq!(op.read(&format!("{dir}{}/a.jpg", crate::trash::TRASH_DIR)).await.unwrap(), b"original");
            let trashed: Vec<String> = op.names(&format!("{dir}{}/", crate::trash::TRASH_DIR)).await.unwrap().into_iter().collect();
            println!("trash after two overwrites and an update: {trashed:?}");
            assert!(!trashed.iter().any(|n| n.contains("edited") || n.contains("new")), "{trashed:?}");
            // A trash that cannot be made: the original stays, nothing hidden is left.
            op.create_dir(&format!("{dir}b/")).await.unwrap();
            put(format!("{dir}b/a.jpg"), b"original").await;
            put(format!("{dir}b/{}", crate::trash::TRASH_DIR), b"a file").await;
            let Err((e, Some(k))) = send(format!("{dir}b/a.jpg"), b"new", Send::Keeping).await else { panic!("overwritten without a trash") };
            println!("blocked trash: {e:?}");
            assert_eq!((op.read(&format!("{dir}b/a.jpg")).await.unwrap(), k.send), (b"original".to_vec(), Send::Keeping));
            let mut names: Vec<String> = op.names(&format!("{dir}b/")).await.unwrap().into_iter().collect();
            names.sort();
            assert_eq!(names, [crate::trash::TRASH_DIR, "a.jpg"]);
            op.delete(dir, source::Deletion::Permanent).await.unwrap();
            assert_eq!(op.check_dir(dir).await.unwrap_err().0, source::NOT_FOUND);
            println!("{url}: all done, {dir} deleted");
        }
    }

    /// "Save a copy" of a kept image: it replaces a file through a hidden name; a copy that fails
    /// leaves the file there as it was and nothing hidden (#90).
    #[test]
    fn copy_whole_replaces_or_leaves_nothing() {
        let dir = std::env::temp_dir().join(format!("eyecansee-copy-whole-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("kept.jpg"), b"new").unwrap();
        std::fs::write(dir.join("a.jpg"), b"old").unwrap();
        let names = || std::fs::read_dir(&dir).unwrap().count();
        assert!(copy_whole(&dir.join("missing.jpg"), &dir.join("a.jpg")).is_err());
        assert_eq!((std::fs::read(dir.join("a.jpg")).unwrap(), names()), (b"old".to_vec(), 2));
        #[cfg(target_os = "linux")]
        {
            std::os::unix::fs::symlink("/dev/full", dir.join(".a.jpg.eyecansee-new")).unwrap();
            assert!(copy_whole(&dir.join("kept.jpg"), &dir.join("a.jpg")).is_err());
            assert_eq!((std::fs::read(dir.join("a.jpg")).unwrap(), names()), (b"old".to_vec(), 2));
        }
        copy_whole(&dir.join("kept.jpg"), &dir.join("a.jpg")).unwrap();
        assert_eq!((std::fs::read(dir.join("a.jpg")).unwrap(), names()), (b"new".to_vec(), 2));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
