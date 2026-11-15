//! The filters of the editor, without the interface: what each does to a piece of image. A filter
//! is an object over a part of the photo (or all of it), under the objects drawn: the preview is
//! made from the photo at a smaller size, the saved file at full size (`edit::save`).

use image::{DynamicImage, GenericImageView, Rgba};

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
        Filter::Blur => img.blur(px),
        Filter::Pixelate => {
            let (w, h) = img.dimensions();
            let block = px.max(1.0);
            let small = img.resize_exact(((w as f32 / block).ceil() as u32).max(1), ((h as f32 / block).ceil() as u32).max(1), image::imageops::FilterType::Triangle);
            small.resize_exact(w, h, image::imageops::FilterType::Nearest)
        }
        Filter::Sharpen => img.unsharpen(px, 2),
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
        Filter::Brightness => img.brighten((amount * 2.55).round() as i32),
        Filter::Contrast => img.adjust_contrast(amount),
    }
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
}
