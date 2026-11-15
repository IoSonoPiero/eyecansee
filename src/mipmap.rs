//! Pictures shown smaller than their own pixels, with halved copies of them (mipmaps): each point
//! of the screen shows the average of the pixels under it. The graphics card's linear filter
//! mixes only the 2 × 2 pixels nearest to each point: a photo shown at a third of its size took
//! one pixel in nine, so edges came out jagged and fine details (grass, hair) grainy. egui-wgpu
//! makes no mipmaps (`mip_level_count: 1`), so the copies are made here and the one drawn is the
//! smallest still as large as the picture on the screen: the filter then mixes at most 2 × 2 of
//! its pixels, each already the average of those under it. Above 100% nothing changes.

use std::ops::Deref;
use std::sync::Arc;

use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use rayon::prelude::*;

/// No copy smaller than this on its longer side: nothing is shown much smaller (the viewer never
/// goes below the fitted size).
pub(crate) const SMALLEST_PICTURE: usize = 128;
/// For thumbnails (256 pixels), shown down to 64 points.
pub(crate) const SMALLEST_THUMB: usize = 64;

/// Half the size, each pixel the average of the 2×2 under it (an odd last row or column left
/// out): ~5 ms for 2048² where a filtered resize took ~60.
pub(crate) fn half(img: &image::RgbaImage) -> image::RgbaImage {
    let ([w, h], raw) = half_raw([img.width() as usize, img.height() as usize], img.as_raw());
    image::RgbaImage::from_raw(w as u32, h as u32, raw).expect("half: size")
}

/// `half` on RGBA bytes (`size` pixels), its rows in parallel: the new size and bytes.
pub(crate) fn half_raw(size: [usize; 2], src: &[u8]) -> ([usize; 2], Vec<u8>) {
    let [w, h] = size;
    let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; hw * hh * 4];
    out.par_chunks_mut(hw * 4).enumerate().for_each(|(y, row)| {
        let y0 = (2 * y).min(h - 1);
        let y1 = (y0 + 1).min(h - 1);
        let (r0, r1) = (&src[y0 * w * 4..(y0 + 1) * w * 4], &src[y1 * w * 4..(y1 + 1) * w * 4]);
        for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (a, b) = (4 * (2 * x).min(w - 1), 4 * (2 * x + 1).min(w - 1));
            for c in 0..4 {
                let sum = r0[a + c] as u32 + r0[b + c] as u32 + r1[a + c] as u32 + r1[b + c] as u32;
                px[c] = ((sum + 2) / 4) as u8;
            }
        }
    });
    ([hw, hh], out)
}

/// The halved copies of `img`, each half the one before, while the longer side stays at least
/// `smallest`. Its bytes are premultiplied: averaged as they are, a transparent pixel does not
/// darken its neighbours.
pub(crate) fn halves(img: &ColorImage, smallest: usize) -> Vec<ColorImage> {
    let mut out: Vec<ColorImage> = Vec::new();
    loop {
        let last = out.last().unwrap_or(img);
        if last.size[0].max(last.size[1]) / 2 < smallest {
            return out;
        }
        let (size, raw) = half_raw(last.size, last.as_raw());
        out.push(ColorImage::from_rgba_premultiplied(size, &raw));
    }
}

/// `halves` for a picture of the viewer, the editor or "Save in another format".
pub(crate) fn picture_halves(img: &ColorImage) -> Vec<ColorImage> {
    halves(img, SMALLEST_PICTURE)
}

/// Which copy to draw at `scale` screen pixels per pixel of the picture (0 the picture itself,
/// `copies` at most): the smallest one still at least as large as on the screen.
pub(crate) fn level(scale: f32, copies: usize) -> usize {
    if !(scale > 0.0 && scale < 1.0) {
        return 0;
    }
    // A hair over: at 50% the half itself, pixel for pixel, not the picture squeezed.
    (((1.0 / scale).log2() + 1e-3).floor() as usize).min(copies)
}

/// A picture on the graphics card with its halved copies.
#[derive(Clone)]
pub(crate) struct Picture {
    tex: TextureHandle,
    halves: Vec<TextureHandle>,
}

impl Picture {
    pub(crate) fn new(ctx: &egui::Context, name: &str, image: ColorImage, halves: Vec<ColorImage>) -> Self {
        let halves = halves.into_iter().enumerate().map(|(i, h)| ctx.load_texture(format!("{name} 1/{}", 2 << i), h, TextureOptions::LINEAR)).collect();
        Self { tex: ctx.load_texture(name, image, TextureOptions::LINEAR), halves }
    }

    /// With its copies made here: small pictures, or a picture opened once (the editor).
    pub(crate) fn now(ctx: &egui::Context, name: &str, image: ColorImage, smallest: usize) -> Self {
        let halves = halves(&image, smallest);
        Self::new(ctx, name, image, halves)
    }

    /// The texture to draw at `scale` screen pixels per pixel of the picture.
    pub(crate) fn id_at(&self, scale: f32) -> TextureId {
        self.texture_at(scale).id()
    }

    /// The copy for `scale` screen pixels per pixel of the picture.
    pub(crate) fn texture_at(&self, scale: f32) -> &TextureHandle {
        match level(scale, self.halves.len()) {
            0 => &self.tex,
            l => &self.halves[l - 1],
        }
    }

    /// Another picture in its place (the next frame of an animation, another page), with its
    /// copies made again: the old ones would still show the first.
    pub(crate) fn set(&mut self, ctx: &egui::Context, image: Arc<ColorImage>) {
        let halves = picture_halves(&image);
        self.halves.truncate(halves.len());
        for (i, h) in halves.into_iter().enumerate() {
            match self.halves.get_mut(i) {
                Some(t) => t.set(h, TextureOptions::LINEAR),
                None => self.halves.push(ctx.load_texture(format!("frame 1/{}", 2 << i), h, TextureOptions::LINEAR)),
            }
        }
        self.tex.set(image, TextureOptions::LINEAR);
    }
}

/// A picture without copies, drawn as before.
impl From<TextureHandle> for Picture {
    fn from(tex: TextureHandle) -> Self {
        Self { tex, halves: Vec::new() }
    }
}

/// Its size and id are the picture's own.
impl Deref for Picture {
    type Target = TextureHandle;
    fn deref(&self) -> &TextureHandle {
        &self.tex
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `size` pixels, white where `white(x, y)`, black elsewhere.
    fn pattern(size: [usize; 2], white: impl Fn(usize, usize) -> bool) -> ColorImage {
        let pixels = (0..size[0] * size[1]).map(|i| if white(i % size[0], i / size[0]) { egui::Color32::WHITE } else { egui::Color32::BLACK }).collect();
        ColorImage::new(size, pixels)
    }

    /// The linear filter of the graphics card: `img` taken at the centre of each of `out` pixels,
    /// as when drawn on that many screen pixels. The grey of each.
    fn drawn(img: &ColorImage, out: [usize; 2]) -> Vec<f32> {
        let [w, h] = img.size;
        let at = |x: isize, y: isize| img.pixels[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize].r() as f32;
        let mut v = Vec::new();
        for oy in 0..out[1] {
            for ox in 0..out[0] {
                let u = (ox as f32 + 0.5) * w as f32 / out[0] as f32 - 0.5;
                let t = (oy as f32 + 0.5) * h as f32 / out[1] as f32 - 0.5;
                let (x, y) = (u.floor(), t.floor());
                let (fx, fy) = (u - x, t - y);
                let (x, y) = (x as isize, y as isize);
                let top = at(x, y) * (1.0 - fx) + at(x + 1, y) * fx;
                let bottom = at(x, y + 1) * (1.0 - fx) + at(x + 1, y + 1) * fx;
                v.push(top * (1.0 - fy) + bottom * fy);
            }
        }
        v
    }

    /// As the viewer draws `img` on `out` pixels: the copy `level` chooses, through the filter.
    fn shown(img: &ColorImage, out: [usize; 2]) -> Vec<f32> {
        let copies = halves(img, 1);
        match level(out[0] as f32 / img.size[0] as f32, copies.len()) {
            0 => drawn(img, out),
            l => drawn(&copies[l - 1], out),
        }
    }

    fn range(v: &[f32]) -> (f32, f32) {
        (v.iter().copied().fold(f32::MAX, f32::min), v.iter().copied().fold(f32::MIN, f32::max))
    }

    /// Lines one pixel thin and a chessboard shown at a third, at a quarter and as a 4032 photo
    /// fitted to 1920: an even grey, the average of each place. The filter alone takes a few
    /// pixels: white and black stripes.
    #[test]
    fn thin_lines_and_chessboard_shown_smaller_are_grey() {
        let lines = pattern([960, 540], |_, y| y % 2 == 0);
        let chess = pattern([960, 540], |x, y| (x + y) % 2 == 0);
        for img in [&lines, &chess] {
            for out in [[320, 180], [240, 135], [457, 257]] {
                let (low, high) = range(&shown(img, out));
                assert!(high - low <= 2.0 && (low - 127.5).abs() < 2.0, "{out:?}: from {low} to {high}");
            }
        }
        let (low, high) = range(&drawn(&lines, [320, 180]));
        assert!(high - low > 200.0, "the filter alone: from {low} to {high}");
    }

    /// A slanted edge shown at a quarter and at an eighth: each point the average of the pixels it
    /// covers, so the edge goes through the greys instead of in steps. The filter alone makes
    /// each point black or white.
    #[test]
    fn slanted_edge_shown_smaller_is_the_average() {
        let edge = pattern([800, 800], |x, y| x * 7 > y * 3 + 1000);
        for k in [4, 8] {
            let out = [800 / k, 800 / k];
            let average: Vec<f32> = (0..out[0] * out[1])
                .map(|i| {
                    let (ox, oy) = (i % out[0], i / out[0]);
                    let white = (0..k * k).filter(|j| edge.pixels[(oy * k + j / k) * 800 + ox * k + j % k].r() == 255).count();
                    255.0 * white as f32 / (k * k) as f32
                })
                .collect();
            let off = |v: &[f32]| v.iter().zip(&average).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(off(&shown(&edge, out)) <= 1.0, "1/{k}: {}", off(&shown(&edge, out)));
            assert!(off(&drawn(&edge, out)) > 50.0, "1/{k}, the filter alone: {}", off(&drawn(&edge, out)));
        }
    }

    #[test]
    fn level_is_the_smallest_copy_still_as_large() {
        assert_eq!(level(2.0, 5), 0);
        assert_eq!(level(1.0, 5), 0);
        assert_eq!(level(0.6, 5), 0);
        assert_eq!(level(0.5, 5), 1);
        assert_eq!(level(0.3, 5), 1);
        assert_eq!(level(0.25, 5), 2);
        assert_eq!(level(0.01, 5), 5);
        assert_eq!(level(0.0, 5), 0);
    }

    #[test]
    fn halves_stop_at_the_smallest_side() {
        let sizes: Vec<_> = halves(&pattern([1000, 300], |_, _| true), 128).iter().map(|h| h.size).collect();
        assert_eq!(sizes, [[500, 150], [250, 75]]);
        assert!(halves(&pattern([200, 100], |_, _| true), 128).is_empty());
    }

    /// The average of 2 × 2, an odd last column left out; a single pixel stays itself.
    #[test]
    fn half_is_the_average_of_two_by_two() {
        let img = image::RgbaImage::from_fn(3, 2, |x, _| image::Rgba([(x * 100) as u8, 0, 0, 255]));
        let h = half(&img);
        assert_eq!((h.width(), h.height()), (1, 1));
        assert_eq!(h.get_pixel(0, 0).0, [50, 0, 0, 255]);
        let one = image::RgbaImage::from_pixel(1, 1, image::Rgba([9, 9, 9, 9]));
        assert_eq!(half(&one).get_pixel(0, 0).0, [9, 9, 9, 9]);
    }
}
