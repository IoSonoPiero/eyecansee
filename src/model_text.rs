//! Texts of 3D models (VRML and X3D `Text`): the letters' outlines, laid out by usvg with the
//! system's fonts, flattened and filled with triangles (earcutr), flat in the XY plane with the
//! baseline of the first line on y = 0, as VRML places them.

use resvg::tiny_skia::{PathSegment, Point, Transform};
use resvg::usvg;

/// Size the text is laid out at, then scaled to the model's size: enough for smooth curves.
const LAYOUT: f32 = 100.0;
/// Points along each curve of a letter.
const CURVE_STEPS: usize = 6;

/// The triangles of `lines` (positions, triangles), `size` high, lines `spacing` × `size` apart,
/// aligned by `justify` (BEGIN, MIDDLE, END).
pub(crate) fn mesh(lines: &[String], size: f32, spacing: f32, justify: &str) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let anchor = match justify {
        "MIDDLE" => "middle",
        "END" => "end",
        _ => "start",
    };
    let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">"#);
    for (i, line) in lines.iter().enumerate() {
        let text = line.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        svg.push_str(&format!(r#"<text x="0" y="{}" font-size="{LAYOUT}" font-family="sans-serif" text-anchor="{anchor}">{text}</text>"#, i as f32 * spacing * LAYOUT));
    }
    svg.push_str("</svg>");
    let Ok(tree) = usvg::Tree::from_str(&svg, &crate::decode::usvg_options()) else { return Default::default() };
    let mut rings: Vec<Vec<[f32; 2]>> = Vec::new();
    outlines(tree.root(), &mut rings);
    // SVG has y down: turned up, and brought to `size`.
    let k = size / LAYOUT;
    let rings: Vec<Vec<[f32; 2]>> = rings.into_iter().map(|r| r.into_iter().map(|[x, y]| [x * k, -y * k]).collect()).collect();
    fill(&rings)
}

fn outlines(g: &usvg::Group, rings: &mut Vec<Vec<[f32; 2]>>) {
    for node in g.children() {
        match node {
            usvg::Node::Group(g) => outlines(g, rings),
            usvg::Node::Text(t) => outlines(t.flattened(), rings),
            usvg::Node::Path(p) => path_rings(p.data(), p.abs_transform(), rings),
            usvg::Node::Image(_) => {}
        }
    }
}

/// The closed outlines of a path, its curves as short lines.
fn path_rings(path: &resvg::tiny_skia::Path, t: Transform, rings: &mut Vec<Vec<[f32; 2]>>) {
    let mut ring: Vec<[f32; 2]> = Vec::new();
    let mut at = Point::zero();
    let push = |ring: &mut Vec<[f32; 2]>, mut p: Point| {
        t.map_point(&mut p);
        ring.push([p.x, p.y]);
    };
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                if ring.len() >= 3 {
                    rings.push(std::mem::take(&mut ring));
                }
                ring.clear();
                push(&mut ring, p);
                at = p;
            }
            PathSegment::LineTo(p) => {
                push(&mut ring, p);
                at = p;
            }
            PathSegment::QuadTo(c, p) => {
                for i in 1..=CURVE_STEPS {
                    let s = i as f32 / CURVE_STEPS as f32;
                    let q = |a: f32, b: f32, e: f32| (1.0 - s) * (1.0 - s) * a + 2.0 * (1.0 - s) * s * b + s * s * e;
                    push(&mut ring, Point::from_xy(q(at.x, c.x, p.x), q(at.y, c.y, p.y)));
                }
                at = p;
            }
            PathSegment::CubicTo(c1, c2, p) => {
                for i in 1..=CURVE_STEPS {
                    let s = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - s;
                    let q = |a: f32, b: f32, c: f32, e: f32| u * u * u * a + 3.0 * u * u * s * b + 3.0 * u * s * s * c + s * s * s * e;
                    push(&mut ring, Point::from_xy(q(at.x, c1.x, c2.x, p.x), q(at.y, c1.y, c2.y, p.y)));
                }
                at = p;
            }
            PathSegment::Close => {
                if ring.len() >= 3 {
                    rings.push(std::mem::take(&mut ring));
                }
            }
        }
    }
    if ring.len() >= 3 {
        rings.push(ring);
    }
}

fn area(r: &[[f32; 2]]) -> f32 {
    (0..r.len()).map(|i| r[i][0] * r[(i + 1) % r.len()][1] - r[(i + 1) % r.len()][0] * r[i][1]).sum::<f32>() / 2.0
}

fn inside(p: [f32; 2], r: &[[f32; 2]]) -> bool {
    let mut odd = false;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            odd = !odd;
        }
    }
    odd
}

/// Outlines inside an even number of others are letters, the others their holes (the inside of
/// an "o"): each hole goes with the smallest letter around it, then each letter is triangulated.
fn fill(rings: &[Vec<[f32; 2]>]) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let around = |i: usize| (0..rings.len()).filter(|&j| j != i && inside(rings[i][0], &rings[j])).collect::<Vec<_>>();
    let depth: Vec<Vec<usize>> = (0..rings.len()).map(around).collect();
    let (mut pos, mut tris) = (Vec::new(), Vec::new());
    for outer in (0..rings.len()).filter(|&i| depth[i].len().is_multiple_of(2)) {
        let holes: Vec<usize> = (0..rings.len())
            .filter(|&h| depth[h].len() % 2 == 1 && depth[h].iter().filter(|&&o| depth[o].len().is_multiple_of(2)).min_by(|&&a, &&b| area(&rings[a]).abs().total_cmp(&area(&rings[b]).abs())) == Some(&outer))
            .collect();
        let mut flat: Vec<f32> = rings[outer].iter().flatten().copied().collect();
        let mut starts = Vec::new();
        for &h in &holes {
            starts.push(flat.len() / 2);
            flat.extend(rings[h].iter().flatten());
        }
        let Ok(idx) = earcutr::earcut(&flat, &starts, 2) else { continue };
        let base = pos.len() as u32;
        pos.extend(flat.as_chunks::<2>().0.iter().map(|p| [p[0], p[1], 0.0]));
        tris.extend(idx.as_chunks::<3>().0.iter().map(|t| t.map(|v| base + v as u32)));
    }
    (pos, tris)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_with_their_holes() {
        // A square with a square hole: 8 triangles, none inside the hole.
        let outer = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let hole = vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]];
        let (pos, tris) = fill(&[outer, hole]);
        assert_eq!(tris.len(), 8);
        let covered: f32 = tris.iter().map(|t| area(&t.map(|v| [pos[v as usize][0], pos[v as usize][1]])).abs()).sum();
        assert!((covered - 12.0).abs() < 1e-4);
    }

    #[test]
    #[ignore]
    fn text_live() {
        // With the system's fonts: "Hello" one unit high, on the baseline.
        let (pos, tris) = mesh(&["Hello".into()], 1.0, 1.0, "BEGIN");
        println!("{} triangles", tris.len());
        assert!(tris.len() > 50 && pos.iter().all(|p| p[1] > -0.4 && p[1] < 1.2));
    }
}
