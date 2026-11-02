//! FBX (Autodesk: Maya, 3ds Max, Blender, game engines), binary and text, through `ufbx` (C,
//! MIT or public domain): every visible mesh in the place of its node, in the colour of the
//! material of each face or of its vertex colours, with Y up.

use crate::model::{Builder, Mesh};

pub fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let opts = ufbx::LoadOpts { target_axes: ufbx::CoordinateAxes::right_handed_y_up(), ignore_animation: true, ignore_embedded: true, ..Default::default() };
    let scene = ufbx::load_memory(bytes, opts).map_err(|e| e.description.to_string())?;
    let mut b = Builder::default();
    let mut idx = Vec::new();
    for node in scene.nodes.iter().filter(|n| n.visible) {
        let Some(mesh) = node.mesh.as_ref() else { continue };
        let m = &node.geometry_to_world;
        let pos: Vec<[f32; 3]> = mesh.vertex_position.indices.iter().map(|&i| {
            let p = ufbx::transform_position(m, mesh.vertex_position.values[i as usize]);
            [p.x as f32, p.y as f32, p.z as f32]
        }).collect();
        let materials = if node.materials.is_empty() { &mesh.materials } else { &node.materials };
        let mut tris = Vec::new();
        let mut colors = Vec::new();
        for (f, face) in mesh.faces.iter().enumerate() {
            let n = ufbx::triangulate_face_vec(&mut idx, mesh, *face) as usize;
            let mat = mesh.face_material.get(f).and_then(|&i| materials.get(i as usize));
            for t in idx.as_chunks::<3>().0.iter().take(n) {
                tris.push(*t);
                colors.push(mat.and_then(|m| color(m)).or_else(|| vertex_color(mesh, t)));
            }
        }
        b.add(pos, tris, |i| colors[i])?;
    }
    b.finish(false)
}

fn rgb(c: ufbx::Vec4) -> [u8; 3] {
    [c.x, c.y, c.z].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8)
}

/// The base colour of PBR materials, otherwise the classic diffuse colour.
fn color(m: &ufbx::Material) -> Option<[u8; 3]> {
    [&m.pbr.base_color, &m.fbx.diffuse_color].into_iter().find(|c| c.has_value).map(|c| rgb(c.value_vec4))
}

/// The average of the triangle's corners, the indices being of `vertex_indices` (corners).
fn vertex_color(mesh: &ufbx::Mesh, t: &[u32; 3]) -> Option<[u8; 3]> {
    let vc = &mesh.vertex_color;
    if !vc.exists {
        return None;
    }
    let c: Vec<ufbx::Vec4> = t.iter().filter_map(|&i| vc.indices.get(i as usize).and_then(|&k| vc.values.get(k as usize)).copied()).collect();
    (c.len() == 3).then(|| rgb(ufbx::Vec4 { x: (c[0].x + c[1].x + c[2].x) / 3.0, y: (c[0].y + c[1].y + c[2].y) / 3.0, z: (c[0].z + c[1].z + c[2].z) / 3.0, w: 1.0 }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn not_fbx_refused() {
        assert!(super::read(b"not an fbx file at all").is_err());
    }
}
