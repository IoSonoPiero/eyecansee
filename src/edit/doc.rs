//! The document of the editor: what "Rotate and crop" does (`rotate::Edit`: flips and quarter
//! turns, free angle, crop, fill) plus the objects drawn on the image. The objects are in pixels
//! of the image after the flips and quarter turns and before the free angle, measured from the
//! centre of the image (x to the right, y down, as `rotate::Crop`): a change of crop leaves them
//! over the same part of the image, and a flip or quarter turn only changes signs, so it is exact.

use crate::rotate::{Crop, Edit};
use crate::transform::{Op, Turn};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub fn new(x: f32, y: f32) -> Pt {
        Pt { x, y }
    }

    /// Where the point goes when the image is flipped or turned by `op` around its centre.
    fn turned(self, op: Op) -> Pt {
        let Pt { x, y } = self;
        match op {
            Op::Cw => Pt::new(-y, x),
            Op::Ccw => Pt::new(y, -x),
            Op::Half => Pt::new(-x, -y),
            Op::FlipH => Pt::new(-x, y),
            Op::FlipV => Pt::new(x, -y),
        }
    }
}

/// A rectangle by its centre and size, turned by `rot` degrees clockwise around its centre.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Area {
    pub centre: Pt,
    pub w: f32,
    pub h: f32,
    pub rot: f32,
}

impl Area {
    fn turned(self, op: Op) -> Area {
        // A quarter turn of the image turns the rectangle with it: its own angle, measured on the
        // image, stays; a flip mirrors it.
        Area { centre: self.centre.turned(op), rot: if is_flip(op) { -self.rot } else { self.rot }, ..self }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Shape {
    Line(Pt, Pt),
    /// From the first point to the second, where the head is.
    Arrow(Pt, Pt),
    Rect(Area),
    Highlight(Area),
    /// `at` is the centre of the text, `size` the height of its letters in pixels. A flip moves
    /// and tilts the text but never mirrors the letters.
    Text { at: Pt, text: String, size: f32, rot: f32 },
    /// Solid box over private data: nothing under it is left in the saved file.
    Cover(Area),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Style {
    /// RGBA.
    pub color: [u8; 4],
    /// Of the lines, in pixels of the image.
    pub width: f32,
    /// 0 to 1, over the alpha of `color`.
    pub opacity: f32,
    pub dash: Dash,
}

impl Default for Style {
    fn default() -> Style {
        Style { color: [255, 0, 0, 255], width: 4.0, opacity: 1.0, dash: Dash::Solid }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Obj {
    pub shape: Shape,
    pub style: Style,
}

impl Obj {
    fn turned(&self, op: Op) -> Obj {
        let shape = match &self.shape {
            Shape::Line(a, b) => Shape::Line(a.turned(op), b.turned(op)),
            Shape::Arrow(a, b) => Shape::Arrow(a.turned(op), b.turned(op)),
            Shape::Rect(r) => Shape::Rect(r.turned(op)),
            Shape::Highlight(r) => Shape::Highlight(r.turned(op)),
            Shape::Cover(r) => Shape::Cover(r.turned(op)),
            Shape::Text { at, text, size, rot } => Shape::Text { at: at.turned(op), text: text.clone(), size: *size, rot: if is_flip(op) { -rot } else { *rot } },
        };
        Obj { shape, style: self.style }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Doc {
    pub turn: Turn,
    /// Clockwise, in degrees, as `rotate::Edit`.
    pub angle: f32,
    pub crop: Crop,
    pub fill: Option<[u8; 3]>,
    pub objects: Vec<Obj>,
}

impl Doc {
    /// Nothing changed yet on an upright image of `w` × `h` pixels: the crop is all of it.
    pub fn new(w: u32, h: u32, fill: Option<[u8; 3]>) -> Doc {
        Doc { turn: Turn::default(), angle: 0.0, crop: Crop::centred(w as f32, h as f32), fill, objects: Vec::new() }
    }

    /// The flips, quarter turns, angle and crop, as "Rotate and crop" applies them to the pixels.
    pub fn edit(&self) -> Edit {
        Edit { turn: self.turn, angle: self.angle, crop: self.crop, fill: self.fill }
    }

    /// The image flipped or turned by `op` as it stands now: the turn, the crop and the objects go
    /// with it. Turning commutes with the free angle; a flip turns it the other way.
    pub fn turned(&mut self, op: Op) {
        self.turn = self.turn.then(op);
        let c = Pt::new(self.crop.cx, self.crop.cy).turned(op);
        let sideways = matches!(op, Op::Cw | Op::Ccw);
        let (w, h) = if sideways { (self.crop.h, self.crop.w) } else { (self.crop.w, self.crop.h) };
        self.crop = Crop { cx: c.x, cy: c.y, w, h };
        if is_flip(op) {
            self.angle = -self.angle;
        }
        self.objects = self.objects.iter().map(|o| o.turned(op)).collect();
    }

    /// The point of the upright image, from its centre, that `p` (from the centre of the image
    /// after the flips and quarter turns) is over: what the turn did, undone.
    pub fn source(&self, p: Pt) -> Pt {
        let (flip, quarter) = self.turn.parts();
        let mut p = p;
        for _ in 0..quarter {
            p = p.turned(Op::Ccw);
        }
        if flip { p.turned(Op::FlipH) } else { p }
    }
}

fn is_flip(op: Op) -> bool {
    matches!(op, Op::FlipH | Op::FlipV)
}

#[cfg(test)]
pub(crate) mod tests {
    use image::{DynamicImage, GenericImageView, RgbImage};

    use super::*;

    const OPS: [Op; 5] = [Op::Cw, Op::Ccw, Op::Half, Op::FlipH, Op::FlipV];

    pub(crate) fn sample() -> Doc {
        let mut d = Doc::new(40, 30, None);
        d.angle = 3.5;
        d.crop = Crop { cx: 2.0, cy: -1.5, w: 20.0, h: 10.0 };
        let area = Area { centre: Pt::new(-7.25, 4.5), w: 6.0, h: 3.0, rot: -3.5 };
        let style = Style::default();
        d.objects = vec![
            Obj { shape: Shape::Line(Pt::new(-10.0, -5.0), Pt::new(3.0, 8.0)), style },
            Obj { shape: Shape::Arrow(Pt::new(0.1, 0.2), Pt::new(-12.3, 7.7)), style: Style { dash: Dash::Dashed, ..style } },
            Obj { shape: Shape::Rect(area), style },
            Obj { shape: Shape::Highlight(area), style: Style { opacity: 0.4, ..style } },
            Obj { shape: Shape::Text { at: Pt::new(5.0, -6.0), text: "à €".into(), size: 12.0, rot: -3.5 }, style },
            Obj { shape: Shape::Cover(Area { rot: 0.0, ..area }), style },
        ];
        d
    }

    /// Every point of every object, in order.
    fn points(d: &Doc) -> Vec<Pt> {
        d.objects
            .iter()
            .flat_map(|o| match &o.shape {
                Shape::Line(a, b) | Shape::Arrow(a, b) => vec![*a, *b],
                Shape::Rect(r) | Shape::Highlight(r) | Shape::Cover(r) => vec![r.centre],
                Shape::Text { at, .. } => vec![*at],
            })
            .collect()
    }

    /// Four quarter turns, two halves, two flips of either kind, a turn and its opposite: the
    /// document as it was, exactly.
    #[test]
    fn turns_come_back() {
        let start = sample();
        let back = |ops: &[Op]| {
            let mut d = start.clone();
            ops.iter().for_each(|&op| d.turned(op));
            assert_eq!(d, start, "{ops:?}");
        };
        back(&[Op::Cw; 4]);
        back(&[Op::Ccw; 4]);
        back(&[Op::Half; 2]);
        back(&[Op::FlipH; 2]);
        back(&[Op::FlipV; 2]);
        back(&[Op::Cw, Op::Ccw]);
        back(&[Op::FlipH, Op::FlipV, Op::Half]);
    }

    /// After any flip or turn every object is over the same point of the upright image.
    #[test]
    fn objects_stay_on_the_image() {
        let start = sample();
        let before: Vec<Pt> = points(&start).into_iter().map(|p| start.source(p)).collect();
        for op in OPS {
            for again in OPS {
                let mut d = start.clone();
                d.turned(op);
                d.turned(again);
                let after: Vec<Pt> = points(&d).into_iter().map(|p| d.source(p)).collect();
                assert_eq!(after, before, "{op:?} {again:?}");
            }
        }
    }

    /// `source` agrees with the pixels: the pixel of the turned image at a point is the pixel of
    /// the original at its source, for each of the 8 orientations.
    #[test]
    fn source_matches_the_pixels() {
        let (w, h) = (7u32, 4u32);
        let original = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8, y as u8, 0])));
        let mut d = Doc::new(w, h, None);
        for op in [Op::Cw, Op::Cw, Op::Cw, Op::FlipH, Op::Cw, Op::Cw, Op::Cw] {
            d.turned(op);
            let mut img = original.clone();
            d.turn.apply(&mut img);
            let (tw, th) = img.dimensions();
            for (x, y) in (0..tw).flat_map(|x| (0..th).map(move |y| (x, y))) {
                // Centre of the pixel, from the centre of the image.
                let p = Pt::new(x as f32 + 0.5 - tw as f32 / 2.0, y as f32 + 0.5 - th as f32 / 2.0);
                let s = d.source(p);
                let (sx, sy) = ((s.x + w as f32 / 2.0 - 0.5).round() as u32, (s.y + h as f32 / 2.0 - 0.5).round() as u32);
                assert_eq!(img.get_pixel(x, y), original.get_pixel(sx, sy), "{:?} at {x},{y}", d.turn);
            }
        }
    }

    /// The crop goes with the image: turned by a quarter its sides swap, after a flip and the
    /// angle the other way the same part of the picture is in it.
    #[test]
    fn crop_and_angle_follow() {
        let mut d = sample();
        d.turned(Op::Cw);
        assert_eq!(d.crop, Crop { cx: 1.5, cy: 2.0, w: 10.0, h: 20.0 });
        assert_eq!(d.angle, 3.5);
        d.turned(Op::FlipH);
        assert_eq!(d.crop, Crop { cx: -1.5, cy: 2.0, w: 10.0, h: 20.0 });
        assert_eq!(d.angle, -3.5);
        assert_eq!(d.edit(), Edit { turn: d.turn, angle: -3.5, crop: d.crop, fill: None });
    }

    /// The pixels agree: the edit of the document turned by `op` gives the edited image turned by
    /// `op`. Exact with no angle (crop edges on whole pixels); with an angle, the same but for the
    /// resampling.
    #[test]
    fn turned_edit_is_turned_image() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(40, 30, |x, y| image::Rgb([(x * 6) as u8, (y * 8) as u8, ((x * y) % 256) as u8])));
        for angle in [0.0, 30.0] {
            for op in OPS {
                let mut d = Doc::new(40, 30, Some([0, 0, 0]));
                d.turned(Op::Cw);
                d.angle = angle;
                d.crop = Crop { cx: 1.0, cy: -2.0, w: 16.0, h: 10.0 };
                let mut expected = img.clone();
                d.edit().apply(&mut expected);
                Turn::default().then(op).apply(&mut expected);
                d.turned(op);
                let mut got = img.clone();
                d.edit().apply(&mut got);
                assert_eq!(got.dimensions(), expected.dimensions(), "{op:?} {angle}");
                let diff = got.to_rgb8().as_raw().iter().zip(expected.to_rgb8().as_raw()).map(|(a, b)| a.abs_diff(*b) as u64).max().unwrap();
                assert!(diff <= if angle == 0.0 { 0 } else { 2 }, "{op:?} {angle}: {diff}");
            }
        }
    }
}
