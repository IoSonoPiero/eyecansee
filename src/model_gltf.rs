//! glTF 2.0 (GLB, and .gltf with its data inside or in the files next to it): the default
//! scene's nodes with their matrices, every primitive (triangles, strips, fans; lines and points
//! as thin solids), base colours and vertex colours.

use crate::model::{Builder, IDENTITY, Mat4, Mesh, Sibling, apply, average, mul};

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
pub fn read(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
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
