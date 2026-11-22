//! glTF 2.0 (GLB, and .gltf with its data inside or in the files next to it): the default
//! scene's nodes with their matrices, every primitive (triangles, strips, fans; lines and points
//! as thin solids), base colours, vertex colours, the pictures of the base colours and the
//! rest of the material (metal, roughness, bumps, occlusion, glow) with the vertices' normals.

use std::collections::{HashMap, HashSet};

use base64::Engine;

use crate::model::{Builder, IDENTITY, Mat4, Mesh, Sibling, Surface, apply, average, mul};

/// What a glTF that cannot be read needs, said plainly: version 1.0.
fn gltf_error(e: String) -> String {
    if e.contains("expected a sequence") {
        // glTF 1.0 keeps its parts in maps by name, 2.0 in lists.
        "glTF 1.0 is not supported (only glTF 2.0)".into()
    } else {
        e
    }
}

/// Fluent id of a .gltf whose data file next to it cannot be read (#178), sent as
/// `id\u{1}name` (`I18n::load_error`).
pub const MISSING_BIN: &str = "error-gltf-bin-missing";
/// Fluent id of a .gltf whose data file is out of its folder or on the internet, which is
/// never read (`model::sibling_path`), sent as `id\u{1}name`.
pub const OUTSIDE_BIN: &str = "error-gltf-bin-outside";
/// Fluent id of a glTF whose data written inside it (base64) cannot be decoded, sent as `id\u{1}`.
pub const DAMAGED_INSIDE: &str = "error-gltf-data-damaged";
/// Fluent id of a GLB without its block of data (chunk BIN), sent as `id\u{1}`.
pub const INCOMPLETE: &str = "error-glb-incomplete";
/// Fluent id of a GLB cut short whose whole length cannot be told (`cut`), sent as `id\u{1}`.
pub const CUT: &str = "error-glb-cut";
/// Fluent id of a GLB cut short with the length its header gives and the file's, in bytes, sent
/// as `id\u{1}declared actual` (`cut`) and said as sizes by `cut_text`.
pub const CUT_SIZES: &str = "error-glb-cut-sizes";
/// The ids above: the ones with a name first.
const ERRORS: [&str; 6] = [MISSING_BIN, OUTSIDE_BIN, DAMAGED_INSIDE, INCOMPLETE, CUT, CUT_SIZES];
/// A length above this in a GLB's header is not believed: what EyeCanSee reads of a model in all
/// (`model_3mf::MAX_UNPACKED`, 2 GB), under the 4 GB its 32 bits allow; a cut file whose header
/// gives more says only that it is incomplete.
const MAX_DECLARED: u64 = crate::model_3mf::MAX_UNPACKED;

/// Whether `id` is one of the errors of a glTF whose data cannot be read.
pub fn is_error(id: &str) -> bool {
    ERRORS.contains(&id)
}

/// Convert's form of those errors (`is_error`): the file the model names and the model, or only
/// the model in brackets after the sentence.
pub fn convert_error(id: &str, file: &str, name: &str) -> (&'static str, String) {
    let id = ERRORS.into_iter().find(|e| *e == id).unwrap_or(MISSING_BIN);
    (id, if file.is_empty() { format!(" ({name})") } else { format!("{file} ({name})") })
}

/// The error of a data file `uri` that could not be read next to the model: refused because it
/// is out of the model's folder or on the internet, else missing; with its name as the model
/// gives it (percent-decoded, at most 120 characters, without control characters).
pub(crate) fn unread_file(uri: &str) -> String {
    let id = if crate::model::leaves_folder(uri) { OUTSIDE_BIN } else { MISSING_BIN };
    let name: String = crate::model::percent_decoded(uri).unwrap_or_else(|| uri.to_string()).chars().filter(|c| !c.is_control()).take(120).collect();
    format!("{id}\u{1}{name}")
}

/// The error of a buffer whose data is not there: a file next to the model, data inside the
/// .gltf that cannot be decoded, or a GLB without its block of data.
fn unread_buffer(source: gltf::buffer::Source) -> String {
    match source {
        gltf::buffer::Source::Uri(uri) if uri.starts_with("data:") => format!("{DAMAGED_INSIDE}\u{1}"),
        gltf::buffer::Source::Uri(uri) => unread_file(uri),
        gltf::buffer::Source::Bin => format!("{INCOMPLETE}\u{1}"),
    }
}

/// GLB, and .gltf with its data inside or in the files next to it (`sibling`); compressed ones
/// (Draco, meshopt, quantized) first made plain (`model_gltf_unpack`). A GLB cut short is
/// refused first (`cut`).
pub fn read(bytes: &[u8], sibling: Sibling, unpacked: &crate::model_3mf::Unpacked) -> Result<Mesh, String> {
    if let Some(e) = cut(bytes) {
        return Err(e);
    }
    // A GLB by its first bytes, also when its data is first made plain; a .gltf as the others.
    let limit = if bytes.starts_with(b"glTF") { crate::model_limit::GLB } else { crate::model_limit::OTHER };
    match crate::model_gltf_unpack::plain(bytes, sibling, unpacked)? {
        Some(glb) => read_plain(&glb, sibling, limit),
        None => read_plain(bytes, sibling, limit),
    }
}

/// The error of a GLB cut short, as after a download or a copy that stopped (#178): its header
/// (12 bytes), its JSON or its block of data ends after the file, or the header gives a length
/// larger than the file (which the glTF reader refuses). With the two lengths when the header's
/// is believable: larger than the file and at most `MAX_DECLARED`. A header that gives less than
/// its own 12 bytes is not believed either (the glTF reader would subtract them from it). A whole
/// GLB is left to the reader, whatever its header gives.
fn cut(bytes: &[u8]) -> Option<String> {
    let len = bytes.len() as u64;
    if len < 12 {
        // The first bytes of a GLB's header, or nothing at all.
        return (bytes.starts_with(b"glTF") || b"glTF".starts_with(bytes)).then(|| format!("{CUT}\u{1}"));
    }
    if !bytes.starts_with(b"glTF") {
        return None;
    }
    let u32_at = |at: u64| Some(u64::from(u32::from_le_bytes(bytes.get(at as usize..at as usize + 4)?.try_into().ok()?)));
    let declared = u32_at(8)?;
    // The JSON chunk (8 bytes of header, then the JSON), then the block of data if there is one.
    let parts_cut = match u32_at(12).map(|n| 20 + n) {
        Some(end) if end <= len => len > end && u32_at(end).is_none_or(|n| end + 8 + n > len),
        _ => true,
    };
    if !parts_cut && (12..=len).contains(&declared) {
        return None;
    }
    Some(if declared > len && declared <= MAX_DECLARED { format!("{CUT_SIZES}\u{1}{declared} {len}") } else { format!("{CUT}\u{1}") })
}

/// The sentence of a GLB cut short with its two lengths (`CUT_SIZES`, its detail `declared
/// actual` and then Convert's name), the sizes as the program writes them (`human::size`); the
/// sentence without them when the two read the same ("1 KB").
pub(crate) fn cut_text(i18n: &crate::i18n::I18n, id: &str, detail: &str) -> Option<String> {
    if id != CUT_SIZES {
        return None;
    }
    let (declared, rest) = detail.split_once(' ')?;
    let (actual, rest) = rest.split_at(rest.find(' ').unwrap_or(rest.len()));
    let [expected, actual] = [declared, actual].map(|n| n.parse().ok().map(|n| crate::human::size(i18n, n)));
    let (expected, actual) = (expected?, actual?);
    Some(if expected == actual { i18n.t_args(CUT, &[("detail", rest)]) } else { i18n.t_args(CUT_SIZES, &[("expected", &expected), ("actual", &actual), ("detail", rest)]) })
}

fn read_plain(bytes: &[u8], sibling: Sibling, limit: crate::model_limit::Limit) -> Result<Mesh, String> {
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
    if let Some(b) = g.document.buffers().find(|b| data(b.clone()).is_none()) {
        return Err(unread_buffer(b.source()));
    }
    let mut b = Builder::up_to(limit);
    let mut extra: Extra = (Vec::new(), Vec::new(), HashMap::new(), HashSet::new(), 0);
    // The materials' pictures, read once: in the file, in base64, or next to it; decoded in
    // parallel (a 2048² picture takes ~30 ms).
    let mut wanted: Vec<gltf::Image> = Vec::new();
    for mat in g.document.materials() {
        let pbr = mat.pbr_metallic_roughness();
        let textures = [pbr.base_color_texture().map(|t| t.texture()), pbr.metallic_roughness_texture().map(|t| t.texture()), mat.normal_texture().map(|t| t.texture()), mat.occlusion_texture().map(|t| t.texture()), mat.emissive_texture().map(|t| t.texture())];
        for img in textures.into_iter().flatten().map(|t| t.source()) {
            if !wanted.iter().any(|w| w.index() == img.index()) {
                wanted.push(img);
            }
        }
    }
    let bytes: Vec<Option<Vec<u8>>> = wanted
        .iter()
        .map(|img| match img.source() {
            gltf::image::Source::View { view, .. } => data(view.buffer()).and_then(|d| d.get(view.offset()..view.offset() + view.length())).map(<[u8]>::to_vec),
            gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => uri.split_once(";base64,").and_then(|(_, d)| base64::engine::general_purpose::STANDARD.decode(d).ok()),
            gltf::image::Source::Uri { uri, .. } => sibling(uri),
        })
        .collect();
    use rayon::prelude::*;
    let decoded: Vec<Option<image::RgbaImage>> = bytes.into_par_iter().map(|d| d.and_then(|d| image::load_from_memory(&d).ok()).map(|i| i.to_rgba8())).collect();
    for (img, pic) in wanted.iter().zip(decoded) {
        let k = pic.map_or(0, |p| b.texture(p));
        extra.2.insert(img.index(), k);
    }
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
    if extra.4 > 0 {
        log!("glTF: nodes already placed and met again, left out: {}", extra.4);
    }
    crate::model_lines::add(&mut b, &extra.0, &extra.1)?;
    b.finish(false)
}

/// Line segments and points of a glTF, made solid at the end (`model_lines`), the number of
/// each image's picture in the model (`Builder::texture`, 0 when it could not be read), the
/// nodes already placed, and how many times one of them was met again.
type Extra = (Vec<crate::model_lines::Segment>, Vec<crate::model_lines::Point>, HashMap<usize, u16>, HashSet<usize>, usize);

/// Fluent id of the warning for a node met again in the hierarchy (#179).
pub const NODE_AGAIN: &str = "model-node-again";
/// The same said of a file made from the model (Save as…, Export as a 3D model…): it has that part once.
pub const NODE_AGAIN_SAVED: &str = "model-node-again-saved";

fn gltf_node<'a, 's, F: Clone + for<'x> Fn(gltf::Buffer<'x>) -> Option<&'s [u8]>>(b: &mut Builder, extra: &mut Extra, node: &gltf::Node<'a>, parent: &Mat4, data: &F, depth: u32) -> Result<(), String> {
    if depth > 64 {
        return Err("nodes nested too deep".into());
    }
    // The nodes are trees, each node with one parent at most (glTF 2.0, "Nodes and Hierarchy"):
    // a node met again (two parents, listed twice, a loop) is placed once, so that a file cannot
    // place its nodes over and over; said in the log (the first ten) and in the viewer (#179).
    if !extra.3.insert(node.index()) {
        if extra.4 < 10 {
            log!("glTF: node {} met again in the hierarchy (glTF allows one parent): placed once", node.index());
        }
        extra.4 += 1;
        b.warn(NODE_AGAIN);
        return Ok(());
    }
    let m = mul(parent, &node.transform().matrix());
    if let Some(mesh) = node.mesh() {
        gltf_mesh(b, extra, &mesh, &m, data)?;
        crate::model_lines::fits(extra.0.len() + extra.1.len(), b.limit())?;
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
        let (tex, uvs, set) = texture_coords(&prim, &r, &extra.2);
        let surface = if prim.material().unlit() { 0 } else { b.surface(surface(&prim.material(), &extra.2, set)) };
        // Normals turned with the node (scaled unevenly they lean a little: kept simple).
        let origin = apply(m, [0.0; 3]);
        let normals: Vec<[f32; 3]> = r.read_normals().map(|n| n.map(|v| std::array::from_fn(|k| apply(m, v)[k] - origin[k])).collect()).unwrap_or_default();
        let shine = |i: usize| {
            let corners = tris[i].map(|v| uvs.get(v as usize).copied());
            let uv = corners.iter().all(Option::is_some).then(|| (corners.map(|c| c.unwrap_or_default()), tex));
            let n = tris[i].map(|v| normals.get(v as usize).copied());
            (uv, surface, n.iter().all(Option::is_some).then(|| n.map(Option::unwrap_or_default)))
        };
        b.add_shiny(pos, tris.clone(), colors, shine)?;
    }
    Ok(())
}

/// The material's surface: factors, and the pictures (`Builder::texture`) read on the base
/// colour's texture coordinates `set`; one on another set is left out (an occlusion baked on a
/// second set would darken the wrong places), and so is a glow whose picture is missing.
// ponytail: each picture's own texture transform is not followed
fn surface(mat: &gltf::Material, tex: &HashMap<usize, u16>, set: u32) -> Surface {
    let pic = |t: Option<(gltf::Texture, u32)>| t.filter(|(_, s)| *s == set).and_then(|(t, _)| tex.get(&t.source().index()).copied()).unwrap_or(0);
    let pbr = mat.pbr_metallic_roughness();
    let occlusion = mat.occlusion_texture().map(|t| (t.texture(), t.tex_coord()));
    let glow_map = pic(mat.emissive_texture().map(|t| (t.texture(), t.tex_coord())));
    let lost = mat.emissive_texture().is_some() && glow_map == 0;
    let glow = mat.emissive_factor().map(|v| if lost { 0.0 } else { (v * mat.emissive_strength().unwrap_or(1.0)).clamp(0.0, 1.0).powf(1.0 / 2.2) });
    let occlusion_map = pic(occlusion.clone());
    Surface {
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        occlusion: if occlusion_map > 0 { mat.occlusion_texture().map_or(0.0, |o| o.strength()) } else { 0.0 },
        glow,
        bumps: pic(mat.normal_texture().map(|t| (t.texture(), t.tex_coord()))),
        rough_metal: pic(pbr.metallic_roughness_texture().map(|t| (t.texture(), t.tex_coord()))),
        occlusion_map,
        glow_map,
    }
}

/// The picture of a primitive's base colour (0 without one) and the texture coordinates, of its
/// set (else the first), moved, turned and scaled as KHR_texture_transform says; and that set.
fn texture_coords<'a, 's, F: Clone + for<'x> Fn(gltf::Buffer<'x>) -> Option<&'s [u8]>>(prim: &gltf::Primitive<'a>, r: &gltf::mesh::Reader<'a, 's, F>, tex: &HashMap<usize, u16>) -> (u16, Vec<[f32; 2]>, u32) {
    let info = prim.material().pbr_metallic_roughness().base_color_texture();
    let k = info.as_ref().and_then(|i| tex.get(&i.texture().source().index()).copied()).unwrap_or(0);
    let transform = info.as_ref().and_then(|i| i.texture_transform());
    let set = transform.as_ref().and_then(|t| t.tex_coord()).or(info.as_ref().map(|i| i.tex_coord())).unwrap_or(0);
    let Some(coords) = r.read_tex_coords(set) else { return (0, Vec::new(), set) };
    let fix = |uv: [f32; 2]| match &transform {
        Some(t) => {
            let ([sx, sy], [ox, oy], (sin, cos)) = (t.scale(), t.offset(), t.rotation().sin_cos());
            let (x, y) = (uv[0] * sx, uv[1] * sy);
            [cos * x + sin * y + ox, -sin * x + cos * y + oy]
        }
        None => uv,
    };
    (k, coords.into_f32().map(fix).collect(), set)
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    /// A glTF with one triangle in node 2, its data inside, child of nodes 0 and 1; `scene`: the
    /// scene's nodes.
    fn triangle(scene: &str) -> String {
        let floats: Vec<u8> = [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let data = base64::engine::general_purpose::STANDARD.encode(floats);
        format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"uri":"data:application/octet-stream;base64,{data}","byteLength":36}}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[0,0,0]}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"nodes":[{{"children":[2]}},{{"children":[2]}},{{"mesh":0}}],"scenes":[{{"nodes":[{scene}]}}],"scene":0}}"#
        )
    }

    /// #178: a .gltf whose .bin is not next to it is refused naming that file, plain or packed
    /// (which said "no triangles in the file"), in the user's language.
    #[test]
    fn missing_bin_named() {
        let body = r#""buffers":[{"uri":"missing%20model.bin","byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[0,0,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0"#;
        for packed in ["", r#""extensionsUsed":["KHR_mesh_quantization"],"#] {
            let gltf = format!(r#"{{"asset":{{"version":"2.0"}},{packed}{body}}}"#);
            let e = crate::model::parse(gltf.as_bytes(), "a.gltf").unwrap_err();
            assert_eq!(e, format!("{}\u{1}missing model.bin", super::MISSING_BIN), "{packed}");
            assert_eq!(crate::i18n::I18n::new("en").load_error(&e), "The file with the model's data is missing: missing model.bin");
            assert_eq!(crate::i18n::I18n::new("it").load_error(&e), "Manca il file con i dati del modello: missing model.bin");
            // Convert: the file, and the model it belongs to.
            let job = crate::batch::Job { format: Some(crate::encode::Format::Png), ..Default::default() };
            let (id, detail) = crate::batch::convert(gltf.as_bytes(), "a.gltf", &job).unwrap_err();
            assert_eq!(crate::i18n::I18n::new("en").error(id, &detail), "The file with the model's data is missing: missing model.bin (a.gltf)");
        }
        // A control character in the name does not reach the message (`\u{1}` separates the id).
        assert_eq!(super::unread_file("a%01b.bin"), format!("{}\u{1}ab.bin", super::MISSING_BIN));
    }

    /// A glTF with one triangle whose data is in `buffer` (the buffer's JSON object), plain or
    /// packed (`KHR_mesh_quantization`, read through `model_gltf_unpack`).
    fn with_buffer(buffer: &str, packed: bool) -> String {
        let packed = if packed { r#""extensionsUsed":["KHR_mesh_quantization"],"# } else { "" };
        format!(
            r#"{{"asset":{{"version":"2.0"}},{packed}"buffers":[{buffer}],"bufferViews":[{{"buffer":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","max":[1,1,0],"min":[0,0,0]}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
        )
    }

    /// The text of `e` in English and Italian.
    fn said(e: &str) -> [String; 2] {
        ["en", "it"].map(|lang| crate::i18n::I18n::new(lang).load_error(e))
    }

    /// Convert's text (English) of the glTF `bytes` named `name`.
    fn converted(bytes: &[u8], name: &str) -> String {
        let job = crate::batch::Job { format: Some(crate::encode::Format::Png), ..Default::default() };
        let (id, detail) = crate::batch::convert(bytes, name, &job).unwrap_err();
        crate::i18n::I18n::new("en").error(id, &detail)
    }

    /// #178: data written inside the .gltf (base64) that cannot be decoded is said to be damaged,
    /// not a missing file, plain or packed, in the user's language and in Convert.
    #[test]
    fn damaged_data_inside() {
        for packed in [false, true] {
            let gltf = with_buffer(r#"{"uri":"data:application/octet-stream;base64,@@@@","byteLength":36}"#, packed);
            let e = crate::model::parse(gltf.as_bytes(), "a.gltf").unwrap_err();
            assert_eq!(e, format!("{}\u{1}", super::DAMAGED_INSIDE), "packed {packed}");
            let [en, it] = said(&e);
            assert_eq!(en, "The model's data written inside the file is damaged");
            assert_eq!(it, "I dati del modello scritti dentro il file sono rovinati");
            assert_eq!(converted(gltf.as_bytes(), "a.gltf"), "The model's data written inside the file is damaged (a.gltf)");
        }
    }

    /// #178: a GLB without its block of data (chunk BIN) is said to be incomplete, plain or packed.
    #[test]
    fn glb_without_its_data() {
        for packed in [false, true] {
            let mut json = with_buffer(r#"{"byteLength":36}"#, packed).into_bytes();
            while !json.len().is_multiple_of(4) {
                json.push(b' ');
            }
            let mut glb = b"glTF".to_vec();
            glb.extend(2u32.to_le_bytes());
            glb.extend((20 + json.len() as u32).to_le_bytes());
            glb.extend((json.len() as u32).to_le_bytes());
            glb.extend(b"JSON");
            glb.extend(&json);
            let e = crate::model::parse(&glb, "a.glb").unwrap_err();
            assert_eq!(e, format!("{}\u{1}", super::INCOMPLETE), "packed {packed}");
            let [en, it] = said(&e);
            assert_eq!(en, "The file is incomplete: the block with the model's data is missing");
            assert_eq!(it, "Il file è incompleto: manca il blocco con i dati del modello");
            assert_eq!(converted(&glb, "a.glb"), "The file is incomplete: the block with the model's data is missing (a.glb)");
        }
    }

    /// A GLB with one triangle, its block of data whole; `packed` as in `with_buffer`.
    fn whole_glb(packed: bool) -> Vec<u8> {
        let mut json = with_buffer(r#"{"byteLength":36}"#, packed).into_bytes();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let floats: Vec<u8> = [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut glb = b"glTF".to_vec();
        glb.extend(2u32.to_le_bytes());
        glb.extend((28 + json.len() as u32 + 36).to_le_bytes());
        glb.extend((json.len() as u32).to_le_bytes());
        glb.extend(b"JSON");
        glb.extend(&json);
        glb.extend(36u32.to_le_bytes());
        glb.extend(b"BIN\0");
        glb.extend(floats);
        glb
    }

    /// The error of `bytes` read as a GLB, and its text in English and Italian.
    fn refused(bytes: &[u8]) -> (String, [String; 2]) {
        let e = crate::model::parse(bytes, "a.glb").unwrap_err();
        let said = said(&e);
        (e, said)
    }

    const CUT_EN: &str = "The file is incomplete";
    const CUT_IT: &str = "Il file è incompleto";

    /// #178: a GLB cut short (a download that stopped), in its block of data or in its JSON, plain
    /// or packed: when its header gives the whole length, the two sizes are said; when it gives the
    /// cut one, only that it is incomplete. The library said "could not completely read the object"
    /// (data) or "chunk length (…) exceeds data length" (JSON).
    #[test]
    fn glb_cut() {
        for packed in [false, true] {
            let glb = whole_glb(packed);
            assert_eq!(crate::model::parse(&glb, "a.glb").unwrap().tris.len(), 1, "packed {packed}");
            let json_end = glb.len() - 36 - 8;
            // In the middle of the data, of the block's header, of the JSON, of the JSON's header.
            for cut in [glb.len() - 18, json_end + 4, json_end, json_end - 10, 16] {
                let short = &glb[..cut];
                let (e, _) = refused(short);
                assert_eq!(e, format!("{}\u{1}{} {cut}", super::CUT_SIZES, glb.len()), "packed {packed} cut {cut}");
                // Sizes of less than a KB read the same ("1 KB"): the sentence without them.
                assert_eq!(said(&e), [CUT_EN, CUT_IT], "packed {packed} cut {cut}");
                assert_eq!(converted(short, "a.glb"), "The file is incomplete (a.glb)");
                // The header gives the cut length: nothing to compare. Cut right after its JSON,
                // it is a whole GLB without its block of data.
                let mut lying = short.to_vec();
                lying[8..12].copy_from_slice(&(cut as u32).to_le_bytes());
                let id = if cut == json_end { super::INCOMPLETE } else { super::CUT };
                assert_eq!(refused(&lying).0, format!("{id}\u{1}"), "packed {packed} cut {cut}");
                if id == super::CUT {
                    assert_eq!(refused(&lying).1, [CUT_EN, CUT_IT], "packed {packed} cut {cut}");
                }
            }
            // Every length short of the whole: never a sentence of the library.
            for cut in 0..glb.len() {
                let e = crate::model::parse(&glb[..cut], "a.glb").unwrap_err();
                assert!(e.starts_with("error-glb-cut"), "packed {packed} cut {cut}: {e}");
            }
        }
    }

    /// #178: the two sizes in the user's language, in the viewer and in Convert (also in the
    /// summary's Markdown): a GLB of 12.4 MB cut at 3.1 MB.
    #[test]
    fn glb_cut_sizes() {
        let (declared, actual) = (13_002_342_u32, 3_250_586_usize);
        let mut glb = whole_glb(false);
        let json_end = glb.len() - 36 - 8;
        glb[8..12].copy_from_slice(&declared.to_le_bytes());
        glb[json_end..json_end + 4].copy_from_slice(&(declared - json_end as u32 - 8).to_le_bytes());
        glb.resize(actual, 0);
        let (e, [en, it]) = refused(&glb);
        assert_eq!(e, format!("{}\u{1}{declared} {actual}", super::CUT_SIZES));
        // Save as… and Export as a 3D model… read it the same way, and say it with `load_error`.
        let none: crate::model::Sibling = &|_| None;
        assert_eq!(crate::decode::decode_full_noted(&glb, "a.glb", none).unwrap_err(), e);
        assert_eq!(crate::model_export::export_file(&glb, "a.glb", none, crate::model_export::Kind::Stl).unwrap_err(), e);
        assert_eq!(en, "The file is incomplete: it should be 12.4 MB but it is 3.1 MB.\nPerhaps the download or the copy stopped: try downloading or copying it again.");
        assert_eq!(it, "Il file è incompleto: dovrebbe essere di 12,4 MB ma è di 3,1 MB.\nForse lo scaricamento o la copia si è interrotta: prova a scaricarlo o copiarlo di nuovo.");
        assert_eq!(converted(&glb, "a b.glb"), "The file is incomplete (a b.glb): it should be 12.4 MB but it is 3.1 MB.\nPerhaps the download or the copy stopped: try downloading or copying it again.");
        let job = crate::batch::Job { format: Some(crate::encode::Format::Png), ..Default::default() };
        let (id, detail) = crate::batch::convert(&glb, "a.glb", &job).unwrap_err();
        let sizes = crate::save_dialogs::md("it should be 12.4 MB but it is 3.1 MB");
        assert!(crate::i18n::I18n::new("en").error_md(id, &detail).contains(&sizes));
    }

    /// #178: a header cut itself (less than 12 bytes), or one that gives a length that cannot be
    /// right (less than its own 12 bytes, or more than `MAX_DECLARED`), says only that the file is
    /// incomplete; a GLB whose parts are whole opens whatever its header gives, if it is believable.
    #[test]
    fn glb_cut_header() {
        let plain = (format!("{}\u{1}", super::CUT), [CUT_EN.to_string(), CUT_IT.to_string()]);
        for header in [&b""[..], b"g", b"glT", b"glTF", b"glTF\x02\0\0\0\x10\0\0"] {
            assert_eq!(refused(header), plain, "{header:?}");
        }
        for packed in [false, true] {
            let glb = whole_glb(packed);
            for declared in [super::MAX_DECLARED as u32 + 1, u32::MAX] {
                let mut short = glb[..glb.len() - 18].to_vec();
                short[8..12].copy_from_slice(&declared.to_le_bytes());
                assert_eq!(refused(&short), plain, "packed {packed} declared {declared}");
            }
            // Less than the header: the library would subtract the header from it.
            for declared in [0u32, 11] {
                let mut whole = glb.clone();
                whole[8..12].copy_from_slice(&declared.to_le_bytes());
                assert_eq!(refused(&whole), plain, "packed {packed} declared {declared}");
            }
            // Whole parts and a header that gives less than the file (bytes after the model): read.
            let mut longer = glb.clone();
            longer.extend([0; 8]);
            assert_eq!(crate::model::parse(&longer, "a.glb").unwrap().tris.len(), 1, "packed {packed}");
        }
    }

    /// #178: a .gltf whose data file is in a folder above, at a whole path or on the internet is
    /// not said missing: EyeCanSee does not read there (`model::sibling_path` refuses it).
    #[test]
    fn data_file_outside_the_folder() {
        for (uri, name) in [("../x.bin", "../x.bin"), ("%2E%2E/x.bin", "../x.bin"), ("/x.bin", "/x.bin"), ("C:/x.bin", "C:/x.bin"), (r"C:\\x.bin", r"C:\x.bin"), ("http://h/x.bin", "http://h/x.bin"), ("https://h/x.bin", "https://h/x.bin")] {
            for packed in [false, true] {
                let gltf = with_buffer(&format!(r#"{{"uri":"{uri}","byteLength":36}}"#), packed);
                let e = crate::model::parse(gltf.as_bytes(), "a.gltf").unwrap_err();
                assert_eq!(e, format!("{}\u{1}{name}", super::OUTSIDE_BIN), "{uri} packed {packed}");
            }
        }
        let e = super::unread_file("https://h/x.bin");
        let [en, it] = said(&e);
        assert_eq!(en, "The file with the model's data is not read: EyeCanSee does not read files outside the model's folder or from the internet.\nhttps://h/x.bin");
        assert_eq!(it, "Il file con i dati del modello non viene letto: EyeCanSee non legge file fuori dalla cartella del modello né da internet.\nhttps://h/x.bin");
        let gltf = with_buffer(r#"{"uri":"../x.bin","byteLength":36}"#, false);
        assert_eq!(converted(gltf.as_bytes(), "a.gltf"), "The file with the model's data is not read: EyeCanSee does not read files outside the model's folder or from the internet.\n../x.bin (a.gltf)");
        // A name that is not out of the folder, even one that cannot be decoded, is missing.
        assert_eq!(super::unread_file("%2"), format!("{}\u{1}%2", super::MISSING_BIN));
    }

    /// #178: the data file in the folder above, there for real, is not read; next to the model it is.
    #[test]
    fn data_file_above_not_read() {
        let dir = std::env::temp_dir().join(format!("ecs-gltf-above-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let floats: Vec<u8> = [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        std::fs::write(dir.join("x.bin"), &floats).unwrap();
        std::fs::write(dir.join("sub/x.bin"), &floats).unwrap();
        let model = dir.join("sub/a.gltf").to_string_lossy().to_string();
        let next = |rel: &str| crate::model::sibling_path(&model, rel).and_then(|p| std::fs::read(p).ok());
        for packed in [false, true] {
            let above = with_buffer(r#"{"uri":"../x.bin","byteLength":36}"#, packed);
            assert_eq!(crate::model::parse_with(above.as_bytes(), &model, &next).unwrap_err(), format!("{}\u{1}../x.bin", super::OUTSIDE_BIN));
            let beside = with_buffer(r#"{"uri":"x.bin","byteLength":36}"#, packed);
            assert_eq!(crate::model::parse_with(beside.as_bytes(), &model, &next).unwrap().tris.len(), 1);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// #179: a node with two parents (outside the standard) is placed once, as before, and the
    /// model says so for the viewer, once; reached through one parent only, nothing is said.
    #[test]
    fn node_with_two_parents_warns() {
        let twice = crate::model::parse(triangle("0,1").as_bytes(), "a.gltf").unwrap();
        assert_eq!(twice.tris.len(), 1);
        assert_eq!(twice.warnings, vec![super::NODE_AGAIN]);
        let once = crate::model::parse(triangle("0").as_bytes(), "a.gltf").unwrap();
        assert_eq!(once.tris.len(), 1);
        assert!(once.warnings.is_empty());
        for lang in ["en", "it"] {
            assert_ne!(crate::i18n::I18n::new(lang).t(super::NODE_AGAIN), super::NODE_AGAIN);
        }
    }

    /// #179: Save as… (the picture of the model, `decode_full_noted`) and Export as a 3D model…
    /// (`export_file`) carry the warning of a node with two parents, said of the new file in both
    /// languages; nothing for a model reached through one parent. The exported file has the
    /// part once, as the line says.
    #[test]
    fn node_with_two_parents_said_of_the_new_file() {
        use crate::model_export::{Kind, export_file};
        let none = &|_: &str| None;
        let (twice, once) = (triangle("0,1"), triangle("0"));
        let (picture, notes) = crate::decode::decode_full_noted(twice.as_bytes(), "a.gltf", none).unwrap();
        assert_eq!((picture.width(), &notes), (1920, &vec![super::NODE_AGAIN]));
        assert!(crate::decode::decode_full_noted(once.as_bytes(), "a.gltf", none).unwrap().1.is_empty());
        // What the Save as page reads: the original as `load_original` makes it.
        let mut original = crate::encode::Original::with_meta(picture, &[]);
        original.notes = notes;
        assert_eq!(original.alone().turned(Default::default()).notes, vec![super::NODE_AGAIN]);
        for kind in Kind::ALL {
            let (data, notes) = export_file(twice.as_bytes(), "a.gltf", none, kind).unwrap();
            assert_eq!(notes, vec![super::NODE_AGAIN], "{kind:?}");
            let back = crate::model::parse(&data, &format!("b.{}", kind.ext())).unwrap();
            assert_eq!((back.tris.len(), back.warnings.len()), (1, 0), "{kind:?}");
            assert!(export_file(once.as_bytes(), "a.gltf", none, kind).unwrap().1.is_empty(), "{kind:?}");
        }
        let line = |lang| crate::model::saved_lines(&crate::i18n::I18n::new(lang), "a.gltf", &[super::NODE_AGAIN]);
        assert_eq!(line("en"), ["a.gltf: a part of the model appeared more than once in its hierarchy; the new file has it only once"]);
        assert_eq!(line("it"), ["a.gltf: una parte del modello compariva più volte nella sua gerarchia; il file nuovo la contiene una volta sola"]);
    }

    /// Audit 6.6: a chain of 60 nodes, each one with the next as its child twice, none with a
    /// mesh: 2^60 visits in a .gltf of a few hundred bytes (`gltf` accepts a node with more than
    /// one parent). It must end (refused, or found empty) without visiting them all.
    /// `AUDIT_N` = fewer levels, to measure how the time grows. Run alone, with a time limit.
    #[test]
    fn audit_6_6_gltf_node() {
        let levels: u32 = std::env::var("AUDIT_N").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
        let nodes: Vec<String> = (1..=levels).map(|k| if k < levels { format!(r#"{{"children":[{k},{k}]}}"#) } else { "{}".into() }).collect();
        let gltf = format!(r#"{{"asset":{{"version":"2.0"}},"scenes":[{{"nodes":[0]}}],"scene":0,"nodes":[{}]}}"#, nodes.join(","));
        println!("glTF of {} bytes, 2^{} visits", gltf.len(), levels - 1);
        let t = std::time::Instant::now();
        let r = crate::model::parse(gltf.as_bytes(), "a.gltf");
        println!("ended in {:.2?}: {:?}", t.elapsed(), r.as_ref().map(|m| m.tris.len()));
        assert!(r.is_err());
    }
}
