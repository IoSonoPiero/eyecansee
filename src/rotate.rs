//! Rotation by any angle and crop (Image > Transform > Rotate and crop…), without the interface.
//! The crop is a rectangle of the rotated image, in pixels of the original, measured from the
//! centre of the image: x to the right, y down. The angle is clockwise, in degrees.

use image::{DynamicImage, ImageBuffer};

use crate::transform::Turn;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Crop {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
}

impl Crop {
    pub fn centred(w: f32, h: f32) -> Crop {
        Crop { cx: 0.0, cy: 0.0, w, h }
    }

    /// The largest rectangle with sides in the ratio `ratio` (width / height) inside this one, on
    /// the same centre.
    pub fn fit(self, ratio: f32) -> Crop {
        let (w, h) = if self.w / self.h > ratio { (self.h * ratio, self.h) } else { (self.w, self.w / ratio) };
        Crop { w, h, ..self }
    }

    /// Size of the result in pixels, at least 1×1.
    pub fn pixels(self) -> (u32, u32) {
        (self.w.round().max(1.0) as u32, self.h.round().max(1.0) as u32)
    }
}

/// What "Rotate and crop" does to an image: the flips and quarter turns of the viewer first, then
/// the rotation and the crop. Where the crop goes past the image: `fill`, or transparent if `None`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Edit {
    pub turn: Turn,
    pub angle: f32,
    pub crop: Crop,
    pub fill: Option<[u8; 3]>,
}

impl Edit {
    pub fn apply(&self, img: &mut DynamicImage) {
        self.turn.apply(img);
        *img = rotate_crop(img, self.angle, self.crop, self.fill);
    }
}

/// The angle that makes a line drawn on the screen (`dx`, `dy`, y down) level, or upright if it is
/// nearer to upright: the image is shown rotated by `angle`.
pub fn straightened(angle: f32, dx: f32, dy: f32) -> f32 {
    if dx == 0.0 && dy == 0.0 {
        return angle;
    }
    // Slope of the line, clockwise, between -90° and 90°.
    let mut slope = dy.atan2(dx).to_degrees();
    if slope > 90.0 {
        slope -= 180.0;
    } else if slope <= -90.0 {
        slope += 180.0;
    }
    if slope.abs() > 45.0 {
        slope -= 90.0 * slope.signum();
    }
    let a = (((angle - slope) * 10.0).round() / 10.0 + 180.0).rem_euclid(360.0) - 180.0;
    if a == -180.0 { 180.0 } else { a }
}

/// Size of the box around `w`×`h` rotated by `angle`.
pub fn bounds(w: f32, h: f32, angle: f32) -> (f32, f32) {
    let (s, c) = sin_cos(angle);
    (w * c + h * s, w * s + h * c)
}

/// The largest centred rectangle with sides in the ratio `ratio` that has only image in it, once
/// `w`×`h` is rotated by `angle`: what is left when the empty corners are cut away.
pub fn inside(w: f32, h: f32, angle: f32, ratio: f32) -> Crop {
    let (s, c) = sin_cos(angle);
    // Half height b, half width ratio·b: each corner of the crop, turned back, stays in the image.
    let b = (w / 2.0 / (ratio * c + s)).min(h / 2.0 / (ratio * s + c));
    Crop::centred(2.0 * b * ratio, 2.0 * b)
}

/// |sin| and |cos| of the angle in degrees; exact on the quarter turns.
fn sin_cos(angle: f32) -> (f32, f32) {
    let (s, c) = (angle as f64).to_radians().sin_cos();
    let clean = |v: f64| if v.abs() < 1e-9 { 0.0 } else { v.abs() as f32 };
    (clean(s), clean(c))
}

/// `img` rotated by `angle` and cut to `crop`, every pixel taken from the four around the point
/// it comes from (bilinear). 16-bit images stay 16-bit; without transparency in the image nor in
/// the fill, the result has none either.
pub fn rotate_crop(img: &DynamicImage, angle: f32, crop: Crop, fill: Option<[u8; 3]>) -> DynamicImage {
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let (ow, oh) = crop.pixels();
    // Left and top on whole pixels of the original: with no rotation the pixels are copied as they are.
    let left = (crop.cx - ow as f32 / 2.0 + iw / 2.0).round() - iw / 2.0;
    let top = (crop.cy - oh as f32 / 2.0 + ih / 2.0).round() - ih / 2.0;
    let x0 = left + iw / 2.0;
    let y0 = top + ih / 2.0;
    if angle.rem_euclid(360.0) == 0.0 && x0 >= 0.0 && y0 >= 0.0 && x0 + ow as f32 <= iw && y0 + oh as f32 <= ih {
        return img.crop_imm(x0 as u32, y0 as u32, ow, oh);
    }
    let deep = matches!(img, DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_) | DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_));
    let opaque = fill.is_some() && !img.color().has_alpha();
    let fill = fill.map_or([0.0; 4], |c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]);
    let map = Map::new(iw, ih, angle, left, top);
    let size = (img.width(), img.height());
    let out = if deep {
        let px = resample(img.to_rgba16().as_raw(), size, &map, (ow, oh), fill, 65535.0, |v| v as u16);
        DynamicImage::ImageRgba16(ImageBuffer::from_raw(ow, oh, px).expect("buffer of the right size"))
    } else {
        let px = resample(img.to_rgba8().as_raw(), size, &map, (ow, oh), fill, 255.0, |v| v as u8);
        DynamicImage::ImageRgba8(ImageBuffer::from_raw(ow, oh, px).expect("buffer of the right size"))
    };
    match (opaque, deep) {
        (true, true) => DynamicImage::ImageRgb16(out.to_rgb16()),
        (true, false) => DynamicImage::ImageRgb8(out.to_rgb8()),
        _ => out,
    }
}

/// From a pixel of the result to the point of the original it comes from (pixel centres at .5).
struct Map {
    sin: f32,
    cos: f32,
    left: f32,
    top: f32,
    iw: f32,
    ih: f32,
}

impl Map {
    fn new(iw: f32, ih: f32, angle: f32, left: f32, top: f32) -> Map {
        let (s, c) = (angle as f64).to_radians().sin_cos();
        Map { sin: s as f32, cos: c as f32, left, top, iw, ih }
    }

    /// Position in the original, in pixels from its top left corner (centres at whole numbers).
    fn source(&self, x: u32, y: u32) -> (f32, f32) {
        let (px, py) = (self.left + x as f32 + 0.5, self.top + y as f32 + 0.5);
        // Turned back by the angle (clockwise on screen, with y going down).
        let sx = px * self.cos + py * self.sin;
        let sy = -px * self.sin + py * self.cos;
        (sx + self.iw / 2.0 - 0.5, sy + self.ih / 2.0 - 0.5)
    }
}

/// Bilinear, with the colours weighted by their opacity so the fill does not bleed into the edge
/// as a dark or light fringe. RGBA samples of `w`×`h` in, of `ow`×`oh` out, `max` the largest
/// value of a sample, `back` from 0..=`max` (rounded) to a sample. Rows split among the cores.
fn resample<S: Copy + Default + Into<f32> + Send + Sync>(src: &[S], (w, h): (u32, u32), map: &Map, (ow, oh): (u32, u32), fill: [f32; 4], max: f32, back: fn(f32) -> S) -> Vec<S> {
    let (w, h) = (w as i64, h as i64);
    let pixel = |x: i64, y: i64| -> [f32; 4] {
        if x < 0 || y < 0 || x >= w || y >= h {
            return fill;
        }
        let i = ((y * w + x) * 4) as usize;
        [src[i].into() / max, src[i + 1].into() / max, src[i + 2].into() / max, src[i + 3].into() / max]
    };
    let mut out = vec![S::default(); ow as usize * oh as usize * 4];
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let rows = (oh as usize).div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (band, chunk) in out.chunks_mut(rows * ow as usize * 4).enumerate() {
            let pixel = &pixel;
            scope.spawn(move || {
                for (i, px) in chunk.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let (x, y) = ((i % ow as usize) as u32, (band * rows + i / ow as usize) as u32);
                    let (sx, sy) = map.source(x, y);
                    let (fx, fy) = (sx.floor(), sy.floor());
                    let (ax, ay) = (sx - fx, sy - fy);
                    let (x0, y0) = (fx as i64, fy as i64);
                    let mut acc = [0.0f32; 4];
                    for (dx, dy, k) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] {
                        let p = pixel(x0 + dx, y0 + dy);
                        for c in 0..3 {
                            acc[c] += p[c] * p[3] * k;
                        }
                        acc[3] += p[3] * k;
                    }
                    for c in 0..3 {
                        let v = if acc[3] > 0.0 { acc[c] / acc[3] } else { 0.0 };
                        px[c] = back((v * max + 0.5).clamp(0.0, max));
                    }
                    px[3] = back((acc[3] * max + 0.5).clamp(0.0, max));
                }
            });
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage};

    fn sample(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| Rgb([(x * 37 % 256) as u8, (y * 53 % 256) as u8, ((x + y) * 11 % 256) as u8])))
    }

    /// No rotation: the crop is the same pixels, in the same type.
    #[test]
    fn plain_crop() {
        let img = sample(40, 30);
        let crop = Crop { cx: -5.0, cy: 3.0, w: 20.0, h: 10.0 };
        let out = rotate_crop(&img, 0.0, crop, None);
        assert_eq!(out.to_rgb8(), img.crop_imm(5, 13, 20, 10).to_rgb8());
        assert!(matches!(out, DynamicImage::ImageRgb8(_)));
    }

    /// A quarter turn by angle gives the same pixels as `rotate90`.
    #[test]
    fn quarter_turn() {
        let img = sample(12, 8);
        let (bw, bh) = bounds(12.0, 8.0, 90.0);
        assert_eq!((bw, bh), (8.0, 12.0));
        let out = rotate_crop(&img, 90.0, Crop::centred(bw, bh), Some([0, 0, 0]));
        assert_eq!(out.to_rgb8(), img.rotate90().to_rgb8());
    }

    /// The corners outside the image take the fill, or stay transparent; the centre is the image.
    #[test]
    fn fill() {
        let img = sample(40, 40);
        let (bw, bh) = bounds(40.0, 40.0, 45.0);
        let out = rotate_crop(&img, 45.0, Crop::centred(bw, bh), Some([10, 200, 30]));
        assert!(matches!(out, DynamicImage::ImageRgb8(_)));
        assert_eq!(out.get_pixel(1, 1).0, [10, 200, 30, 255]);
        let clear = rotate_crop(&img, 45.0, Crop::centred(bw, bh), None);
        assert_eq!(clear.get_pixel(1, 1)[3], 0);
        let mid = (clear.width() / 2, clear.height() / 2);
        assert_eq!(clear.get_pixel(mid.0, mid.1)[3], 255);
    }

    /// The crop inside: the image's own ratio when rotated by nothing, and with every pixel of the
    /// result opaque when rotated (no empty corner left).
    #[test]
    fn inside_has_no_corners() {
        assert_eq!(inside(40.0, 30.0, 0.0, 40.0 / 30.0), Crop::centred(40.0, 30.0));
        for angle in [3.0, -12.5, 30.0, 89.0, 135.0] {
            let crop = inside(60.0, 40.0, angle, 1.5);
            assert!((crop.w / crop.h - 1.5).abs() < 1e-4);
            // Shrunk by a pixel: the edge pixels mix in a bit of the outside.
            let crop = Crop { w: crop.w - 2.0, h: crop.h - 2.0, ..crop };
            let out = rotate_crop(&sample(60, 40), angle, crop, None);
            assert!(out.pixels().all(|(_, _, p)| p[3] == 255), "{angle}");
        }
    }

    /// Save: the file in its own format, rotated and cropped to the size of the crop.
    #[test]
    fn saved_file() {
        let mut png = Vec::new();
        sample(60, 40).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let edit = Edit { turn: Turn::default(), angle: 10.0, crop: inside(60.0, 40.0, 10.0, 1.0), fill: None };
        let (out, lossy) = crate::encode::edited_file(&png, "x.png", |img| edit.apply(img)).unwrap();
        assert!(!lossy);
        let back = crate::decode::decode_full(&out, "x.png").unwrap();
        assert_eq!(back.dimensions(), edit.crop.pixels());
    }

    #[test]
    fn straighten() {
        // A horizon going down to the right by 5° on screen: turned back by 5°.
        let (dx, dy) = (100.0, 100.0 * 5f32.to_radians().tan());
        assert_eq!(straightened(0.0, dx, dy), -5.0);
        // Drawn from right to left: the same line.
        assert_eq!(straightened(0.0, -dx, -dy), -5.0);
        // On an image already turned by 2°.
        assert_eq!(straightened(2.0, dx, dy), -3.0);
        // Nearly upright (a wall): made upright, not level.
        assert_eq!(straightened(0.0, dy, dx), 5.0);
        assert_eq!(straightened(7.0, 0.0, 0.0), 7.0);
    }

    #[test]
    fn fit_ratio() {
        assert_eq!(Crop::centred(100.0, 50.0).fit(1.0), Crop::centred(50.0, 50.0));
        assert_eq!(Crop::centred(100.0, 50.0).fit(4.0), Crop::centred(100.0, 25.0));
    }
}
