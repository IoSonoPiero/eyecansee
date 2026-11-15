//! A piece of image pasted in the editor (`Shape::Patch`): turned by any angle (its handle, or the
//! angle in the properties bar), by quarter turns, mirrored, sized, brought to the front or sent
//! to the back. Only its box and whether it is mirrored change: the pixels stay those pasted, so
//! however many changes, the piece is drawn once from them and loses nothing (user, 2026-11-14).
//! Angles as seen on the screen (`edit::turn`): the box's own angle plus the image's free angle.

use super::doc::{Area, Obj};
use super::turn::{normal, rot_for, shown};

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
    use crate::edit::doc::Pt;

    fn area(rot: f32) -> Area {
        Area { centre: Pt::new(10.0, 20.0), w: 40.0, h: 20.0, rot }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
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
