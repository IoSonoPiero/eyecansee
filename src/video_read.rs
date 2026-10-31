//! Videos read from any source (disk, phone, server) without reading them whole: the preview a
//! camera stored inside, the metadata, the stream mpv plays, and a still for the thumbnails.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::Semaphore;

use eframe::egui::ColorImage;

use crate::decode;
use crate::source::{File, Store};
use crate::thumbcache;
use crate::loader::THUMB_SIDE;

/// The preview a camera stored inside the video (`mp4::thumbnail`), saved in the cache: 64 KB
/// read instead of opening the video.
pub(crate) async fn preview(op: &Store, base: &str, file: &File) -> Option<ColorImage> {
    let start = std::time::Instant::now();
    let found = match phone_thumbnail(op, &file.path).await {
        Some(jpeg) => Some((jpeg, "the phone's thumbnail")),
        // A phone on Windows reads a file only from the start: the preview, at the end with the
        // `moov`, is looked for in the whole copy the frame needs anyway (`video_still`).
        None if whole_only(op) => None,
        None => preview_jpeg(op, &file.path, file.size).await.map(|j| (j, "the video's preview")),
    };
    log!("preview {:?}: {} in {:.2} s", file.path, found.as_ref().map_or("none", |f| f.1), start.elapsed().as_secs_f64());
    let jpeg = found?.0;
    let (base, file) = (base.to_string(), file.clone());
    tokio::task::spawn_blocking(move || {
        let img = decode::thumbnail(&jpeg, "preview.jpg", THUMB_SIDE).ok()?;
        thumbcache::save(&base, &file, &img);
        Some(decode::to_color_image(img))
    })
    .await
    .ok()
    .flatten()
}

/// The preview of `mp4::thumbnail`: in the first bytes, or in a `moov` after the media data. Only
/// box headers are read on the way (16 bytes each), then the picture: a phone's `moov` is ~1 MB
/// of sample tables without a preview, a DJI drone's 1.5 MB with a 625 KB cover (2026-10-27).
async fn preview_jpeg(op: &Store, path: &str, size: u64) -> Option<Vec<u8>> {
    use crate::mp4;
    if whole_only(op) {
        return None;
    }
    let head = op.read_range(path, 0, size.min(mp4::HEAD)).await.ok()?;
    if let Some(jpeg) = mp4::thumbnail(&head) {
        return Some(jpeg.to_vec());
    }
    let head = &head;
    let read = |at: u64, len: u64| async move {
        match head.get(at as usize..(at + len) as usize) {
            Some(bytes) => Some(bytes.to_vec()),
            None => op.read_range(path, at, len).await.ok(),
        }
    };
    let (moov, moov_len) = find_box(&read, 0, size, b"moov").await?;
    let (udta, udta_len) = find_box(&read, moov + 8, moov + moov_len, b"udta").await?;
    let (at, len) = match find_box(&read, udta + 8, udta + udta_len, b"thmb").await {
        Some(thmb) => thmb,
        None => {
            // `meta` is a full box: 4 bytes of version and flags before its children.
            let (meta, meta_len) = find_box(&read, udta + 8, udta + udta_len, b"meta").await?;
            let (ilst, ilst_len) = find_box(&read, meta + 12, meta + meta_len, b"ilst").await?;
            find_box(&read, ilst + 8, ilst + ilst_len, b"covr").await?
        }
    };
    if len > mp4::IMAGE_MAX {
        return None;
    }
    mp4::image(&read(at + 8, len - 8).await?).map(<[u8]>::to_vec)
}

/// Offset and size of the first box of type `kind` between `start` and `end`, reading only the
/// headers (at most 16 boxes: ftyp, free, mdat, moov; mvhd, trak…, udta).
async fn find_box<F, Fut>(read: &F, start: u64, end: u64, kind: &[u8; 4]) -> Option<(u64, u64)>
where
    F: Fn(u64, u64) -> Fut,
    Fut: std::future::Future<Output = Option<Vec<u8>>>,
{
    let mut at = start;
    for _ in 0..16 {
        let header = read(at, end.checked_sub(at)?.min(16)).await?;
        let (k, len) = crate::mp4::box_header(&header)?;
        let len = if len == 0 { end - at } else { len };
        if at + len > end {
            return None;
        }
        if &k == kind {
            return Some((at, len));
        }
        at += len;
        if at + 8 > end {
            return None;
        }
    }
    None
}

/// The metadata panel for a video: container tags and tracks (`mpv::info`), the file not read whole.
pub(crate) async fn video_meta(op: &Store, path: &str) -> Result<Option<crate::meta::Meta>, String> {
    let size = op.size(path).await.map_err(|(_, detail)| detail)?;
    let input = video_input(Handle::current(), op, path, size);
    let file = op.local_file(path);
    let srt = drone_start(op, path).await;
    tokio::task::spawn_blocking(move || {
        let info = crate::mpv::info(input)?;
        let mut meta = crate::meta::from_video(&info, size);
        // A dashcam's track beside the videos, for the files on this computer.
        if let (Some(file), Some(start)) = (file, crate::meta::video_start(&info)) {
            let end = start + info.duration.unwrap_or(0.0).ceil() as i64;
            crate::meta::add_track(&mut meta, &crate::gps_track::points(&file, start, end));
        }
        if meta.gps.is_none() {
            meta.gps = srt;
        }
        Ok(Some(meta))
    })
        .await
        .unwrap_or_else(|e| Err(e.to_string()))
}

/// What a conversion writes in the new file beyond the tags FFmpeg copies: the date as
/// `date_recorded` too (Matroska has no `creation_time`: FFmpeg turns it into DateUTC, which
/// libmpv does not report), and the position kept beside the video: the dashcam's start point,
/// altitude and speeds (`gps_track`), or the drone's start point (`.SRT`); not when the file
/// carries its own `location` (copied with the other tags). Only the drone's without libmpv.
pub(crate) async fn track_tags(op: &Store, path: &str) -> Vec<(String, String)> {
    let mut tags = Vec::new();
    let mut located = false;
    if let Some(file) = op.local_file(path) {
        let found = tokio::task::spawn_blocking(move || {
            let info = crate::mpv::info(crate::mpv::Input::File(file.clone())).ok()?;
            let mut tags = Vec::new();
            if let Some((_, t)) = info.tags.iter().find(|(k, _)| *k == "creation_time") {
                tags.push(("date_recorded".to_string(), crate::meta::first(t).to_string()));
            }
            if info.tags.iter().any(|(k, _)| *k == "location" || *k == "com.apple.quicktime.location.ISO6709") {
                return Some((tags, true));
            }
            let points = crate::meta::video_start(&info)
                .map(|start| crate::gps_track::points(&file, start, start + info.duration.unwrap_or(0.0).ceil() as i64))
                .unwrap_or_default();
            let Some(&(_, lat, lon, alt)) = points.first() else { return Some((tags, false)) };
            tags.push(("location".to_string(), crate::gps_track::location(lat, lon, Some(alt))));
            if let Some((avg, top)) = crate::gps_track::speeds(&points) {
                tags.push(("speed_average_kmh".into(), format!("{avg:.0}")));
                tags.push(("speed_top_kmh".into(), format!("{top:.0}")));
            }
            Some((tags, true))
        })
        .await
        .ok()
        .flatten();
        if let Some((t, l)) = found {
            (tags, located) = (t, l);
        }
    }
    if !located && let Some((lat, lon)) = drone_start(op, path).await {
        tags.push(("location".into(), crate::gps_track::location(lat, lon, None)));
    }
    tags
}

/// A drone's position when the video started, from the first frame of the `.SRT` with the same
/// name (`gps_track::srt_start`); any source, only its first 2 KB read.
async fn drone_start(op: &Store, path: &str) -> Option<(f64, f64)> {
    let stem = path.rsplit_once('.')?.0;
    for ext in ["SRT", "srt"] {
        let srt = format!("{stem}.{ext}");
        if let Ok(size) = op.size(&srt).await {
            let head = op.read_range(&srt, 0, size.min(crate::gps_track::SRT_HEAD)).await.ok()?;
            return crate::gps_track::srt_start(&String::from_utf8_lossy(&head));
        }
    }
    None
}

/// The thumbnail a phone keeps for its files, decodable: a video's still in one small read.
async fn phone_thumbnail(op: &Store, path: &str) -> Option<Vec<u8>> {
    let Store::Mtp(phone) = op else { return None };
    let jpeg = phone.thumbnail(path).await.ok().filter(|j| !j.is_empty())?;
    image::load_from_memory(&jpeg).ok().map(|_| jpeg)
}

/// A phone on Windows is read through WPD, whose streams cannot seek: every read at an offset
/// reads the file from the start up to there, and the Samsung A57 dropped the connection after
/// minutes of that (2026-10-27). Such a file is read whole, in order, once.
fn whole_only(op: &Store) -> bool {
    cfg!(windows) && matches!(op, Store::Mtp(_))
}

/// What mpv plays: the file itself when it is on this computer (also through gvfs), otherwise
/// read a piece at a time from the phone or the server.
pub(crate) fn video_input(rt: Handle, op: &Store, path: &str, size: u64) -> crate::mpv::Input {
    video_input_copied(rt, op, path, size).0
}

/// As `video_input`, with the bytes copied so far when the file is copied whole first (a phone
/// on Windows): the viewer shows how far that is, since nothing plays until it is done.
pub(crate) fn video_input_copied(rt: Handle, op: &Store, path: &str, size: u64) -> (crate::mpv::Input, Option<Arc<AtomicU64>>) {
    match op.local_file(path) {
        Some(file) => (crate::mpv::Input::File(file), None),
        None => {
            let reader = StoreReader::new(rt, op, path, size, whole_only(op));
            let copied = reader.copy.is_some().then(|| reader.copied.clone());
            (crate::mpv::Input::Stream(Arc::new(reader)), copied)
        }
    }
}

/// A whole copy and the file kept open on it.
type OpenCopy = (PathBuf, std::sync::Mutex<std::fs::File>);

struct StoreReader {
    rt: Handle,
    op: Store,
    path: String,
    size: u64,
    /// Copied to a temporary file at the first read (`whole_only`), kept open: `COPIES` may remove
    /// it while it plays (thumbnails copy other videos), and an open file is still read.
    copy: Option<std::sync::OnceLock<Result<OpenCopy, String>>>,
    /// Bytes of that copy done.
    copied: Arc<AtomicU64>,
}

impl StoreReader {
    fn new(rt: Handle, op: &Store, path: &str, size: u64, whole: bool) -> Self {
        StoreReader { rt, op: op.clone(), path: path.to_string(), size, copy: whole.then(std::sync::OnceLock::new), copied: Arc::default() }
    }

    async fn copy_whole(&self) -> Result<OpenCopy, String> {
        let tmp = copy_whole(&self.op, &self.path, self.size, &self.copied).await?;
        let file = std::fs::File::open(&tmp).map_err(|e| e.to_string())?;
        Ok((tmp, std::sync::Mutex::new(file)))
    }
    fn read_copy(file: &std::sync::Mutex<std::fs::File>, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = file.lock().unwrap();
        file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        (&mut *file).take(len).read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// A whole copy being made: whoever asks for the same file meanwhile (the viewer, a thumbnail
/// asked again after the folder was read again) waits for it and follows its bytes. Three copies
/// of the same video at once made each three times slower (2026-10-31).
struct InFlight {
    done: tokio::sync::OnceCell<Result<PathBuf, String>>,
    copied: AtomicU64,
}

static IN_FLIGHT: std::sync::Mutex<Vec<(String, Arc<InFlight>)>> = std::sync::Mutex::new(Vec::new());

/// One whole copy at a time: the phone answers one request at a time, two copies shared its speed
/// and both ended late (6.4 s alone, 15 s each in pairs).
static ONE_COPY: Semaphore = Semaphore::const_new(1);

/// The whole file in a temporary file, read in order in one go (`copied` counts the bytes); one
/// copied before and still kept (`COPIES`), or being copied now, is used.
async fn copy_whole(op: &Store, path: &str, size: u64, copied: &AtomicU64) -> Result<PathBuf, String> {
    // ponytail: path and size only; two phones with the same path and size would share a copy.
    let key = format!("{path}\0{size}");
    if let Some(tmp) = kept_copy(&key, size) {
        copied.store(size, Ordering::Relaxed);
        return Ok(tmp);
    }
    let flight = {
        let mut all = IN_FLIGHT.lock().unwrap();
        match all.iter().find(|(k, _)| *k == key) {
            Some((_, f)) => f.clone(),
            None => {
                let f = Arc::new(InFlight { done: tokio::sync::OnceCell::new(), copied: AtomicU64::new(0) });
                all.push((key.clone(), f.clone()));
                f
            }
        }
    };
    let make = flight.done.get_or_init(|| async {
        let _one = ONE_COPY.acquire().await;
        match kept_copy(&key, size) {
            Some(tmp) => Ok(tmp),
            None => copy_now(op, path, size, &flight.copied, key.clone()).await,
        }
    });
    // Its bytes passed on to this caller's counter (the viewer's bar) while it lasts.
    tokio::pin!(make);
    let r = loop {
        tokio::select! {
            r = &mut make => break r.clone(),
            () = tokio::time::sleep(std::time::Duration::from_millis(200)) => copied.store(flight.copied.load(Ordering::Relaxed), Ordering::Relaxed),
        }
    };
    IN_FLIGHT.lock().unwrap().retain(|(_, f)| !Arc::ptr_eq(f, &flight));
    if r.is_ok() {
        copied.store(size, Ordering::Relaxed);
    }
    r
}

async fn copy_now(op: &Store, path: &str, size: u64, copied: &AtomicU64, key: String) -> Result<PathBuf, String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let tmp = std::env::temp_dir().join(format!("eyecansee-video-{}-{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<std::io::Result<bytes::Bytes>>(4);
    let never = std::sync::atomic::AtomicBool::new(false);
    let write = async {
        let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        while let Some(chunk) = rx.recv().await {
            std::io::Write::write_all(&mut file, &chunk.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    let (sent, written) = futures_util::future::join(op.send_chunks(path, size, tx, copied, &never), write).await;
    match sent.map_err(|(id, d)| format!("{id} {d}")).and(written) {
        Ok(()) => {
            keep_copy(key, tmp.clone());
            Ok(tmp)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Videos copied whole (`StoreReader::copy_whole`), newest last: opened again, they play at once.
/// ponytail: a count, not a size; a cap in bytes if videos of several GB fill the disk.
static COPIES: std::sync::Mutex<Vec<(String, PathBuf)>> = std::sync::Mutex::new(Vec::new());
const KEPT_COPIES: usize = 3;

/// The copy kept for `key`, if it is still there whole (moved to the newest place).
fn kept_copy(key: &str, size: u64) -> Option<PathBuf> {
    let mut copies = COPIES.lock().unwrap();
    let i = copies.iter().position(|(k, _)| k == key)?;
    let entry = copies.remove(i);
    if std::fs::metadata(&entry.1).is_ok_and(|m| m.len() == size) {
        let tmp = entry.1.clone();
        copies.push(entry);
        Some(tmp)
    } else {
        let _ = std::fs::remove_file(&entry.1);
        None
    }
}

fn keep_copy(key: String, tmp: PathBuf) {
    let mut copies = COPIES.lock().unwrap();
    copies.push((key, tmp));
    while copies.len() > KEPT_COPIES {
        let _ = std::fs::remove_file(copies.remove(0).1);
    }
}

/// At start (after `single::hand_over`, so no other copy of the app is using them): the copies a
/// copy of the app closed by force or crashed left in the temporary folder.
pub(crate) fn remove_old_copies() {
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("eyecansee-video-") && name.ends_with(".tmp") {
            log!("old video copy removed: {name}");
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// At exit: the copies go.
pub(crate) fn forget_copies() {
    for (_, tmp) in COPIES.lock().unwrap().drain(..) {
        let _ = std::fs::remove_file(tmp);
    }
}

impl crate::mpv::Reader for StoreReader {
    fn size(&self) -> u64 {
        self.size
    }

    /// From mpv's threads, outside the runtime: waiting here is allowed.
    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        match &self.copy {
            Some(copy) => match copy.get_or_init(|| self.rt.block_on(self.copy_whole())) {
                Ok((_, file)) => Self::read_copy(file, offset, len),
                Err(e) => Err(e.clone()),
            },
            // With the error's id: the log said only the path when the file was gone (2026-10-29).
            None => self.rt.block_on(self.op.read_range(&self.path, offset, len)).map_err(|(id, detail)| format!("{id} {detail}")),
        }
    }
}

/// Video stills at a time from a slow disk (external, phone, server). Many at once make an SD card
/// on USB 2 seek among them and none ends: 32 on a dashcam all failed after 20 s (2026-10-27).
pub(crate) static SLOW: Semaphore = Semaphore::const_new(2);

/// A video whose stills go through `SLOW`: not on the disk of the home folder.
pub(crate) fn slow_video(op: &Store, path: &str) -> bool {
    decode::is_played(path) && op.local_file(path).is_none_or(|f| !on_home_disk(&f))
}

#[cfg(unix)]
fn on_home_disk(file: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let dev = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.dev()).ok();
    std::env::home_dir().is_some_and(|home| dev(file).is_some() && dev(file) == dev(&home))
}

/// Same drive letter as the home folder.
#[cfg(windows)]
fn on_home_disk(file: &std::path::Path) -> bool {
    std::env::home_dir().is_some_and(|home| file.components().next() == home.components().next())
}

/// A frame of a video (see `mpv::still`), opaque so the cache keeps it as JPEG. On the home disk
/// one per thumbnail worker, as images: with one decoding thread each, 32 at a time make 73 a
/// second, 4 made 11 (2026-10-27).
pub(crate) async fn video_still(op: &Store, path: &str, size: u64, side: u32) -> Result<image::DynamicImage, String> {
    if whole_only(op) {
        return still_from_copy(op, path, size, side).await;
    }
    let input = video_input(Handle::current(), op, path, size);
    tokio::task::spawn_blocking(move || crate::mpv::still(input, side).map(|i| image::DynamicImage::ImageRgba8(i).to_rgb8().into()))
        .await
        .unwrap_or_else(|e| Err(e.to_string()))
}

/// `video_still` for a phone on Windows: the file copied whole once (kept, so the viewer opens it
/// at once), then the camera's preview looked for in the copy, then a frame of it.
async fn still_from_copy(op: &Store, path: &str, size: u64, side: u32) -> Result<image::DynamicImage, String> {
    let start = std::time::Instant::now();
    let tmp = copy_whole(op, path, size, &AtomicU64::new(0)).await?;
    let copied = start.elapsed().as_secs_f64();
    // A thumbnail only: the preview is small, the viewer's still (side 0) is the full frame.
    if side > 0 {
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&std::env::temp_dir().to_string_lossy())).map_err(|e| e.to_string())?);
        let name = tmp.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(jpeg) = preview_jpeg(&local, &name, size).await
            && let Ok(img) = decode::thumbnail(&jpeg, "preview.jpg", side)
        {
            log!("still {path:?}: the video's preview, copied in {copied:.2} s");
            return Ok(img);
        }
    }
    let input = crate::mpv::Input::File(tmp);
    let img = tokio::task::spawn_blocking(move || crate::mpv::still(input, side).map(|i| image::DynamicImage::ImageRgba8(i).to_rgb8().into()))
        .await
        .unwrap_or_else(|e| Err(e.to_string()))?;
    log!("still {path:?}: a frame, copied in {copied:.2} s, {:.2} s in all", start.elapsed().as_secs_f64());
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpv::Reader;

    /// The same file asked twice at once (the viewer during its thumbnail): copied once.
    #[test]
    fn one_copy_at_once() {
        let dir = std::env::temp_dir().join(format!("eyecansee-once-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = vec![7u8; 2_000_000];
        std::fs::write(dir.join("w.mp4"), &data).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (a, b) = (AtomicU64::new(0), AtomicU64::new(0));
        let (x, y) = rt.block_on(futures_util::future::join(copy_whole(&op, "w.mp4", 2_000_000, &a), copy_whole(&op, "w.mp4", 2_000_000, &b)));
        assert_eq!(x.unwrap(), y.unwrap(), "one copy for both");
        assert_eq!(b.load(Ordering::Relaxed), 2_000_000, "the one waiting sees it done");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A reader that copies the file whole first (a phone on Windows): the reads match the file,
    /// the copy goes away with the reader.
    #[test]
    fn whole_copy_reader() {
        let dir = std::env::temp_dir().join(format!("eyecansee-whole-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(dir.join("v.mp4"), &data).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let reader = StoreReader::new(rt.handle().clone(), &op, "v.mp4", data.len() as u64, true);
        assert_eq!(reader.read(2_999_000, 5_000).unwrap(), data[2_999_000..]);
        assert_eq!(reader.read(10, 100).unwrap(), data[10..110]);
        // What the viewer's bar shows: all of it once copied.
        assert_eq!(reader.copied.load(Ordering::Relaxed), data.len() as u64);
        let tmp = reader.copy.as_ref().unwrap().get().unwrap().as_ref().unwrap().0.clone();
        assert!(tmp.exists());
        // Kept after the viewer closes: opened again, the same copy, nothing read.
        drop(reader);
        std::fs::remove_file(dir.join("v.mp4")).unwrap();
        let again = StoreReader::new(rt.handle().clone(), &op, "v.mp4", data.len() as u64, true);
        assert_eq!(again.read(10, 100).unwrap(), data[10..110]);
        assert_eq!(again.copy.as_ref().unwrap().get().unwrap().as_ref().unwrap().0, tmp);
        // Removed from the cache while open (thumbnails of other videos): still read.
        forget_copies();
        assert!(!tmp.exists() || cfg!(windows));
        assert_eq!(again.read(2_999_000, 5_000).unwrap(), data[2_999_000..]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
