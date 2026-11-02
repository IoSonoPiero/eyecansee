//! Lines and points of a 3D model (VRML and X3D line and point sets) made into thin solids, so
//! that the triangle viewer can show them: a square tube along each segment, a small octahedron
//! on each point, as thick as a fraction of the whole model.

use crate::model::Builder;

/// Thickness of lines and points against the diagonal of the whole model.
const THICKNESS: f32 = 1.0 / 400.0;

pub(crate) type Segment = ([f32; 3], [f32; 3], Option<[u8; 3]>);
pub(crate) type Point = ([f32; 3], Option<[u8; 3]>);

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f32; 3]) -> Option<[f32; 3]> {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    (l > 1e-12 && l.is_finite()).then(|| a.map(|v| v / l))
}

/// Adds the lines and points to `b`, sized on everything in it and in them.
pub(crate) fn add(b: &mut Builder, lines: &[Segment], points: &[Point]) -> Result<(), String> {
    let ends = lines.iter().flat_map(|l| [l.0, l.1]).chain(points.iter().map(|p| p.0));
    let (mut lo, mut hi) = b.bounds().unwrap_or(([f32::MAX; 3], [f32::MIN; 3]));
    for p in ends.filter(|p| p.iter().all(|c| c.is_finite())) {
        for k in 0..3 {
            (lo[k], hi[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
        }
    }
    let diag = (0..3).map(|k| (hi[k] - lo[k]).max(0.0).powi(2)).sum::<f32>().sqrt();
    let r = (diag * THICKNESS).max(f32::MIN_POSITIVE);
    for &(a, c, color) in lines {
        let Some(d) = unit(sub(c, a)) else { continue };
        let other = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let (Some(u), Some(v)) = (unit(cross(d, other)), unit(cross(d, cross(d, other)))) else { continue };
        let corner = |p: [f32; 3], k: usize| {
            let (s, t) = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)][k];
            std::array::from_fn(|i| p[i] + r * (s * u[i] + t * v[i]))
        };
        let pos: Vec<[f32; 3]> = (0..4).map(|k| corner(a, k)).chain((0..4).map(|k| corner(c, k))).collect();
        let tris = (0..4u32).flat_map(|k| {
            let n = (k + 1) % 4;
            [[k, n, n + 4], [k, n + 4, k + 4]]
        });
        b.add(pos, tris, |_| color)?;
    }
    for &(p, color) in points {
        let pos: Vec<[f32; 3]> = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]].iter().map(|o: &[f32; 3]| std::array::from_fn(|i| p[i] + o[i] * r * 1.5)).collect();
        let tris = [[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]];
        b.add(pos, tris, |_| color)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_square_of_lines_and_a_point() {
        let mut b = crate::model::Builder::default();
        let sq = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let lines: Vec<super::Segment> = (0..4).map(|i| (sq[i], sq[(i + 1) % 4], Some([255, 0, 0]))).collect();
        super::add(&mut b, &lines, &[([0.5, 0.5, 0.0], None)]).unwrap();
        let m = b.finish(false).unwrap();
        assert_eq!(m.tris.len(), 4 * 8 + 8);
        assert_eq!(m.colors[0], [255, 0, 0]);
    }
}
