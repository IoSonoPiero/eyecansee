//! What some objects of the editor take from the photo itself, without the interface: the part
//! a lens magnifies, the highlighter's multiply. The same functions make the preview (smaller)
//! and the saved file (full size).

use image::{DynamicImage, RgbaImage, imageops};

use super::doc::{Area, Doc};

/// What a lens over `area` shows: the photo under a box `zoom` times smaller around the same
/// centre, stretched smoothly to the size of the lens, at most `max` pixels a side. The photo
/// only, without the objects drawn and the filters.
pub fn lens(doc: &Doc, photo: &DynamicImage, area: &Area, zoom: f32, max: u32) -> RgbaImage {
    let small = Area { w: area.w / zoom, h: area.h / zoom, ..*area };
    let part = doc.pixels_under(photo, &small);
    let k = (max as f32 / area.w.max(area.h)).min(1.0);
    let (w, h) = (((area.w * k).round() as u32).max(1), ((area.h * k).round() as u32).max(1));
    imageops::resize(&part, w, h, imageops::FilterType::Triangle)
}

/// What the highlighter does to each colour of a pixel: times this. The photo multiplied by
/// `colour`, at `opacity` (1: in full; white becomes the colour, black stays black).
pub fn highlight_factor(colour: [u8; 3], opacity: f32) -> [f32; 3] {
    colour.map(|c| 1.0 - opacity * (1.0 - c as f32 / 255.0))
}

/// The highlighter's preview: `colour` multiplied in full into the pixels, the alpha as it was;
/// its opacity is the drawing's (`highlight_factor` at that opacity).
pub fn highlighted(pixels: &mut RgbaImage, colour: [u8; 3]) {
    let f = highlight_factor(colour, 1.0);
    for p in pixels.pixels_mut() {
        for k in 0..3 {
            p[k] = (p[k] as f32 * f[k]).round() as u8;
        }
    }
}

/// The colour as one number, exact in an `f32`: what a highlighter's preview was made for.
pub fn colour_key([r, g, b]: [u8; 3]) -> f32 {
    (r as u32 * 65536 + g as u32 * 256 + b as u32) as f32
}

#[cfg(test)]
mod tests {
    use image::{Rgb, RgbImage};

    use super::*;
    use crate::edit::doc::Pt;

    /// The highlighter: yellow keeps black black, makes white yellow and grey a darker yellow;
    /// at half opacity half way.
    #[test]
    fn multiply() {
        let mut px = RgbaImage::from_fn(3, 1, |x, _| {
            let v = [0, 255, 128][x as usize];
            image::Rgba([v, v, v, 200])
        });
        highlighted(&mut px, [255, 230, 0]);
        assert_eq!((px.get_pixel(0, 0).0, px.get_pixel(1, 0).0, px.get_pixel(2, 0).0), ([0, 0, 0, 200], [255, 230, 0, 200], [128, 115, 0, 200]));
        assert_eq!(highlight_factor([255, 0, 128], 0.5), [1.0, 0.5, 0.5 + 0.5 * 128.0 / 255.0]);
    }

    /// A lens 3 times: a white square of 4 pixels in the middle of the photo fills 12 in the
    /// lens; the preview is the same at a smaller size.
    #[test]
    fn lens_magnifies() {
        let photo = DynamicImage::ImageRgb8(RgbImage::from_fn(60, 40, |x, y| if (28..32).contains(&x) && (18..22).contains(&y) { Rgb([255; 3]) } else { Rgb([0; 3]) }));
        let doc = Doc::new(60, 40, None);
        let area = Area { centre: Pt::new(0.0, 0.0), w: 30.0, h: 30.0, rot: 0.0 };
        let full = lens(&doc, &photo, &area, 3.0, 8000);
        assert_eq!(full.dimensions(), (30, 30));
        // Smoothed at the edges: white well inside the 9..21 of the square, black outside it.
        assert_eq!((full.get_pixel(15, 15).0, full.get_pixel(11, 11).0, full.get_pixel(3, 3).0, full.get_pixel(26, 15).0), ([255; 4], [255; 4], [0, 0, 0, 255], [0, 0, 0, 255]));
        let small = lens(&doc, &photo, &area, 3.0, 15);
        assert_eq!((small.dimensions(), small.get_pixel(7, 7).0), ((15, 15), [255; 4]));
    }
}
