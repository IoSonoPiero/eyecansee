//! PLY models (Stanford's scans, MeshLab, 3D scanners): points with their colours, faces and
//! triangle strips, or points alone (`point_cloud`).

use std::io::Cursor;

use ply_rs_bw::{parser::Parser, ply::DefaultElement, ply::Property};

use crate::model::{Builder, Mesh, TOO_FAR, average};
use crate::model_limit::{Limit, PLY_BINARY, PLY_TEXT};

/// The mark of the PLY files `model_export` writes with Z up, as Blender expects them (tried with
/// Blender 4.5, 2026-11-02); without it a PLY is read with Y up, as the scans have it.
pub(crate) const PLY_Z_UP: &str = "comment EyeCanSee, Z up";

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

/// A binary PLY read by EyeCanSee's own reader (`binary`), an ASCII one (or a header it leaves)
/// by the parser of `ply-rs-bw`.
pub(crate) fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let z_up = bytes.windows(10).position(|w| w == b"end_header").is_some_and(|end| String::from_utf8_lossy(&bytes[..end]).lines().any(|l| l.trim_end() == PLY_Z_UP));
    binary(bytes, z_up).unwrap_or_else(|| parsed(bytes, z_up))
}

/// The file read by the parser of `ply-rs-bw`: about 320 bytes for each point and face while
/// it is read (#191), so only for ASCII files and the headers `binary` leaves to it.
fn parsed(bytes: &[u8], z_up: bool) -> Result<Mesh, String> {
    let limit = parsed_limit(bytes);
    // Counted before the parser reads it all (`model_limit`).
    if limit == PLY_TEXT {
        PLY_TEXT.check_count(crate::model_limit::ply_text_count(bytes))?;
    }
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
        return point_cloud(pos, &rgb, limit);
    }
    let mut b = Builder::up_to(limit);
    let colors = |i: usize| face_rgb.get(i).copied().flatten().or_else(|| average(&rgb, tris[i])).map(|c| c.map(|v| v as u8));
    b.add(pos, tris.clone(), colors)?;
    b.finish(z_up)
}

/// The cap of a file read by the parser of `ply-rs-bw`.
fn parsed_limit(bytes: &[u8]) -> Limit {
    if in_text(bytes) { PLY_TEXT } else { PLY_BINARY }
}

/// Whether the PLY is in text (ASCII), as its header's `format` line says.
fn in_text(bytes: &[u8]) -> bool {
    let Some(end) = bytes.windows(10).position(|w| w == b"end_header") else { return false };
    String::from_utf8_lossy(&bytes[..end]).lines().any(|l| l.split_whitespace().take(2).eq(["format", "ascii"]))
}

/// The type of a PLY property, and its size in the file.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ty {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Ty {
    fn of(name: &str) -> Option<Ty> {
        Some(match name {
            "char" | "int8" => Ty::I8,
            "uchar" | "uint8" => Ty::U8,
            "short" | "int16" => Ty::I16,
            "ushort" | "uint16" => Ty::U16,
            "int" | "int32" => Ty::I32,
            "uint" | "uint32" => Ty::U32,
            "float" | "float32" => Ty::F32,
            "double" | "float64" => Ty::F64,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Ty::I8 | Ty::U8 => 1,
            Ty::I16 | Ty::U16 => 2,
            Ty::I32 | Ty::U32 | Ty::F32 => 4,
            Ty::F64 => 8,
        }
    }

    fn float(self) -> bool {
        matches!(self, Ty::F32 | Ty::F64)
    }
}

/// A property: its name, the type of its count when it is a list, and the type of its values.
struct Prop {
    name: String,
    list: Option<Ty>,
    ty: Ty,
}

struct Element {
    name: String,
    count: usize,
    props: Vec<Prop>,
}

/// The header of a binary PLY: byte order, elements, where the data starts. `None` for an ASCII
/// file, or a header the parser of `ply-rs-bw` reads in its own way (left to it).
fn binary_header(bytes: &[u8]) -> Option<(bool, Vec<Element>, usize)> {
    let end = bytes.windows(10).position(|w| w == b"end_header")?;
    let body = end + 10 + bytes[end + 10..].iter().position(|&c| c == b'\n')? + 1;
    let (mut big, mut elements) = (None, Vec::<Element>::new());
    // The lines the parser knows, as `ply_header_cleaned` keeps them.
    for l in String::from_utf8_lossy(&bytes[..end]).lines() {
        match l.split_whitespace().collect::<Vec<_>>().as_slice() {
            ["format", "binary_little_endian", "1.0"] => big = Some(false),
            ["format", "binary_big_endian", "1.0"] => big = Some(true),
            ["format", ..] => return None,
            ["element", name, count] => elements.push(Element { name: name.to_string(), count: count.parse().ok()?, props: Vec::new() }),
            ["property", "list", c, t, name] => elements.last_mut()?.props.push(Prop { name: name.to_string(), list: Some(Ty::of(c)?), ty: Ty::of(t)? }),
            ["property", t, name] => elements.last_mut()?.props.push(Prop { name: name.to_string(), list: None, ty: Ty::of(t)? }),
            [first, ..] if ["property", "element"].contains(first) => return None,
            _ => {}
        }
    }
    Some((big?, elements, body))
}

/// The data of a binary PLY, read one value at a time.
struct Data<'a> {
    bytes: &'a [u8],
    at: usize,
    big: bool,
}

const CUT: &str = "the file is cut short";

impl Data<'_> {
    fn value(&mut self, t: Ty) -> Result<f64, String> {
        let n = t.size();
        let b = self.bytes.get(self.at..self.at + n).ok_or(CUT)?;
        self.at += n;
        let mut a = [0u8; 8];
        a[..n].copy_from_slice(b);
        if self.big {
            a[..n].reverse();
        }
        let [b0, b1, b2, b3, ..] = a;
        Ok(match t {
            Ty::I8 => f64::from(b0 as i8),
            Ty::U8 => f64::from(b0),
            Ty::I16 => f64::from(i16::from_le_bytes([b0, b1])),
            Ty::U16 => f64::from(u16::from_le_bytes([b0, b1])),
            Ty::I32 => f64::from(i32::from_le_bytes([b0, b1, b2, b3])),
            Ty::U32 => f64::from(u32::from_le_bytes([b0, b1, b2, b3])),
            Ty::F32 => f64::from(f32::from_le_bytes([b0, b1, b2, b3])),
            Ty::F64 => f64::from_le_bytes(a),
        })
    }

    /// An index of a list: as `list` reads them, below 0 is `u32::MAX` (the strip separator).
    fn index(&mut self, t: Ty) -> Result<u32, String> {
        let v = self.value(t)?;
        Ok(if v < 0.0 { u32::MAX } else { v as u32 })
    }

    /// The length of a list.
    fn count(&mut self, t: Ty) -> Result<usize, String> {
        let n = self.value(t)?;
        if n < 0.0 || t.float() {
            return Err("a list of the file has no length".into());
        }
        Ok(n as usize)
    }

    fn skip(&mut self, n: usize, t: Ty) -> Result<(), String> {
        let end = n.checked_mul(t.size()).and_then(|b| b.checked_add(self.at)).filter(|&e| e <= self.bytes.len()).ok_or(CUT)?;
        self.at = end;
        Ok(())
    }

    fn left(&self) -> usize {
        self.bytes.len() - self.at
    }
}

/// What a binary PLY gives, read straight into the lists `Builder` takes: about 30 bytes a
/// triangle, where the parser of `ply-rs-bw` keeps every point and face as a table of named
/// values (about 320 bytes each, #191).
#[derive(Default)]
struct Scan {
    pos: Vec<[f32; 3]>,
    /// One per point, or empty: the points have no colours.
    rgb: Vec<[f32; 3]>,
    /// The triangles of the faces, then those of the strips.
    tris: Vec<[u32; 3]>,
    /// One per triangle of a face (MeshLab), or empty.
    face_rgb: Vec<[f32; 3]>,
    strips: Vec<[u32; 3]>,
}

/// The bytes of a row of `e` at least (a list its count alone).
fn row_size(e: &Element) -> usize {
    e.props.iter().map(|p| p.list.unwrap_or(p.ty).size()).sum::<usize>().max(1)
}

/// Where the property `name` is, the last one as the parser's tables keep it.
fn find(props: &[Prop], name: &str) -> Option<usize> {
    props.iter().rposition(|p| p.name == name)
}

/// Red, green and blue among `props`, as `ply_color` takes them: values, multiplied by 255 when red
/// is a float.
fn rgb_of(props: &[Prop]) -> Option<([usize; 3], f64)> {
    let [r, g, b] = ["red", "green", "blue"].map(|k| find(props, k).filter(|&i| props[i].list.is_none()));
    let (r, g, b) = (r?, g?, b?);
    Some(([r, g, b], if props[r].ty.float() { 255.0 } else { 1.0 }))
}

impl Scan {
    fn vertices(&mut self, d: &mut Data, e: &Element) -> Result<(), String> {
        let at = |k: &str| find(&e.props, k).filter(|&i| e.props[i].list.is_none());
        let (x, y, z) = (at("x"), at("y"), at("z"));
        let colour = rgb_of(&e.props);
        // As `Builder` would refuse them, before their memory is asked for.
        if e.count > PLY_BINARY.offered() {
            return Err(crate::model_limit::too_many(crate::model_limit::OFFERED, PLY_BINARY.offered()));
        }
        let rows = e.count.min(d.left() / row_size(e));
        self.pos.reserve_exact(rows);
        if colour.is_some() {
            self.rgb.reserve_exact(rows);
        }
        let mut v = vec![f64::NAN; e.props.len()];
        for _ in 0..e.count {
            for (k, p) in e.props.iter().enumerate() {
                match p.list {
                    Some(c) => {
                        let n = d.count(c)?;
                        d.skip(n, p.ty)?;
                    }
                    None => v[k] = d.value(p.ty)?,
                }
            }
            let get = |i: Option<usize>| i.map_or(f32::NAN, |i| v[i] as f32);
            self.pos.push([get(x), get(y), get(z)]);
            if let Some(([r, g, b], scale)) = colour {
                self.rgb.push([r, g, b].map(|i| (v[i] * scale) as f32));
            }
        }
        Ok(())
    }

    /// Faces (a fan of triangles each, with its colour when the faces have one) or strips.
    fn faces(&mut self, d: &mut Data, e: &Element, strips: bool) -> Result<(), String> {
        let list = if strips { find(&e.props, "vertex_indices") } else { find(&e.props, "vertex_indices").or_else(|| find(&e.props, "vertex_index")) };
        // As `list` reads them: indices that are whole numbers, in a list.
        let list = list.filter(|&i| e.props[i].list.is_some() && !e.props[i].ty.float());
        let colour = if strips { None } else { rgb_of(&e.props) };
        // A face with a triangle takes at least its count and three indices.
        if let Some(p) = list.map(|i| &e.props[i]).filter(|_| !strips) {
            let least = row_size(e) + 3 * p.ty.size();
            self.tris.reserve_exact(e.count.min(d.left() / least).min(PLY_BINARY.max + 1));
        }
        let (mut v, mut corners) = (vec![0.0; e.props.len()], Vec::new());
        for _ in 0..e.count {
            corners.clear();
            for (k, p) in e.props.iter().enumerate() {
                match p.list {
                    Some(c) if Some(k) == list => {
                        let n = d.count(c)?;
                        corners.reserve(n.min(d.left() / p.ty.size()));
                        for _ in 0..n {
                            corners.push(d.index(p.ty)?);
                        }
                    }
                    Some(c) => {
                        let n = d.count(c)?;
                        d.skip(n, p.ty)?;
                    }
                    None => v[k] = d.value(p.ty)?,
                }
            }
            PLY_BINARY.check(self.tris.len() + self.strips.len())?;
            if strips {
                for strip in corners.split(|&v| v == u32::MAX) {
                    self.strips.extend(strip.windows(3).enumerate().map(|(i, w)| if i % 2 == 0 { [w[0], w[1], w[2]] } else { [w[1], w[0], w[2]] }));
                }
                continue;
            }
            let fan = (1..corners.len().saturating_sub(1)).map(|i| [corners[0], corners[i], corners[i + 1]]);
            let before = self.tris.len();
            self.tris.extend(fan);
            if let Some(([r, g, b], scale)) = colour {
                let c = [r, g, b].map(|i| (v[i] * scale) as f32);
                self.face_rgb.extend(std::iter::repeat_n(c, self.tris.len() - before));
            }
        }
        Ok(())
    }

    /// An element EyeCanSee does not use: gone over.
    fn skip(d: &mut Data, e: &Element) -> Result<(), String> {
        for _ in 0..e.count {
            for p in &e.props {
                match p.list {
                    Some(c) => {
                        let n = d.count(c)?;
                        d.skip(n, p.ty)?;
                    }
                    None => d.skip(1, p.ty)?,
                }
            }
        }
        Ok(())
    }
}

/// A binary PLY read without `ply-rs-bw` (#191); `None` when it is not one (ASCII), or its
/// header is of a kind left to the parser: then the parser reads it, as before.
fn binary(bytes: &[u8], z_up: bool) -> Option<Result<Mesh, String>> {
    let (big, elements, body) = binary_header(bytes)?;
    // Two elements of the same name: the parser keeps the last one, left to it.
    let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
    if names.iter().enumerate().any(|(i, n)| names[..i].contains(n)) {
        return None;
    }
    if !names.contains(&"vertex") {
        return Some(Err("no vertices in the file".into()));
    }
    let mut d = Data { bytes, at: body, big };
    let mut s = Scan::default();
    for e in &elements {
        // Without properties a row takes no byte: its count cannot be checked against the file,
        // and there is nothing to read (points of nothing would only fill the memory).
        if e.props.is_empty() {
            continue;
        }
        let read = match e.name.as_str() {
            "vertex" => s.vertices(&mut d, e),
            "face" => s.faces(&mut d, e, false),
            "tristrips" => s.faces(&mut d, e, true),
            _ => Scan::skip(&mut d, e),
        };
        if let Err(e) = read {
            return Some(Err(e));
        }
    }
    Some(s.mesh(z_up))
}

impl Scan {
    fn mesh(self, z_up: bool) -> Result<Mesh, String> {
        let Scan { pos, rgb, mut tris, face_rgb, strips } = self;
        tris.extend(strips);
        if tris.is_empty() {
            return point_cloud(pos, &rgb, PLY_BINARY);
        }
        let mut b = Builder::up_to(PLY_BINARY);
        if rgb.is_empty() && face_rgb.is_empty() {
            b.add(pos, tris, |_| None)?;
        } else {
            // One colour a triangle (4 bytes), so that the triangles are not copied for them.
            let colors: Vec<Option<[u8; 3]>> = tris.iter().enumerate().map(|(i, &t)| face_rgb.get(i).copied().or_else(|| average(&rgb, t)).map(|c| c.map(|v| v as u8))).collect();
            drop((rgb, face_rgb));
            b.add(pos, tris, |i| colors[i])?;
        }
        b.finish(z_up)
    }
}

/// Points without faces (3D scans): each one a small tetrahedron, as large as the space between
/// points on a surface, so that the cloud looks like a surface of dots.
fn point_cloud(pos: Vec<[f32; 3]>, rgb: &[[f32; 3]], limit: Limit) -> Result<Mesh, String> {
    let pos: Vec<([f32; 3], Option<[u8; 3]>)> = pos.into_iter().enumerate().filter(|(_, p)| p.iter().all(|c| c.is_finite())).map(|(i, p)| (p, rgb.get(i).map(|c| c.map(|v| v as u8)))).collect();
    // Said as points, before the 4 triangles of each are made (Piero, 22/11/2026).
    limit.check_points(pos.len())?;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (p, _) in &pos {
        for k in 0..3 {
            (lo[k], hi[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
        }
    }
    let diag = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt();
    let s = (diag / (pos.len() as f32).sqrt() * 0.6).clamp(diag / 2000.0, diag / 100.0) / 3f32.sqrt();
    let corners = [[1.0, 1.0, 1.0], [1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [-1.0, -1.0, 1.0f32]];
    let mut b = Builder::up_to(limit);
    let tris: Vec<[u32; 3]> = (0..pos.len() as u32).flat_map(|i| [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]].map(|t| t.map(|v| i * 4 + v))).collect();
    let colors: Vec<Option<[u8; 3]>> = pos.iter().map(|(_, c)| *c).collect();
    let points = pos.iter().flat_map(|(p, _)| corners.map(|c| std::array::from_fn(|k| p[k] + c[k] * s))).collect();
    b.add(points, tris, |i| colors[i / 4])?;
    b.finish(false).map_err(|e| if e == TOO_FAR { e } else { "no points in the file".to_string() })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A binary PLY of a grid of `n` × `n` points and its triangles, as the scanners write it.
    pub(crate) fn grid(n: u32, big: bool) -> Vec<u8> {
        let f = |v: f32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
        let u = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
        let faces = 2 * (n - 1) * (n - 1);
        let mut out = format!("ply\nformat binary_{}_endian 1.0\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nelement face {faces}\nproperty list uchar uint vertex_indices\nend_header\n", if big { "big" } else { "little" }, n * n).into_bytes();
        for y in 0..n {
            for x in 0..n {
                for c in [x as f32, y as f32, ((x * y) % 7) as f32] {
                    out.extend(f(c));
                }
            }
        }
        for y in 0..n - 1 {
            for x in 0..n - 1 {
                let i = y * n + x;
                for t in [[i, i + 1, i + n], [i + 1, i + n + 1, i + n]] {
                    out.push(3);
                    t.iter().for_each(|&v| out.extend(u(v)));
                }
            }
        }
        out
    }

    /// Two meshes alike, bit for bit (NaN points too).
    fn alike(a: &Mesh, b: &Mesh) -> Result<(), String> {
        let bits = |m: &Mesh| m.pos.iter().map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let normals = |m: &Mesh| m.normals.iter().map(|n| n.map(|c| c.map(f32::to_bits))).collect::<Vec<_>>();
        let checks = [
            ("points", bits(a) == bits(b)),
            ("triangles", a.tris == b.tris),
            ("colours", a.colors == b.colors),
            ("normals", normals(a) == normals(b)),
            ("surfaces", a.surface == b.surface && a.surfaces == b.surfaces),
            ("up", a.z_up == b.z_up),
            ("box", a.center == b.center && a.radius == b.radius && a.half == b.half),
        ];
        match checks.iter().find(|c| !c.1) {
            Some((what, _)) => Err(format!("{what} differ")),
            None => Ok(()),
        }
    }

    /// The file read by EyeCanSee's own reader as by the parser of before: the same mesh, or an
    /// error from both. `true` when the reader took it (a binary file).
    fn same_as_before(bytes: &[u8]) -> Result<bool, String> {
        let z_up = read(bytes).map(|m| m.z_up).unwrap_or(false);
        let Some(new) = binary(bytes, z_up) else { return Ok(false) };
        match (new, parsed(bytes, z_up)) {
            (Ok(a), Ok(b)) => alike(&a, &b).map(|()| true),
            (Err(_), Err(_)) => Ok(true),
            (a, b) => Err(format!("now {:?}, before {:?}", a.map(|m| m.tris.len()), b.map(|m| m.tris.len()))),
        }
    }

    /// A PLY with `header` (the lines between "format" and "end_header") and `body`.
    fn ply(format: &str, header: &str, body: &[u8]) -> Vec<u8> {
        [format!("ply\nformat {format} 1.0\n{header}end_header\n").as_bytes(), body].concat()
    }

    fn le(values: &[(f64, &str)]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|&(v, t)| match t {
                "c" => vec![v as i8 as u8],
                "u" => vec![v as u8],
                "s" => (v as i16).to_le_bytes().to_vec(),
                "i" => (v as i32).to_le_bytes().to_vec(),
                "I" => (v as u32).to_le_bytes().to_vec(),
                "f" => (v as f32).to_le_bytes().to_vec(),
                _ => v.to_le_bytes().to_vec(),
            })
            .collect()
    }

    /// A binary PLY left to the parser (two elements of the same name) is read whole before its
    /// triangles or points can be counted: refused without the why (Piero, 22/11/2026), as the
    /// other formats read whole.
    #[test]
    fn binary_left_to_the_parser_without_the_why() {
        let head = "element vertex 1\nproperty float x\nproperty float y\nproperty float z\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\n";
        let bytes = ply("binary_little_endian", head, &le(&[(0.0, "f"); 6]));
        assert!(binary(&bytes, false).is_none() && !in_text(&bytes));
        let limit = parsed_limit(&bytes);
        let (en, it) = (crate::i18n::I18n::new("en"), crate::i18n::I18n::new("it"));
        assert_eq!(en.load_error(&limit.error()).lines().nth(1), Some("EyeCanSee does not open binary PLY files with more than 50 million triangles."));
        assert_eq!(it.load_error(&limit.error()).lines().nth(1), Some("EyeCanSee non apre i file PLY binari con più di 50 milioni di triangoli."));
        let points = limit.check_points(usize::MAX).unwrap_err();
        assert_eq!(en.load_error(&points).lines().nth(1), Some("EyeCanSee does not open binary PLY files with more than 12.5 million points."));
        assert_eq!(it.load_error(&points).lines().nth(1), Some("EyeCanSee non apre i file PLY binari con più di 12,5 milioni di punti."));
        // EyeCanSee's own reader counts before it reads: the why stays.
        assert_eq!(en.load_error(&PLY_BINARY.error()).lines().nth(1), Some("EyeCanSee does not open binary PLY files with more than 50 million triangles, so that reading them does not fill the computer's memory."));
    }

    /// #191: the reader of the binary files gives what the parser gave, on every kind the files
    /// in the collection have and on files made to break it.
    #[test]
    fn binary_as_before_191() {
        let xyz = "element vertex 4\nproperty float x\nproperty float y\nproperty float z\n";
        let points: Vec<(f64, &str)> = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0]].iter().flatten().map(|&v| (v, "f")).collect();
        let quad = |t: &'static str| vec![(4.0, "u"), (0.0, t), (1.0, t), (2.0, t), (3.0, t)];
        let mut cases: Vec<(&str, Vec<u8>)> = vec![
            ("grid, little endian", grid(12, false)),
            ("grid, big endian", grid(12, true)),
            ("quad", ply("binary_little_endian", &format!("{xyz}element face 1\nproperty list uchar int vertex_indices\n"), &[le(&points), le(&quad("i"))].concat())),
            ("vertex_index, uint, CRLF and comments", ply("binary_little_endian", &format!("comment made by hand\r\n{xyz}comment among the properties\nobj_info x\nelement face 1\nproperty list uchar uint vertex_index\r\n"), &[le(&points), le(&quad("I"))].concat())),
            ("both names: vertex_indices wins", ply("binary_little_endian", &format!("{xyz}element face 1\nproperty list uchar int vertex_index\nproperty list uchar int vertex_indices\n"), &[le(&points), le(&[(3.0, "u"), (0.0, "i"), (1.0, "i"), (2.0, "i")]), le(&quad("i"))].concat())),
            ("float indices: no faces, a cloud", ply("binary_little_endian", &format!("{xyz}element face 1\nproperty list uchar float vertex_indices\n"), &[le(&points), le(&quad("f"))].concat())),
            ("indices outside and negative", ply("binary_little_endian", &format!("{xyz}element face 2\nproperty list uchar int vertex_indices\n"), &[le(&points), le(&[(3.0, "u"), (0.0, "i"), (1.0, "i"), (9.0, "i"), (3.0, "u"), (0.0, "i"), (-1.0, "i"), (2.0, "i")]), le(&quad("i"))].concat())),
        ];
        // Colours of the points as bytes and as floats, of the faces, a list among the points, an
        // element of its own in between, strips with -1, doubles, a short face.
        let rgb = "property uchar red\nproperty uchar green\nproperty uchar blue\n";
        let colored: Vec<(f64, &str)> = points.chunks(3).enumerate().flat_map(|(i, p)| [p.to_vec(), vec![(i as f64 * 60.0, "u"), (255.0, "u"), (7.0, "u")]].concat()).collect();
        cases.push(("point colours", ply("binary_little_endian", &format!("{xyz}{rgb}element face 1\nproperty list uchar int vertex_indices\n"), &[le(&colored), le(&quad("i"))].concat())));
        let frgb = "property float red\nproperty float green\nproperty float blue\n";
        let fcolored: Vec<(f64, &str)> = points.chunks(3).enumerate().flat_map(|(i, p)| [p.to_vec(), vec![(i as f64 / 4.0, "f"), (1.0, "f"), (0.5, "f")]].concat()).collect();
        cases.push(("float colours", ply("binary_little_endian", &format!("{xyz}{frgb}element face 1\nproperty list uchar int vertex_indices\n"), &[le(&fcolored), le(&quad("i"))].concat())));
        cases.push(("face colours", ply("binary_little_endian", &format!("{xyz}element face 2\nproperty list uchar int vertex_indices\n{rgb}"), &[le(&points), le(&quad("i")), le(&[(255.0, "u"), (0.0, "u"), (0.0, "u"), (3.0, "u"), (0.0, "i"), (2.0, "i"), (3.0, "i"), (0.0, "u"), (0.0, "u"), (255.0, "u")])].concat())));
        let listed = "element vertex 2\nproperty float x\nproperty list uchar float extra\nproperty float y\nproperty float z\n";
        let mut body = le(&[(0.0, "f"), (2.0, "u"), (5.0, "f"), (6.0, "f"), (0.0, "f"), (0.0, "f"), (1.0, "f"), (0.0, "u"), (1.0, "f"), (1.0, "f")]);
        body.extend(le(&[(2.0, "s"), (9.0, "d"), (1.0, "u"), (8.0, "u"), (0.0, "s"), (3.0, "d"), (0.0, "u")]));
        cases.push(("lists among the points, an element of its own", ply("binary_little_endian", &format!("{listed}element extra 2\nproperty short a\nproperty double b\nproperty list uchar uchar c\n"), &body)));
        let strips = format!("{xyz}element tristrips 1\nproperty list int int vertex_indices\n");
        cases.push(("strips", ply("binary_little_endian", &strips, &[le(&points), le(&[(7.0, "i"), (0.0, "i"), (1.0, "i"), (2.0, "i"), (3.0, "i"), (-1.0, "i"), (1.0, "i"), (2.0, "i")])].concat())));
        cases.push(("strips before faces", ply("binary_little_endian", &format!("{strips}element face 1\nproperty list uchar int vertex_indices\n"), &[le(&points), le(&[(4.0, "i"), (0.0, "i"), (1.0, "i"), (2.0, "i"), (3.0, "i")]), le(&quad("i"))].concat())));
        let doubles = "element vertex 3\nproperty double x\nproperty double y\nproperty double z\nelement face 2\nproperty list uchar int vertex_indices\n";
        cases.push(("doubles, a face of two", ply("binary_big_endian", doubles, &[[0.0f64, 0.0, 0.0, 1.0, 0.0, 0.0, 0.1, 1.0, 1e-30].iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>(), vec![3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 1]].concat())));
        cases.push(("a cloud with colours", ply("binary_little_endian", &format!("{xyz}{rgb}"), &le(&colored))));
        cases.push(("Z up", ply("binary_little_endian", &format!("{}\n{xyz}element face 1\nproperty list uchar int vertex_indices\n", PLY_Z_UP), &[le(&points), le(&quad("i"))].concat())));
        cases.push(("no x", ply("binary_little_endian", "element vertex 3\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\n", &[le(&[(0.0, "f"); 6]), le(&[(3.0, "u"), (0.0, "i"), (1.0, "i"), (2.0, "i")])].concat())));
        cases.push(("no vertex element", ply("binary_little_endian", "element face 1\nproperty list uchar int vertex_indices\n", &le(&quad("i")))));
        // Cut short, at every length: an error, never a panic.
        let whole = cases[2].1.clone();
        for n in (whole.len() - 30..whole.len()).step_by(3) {
            cases.push(("cut short", whole[..n].to_vec()));
        }
        for (what, bytes) in &cases {
            assert_eq!(same_as_before(bytes), Ok(true), "{what}");
        }
        // The files are what they say: colours, strips and points read.
        let got = |what: &str| read(&cases.iter().find(|c| c.0 == what).unwrap().1);
        assert_eq!(got("face colours").unwrap().colors, vec![[255, 0, 0], [255, 0, 0], [0, 0, 255]]);
        assert_eq!(got("point colours").unwrap().colors.len(), 2);
        assert_eq!(got("strips").unwrap().tris, vec![[0, 1, 2], [2, 1, 3]]);
        assert_eq!(got("strips before faces").unwrap().tris.len(), 2 + 2);
        assert_eq!(got("lists among the points, an element of its own").unwrap().tris.len(), 8);
        assert_eq!(got("both names: vertex_indices wins").unwrap().tris.len(), 2);
        assert!(got("float colours").unwrap().colors[0][1] == 255 && got("cut short").is_err() && got("no vertex element").is_err());
        // Counts beyond the file: an error at once, without asking for their memory (the parser
        // of before is not asked: it would reserve them).
        for head in ["element vertex 4000000000\nproperty float x\n", "element vertex 1\nproperty float x\nelement face 1\nproperty list uint int vertex_indices\n"] {
            let bytes = ply("binary_little_endian", head, &le(&[(0.0, "f"), (4e9, "I"), (0.0, "i")]));
            let (r, peak) = crate::test_alloc::peak(|| read(&bytes));
            assert!(r.is_err() && peak < 1 << 20, "{head}: {peak} bytes");
        }
        // An element without properties takes no byte a row: its count cannot be checked against
        // the file, and nothing is read of it (30 million points of nothing were 360 MB).
        let (r, peak) = crate::test_alloc::peak(|| read(b"ply\nformat binary_little_endian 1.0\nelement vertex 30000000\nelement face 30000000\nend_header\n"));
        assert!(r.is_err() && peak < 1 << 20, "{peak} bytes");
        // An ASCII file is still read by the parser.
        assert!(binary(b"ply\nformat ascii 1.0\nelement vertex 0\nend_header\n", false).is_none());
    }

    /// #191: every binary PLY under `EYECANSEE_PLY` (and its folders) read by EyeCanSee's reader
    /// as by the parser of before. Files over `EYECANSEE_PLY_MAX` MB (25 if not said) are left
    /// out: the parser of before takes about 320 bytes a point or face (a 137 MB scan, 6 GB).
    #[test]
    #[ignore]
    fn binary_as_before_live() {
        let Ok(dir) = std::env::var("EYECANSEE_PLY") else { return };
        let max: u64 = std::env::var("EYECANSEE_PLY_MAX").ok().and_then(|v| v.parse().ok()).unwrap_or(25) << 20;
        let mut stack = vec![std::path::PathBuf::from(dir)];
        let (mut read_by_us, mut failed) = (0, Vec::new());
        while let Some(d) = stack.pop() {
            for f in std::fs::read_dir(&d).unwrap().flatten() {
                let (path, size) = (f.path(), f.metadata().map(|m| m.len()).unwrap_or(0));
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ply")) {
                    if size > max {
                        println!("{}: left out, {} MB", path.display(), size >> 20);
                        continue;
                    }
                    let r = same_as_before(&std::fs::read(&path).unwrap());
                    println!("{}: {r:?}", path.display());
                    match r {
                        Ok(true) => read_by_us += 1,
                        Ok(false) => {}
                        Err(e) => failed.push(format!("{}: {e}", path.display())),
                    }
                }
            }
        }
        println!("{read_by_us} binary files alike");
        assert!(failed.is_empty(), "{failed:#?}");
    }

    /// Megabytes of `key` in /proc/self/status (VmRSS, VmHWM).
    #[cfg(target_os = "linux")]
    fn status_mb(key: &str) -> u64 {
        let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) / 1024
    }

    /// #191: the model in `EYECANSEE_PROBE` read as the viewer reads it, with the peak of the
    /// whole process while it is read (VmHWM), what the process keeps once it is read (VmRSS) and
    /// the bytes a triangle at the peak. `EYECANSEE_PROBE_BEFORE=1` reads it with the parser of
    /// before too, and compares. Run alone in a release build, in a cgroup with a ceiling on the
    /// memory (`systemd-run --user --scope -p MemoryMax=12G -p MemorySwapMax=0`).
    #[test]
    #[ignore]
    #[cfg(target_os = "linux")]
    fn peak_live_191() {
        let Ok(path) = std::env::var("EYECANSEE_PROBE") else { return };
        let bytes = std::fs::read(&path).unwrap();
        let name = std::path::Path::new(&path).file_name().unwrap().to_string_lossy().to_string();
        let file = bytes.len() >> 20;
        let _ = std::fs::write("/proc/self/clear_refs", "5");
        let (start, t) = (status_mb("VmRSS:"), std::time::Instant::now());
        let m = match crate::model::parse(&bytes, &name) {
            Ok(m) => m,
            Err(e) => return println!("RESULT error {name}: {e} (peak {} MB)", status_mb("VmHWM:")),
        };
        let (took, peak, kept) = (t.elapsed(), status_mb("VmHWM:"), status_mb("VmRSS:"));
        let n = m.tris.len().max(1) as u64;
        println!("RESULT {name}: {} triangles, {} points, file {file} MB, read in {took:.1?}", m.tris.len(), m.pos.len());
        println!("RESULT peak {peak} MB ({} bytes a triangle beyond the file and the start), kept {kept} MB, {} MB of vertices for the graphics card", (peak.saturating_sub(start)) * (1 << 20) / n, (n * 132) >> 20);
        if std::env::var("EYECANSEE_PROBE_BEFORE").as_deref() == Ok("1") {
            let z_up = m.z_up;
            let _ = std::fs::write("/proc/self/clear_refs", "5");
            let t = std::time::Instant::now();
            match parsed(&bytes, z_up) {
                Ok(before) => {
                    println!("RESULT before: peak {} MB, read in {:.1?}", status_mb("VmHWM:"), t.elapsed());
                    println!("RESULT alike: {:?}", alike(&m, &before));
                }
                Err(e) => println!("RESULT before: error {e}"),
            }
        }
    }

    /// #191, for the ceiling of the triangles (#179): the model in `EYECANSEE_PROBE` written into
    /// `EYECANSEE_PROBE_OUT` in the other formats, to measure them with `peak_live_191`: STL, OBJ,
    /// GLB (`model_export`), PLY in ASCII and 3MF (written here, without colours).
    #[test]
    #[ignore]
    fn formats_for_191() {
        use std::io::Write;
        let (Ok(path), Ok(out)) = (std::env::var("EYECANSEE_PROBE"), std::env::var("EYECANSEE_PROBE_OUT")) else { return };
        let m = crate::model::parse(&std::fs::read(&path).unwrap(), "probe.ply").unwrap();
        let out = std::path::Path::new(&out);
        for kind in [crate::model_export::Kind::Stl, crate::model_export::Kind::Obj, crate::model_export::Kind::Glb] {
            std::fs::write(out.join(format!("probe.{}", kind.ext())), crate::model_export::export(&m, kind)).unwrap();
        }
        let mut ply = std::io::BufWriter::new(std::fs::File::create(out.join("probe-ascii.ply")).unwrap());
        write!(ply, "ply\nformat ascii 1.0\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nelement face {}\nproperty list uchar int vertex_indices\nend_header\n", m.pos.len(), m.tris.len()).unwrap();
        m.pos.iter().for_each(|p| writeln!(ply, "{} {} {}", p[0], p[1], p[2]).unwrap());
        m.tris.iter().for_each(|t| writeln!(ply, "3 {} {} {}", t[0], t[1], t[2]).unwrap());
        drop(ply);
        let mut zip = zip::ZipWriter::new(std::fs::File::create(out.join("probe.3mf")).unwrap());
        zip.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        let mut w = std::io::BufWriter::new(&mut zip);
        write!(w, r#"<?xml version="1.0" encoding="UTF-8"?><model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources><object id="1" type="model"><mesh><vertices>"#).unwrap();
        m.pos.iter().for_each(|p| write!(w, r#"<vertex x="{}" y="{}" z="{}"/>"#, p[0], p[1], p[2]).unwrap());
        write!(w, "</vertices><triangles>").unwrap();
        m.tris.iter().for_each(|t| write!(w, r#"<triangle v1="{}" v2="{}" v3="{}"/>"#, t[0], t[1], t[2]).unwrap());
        write!(w, r#"</triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#).unwrap();
        drop(w);
        zip.finish().unwrap();
    }

    /// #191: reading a binary PLY took about 870 bytes per triangle at its peak (Lucy, 28 million
    /// triangles: 21.9 GB), four times what the model keeps once open. A grid of 200 000
    /// triangles, read on this thread: the peak per triangle.
    #[test]
    fn peak_per_triangle_191() {
        let bytes = grid(317, false);
        let (m, peak) = crate::test_alloc::peak(|| crate::model::parse(&bytes, "scan.ply").unwrap());
        let per = peak / m.tris.len();
        println!("{} triangles, peak {} MB, {per} bytes a triangle", m.tris.len(), peak >> 20);
        assert_eq!(m.tris.len(), 2 * 316 * 316);
        assert!(per < 250, "{per} bytes a triangle at the peak");
    }
}
