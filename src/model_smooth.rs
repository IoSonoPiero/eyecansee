//! Normals for the models that do not give them (scans, STL, most CAD exports): at each corner
//! the average of the faces around that point that bend less than about 45° from this one, so
//! curved surfaces look smooth and edges stay sharp.

use std::collections::HashMap;

use rayon::prelude::*;

/// Faces meeting at a sharper angle keep their edge (cosine of ~45°).
const CREASE: f32 = 0.70;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 { a.map(|v| v / l) } else { [0.0; 3] }
}

/// The corners' normals of each triangle; zero where a triangle has no area (the renderers then
/// use the face's own).
pub(crate) fn normals(pos: &[V3], tris: &[[u32; 3]]) -> Vec<[V3; 3]> {
    // The same point written more than once (STL repeats it for every triangle) is one point.
    let mut ids: HashMap<[u32; 3], u32> = HashMap::with_capacity(pos.len());
    let weld: Vec<u32> = pos
        .iter()
        .map(|p| {
            let n = ids.len() as u32;
            *ids.entry(p.map(|v| (v + 0.0).to_bits())).or_insert(n)
        })
        .collect();
    // Not made unit: larger faces weigh more.
    let faces: Vec<V3> = tris
        .par_iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| pos[v as usize]);
            let (u, v) = (sub(b, a), sub(c, a));
            [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        })
        .collect();
    let units: Vec<V3> = faces.par_iter().map(|&f| unit(f)).collect();
    // The faces around each point, one list after the other.
    let mut start = vec![0usize; ids.len() + 1];
    for t in tris {
        for &v in t {
            start[weld[v as usize] as usize + 1] += 1;
        }
    }
    for i in 1..start.len() {
        start[i] += start[i - 1];
    }
    let mut next = start.clone();
    let mut around = vec![0u32; tris.len() * 3];
    for (f, t) in tris.iter().enumerate() {
        for &v in t {
            let w = weld[v as usize] as usize;
            around[next[w]] = f as u32;
            next[w] += 1;
        }
    }
    tris.par_iter()
        .enumerate()
        .map(|(f, t)| {
            t.map(|v| {
                let w = weld[v as usize] as usize;
                let mut n = [0.0; 3];
                for &g in &around[start[w]..start[w + 1]] {
                    if dot(units[g as usize], units[f]) >= CREASE {
                        n = [0, 1, 2].map(|k| n[k] + faces[g as usize][k]);
                    }
                }
                unit(n)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_across_a_gentle_fold_sharp_across_a_right_angle() {
        // Two triangles folded by 20° along the edge 0-1, each with its own copy of the points.
        let (s, c) = 20f32.to_radians().sin_cos();
        let pos = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [0.0, -c, s], [1.0, 0.0, 0.0]];
        let n = normals(&pos, &[[0, 1, 2], [3, 4, 5]]);
        // On the shared edge both corners lean the same way, half way.
        assert!((n[0][0][1] - n[1][0][1]).abs() < 1e-5 && n[0][0][1].abs() > 0.1);
        // At a right angle each face keeps its own normal.
        let pos = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]];
        let n = normals(&pos, &[[0, 1, 2], [3, 4, 5]]);
        assert_eq!(n[0][0], [0.0, 0.0, 1.0]);
        assert_eq!(n[1][0], [0.0, 1.0, 0.0]);
    }
}
