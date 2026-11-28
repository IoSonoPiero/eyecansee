//! The frames of an animated image (GIF, WebP, APNG) and the pages of a multi-page TIFF, read for
//! the viewer, which plays or turns them (`frames_view.rs`). No egui here but its image type.

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::ColorImage;
use image::AnimationDecoder;

use crate::encode::Format;

/// Most memory the frames of one image may take, as decoded (4 bytes a pixel): beyond it the
/// viewer shows only the first, with a note. Up to five images around the open one are read ahead.
pub const MAX_BYTES: usize = 256 << 20;
/// Most pages of a TIFF read (audit 3.12): beyond them no more directories are read, and the
/// viewer shows the first page, as when too large. A thousand pages of 256 × 256 already reach
/// `MAX_BYTES`; a file of many empty pages took a thread for long.
pub const MAX_PAGES: usize = 1000;

/// A frame shorter than this is shown for `SLOW` instead, as the browsers do: GIFs saved with no
/// delay (0 or 10 ms) were meant to run at about ten frames a second.
const FAST: Duration = Duration::from_millis(20);
const SLOW: Duration = Duration::from_millis(100);

pub enum Sequence {
    /// The frames of an animation, each with how long it stays.
    Animation(Vec<(Arc<ColorImage>, Duration)>),
    /// The pages of a TIFF.
    Pages(Vec<Arc<ColorImage>>),
    /// More than one picture, too many to keep in memory: the first one only.
    TooLarge,
}

impl Sequence {
    /// How many pictures the viewer can show (0 for one too large).
    pub fn len(&self) -> usize {
        match self {
            Sequence::Animation(f) => f.len(),
            Sequence::Pages(p) => p.len(),
            Sequence::TooLarge => 0,
        }
    }

    pub fn image(&self, at: usize) -> Option<Arc<ColorImage>> {
        match self {
            Sequence::Animation(f) => f.get(at).map(|f| f.0.clone()),
            Sequence::Pages(p) => p.get(at).cloned(),
            Sequence::TooLarge => None,
        }
    }
}

/// The frames or pages of an image, when it has more than one; `None` for a single picture or a
/// format the viewer does not play.
pub fn read(bytes: &[u8], name: &str) -> Option<Sequence> {
    let cursor = || Cursor::new(bytes);
    let read = match Format::of_file(name)? {
        Format::Gif => image::codecs::gif::GifDecoder::new(cursor()).ok().and_then(sized).map(|d| animation(d.into_frames())),
        Format::Webp => image::codecs::webp::WebPDecoder::new(cursor()).ok().filter(|d| d.has_animation()).and_then(sized).map(|d| animation(d.into_frames())),
        Format::Png => image::codecs::png::PngDecoder::new(cursor()).ok().filter(|d| d.is_apng().unwrap_or(false)).and_then(sized).and_then(|d| d.apng().ok()).map(|d| animation(d.into_frames())),
        Format::Tiff => Some(pages(bytes, name)),
        _ => None,
    };
    match read? {
        Ok(Sequence::TooLarge) => Some(Sequence::TooLarge),
        Ok(s) if s.len() < 2 => None,
        Ok(s) => Some(s),
        Err(e) => {
            log!("frames of {name:?}: {e}");
            None
        }
    }
}

/// The decoder, when its canvas is within the ceiling (`pixel_limit`): the canvas is made before
/// the first frame.
fn sized<D: image::ImageDecoder>(mut decoder: D) -> Option<D> {
    crate::pixel_limit::checked(&mut decoder).inspect_err(|e| log!("frames: {e}")).ok()?;
    Some(decoder)
}

fn animation(frames: image::Frames<'_>) -> Result<Sequence, String> {
    let mut out = Vec::new();
    let mut size = 0;
    for frame in frames {
        let frame = frame.map_err(|e| e.to_string())?;
        let (num, den) = frame.delay().numer_denom_ms();
        let buffer = frame.into_buffer();
        size += buffer.len();
        if size > MAX_BYTES {
            return Ok(Sequence::TooLarge);
        }
        let image = ColorImage::from_rgba_unmultiplied([buffer.width() as usize, buffer.height() as usize], buffer.as_raw());
        out.push((Arc::new(image), delay(num, den)));
    }
    Ok(Sequence::Animation(out))
}

fn delay(num: u32, den: u32) -> Duration {
    let d = Duration::from_micros(num as u64 * 1000 / den.max(1) as u64);
    if d < FAST { SLOW } else { d }
}

/// The pages of a TIFF, each decoded as the viewer decodes a TIFF (`decode::decode`): the file
/// with its header pointing at that page's directory.
fn pages(bytes: &[u8], name: &str) -> Result<Sequence, String> {
    let offsets = page_offsets(bytes)?;
    if offsets.len() < 2 {
        return Ok(Sequence::Pages(Vec::new()));
    }
    if offsets.len() > MAX_PAGES {
        return Ok(Sequence::TooLarge);
    }
    // One copy of the file for all the pages, its header pointed at each in turn (audit 3.12).
    let mut page = bytes.to_vec();
    #[cfg(test)]
    COPIED.with(|c| c.set(c.get() + bytes.len()));
    let mut out = Vec::new();
    let mut size = 0;
    for offset in offsets {
        point_at(&mut page, offset).ok_or("not a TIFF header")?;
        let image = crate::decode::decode(&page, name)?;
        size += image.pixels.len() * 4;
        if size > MAX_BYTES {
            return Ok(Sequence::TooLarge);
        }
        out.push(Arc::new(image));
    }
    Ok(Sequence::Pages(out))
}

/// Where the directory of each page is, in order; one more than `MAX_PAGES` at most.
fn page_offsets(bytes: &[u8]) -> Result<Vec<u64>, String> {
    let mut d = tiff::decoder::Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut out: Vec<u64> = d.ifd_pointer().map(|p| p.0).into_iter().collect();
    while d.more_images() && out.len() <= MAX_PAGES {
        d.next_image().map_err(|e| e.to_string())?;
        out.extend(d.ifd_pointer().map(|p| p.0));
    }
    Ok(out)
}

/// The TIFF `bytes` with its header pointed at the directory at `offset`: read as a single
/// picture, it is that page.
fn point_at(bytes: &mut [u8], offset: u64) -> Option<()> {
    let little = match bytes.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let magic = bytes.get(2..4)?;
    let magic = if little { u16::from_le_bytes([magic[0], magic[1]]) } else { u16::from_be_bytes([magic[0], magic[1]]) };
    match magic {
        42 => {
            let o = u32::try_from(offset).ok()?;
            bytes.get_mut(4..8)?.copy_from_slice(&if little { o.to_le_bytes() } else { o.to_be_bytes() });
        }
        43 => bytes.get_mut(8..16)?.copy_from_slice(&if little { offset.to_le_bytes() } else { offset.to_be_bytes() }),
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
thread_local! {
    /// Tests only: bytes of the file copied on this thread to read its pages (audit 3.12).
    static COPIED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Delay, Frame, ImageEncoder, Rgba, RgbaImage};

    fn frame(colour: [u8; 4], ms: u32) -> Frame {
        Frame::from_parts(RgbaImage::from_pixel(8, 6, Rgba(colour)), 0, 0, Delay::from_numer_denom_ms(ms, 1))
    }

    fn gif(frames: Vec<Frame>) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.encode_frames(frames).unwrap();
        }
        out
    }

    #[test]
    fn gif_frames_and_delays() {
        let bytes = gif(vec![frame([255, 0, 0, 255], 200), frame([0, 255, 0, 255], 0), frame([0, 0, 255, 255], 50)]);
        let Some(Sequence::Animation(f)) = read(&bytes, "a.gif") else { panic!("not an animation") };
        assert_eq!(f.len(), 3);
        assert_eq!(f[0].1, Duration::from_millis(200));
        // No delay: shown as the browsers show it.
        assert_eq!(f[1].1, SLOW);
        assert_eq!(f[2].1, Duration::from_millis(50));
        assert_eq!(f[1].0.size, [8, 6]);
        assert_eq!(f[1].0.pixels[0], eframe::egui::Color32::from_rgb(0, 255, 0));
    }

    #[test]
    fn single_gif_is_not_a_sequence() {
        let bytes = gif(vec![frame([255, 0, 0, 255], 100)]);
        assert!(read(&bytes, "a.gif").is_none());
    }

    #[test]
    fn plain_png_is_not_a_sequence() {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes).write_image(&[0u8; 4 * 4 * 3], 4, 4, image::ExtendedColorType::Rgb8).unwrap();
        assert!(read(&bytes, "a.png").is_none());
    }

    #[test]
    fn huge_animation_is_too_large() {
        // 2048×2048 RGBA is 16 MB a frame: past MAX_BYTES before the last.
        let n = MAX_BYTES / (2048 * 2048 * 4) + 1;
        let big = (0..n).map(|i| Frame::from_parts(RgbaImage::from_pixel(2048, 2048, Rgba([i as u8, 0, 0, 255])), 0, 0, Delay::from_numer_denom_ms(100, 1)));
        let bytes = gif(big.collect());
        assert!(matches!(read(&bytes, "big.gif"), Some(Sequence::TooLarge)));
    }

    fn tiff_pages(pages: &[(u32, u32, u8)]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let mut enc = tiff::encoder::TiffEncoder::new(&mut out).unwrap();
        for &(w, h, v) in pages {
            let data = vec![v; (w * h * 3) as usize];
            enc.write_image::<tiff::encoder::colortype::RGB8>(w, h, &data).unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn tiff_pages_each_decoded() {
        let bytes = tiff_pages(&[(4, 3, 10), (5, 2, 200), (2, 2, 90)]);
        let Some(Sequence::Pages(p)) = read(&bytes, "scan.tif") else { panic!("not pages") };
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].size, [4, 3]);
        assert_eq!(p[1].size, [5, 2]);
        assert_eq!(p[1].pixels[0], eframe::egui::Color32::from_gray(200));
        assert_eq!(p[2].pixels[0], eframe::egui::Color32::from_gray(90));
    }

    #[test]
    fn single_page_tiff_is_not_a_sequence() {
        assert!(read(&tiff_pages(&[(4, 3, 10)]), "one.tiff").is_none());
    }

    #[test]
    fn big_endian_header_patched() {
        let mut b = vec![b'M', b'M', 0, 42, 0, 0, 0, 8];
        b.extend([0; 8]);
        point_at(&mut b, 0x0102_0304).unwrap();
        assert_eq!(&b[4..8], &[1, 2, 3, 4]);
        assert!(point_at(&mut b"XX\0\0".to_vec(), 8).is_none());
    }

    /// Audit 3.12: the whole file was copied for each page, only to point its header at that
    /// page (a scan of 200 pages in 300 MB: 60 GB copied). Now once for all of them.
    #[test]
    fn tiff_copied_once_for_all_its_pages() {
        let bytes = tiff_pages(&[(4, 3, 10); 20]);
        let before = COPIED.with(std::cell::Cell::get);
        let Some(Sequence::Pages(p)) = read(&bytes, "scan.tif") else { panic!("not pages") };
        assert_eq!(p.len(), 20);
        let copied = COPIED.with(std::cell::Cell::get) - before;
        assert!(copied <= bytes.len(), "{copied} bytes copied for a file of {}: {:.0} times", bytes.len(), copied as f64 / bytes.len() as f64);
    }

    /// Audit 3.12: no ceiling on the pages: a small file of thousands of empty-ish pages kept a
    /// thread reading and decoding every one of them. Past the ceiling (1000 pages) no more
    /// directories are read, and the viewer shows the first page with its note.
    #[test]
    fn tiff_pages_have_a_ceiling() {
        let bytes = tiff_pages(&[(1, 1, 10); 3000]);
        let read_dirs = page_offsets(&bytes).unwrap().len();
        assert!(read_dirs <= MAX_PAGES + 1, "{read_dirs} directories read");
        assert!(matches!(read(&bytes, "many.tif"), Some(Sequence::TooLarge)));
    }

    /// Audit 3.12: the ceiling on the pages turns down no TIFF of the test files (any file with a
    /// TIFF header, RAW ones too): `EYECANSEE_TIFF_SAMPLES=notes/test-files cargo test --release
    /// tiff_pages_of_the_samples -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn tiff_pages_of_the_samples() {
        let root = std::path::PathBuf::from(std::env::var_os("EYECANSEE_TIFF_SAMPLES").expect("EYECANSEE_TIFF_SAMPLES"));
        let (mut seen, mut most) = (0, (0, String::new()));
        let mut dirs = vec![root];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else { continue };
                if !(bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") || bytes.starts_with(b"II+\0") || bytes.starts_with(b"MM\0+")) {
                    continue;
                }
                let Ok(offsets) = page_offsets(&bytes) else { continue };
                seen += 1;
                println!("{}: {} pages", path.display(), offsets.len());
                if offsets.len() > most.0 {
                    most = (offsets.len(), path.display().to_string());
                }
                assert!(offsets.len() <= MAX_PAGES, "{}: {} pages, past the ceiling", path.display(), offsets.len());
            }
        }
        println!("{seen} TIFF files, the most pages {} in {}", most.0, most.1);
        assert!(seen > 0);
    }

    /// Audit 3.4: a GIF of 86 bytes whose screen is 65535 x 65535 does not have its first canvas
    /// (17 GB) made before the size is looked at.
    #[test]
    fn audit_3_4_gif_screen() {
        let mut bytes = gif(vec![frame([255, 0, 0, 255], 100), frame([0, 255, 0, 255], 100)]);
        // Logical screen: width and height, little endian, after "GIF89a".
        bytes[6..10].copy_from_slice(&[0xFF; 4]);
        assert!(matches!(read(&bytes, "a.gif"), None | Some(Sequence::TooLarge)));
    }
}
