//! Reading and decoding in the background: the UI asks, the loader answers through a channel.
//! Images for the viewer start straight away; thumbnails go through a priority queue that the
//! UI rewrites when the selection or the visible rows change.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use eframe::egui::{ColorImage, Context};
use tokio::runtime::Handle;
use tokio::sync::{Notify, Semaphore};

use crate::source::{self, File, Listing, Store};
use crate::{decode, thumbcache, transfer};

/// Error id of an image left out of "Save": animated, several pages, a Motion Photo (`convert::can_turn_in_place`).
pub const SKIPPED: &str = "save-skipped";

/// Side of the decoded thumbnails; the grid shows them at this size or smaller.
pub const THUMB_SIDE: u32 = 256;

pub enum Msg {
    Listed(String, Result<Listing, source::Error>),
    /// Address handed over by EyeCanSee started again ("" = none): `single::listen`.
    Open(String),
    /// Connected external devices, sent by `devices::watch` when the list changes.
    Devices(Vec<crate::devices::Device>),
    /// Outcome of mounting a partition: absolute path or error.
    Mounted(Result<String, String>),
    Loaded { generation: u64, index: usize, thumb: bool, image: Result<ColorImage, String> },
    /// Something changed in the watched local folder (`App::watch`).
    Changed,
    /// Outcome of deleting an item (path relative to the source), and how.
    Deleted(String, source::Deletion, Result<(), source::Error>),
    /// Outcome of putting an item back from the trash: its path in the trash, where it went.
    Restored(String, Result<String, source::Error>),
    /// Everything in a trash folder, to put back one at a time (Restore all).
    TrashItems(Vec<String>),
    /// Outcome of renaming an item: old path, new path.
    Renamed(String, Result<String, source::Error>),
    /// Names already in the paste destination (to ask about), or an error.
    PasteCheck(Result<Vec<String>, source::Error>),
    /// End of a copy or move: source items that went away, outcome.
    Pasted(Vec<String>, Result<(), source::Error>),
    /// Outcome of `update::check`: a newer version downloaded and checked, up to date, or an error.
    Update(Result<Option<crate::update::Ready>, String>),
    /// "Save in another format": the original decoded, its preview and file size.
    Original(Result<(Arc<crate::convert::Original>, ColorImage, u64), String>),
    /// The image encoded with the options of `generation`, and the encoded file decoded back
    /// (`None`: no decoder for that format in this build).
    Encoded(u64, Result<(Arc<Vec<u8>>, Option<ColorImage>), String>),
    /// The converted image written: its path, or an error.
    Saved(Result<String, source::Error>),
    /// "Save" from the viewer, before the confirmation: the image, the turn, and the file to write
    /// with whether it lost quality (encoded again), or an error (`SKIPPED`: not saveable over itself).
    Prepared(String, crate::transform::Turn, Result<(Arc<Vec<u8>>, bool), source::Error>),
    /// An image saved over itself after a turn ("Save"): its path and the outcome.
    Replaced(String, Result<(), source::Error>),
    /// Thumbnails gone from the disk cache (`thumbcache::forget`): their paths.
    Forgotten(Vec<String>),
    /// Metadata of an image: `None` = no EXIF.
    Meta { generation: u64, index: usize, meta: Result<Option<crate::meta::Meta>, String> },
}

#[derive(Default)]
struct ThumbQueue {
    generation: u64,
    items: VecDeque<(usize, File)>,
    /// Source closed: the workers exit and release the source. They used to stay alive forever,
    /// and a phone stayed open: reopening it after switching elsewhere gave "device in use".
    closed: bool,
}

pub struct Loader {
    rt: Handle,
    op: Store,
    base: String,
    tx: Sender<Msg>,
    ctx: Context,
    /// Changes at every folder change: requests of the old one still queued are discarded.
    generation: Arc<AtomicU64>,
    full: Arc<Semaphore>,
    queue: Arc<Mutex<ThumbQueue>>,
    wake: Arc<Notify>,
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.queue.lock().unwrap().closed = true;
        self.wake.notify_waiters();
    }
}

impl Loader {
    pub fn new(rt: Handle, op: Store, base: String, tx: Sender<Msg>, ctx: Context) -> Self {
        let loader = Self {
            rt,
            op,
            base: base.clone(),
            tx,
            ctx,
            generation: Arc::default(),
            full: Arc::new(Semaphore::new(2)),
            queue: Arc::default(),
            wake: Arc::default(),
        };
        let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
        // A phone answers one request at a time: more parallel reads are not faster, and the
        // listing of the next folder would wait for all those already started (12 photos from
        // Pictures/To sort: listing of Funny in 2.3 s instead of 0.5).
        let workers = if matches!(loader.op, Store::Mtp(_)) { 2 } else { cores };
        for _ in 0..workers {
            loader.rt.spawn(thumb_worker(
                loader.op.clone(),
                base.clone(),
                loader.queue.clone(),
                loader.wake.clone(),
                loader.tx.clone(),
                loader.ctx.clone(),
            ));
        }
        loader
    }

    pub fn list(&self, dir: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        let task = {
            let dir = dir.clone();
            async move { op.list(&dir).await }
        };
        let rt = self.rt.clone();
        self.rt.spawn(async move {
            // Separate task: a panic inside OpenDAL becomes an error shown, not an endless spinner.
            let r = rt.spawn(task).await.unwrap_or_else(|e| Err(("error-detail", e.to_string())));
            let _ = tx.send(Msg::Listed(dir, r));
            ctx.request_repaint();
        });
    }

    pub fn next_generation(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.set_thumb_queue(generation, Vec::new());
        generation
    }

    /// Replaces the thumbnail queue: the order is the priority.
    pub fn set_thumb_queue(&self, generation: u64, items: Vec<(usize, File)>) {
        *self.queue.lock().unwrap() = ThumbQueue { generation, items: items.into(), closed: false };
        self.wake.notify_waiters();
    }

    /// Maintenance of this source's thumbnail cache, in the background; then the space limit.
    pub fn maintain(&self, limit: u64) {
        let (op, base) = (self.op.clone(), self.base.clone());
        self.rt.spawn(async move {
            maintain(&op, &base).await;
            thumbcache::trim(limit);
        });
    }

    pub fn store(&self) -> Store {
        self.op.clone()
    }

    pub fn paste_check(&self, clip: transfer::Clip, dir: String) {
        let (op, base, tx, ctx) = (self.op.clone(), self.base.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let _ = tx.send(Msg::PasteCheck(transfer::conflicts(&clip, &op, &base, &dir).await));
            ctx.request_repaint();
        });
    }

    pub fn paste(&self, clip: transfer::Clip, dir: String, choices: HashMap<String, transfer::Choice>, progress: Arc<transfer::Progress>) {
        let (op, base, tx, ctx) = (self.op.clone(), self.base.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let (gone, r) = transfer::run(&clip, &op, &base, &dir, &choices, &progress).await;
            for path in &gone {
                thumbcache::delete(&clip.base, path);
            }
            let _ = tx.send(Msg::Pasted(gone, r));
            ctx.request_repaint();
        });
    }

    pub fn deletion(&self, path: &str, local_permanent: bool, remote: crate::settings::RemoteDelete) -> source::Deletion {
        self.op.deletion(path, local_permanent, remote)
    }

    /// Deletes in the background, one item after the other (on a phone they share the trash folder
    /// being created), stopping at the first error; the thumbnails go too.
    pub fn delete(&self, paths: Vec<String>, how: source::Deletion) {
        let (op, base, tx, ctx) = (self.op.clone(), self.base.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            for path in paths {
                let r = op.delete(&path, how).await;
                let failed = r.is_err();
                if !failed {
                    thumbcache::delete(&base, &path);
                }
                let trash = source::parent(&path);
                let _ = tx.send(Msg::Deleted(path, how, r));
                if !failed && source::is_trash(&trash) {
                    drop_empty_trash(&op, trash, &tx).await;
                }
                ctx.request_repaint();
                if failed {
                    break;
                }
            }
        });
    }

    pub fn restore(&self, path: String, conflict: source::Conflict) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = op.restore(&path, conflict).await;
            let ok = r.is_ok();
            let trash = source::parent(&path);
            let _ = tx.send(Msg::Restored(path, r));
            if ok {
                drop_empty_trash(&op, trash, &tx).await;
            }
            ctx.request_repaint();
        });
    }

    pub fn trash_items(&self, trash: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            match op.children(&trash).await {
                Ok(paths) => {
                    let _ = tx.send(Msg::TrashItems(paths));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Restored(trash, Err(e)));
                }
            }
            ctx.request_repaint();
        });
    }

    pub fn rename(&self, path: String, new_name: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = op.rename(&path, &new_name).await;
            let _ = tx.send(Msg::Renamed(path, r));
            ctx.request_repaint();
        });
    }

    /// Original for "Save in another format": decoded in full, with a preview no larger than a texture.
    pub fn load_original(&self, path: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = match op.read(&path).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || {
                    let original = crate::convert::Original::open(&bytes, &path)?;
                    let preview = decode::preview(&original.image);
                    Ok((Arc::new(original), preview, bytes.len() as u64))
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string())),
                Err((_, detail)) => Err(detail),
            };
            let _ = tx.send(Msg::Original(r));
            ctx.request_repaint();
        });
    }

    /// A file written whole (an image saved in another format).
    pub fn save(&self, path: String, data: Arc<Vec<u8>>) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = write_whole(&op, &path, data.to_vec()).await.map(|()| path);
            let _ = tx.send(Msg::Saved(r));
            ctx.request_repaint();
        });
    }

    /// Can "Save" overwrite images here? It needs renaming (not on FTP).
    pub fn can_rename(&self) -> bool {
        self.op.can_rename()
    }

    /// "Save" from the viewer, first half: the image turned in memory. The confirmation shows
    /// whether it lost quality, then `replace_with` writes exactly that.
    pub fn prepare(&self, path: String, turn: crate::transform::Turn) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = turned(&op, &path, turn).await.map(|(data, lossy)| (Arc::new(data), lossy));
            let _ = tx.send(Msg::Prepared(path, turn, r));
            ctx.request_repaint();
        });
    }

    pub fn replace_with(&self, path: String, data: Arc<Vec<u8>>, how: source::Deletion) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = replace(&op, &path, data.to_vec(), how).await;
            let _ = tx.send(Msg::Replaced(path, r));
            ctx.request_repaint();
        });
    }

    /// "Save" on images of the grid: each turned in its own format and written over itself, one
    /// after the other; those that cannot be (`SKIPPED`) are left as they are.
    pub fn replace(&self, paths: Vec<String>, turn: crate::transform::Turn, how: source::Deletion) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            for path in paths {
                let r = match turned(&op, &path, turn).await {
                    Ok((data, _)) => replace(&op, &path, data, how).await,
                    Err(e) => Err(e),
                };
                let _ = tx.send(Msg::Replaced(path, r));
                ctx.request_repaint();
            }
        });
    }

    /// EXIF metadata for the panel. Reads the file again (the thumbnail may come from the cache,
    /// without having read it); on the phone it goes through the same queue as the viewer images.
    pub fn load_meta(&self, generation: u64, index: usize, path: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        let current = self.generation.clone();
        let permits = self.full.clone();
        self.rt.spawn(async move {
            let _permit = permits.acquire_owned().await;
            if current.load(Ordering::Relaxed) != generation {
                return;
            }
            let meta = match op.read(&path).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || crate::meta::read(&bytes)).await.map_err(|e| e.to_string()),
                Err((_, detail)) => Err(detail),
            };
            let _ = tx.send(Msg::Meta { generation, index, meta });
            ctx.request_repaint();
        });
    }

    /// Full-resolution image for the viewer.
    pub fn load_full(&self, generation: u64, index: usize, path: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        let current = self.generation.clone();
        let permits = self.full.clone();
        self.rt.spawn(async move {
            let _permit = permits.acquire_owned().await;
            if current.load(Ordering::Relaxed) != generation {
                return;
            }
            let image = match op.read(&path).await {
                Ok(bytes) => {
                    tokio::task::spawn_blocking(move || decode::decode(&bytes, &path))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                }
                Err((_, detail)) => Err(detail),
            };
            let _ = tx.send(Msg::Loaded { generation, index, thumb: false, image });
            ctx.request_repaint();
        });
    }
}

async fn write_whole(op: &Store, path: &str, data: Vec<u8>) -> Result<(), source::Error> {
    let (chunks, rx) = tokio::sync::mpsc::channel(1);
    let size = data.len() as u64;
    let _ = chunks.send(Ok(bytes::Bytes::from(data))).await;
    drop(chunks);
    op.write_chunks(path, size, rx).await
}

/// The image read and turned in its own format, and whether it lost quality.
async fn turned(op: &Store, path: &str, turn: crate::transform::Turn) -> Result<(Vec<u8>, bool), source::Error> {
    let bytes = op.read(path).await?;
    let name = source::name(path).to_string();
    tokio::task::spawn_blocking(move || {
        if !crate::convert::can_turn_in_place(&bytes, &name) {
            return Err((SKIPPED, name));
        }
        crate::convert::turned_file(&bytes, &name, turn).map_err(|e| ("error-detail", e))
    })
    .await
    .map_err(|e| ("error-detail", e.to_string()))?
}

/// Writes `data` over the image `path`; the original goes as `how` says, under its own name.
/// Nothing is written where renaming is not possible. At every failure, and if the app closes
/// half way (`Store::recover`), the image keeps a visible file under its name: the original, put
/// back from its hidden copy if it had already left.
async fn replace(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), source::Error> {
    if !op.can_rename() {
        return Err(("error-rename-unsupported", path.to_string()));
    }
    source::SAVING.lock().unwrap().insert(path.to_string());
    let r = swap(op, path, data, how).await;
    source::SAVING.lock().unwrap().remove(path);
    r
}

async fn swap(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), source::Error> {
    let own = source::name(path);
    let (new, old) = source::save_names(path);
    // An original left by an earlier save cut short after its new version took the name: it goes
    // where that save would have sent it.
    if op.names(&source::parent(path)).await?.contains(source::name(&old)) {
        put_away(op, &old, own, how).await?;
    }
    write_whole(op, &new, data).await?;
    // The original leaves its name and waits as `old`. To the system trash it goes now with its
    // own name (restoring it from there gives it back), `old` being a second name of it.
    let left = match how {
        source::Deletion::SystemTrash => match op.link_or_copy(path, &old).await {
            Ok(()) => op.delete(path, how).await,
            Err(e) => Err(e),
        },
        _ => op.rename(path, source::name(&old)).await.map(|_| ()),
    };
    let named = match left {
        Ok(()) => op.rename(&new, own).await.map(|_| ()),
        Err(e) => Err(e),
    };
    if let Err(e) = named {
        undo(op, path, &new, &old).await;
        return Err(e);
    }
    // The new image is in place: if the original cannot go, it stays hidden until the next save.
    if let Err(e) = put_away(op, &old, own, how).await {
        log!("save {path:?}: original kept as {old:?}: {e:?}");
    }
    Ok(())
}

/// After a failed step of `swap`: the original back under its name if that is free (never over
/// another file), the new file gone.
async fn undo(op: &Store, path: &str, new: &str, old: &str) {
    let names = op.names(&source::parent(path)).await.unwrap_or_default();
    if names.contains(source::name(old)) && !names.contains(source::name(path)) {
        let r = op.rename(old, source::name(path)).await;
        log!("save {path:?}: original back: {r:?}");
    }
    if names.contains(source::name(new)) {
        let _ = op.delete(new, source::Deletion::Permanent).await;
    }
}

/// The original, waiting as `old`, goes as `how` says, under its own name `own`. With the system
/// trash it is already there and `old` is only a second name.
async fn put_away(op: &Store, old: &str, own: &str, how: source::Deletion) -> Result<(), source::Error> {
    match how {
        source::Deletion::TrashFolder => op.to_trash_folder(old, own).await,
        _ => op.delete(old, source::Deletion::Permanent).await,
    }
}

async fn thumb_worker(
    op: Store,
    base: String,
    queue: Arc<Mutex<ThumbQueue>>,
    wake: Arc<Notify>,
    tx: Sender<Msg>,
    ctx: Context,
) {
    loop {
        // Register before looking at the queue: a new queue arriving in the meantime must not be lost.
        let notified = wake.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let next = {
            let mut q = queue.lock().unwrap();
            if q.closed {
                return;
            }
            let generation = q.generation;
            q.items.pop_front().map(|item| (generation, item))
        };
        let Some((generation, (index, file))) = next else {
            notified.await;
            continue;
        };
        let image = thumbnail(&op, &base, file).await;
        if tx.send(Msg::Loaded { generation, index, thumb: true, image }).is_err() {
            return; // window closed
        }
        ctx.request_repaint();
    }
}

/// From the disk cache if present, otherwise generated and saved.
async fn thumbnail(op: &Store, base: &str, file: File) -> Result<ColorImage, String> {
    let cached = {
        let (base, file) = (base.to_string(), file.clone());
        tokio::task::spawn_blocking(move || thumbcache::load(&base, &file)).await.ok().flatten()
    };
    if let Some(img) = cached {
        return Ok(decode::to_color_image(img));
    }
    let bytes = op.read(&file.path).await.map_err(|(_, detail)| detail)?;
    let base = base.to_string();
    tokio::task::spawn_blocking(move || {
        let img = decode::thumbnail(&bytes, &file.path, THUMB_SIDE)?;
        thumbcache::save(&base, &file, &img);
        Ok(decode::to_color_image(img))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

/// Compares the saved thumbnails with the current folders, read again with the same listing that
/// created them (so size and date have the same format). File gone: thumbnail deleted. File
/// changed: thumbnail redone. When in doubt nothing is deleted: a folder that no longer exists
/// could be an unmounted disk, a network error means the source is unreachable.
/// Known limit: an empty mount point (disk unplugged) exists, so thumbnails saved directly
/// inside it would be deleted; those in subfolders would not.
// ponytail: one folder at a time; in parallel if it turns out too long on slow FTP
async fn maintain(op: &Store, base: &str) {
    let saved = {
        let base = base.to_string();
        tokio::task::spawn_blocking(move || thumbcache::entries(&base)).await.unwrap_or_default()
    };
    let mut by_dir: BTreeMap<String, Vec<File>> = BTreeMap::new();
    for f in saved {
        by_dir.entry(source::parent(&f.path)).or_default().push(f);
    }
    for (dir, saved) in by_dir {
        let listing = match op.check_dir(&dir).await {
            Ok(()) => op.list(&dir).await,
            Err(e) => Err(e),
        };
        let listing = match listing {
            Ok(l) => l,
            Err((source::NOT_FOUND, _)) => continue,
            Err(_) => return,
        };
        let current: HashMap<&str, &File> = listing.files.iter().map(|f| (f.path.as_str(), f)).collect();
        for old in saved {
            match current.get(old.path.as_str()) {
                None => thumbcache::delete(base, &old.path),
                Some(f) if f.size != old.size || f.modified != old.modified => {
                    let _ = thumbnail(op, base, (*f).clone()).await;
                }
                Some(_) => {}
            }
        }
    }
}

/// A trash folder left empty goes, so its strip does not say there is something to restore.
async fn drop_empty_trash(op: &Store, trash: String, tx: &Sender<Msg>) {
    if op.names(&trash).await.is_ok_and(|n| n.is_empty()) {
        let how = source::Deletion::Permanent;
        let r = op.delete(&trash, how).await;
        let _ = tx.send(Msg::Deleted(trash, how, r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Deletion, TRASH_DIR};

    /// Saving over an image: the new file takes the name, the original goes as asked under its own
    /// name ("a (2).png" when the trash already has one), nothing hidden stays. A failed step puts
    /// the original back; a source that cannot rename gets nothing written.
    #[test]
    fn replace_steps() {
        let dir = std::env::temp_dir().join(format!("eyecansee-replace-{}", std::process::id()));
        let (p, trash) = (dir.join("p"), dir.join("p").join(TRASH_DIR));
        std::fs::create_dir_all(&trash).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let hidden = || {
            std::fs::read_dir(&p).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains(".eyecansee-")).collect::<Vec<_>>()
        };
        std::fs::write(p.join("a.png"), b"old").unwrap();
        std::fs::write(trash.join("a.png"), b"older").unwrap();
        // An original left next to its new version by a save cut short goes first.
        std::fs::write(p.join(".a.png.eyecansee-old"), b"stale").unwrap();
        rt.block_on(replace(&op, "p/a.png", b"new".to_vec(), Deletion::TrashFolder)).unwrap();
        assert_eq!(std::fs::read(p.join("a.png")).unwrap(), b"new");
        assert_eq!(std::fs::read(trash.join("a (2).png")).unwrap(), b"stale");
        assert_eq!(std::fs::read(trash.join("a (3).png")).unwrap(), b"old");
        assert!(hidden().is_empty());
        rt.block_on(replace(&op, "p/a.png", b"newer".to_vec(), Deletion::Permanent)).unwrap();
        assert_eq!(std::fs::read(p.join("a.png")).unwrap(), b"newer");
        assert_eq!(std::fs::read_dir(&trash).unwrap().count(), 3);
        assert!(hidden().is_empty());

        // A step failed half way: the original back under its name, the new file gone.
        let (new, old) = source::save_names("p/a.png");
        std::fs::rename(p.join("a.png"), p.join(".a.png.eyecansee-old")).unwrap();
        std::fs::write(p.join(".a.png.eyecansee-new"), b"half").unwrap();
        rt.block_on(undo(&op, "p/a.png", &new, &old));
        assert_eq!(std::fs::read(p.join("a.png")).unwrap(), b"newer");
        assert!(hidden().is_empty());

        // The second name of the system trash flow: a link (or copy), never over a file.
        rt.block_on(op.link_or_copy("p/a.png", "p/b.png")).unwrap();
        assert_eq!(std::fs::read(p.join("b.png")).unwrap(), b"newer");
        assert!(rt.block_on(op.link_or_copy("p/a.png", "p/b.png")).is_err());

        // Nothing can be written in the folder: the original stays as it was.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o555)).unwrap();
            // Not as root, who writes anyway.
            if std::fs::write(p.join("probe"), b"").is_err() {
                assert!(rt.block_on(replace(&op, "p/a.png", b"x".to_vec(), Deletion::Permanent)).is_err());
                assert_eq!(std::fs::read(p.join("a.png")).unwrap(), b"newer");
                assert!(hidden().is_empty());
            }
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // FTP cannot rename: refused before anything is read or written (nothing listens there).
        let ftp = Store::Dal(opendal::Operator::new(opendal::services::Ftp::default().endpoint("ftp://127.0.0.1:9")).unwrap());
        assert_eq!(rt.block_on(replace(&ftp, "a.png", b"x".to_vec(), Deletion::Permanent)).unwrap_err().0, "error-rename-unsupported");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
