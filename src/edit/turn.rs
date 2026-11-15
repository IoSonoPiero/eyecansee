//! The angle of an object on the screen and its turn handle, without the interface: moved from
//! `patch.rs` (the pasted piece, the first object with a handle) on 2026-11-15. Angles as seen on
//! the screen: the box's own angle plus the image's free angle (Rotate and crop).

use super::doc::{Area, Pt};
use super::geom::turned;

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
}
