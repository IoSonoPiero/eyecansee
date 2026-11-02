//! glTF made smaller (Draco geometry, meshopt buffers, quantized numbers) turned into a plain GLB
//! in memory for `model_gltf`: every buffer one after the other in one, the compressed parts
//! decoded after them, the vertices' numbers as floats.
// ponytail: sparse accessors and morph targets are not unpacked

use base64::Engine;
use serde_json::{Map, Value, json};

use crate::model::Sibling;

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

/// A plain GLB, or `None` when the file uses none of the packings.
pub fn plain(bytes: &[u8], sibling: Sibling) -> Result<Option<Vec<u8>>, String> {
    let Some((json, bin)) = split(bytes) else { return Ok(None) };
    let Ok(mut j) = serde_json::from_slice::<Value>(json) else { return Ok(None) };
    let used = j["extensionsUsed"].as_array().is_some_and(|a| a.iter().any(|e| PACKED.contains(&e.as_str().unwrap_or(""))));
    if !used {
        return Ok(None);
    }
    let mut p = Packed { data: Vec::new(), views: j["bufferViews"].as_array().cloned().unwrap_or_default(), accessors: j["accessors"].as_array().cloned().unwrap_or_default() };
    // Every buffer into one, each view moved to where its buffer landed.
    let mut base = Vec::new();
    for (i, buf) in j["buffers"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let bytes = match buf["uri"].as_str() {
            None => bin.filter(|_| i == 0).map(<[u8]>::to_vec),
            Some(u) if u.starts_with("data:") => u.split_once(";base64,").and_then(|(_, d)| base64::engine::general_purpose::STANDARD.decode(d).ok()),
            Some(u) => sibling(u),
        };
        p.align();
        base.push(p.data.len());
        p.data.extend(bytes.unwrap_or_default());
    }
    for v in &mut p.views {
        let at = base.get(num(&v["buffer"])).copied().unwrap_or(0);
        v["byteOffset"] = json!(num(&v["byteOffset"]) + at);
        v["buffer"] = json!(0);
    }
    for k in 0..p.views.len() {
        p.meshopt(k, &base)?;
    }
    let mut meshes = j["meshes"].as_array().cloned().unwrap_or_default();
    for mesh in &mut meshes {
        for prim in mesh["primitives"].as_array_mut().into_iter().flatten() {
            p.draco(prim)?;
            p.floats(prim);
        }
    }
    j["meshes"] = json!(meshes);
    j["bufferViews"] = json!(p.views);
    j["accessors"] = json!(p.accessors);
    j["buffers"] = json!([{ "byteLength": p.data.len() }]);
    for list in ["extensionsUsed", "extensionsRequired"] {
        if let Some(a) = j[list].as_array_mut() {
            a.retain(|e| !PACKED.contains(&e.as_str().unwrap_or("")));
        }
    }
    Ok(Some(glb(&serde_json::to_vec(&j).map_err(|e| e.to_string())?, p.data)))
}

fn glb(json: &[u8], mut bin: Vec<u8>) -> Vec<u8> {
    let mut json = json.to_vec();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut out = b"glTF".to_vec();
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
struct Packed {
    data: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Packed {
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
        let at = base.get(num(&e["buffer"])).copied().unwrap_or(0) + num(&e["byteOffset"]);
        let src = self.data.get(at..at + num(&e["byteLength"])).ok_or("meshopt data outside the buffer")?.to_vec();
        let (count, stride) = (num(&e["count"]), num(&e["byteStride"]));
        if count.saturating_mul(stride) > MAX_DECODED || stride == 0 {
            return Err("meshopt data too large".into());
        }
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
        let src = self.data.get(at..at + num(&v["byteLength"])).ok_or("Draco data outside the buffer")?;
        let mesh = draco_oxide_decoder::decode_mesh(src).map_err(|e| format!("Draco: {e}"))?;
        let indices: Vec<u32> = mesh.faces.iter().flat_map(|f| f.map(|p| usize::from(p) as u32)).collect();
        let points = mesh.faces.iter().flatten().map(|&p| usize::from(p) + 1).max().unwrap_or(0);
        let slot = prim["indices"].as_u64().map(|i| i as usize);
        prim["indices"] = json!(self.accessor(slot, &[], 0, &indices));
        for (semantic, id) in e["attributes"].as_object().cloned().unwrap_or_default() {
            let Some(att) = mesh.attributes.iter().find(|a| a.get_id().as_usize() == num(&id)) else { continue };
            let normalized = prim["attributes"][&semantic].as_u64().and_then(|a| self.accessors.get(a as usize)).is_some_and(|a| a["normalized"].as_bool() == Some(true));
            let n = att.get_num_components();
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
    fn floats(&mut self, prim: &mut Value) {
        let names: Vec<String> = prim["attributes"].as_object().map(Map::keys).into_iter().flatten().filter(|s| ["POSITION", "NORMAL", "TANGENT"].contains(&s.as_str()) || s.starts_with("TEXCOORD_")).cloned().collect();
        for name in names {
            let a = self.accessors.get(num(&prim["attributes"][&name])).cloned().unwrap_or_default();
            if a["componentType"].as_u64() == Some(5126) || a.get("sparse").is_some() {
                continue;
            }
            let Some((values, n)) = self.read(&a) else { continue };
            prim["attributes"][&name] = json!(self.accessor(None, &values, n, &[]));
        }
    }

    /// The numbers of an accessor of integers, as floats (normalized ones to −1..1 or 0..1).
    fn read(&self, a: &Value) -> Option<(Vec<f32>, usize)> {
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
        let (start, count, normalized) = (num(&v["byteOffset"]) + num(&a["byteOffset"]), num(&a["count"]), a["normalized"].as_bool() == Some(true));
        let component = a["componentType"].as_u64()?;
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
        Some((out, n))
    }
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
        let plain = super::plain(&glb, &|_| None).unwrap().unwrap();
        let m = crate::model::parse(&plain, "q.glb").unwrap();
        assert_eq!(m.tris.len(), 1);
        assert!((m.radius - 2f32.sqrt() / 2.0).abs() < 1e-4, "{}", m.radius);
        // A plain file is left alone.
        assert!(super::plain(b"{}", &|_| None).unwrap().is_none());
    }
}
