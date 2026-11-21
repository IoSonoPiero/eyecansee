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
use crate::thumb_memory::{self, Budget, Reservation};
use crate::video_read::{SLOW, preview, slow_video, video_still};
use crate::view_memory::NotRead;
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
        tx.send(Msg::Loaded { generation, index, thumb: true, image, halves: Vec::new() }).is_ok()
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
            None => send(generation, index, generate(&op, &base, file, || wanted(&queue, generation)).await),
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
        if !send(generation, index, generate(op, base, file, || true).await) {
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
            None => generate(op, base, file, || true).await,
        },
        None => generate(op, base, file, || true).await,
    }
}

/// The details of `file` for the line under its thumbnail (#139): from the cache; else, for a
/// picture whose thumbnail is already made (a cache from before #139), read from the file once and
/// saved. `None` while its thumbnail is still to be made: `generate` saves them with it. Videos,
/// sounds, models and drawings: nothing read, nothing to show.
pub(crate) async fn file_info(op: &Store, base: &str, file: &File) -> Option<crate::thumb_details::Info> {
    let (b, f) = (base.to_string(), file.clone());
    let (info, thumb) = tokio::task::spawn_blocking(move || match thumbcache::load_info(&b, &f) {
        Some(info) => (Some(info), None),
        None => (None, thumbcache::load(&b, &f).map(|t| (t.width(), t.height()))),
    })
    .await
    .ok()?;
    if info.is_some() {
        return info;
    }
    let path = &file.path;
    // A DWG or DXF whose thumbnail was made before its number was kept with it: read once.
    if crate::cad_convert::can_convert(path) {
        let (bytes, name) = (op.read(path).await.ok()?, path.clone());
        let drawing = tokio::task::spawn_blocking(move || crate::cad_meta::summary_of(&bytes, &name)).await.ok()?;
        let info = crate::thumb_details::Info { drawing, ..Default::default() };
        thumbcache::save_info(base, file, &info);
        return Some(info);
    }
    if crate::model::is_model(path) || crate::cad::is_cad(path) || crate::occt::needs_engine(path) {
        return Some(Default::default());
    }
    if decode::is_played(path) {
        return video_info(op, base, file).await;
    }
    let thumb = thumb?;
    // The start of the file first: the size and the EXIF of a photo are in it. The whole file
    // only when they are not there (a TIFF or a RAW whose blocks are further on).
    let head = op.read_range(path, 0, HEAD.min(file.size)).await.ok();
    let full = file.size;
    let of = |bytes: Vec<u8>| {
        let name = file.path.clone();
        tokio::task::spawn_blocking(move || {
            let info = crate::thumb_details::Info::of(&bytes, &name, thumb, None);
            match info.pixels {
                Some(_) => info,
                // A rare format whose header is not read: decoded, as for its thumbnail.
                // Not one too large for the memory (`thumb_memory::over_half`).
                None if bytes.len() as u64 >= full => match thumb_memory::wait_for(&bytes, &name) {
                    Ok(_memory) => crate::thumb_details::Info { pixels: decode::thumbnail_sized(&bytes, &name, 1).ok().and_then(|(_, own)| own), ..info },
                    Err(_) => info,
                },
                None => info,
            }
        })
    };
    let mut info = match head {
        Some(bytes) => of(bytes).await.ok()?,
        None => crate::thumb_details::Info::default(),
    };
    if info.pixels.is_none() && file.size > HEAD {
        info = of(op.read(path).await.ok()?).await.ok()?;
    }
    thumbcache::save_info(base, file, &info);
    Some(info)
}

/// How much of a photo is read for its details when its thumbnail is already made.
const HEAD: u64 = 256 * 1024;

/// A video's or a sound's details: from the metadata the panel read already, else read as the
/// panel does (container and tracks, not the whole file) and kept for both. A phone on Windows
/// without partial reads would copy the whole video first: nothing then (the viewer reads it
/// when the video is opened). Without the video engine nothing is read either.
async fn video_info(op: &Store, base: &str, file: &File) -> Option<crate::thumb_details::Info> {
    let key = crate::video_read::meta_key(op, file).await;
    let (b, k) = (base.to_string(), key.clone());
    let saved = tokio::task::spawn_blocking(move || thumbcache::load_meta(&b, &k)).await.ok().flatten();
    let meta = match saved.as_deref().and_then(crate::meta::Meta::from_json) {
        Some(meta) => meta,
        None if crate::video_read::whole_only(op) || !crate::mpv_load::available() => return Some(Default::default()),
        None => {
            let meta = crate::video_read::video_meta(op, &file.path).await.ok().flatten()?;
            thumbcache::save_meta(base, &key, meta.to_json());
            meta
        }
    };
    let info = crate::thumb_details::Info::of_video(&meta);
    thumbcache::save_info(base, file, &info);
    Some(info)
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

/// Is the queue still the one of `generation` (same folder, source open)?
fn wanted(queue: &Mutex<ThumbQueue>, generation: u64) -> bool {
    let q = queue.lock().unwrap();
    q.generation == generation && !q.closed
}

/// Made from the file and saved in the cache. For a video `preview` was tried before (the
/// queue, `thumbnail`): tried again here it read the end of the file twice. `wanted` says, after
/// a wait for memory, whether the thumbnail is still wanted (the folder may have changed).
async fn generate(op: &Store, base: &str, file: File, wanted: impl Fn() -> bool) -> Result<ColorImage, String> {
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
        return Err(crate::occt::MISSING.into());
    }
    let picture = !crate::model::is_model(&file.path) && !crate::cad::is_cad(&file.path);
    let bytes = if picture { read_picture(op, &file, thumb_memory::budget()).await? } else { op.read(&file.path).await.map_err(|(_, detail)| detail)? };
    let (bytes, memory) = if picture { reserved(op, &file.path, bytes, thumb_memory::budget(), wanted).await? } else { (bytes, None) };
    let base = base.to_string();
    let (sibling, local) = (siblings(op.clone(), file.path.clone()), crate::occt::local(op, &file.path));
    tokio::task::spawn_blocking(move || {
        // Given back when the thumbnail is made, or the decoder fails or panics.
        let _memory = memory;
        let img = if crate::model::is_model(&file.path) {
            crate::occt::from_file(local, || decode::model_picture_with(&bytes, &file.path, THUMB_SIDE, THUMB_SIDE * 3 / 4, &sibling))?
        } else if crate::cad::is_cad(&file.path) {
            let d = crate::cad::read_with(&bytes, &file.path, &sibling)?;
            // Its number and revision kept with it, for the line under it (#144).
            thumbcache::save_info(&base, &file, &crate::thumb_details::Info { drawing: d.summary.clone(), ..Default::default() });
            image::DynamicImage::ImageRgba8(decode::cad_render(&d, THUMB_SIDE, None)?)
        } else {
            let (img, own) = decode::thumbnail_sized(&bytes, &file.path, THUMB_SIDE)?;
            // The file is read now anyway: its details are kept with its thumbnail, for the line
            // under it (#139), so they never cost a read of their own.
            thumbcache::save_info(&base, &file, &crate::thumb_details::Info::of(&bytes, &file.path, (img.width(), img.height()), own));
            img
        };
        thumbcache::save(&base, &file, &img);
        Ok(decode::to_color_image(img))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

/// A picture's bytes, unless it is too large for the memory (`thumb_memory::TOO_LARGE`): told
/// before decoding, from the bytes of a file up to `view_memory::ONCE` read at once, from the
/// start of a larger one when that holds its header, without reading the rest (Piero, 20 and
/// 21/11/2026).
async fn read_picture(op: &Store, file: &File, budget: &Budget) -> Result<Vec<u8>, String> {
    let path = &file.path;
    let too_large = |bytes: &[u8], size| thumb_memory::estimate_sized(bytes, size, path).over_half(bytes.len(), size, path, budget.ram());
    match crate::view_memory::read_unless_too_large(op, path, Some(file.size), too_large).await {
        Ok(bytes) => Ok(bytes),
        Err(NotRead::TooLarge(_)) => Err(thumb_memory::TOO_LARGE.into()),
        Err(NotRead::Failed((_, detail))) => Err(detail),
    }
}

/// A picture's bytes with the memory of its thumbnail reserved (`thumb_memory`), waiting while
/// there is no room. From a local disk the bytes are not kept while waiting, but read again once
/// there is room; from a phone or a server (2 to 4 workers) they are kept, not asked for twice.
/// After a wait, a thumbnail no longer `wanted` is not made: the new folder's would wait for it.
/// One `over_half` the RAM is not made at all (`thumb_memory::TOO_LARGE`).
async fn reserved(op: &Store, path: &str, bytes: Vec<u8>, budget: &Budget, wanted: impl Fn() -> bool) -> Result<(Vec<u8>, Option<Reservation>), String> {
    let name = path.to_string();
    let (bytes, need) = tokio::task::spawn_blocking(move || {
        let need = thumb_memory::estimate(&bytes, &name);
        (bytes, need)
    })
    .await
    .map_err(|e| e.to_string())?;
    if budget.too_large(need) {
        log!("thumbnail of {path}: not made, {} MiB is over half the RAM", need >> 20);
        return Err(thumb_memory::TOO_LARGE.into());
    }
    if let Some(memory) = budget.try_reserve(need) {
        return Ok((bytes, Some(memory)));
    }
    log!("thumbnail of {path}: waits for {} MiB of memory", need >> 20);
    let local = matches!(op, Store::Dal(_));
    let kept = (!local).then_some(bytes);
    let memory = Some(budget.reserve(need).await);
    if !wanted() {
        return Err(NOT_WANTED.into());
    }
    let bytes = match kept {
        Some(bytes) => bytes,
        None => op.read(path).await.map_err(|(_, detail)| detail)?,
    };
    Ok((bytes, memory))
}

/// A thumbnail of a folder no longer open, given up after its wait for memory (never shown: its
/// generation is an old one).
const NOT_WANTED: &str = "thumbnail no longer wanted";

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Polls `fut` for about half a second: it must still be waiting.
    async fn still_waiting<F: std::future::Future + Unpin>(fut: &mut F) {
        for _ in 0..50 {
            assert!(futures_util::poll!(&mut *fut).is_pending());
            std::thread::sleep(std::time::Duration::from_millis(10));
            tokio::task::yield_now().await;
        }
    }

    /// With room, the bytes read are used at once. Without, the thumbnail waits; from a local
    /// disk its bytes are read again once there is room (the file changed meanwhile: the new
    /// ones), and a thumbnail no longer wanted after its wait is given up. One over half the RAM
    /// is not made.
    #[test]
    fn memory_reserved_before_decoding() {
        let dir = std::env::temp_dir().join(format!("thumb-memory-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        std::fs::write(dir.join("a.png"), b"first").unwrap();
        let budget = Budget::new(1 << 30);
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(async {
            let (bytes, memory) = reserved(&op, "a.png", b"first".to_vec(), &budget, || true).await.unwrap();
            assert_eq!(bytes, b"first");
            drop(memory.unwrap());

            let all = budget.try_reserve(u64::MAX).unwrap();
            let mut waiting = Box::pin(reserved(&op, "a.png", b"first".to_vec(), &budget, || true));
            still_waiting(&mut waiting).await;
            std::fs::write(dir.join("a.png"), b"second").unwrap();
            drop(all);
            let (bytes, memory) = waiting.await.unwrap();
            assert_eq!(bytes, b"second");
            drop(memory);

            let all = budget.try_reserve(u64::MAX).unwrap();
            let mut stale = Box::pin(reserved(&op, "a.png", b"first".to_vec(), &budget, || false));
            still_waiting(&mut stale).await;
            drop(all);
            assert_eq!(stale.await.unwrap_err(), NOT_WANTED);
            assert!(budget.try_reserve(u64::MAX).is_some(), "given back");

            // Over half the RAM (here 400 MiB; the file, unread, is estimated at 512 MiB and more):
            // not made, nothing reserved.
            let small = Budget::new(100 << 20);
            assert_eq!(reserved(&op, "a.png", b"first".to_vec(), &small, || true).await.unwrap_err(), thumb_memory::TOO_LARGE);
            assert!(small.try_reserve(100 << 20).is_some());
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The thumbnail of a RAW of 230 MB too large for the memory (bytes at random, as `huge.cr3`):
    /// refused from the start of the file (it is over `view_memory::ONCE`), the rest never read
    /// (Piero, 20/11/2026).
    #[test]
    fn too_large_told_from_the_start() {
        let dir = std::env::temp_dir().join(format!("thumb-head-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let size = 230 << 20;
        std::fs::File::create(dir.join("huge.cr3")).unwrap().set_len(size).unwrap();
        let file = File { path: "huge.cr3".into(), size, modified: String::new() };
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        source::BYTES_READ.with(|n| n.set(0));
        // 2 GiB to share, out of 8 GiB of RAM: 16.6 GB needed.
        let r = rt.block_on(read_picture(&op, &file, &Budget::new(2 << 30)));
        assert_eq!(r.unwrap_err(), thumb_memory::TOO_LARGE);
        assert_eq!(source::BYTES_READ.with(std::cell::Cell::get), crate::view_memory::HEAD);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
