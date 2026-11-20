//! glTF made smaller (Draco geometry, meshopt buffers, quantized numbers) turned into a plain GLB
//! in memory for `model_gltf`: every buffer one after the other in one, the compressed parts
//! decoded after them, the vertices' numbers as floats.
// ponytail: sparse accessors and morph targets are not unpacked

use std::collections::HashMap;

use base64::Engine;
use serde_json::{Map, Value, json};

use crate::model::Sibling;
use crate::model_3mf::Unpacked;

const PACKED: [&str; 4] = ["KHR_draco_mesh_compression", "EXT_meshopt_compression", "KHR_meshopt_compression", "KHR_mesh_quantization"];
/// A decoded part larger than this is refused (a small file could ask for gigabytes).
const MAX_DECODED: usize = 1 << 30;

/// The JSON and the binary chunk of a GLB, or the JSON of a .gltf.
fn split(bytes: &[u8]) -> Option<(&[u8], Option<&[u8]>)> {
    if !bytes.starts_with(b"glTF") {
        return Some((bytes, None));
    }
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize);
    let json_len = u32_at(12)?;
    let json = bytes.get(20..20 + json_len)?;
    let bin = u32_at(20 + json_len).and_then(|n| bytes.get(28 + json_len..28 + json_len + n));
    Some((json, bin))
}

fn num(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}

/// A plain GLB, or `None` when the file uses none of the packings. What it gathers and decodes
/// counts in `unpacked`.
pub fn plain(bytes: &[u8], sibling: Sibling, unpacked: &Unpacked) -> Result<Option<Vec<u8>>, String> {
    let Some((json, bin)) = split(bytes) else { return Ok(None) };
    let Ok(mut j) = serde_json::from_slice::<Value>(json) else { return Ok(None) };
    let used = j["extensionsUsed"].as_array().is_some_and(|a| a.iter().any(|e| PACKED.contains(&e.as_str().unwrap_or(""))));
    if !used {
        return Ok(None);
    }
    let mut p = Packed { unpacked, converted: HashMap::new(), data: Vec::new(), read_end: 0, views: j["bufferViews"].as_array().cloned().unwrap_or_default(), accessors: j["accessors"].as_array().cloned().unwrap_or_default() };
    // Every buffer into one, each view moved to where its buffer landed.
    let mut base = Vec::new();
    for (i, buf) in j["buffers"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let bytes = match buf["uri"].as_str() {
            None => bin.filter(|_| i == 0).map(<[u8]>::to_vec),
            Some(u) if u.starts_with("data:") => u.split_once(";base64,").and_then(|(_, d)| base64::engine::general_purpose::STANDARD.decode(d).ok()),
            Some(u) => sibling(u).inspect(|b| unpacked.read_too(b.len())),
        };
        let bytes = bytes.unwrap_or_default();
        unpacked.spend(bytes.len() as u64)?;
        p.align();
        base.push(p.data.len());
        p.data.extend(bytes);
    }
    // A view that is not an object is left for the glTF reader to refuse.
    for v in p.views.iter_mut().filter(|v| v.is_object()) {
        let at = base.get(num(&v["buffer"])).copied().unwrap_or(0);
        v["byteOffset"] = json!(num(&v["byteOffset"]).saturating_add(at));
        v["buffer"] = json!(0);
    }
    for k in 0..p.views.len() {
        p.meshopt(k, &base)?;
    }
    p.read_end = p.data.len();
    let mut meshes = j["meshes"].as_array().cloned().unwrap_or_default();
    for mesh in meshes.iter_mut().filter(|m| m.is_object()) {
        for prim in mesh["primitives"].as_array_mut().into_iter().flatten().filter(|p| p["attributes"].is_object()) {
            p.draco(prim)?;
            p.floats(prim)?;
        }
    }
    j["meshes"] = json!(meshes);
    j["bufferViews"] = json!(p.views);
    j["accessors"] = json!(p.accessors);
    j["buffers"] = json!([{ "byteLength": p.data.len() }]);
    // `get_mut`: `j[list]` would add a missing list as `null`, which the glTF reader refuses.
    for list in ["extensionsUsed", "extensionsRequired"] {
        if let Some(a) = j.get_mut(list).and_then(Value::as_array_mut) {
            a.retain(|e| !PACKED.contains(&e.as_str().unwrap_or("")));
        }
    }
    let json = serde_json::to_vec(&j).map_err(|e| e.to_string())?;
    // A GLB gives its sizes in 32 bits.
    if json.len().saturating_add(p.data.len()) > u32::MAX as usize - 64 {
        return Err("glTF too large once unpacked".into());
    }
    Ok(Some(glb(&json, p.data)))
}

fn glb(json: &[u8], mut bin: Vec<u8>) -> Vec<u8> {
    let mut json = json.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut out = Vec::with_capacity(28 + json.len() + bin.len());
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend((28 + json.len() as u32 + bin.len() as u32).to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(json);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// The one buffer being made, with the views and accessors that point into it.
struct Packed<'u> {
    /// What is gathered and decoded, counted for the whole file.
    unpacked: &'u Unpacked,
    /// The accessors of integers already made floats, by their number: each once, however many
    /// primitives name it.
    converted: HashMap<usize, usize>,
    data: Vec<u8>,
    /// The end of the file's buffers and of the meshopt data: integers are read only up to here,
    /// never from the floats appended after.
    read_end: usize,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Packed<'_> {
    fn align(&mut self) {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
    }

    /// A new view on `bytes`, appended.
    fn view(&mut self, bytes: &[u8]) -> usize {
        self.align();
        self.views.push(json!({ "buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len() }));
        self.data.extend_from_slice(bytes);
        self.views.len() - 1
    }

    /// An accessor of floats (`n` per element), or of u32 indices when `n` is 0: in the place of
    /// accessor `slot` (Draco's have no data of their own), else a new one.
    fn accessor(&mut self, slot: Option<usize>, values: &[f32], n: usize, indices: &[u32]) -> usize {
        let (bytes, kind, count, component): (Vec<u8>, &str, usize, u32) = if n == 0 {
            (indices.iter().flat_map(|v| v.to_le_bytes()).collect(), "SCALAR", indices.len(), 5125)
        } else {
            (values.iter().flat_map(|v| v.to_le_bytes()).collect(), ["", "SCALAR", "VEC2", "VEC3", "VEC4"][n.min(4)], values.len() / n, 5126)
        };
        let view = self.view(&bytes);
        let mut a = json!({ "bufferView": view, "componentType": component, "count": count, "type": kind });
        if n == 3 {
            // Positions need their box.
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in values.chunks(3) {
                for k in 0..3 {
                    (lo[k], hi[k]) = (lo[k].min(v[k]), hi[k].max(v[k]));
                }
            }
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        match slot.filter(|&s| s < self.accessors.len()) {
            Some(s) => {
                self.accessors[s] = a;
                s
            }
            None => {
                self.accessors.push(a);
                self.accessors.len() - 1
            }
        }
    }

    /// A view compressed with meshopt, decoded and moved after the rest.
    fn meshopt(&mut self, k: usize, base: &[usize]) -> Result<(), String> {
        let ext = &self.views[k]["extensions"];
        let Some(e) = ext.get("EXT_meshopt_compression").or_else(|| ext.get("KHR_meshopt_compression")).cloned() else { return Ok(()) };
        let at = base.get(num(&e["buffer"])).copied().unwrap_or(0).checked_add(num(&e["byteOffset"]));
        let range = at.and_then(|at| Some(at..at.checked_add(num(&e["byteLength"]))?));
        let src = range.and_then(|r| self.data.get(r)).ok_or("meshopt data outside the buffer")?.to_vec();
        let (count, stride) = (num(&e["count"]), num(&e["byteStride"]));
        if count.saturating_mul(stride) > MAX_DECODED || stride == 0 {
            return Err("meshopt data too large".into());
        }
        if !meshopt_shape(e["mode"].as_str(), e["filter"].as_str(), count, stride) {
            return Err("meshopt data with a size the format does not allow".into());
        }
        self.unpacked.spend((count * stride) as u64)?;
        let mut out = vec![0u8; count * stride];
        let code = unsafe {
            match e["mode"].as_str() {
                Some("ATTRIBUTES") => meshopt::ffi::meshopt_decodeVertexBuffer(out.as_mut_ptr().cast(), count, stride, src.as_ptr(), src.len()),
                Some("TRIANGLES") => meshopt::ffi::meshopt_decodeIndexBuffer(out.as_mut_ptr().cast(), count, stride, src.as_ptr(), src.len()),
                Some("INDICES") => meshopt::ffi::meshopt_decodeIndexSequence(out.as_mut_ptr().cast(), count, stride, src.as_ptr(), src.len()),
                _ => -1,
            }
        };
        if code != 0 {
            return Err("meshopt data could not be decoded".into());
        }
        // The filters work in place on the decoded vertices.
        unsafe {
            let (ptr, n) = (out.as_mut_ptr().cast(), count);
            match e["filter"].as_str() {
                Some("OCTAHEDRAL") => meshopt::ffi::meshopt_decodeFilterOct(ptr, n, stride),
                Some("QUATERNION") => meshopt::ffi::meshopt_decodeFilterQuat(ptr, n, stride),
                Some("EXPONENTIAL") => meshopt::ffi::meshopt_decodeFilterExp(ptr, n, stride),
                Some("COLOR") => meshopt::ffi::meshopt_decodeFilterColor(ptr, n, stride),
                _ => {}
            }
        }
        self.align();
        let v = &mut self.views[k];
        v["byteOffset"] = json!(self.data.len());
        v["byteLength"] = json!(out.len());
        self.data.extend(out);
        if let Some(m) = v.as_object_mut() {
            m.remove("extensions");
        }
        Ok(())
    }

    /// A primitive compressed with Draco: its indices and attributes as new accessors.
    fn draco(&mut self, prim: &mut Value) -> Result<(), String> {
        let Some(e) = prim["extensions"].get("KHR_draco_mesh_compression").cloned() else { return Ok(()) };
        let v = self.views.get(num(&e["bufferView"])).cloned().unwrap_or_default();
        let at = num(&v["byteOffset"]);
        let src = at.checked_add(num(&v["byteLength"])).and_then(|end| self.data.get(at..end)).ok_or("Draco data outside the buffer")?;
        let mesh = draco_oxide_decoder::decode_mesh(src).map_err(|e| format!("Draco: {e}"))?;
        let indices: Vec<u32> = mesh.faces.iter().flat_map(|f| f.map(|p| usize::from(p) as u32)).collect();
        let points = mesh.faces.iter().flatten().map(|&p| usize::from(p) + 1).max().unwrap_or(0);
        self.unpacked.spend(indices.len() as u64 * 4)?;
        let slot = prim["indices"].as_u64().map(|i| i as usize);
        prim["indices"] = json!(self.accessor(slot, &[], 0, &indices));
        for (semantic, id) in e["attributes"].as_object().cloned().unwrap_or_default() {
            let Some(att) = mesh.attributes.iter().find(|a| a.get_id().as_usize() == num(&id)) else { continue };
            let normalized = prim["attributes"][&semantic].as_u64().and_then(|a| self.accessors.get(a as usize)).is_some_and(|a| a["normalized"].as_bool() == Some(true));
            let n = att.get_num_components();
            // `points` is the largest index in the faces: a made-up one asks for gigabytes.
            if points.checked_mul(n * 4).is_none_or(|b| b > MAX_DECODED) {
                return Err("Draco data too large".into());
            }
            self.unpacked.spend((points * n * 4) as u64)?;
            let values = draco_values(att, points, normalized);
            let slot = prim["attributes"][&semantic].as_u64().map(|i| i as usize);
            prim["attributes"][&semantic] = json!(self.accessor(slot, &values, n, &[]));
        }
        if let Some(m) = prim["extensions"].as_object_mut() {
            m.remove("KHR_draco_mesh_compression");
        }
        Ok(())
    }

    /// Positions, normals, tangents and texture coordinates stored as integers, made floats.
    fn floats(&mut self, prim: &mut Value) -> Result<(), String> {
        let names: Vec<String> = prim["attributes"].as_object().map(Map::keys).into_iter().flatten().filter(|s| ["POSITION", "NORMAL", "TANGENT"].contains(&s.as_str()) || s.starts_with("TEXCOORD_")).cloned().collect();
        for name in names {
            let source = num(&prim["attributes"][&name]);
            if let Some(&k) = self.converted.get(&source) {
                prim["attributes"][&name] = json!(k);
                continue;
            }
            let a = self.accessors.get(source).cloned().unwrap_or_default();
            if a["componentType"].as_u64() == Some(5126) || a.get("sparse").is_some() {
                continue;
            }
            let Some(read) = self.read(&a) else { continue };
            let (values, n) = read?;
            let k = self.accessor(None, &values, n, &[]);
            self.converted.insert(source, k);
            prim["attributes"][&name] = json!(k);
        }
        Ok(())
    }

    /// The numbers of an accessor of integers, as floats (normalized ones to −1..1 or 0..1);
    /// an error when its elements do not fit in the data.
    fn read(&self, a: &Value) -> Option<Result<(Vec<f32>, usize), String>> {
        let n: usize = match a["type"].as_str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let size: usize = match a["componentType"].as_u64()? {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 => 4,
            _ => return None,
        };
        let v = self.views.get(num(&a["bufferView"]))?;
        // Vertex attributes start every element on 4 bytes.
        let stride = match num(&v["byteStride"]) {
            0 => (n * size).div_ceil(4) * 4,
            s => s,
        };
        let (start, count, normalized) = (num(&v["byteOffset"]).saturating_add(num(&a["byteOffset"])), num(&a["count"]), a["normalized"].as_bool() == Some(true));
        let component = a["componentType"].as_u64()?;
        // The count is the file's word: its last element must end inside the data before memory
        // is asked for by it.
        let end = count.checked_sub(1).map_or(Some(0), |last| last.checked_mul(stride)?.checked_add(start)?.checked_add(n * size));
        if end.is_none_or(|end| end > self.read_end) {
            return Some(Err("accessor data outside the buffer".into()));
        }
        if let Err(e) = self.unpacked.spend((count * n * 4) as u64) {
            return Some(Err(e));
        }
        let mut out = Vec::with_capacity(count * n);
        for e in 0..count {
            for c in 0..n {
                let at = start + e * stride + c * size;
                let b = self.data.get(at..at + size)?;
                let (x, max) = match component {
                    5120 => (b[0] as i8 as f32, 127.0),
                    5121 => (b[0] as f32, 255.0),
                    5122 => (i16::from_le_bytes([b[0], b[1]]) as f32, 32767.0),
                    5123 => (u16::from_le_bytes([b[0], b[1]]) as f32, 65535.0),
                    _ => (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32, u32::MAX as f32),
                };
                out.push(if normalized { (x / max).max(-1.0) } else { x });
            }
        }
        Some(Ok((out, n)))
    }
}

/// Whether `count` elements of `stride` bytes are what the meshopt decoder of `mode` and its
/// `filter` expect, as the glTF extension allows. The decoders are C code that trusts these
/// numbers (its own checks are compiled out of a release build): with others they write past
/// the buffer made for them, so a file that names others is refused.
fn meshopt_shape(mode: Option<&str>, filter: Option<&str>, count: usize, stride: usize) -> bool {
    let mode_ok = match mode {
        Some("ATTRIBUTES") => stride.is_multiple_of(4) && stride <= 256,
        Some("TRIANGLES") => count.is_multiple_of(3) && (stride == 2 || stride == 4),
        Some("INDICES") => stride == 2 || stride == 4,
        _ => false,
    };
    let filter_ok = match filter {
        None | Some("NONE") => true,
        // The filters are for vertices only.
        Some(_) if mode != Some("ATTRIBUTES") => false,
        Some("OCTAHEDRAL") | Some("COLOR") => stride == 4 || stride == 8,
        Some("QUATERNION") => stride == 8,
        Some("EXPONENTIAL") => stride.is_multiple_of(4),
        Some(_) => false,
    };
    mode_ok && filter_ok
}

/// A Draco attribute's values at each point, as floats.
fn draco_values(att: &draco_oxide_core::attribute::Attribute, points: usize, normalized: bool) -> Vec<f32> {
    use draco_oxide_core::attribute::ComponentDataType as T;
    let (kind, n) = (att.get_component_type(), att.get_num_components());
    let size = kind.size();
    let bytes = att.get_data_as_bytes();
    let map = att.point_map_as_slice();
    let mut out = Vec::with_capacity(points * n);
    for p in 0..points {
        let value = map.map_or(p, |m| m.get(p).map_or(0, |&v| usize::from(v)));
        for c in 0..n {
            let at = (value * n + c) * size;
            let Some(b) = bytes.get(at..at + size) else {
                out.push(0.0);
                continue;
            };
            let (x, max) = match kind {
                T::F32 => (f32::from_le_bytes([b[0], b[1], b[2], b[3]]), 1.0),
                T::F64 => (f64::from_le_bytes(b.try_into().unwrap_or([0; 8])) as f32, 1.0),
                T::I8 => (b[0] as i8 as f32, 127.0),
                T::U8 => (b[0] as f32, 255.0),
                T::I16 => (i16::from_le_bytes([b[0], b[1]]) as f32, 32767.0),
                T::U16 => (u16::from_le_bytes([b[0], b[1]]) as f32, 65535.0),
                T::I32 => (i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32, 1.0),
                _ => (u32::from_le_bytes(b.get(..4).and_then(|s| s.try_into().ok()).unwrap_or([0; 4])) as f32, 1.0),
            };
            out.push(if normalized && !kind.is_float() { (x / max).max(-1.0) } else { x });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::meshopt_shape;
    use crate::model_3mf::Unpacked;
    /// Only the sizes the format allows reach the C decoders: a file that names others (a stride
    /// of 1 for triangles, vertices wider than 256 bytes, a filter on indices) is refused.
    #[test]
    fn meshopt_sizes_the_format_does_not_allow_are_refused() {
        assert!(meshopt_shape(Some("ATTRIBUTES"), None, 100, 12));
        assert!(meshopt_shape(Some("ATTRIBUTES"), Some("OCTAHEDRAL"), 100, 4));
        assert!(meshopt_shape(Some("ATTRIBUTES"), Some("QUATERNION"), 100, 8));
        assert!(meshopt_shape(Some("ATTRIBUTES"), Some("EXPONENTIAL"), 100, 12));
        assert!(meshopt_shape(Some("TRIANGLES"), Some("NONE"), 300, 2));
        assert!(meshopt_shape(Some("INDICES"), None, 7, 4));
        for (mode, filter, count, stride) in [
            (Some("ATTRIBUTES"), None, 100, 6),
            (Some("ATTRIBUTES"), None, 100, 260),
            (Some("ATTRIBUTES"), Some("OCTAHEDRAL"), 100, 12),
            (Some("ATTRIBUTES"), Some("QUATERNION"), 100, 4),
            (Some("ATTRIBUTES"), Some("EXPONENTIAL"), 100, 6),
            (Some("ATTRIBUTES"), Some("SOMETHING"), 100, 8),
            (Some("TRIANGLES"), None, 300, 1),
            (Some("TRIANGLES"), None, 301, 2),
            (Some("TRIANGLES"), Some("OCTAHEDRAL"), 300, 4),
            (Some("INDICES"), None, 7, 8),
            (None, None, 3, 4),
            (Some("OTHER"), None, 3, 4),
        ] {
            assert!(!meshopt_shape(mode, filter, count, stride), "{mode:?} {filter:?} {count} {stride}");
        }
    }

    /// A triangle with its positions as unnormalized 16-bit integers (KHR_mesh_quantization).
    #[test]
    fn quantized_positions_become_floats() {
        let bin: Vec<u8> = [[0i16, 0, 0], [100, 0, 0], [0, 100, 0]].iter().flat_map(|p| p.iter().chain(&[0i16]).flat_map(|v| v.to_le_bytes())).collect();
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_mesh_quantization"],"extensionsRequired":["KHR_mesh_quantization"],"buffers":[{{"byteLength":{}}}],"bufferViews":[{{"buffer":0,"byteLength":{},"byteStride":8}}],"accessors":[{{"bufferView":0,"componentType":5122,"count":3,"type":"VEC3","min":[0,0,0],"max":[100,100,0]}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"nodes":[{{"mesh":0,"scale":[0.01,0.01,0.01]}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,
            bin.len(),
            bin.len()
        );
        let glb = super::glb(json.as_bytes(), bin);
        let plain = super::plain(&glb, &|_| None, &Unpacked::default()).unwrap().unwrap();
        let m = crate::model::parse(&plain, "q.glb").unwrap();
        assert_eq!(m.tris.len(), 1);
        assert!((m.radius - 2f32.sqrt() / 2.0).abs() < 1e-4, "{}", m.radius);
        // A plain file is left alone.
        assert!(super::plain(b"{}", &|_| None, &Unpacked::default()).unwrap().is_none());
    }

    /// Audit 6.4 (a): an accessor that declares a million million elements over a buffer of 24
    /// bytes must be refused before memory is asked for by that count (12 TB).
    #[test]
    fn audit_6_4_gltf_count() {
        let bin = vec![0u8; 24];
        let json = r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_mesh_quantization"],"extensionsRequired":["KHR_mesh_quantization"],"buffers":[{"byteLength":24}],"bufferViews":[{"buffer":0,"byteLength":24,"byteStride":8}],"accessors":[{"bufferView":0,"componentType":5122,"count":1000000000000,"type":"VEC3","min":[0,0,0],"max":[100,100,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
        let glb = super::glb(json.as_bytes(), bin);
        println!("GLB of {} bytes", glb.len());
        assert!(crate::model::parse(&glb, "q.glb").is_err());
    }

    /// Audit 6.4 (a), second way: each attribute made floats is appended to the data, so a count
    /// checked against the data as it grows lets every attribute ask for five times the one
    /// before. The counts may only reach the file's own data.
    #[test]
    fn audit_6_4_gltf_grown_buffer() {
        let bin = vec![0u8; 24];
        // 6 elements of 4 bytes fill the 24; after them 96 bytes of floats are appended, and 30
        // elements would fill the 120 bytes there are then.
        let json = r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_mesh_quantization"],"buffers":[{"byteLength":24}],"bufferViews":[{"buffer":0,"byteLength":24}],"accessors":[{"bufferView":0,"componentType":5121,"count":6,"type":"VEC4"},{"bufferView":0,"componentType":5121,"count":30,"type":"VEC4"}],"meshes":[{"primitives":[{"attributes":{"TEXCOORD_0":0,"TEXCOORD_1":1}}]}]}"#;
        assert!(super::plain(&super::glb(json.as_bytes(), bin), &|_| None, &Unpacked::default()).is_err());
    }

    /// A GLB with a triangle and `views` views of 64 KB compressed with meshopt (4096 vertices of
    /// 16 bytes), all decoded from the same few bytes of the file at `offset`.
    fn meshopt_views(views: usize, offset: u64) -> Vec<u8> {
        let (count, stride) = (4096usize, 16usize);
        let zeros = vec![0u8; count * stride];
        let mut packed = vec![0u8; unsafe { meshopt::ffi::meshopt_encodeVertexBufferBound(count, stride) }];
        let n = unsafe { meshopt::ffi::meshopt_encodeVertexBuffer(packed.as_mut_ptr(), packed.len(), zeros.as_ptr().cast(), count, stride) };
        // A triangle in plain floats, then the packed zeros.
        let mut bin: Vec<u8> = [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
        bin.extend(&packed[..n]);
        let size = count * stride;
        let view = format!(r#",{{"buffer":1,"byteLength":{size},"byteStride":{stride},"extensions":{{"EXT_meshopt_compression":{{"buffer":0,"byteOffset":{offset},"byteLength":{n},"byteStride":{stride},"count":{count},"mode":"ATTRIBUTES"}}}}}}"#);
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["EXT_meshopt_compression"],"extensionsRequired":["EXT_meshopt_compression"],"buffers":[{{"byteLength":{}}},{{"byteLength":{size},"extensions":{{"EXT_meshopt_compression":{{"fallback":true}}}}}}],"bufferViews":[{{"buffer":0,"byteLength":36}}{}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,
            bin.len(),
            view.repeat(views)
        );
        super::glb(json.as_bytes(), bin)
    }

    /// Audit 6.19 (c): what meshopt decodes is limited for the whole file, not only for each view:
    /// 8 views of 128 MB took 2 GB from a GLB of 134 KB, each far under its own limit. The same at
    /// a smaller scale, with a limit of 256 KB instead of `MAX_UNPACKED`: two views of 64 KB are
    /// decoded, eight are refused. Long before the limit, such a file passes `MAX_RATIO`.
    #[test]
    fn audit_6_19_meshopt_total() {
        let limit = 256 << 10;
        let two = super::plain(&meshopt_views(2, 36), &|_| None, &Unpacked::new(limit)).unwrap().unwrap();
        assert_eq!(crate::model::parse(&two, "m.glb").unwrap().tris.len(), 1);
        let r = super::plain(&meshopt_views(8, 36), &|_| None, &Unpacked::new(limit));
        assert!(r.as_ref().is_err_and(|e| e.contains("unpacks")), "{:?}", r.map(|g| g.map(|g| g.len())));
        // And by the ratio, past what any file may unpack (64 KB here instead of `RATIO_FREE`):
        // 128 KB from a GLB of 1 KB.
        let two = meshopt_views(2, 36);
        let r = super::plain(&two, &|_| None, &Unpacked::with(two.len(), crate::model_3mf::MAX_UNPACKED, 64 << 10));
        assert!(r.as_ref().is_err_and(|e| e.contains("times its size")), "{:?}", r.map(|g| g.map(|g| g.len())));
    }

    /// Offsets and parts that are not what the format says are refused, without a panic: a meshopt
    /// view at the end of the numbers, a view and a mesh that are numbers and not objects.
    #[test]
    fn offsets_and_parts_out_of_shape_are_refused() {
        assert!(super::plain(&meshopt_views(1, u64::MAX), &|_| None, &Unpacked::default()).is_err());
        let json = r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_mesh_quantization"],"buffers":[{"byteLength":4,"byteOffset":18446744073709551615}],"bufferViews":[1,{"buffer":0,"byteOffset":18446744073709551615,"byteLength":4}],"meshes":[5,{"primitives":[7,{"attributes":3}]}]}"#;
        let plain = super::plain(&super::glb(json.as_bytes(), vec![0; 4]), &|_| None, &Unpacked::default());
        assert!(plain.is_ok_and(|g| crate::model::parse(&g.unwrap(), "q.glb").is_err()));
    }

    /// A packed glTF that does not list its packing as required (only as used) is read: the list
    /// left out came back as `null`, refused as "glTF 1.0 is not supported".
    #[test]
    fn packing_used_but_not_required_is_read() {
        let bin: Vec<u8> = [[0i16, 0, 0], [100, 0, 0], [0, 100, 0]].iter().flat_map(|p| p.iter().chain(&[0i16]).flat_map(|v| v.to_le_bytes())).collect();
        let json = r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_mesh_quantization"],"buffers":[{"byteLength":24}],"bufferViews":[{"buffer":0,"byteLength":24,"byteStride":8}],"accessors":[{"bufferView":0,"componentType":5122,"count":3,"type":"VEC3","min":[0,0,0],"max":[100,100,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
        assert_eq!(crate::model::parse(&super::glb(json.as_bytes(), bin), "q.glb").map(|m| m.tris.len()), Ok(1));
    }

    /// An accessor of integers named by many primitives is made floats once (it was once for each
    /// time it is named: a total too).
    #[test]
    fn an_accessor_named_many_times_is_made_floats_once() {
        let bin: Vec<u8> = [[0i16, 0, 0], [100, 0, 0], [0, 100, 0]].iter().flat_map(|p| p.iter().chain(&[0i16]).flat_map(|v| v.to_le_bytes())).collect();
        let prims = vec![r#"{"attributes":{"POSITION":0}}"#; 50].join(",");
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_mesh_quantization"],"buffers":[{{"byteLength":24}}],"bufferViews":[{{"buffer":0,"byteLength":24,"byteStride":8}}],"accessors":[{{"bufferView":0,"componentType":5122,"count":3,"type":"VEC3","min":[0,0,0],"max":[100,100,0]}}],"meshes":[{{"primitives":[{prims}]}}],"nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
        );
        // 24 bytes of the file and 36 of floats: 50 times the floats would not fit.
        let plain = super::plain(&super::glb(json.as_bytes(), bin), &|_| None, &Unpacked::new(24 + 36 * 2)).unwrap().unwrap();
        assert_eq!(crate::model::parse(&plain, "q.glb").unwrap().tris.len(), 50);
    }
}
