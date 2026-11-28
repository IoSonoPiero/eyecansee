//! Measuring a CAD drawing (#141): the point a click snaps to (an end, the middle of a line, the
//! centre of a circle or arc, where two lines cross), and what is measured from the points
//! clicked: a distance, an area with its perimeter, an angle. In the drawing's units, Y up.

use crate::cad::{Drawing, Item};
use crate::cad_geom::P;

/// What a point snapped to, for the mark drawn on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    End,
    Middle,
    Centre,
    Cross,
    /// Nothing near: the point under the pointer.
    Free,
}

/// Lines met near the pointer whose crossings are tried; beyond, too dense to choose among.
const CROSS_MAX: usize = 400;

/// The point to snap to within `radius` of `at`, the closest; ends and centres before middles,
/// middles before crossings, as CAD programs order them when they are equally close.
pub fn snap(d: &Drawing, at: P, radius: f64) -> (P, Snap) {
    let near = |p: P| (p[0] - at[0]).hypot(p[1] - at[1]) <= radius;
    let mut best: Option<(f64, u8, P, Snap)> = None;
    let mut offer = |p: P, kind: Snap, rank: u8| {
        if !near(p) {
            return;
        }
        let dist = (p[0] - at[0]).hypot(p[1] - at[1]);
        // A small step per rank: an end a hair further than a middle still wins.
        let score = dist + f64::from(rank) * radius * 0.15;
        if best.is_none_or(|b| score < b.0) {
            best = Some((score, rank, p, kind));
        }
    };
    let mut close: Vec<[P; 2]> = Vec::new();
    for c in d.shown_centres() {
        offer(c, Snap::Centre, 0);
    }
    for item in d.shown_items() {
        let Item::Line { pts, .. } = item else { continue };
        for seg in pts.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            if a[0].is_nan() || b[0].is_nan() {
                continue;
            }
            if !box_near([a, b], at, radius) {
                continue;
            }
            offer(a, Snap::End, 0);
            offer(b, Snap::End, 0);
            offer([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], Snap::Middle, 1);
            if close.len() < CROSS_MAX {
                close.push([a, b]);
            }
        }
    }
    for (i, s) in close.iter().enumerate() {
        for t in &close[i + 1..] {
            if let Some(p) = crossing(*s, *t) {
                offer(p, Snap::Cross, 2);
            }
        }
    }
    best.map_or((at, Snap::Free), |b| (b.2, b.3))
}

/// `snap` for the drawing drawn, kept from frame to frame.
#[derive(Default)]
pub(crate) struct Snapper {}

impl Snapper {
    pub(crate) fn snap(&mut self, d: &std::sync::Arc<Drawing>, at: P, radius: f64) -> (P, Snap) {
        snap(d, at, radius)
    }
}

/// Whether the segment's box comes within `r` of `p`.
fn box_near([a, b]: [P; 2], p: P, r: f64) -> bool {
    p[0] >= a[0].min(b[0]) - r && p[0] <= a[0].max(b[0]) + r && p[1] >= a[1].min(b[1]) - r && p[1] <= a[1].max(b[1]) + r
}

/// Where two segments cross, if they do (not at their shared ends, which are ends already).
fn crossing([a, b]: [P; 2], [c, e]: [P; 2]) -> Option<P> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [e[0] - c[0], e[1] - c[1]];
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let q = [c[0] - a[0], c[1] - a[1]];
    let t = (q[0] * s[1] - q[1] * s[0]) / den;
    let u = (q[0] * r[1] - q[1] * r[0]) / den;
    let inside = |v: f64| (1e-9..=1.0 - 1e-9).contains(&v);
    (inside(t) && inside(u)).then(|| [a[0] + t * r[0], a[1] + t * r[1]])
}

pub fn distance(a: P, b: P) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// Area and perimeter of the closed shape through `pts` (the last joined to the first).
pub fn area(pts: &[P]) -> (f64, f64) {
    let n = pts.len();
    if n < 3 {
        return (0.0, 0.0);
    }
    let mut twice = 0.0;
    let mut perimeter = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        twice += a[0] * b[1] - b[0] * a[1];
        perimeter += distance(a, b);
    }
    (twice.abs() / 2.0, perimeter)
}

/// The angle at `vertex` between the lines to `a` and to `b`, in degrees, 0 to 180.
pub fn angle(a: P, vertex: P, b: P) -> f64 {
    let u = [a[0] - vertex[0], a[1] - vertex[1]];
    let v = [b[0] - vertex[0], b[1] - vertex[1]];
    let cross = u[0] * v[1] - u[1] * v[0];
    let dot = u[0] * v[0] + u[1] * v[1];
    cross.abs().atan2(dot).to_degrees()
}

/// What is measured from the points clicked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Between two points.
    #[default]
    Distance,
    /// Of the shape through the points.
    Area,
    /// At the second point, between the lines to the first and the third.
    Angle,
}

impl Mode {
    /// The points it takes; the area takes as many as clicked.
    pub fn points(self) -> usize {
        match self {
            Mode::Distance => 2,
            Mode::Area => usize::MAX,
            Mode::Angle => 3,
        }
    }
}

/// A measure done.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measured {
    Distance { length: f64, dx: f64, dy: f64 },
    Area { area: f64, perimeter: f64 },
    Angle(f64),
}

/// The measure of `pts` in `mode`, once there are enough of them (three for an area).
pub fn measured(mode: Mode, pts: &[P]) -> Option<Measured> {
    match (mode, pts) {
        (Mode::Distance, [a, b, ..]) => Some(Measured::Distance { length: distance(*a, *b), dx: (b[0] - a[0]).abs(), dy: (b[1] - a[1]).abs() }),
        (Mode::Area, pts) if pts.len() >= 3 => {
            let (area, perimeter) = area(pts);
            Some(Measured::Area { area, perimeter })
        }
        (Mode::Angle, [a, v, b, ..]) => Some(Measured::Angle(angle(*a, *v, *b))),
        _ => None,
    }
}

/// A measure as written on screen: four significant decimals at most, fewer for large values,
/// trailing zeros dropped, `decimal` as the language writes it.
pub fn number(v: f64, decimal: &str) -> String {
    let places = match v.abs() {
        a if a >= 10_000.0 => 1,
        a if a >= 100.0 => 2,
        a if a >= 1.0 => 3,
        _ => 4,
    };
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    let s = if s == "-0" { "0".to_string() } else { s };
    s.replace('.', decimal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square of 100 with a diagonal, and a circle of radius 10 at (50, 50).
    const DXF: &str = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n4\n70\n1\n10\n0\n20\n0\n10\n100\n20\n0\n10\n100\n20\n100\n10\n0\n20\n100\n0\nLINE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n100\n21\n100\n31\n0\n0\nLINE\n8\n0\n10\n0\n20\n100\n30\n0\n11\n100\n21\n0\n31\n0\n0\nCIRCLE\n8\n0\n10\n50\n20\n50\n30\n0\n40\n10\n0\nENDSEC\n0\nEOF\n";

    fn close(a: P, b: P) -> bool {
        distance(a, b) < 1e-6
    }

    #[test]
    fn points_snap_to_ends_middles_centres_and_crossings() {
        let d = crate::cad::read(DXF.as_bytes(), "m.dxf").unwrap();
        let (p, k) = snap(&d, [101.0, 98.5], 3.0);
        assert!(close(p, [100.0, 100.0]) && k == Snap::End, "{p:?} {k:?}");
        let (p, k) = snap(&d, [49.0, 1.0], 3.0);
        assert!(close(p, [50.0, 0.0]) && k == Snap::Middle, "{p:?} {k:?}");
        // The centre of the circle is also where the diagonals cross: the centre wins.
        let (p, k) = snap(&d, [51.0, 49.0], 3.0);
        assert!(close(p, [50.0, 50.0]) && k == Snap::Centre, "{p:?} {k:?}");
        // On a diagonal, far from its ends and from the circle: the point itself.
        let (p, k) = snap(&d, [30.0, 70.0], 2.0);
        assert!(close(p, [30.0, 70.0]) && k == Snap::Free, "{p:?} {k:?}");
    }

    #[test]
    fn crossings_found() {
        assert!(close(crossing([[0.0, 0.0], [10.0, 10.0]], [[0.0, 10.0], [10.0, 0.0]]).unwrap(), [5.0, 5.0]));
        assert_eq!(crossing([[0.0, 0.0], [10.0, 0.0]], [[0.0, 1.0], [10.0, 1.0]]), None);
        // Touching at an end is an end, not a crossing.
        assert_eq!(crossing([[0.0, 0.0], [10.0, 0.0]], [[10.0, 0.0], [10.0, 5.0]]), None);
    }

    #[test]
    fn measures_once_there_are_points_enough() {
        let pts = [[0.0, 0.0], [3.0, 4.0], [3.0, 0.0]];
        assert_eq!(measured(Mode::Distance, &pts[..1]), None);
        assert_eq!(measured(Mode::Distance, &pts[..2]), Some(Measured::Distance { length: 5.0, dx: 3.0, dy: 4.0 }));
        assert_eq!(measured(Mode::Area, &pts[..2]), None);
        assert_eq!(measured(Mode::Area, &pts), Some(Measured::Area { area: 6.0, perimeter: 12.0 }));
        let Some(Measured::Angle(a)) = measured(Mode::Angle, &[[3.0, 4.0], [0.0, 0.0], [3.0, 0.0]]) else { panic!() };
        assert!((a - 53.130102).abs() < 1e-5);
    }

    #[test]
    fn numbers_written() {
        assert_eq!(number(1234.5678, ","), "1234,57");
        assert_eq!(number(12.0, "."), "12");
        assert_eq!(number(0.123456, "."), "0.1235");
        assert_eq!(number(54321.04, "."), "54321");
        assert_eq!(number(-0.00001, "."), "0");
    }

    #[test]
    fn distance_area_and_angle() {
        assert_eq!(distance([0.0, 0.0], [3.0, 4.0]), 5.0);
        assert_eq!(area(&[[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]]), (12.0, 14.0));
        // Clicked the other way round: the same.
        assert_eq!(area(&[[0.0, 3.0], [4.0, 3.0], [4.0, 0.0], [0.0, 0.0]]), (12.0, 14.0));
        assert_eq!(area(&[[0.0, 0.0], [1.0, 1.0]]), (0.0, 0.0));
        assert!((angle([10.0, 0.0], [0.0, 0.0], [0.0, 5.0]) - 90.0).abs() < 1e-9);
        assert!((angle([10.0, 0.0], [0.0, 0.0], [-1.0, 1.0]) - 135.0).abs() < 1e-9);
    }
}
