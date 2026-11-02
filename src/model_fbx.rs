//! FBX (Autodesk: Maya, 3ds Max, Blender, game engines), binary and text, through `ufbx` (C,
//! MIT or public domain): every visible mesh in the place of its node, in the colour of the
//! material of each face or of its vertex colours, with its picture (in the file or next to it),
//! its roughness, metal, bumps, occlusion and glow, and the vertices' normals, with Y up.

use crate::model::{Builder, Mesh, Sibling, Surface};

pub fn read(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
    let opts = ufbx::LoadOpts { target_axes: ufbx::CoordinateAxes::right_handed_y_up(), ignore_animation: true, ..Default::default() };
    let scene = ufbx::load_memory(bytes, opts).map_err(|e| e.description.to_string())?;
    let mut b = Builder::default();
    let mut idx = Vec::new();
    // Picture and surface of each material, made once.
    let mut looks: std::collections::HashMap<u32, (u16, u16)> = std::collections::HashMap::new();
    for node in scene.nodes.iter().filter(|n| n.visible) {
        let Some(mesh) = node.mesh.as_ref() else { continue };
        let m = &node.geometry_to_world;
        let pos: Vec<[f32; 3]> = mesh.vertex_position.indices.iter().map(|&i| {
            let p = ufbx::transform_position(m, mesh.vertex_position.values[i as usize]);
            [p.x as f32, p.y as f32, p.z as f32]
        }).collect();
        let materials = if node.materials.is_empty() { &mesh.materials } else { &node.materials };
        let (mut tris, mut colors, mut shine) = (Vec::new(), Vec::new(), Vec::new());
        for (f, face) in mesh.faces.iter().enumerate() {
            let n = ufbx::triangulate_face_vec(&mut idx, mesh, *face) as usize;
            let mat = mesh.face_material.get(f).and_then(|&i| materials.get(i as usize));
            let (tex, surface) = mat.map_or((0, 0), |m| {
                *looks.entry(m.element.element_id).or_insert_with(|| {
                    let tex = picture(&mut b, [&m.pbr.base_color, &m.fbx.diffuse_color], sibling);
                    let s = surface(&mut b, m, sibling);
                    (tex, b.surface(s))
                })
            });
            for t in idx.as_chunks::<3>().0.iter().take(n) {
                tris.push(*t);
                let corners = (tex > 0 || surface > 0).then(|| corners(mesh, t)).flatten();
                shine.push((corners.map(|c| (c, tex)), surface, normals(mesh, &node.geometry_to_world, t)));
                // The picture alone, as 3D programs show it.
                let base = if tex > 0 && corners.is_some() { Some([255; 3]) } else { mat.and_then(|m| color(m)) };
                // Vertex colours tint the material's, as 3D programs show them.
                colors.push(match (base, vertex_color(mesh, t)) {
                    (Some(m), Some(v)) => Some(std::array::from_fn(|k| (m[k] as u16 * v[k] as u16 / 255) as u8)),
                    (m, v) => v.or(m),
                });
            }
        }
        b.add_shiny(pos, tris, |i| colors[i], |i| shine[i])?;
    }
    b.finish(false)
}

/// How the material takes the light. A picture of roughness or metal takes the place of its value.
fn surface(b: &mut Builder, m: &ufbx::Material, sibling: Sibling) -> Surface {
    let value = |c: &ufbx::MaterialMap, default: f32| if c.texture.is_some() { 1.0 } else if c.has_value { c.value_vec4.x as f32 } else { default };
    let glow = if m.pbr.emission_color.has_value { rgb(m.pbr.emission_color.value_vec4).map(|v| v as f32 / 255.0 * value(&m.pbr.emission_factor, 1.0)) } else { [0.0; 3] };
    Surface {
        metallic: value(&m.pbr.metalness, 0.0).clamp(0.0, 1.0),
        roughness: value(&m.pbr.roughness, 1.0).clamp(0.0, 1.0),
        occlusion: if m.pbr.ambient_occlusion.texture.is_some() { 1.0 } else { 0.0 },
        glow: glow.map(|v| v.clamp(0.0, 1.0)),
        bumps: picture(b, [&m.pbr.normal_map, &m.fbx.normal_map], sibling),
        rough_metal: rough_metal(b, m, sibling),
        occlusion_map: picture(b, [&m.pbr.ambient_occlusion, &m.pbr.ambient_occlusion], sibling),
        glow_map: picture(b, [&m.pbr.emission_color, &m.fbx.emission_color], sibling),
    }
}

/// The corners' normals, turned with the node.
fn normals(mesh: &ufbx::Mesh, m: &ufbx::Matrix, t: &[u32; 3]) -> Option<[[f32; 3]; 3]> {
    let vn = &mesh.vertex_normal;
    if !vn.exists {
        return None;
    }
    let c = t.map(|i| vn.indices.get(i as usize).and_then(|&k| vn.values.get(k as usize)).map(|&n| ufbx::transform_direction(m, n)).map(|n| [n.x as f32, n.y as f32, n.z as f32]));
    c.iter().all(Option::is_some).then(|| c.map(Option::unwrap_or_default))
}

/// The picture of the first of `maps` that has one: in the FBX file, or the file it names.
fn picture(b: &mut Builder, maps: [&ufbx::MaterialMap; 2], sibling: Sibling) -> u16 {
    let Some(file) = maps.into_iter().find_map(file) else { return 0 };
    b.texture_file(name(file), &|n| if file.content.is_empty() { sibling(n) } else { Some(file.content.to_vec()) })
}

/// The picture file of a map; of layered and procedural textures, their first.
fn file(map: &ufbx::MaterialMap) -> Option<&ufbx::Texture> {
    let t = map.texture.as_ref()?;
    if t.type_ == ufbx::TextureType::File { Some(t) } else { t.file_textures.first().map(|f| &**f) }
}

fn name(file: &ufbx::Texture) -> &str {
    if file.relative_filename.is_empty() { &file.filename } else { &file.relative_filename }
}

/// Roughness and metal, two grey pictures in FBX, as one picture the way glTF has them (green
/// and blue): what the renderers read.
fn rough_metal(b: &mut Builder, m: &ufbx::Material, sibling: Sibling) -> u16 {
    let grey = |map: &ufbx::MaterialMap| {
        let f = file(map)?;
        let n = name(f);
        let bytes = if f.content.is_empty() { sibling(n).or_else(|| sibling(n.rsplit(['/', '\\']).next()?)) } else { Some(f.content.to_vec()) };
        Some(image::load_from_memory(&bytes?).ok()?.to_luma8())
    };
    combined(grey(&m.pbr.roughness).as_ref(), grey(&m.pbr.metalness).as_ref()).map_or(0, |img| b.texture(img))
}

/// Roughness in green, metal in blue, at the size of the larger; white where one is missing.
fn combined(rough: Option<&image::GrayImage>, metal: Option<&image::GrayImage>) -> Option<image::RgbaImage> {
    let (w, h) = [rough, metal].into_iter().flatten().map(|i| i.dimensions()).max_by_key(|&(w, h)| w as u64 * h as u64)?;
    let at = |i: Option<&image::GrayImage>, x: u32, y: u32| i.map_or(255, |i| i.get_pixel(x * i.width() / w, y * i.height() / h)[0]);
    Some(image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([0, at(rough, x, y), at(metal, x, y), 255])))
}

/// Where the triangle's corners lie on the picture (V downwards, as the picture's rows).
fn corners(mesh: &ufbx::Mesh, t: &[u32; 3]) -> Option<[[f32; 2]; 3]> {
    let uv = &mesh.vertex_uv;
    if !uv.exists {
        return None;
    }
    let c = t.map(|i| uv.indices.get(i as usize).and_then(|&k| uv.values.get(k as usize)).map(|p| [p.x as f32, 1.0 - p.y as f32]));
    c.iter().all(Option::is_some).then(|| c.map(Option::unwrap_or_default))
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
    fn roughness_and_metal_in_one_picture() {
        let rough = image::GrayImage::from_pixel(4, 4, image::Luma([10]));
        let metal = image::GrayImage::from_pixel(2, 2, image::Luma([200]));
        let c = super::combined(Some(&rough), Some(&metal)).unwrap();
        assert_eq!((c.dimensions(), c.get_pixel(3, 3).0), ((4, 4), [0, 10, 200, 255]));
        assert_eq!(super::combined(None, Some(&metal)).unwrap().get_pixel(0, 0).0, [0, 255, 200, 255]);
        assert!(super::combined(None, None).is_none());
    }

    #[test]
    fn not_fbx_refused() {
        assert!(super::read(b"not an fbx file at all", &|_| None).is_err());
    }
}
