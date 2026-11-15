//! The objects of the editor under the mouse, without the interface: what is under a point, the
//! handles of an object, an object moved, resized by a handle, or drawn by a drag. Points are
//! those of `doc` (from the centre of the turned image, before the free angle), as the page turns
//! the pointer into them.

use serde::{Deserialize, Serialize};

use super::doc::{Area, Obj, Pt, Shape, Style};

/// What a drag on the image draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Kind {
    Line,
    Arrow,
    Pen,
    Rect,
    Ellipse,
    Step,
    Bubble,
    Lens,
    Spotlight,
    Stamp,
    Highlight,
    Text,
    Cover,
    /// A piece of image: pasted, never drawn.
    Patch,
    /// A filter: from the menu, never drawn.
    Filter,
}

impl Kind {
    pub fn of(o: &Obj) -> Kind {
        match o.shape {
            Shape::Line(..) => Kind::Line,
            Shape::Arrow(..) => Kind::Arrow,
            Shape::Pen(_) => Kind::Pen,
            Shape::Rect(_) => Kind::Rect,
            Shape::Ellipse(_) => Kind::Ellipse,
            Shape::Step { .. } => Kind::Step,
            Shape::Bubble { .. } => Kind::Bubble,
            Shape::Lens { .. } => Kind::Lens,
            Shape::Spotlight(_) => Kind::Spotlight,
            Shape::Stamp(_) => Kind::Stamp,
            Shape::Highlight { .. } => Kind::Highlight,
            Shape::Text { .. } => Kind::Text,
            Shape::Cover(_) => Kind::Cover,
            Shape::Patch { .. } => Kind::Patch,
            Shape::Filter { .. } => Kind::Filter,
        }
    }
}

/// `p` turned by `deg` degrees clockwise (y down) around the origin.
pub fn turned(p: Pt, deg: f32) -> Pt {
    let (s, c) = deg.to_radians().sin_cos();
    Pt::new(p.x * c - p.y * s, p.x * s + p.y * c)
}

fn sub(a: Pt, b: Pt) -> Pt {
    Pt::new(a.x - b.x, a.y - b.y)
}

/// The box a text takes, about: letters 0.6 of their height wide, lines 1.2 apart.
pub fn text_area(at: Pt, text: &str, size: f32, rot: f32) -> Area {
    let lines = text.lines().count().max(1);
    let chars = text.lines().map(|l| l.chars().count()).max().unwrap_or(0).max(1);
    Area { centre: at, w: 0.6 * size * chars as f32, h: 1.2 * size * lines as f32, rot }
}

fn inside(a: &Area, p: Pt, tol: f32) -> bool {
    let d = turned(sub(p, a.centre), -a.rot);
    d.x.abs() <= a.w / 2.0 + tol && d.y.abs() <= a.h / 2.0 + tol
}

/// On the border of the box, `width` wide, or within `tol` of it: not in the empty inside.
fn on_border(a: &Area, p: Pt, width: f32, tol: f32) -> bool {
    let reach = width / 2.0 + tol;
    let d = turned(sub(p, a.centre), -a.rot);
    inside(a, p, reach) && !(d.x.abs() < a.w / 2.0 - reach && d.y.abs() < a.h / 2.0 - reach)
}

/// On the line of the ellipse of the box, `width` wide, or within `tol` of it.
fn on_ellipse(a: &Area, p: Pt, width: f32, tol: f32) -> bool {
    let reach = width / 2.0 + tol;
    let inner = Area { w: a.w - 2.0 * reach, h: a.h - 2.0 * reach, ..*a };
    inside_ellipse(a, p, reach) && (inner.w <= 0.0 || inner.h <= 0.0 || !inside_ellipse(&inner, p, 0.0))
}

/// Inside the ellipse of the box, or within `tol` of it.
fn inside_ellipse(a: &Area, p: Pt, tol: f32) -> bool {
    let d = turned(sub(p, a.centre), -a.rot);
    (d.x / (a.w / 2.0 + tol)).powi(2) + (d.y / (a.h / 2.0 + tol)).powi(2) <= 1.0
}

/// Points along a line from `a` to `b`, bent by the control point `c` (a quadratic Bézier
/// curve): the two ends alone when straight, 17 points when bent.
pub fn curve(a: Pt, b: Pt, c: Option<Pt>) -> Vec<Pt> {
    let Some(c) = c else { return vec![a, b] };
    (0..=16)
        .map(|i| {
            let t = i as f32 / 16.0;
            let (u, v, w) = ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t);
            Pt::new(u * a.x + v * c.x + w * b.x, u * a.y + v * c.y + w * b.y)
        })
        .collect()
}

/// The middle of the line on the curve: the handle that bends it.
pub fn bend_handle(a: Pt, b: Pt, c: Option<Pt>) -> Pt {
    let c = c.unwrap_or(Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0));
    Pt::new(0.25 * a.x + 0.5 * c.x + 0.25 * b.x, 0.25 * a.y + 0.5 * c.y + 0.25 * b.y)
}

/// The control point that puts the middle of the curve at `m`; `None` when that is straight
/// (within half a pixel of the middle of the two ends).
fn bent_to(a: Pt, b: Pt, m: Pt) -> Option<Pt> {
    let mid = Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    ((m.x - mid.x).hypot(m.y - mid.y) > 0.5).then(|| Pt::new(2.0 * m.x - mid.x, 2.0 * m.y - mid.y))
}

/// Distance from `p` to the line, straight or bent.
fn curve_distance(p: Pt, a: Pt, b: Pt, c: Option<Pt>) -> f32 {
    curve(a, b, c).windows(2).map(|s| segment_distance(p, s[0], s[1])).fold(f32::INFINITY, f32::min)
}

fn segment_distance(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// Is `p` on the object, or within `tol` of it? Boxes count all their inside.
pub fn hit(o: &Obj, p: Pt, tol: f32) -> bool {
    match &o.shape {
        Shape::Line(a, b, c) => curve_distance(p, *a, *b, *c) <= o.style.width / 2.0 + tol,
        // The head is as wide as `render::arrow` makes it.
        Shape::Arrow(a, b, c) => curve_distance(p, *a, *b, *c) <= (2.0 * o.style.width + 2.0).max(o.style.width / 2.0) + tol,
        Shape::Text { path: Some((a, b, c)), size, .. } => curve_distance(p, *a, *b, *c) <= 0.6 * size + tol,
        Shape::Pen(points) => points.windows(2).map(|s| segment_distance(p, s[0], s[1])).chain(points.first().map(|q| (p.x - q.x).hypot(p.y - q.y))).fold(f32::INFINITY, f32::min) <= o.style.width / 2.0 + tol,
        Shape::Text { at, text, size, rot, .. } => inside(&text_area(*at, text, *size, *rot), p, tol),
        // Without a fill, only their border: a click in their empty inside is not on them.
        Shape::Ellipse(r) if o.style.fill.is_none() => on_ellipse(r, p, o.style.width, tol),
        Shape::Ellipse(r) => inside_ellipse(r, p, o.style.width / 2.0 + tol),
        Shape::Rect(r) if o.style.fill.is_none() => on_border(r, p, o.style.width, tol),
        Shape::Step { area, .. } => inside_ellipse(area, p, tol),
        // The box, or along the tail (as wide as `render::bubble` makes it at its base).
        Shape::Bubble { area, tail, .. } => inside(area, p, tol) || segment_distance(p, area.centre, *tail) <= 0.15 * area.w.min(area.h) + tol,
        shape => shape.area().is_some_and(|r| inside(r, p, tol)),
    }
}

/// The topmost object under `p`; a filter or a spotlight only where no object is, as they are
/// drawn under them all (a filter on the whole image took every click).
pub fn under(objects: &[Obj], p: Pt, tol: f32) -> Option<usize> {
    let layer = |o: &Obj| o.shape.is_layer();
    objects.iter().rposition(|o| !layer(o) && hit(o, p, tol)).or_else(|| objects.iter().rposition(|o| layer(o) && hit(o, p, tol)))
}

/// The topmost object of `kind` under `p`: what a drawing tool's click takes.
pub fn under_of(objects: &[Obj], p: Pt, tol: f32, kind: Kind) -> Option<usize> {
    objects.iter().rposition(|o| Kind::of(o) == kind && hit(o, p, tol))
}

/// The object moved by `d`.
pub fn moved(o: &Obj, d: Pt) -> Obj {
    let m = |p: Pt| Pt::new(p.x + d.x, p.y + d.y);
    let mut o = o.clone();
    for p in o.shape.points_mut() {
        *p = m(*p);
    }
    if let Some(a) = o.shape.area_mut() {
        a.centre = m(a.centre);
    }
    o
}

/// The box around a stroke drawn by hand, upright on the image.
pub fn pen_box(points: &[Pt]) -> Area {
    let (mut l, mut t, mut r, mut b) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points {
        (l, t, r, b) = (l.min(p.x), t.min(p.y), r.max(p.x), b.max(p.y));
    }
    Area { centre: Pt::new((l + r) / 2.0, (t + b) / 2.0), w: r - l, h: b - t, rot: 0.0 }
}

/// Corners of a box, clockwise from the top left one (on the box's own sides).
fn corners(r: &Area) -> [Pt; 4] {
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
        let d = turned(Pt::new(sx * r.w / 2.0, sy * r.h / 2.0), r.rot);
        Pt::new(r.centre.x + d.x, r.centre.y + d.y)
    })
}

/// Points that resize the object when dragged: the two ends of a line and its middle (which
/// bends it), the four corners of a box. A text has none: its size is in the panel.
pub fn handles(o: &Obj) -> Vec<Pt> {
    match &o.shape {
        Shape::Line(a, b, c) | Shape::Arrow(a, b, c) => vec![*a, *b, bend_handle(*a, *b, *c)],
        // On a curve: its two ends and the middle that bends it, as a line.
        Shape::Text { path: Some((a, b, c)), .. } => vec![*a, *b, bend_handle(*a, *b, *c)],
        Shape::Pen(points) => corners(&pen_box(points)).to_vec(),
        // The corners, then the end of the tail.
        Shape::Bubble { area, tail, .. } => corners(area).into_iter().chain([*tail]).collect(),
        shape => shape.area().map_or_else(Vec::new, |r| corners(r).to_vec()),
    }
}

/// The object with handle `i` moved to `p`: the other end of a line, the opposite corner of a box
/// stay where they were. A box keeps its own angle and is at least a pixel wide; a step stays
/// square, and so does a stamp.
#[cfg(test)]
pub fn with_handle(o: &Obj, i: usize, p: Pt) -> Obj {
    with_handle_kept(o, i, p, false)
}

/// `with_handle`; with `keep` (Shift held) a box keeps the proportions it had.
pub fn with_handle_kept(o: &Obj, i: usize, p: Pt, keep: bool) -> Obj {
    let square = matches!(o.shape, Shape::Step { .. } | Shape::Stamp(_));
    let resized = |r: &Area| {
        let fixed = corners(r)[(i + 2) % 4];
        // The dragged corner from the fixed one, on the box's own sides.
        let mut q = turned(sub(p, fixed), -r.rot);
        if square {
            let side = q.x.abs().max(q.y.abs());
            q = Pt::new(side.copysign(q.x), side.copysign(q.y));
        } else if keep && r.w > 0.0 && r.h > 0.0 {
            // As much larger as the farther of the two sides says, both alike.
            let k = (q.x.abs() / r.w).max(q.y.abs() / r.h);
            q = Pt::new((k * r.w).copysign(q.x), (k * r.h).copysign(q.y));
        }
        let mid = turned(Pt::new(q.x / 2.0, q.y / 2.0), r.rot);
        Area { centre: Pt::new(fixed.x + mid.x, fixed.y + mid.y), w: q.x.abs().max(1.0), h: q.y.abs().max(1.0), rot: r.rot }
    };
    let mut o = o.clone();
    match &mut o.shape {
        Shape::Line(a, b, c) => o.shape = line_handle(*a, *b, *c, i, p, Shape::Line),
        Shape::Arrow(a, b, c) => o.shape = line_handle(*a, *b, *c, i, p, Shape::Arrow),
        Shape::Text { path: Some((a, b, c)), .. } => {
            let Shape::Line(na, nb, nc) = line_handle(*a, *b, *c, i, p, Shape::Line) else { unreachable!() };
            (*a, *b, *c) = (na, nb, nc);
        }
        Shape::Bubble { tail, .. } if i == 4 => *tail = p,
        // Its points stretched with the box, from the opposite corner (a flat stroke stays flat).
        Shape::Pen(points) => {
            let (from, to) = (pen_box(points), resized(&pen_box(points)));
            let k = |a: f32, b: f32| if a > 0.0 { b / a } else { 1.0 };
            let (kx, ky) = (k(from.w, to.w), k(from.h, to.h));
            for q in points.iter_mut() {
                *q = Pt::new(to.centre.x + (q.x - from.centre.x) * kx, to.centre.y + (q.y - from.centre.y) * ky);
            }
        }
        shape => {
            if let Some(r) = shape.area_mut() {
                *r = resized(r);
            }
        }
    }
    o
}

/// A line with handle `i` moved to `p`: an end (the bend goes with it, so the curve keeps its
/// shape), or the middle, which bends it.
fn line_handle(a: Pt, b: Pt, c: Option<Pt>, i: usize, p: Pt, shape: fn(Pt, Pt, Option<Pt>) -> Shape) -> Shape {
    // The control point moves by half of what the end moved: the middle of the curve follows.
    let half = |from: Pt| c.map(|c| Pt::new(c.x + (p.x - from.x) / 2.0, c.y + (p.y - from.y) / 2.0));
    match i {
        0 => shape(p, b, half(a)),
        1 => shape(a, p, half(b)),
        _ => shape(a, b, bent_to(a, b, p)),
    }
}

/// The object a drag from `a` to `b` draws, on an image shown turned by `angle`: boxes and text
/// upright on the screen (their own angle `-angle`), a text with no letters yet, its size
/// `style.width`; with `square` (Shift held) a box as wide as high. `None` for a drag too short
/// to draw anything.
pub fn drawn(kind: Kind, a: Pt, b: Pt, angle: f32, style: Style, square: bool) -> Option<Obj> {
    let (sa, mut sb) = (turned(a, angle), turned(b, angle));
    if square {
        let side = (sb.x - sa.x).abs().max((sb.y - sa.y).abs());
        sb = Pt::new(sa.x + side.copysign(sb.x - sa.x), sa.y + side.copysign(sb.y - sa.y));
    }
    let area = || {
        let (w, h) = ((sb.x - sa.x).abs(), (sb.y - sa.y).abs());
        (w >= 1.0 && h >= 1.0).then(|| Area { centre: turned(Pt::new((sa.x + sb.x) / 2.0, (sa.y + sb.y) / 2.0), -angle), w, h, rot: -angle })
    };
    let long = (b.x - a.x).hypot(b.y - a.y) >= 1.0;
    let shape = match kind {
        Kind::Line => long.then_some(Shape::Line(a, b, None))?,
        Kind::Arrow => long.then_some(Shape::Arrow(a, b, None))?,
        // Its first two points; the page adds the others as the pointer goes on.
        Kind::Pen => long.then_some(Shape::Pen(vec![a, b]))?,
        Kind::Rect => Shape::Rect(area()?),
        Kind::Ellipse => Shape::Ellipse(area()?),
        Kind::Spotlight => Shape::Spotlight(area()?),
        Kind::Lens => Shape::Lens { area: area()?, preview: Default::default() },
        // The tail at first down and to the left, out of the box.
        Kind::Bubble => {
            let area = area()?;
            let t = turned(Pt::new(-0.3 * area.w, 0.9 * area.h), area.rot);
            Shape::Bubble { area, tail: Pt::new(area.centre.x + t.x, area.centre.y + t.y), text: String::new(), font: None }
        }
        Kind::Highlight => Shape::Highlight { area: area()?, preview: Default::default() },
        Kind::Cover => Shape::Cover(area()?),
        // Placed with a click (`step`).
        Kind::Patch | Kind::Filter | Kind::Step | Kind::Stamp => return None,
        Kind::Text => Shape::Text { at: a, text: String::new(), size: style.width, rot: -angle, font: None, path: None },
    };
    Some(Obj { shape, style })
}

/// Whether nothing of `o` is in the result, the part of the image turned by `angle` that `crop`
/// keeps: its box, with its line and an arrow's head, does not reach it at all. A spotlight never
/// is: it darkens all the rest, the result too.
pub fn outside(o: &Obj, angle: f32, crop: crate::rotate::Crop) -> bool {
    let w = o.style.width;
    let line = |a: Pt, b: Pt, c: Option<Pt>| [a, b].into_iter().chain(c).collect::<Vec<_>>();
    let (points, reach) = match &o.shape {
        Shape::Spotlight(_) => return false,
        // The curve stays within its ends and its control point.
        Shape::Line(a, b, c) => (line(*a, *b, *c), w / 2.0),
        // The head is 4 widths and 4 pixels long, as wide.
        Shape::Arrow(a, b, c) => (line(*a, *b, *c), 2.5 * w + 2.0),
        Shape::Pen(points) => (points.clone(), w / 2.0),
        Shape::Bubble { area, tail, .. } => (corners(area).into_iter().chain([*tail]).collect(), w / 2.0),
        Shape::Text { path: Some((a, b, c)), size, .. } => (line(*a, *b, *c), *size),
        Shape::Text { at, text, size, rot, .. } => (corners(&text_area(*at, text, *size, *rot)).to_vec(), 0.0),
        Shape::Rect(a) | Shape::Ellipse(a) | Shape::Lens { area: a, .. } => (corners(a).to_vec(), w / 2.0),
        shape => (shape.area().map_or_else(Vec::new, |a| corners(a).to_vec()), 0.0),
    };
    let (mut l, mut t, mut r, mut b) = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points.into_iter().map(|p| turned(p, angle)) {
        (l, t, r, b) = (l.min(p.x), t.min(p.y), r.max(p.x), b.max(p.y));
    }
    let (hw, hh) = (crop.w / 2.0, crop.h / 2.0);
    !(l - reach < crop.cx + hw && r + reach > crop.cx - hw && t - reach < crop.cy + hh && b + reach > crop.cy - hh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pt, b: Pt) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    fn rect(rot: f32) -> Obj {
        Obj { shape: Shape::Rect(Area { centre: Pt::new(10.0, 5.0), w: 20.0, h: 10.0, rot }), style: Style { fill: Some([0; 3]), ..Style::default() } }
    }

    /// Lines by their distance and width, boxes by their inside (turned too), texts by their box,
    /// the topmost first.
    #[test]
    fn what_is_under() {
        let line = Obj { shape: Shape::Line(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), None), style: Style { width: 4.0, ..Style::default() } };
        assert!(hit(&line, Pt::new(50.0, 3.0), 1.0) && !hit(&line, Pt::new(50.0, 4.0), 1.0) && !hit(&line, Pt::new(104.0, 0.0), 1.0));
        assert!(hit(&rect(0.0), Pt::new(19.0, 9.0), 0.0) && !hit(&rect(0.0), Pt::new(10.0, 11.0), 0.0));
        // Turned upright: 10 wide, 20 high around 10, 5.
        assert!(hit(&rect(90.0), Pt::new(10.0, 14.0), 0.0) && !hit(&rect(90.0), Pt::new(19.0, 5.0), 0.0));
        let text = Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "abcd".into(), size: 10.0, rot: 0.0, font: None, path: None }, style: Style::default() };
        assert!(hit(&text, Pt::new(11.0, 5.0), 0.0) && !hit(&text, Pt::new(13.0, 0.0), 0.0));
        assert_eq!(under(&[rect(0.0), rect(0.0), line], Pt::new(10.0, 5.0), 0.0), Some(1));
        assert_eq!(under(&[rect(0.0)], Pt::new(-50.0, 5.0), 0.0), None);
        // A filled ellipse by its inside: not the corners of its box.
        let ellipse = Obj { shape: Shape::Ellipse(Area { centre: Pt::new(0.0, 0.0), w: 20.0, h: 10.0, rot: 0.0 }), style: Style { width: 2.0, fill: Some([0; 3]), ..Style::default() } };
        assert!(hit(&ellipse, Pt::new(9.0, 0.0), 0.0) && !hit(&ellipse, Pt::new(8.0, 4.5), 0.0));
    }

    /// A corner dragged: the opposite one stays, the box keeps its angle; the ends of a line.
    #[test]
    fn handles_resize() {
        for rot in [0.0, 30.0, -90.0] {
            let o = rect(rot);
            let before = handles(&o);
            let to = Pt::new(before[2].x + 7.0, before[2].y - 3.0);
            let after = handles(&with_handle(&o, 2, to));
            assert!(close(after[0], before[0]), "{rot}: opposite corner {:?} {:?}", after[0], before[0]);
            let Shape::Rect(r) = with_handle(&o, 2, to).shape else { unreachable!() };
            assert_eq!(r.rot, rot);
        }
        // Straight: the corner goes where it was dragged.
        assert!(close(handles(&with_handle(&rect(0.0), 2, Pt::new(30.0, 30.0)))[2], Pt::new(30.0, 30.0)));
        let line = Obj { shape: Shape::Arrow(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), None), style: Style::default() };
        assert_eq!(with_handle(&line, 1, Pt::new(5.0, 5.0)).shape, Shape::Arrow(Pt::new(0.0, 0.0), Pt::new(5.0, 5.0), None));
        assert_eq!(handles(&moved(&line, Pt::new(1.0, 2.0))), vec![Pt::new(1.0, 2.0), Pt::new(11.0, 2.0), Pt::new(6.0, 2.0)]);
    }

    /// Drawn on an image shown at 30°: a box upright on the screen, its corners where the drag
    /// started and ended; nothing for a click without a drag (but a text).
    #[test]
    fn drawn_upright_on_screen() {
        let (a, b) = (turned(Pt::new(-10.0, -5.0), -30.0), turned(Pt::new(30.0, 15.0), -30.0));
        let o = drawn(Kind::Cover, a, b, 30.0, Style::default(), false).unwrap();
        let Shape::Cover(r) = o.shape else { unreachable!() };
        assert!((r.w - 40.0).abs() < 1e-3 && (r.h - 20.0).abs() < 1e-3 && r.rot == -30.0);
        let h = handles(&o);
        assert!(close(h[0], a) && close(h[2], b), "{h:?}");
        assert!(drawn(Kind::Rect, a, a, 30.0, Style::default(), false).is_none() && drawn(Kind::Arrow, a, a, 0.0, Style::default(), false).is_none());
        let t = drawn(Kind::Text, a, a, 30.0, Style { width: 24.0, ..Style::default() }, false).unwrap();
        // With Shift: as wide as high, the longer side kept, towards where the drag went.
        let Shape::Ellipse(e) = drawn(Kind::Ellipse, Pt::new(0.0, 0.0), Pt::new(-30.0, 10.0), 0.0, Style::default(), true).unwrap().shape else { unreachable!() };
        assert_eq!((e.w, e.h, e.centre), (30.0, 30.0, Pt::new(-15.0, 15.0)));
        assert_eq!(t.shape, Shape::Text { at: a, text: String::new(), size: 24.0, rot: -30.0, font: None, path: None });
    }

    /// Drawn on the dark area around the result: outside if nothing of it, its line included,
    /// reaches the crop; a part inside is enough to stay. With the image turned and cropped.
    #[test]
    fn outside_the_result() {
        use crate::rotate::Crop;
        let rect = |x: f32, y: f32, width: f32| Obj { shape: Shape::Rect(Area { centre: Pt::new(x, y), w: 4.0, h: 4.0, rot: 0.0 }), style: Style { width, ..Style::default() } };
        // Only x from 0 to 20 of the image is kept.
        let crop = Crop { cx: 10.0, cy: 0.0, w: 20.0, h: 48.0 };
        assert!(outside(&rect(-10.0, 0.0, 0.0), 0.0, crop) && outside(&rect(30.0, 0.0, 0.0), 0.0, crop) && outside(&rect(10.0, 30.0, 0.0), 0.0, crop));
        assert!(!outside(&rect(-1.0, 0.0, 0.0), 0.0, crop) && !outside(&rect(10.0, 0.0, 0.0), 0.0, crop), "partly or all inside");
        // Its border, as wide as `width` on the edge of the box: 2 pixels out of 4 reach x = 0.
        assert!(outside(&rect(-4.0, 0.0, 4.0), 0.0, crop) && !outside(&rect(-4.0, 0.0, 6.0), 0.0, crop));
        // A box larger than the result, all round it.
        let over = Obj { shape: Shape::Cover(Area { centre: Pt::new(0.0, 0.0), w: 200.0, h: 200.0, rot: 30.0 }), style: Style::default() };
        assert!(!outside(&over, 0.0, crop));
        // Turned by 90°, the result is 48 wide and 64 high: y = 28 of the image goes to the side,
        // out of it, x = 28 down, in it.
        let turned = Crop::centred(48.0, 64.0);
        assert!(outside(&rect(0.0, 28.0, 0.0), 90.0, turned) && !outside(&rect(28.0, 0.0, 0.0), 90.0, turned));
        // Lines by their ends and bend, strokes by their points.
        let line = |a: Pt, b: Pt, c: Option<Pt>| Obj { shape: Shape::Line(a, b, c), style: Style { width: 2.0, ..Style::default() } };
        assert!(outside(&line(Pt::new(-30.0, -10.0), Pt::new(-5.0, 10.0), None), 0.0, crop));
        assert!(!outside(&line(Pt::new(-30.0, -10.0), Pt::new(-5.0, 10.0), Some(Pt::new(15.0, 0.0))), 0.0, crop), "bent into it");
        let pen = Obj { shape: Shape::Pen(vec![Pt::new(-30.0, 0.0), Pt::new(-10.0, 5.0), Pt::new(5.0, 0.0)]), style: Style::default() };
        assert!(!outside(&pen, 0.0, crop) && outside(&Obj { shape: Shape::Pen(vec![Pt::new(-30.0, 0.0), Pt::new(-10.0, 5.0)]), ..pen }, 0.0, crop));
        // A spotlight darkens the rest, the result too: never outside.
        let spot = Obj { shape: Shape::Spotlight(Area { centre: Pt::new(-50.0, 0.0), w: 4.0, h: 4.0, rot: 0.0 }), style: Style::default() };
        assert!(!outside(&spot, 0.0, crop));
    }
}

/// `new`, made again by a drag (drawn, moved, resized) from what was `old`, with `old`'s preview
/// until its own is made: a highlighter or a magnifier being drawn was empty until let go, one
/// moved showed the part it came from.
pub fn redrawn(old: &mut Obj, mut new: Obj) -> Obj {
    if let (Some(from), Some(to)) = (old.shape.preview_mut(), new.shape.preview_mut()) {
        *to = std::mem::take(from);
    }
    new
}

/// A step placed with a click at `at` on an image shown turned by `angle`: upright on the screen,
/// as wide as `style.width`, numbered `n`.
pub fn step(at: Pt, n: u32, angle: f32, style: Style) -> Obj {
    Obj { shape: Shape::Step { area: Area { centre: at, w: style.width, h: style.width, rot: -angle }, n }, style }
}

/// A stamp placed with a click at `at`, as `step`, its icon that of the style.
pub fn stamp(at: Pt, angle: f32, style: Style) -> Obj {
    Obj { shape: Shape::Stamp(Area { centre: at, w: style.width, h: style.width, rot: -angle }), style }
}

/// A stroke drawn by hand with the pointer now at `p`: the point added when it is at least
/// `step` from the last one (a pointer that hardly moved would add a crowd of points).
pub fn pen_to(o: &mut Obj, p: Pt, step: f32) {
    if let Shape::Pen(points) = &mut o.shape
        && points.last().is_none_or(|q| (p.x - q.x).hypot(p.y - q.y) >= step)
    {
        points.push(p);
    }
}

#[cfg(test)]
mod curves {
    use super::*;

    /// The middle handle bends the line: the curve then passes where it was dropped, the hit test
    /// follows the curve and not the straight line; dropped back on the straight middle, straight
    /// again; an end dragged keeps the middle of the curve following it.
    #[test]
    fn bent_by_the_middle() {
        let line = Obj { shape: Shape::Line(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0), None), style: Style { width: 2.0, ..Style::default() } };
        let bent = with_handle(&line, 2, Pt::new(50.0, 30.0));
        let h = handles(&bent);
        assert!((h[2].x - 50.0).abs() < 1e-3 && (h[2].y - 30.0).abs() < 1e-3, "{h:?}");
        assert!(hit(&bent, Pt::new(50.0, 30.0), 0.5) && !hit(&bent, Pt::new(50.0, 0.0), 0.5));
        assert_eq!(with_handle(&bent, 2, Pt::new(50.0, 0.2)).shape, line.shape);
        let moved_end = with_handle(&bent, 1, Pt::new(120.0, 0.0));
        let m = handles(&moved_end)[2];
        assert!((m.x - 60.0).abs() < 1e-3 && (m.y - 30.0).abs() < 1e-3, "{m:?}");
    }

    /// A stroke by hand: points added only when the pointer moved enough; hit along it; dragging
    /// a corner stretches it from the opposite one.
    #[test]
    fn by_hand() {
        let mut o = drawn(Kind::Pen, Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), 0.0, Style { width: 2.0, ..Style::default() }, false).unwrap();
        pen_to(&mut o, Pt::new(10.5, 0.0), 2.0);
        pen_to(&mut o, Pt::new(10.0, 10.0), 2.0);
        assert_eq!(o.shape, Shape::Pen(vec![Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 10.0)]));
        assert!(hit(&o, Pt::new(11.0, 5.0), 0.5) && !hit(&o, Pt::new(5.0, 5.0), 0.5));
        // Bottom right corner (10, 10) to (20, 30): twice as wide, three times as high.
        let big = with_handle(&o, 2, Pt::new(20.0, 30.0));
        assert_eq!(big.shape, Shape::Pen(vec![Pt::new(0.0, 0.0), Pt::new(20.0, 0.0), Pt::new(20.0, 30.0)]));
        assert_eq!(handles(&moved(&big, Pt::new(1.0, 1.0)))[0], Pt::new(1.0, 1.0));
    }

    /// A step: hit inside its disc, not in the corners of its box; a corner dragged keeps it
    /// square, the opposite corner where it was.
    #[test]
    fn step_stays_round() {
        let o = step(Pt::new(0.0, 0.0), 1, 0.0, Style { width: 20.0, ..Style::default() });
        assert!(hit(&o, Pt::new(9.0, 0.0), 0.0) && !hit(&o, Pt::new(9.0, 9.0), 0.0));
        let Shape::Step { area, .. } = with_handle(&o, 2, Pt::new(30.0, 15.0)).shape else { unreachable!() };
        assert_eq!((area.w, area.h, area.centre), (40.0, 40.0, Pt::new(10.0, 10.0)));
    }

    /// A bubble: drawn with its tail below it, which its fifth handle moves alone; hit along the
    /// tail, outside the box.
    #[test]
    fn bubble_tail() {
        let o = drawn(Kind::Bubble, Pt::new(0.0, 0.0), Pt::new(40.0, 20.0), 0.0, Style::default(), false).unwrap();
        let Shape::Bubble { tail, .. } = o.shape else { unreachable!() };
        assert_eq!(tail, Pt::new(8.0, 28.0));
        assert!(hit(&o, Pt::new(10.0, 24.0), 0.0) && !hit(&o, Pt::new(35.0, 28.0), 0.0));
        let moved_tail = with_handle(&o, 4, Pt::new(60.0, 40.0));
        assert_eq!(handles(&moved_tail)[..4], handles(&o)[..4]);
        assert_eq!(handles(&moved_tail)[4], Pt::new(60.0, 40.0));
    }

    /// A highlighter drawn again as the drag goes on keeps the preview it had: it stays tinted.
    #[test]
    fn redrawn_keeps_the_preview() {
        use crate::edit::doc::{Pixels, Preview};
        let a = Area { centre: Pt::new(0.0, 0.0), w: 4.0, h: 4.0, rot: 0.0 };
        let pixels = Pixels::new(image::RgbaImage::new(2, 2));
        let mut old = Obj { shape: Shape::Highlight { area: a, preview: Preview(Some((a, 1, pixels.clone()))) }, style: Style::default() };
        let new = drawn(Kind::Highlight, Pt::new(0.0, 0.0), Pt::new(9.0, 9.0), 0.0, Style::default(), false).unwrap();
        let Shape::Highlight { preview, .. } = redrawn(&mut old, new).shape else { unreachable!() };
        assert!(preview.0.is_some_and(|(_, _, p)| p == pixels));
    }

    /// Without a fill a rectangle or an ellipse is taken by its border only, its empty inside is
    /// not on it; filled, all of it; a drawing tool takes only its own kind.
    #[test]
    fn empty_inside_and_own_kind() {
        let a = Area { centre: Pt::new(0.0, 0.0), w: 40.0, h: 20.0, rot: 0.0 };
        let empty = Style { width: 2.0, ..Style::default() };
        let rect = Obj { shape: Shape::Rect(a), style: empty };
        let ellipse = Obj { shape: Shape::Ellipse(a), style: empty };
        assert!(hit(&rect, Pt::new(19.0, 3.0), 1.0) && !hit(&rect, Pt::new(0.0, 0.0), 1.0) && !hit(&rect, Pt::new(25.0, 0.0), 1.0));
        assert!(hit(&ellipse, Pt::new(0.0, 9.5), 1.0) && !hit(&ellipse, Pt::new(0.0, 0.0), 1.0));
        let filled = Obj { shape: Shape::Rect(a), style: Style { fill: Some([0; 3]), ..empty } };
        assert!(hit(&filled, Pt::new(0.0, 0.0), 1.0));
        let line = Obj { shape: Shape::Line(Pt::new(-20.0, 0.0), Pt::new(20.0, 0.0), None), style: empty };
        let objects = [filled, line];
        assert_eq!((under(&objects, Pt::new(0.0, 0.0), 1.0), under_of(&objects, Pt::new(0.0, 0.0), 1.0, Kind::Rect), under_of(&objects, Pt::new(0.0, 0.0), 1.0, Kind::Step)), (Some(1), Some(0), None));
    }

    /// A corner dragged with Shift keeps the box's proportions: a box 20 × 10 dragged to 60 wide
    /// and 12 high becomes 60 × 30; without Shift 60 × 12.
    #[test]
    fn proportions_kept() {
        let o = Obj { shape: Shape::Rect(Area { centre: Pt::new(10.0, 5.0), w: 20.0, h: 10.0, rot: 0.0 }), style: Style::default() };
        let size = |o: &Obj| { let a = o.shape.area().unwrap(); (a.w, a.h) };
        assert_eq!(size(&with_handle_kept(&o, 2, Pt::new(60.0, 12.0), true)), (60.0, 30.0));
        assert_eq!(size(&with_handle_kept(&o, 2, Pt::new(60.0, 12.0), false)), (60.0, 12.0));
    }
}
