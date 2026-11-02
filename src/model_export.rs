//! A 3D model saved in another 3D format, with as much of the original as the format holds
//! (user, 2026-11-02): GLB (glTF) keeps the pictures, their coordinates, the normals and the
//! surfaces (metal, roughness, bumps, occlusion, glow); OBJ the normals and colours on the
//! vertices; PLY (binary) and STL (binary, VisCAM colours) a colour per triangle. Where the
//! format cannot hold a picture, its colours are taken from it. Each is written with the up axis
//! other programs expect, so that it opens standing (tried with Blender 4.5, 2026-11-02): Z for
//! STL and PLY (with a mark EyeCanSee reads, `model::PLY_Z_UP`), Y for OBJ and glTF.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::model::{DEFAULT, Mesh};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Stl,
    Obj,
    Ply,
    Glb,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Stl, Kind::Obj, Kind::Ply, Kind::Glb];

    pub fn ext(self) -> &'static str {
        match self {
            Kind::Stl => "stl",
            Kind::Obj => "obj",
            Kind::Ply => "ply",
            Kind::Glb => "glb",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Stl => "STL",
            Kind::Obj => "OBJ",
            Kind::Ply => "PLY",
            Kind::Glb => "glTF (GLB)",
        }
    }
}

/// The model as an exporter sees it: turned to the format's up axis, and the colours of the
/// triangles with their pictures in them.
struct View<'a> {
    mesh: &'a Mesh,
    /// Z up (STL, PLY), else Y up (OBJ, glTF).
    z_up: bool,
}

impl View<'_> {
    fn turn(&self, p: [f32; 3]) -> [f32; 3] {
        match (self.z_up, self.mesh.z_up) {
            (true, false) => [p[0], -p[2], p[1]],
            (false, true) => [p[0], p[2], -p[1]],
            _ => p,
        }
    }

    fn corner(&self, t: usize, k: usize) -> [f32; 3] {
        self.turn(self.mesh.pos[self.mesh.tris[t][k] as usize])
    }

    /// Normals at the corners: the file's, else the face's own.
    fn normals(&self, t: usize) -> [[f32; 3]; 3] {
        let face = face_normal([0, 1, 2].map(|k| self.corner(t, k)));
        let given = self.mesh.normals.get(t).copied().unwrap_or_default();
        given.map(|n| if n == [0.0; 3] { face } else { unit(self.turn(n)) })
    }

    /// Whether the colours are worth writing: the model has some, or pictures to take them from.
    fn colored(&self) -> bool {
        !self.mesh.colors.is_empty() || self.mesh.tex.iter().any(|&k| k > 0)
    }

    fn picture(&self, t: usize) -> Option<&image::RgbaImage> {
        let k = *self.mesh.tex.get(t)?;
        (k > 0).then(|| self.mesh.textures.get(k as usize - 1).map(|a| a.as_ref())).flatten()
    }

    /// The triangle's own colour, which tints its picture: white under a picture without one.
    fn tint(&self, t: usize) -> [u8; 3] {
        let white = self.picture(t).is_some();
        self.mesh.colors.get(t).copied().unwrap_or(if white { [255; 3] } else { DEFAULT })
    }

    /// The colour seen at `uv` of the triangle: its tint, times its picture there.
    fn seen(&self, t: usize, uv: [f32; 2]) -> [u8; 3] {
        let c = self.tint(t);
        match self.picture(t) {
            Some(img) => {
                let x = crate::render3d::texel(img, uv);
                std::array::from_fn(|j| (u16::from(c[j]) * u16::from(x[j]) / 255) as u8)
            }
            None => c,
        }
    }

    fn corner_colors(&self, t: usize) -> [[u8; 3]; 3] {
        match self.mesh.uv.get(t) {
            Some(uv) if self.picture(t).is_some() => uv.map(|c| self.seen(t, c)),
            _ => [self.tint(t); 3],
        }
    }

    /// One colour for the whole triangle: its corners and its middle averaged.
    fn face_color(&self, t: usize) -> [u8; 3] {
        let Some(uv) = self.mesh.uv.get(t).filter(|_| self.picture(t).is_some()) else { return self.tint(t) };
        let middle = [0, 1].map(|j| (uv[0][j] + uv[1][j] + uv[2][j]) / 3.0);
        let samples = [uv[0], uv[1], uv[2], middle].map(|c| self.seen(t, c));
        std::array::from_fn(|j| (samples.iter().map(|s| u32::from(s[j])).sum::<u32>() / 4) as u8)
    }
}

fn face_normal([a, b, c]: [[f32; 3]; 3]) -> [f32; 3] {
    let (u, v) = ([0, 1, 2].map(|i| b[i] - a[i]), [0, 1, 2].map(|i| c[i] - a[i]));
    unit([u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]])
}

fn unit(n: [f32; 3]) -> [f32; 3] {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(f32::MIN_POSITIVE);
    n.map(|x| x / len)
}

/// The file's bytes.
pub fn export(mesh: &Mesh, kind: Kind) -> Vec<u8> {
    let v = View { mesh, z_up: matches!(kind, Kind::Stl | Kind::Ply) };
    match kind {
        Kind::Stl => stl(&v),
        Kind::Obj => obj(&v),
        Kind::Ply => ply(&v),
        Kind::Glb => glb(&v),
    }
}

fn floats(out: &mut Vec<u8>, v: &[f32]) {
    out.extend(v.iter().flat_map(|f| f.to_le_bytes()));
}

fn stl(v: &View) -> Vec<u8> {
    let mut out = b"EyeCanSee".to_vec();
    out.resize(80, 0);
    out.extend((v.mesh.tris.len() as u32).to_le_bytes());
    let colored = v.colored();
    for t in 0..v.mesh.tris.len() {
        let corners = [0, 1, 2].map(|k| v.corner(t, k));
        floats(&mut out, &face_normal(corners));
        for p in corners {
            floats(&mut out, &p);
        }
        // VisCAM: bit 15 on, then 5 bits each of red, green, blue (blue lowest).
        let attr = if colored {
            let [r, g, b] = v.face_color(t).map(|c| u16::from(c) * 31 / 255);
            0x8000 | (r << 10) | (g << 5) | b
        } else {
            0
        };
        out.extend(attr.to_le_bytes());
    }
    out
}

fn obj(v: &View) -> Vec<u8> {
    let mut s = String::from("# EyeCanSee\n");
    let (colored, smooth) = (v.colored(), !v.mesh.normals.is_empty());
    // Each triangle its own corners: colours and normals are per corner.
    for t in 0..v.mesh.tris.len() {
        let colors = v.corner_colors(t);
        for (k, c) in colors.iter().enumerate() {
            let p = v.corner(t, k);
            let c = c.map(|x| f32::from(x) / 255.0);
            let _ = if colored { writeln!(s, "v {} {} {} {:.4} {:.4} {:.4}", p[0], p[1], p[2], c[0], c[1], c[2]) } else { writeln!(s, "v {} {} {}", p[0], p[1], p[2]) };
        }
        if smooth {
            for n in v.normals(t) {
                let _ = writeln!(s, "vn {:.5} {:.5} {:.5}", n[0], n[1], n[2]);
            }
        }
    }
    for t in 0..v.mesh.tris.len() {
        let [a, b, c] = [1, 2, 3].map(|k| t * 3 + k);
        let _ = if smooth { writeln!(s, "f {a}//{a} {b}//{b} {c}//{c}") } else { writeln!(s, "f {a} {b} {c}") };
    }
    s.into_bytes()
}

fn ply(v: &View) -> Vec<u8> {
    let colored = v.colored();
    let mut out = format!(
        "ply\nformat binary_little_endian 1.0\n{}\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nelement face {}\nproperty list uchar uint vertex_indices\n{}end_header\n",
        crate::model::PLY_Z_UP,
        v.mesh.pos.len(),
        v.mesh.tris.len(),
        if colored { "property uchar red\nproperty uchar green\nproperty uchar blue\n" } else { "" }
    )
    .into_bytes();
    for p in &v.mesh.pos {
        floats(&mut out, &v.turn(*p));
    }
    for (t, tri) in v.mesh.tris.iter().enumerate() {
        out.push(3);
        out.extend(tri.iter().flat_map(|i| i.to_le_bytes()));
        if colored {
            out.extend(v.face_color(t));
        }
    }
    out
}

/// The binary part of a GLB and what points into it.
#[derive(Default)]
struct Bin {
    bytes: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

impl Bin {
    fn view(&mut self, data: &[u8]) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        self.views.push(format!(r#"{{"buffer":0,"byteOffset":{},"byteLength":{}}}"#, self.bytes.len(), data.len()));
        self.bytes.extend(data);
        self.views.len() - 1
    }

    /// Floats `n` at a time as an accessor (VEC2 or VEC3); positions with their box, as glTF asks.
    fn accessor(&mut self, data: &[f32], n: usize, bounds: bool) -> usize {
        let mut raw = Vec::with_capacity(data.len() * 4);
        floats(&mut raw, data);
        let view = self.view(&raw);
        let kind = if n == 2 { "VEC2" } else { "VEC3" };
        let mut a = format!(r#"{{"bufferView":{view},"componentType":5126,"count":{},"type":"{kind}""#, data.len() / n);
        if bounds {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in data.as_chunks::<3>().0 {
                for i in 0..3 {
                    (lo[i], hi[i]) = (lo[i].min(p[i]), hi[i].max(p[i]));
                }
            }
            let _ = write!(a, r#","min":[{},{},{}],"max":[{},{},{}]"#, lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
        }
        a.push('}');
        self.accessors.push(a);
        self.accessors.len() - 1
    }
}

/// The triangles grouped by picture and surface: one glTF primitive each, with its material.
fn glb(v: &View) -> Vec<u8> {
    let m = v.mesh;
    let mut groups: BTreeMap<(u16, u16), Vec<usize>> = BTreeMap::new();
    for t in 0..m.tris.len() {
        groups.entry((m.tex.get(t).copied().unwrap_or(0), m.surface.get(t).copied().unwrap_or(0))).or_default().push(t);
    }
    let mut bin = Bin::default();
    let (mut primitives, mut materials) = (Vec::new(), Vec::new());
    for (&(tex, surface), tris) in &groups {
        let attributes = glb_attributes(v, &mut bin, tris);
        let material = if tex > 0 || surface > 0 {
            materials.push(glb_material(m, tex, surface));
            format!(r#","material":{}"#, materials.len() - 1)
        } else {
            String::new()
        };
        primitives.push(format!(r#"{{"attributes":{{{attributes}}}{material}}}"#));
    }
    // The pictures as PNG, each a texture (repeating, as the viewers draw them).
    let mut images = Vec::new();
    for img in &m.textures {
        let mut png = Vec::new();
        if img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_ok() {
            images.push(format!(r#"{{"bufferView":{},"mimeType":"image/png"}}"#, bin.view(&png)));
        }
    }
    let pictures = if images.len() == m.textures.len() && !images.is_empty() {
        let textures: Vec<String> = (0..images.len()).map(|i| format!(r#"{{"source":{i}}}"#)).collect();
        format!(r#","textures":[{}],"images":[{}]"#, textures.join(","), images.join(","))
    } else {
        String::new()
    };
    let materials = if materials.is_empty() { String::new() } else { format!(r#","materials":[{}]"#, materials.join(",")) };
    while !bin.bytes.len().is_multiple_of(4) {
        bin.bytes.push(0);
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"EyeCanSee"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{}]}}]{materials}{pictures},"buffers":[{{"byteLength":{}}}],"bufferViews":[{}],"accessors":[{}]}}"#,
        primitives.join(","),
        bin.bytes.len(),
        bin.views.join(","),
        bin.accessors.join(",")
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut out = Vec::with_capacity(28 + json.len() + bin.bytes.len());
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((12 + 8 + json.len() as u32 + 8 + bin.bytes.len() as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out.extend((bin.bytes.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin.bytes);
    out
}

/// Positions, normals, picture coordinates and colours of the triangles `tris`, each its own
/// corners: the `attributes` of their primitive.
fn glb_attributes(v: &View, bin: &mut Bin, tris: &[usize]) -> String {
    let m = v.mesh;
    let (mut pos, mut normals, mut uvs, mut colors) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for &t in tris {
        pos.extend((0..3).flat_map(|k| v.corner(t, k)));
        if !m.normals.is_empty() {
            normals.extend(v.normals(t).into_iter().flatten());
        }
        if let Some(uv) = m.uv.get(t) {
            uvs.extend(uv.iter().flatten());
        }
        // The triangle's own colour (glTF tints the picture with it), linear.
        if !m.colors.is_empty() {
            let c = v.tint(t).map(|x| (f32::from(x) / 255.0).powf(2.2));
            colors.extend((0..3).flat_map(|_| c));
        }
    }
    let mut a = format!(r#""POSITION":{}"#, bin.accessor(&pos, 3, true));
    for (name, data, n) in [("NORMAL", &normals, 3), ("TEXCOORD_0", &uvs, 2), ("COLOR_0", &colors, 3)] {
        if !data.is_empty() {
            let _ = write!(a, r#","{name}":{}"#, bin.accessor(data, n, false));
        }
    }
    a
}

/// The material of a picture (`tex`, 0 = none) and a surface (0 = matte): glTF's metallic-roughness.
fn glb_material(m: &Mesh, tex: u16, surface: u16) -> String {
    let s = (surface as usize).checked_sub(1).and_then(|i| m.surfaces.get(i)).copied().unwrap_or_default();
    let pic = |k: u16| (k > 0 && (k as usize) <= m.textures.len()).then(|| k - 1);
    let mut pbr = format!(r#""metallicFactor":{},"roughnessFactor":{}"#, s.metallic, s.roughness);
    if let Some(i) = pic(tex) {
        let _ = write!(pbr, r#","baseColorTexture":{{"index":{i}}}"#);
    }
    if let Some(i) = pic(s.rough_metal) {
        let _ = write!(pbr, r#","metallicRoughnessTexture":{{"index":{i}}}"#);
    }
    let mut out = format!(r#"{{"pbrMetallicRoughness":{{{pbr}}}"#);
    if let Some(i) = pic(s.bumps) {
        let _ = write!(out, r#","normalTexture":{{"index":{i}}}"#);
    }
    if let Some(i) = pic(s.occlusion_map) {
        let _ = write!(out, r#","occlusionTexture":{{"index":{i},"strength":{}}}"#, s.occlusion);
    }
    if s.glow != [0.0; 3] {
        // Read as sRGB (`model_gltf::surface`): back to linear.
        let [r, g, b] = s.glow.map(|x| x.powf(2.2));
        let _ = write!(out, r#","emissiveFactor":[{r},{g},{b}]"#);
    }
    if let Some(i) = pic(s.glow_map) {
        let _ = write!(out, r#","emissiveTexture":{{"index":{i}}}"#);
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each format read back by EyeCanSee gives the same triangles and colours.
    #[test]
    fn written_and_read_back() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n";
        let mut m = crate::model::parse(obj, "q.obj").unwrap();
        m.colors = vec![[255, 0, 0], [0, 0, 255]];
        for kind in Kind::ALL {
            let back = crate::model::parse(&export(&m, kind), &format!("x.{}", kind.ext())).unwrap();
            assert_eq!(back.tris.len(), 2, "{kind:?}");
            assert_eq!(back.colors.len(), 2, "{kind:?}");
            // STL keeps 5 bits a channel.
            assert!(back.colors[0][0] > 240 && back.colors[1][2] > 240, "{kind:?} {:?}", back.colors);
            let size = |m: &Mesh| (m.radius * 1000.0).round();
            assert_eq!(size(&back), size(&m), "{kind:?}");
        }
    }

    /// Up stays up in every format: a model read from an STL (Z up) and from a glTF (Y up),
    /// exported and read back, has each point where it was, seen with Y up.
    #[test]
    fn axes_kept() {
        // A point high above the others: up is +Z in the STL, +Y in the OBJ.
        let tall = |z_up: bool| {
            let up = |h: f32| if z_up { [0.0, 0.0, h] } else { [0.0, h, 0.0] };
            format!("v 0 0 0\nv 1 0 0\nv {} {} {}\nf 1 2 3\n", up(5.0)[0], up(5.0)[1], up(5.0)[2])
        };
        for z_up in [true, false] {
            let mut m = crate::model::parse(tall(z_up).as_bytes(), "q.obj").unwrap();
            m.z_up = z_up;
            let seen = |m: &Mesh| m.pos.iter().map(|p| if m.z_up { [p[0], p[2], -p[1]] } else { *p }).collect::<Vec<_>>();
            for kind in Kind::ALL {
                let back = crate::model::parse(&export(&m, kind), &format!("x.{}", kind.ext())).unwrap();
                let (a, b) = (seen(&m), seen(&back));
                let top = |v: &[[f32; 3]]| v.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
                assert_eq!(top(&a), top(&b), "{kind:?} z_up {z_up}: {a:?} -> {b:?}");
                assert!(b.iter().any(|p| (p[0] - 1.0).abs() < 1e-6), "{kind:?} z_up {z_up}: {b:?}");
                // Each format with the up axis other programs expect: Z for STL and PLY.
                let format_z = matches!(kind, Kind::Stl | Kind::Ply);
                assert_eq!(back.z_up, format_z, "{kind:?}");
                if format_z == z_up {
                    assert_eq!(back.pos, m.pos, "{kind:?}");
                }
            }
        }
    }

    /// Every model under `EYECANSEE_MODELS` (subfolders too) exported in each format and read
    /// back: triangles, pictures and coloured triangles before and after.
    #[test]
    #[ignore]
    fn model_export_live() {
        let Ok(dir) = std::env::var("EYECANSEE_MODELS") else { return };
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for f in std::fs::read_dir(&d).unwrap().flatten() {
                let path = f.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path.to_string_lossy().to_string();
                if !crate::model::is_model(&name) || crate::occt::is_occt(&name) {
                    continue;
                }
                let next = |rel: &str| crate::model::sibling_path(&name, rel).and_then(|p| std::fs::read(p).ok());
                let Ok(m) = crate::model::parse_with(&std::fs::read(&path).unwrap(), &name, &next) else { continue };
                let colored = |m: &Mesh| m.colors.iter().filter(|c| **c != DEFAULT).count();
                print!("{}: {} tris, {} pictures, {} coloured", f.file_name().to_string_lossy(), m.tris.len(), m.textures.len(), colored(&m));
                for kind in Kind::ALL {
                    match crate::model::parse(&export(&m, kind), &format!("x.{}", kind.ext())) {
                        Ok(b) => print!(" | {}: {} tris, {} pictures, {} coloured", kind.ext(), b.tris.len(), b.textures.len(), colored(&b)),
                        Err(e) => print!(" | {}: ERROR {e}", kind.ext()),
                    }
                }
                println!();
            }
        }
    }

    /// Every model in `EYECANSEE_AXES` (one folder): its up axis as read, and its size along X, Y
    /// and Z as stored and as shown (Y up), to compare a model with its exports.
    #[test]
    #[ignore]
    fn axes_live() {
        let Ok(dir) = std::env::var("EYECANSEE_AXES") else { return };
        for f in std::fs::read_dir(dir).unwrap().flatten() {
            let name = f.path().to_string_lossy().to_string();
            if !crate::model::is_model(&name) {
                continue;
            }
            match crate::model::parse(&std::fs::read(f.path()).unwrap(), &name) {
                Ok(m) => {
                    let size = |v: &[[f32; 3]]| [0, 1, 2].map(|k| (v.iter().map(|p| p[k]).fold(f32::MIN, f32::max) - v.iter().map(|p| p[k]).fold(f32::MAX, f32::min)).round());
                    let shown: Vec<[f32; 3]> = m.pos.iter().map(|p| if m.z_up { [p[0], p[2], -p[1]] } else { *p }).collect();
                    println!("{}: z_up {}, stored {:?}, shown {:?}", f.file_name().to_string_lossy(), m.z_up, size(&m.pos), size(&shown));
                    for kind in Kind::ALL {
                        let data = export(&m, kind);
                        // Also written to `EYECANSEE_AXES_OUT`, to open in other programs.
                        if let Ok(out) = std::env::var("EYECANSEE_AXES_OUT") {
                            std::fs::write(format!("{out}/{}.{}", f.file_name().to_string_lossy(), kind.ext()), &data).unwrap();
                        }
                        let b = crate::model::parse(&data, &format!("x.{}", kind.ext())).unwrap();
                        println!("    exported as {}: z_up {}, stored {:?}", kind.ext(), b.z_up, size(&b.pos));
                    }
                }
                Err(e) => println!("{}: ERROR {e}", f.file_name().to_string_lossy()),
            }
        }
    }

    /// A model with a picture (the avocado, 2026-11-02): GLB keeps it, with its coordinates and
    /// the surface; the other formats get its colours.
    #[test]
    fn a_picture_kept_or_turned_into_colours() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n";
        let mut m = crate::model::parse(obj, "q.obj").unwrap();
        // Left half green, right half red; the first triangle on the green half, the second on the red.
        let pic = image::RgbaImage::from_fn(4, 4, |x, _| if x < 2 { image::Rgba([0, 200, 0, 255]) } else { image::Rgba([200, 0, 0, 255]) });
        m.textures = vec![std::sync::Arc::new(pic)];
        m.tex = vec![1, 1];
        m.uv = vec![[[0.1, 0.1], [0.2, 0.1], [0.2, 0.9]], [[0.8, 0.1], [0.9, 0.1], [0.9, 0.9]]];
        m.surfaces = vec![crate::model::Surface { metallic: 0.5, roughness: 0.25, ..Default::default() }];
        m.surface = vec![1, 1];
        m.normals = vec![[[0.0, 0.0, 1.0]; 3]; 2];
        let back = crate::model::parse(&export(&m, Kind::Glb), "x.glb").unwrap();
        assert_eq!((back.textures.len(), back.tex.clone()), (1, vec![1, 1]));
        assert_eq!(back.uv, m.uv);
        assert_eq!(back.textures[0].get_pixel(3, 0).0, [200, 0, 0, 255]);
        assert_eq!(back.surfaces[back.surface[0] as usize - 1].roughness, 0.25);
        for kind in [Kind::Stl, Kind::Obj, Kind::Ply] {
            let back = crate::model::parse(&export(&m, kind), &format!("x.{}", kind.ext())).unwrap();
            let [green, red] = [back.colors[0], back.colors[1]];
            assert!(green[1] > 150 && green[0] < 30 && red[0] > 150 && red[1] < 30, "{kind:?} {:?}", back.colors);
        }
    }
}
