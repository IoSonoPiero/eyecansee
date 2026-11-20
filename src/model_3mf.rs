//! 3MF (3D printing): a zip with the model in XML. Objects with a mesh or made of components
//! (also in other model files of the zip, Production extension), placed by the build items with
//! their transforms; colours from base materials and colour groups, pictures from texture groups
//! (Materials extension).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::rc::Rc;

use roxmltree::{Document, Node};

use crate::model::{Builder, Mesh};

/// What one model file may unpack in all: the files of its zip, its gzip, the meshopt and Draco
/// data of a glTF and its numbers made floats. As much as one file of a zip could unpack to
/// before, so whatever opened then still opens (~10 M triangles take ~1 GB of XML); a file of a
/// few KB made on purpose unpacked 1.6 GB from 12 files of 128 MB, or 2 GB from 8 meshopt views.
pub(crate) const MAX_UNPACKED: u64 = 2 << 30;

/// The count of `MAX_UNPACKED` for one file being opened, shared by the formats it holds.
pub(crate) struct Unpacked {
    left: Cell<u64>,
    over: Cell<bool>,
}

impl Default for Unpacked {
    fn default() -> Self {
        Unpacked::new(MAX_UNPACKED)
    }
}

impl Unpacked {
    pub(crate) fn new(limit: u64) -> Self {
        Unpacked { left: Cell::new(limit), over: Cell::new(false) }
    }

    /// `n` more bytes made from the file; an error once they pass the limit.
    pub(crate) fn spend(&self, n: u64) -> Result<(), String> {
        match self.left.get().checked_sub(n) {
            Some(left) if !self.over.get() => {
                self.left.set(left);
                Ok(())
            }
            _ => {
                self.over.set(true);
                Err(format!("the file unpacks to more than {} MB", MAX_UNPACKED >> 20))
            }
        }
    }

    /// All that `r` unpacks, counted as it comes out: the sizes a zip declares are not trusted.
    pub(crate) fn read(&self, r: impl Read) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        r.take(self.left.get().saturating_add(1)).read_to_end(&mut out).map_err(|e| e.to_string())?;
        self.spend(out.len() as u64)?;
        // Without the room it grew by (up to as much again).
        out.shrink_to_fit();
        Ok(out)
    }

    /// An error when something read on the way (a picture, which is left out when unreadable)
    /// passed the limit.
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.over.get() { self.spend(1) } else { Ok(()) }
    }
}

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

pub fn read(bytes: &[u8], unpacked: &Unpacked) -> Result<Mesh, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let rels = entry(&mut zip, "_rels/.rels", unpacked).unwrap_or_default();
    let main = Document::parse(&rels)
        .ok()
        .and_then(|d| d.descendants().find(|n| n.attribute("Type").is_some_and(|t| t.ends_with("/3dmodel"))).and_then(|n| n.attribute("Target")).map(str::to_string))
        .unwrap_or_else(|| "/3D/3dmodel.model".into());
    let mut files = Files { zip, unpacked, texts: HashMap::new(), pictures: HashMap::new() };
    let text = files.text(&main)?;
    let doc = Document::parse(&text).map_err(|e| e.to_string())?;
    let mut b = Builder::default();
    let model = Model::new(&doc);
    for item in doc.descendants().filter(|n| n.has_tag_name_local("item")) {
        let Some(id) = item.attribute("objectid") else { continue };
        let path = item.attribute_local("path").unwrap_or(&main).to_string();
        place(&mut b, &mut files, &main, &model, &path, id, &transform(item), 0)?;
    }
    files.unpacked.check()?;
    let mut mesh = b.finish(true)?;
    mesh.unit = crate::model::named_unit(doc.root_element().attribute("unit"));
    Ok(mesh)
}

/// The files of the zip, each read once however it is spelled (`/3D/a.model`, `//3D/a.model`).
struct Files<'b, 'u> {
    zip: zip::ZipArchive<Cursor<&'b [u8]>>,
    unpacked: &'u Unpacked,
    /// Texts and pictures by their place in the zip.
    texts: HashMap<usize, Rc<String>>,
    pictures: HashMap<usize, u16>,
}

impl Files<'_, '_> {
    /// Where a path of the model is in the zip.
    fn index(&self, path: &str) -> Option<usize> {
        self.zip.index_for_name(path.trim_start_matches('/'))
    }

    fn text(&mut self, path: &str) -> Result<Rc<String>, String> {
        let i = self.index(path).ok_or_else(|| format!("{path} missing in the file"))?;
        if let Some(t) = self.texts.get(&i) {
            return Ok(t.clone());
        }
        let t = Rc::new(utf8(self.unpacked.read(self.zip.by_index(i).map_err(|e| e.to_string())?)?, path)?);
        self.texts.insert(i, t.clone());
        Ok(t)
    }

    /// A picture of the zip, into the model once (`Builder::texture_file`).
    fn picture(&mut self, b: &mut Builder, path: &str) -> u16 {
        let Some(i) = self.index(path) else { return 0 };
        if let Some(&k) = self.pictures.get(&i) {
            return k;
        }
        let data = RefCell::new(self.zip.by_index(i).ok().and_then(|f| self.unpacked.read(f).ok()));
        let k = b.texture_file(path, &|_| data.borrow_mut().take());
        self.pictures.insert(i, k);
        k
    }
}

fn utf8(bytes: Vec<u8>, name: &str) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{name} unreadable"))
}

/// The text of the zip's file `name`, counted by `unpacked`.
pub(crate) fn entry<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, name: &str, unpacked: &Unpacked) -> Result<String, String> {
    let file = zip.by_name(name).map_err(|_| format!("{name} missing in the file"))?;
    utf8(unpacked.read(file)?, name)
}

/// Objects and colours of one model file, by id.
struct Model<'a, 'i> {
    objects: HashMap<&'a str, Node<'a, 'i>>,
    colors: HashMap<&'a str, Vec<[u8; 3]>>,
    /// Texture groups: their picture's path and their points on it (V downwards, as rows).
    textures: HashMap<&'a str, (&'a str, Vec<[f32; 2]>)>,
}

impl<'a, 'i> Model<'a, 'i> {
    fn new(doc: &'a Document<'i>) -> Self {
        let mut objects = HashMap::new();
        let mut colors = HashMap::new();
        let mut textures = HashMap::new();
        let paths: HashMap<&str, &str> = doc.descendants().filter(|n| n.has_tag_name_local("texture2d")).filter_map(|n| Some((n.attribute("id")?, n.attribute("path")?))).collect();
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
                "texture2dgroup" => {
                    let Some(path) = n.attribute("texid").and_then(|t| paths.get(t)) else { continue };
                    let num = |c: Node, k: &str| c.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                    // (0, 0) is the picture's lower left corner.
                    let points = n.children().filter(|c| c.has_tag_name_local("tex2coord")).map(|c| [num(c, "u"), 1.0 - num(c, "v")]).collect();
                    textures.insert(id, (*path, points));
                }
                _ => {}
            }
        }
        Model { objects, colors, textures }
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

pub(crate) trait Local {
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
    b.visit()?;
    if files.index(path).is_none() || files.index(path) != files.index(main) {
        let text = files.text(path)?;
        let doc = Document::parse(&text).map_err(|e| e.to_string())?;
        let other = Model::new(&doc);
        return place(b, files, path, &other, path, id, m, depth + 1);
    }
    let Some(object) = model.objects.get(id) else { return Ok(()) };
    if let Some(mesh) = object.children().find(|n| n.has_tag_name_local("mesh")) {
        mesh_part(b, files, model, *object, mesh, m)?;
    }
    for c in object.descendants().filter(|n| n.has_tag_name_local("component")) {
        let Some(child) = c.attribute("objectid") else { continue };
        let child_path = c.attribute_local("path").unwrap_or(main).to_string();
        place(b, files, main, model, &child_path, child, &then(&transform(c), m), depth + 1)?;
    }
    Ok(())
}

fn mesh_part(b: &mut Builder, files: &mut Files, model: &Model, object: Node, mesh: Node, m: &Transform) -> Result<(), String> {
    let num = |n: Node, k: &str| n.attribute(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(f32::NAN);
    let pos = mesh.descendants().filter(|n| n.has_tag_name_local("vertex")).map(|v| apply(m, [num(v, "x"), num(v, "y"), num(v, "z")])).collect();
    let triangles: Vec<Node> = mesh.descendants().filter(|n| n.has_tag_name_local("triangle")).collect();
    let index = |n: Node, k: &str| n.attribute(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(u32::MAX);
    let tris: Vec<[u32; 3]> = triangles.iter().map(|&t| [index(t, "v1"), index(t, "v2"), index(t, "v3")]).collect();
    let (opid, oindex) = (object.attribute("pid"), object.attribute("pindex"));
    let pids: Vec<Option<&str>> = triangles.iter().map(|t| t.attribute("pid").or(opid)).collect();
    // The pictures of the texture groups this mesh uses.
    let mut pictures: HashMap<&str, u16> = HashMap::new();
    for &g in pids.iter().flatten() {
        if let (false, Some((path, _))) = (pictures.contains_key(g), model.textures.get(g)) {
            pictures.insert(g, files.picture(b, path));
        }
    }
    let place = |i: usize| {
        let (t, g) = (triangles[i], pids[i]?);
        let (_, points) = model.textures.get(g)?;
        let first = t.attribute("p1").or(oindex);
        let corner = |k: &str| t.attribute(k).or(first).and_then(|p| points.get(p.parse::<usize>().ok()?)).copied();
        Some(([corner("p1")?, corner("p2")?, corner("p3")?], *pictures.get(g)?))
    };
    let colors = |i: usize| {
        let t = triangles[i];
        if place(i).is_some_and(|(_, k)| k > 0) {
            return Some([255; 3]);
        }
        model.color(pids[i], t.attribute("p1").or(oindex))
    };
    b.add_textured(pos, tris, colors, place)
}

/// XML of 3D formats: a DTD is allowed (X3D has one), its entities stay limited by roxmltree.
pub(crate) fn xml_options<'a>() -> roxmltree::ParsingOptions<'a> {
    roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }
}

/// The first file of the zip whose name ends with `ext` (AMF, Collada's ZAE).
pub(crate) fn unzipped(bytes: &[u8], ext: &str, unpacked: &Unpacked) -> Result<String, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let name = zip.file_names().find(|n| n.to_lowercase().ends_with(ext)).map(str::to_string).ok_or_else(|| format!("no {ext} file in the zip"))?;
    entry(&mut zip, &name, unpacked)
}

/// The text of an XML file: UTF-8 (with or without its mark) or UTF-16 with its mark.
pub(crate) fn text(bytes: &[u8]) -> String {
    let utf16 = |be: bool| char::decode_utf16(bytes[2..].as_chunks::<2>().0.iter().map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })).map(|c| c.unwrap_or('\u{FFFD}')).collect();
    match bytes {
        [0xFF, 0xFE, ..] => utf16(false),
        [0xFE, 0xFF, ..] => utf16(true),
        _ => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned(),
    }
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
        let m = read(&zip(model), &Unpacked::default()).unwrap();
        // The triangle with vertex 9 is dropped; component moved by 10 then everything scaled by 2 and raised by 5.
        assert_eq!(m.tris.len(), 2);
        assert_eq!(m.pos[1], [22.0, 0.0, 5.0]);
        assert_eq!(m.colors, vec![[0, 255, 0], [255, 0, 0]]);
        assert!(m.z_up);
    }

    #[test]
    fn a_texture_group() {
        let model = r##"<model><resources><m:texture2d id="7" path="/3D/Textures/t.png"/>
            <m:texture2dgroup id="8" texid="7"><m:tex2coord u="0" v="0"/><m:tex2coord u="1" v="0"/><m:tex2coord u="0" v="1"/></m:texture2dgroup>
            <object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
            <triangles><triangle v1="0" v2="1" v3="2" pid="8" p1="0" p2="1" p3="2"/></triangles></mesh></object>
            </resources><build><item objectid="1"/></build></model>"##.replace("<model>", r#"<model xmlns:m="m">"#);
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(model.as_bytes()).unwrap();
        w.start_file("3D/Textures/t.png", zip::write::SimpleFileOptions::default()).unwrap();
        let mut png = Vec::new();
        image::RgbaImage::new(2, 2).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        w.write_all(&png).unwrap();
        let m = read(&w.finish().unwrap().into_inner(), &Unpacked::default()).unwrap();
        assert_eq!((m.tex.clone(), m.colors.clone(), m.uv[0]), (vec![1], vec![[255; 3]], [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0]]));
    }

    #[test]
    fn transforms_compose_in_order() {
        let scale: Transform = [2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0];
        let shift: Transform = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        assert_eq!(apply(&then(&shift, &scale), [1.0, 0.0, 0.0]), [4.0, 0.0, 0.0]);
        assert_eq!(apply(&then(&scale, &shift), [1.0, 0.0, 0.0]), [3.0, 0.0, 0.0]);
        assert!(read(b"not a zip", &Unpacked::default()).is_err());
    }

    /// Audit 6.6: objects made of ten components each, the next object ten times, 32 deep, down
    /// to an object without a mesh: 10^32 placements in a zip of a few hundred bytes. It must end
    /// (refused, or found empty) without visiting them all. `AUDIT_N` = fewer levels, to measure
    /// how the time grows. Run alone, with a time limit.
    #[test]
    fn audit_6_6_3mf() {
        let levels: u32 = std::env::var("AUDIT_N").ok().and_then(|v| v.parse().ok()).unwrap_or(32);
        let mut model = String::from("<model><resources>");
        for k in 0..levels {
            let components = format!(r#"<component objectid="{}"/>"#, k + 1).repeat(10);
            model += &format!(r#"<object id="{k}"><components>{components}</components></object>"#);
        }
        model += &format!(r#"<object id="{levels}"/></resources><build><item objectid="0"/></build></model>"#);
        let bytes = zip(&model);
        println!("3MF of {} bytes ({} of XML), 10^{levels} placements", bytes.len(), model.len());
        let t = std::time::Instant::now();
        let r = crate::model::parse(&bytes, "a.3mf");
        println!("ended in {:.2?}: {:?}", t.elapsed(), r.as_ref().map(|m| m.tris.len()));
        assert!(r.is_err());
    }

    /// A 3MF whose build places object 1 of each model file in `paths` (named as given), each
    /// file a triangle and a comment of `size` bytes, written once for each name in the zip.
    fn parts(paths: &[String], size: usize) -> Vec<u8> {
        let items: String = paths.iter().map(|p| format!(r#"<item objectid="1" p:path="{p}"/>"#)).collect();
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(format!(r#"<model xmlns:p="p"><resources/><build>{items}</build></model>"#).as_bytes()).unwrap();
        let triangle = r#"<vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles>"#;
        let part = format!(r#"<model><resources><object id="1"><mesh>{triangle}</mesh></object></resources><!--{}--></model>"#, "A".repeat(size));
        let mut names: Vec<&str> = paths.iter().map(|p| p.trim_start_matches('/')).collect();
        names.sort_unstable();
        names.dedup();
        for name in names {
            w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(part.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    /// Audit 6.19 (a): what a 3MF unpacks is limited in all, counted as it comes out, not for
    /// each file of its zip: 12 model files of 128 MB took 1.6 GB from a zip of 1.5 MB, each far
    /// under the 2 GB it was allowed. The same at a smaller scale, with a limit of 512 KB instead
    /// of `MAX_UNPACKED`: four files of 64 KB are read, twelve are refused.
    #[test]
    fn audit_6_19_3mf_total() {
        let paths = |n: usize| (0..n).map(|k| format!("/3D/part{k}.model")).collect::<Vec<_>>();
        let limit = 512 << 10;
        assert_eq!(read(&parts(&paths(4), 64 << 10), &Unpacked::new(limit)).unwrap().tris.len(), 4);
        let r = read(&parts(&paths(12), 64 << 10), &Unpacked::new(limit));
        assert!(r.as_ref().is_err_and(|e| e.contains("unpacks")), "{:?}", r.map(|m| m.tris.len()));
    }

    /// One model file named in four spellings (`/3D/a.model`, `//3D/a.model`...) is one file of
    /// the zip: read and kept once, not once for each spelling (8 spellings of a file of 128 MB
    /// held 1.1 GB). With a limit of twice the file, its four placements are read.
    #[test]
    fn a_file_named_in_several_spellings_is_read_once() {
        let spellings = ["/3D/a.model", "//3D/a.model", "3D/a.model", "///3D/a.model"].map(String::from);
        let m = read(&parts(&spellings, 64 << 10), &Unpacked::new(128 << 10)).unwrap();
        assert_eq!(m.tris.len(), 4);
    }
}
