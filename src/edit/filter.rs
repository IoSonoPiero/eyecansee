//! The filters of the editor, without the interface: what each does to a piece of image. A filter
//! is an object over a part of the photo (or all of it), under the objects drawn: the preview is
//! made from the photo at a smaller size, the saved file at full size (`edit::save`).

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Filter {
    Blur,
    Pixelate,
    Sharpen,
    Gray,
    Sepia,
    Brightness,
    Contrast,
}

impl Filter {
    pub const ALL: [Filter; 7] = [Filter::Blur, Filter::Pixelate, Filter::Sharpen, Filter::Gray, Filter::Sepia, Filter::Brightness, Filter::Contrast];

    /// Fluent id of its name.
    pub fn label(self) -> &'static str {
        match self {
            Filter::Blur => "edit-filter-blur",
            Filter::Pixelate => "edit-filter-pixelate",
            Filter::Sharpen => "edit-filter-sharpen",
            Filter::Gray => "edit-filter-gray",
            Filter::Sepia => "edit-filter-sepia",
            Filter::Brightness => "edit-filter-brightness",
            Filter::Contrast => "edit-filter-contrast",
        }
    }

    /// The range of its amount on an image whose long side is `long` pixels, and where it starts;
    /// `None` for those without one. Blur, pixels and sharpening in pixels of the image, the
    /// others from -100 to 100.
    pub fn amount(self, long: f32) -> Option<(f32, f32, f32)> {
        match self {
            Filter::Blur => Some((1.0, (long / 20.0).max(10.0), (long / 200.0).max(2.0))),
            Filter::Pixelate => Some((2.0, (long / 10.0).max(20.0), (long / 80.0).max(4.0))),
            Filter::Sharpen => Some((0.5, 10.0, 2.0)),
            Filter::Gray | Filter::Sepia => None,
            Filter::Brightness | Filter::Contrast => Some((-100.0, 100.0, 20.0)),
        }
    }

    /// Is the amount a size in pixels (it shrinks with the preview)?
    pub(crate) fn in_pixels(self) -> bool {
        matches!(self, Filter::Blur | Filter::Pixelate | Filter::Sharpen)
    }

    /// Does it hide what is under it, as a cover box (the gain map of an Ultra HDR photo then
    /// goes to its lowest there too)?
    pub fn hides(self) -> bool {
        matches!(self, Filter::Blur | Filter::Pixelate)
    }
}

/// `img` with the filter, at `scale` of the image's size (sizes in pixels follow it); the same
/// kind of image, with the same depth (Sepia goes through 8 bits).
pub fn apply(img: &DynamicImage, f: Filter, amount: f32, scale: f32) -> DynamicImage {
    let px = if f.in_pixels() { (amount * scale).max(0.5) } else { amount };
    match f {
        Filter::Blur => mixing(img, |i| i.blur(px)),
        Filter::Pixelate => mixing(img, |i| {
            let (w, h) = i.dimensions();
            let block = px.max(1.0);
            let small = i.resize_exact(((w as f32 / block).ceil() as u32).max(1), ((h as f32 / block).ceil() as u32).max(1), image::imageops::FilterType::Triangle);
            small.resize_exact(w, h, image::imageops::FilterType::Nearest)
        }),
        Filter::Sharpen => mixing(img, |i| sharpened(i, px)),
        Filter::Gray => like(img.grayscale(), img),
        Filter::Sepia => {
            let mut rgba = img.to_rgba8();
            for p in rgba.pixels_mut() {
                let [r, g, b, a] = p.0.map(|c| c as f32);
                let tone = |x: f32, y: f32, z: f32| (r * x + g * y + b * z).min(255.0) as u8;
                *p = Rgba([tone(0.393, 0.769, 0.189), tone(0.349, 0.686, 0.168), tone(0.272, 0.534, 0.131), a as u8]);
            }
            like(DynamicImage::ImageRgba8(rgba), img)
        }
        Filter::Brightness => brighter(img, amount),
        Filter::Contrast => colours_only(img.adjust_contrast(amount), img),
    }
}

/// Brightness, `amount` from -100 to 100, at the depth of the image: `image`'s `brighten` adds a
/// number in units of the sample, so a 16-bit image got 257 times less than the 8-bit preview,
/// and on floats it cut the samples to 0 or 1 and a NaN ended in a panic (audit 5.5).
fn brighter(img: &DynamicImage, amount: f32) -> DynamicImage {
    let eight = (amount * 2.55).round() as i32;
    match img {
        DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_) => img.brighten(eight * 257),
        DynamicImage::ImageRgb32F(b) => DynamicImage::ImageRgb32F(floats(b, |c| (c + amount / 100.0).max(0.0))),
        DynamicImage::ImageRgba32F(b) => DynamicImage::ImageRgba32F(floats(b, |c| (c + amount / 100.0).max(0.0))),
        _ => img.brighten(eight),
    }
}

/// Sharpen by `sigma`, leaving alone what differs from its blur by 2 steps of 8 bits or less, at
/// any depth: `image`'s `unsharpen` takes the threshold in units of the sample and cuts floats to
/// whole numbers (nothing sharpened, a NaN a panic).
fn sharpened(img: &DynamicImage, sigma: f32) -> DynamicImage {
    const THRESHOLD: i32 = 2;
    match img {
        DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_) | DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_) => img.unsharpen(sigma, THRESHOLD * 257),
        DynamicImage::ImageRgb32F(b) => DynamicImage::ImageRgb32F(sharp_floats(b, sigma, THRESHOLD as f32 / 255.0)),
        DynamicImage::ImageRgba32F(b) => DynamicImage::ImageRgba32F(sharp_floats(b, sigma, THRESHOLD as f32 / 255.0)),
        _ => img.unsharpen(sigma, THRESHOLD),
    }
}

/// `unsharpen` on floats: each sample pushed away from its blur when they differ by more than
/// `step`, never below 0.
fn sharp_floats<P: image::Pixel<Subpixel = f32> + 'static>(b: &ImageBuffer<P, Vec<f32>>, sigma: f32, step: f32) -> ImageBuffer<P, Vec<f32>> {
    let mut out = image::imageops::blur(b, sigma);
    for (d, &c) in out.iter_mut().zip(b.iter()) {
        let diff = c - *d;
        // A NaN compares as false: kept as it was.
        *d = if diff.abs() > step { (c + diff).max(0.0) } else { c };
    }
    out
}

/// Each colour of `b` through `f`; the alpha as it was.
fn floats<P: image::Pixel<Subpixel = f32>>(b: &ImageBuffer<P, Vec<f32>>, f: impl Fn(f32) -> f32) -> ImageBuffer<P, Vec<f32>> {
    let mut out = b.clone();
    let n = usize::from(P::CHANNEL_COUNT);
    let colours = if P::HAS_ALPHA { n - 1 } else { n };
    for p in out.chunks_exact_mut(n) {
        for c in &mut p[..colours] {
            *c = f(*c);
        }
    }
    out
}

/// `out` with the alpha of `img` back: `image`'s contrast changes every channel, so a negative
/// one made its part see-through (audit 5.4). A filter changes the colours only.
fn colours_only(mut out: DynamicImage, img: &DynamicImage) -> DynamicImage {
    fn alpha<P: image::Pixel>(out: &mut ImageBuffer<P, Vec<P::Subpixel>>, img: &ImageBuffer<P, Vec<P::Subpixel>>) {
        let n = usize::from(P::CHANNEL_COUNT);
        for (o, i) in out.chunks_exact_mut(n).zip(img.chunks_exact(n)) {
            o[n - 1] = i[n - 1];
        }
    }
    match (&mut out, img) {
        (DynamicImage::ImageLumaA8(o), DynamicImage::ImageLumaA8(i)) => alpha(o, i),
        (DynamicImage::ImageLumaA16(o), DynamicImage::ImageLumaA16(i)) => alpha(o, i),
        (DynamicImage::ImageRgba8(o), DynamicImage::ImageRgba8(i)) => alpha(o, i),
        (DynamicImage::ImageRgba16(o), DynamicImage::ImageRgba16(i)) => alpha(o, i),
        (DynamicImage::ImageRgba32F(o), DynamicImage::ImageRgba32F(i)) => alpha(o, i),
        _ => {}
    }
    out
}

/// Blur, Pixelate and Sharpen mix each pixel with its neighbours. Where some are transparent (the
/// empty corners of a straightened photo, a transparent PNG) each counts as much as its alpha, so
/// a transparent one brings in no colour (its black did), and the alpha stays as it was: a filter
/// changes the colours, never what is transparent. The empty corners stay empty and the edge of
/// the photo by them opaque, on the screen as in the saved file (the file blurred the edge of the
/// photo into the corners, see-through there).
fn mixing(img: &DynamicImage, f: impl Fn(&DynamicImage) -> DynamicImage) -> DynamicImage {
    fn sized<P: image::Pixel>((w, h): (u32, u32), raw: Vec<P::Subpixel>) -> ImageBuffer<P, Vec<P::Subpixel>> {
        ImageBuffer::from_raw(w, h, raw).expect("buffer of the right size")
    }
    let opaque = match img {
        DynamicImage::ImageRgba8(b) => b.pixels().all(|p| p[3] == u8::MAX),
        DynamicImage::ImageRgba16(b) => b.pixels().all(|p| p[3] == u16::MAX),
        DynamicImage::ImageRgba32F(b) => b.pixels().all(|p| p[3] >= 1.0),
        _ => true,
    };
    if opaque {
        return f(img);
    }
    let (w, h) = img.dimensions();
    match img {
        DynamicImage::ImageRgba8(b) => DynamicImage::ImageRgba8(sized((w, h), weighted(b.as_raw(), 255.0, |v| v.round() as u8, |raw| f(&DynamicImage::ImageRgba8(sized((w, h), raw))).to_rgba8().into_raw()))),
        DynamicImage::ImageRgba16(b) => DynamicImage::ImageRgba16(sized((w, h), weighted(b.as_raw(), 65535.0, |v| v.round() as u16, |raw| f(&DynamicImage::ImageRgba16(sized((w, h), raw))).to_rgba16().into_raw()))),
        DynamicImage::ImageRgba32F(b) => DynamicImage::ImageRgba32F(sized((w, h), weighted(b.as_raw(), 1.0, |v| v, |raw| f(&DynamicImage::ImageRgba32F(sized((w, h), raw))).to_rgba32f().into_raw()))),
        _ => f(img),
    }
}

/// `f` on the RGBA samples with the colours times their alpha (`max` the largest value, `back`
/// from a value to a sample), divided again by the alpha it gives back; the alpha of `samples`
/// kept.
fn weighted<S: Copy + Into<f32>>(samples: &[S], max: f32, back: fn(f32) -> S, f: impl Fn(Vec<S>) -> Vec<S>) -> Vec<S> {
    let mut times = samples.to_vec();
    for p in times.as_chunks_mut::<4>().0 {
        let a = p[3].into() / max;
        for v in &mut p[..3] {
            *v = back((*v).into() * a);
        }
    }
    let out = f(times);
    let mut kept = samples.to_vec();
    let top = if max > 1.0 { max } else { f32::MAX };
    for (p, o) in kept.as_chunks_mut::<4>().0.iter_mut().zip(out.as_chunks::<4>().0) {
        // A transparent pixel stays as it was, its colour too.
        let a = o[3].into() / max;
        if a > 0.0 && p[3].into() > 0.0 {
            for (v, c) in p[..3].iter_mut().zip(&o[..3]) {
                *v = back(((*c).into() / a).min(top));
            }
        }
    }
    kept
}

/// `out` in the colour type of `like` (grey becomes RGB again; the alpha kept or dropped).
fn like(out: DynamicImage, like: &DynamicImage) -> DynamicImage {
    use image::ColorType::*;
    match like.color() {
        Rgb8 => DynamicImage::ImageRgb8(out.to_rgb8()),
        Rgba8 => DynamicImage::ImageRgba8(out.to_rgba8()),
        Rgb16 => DynamicImage::ImageRgb16(out.to_rgb16()),
        Rgba16 => DynamicImage::ImageRgba16(out.to_rgba16()),
        Rgb32F => DynamicImage::ImageRgb32F(out.to_rgb32f()),
        Rgba32F => DynamicImage::ImageRgba32F(out.to_rgba32f()),
        _ => out,
    }
}

#[cfg(test)]
mod tests {
    use image::RgbImage;

    use super::*;

    fn stripes() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(40, 20, |x, _| if (x / 2) % 2 == 0 { image::Rgb([250, 20, 20]) } else { image::Rgb([20, 20, 250]) }))
    }

    /// Each filter does what its name says, on the same kind of image.
    #[test]
    fn filters_do_what_they_say() {
        let img = stripes();
        let px = |i: &DynamicImage, x| i.get_pixel(x, 10).0;
        // Blurred: the stripes 2 wide mix towards purple.
        let b = apply(&img, Filter::Blur, 3.0, 1.0);
        assert!(px(&b, 10)[0].abs_diff(px(&b, 10)[2]) < 60, "{:?}", px(&b, 10));
        // In blocks of 8: the 8 pixels of a block the same.
        let p = apply(&img, Filter::Pixelate, 8.0, 1.0);
        assert!((0..8).all(|x| px(&p, x) == px(&p, 0)));
        // Grey: the channels equal, still RGB.
        let g = apply(&img, Filter::Gray, 0.0, 1.0);
        assert_eq!(g.color(), image::ColorType::Rgb8);
        let [r, gg, bb, _] = px(&g, 0);
        assert!(r == gg && gg == bb);
        // Brighter and darker.
        assert!(px(&apply(&img, Filter::Brightness, 50.0, 1.0), 0)[1] > 20 && px(&apply(&img, Filter::Brightness, -50.0, 1.0), 0)[0] < 250);
        // Sepia: warm, red over blue.
        let s = apply(&img, Filter::Sepia, 0.0, 1.0);
        assert!(px(&s, 2)[0] > px(&s, 2)[2]);
        // 16 bits stay 16 bits.
        let deep = DynamicImage::ImageRgb16(img.to_rgb16());
        assert_eq!(apply(&deep, Filter::Gray, 0.0, 1.0).color(), image::ColorType::Rgb16);
    }

    /// Brightness and Sharpen on a 16-bit or a floating-point image (a RAW, a 16-bit PNG or TIFF,
    /// an OpenEXR) do what they do on the 8-bit preview: Brightness added 50 out of 65 535 (nothing
    /// seen) or cut the floats to 0 or 1, and Sharpen's threshold was in units of the sample
    /// (audit 5.5). Stripes 1 pixel wide 3 apart, which the threshold leaves as they are, and
    /// stripes far apart, which it sharpens.
    #[test]
    fn every_depth_alike() {
        let faint = DynamicImage::ImageRgb8(RgbImage::from_fn(40, 20, |x, _| if x % 2 == 0 { image::Rgb([100, 120, 140]) } else { image::Rgb([103, 123, 143]) }));
        for img in [stripes(), faint] {
            for (f, amount) in [(Filter::Brightness, 50.0), (Filter::Brightness, -40.0), (Filter::Sharpen, 2.0), (Filter::Contrast, 30.0)] {
                let eight = apply(&img, f, amount, 1.0).to_rgb8();
                for deep in [DynamicImage::ImageRgb16(img.to_rgb16()), DynamicImage::ImageRgba16(img.to_rgba16()), DynamicImage::ImageRgb32F(img.to_rgb32f()), DynamicImage::ImageRgba32F(img.to_rgba32f())] {
                    let out = apply(&deep, f, amount, 1.0);
                    assert_eq!(out.color(), deep.color(), "{f:?}: the same depth");
                    for (x, y, p) in out.to_rgb8().enumerate_pixels() {
                        let e = eight.get_pixel(x, y);
                        assert!(p.0.iter().zip(e.0).all(|(a, b)| a.abs_diff(b) <= 1), "{f:?} {amount} {:?} at {x},{y}: {p:?}, 8 bits {e:?}", deep.color());
                    }
                }
            }
        }
    }

    /// A floating-point image with a pixel that is not a number and an infinite one: every filter
    /// gives an image (Brightness and Sharpen ended in a panic, and the save in an error).
    #[test]
    fn floats_not_numbers() {
        let mut img = image::Rgb32FImage::from_pixel(16, 8, image::Rgb([0.5, 0.25, 0.75]));
        img.put_pixel(3, 3, image::Rgb([f32::NAN, f32::INFINITY, f32::NEG_INFINITY]));
        let img = DynamicImage::ImageRgb32F(img);
        for f in Filter::ALL {
            let amount = f.amount(16.0).map_or(0.0, |(.., start)| start);
            assert_eq!(apply(&img, f, amount, 1.0).dimensions(), (16, 8), "{f:?}");
        }
    }
}
