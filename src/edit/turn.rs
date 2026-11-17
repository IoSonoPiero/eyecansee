//! The angle of an object on the screen and its turn handle, without the interface: moved from
//! `patch.rs` (the pasted piece, the first object with a handle) on 2026-11-15; every object with
//! a box, and a text, turns the same way since (issue #137). Angles as seen on the screen: the
//! box's own angle plus the image's free angle (Rotate and crop).

use super::doc::{Area, Pt, Shape};
use super::geom::{bend_handle, text_area, turned};

/// The steps of the turn with Shift held, in degrees.
pub const SNAP: f32 = 15.0;

/// `deg` in -180 (excluded) to 180.
pub(crate) fn normal(deg: f32) -> f32 {
    let d = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if d == -180.0 { 180.0 } else { d }
}

/// The angle of a box on the screen, on an image turned by `angle`.
pub fn shown(rot: f32, angle: f32) -> f32 {
    normal(rot + angle)
}

/// The box's own angle that shows it at `deg` on the screen.
pub fn rot_for(deg: f32, angle: f32) -> f32 {
    normal(deg - angle)
}

/// The handle that turns the box: `reach` pixels above the middle of its top side, on its own axes.
pub fn turn_handle(a: &Area, reach: f32) -> Pt {
    let d = turned(Pt::new(0.0, -(a.h / 2.0 + reach)), a.rot);
    Pt::new(a.centre.x + d.x, a.centre.y + d.y)
}

/// The box's own angle with its turn handle dragged to `p`: the handle points at `p`. With `snap`
/// (Shift held) the angle on the screen goes by 15°.
pub fn turned_to(a: &Area, p: Pt, angle: f32, snap: bool) -> f32 {
    let (dx, dy) = (p.x - a.centre.x, p.y - a.centre.y);
    if dx == 0.0 && dy == 0.0 {
        return a.rot;
    }
    // The handle is straight up (-90°) when the box is not turned.
    let mut deg = shown(dy.atan2(dx).to_degrees() + 90.0, angle);
    if snap {
        deg = normal((deg / SNAP).round() * SNAP);
    }
    rot_for(deg, angle)
}

/// The box an object turns about, its handle above it: its own box, the box of a text's letters,
/// or for a text on a curve a box along the line from one end to the other, round the middle of
/// the curve. `None` for what does not turn: lines and arrows (their ends set their direction),
/// strokes by hand, steps (turned, the number would be crooked) and the filters that do not hide
/// (issue #137).
pub fn turn_box(shape: &Shape) -> Option<Area> {
    match shape {
        Shape::Line(..) | Shape::Arrow(..) | Shape::Pen(_) | Shape::Step { .. } => None,
        Shape::Filter { filter, .. } if !filter.hides() => None,
        Shape::Text { size, path: Some((a, b, c)), .. } => {
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            Some(Area { centre: bend_handle(*a, *b, *c), w: dx.hypot(dy), h: 1.2 * size, rot: dy.atan2(dx).to_degrees() })
        }
        Shape::Text { at, text, size, rot, .. } => Some(text_area(*at, text, *size, *rot)),
        shape => shape.area().copied(),
    }
}

/// The object turned round the middle of its `turn_box` until the box's own angle is `rot`: a
/// bubble's tail and a text's curve turn with it, and a text's own angle follows its curve's, so
/// taken off the curve it lies as the curve did.
pub fn set_rot(shape: &mut Shape, rot: f32) {
    let Some(b) = turn_box(shape) else { return };
    let d = rot - b.rot;
    let about = |p: &mut Pt| {
        let q = turned(Pt::new(p.x - b.centre.x, p.y - b.centre.y), d);
        *p = Pt::new(b.centre.x + q.x, b.centre.y + q.y);
    };
    match shape {
        Shape::Text { rot: own, path, .. } => {
            *own = normal(*own + d);
            if let Some((a, b, c)) = path {
                about(a);
                about(b);
                if let Some(c) = c {
                    about(c);
                }
            }
        }
        Shape::Bubble { area, tail, .. } => {
            area.rot = normal(rot);
            about(tail);
        }
        shape => {
            if let Some(a) = shape.area_mut() {
                a.rot = normal(rot);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rot: f32) -> Area {
        Area { centre: Pt::new(10.0, 20.0), w: 40.0, h: 20.0, rot }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// The handle above the box points where it is dragged; with Shift the angle on the screen
    /// goes by 15°, also on an image turned by a free angle.
    #[test]
    fn turned_by_the_handle() {
        let a = area(0.0);
        let h = turn_handle(&a, 5.0);
        assert!(near(h.x, 10.0) && near(h.y, 5.0), "{h:?}");
        // To the right of the centre: a quarter turn clockwise.
        assert!(near(turned_to(&a, Pt::new(50.0, 20.0), 0.0, false), 90.0));
        assert!(near(turned_to(&a, Pt::new(10.0, 60.0), 0.0, false), 180.0));
        // 20° from up: 20 alone, 15 with Shift; 23° with Shift: 30, the nearer step.
        let at = |deg: f32| {
            let d = turned(Pt::new(0.0, -30.0), deg);
            Pt::new(10.0 + d.x, 20.0 + d.y)
        };
        assert!(near(turned_to(&a, at(20.0), 0.0, false), 20.0));
        assert!(near(turned_to(&a, at(20.0), 0.0, true), 15.0));
        assert!(near(turned_to(&a, at(23.0), 0.0, true), 30.0));
        assert!(near(turned_to(&a, at(-52.0), 0.0, true), -45.0));
        // The image turned by 10°: the pointer 10° from up on the image is 20° on the screen,
        // 15° with Shift, 5° of the box's own.
        assert!(near(turned_to(&a, at(10.0), 10.0, true), 5.0));
        assert!(near(shown(5.0, 10.0), 15.0));
        // The handle dragged to its turned place gives back the angle.
        for rot in [-170.0, -90.0, 0.0, 33.0, 135.0] {
            let b = area(rot);
            assert!(near(turned_to(&b, turn_handle(&b, 12.0), 0.0, false), rot), "{rot}");
        }
    }

    /// Which objects turn: those with a box and texts; not lines, arrows, strokes by hand, steps
    /// or the filters that do not hide (issue #137).
    #[test]
    fn what_turns() {
        use crate::edit::doc::{Preview, Shape};
        use crate::edit::filter::Filter;
        let a = area(0.0);
        let filter = |filter| Shape::Filter { area: a, filter, amount: 4.0, preview: Preview::default() };
        for shape in [Shape::Rect(a), Shape::Ellipse(a), Shape::Cover(a), Shape::Spotlight(a), Shape::Stamp(a), Shape::Highlight { area: a, preview: Preview::default() }, Shape::Lens { area: a, preview: Preview::default() }, filter(Filter::Blur), filter(Filter::Pixelate)] {
            assert_eq!(turn_box(&shape), Some(a), "{shape:?}");
        }
        let p = Pt::new(0.0, 0.0);
        for shape in [Shape::Line(p, Pt::new(9.0, 0.0), None), Shape::Arrow(p, Pt::new(9.0, 0.0), None), Shape::Pen(vec![p]), Shape::Step { area: a, n: 1 }, filter(Filter::Gray), filter(Filter::Sharpen)] {
            assert_eq!(turn_box(&shape), None, "{shape:?}");
        }
    }

    /// A box turned by its handle or its angle: its centre and sides stay; a bubble's tail turns
    /// with it round the centre; a text keeps its place and takes the angle.
    #[test]
    fn turned_with_what_it_has() {
        use crate::edit::doc::Shape;
        let mut rect = Shape::Rect(area(10.0));
        set_rot(&mut rect, 100.0);
        assert_eq!(rect, Shape::Rect(area(100.0)));
        set_rot(&mut rect, 190.0);
        assert!(near(rect.area().unwrap().rot, -170.0));
        // The tail 30 below the centre (10, 20): a quarter turn clockwise puts it 30 to the left.
        let mut bubble = Shape::Bubble { area: area(0.0), tail: Pt::new(10.0, 50.0), text: String::new(), font: None };
        set_rot(&mut bubble, 90.0);
        let Shape::Bubble { area: b, tail, .. } = &bubble else { unreachable!() };
        assert!(near(b.rot, 90.0) && near(tail.x, -20.0) && near(tail.y, 20.0), "{tail:?}");
        let mut text = Shape::Text { at: Pt::new(3.0, 4.0), text: "Hi".into(), size: 10.0, rot: 5.0, font: None, path: None };
        let h = turn_handle(&turn_box(&text).unwrap(), 8.0);
        assert!(near(turned_to(&turn_box(&text).unwrap(), h, 0.0, false), 5.0));
        set_rot(&mut text, -45.0);
        let Shape::Text { at, rot, .. } = text else { unreachable!() };
        assert!(near(rot, -45.0) && at == Pt::new(3.0, 4.0));
    }

    /// A text on a curve turns with its line, round the middle of the curve: the ends and the bend
    /// turn, the box follows the line and the text's own angle turns as much.
    #[test]
    fn text_on_a_curve() {
        use crate::edit::doc::Shape;
        let path = (Pt::new(-10.0, 0.0), Pt::new(10.0, 0.0), Some(Pt::new(0.0, -8.0)));
        let mut text = Shape::Text { at: Pt::new(0.0, 0.0), text: "Hi".into(), size: 10.0, rot: 0.0, font: None, path: Some(path) };
        let b = turn_box(&text).unwrap();
        // The middle of the curve is 4 above the line, its box 20 long along it.
        assert!(near(b.centre.x, 0.0) && near(b.centre.y, -4.0) && near(b.w, 20.0) && near(b.rot, 0.0), "{b:?}");
        set_rot(&mut text, 90.0);
        let Shape::Text { rot, path: Some((a, e, Some(c))), .. } = &text else { unreachable!() };
        // Turned clockwise round (0, -4): the left end goes up, the bend to the right.
        assert!(near(a.x, -4.0) && near(a.y, -14.0) && near(e.x, -4.0) && near(e.y, 6.0) && near(c.x, 4.0) && near(c.y, -4.0), "{a:?} {e:?} {c:?}");
        assert!(near(*rot, 90.0) && near(turn_box(&text).unwrap().rot, 90.0));
        assert!(near(turn_box(&text).unwrap().centre.y, -4.0));
    }
}
