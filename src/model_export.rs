//! A 3D model saved in another 3D format, with as much of the original as the format holds:
//! GLB (glTF) keeps the pictures, their coordinates, the normals and the
//! surfaces (metal, roughness, bumps, occlusion, glow); OBJ the normals and colours on the
//! vertices, and the pictures and materials in a `.mtl` next to it (`model_export_obj`); PLY
//! (binary) the normals and the colours on the points and on the triangles; STL (binary, VisCAM
//! colours) a colour per triangle. Where the
//! format cannot hold a picture, its colours are taken from it. Each is written with the up axis
//! other programs expect, so that it opens standing (tried with Blender 4.5, 2026-11-02): Z for
//! STL and PLY (with a mark EyeCanSee reads, `model_ply::PLY_Z_UP`), Y for OBJ and glTF. glTF counts
//! in metres: a model whose unit is known is converted; the others keep their numbers and say the
//! unit in their header (`unit_line`), which other programs take as a comment.

use std::collections::{BTreeMap, HashMap};
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

    /// The format of a file written with the extension `ext`.
    pub fn of_ext(ext: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| ext.eq_ignore_ascii_case(k.ext()))
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
pub(crate) struct View<'a> {
    pub(crate) mesh: &'a Mesh,
    /// Z up (STL, PLY), else Y up (OBJ, glTF).
    z_up: bool,
    /// What the positions are multiplied by: to metres for glTF, else 1.
    pub(crate) scale: f32,
}

impl View<'_> {
    pub(crate) fn turn(&self, p: [f32; 3]) -> [f32; 3] {
        match (self.z_up, self.mesh.z_up) {
            (true, false) => [p[0], -p[2], p[1]],
            (false, true) => [p[0], p[2], -p[1]],
            _ => p,
        }
    }

    pub(crate) fn corner(&self, t: usize, k: usize) -> [f32; 3] {
        self.turn(self.mesh.pos[self.mesh.tris[t][k] as usize]).map(|c| c * self.scale)
    }

    /// Normals at the corners: the file's, else the face's own.
    pub(crate) fn normals(&self, t: usize) -> [[f32; 3]; 3] {
        let face = face_normal([0, 1, 2].map(|k| self.corner(t, k)));
        let given = self.mesh.normals.get(t).copied().unwrap_or_default();
        given.map(|n| if n == [0.0; 3] { face } else { unit(self.turn(n)) })
    }

    /// Whether the colours are worth writing: the model has some, or pictures to take them from.
    pub(crate) fn colored(&self) -> bool {
        !self.mesh.colors.is_empty() || self.mesh.tex.iter().any(|&k| k > 0)
    }

    pub(crate) fn picture(&self, t: usize) -> Option<&image::RgbaImage> {
        let k = *self.mesh.tex.get(t)?;
        (k > 0).then(|| self.mesh.textures.get(k as usize - 1).map(|a| a.as_ref())).flatten()
    }

    /// The triangle's own colour, which tints its picture: white under a picture without one.
    pub(crate) fn tint(&self, t: usize) -> [u8; 3] {
        let white = self.picture(t).is_some();
        self.mesh.colors.get(t).copied().unwrap_or(if white { [255; 3] } else { DEFAULT })
    }

    /// The colour seen at `uv` of the triangle: its tint, times its picture there.
    pub(crate) fn seen(&self, t: usize, uv: [f32; 2]) -> [u8; 3] {
        let c = self.tint(t);
        match self.picture(t) {
            Some(img) => {
                let x = crate::render3d::texel(img, uv);
                std::array::from_fn(|j| (u16::from(c[j]) * u16::from(x[j]) / 255) as u8)
            }
            None => c,
        }
    }

    pub(crate) fn corner_colors(&self, t: usize) -> [[u8; 3]; 3] {
        match self.mesh.uv.get(t) {
            Some(uv) if self.picture(t).is_some() => uv.map(|c| self.seen(t, c)),
            _ => [self.tint(t); 3],
        }
    }

    /// One colour for the whole triangle: its corners and its middle averaged.
    pub(crate) fn face_color(&self, t: usize) -> [u8; 3] {
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

/// The model in `bytes` (named `name`, the files next to it read through `sibling`) written as
/// `kind`, with what was read differently from its file (`Mesh::warnings`): the export says the
/// new file has it so (#179); and `model_limit::BEYOND_EXPORT` when the new file has more
/// triangles than EyeCanSee opens in its format.
/// `tick` is told how far the writing is (0 to 1, from 0 once the model is read) and stops it
/// with `false`: then the error is `model_export_job::CANCELLED` (#312).
pub fn export_file(bytes: &[u8], name: &str, sibling: crate::model::Sibling, kind: Kind, tick: Tick, beside: &Beside) -> Result<(Made, Vec<&'static str>), String> {
    let mesh = crate::model::parse_with(bytes, name, sibling)?;
    let mut notes = mesh.warnings.clone();
    if crate::model_limit::of_export(kind).check(mesh.tris.len()).is_err() {
        notes.push(crate::model_limit::BEYOND_EXPORT);
    }
    notes.extend(lost(&mesh, kind));
    let cancelled = || crate::model_export_job::CANCELLED.to_string();
    if !tick(0.0) {
        return Err(cancelled());
    }
    Ok((export_with(&mesh, kind, tick, beside).ok_or_else(cancelled)?, notes))
}

/// The files an export makes: the model, and the files next to it (an OBJ's `.mtl` and
/// pictures), each with its name.
#[derive(Debug)]
pub struct Made {
    pub main: Vec<u8>,
    pub beside: Vec<(String, Vec<u8>)>,
}

/// How the files next to the new one are named: after it (`stem`, its name without the
/// extension), each with a free name for the one wanted (`free`).
pub struct Beside<'a> {
    pub stem: &'a str,
    pub free: &'a dyn Fn(&str) -> String,
}

/// A model exported into a format that cannot hold its pictures: their colours are on it instead.
pub(crate) const LOST_PICTURES: &str = "model-export-lost-pictures";
/// A model exported into a format that cannot hold all of how its surfaces take the light.
pub(crate) const LOST_SURFACES: &str = "model-export-lost-surfaces";

/// What `kind` cannot hold of `mesh`, said at the end of the export (audit 6.16, the project's
/// rule): STL and PLY have no place for pictures (their colours are put on the model) nor for
/// surfaces; a `.mtl` has none for the pictures of roughness and metal and of the shadows in the
/// creases. glTF holds all a `Mesh` has.
/// A surface counts when it shows: smoother than matte, glowing, or with pictures; a glTF
/// without a material (metal, as rough as matte, by the format's default) is not named.
pub(crate) fn lost(mesh: &Mesh, kind: Kind) -> Vec<&'static str> {
    let s = &mesh.surfaces;
    let shiny = s.iter().any(|s| s.roughness < 1.0 || s.glow != [0.0; 3] || s.bumps > 0 || s.rough_metal > 0 || s.occlusion_map > 0 || s.glow_map > 0);
    let maps = s.iter().any(|s| s.rough_metal > 0 || s.occlusion_map > 0);
    let pictures = mesh.tex.iter().any(|&k| k > 0);
    match kind {
        Kind::Stl | Kind::Ply => [(pictures, LOST_PICTURES), (shiny, LOST_SURFACES)].into_iter().filter_map(|(lost, id)| lost.then_some(id)).collect(),
        Kind::Obj if maps => vec![LOST_SURFACES],
        Kind::Obj | Kind::Glb => Vec::new(),
    }
}

/// Told how far an export is, 0 to 1; `false` stops it (`export_file`).
pub type Tick<'a> = &'a dyn Fn(f32) -> bool;

/// How often the writers tell how far they are: every this many triangles.
pub(crate) const TICK_EVERY: usize = 1 << 16;

/// `tick` at the triangle `t` of `n`, every `TICK_EVERY`: `false` when it says to stop.
pub(crate) fn ticked(tick: Tick, t: usize, n: usize) -> bool {
    !t.is_multiple_of(TICK_EVERY) || tick(t as f32 / n.max(1) as f32)
}

/// How many metres a unit of `model::Mesh::unit` is.
fn metres(unit: &str) -> Option<f32> {
    Some(match unit {
        "µm" => 1e-6,
        "mm" => 1e-3,
        "cm" => 1e-2,
        "m" => 1.0,
        "in" => 0.0254,
        "ft" => 0.3048,
        _ => return None,
    })
}

/// The unit in the header of a format without a place for it (STL, OBJ, PLY), after its mark of
/// comment; nothing when the model does not say it.
pub(crate) fn unit_line(mesh: &Mesh, comment: &str) -> String {
    mesh.unit.map(|u| format!("{comment} unit: {u}\n")).unwrap_or_default()
}

/// The file's bytes (tests).
#[cfg(test)]
pub fn export(mesh: &Mesh, kind: Kind) -> Vec<u8> {
    export_made(mesh, kind).main
}

/// The files, those next to it named after "x" (tests).
#[cfg(test)]
pub fn export_made(mesh: &Mesh, kind: Kind) -> Made {
    export_with(mesh, kind, &|_| true, &Beside { stem: "x", free: &|n| n.to_string() }).unwrap()
}

/// The files of `mesh` as `kind`, telling `tick` how far it is; `None` when `tick` stopped it.
pub fn export_with(mesh: &Mesh, kind: Kind, tick: Tick, beside: &Beside) -> Option<Made> {
    let scale = match (kind, mesh.unit) {
        (Kind::Glb, Some(u)) => metres(u).unwrap_or(1.0),
        _ => 1.0,
    };
    let v = View { mesh, z_up: matches!(kind, Kind::Stl | Kind::Ply), scale };
    let alone = |main: Option<Vec<u8>>| main.map(|main| Made { main, beside: Vec::new() });
    match kind {
        Kind::Stl => alone(stl(&v, tick)),
        Kind::Obj => crate::model_export_obj::obj(&v, tick, beside),
        Kind::Ply => alone(ply(&v, tick)),
        Kind::Glb => alone(glb(&v, tick)),
    }
}

pub(crate) fn floats(out: &mut Vec<u8>, v: &[f32]) {
    out.extend(v.iter().flat_map(|f| f.to_le_bytes()));
}

fn stl(v: &View, tick: Tick) -> Option<Vec<u8>> {
    // The header is free text: the unit in it, as in OBJ and PLY.
    let mut out = format!("EyeCanSee{}", v.mesh.unit.map(|u| format!(", unit: {u}")).unwrap_or_default()).into_bytes();
    out.resize(80, 0);
    out.extend((v.mesh.tris.len() as u32).to_le_bytes());
    let colored = v.colored();
    for t in 0..v.mesh.tris.len() {
        if !ticked(tick, t, v.mesh.tris.len()) {
            return None;
        }
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
    Some(out)
}

/// Binary PLY (audit 6.16): the points shared by the triangles, split only where their normals
/// differ; normals on the points when the model has them; colours on the points (the average of
/// what its corners show, which Blender reads) and on the faces (each triangle's own, which
/// EyeCanSee and MeshLab read).
fn ply(v: &View, tick: Tick) -> Option<Vec<u8>> {
    let m = v.mesh;
    let n = m.tris.len();
    let (colored, smooth) = (v.colored(), !m.normals.is_empty());
    // Three rounds over the triangles: the points, their colours, the faces; each a third of the way.
    let third = |t: usize, round: u8| !t.is_multiple_of(TICK_EVERY) || tick((t as f32 / n.max(1) as f32 + f32::from(round)) / 3.0);
    // Each point: its position (an index in `pos`) and its normal.
    let mut points: Vec<(u32, [f32; 3])> = Vec::new();
    let mut corners: Vec<[u32; 3]> = Vec::new();
    if smooth {
        let mut seen: HashMap<(u32, [u32; 3]), u32> = HashMap::new();
        corners.reserve(n);
        for (t, tri) in m.tris.iter().enumerate() {
            if !third(t, 0) {
                return None;
            }
            let normals = v.normals(t);
            corners.push(std::array::from_fn(|k| {
                let key = (tri[k], normals[k].map(f32::to_bits));
                *seen.entry(key).or_insert_with(|| {
                    points.push((key.0, normals[k]));
                    points.len() as u32 - 1
                })
            }));
        }
    }
    let count = if smooth { points.len() } else { m.pos.len() };
    let corner = |t: usize| if smooth { corners[t] } else { m.tris[t] };
    let mut sums = if colored { vec![[0u32; 4]; count] } else { Vec::new() };
    for t in (0..n).filter(|_| colored) {
        if !third(t, 1) {
            return None;
        }
        for (k, c) in v.corner_colors(t).into_iter().enumerate() {
            let s = &mut sums[corner(t)[k] as usize];
            *s = [s[0] + u32::from(c[0]), s[1] + u32::from(c[1]), s[2] + u32::from(c[2]), s[3] + 1];
        }
    }
    let rgb = "property uchar red\nproperty uchar green\nproperty uchar blue\n";
    let mut out = format!(
        "ply\nformat binary_little_endian 1.0\n{}\n{}element vertex {count}\nproperty float x\nproperty float y\nproperty float z\n{}{}element face {n}\nproperty list uchar uint vertex_indices\n{}end_header\n",
        crate::model_ply::PLY_Z_UP,
        unit_line(m, "comment"),
        if smooth { "property float nx\nproperty float ny\nproperty float nz\n" } else { "" },
        if colored { rgb } else { "" },
        if colored { rgb } else { "" },
    )
    .into_bytes();
    for i in 0..count {
        let (at, normal) = if smooth { points[i] } else { (i as u32, [0.0; 3]) };
        floats(&mut out, &v.turn(m.pos[at as usize]).map(|c| c * v.scale));
        if smooth {
            // Turned already (`View::normals`).
            floats(&mut out, &normal);
        }
        if colored {
            let s = sums[i];
            out.extend((0..3).map(|j| (s[j] / s[3].max(1)) as u8));
        }
    }
    for t in 0..n {
        if !third(t, 2) {
            return None;
        }
        out.push(3);
        out.extend(corner(t).iter().flat_map(|i| i.to_le_bytes()));
        if colored {
            out.extend(v.face_color(t));
        }
    }
    Some(out)
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
fn glb(v: &View, tick: Tick) -> Option<Vec<u8>> {
    let m = v.mesh;
    let mut groups: BTreeMap<(u16, u16), Vec<usize>> = BTreeMap::new();
    for t in 0..m.tris.len() {
        groups.entry((m.tex.get(t).copied().unwrap_or(0), m.surface.get(t).copied().unwrap_or(0))).or_default().push(t);
    }
    let mut bin = Bin::default();
    let (mut primitives, mut materials) = (Vec::new(), Vec::new());
    let mut written = 0;
    for (&(tex, surface), tris) in &groups {
        let attributes = glb_attributes(v, &mut bin, tris, (written, tick))?;
        written += tris.len();
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
    Some(out)
}

/// Positions, normals, picture coordinates and colours of the triangles `tris`, each its own
/// corners: the `attributes` of their primitive.
/// `tick` told how far, the triangles before these being `before`; `None` when it stops.
fn glb_attributes(v: &View, bin: &mut Bin, tris: &[usize], (before, tick): (usize, Tick)) -> Option<String> {
    let m = v.mesh;
    let (mut pos, mut normals, mut uvs, mut colors) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, &t) in tris.iter().enumerate() {
        if !ticked(tick, before + i, m.tris.len()) {
            return None;
        }
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
    Some(a)
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
pub(crate) mod tests {
    use super::*;

    /// Audit 6.5: glTF counts in metres. A 3MF in millimetres, a triangle 100 mm wide, exported as
    /// GLB and read back measures 0.1 m; STL, OBJ and PLY, without a place for the unit, keep the
    /// numbers and say the unit in their header.
    #[test]
    fn audit_6_5_unit_kept() {
        use std::io::Write;
        let model = r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>
            <object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="100" y="0" z="0"/><vertex x="0" y="50" z="0"/></vertices>
            <triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        w.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(model.as_bytes()).unwrap();
        let m = crate::model::parse(&w.finish().unwrap().into_inner(), "part.3mf").unwrap();
        assert_eq!(m.unit, Some("mm"));
        let back = crate::model::parse(&export(&m, Kind::Glb), "part.glb").unwrap();
        assert_eq!(back.unit, Some("m"));
        let size = crate::model_measure::sizes(&back).size;
        let widest = size.iter().copied().fold(0.0, f64::max);
        assert!((widest - 0.1).abs() < 1e-6, "{size:?}");
        for kind in [Kind::Stl, Kind::Obj, Kind::Ply] {
            let out = export(&m, kind);
            let head = String::from_utf8_lossy(&out[..out.len().min(200)]).to_string();
            assert!(head.contains("unit: mm"), "{kind:?}: {head:?}");
            let back = crate::model::parse(&out, &format!("x.{}", kind.ext())).unwrap();
            assert_eq!(crate::model_measure::sizes(&back).size.iter().copied().fold(0.0, f64::max), 100.0, "{kind:?}");
        }
    }

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
                    let made = export_made(&m, kind);
                    let beside = |rel: &str| made.beside.iter().find(|(n, _)| n == rel).map(|(_, d)| d.clone());
                    match crate::model::parse_with(&made.main, &format!("x.{}", kind.ext()), &beside) {
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
    /// the surface; STL and PLY get its colours (OBJ keeps it: `audit_6_16_obj_keeps_the_picture`).
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
        for kind in [Kind::Stl, Kind::Ply] {
            let back = crate::model::parse(&export(&m, kind), &format!("x.{}", kind.ext())).unwrap();
            let [green, red] = [back.colors[0], back.colors[1]];
            assert!(green[1] > 150 && green[0] < 30 && red[0] > 150 && red[1] < 30, "{kind:?} {:?}", back.colors);
        }
    }

    /// The quad of `a_picture_kept_or_turned_into_colours`: a picture, green on the left and red
    /// on the right, a shiny surface and the normals.
    pub(crate) fn pictured() -> Mesh {
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n";
        let mut m = crate::model::parse(obj, "q.obj").unwrap();
        let pic = image::RgbaImage::from_fn(4, 4, |x, _| if x < 2 { image::Rgba([0, 200, 0, 255]) } else { image::Rgba([200, 0, 0, 255]) });
        m.textures = vec![std::sync::Arc::new(pic)];
        m.tex = vec![1, 1];
        m.uv = vec![[[0.1, 0.1], [0.2, 0.1], [0.2, 0.9]], [[0.8, 0.1], [0.9, 0.1], [0.9, 0.9]]];
        m.surfaces = vec![crate::model::Surface { metallic: 0.5, roughness: 0.25, ..Default::default() }];
        m.surface = vec![1, 1];
        m.normals = vec![[[0.0, 0.0, 1.0]; 3]; 2];
        m
    }

    /// Audit 6.16: OBJ keeps the picture as other programs read it, with its coordinates (`vt`)
    /// and a `.mtl` that names it; the points the triangles share are written once (each
    /// triangle had its own: the surface came apart).
    #[test]
    fn audit_6_16_obj_keeps_the_picture() {
        let text = String::from_utf8(export(&pictured(), Kind::Obj)).unwrap();
        for line in ["\nmtllib ", "\nvt ", "\nusemtl "] {
            assert!(text.contains(line), "no {line:?} in\n{text}");
        }
        assert_eq!(text.lines().filter(|l| l.starts_with("v ")).count(), 4, "a quad has 4 points:\n{text}");
        // Read back with the files next to it: the picture, its coordinates and the surface.
        let m = pictured();
        let made = export_made(&m, Kind::Obj);
        let names: Vec<&str> = made.beside.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["x.png", "x.mtl"]);
        let mtl = String::from_utf8_lossy(&made.beside[1].1).to_string();
        for line in ["\nmap_Kd x.png\n", "\nPr 0.25\n", "\nPm 0.5\n"] {
            assert!(mtl.contains(line), "no {line:?} in\n{mtl}");
        }
        let beside = |rel: &str| made.beside.iter().find(|(n, _)| n == rel).map(|(_, d)| d.clone());
        let back = crate::model::parse_with(&made.main, "x.obj", &beside).unwrap();
        assert_eq!((back.textures.len(), back.tex.clone()), (1, vec![1, 1]));
        assert_eq!(back.textures[0].get_pixel(3, 0).0, [200, 0, 0, 255]);
        let near = |a: &[[[f32; 2]; 3]], b: &[[[f32; 2]; 3]]| a.iter().flatten().flatten().zip(b.iter().flatten().flatten()).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(back.uv.len() == 2 && near(&back.uv, &m.uv), "{:?}", back.uv);
        assert_eq!(back.surfaces[back.surface[0] as usize - 1].roughness, 0.25);
        // Coloured triangles without a picture: their colours exactly, each on its points.
        let mut plain = crate::model::parse(b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n", "q.obj").unwrap();
        plain.colors = vec![[255, 0, 0], [0, 0, 255]];
        let made = export_made(&plain, Kind::Obj);
        assert!(made.beside.is_empty(), "a model without pictures or surfaces needs no .mtl");
        let back = crate::model::parse(&made.main, "x.obj").unwrap();
        assert_eq!(back.colors, plain.colors);
    }

    /// Audit 6.16: the files next to an OBJ never take the name of one already there: "x.png"
    /// taken, the picture is "x (2).png", and the `.mtl` names it.
    #[test]
    fn audit_6_16_obj_names_beside_free() {
        let taken = ["x.png"];
        let free = |n: &str| if taken.contains(&n) { n.replace(".png", "_2.png") } else { n.to_string() };
        let made = export_with(&pictured(), Kind::Obj, &|_| true, &Beside { stem: "x", free: &free }).unwrap();
        assert_eq!(made.beside[0].0, "x_2.png");
        assert!(String::from_utf8_lossy(&made.beside[1].1).contains("\nmap_Kd x_2.png\n"));
    }
    /// Audit 6.16: PLY keeps the normals and puts the colours on the points too, where Blender
    /// reads them (it leaves the faces' out).
    #[test]
    fn audit_6_16_ply_normals_and_point_colours() {
        let out = export(&pictured(), Kind::Ply);
        let end = out.windows(10).position(|w| w == b"end_header").unwrap();
        let head = String::from_utf8_lossy(&out[..end]).to_string();
        let points = &head[head.find("element vertex").unwrap()..head.find("element face").unwrap()];
        for p in ["property float nx", "property float ny", "property float nz", "property uchar red"] {
            assert!(points.contains(p), "no {p:?} on the points:\n{head}");
        }
        // The quad's four points, shared: its normals are the same at each.
        assert!(points.starts_with("element vertex 4\n"), "{head}");
        // Each point: x y z, nx ny nz, red green blue; the normal +Z of the model (Y up) is -Y with Z up.
        let body = &out[end + "end_header\n".len()..];
        let point = |i: usize| &body[i * 27..(i + 1) * 27];
        let f = |b: &[u8], k: usize| f32::from_le_bytes(b[k * 4..k * 4 + 4].try_into().unwrap());
        assert_eq!([3, 4, 5].map(|k| f(point(0), k)), [0.0, -1.0, 0.0]);
        // The first point is on the green triangle and on the red one: its colour is between.
        let [r, g, _] = [24, 25, 26].map(|k| point(0)[k]);
        assert!(r > 50 && g > 50, "{:?}", &point(0)[24..]);
        // Read back by EyeCanSee: the faces' own colours.
        let back = crate::model::parse(&out, "x.ply").unwrap();
        assert!(back.colors[0][1] > 150 && back.colors[1][0] > 150, "{:?}", back.colors);
    }
    /// A bent sheet of `n` × `n` squares as OBJ text: a curved surface, read with the normals
    /// EyeCanSee works out.
    fn bent(n: usize) -> String {
        let mut s = String::new();
        for i in 0..=n {
            for j in 0..=n {
                let (x, y) = (i as f32 / n as f32, j as f32 / n as f32);
                s += &format!("v {x} {y} {}\n", (x * 3.0).sin() * (y * 2.0).cos() * 0.3);
            }
        }
        let at = |i: usize, j: usize| i * (n + 1) + j + 1;
        for i in 0..n {
            for j in 0..n {
                s += &format!("f {} {} {}\nf {} {} {}\n", at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j), at(i + 1, j + 1), at(i, j + 1));
            }
        }
        s
    }

    /// Audit 6.16: on a curved model read from a file (normals worked out by EyeCanSee), PLY and
    /// OBJ write each point once, not once per triangle.
    #[test]
    fn audit_6_16_curved_points_shared() {
        let m = crate::model::parse(bent(16).as_bytes(), "s.obj").unwrap();
        assert!(!m.normals.is_empty());
        let out = export(&m, Kind::Ply);
        let head = String::from_utf8_lossy(&out[..out.windows(10).position(|w| w == b"end_header").unwrap()]).to_string();
        let points: usize = head.lines().find_map(|l| l.strip_prefix("element vertex ")).unwrap().parse().unwrap();
        assert_eq!(points, m.pos.len(), "PLY: {points} points for {} in the model, {} triangles", m.pos.len(), m.tris.len());
        let text = String::from_utf8(export(&m, Kind::Obj)).unwrap();
        let normals = text.lines().filter(|l| l.starts_with("vn ")).count();
        assert!(normals <= m.pos.len(), "OBJ: {normals} normals for {} points", m.pos.len());
    }

    /// #312: a scan of about 10 million triangles (a binary PLY of 190 MB, as the one of the
    /// issue) read and exported as `EYECANSEE_BIG_KIND` (obj, ply…): the time of each step and
    /// the size of the new file; the peak of memory from outside (`/usr/bin/time -v`).
    #[test]
    #[ignore]
    fn export_big_live() {
        let Some(kind) = std::env::var("EYECANSEE_BIG_KIND").ok().and_then(|k| Kind::of_ext(&k)) else { return };
        let n = 2236u32;
        let mut ply = format!("ply\nformat binary_little_endian 1.0\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nelement face {}\nproperty list uchar uint vertex_indices\nend_header\n", (n + 1) * (n + 1), 2 * n * n).into_bytes();
        for i in 0..=n {
            for j in 0..=n {
                let (x, y) = (i as f32 / n as f32, j as f32 / n as f32);
                floats(&mut ply, &[x, y, (x * 3.0).sin() * (y * 2.0).cos() * 0.3]);
            }
        }
        let at = |i: u32, j: u32| i * (n + 1) + j;
        for i in 0..n {
            for j in 0..n {
                for t in [[at(i, j), at(i + 1, j), at(i + 1, j + 1)], [at(i, j), at(i + 1, j + 1), at(i, j + 1)]] {
                    ply.push(3);
                    ply.extend(t.iter().flat_map(|v| v.to_le_bytes()));
                }
            }
        }
        println!("PLY of {} MB, {} triangles", ply.len() >> 20, 2 * n * n);
        let start = std::time::Instant::now();
        let m = crate::model::parse(&ply, "big.ply").unwrap();
        drop(ply);
        println!("read in {:?}", start.elapsed());
        let start = std::time::Instant::now();
        let made = export_made(&m, kind);
        println!("{} written in {:?}: {} MB", kind.label(), start.elapsed(), made.main.len() >> 20);
    }

    /// Audit 6.16 (#340): a model whose name has spaces: the files next to it have none (spaces
    /// become "_", as `mtllib` and many readers split names at spaces; Piero, 24/11), and read
    /// back with them, the `.mtl` and the picture are found.
    #[test]
    fn audit_6_16_obj_names_with_spaces_read_back() {
        let free = |n: &str| n.to_string();
        let made = export_with(&pictured(), Kind::Obj, &|_| true, &Beside { stem: "my  model\tx", free: &free }).unwrap();
        let names: Vec<&str> = made.beside.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["my__model_x.png", "my__model_x.mtl"]);
        assert!(String::from_utf8_lossy(&made.main).contains("\nmtllib my__model_x.mtl\n"));
        let beside = |rel: &str| made.beside.iter().find(|(n, _)| n == rel).map(|(_, d)| d.clone());
        let back = crate::model::parse_with(&made.main, "my  model\tx.obj", &beside).unwrap();
        assert_eq!((back.textures.len(), back.tex.clone()), (1, vec![1, 1]), "the .mtl or the picture not found");
    }

    /// #340: next to an OBJ only the pictures its `.mtl` names: those of roughness and metal and
    /// of the shadows in the creases have no place there, and were written without use.
    #[test]
    fn obj_only_the_pictures_named() {
        let mut m = pictured();
        let plain = |c| std::sync::Arc::new(image::RgbaImage::from_pixel(2, 2, image::Rgba(c)));
        m.textures.extend([plain([0, 128, 255, 255]), plain([200, 200, 200, 255])]);
        (m.surfaces[0].rough_metal, m.surfaces[0].occlusion_map) = (2, 3);
        let made = export_made(&m, Kind::Obj);
        let names: Vec<&str> = made.beside.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["x.png", "x.mtl"]);
    }

    /// #340: pictures read smaller (beyond 2048 pixels) or left out (beyond 64) are said at the
    /// end of an export into a format that keeps pictures (glTF, OBJ): there it is not "all".
    #[test]
    fn pictures_smaller_or_left_out_said() {
        let none: crate::model::Sibling = &|_| None;
        let notes = |m: &Mesh, kind| export_file(&export(m, Kind::Glb), "q.glb", none, kind, &|_| true, &Beside { stem: "q", free: &|n| n.to_string() }).unwrap().1;
        let mut big = pictured();
        big.textures[0] = std::sync::Arc::new(image::RgbaImage::from_pixel(3000, 4, image::Rgba([0, 200, 0, 255])));
        for kind in [Kind::Glb, Kind::Obj] {
            assert!(notes(&big, kind).contains(&"model-export-lost-smaller"), "{kind:?}: {:?}", notes(&big, kind));
        }
        assert!(!notes(&pictured(), Kind::Glb).contains(&"model-export-lost-smaller"));
        // 65 triangles, each with a picture of its own.
        let obj: String = (0..65).map(|i| format!("v {i} 0 0\nv {i}.5 0 0\nv {i} 1 0\n")).chain((0..65).map(|i| format!("f {} {} {}\n", 3 * i + 1, 3 * i + 2, 3 * i + 3))).collect();
        let mut many = crate::model::parse(obj.as_bytes(), "m.obj").unwrap();
        many.textures = (0..65).map(|i| std::sync::Arc::new(image::RgbaImage::from_pixel(2, 2, image::Rgba([i as u8, 0, 0, 255])))).collect();
        many.tex = (1..=65).collect();
        many.uv = vec![[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]; 65];
        assert!(notes(&many, Kind::Glb).contains(&"model-export-lost-left-out"), "{:?}", notes(&many, Kind::Glb));
    }

    /// #312: every writer stops when told to, also on a model of one material (glTF wrote a
    /// material's triangles before asking), and says how far it is on the way.
    #[test]
    fn every_writer_stops_and_tells() {
        let m = crate::model::parse(bent(300).as_bytes(), "s.obj").unwrap();
        assert!(m.tris.len() > 2 * TICK_EVERY);
        for kind in Kind::ALL {
            let seen = std::cell::RefCell::new(Vec::new());
            let stop = |f: f32| {
                seen.borrow_mut().push(f);
                f < 0.3
            };
            assert!(export_with(&m, kind, &stop, &Beside { stem: "x", free: &|n| n.to_string() }).is_none(), "{kind:?} did not stop");
            assert!(seen.borrow().len() >= 2, "{kind:?}: {:?}", seen.borrow());
        }
    }

    /// Audit 6.16: what the new format cannot hold is said (project rule): a model with a
    /// picture and a shiny surface, as STL and PLY, says the picture became colours and the
    /// surface is lost; as OBJ and glTF, which hold them, nothing.
    #[test]
    fn audit_6_16_losses_said() {
        let glb = export(&pictured(), Kind::Glb);
        let none: crate::model::Sibling = &|_| None;
        let notes = |kind| export_file(&glb, "q.glb", none, kind, &|_| true, &Beside { stem: "q", free: &|n| n.to_string() }).unwrap().1;
        for kind in [Kind::Stl, Kind::Ply] {
            assert_eq!(notes(kind), ["model-export-lost-pictures", "model-export-lost-surfaces"], "{kind:?}");
        }
        for kind in [Kind::Obj, Kind::Glb] {
            assert!(notes(kind).is_empty(), "{kind:?}: {:?}", notes(kind));
        }
    }
}
