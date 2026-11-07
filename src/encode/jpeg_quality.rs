//! The quality a JPEG was saved at, read back from its quantization tables, and its colour
//! subsampling: a JPEG saved again as JPEG by the editor or the viewer keeps both, so it is not
//! compressed harder a second time (user, 2026-11-06). The tables are compared with the standard
//! IJG ones scaled for each quality from 1 to 100 (what libjpeg, libjpeg-turbo and most cameras
//! and programs write), as ImageMagick estimates it; with other tables the nearest wins.

use super::{Chroma, Options};

/// The quality when it cannot be read (no table, a broken file).
pub const FALLBACK: u8 = 92;

/// The IJG tables of quality 50, luminance and chrominance, in natural (row) order.
const LUMA: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120,
    101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
];

/// Where each value of a table stored in the file (zigzag order) goes in natural order.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60,
    61, 54, 47, 55, 62, 63,
];

/// A standard table scaled for `q` as libjpeg does (`jpeg_quality_scaling`, baseline values).
fn scaled(table: &[u16; 64], q: u32) -> [u16; 64] {
    let scale = if q < 50 { 5000 / q } else { 200 - 2 * q };
    table.map(|v| ((v as u32 * scale + 50) / 100).clamp(1, 255) as u16)
}

/// The quantization tables (by id, natural order) and the sampling (horizontal, vertical) of the
/// components of a JPEG, read up to its first frame header.
type Read = (Vec<(u8, [u16; 64])>, Vec<(u8, u8)>);

fn read(bytes: &[u8]) -> Option<Read> {
    if bytes.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let (mut at, mut tables, mut sampling) = (2, Vec::new(), Vec::new());
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        let body = bytes.get(at + 4..at + 2 + len)?;
        match marker {
            0xDB => {
                let mut p = 0;
                while p < body.len() {
                    let (precision, id) = (body[p] >> 4, body[p] & 15);
                    let size = if precision == 0 { 1 } else { 2 };
                    let values = body.get(p + 1..p + 1 + 64 * size)?;
                    let mut table = [0u16; 64];
                    for (i, &to) in ZIGZAG.iter().enumerate() {
                        table[to] = if size == 1 { values[i] as u16 } else { u16::from_be_bytes([values[2 * i], values[2 * i + 1]]) };
                    }
                    tables.push((id, table));
                    p += 1 + 64 * size;
                }
            }
            // The frame header: the components' sampling, then the tables are all read.
            0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                let n = *body.get(5)? as usize;
                for c in 0..n {
                    let s = *body.get(6 + 3 * c + 1)?;
                    sampling.push((s >> 4, s & 15));
                }
                return Some((tables, sampling));
            }
            0xDA | 0xD9 => return None,
            _ => {}
        }
        at += 2 + len;
    }
    None
}

/// The quality (1-100) whose standard tables are nearest to the file's, and its colour
/// subsampling; `None` when the file has no table to read.
pub fn of(bytes: &[u8]) -> Option<(u8, Chroma)> {
    let (tables, sampling) = read(bytes)?;
    let luma = tables.iter().find(|(id, _)| *id == 0).or(tables.first())?.1;
    let chroma = tables.iter().find(|(id, _)| *id == 1).map(|t| t.1);
    let distance = |q: u32| {
        let d = |a: &[u16; 64], b: &[u16; 64]| a.iter().zip(b).map(|(x, y)| (*x as i64 - *y as i64).abs()).sum::<i64>();
        d(&luma, &scaled(&LUMA, q)) + chroma.map_or(0, |c| d(&c, &scaled(&CHROMA, q)))
    };
    let q = (1..=100).min_by_key(|&q| distance(q))? as u8;
    let chroma = match sampling.as_slice() {
        [_] => Chroma::Grey,
        [(h, v), rest @ ..] if rest.iter().all(|s| *s == (1, 1)) => match (h, v) {
            (1, 1) => Chroma::Full,
            (2, 1) => Chroma::Half,
            _ => Chroma::Quarter,
        },
        _ => Chroma::Quarter,
    };
    Some((q, chroma))
}

/// The options to write a JPEG again as `original` was: its quality and subsampling, or quality
/// `FALLBACK` (and 4:2:0) when they cannot be read.
pub fn like(original: &[u8], o: Options) -> Options {
    let (jpeg_quality, chroma) = of(original).unwrap_or((FALLBACK, Chroma::Quarter));
    Options { jpeg_quality, chroma, ..o }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::encode::{Format, Original};

    /// A JPEG of a gradient saved by libjpeg-turbo (as EyeCanSee writes them) at `q` with `chroma`.
    pub(crate) fn jpeg(q: u8, chroma: Chroma) -> Vec<u8> {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])));
        Original::with_meta(img, &[]).encode(&Options { format: Format::Jpeg, jpeg_quality: q, chroma, ..Options::default() }).unwrap()
    }

    /// The quality and subsampling come back as saved, for every quality and the usual samplings.
    #[test]
    fn read_back() {
        for q in [1, 10, 49, 50, 51, 75, 80, 90, 92, 95, 99, 100] {
            assert_eq!(of(&jpeg(q, Chroma::Quarter)), Some((q, Chroma::Quarter)), "{q}");
        }
        assert_eq!(of(&jpeg(95, Chroma::Full)), Some((95, Chroma::Full)));
        assert_eq!(of(&jpeg(70, Chroma::Half)), Some((70, Chroma::Half)));
        assert_eq!(of(&jpeg(85, Chroma::Grey)), Some((85, Chroma::Grey)));
    }

    /// Tables that are not the standard ones (here those of 80 with some values changed by
    /// a few): the nearest quality.
    #[test]
    fn nearest_for_other_tables() {
        let mut bytes = jpeg(80, Chroma::Quarter);
        let dqt = bytes.windows(2).position(|w| w == [0xFF, 0xDB]).unwrap();
        for i in [10, 20, 30, 40, 50, 60] {
            bytes[dqt + 5 + i] = bytes[dqt + 5 + i].saturating_add(3);
        }
        let (q, _) = of(&bytes).unwrap();
        assert!((78..=80).contains(&q), "{q}");
    }

    /// The viewer's Save, when it must encode again (a turn the JPEG's size does not allow
    /// without loss), keeps the quality too.
    #[test]
    fn viewer_save_keeps_it() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(61, 45, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])));
        let bytes = Original::with_meta(img, &[]).encode(&Options { format: Format::Jpeg, jpeg_quality: 75, chroma: Chroma::Half, ..Options::default() }).unwrap();
        let (out, lossy) = crate::save_back::edited_file(&bytes, "a.jpg", |i| *i = i.rotate90()).unwrap();
        assert!(lossy);
        assert_eq!(of(&out), Some((75, Chroma::Half)));
    }

    /// Nothing to read (not a JPEG, cut before its tables): the options at 92 and 4:2:0.
    #[test]
    fn fallback() {
        assert_eq!(of(b"not a jpeg"), None);
        let cut = &jpeg(80, Chroma::Full)[..20];
        assert_eq!(of(cut), None);
        let o = like(cut, Options::default());
        assert_eq!((o.jpeg_quality, o.chroma), (92, Chroma::Quarter));
    }

    /// "Save in another format" proposes what the original had (`Original::jpeg`): a JPEG at 92
    /// and 4:4:4 gives both; a PNG nothing (the window keeps 90 and 4:2:0).
    #[test]
    fn save_as_proposes_it() {
        let bytes = jpeg(92, Chroma::Full);
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!(Original::with_meta(img.clone(), &bytes).jpeg, Some((92, Chroma::Full)));
        let png = Original::with_meta(img.clone(), &[]).encode(&Options { format: Format::Png, ..Options::default() }).unwrap();
        assert_eq!(Original::with_meta(img, &png).jpeg, None);
    }
}
