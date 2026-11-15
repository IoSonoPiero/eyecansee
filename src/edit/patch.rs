//! A piece of image pasted in the editor (`Shape::Patch`): turned by any angle (its handle, or the
//! angle in the properties bar), by quarter turns, mirrored, sized, brought to the front or sent
//! to the back. Only its box and whether it is mirrored change: the pixels stay those pasted, so
//! however many changes, the piece is drawn once from them and loses nothing (user, 2026-11-14).
//! Angles as seen on the screen: the box's own angle plus the image's free angle.

use super::doc::{Area, Obj, Pt};
use super::geom::turned;

/// The steps of the turn with Shift held, in degrees.
pub const SNAP: f32 = 15.0;

/// `deg` in -180 (excluded) to 180.
fn normal(deg: f32) -> f32 {
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

/// A quarter turn of the box, to the left (`left`) or to the right.
pub fn quarter(a: &Area, left: bool) -> Area {
    Area { rot: normal(a.rot + if left { -90.0 } else { 90.0 }), ..*a }
}

/// The box and whether its pixels are mirrored after a flip on the screen, left to right
/// (`horizontal`) or top to bottom. A mirror on the screen of a box turned by φ is the box turned
/// by -φ (180° - φ top to bottom) with its pixels mirrored in its own axes.
pub fn flipped(a: &Area, mirrored: bool, horizontal: bool, angle: f32) -> (Area, bool) {
    let deg = shown(a.rot, angle);
    let deg = if horizontal { -deg } else { 180.0 - deg };
    (Area { rot: rot_for(deg, angle), ..*a }, !mirrored)
}

/// The box `w` × `h` pixels around the same centre (at least a pixel); with `keep` the side not
/// given follows the proportions the box has: `w` given when `width`, else `h`.
pub fn sized(a: &Area, side: f32, width: bool, keep: bool) -> Area {
    let side = side.max(1.0);
    let ratio = if a.w > 0.0 && a.h > 0.0 { a.h / a.w } else { 1.0 };
    let (w, h) = match (width, keep) {
        (true, true) => (side, (side * ratio).max(1.0)),
        (true, false) => (side, a.h),
        (false, true) => ((side / ratio).max(1.0), side),
        (false, false) => (a.w, side),
    };
    Area { w, h, ..*a }
}

/// Object `i` drawn over all the others: its new place.
pub fn to_front(objects: &mut Vec<Obj>, i: usize) -> usize {
    let o = objects.remove(i);
    objects.push(o);
    objects.len() - 1
}

/// Object `i` drawn under all the others: its new place.
pub fn to_back(objects: &mut Vec<Obj>, i: usize) -> usize {
    let o = objects.remove(i);
    objects.insert(0, o);
    0
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

    /// Quarter turns, four of them back where they were; angles kept in -180 to 180.
    #[test]
    fn quarter_turns() {
        let mut a = area(30.0);
        a = quarter(&a, false);
        assert!(near(a.rot, 120.0));
        a = quarter(&quarter(&a, false), false);
        assert!(near(a.rot, -60.0));
        a = quarter(&a, false);
        assert!(near(a.rot, 30.0));
        assert!(near(quarter(&area(-150.0), true).rot, 120.0));
        assert_eq!((a.w, a.h, a.centre), (40.0, 20.0, Pt::new(10.0, 20.0)));
    }

    /// A mirror on the screen: the angle the other way, the pixels mirrored; twice, as it was.
    /// Top to bottom is left to right and half a turn.
    #[test]
    fn mirrors() {
        let a = area(30.0);
        let (h, m) = flipped(&a, false, true, 0.0);
        assert!(near(h.rot, -30.0) && m);
        let (v, m) = flipped(&a, false, false, 0.0);
        assert!(near(v.rot, 150.0) && m);
        let (back, m) = flipped(&h, true, true, 0.0);
        assert!(near(back.rot, 30.0) && !m);
        // On an image turned by 10°: 40° on the screen becomes -40°, -50° of its own.
        let (t, _) = flipped(&a, false, true, 10.0);
        assert!(near(t.rot, -50.0) && near(shown(t.rot, 10.0), -40.0));
        assert_eq!((h.w, h.h, h.centre), (a.w, a.h, a.centre));
    }

    /// Width and height typed: with the proportions kept the other side follows, without they
    /// change alone; never under a pixel; the centre stays.
    #[test]
    fn sizes() {
        let a = area(45.0);
        let b = sized(&a, 100.0, true, true);
        assert!(near(b.w, 100.0) && near(b.h, 50.0) && b.centre == a.centre && b.rot == 45.0);
        let c = sized(&a, 10.0, false, true);
        assert!(near(c.w, 20.0) && near(c.h, 10.0));
        let d = sized(&a, 70.0, true, false);
        assert!(near(d.w, 70.0) && near(d.h, 20.0));
        let e = sized(&a, 0.0, false, false);
        assert!(near(e.h, 1.0) && near(e.w, 40.0));
        // Twice with the proportions kept and back: the same box, the ratio never drifts.
        let f = sized(&sized(&sized(&a, 333.0, true, true), 7.0, false, true), 40.0, true, true);
        assert!(near(f.w, 40.0) && near(f.h, 20.0), "{f:?}");
    }

    /// Front and back: the order of the others is kept.
    #[test]
    fn order() {
        use crate::edit::doc::{Shape, Style};
        let o = |x: f32| Obj { shape: Shape::Rect(Area { centre: Pt::new(x, 0.0), w: 1.0, h: 1.0, rot: 0.0 }), style: Style::default() };
        let xs = |v: &[Obj]| v.iter().map(|o| o.shape.area().unwrap().centre.x).collect::<Vec<_>>();
        let mut v = vec![o(0.0), o(1.0), o(2.0), o(3.0)];
        assert_eq!(to_front(&mut v, 1), 3);
        assert_eq!(xs(&v), [0.0, 2.0, 3.0, 1.0]);
        assert_eq!(to_back(&mut v, 2), 0);
        assert_eq!(xs(&v), [3.0, 0.0, 2.0, 1.0]);
    }
}
