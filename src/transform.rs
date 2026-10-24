//! Flips and quarter turns (Image > Transform), as GIMP has them. Any mix of them is one of the 8
//! orientations of a rectangle, kept as a horizontal flip done first and then clockwise quarter
//! turns: the viewer draws it by moving the texture corners, saving turns the pixels once.
//! A JPEG is turned without re-encoding (libjpeg-turbo, as jpegtran) when its size allows it.

use eframe::egui;
use image::DynamicImage;
use image::metadata::Orientation;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Turn {
    flip: bool,
    /// Clockwise quarter turns, 0 to 3.
    quarter: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    FlipH,
    FlipV,
    Cw,
    Ccw,
    Half,
}

impl Turn {
    pub fn is_none(self) -> bool {
        self == Turn::default()
    }

    /// Width and height swap.
    pub fn sideways(self) -> bool {
        self.quarter % 2 == 1
    }

    /// `op` done after this.
    pub fn then(self, op: Op) -> Turn {
        let q = self.quarter;
        match op {
            Op::Cw => Turn { flip: self.flip, quarter: (q + 1) % 4 },
            Op::Ccw => Turn { flip: self.flip, quarter: (q + 3) % 4 },
            Op::Half => Turn { flip: self.flip, quarter: (q + 2) % 4 },
            // F·R^q = R^-q·F; a vertical flip is a horizontal one and half a turn.
            Op::FlipH => Turn { flip: !self.flip, quarter: (4 - q) % 4 },
            Op::FlipV => Turn { flip: !self.flip, quarter: (6 - q) % 4 },
        }
    }

    /// This done after `first`.
    fn after(self, first: Turn) -> Turn {
        let mut t = first;
        if self.flip {
            t = t.then(Op::FlipH);
        }
        for _ in 0..self.quarter {
            t = t.then(Op::Cw);
        }
        t
    }

    /// What an EXIF orientation does to make the image upright.
    pub fn from_orientation(o: Orientation) -> Turn {
        let (flip, quarter) = match o {
            Orientation::NoTransforms => (false, 0),
            Orientation::Rotate90 => (false, 1),
            Orientation::Rotate180 => (false, 2),
            Orientation::Rotate270 => (false, 3),
            Orientation::FlipHorizontal => (true, 0),
            Orientation::FlipVertical => (true, 2),
            // Turned first, then flipped: the same as flipped first and turned the other way.
            Orientation::Rotate90FlipH => (true, 3),
            Orientation::Rotate270FlipH => (true, 1),
        };
        Turn { flip, quarter }
    }

    pub fn apply(self, img: &mut DynamicImage) {
        if self.flip {
            *img = img.fliph();
        }
        *img = match self.quarter {
            1 => img.rotate90(),
            2 => img.rotate180(),
            3 => img.rotate270(),
            _ => return,
        };
    }

    /// Point of the texture (0 to 1 on each side) shown at `shown` once turned.
    pub fn source_uv(self, shown: egui::Pos2) -> egui::Pos2 {
        let mut p = shown;
        for _ in 0..self.quarter {
            p = egui::pos2(p.y, 1.0 - p.x);
        }
        if self.flip { egui::pos2(1.0 - p.x, p.y) } else { p }
    }

    fn jpeg_op(self) -> turbojpeg::TransformOp {
        use turbojpeg::TransformOp::*;
        match (self.flip, self.quarter) {
            (false, 0) => None,
            (false, 1) => Rot90,
            (false, 2) => Rot180,
            (false, 3) => Rot270,
            (true, 0) => Hflip,
            (true, 2) => Vflip,
            (true, 1) => Transverse,
            _ => Transpose,
        }
    }
}

/// The JPEG turned without decoding it, upright (its EXIF orientation is done too and set back to
/// "upright"). `None` when the size is not a multiple of the blocks: the edge could not be kept.
pub fn jpeg_lossless(bytes: &[u8], turn: Turn) -> Option<Vec<u8>> {
    use image::ImageDecoder;
    let exif = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes))
        .ok()
        .and_then(|mut d| d.orientation().ok())
        .unwrap_or(Orientation::NoTransforms);
    let total = turn.after(Turn::from_orientation(exif));
    let mut t = turbojpeg::Transform::op(total.jpeg_op());
    t.perfect = true;
    let mut out = turbojpeg::transform(&t, bytes).ok()?.to_vec();
    upright_exif(&mut out);
    Some(out)
}

/// Sets the orientation in the EXIF segment of a JPEG to "upright", in place.
fn upright_exif(jpeg: &mut [u8]) {
    let mut i = 2;
    while i + 4 <= jpeg.len() && jpeg[i] == 0xFF {
        let marker = jpeg[i + 1];
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if marker == 0xDA || i + 2 + len > jpeg.len() {
            return;
        }
        let body = &mut jpeg[i + 4..i + 2 + len];
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            let _ = Orientation::remove_from_exif_chunk(&mut body[6..]);
            return;
        }
        i += 2 + len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage};

    const OPS: [Op; 5] = [Op::FlipH, Op::FlipV, Op::Cw, Op::Ccw, Op::Half];

    fn sample() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(3, 2, |x, y| Rgb([x as u8 * 80, y as u8 * 200, 7])))
    }

    fn by_hand(img: &DynamicImage, op: Op) -> DynamicImage {
        match op {
            Op::FlipH => img.fliph(),
            Op::FlipV => img.flipv(),
            Op::Cw => img.rotate90(),
            Op::Ccw => img.rotate270(),
            Op::Half => img.rotate180(),
        }
    }

    /// Every pair of operations gives the same pixels as doing them one after the other, and the
    /// viewer's corners agree with the pixels.
    #[test]
    fn compose() {
        for a in OPS {
            for b in OPS {
                let turn = Turn::default().then(a).then(b);
                let mut img = sample();
                turn.apply(&mut img);
                let expected = by_hand(&by_hand(&sample(), a), b);
                assert_eq!(img.to_rgb8(), expected.to_rgb8(), "{a:?} {b:?}");
                let (w, h) = expected.dimensions();
                for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
                    let shown = egui::pos2((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                    let s = turn.source_uv(shown);
                    let src = sample().get_pixel((s.x * 3.0) as u32, (s.y * 2.0) as u32);
                    assert_eq!(src, expected.get_pixel(x, y), "{a:?} {b:?} corner {x},{y}");
                }
            }
        }
    }

    /// Lossless JPEG turn of a 32×16 image with EXIF orientation 6: same as decoding and turning.
    #[test]
    fn jpeg_turn() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(32, 16, |x, y| Rgb([(x * 8) as u8, (y * 16) as u8, 128])));
        let mut jpeg = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        // EXIF with orientation 6 (turn clockwise), inserted after SOI.
        let tiff: [u8; 26] = [b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0];
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        jpeg.splice(2..2, app1);
        let shown = crate::decode::decode_full(&jpeg, "x.jpg").unwrap();
        assert_eq!(shown.dimensions(), (16, 32));

        let turn = Turn::default().then(Op::Cw).then(Op::FlipH);
        let out = jpeg_lossless(&jpeg, turn).unwrap();
        assert!(!crate::encode::turned_file(&jpeg, "x.jpg", turn).unwrap().1, "turned without loss");
        let got = crate::decode::decode_full(&out, "x.jpg").unwrap().to_rgb8();
        let mut expected = shown;
        turn.apply(&mut expected);
        assert_eq!(got.dimensions(), expected.dimensions());
        let diff = got.pixels().zip(expected.to_rgb8().pixels()).map(|(a, b)| (a[0] as i32 - b[0] as i32).abs().max((a[1] as i32 - b[1] as i32).abs())).max().unwrap();
        assert!(diff <= 3, "{diff}");
    }

    /// Sizes the blocks do not divide: the JPEG is encoded again, still turned; a PNG too.
    #[test]
    fn saved_file() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(30, 17, |x, _| Rgb([(x * 8) as u8, 0, 0])));
        for name in ["x.jpg", "x.png"] {
            let mut file = Vec::new();
            let format = if name.ends_with("jpg") { image::ImageFormat::Jpeg } else { image::ImageFormat::Png };
            img.write_to(&mut std::io::Cursor::new(&mut file), format).unwrap();
            if name.ends_with("jpg") {
                assert!(jpeg_lossless(&file, Turn::default().then(Op::Cw)).is_none());
            }
            let (out, lossy) = crate::encode::turned_file(&file, name, Turn::default().then(Op::Cw)).unwrap();
            // Encoded again: with loss for the JPEG, not for the PNG (the confirmation says so).
            assert_eq!(lossy, name.ends_with("jpg"), "{name}");
            let back = crate::decode::decode_full(&out, name).unwrap();
            assert_eq!(back.dimensions(), (17, 30), "{name}");
            // The left edge (dark) is now at the top.
            assert!(back.get_pixel(8, 0)[0] < 40 && back.get_pixel(8, 29)[0] > 180, "{name}");
        }
        assert!(crate::encode::turned_file(b"", "x.gif", Turn::default()).is_err());
    }
}
