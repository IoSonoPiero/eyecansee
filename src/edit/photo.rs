//! What some objects of the editor take from the photo itself, without the interface: the part
//! a lens magnifies. The same function makes the preview (smaller) and the saved file (full
//! size).

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

#[cfg(test)]
mod tests {
    use image::{Rgb, RgbImage};

    use super::*;
    use crate::edit::doc::Pt;

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
