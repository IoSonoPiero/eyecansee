//! 3D models made of triangles (STL, OBJ, PLY, glTF/GLB, 3MF in `model_3mf`), read into one
//! `Mesh` that `render3d` draws: in the grid, in the viewer and in screenshots.

use std::io::Cursor;

use ply_rs_bw::{parser::Parser, ply::DefaultElement, ply::Property};

/// Extensions read as 3D models.
const MODEL: &[&str] = &["stl", "obj", "ply", "glb", "gltf", "3mf", "ifc", "fbx", "3ds", "off", "amf", "dae", "zae", "wrl", "wrz", "x3d", "x3dv", "x3dz", "x3dvz"];

/// Beyond this the file is refused instead of filling the memory (a 3D scan of a statue is ~10 M).
const MAX_TRIANGLES: usize = 50_000_000;

/// Also STEP, IGES and BREP (`occt`), read by the CAD engine.
pub fn is_model(name: &str) -> bool {
    ext(name).is_some_and(|e| MODEL.contains(&e.as_str())) || crate::occt::is_occt(name)
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

    /// The box around the points added so far (all of them, used or not).
    pub(crate) fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let finite = self.pos.iter().filter(|p| p.iter().all(|c| c.is_finite()));
        finite.fold(None, |acc, p| {
            let (mut lo, mut hi) = acc.unwrap_or((*p, *p));
            for k in 0..3 {
                (lo[k], hi[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
            }
            Some((lo, hi))
        })
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

/// Reads a file next to the model by the name the model gives it (a glTF's `.bin`, an OBJ's
/// `.mtl`), from the same source; `None` when missing or refused (`sibling_path`).
pub type Sibling<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

#[cfg(test)]
pub fn parse(bytes: &[u8], name: &str) -> Result<Mesh, String> {
    parse_with(bytes, name, &|_| None)
}

/// Like `parse`, reading the files next to the model through `sibling`.
pub fn parse_with(bytes: &[u8], name: &str, sibling: Sibling) -> Result<Mesh, String> {
    match ext(name).as_deref() {
        Some("stl") => stl(bytes),
        Some("obj") => obj(bytes, sibling),
        Some("ply") => ply(bytes),
        Some("glb" | "gltf") => gltf(bytes, sibling),
        Some("3mf") => crate::model_3mf::read(bytes),
        Some("ifc") => crate::model_ifc::read(bytes),
        Some("fbx") => crate::model_fbx::read(bytes),
        Some("3ds") => crate::model_3ds::read(bytes),
        Some("off") => crate::model_off::read(bytes),
        Some("amf") => crate::model_amf::read(bytes),
        Some(e @ ("dae" | "zae")) => crate::model_dae::read(bytes, e == "zae"),
        Some("wrl" | "wrz" | "x3d" | "x3dv" | "x3dz" | "x3dvz") => crate::model_x3d::read(bytes, sibling),
        _ if crate::occt::is_occt(name) => crate::occt::read(bytes, name),
        _ => Err("not a 3D model".into()),
    }
}

fn stl(bytes: &[u8]) -> Result<Mesh, String> {
    let n = bytes.get(80..84).map_or(0, |c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize);
    // Binary when the size matches its count, even if the header starts with "solid".
    if bytes.len() >= 84 && n.checked_mul(50).and_then(|b| b.checked_add(84)) == Some(bytes.len()) {
        return stl_binary(bytes, n);
    }
    // "solid" then a line break or a tab, without a name: stl_io takes it for binary.
    let named;
    let bytes = match bytes.strip_prefix(b"solid") {
        Some(rest) if rest.first().is_some_and(|c| *c != b' ') => {
            named = [&b"solid model"[..], rest].concat();
            &named[..]
        }
        _ => bytes,
    };
    let m = stl_io::read_stl(&mut Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut b = Builder::default();
    let tris = m.faces.iter().map(|f| f.vertices.map(|v| v as u32));
    b.add(m.vertices.iter().map(|v| v.0).collect(), tris, |_| None)?;
    b.finish(true)
}

/// Binary STL, with the colours of its triangles: Materialise's ("COLOR=" in the header, red in
/// the low bits, bit 15 off when the triangle has one) or VisCAM's (bit 15 on, blue in the low bits).
fn stl_binary(bytes: &[u8], n: usize) -> Result<Mesh, String> {
    let materialise = bytes[..80].windows(6).any(|w| w == b"COLOR=");
    let f = |c: &[u8], i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
    let five = |v: u16| ((v & 31) as u32 * 255 / 31) as u8;
    let mut pos = Vec::with_capacity(n * 3);
    let mut colors = Vec::with_capacity(n);
    for c in bytes[84..].as_chunks::<50>().0.iter().take(n) {
        for k in 0..3 {
            pos.push([f(c, 12 + k * 12), f(c, 16 + k * 12), f(c, 20 + k * 12)]);
        }
        let a = u16::from_le_bytes([c[48], c[49]]);
        colors.push(match (materialise, a & 0x8000 != 0) {
            (true, false) => Some([five(a), five(a >> 5), five(a >> 10)]),
            (false, true) => Some([five(a >> 10), five(a >> 5), five(a)]),
            _ => None,
        });
    }
    let mut b = Builder::default();
    b.add(pos, (0..n as u32).map(|i| [i * 3, i * 3 + 1, i * 3 + 2]), |i| colors[i])?;
    b.finish(true)
}

/// The file a model names next to itself, as a path of the same source: `rel` is relative to
/// the model's folder, may be percent-encoded (glTF URIs), and may not leave that folder.
pub fn sibling_path(model: &str, rel: &str) -> Option<String> {
    let rel = percent_decoded(rel)?.replace('\\', "/");
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let refused = rel.starts_with('/') || rel.contains(':') || parts.is_empty() || parts.contains(&"..");
    (!refused).then(|| format!("{}{}", crate::source::parent(model), parts.join("/")))
}

fn percent_decoded(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

/// With the colours of its materials (`Kd` of the `.mtl` files it names) or of its vertices.
fn obj(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
    let options = tobj::LoadOptions { triangulate: true, ignore_points: true, ignore_lines: true, ..Default::default() };
    let mtl = |p: &std::path::Path| match p.to_str().and_then(sibling) {
        Some(data) => tobj::load_mtl_buf(&mut Cursor::new(utf8(&data).as_bytes())),
        None => Err(tobj::LoadError::OpenFileFailed),
    };
    let (models, materials) = tobj::load_obj_buf(&mut Cursor::new(utf8(bytes).as_bytes()), &options, mtl).map_err(|e| e.to_string())?;
    let materials = materials.unwrap_or_default();
    let mut b = Builder::default();
    for m in models {
        let m = m.mesh;
        let pos = m.positions.as_chunks::<3>().0.to_vec();
        let tris: Vec<[u32; 3]> = m.indices.as_chunks::<3>().0.to_vec();
        let vc: Vec<[f32; 3]> = m.vertex_color.as_chunks::<3>().0.to_vec();
        let kd = m.material_id.and_then(|i| materials.get(i)).and_then(|mat| mat.diffuse);
        let to_u8 = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
        let colors = |i: usize| average(&vc, tris[i]).or(kd).map(to_u8);
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

/// Text files of old programs: UTF-8, UTF-16 with its mark, or else Latin-1 (3ds Max's OBJ).
fn utf8(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return crate::model_3mf::text(bytes).into();
    }
    match std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)) {
        Ok(s) => s.into(),
        Err(_) => bytes.iter().map(|&b| b as char).collect::<String>().into(),
    }
}

/// The header with only the lines the parser knows: old Blender wrote one without "comment",
/// others put comments among the properties.
fn ply_header_cleaned(bytes: &[u8]) -> Option<Vec<u8>> {
    let end = bytes.windows(10).position(|w| w == b"end_header")?;
    let head = String::from_utf8_lossy(&bytes[..end]);
    let keep = |l: &&str| ["ply", "format ", "element ", "property "].iter().any(|k| l.trim_start().starts_with(k));
    let lines: Vec<&str> = head.lines().filter(keep).collect();
    let mut out = (lines.join("\n") + "\n").into_bytes();
    out.extend_from_slice(&bytes[end..]);
    Some(out)
}

fn ply(bytes: &[u8]) -> Result<Mesh, String> {
    let cleaned = ply_header_cleaned(bytes);
    let bytes = cleaned.as_deref().unwrap_or(bytes);
    let ply = Parser::<DefaultElement>::new().read_ply(&mut Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let vertices = ply.payload.get("vertex").ok_or("no vertices in the file")?;
    let coord = |v: &DefaultElement, k: &str| v.get(k).and_then(scalar).unwrap_or(f64::NAN) as f32;
    let pos = vertices.iter().map(|v| [coord(v, "x"), coord(v, "y"), coord(v, "z")]).collect();
    let rgb: Vec<[f32; 3]> = vertices.iter().filter_map(ply_color).collect();
    let rgb = if rgb.len() == vertices.len() { rgb } else { Vec::new() };
    let mut tris = Vec::new();
    // Colours of the faces (MeshLab writes them), one per triangle of the face.
    let mut face_rgb: Vec<Option<[f32; 3]>> = Vec::new();
    for f in ply.payload.get("face").into_iter().flatten() {
        let Some(list) = f.get("vertex_indices").or_else(|| f.get("vertex_index")).and_then(list) else { continue };
        let n = list.len().saturating_sub(2);
        tris.extend((1..list.len().saturating_sub(1)).map(|i| [list[0], list[i], list[i + 1]]));
        face_rgb.extend(std::iter::repeat_n(ply_color(f), n));
    }
    // Triangle strips (Stanford's scans): -1 starts a new strip, every other triangle turns the other way.
    for s in ply.payload.get("tristrips").into_iter().flatten() {
        let Some(list) = s.get("vertex_indices").and_then(list) else { continue };
        for strip in list.split(|&v| v == u32::MAX) {
            tris.extend(strip.windows(3).enumerate().map(|(i, w)| if i % 2 == 0 { [w[0], w[1], w[2]] } else { [w[1], w[0], w[2]] }));
        }
    }
    if tris.is_empty() {
        return point_cloud(pos, &rgb);
    }
    let mut b = Builder::default();
    let colors = |i: usize| face_rgb.get(i).copied().flatten().or_else(|| average(&rgb, tris[i])).map(|c| c.map(|v| v as u8));
    b.add(pos, tris.clone(), colors)?;
    b.finish(false)
}

/// Points without faces (3D scans): each one a small tetrahedron, as large as the space between
/// points on a surface, so that the cloud looks like a surface of dots.
fn point_cloud(pos: Vec<[f32; 3]>, rgb: &[[f32; 3]]) -> Result<Mesh, String> {
    let pos: Vec<([f32; 3], Option<[u8; 3]>)> = pos.into_iter().enumerate().filter(|(_, p)| p.iter().all(|c| c.is_finite())).map(|(i, p)| (p, rgb.get(i).map(|c| c.map(|v| v as u8)))).collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (p, _) in &pos {
        for k in 0..3 {
            (lo[k], hi[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
        }
    }
    let diag = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt();
    let s = (diag / (pos.len() as f32).sqrt() * 0.6).clamp(diag / 2000.0, diag / 100.0) / 3f32.sqrt();
    let corners = [[1.0, 1.0, 1.0], [1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [-1.0, -1.0, 1.0f32]];
    let mut b = Builder::default();
    let tris: Vec<[u32; 3]> = (0..pos.len() as u32).flat_map(|i| [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]].map(|t| t.map(|v| i * 4 + v))).collect();
    let colors: Vec<Option<[u8; 3]>> = pos.iter().map(|(_, c)| *c).collect();
    let points = pos.iter().flat_map(|(p, _)| corners.map(|c| std::array::from_fn(|k| p[k] + c[k] * s))).collect();
    b.add(points, tris, |i| colors[i / 4])?;
    b.finish(false).map_err(|_| "no points in the file".to_string())
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
pub(crate) type Mat4 = [[f32; 4]; 4];
pub(crate) const IDENTITY: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub(crate) fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

pub(crate) fn apply(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

/// A turn of `angle` radians about `axis` (right-handed).
pub(crate) fn rotation(axis: [f32; 3], angle: f32) -> Mat4 {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt().max(f32::MIN_POSITIVE);
    let [x, y, z] = axis.map(|v| v / len);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [[t * x * x + c, t * x * y + s * z, t * x * z - s * y, 0.0], [t * x * y - s * z, t * y * y + c, t * y * z + s * x, 0.0], [t * x * z + s * y, t * y * z - s * x, t * z * z + c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

/// What a glTF that cannot be read needs, said plainly: compressed geometry, version 1.0.
fn gltf_error(e: String) -> String {
    let has = |s: &str| e.contains(s);
    if has("KHR_draco_mesh_compression") {
        "glTF compressed with Draco is not supported".into()
    } else if has("EXT_meshopt_compression") || has("KHR_meshopt_compression") {
        "glTF compressed with meshopt is not supported".into()
    } else if has("KHR_mesh_quantization") {
        "glTF with quantized geometry (KHR_mesh_quantization) is not supported".into()
    } else if has("expected a sequence") {
        // glTF 1.0 keeps its parts in maps by name, 2.0 in lists.
        "glTF 1.0 is not supported (only glTF 2.0)".into()
    } else {
        e
    }
}

/// GLB, and .gltf with its data inside or in the files next to it (`sibling`).
fn gltf(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
    use base64::Engine;
    let g = gltf::Gltf::from_slice(bytes).map_err(|e| gltf_error(e.to_string()))?;
    let inside: Vec<Option<Vec<u8>>> = g
        .document
        .buffers()
        .map(|b| match b.source() {
            gltf::buffer::Source::Uri(uri) if uri.starts_with("data:") => uri.split_once(";base64,").and_then(|(_, data)| base64::engine::general_purpose::STANDARD.decode(data).ok()),
            gltf::buffer::Source::Uri(uri) => sibling(uri),
            gltf::buffer::Source::Bin => None,
        })
        .collect();
    let data = |b: gltf::Buffer<'_>| match b.source() {
        gltf::buffer::Source::Bin => g.blob.as_deref(),
        gltf::buffer::Source::Uri(_) => inside[b.index()].as_deref(),
    };
    if g.document.buffers().any(|b| data(b).is_none()) {
        return Err("the model's data file (.bin) is missing".into());
    }
    let mut b = Builder::default();
    let mut extra = (Vec::new(), Vec::new());
    match g.document.default_scene().or_else(|| g.document.scenes().next()) {
        Some(scene) => {
            for node in scene.nodes() {
                gltf_node(&mut b, &mut extra, &node, &IDENTITY, &data, 0)?;
            }
        }
        None => {
            for mesh in g.document.meshes() {
                gltf_mesh(&mut b, &mut extra, &mesh, &IDENTITY, &data)?;
            }
        }
    }
    crate::model_lines::add(&mut b, &extra.0, &extra.1)?;
    b.finish(false)
}

/// Line segments and points of a glTF, made solid at the end (`model_lines`).
type Extra = (Vec<crate::model_lines::Segment>, Vec<crate::model_lines::Point>);

fn gltf_node<'a, 's, F: Clone + for<'x> Fn(gltf::Buffer<'x>) -> Option<&'s [u8]>>(b: &mut Builder, extra: &mut Extra, node: &gltf::Node<'a>, parent: &Mat4, data: &F, depth: u32) -> Result<(), String> {
    if depth > 64 {
        return Err("nodes nested too deep".into());
    }
    let m = mul(parent, &node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        gltf_mesh(b, extra, &mesh, &m, data)?;
    }
    for child in node.children() {
        gltf_node(b, extra, &child, &m, data, depth + 1)?;
    }
    Ok(())
}

fn gltf_mesh<'a, 's, F: Clone + for<'x> Fn(gltf::Buffer<'x>) -> Option<&'s [u8]>>(b: &mut Builder, extra: &mut Extra, mesh: &gltf::Mesh<'a>, m: &Mat4, data: &F) -> Result<(), String> {
    use gltf::mesh::Mode;
    for prim in mesh.primitives() {
        let r = prim.reader(data.clone());
        let Some(pos) = r.read_positions() else { continue };
        let pos: Vec<[f32; 3]> = pos.map(|p| apply(m, p)).collect();
        if !matches!(prim.mode(), Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan) {
            let idx: Vec<u32> = r.read_indices().map_or_else(|| (0..pos.len() as u32).collect(), |i| i.into_u32().collect());
            let at = |i: u32| pos.get(i as usize).copied();
            let pairs: Vec<[u32; 2]> = match prim.mode() {
                Mode::Lines => idx.as_chunks::<2>().0.to_vec(),
                Mode::LineStrip => idx.windows(2).map(|w| [w[0], w[1]]).collect(),
                Mode::LineLoop => idx.windows(2).map(|w| [w[0], w[1]]).chain(idx.first().zip(idx.last()).map(|(&a, &z)| [z, a])).collect(),
                Mode::Points => {
                    extra.1.extend(idx.iter().filter_map(|&i| Some((at(i)?, None))));
                    continue;
                }
                _ => continue,
            };
            extra.0.extend(pairs.iter().filter_map(|&[a, z]| Some((at(a)?, at(z)?, None))));
            continue;
        }
        let idx: Vec<u32> = match r.read_indices() {
            Some(i) => i.into_u32().collect(),
            None => (0..pos.len() as u32).collect(),
        };
        let tris: Vec<[u32; 3]> = match prim.mode() {
            Mode::TriangleStrip => idx.windows(3).enumerate().map(|(k, w)| if k % 2 == 0 { [w[0], w[1], w[2]] } else { [w[1], w[0], w[2]] }).collect(),
            Mode::TriangleFan => (1..idx.len().saturating_sub(1)).map(|k| [idx[0], idx[k], idx[k + 1]]).collect(),
            _ => idx.as_chunks::<3>().0.to_vec(),
        };
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
    fn files_next_to_the_model() {
        assert_eq!(sibling_path("phone/DCIM/a.gltf", "a%20b.bin").as_deref(), Some("phone/DCIM/a b.bin"));
        assert_eq!(sibling_path("d/a.obj", "./tex\\m.mtl").as_deref(), Some("d/tex/m.mtl"));
        for bad in ["../x.bin", "a/../../x", "/etc/passwd", "C:/x.bin", "https://h/x.bin", "%2", ""] {
            assert_eq!(sibling_path("d/a.gltf", bad), None, "{bad}");
        }
        // An OBJ with its materials: each group in the colour of its material.
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl red\nf 1 2 3\no b\nusemtl blue\nf 1 3 2\n";
        let mtl = |name: &str| (name == "m.mtl").then(|| b"newmtl red\nKd 1 0 0\nnewmtl blue\nKd 0 0 1\n".to_vec());
        let m = parse_with(obj.as_bytes(), "x.obj", &mtl).unwrap();
        assert_eq!(m.colors, vec![[255, 0, 0], [0, 0, 255]]);
        // Without the .mtl it still opens, uncoloured.
        assert!(parse(obj.as_bytes(), "x.obj").unwrap().colors.is_empty());
        // A .gltf whose triangle is in the .bin next to it.
        let gltf = r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"tri%20data.bin","byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
        let bin: Vec<u8> = [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let next = |name: &str| (name == "tri%20data.bin").then(|| bin.clone());
        assert_eq!(parse_with(gltf.as_bytes(), "t.gltf", &next).unwrap().tris.len(), 1);
        assert!(parse(gltf.as_bytes(), "t.gltf").is_err());
    }

    #[test]
    fn a_point_cloud_becomes_dots() {
        let ply = "ply\nformat ascii 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n0 0 0 255 0 0\n1 0 0 0 0 255\n";
        let m = parse(ply.as_bytes(), "scan.ply").unwrap();
        assert_eq!((m.tris.len(), m.colors[0], m.colors[7]), (8, [255, 0, 0], [0, 0, 255]));
    }

    #[test]
    fn stl_without_a_name_and_with_colours() {
        let ascii = b"solid\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n";
        assert_eq!(parse(ascii, "a.stl").unwrap().tris.len(), 1);
        // A binary file whose header starts with "solid", one red triangle (VisCAM: bit 15, red high).
        let mut bin = b"solid but binary".to_vec();
        bin.resize(80, 0);
        bin.extend(1u32.to_le_bytes());
        bin.extend([0.0f32, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|v| v.to_le_bytes()));
        bin.extend((0x8000u16 | (31 << 10)).to_le_bytes());
        let m = parse(&bin, "b.stl").unwrap();
        assert_eq!((m.tris.len(), m.colors[0]), (1, [255, 0, 0]));
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
        for sub in std::fs::read_dir(&dir).unwrap().flatten().filter(|s| s.path().is_dir()) {
            for f in std::fs::read_dir(sub.path()).unwrap().flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if !is_model(&name) {
                    continue;
                }
                let bytes = std::fs::read(f.path()).unwrap();
                let path = f.path().to_string_lossy().to_string();
                let next = |rel: &str| sibling_path(&path, rel).and_then(|p| std::fs::read(p).ok());
                let t = std::time::Instant::now();
                match parse_with(&bytes, &name, &next) {
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
