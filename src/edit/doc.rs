//! The document of the editor: what "Rotate and crop" does (`rotate::Edit`: flips and quarter
//! turns, free angle, crop, fill) plus the objects drawn on the image. The objects are in pixels
//! of the image after the flips and quarter turns and before the free angle, measured from the
//! centre of the image (x to the right, y down, as `rotate::Crop`): a change of crop leaves them
//! over the same part of the image, and a flip or quarter turn only changes signs, so it is exact.

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

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
    /// The box over the same part of the image once it is flipped or turned: a quarter turn swaps
    /// its sides (it kept them, and a box moved off what it covered: a cover box left private
    /// data in view, found on 2026-11-05), a flip mirrors its angle.
    fn turned(self, op: Op) -> Area {
        let (w, h) = if matches!(op, Op::Cw | Op::Ccw) { (self.h, self.w) } else { (self.w, self.h) };
        Area { centre: self.centre.turned(op), w, h, rot: if is_flip(op) { -self.rot } else { self.rot } }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Shape {
    /// From the first point to the second; the third, if any, bends it (the control point of a
    /// quadratic Bézier curve).
    Line(Pt, Pt, Option<Pt>),
    /// From the first point to the second, where the head is; bent as a line.
    Arrow(Pt, Pt, Option<Pt>),
    Rect(Area),
    /// A stroke drawn by hand: the points the pointer went through, drawn smoothed.
    Pen(Vec<Pt>),
    /// A step: a disc of the colour of the style with the number `n` in it, in black or white,
    /// whichever reads better on it; its box is square.
    Step { area: Area, n: u32 },
    /// A speech bubble: rounded or oval (`Style::round`) in its box, a tail to `tail`, its text
    /// in the middle in `font` (as `Text`), the letters as the style says (`ink`, `letters`).
    Bubble { area: Area, tail: Pt, text: String, font: Option<String> },
    /// A lens: the photo under the box magnified `Style::zoom` times, round (`Style::round`) or
    /// as the box, with a border; `preview` as a filter's, made from the photo.
    Lens { area: Area, preview: Preview },
    /// The ellipse inside the box, with a border and a fill as the rectangle.
    Ellipse(Area),
    Highlight(Area),
    /// `at` is the centre of the text, `size` the height of its letters in pixels, `font` the
    /// family of an installed font (`None`: the sans-serif one of the system). A flip moves and
    /// tilts the text but never mirrors the letters.
    /// `path`, if any: the line the letters follow instead (two ends and a control point that
    /// bends it, as `Line`), the text in the middle of it on one line.
    Text { at: Pt, text: String, size: f32, rot: f32, font: Option<String>, path: Option<(Pt, Pt, Option<Pt>)> },
    /// Solid box over private data: nothing under it is left in the saved file.
    Cover(Area),
    /// A piece of image (copied or cut with Selection, or pasted from another program) stretched
    /// over `area`.
    Patch { area: Area, pixels: Pixels },
    /// A filter on the part of the photo under `area` (all of it for the whole image), under the
    /// objects drawn; `amount` as `Filter::amount` says.
    Filter { area: Area, filter: crate::edit::filter::Filter, amount: f32, preview: Preview },
}

/// The preview of a filter: its pixels, at a smaller size, and the box and amount they were made
/// for. Not part of what the document is: two documents that differ only here are the same, for
/// the history and for saving (which filters the photo at full size).
#[derive(Clone, Debug, Default)]
pub struct Preview(pub Option<(Area, f32, Pixels)>);

impl PartialEq for Preview {
    fn eq(&self, _: &Preview) -> bool {
        true
    }
}

/// The pixels of a patch, RGBA 8 bits. Shared by the states of the history and compared by
/// identity, never pixel by pixel; the PNG the drawing needs is made once.
#[derive(Clone)]
pub struct Pixels(Arc<(image::RgbaImage, OnceLock<String>)>);

impl Pixels {
    pub fn new(image: image::RgbaImage) -> Pixels {
        Pixels(Arc::new((image, OnceLock::new())))
    }

    pub fn image(&self) -> &image::RgbaImage {
        &self.0.0
    }

    /// The pixels as a PNG in base64, for an SVG `<image>`; made the first time.
    pub fn png_base64(&self) -> &str {
        self.0.1.get_or_init(|| {
            use base64::Engine;
            let mut png = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new_with_quality(&mut png, image::codecs::png::CompressionType::Fast, image::codecs::png::FilterType::Sub);
            let img = self.image();
            let _ = image::ImageEncoder::write_image(encoder, img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8);
            base64::engine::general_purpose::STANDARD.encode(png)
        })
    }

    /// The same pixels turned or flipped as `op` does to the image.
    fn turned(&self, op: Op) -> Pixels {
        use image::imageops;
        let img = self.image();
        Pixels::new(match op {
            Op::Cw => imageops::rotate90(img),
            Op::Ccw => imageops::rotate270(img),
            Op::Half => imageops::rotate180(img),
            Op::FlipH => imageops::flip_horizontal(img),
            Op::FlipV => imageops::flip_vertical(img),
        })
    }
}

impl PartialEq for Pixels {
    fn eq(&self, other: &Pixels) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Pixels({}×{})", self.image().width(), self.image().height())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// The end of an arrow: a filled head, an open one, a head at both ends, a dot.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Head {
    #[default]
    Filled,
    Open,
    Double,
    Dot,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Style {
    /// RGB: the only transparency is `opacity`.
    pub color: [u8; 3],
    /// Of the lines, in pixels of the image.
    pub width: f32,
    /// 0 (invisible) to 1 (solid).
    pub opacity: f32,
    pub dash: Dash,
    /// Arrows only. Styles remembered before it had none: the filled head.
    #[serde(default)]
    pub head: Head,
    /// Rectangles, ellipses and bubbles only: the inside, at the same opacity as the border;
    /// `None`: empty.
    #[serde(default)]
    pub fill: Option<[u8; 3]>,
    /// Bubbles: the colour of the letters.
    #[serde(default)]
    pub ink: [u8; 3],
    /// Bubbles: the height of the letters, in pixels of the image (0 in styles kept before it).
    #[serde(default)]
    pub letters: f32,
    /// Bubbles oval instead of rounded, lenses round instead of square.
    #[serde(default)]
    pub round: bool,
    /// Lenses: how many times they magnify.
    #[serde(default = "Style::two")]
    pub zoom: u8,
}

impl Default for Style {
    fn default() -> Style {
        Style { color: [255, 0, 0], width: 4.0, opacity: 1.0, dash: Dash::Solid, head: Head::Filled, fill: None, ink: [0, 0, 0], letters: 0.0, round: false, zoom: 2 }
    }
}

/// Opacity the highlighter starts with: a fill over the photo, which shows through it.
pub const HIGHLIGHT_OPACITY: f32 = 0.4;

impl Style {
    fn two() -> u8 {
        2
    }

    /// Where the highlighter starts: yellow, at `HIGHLIGHT_OPACITY`.
    pub fn highlighter() -> Style {
        Style { color: [255, 230, 0], opacity: HIGHLIGHT_OPACITY, ..Style::default() }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Obj {
    pub shape: Shape,
    pub style: Style,
}

impl Shape {
    /// The box of the objects drawn in one: rectangles, cover boxes, patches, filters…
    pub fn area(&self) -> Option<&Area> {
        match self {
            Shape::Rect(a) | Shape::Ellipse(a) | Shape::Highlight(a) | Shape::Cover(a) | Shape::Patch { area: a, .. } | Shape::Filter { area: a, .. } | Shape::Step { area: a, .. } | Shape::Bubble { area: a, .. } | Shape::Lens { area: a, .. } => Some(a),
            Shape::Line(..) | Shape::Arrow(..) | Shape::Pen(_) | Shape::Text { .. } => None,
        }
    }

    pub fn area_mut(&mut self) -> Option<&mut Area> {
        match self {
            Shape::Rect(a) | Shape::Ellipse(a) | Shape::Highlight(a) | Shape::Cover(a) | Shape::Patch { area: a, .. } | Shape::Filter { area: a, .. } | Shape::Step { area: a, .. } | Shape::Bubble { area: a, .. } | Shape::Lens { area: a, .. } => Some(a),
            Shape::Line(..) | Shape::Arrow(..) | Shape::Pen(_) | Shape::Text { .. } => None,
        }
    }

    /// The points of the object that are not in its box: the ends and bend of a line, where a
    /// text is and the line it follows.
    pub fn points_mut(&mut self) -> Vec<&mut Pt> {
        fn line<'a>(a: &'a mut Pt, b: &'a mut Pt, c: &'a mut Option<Pt>) -> Vec<&'a mut Pt> {
            let mut v = vec![a, b];
            v.extend(c.as_mut());
            v
        }
        match self {
            Shape::Line(a, b, c) | Shape::Arrow(a, b, c) => line(a, b, c),
            Shape::Pen(points) => points.iter_mut().collect(),
            Shape::Bubble { tail, .. } => vec![tail],
            Shape::Text { at, path, .. } => {
                let mut v = vec![at];
                if let Some((a, b, c)) = path {
                    v.extend(line(a, b, c));
                }
                v
            }
            _ => Vec::new(),
        }
    }
}

impl Shape {
    /// The letters of a text or a bubble.
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Shape::Text { text, .. } | Shape::Bubble { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The font of a text or a bubble.
    pub fn font_mut(&mut self) -> Option<&mut Option<String>> {
        match self {
            Shape::Text { font, .. } | Shape::Bubble { font, .. } => Some(font),
            _ => None,
        }
    }
}

impl Obj {
    fn turned(&self, op: Op) -> Obj {
        let mut shape = self.shape.clone();
        for p in shape.points_mut() {
            *p = p.turned(op);
        }
        if let Some(a) = shape.area_mut() {
            *a = a.turned(op);
        }
        match &mut shape {
            // The picture in the box turns (or flips) as the image did.
            Shape::Patch { pixels, .. } => *pixels = pixels.turned(op),
            // Made again for the photo under it.
            Shape::Filter { preview, .. } | Shape::Lens { preview, .. } => *preview = Preview::default(),
            Shape::Text { rot, .. } => *rot = text_turned(*rot, op),
            _ => {}
        }
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

    /// The pixels of the photo `img` (upright, as read) under the box `a`, one per pixel of the
    /// box, along its own sides; transparent where the box goes past the photo. The objects
    /// drawn are not in them. At most 8000 pixels a side.
    pub fn pixels_under(&self, img: &image::DynamicImage, a: &Area) -> image::RgbaImage {
        self.pixels_under_at(img, a, 1.0)
    }

    /// `pixels_under` at `scale` pixels per pixel of the box (a filter's preview).
    pub fn pixels_under_at(&self, img: &image::DynamicImage, a: &Area, scale: f32) -> image::RgbaImage {
        use image::GenericImageView;
        let (w, h) = ((a.w * scale).round().clamp(1.0, 8000.0) as u32, (a.h * scale).round().clamp(1.0, 8000.0) as u32);
        let (s, c) = a.rot.to_radians().sin_cos();
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        image::RgbaImage::from_fn(w, h, |u, v| {
            let (x, y) = ((u as f32 + 0.5) * a.w / w as f32 - a.w / 2.0, (v as f32 + 0.5) * a.h / h as f32 - a.h / 2.0);
            let q = self.source(Pt::new(a.centre.x + x * c - y * s, a.centre.y + x * s + y * c));
            let (px, py) = (q.x + iw / 2.0, q.y + ih / 2.0);
            if px >= 0.0 && py >= 0.0 && px < iw && py < ih { img.get_pixel(px as u32, py as u32) } else { image::Rgba([0; 4]) }
        })
    }

    /// The number of the next step: one more than the highest there is.
    pub fn next_step(&self) -> u32 {
        self.objects.iter().filter_map(|o| if let Shape::Step { n, .. } = o.shape { Some(n) } else { None }).max().unwrap_or(0) + 1
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

/// The angle of a text once the image is turned or flipped: it turns with the image (it stayed
/// upright, user 2026-11-05); a flip tilts it without mirroring the letters, a vertical one as a
/// horizontal one and half a turn, so any two operations that give the same image give the same
/// text.
fn text_turned(rot: f32, op: Op) -> f32 {
    let r = match op {
        Op::Cw => rot + 90.0,
        Op::Ccw => rot - 90.0,
        Op::Half => rot + 180.0,
        Op::FlipH => -rot,
        Op::FlipV => 180.0 - rot,
    };
    (r + 180.0).rem_euclid(360.0) - 180.0
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
            Obj { shape: Shape::Line(Pt::new(-10.0, -5.0), Pt::new(3.0, 8.0), Some(Pt::new(4.5, -2.25))), style },
            Obj { shape: Shape::Arrow(Pt::new(0.1, 0.2), Pt::new(-12.3, 7.7), None), style: Style { dash: Dash::Dashed, ..style } },
            Obj { shape: Shape::Rect(area), style },
            Obj { shape: Shape::Ellipse(Area { rot: 12.0, ..area }), style },
            Obj { shape: Shape::Step { area: Area { w: 5.0, h: 5.0, ..area }, n: 3 }, style },
            Obj { shape: Shape::Bubble { area: Area { rot: 30.0, ..area }, tail: Pt::new(-2.0, 9.5), text: "Ciao".into(), font: None }, style },
            Obj { shape: Shape::Lens { area, preview: Preview::default() }, style },
            Obj { shape: Shape::Pen(vec![Pt::new(1.0, 2.0), Pt::new(3.5, -4.0), Pt::new(8.0, 0.5)]), style },
            Obj { shape: Shape::Highlight(area), style: Style { opacity: 0.4, ..style } },
            Obj { shape: Shape::Text { at: Pt::new(5.0, -6.0), text: "à €".into(), size: 12.0, rot: -3.5, font: None, path: None }, style },
            Obj { shape: Shape::Cover(Area { rot: 0.0, ..area }), style },
        ];
        d
    }

    /// Every point of every object, in order.
    fn points(d: &Doc) -> Vec<Pt> {
        d.objects
            .iter()
            .flat_map(|o| match &o.shape {
                Shape::Line(a, b, c) | Shape::Arrow(a, b, c) => [*a, *b].into_iter().chain(*c).collect(),
                Shape::Text { at, .. } => vec![*at],
                Shape::Pen(points) => points.clone(),
                shape => vec![shape.area().unwrap().centre],
            })
            .collect()
    }

    /// The next step is one more than the highest, whatever the order; 1 with none.
    #[test]
    fn next_step() {
        let mut d = sample();
        assert_eq!(d.next_step(), 4);
        d.objects.retain(|o| !matches!(o.shape, Shape::Step { .. }));
        assert_eq!(d.next_step(), 1);
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

    /// A box stays over the same part of the image after any flip or turn: its four corners are
    /// over the same points of the upright image (a quarter turn once kept its sides).
    #[test]
    fn boxes_follow_turns() {
        let key = |p: Pt| ((p.x * 100.0).round() as i32, (p.y * 100.0).round() as i32);
        let corners = |d: &Doc| {
            let Shape::Rect(a) = d.objects[0].shape else { unreachable!() };
            let mut v: Vec<(i32, i32)> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .iter()
                .map(|(sx, sy)| {
                    let (s, c) = a.rot.to_radians().sin_cos();
                    let (x, y) = (sx * a.w / 2.0, sy * a.h / 2.0);
                    key(d.source(Pt::new(a.centre.x + x * c - y * s, a.centre.y + x * s + y * c)))
                })
                .collect();
            v.sort();
            v
        };
        for rot in [0.0, 25.0] {
            let mut start = Doc::new(100, 80, None);
            start.objects.push(Obj { shape: Shape::Rect(Area { centre: Pt::new(10.0, 3.0), w: 40.0, h: 20.0, rot }), style: Style::default() });
            for op in OPS {
                let mut d = start.clone();
                d.turned(op);
                assert_eq!(corners(&d), corners(&start), "{op:?} at {rot}°");
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

    /// The pixels under a box are those of the photo there, also with the image turned: a box
    /// over the top left quarter gives the top left quarter, wherever the turn put it.
    #[test]
    fn pixels_under_a_box() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(8, 6, |x, y| image::Rgb([x as u8 * 30, y as u8 * 40, 7])));
        let quarter = img.crop_imm(0, 0, 4, 3).to_rgba8();
        let mut d = Doc::new(8, 6, None);
        let a = Area { centre: Pt::new(-2.0, -1.5), w: 4.0, h: 3.0, rot: 0.0 };
        assert_eq!(d.pixels_under(&img, &a), quarter);
        d.turned(Op::Cw);
        let turned = Area { centre: Pt::new(-2.0, -1.5), w: 4.0, h: 3.0, rot: 0.0 }.turned(Op::Cw);
        assert_eq!(d.pixels_under(&img, &turned), image::imageops::rotate90(&quarter));
        // Past the photo: transparent.
        let out = d.pixels_under(&img, &Area { centre: Pt::new(20.0, 0.0), w: 2.0, h: 2.0, rot: 0.0 });
        assert!(out.pixels().all(|p| p[3] == 0));
    }
}
