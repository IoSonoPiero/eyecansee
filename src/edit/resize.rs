//! Resize (#114), without the interface: the size asked in the window (`Form`) and the document
//! made to it (`resized`). The photo is resampled from the original, with the filters and the
//! sizes of Convert and resize (`batch::Filter`, `batch::target`); what is drawn on it and the
//! crop are scaled by as much, so the editor goes on in pixels of the new size. The size is a part
//! of the document (`Doc::scale`): Undo and Redo take it back and forth like any other step, and
//! every save starts again from the original (`at_size`), never from a photo resized twice.

use std::borrow::Cow;

use crate::batch::{self, Filter, Size};
use crate::encode::Original;
use crate::rotate::Crop;

use super::doc::{Area, Doc, Obj, Preview, Pt, Shape};

/// No side longer, as a new image.
pub const MAX_SIDE: u32 = crate::new_image::MAX_SIDE;
/// As Convert and resize.
pub const MAX_PERCENT: u32 = 1000;

/// The photo of the document: the original's pixels (upright as read, before the flips and
/// turns) resampled to `w` × `h` with `filter`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scale {
    pub w: u32,
    pub h: u32,
    pub filter: Filter,
}

/// The original resampled as `scale` says, its metadata kept (the sizes in them are set when the
/// file is written, `Original::encode`).
pub fn resample(original: &Original, scale: Scale) -> Original {
    original.with_image(original.image.resize_exact(scale.w, scale.h, scale.filter.kind()))
}

/// The photo the document `scale` is drawn on: `original` itself when it is already at that size
/// (the editor passes the photo it shows), otherwise resampled from it.
pub fn at_size(original: &Original, scale: Option<Scale>) -> Cow<'_, Original> {
    match scale {
        Some(s) if (s.w, s.h) != (original.image.width(), original.image.height()) => Cow::Owned(resample(original, s)),
        _ => Cow::Borrowed(original),
    }
}

/// The window's choices. Every size in pixels of the result (the crop), width first.
#[derive(Clone, PartialEq, Debug)]
pub struct Form {
    /// The result now.
    pub from: (u32, u32),
    /// The size asked.
    pub size: (u32, u32),
    /// Width and height change together.
    pub locked: bool,
    /// The proportions can be unlocked: not on an image turned by a free angle, which would be
    /// slanted.
    pub unlockable: bool,
    /// Typed in percent of the size now instead of pixels.
    pub percent: bool,
    /// The filter chosen; `None`: the one `filter` gives for the size.
    pub chosen: Option<Filter>,
    /// Pixels of the photo per pixel of the result, along the width and the height of the result.
    per_pixel: (f32, f32),
}

impl Form {
    /// On a result of `from` pixels; the photo, after the flips and turns, `photo` pixels; `angle`
    /// the free angle.
    pub fn new(from: (u32, u32), crop: &Crop, photo: (f32, f32), angle: f32) -> Form {
        Form { from, size: from, locked: true, unlockable: angle == 0.0, percent: false, chosen: None, per_pixel: (photo.0 / crop.w, photo.1 / crop.h) }
    }

    pub fn set_width(&mut self, w: u32) {
        self.size.0 = w.clamp(1, MAX_SIDE * 4);
        if self.locked {
            self.size.1 = batch::target(self.from.0, self.from.1, Size::Fit(self.size.0, u32::MAX), true).1;
        }
    }

    pub fn set_height(&mut self, h: u32) {
        self.size.1 = h.clamp(1, MAX_SIDE * 4);
        if self.locked {
            self.size.0 = batch::target(self.from.0, self.from.1, Size::Fit(u32::MAX, self.size.1), true).0;
        }
    }

    /// The size in percent of the size now, rounded.
    pub fn percents(&self) -> (u32, u32) {
        let p = |a: u32, b: u32| (a as f64 * 100.0 / b as f64).round() as u32;
        (p(self.size.0, self.from.0), p(self.size.1, self.from.1))
    }

    /// `p` percent of the width (`width`) or of the height; both with the proportions locked.
    pub fn set_percent(&mut self, width: bool, p: u32) {
        let (w, h) = batch::target(self.from.0, self.from.1, Size::Percent(p.clamp(1, MAX_PERCENT)), true);
        match () {
            _ if self.locked => self.size = (w, h),
            _ if width => self.size.0 = w,
            _ => self.size.1 = h,
        }
    }

    /// The proportions locked again: the height follows the width.
    pub fn lock(&mut self, locked: bool) {
        self.locked = locked || !self.unlockable;
        if self.locked {
            self.set_width(self.size.0);
        }
    }

    /// The filter used: the one chosen, or Lanczos to make smaller (the sharpest) and Catmull-Rom
    /// (bicubic) to make larger, which rings less around edges.
    pub fn filter(&self) -> Filter {
        let larger = self.size.0 as u64 * self.size.1 as u64 > self.from.0 as u64 * self.from.1 as u64;
        self.chosen.unwrap_or(if larger { Filter::CatmullRom } else { Filter::Lanczos3 })
    }

    /// Why Resize is off, a Fluent id: the size it has, or too large (the result or the photo).
    pub fn why_not(&self) -> Option<&'static str> {
        let photo = (self.size.0 as f32 * self.per_pixel.0, self.size.1 as f32 * self.per_pixel.1);
        match () {
            _ if self.size == self.from => Some("resize-same"),
            _ if self.size.0.max(self.size.1) > MAX_SIDE || photo.0.max(photo.1).round() > MAX_SIDE as f32 => Some("resize-too-big"),
            _ => None,
        }
    }
}

/// `doc` with its result at `out` pixels: the photo, now `photo` pixels upright, resampled with
/// `filter` (back to the original, `original` pixels, it is no longer resampled); the crop and
/// every object scaled with it, the widths of the lines and the letters too.
pub fn resized(doc: &Doc, photo: (u32, u32), original: (u32, u32), out: (u32, u32), filter: Filter) -> Doc {
    let sideways = doc.turn.sideways();
    let turned = |(a, b): (u32, u32)| if sideways { (b, a) } else { (a, b) };
    let (pw, ph) = turned(photo);
    let side = |v: u32, k: f32| ((v as f32 * k).round() as u32).max(1);
    let new = (side(pw, out.0 as f32 / doc.crop.w), side(ph, out.1 as f32 / doc.crop.h));
    // The scale the photo really gets, its sides being whole pixels.
    let (kx, ky) = (new.0 as f32 / pw as f32, new.1 as f32 / ph as f32);
    let c = doc.crop;
    let mut d = doc.clone();
    d.crop = Crop { cx: c.cx * kx, cy: c.cy * ky, w: out.0 as f32, h: out.1 as f32 };
    let long = new.0.max(new.1) as f32;
    for o in &mut d.objects {
        scale_obj(o, kx, ky, long);
    }
    let upright = turned(new);
    d.scale = (upright != original).then_some(Scale { w: upright.0, h: upright.1, filter });
    d
}

/// The object over the same part of the photo scaled by `kx`, `ky`: its points and box, its
/// widths and letters by the mean of the two, in whole pixels within the ranges of the properties
/// bar (which would round them, a step more in the history that Undo had to take first); the
/// previews made again. `long`: the photo's long side, now.
fn scale_obj(o: &mut Obj, kx: f32, ky: f32, long: f32) {
    let k = (kx * ky).sqrt();
    // A lens may have no border (0 stays 0).
    let whole = |v: f32, least: f32| if v > 0.0 { (v * k).round().max(least) } else { v };
    for p in o.shape.points_mut() {
        *p = Pt::new(p.x * kx, p.y * ky);
    }
    if let Some(a) = o.shape.area_mut() {
        *a = scale_area(*a, kx, ky);
    }
    match &mut o.shape {
        Shape::Text { size, rot, .. } => {
            *size = whole(*size, 4.0);
            *rot = angle(*rot, kx, ky);
        }
        Shape::Filter { filter, amount, preview, .. } => {
            if let (true, Some((lo, hi, _))) = (filter.in_pixels(), filter.amount(long)) {
                *amount = (*amount * k).round().clamp(lo, hi);
            }
            *preview = Preview::default();
        }
        Shape::Lens { preview, .. } | Shape::Highlight { preview, .. } => *preview = Preview::default(),
        _ => {}
    }
    // Texts, steps and stamps: their size, at least 4 pixels as in the bar.
    let sized = matches!(o.shape, Shape::Text { .. } | Shape::Step { .. } | Shape::Stamp(_));
    o.style.width = whole(o.style.width, if sized { 4.0 } else { 1.0 });
    o.style.letters = whole(o.style.letters, 4.0);
}

/// The box over the same part: its sides along its own angle scaled, as the image under it.
fn scale_area(a: Area, kx: f32, ky: f32) -> Area {
    let (s, c) = a.rot.to_radians().sin_cos();
    let along = |x: f32, y: f32| (x * kx).hypot(y * ky);
    Area { centre: Pt::new(a.centre.x * kx, a.centre.y * ky), w: a.w * along(c, s), h: a.h * along(-s, c), rot: angle(a.rot, kx, ky) }
}

/// An angle (degrees, clockwise) once the image is stretched by `kx`, `ky`: the same with the
/// proportions kept.
fn angle(rot: f32, kx: f32, ky: f32) -> f32 {
    if kx == ky {
        return rot;
    }
    let (s, c) = rot.to_radians().sin_cos();
    (s * ky).atan2(c * kx).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::doc::Style;
    use crate::edit::history::History;
    use crate::transform::Op;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Obj {
        Obj { shape: Shape::Rect(Area { centre: Pt::new(x, y), w, h, rot: 0.0 }), style: Style { width: 4.0, ..Style::default() } }
    }

    /// Width and height follow each other with the proportions locked, in pixels and percent, as
    /// Convert and resize sizes them; unlocked, one alone; locked again, the height follows.
    #[test]
    fn form_sizes() {
        let crop = Crop::centred(400.0, 300.0);
        let mut f = Form::new((400, 300), &crop, (400.0, 300.0), 0.0);
        assert_eq!(f.why_not(), Some("resize-same"));
        f.set_width(200);
        assert_eq!(f.size, (200, 150));
        f.set_height(600);
        assert_eq!((f.size, f.percents()), ((800, 600), (200, 200)));
        f.set_percent(true, 33);
        assert_eq!(f.size, (132, 99));
        assert_eq!(f.why_not(), None);
        f.lock(false);
        f.set_width(100);
        f.set_percent(false, 50);
        assert_eq!(f.size, (100, 150));
        f.lock(true);
        assert_eq!(f.size, (100, 75));
        // Never 0, never past the largest side (the crop is a quarter of the photo here).
        f.set_width(0);
        assert_eq!(f.size, (1, 1));
        let mut f = Form::new((400, 300), &Crop::centred(100.0, 75.0), (400.0, 300.0), 0.0);
        f.set_width(6000);
        assert_eq!(f.why_not(), Some("resize-too-big"));
        // Turned by a free angle the proportions stay locked.
        let mut f = Form::new((400, 300), &crop, (400.0, 300.0), 2.5);
        f.lock(false);
        assert!(f.locked && !f.unlockable);
    }

    /// Lanczos to make smaller, Catmull-Rom to make larger, unless one is chosen.
    #[test]
    fn filter_for_the_size() {
        let mut f = Form::new((400, 300), &Crop::centred(400.0, 300.0), (400.0, 300.0), 0.0);
        f.set_width(200);
        assert_eq!(f.filter(), Filter::Lanczos3);
        f.set_width(800);
        assert_eq!(f.filter(), Filter::CatmullRom);
        f.chosen = Some(Filter::Nearest);
        assert_eq!(f.filter(), Filter::Nearest);
    }

    /// Halved: the photo, the crop and what is drawn, the lines' widths too; back to the size of
    /// the original it is no longer resampled. Undo takes it back in one step.
    #[test]
    fn halved_with_the_objects() {
        let mut doc = Doc::new(400, 300, None);
        doc.crop = Crop { cx: 20.0, cy: -10.0, w: 200.0, h: 100.0 };
        doc.objects.push(rect(40.0, 20.0, 60.0, 30.0));
        doc.objects.push(Obj { shape: Shape::Line(Pt::new(-100.0, 50.0), Pt::new(100.0, -50.0), None), style: Style::default() });
        let mut h = History::new(doc.clone());
        let half = resized(&doc, (400, 300), (400, 300), (100, 50), Filter::Lanczos3);
        assert_eq!(half.scale, Some(Scale { w: 200, h: 150, filter: Filter::Lanczos3 }));
        assert_eq!(half.crop, Crop { cx: 10.0, cy: -5.0, w: 100.0, h: 50.0 });
        assert_eq!(half.objects[0].shape, Shape::Rect(Area { centre: Pt::new(20.0, 10.0), w: 30.0, h: 15.0, rot: 0.0 }));
        assert_eq!(half.objects[0].style.width, 2.0);
        assert_eq!(half.objects[1].shape, Shape::Line(Pt::new(-50.0, 25.0), Pt::new(50.0, -25.0), None));
        // Whole pixels, as the properties bar shows them: 12 by 0.5003 is 6, not 6.0037.
        let mut odd = Doc::new(1749, 3000, None);
        odd.objects.push(Obj { style: Style { width: 12.0, ..Style::default() }, ..rect(0.0, 0.0, 10.0, 10.0) });
        let odd = resized(&odd, (1749, 3000), (1749, 3000), (875, 1501), Filter::Lanczos3);
        assert_eq!(odd.objects[0].style.width, 6.0);
        h.push(half.clone());
        h.undo();
        assert_eq!(*h.doc(), doc);
        h.redo();
        assert_eq!(*h.doc(), half);
        let back = resized(&half, (200, 150), (400, 300), (200, 100), Filter::CatmullRom);
        assert_eq!((back.scale, back.crop, back.objects.clone()), (None, doc.crop, doc.objects.clone()));
    }

    /// Turned a quarter, stretched along the result's width: the photo, upright, along its height.
    /// A box at an angle keeps covering the same part.
    #[test]
    fn stretched_after_a_turn() {
        let mut doc = Doc::new(300, 200, None);
        doc.turned(Op::Cw);
        doc.objects.push(Obj { shape: Shape::Cover(Area { centre: Pt::new(0.0, 0.0), w: 40.0, h: 10.0, rot: 90.0 }), style: Style::default() });
        let wide = resized(&doc, (300, 200), (300, 200), (400, 300), Filter::Bilinear);
        assert_eq!(wide.scale, Some(Scale { w: 300, h: 400, filter: Filter::Bilinear }));
        assert_eq!((wide.crop.w, wide.crop.h), (400.0, 300.0));
        let Shape::Cover(a) = wide.objects[0].shape else { panic!() };
        // Upright in the result: its 40 pixels along the height, which did not change.
        assert!((a.w - 40.0).abs() < 1e-3 && (a.h - 20.0).abs() < 1e-3 && (a.rot - 90.0).abs() < 1e-3, "{a:?}");
    }

    /// The photo resampled from the original: its new size, its metadata still there; already at
    /// the size asked, it is not resampled again.
    #[test]
    fn photo_at_size() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(40, 20, image::Rgb([9, 99, 199])));
        let jpeg = Original::with_meta(img, &[]).encode(&crate::encode::Options::default()).unwrap();
        let original = Original::with_meta(crate::decode::decode_full(&jpeg, "a.jpg").unwrap(), &jpeg);
        let s = Scale { w: 10, h: 5, filter: Filter::Nearest };
        let small = at_size(&original, Some(s));
        assert!(matches!(small, Cow::Owned(_)));
        assert_eq!((small.image.width(), small.image.height()), (10, 5));
        assert!(matches!(at_size(&small, Some(s)), Cow::Borrowed(_)));
        assert!(matches!(at_size(&original, None), Cow::Borrowed(_)));
    }
}
