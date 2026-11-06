//! What some objects of the editor take from the photo itself, without the interface: the part
//! a lens magnifies, the highlighter's multiply. The same functions make the preview (smaller)
//! and the saved file (full size).

use std::hash::{Hash, Hasher};

use image::{DynamicImage, RgbaImage, imageops};

use super::doc::{Area, Doc, Pt, Shape};

/// A filter's pixels over its box (at any size), for a lens over it.
pub type Filtered<'a> = (Area, &'a RgbaImage);

/// What a lens over `area` shows: the photo under a box `zoom` times smaller around the same
/// centre, stretched smoothly to the size of the lens, at most `max` pixels a side; with the
/// filters (`filtered`) and the cover boxes of `doc` on it, whatever their order, so it never
/// shows what they hide. Not the other objects drawn.
pub fn lens(doc: &Doc, photo: &DynamicImage, area: &Area, zoom: f32, max: u32, filtered: &[Filtered]) -> RgbaImage {
    let small = Area { w: area.w / zoom, h: area.h / zoom, ..*area };
    let k = (max as f32 / area.w.max(area.h)).min(1.0);
    // Read at the size it is shown, never more than a pixel for a pixel of the photo.
    let mut part = doc.pixels_under_at(photo, &small, (k * zoom).min(1.0));
    hide(doc, &mut part, &small, filtered);
    let (w, h) = (((area.w * k).round() as u32).max(1), ((area.h * k).round() as u32).max(1));
    imageops::resize(&part, w, h, imageops::FilterType::Triangle)
}

/// The filters and the cover boxes on `part`, the pixels of the box `a` along its own sides: a
/// pixel of a filter's box takes the filter's pixel there, a pixel a cover box touches (within a
/// pixel of `part`) takes its colour.
fn hide(doc: &Doc, part: &mut RgbaImage, a: &Area, filtered: &[Filtered]) {
    let covers: Vec<(Area, [u8; 3])> = doc.objects.iter().filter_map(|o| if let Shape::Cover(c) = o.shape { Some((c, o.style.color)) } else { None }).collect();
    if covers.is_empty() && filtered.is_empty() {
        return;
    }
    let (w, h) = (part.width() as f32, part.height() as f32);
    let reach = (a.w / w).max(a.h / h);
    let (s, c) = a.rot.to_radians().sin_cos();
    for (u, v, px) in part.enumerate_pixels_mut() {
        let (x, y) = ((u as f32 + 0.5) * a.w / w - a.w / 2.0, (v as f32 + 0.5) * a.h / h - a.h / 2.0);
        let p = Pt::new(a.centre.x + x * c - y * s, a.centre.y + x * s + y * c);
        for (box_, pixels) in filtered {
            if let Some((fx, fy)) = local(box_, p, 0.0) {
                let (fw, fh) = pixels.dimensions();
                *px = *pixels.get_pixel(((fx * fw as f32 / box_.w) as u32).min(fw - 1), ((fy * fh as f32 / box_.h) as u32).min(fh - 1));
            }
        }
        for (box_, [r, g, b]) in &covers {
            if local(box_, p, reach).is_some() {
                *px = image::Rgba([*r, *g, *b, 255]);
            }
        }
    }
}

/// Where `p` is in the box `a`, from its top left corner along its sides; `None` outside it by
/// more than `tol`.
fn local(a: &Area, p: Pt, tol: f32) -> Option<(f32, f32)> {
    let d = super::geom::turned(Pt::new(p.x - a.centre.x, p.y - a.centre.y), -a.rot);
    (d.x.abs() <= a.w / 2.0 + tol && d.y.abs() <= a.h / 2.0 + tol).then(|| ((d.x + a.w / 2.0).clamp(0.0, a.w), (d.y + a.h / 2.0).clamp(0.0, a.h)))
}

/// What a lens's picture depends on besides its box and zoom: the cover boxes and the filters
/// (with whether their preview is made). When it changes, the previews of the lenses are made
/// again.
pub fn hiders_key(doc: &Doc) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    for o in &doc.objects {
        let (area, more) = match &o.shape {
            Shape::Cover(a) => (a, [o.style.color[0] as f32, o.style.color[1] as f32, o.style.color[2] as f32, 0.0]),
            Shape::Filter { area, filter, amount, preview } => (area, [*filter as u8 as f32, *amount, preview.0.is_some() as u8 as f32, 1.0]),
            _ => continue,
        };
        for v in [area.centre.x, area.centre.y, area.w, area.h, area.rot].into_iter().chain(more) {
            v.to_bits().hash(&mut h);
        }
    }
    h.finish()
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

    /// A lens over a cover box shows the box, also where only its edge is in view; over a filter
    /// it shows the filter's pixels; elsewhere the photo.
    #[test]
    fn lens_shows_what_hides() {
        let photo = DynamicImage::ImageRgb8(RgbImage::from_pixel(60, 40, Rgb([255; 3])));
        let mut doc = Doc::new(60, 40, None);
        doc.objects.push(crate::edit::doc::Obj { shape: Shape::Cover(Area { centre: Pt::new(-2.0, 0.0), w: 4.0, h: 4.0, rot: 0.0 }), style: crate::edit::doc::Style { color: [0, 0, 255], ..Default::default() } });
        let area = Area { centre: Pt::new(0.0, 0.0), w: 20.0, h: 20.0, rot: 0.0 };
        let filtered = RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255]));
        let f = Area { centre: Pt::new(4.0, 0.0), w: 4.0, h: 4.0, rot: 0.0 };
        let l = lens(&doc, &photo, &area, 2.0, 8000, &[(f, &filtered)]);
        // The lens's 10 × 10 of photo from -5 to 5, twice as large: the cover from -4 to 0 at 2..10,
        // the filter from 2 to 6 at 14..20.
        assert_eq!((l.get_pixel(5, 10).0, l.get_pixel(17, 10).0, l.get_pixel(10, 2).0), ([0, 0, 255, 255], [0, 255, 0, 255], [255; 4]));
    }

    /// A lens 3 times: a white square of 4 pixels in the middle of the photo fills 12 in the
    /// lens; the preview is the same at a smaller size.
    #[test]
    fn lens_magnifies() {
        let photo = DynamicImage::ImageRgb8(RgbImage::from_fn(60, 40, |x, y| if (28..32).contains(&x) && (18..22).contains(&y) { Rgb([255; 3]) } else { Rgb([0; 3]) }));
        let doc = Doc::new(60, 40, None);
        let area = Area { centre: Pt::new(0.0, 0.0), w: 30.0, h: 30.0, rot: 0.0 };
        let full = lens(&doc, &photo, &area, 3.0, 8000, &[]);
        assert_eq!(full.dimensions(), (30, 30));
        // Smoothed at the edges: white well inside the 9..21 of the square, black outside it.
        assert_eq!((full.get_pixel(15, 15).0, full.get_pixel(11, 11).0, full.get_pixel(3, 3).0, full.get_pixel(26, 15).0), ([255; 4], [255; 4], [0, 0, 0, 255], [0, 0, 0, 255]));
        let small = lens(&doc, &photo, &area, 3.0, 15, &[]);
        assert_eq!((small.dimensions(), small.get_pixel(7, 7).0), ((15, 15), [255; 4]));
    }
}
