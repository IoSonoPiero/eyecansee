//! Geometry of CAD drawings seen from above: 2D affine transforms (blocks, entity coordinate
//! systems), curves turned into points (arcs, polyline bulges, ellipses, splines) and the lines
//! of hatch patterns cut by their boundaries.

use std::f64::consts::TAU;

pub type P = [f64; 2];

/// Points for a whole circle: smooth at the size of a 4096-pixel picture.
const CIRCLE_POINTS: f64 = 128.0;

/// `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn apply(&self, p: P) -> P {
        let [a, b, c, d, e, f] = self.0;
        [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
    }

    /// `self` after `inner`: a point goes through `inner` first.
    pub fn then_inner(&self, inner: &Affine) -> Affine {
        let [a, b, c, d, e, f] = self.0;
        let [ia, ib, ic, id, ie, if_] = inner.0;
        Affine([a * ia + c * ib, b * ia + d * ib, a * ic + c * id, b * ic + d * id, a * ie + c * if_ + e, b * ie + d * if_ + f])
    }

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn rotate(angle: f64) -> Affine {
        let (s, c) = angle.sin_cos();
        Affine([c, s, -s, c, 0.0, 0.0])
    }

    pub fn scale(x: f64, y: f64) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// How much lengths grow (for text heights): the square root of the area factor.
    pub fn length_scale(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }

    /// Angle of the X axis after the transform.
    pub fn angle(&self) -> f64 {
        self.0[1].atan2(self.0[0])
    }

    /// From an entity's own coordinate system (its extrusion `normal`, "arbitrary axis
    /// algorithm" of the DXF reference) to the drawing seen from above. Z is dropped.
    pub fn ocs(normal: [f64; 3]) -> Affine {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len.is_nan() || len <= 0.0 || (normal[0].abs() < 1e-12 && normal[1].abs() < 1e-12 && normal[2] > 0.0) {
            return Affine::IDENTITY;
        }
        let n = normal.map(|v| v / len);
        let cross = |u: [f64; 3], v: [f64; 3]| [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 { cross([0.0, 1.0, 0.0], n) } else { cross([0.0, 0.0, 1.0], n) };
        let l = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
        let ax = ax.map(|v| v / l);
        let ay = cross(n, ax);
        Affine([ax[0], ax[1], ay[0], ay[1], 0.0, 0.0])
    }
}

/// Points of an arc around `c`, counterclockwise from `a0` to `a1` (radians).
pub fn arc(c: P, r: f64, a0: f64, a1: f64) -> Vec<P> {
    let mut sweep = (a1 - a0) % TAU;
    if sweep <= 0.0 {
        sweep += TAU;
    }
    let n = ((sweep / TAU * CIRCLE_POINTS).ceil() as usize).clamp(2, CIRCLE_POINTS as usize);
    (0..=n).map(|i| a0 + sweep * i as f64 / n as f64).map(|a| [c[0] + r * a.cos(), c[1] + r * a.sin()]).collect()
}

pub fn circle(c: P, r: f64) -> Vec<P> {
    let n = CIRCLE_POINTS as usize;
    (0..=n).map(|i| TAU * i as f64 / n as f64).map(|a| [c[0] + r * a.cos(), c[1] + r * a.sin()]).collect()
}

/// A polyline with bulges (the tangent of a quarter of each segment's arc, negative = clockwise):
/// its points, the arcs turned into points.
pub fn bulged(vertices: &[(P, f64)], closed: bool) -> Vec<P> {
    let mut out = Vec::with_capacity(vertices.len());
    let n = vertices.len();
    let segments = if closed { n } else { n.saturating_sub(1) };
    if let Some(&(first, _)) = vertices.first() {
        out.push(first);
    }
    for i in 0..segments {
        let (p, bulge) = vertices[i];
        let (q, _) = vertices[(i + 1) % n];
        if bulge.abs() < 1e-9 || !bulge.is_finite() {
            out.push(q);
            continue;
        }
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let chord = (dx * dx + dy * dy).sqrt();
        if chord == 0.0 {
            continue;
        }
        let angle = 4.0 * bulge.atan();
        let r = chord / (2.0 * (angle / 2.0).sin());
        // The centre is on the perpendicular through the chord's middle.
        let h = r * (angle / 2.0).cos();
        let m = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
        let c = [m[0] - h * dy / chord, m[1] + h * dx / chord];
        let a0 = (p[1] - c[1]).atan2(p[0] - c[0]);
        let n = ((angle.abs() / TAU * CIRCLE_POINTS).ceil() as usize).max(2);
        let r = r.abs();
        out.extend((1..=n).map(|k| a0 + angle * k as f64 / n as f64).map(|a| [c[0] + r * a.cos(), c[1] + r * a.sin()]));
    }
    out
}

/// Ellipse around `c` with half major axis `major` and the minor `ratio` of it, from parameter `t0`
/// to `t1` (radians, counterclockwise; equal or a whole turn = closed).
pub fn ellipse(c: P, major: P, ratio: f64, t0: f64, t1: f64) -> Vec<P> {
    let minor = [-major[1] * ratio, major[0] * ratio];
    let mut sweep = (t1 - t0) % TAU;
    if sweep <= 1e-9 {
        sweep += TAU;
    }
    let n = ((sweep / TAU * CIRCLE_POINTS).ceil() as usize).max(2);
    (0..=n)
        .map(|i| t0 + sweep * i as f64 / n as f64)
        .map(|t| [c[0] + major[0] * t.cos() + minor[0] * t.sin(), c[1] + major[1] * t.cos() + minor[1] * t.sin()])
        .collect()
}

/// A B-spline (NURBS when `weights` are given) of `degree`, sampled along its knots. `None` when
/// the knots do not match the control points.
pub fn spline(degree: usize, knots: &[f64], control: &[P], weights: &[f64]) -> Option<Vec<P>> {
    let n = control.len();
    if degree == 0 || n <= degree || knots.len() != n + degree + 1 || knots.windows(2).any(|w| w[1].is_nan() || w[1] < w[0]) {
        return None;
    }
    let w = |i: usize| if weights.len() == n { weights[i] } else { 1.0 };
    let (t0, t1) = (knots[degree], knots[n]);
    if t1.is_nan() || t1 <= t0 {
        return None;
    }
    let samples = (n * 8).clamp(16, 4096);
    let mut out = Vec::with_capacity(samples + 1);
    for s in 0..=samples {
        let t = t0 + (t1 - t0) * s as f64 / samples as f64;
        // The span holding t, then de Boor on homogeneous points.
        let k = (degree..n).rev().find(|&k| knots[k] <= t).unwrap_or(degree);
        let mut d: Vec<[f64; 3]> = (0..=degree).map(|j| control[j + k - degree]).enumerate().map(|(j, p)| {
            let wt = w(j + k - degree);
            [p[0] * wt, p[1] * wt, wt]
        }).collect();
        for r in 1..=degree {
            for j in (r..=degree).rev() {
                let i = j + k - degree;
                let den = knots[i + degree + 1 - r] - knots[i];
                let a = if den == 0.0 { 0.0 } else { (t - knots[i]) / den };
                d[j] = [0, 1, 2].map(|c| (1.0 - a) * d[j - 1][c] + a * d[j][c]);
            }
        }
        let [x, y, wt] = d[degree];
        if wt != 0.0 {
            out.push([x / wt, y / wt]);
        }
    }
    Some(out)
}

/// One family of a hatch pattern: lines at `angle` through `base`, repeated by `offset`, with
/// dashes (positive), gaps (negative) and dots (zero) along them; empty = continuous.
pub struct PatternLine<'a> {
    pub angle: f64,
    pub base: P,
    pub offset: P,
    pub dashes: &'a [f64],
}

/// The pieces of a pattern family inside the boundaries (even-odd, as hatches fill islands), at
/// most `budget` of them: `None` when there would be more (the hatch is too dense to draw).
pub fn hatch_lines(boundaries: &[Vec<P>], l: &PatternLine, budget: usize) -> Option<Vec<[P; 2]>> {
    let (d, n) = ([l.angle.cos(), l.angle.sin()], [-l.angle.sin(), l.angle.cos()]);
    let dot = |a: P, b: P| a[0] * b[0] + a[1] * b[1];
    let spacing = dot(l.offset, n);
    let points = boundaries.iter().flatten();
    let (lo, hi) = points.fold((f64::MAX, f64::MIN), |(lo, hi), &p| {
        let s = dot([p[0] - l.base[0], p[1] - l.base[1]], n);
        (lo.min(s), hi.max(s))
    });
    if spacing.abs() < 1e-12 || lo > hi || !spacing.is_finite() {
        return None;
    }
    let (k0, k1) = ((lo / spacing).min(hi / spacing).ceil(), (lo / spacing).max(hi / spacing).floor());
    if k1 - k0 > budget as f64 {
        return None;
    }
    let period: f64 = l.dashes.iter().map(|v| v.abs()).sum();
    let mut out = Vec::new();
    for k in k0 as i64..=k1 as i64 {
        let o = [l.base[0] + k as f64 * l.offset[0], l.base[1] + k as f64 * l.offset[1]];
        let at = |t: f64| [o[0] + t * d[0], o[1] + t * d[1]];
        let mut ts: Vec<f64> = Vec::new();
        for b in boundaries {
            for w in b.windows(2) {
                let (sp, sq) = (dot([w[0][0] - o[0], w[0][1] - o[1]], n), dot([w[1][0] - o[0], w[1][1] - o[1]], n));
                if (sp <= 0.0) != (sq <= 0.0) {
                    let f = sp / (sp - sq);
                    ts.push(dot([w[0][0] + (w[1][0] - w[0][0]) * f - o[0], w[0][1] + (w[1][1] - w[0][1]) * f - o[1]], d));
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        for pair in ts.as_chunks::<2>().0 {
            let (t0, t1) = (pair[0], pair[1]);
            if period < 1e-12 {
                out.push([at(t0), at(t1)]);
            } else {
                // Dashes start from the base point of the line: the same pattern on every line.
                let mut s = (t0 / period).floor() * period;
                while s < t1 {
                    for &len in l.dashes {
                        let (a, b) = (s.max(t0), (s + len.max(0.0)).min(t1));
                        if len >= 0.0 && a <= b {
                            out.push([at(a), at(b)]);
                        }
                        s += len.abs();
                    }
                    if out.len() > budget {
                        return None;
                    }
                }
            }
        }
        if out.len() > budget {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P, b: P) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn transforms_and_the_extrusion_pointing_down() {
        let t = Affine::translate(10.0, 0.0).then_inner(&Affine::rotate(std::f64::consts::FRAC_PI_2)).then_inner(&Affine::scale(2.0, 2.0));
        assert!(close(t.apply([1.0, 0.0]), [10.0, 2.0]));
        assert!((t.length_scale() - 2.0).abs() < 1e-12);
        // Normal (0, 0, −1): X mirrored, Y unchanged (the usual mirrored arcs of DWG files).
        let m = Affine::ocs([0.0, 0.0, -1.0]);
        assert!(close(m.apply([3.0, 4.0]), [-3.0, 4.0]));
        assert_eq!(Affine::ocs([0.0, 0.0, 1.0]), Affine::IDENTITY);
    }

    #[test]
    fn bulges_arcs_and_ellipses() {
        // Bulge 1 = half circle of radius 1 from (0,0) to (2,0), counterclockwise: below the chord.
        let pts = bulged(&[([0.0, 0.0], 1.0), ([2.0, 0.0], 0.0)], false);
        assert!(close(pts[0], [0.0, 0.0]) && close(*pts.last().unwrap(), [2.0, 0.0]));
        assert!(pts.iter().all(|p| ((p[0] - 1.0).hypot(p[1]) - 1.0).abs() < 1e-9));
        assert!(pts.iter().any(|p| p[1] < -0.99));
        let a = arc([0.0, 0.0], 2.0, 0.0, std::f64::consts::FRAC_PI_2);
        assert!(close(a[0], [2.0, 0.0]) && close(*a.last().unwrap(), [0.0, 2.0]));
        // Arc across 0°: from 350° to 10°, 20° long.
        let a = arc([0.0, 0.0], 1.0, 350f64.to_radians(), 10f64.to_radians());
        assert!(a.iter().all(|p| p[0] > 0.98));
        let e = ellipse([0.0, 0.0], [2.0, 0.0], 0.5, 0.0, 0.0);
        assert!(e.iter().all(|p| ((p[0] / 2.0).powi(2) + (p[1] / 1.0).powi(2) - 1.0).abs() < 1e-9) && e.iter().any(|p| p[1] > 0.99));
    }

    #[test]
    fn splines_through_their_ends_and_bad_knots_refused() {
        // Clamped quadratic: starts and ends on the end control points.
        let c = [[0.0, 0.0], [1.0, 2.0], [2.0, 0.0]];
        let s = spline(2, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], &c, &[]).unwrap();
        assert!(close(s[0], c[0]) && close(*s.last().unwrap(), c[2]));
        // Middle of the curve: (1, 1).
        assert!(s.iter().any(|p| close(*p, [1.0, 1.0])));
        assert!(spline(2, &[0.0, 1.0], &c, &[]).is_none());
        assert!(spline(2, &[0.0, 0.0, 0.0, f64::NAN, 1.0, 1.0], &c, &[]).is_none());
    }

    #[test]
    fn hatch_lines_cut_by_the_boundary_and_its_island() {
        let square = |s: f64| vec![[-s, -s], [s, -s], [s, s], [-s, s], [-s, -s]];
        let horizontal = PatternLine { angle: 0.0, base: [0.0, 0.0], offset: [0.0, 1.0], dashes: &[] };
        let lines = hatch_lines(&[square(10.5)], &horizontal, 1000).unwrap();
        assert_eq!(lines.len(), 21);
        assert!(lines.iter().all(|l| (l[0][0] + 10.5).abs() < 1e-9 && (l[1][0] - 10.5).abs() < 1e-9));
        // An island: the line through the middle is cut in two.
        let lines = hatch_lines(&[square(10.5), square(5.5)], &horizontal, 1000).unwrap();
        assert_eq!(lines.iter().filter(|l| l[0][1].abs() < 1e-9).count(), 2);
        // Dashes of 2 and gaps of 1 from the base point, on [-10, 10].
        let dashed = PatternLine { dashes: &[2.0, -1.0], ..horizontal };
        let lines = hatch_lines(&[square(10.5)], &dashed, 1000).unwrap();
        // The first dash of each line is the end of the one from -12 to -10, cut at -10.5.
        assert!(close(lines[0][0], [-10.5, -10.0]) && close(lines[0][1], [-10.0, -10.0]));
        assert!(close(lines[1][0], [-9.0, -10.0]) && close(lines[1][1], [-7.0, -10.0]));
        assert!(hatch_lines(&[square(10.5)], &horizontal, 5).is_none());
    }
}
