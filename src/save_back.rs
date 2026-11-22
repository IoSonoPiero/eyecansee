//! Writing an image back over its original in its own format, without the interface: the
//! lossless turns that "Save" writes, the re-encoding after an edit, and whether a file can be
//! written back at all. Split from `encode.rs` on 2026-11-05.

use std::io::Cursor;

use image::DynamicImage;
use image::codecs::{gif, png};

use crate::encode::{Format, Options, Original};
use crate::transform::Turn;

/// The file turned, in its own format: a JPEG without re-encoding when its size allows it,
/// otherwise at quality 95; the others encoded again, a lossless WebP still lossless. Also says
/// whether it was encoded again with loss (the confirmation warns about it).
pub fn turned_file(bytes: &[u8], name: &str, turn: Turn) -> Result<(Vec<u8>, bool), String> {
    turned_file_beside(bytes, name, turn, None)
}

/// As `turned_file`, for a copy (Convert and resize, the editor's copy): with the XMP of the
/// photo's `sidecar` (#24) joined to the file's own, as in the copies encoded again.
pub fn turned_file_beside(bytes: &[u8], name: &str, turn: Turn, sidecar: Option<&[u8]>) -> Result<(Vec<u8>, bool), String> {
    // A JPEG too large to open is not turned either: libjpeg-turbo would ask for its memory.
    if Format::of_file(name) == Some(Format::Jpeg)
        && let Some(h) = crate::decode::header(bytes, name)
    {
        crate::pixel_limit::check(h.size[0].into(), h.size[1].into())?;
    }
    // A phone photo with an HDR gain map, or a Motion Photo: all of it turned, HDR and video kept.
    if Format::of_file(name) == Some(Format::Jpeg)
        && let Some(r) = crate::ultrahdr::turned(bytes, turn, sidecar)
    {
        return r;
    }
    if Format::of_file(name) == Some(Format::Jpeg)
        && can_turn_in_place(bytes, name)
        && !turn_decodes(bytes, name, turn)
        && let Some(out) = crate::transform::jpeg_lossless(bytes, turn)
    {
        let out = match sidecar {
            Some(side) => crate::meta_write::jpeg_with_sidecar(out, side),
            None => out,
        };
        return Ok((out, false));
    }
    let mut img = crate::decode::decode_full(bytes, name)?;
    turn.apply(&mut img);
    encoded_beside(bytes, name, img, sidecar, turn)
}

/// Whether turning the file `bytes` (all of it) by `turn`, as `turned_file_beside` does it,
/// decodes its picture, and so needs the memory of a decoding (#177): every format but JPEG, and
/// a JPEG not turned without re-encoding (its size not whole blocks for that turn, or carrying
/// more than one picture). A phone photo (Ultra HDR, Motion Photo) by its photo: its gain map,
/// a sixteenth of it, is not counted. Save over the original and Convert ask the same.
pub fn turn_decodes(bytes: &[u8], name: &str, turn: Turn) -> bool {
    let jpeg = Format::of_file(name) == Some(Format::Jpeg);
    let whole = jpeg && (can_turn_in_place(bytes, name) || crate::ultrahdr::turns_whole(bytes));
    !(whole && crate::transform::jpeg_lossless_fits(bytes, turn.after(crate::transform::exif_turn(bytes))))
}

/// The memory turning `bytes` (the start of a file of `size` bytes, or all of it) needs, when
/// the turn decodes them (`turn_decodes`) and that is over half of `ram` (`view_memory`, Piero,
/// 21/11/2026). Of a JPEG only all of it can tell: from its start, nothing yet.
pub fn turn_too_large(bytes: &[u8], size: u64, name: &str, turn: Turn, ram: u64) -> Option<u64> {
    if Format::of_file(name) == Some(Format::Jpeg) && (bytes.len() as u64) < size || !turn_decodes(bytes, name, turn) {
        return None;
    }
    crate::view_memory::estimate(bytes, size, name, crate::view_memory::Use::Whole).over_half(bytes.len(), size, name, ram)
}

/// The file decoded, changed by `edit` and encoded again in its own format: a JPEG at its own
/// quality and subsampling (`encode::jpeg_quality`), a lossless WebP still lossless. Also says whether it lost quality (the confirmation warns about it).
#[cfg(test)]
pub fn edited_file(bytes: &[u8], name: &str, edit: impl FnOnce(&mut DynamicImage)) -> Result<(Vec<u8>, bool), String> {
    let mut img = crate::decode::decode_full(bytes, name)?;
    edit(&mut img);
    encoded_back(bytes, name, img)
}

/// `image` (the file `bytes`, already decoded and changed) encoded again in the file's format,
/// with its metadata, as `edited_file`; the file itself is not decoded again.
#[cfg(test)]
pub fn encoded_back(bytes: &[u8], name: &str, image: DynamicImage) -> Result<(Vec<u8>, bool), String> {
    encoded_beside(bytes, name, image, None, Turn::default())
}

/// As `encoded_back`, with the XMP of the photo's `sidecar` added (#24): for a copy in the same
/// format (Convert and resize), not for the file itself, which keeps its sidecar beside it.
/// `turn`: how `image` was turned from the file (upside down, the Photo Sphere tags go, #188).
pub fn encoded_beside(bytes: &[u8], name: &str, image: DynamicImage, sidecar: Option<&[u8]>, turn: Turn) -> Result<(Vec<u8>, bool), String> {
    // A PSD comes back flattened: "lossy" when there was more than one plain layer.
    if crate::psd::is_psd(name) {
        return Ok((crate::psd::encode(&image, bytes)?, !crate::psd::simple(bytes)));
    }
    let format = Format::of_file(name).filter(|_| can_turn_in_place(bytes, name)).ok_or_else(|| name.to_string())?;
    let webp_lossless = format == Format::Webp && bytes.windows(4).any(|w| w == b"VP8L");
    // A JPEG at its own quality and subsampling: not compressed harder a second time.
    let o = Options { format, jpeg_quality: 95, webp_lossless, ..Default::default() };
    let o = if format == Format::Jpeg { crate::encode::jpeg_quality::like(bytes, o) } else { o };
    // GIF too: its colours are chosen again (at most 256).
    let lossy = matches!(format, Format::Jpeg | Format::Avif | Format::Gif) || (format == Format::Webp && !webp_lossless);
    Ok((Original::with_meta_beside(image, bytes, name, sidecar).upside(turn).encode(&o)?, lossy))
}

/// Can the file be turned and written back over itself, losing at most a little quality? Its
/// format must be one EyeCanSee writes, and the file a single picture: not animated (APNG, WebP,
/// AVIF sequence), not several pages (TIFF), not a JPEG carrying more than the picture (the video
/// of a Motion Photo after its end, MPF secondary images or depth maps), which would be lost.
pub fn can_turn_in_place(bytes: &[u8], name: &str) -> bool {
    if crate::psd::is_psd(name) {
        return crate::psd::writable();
    }
    let cursor = || Cursor::new(bytes);
    match Format::of_file(name) {
        Some(Format::Jpeg) => plain_jpeg(bytes),
        Some(Format::Png) => png::PngDecoder::new(cursor()).is_ok_and(|d| d.is_apng().is_ok_and(|a| !a)),
        Some(Format::Webp) => image::codecs::webp::WebPDecoder::new(cursor()).is_ok_and(|d| !d.has_animation()),
        Some(Format::Tiff) => tiff::decoder::Decoder::new(cursor()).is_ok_and(|d| !d.more_images()),
        Some(Format::Avif) => !avif_sequence(bytes),
        // Only the first frame is decoded: an animated GIF would lose the others.
        // A screen beyond the ceiling is not made (`pixel_limit`): such a file does not even open.
        Some(Format::Gif) => gif::GifDecoder::new(cursor()).is_ok_and(|mut d| crate::pixel_limit::checked(&mut d).is_ok() && image::AnimationDecoder::into_frames(d).take(2).count() == 1),
        Some(_) => true,
        None => false,
    }
}

/// A JPEG that ends at its EOI marker (zeros after it are padding) and has no MPF segment.
/// libjpeg-turbo drops what follows EOI (tried: 27 bytes after it, gone after a turn).
fn plain_jpeg(b: &[u8]) -> bool {
    jpeg_end(b).is_some_and(|(end, mpf)| !mpf && b[end..].iter().all(|&x| x == 0))
}

/// Where the picture of a JPEG ends (just after its EOI marker), and whether it has an MPF
/// segment (more pictures after it). `None` if it is not a JPEG that ends.
pub(crate) fn jpeg_end(b: &[u8]) -> Option<(usize, bool)> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let (mut i, mut mpf) = (2, false);
    while i + 1 < b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        match marker {
            0xFF => {
                i += 1; // fill byte
                continue;
            }
            0xD9 => return Some((i + 2, mpf)),
            0x01 | 0xD0..=0xD7 => {
                i += 2;
                continue;
            }
            _ => {}
        }
        let len = b.get(i + 2..i + 4).map(|l| u16::from_be_bytes([l[0], l[1]]) as usize).filter(|l| *l >= 2)?;
        mpf |= marker == 0xE2 && b.get(i + 4..i + 2 + len).is_some_and(|body| body.starts_with(b"MPF\0"));
        i += 2 + len;
        if marker == 0xDA {
            // Coded data, up to the next marker: not a stuffed 0xFF00, not a restart.
            while i + 1 < b.len() && !(b[i] == 0xFF && b[i + 1] != 0 && !(0xD0..=0xD7).contains(&b[i + 1])) {
                i += 1;
            }
        }
    }
    None
}

/// AVIF image sequence: brand "avis" in the ftyp box.
fn avif_sequence(b: &[u8]) -> bool {
    let size = b.get(..4).map_or(0, |s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize);
    b.get(4..8) == Some(b"ftyp") && b.get(8..size.min(b.len())).is_some_and(|brands| brands.as_chunks::<4>().0.contains(b"avis"))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::codecs::{gif, png};
    use image::DynamicImage;

    use super::can_turn_in_place;
    use crate::encode::tests::{animated_webp, apng, tiff_pages};

    /// What cannot be saved over itself, found inside the file: animations, pages, what a JPEG
    /// carries besides its picture. The plain files of the same formats can.
    #[test]
    fn turn_in_place() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(8, 8, |x, y| image::Rgb([x as u8 * 30, y as u8 * 30, 0])));
        let file = |f| {
            let mut v = Vec::new();
            img.write_to(&mut Cursor::new(&mut v), f).unwrap();
            v
        };
        // JPEG: plain, with zero padding, with a video after EOI, with an MPF segment.
        let jpeg = file(image::ImageFormat::Jpeg);
        assert!(can_turn_in_place(&jpeg, "a.jpg"));
        assert!(can_turn_in_place(&[jpeg.as_slice(), &[0; 16]].concat(), "a.jpeg"));
        assert!(!can_turn_in_place(&[jpeg.as_slice(), b"\0\0\0\x18ftypmp42"].concat(), "a.jpg"));
        let mut mpf = jpeg.clone();
        mpf.splice(2..2, [&[0xFF, 0xE2, 0, 10][..], b"MPF\0II*\0"].concat());
        assert!(!can_turn_in_place(&mpf, "a.jpg"));

        let png = file(image::ImageFormat::Png);
        assert!(can_turn_in_place(&png, "a.png"));
        let apng = apng(&png);
        assert!(png::PngDecoder::new(Cursor::new(&apng)).is_ok_and(|d| d.is_apng().unwrap()));
        assert!(!can_turn_in_place(&apng, "a.png"));

        assert!(can_turn_in_place(&file(image::ImageFormat::WebP), "a.webp"));
        let animated = animated_webp(&img);
        assert!(image::codecs::webp::WebPDecoder::new(Cursor::new(&animated)).is_ok_and(|d| d.has_animation()));
        assert!(!can_turn_in_place(&animated, "a.webp"));

        // TIFF of two pages.
        assert!(can_turn_in_place(&file(image::ImageFormat::Tiff), "a.tif"));
        assert!(!can_turn_in_place(&tiff_pages(&img), "a.tiff"));

        // AVIF sequence: brand "avis".
        let ftyp = |major: &[u8]| [&24u32.to_be_bytes()[..], b"ftyp", major, &[0; 4], major, b"mif1"].concat();
        assert!(can_turn_in_place(&ftyp(b"avif"), "a.avif"));
        assert!(!can_turn_in_place(&ftyp(b"avis"), "a.avif"));

        assert!(can_turn_in_place(&file(image::ImageFormat::Gif), "a.gif"));
        let mut animated_gif = Vec::new();
        let frame = || image::Frame::new(img.to_rgba8());
        gif::GifEncoder::new(&mut animated_gif).encode_frames([frame(), frame()]).unwrap();
        assert!(!can_turn_in_place(&animated_gif, "a.gif"));
        assert!(!can_turn_in_place(&jpeg, "a.heic"));
    }

    /// A JPEG named `.jfif` or `.jpe` is saved back as a JPEG: turned without re-encoding, still
    /// a JPEG (before, Save refused it: only `.jpg` and `.jpeg` were known).
    #[test]
    fn other_jpeg_names() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(32, 16, |x, y| image::Rgb([x as u8 * 8, y as u8 * 16, 0])));
        let mut jpeg = Vec::new();
        img.write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        for name in ["a.jfif", "a.JPE"] {
            assert!(can_turn_in_place(&jpeg, name), "{name}");
            let (out, lossy) = super::turned_file(&jpeg, name, crate::transform::Turn::default().then(crate::transform::Op::Cw)).unwrap();
            assert!(!lossy, "{name}");
            assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Jpeg, "{name}");
            assert_eq!(crate::decode::decode_full(&out, name).unwrap().width(), 16, "{name}");
        }
    }

    /// A JPEG of `w` × `h` with `subsamp`, its EXIF orientation 6 when `exif6`.
    fn jpeg_of(w: usize, h: usize, subsamp: turbojpeg::Subsamp, exif6: bool) -> Vec<u8> {
        let rgb: Vec<u8> = (0..w * h * 3).map(|i| (i * 7 % 251) as u8).collect();
        let mut c = turbojpeg::Compressor::new().unwrap();
        c.set_subsamp(subsamp).unwrap();
        let mut jpeg = c.compress_to_vec(turbojpeg::Image { pixels: rgb.as_slice(), width: w, pitch: w * 3, height: h, format: turbojpeg::PixelFormat::RGB }).unwrap();
        if exif6 {
            let field = exif::Field { tag: exif::Tag::Orientation, ifd_num: exif::In::PRIMARY, value: exif::Value::Short(vec![6]) };
            let tiff = crate::meta::write_exif(&[field], true).unwrap();
            let app1 = [&[0xFF, 0xE1][..], &((2 + 6 + tiff.len()) as u16).to_be_bytes(), b"Exif\0\0", &tiff].concat();
            jpeg.splice(2..2, app1);
        }
        jpeg
    }

    /// Every turn (with the EXIF orientation done too) × JPEGs of whole blocks or not on each
    /// side, at 4:4:4, 4:2:2 and 4:2:0, a phone's Ultra HDR photo: `turn_decodes` says what
    /// `turned_file` does, a turn without re-encoding when it says no (#177: only a decoding asks
    /// for its memory). Every other format decodes.
    #[test]
    fn turn_decodes_as_turned_file() {
        use crate::transform::{Op, Turn};
        let t = Turn::default();
        let turns = [t, t.then(Op::Cw), t.then(Op::Half), t.then(Op::Ccw), t.then(Op::FlipH), t.then(Op::FlipV), t.then(Op::FlipH).then(Op::Cw), t.then(Op::FlipH).then(Op::Ccw)];
        let (mut lossless, mut decoded) = (0, 0);
        for subsamp in [turbojpeg::Subsamp::None, turbojpeg::Subsamp::Sub2x1, turbojpeg::Subsamp::Sub2x2] {
            for (w, h) in [(32, 32), (36, 32), (32, 40), (40, 36)] {
                for exif6 in [false, true] {
                    let jpeg = jpeg_of(w, h, subsamp, exif6);
                    for turn in turns {
                        let (_, lossy) = super::turned_file(&jpeg, "a.jpg", turn).unwrap();
                        assert_eq!(super::turn_decodes(&jpeg, "a.jpg", turn), lossy, "{subsamp:?} {w}x{h} exif6 {exif6} {turn:?}");
                        if lossy { decoded += 1 } else { lossless += 1 }
                    }
                }
            }
        }
        assert!(lossless > 0 && decoded > 0, "{lossless} {decoded}");
        // Photo and gain map (half its sides) both of whole blocks, or neither: the gain map is
        // not counted (`turn_decodes`), the `lossy` of `turned` is of either.
        for (w, h) in [(64, 48), (68, 44)] {
            let hdr = crate::ultrahdr::tests::sample(w, h);
            for turn in turns {
                let (_, lossy) = crate::ultrahdr::turned(&hdr, turn, None).unwrap().unwrap();
                assert_eq!(super::turn_decodes(&hdr, "a.jpg", turn), lossy, "Ultra HDR {w}x{h} {turn:?}");
            }
        }
        let png = {
            let mut v = Vec::new();
            DynamicImage::new_rgb8(8, 8).write_to(&mut Cursor::new(&mut v), image::ImageFormat::Png).unwrap();
            v
        };
        assert!(super::turn_decodes(&png, "a.png", t.then(Op::Cw)));
    }

    /// Save and Convert ask for the memory of a turn only when it decodes, and of a JPEG only from
    /// all of it: a JPEG turned without re-encoding never; one that is encoded again, a PNG,
    /// when over half the RAM.
    #[test]
    fn turn_too_large_only_when_decoded() {
        use crate::transform::{Op, Turn};
        let cw = Turn::default().then(Op::Cw);
        let (fits, not) = (jpeg_of(32, 32, turbojpeg::Subsamp::Sub2x2, false), jpeg_of(32, 40, turbojpeg::Subsamp::Sub2x2, false));
        let len = |b: &[u8]| b.len() as u64;
        let need = |b: &[u8], name| crate::view_memory::estimate(b, len(b), name, crate::view_memory::Use::Whole).need;
        assert_eq!(super::turn_too_large(&fits, len(&fits), "a.jpg", cw, 0), None, "without decoding");
        assert_eq!(super::turn_too_large(&not, len(&not), "a.jpg", cw, 0), Some(need(&not, "a.jpg")));
        assert_eq!(super::turn_too_large(&not, len(&not), "a.jpg", cw, 1 << 40), None, "room enough");
        assert_eq!(super::turn_too_large(&not, len(&not) + 1, "a.jpg", cw, 0), None, "only its start");
        let mut png = Vec::new();
        DynamicImage::new_rgb8(30, 20).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        assert_eq!(super::turn_too_large(&png, len(&png), "a.png", cw, 0), Some(need(&png, "a.png")));
    }

    /// Audit 3.4: the check made at every opening ("can this file be turned in place?") decodes
    /// the first frames of a GIF: one whose screen is 65535 x 65535 is answered "no" without
    /// asking for its 17 GB.
    #[test]
    fn audit_3_4_gif_turn_check() {
        let mut bytes = Vec::new();
        {
            let frame = || image::Frame::new(image::RgbaImage::from_pixel(8, 6, image::Rgba([255, 0, 0, 255])));
            image::codecs::gif::GifEncoder::new(&mut bytes).encode_frames(vec![frame(), frame()]).unwrap();
        }
        bytes[6..10].copy_from_slice(&[0xFF; 4]);
        assert!(!can_turn_in_place(&bytes, "a.gif"));
    }
}
