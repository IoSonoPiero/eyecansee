//! What the graphics card is given for a model, made without it: the triangles' vertices (`STRIDE`
//! bytes each) grouped by their pictures, and the pictures with their smaller copies (mipmaps).

use crate::model::{DEFAULT, Mesh, Surface};
use crate::render3d;

/// The surface of triangle `i`, if it has one.
fn surface(mesh: &Mesh, i: usize) -> Option<&Surface> {
    mesh.surfaces.get((*mesh.surface.get(i)? as usize).checked_sub(1)?)
}

/// Triangle `i`'s pictures: colour, bumps, roughness and metal, occlusion, glow (0 = none).
fn key(mesh: &Mesh, i: usize) -> [u16; 5] {
    let tex = mesh.tex.get(i).copied().unwrap_or(0);
    surface(mesh, i).map_or([tex, 0, 0, 0, 0], |s| [tex, s.bumps, s.rough_metal, s.occlusion_map, s.glow_map])
}

/// The triangles' pictures (`key`, 0 where it is not among the `pictures`) and the triangles
/// sorted by them.
pub(super) fn order(mesh: &Mesh, pictures: usize) -> (Vec<[u16; 5]>, Vec<usize>) {
    let keys: Vec<[u16; 5]> = (0..mesh.tris.len()).map(|i| key(mesh, i).map(|k| if (k as usize) < pictures { k } else { 0 })).collect();
    let mut order: Vec<usize> = (0..mesh.tris.len()).collect();
    order.sort_by_key(|&i| keys[i]);
    (keys, order)
}

/// A picture's bytes followed by those of its smaller copies down to 1 × 1, and how many there
/// are with it.
pub(super) fn mips(img: &image::RgbaImage) -> (u32, Vec<u8>) {
    let levels = 32 - img.width().max(img.height()).leading_zeros();
    let mut data = img.as_raw().clone();
    let mut level = img.clone();
    for _ in 1..levels {
        level = crate::mipmap::half(&level);
        data.extend_from_slice(level.as_raw());
    }
    (levels, data)
}

/// The face of a section: a square on the plane, larger than the model, in the cut's colour and
/// marked (4) as not cut itself; two triangles (`STRIDE` bytes a vertex).
pub(super) fn cap_vertices(mesh: &Mesh, cut: &render3d::Cut, out: &mut Vec<u8>) {
    let plane = cut.plane(mesh);
    let axis = cut.axis.min(2);
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let r = mesh.radius * 2.0;
    let corner = |a: f32, b: f32| {
        let mut p = mesh.center;
        p[axis] = plane[3] * plane[axis];
        p[u] += a * r;
        p[v] += b * r;
        p
    };
    let [cr, cg, cb] = render3d::CAP;
    for p in [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)] {
        out.extend(p.iter().flat_map(|f| f.to_le_bytes()));
        out.extend([cr, cg, cb, 255]);
        out.extend([0.0f32; 5].iter().flat_map(|f| f.to_le_bytes()));
        out.extend([0, 255, 0, 4, 0, 0, 0, 255]);
    }
}

/// The three vertices of triangle `i` (`STRIDE` bytes each).
pub(super) fn vertices(mesh: &Mesh, i: usize, out: &mut Vec<u8>) {
    let [r, g, b] = mesh.colors.get(i).copied().unwrap_or(DEFAULT);
    let uv = mesh.uv.get(i).copied().unwrap_or_default();
    let normals = mesh.normals.get(i).copied().unwrap_or_default();
    let unorm = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (pbr, glow) = match surface(mesh, i) {
        Some(s) => ([unorm(s.metallic), unorm(s.roughness), unorm(s.occlusion), 1 + if s.bumps > 0 { 2 } else { 0 }], s.glow.map(unorm)),
        None => ([0, 255, 0, 0], [0; 3]),
    };
    for (k, v) in mesh.tris[i].iter().enumerate() {
        out.extend(mesh.pos[*v as usize].iter().flat_map(|f| f.to_le_bytes()));
        out.extend([r, g, b, 255]);
        out.extend(uv[k].iter().chain(&normals[k]).flat_map(|f| f.to_le_bytes()));
        out.extend(pbr);
        out.extend([glow[0], glow[1], glow[2], 255]);
    }
}
