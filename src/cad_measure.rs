//! Measuring a CAD drawing (#141): the point a click snaps to (an end, the middle of a line, the
//! centre of a circle or arc, where two lines cross), and what is measured from the points
//! clicked: a distance, an area with its perimeter, an angle. In the drawing's units, Y up.

use std::sync::{Arc, Weak};

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

#[cfg(test)]
thread_local! {
    /// Segments looked at by `snap` on this thread (audit 6.15 c).
    static EXAMINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The point to snap to within `radius` of `at`, the closest; ends and centres before middles,
/// middles before crossings, as CAD programs order them when they are equally close.
pub fn snap(d: &Drawing, at: P, radius: f64) -> (P, Snap) {
    snap_among(d, at, radius, None)
}

/// `snap` looking only at the segments of `runs` (`Grid::near`), or at all.
fn snap_among(d: &Drawing, at: P, radius: f64, runs: Option<&[(u32, u32)]>) -> (P, Snap) {
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
    let mut look = |seg: &[P]| {
        let (a, b) = (seg[0], seg[1]);
        #[cfg(test)]
        EXAMINED.with(|n| n.set(n.get() + 1));
        if a[0].is_nan() || b[0].is_nan() || !box_near([a, b], at, radius) {
            return;
        }
        offer(a, Snap::End, 0);
        offer(b, Snap::End, 0);
        offer([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], Snap::Middle, 1);
        if close.len() < CROSS_MAX {
            close.push([a, b]);
        }
    };
    match runs {
        None => d.shown_items().for_each(|item| {
            if let Item::Line { pts, .. } = item {
                pts.windows(2).for_each(&mut look);
            }
        }),
        Some(runs) => runs.iter().for_each(|&(i, k)| {
            if let Some(Item::Line { pts, .. }) = d.items.get(i as usize) {
                run(pts, k as usize).windows(2).for_each(&mut look);
            }
        }),
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

/// `snap` with the drawing's `Grid`, made at its first point and kept while that drawing is the
/// one drawn (known by its place, as `model_measure::Memo`).
#[derive(Default)]
pub(crate) struct Snapper {
    grid: Option<(Weak<Drawing>, Grid)>,
}

impl Snapper {
    pub(crate) fn snap(&mut self, d: &Arc<Drawing>, at: P, radius: f64) -> (P, Snap) {
        if !self.grid.as_ref().is_some_and(|(of, _)| std::ptr::eq(of.as_ptr(), Arc::as_ptr(d))) {
            self.grid = Some((Arc::downgrade(d), Grid::new(d)));
        }
        match self.grid.as_ref().and_then(|(_, g)| g.near([at[0] - radius, at[1] - radius, at[0] + radius, at[1] + radius])) {
            Some(runs) => snap_among(d, at, radius, Some(&runs)),
            None => snap(d, at, radius),
        }
    }
}

/// Segments of a line placed in the grid together (`Grid`).
const RUN: usize = 64;
/// At most this many cells along each side of the grid.
const CELLS: usize = 1024;
/// A run of segments over more cells than this is looked at for every point instead.
const WIDE: usize = 64;

/// The lines shown, in runs of `RUN` segments (item, run), by the cells of a grid over the
/// drawing (audit 6.15 c): made once for each drawing drawn, where `snap` looked at every
/// segment at every frame with the pointer over the drawing.
struct Grid {
    lo: P,
    cell: [f64; 2],
    side: usize,
    /// Where each cell's runs start in `runs`, cell after cell, and where the last ends.
    starts: Vec<u32>,
    runs: Vec<(u32, u32)>,
    wide: Vec<(u32, u32)>,
}

/// The box around the points, the gaps of filled areas left out.
fn bounds(pts: &[P]) -> Option<[f64; 4]> {
    let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in pts.iter().filter(|p| !p[0].is_nan()) {
        b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
    }
    (b[0] <= b[2] && b[1] <= b[3]).then_some(b)
}

/// The runs of the lines shown (item, run) and their boxes.
fn boxes(d: &Drawing) -> impl Iterator<Item = ((u32, u32), [f64; 4])> + '_ {
    d.shown_indexed().flat_map(|(i, item)| {
        let pts: &[P] = match item {
            Item::Line { pts, .. } => pts,
            _ => &[],
        };
        (0..pts.len().saturating_sub(1).div_ceil(RUN)).filter_map(move |k| bounds(run(pts, k)).map(|b| ((i as u32, k as u32), b)))
    })
}

/// The points of the run `k` of a line: its segments and the point after the last.
fn run(pts: &[P], k: usize) -> &[P] {
    &pts[(k * RUN).min(pts.len())..((k + 1) * RUN + 1).min(pts.len())]
}

impl Grid {
    /// Made in three passes over the lines, without keeping their boxes: a drawing at its
    /// largest has 10 million runs.
    fn new(d: &Drawing) -> Grid {
        let (mut all, mut count) = ([f64::MAX, f64::MAX, f64::MIN, f64::MIN], 0);
        for (_, b) in boxes(d) {
            all = [all[0].min(b[0]), all[1].min(b[1]), all[2].max(b[2]), all[3].max(b[3])];
            count += 1;
        }
        let side = ((count as f64).sqrt().ceil() as usize).clamp(1, CELLS);
        let (lo, cell) = if count == 0 { ([0.0; 2], [1.0; 2]) } else { ([all[0], all[1]], [0, 1].map(|k| ((all[k + 2] - all[k]) / side as f64).max(f64::MIN_POSITIVE))) };
        let mut g = Grid { lo, cell, side, starts: vec![0; side * side + 1], runs: Vec::new(), wide: Vec::new() };
        // Counted first, then placed.
        let narrow = |r: [usize; 4]| (r[1] - r[0] + 1) * (r[3] - r[2] + 1) <= WIDE;
        for (id, b) in boxes(d) {
            let r = g.range(b);
            if narrow(r) {
                Grid::each(side, r, |c| g.starts[c + 1] += 1);
            } else {
                g.wide.push(id);
            }
        }
        for c in 0..side * side {
            g.starts[c + 1] += g.starts[c];
        }
        let mut next: Vec<u32> = g.starts[..side * side].to_vec();
        let mut runs = vec![(0, 0); g.starts[side * side] as usize];
        for (id, b) in boxes(d) {
            let r = g.range(b);
            if narrow(r) {
                Grid::each(side, r, |c| {
                    runs[next[c] as usize] = id;
                    next[c] += 1;
                });
            }
        }
        g.runs = runs;
        g
    }

    /// The cells under the box `b` (x from, x to, y from, y to), within the grid.
    fn range(&self, b: [f64; 4]) -> [usize; 4] {
        let at = |v: f64, k: usize| (((v - self.lo[k]) / self.cell[k]).floor().max(0.0) as usize).min(self.side - 1);
        [at(b[0], 0), at(b[2], 0), at(b[1], 1), at(b[3], 1)]
    }

    fn each(side: usize, r: [usize; 4], mut f: impl FnMut(usize)) {
        for y in r[2]..=r[3] {
            for x in r[0]..=r[1] {
                f(y * side + x);
            }
        }
    }

    /// The runs whose cells the box `b` reaches, in the drawing's order; `None` when it covers
    /// a quarter of the grid or more, where looking at every segment costs no more.
    fn near(&self, b: [f64; 4]) -> Option<Vec<(u32, u32)>> {
        let r = self.range(b);
        if (r[1] - r[0] + 1) * (r[3] - r[2] + 1) * 4 > self.side * self.side && self.side > 1 {
            return None;
        }
        let mut out = self.wide.clone();
        Grid::each(self.side, r, |c| out.extend_from_slice(&self.runs[self.starts[c] as usize..self.starts[c + 1] as usize]));
        out.sort_unstable();
        out.dedup();
        Some(out)
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

    fn line(pts: Vec<P>) -> Item {
        Item::Line { pts, color: [200; 3], fill: None, stroke: crate::cad::Stroke::default() }
    }

    /// A grid of `n` × `n` short lines, 10 units apart, and a long one across it.
    fn lines(n: usize) -> Drawing {
        let mut items: Vec<Item> = (0..n * n).map(|i| line(vec![[(i % n) as f64 * 10.0, (i / n) as f64 * 10.0], [(i % n) as f64 * 10.0 + 5.0, (i / n) as f64 * 10.0 + 3.0]])).collect();
        items.push(line((0..=n * 10).map(|k| [k as f64, 501.0 + (k % 7) as f64 * 0.3]).collect()));
        Drawing { items, ..Default::default() }
    }

    /// Audit 6.15 (c): the point the pointer snaps to was looked for among all the segments of
    /// the drawing at every frame with the pointer over it: 100 frames on 10 000 lines looked at
    /// a million segments.
    #[test]
    fn audit_6_15_c_snap_looks_near() {
        let d = std::sync::Arc::new(lines(100));
        let mut snapper = Snapper::default();
        EXAMINED.with(|n| n.set(0));
        for k in 0..100 {
            snapper.snap(&d, [k as f64 * 9.7, 503.0], 2.0);
        }
        let examined = EXAMINED.with(|n| n.get());
        println!("{examined} segments looked at for 100 frames");
        assert!(examined < 100 * 1000, "segments looked at for 100 frames: {examined}");
    }

    /// Audit 6.15 (c): the grid is made at the first point, on the window's thread: its time and
    /// memory for 2 million lines of one segment (each its own run, the most there can be), and
    /// the segments looked at for a point then. `--release`.
    #[test]
    #[ignore]
    fn audit_6_15_c_grid_time() {
        let d = std::sync::Arc::new(lines(1415));
        let t = std::time::Instant::now();
        let (grid, peak) = crate::test_alloc::peak(|| Grid::new(&d));
        println!("{} lines: grid in {:.0?}, {} MB at most while made, {} MB kept", d.items.len(), t.elapsed(), peak >> 20, (grid.runs.len() * 8 + grid.starts.len() * 4) >> 20);
        let mut snapper = Snapper { grid: Some((std::sync::Arc::downgrade(&d), grid)) };
        EXAMINED.with(|n| n.set(0));
        let t = std::time::Instant::now();
        snapper.snap(&d, [7000.0, 7000.0], 2.0);
        println!("one point: {} segments looked at, {:.2?}", EXAMINED.with(|n| n.get()), t.elapsed());
        let t = std::time::Instant::now();
        snap(&d, [7000.0, 7000.0], 2.0);
        println!("one point among all the segments: {:.2?}", t.elapsed());
    }

    /// Audit 6.15 (c): with the grid, the point found is the one found among all the segments:
    /// ends, middles and crossings, long lines and areas with gaps, near and far, with a small and
    /// a large radius; and a drawing drawn anew (other layers) gets its own grid.
    #[test]
    fn audit_6_15_c_snap_as_before() {
        let mut d = lines(40);
        d.items.push(line(vec![[0.0, 0.0], [400.0, 400.0]]));
        d.items.push(line(vec![[0.0, 400.0], [400.0, 0.0], [f64::NAN, f64::NAN], [100.0, 100.0], [300.0, 120.0], [200.0, 330.0], [100.0, 100.0]]));
        d.items.push(line((0..2000).map(|k| [200.0 + 150.0 * (k as f64 / 300.0).cos(), 200.0 + 150.0 * (k as f64 / 300.0).sin()]).collect()));
        d.centres.push(([200.0, 200.0], 0));
        let mut snapper = Snapper::default();
        let mut seed: u64 = 7;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for round in 0..2 {
            let d = std::sync::Arc::new(Drawing { items: d.items.clone(), centres: d.centres.clone(), ..Default::default() });
            for i in 0..3000 {
                let at = [next() * 460.0 - 30.0, next() * 460.0 - 30.0];
                let radius = [0.5, 2.0, 8.0, 50.0, 500.0][i % 5];
                assert_eq!(snapper.snap(&d, at, radius), snap(&d, at, radius), "round {round}, point {i} at {at:?}, radius {radius}");
            }
        }
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
