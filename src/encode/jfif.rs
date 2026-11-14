//! The JFIF density of a JPEG (its APP0 segment: unit and density across and down), read from an
//! original JPEG and written into every JPEG made from it. libjpeg-turbo writes 1:1 without unit:
//! a 600 dpi scan saved again came out with no unit at all.

/// Unit (0 none: the two numbers are only an aspect ratio, 1 dots per inch, 2 dots per
/// centimetre) and density across (`x`) and down (`y`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Density {
    pub unit: u8,
    pub x: u16,
    pub y: u16,
}

/// Where the unit is in the APP0 segment: marker (2 bytes), length (2), "JFIF\0" (5), version (2).
/// The two densities follow it, two bytes each, big-endian.
const UNIT: usize = 11;

/// Start of the JFIF APP0 segment of a JPEG, if it has one long enough to hold the density.
fn app0(bytes: &[u8]) -> Option<usize> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    crate::ultrahdr::segments(bytes).into_iter().find(|&(m, s, e)| m == 0xE0 && e >= s + UNIT + 5 && bytes[s + 4..].starts_with(b"JFIF\0")).map(|(_, s, _)| s)
}

/// The density of a JPEG; `None` when `bytes` is not a JPEG, has no JFIF segment, or one whose
/// values the specification does not allow (zero density, unknown unit).
pub fn read(bytes: &[u8]) -> Option<Density> {
    let s = app0(bytes)?;
    let at = |i: usize| u16::from_be_bytes([bytes[s + UNIT + i], bytes[s + UNIT + i + 1]]);
    let d = Density { unit: bytes[s + UNIT], x: at(1), y: at(3) };
    (d.unit <= 2 && d.x > 0 && d.y > 0).then_some(d)
}

/// `d` written into the JFIF segment the encoder made; a JPEG without one (an RGB JPEG with an
/// Adobe segment, where JFIF would be wrong) is left as it is.
pub fn set(jpeg: &mut [u8], d: Density) {
    if let Some(s) = app0(jpeg) {
        jpeg[s + UNIT] = d.unit;
        jpeg[s + UNIT + 1..s + UNIT + 3].copy_from_slice(&d.x.to_be_bytes());
        jpeg[s + UNIT + 3..s + UNIT + 5].copy_from_slice(&d.y.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{Format, Options, Original};
    use crate::transform::Turn;
    use image::DynamicImage;

    /// What libjpeg-turbo writes when nothing is carried over.
    const PLAIN: Density = Density { unit: 0, x: 1, y: 1 };

    fn photo(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 5) as u8, (y * 5) as u8, 90])))
    }

    fn jpeg_options() -> Options {
        Options { format: Format::Jpeg, ..Options::default() }
    }

    /// A JPEG written by EyeCanSee with the density `d`, as a scanner or a camera would leave it.
    fn jpeg_with(d: Density) -> Vec<u8> {
        let mut bytes = Original::with_meta(photo(40, 30), &[]).encode(&jpeg_options()).unwrap();
        set(&mut bytes, d);
        assert_eq!(read(&bytes), Some(d));
        bytes
    }

    /// The JPEG `bytes` decoded and written again as JPEG, as Save, Save as and Convert do.
    fn again(bytes: &[u8], o: &Options) -> Vec<u8> {
        Original::with_meta(crate::decode::decode_full(bytes, "a.jpg").unwrap(), bytes).alone().encode(o).unwrap()
    }

    /// 600 dpi, as the Orchid test photo: kept by Save as JPEG, by a resize (the editor's and
    /// Convert and resize's), with any quality or subsampling, and also in lossless JPEG.
    #[test]
    fn inches_600() {
        let d = Density { unit: 1, x: 600, y: 600 };
        let bytes = jpeg_with(d);
        assert_eq!(read(&again(&bytes, &jpeg_options())), Some(d));
        let resized = Original::with_meta(photo(40, 30), &bytes).with_image(photo(20, 15));
        assert_eq!(read(&resized.encode(&jpeg_options()).unwrap()), Some(d));
        let o = crate::encode::jpeg_quality::like(&bytes, jpeg_options());
        assert_eq!(read(&again(&bytes, &o)), Some(d));
        let lossless = again(&bytes, &Options { jpeg_lossless: true, ..jpeg_options() });
        assert!(read(&lossless).is_none_or(|got| got == d));
    }

    #[test]
    fn inches_72() {
        let d = Density { unit: 1, x: 72, y: 72 };
        assert_eq!(read(&again(&jpeg_with(d), &jpeg_options())), Some(d));
    }

    /// Dots per centimetre, different across and down: unit and both numbers kept as they were.
    #[test]
    fn centimetres() {
        let d = Density { unit: 2, x: 118, y: 59 };
        assert_eq!(read(&again(&jpeg_with(d), &jpeg_options())), Some(d));
        let grey = Options { chroma: crate::encode::Chroma::Grey, ..jpeg_options() };
        assert_eq!(read(&again(&jpeg_with(d), &grey)), Some(d));
    }

    /// Without its metadata the file gets libjpeg-turbo's 1:1, as before.
    #[test]
    fn not_without_metadata() {
        let bytes = jpeg_with(Density { unit: 1, x: 600, y: 600 });
        assert_eq!(read(&again(&bytes, &Options { keep_meta: false, ..jpeg_options() })), Some(PLAIN));
    }

    /// A source without JFIF segment (as phones write their photos), a PNG, a damaged density:
    /// what libjpeg-turbo writes, as before; and a written PNG does not change.
    #[test]
    fn source_without_jfif() {
        let mut bytes = jpeg_with(Density { unit: 1, x: 300, y: 300 });
        let (_, s, e) = crate::ultrahdr::segments(&bytes).into_iter().find(|&(m, ..)| m == 0xE0).unwrap();
        bytes.drain(s..e);
        assert_eq!(read(&bytes), None);
        assert_eq!(read(&again(&bytes, &jpeg_options())), Some(PLAIN));
        let png = Original::with_meta(photo(40, 30), &[]).encode(&Options { format: Format::Png, ..Options::default() }).unwrap();
        let from_png = Original::with_meta(photo(40, 30), &png).encode(&jpeg_options()).unwrap();
        assert_eq!(read(&from_png), Some(PLAIN));
        let mut zero = jpeg_with(Density { unit: 1, x: 300, y: 300 });
        set(&mut zero, Density { unit: 1, x: 0, y: 300 });
        assert_eq!(read(&zero), None);
        assert_eq!(read(&again(&zero, &jpeg_options())), Some(PLAIN));
        let jpeg = jpeg_with(Density { unit: 1, x: 600, y: 600 });
        let png_again = Original::with_meta(photo(40, 30), &jpeg).encode(&Options { format: Format::Png, ..Options::default() }).unwrap();
        assert_eq!(png_again, png);
    }

    /// An Ultra HDR photo written again as JPEG (Convert and resize, Save as, the editor) keeps
    /// the density of its photo.
    #[test]
    fn ultra_hdr() {
        let d = Density { unit: 1, x: 600, y: 600 };
        let mut file = crate::ultrahdr::tests::sample(64, 48);
        set(&mut file, d);
        assert_eq!(read(&file), Some(d));
        let image = crate::decode::decode_full(&file, "a.jpg").unwrap().resize_exact(32, 24, image::imageops::FilterType::Triangle);
        let out = crate::ultrahdr::resized(&file, image, Turn::default(), None, &jpeg_options()).unwrap();
        assert!(crate::ultrahdr::is(&out));
        assert_eq!(read(crate::ultrahdr::split(&out).unwrap().0), Some(d));
        let image = crate::decode::decode_full(&file, "a.jpg").unwrap();
        let edited = crate::ultrahdr::edited(&file, image, &[], &[], None).unwrap();
        assert_eq!(read(crate::ultrahdr::split(&edited).unwrap().0), Some(d));
    }
}
