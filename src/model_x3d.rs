//! VRML 97 and X3D (`.wrl`, `.x3d`, `.x3dv`, also gzipped): the scene read by `model_vrml`,
//! walked through transforms, groups, switches, levels of detail and inlined files next to it;
//! the shapes' face sets, triangle sets, elevation grids and primitives (box, sphere, cylinder,
//! cone) in the colour of their material or of their vertices and faces, or with their picture
//! (ImageTexture, on face and triangle sets with texture coordinates).

use std::rc::Rc;

use crate::model::{Builder, IDENTITY, Mat4, Mesh, Sibling, apply, mul, rotation};
use crate::model_vrml::SceneNode;

/// Nodes visited, so that a file using its groups again and again cannot take forever.
const MAX_VISITS: usize = 10_000_000;
/// Inlined files inside inlined files.
const MAX_INLINE: u32 = 8;
/// Segments around a sphere, a cylinder or a cone, and rings of a sphere.
const SEGMENTS: usize = 32;
const RINGS: usize = 16;

pub fn read(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
    let scene = crate::model_vrml::parse(bytes)?;
    let mut b = Builder::default();
    let mut w = Walk { b: &mut b, sibling, visits: 0, lines: Vec::new(), points: Vec::new() };
    w.node(&scene, &IDENTITY, 0)?;
    let (lines, points) = (std::mem::take(&mut w.lines), std::mem::take(&mut w.points));
    crate::model_lines::add(&mut b, &lines, &points)?;
    b.finish(false)
}

struct Walk<'a, 's> {
    b: &'a mut Builder,
    sibling: Sibling<'s>,
    visits: usize,
    /// Line and point sets, made solid at the end (`model_lines`), when the model's size is known.
    lines: Vec<crate::model_lines::Segment>,
    points: Vec<crate::model_lines::Point>,
}

impl Walk<'_, '_> {
    fn node(&mut self, n: &SceneNode, m: &Mat4, inline: u32) -> Result<(), String> {
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return Err("too many nodes in the scene".into());
        }
        match n.kind.as_str() {
            "Transform" => {
                let m = mul(m, &transform(n));
                n.field_nodes("children").iter().try_for_each(|c| self.node(c, &m, inline))
            }
            "Shape" => self.shape(n, m),
            "Switch" => {
                let choices = if n.field("choice").is_some() { n.field_nodes("choice") } else { n.field_nodes("children") };
                let i = n.num("whichChoice", -1.0);
                match choices.get(i as usize).filter(|_| i >= 0.0) {
                    Some(c) => self.node(c, m, inline),
                    None => Ok(()),
                }
            }
            // The most detailed level.
            "LOD" => {
                let levels = if n.field("level").is_some() { n.field_nodes("level") } else { n.field_nodes("children") };
                levels.first().map_or(Ok(()), |c| self.node(c, m, inline))
            }
            "Inline" if inline < MAX_INLINE => self.inline(n, m, inline),
            "ProtoDeclare" | "ExternProtoDeclare" | "Script" | "Inline" => Ok(()),
            _ => n.children().try_for_each(|c| self.node(c, m, inline)),
        }
    }

    fn inline(&mut self, n: &SceneNode, m: &Mat4, inline: u32) -> Result<(), String> {
        let Some(bytes) = n.strs("url").iter().filter(|u| !u.contains("://")).find_map(|u| (self.sibling)(u)) else { return Ok(()) };
        let scene = crate::model_vrml::parse(&bytes)?;
        self.node(&scene, m, inline + 1)
    }

    fn shape(&mut self, n: &SceneNode, m: &Mat4) -> Result<(), String> {
        let appearance = n.children().find(|c| c.kind == "Appearance");
        let material = appearance.and_then(|a| a.children().find(|c| c.kind.ends_with("Material")).cloned());
        let color = material.map(|mat| {
            let c = [mat.nums("diffuseColor"), mat.nums("baseColor"), mat.nums("emissiveColor")].into_iter().find(|c| c.len() >= 3).unwrap_or(&[0.8, 0.8, 0.8]);
            rgb(c)
        });
        if let Some(g) = n.children().find(|g| matches!(g.kind.as_str(), "IndexedLineSet" | "LineSet" | "PointSet")) {
            self.line_set(g, m, color);
            return Ok(());
        }
        let Some(part) = n.children().find_map(geometry) else { return Ok(()) };
        let image = appearance.and_then(|a| a.children().find(|c| c.kind == "ImageTexture"));
        let urls = image.map_or(&[][..], |i| i.strs("url"));
        let tex = if part.uv.is_empty() { 0 } else { urls.iter().filter(|u| !u.contains("://")).map(|u| self.b.texture_file(u, self.sibling)).find(|&k| k > 0).unwrap_or(0) };
        let place = |i: usize| part.uv.get(i).copied().flatten().filter(|_| tex > 0).map(|c| (c, tex));
        // An RGB picture takes the place of the material's colour (VRML 97, 4.14).
        let colors = |i: usize| part.colors.get(i).copied().flatten().or(if place(i).is_some() { Some([255; 3]) } else { color });
        let pos = part.pos.iter().map(|&p| apply(m, p)).collect();
        self.b.add_textured(pos, part.tris.clone(), colors, place)
    }
}

impl Walk<'_, '_> {
    /// IndexedLineSet (polylines between the −1s), LineSet (`vertexCount` points each) and
    /// PointSet, with their colours per vertex or per polyline.
    fn line_set(&mut self, g: &SceneNode, m: &Mat4, color: Option<[u8; 3]>) {
        let pos = points(g);
        let polylines: Vec<Vec<(i64, usize)>> = match g.kind.as_str() {
            "IndexedLineSet" => runs(&g.nums("coordIndex").iter().map(|&v| v as i64).collect::<Vec<_>>()),
            "LineSet" => {
                let mut at = 0;
                g.nums("vertexCount").iter().map(|&n| {
                    let run = (at..at + n.max(0.0) as usize).map(|i| (i as i64, i)).collect();
                    at += n.max(0.0) as usize;
                    run
                }).collect()
            }
            _ => {
                let tris: Vec<Tri> = (0..pos.len()).map(|i| Tri { v: [i as u32; 3], at: [i; 3], face: i }).collect();
                let colors = colors(g, &tris);
                self.points.extend(pos.iter().enumerate().map(|(i, &p)| (apply(m, p), colors.get(i).copied().flatten().or(color))));
                return;
            }
        };
        for (f, run) in polylines.iter().enumerate() {
            let tris: Vec<Tri> = run.windows(2).map(|w| Tri { v: [w[0].0 as u32, w[1].0 as u32, w[1].0 as u32], at: [w[0].1, w[1].1, w[1].1], face: f }).collect();
            let colors = colors(g, &tris);
            for (t, c) in tris.iter().zip(colors.into_iter().chain(std::iter::repeat(None))) {
                let (Some(&a), Some(&b)) = (pos.get(t.v[0] as usize), pos.get(t.v[1] as usize)) else { continue };
                self.lines.push((apply(m, a), apply(m, b), c.or(color)));
            }
        }
    }
}

fn rgb(c: &[f32]) -> [u8; 3] {
    [c[0], c[1], c[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
}

/// translation × center × rotation × scale × −center (scaleOrientation left out).
fn transform(n: &SceneNode) -> Mat4 {
    let v3 = |name: &str, d: [f32; 3]| n.nums(name).get(..3).map_or(d, |v| [v[0], v[1], v[2]]);
    let shift = |t: [f32; 3]| {
        let mut m = IDENTITY;
        m[3] = [t[0], t[1], t[2], 1.0];
        m
    };
    let (t, c, s) = (v3("translation", [0.0; 3]), v3("center", [0.0; 3]), v3("scale", [1.0; 3]));
    let r = n.nums("rotation");
    let r = if r.len() >= 4 { rotation([r[0], r[1], r[2]], r[3]) } else { IDENTITY };
    let mut scale = IDENTITY;
    (0..3).for_each(|k| scale[k][k] = s[k]);
    [shift(t), shift(c), r, scale, shift(c.map(|v| -v))].iter().fold(IDENTITY, |acc, x| mul(&acc, x))
}

#[derive(Default)]
struct Part {
    pos: Vec<[f32; 3]>,
    tris: Vec<[u32; 3]>,
    colors: Vec<Option<[u8; 3]>>,
    /// Each triangle's corners on the picture, when the geometry gives them.
    uv: Vec<Option<[[f32; 2]; 3]>>,
}

/// A triangle: its vertices, where its corners are in the index list, and its face.
struct Tri {
    v: [u32; 3],
    at: [usize; 3],
    face: usize,
}

fn geometry(g: &Rc<SceneNode>) -> Option<Part> {
    let n = g.as_ref();
    let idx = |name: &str| n.nums(name).iter().map(|&v| v as i64).collect::<Vec<i64>>();
    let tris: Vec<Tri> = match n.kind.as_str() {
        "IndexedFaceSet" => runs(&idx("coordIndex")).into_iter().enumerate().flat_map(|(f, r)| fan(&r, f)).collect(),
        "IndexedTriangleFanSet" => runs(&idx("index")).into_iter().enumerate().flat_map(|(f, r)| fan(&r, f)).collect(),
        "IndexedTriangleStripSet" => runs(&idx("index")).into_iter().flat_map(|r| strip(&r)).collect(),
        "IndexedTriangleSet" => triples(&idx("index")),
        "TriangleSet" => triples(&(0..points(n).len() as i64).collect::<Vec<_>>()),
        "ElevationGrid" => return Some(elevation(n)),
        "Box" | "Sphere" | "Cylinder" | "Cone" => return Some(primitive(n)),
        "Extrusion" => return Some(extrusion(n)),
        "Text" => return Some(text(n)),
        _ => return None,
    };
    let pos = points(n);
    let colors = colors(n, &tris);
    let uv = texture_coords(n, &tris);
    Some(Part { pos, tris: tris.iter().map(|t| t.v).collect(), colors, uv })
}

fn points(n: &SceneNode) -> Vec<[f32; 3]> {
    n.children().find(|c| c.kind.starts_with("Coordinate")).map(|c| c.nums("point").as_chunks::<3>().0.to_vec()).unwrap_or_default()
}

/// The colour of each triangle: of its face, or the average of its corners.
fn colors(n: &SceneNode, tris: &[Tri]) -> Vec<Option<[u8; 3]>> {
    let Some(c) = n.children().find(|c| c.kind == "Color" || c.kind == "ColorRGBA") else { return Vec::new() };
    let stride = if c.kind == "ColorRGBA" { 4 } else { 3 };
    let list = c.nums("color");
    let get = |i: i64| list.get(i as usize * stride..i as usize * stride + 3).filter(|_| i >= 0);
    let index: Vec<i64> = n.nums("colorIndex").iter().map(|&v| v as i64).collect();
    let per_vertex = n.bool("colorPerVertex", true);
    tris.iter()
        .map(|t| {
            if !per_vertex {
                return get(index.get(t.face).copied().unwrap_or(t.face as i64)).map(rgb);
            }
            // In a face set the colours follow the coordIndex list (or their own colorIndex).
            let corner = |k: usize| if index.is_empty() { Some(t.v[k] as i64) } else { index.get(t.at[k]).copied() };
            let cs: Vec<&[f32]> = (0..3).filter_map(|k| corner(k).and_then(get)).collect();
            (cs.len() == 3).then(|| rgb(&[0, 1, 2].map(|k| (cs[0][k] + cs[1][k] + cs[2][k]) / 3.0)))
        })
        .collect()
}

/// Where each triangle's corners lie on the picture: TextureCoordinate's points by texCoordIndex,
/// or indexed as the coordinates are.
fn texture_coords(n: &SceneNode, tris: &[Tri]) -> Vec<Option<[[f32; 2]; 3]>> {
    let Some(tc) = n.children().find(|c| c.kind == "TextureCoordinate") else { return Vec::new() };
    let pts = tc.nums("point");
    // T upwards, as rows go down: turned.
    let get = |i: i64| pts.get(i as usize * 2..i as usize * 2 + 2).filter(|_| i >= 0).map(|p| [p[0], 1.0 - p[1]]);
    let index: Vec<i64> = n.nums("texCoordIndex").iter().map(|&v| v as i64).collect();
    tris.iter()
        .map(|t| {
            let c = [0, 1, 2].map(|k| if index.is_empty() { Some(t.v[k] as i64) } else { index.get(t.at[k]).copied() }.and_then(get));
            c.iter().all(Option::is_some).then(|| c.map(Option::unwrap_or_default))
        })
        .collect()
}

/// The lists between the −1s, with where each starts.
fn runs(idx: &[i64]) -> Vec<Vec<(i64, usize)>> {
    idx.iter().enumerate().map(|(at, &v)| (v, at)).collect::<Vec<_>>().split(|&(v, _)| v < 0).filter(|r| !r.is_empty()).map(<[_]>::to_vec).collect()
}

fn tri(c: [(i64, usize); 3], face: usize) -> Tri {
    Tri { v: c.map(|(v, _)| v as u32), at: c.map(|(_, a)| a), face }
}

fn fan(r: &[(i64, usize)], face: usize) -> Vec<Tri> {
    (1..r.len().saturating_sub(1)).map(|k| tri([r[0], r[k], r[k + 1]], face)).collect()
}

fn strip(r: &[(i64, usize)]) -> Vec<Tri> {
    (0..r.len().saturating_sub(2)).map(|k| tri(if k % 2 == 0 { [r[k], r[k + 1], r[k + 2]] } else { [r[k + 1], r[k], r[k + 2]] }, k)).collect()
}

fn triples(idx: &[i64]) -> Vec<Tri> {
    idx.as_chunks::<3>().0.iter().enumerate().map(|(f, c)| tri([(c[0], f * 3), (c[1], f * 3 + 1), (c[2], f * 3 + 2)], f)).collect()
}

/// A text, with its FontStyle's size, line spacing and alignment along the lines.
fn text(n: &SceneNode) -> Part {
    let style = n.children().find(|c| c.kind == "FontStyle" || c.kind == "ScreenFontStyle");
    let size = style.map_or(1.0, |s| s.num("size", 1.0));
    let spacing = style.map_or(1.0, |s| s.num("spacing", 1.0));
    let justify = style.and_then(|s| s.strs("justify").first().cloned()).unwrap_or_default();
    let (pos, tris) = crate::model_text::mesh(n.strs("string"), size, spacing, justify.trim_matches('"'));
    Part { pos, tris, colors: Vec::new(), uv: Vec::new() }
}

/// Heights over a grid in the XZ plane.
fn elevation(n: &SceneNode) -> Part {
    let (nx, nz) = (n.num("xDimension", 0.0).max(0.0) as usize, n.num("zDimension", 0.0).max(0.0) as usize);
    let (sx, sz) = (n.num("xSpacing", 1.0), n.num("zSpacing", 1.0));
    let h = n.nums("height");
    if nx < 2 || nz < 2 || h.len() < nx * nz {
        return Part::default();
    }
    let pos = (0..nz).flat_map(|j| (0..nx).map(move |i| (i, j))).map(|(i, j)| [i as f32 * sx, h[j * nx + i], j as f32 * sz]).collect();
    let mut tris = Vec::new();
    for j in 0..nz - 1 {
        for i in 0..nx - 1 {
            let (a, b) = ((j * nx + i) as u32, ((j + 1) * nx + i) as u32);
            tris.push(Tri { v: [a, b, a + 1], at: [0; 3], face: j * (nx - 1) + i });
            tris.push(Tri { v: [a + 1, b, b + 1], at: [0; 3], face: j * (nx - 1) + i });
        }
    }
    let colors = colors(n, &tris);
    Part { pos, tris: tris.iter().map(|t| t.v).collect(), colors, uv: Vec::new() }
}

fn primitive(n: &SceneNode) -> Part {
    let half = |name: &str, d: f32| n.num(name, d) / 2.0;
    match n.kind.as_str() {
        "Box" => {
            let s = n.nums("size");
            let s = if s.len() >= 3 { [s[0] / 2.0, s[1] / 2.0, s[2] / 2.0] } else { [1.0; 3] };
            let pos = (0..8).map(|i| [0, 1, 2].map(|k| if i >> k & 1 == 1 { s[k] } else { -s[k] })).collect();
            // Corners bottom left, bottom right, top right, top left as X3D looks at each face:
            // from outside with +Y up (front, back, right, left), from above with −Z up, from
            // below with +Z up. The whole picture on each, upright.
            let quads = [[4, 5, 7, 6], [1, 0, 2, 3], [5, 1, 3, 7], [0, 4, 6, 2], [6, 7, 3, 2], [0, 1, 5, 4]];
            let c = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
            let uv = quads.iter().flat_map(|_| [Some([c[0], c[1], c[2]]), Some([c[0], c[2], c[3]])]).collect();
            Part { pos, tris: quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect(), colors: Vec::new(), uv }
        }
        "Sphere" => {
            let r = n.num("radius", 1.0);
            lathe(&(0..=RINGS).map(|k| (k as f32 / RINGS as f32, (k as f32 / RINGS as f32 * std::f32::consts::PI - std::f32::consts::FRAC_PI_2).sin_cos())).map(|(v, (s, c))| (c * r, s * r, v)).collect::<Vec<_>>())
        }
        "Cylinder" => {
            let (r, h) = (n.num("radius", 1.0), half("height", 2.0));
            lathe(&[(0.0, -h, 0.0), (r, -h, 0.0), (r, h, 1.0), (0.0, h, 1.0)])
        }
        _ => {
            let (r, h) = (n.num("bottomRadius", 1.0), half("height", 2.0));
            lathe(&[(0.0, -h, 0.0), (r, -h, 0.0), (0.0, h, 1.0)])
        }
    }
}

/// A profile of (radius, height, height on the picture) turned about the Y axis. The picture goes
/// around from the back (−Z), anticlockwise seen from above, as X3D wraps it; flat rings (caps)
/// get a disc cut from it, seen from above with −Z up or from below with +Z up.
fn lathe(profile: &[(f32, f32, f32)]) -> Part {
    let mut pos = Vec::new();
    for &(r, y, _) in profile {
        for s in 0..SEGMENTS {
            let (sin, cos) = (s as f32 / SEGMENTS as f32 * std::f32::consts::TAU).sin_cos();
            pos.push([r * cos, y, -r * sin]);
        }
    }
    let at = |ring: usize, s: usize| (ring * SEGMENTS + s % SEGMENTS) as u32;
    let (mut tris, mut uv) = (Vec::new(), Vec::new());
    for ring in 0..profile.len() - 1 {
        let cap = profile[ring].1 == profile[ring + 1].1;
        let rim = (2.0 * profile[ring].0.max(profile[ring + 1].0)).max(f32::MIN_POSITIVE);
        let up = if profile[ring].1 > 0.0 { 1.0 } else { -1.0 };
        let corner = |(r, s): (usize, usize)| {
            let p: [f32; 3] = pos[at(r, s) as usize];
            if cap { [0.5 + p[0] / rim, 0.5 + up * p[2] / rim] } else { [s as f32 / SEGMENTS as f32 - 0.25, 1.0 - profile[r].2] }
        };
        for s in 0..SEGMENTS {
            for t in [[(ring, s), (ring, s + 1), (ring + 1, s + 1)], [(ring, s), (ring + 1, s + 1), (ring + 1, s)]] {
                tris.push(t.map(|(r, s)| at(r, s)));
                uv.push(Some(t.map(corner)));
            }
        }
    }
    Part { pos, tris, colors: Vec::new(), uv }
}

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: V3) -> Option<V3> {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    (l > 1e-9).then(|| a.map(|v| v / l))
}

/// A cross section (in XZ) carried along a spine, scaled and turned at each of its points, as
/// VRML defines it; the caps are fans.
fn extrusion(n: &SceneNode) -> Part {
    let pairs = |name: &str, d: &[[f32; 2]]| -> Vec<[f32; 2]> {
        let v: Vec<[f32; 2]> = n.nums(name).as_chunks::<2>().0.to_vec();
        if v.is_empty() { d.to_vec() } else { v }
    };
    let cs = pairs("crossSection", &[[1.0, 1.0], [1.0, -1.0], [-1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]]);
    let scale = pairs("scale", &[[1.0, 1.0]]);
    let mut spine: Vec<V3> = n.nums("spine").as_chunks::<3>().0.to_vec();
    if spine.is_empty() {
        spine = vec![[0.0; 3], [0.0, 1.0, 0.0]];
    }
    let orient = n.nums("orientation").as_chunks::<4>().0;
    let last = spine.len() - 1;
    let closed = last > 1 && spine[0] == spine[last];
    let (prev, next) = (|i: usize| if i > 0 { i - 1 } else if closed { last - 1 } else { 0 }, |i: usize| if i < last { i + 1 } else if closed { 1 } else { last });
    let mut z_prev: Option<V3> = None;
    let mut pos = Vec::new();
    for i in 0..=last {
        let y = unit(sub(spine[next(i)], spine[prev(i)])).unwrap_or([0.0, 1.0, 0.0]);
        let mut z = unit(cross(sub(spine[next(i)], spine[i]), sub(spine[prev(i)], spine[i]))).or(z_prev).unwrap_or_else(|| {
            // A straight spine: the plane turned from Y up to the spine.
            let r = unit(cross([0.0, 1.0, 0.0], y)).map_or(IDENTITY, |axis| rotation(axis, y[1].clamp(-1.0, 1.0).acos()));
            apply(&r, [0.0, 0.0, 1.0])
        });
        if z_prev.is_some_and(|p| p[0] * z[0] + p[1] * z[1] + p[2] * z[2] < 0.0) {
            z = z.map(|v| -v);
        }
        z_prev = Some(z);
        let x = cross(y, z);
        let s = scale.get(i).or(scale.last()).copied().unwrap_or([1.0, 1.0]);
        let r = orient.get(i).or(orient.last()).map_or(IDENTITY, |o| rotation([o[0], o[1], o[2]], o[3]));
        for c in &cs {
            let q = apply(&r, [c[0] * s[0], 0.0, c[1] * s[1]]);
            pos.push(std::array::from_fn(|k| spine[i][k] + x[k] * q[0] + y[k] * q[1] + z[k] * q[2]));
        }
    }
    let m = cs.len() as u32;
    let mut tris: Vec<[u32; 3]> = (0..last as u32).flat_map(|i| (0..m.saturating_sub(1)).flat_map(move |k| [[i * m + k, i * m + k + 1, (i + 1) * m + k + 1], [i * m + k, (i + 1) * m + k + 1, (i + 1) * m + k]])).collect();
    for (cap, ring) in [("beginCap", 0), ("endCap", last as u32)] {
        if n.bool(cap, true) {
            tris.extend((1..m.saturating_sub(1)).map(|k| [ring * m, ring * m + k, ring * m + k + 1]));
        }
    }
    Part { pos, tris, colors: Vec::new(), uv: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_sets_primitives_and_transforms() {
        let wrl = "#VRML V2.0 utf8\nTransform { translation 10 0 0 children [\n Shape { appearance Appearance { material Material { diffuseColor 1 0 0 } }\n  geometry IndexedFaceSet { coord Coordinate { point [ 0 0 0, 1 0 0, 1 1 0, 0 1 0 ] } coordIndex [ 0 1 2 3 -1 ] } }\n Shape { geometry Box { size 2 2 2 } } ] }\n";
        let m = read(wrl.as_bytes(), &|_| None).unwrap();
        assert_eq!(m.tris.len(), 2 + 12);
        assert_eq!(m.colors[0], [255, 0, 0]);
        assert_eq!(m.pos[1], [11.0, 0.0, 0.0]);
        let x3d = "<X3D><Scene><Shape><IndexedFaceSet coordIndex='0 1 2 -1' colorPerVertex='false'><Coordinate point='0 0 0 1 0 0 0 1 0'/><Color color='0 0 1'/></IndexedFaceSet></Shape><Shape><Sphere/></Shape></Scene></X3D>";
        let m = read(x3d.as_bytes(), &|_| None).unwrap();
        assert_eq!(m.colors[0], [0, 0, 255]);
        // A picture on a triangle by its own index, and on a sphere.
        let x3d = "<X3D><Scene><Shape><Appearance><ImageTexture url='\"http://x/a.png\" \"a.png\"'/></Appearance><IndexedFaceSet coordIndex='0 1 2 -1' texCoordIndex='2 1 0 -1'><Coordinate point='0 0 0 1 0 0 0 1 0'/><TextureCoordinate point='0 0 1 0 0 1'/></IndexedFaceSet></Shape><Shape><Appearance><ImageTexture url='a.png'/></Appearance><Sphere/></Shape></Scene></X3D>";
        let mut png = Vec::new();
        image::RgbaImage::new(2, 2).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let m = read(x3d.as_bytes(), &|n| (n == "a.png").then(|| png.clone())).unwrap();
        assert_eq!((m.textures.len(), m.colors[0], m.uv[0]), (1, [255; 3], [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]]));
        assert!(m.tex.iter().all(|&k| k == 1));
        assert_eq!(m.tris.len(), 1 + RINGS * SEGMENTS * 2);
        // The default square along the default spine: a cube of side 2 and height 1.
        let m = read(b"#VRML V2.0 utf8\nShape { geometry Extrusion { } }", &|_| None).unwrap();
        assert_eq!((m.tris.len(), m.center, m.radius), (4 * 2 + 2 * 3, [0.0, 0.5, 0.0], 1.5));
    }
}
