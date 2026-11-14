//! Whether a picture is animated or has several pages, from the first bytes of its file when they
//! tell it, without decoding it: the editor is off for those (`save::animated`), and Save as…
//! says that it saves only the picture shown.

use crate::encode::Format;

/// How much of a file is read first: the start of a PNG, WebP or AVIF always tells; a GIF or a
/// TIFF often does.
pub const HEAD: u64 = 256 * 1024;

/// Animated or of several pages, as `save::animated` says of the whole file, when `head` (the
/// start of the file `name`) is enough to tell; `None`: the rest of the file is needed.
pub fn from_head(head: &[u8], name: &str) -> Option<bool> {
    match Format::of_file(name) {
        Some(Format::Png) => png(head),
        Some(Format::Webp) => webp(head),
        Some(Format::Avif) => avif(head),
        Some(Format::Tiff) => tiff(head),
        Some(Format::Gif) => gif(head),
        // Not a format the editor turns down when animated.
        _ => Some(false),
    }
}

/// Can the file be animated or have pages? Only these are worth reading before the editor opens.
pub fn may_be(name: &str) -> bool {
    matches!(Format::of_file(name), Some(Format::Png | Format::Webp | Format::Avif | Format::Tiff | Format::Gif))
}

/// APNG: an acTL chunk before the first IDAT.
fn png(b: &[u8]) -> Option<bool> {
    if !b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut i = 8;
    loop {
        let len = u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize;
        match b.get(i + 4..i + 8)? {
            b"acTL" => return Some(true),
            b"IDAT" => return Some(false),
            _ => i += 12 + len,
        }
    }
}

/// The animation flag of the VP8X chunk; a simple WebP (VP8, VP8L) has one picture.
fn webp(b: &[u8]) -> Option<bool> {
    if b.get(..4)? != b"RIFF" || b.get(8..12)? != b"WEBP" {
        return None;
    }
    match b.get(12..16)? {
        b"VP8X" => Some(b.get(20)? & 0x02 != 0),
        b"VP8 " | b"VP8L" => Some(false),
        _ => None,
    }
}

/// An image sequence: brand "avis" in the ftyp box, the first of the file.
fn avif(b: &[u8]) -> Option<bool> {
    let size = u32::from_be_bytes(b.get(..4)?.try_into().ok()?) as usize;
    if b.get(4..8)? != b"ftyp" {
        return None;
    }
    Some(b.get(8..size)?.as_chunks::<4>().0.contains(b"avis"))
}

/// A second page: the first directory of the file pointing to another one.
fn tiff(b: &[u8]) -> Option<bool> {
    let le = match b.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        // BigTIFF and the like: read whole.
        _ => return None,
    };
    let u16_at = |i: usize| b.get(i..i + 2).map(|s| if le { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) });
    let u32_at = |i: usize| b.get(i..i + 4).map(|s| if le { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) } else { u32::from_be_bytes([s[0], s[1], s[2], s[3]]) });
    let first = u32_at(4)? as usize;
    let entries = u16_at(first)? as usize;
    Some(u32_at(first + 2 + entries * 12)? != 0)
}

/// A second image descriptor before the trailer.
fn gif(b: &[u8]) -> Option<bool> {
    if !(b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        return None;
    }
    let flags = *b.get(10)?;
    let mut i = 13 + if flags & 0x80 != 0 { 3 << ((flags & 7) + 1) } else { 0 };
    let mut images = 0;
    loop {
        match *b.get(i)? {
            // Extension: its label, then its blocks.
            0x21 => i = blocks(b, i + 2)?,
            0x2C => {
                images += 1;
                if images == 2 {
                    return Some(true);
                }
                let local = *b.get(i + 9)?;
                // Descriptor, colour table, the LZW code size, then the picture's blocks.
                i += 10 + if local & 0x80 != 0 { 3 << ((local & 7) + 1) } else { 0 };
                i = blocks(b, i + 1)?;
            }
            0x3B => return Some(false),
            _ => return None,
        }
    }
}

/// Past the data blocks starting at `i` (each its length, then the bytes; 0 ends them).
fn blocks(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let n = *b.get(i)? as usize;
        i += 1 + n;
        if n == 0 {
            return Some(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat};

    use super::*;
    use crate::encode::tests::{animated_webp, apng, tiff_pages};

    fn encoded(img: &DynamicImage, f: ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), f).unwrap();
        out
    }

    fn gif(frames: usize) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            for n in 0..frames {
                let rgba = image::RgbaImage::from_pixel(8, 8, image::Rgba([n as u8 * 80, 10, 10, 255]));
                enc.encode_frame(image::Frame::new(rgba)).unwrap();
            }
        }
        out
    }

    /// Every kind of file, still and animated: the start tells the same as the whole file
    /// (`save::animated`), or asks for the rest; never the opposite.
    #[test]
    fn the_start_tells_as_the_whole_file() {
        let img = DynamicImage::new_rgb8(8, 8);
        let png = encoded(&img, ImageFormat::Png);
        let files = [
            ("a.png", png.clone(), false),
            ("a.png", apng(&png), true),
            ("a.webp", encoded(&img, ImageFormat::WebP), false),
            ("a.webp", animated_webp(&img), true),
            ("a.tif", encoded(&img, ImageFormat::Tiff), false),
            ("a.tif", tiff_pages(&img), true),
            ("a.gif", gif(1), false),
            ("a.gif", gif(2), true),
            ("a.jpg", encoded(&img, ImageFormat::Jpeg), false),
        ];
        for (name, bytes, animated) in files {
            assert_eq!(crate::edit::save::animated(&bytes, name), animated, "{name} {animated}: the whole file");
            assert_eq!(from_head(&bytes, name), Some(animated), "{name} {animated}: all of it as the start");
            // Cut anywhere: the same answer or none.
            for cut in [0, 8, 16, 30, bytes.len() / 2] {
                let r = from_head(&bytes[..cut.min(bytes.len())], name);
                assert!(r.is_none() || r == Some(animated), "{name} {animated} cut at {cut}: {r:?}");
            }
        }
    }

    /// AVIF: a sequence by its brand, a still picture without it.
    #[test]
    fn avif_by_its_brand() {
        let ftyp = |brands: &[u8]| [&((8 + brands.len()) as u32).to_be_bytes()[..], b"ftyp", brands].concat();
        assert_eq!(from_head(&ftyp(b"avif\0\0\0\0avifmif1"), "a.avif"), Some(false));
        assert_eq!(from_head(&ftyp(b"avis\0\0\0\0avismsf1"), "a.avif"), Some(true));
        assert_eq!(from_head(b"\0\0", "a.avif"), None);
        assert!(may_be("a.GIF") && may_be("a.tiff") && !may_be("a.jpg") && !may_be("a.heic"));
    }
}
