//! A picture reduced to the size of a texture without a copy of it in floats (#177). image-rs's
//! `resize` first makes the whole picture reduced in height only, in 32-bit floats, 16 bytes a
//! pixel: 2 GiB for a picture of 32768 × 16384 reduced to 8192 pixels, more than the decoded
//! picture itself (1.5 GiB in RGB). Here the same triangle filter goes one row of the result at a
//! time: the rows it needs are added up into one row of floats, then reduced in width.

use image::{DynamicImage, GenericImageView, RgbaImage};
use rayon::prelude::*;

/// `img` reduced to fit in `side` × `side`, its proportions kept as `DynamicImage::resize` keeps
/// them, with its triangle filter. Not enlarged: a picture that already fits only becomes RGBA.
pub fn fit(img: &DynamicImage, side: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let ratio = f64::min(side as f64 / w.max(1) as f64, side as f64 / h.max(1) as f64);
    if ratio >= 1.0 {
        return img.to_rgba8();
    }
    let (nw, nh) = (((w as f64 * ratio).round() as u32).max(1), ((h as f64 * ratio).round() as u32).max(1));
    let (cols, rows) = (taps(w, nw), taps(h, nh));
    let mut out = RgbaImage::new(nw, nh);
    out.par_chunks_mut(nw as usize * 4).zip(rows.par_iter()).for_each_init(
        || (vec![0f32; w as usize * 4], vec![0f32; w as usize * 4]),
        |(sum, line), (row, (first, weights))| {
            sum.fill(0.0);
            for (k, weight) in weights.iter().enumerate() {
                read_row(img, first + k as u32, line);
                sum.iter_mut().zip(line.iter()).for_each(|(s, v)| *s += weight * v);
            }
            for (px, (first, weights)) in row.as_chunks_mut::<4>().0.iter_mut().zip(&cols) {
                let mut acc = [0f32; 4];
                for (k, weight) in weights.iter().enumerate() {
                    let at = (*first as usize + k) * 4;
                    for c in 0..4 {
                        acc[c] += weight * sum[at + c];
                    }
                }
                for c in 0..4 {
                    px[c] = acc[c].round().clamp(0.0, 255.0) as u8;
                }
            }
        },
    );
    out
}

/// Row `y` of `img` as RGBA at 8 bits, in floats (`DynamicImage::get_pixel` converts each pixel
/// as `to_rgba8` does).
fn read_row(img: &DynamicImage, y: u32, line: &mut [f32]) {
    for (x, px) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let p = img.get_pixel(x as u32, y).0;
        for c in 0..4 {
            px[c] = f32::from(p[c]);
        }
    }
}

/// For each of the `to` pixels made from `from`: the first pixel it takes and the weights of those
/// it takes, adding up to 1. The triangle filter of image-rs (`imageops::resize`, `Triangle`):
/// centred on the middle of the new pixel, as wide as one new pixel on each side.
fn taps(from: u32, to: u32) -> Vec<(u32, Vec<f32>)> {
    let ratio = from as f32 / to as f32;
    let support = ratio.max(1.0);
    (0..to)
        .map(|i| {
            let centre = (i as f32 + 0.5) * ratio;
            let left = ((centre - support).floor() as i64).clamp(0, from as i64 - 1) as u32;
            let right = ((centre + support).ceil() as i64).clamp(left as i64 + 1, from as i64) as u32;
            let mut weights: Vec<f32> = (left..right).map(|x| (1.0 - ((x as f32 + 0.5 - centre) / support).abs()).max(0.0)).collect();
            let total: f32 = weights.iter().sum();
            if total > 0.0 {
                weights.iter_mut().for_each(|w| *w /= total);
            } else {
                weights = vec![1.0 / weights.len() as f32; weights.len()];
            }
            (left, weights)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The size of `resize`, a plain colour kept, nothing larger made.
    #[test]
    fn sizes_and_plain_colour() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1000, 300, image::Rgb([10, 200, 30])));
        let small = fit(&img, 256);
        let theirs = img.resize(256, 256, image::imageops::FilterType::Triangle);
        assert_eq!(small.dimensions(), theirs.dimensions());
        assert!(small.pixels().all(|p| p.0 == [10, 200, 30, 255]));
        assert_eq!(fit(&img, 4000).dimensions(), (1000, 300));
        let tall = DynamicImage::ImageRgba16(image::ImageBuffer::from_pixel(7, 9000, image::Rgba([65535, 0, 0, 32768])));
        let t = fit(&tall, 100);
        assert_eq!(t.dimensions(), (1, 100));
        assert!(t.pixels().all(|p| p.0 == [255, 0, 0, 128]));
    }

    /// The same pixels as `resize` with the triangle filter, to a level or two of rounding, on a
    /// picture with gradients and sharp edges, reduced a little and a lot.
    #[test]
    fn as_resize() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(1203, 801, |x, y| {
            let edge = if (x / 37 + y / 23) % 2 == 0 { 255 } else { 0 };
            image::Rgb([(x % 256) as u8, (y % 256) as u8, edge])
        }));
        for side in [1000, 600, 97] {
            let ours = fit(&img, side);
            let theirs = img.resize(side, side, image::imageops::FilterType::Triangle).to_rgba8();
            assert_eq!(ours.dimensions(), theirs.dimensions());
            let diff: Vec<u8> = ours.as_raw().iter().zip(theirs.as_raw()).map(|(a, b)| a.abs_diff(*b)).collect();
            let mean = diff.iter().map(|&d| f64::from(d)).sum::<f64>() / diff.len() as f64;
            assert!(mean < 1.0, "{side}: mean difference {mean}");
            assert!(diff.iter().all(|&d| d <= 24), "{side}: largest difference {:?}", diff.iter().max());
        }
    }

    /// Weights add up to 1 and stay inside the picture.
    #[test]
    fn weights() {
        for (from, to) in [(10, 3), (8193, 8192), (32768, 8192), (5, 5)] {
            for (first, w) in taps(from, to) {
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-4);
                assert!(first as usize + w.len() <= from as usize);
            }
        }
    }
}
