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
    Rect,
    Highlight,
    Text,
    Cover,
}

impl Kind {
    pub fn of(o: &Obj) -> Kind {
        match o.shape {
            Shape::Line(..) => Kind::Line,
            Shape::Arrow(..) => Kind::Arrow,
            Shape::Rect(_) => Kind::Rect,
            Shape::Highlight(_) => Kind::Highlight,
            Shape::Text { .. } => Kind::Text,
            Shape::Cover(_) => Kind::Cover,
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

fn segment_distance(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// Is `p` on the object, or within `tol` of it? Boxes count all their inside.
pub fn hit(o: &Obj, p: Pt, tol: f32) -> bool {
    match &o.shape {
        Shape::Line(a, b) => segment_distance(p, *a, *b) <= o.style.width / 2.0 + tol,
        // The head is as wide as `render::arrow` makes it.
        Shape::Arrow(a, b) => segment_distance(p, *a, *b) <= (2.0 * o.style.width + 2.0).max(o.style.width / 2.0) + tol,
        Shape::Rect(r) | Shape::Highlight(r) | Shape::Cover(r) => inside(r, p, tol),
        Shape::Text { at, text, size, rot } => inside(&text_area(*at, text, *size, *rot), p, tol),
    }
}

/// The topmost object under `p`.
pub fn under(objects: &[Obj], p: Pt, tol: f32) -> Option<usize> {
    objects.iter().rposition(|o| hit(o, p, tol))
}

/// The object moved by `d`.
pub fn moved(o: &Obj, d: Pt) -> Obj {
    let m = |p: Pt| Pt::new(p.x + d.x, p.y + d.y);
    let area = |r: &Area| Area { centre: m(r.centre), ..*r };
    let shape = match &o.shape {
        Shape::Line(a, b) => Shape::Line(m(*a), m(*b)),
        Shape::Arrow(a, b) => Shape::Arrow(m(*a), m(*b)),
        Shape::Rect(r) => Shape::Rect(area(r)),
        Shape::Highlight(r) => Shape::Highlight(area(r)),
        Shape::Cover(r) => Shape::Cover(area(r)),
        Shape::Text { at, text, size, rot } => Shape::Text { at: m(*at), text: text.clone(), size: *size, rot: *rot },
    };
    Obj { shape, style: o.style }
}

/// Corners of a box, clockwise from the top left one (on the box's own sides).
fn corners(r: &Area) -> [Pt; 4] {
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
        let d = turned(Pt::new(sx * r.w / 2.0, sy * r.h / 2.0), r.rot);
        Pt::new(r.centre.x + d.x, r.centre.y + d.y)
    })
}

/// Points that resize the object when dragged: the two ends of a line, the four corners of a
/// box. A text has none: its size is in the panel.
pub fn handles(o: &Obj) -> Vec<Pt> {
    match &o.shape {
        Shape::Line(a, b) | Shape::Arrow(a, b) => vec![*a, *b],
        Shape::Rect(r) | Shape::Highlight(r) | Shape::Cover(r) => corners(r).to_vec(),
        Shape::Text { .. } => Vec::new(),
    }
}

/// The object with handle `i` moved to `p`: the other end of a line, the opposite corner of a box
/// stay where they were. A box keeps its own angle and is at least a pixel wide.
pub fn with_handle(o: &Obj, i: usize, p: Pt) -> Obj {
    let resized = |r: &Area| {
        let fixed = corners(r)[(i + 2) % 4];
        // The dragged corner from the fixed one, on the box's own sides.
        let q = turned(sub(p, fixed), -r.rot);
        let mid = turned(Pt::new(q.x / 2.0, q.y / 2.0), r.rot);
        Area { centre: Pt::new(fixed.x + mid.x, fixed.y + mid.y), w: q.x.abs().max(1.0), h: q.y.abs().max(1.0), rot: r.rot }
    };
    let shape = match &o.shape {
        Shape::Line(a, b) => if i == 0 { Shape::Line(p, *b) } else { Shape::Line(*a, p) },
        Shape::Arrow(a, b) => if i == 0 { Shape::Arrow(p, *b) } else { Shape::Arrow(*a, p) },
        Shape::Rect(r) => Shape::Rect(resized(r)),
        Shape::Highlight(r) => Shape::Highlight(resized(r)),
        Shape::Cover(r) => Shape::Cover(resized(r)),
        text => text.clone(),
    };
    Obj { shape, style: o.style }
}

/// The object a drag from `a` to `b` draws, on an image shown turned by `angle`: boxes and text
/// upright on the screen (their own angle `-angle`), a text with no letters yet, its size
/// `style.width`. `None` for a drag too short to draw anything.
pub fn drawn(kind: Kind, a: Pt, b: Pt, angle: f32, style: Style) -> Option<Obj> {
    let (sa, sb) = (turned(a, angle), turned(b, angle));
    let area = || {
        let (w, h) = ((sb.x - sa.x).abs(), (sb.y - sa.y).abs());
        (w >= 1.0 && h >= 1.0).then(|| Area { centre: turned(Pt::new((sa.x + sb.x) / 2.0, (sa.y + sb.y) / 2.0), -angle), w, h, rot: -angle })
    };
    let long = (b.x - a.x).hypot(b.y - a.y) >= 1.0;
    let shape = match kind {
        Kind::Line => long.then_some(Shape::Line(a, b))?,
        Kind::Arrow => long.then_some(Shape::Arrow(a, b))?,
        Kind::Rect => Shape::Rect(area()?),
        Kind::Highlight => Shape::Highlight(area()?),
        Kind::Cover => Shape::Cover(area()?),
        Kind::Text => Shape::Text { at: a, text: String::new(), size: style.width, rot: -angle },
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
        let line = Obj { shape: Shape::Line(Pt::new(0.0, 0.0), Pt::new(100.0, 0.0)), style: Style { width: 4.0, ..Style::default() } };
        assert!(hit(&line, Pt::new(50.0, 3.0), 1.0) && !hit(&line, Pt::new(50.0, 4.0), 1.0) && !hit(&line, Pt::new(104.0, 0.0), 1.0));
        assert!(hit(&rect(0.0), Pt::new(19.0, 9.0), 0.0) && !hit(&rect(0.0), Pt::new(10.0, 11.0), 0.0));
        // Turned upright: 10 wide, 20 high around 10, 5.
        assert!(hit(&rect(90.0), Pt::new(10.0, 14.0), 0.0) && !hit(&rect(90.0), Pt::new(19.0, 5.0), 0.0));
        let text = Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "abcd".into(), size: 10.0, rot: 0.0 }, style: Style::default() };
        assert!(hit(&text, Pt::new(11.0, 5.0), 0.0) && !hit(&text, Pt::new(13.0, 0.0), 0.0));
        assert_eq!(under(&[rect(0.0), rect(0.0), line], Pt::new(10.0, 5.0), 0.0), Some(1));
        assert_eq!(under(&[rect(0.0)], Pt::new(-50.0, 5.0), 0.0), None);
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
        let line = Obj { shape: Shape::Arrow(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)), style: Style::default() };
        assert_eq!(with_handle(&line, 1, Pt::new(5.0, 5.0)).shape, Shape::Arrow(Pt::new(0.0, 0.0), Pt::new(5.0, 5.0)));
        assert_eq!(handles(&moved(&line, Pt::new(1.0, 2.0))), vec![Pt::new(1.0, 2.0), Pt::new(11.0, 2.0)]);
    }

    /// Drawn on an image shown at 30°: a box upright on the screen, its corners where the drag
    /// started and ended; nothing for a click without a drag (but a text).
    #[test]
    fn drawn_upright_on_screen() {
        let (a, b) = (turned(Pt::new(-10.0, -5.0), -30.0), turned(Pt::new(30.0, 15.0), -30.0));
        let o = drawn(Kind::Cover, a, b, 30.0, Style::default()).unwrap();
        let Shape::Cover(r) = o.shape else { unreachable!() };
        assert!((r.w - 40.0).abs() < 1e-3 && (r.h - 20.0).abs() < 1e-3 && r.rot == -30.0);
        let h = handles(&o);
        assert!(close(h[0], a) && close(h[2], b), "{h:?}");
        assert!(drawn(Kind::Rect, a, a, 30.0, Style::default()).is_none() && drawn(Kind::Arrow, a, a, 0.0, Style::default()).is_none());
        let t = drawn(Kind::Text, a, a, 30.0, Style { width: 24.0, ..Style::default() }).unwrap();
        assert_eq!(t.shape, Shape::Text { at: a, text: String::new(), size: 24.0, rot: -30.0 });
    }
}
