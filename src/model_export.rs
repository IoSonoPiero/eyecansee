//! A 3D model saved in another 3D format: STL (binary, with VisCAM colours), OBJ (colours on the
//! vertices), PLY (binary, colours on the faces) and GLB (glTF, colours on the vertices). Each
//! gets the axes its format expects: Z up for STL, Y up for the others.
// ponytail: no textures, normals or materials beyond one colour per triangle

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

/// The file's bytes.
pub fn export(mesh: &Mesh, kind: Kind) -> Vec<u8> {
    // Turned to the format's up axis: Z up for STL, Y up for the others.
    let turn = |p: [f32; 3]| match (kind == Kind::Stl, mesh.z_up) {
        (true, false) => [p[0], -p[2], p[1]],
        (false, true) => [p[0], p[2], -p[1]],
        _ => p,
    };
    let corner = |t: usize, k: usize| turn(mesh.pos[mesh.tris[t][k] as usize]);
    let color = |t: usize| mesh.colors.get(t).copied().unwrap_or(DEFAULT);
    match kind {
        Kind::Stl => stl(mesh, &corner, &color),
        Kind::Obj => obj(mesh, &corner, &color),
        Kind::Ply => ply(mesh, &turn, &color),
        Kind::Glb => glb(mesh, &corner, &color),
    }
}

fn floats(out: &mut Vec<u8>, v: &[f32]) {
    out.extend(v.iter().flat_map(|f| f.to_le_bytes()));
}

fn stl(mesh: &Mesh, corner: &dyn Fn(usize, usize) -> [f32; 3], color: &dyn Fn(usize) -> [u8; 3]) -> Vec<u8> {
    let mut out = b"EyeCanSee".to_vec();
    out.resize(80, 0);
    out.extend((mesh.tris.len() as u32).to_le_bytes());
    for t in 0..mesh.tris.len() {
        let [a, b, c] = [0, 1, 2].map(|k| corner(t, k));
        let (u, v) = ([0, 1, 2].map(|i| b[i] - a[i]), [0, 1, 2].map(|i| c[i] - a[i]));
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(f32::MIN_POSITIVE);
        floats(&mut out, &n.map(|x| x / len));
        for p in [a, b, c] {
            floats(&mut out, &p);
        }
        // VisCAM: bit 15 on, then 5 bits each of red, green, blue (blue lowest).
        let attr = if mesh.colors.is_empty() {
            0
        } else {
            let [r, g, b] = color(t).map(|v| u16::from(v) * 31 / 255);
            0x8000 | (r << 10) | (g << 5) | b
        };
        out.extend(attr.to_le_bytes());
    }
    out
}

fn obj(mesh: &Mesh, corner: &dyn Fn(usize, usize) -> [f32; 3], color: &dyn Fn(usize) -> [u8; 3]) -> Vec<u8> {
    use std::fmt::Write;
    let mut s = String::from("# EyeCanSee\n");
    // Each triangle its own corners: colours are per triangle, OBJ has them per vertex.
    for t in 0..mesh.tris.len() {
        let c = color(t).map(|v| f32::from(v) / 255.0);
        for k in 0..3 {
            let p = corner(t, k);
            let _ = if mesh.colors.is_empty() { writeln!(s, "v {} {} {}", p[0], p[1], p[2]) } else { writeln!(s, "v {} {} {} {:.4} {:.4} {:.4}", p[0], p[1], p[2], c[0], c[1], c[2]) };
        }
    }
    for t in 0..mesh.tris.len() {
        let _ = writeln!(s, "f {} {} {}", t * 3 + 1, t * 3 + 2, t * 3 + 3);
    }
    s.into_bytes()
}

fn ply(mesh: &Mesh, turn: &dyn Fn([f32; 3]) -> [f32; 3], color: &dyn Fn(usize) -> [u8; 3]) -> Vec<u8> {
    let colors = !mesh.colors.is_empty();
    let mut out = format!(
        "ply\nformat binary_little_endian 1.0\ncomment EyeCanSee\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nelement face {}\nproperty list uchar uint vertex_indices\n{}end_header\n",
        mesh.pos.len(),
        mesh.tris.len(),
        if colors { "property uchar red\nproperty uchar green\nproperty uchar blue\n" } else { "" }
    )
    .into_bytes();
    for p in &mesh.pos {
        floats(&mut out, &turn(*p));
    }
    for (t, tri) in mesh.tris.iter().enumerate() {
        out.push(3);
        out.extend(tri.iter().flat_map(|v| v.to_le_bytes()));
        if colors {
            out.extend(color(t));
        }
    }
    out
}

fn glb(mesh: &Mesh, corner: &dyn Fn(usize, usize) -> [f32; 3], color: &dyn Fn(usize) -> [u8; 3]) -> Vec<u8> {
    let n = mesh.tris.len() * 3;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut bin = Vec::with_capacity(n * 16);
    for t in 0..mesh.tris.len() {
        for k in 0..3 {
            let p = corner(t, k);
            for i in 0..3 {
                (lo[i], hi[i]) = (lo[i].min(p[i]), hi[i].max(p[i]));
            }
            floats(&mut bin, &p);
        }
    }
    let colors_at = bin.len();
    let with_colors = !mesh.colors.is_empty();
    if with_colors {
        for t in 0..mesh.tris.len() {
            // glTF colours are linear.
            let c = color(t).map(|v| (f32::from(v) / 255.0).powf(2.2));
            for _ in 0..3 {
                floats(&mut bin, &c);
            }
        }
    }
    let (views, accessors, attributes) = if with_colors {
        (
            format!(r#"[{{"buffer":0,"byteOffset":0,"byteLength":{colors_at}}},{{"buffer":0,"byteOffset":{colors_at},"byteLength":{}}}]"#, bin.len() - colors_at),
            format!(r#",{{"bufferView":1,"componentType":5126,"count":{n},"type":"VEC3"}}"#),
            r#","COLOR_0":1"#,
        )
    } else {
        (format!(r#"[{{"buffer":0,"byteOffset":0,"byteLength":{colors_at}}}]"#), String::new(), "")
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"EyeCanSee"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0{attributes}}}}}]}}],"buffers":[{{"byteLength":{}}}],"bufferViews":{views},"accessors":[{{"bufferView":0,"componentType":5126,"count":{n},"type":"VEC3","min":[{},{},{}],"max":[{},{},{}]}}{accessors}]}}"#,
        bin.len(),
        lo[0],
        lo[1],
        lo[2],
        hi[0],
        hi[1],
        hi[2]
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let mut out = Vec::with_capacity(28 + json.len() + bin.len());
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((12 + 8 + json.len() as u32 + 8 + bin.len() as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
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
}
