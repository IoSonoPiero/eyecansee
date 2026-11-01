//! 3D models made of triangles (STL, OBJ, PLY, glTF/GLB, 3MF in `model_3mf`), read into one
//! `Mesh` that `render3d` draws: in the grid, in the viewer and in screenshots.

use std::io::Cursor;

use ply_rs_bw::{parser::Parser, ply::DefaultElement, ply::Property};

/// Extensions read as 3D models.
const MODEL: &[&str] = &["stl", "obj", "ply", "glb", "gltf", "3mf"];

/// Beyond this the file is refused instead of filling the memory (a 3D scan of a statue is ~10 M).
const MAX_TRIANGLES: usize = 50_000_000;

pub fn is_model(name: &str) -> bool {
    ext(name).is_some_and(|e| MODEL.contains(&e.as_str()))
}

fn ext(name: &str) -> Option<String> {
    Some(name.rsplit_once('.')?.1.to_lowercase())
}

/// Triangles in one list, with the box around them for the camera.
#[derive(Debug)]
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
    /// One per triangle, or empty: all in the default colour.
    pub colors: Vec<[u8; 3]>,
    /// Z points up (3D printing, CAD: STL, 3MF), otherwise Y (glTF, OBJ, scans in PLY).
    pub z_up: bool,
    pub center: [f32; 3],
    pub radius: f32,
}

/// Collects the parts of a model (glTF primitives, OBJ groups, 3MF objects) into one mesh.
#[derive(Default)]
pub(crate) struct Builder {
    pos: Vec<[f32; 3]>,
    tris: Vec<[u32; 3]>,
    colors: Vec<Option<[u8; 3]>>,
}

impl Builder {
    /// Adds a part; its triangles index its own `pos`. Triangles outside `pos` are dropped
    /// (files are not trusted). `colors`: one per triangle, or `None` for the default.
    pub(crate) fn add(&mut self, pos: Vec<[f32; 3]>, tris: impl IntoIterator<Item = [u32; 3]>, mut color: impl FnMut(usize) -> Option<[u8; 3]>) -> Result<(), String> {
        let base = self.pos.len() as u32;
        let n = pos.len() as u32;
        for (i, t) in tris.into_iter().enumerate() {
            if t.iter().all(|&v| v < n) {
                self.tris.push(t.map(|v| v + base));
                self.colors.push(color(i));
            }
            if self.tris.len() > MAX_TRIANGLES {
                return Err(format!("more than {MAX_TRIANGLES} triangles"));
            }
        }
        self.pos.extend(pos);
        Ok(())
    }

    pub(crate) fn finish(self, z_up: bool) -> Result<Mesh, String> {
        let Builder { pos, tris, colors } = self;
        let finite = |p: &[f32; 3]| p.iter().all(|c| c.is_finite());
        let tris: Vec<(usize, [u32; 3])> = tris.into_iter().enumerate().filter(|(_, t)| t.iter().all(|&v| finite(&pos[v as usize]))).collect();
        if tris.is_empty() {
            return Err("no triangles in the file".into());
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &(_, t) in &tris {
            for v in t {
                let p = pos[v as usize];
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        let center = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
        let radius = ((0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt() / 2.0).max(f32::MIN_POSITIVE);
        let colors = if colors.iter().any(Option::is_some) { tris.iter().map(|&(i, _)| colors[i].unwrap_or(DEFAULT)).collect() } else { Vec::new() };
        Ok(Mesh { pos, tris: tris.into_iter().map(|(_, t)| t).collect(), colors, z_up, center, radius })
    }
}

/// Light grey with a touch of blue, as in CAD viewers.
pub const DEFAULT: [u8; 3] = [200, 205, 214];

pub fn parse(bytes: &[u8], name: &str) -> Result<Mesh, String> {
    match ext(name).as_deref() {
        Some("stl") => stl(bytes),
        Some("obj") => obj(bytes),
        Some("ply") => ply(bytes),
        Some("glb" | "gltf") => gltf(bytes),
        Some("3mf") => crate::model_3mf::read(bytes),
        _ => Err("not a 3D model".into()),
    }
}

fn stl(bytes: &[u8]) -> Result<Mesh, String> {
    let m = stl_io::read_stl(&mut Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut b = Builder::default();
    let tris = m.faces.iter().map(|f| f.vertices.map(|v| v as u32));
    b.add(m.vertices.iter().map(|v| v.0).collect(), tris, |_| None)?;
    b.finish(true)
}

// ponytail: the materials (.mtl) are another file, and sources read one file: no colours from them
fn obj(bytes: &[u8]) -> Result<Mesh, String> {
    let options = tobj::LoadOptions { triangulate: true, ignore_points: true, ignore_lines: true, ..Default::default() };
    let (models, _) = tobj::load_obj_buf(&mut Cursor::new(bytes), &options, |_| Err(tobj::LoadError::OpenFileFailed)).map_err(|e| e.to_string())?;
    let mut b = Builder::default();
    for m in models {
        let m = m.mesh;
        let pos = m.positions.as_chunks::<3>().0.to_vec();
        let tris: Vec<[u32; 3]> = m.indices.as_chunks::<3>().0.to_vec();
        let vc: Vec<[f32; 3]> = m.vertex_color.as_chunks::<3>().0.to_vec();
        let colors = |i: usize| average(&vc, tris[i]).map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8));
        b.add(pos, tris.clone(), colors)?;
    }
    b.finish(false)
}

/// The average colour of a triangle's vertices, if they have colours.
fn average(colors: &[[f32; 3]], t: [u32; 3]) -> Option<[f32; 3]> {
    let c = t.map(|v| colors.get(v as usize));
    let [Some(a), Some(b), Some(c)] = c else { return None };
    Some([0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0))
}

fn ply(bytes: &[u8]) -> Result<Mesh, String> {
    let ply = Parser::<DefaultElement>::new().read_ply(&mut Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let vertices = ply.payload.get("vertex").ok_or("no vertices in the file")?;
    let coord = |v: &DefaultElement, k: &str| v.get(k).and_then(scalar).unwrap_or(f64::NAN) as f32;
    let pos = vertices.iter().map(|v| [coord(v, "x"), coord(v, "y"), coord(v, "z")]).collect();
    let rgb: Vec<[f32; 3]> = vertices.iter().filter_map(ply_color).collect();
    let rgb = if rgb.len() == vertices.len() { rgb } else { Vec::new() };
    let mut tris = Vec::new();
    for f in ply.payload.get("face").into_iter().flatten() {
        let Some(list) = f.get("vertex_indices").or_else(|| f.get("vertex_index")).and_then(list) else { continue };
        tris.extend((1..list.len().saturating_sub(1)).map(|i| [list[0], list[i], list[i + 1]]));
    }
    // Triangle strips (Stanford's scans): -1 starts a new strip, every other triangle turns the other way.
    for s in ply.payload.get("tristrips").into_iter().flatten() {
        let Some(list) = s.get("vertex_indices").and_then(list) else { continue };
        for strip in list.split(|&v| v == u32::MAX) {
            tris.extend(strip.windows(3).enumerate().map(|(i, w)| if i % 2 == 0 { [w[0], w[1], w[2]] } else { [w[1], w[0], w[2]] }));
        }
    }
    let mut b = Builder::default();
    let colors = |i: usize| average(&rgb, tris[i]).map(|c| c.map(|v| v as u8));
    b.add(pos, tris.clone(), colors)?;
    b.finish(false)
}

/// A vertex colour on 0-255, from bytes or from 0-1 floats.
fn ply_color(v: &DefaultElement) -> Option<[f32; 3]> {
    let c = ["red", "green", "blue"].map(|k| v.get(k));
    let [Some(r), Some(g), Some(b)] = c else { return None };
    let scale = if matches!(r, Property::Float(_) | Property::Double(_)) { 255.0 } else { 1.0 };
    Some([scalar(r)? * scale, scalar(g)? * scale, scalar(b)? * scale].map(|v| v as f32))
}

fn scalar(p: &Property) -> Option<f64> {
    Some(match *p {
        Property::Char(v) => v.into(),
        Property::UChar(v) => v.into(),
        Property::Short(v) => v.into(),
        Property::UShort(v) => v.into(),
        Property::Int(v) => v.into(),
        Property::UInt(v) => v.into(),
        Property::Float(v) => v.into(),
        Property::Double(v) => v,
        _ => return None,
    })
}

/// An index list; negative values become `u32::MAX` (the strip separator, an invalid index otherwise).
fn list(p: &Property) -> Option<Vec<u32>> {
    let signed = |v: i64| u32::try_from(v).unwrap_or(u32::MAX);
    Some(match p {
        Property::ListChar(v) => v.iter().map(|&i| signed(i.into())).collect(),
        Property::ListUChar(v) => v.iter().map(|&i| i.into()).collect(),
        Property::ListShort(v) => v.iter().map(|&i| signed(i.into())).collect(),
        Property::ListUShort(v) => v.iter().map(|&i| i.into()).collect(),
        Property::ListInt(v) => v.iter().map(|&i| signed(i.into())).collect(),
        Property::ListUInt(v) => v.clone(),
        _ => return None,
    })
}

/// Column-major, as glTF writes it.
type Mat4 = [[f32; 4]; 4];
const IDENTITY: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

fn apply(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

// ponytail: GLB and .gltf with the data inside; a .gltf with a separate .bin needs reading a second file
fn gltf(bytes: &[u8]) -> Result<Mesh, String> {
    use base64::Engine;
    let g = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
    let inside: Vec<Option<Vec<u8>>> = g
        .document
        .buffers()
        .map(|b| match b.source() {
            gltf::buffer::Source::Uri(uri) => uri.split_once(";base64,").and_then(|(_, data)| base64::engine::general_purpose::STANDARD.decode(data).ok()),
            gltf::buffer::Source::Bin => None,
        })
        .collect();
    let data = |b: gltf::Buffer<'_>| match b.source() {
        gltf::buffer::Source::Bin => g.blob.as_deref(),
        gltf::buffer::Source::Uri(_) => inside[b.index()].as_deref(),
    };
    if g.document.buffers().any(|b| data(b).is_none()) {
        return Err("the model's data is in another file (.bin): open the .glb version".into());
    }
    let mut b = Builder::default();
    match g.document.default_scene().or_else(|| g.document.scenes().next()) {
        Some(scene) => {
            for node in scene.nodes() {
                gltf_node(&mut b, &node, &IDENTITY, &data, 0)?;
            }
        }
        None => {
            for mesh in g.document.meshes() {
                gltf_mesh(&mut b, &mesh, &IDENTITY, &data)?;
            }
        }
    }
    b.finish(false)
}

fn gltf_node<'a, 's, F: Clone + for<'x> Fn(gltf::Buffer<'x>) -> Option<&'s [u8]>>(b: &mut Builder, node: &gltf::Node<'a>, parent: &Mat4, data: &F, depth: u32) -> Result<(), String> {
    if depth > 64 {
        return Err("nodes nested too deep".into());
    }
    let m = mul(parent, &node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        gltf_mesh(b, &mesh, &m, data)?;
    }
    for child in node.children() {
        gltf_node(b, &child, &m, data, depth + 1)?;
    }
    Ok(())
}

fn gltf_mesh<'a, 's, F: Clone + for<'x> Fn(gltf::Buffer<'x>) -> Option<&'s [u8]>>(b: &mut Builder, mesh: &gltf::Mesh<'a>, m: &Mat4, data: &F) -> Result<(), String> {
    for prim in mesh.primitives().filter(|p| p.mode() == gltf::mesh::Mode::Triangles) {
        let r = prim.reader(data.clone());
        let Some(pos) = r.read_positions() else { continue };
        let pos: Vec<[f32; 3]> = pos.map(|p| apply(m, p)).collect();
        let idx: Vec<u32> = match r.read_indices() {
            Some(i) => i.into_u32().collect(),
            None => (0..pos.len() as u32).collect(),
        };
        let tris: Vec<[u32; 3]> = idx.as_chunks::<3>().0.to_vec();
        let vc: Vec<[f32; 3]> = r.read_colors(0).map(|c| c.into_rgb_f32().collect()).unwrap_or_default();
        let base = prim.material().pbr_metallic_roughness().base_color_factor();
        let base = [base[0], base[1], base[2]];
        // glTF colours are linear: brought to sRGB for the screen.
        let srgb = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8);
        let colors = |i: usize| {
            let c = average(&vc, tris[i]).unwrap_or([1.0; 3]);
            (base != [1.0; 3] || !vc.is_empty()).then(|| srgb([c[0] * base[0], c[1] * base[1], c[2] * base[2]]))
        };
        b.add(pos, tris.clone(), colors)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE_OBJ: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\nf 1 2 3 4\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";

    #[test]
    fn obj_quads_become_triangles() {
        let m = parse(CUBE_OBJ.as_bytes(), "cube.OBJ").unwrap();
        assert_eq!((m.pos.len(), m.tris.len()), (8, 12));
        assert_eq!(m.center, [0.5; 3]);
        assert!((m.radius - 3f32.sqrt() / 2.0).abs() < 1e-6);
        assert!(m.colors.is_empty() && !m.z_up);
    }

    #[test]
    fn ascii_stl_and_ply_with_colours_and_strips() {
        let stl = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        let m = parse(stl.as_bytes(), "t.stl").unwrap();
        assert_eq!((m.tris.len(), m.z_up), (1, true));
        let ply = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nelement tristrips 1\nproperty list int int vertex_indices\nend_header\n0 0 0 255 0 0\n1 0 0 255 0 0\n1 1 0 255 0 0\n0 1 0 0 0 255\n4 0 1 2 3\n5 0 1 2 -1 3\n";
        let m = parse(ply.as_bytes(), "q.ply").unwrap();
        // The quad fan (2) and the strip 0 1 2 (1); "-1 3" alone is no triangle.
        assert_eq!(m.tris, vec![[0, 1, 2], [0, 2, 3], [0, 1, 2]]);
        assert_eq!(m.colors[0], [255, 0, 0]);
        assert_eq!(m.colors[1], [170, 0, 85]);
    }

    #[test]
    fn bad_indices_and_empty_files_refused() {
        assert!(parse(b"v 0 0 0\nf 1 2 3\n", "x.obj").is_err());
        assert!(parse(b"", "x.stl").is_err());
        assert!(parse(b"{}", "x.gltf").is_err());
        assert!(is_model("a.GLB") && is_model("b.3mf") && !is_model("c.png"));
    }

    /// Every model under `EYECANSEE_MODELS` read and drawn into `EYECANSEE_MODELS_OUT` (PNG), with times.
    #[test]
    #[ignore]
    fn models_live() {
        let (Ok(dir), Ok(out)) = (std::env::var("EYECANSEE_MODELS"), std::env::var("EYECANSEE_MODELS_OUT")) else { return };
        let mut failed = Vec::new();
        for sub in std::fs::read_dir(&dir).unwrap().flatten() {
            for f in std::fs::read_dir(sub.path()).unwrap().flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if !is_model(&name) {
                    continue;
                }
                let bytes = std::fs::read(f.path()).unwrap();
                let t = std::time::Instant::now();
                match parse(&bytes, &name) {
                    Ok(m) => {
                        let read = t.elapsed();
                        let t = std::time::Instant::now();
                        let img = crate::render3d::render(&m, &Default::default(), 800, 600, 2);
                        println!("{name}: {} triangles, read {read:.0?}, drawn {:.0?}", m.tris.len(), t.elapsed());
                        img.save(format!("{out}/{name}.png")).unwrap();
                    }
                    Err(e) => {
                        println!("{name}: ERROR {e}");
                        failed.push(name);
                    }
                }
            }
        }
        println!("failed: {failed:?}");
    }

    #[test]
    fn gltf_matrices() {
        let t: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [5.0, 0.0, 0.0, 1.0]];
        let s: Mat4 = [[2.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        // Scaled first, then moved.
        assert_eq!(apply(&mul(&t, &s), [1.0, 1.0, 1.0]), [7.0, 2.0, 2.0]);
    }
}
