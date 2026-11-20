//! Memory of the thumbnails made at the same time (#177). On a local disk there are as many
//! workers as logical processors, and every thumbnail but a JPEG's decodes its picture whole: 16
//! to 32 large photos (RAW, HEIC, PSD, 16-bit TIFF) at once took more memory than a PC with 16 GB
//! has. So, before decoding, each thumbnail reserves the memory it will use, estimated from the
//! file's header with factors measured one file at a time (`notes/rapporti/miniature-177-*`), out
//! of a quarter of the computer's RAM (Piero, 20/11/2026), and waits while there is no room. A
//! picture larger than the whole budget waits until no other is being made, then is made alone.
//! No photo is refused: the ceiling on the size of a picture is `pixel_limit`'s.

use std::sync::{Arc, LazyLock};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const MIB: u64 = 1024 * 1024;

/// The RAM assumed when it cannot be read: the smallest PC EyeCanSee is made for (8 GiB), so the
/// budget is never larger than on that PC.
const FALLBACK_RAM: u64 = 8 * 1024 * MIB;

/// The memory of the thumbnails made at the same time, in MiB: one permit per MiB.
pub struct Budget {
    places: Arc<Semaphore>,
    total: u32,
}

/// The memory a thumbnail reserved: given back when dropped, also on an error or a panic.
pub type Reservation = OwnedSemaphorePermit;

impl Budget {
    pub fn new(bytes: u64) -> Self {
        let total = u32::try_from(bytes / MIB).unwrap_or(u32::MAX).max(1);
        Budget { places: Arc::new(Semaphore::new(total as usize)), total }
    }

    /// The permits for `bytes`: at least one, at most all (a picture larger than the budget).
    fn places(&self, bytes: u64) -> u32 {
        u32::try_from(bytes.div_ceil(MIB)).unwrap_or(u32::MAX).clamp(1, self.total)
    }

    /// `bytes` if there is room now and nobody is waiting before.
    pub fn try_reserve(&self, bytes: u64) -> Option<Reservation> {
        self.places.clone().try_acquire_many_owned(self.places(bytes)).ok()
    }

    /// `bytes`, once there is room. The waiting ones are served in order (tokio's semaphore is
    /// fair), so a large picture is not passed over for ever by small ones. `None` only if the
    /// semaphore were closed, which it never is.
    pub async fn reserve(&self, bytes: u64) -> Option<Reservation> {
        self.places.clone().acquire_many_owned(self.places(bytes)).await.ok()
    }
}

/// The budget of all the thumbnails: a quarter of the RAM.
pub fn budget() -> &'static Budget {
    static BUDGET: LazyLock<Budget> = LazyLock::new(|| Budget::new(quarter(ram())));
    &BUDGET
}

/// A quarter of `ram`, or of `FALLBACK_RAM` when it could not be read (written in the log).
fn quarter(ram: Option<u64>) -> u64 {
    match ram {
        Some(ram) => {
            log!("thumbnails: at most {} MiB of memory at the same time (RAM {} MiB)", ram / 4 / MIB, ram / MIB);
            ram / 4
        }
        None => {
            log!("thumbnails: RAM not read, {} MiB assumed: at most {} MiB of memory at the same time", FALLBACK_RAM / MIB, FALLBACK_RAM / 4 / MIB);
            FALLBACK_RAM / 4
        }
    }
}

/// The computer's physical memory.
#[cfg(target_os = "linux")]
fn ram() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    meminfo_total(&info)
}

/// "MemTotal" of /proc/meminfo, in bytes.
#[cfg(any(target_os = "linux", test))]
fn meminfo_total(info: &str) -> Option<u64> {
    let kib = info.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()?;
    (kib > 0).then_some(kib * 1024)
}

#[cfg(windows)]
fn ram() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `status` is valid and writable for the whole call.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    (ok && status.ullTotalPhys > 0).then_some(status.ullTotalPhys)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn ram() -> Option<u64> {
    None
}

/// The memory of the thumbnail of `bytes`, waited for on a thread that may block (inside
/// `spawn_blocking`): for a picture decoded outside the workers (`thumb_queue::file_info`).
pub fn wait_for(bytes: &[u8], name: &str) -> Option<Reservation> {
    let need = estimate(bytes, name);
    budget().try_reserve(need).or_else(|| tokio::runtime::Handle::try_current().ok()?.block_on(budget().reserve(need)))
}

// The factors below: the peak of one thumbnail (VmHWM, build --release, one file per process)
// against width × height × the bytes per pixel of the decoded picture, on photos of 24 to 512
// megapixels (`notes/rapporti/miniature-177-2026-11-20.md`). Each is the largest measured for
// its decoder plus about 15% (five runs of the same file differ by less than 1%).

/// Besides the pixels, every thumbnail: decoder, its threads, the small copies (24-50 MB measured
/// for pictures under 2 megapixels, part of which is the process's own, once only).
const FIXED: u64 = 32 * MIB;

/// A JPEG decoded already reduced (`decode::jpeg_scaled`), per pixel of the whole picture:
/// 0.29-0.39 measured from 96 megapixels up, at most 1.9 at 24 (with `FIXED`, 0.6 covers them).
const JPEG_SCALED: f64 = 0.6;
/// A progressive JPEG reduced the same: libjpeg-turbo keeps the coefficients of the whole picture
/// (6.0 a pixel measured, 4:4:4).
const JPEG_PROGRESSIVE: f64 = 7.0;
/// A JPEG decoded whole (lossless, CMYK), RGB: 1.2 and 1.7 times the picture.
const JPEG_WHOLE: f64 = 2.0;
/// HEIC and AVIF, always decoded to RGBA at 8 bits (4 bytes): 2.3-2.8 times.
const HEIF: f64 = 3.25;
/// A PSD at 8 bits, RGBA (4 bytes): 3.0-3.3 times; every layer is decoded too, 4 bytes a pixel.
const PSD: f64 = 3.8;
const PSD_LAYER: f64 = 4.6;
/// PSD at 16 bits and PSB go to ImageMagick (16-bit RGBA, 8 bytes): 3.5 times; every layer 8
/// bytes a pixel more.
const PSD_MAGICK: f64 = 4.0;
const PSD_MAGICK_LAYER: f64 = 9.2;
/// JPEG XL: jxl-oxide works in 32-bit floats, 9.2 bytes a pixel for every channel measured.
const JXL_CHANNEL: f64 = 11.0;
/// The other formats `image` decodes (TIFF, PNG, WebP, GIF, EXR…): 1.3-2.0 times the picture.
const IMAGE: f64 = 2.3;
/// A DNG developed, per pixel of its sensor: 29-48.5 measured (iPhone ProRAW the highest).
const DNG: f64 = 56.0;
/// The other RAW formats give their size only with the sensor data: the estimate goes by the
/// file's size instead, 17-61 times measured on 44 files (a Canon CR3 the highest).
const RAW_PER_BYTE: u64 = 72;
/// An SVG, rasterised at `decode::SVG_SIDE` (12.7 bytes a pixel of the raster measured), and its
/// tree, several times the file.
const SVG: f64 = 4.0;
const SVG_PER_BYTE: u64 = 8;
/// A file whose header says no size (formats read only by ImageMagick or the rare decoders, a wrong
/// extension): 2.3-28 times its size measured (JPEG 2000 the highest), at least `UNKNOWN_FLOOR`.
const UNKNOWN_PER_BYTE: u64 = 32;
const UNKNOWN_FLOOR: u64 = 512 * MIB;

/// The memory the thumbnail of `bytes` takes, file included, from its header. Never refused, never
/// zero; more than the whole budget only means alone (`Budget::places`).
pub fn estimate(bytes: &[u8], name: &str) -> u64 {
    let file = bytes.len() as u64;
    let picture = if bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        && let Some(jpeg) = jpeg(bytes)
    {
        jpeg
    } else if crate::decode::is_raw(name) {
        match crate::raw_meta::dng_pixels(bytes) {
            Some(px) => times(px, DNG),
            None => file.saturating_mul(RAW_PER_BYTE),
        }
    } else if crate::decode::is_svg(name) {
        let side = crate::decode::SVG_SIDE as u64;
        times(side * side * 4, SVG).saturating_add(file.saturating_mul(SVG_PER_BYTE))
    } else {
        match crate::decode::header(bytes, name) {
            Some(h) => decoded(&h, bytes),
            None => file.saturating_mul(UNKNOWN_PER_BYTE).max(UNKNOWN_FLOOR),
        }
    };
    file.saturating_add(picture).saturating_add(FIXED)
}

/// A JPEG: reduced while decoded, or whole (lossless, CMYK: `decode::jpeg_scaled` fails on them).
fn jpeg(bytes: &[u8]) -> Option<u64> {
    use turbojpeg::Colorspace;
    let h = turbojpeg::Decompressor::new().ok()?.read_header(bytes).ok()?;
    let px = (h.width as u64).saturating_mul(h.height as u64);
    Some(if h.is_lossless || matches!(h.colorspace, Colorspace::CMYK | Colorspace::YCCK) {
        times(px * 3, JPEG_WHOLE)
    } else if h.is_progressive {
        times(px, JPEG_PROGRESSIVE)
    } else {
        times(px, JPEG_SCALED)
    })
}

/// The pictures whose header `decode::header` reads, by their decoder.
fn decoded(h: &crate::decode::Header, bytes: &[u8]) -> u64 {
    let px = u64::from(h.size[0]) * u64::from(h.size[1]);
    let (channels, depth) = match &h.color {
        Some(c) => (if c.rgb { 3 } else { 1 } + u64::from(c.alpha), u64::from(c.bits.div_ceil(8).max(1))),
        // A PSD in CMYK, Lab or indexed colours: four channels at 16 bits, to be safe.
        None => (4, 2),
    };
    match h.format.as_str() {
        "HEIC" | "HEIF" | "HIF" | "AVIF" => times(px * 4, HEIF),
        "JPEG XL" => times(px * channels, JXL_CHANNEL),
        "PSD" | "PSB" => {
            let layers = f64::from(crate::psd::layers(bytes).unwrap_or(0));
            if h.format == "PSD" && depth == 1 && h.color.is_some() {
                times(px, 4.0 * PSD + layers * PSD_LAYER)
            } else {
                times(px, 8.0 * PSD_MAGICK + layers * PSD_MAGICK_LAYER)
            }
        }
        _ => times(px * channels * depth, IMAGE),
    }
}

fn times(bytes: u64, factor: f64) -> u64 {
    (bytes as f64 * factor).min(u64::MAX as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    const GIB: u64 = 1024 * MIB;

    /// A reservation that fits is given at once, one that does not waits, nobody passes it, and
    /// it gets its turn as soon as the first is given back.
    #[test]
    fn reserve_and_wait() {
        let b = Budget::new(100 * MIB);
        let first = b.try_reserve(60 * MIB).unwrap();
        assert!(b.try_reserve(50 * MIB).is_none());
        let mut waiting = Box::pin(b.reserve(50 * MIB));
        assert!(waiting.as_mut().now_or_never().is_none());
        assert!(b.try_reserve(40 * MIB).is_none(), "nobody passes the one waiting");
        drop(first);
        let second = waiting.now_or_never().flatten().unwrap();
        assert_eq!(second.num_permits(), 50);
        drop(second);
        assert_eq!(b.places.available_permits(), 100);
    }

    /// A picture larger than the whole budget waits until nothing else is reserved, then takes
    /// it all: alone, never refused.
    #[test]
    fn larger_than_budget_goes_alone() {
        let b = Budget::new(100 * MIB);
        let small = b.try_reserve(MIB).unwrap();
        assert!(b.try_reserve(10 * GIB).is_none());
        let mut big = Box::pin(b.reserve(10 * GIB));
        assert!(big.as_mut().now_or_never().is_none());
        drop(small);
        let big = big.now_or_never().flatten().unwrap();
        assert_eq!(big.num_permits(), 100);
        assert!(b.try_reserve(1).is_none());
        drop(big);
        assert!(b.try_reserve(u64::MAX).is_some());
    }

    /// The memory reserved is given back when the work fails or panics.
    #[test]
    fn given_back_on_error_and_panic() {
        let b = Budget::new(100 * MIB);
        let failing = |r: Reservation| -> Result<(), String> {
            let _r = r;
            Err("broken file".into())
        };
        assert!(failing(b.try_reserve(80 * MIB).unwrap()).is_err());
        assert_eq!(b.places.available_permits(), 100);
        let r = b.try_reserve(80 * MIB).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let joined = rt.block_on(async {
            tokio::task::spawn_blocking(move || {
                let _r = r;
                panic!("decoder");
            })
            .await
        });
        assert!(joined.is_err());
        assert_eq!(b.places.available_permits(), 100);
    }

    /// A quarter of the RAM; when it is not read, a quarter of 8 GiB.
    #[test]
    fn budget_from_ram() {
        assert_eq!(quarter(Some(16 * GIB)), 4 * GIB);
        assert_eq!(quarter(None), 2 * GIB);
        assert_eq!(meminfo_total("MemTotal:       32529440 kB\nMemFree: 1 kB\n"), Some(32529440 * 1024));
        assert_eq!(meminfo_total("MemFree: 1 kB\n"), None);
        assert_eq!(meminfo_total("MemTotal: x kB\n"), None);
        assert_eq!(Budget::new(0).total, 1);
        #[cfg(target_os = "linux")]
        assert!(ram().is_some_and(|r| r > GIB));
    }

    fn jpeg_bytes(w: u32, h: u32, lossless: bool) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let mut c = turbojpeg::Compressor::new().unwrap();
        c.set_lossless(lossless).unwrap();
        let pixels = turbojpeg::Image { pixels: img.as_raw().as_slice(), width: w as usize, pitch: w as usize * 3, height: h as usize, format: turbojpeg::PixelFormat::RGB };
        c.compress_to_vec(pixels).unwrap()
    }

    /// A JPEG read reduced reserves little; a lossless one, decoded whole, its whole picture.
    #[test]
    fn jpeg_estimates() {
        let (w, h) = (1200, 800);
        let px = u64::from(w * h);
        let small = jpeg_bytes(w, h, false);
        assert_eq!(estimate(&small, "a.jpg"), small.len() as u64 + times(px, JPEG_SCALED) + FIXED);
        let whole = jpeg_bytes(w, h, true);
        assert_eq!(estimate(&whole, "a.jpg"), whole.len() as u64 + times(px * 3, JPEG_WHOLE) + FIXED);
    }

    /// The other formats by the size their header says, and a 16-bit picture twice an 8-bit one.
    #[test]
    fn header_estimates() {
        let png = |img: image::DynamicImage| {
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let p8 = png(image::DynamicImage::new_rgb8(1000, 500));
        let p16 = png(image::DynamicImage::new_rgb16(1000, 500));
        assert_eq!(estimate(&p8, "a.png"), p8.len() as u64 + times(500_000 * 3, IMAGE) + FIXED);
        assert_eq!(estimate(&p16, "a.png"), p16.len() as u64 + times(500_000 * 6, IMAGE) + FIXED);
    }

    /// A PSD of 100 × 50 with `layers` layers, 8 or 16 bits, its sections empty but for the count.
    fn psd_bytes(bits: u16, layers: i16) -> Vec<u8> {
        let mut b = b"8BPS".to_vec();
        b.extend(1u16.to_be_bytes());
        b.extend([0; 6]);
        b.extend(3u16.to_be_bytes());
        b.extend(50u32.to_be_bytes());
        b.extend(100u32.to_be_bytes());
        b.extend(bits.to_be_bytes());
        b.extend(3u16.to_be_bytes());
        b.extend([0; 8]);
        b.extend(6u32.to_be_bytes());
        b.extend(2u32.to_be_bytes());
        b.extend(layers.to_be_bytes());
        b
    }

    /// Each layer of a PSD counts, and a 16-bit one (ImageMagick's) more than an 8-bit one.
    #[test]
    fn psd_estimates() {
        assert_eq!(crate::psd::layers(&psd_bytes(8, 0)), Some(0));
        assert_eq!(crate::psd::layers(&psd_bytes(8, -3)), Some(3));
        assert_eq!(crate::psd::layers(b"8BPS"), None);
        let flat = estimate(&psd_bytes(8, 0), "a.psd");
        let layered = estimate(&psd_bytes(8, 3), "a.psd");
        assert_eq!(layered - flat, times(5000, 4.0 * PSD + 3.0 * PSD_LAYER) - times(5000, 4.0 * PSD));
        assert!(estimate(&psd_bytes(16, 0), "a.psd") > flat);
        // Photoshop's 16-bit files: no layer info, the layers in an "Lr16" block after the mask,
        // behind another block.
        let mut b = psd_bytes(16, 0);
        b.truncate(b.len() - 10);
        b.extend(38u32.to_be_bytes());
        b.extend([0; 8]);
        b.extend(b"8BIMlnsr");
        b.extend(4u32.to_be_bytes());
        b.extend(b"rend");
        b.extend(b"8BIMLr16");
        b.extend(2u32.to_be_bytes());
        b.extend(5i16.to_be_bytes());
        assert_eq!(crate::psd::layers(&b), Some(5));
    }

    /// No size from the header: a RAW by its file, anything else at least `UNKNOWN_FLOOR`.
    #[test]
    fn unknown_sizes() {
        let raw = vec![0u8; 1000];
        assert_eq!(estimate(&raw, "a.cr3"), 1000 + 1000 * RAW_PER_BYTE + FIXED);
        assert_eq!(estimate(&raw, "a.xcf"), 1000 + UNKNOWN_FLOOR + FIXED);
        assert_eq!(estimate(&[], "a.heic"), UNKNOWN_FLOOR + FIXED);
    }

    /// Peak memory of ONE thumbnail made as `thumb_queue::generate` makes it, for the factors
    /// above: `EYECANSEE_PEAK_FILE=<file> cargo test --release thumb_peak_live -- --ignored
    /// --exact --nocapture`, one file per process, with a temporary `XDG_CACHE_HOME`. Linux only
    /// (VmHWM of /proc/self/status). The file's bytes are read before, so not in the delta.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn thumb_peak_live() {
        let status = |key: &str| -> u64 {
            let s = std::fs::read_to_string("/proc/self/status").unwrap();
            let line = s.lines().find(|l| l.starts_with(key)).unwrap();
            line.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() * 1024
        };
        let path = std::env::var("EYECANSEE_PEAK_FILE").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let file = crate::source::File { path: path.clone(), size: bytes.len() as u64, modified: String::new() };
        let need = estimate(&bytes, &path);
        let before = status("VmRSS:");
        let (img, own) = crate::decode::thumbnail_sized(&bytes, &path, crate::loader::THUMB_SIDE).unwrap();
        let info = crate::thumb_details::Info::of(&bytes, &path, (img.width(), img.height()), own);
        crate::thumbcache::save("/", &file, &img);
        let _ = crate::decode::to_color_image(img);
        let delta = status("VmHWM:").saturating_sub(before);
        let header = crate::decode::header(&bytes, &path).map(|h| (h.size, h.color.map(|c| (c.rgb, c.alpha, c.bits))));
        println!("PEAK\t{path}\tfile={}\tdelta={delta}\testimate={need}\tcovered={}\town={own:?}\tpixels={:?}\theader={header:?}", bytes.len(), need >= delta + bytes.len() as u64, info.pixels);
    }
}
