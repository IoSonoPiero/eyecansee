//! PLY models (Stanford's scans, MeshLab, 3D scanners): points with their colours, faces and
//! triangle strips, or points alone (`point_cloud`).

use std::io::Cursor;

use ply_rs_bw::{parser::Parser, ply::DefaultElement, ply::Property};

use crate::model::{Builder, Mesh, TOO_FAR, average};

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

pub(crate) fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let z_up = bytes.windows(10).position(|w| w == b"end_header").is_some_and(|end| String::from_utf8_lossy(&bytes[..end]).lines().any(|l| l.trim_end() == PLY_Z_UP));
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
    b.finish(z_up)
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
