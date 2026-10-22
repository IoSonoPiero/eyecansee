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
use crate::{decode, thumbcache};

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

    pub fn deletion(&self, path: &str, local_permanent: bool, remote: crate::settings::RemoteDelete) -> source::Deletion {
        self.op.deletion(path, local_permanent, remote)
    }

    /// Deletes in the background; the thumbnail goes too.
    pub fn delete(&self, path: String, how: source::Deletion) {
        let (op, base, tx, ctx) = (self.op.clone(), self.base.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = op.delete(&path, how).await;
            if r.is_ok() {
                thumbcache::delete(&base, &path);
            }
            let _ = tx.send(Msg::Deleted(path, how, r));
            ctx.request_repaint();
        });
    }

    pub fn restore(&self, path: String, conflict: source::Conflict) {
        let (op, tx, ctx) = (self.op.clone(), self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = op.restore(&path, conflict).await;
            let _ = tx.send(Msg::Restored(path, r));
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
