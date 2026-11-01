//! 3MF (3D printing): a zip with the model in XML. Objects with a mesh or made of components
//! (also in other model files of the zip, Production extension), placed by the build items with
//! their transforms; colours from base materials and colour groups.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use roxmltree::{Document, Node};

use crate::model::{Builder, Mesh};

/// Model files larger than this are refused (zip bombs): ~10 M triangles take ~1 GB of XML.
const MAX_XML: u64 = 2 << 30;

/// Row-major 4×3, as 3MF writes it: a point is `[x y z 1] × M`.
type Transform = [f32; 12];
const IDENTITY: Transform = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn then(a: &Transform, b: &Transform) -> Transform {
    // a first, then b: rows of a × the 3×3 of b, plus b's translation for the last row.
    let mut m = [0.0; 12];
    for r in 0..4 {
        for c in 0..3 {
            m[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum::<f32>() + if r == 3 { b[9 + c] } else { 0.0 };
        }
    }
    m
}

fn apply(m: &Transform, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|c| p[0] * m[c] + p[1] * m[3 + c] + p[2] * m[6 + c] + m[9 + c])
}

fn transform(node: Node) -> Transform {
    let v: Vec<f32> = node.attribute("transform").unwrap_or("").split_whitespace().filter_map(|s| s.parse().ok()).collect();
    v.try_into().unwrap_or(IDENTITY)
}

pub fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let rels = entry(&mut zip, "_rels/.rels").unwrap_or_default();
    let main = Document::parse(&rels)
        .ok()
        .and_then(|d| d.descendants().find(|n| n.attribute("Type").is_some_and(|t| t.ends_with("/3dmodel"))).and_then(|n| n.attribute("Target")).map(str::to_string))
        .unwrap_or_else(|| "/3D/3dmodel.model".into());
    let mut files = Files { zip, texts: HashMap::new() };
    let text = files.text(&main)?;
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;
    let mut b = Builder::default();
    let model = Model::new(&doc);
    for item in doc.descendants().filter(|n| n.has_tag_name_local("item")) {
        let Some(id) = item.attribute("objectid") else { continue };
        let path = item.attribute_local("path").unwrap_or(&main).to_string();
        place(&mut b, &mut files, &main, &model, &path, id, &transform(item), 0)?;
    }
    b.finish(true)
}

/// The text files of the zip, read once.
struct Files<'b> {
    zip: zip::ZipArchive<Cursor<&'b [u8]>>,
    texts: HashMap<String, String>,
}

impl Files<'_> {
    fn text(&mut self, path: &str) -> Result<String, String> {
        if let Some(t) = self.texts.get(path) {
            return Ok(t.clone());
        }
        let t = entry(&mut self.zip, path.trim_start_matches('/')).ok_or_else(|| format!("{path} missing in the file"))?;
        self.texts.insert(path.to_string(), t.clone());
        Ok(t)
    }
}

fn entry<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let mut s = String::new();
    zip.by_name(name).ok()?.take(MAX_XML).read_to_string(&mut s).ok()?;
    Some(s)
}

/// Objects and colours of one model file, by id.
struct Model<'a, 'i> {
    objects: HashMap<&'a str, Node<'a, 'i>>,
    colors: HashMap<&'a str, Vec<[u8; 3]>>,
}

impl<'a, 'i> Model<'a, 'i> {
    fn new(doc: &'a Document<'i>) -> Self {
        let mut objects = HashMap::new();
        let mut colors = HashMap::new();
        for n in doc.descendants() {
            let Some(id) = n.attribute("id") else { continue };
            match n.tag_name().name() {
                "object" => {
                    objects.insert(id, n);
                }
                "basematerials" | "colorgroup" => {
                    let list = n.children().filter_map(|c| c.attribute("displaycolor").or_else(|| c.attribute("color")).and_then(hex)).collect();
                    colors.insert(id, list);
                }
                _ => {}
            }
        }
        Model { objects, colors }
    }

    fn color(&self, pid: Option<&str>, index: Option<&str>) -> Option<[u8; 3]> {
        self.colors.get(pid?)?.get(index?.parse::<usize>().ok()?).copied()
    }
}

/// `#RRGGBB` or `#RRGGBBAA`.
fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

trait Local {
    fn has_tag_name_local(&self, name: &str) -> bool;
    fn attribute_local(&self, name: &str) -> Option<&str>;
}

impl Local for Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name() == name
    }
    /// An attribute by its name without the namespace (`p:path`).
    fn attribute_local(&self, name: &str) -> Option<&str> {
        self.attributes().find(|a| a.name() == name).map(|a| a.value())
    }
}

/// Adds object `id` of the model file `path`, moved by `m`.
#[allow(clippy::too_many_arguments)]
fn place(b: &mut Builder, files: &mut Files, main: &str, model: &Model, path: &str, id: &str, m: &Transform, depth: u32) -> Result<(), String> {
    if depth > 32 {
        return Err("components nested too deep".into());
    }
    if path != main {
        let text = files.text(path)?;
        let doc = Document::parse(&text).map_err(|e| e.to_string())?;
        let other = Model::new(&doc);
        return place(b, files, path, &other, path, id, m, depth + 1);
    }
    let Some(object) = model.objects.get(id) else { return Ok(()) };
    if let Some(mesh) = object.children().find(|n| n.has_tag_name_local("mesh")) {
        mesh_part(b, model, *object, mesh, m)?;
    }
    for c in object.descendants().filter(|n| n.has_tag_name_local("component")) {
        let Some(child) = c.attribute("objectid") else { continue };
        let child_path = c.attribute_local("path").unwrap_or(main).to_string();
        place(b, files, main, model, &child_path, child, &then(&transform(c), m), depth + 1)?;
    }
    Ok(())
}

fn mesh_part(b: &mut Builder, model: &Model, object: Node, mesh: Node, m: &Transform) -> Result<(), String> {
    let num = |n: Node, k: &str| n.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let pos = mesh.descendants().filter(|n| n.has_tag_name_local("vertex")).map(|v| apply(m, [num(v, "x"), num(v, "y"), num(v, "z")])).collect();
    let triangles: Vec<Node> = mesh.descendants().filter(|n| n.has_tag_name_local("triangle")).collect();
    let index = |n: Node, k: &str| n.attribute(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(u32::MAX);
    let tris: Vec<[u32; 3]> = triangles.iter().map(|&t| [index(t, "v1"), index(t, "v2"), index(t, "v3")]).collect();
    let (opid, oindex) = (object.attribute("pid"), object.attribute("pindex"));
    let colors = |i: usize| {
        let t = triangles[i];
        model.color(t.attribute("pid").or(opid), t.attribute("p1").or(oindex))
    };
    b.add(pos, tris, colors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip(model: &str) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(model.as_bytes()).unwrap();
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn components_build_transforms_and_colours() {
        let model = r##"<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>
            <basematerials id="5"><base name="r" displaycolor="#FF0000"/><base name="g" displaycolor="#00FF00FF"/></basematerials>
            <object id="1" pid="5" pindex="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
            <triangles><triangle v1="0" v2="1" v3="2"/><triangle v1="0" v2="1" v3="2" p1="0"/><triangle v1="0" v2="1" v3="9"/></triangles></mesh></object>
            <object id="2"><components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 10 0 0"/></components></object>
            </resources><build><item objectid="2" transform="2 0 0 0 2 0 0 0 2 0 0 5"/></build></model>"##;
        let m = read(&zip(model)).unwrap();
        // The triangle with vertex 9 is dropped; component moved by 10 then everything scaled by 2 and raised by 5.
        assert_eq!(m.tris.len(), 2);
        assert_eq!(m.pos[1], [22.0, 0.0, 5.0]);
        assert_eq!(m.colors, vec![[0, 255, 0], [255, 0, 0]]);
        assert!(m.z_up);
    }

    #[test]
    fn transforms_compose_in_order() {
        let scale: Transform = [2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0];
        let shift: Transform = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        assert_eq!(apply(&then(&shift, &scale), [1.0, 0.0, 0.0]), [4.0, 0.0, 0.0]);
        assert_eq!(apply(&then(&scale, &shift), [1.0, 0.0, 0.0]), [3.0, 0.0, 0.0]);
        assert!(read(b"not a zip").is_err());
    }
}
