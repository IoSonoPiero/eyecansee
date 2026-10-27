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
use crate::video_read::{preview, slow_video, video_input, video_meta, video_still, SLOW};

/// Error id of an image left out of "Save": animated, several pages, a Motion Photo (`encode::can_turn_in_place`).
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
    /// New folder made (its path).
    MadeFolder(String, Result<(), source::Error>),
    /// Names already in the paste destination (to ask about), or an error.
    PasteCheck(Result<Vec<String>, source::Error>),
    /// End of a copy or move: source items that went away, outcome.
    Pasted(Vec<String>, Result<(), source::Error>),
    /// Outcome of `update::check`: a newer version downloaded and checked, up to date, or an error.
    Update(Result<Option<crate::update::Ready>, String>),
    /// "Save in another format": the original decoded, its preview and file size.
    Original(Result<(Arc<crate::encode::Original>, ColorImage, u64), String>),
    /// The image encoded with the options of `generation`, and the encoded file decoded back
    /// (`None`: no decoder for that format in this build).
    Encoded(u64, Result<(Arc<Vec<u8>>, Option<ColorImage>), String>),
    /// The converted image written: its path, or an error.
    Saved(Result<String, source::Error>),
    /// "Save" from the viewer, before the confirmation: the image, the turn, and the file to write
    /// with whether it lost quality (encoded again), or an error (`SKIPPED`: not saveable over itself).
    Prepared(String, crate::transform::Turn, Result<(Arc<Vec<u8>>, bool), source::Error>),
    /// "Rotate and crop", before the confirmation: the image and the file to write, as `Prepared`.
    /// Also the result as the viewer shows it, so it is there at once once saved.
    Edited(String, Result<(Arc<Vec<u8>>, bool, ColorImage), source::Error>),
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
    /// Items taken from `items` so far: the position of each one in the queue.
    taken: usize,
    /// Videos of a slow disk not in the cache, by position: made two at a time (`SLOW`), always
    /// the first one, so they come in the order of the queue whatever order the cache checks end in.
    slow: BTreeMap<usize, (usize, File)>,
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
        *self.queue.lock().unwrap() = ThumbQueue { generation, items: items.into(), taken: 0, slow: BTreeMap::new(), closed: false };
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

    /// What the viewer plays (see `video_input`).
    pub fn video_input(&self, path: &str, size: u64) -> crate::mpv::Input {
        video_input(self.rt.clone(), &self.op, path, size)
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
            'all: for path in paths {
                // Emptying a trash: the folder (if it is there at all), then Android's trashed
                // files of its folder. The view leaves the trash at the folder's message.
                let (missing, items) = if source::is_trash(&path) {
                    let (exists, android) = trash_contents(&op, &path).await;
                    (!exists, std::iter::once(path.clone()).chain(android).collect())
                } else {
                    (false, vec![path])
                };
                for (i, path) in items.into_iter().enumerate() {
                    let r = if i == 0 && missing { Ok(()) } else { op.delete(&path, how).await };
                    let failed = r.is_err();
                    if !failed {
                        thumbcache::delete(&base, &path);
                    }
                    let trash = source::trash_of(&path);
                    let _ = tx.send(Msg::Deleted(path, how, r));
                    if !failed && source::is_trash(&trash) {
                        drop_empty_trash(&op, trash, &tx).await;
                    }
                    ctx.request_repaint();
                    if failed {
                        break 'all;
                    }
                }
            }
        });
    }

    pub fn restore(&self, path: String, conflict: source::Conflict) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = op.restore(&path, conflict).await;
            let ok = r.is_ok();
            let trash = source::trash_of(&path);
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

    pub fn make_folder(&self, path: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = op.create_dir(&path).await;
            let _ = tx.send(Msg::MadeFolder(path, r));
            ctx.request_repaint();
        });
    }

    /// Original for "Save in another format": decoded in full, with a preview no larger than a texture.
    pub fn load_original(&self, path: String) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = match op.read(&path).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || {
                    let original = crate::encode::Original::open(&bytes, &path)?;
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

    /// "Rotate and crop", before the confirmation (`Msg::Edited`): the image of the page, already
    /// decoded and with the applied edits, gets the one not applied yet and is encoded in the file's
    /// format. The file is read only for its format and metadata, not decoded again (it was slow
    /// with an 83 MB PSD).
    /// `applied`: the edits the image already has, for an Ultra HDR photo's gain map.
    pub fn prepare_edit(&self, path: String, image: Arc<crate::encode::Original>, applied: Vec<crate::rotate::Edit>, pending: Option<crate::rotate::Edit>) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = rewritten(&op, &path, move |bytes, name| {
                let mut img = image.image.clone();
                if let Some(e) = pending {
                    e.apply(&mut img);
                }
                let shown = decode::preview(&img);
                if crate::ultrahdr::is(bytes) {
                    let edits: Vec<_> = applied.into_iter().chain(pending).collect();
                    return crate::ultrahdr::edited(bytes, img, &edits).map(|data| (data, true, shown));
                }
                crate::encode::encoded_back(bytes, name, img).map(|(data, lossy)| (data, lossy, shown))
            })
            .await;
            let _ = tx.send(Msg::Edited(path, r.map(|(data, lossy, shown)| (Arc::new(data), lossy, shown))));
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

    /// Metadata for the panel: saved in the cache with the thumbnails, otherwise read from the file
    /// (the thumbnail may come from the cache, without having read it); on the phone it goes
    /// through the same queue as the viewer images.
    pub fn load_meta(&self, generation: u64, index: usize, file: File) {
        let (op, tx, ctx, base) = (self.op.clone(), self.tx.clone(), self.ctx.clone(), self.base.clone());
        let current = self.generation.clone();
        let permits = self.full.clone();
        self.rt.spawn(async move {
            // From the cache first, without waiting for a place: what was read once comes at once.
            let saved = {
                let (base, file) = (base.clone(), file.clone());
                tokio::task::spawn_blocking(move || thumbcache::load_meta(&base, &file)).await.ok().flatten()
            };
            if let Some(meta) = saved.as_deref().and_then(crate::meta::Meta::from_json) {
                let _ = tx.send(Msg::Meta { generation, index, meta: Ok(Some(meta)) });
                ctx.request_repaint();
                return;
            }
            let _permit = permits.acquire_owned().await;
            if current.load(Ordering::Relaxed) != generation {
                return;
            }
            let path = file.path.clone();
            // No EXIF in videos, and they are not read whole.
            let meta = if decode::is_video(&path) {
                video_meta(&op, &path).await
            } else {
                match op.read(&path).await {
                    Ok(bytes) => tokio::task::spawn_blocking(move || crate::meta::read_file(&bytes, &path)).await.map_err(|e| e.to_string()),
                    Err((_, detail)) => Err(detail),
                }
            };
            if let Ok(Some(m)) = &meta {
                thumbcache::save_meta(&base, &file, m.to_json());
            }
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
            // A video is not read whole: libmpv opens it.
            let image = if decode::is_video(&path) {
                let _place = if slow_video(&op, &path) { SLOW.acquire().await.ok() } else { None };
                match op.size(&path).await {
                    Ok(size) => video_still(&op, &path, size, 0).await.map(decode::to_color_image),
                    Err((_, detail)) => Err(detail),
                }
            } else {
                match op.read(&path).await {
                    Ok(bytes) => tokio::task::spawn_blocking(move || decode::decode(&bytes, &path))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string())),
                    Err((_, detail)) => Err(detail),
                }
            };
            let _ = tx.send(Msg::Loaded { generation, index, thumb: false, image });
            ctx.request_repaint();
        });
    }
}

pub(crate) async fn write_whole(op: &Store, path: &str, data: Vec<u8>) -> Result<(), source::Error> {
    let (chunks, rx) = tokio::sync::mpsc::channel(1);
    let size = data.len() as u64;
    let _ = chunks.send(Ok(bytes::Bytes::from(data))).await;
    drop(chunks);
    op.write_chunks(path, size, rx).await
}

/// The image read and turned in its own format, and whether it lost quality.
async fn turned(op: &Store, path: &str, turn: crate::transform::Turn) -> Result<(Vec<u8>, bool), source::Error> {
    rewritten(op, path, move |bytes, name| crate::encode::turned_file(bytes, name, turn)).await
}

/// The file read and passed to `make` (in the background), which gives the file to write and
/// whether it lost quality; `SKIPPED` if it cannot be written back over itself. An Ultra HDR
/// photo passes when `ultra_hdr`: only flips and quarter turns know how to keep its gain map.
async fn rewritten<T: Send + 'static>(op: &Store, path: &str, make: impl FnOnce(&[u8], &str) -> Result<T, String> + Send + 'static) -> Result<T, source::Error> {
    let bytes = op.read(path).await?;
    let name = source::name(path).to_string();
    tokio::task::spawn_blocking(move || {
        if !crate::encode::can_turn_in_place(&bytes, &name) && !crate::ultrahdr::is(&bytes) {
            return Err((SKIPPED, name));
        }
        make(&bytes, &name).map_err(|e| ("error-detail", e))
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
    let send = |generation, index, image| {
        ctx.request_repaint();
        tx.send(Msg::Loaded { generation, index, thumb: true, image }).is_ok()
    };
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
            let (generation, at) = (q.generation, q.taken);
            let item = q.items.pop_front();
            q.taken += usize::from(item.is_some());
            item.map(|item| (generation, at, item))
        };
        let Some((generation, at, (index, file))) = next else {
            notified.await;
            continue;
        };
        // From the cache at once: a thumbnail already made never waits for a SLOW place.
        // Then a preview inside the video: it never needs a SLOW place either.
        let found = match cached(&base, &file).await {
            None if decode::is_video(&file.path) => preview(&op, &base, &file).await,
            found => found,
        };
        let sent = match found {
            Some(img) => send(generation, index, Ok(img)),
            None if slow_video(&op, &file.path) => {
                {
                    let mut q = queue.lock().unwrap();
                    if q.generation == generation {
                        q.slow.insert(at, (index, file));
                    }
                }
                make_slow(&op, &base, &queue, &send).await
            }
            None => send(generation, index, generate(&op, &base, file).await),
        };
        if !sent {
            return; // window closed
        }
    }
}

/// While a SLOW place is free, the slow videos waiting, first position first. Whoever adds one
/// tries afterwards; whoever finds none gives the place back and looks again, so none is left
/// waiting. False if the window is closed.
async fn make_slow(op: &Store, base: &str, queue: &Mutex<ThumbQueue>, send: &impl Fn(u64, usize, Result<ColorImage, String>) -> bool) -> bool {
    loop {
        let Ok(place) = SLOW.try_acquire() else { return true };
        let next = {
            let mut q = queue.lock().unwrap();
            let generation = q.generation;
            q.slow.pop_first().map(|(_, item)| (generation, item))
        };
        let Some((generation, (index, file))) = next else {
            drop(place);
            if queue.lock().unwrap().slow.is_empty() {
                return true;
            }
            continue;
        };
        if !send(generation, index, generate(op, base, file).await) {
            return false;
        }
    }
}

/// From the disk cache if present, otherwise generated and saved.
pub(crate) async fn thumbnail(op: &Store, base: &str, file: File) -> Result<ColorImage, String> {
    match cached(base, &file).await {
        Some(img) => Ok(img),
        None => generate(op, base, file).await,
    }
}

async fn cached(base: &str, file: &File) -> Option<ColorImage> {
    let (base, file) = (base.to_string(), file.clone());
    tokio::task::spawn_blocking(move || thumbcache::load(&base, &file)).await.ok().flatten().map(decode::to_color_image)
}

/// Made from the file and saved in the cache.
async fn generate(op: &Store, base: &str, file: File) -> Result<ColorImage, String> {
    if decode::is_video(&file.path) {
        if let Some(img) = preview(op, base, &file).await {
            return Ok(img);
        }
        let img = video_still(op, &file.path, file.size, THUMB_SIDE).await?;
        let base = base.to_string();
        return tokio::task::spawn_blocking(move || {
            thumbcache::save(&base, &file, &img);
            decode::to_color_image(img)
        })
        .await
        .map_err(|e| e.to_string());
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
/// A trash folder left with nothing in it (Android's trashed files of its folder count) goes.
async fn drop_empty_trash(op: &Store, trash: String, tx: &Sender<Msg>) {
    if op.names(&trash).await.is_ok_and(|n| n.is_empty()) && android_trashed_files(op, &trash).await.is_empty() {
        let how = source::Deletion::Permanent;
        let r = op.delete(&trash, how).await;
        let _ = tx.send(Msg::Deleted(trash, how, r));
    }
}

/// Whether the trash folder exists, and Android's trashed files of its folder.
async fn trash_contents(op: &Store, trash: &str) -> (bool, Vec<String>) {
    (op.is_dir(trash).await, android_trashed_files(op, trash).await)
}

async fn android_trashed_files(op: &Store, trash: &str) -> Vec<String> {
    op.list(&source::parent(trash)).await.map(|l| l.trashed.into_iter().map(|f| f.path).collect()).unwrap_or_default()
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
        rt.block_on(drop_empty_trash(&op, trash.clone(), &tx));
        assert!(p.join(TRASH_DIR).is_dir() && rx.try_recv().is_err(), "kept while Android's file is there");
        std::fs::remove_file(p.join(".trashed-1-a.jpg")).unwrap();
        rt.block_on(drop_empty_trash(&op, trash.clone(), &tx));
        assert!(!p.join(TRASH_DIR).exists() && matches!(rx.try_recv(), Ok(Msg::Deleted(t, _, Ok(()))) if t == trash));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // Tests by hand of "Save" with the real `replace`, as the app does it (`tools/test-save.sh`
    // runs the local ones). Each works in a new folder, left there to look at the files: under
    // `EYECANSEE_SAVE_OUT` if set, otherwise the system's temporary folder.
    //   cargo test --release save_live_ -- --ignored --nocapture
    //   EYECANSEE_SAVE_DIR=/run/media/<user>/<stick> cargo test --release save_live_dir -- --ignored --nocapture
    //   EYECANSEE_MTP=<serial> cargo test --release mtp_save_live -- --ignored --nocapture

    use crate::transform::{Op, Turn};
    use std::path::{Path, PathBuf};

    /// A new folder for one test, and a store with its root there.
    fn live_dir(parent: Option<PathBuf>, name: &str) -> (PathBuf, Store) {
        let parent = parent.or_else(|| std::env::var_os("EYECANSEE_SAVE_OUT").map(PathBuf::from)).unwrap_or_else(std::env::temp_dir);
        let dir = parent.join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        println!("{name}: {}", dir.display());
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        (dir, op)
    }

    /// Saving as the grid does: read, turned (`SKIPPED` if not possible), written over itself.
    fn save(rt: &tokio::runtime::Runtime, op: &Store, path: &str, turn: Turn, how: Deletion) -> Result<(), source::Error> {
        rt.block_on(async {
            let (data, _) = turned(op, path, turn).await?;
            replace(op, path, data, how).await
        })
    }

    fn hidden_in(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains(".eyecansee-")).collect()
    }

    /// A photo-like picture (gradients and noise), so that a lossy step would show.
    fn picture(w: u32, h: u32) -> DynamicImage {
        let mut seed = 12345u32;
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let n = (seed >> 24) as u8 / 8;
            image::Rgb([(x * 255 / w) as u8 ^ n, (y * 255 / h) as u8, ((x + y) % 256) as u8])
        }))
    }

    use image::DynamicImage;

    fn encode(img: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut v = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut v), format).unwrap();
        v
    }

    /// A JPEG with EXIF: orientation 6 (to be turned clockwise), camera model, date taken, GPS.
    fn jpeg_with_exif(img: &DynamicImage) -> Vec<u8> {
        use exif::{Field, In, Rational, Tag, Value};
        let field = |tag, value| Field { tag, ifd_num: In::PRIMARY, value };
        let fields = [
            field(Tag::Orientation, Value::Short(vec![6])),
            field(Tag::Model, Value::Ascii(vec![b"Test Camera".to_vec()])),
            field(Tag::DateTimeOriginal, Value::Ascii(vec![b"2026:10:24 10:00:00".to_vec()])),
            field(Tag::GPSLatitudeRef, Value::Ascii(vec![b"N".to_vec()])),
            field(Tag::GPSLatitude, Value::Rational(vec![(45, 1).into(), (30, 1).into(), Rational::from((0, 1))])),
        ];
        let mut writer = exif::experimental::Writer::new();
        for f in &fields {
            writer.push_field(f);
        }
        let mut tiff = std::io::Cursor::new(Vec::new());
        writer.write(&mut tiff, false).unwrap();
        let mut jpeg = encode(img, image::ImageFormat::Jpeg);
        let app1 = [&[0xFF, 0xE1][..], &((2 + 6 + tiff.get_ref().len()) as u16).to_be_bytes(), b"Exif\0\0", tiff.get_ref()].concat();
        jpeg.splice(2..2, app1);
        jpeg
    }

    /// The test files: JPEG 1024×768 (turned without loss) and 1001×751 (encoded again), PNG,
    /// animated WebP, APNG, two-page TIFF, JPEG with EXIF 6 and GPS, and a JPEG followed by a
    /// video as a Motion Photo is.
    fn fixtures(dir: &Path) {
        let small = picture(8, 8);
        let png = encode(&small, image::ImageFormat::Png);
        let jpeg = encode(&picture(1024, 768), image::ImageFormat::Jpeg);
        let files: [(&str, Vec<u8>); 8] = [
            ("even.jpg", jpeg.clone()),
            ("odd.jpg", encode(&picture(1001, 751), image::ImageFormat::Jpeg)),
            ("plain.png", encode(&picture(300, 200), image::ImageFormat::Png)),
            ("animated.webp", crate::encode::tests::animated_webp(&small)),
            ("animated.png", crate::encode::tests::apng(&png)),
            ("pages.tif", crate::encode::tests::tiff_pages(&small)),
            ("exif6.jpg", jpeg_with_exif(&picture(1024, 768))),
            ("motion.jpg", [jpeg.as_slice(), b"\0\0\0\x18ftypmp42\0\0\0\0isommp42", &[7; 4096]].concat()),
        ];
        for (name, data) in files {
            std::fs::write(dir.join(name), data).unwrap();
        }
    }

    fn check_trash(dir: Option<PathBuf>) {
        let (dir, op) = live_dir(dir, "save-trash");
        fixtures(&dir);
        let original = std::fs::read(dir.join("even.jpg")).unwrap();
        let ours = |dir: &Path| {
            // Windows' Recycle Bin gives "C:\...", canonicalize "\\?\C:\...".
            let canonical = PathBuf::from(dir.canonicalize().unwrap().to_string_lossy().trim_start_matches(r"\\?\").to_string());
            trash::os_limited::list().unwrap().into_iter().filter(|i| i.original_parent == canonical).collect::<Vec<_>>()
        };
        save(&tokio::runtime::Runtime::new().unwrap(), &op, "even.jpg", Turn::default().then(Op::Cw), Deletion::SystemTrash).unwrap();
        assert!(hidden_in(&dir).is_empty(), "hidden files left");
        let items = ours(&dir);
        assert_eq!(items.iter().map(|i| i.name.to_string_lossy().into_owned()).collect::<Vec<_>>(), ["even.jpg"], "in the trash under its own name");
        // Back from the trash, where the saved image makes room for it.
        std::fs::rename(dir.join("even.jpg"), dir.join("even-saved.jpg")).unwrap();
        trash::os_limited::restore_all(items).unwrap();
        assert_eq!(std::fs::read(dir.join("even.jpg")).unwrap(), original, "restored byte for byte");
        let left = ours(&dir);
        if !left.is_empty() {
            trash::os_limited::purge_all(left).unwrap();
        }
    }

    fn check_lossless(dir: Option<PathBuf>) {
        let (dir, op) = live_dir(dir, "save-lossless");
        fixtures(&dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let decoded = |name: &str| crate::decode::decode_full(&std::fs::read(dir.join(name)).unwrap(), name).unwrap().to_rgb8();
        let before = decoded("even.jpg");
        for _ in 0..4 {
            save(&rt, &op, "even.jpg", Turn::default().then(Op::Cw), Deletion::Permanent).unwrap();
        }
        assert!(before == decoded("even.jpg"), "four quarter turns: same pixels");
        // EXIF 6: shown upright and turned once more; then upright with no orientation left,
        // model, date and GPS still there.
        let mut expected = crate::decode::decode_full(&std::fs::read(dir.join("exif6.jpg")).unwrap(), "x.jpg").unwrap();
        let turn = Turn::default().then(Op::Cw);
        turn.apply(&mut expected);
        save(&rt, &op, "exif6.jpg", turn, Deletion::Permanent).unwrap();
        let saved = std::fs::read(dir.join("exif6.jpg")).unwrap();
        let got = crate::decode::decode_full(&saved, "x.jpg").unwrap().to_rgb8();
        assert_eq!(got.dimensions(), expected.to_rgb8().dimensions());
        let diff = got.pixels().zip(expected.to_rgb8().pixels()).map(|(a, b)| (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).max().unwrap()).max().unwrap();
        assert!(diff <= 3, "pixels differ by {diff}");
        let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&saved)).unwrap();
        let get = |tag| exif.get_field(tag, exif::In::PRIMARY).map(|f| f.display_value().to_string());
        assert!(get(exif::Tag::Orientation).is_none_or(|o| o == "row 0 at top and column 0 at left"), "orientation {:?}", get(exif::Tag::Orientation));
        assert!(get(exif::Tag::Model).is_some() && get(exif::Tag::DateTimeOriginal).is_some() && get(exif::Tag::GPSLatitude).is_some());
        // Not a multiple of the blocks: encoded again, still turned.
        save(&rt, &op, "odd.jpg", Turn::default().then(Op::Cw), Deletion::Permanent).unwrap();
        assert_eq!(decoded("odd.jpg").dimensions(), (751, 1001));
        assert!(hidden_in(&dir).is_empty());
    }

    fn check_excluded(dir: Option<PathBuf>) {
        let (dir, op) = live_dir(dir, "save-excluded");
        fixtures(&dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        for name in ["animated.webp", "animated.png", "pages.tif", "motion.jpg"] {
            let before = std::fs::read(dir.join(name)).unwrap();
            let r = save(&rt, &op, name, Turn::default().then(Op::Cw), Deletion::Permanent);
            assert_eq!(r.map_err(|e| e.0), Err(SKIPPED), "{name}");
            assert_eq!(std::fs::read(dir.join(name)).unwrap(), before, "{name} unchanged");
        }
        assert!(hidden_in(&dir).is_empty());
    }

    fn check_recover(dir: Option<PathBuf>) {
        let (dir, op) = live_dir(dir, "save-recover");
        std::fs::write(dir.join(".x.jpg.eyecansee-old"), b"the original").unwrap();
        std::fs::write(dir.join("y.jpg"), b"y").unwrap();
        std::fs::write(dir.join(".y.jpg.eyecansee-new"), b"half written").unwrap();
        let l = tokio::runtime::Runtime::new().unwrap().block_on(op.list("")).unwrap();
        assert_eq!(l.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["x.jpg", "y.jpg"]);
        assert_eq!(std::fs::read(dir.join("x.jpg")).unwrap(), b"the original");
        assert!(hidden_in(&dir).is_empty());
    }

    /// Local disk with the real system trash: the original goes there under its own name and
    /// comes back identical; only the items of this test are taken out of the trash.
    #[test]
    #[ignore]
    fn save_live_trash() {
        check_trash(None);
    }

    /// Four quarter turns of a JPEG give back the same pixels; EXIF 6 ends upright, metadata kept.
    #[test]
    #[ignore]
    fn save_live_lossless() {
        check_lossless(None);
    }

    /// Animated, multi-page and Motion Photo files are not touched.
    #[test]
    #[ignore]
    fn save_live_excluded() {
        check_excluded(None);
    }

    /// Read-only folder: the original stays, no hidden file.
    #[test]
    #[ignore]
    #[cfg(unix)]
    fn save_live_readonly() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, op) = live_dir(None, "save-readonly");
        fixtures(&dir);
        let before = std::fs::read(dir.join("even.jpg")).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let writable = std::fs::write(dir.join("probe"), b"").is_ok();
        let r = save(&tokio::runtime::Runtime::new().unwrap(), &op, "even.jpg", Turn::default().then(Op::Cw), Deletion::Permanent);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!writable, "the folder can be written anyway (root?)");
        assert!(r.is_err());
        assert_eq!(std::fs::read(dir.join("even.jpg")).unwrap(), before);
        assert!(hidden_in(&dir).is_empty());
    }

    /// Hidden files of a save cut short, tidied up when the folder is read.
    #[test]
    #[ignore]
    fn save_live_recover() {
        check_recover(None);
    }

    /// The same on a folder given by `EYECANSEE_SAVE_DIR`, for a stick (exFAT: no hard links, so
    /// the system trash flow copies; its trash is the stick's own).
    #[test]
    #[ignore]
    fn save_live_dir() {
        let dir = PathBuf::from(std::env::var_os("EYECANSEE_SAVE_DIR").expect("EYECANSEE_SAVE_DIR not set"));
        check_trash(Some(dir.clone()));
        check_lossless(Some(dir.clone()));
        check_excluded(Some(dir.clone()));
        check_recover(Some(dir));
    }

    /// On a real phone, not mounted by the desktop and with EyeCanSee closed: a test photo in a
    /// folder of its own, turned twice (the originals in its trash folder as "a.jpg" and
    /// "a (2).jpg"), the first one restored; then everything it made is deleted.
    #[test]
    #[ignore]
    fn mtp_save_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let op = Store::Mtp(Arc::new(crate::mtp::Phone::new(&serial)));
        let root = rt.block_on(op.list("")).unwrap();
        let dir = format!("{}EyeCanSee-save-test/", root.dirs[0]);
        let trash = format!("{dir}{TRASH_DIR}/");
        println!("mtp_save_live: {dir}");
        let original = encode(&picture(1024, 768), image::ImageFormat::Jpeg);
        let r = rt.block_on(async {
            op.create_dir(&dir).await?;
            write_whole(&op, &format!("{dir}a.jpg"), original.clone()).await?;
            Ok::<_, source::Error>(())
        });
        let run = || -> Result<(), String> {
            r.clone().map_err(|e| format!("{e:?}"))?;
            let path = format!("{dir}a.jpg");
            let names = |d: &str| rt.block_on(op.names(d)).unwrap();
            for expected in [vec!["a.jpg"], vec!["a (2).jpg", "a.jpg"]] {
                save(&rt, &op, &path, Turn::default().then(Op::Cw), Deletion::TrashFolder).map_err(|e| format!("save: {e:?}"))?;
                let mut in_trash: Vec<String> = names(&trash).into_iter().collect();
                in_trash.sort();
                if in_trash != expected {
                    return Err(format!("trash {in_trash:?}, expected {expected:?}"));
                }
                if names(&dir).iter().any(|n| n.contains(".eyecansee-")) {
                    return Err(format!("hidden files left in {:?}", names(&dir)));
                }
            }
            let back = rt.block_on(op.restore(&format!("{trash}a.jpg"), source::Conflict::KeepBoth)).map_err(|e| format!("restore: {e:?}"))?;
            if rt.block_on(op.read(&back)).map_err(|e| format!("{e:?}"))? != original {
                return Err(format!("{back} differs from the original"));
            }
            Ok(())
        };
        let outcome = run();
        let cleaned = rt.block_on(op.delete(&dir, Deletion::Permanent));
        outcome.unwrap();
        cleaned.unwrap();
    }
}
