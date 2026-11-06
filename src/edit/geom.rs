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
            Shape::Highlight(_) => Kind::Highlight,
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
        Shape::Ellipse(r) => inside_ellipse(r, p, o.style.width / 2.0 + tol),
        Shape::Step { area, .. } => inside_ellipse(area, p, tol),
        shape => shape.area().is_some_and(|r| inside(r, p, tol)),
    }
}

/// The topmost object under `p`; a filter only where no object is, as it is drawn under them all
/// (a filter on the whole image took every click).
pub fn under(objects: &[Obj], p: Pt, tol: f32) -> Option<usize> {
    let filter = |o: &Obj| matches!(o.shape, Shape::Filter { .. });
    objects.iter().rposition(|o| !filter(o) && hit(o, p, tol)).or_else(|| objects.iter().rposition(|o| filter(o) && hit(o, p, tol)))
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
        shape => shape.area().map_or_else(Vec::new, |r| corners(r).to_vec()),
    }
}

/// The object with handle `i` moved to `p`: the other end of a line, the opposite corner of a box
/// stay where they were. A box keeps its own angle and is at least a pixel wide; a step stays
/// square.
pub fn with_handle(o: &Obj, i: usize, p: Pt) -> Obj {
    let square = matches!(o.shape, Shape::Step { .. });
    let resized = |r: &Area| {
        let fixed = corners(r)[(i + 2) % 4];
        // The dragged corner from the fixed one, on the box's own sides.
        let mut q = turned(sub(p, fixed), -r.rot);
        if square {
            let side = q.x.abs().max(q.y.abs());
            q = Pt::new(side.copysign(q.x), side.copysign(q.y));
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
        Kind::Highlight => Shape::Highlight(area()?),
        Kind::Cover => Shape::Cover(area()?),
        // Placed with a click (`step`).
        Kind::Patch | Kind::Filter | Kind::Step => return None,
        Kind::Text => Shape::Text { at: a, text: String::new(), size: style.width, rot: -angle, font: None, path: None },
    };
    Some(Obj { shape, style })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pt, b: Pt) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    fn rect(rot: f32) -> Obj {
        Obj { shape: Shape::Rect(Area { centre: Pt::new(10.0, 5.0), w: 20.0, h: 10.0, rot }), style: Style::default() }
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
        // An ellipse by its inside: not the corners of its box.
        let ellipse = Obj { shape: Shape::Ellipse(Area { centre: Pt::new(0.0, 0.0), w: 20.0, h: 10.0, rot: 0.0 }), style: Style { width: 2.0, ..Style::default() } };
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
}

/// A step placed with a click at `at` on an image shown turned by `angle`: upright on the screen,
/// as wide as `style.width`, numbered `n`.
pub fn step(at: Pt, n: u32, angle: f32, style: Style) -> Obj {
    Obj { shape: Shape::Step { area: Area { centre: at, w: style.width, h: style.width, rot: -angle }, n }, style }
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
}
