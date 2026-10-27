//! Videos read from any source (disk, phone, server) without reading them whole: the preview a
//! camera stored inside, the metadata, the stream mpv plays, and a still for the thumbnails.

use std::path::{Path, PathBuf};
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
    let jpeg = match phone_thumbnail(op, &file.path).await {
        Some(jpeg) => jpeg,
        None => preview_jpeg(op, &file.path, file.size).await?,
    };
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
    match op.local_file(path) {
        Some(file) => crate::mpv::Input::File(file),
        None => crate::mpv::Input::Stream(Arc::new(StoreReader::new(rt, op, path, size, whole_only(op)))),
    }
}

struct StoreReader {
    rt: Handle,
    op: Store,
    path: String,
    size: u64,
    /// Copied to a temporary file at the first read (`whole_only`), removed with the reader.
    copy: Option<std::sync::OnceLock<Result<PathBuf, String>>>,
}

impl StoreReader {
    fn new(rt: Handle, op: &Store, path: &str, size: u64, whole: bool) -> Self {
        StoreReader { rt, op: op.clone(), path: path.to_string(), size, copy: whole.then(std::sync::OnceLock::new) }
    }

    /// The whole file in a temporary file, read in order in one go.
    async fn copy_whole(&self) -> Result<PathBuf, String> {
        static N: AtomicU64 = AtomicU64::new(0);
        let tmp = std::env::temp_dir().join(format!("eyecansee-video-{}-{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let (tx, mut rx) = tokio::sync::mpsc::channel::<std::io::Result<bytes::Bytes>>(4);
        let (done, never) = (AtomicU64::new(0), std::sync::atomic::AtomicBool::new(false));
        let write = async {
            let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            while let Some(chunk) = rx.recv().await {
                std::io::Write::write_all(&mut file, &chunk.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            }
            Ok(())
        };
        let (sent, written) = futures_util::future::join(self.op.send_chunks(&self.path, self.size, tx, &done, &never), write).await;
        match sent.map_err(|(_, d)| d).and(written) {
            Ok(()) => Ok(tmp),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    fn read_copy(&self, copy: &Path, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(copy).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        file.take(len).read_to_end(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

impl Drop for StoreReader {
    fn drop(&mut self) {
        if let Some(Ok(tmp)) = self.copy.as_ref().and_then(|c| c.get()) {
            let _ = std::fs::remove_file(tmp);
        }
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
                Ok(tmp) => self.read_copy(tmp, offset, len),
                Err(e) => Err(e.clone()),
            },
            None => self.rt.block_on(self.op.read_range(&self.path, offset, len)).map_err(|(_, detail)| detail),
        }
    }
}

/// Video stills at a time from a slow disk (external, phone, server). Many at once make an SD card
/// on USB 2 seek among them and none ends: 32 on a dashcam all failed after 20 s (2026-10-27).
pub(crate) static SLOW: Semaphore = Semaphore::const_new(2);

/// A video whose stills go through `SLOW`: not on the disk of the home folder.
pub(crate) fn slow_video(op: &Store, path: &str) -> bool {
    decode::is_video(path) && op.local_file(path).is_none_or(|f| !on_home_disk(&f))
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
    let input = video_input(Handle::current(), op, path, size);
    tokio::task::spawn_blocking(move || crate::mpv::still(input, side).map(|i| image::DynamicImage::ImageRgba8(i).to_rgb8().into()))
        .await
        .unwrap_or_else(|e| Err(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpv::Reader;

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
        let tmp = reader.copy.as_ref().unwrap().get().unwrap().clone().unwrap();
        assert!(tmp.exists());
        drop(reader);
        assert!(!tmp.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
