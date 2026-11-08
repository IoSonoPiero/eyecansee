//! Thumbnails in the background: the priority queue the UI rewrites, the workers that take from
//! it (from the cache when there, otherwise made from the file: videos of slow disks two at a
//! time, after the pictures), and the upkeep of the cache. Split from `loader.rs` on 2026-11-01.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use eframe::egui::{ColorImage, Context};
use tokio::sync::Notify;

use crate::loader::{Msg, THUMB_SIDE, siblings};
use crate::source::{self, File, Store};
use crate::video_read::{SLOW, preview, slow_video, video_still};
use crate::{decode, thumbcache};

#[derive(Default)]
pub(crate) struct ThumbQueue {
    pub(crate) generation: u64,
    pub(crate) items: VecDeque<(usize, File)>,
    /// Items taken from `items` so far: the position of each one in the queue.
    pub(crate) taken: usize,
    /// Videos of a slow disk not in the cache, by position: made two at a time (`SLOW`), always
    /// the first one, so they come in the order of the queue whatever order the cache checks end in.
    pub(crate) slow: BTreeMap<usize, (usize, File)>,
    /// Source closed: the workers exit and release the source. They used to stay alive forever,
    /// and a phone stayed open: reopening it after switching elsewhere gave "device in use".
    pub(crate) closed: bool,
}

pub(crate) async fn thumb_worker(
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
            // No pictures left: now the slow videos waiting.
            if !make_slow(&op, &base, &queue, &send).await {
                return;
            }
            notified.await;
            continue;
        };
        // From the cache at once: a thumbnail already made never waits for a SLOW place.
        // Then a preview inside the video: it never needs a SLOW place either.
        let found = match cached(&base, &file).await {
            None if decode::is_played(&file.path) => preview(&op, &base, &file).await,
            found => found,
        };
        let sent = match found {
            Some(img) => send(generation, index, Ok(img)),
            // Made after the pictures still queued: a phone video is copied whole first (8.8 s for
            // one on a POCO X6 Pro, Windows), and every photo waited behind it (2026-11-08).
            None if slow_video(&op, &file.path) => {
                let pictures_left = {
                    let mut q = queue.lock().unwrap();
                    if q.generation == generation {
                        q.slow.insert(at, (index, file));
                    }
                    !q.items.is_empty()
                };
                pictures_left || make_slow(&op, &base, &queue, &send).await
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
        None if decode::is_played(&file.path) => match preview(op, base, &file).await {
            Some(img) => Ok(img),
            None => generate(op, base, file).await,
        },
        None => generate(op, base, file).await,
    }
}

/// From the disk cache, or the one a conversion just made for this file (saved to the cache now).
async fn cached(base: &str, file: &File) -> Option<ColorImage> {
    let (base, file) = (base.to_string(), file.clone());
    tokio::task::spawn_blocking(move || {
        thumbcache::load(&base, &file).or_else(|| {
            let data = thumbcache::take(&file.path, file.size)?;
            thumbcache::save_data(&base, &file, data.clone());
            image::load_from_memory(&data).ok()
        })
    })
    .await
    .ok()
    .flatten()
    .map(decode::to_color_image)
}

/// Made from the file and saved in the cache. For a video `preview` was tried before (the
/// queue, `thumbnail`): tried again here it read the end of the file twice.
async fn generate(op: &Store, base: &str, file: File) -> Result<ColorImage, String> {
    if decode::is_played(&file.path) {
        let img = video_still(op, &file.path, file.size, THUMB_SIDE).await?;
        let base = base.to_string();
        return tokio::task::spawn_blocking(move || {
            thumbcache::save(&base, &file, &img);
            decode::to_color_image(img)
        })
        .await
        .map_err(|e| e.to_string());
    }
    // Without the CAD engine a STEP, IGES or BREP gives no picture: not read for nothing, whole,
    // from a phone or a server (#71).
    if crate::occt::needs_engine(&file.path) {
        return Err("the CAD engine is missing".into());
    }
    let bytes = op.read(&file.path).await.map_err(|(_, detail)| detail)?;
    let base = base.to_string();
    let sibling = siblings(op.clone(), file.path.clone());
    tokio::task::spawn_blocking(move || {
        let img = if crate::model::is_model(&file.path) {
            decode::model_picture_with(&bytes, &file.path, THUMB_SIDE, THUMB_SIDE * 3 / 4, &sibling)?
        } else if crate::cad::is_cad(&file.path) {
            image::DynamicImage::ImageRgba8(decode::cad_render(&crate::cad::read_with(&bytes, &file.path, &sibling)?, THUMB_SIDE, None)?)
        } else {
            decode::thumbnail(&bytes, &file.path, THUMB_SIDE)?
        };
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
pub(crate) async fn maintain(op: &Store, base: &str) {
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
