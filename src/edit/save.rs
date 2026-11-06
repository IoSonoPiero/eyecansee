//! Saving an edit, without the interface: from the document and the original to the bytes of the
//! copy and the name proposed for it. Nothing is written here (disk, phone or server: the caller).
//! The original is cloned and turned, rotated and cropped as "Rotate and crop" does it; the cover
//! boxes are written into the pixels, solid and on whole pixels; each other object is drawn on
//! its own box (`render`) and laid over the image, 8 bits, 16 bits or floating point. Metadata as
//! every save since 0.73.1 (`Original::encode`), without what described the parts not written
//! (Motion Photo video, gain map), unless the copy stays Ultra HDR.

use std::collections::HashSet;

use image::{DynamicImage, GenericImage, GenericImageView};
use resvg::{tiny_skia, usvg};

use super::doc::{Area, Doc, Obj, Pixels, Preview, Shape};
use super::render::{self, View};
use crate::encode::{Format, Options, Original};

/// What the copy keeps of the original's depth, for a line in the Save window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Depth {
    Kept,
    /// 16 bits per colour (a RAW, a 16-bit TIFF…) in a format of 8 (JPEG…).
    To8Bits,
    /// Floating point (OpenEXR, Radiance HDR) in 16 bits: what is brighter than white is cut.
    HdrCut,
}

#[derive(Debug)]
pub struct Saved {
    pub data: Vec<u8>,
    /// The name proposed, in the folder of the original.
    pub name: String,
    /// Encoded again with loss (the Save window says so).
    pub lossy: bool,
    pub depth: Depth,
}

/// The copy of `name` (the file `bytes`, decoded as `original`) with `doc` done to it. `word` is
/// "edited" in the language of the interface (`edit-copy-word`), `taken` the names in the folder;
/// `options` gives the fonts (`decode::usvg_options`).
pub fn save(doc: &Doc, original: &Original, bytes: &[u8], name: &str, word: &str, taken: &[String], options: &usvg::Options) -> Result<Saved, String> {
    if animated(bytes, name) {
        return Err(format!("{name}: animated or with several pages"));
    }
    let same = Format::of_file(name);
    let size = (original.image.width(), original.image.height());
    // Flips and quarter turns alone: as the viewer's Save, a JPEG without re-encoding when its
    // size allows it, an Ultra HDR photo still HDR.
    if same.is_some() && only_turned(doc, size) && (crate::save_back::can_turn_in_place(bytes, name) || crate::ultrahdr::is(bytes)) {
        let (data, lossy) = crate::save_back::turned_file(bytes, name, doc.turn)?;
        return Ok(Saved { data, name: copy_name(name, None, word, taken), lossy, depth: Depth::Kept });
    }
    let mut o = Original::clone(original);
    let (float, deep) = (is_float(&o.image), is_deep(&o.image));
    let holes = edited(doc, &original.image, &mut o.image, options);
    if same == Some(Format::Jpeg) && crate::ultrahdr::is(bytes) {
        let data = crate::ultrahdr::edited(bytes, o.image, &[doc.edit()], &holes)?;
        return Ok(Saved { data, name: copy_name(name, None, word, taken), lossy: true, depth: Depth::Kept });
    }
    let format = target(name, float, transparent(&o.image));
    if float {
        o.image = if o.image.color().has_alpha() { DynamicImage::ImageRgba16(o.image.to_rgba16()) } else { DynamicImage::ImageRgb16(o.image.to_rgb16()) };
    }
    let webp_lossless = format == Format::Webp && bytes.windows(4).any(|w| w == b"VP8L");
    let opts = Options { format, jpeg_quality: 95, webp_lossless, ..Options::default() };
    let lossy = matches!(format, Format::Jpeg | Format::Avif | Format::Gif) || (format == Format::Webp && !webp_lossless);
    let data = o.alone().encode(&opts)?;
    let ext = if same.is_some() { None } else { Some(format.ext()) };
    Ok(Saved { data, name: copy_name(name, ext, word, taken), lossy, depth: depth(float, deep, format) })
}

/// The format of the copy: the original's when EyeCanSee writes it back; otherwise (HEIC, JPEG
/// XL, RAW, PSD, OpenEXR…) PNG with transparency or floating point, JPEG without.
fn target(name: &str, float: bool, transparent: bool) -> Format {
    Format::of_file(name).unwrap_or(if float || transparent { Format::Png } else { Format::Jpeg })
}

/// The result, as it would be saved: for the clipboard.
pub fn result(doc: &Doc, original: &Original, options: &usvg::Options) -> DynamicImage {
    let mut img = original.image.clone();
    edited(doc, &original.image, &mut img, options);
    img
}

/// `img` (a copy of `photo`, upright as read) turned, rotated and cropped, with the objects; the
/// boxes of `paint`.
fn edited(doc: &Doc, photo: &DynamicImage, img: &mut DynamicImage, options: &usvg::Options) -> Vec<[f32; 4]> {
    doc.edit().apply(img);
    paint(doc, photo, img, options)
}

fn depth(float: bool, deep: bool, format: Format) -> Depth {
    match () {
        _ if float => Depth::HdrCut,
        _ if deep && !matches!(format, Format::Png | Format::Tiff) => Depth::To8Bits,
        _ => Depth::Kept,
    }
}

/// Animated or with several pages: only the first would be saved (the menu item is off for them).
pub(crate) fn animated(bytes: &[u8], name: &str) -> bool {
    matches!(Format::of_file(name), Some(Format::Png | Format::Webp | Format::Tiff | Format::Avif | Format::Gif)) && !crate::save_back::can_turn_in_place(bytes, name)
}

/// Only flips and quarter turns: no angle, no object, the crop all the image.
fn only_turned(doc: &Doc, (w, h): (u32, u32)) -> bool {
    let whole = if doc.turn.sideways() { (h, w) } else { (w, h) };
    doc.angle.rem_euclid(360.0) == 0.0 && doc.objects.is_empty() && doc.crop.pixels() == whole && doc.crop.cx.abs() < 0.5 && doc.crop.cy.abs() < 0.5
}

/// "photo (edited).jpg", "photo (edited 2).jpg"… the first free in `taken` (letter case aside, as
/// on Windows and most phones); `ext` replaces the extension when the format changes. A copy of a
/// copy is numbered, not "(edited) (edited)".
pub fn copy_name(name: &str, ext: Option<&str>, word: &str, taken: &[String]) -> String {
    let (stem, old) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i + 1..]),
        _ => (name, ""),
    };
    let ext = ext.unwrap_or(old);
    let stem = earlier_copy(stem, word).unwrap_or(stem);
    let taken: HashSet<String> = taken.iter().map(|t| t.to_lowercase()).collect();
    (1..)
        .map(|n| {
            let label = if n == 1 { format!("{stem} ({word})") } else { format!("{stem} ({word} {n})") };
            if ext.is_empty() { label } else { format!("{label}.{ext}") }
        })
        .find(|full| !taken.contains(&full.to_lowercase()))
        .expect("a free name")
}

/// The stem before " (edited)" or " (edited 3)", if `stem` is a copy already.
fn earlier_copy<'a>(stem: &'a str, word: &str) -> Option<&'a str> {
    let inner = stem.strip_suffix(')')?;
    let (before, label) = inner.rsplit_once(" (")?;
    let rest = label.strip_prefix(word)?;
    (rest.is_empty() || rest.strip_prefix(' ').is_some_and(|n| n.parse::<u32>().is_ok())).then_some(before)
}

/// The filters, highlighters and spotlights on the photo, then cover boxes into the pixels and
/// the other objects over them, in their order (a lens with its picture made from `photo` at
/// full size). Returns the boxes whose gain an Ultra HDR photo sets to the lowest: every cover
/// box, every object drawn and the filters that hide (left, top, right, bottom in pixels of the
/// result).
fn paint(doc: &Doc, photo: &DynamicImage, img: &mut DynamicImage, options: &usvg::Options) -> Vec<[f32; 4]> {
    let size = (photo.width(), photo.height());
    if doc.objects.is_empty() {
        return Vec::new();
    }
    in_colour(img);
    let (iw, ih) = if doc.turn.sideways() { (size.1, size.0) } else { size };
    let corner = crate::rotate::corner(iw as f32, ih as f32, doc.crop);
    let mut holes = Vec::new();
    // The filters first, at full size on the photo: the objects drawn stay over them.
    for o in &doc.objects {
        if let Shape::Filter { area, filter, amount, .. } = &o.shape {
            let b = filter_in_place(img, area, doc.angle, corner, *filter, *amount);
            if filter.hides() {
                holes.push(b);
            }
        }
    }
    // The highlighters, multiplied into the pixels; an Ultra HDR photo keeps its gain there.
    for o in &doc.objects {
        if let Shape::Highlight { area, .. } = &o.shape {
            highlight_in_place(img, area, doc.angle, corner, super::photo::highlight_factor(o.style.color, o.style.opacity));
        }
    }
    // The spotlights over the whole result, the transparent pixels left as they are; an Ultra HDR
    // photo keeps its gain there (darkened, the HDR one is darkened as much).
    let spots: Vec<Obj> = doc.objects.iter().filter(|o| matches!(o.shape, Shape::Spotlight(_))).cloned().collect();
    if !spots.is_empty() {
        let all = Doc { turn: doc.turn, angle: doc.angle, crop: doc.crop, fill: doc.fill, objects: spots };
        if let Some(p) = render::render(&all, size, &View { x: 0.0, y: 0.0, w: img.width(), h: img.height(), scale: 1.0 }, options) {
            over(img, &p, 0, 0, true);
        }
    }
    // In the order they were drawn: a cover box hides what was drawn before it, what comes after
    // it stays on top (as layers; user's choice of 2026-11-05).
    for o in doc.objects.iter().filter(|o| !o.shape.is_layer()) {
        if let Shape::Cover(area) = &o.shape {
            holes.push(cover(img, area, doc.angle, corner, o.style.color));
            continue;
        }
        let one = Doc { turn: doc.turn, angle: doc.angle, crop: doc.crop, fill: doc.fill, objects: vec![full_size(doc, photo, o)] };
        let Some(b) = render::bounds(&one, size, options) else { continue };
        let (x0, y0) = (b[0].floor().max(0.0), b[1].floor().max(0.0));
        let (x1, y1) = (b[2].ceil().min(img.width() as f32), b[3].ceil().min(img.height() as f32));
        if x1 > x0 && y1 > y0 {
            let view = View { x: x0, y: y0, w: (x1 - x0) as u32, h: (y1 - y0) as u32, scale: 1.0 };
            if let Some(p) = render::render(&one, size, &view, options) {
                over(img, &p, x0 as u32, y0 as u32, false);
            }
        }
        holes.push(b);
    }
    holes
}

/// The object as drawn in the saved file: a lens with its picture at full size.
fn full_size(doc: &Doc, photo: &DynamicImage, o: &Obj) -> Obj {
    match &o.shape {
        Shape::Lens { area, .. } => {
            let zoom = o.style.zoom.max(1) as f32;
            let pixels = Pixels::new(super::photo::lens(doc, photo, area, zoom, 8000));
            Obj { shape: Shape::Lens { area: *area, preview: Preview(Some((*area, zoom, pixels))) }, style: o.style }
        }
        _ => o.clone(),
    }
}

/// A grey image gets colours, for objects that have them.
fn in_colour(img: &mut DynamicImage) {
    *img = match img {
        DynamicImage::ImageLuma8(_) => DynamicImage::ImageRgb8(img.to_rgb8()),
        DynamicImage::ImageLumaA8(_) => DynamicImage::ImageRgba8(img.to_rgba8()),
        DynamicImage::ImageLuma16(_) => DynamicImage::ImageRgb16(img.to_rgb16()),
        DynamicImage::ImageLumaA16(_) => DynamicImage::ImageRgba16(img.to_rgba16()),
        _ => return,
    };
}

/// The filter on the pixels of the result under the box, at full size; returns its box. A box
/// upright in the result is filtered in one piece, keeping the depth of the image; a turned one
/// pixel by pixel inside it (through 8 bits there).
fn filter_in_place(img: &mut DynamicImage, area: &Area, angle: f32, (left, top): (f32, f32), f: crate::edit::filter::Filter, amount: f32) -> [f32; 4] {
    let (s, c) = angle.to_radians().sin_cos();
    let (cx, cy) = (area.centre.x * c - area.centre.y * s - left, area.centre.x * s + area.centre.y * c - top);
    let rot = area.rot + angle;
    let (rs, rc) = rot.to_radians().sin_cos();
    let (hw, hh) = (area.w / 2.0, area.h / 2.0);
    let (ex, ey) = (rc.abs() * hw + rs.abs() * hh, rs.abs() * hw + rc.abs() * hh);
    let bounds = [cx - ex, cy - ey, cx + ex, cy + ey];
    let (x0, y0) = (bounds[0].floor().max(0.0) as u32, bounds[1].floor().max(0.0) as u32);
    let (x1, y1) = ((bounds[2].ceil().max(0.0) as u32).min(img.width()), (bounds[3].ceil().max(0.0) as u32).min(img.height()));
    if x1 <= x0 || y1 <= y0 {
        return bounds;
    }
    let part = crate::edit::filter::apply(&img.crop_imm(x0, y0, x1 - x0, y1 - y0), f, amount, 1.0);
    if (rot / 90.0 - (rot / 90.0).round()).abs() * 90.0 < 1e-3 {
        image::imageops::replace(img, &part, x0 as i64, y0 as i64);
    } else {
        for (y, x) in (y0..y1).flat_map(|y| (x0..x1).map(move |x| (y, x))) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let (u, v) = (dx * rc + dy * rs, -dx * rs + dy * rc);
            if u.abs() < hw && v.abs() < hh {
                img.put_pixel(x, y, part.get_pixel(x - x0, y - y0));
            }
        }
    }
    bounds
}

/// The highlighter multiplied into the pixels of the result whose centre is inside its box, in
/// the image's own depth (`f` from `photo::highlight_factor`).
fn highlight_in_place(img: &mut DynamicImage, area: &Area, angle: f32, (left, top): (f32, f32), f: [f32; 3]) {
    let (s, c) = angle.to_radians().sin_cos();
    let (cx, cy) = (area.centre.x * c - area.centre.y * s - left, area.centre.x * s + area.centre.y * c - top);
    let (rs, rc) = (area.rot + angle).to_radians().sin_cos();
    let (hw, hh) = (area.w / 2.0, area.h / 2.0);
    let (ex, ey) = (rc.abs() * hw + rs.abs() * hh, rs.abs() * hw + rc.abs() * hh);
    let range = |lo: f32, hi: f32, max: u32| (lo.floor().max(0.0) as u32).min(max)..(hi.ceil().max(0.0) as u32).min(max);
    for y in range(cy - ey, cy + ey, img.height()) {
        for x in range(cx - ex, cx + ex, img.width()) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if (dx * rc + dy * rs).abs() < hw && (-dx * rs + dy * rc).abs() < hh {
                times(img, x, y, f);
            }
        }
    }
}

/// The colours of the pixel at `x`, `y` times `f`, its alpha as it was.
fn times(img: &mut DynamicImage, x: u32, y: u32, f: [f32; 3]) {
    fn each<S: Copy + Into<f32>>(p: &mut [S], f: [f32; 3], back: fn(f32) -> S) {
        for (v, k) in p.iter_mut().zip(f) {
            *v = back((*v).into() * k);
        }
    }
    match img {
        DynamicImage::ImageRgb8(b) => each(&mut b.get_pixel_mut(x, y).0, f, |v| v.round() as u8),
        DynamicImage::ImageRgba8(b) => each(&mut b.get_pixel_mut(x, y).0, f, |v| v.round() as u8),
        DynamicImage::ImageRgb16(b) => each(&mut b.get_pixel_mut(x, y).0, f, |v| v.round() as u16),
        DynamicImage::ImageRgba16(b) => each(&mut b.get_pixel_mut(x, y).0, f, |v| v.round() as u16),
        DynamicImage::ImageRgb32F(b) => each(&mut b.get_pixel_mut(x, y).0, f, |v| v),
        DynamicImage::ImageRgba32F(b) => each(&mut b.get_pixel_mut(x, y).0, f, |v| v),
        _ => {}
    }
}

/// The cover box solid in the pixels of the result: every pixel it touches even in part, opaque,
/// no soft edge, so nothing of what was under it is left. Returns its box, left, top, right,
/// bottom.
fn cover(img: &mut DynamicImage, area: &Area, angle: f32, (left, top): (f32, f32), [r, g, b]: [u8; 3]) -> [f32; 4] {
    let (s, c) = angle.to_radians().sin_cos();
    let (cx, cy) = (area.centre.x * c - area.centre.y * s - left, area.centre.x * s + area.centre.y * c - top);
    // Its angle in the result, exactly upright when it is within a thousandth of a degree.
    let mut rot = area.rot + angle;
    if (rot / 90.0 - (rot / 90.0).round()).abs() * 90.0 < 1e-3 {
        rot = (rot / 90.0).round() * 90.0;
    }
    let (rs, rc) = rot.to_radians().sin_cos();
    let (rs, rc) = (rs.abs().min(1.0), rc.abs().min(1.0));
    let (hw, hh) = (area.w / 2.0, area.h / 2.0);
    let (ex, ey) = (rc * hw + rs * hh, rs * hw + rc * hh);
    let bounds = [cx - ex, cy - ey, cx + ex, cy + ey];
    // A pixel touches the box when they overlap along its sides too (along x and y the loop's
    // range sees to it); `reach` is half a pixel measured along a side of the box.
    let reach = 0.5 * (rc + rs);
    let (sin, cos) = rot.to_radians().sin_cos();
    let range = |lo: f32, hi: f32, max: u32| (lo.floor().max(0.0) as u32).min(max)..(hi.ceil().max(0.0) as u32).min(max);
    for y in range(bounds[1], bounds[3], img.height()) {
        for x in range(bounds[0], bounds[2], img.width()) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
            if u.abs() < hw + reach && v.abs() < hh + reach {
                img.put_pixel(x, y, image::Rgba([r, g, b, 255]));
            }
        }
    }
    bounds
}

/// The pixmap `p` (premultiplied) laid over `img` with its top left corner at `x0`, `y0`, in the
/// image's own depth; with `keep_alpha` the transparency of `img` stays as it was ("atop").
fn over(img: &mut DynamicImage, p: &tiny_skia::Pixmap, x0: u32, y0: u32, keep_alpha: bool) {
    let w = img.width();
    // As three channels, the alpha is not touched.
    let c = |n: usize| if keep_alpha { 3 } else { n };
    match img {
        DynamicImage::ImageRgb8(b) => blend(b, (3, 3), w, 255.0, |v| v as u8, p, x0, y0),
        DynamicImage::ImageRgba8(b) => blend(b, (4, c(4)), w, 255.0, |v| v as u8, p, x0, y0),
        DynamicImage::ImageRgb16(b) => blend(b, (3, 3), w, 65535.0, |v| v as u16, p, x0, y0),
        DynamicImage::ImageRgba16(b) => blend(b, (4, c(4)), w, 65535.0, |v| v as u16, p, x0, y0),
        DynamicImage::ImageRgb32F(b) => blend(b, (3, 3), w, 1.0, |v| v, p, x0, y0),
        DynamicImage::ImageRgba32F(b) => blend(b, (4, c(4)), w, 1.0, |v| v, p, x0, y0),
        other => {
            *other = DynamicImage::ImageRgba8(other.to_rgba8());
            over(other, p, x0, y0, keep_alpha);
        }
    }
}

/// "Over" of `p` on the samples (`stride` 3 or 4 a pixel, `channels` blended: 3 leaves the
/// alpha alone, `max` the largest value); `back` from a value already rounded to a sample.
#[allow(clippy::too_many_arguments)]
fn blend<S: Copy + Into<f32>>(data: &mut [S], (stride, channels): (usize, usize), width: u32, max: f32, back: fn(f32) -> S, p: &tiny_skia::Pixmap, x0: u32, y0: u32) {
    let round = |v: f32| if max > 1.0 { (v * max + 0.5).clamp(0.0, max) } else { v };
    for (i, px) in p.pixels().iter().enumerate() {
        let a = px.alpha() as f32 / 255.0;
        if a == 0.0 {
            continue;
        }
        let (x, y) = (x0 + i as u32 % p.width(), y0 + i as u32 / p.width());
        let at = (y as usize * width as usize + x as usize) * stride;
        let Some(dst) = data.get_mut(at..at + channels) else { continue };
        let below = if channels == 4 { dst[3].into() / max } else { 1.0 };
        let out = a + below * (1.0 - a);
        for (k, src) in [px.red(), px.green(), px.blue()].into_iter().enumerate() {
            let v = (src as f32 / 255.0 + dst[k].into() / max * below * (1.0 - a)) / out;
            dst[k] = back(round(v));
        }
        if channels == 4 {
            dst[3] = back(round(out));
        }
    }
}

fn is_float(img: &DynamicImage) -> bool {
    matches!(img, DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_))
}

fn is_deep(img: &DynamicImage) -> bool {
    matches!(img, DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_) | DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_))
}

/// Some pixel not opaque.
fn transparent(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageRgba8(b) => b.pixels().any(|p| p[3] < u8::MAX),
        DynamicImage::ImageLumaA8(b) => b.pixels().any(|p| p[1] < u8::MAX),
        DynamicImage::ImageRgba16(b) => b.pixels().any(|p| p[3] < u16::MAX),
        DynamicImage::ImageLumaA16(b) => b.pixels().any(|p| p[1] < u16::MAX),
        DynamicImage::ImageRgba32F(b) => b.pixels().any(|p| p[3] < 1.0),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
