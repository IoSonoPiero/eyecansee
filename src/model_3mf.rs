//! 3MF (3D printing): a zip with the model in XML. Objects with a mesh or made of components
//! (also in other model files of the zip, Production extension), placed by the build items with
//! their transforms; colours from base materials and colour groups, pictures from texture groups
//! (Materials extension).

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::io::{Cursor, Read};

use roxmltree::{Document, Node};

use crate::model::{Builder, Mesh};

/// What one model file may unpack in all: the files of its zip, its gzip, the meshopt and Draco
/// data of a glTF and its numbers made floats. As much as one file of a zip could unpack to before
/// (~10 M triangles take ~1 GB of XML); a safety net, as files made on purpose stop at `MAX_RATIO`.
pub(crate) const MAX_UNPACKED: u64 = 2 << 30;
/// Beyond `RATIO_FREE`, a file may unpack to this many times its size (and the files next to it
/// that it reads), as zip bombs are told apart elsewhere (Apache POI: 0.01). XML and VRML text
/// pack 5 to 10 times, Draco and meshopt data some 10 times; the files made on purpose pack a
/// thousand times (a 3MF of 1.5 MB unpacked 1.6 GB, a GLB of 134 KB 2 GB).
const MAX_RATIO: u64 = 100;
/// What any file may unpack before `MAX_RATIO` is asked: a small file that packs well opens.
const RATIO_FREE: u64 = 16 << 20;

/// What a file holding `held` bytes may unpack to, by `MAX_RATIO` and `RATIO_FREE` (also the
/// zeros a glTF's accessors make without data behind them, `model_gltf::reading_fits`).
pub(crate) fn may_unpack(held: u64) -> u64 {
    RATIO_FREE.max(held.saturating_mul(MAX_RATIO))
}

/// What one file being opened has unpacked, against `MAX_UNPACKED` and `MAX_RATIO`, shared by the
/// formats it holds. Counted in bytes that come out, against bytes really read: the sizes a zip
/// declares are not trusted.
pub(crate) struct Unpacked {
    /// Bytes of the file and of the files next to it read for it.
    read: Cell<u64>,
    made: Cell<u64>,
    limit: u64,
    /// Up to here `MAX_RATIO` is not asked (`u64::MAX`: never).
    free: u64,
    over: Cell<bool>,
}

#[cfg(test)]
impl Default for Unpacked {
    fn default() -> Self {
        Unpacked::new(MAX_UNPACKED)
    }
}

impl Unpacked {
    /// For a file of `size` bytes.
    pub(crate) fn for_file(size: usize) -> Self {
        Unpacked::with(size, MAX_UNPACKED, RATIO_FREE)
    }

    pub(crate) fn with(size: usize, limit: u64, free: u64) -> Self {
        Unpacked { read: Cell::new(size as u64), made: Cell::new(0), limit, free, over: Cell::new(false) }
    }

    /// Only a limit in all, without `MAX_RATIO`.
    #[cfg(test)]
    pub(crate) fn new(limit: u64) -> Self {
        Unpacked::with(0, limit, u64::MAX)
    }

    /// A file next to this one, read for it (an inlined file, a glTF buffer).
    pub(crate) fn read_too(&self, n: usize) {
        self.read.set(self.read.get().saturating_add(n as u64));
    }

    /// How much more may be made: what is left of the limit and of `MAX_RATIO`.
    fn allowed(&self) -> u64 {
        let ratio = if self.free == u64::MAX { u64::MAX } else { self.free.max(self.read.get().saturating_mul(MAX_RATIO)) };
        self.limit.min(ratio).saturating_sub(self.made.get())
    }

    /// `n` more bytes made from the file; an error once they pass the limit.
    pub(crate) fn spend(&self, n: u64) -> Result<(), String> {
        if n <= self.allowed() && !self.over.get() {
            self.made.set(self.made.get() + n);
            return Ok(());
        }
        self.over.set(true);
        if self.made.get().saturating_add(n) > self.limit {
            Err(crate::model_limit::too_many(crate::model_limit::UNPACKED, self.limit >> 20))
        } else {
            Err(crate::model_limit::too_many(crate::model_limit::UNPACKED_TIMES, MAX_RATIO))
        }
    }

    /// All that `r` unpacks, counted as it comes out: never more than is allowed is read.
    pub(crate) fn read(&self, r: impl Read) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        r.take(self.allowed().saturating_add(1)).read_to_end(&mut out).map_err(|e| e.to_string())?;
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
    let main = parse_xml(&rels, crate::model_limit::THREE_MF)
        .ok()
        .and_then(|d| d.descendants().find(|n| n.attribute("Type").is_some_and(|t| t.ends_with("/3dmodel"))).and_then(|n| n.attribute("Target")).map(str::to_string))
        .unwrap_or_else(|| "/3D/3dmodel.model".into());
    let n = zip.len();
    let (texts, docs, models) = (slots(n), slots(n), slots(n));
    let loaded = Loaded { texts: &texts, docs: &docs, models: &models };
    let mut files = Files { zip, unpacked, pictures: HashMap::new(), written: 0, tree: Tree::of(crate::model_limit::THREE_MF) };
    let doc = loaded.model(&mut files, &main)?.doc;
    let mut b = Builder::up_to(crate::model_limit::THREE_MF);
    for item in doc.descendants().filter(|n| n.has_tag_name_local("item")) {
        let Some(id) = item.attribute("objectid") else { continue };
        let path = item.attribute_local("path").unwrap_or(&main).to_string();
        place(&mut b, &mut files, &loaded, &path, id, &transform(item), 0)?;
    }
    files.unpacked.check()?;
    let mut mesh = b.finish(true)?;
    mesh.unit = crate::model::named_unit(doc.root_element().attribute("unit"));
    Ok(mesh)
}

fn slots<T>(n: usize) -> Vec<OnceCell<T>> {
    std::iter::repeat_with(OnceCell::new).take(n).collect()
}

/// The model files of the zip, by their place in it: each read, parsed and made a `Model` once,
/// when it is first placed, however many components place it (audit 27/11, D.6 and 6.12: it was
/// parsed again at each of them).
struct Loaded<'m, 's, 'i> {
    texts: &'i [OnceCell<String>],
    docs: &'s [OnceCell<Box<Document<'i>>>],
    models: &'m [OnceCell<Box<Model<'s, 'i>>>],
}

impl<'m, 's, 'i> Loaded<'m, 's, 'i> {
    fn model(&self, files: &mut Files, path: &str) -> Result<&'m Model<'s, 'i>, String> {
        let i = files.index(path).ok_or_else(|| format!("{path} missing in the file"))?;
        if let Some(m) = self.models[i].get() {
            return Ok(m);
        }
        let text = files.text(i, path)?;
        let text = self.texts[i].get_or_init(|| text);
        let doc = files.parse(text)?;
        let doc = self.docs[i].get_or_init(|| Box::new(doc));
        Ok(self.models[i].get_or_init(|| Box::new(Model::new(doc))))
    }
}

/// The files of the zip, each read once however it is spelled (`/3D/a.model`, `//3D/a.model`).
struct Files<'b, 'u> {
    zip: zip::ZipArchive<Cursor<&'b [u8]>>,
    unpacked: &'u Unpacked,
    /// Pictures by their place in the zip.
    pictures: HashMap<usize, u16>,
    /// The triangles written in the texts read so far, counted before they are parsed (`model_limit`).
    written: usize,
    /// What the trees of its model files may still take, all of them together (audit 27/11, D.4).
    tree: Tree,
}

impl Files<'_, '_> {
    /// Where a path of the model is in the zip.
    fn index(&self, path: &str) -> Option<usize> {
        self.zip.index_for_name(path.trim_start_matches('/'))
    }

    /// The text of a model file, within the limit the model files share.
    fn parse<'t>(&mut self, text: &'t str) -> Result<Document<'t>, String> {
        #[cfg(test)]
        PARSES.with(|p| p.set(p.get() + 1));
        parse_within(text, crate::model_limit::THREE_MF, &mut self.tree)
    }

    /// The text of the file at `i` in the zip (named `path` in the model), its triangles counted.
    fn text(&mut self, i: usize, path: &str) -> Result<String, String> {
        let t = utf8(self.unpacked.read(self.zip.by_index(i).map_err(|e| e.to_string())?)?, path)?;
        self.written = self.written.saturating_add(crate::model_limit::xml_triangles(&t));
        crate::model_limit::THREE_MF.check(self.written)?;
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
    doc: &'a Document<'i>,
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
        Model { doc, objects, colors, textures }
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
fn place(b: &mut Builder, files: &mut Files, loaded: &Loaded, path: &str, id: &str, m: &Transform, depth: u32) -> Result<(), String> {
    if depth > 32 {
        return Err("components nested too deep".into());
    }
    b.visit()?;
    let model = loaded.model(files, path)?;
    let Some(object) = model.objects.get(id) else { return Ok(()) };
    if let Some(mesh) = object.children().find(|n| n.has_tag_name_local("mesh")) {
        mesh_part(b, files, model, *object, mesh, m)?;
    }
    for c in object.descendants().filter(|n| n.has_tag_name_local("component")) {
        let Some(child) = c.attribute("objectid") else { continue };
        let child_path = c.attribute_local("path").unwrap_or(path);
        place(b, files, loaded, child_path, child, &then(&transform(c), m), depth + 1)?;
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

/// XML of 3D formats: a DTD is allowed (X3D has one), its entities stay limited by roxmltree;
/// the nodes at most those of the format that allows the most (`tree_most`), each format's own
/// set by `parse_xml`.
pub(crate) fn xml_options<'a>() -> roxmltree::ParsingOptions<'a> {
    let nodes = [crate::model_limit::THREE_MF, crate::model_limit::AMF, crate::model_limit::COLLADA, crate::model_limit::X3D].map(|l| tree_most(l).0).into_iter().max().unwrap_or(0);
    roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::try_from(nodes).unwrap_or(u32::MAX), ..Default::default() }
}

/// The nodes and the attributes roxmltree may build for the XML of one model, by triangle of
/// the cap of its format (audit 27/11, D.4): its tree takes about 80 bytes a node and 72 an
/// attribute, 10 to 20 times the text, which is what `Unpacked` counts. Counted by triangle read
/// (`audit_d4_nodes_live`): the real 3MF files of the collection up to 4.1 nodes and 9.6
/// attributes (with textures), made ones 3.1 nodes and 4.6 attributes with shared points and one
/// element a line, 8 and 12 with three points of its own for each triangle; AMF real up to 11
/// nodes, made 20 (shared points), 26 (with normals), 57 and 93 (points of their own); Collada and
/// X3D up to 3.2 nodes and 1.2 attributes. So the 3MF and AMF files with shared points open up to
/// the cap of their format; those with points of their own for each triangle are refused before
/// (a 3MF with textures from 8.7 million triangles, an AMF from 2.6 million), as their tree at the
/// cap would take far more than the 8 GB the caps are made for.
fn tree_most(limit: crate::model_limit::Limit) -> (usize, usize) {
    let (nodes, attributes) = match limit.format {
        "three-mf" => (9, 14),
        "amf" => (30, 2),
        _ => (4, 2),
    };
    (nodes * limit.max, attributes * limit.max)
}

/// What the trees of the XML of one model may still take (`tree_most`): one file, or all the
/// model files of a 3MF together.
pub(crate) struct Tree {
    nodes: usize,
    attributes: usize,
}

impl Tree {
    pub(crate) fn of(limit: crate::model_limit::Limit) -> Self {
        let (nodes, attributes) = tree_most(limit);
        Tree { nodes, attributes }
    }
}

/// `text` as XML of the format of `limit`, refused with `MEMORY` before its tree takes more than
/// `tree_most`: its attributes counted first (one `=` each at least), its nodes by roxmltree as
/// it builds them. A DTD only outside 3MF, as before.
pub(crate) fn parse_xml(text: &str, limit: crate::model_limit::Limit) -> Result<Document<'_>, String> {
    parse_within(text, limit, &mut Tree::of(limit))
}

/// `parse_xml` within what is `left`, which the tree then takes.
fn parse_within<'t>(text: &'t str, limit: crate::model_limit::Limit, left: &mut Tree) -> Result<Document<'t>, String> {
    let (nodes, attributes) = tree_most(limit);
    let refused = |n: usize| crate::model_limit::too_many(crate::model_limit::MEMORY, n);
    let equals = text.bytes().filter(|&b| b == b'=').count();
    if equals > left.attributes {
        return Err(refused(attributes));
    }
    let options = roxmltree::ParsingOptions { allow_dtd: limit != crate::model_limit::THREE_MF, nodes_limit: u32::try_from(left.nodes).unwrap_or(u32::MAX), ..xml_options() };
    let doc = Document::parse_with_options(text, options).map_err(|e| match e {
        roxmltree::Error::NodesLimitReached => refused(nodes),
        e => e.to_string(),
    })?;
    left.nodes = left.nodes.saturating_sub(doc.descendants().count());
    left.attributes -= equals;
    Ok(doc)
}

#[cfg(test)]
thread_local! {
    /// The model files parsed by `Files::parse` on this thread, for the tests that count them.
    static PARSES: Cell<usize> = const { Cell::new(0) };
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

    /// D1: the triangles counted before the XML is parsed (`model_limit::xml_triangles`) are the
    /// ones read, with and without the prefix of the namespace; a triangle outside the points is
    /// counted, as written in the file, and dropped by the reading.
    #[test]
    fn counted_before_parsing_d1() {
        let model = r#"<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:m="http://schemas.microsoft.com/3dmanufacturing/core/2015/02"><resources>
            <object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices>
            <triangles><triangle v1="0" v2="1" v3="2"/><m:triangle v1="0" v2="2" v3="1"/></triangles></mesh></object>
            </resources><build><item objectid="1"/></build></model>"#;
        assert_eq!(crate::model_limit::xml_triangles(model), 2);
        assert_eq!(read(&zip(model), &Unpacked::default()).unwrap().tris.len(), 2);
        let outside = model.replace(r#"v3="1""#, r#"v3="9""#);
        assert_eq!(crate::model_limit::xml_triangles(&outside), 2);
        assert_eq!(read(&zip(&outside), &Unpacked::default()).unwrap().tris.len(), 1);
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
    /// file a triangle and the comment `filler`, written once for each name in the zip.
    fn parts(paths: &[String], filler: &str) -> Vec<u8> {
        let items: String = paths.iter().map(|p| format!(r#"<item objectid="1" p:path="{p}"/>"#)).collect();
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file("3D/3dmodel.model", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(format!(r#"<model xmlns:p="p"><resources/><build>{items}</build></model>"#).as_bytes()).unwrap();
        let triangle = r#"<vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles>"#;
        let part = format!(r#"<model><resources><object id="1"><mesh>{triangle}</mesh></object></resources><!--{filler}--></model>"#);
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
        assert_eq!(read(&parts(&paths(4), &"A".repeat(64 << 10)), &Unpacked::new(limit)).unwrap().tris.len(), 4);
        let r = read(&parts(&paths(12), &"A".repeat(64 << 10)), &Unpacked::new(limit));
        assert!(r.as_ref().is_err_and(|e| e.starts_with(crate::model_limit::UNPACKED)), "{:?}", r.map(|m| m.tris.len()));
    }

    /// One model file named in four spellings (`/3D/a.model`, `//3D/a.model`...) is one file of
    /// the zip: read and kept once, not once for each spelling (8 spellings of a file of 128 MB
    /// held 1.1 GB). With a limit of twice the file, its four placements are read.
    #[test]
    fn a_file_named_in_several_spellings_is_read_once() {
        let spellings = ["/3D/a.model", "//3D/a.model", "3D/a.model", "///3D/a.model"].map(String::from);
        let m = read(&parts(&spellings, &"A".repeat(64 << 10)), &Unpacked::new(128 << 10)).unwrap();
        assert_eq!(m.tris.len(), 4);
    }

    /// Audit 6.19 (a), the ratio: past what any file may unpack (64 KB here instead of
    /// `RATIO_FREE`), a 3MF that unpacks to more than `MAX_RATIO` times its size is refused, as the
    /// files made on purpose pack a thousand times; one whose XML holds numbers, which pack a few
    /// times as in a real model, opens.
    #[test]
    fn a_3mf_that_unpacks_to_100_times_its_size_is_refused() {
        let paths: Vec<String> = (0..4).map(|k| format!("/3D/part{k}.model")).collect();
        let ratio = |bytes: &[u8]| Unpacked::with(bytes.len(), MAX_UNPACKED, 64 << 10);
        let packed = parts(&paths, &"A".repeat(64 << 10));
        assert!(read(&packed, &ratio(&packed)).is_err_and(|e| e.starts_with(crate::model_limit::UNPACKED_TIMES)));
        let mut seed = 1u32;
        let numbers: String = (0..8000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                format!("{:.4} ", f64::from(seed >> 8) / 1e4)
            })
            .collect();
        let real = parts(&paths, &numbers);
        assert_eq!(read(&real, &ratio(&real)).unwrap().tris.len(), 4);
    }

    /// Audit 27/11, D.4 (#181): the cap of what a file unpacks counts the bytes of its text, but
    /// roxmltree builds on them a tree 10 to 20 times larger (about 80 bytes a node, 72 an
    /// attribute): the XML of the 3D formats was parsed without a limit on its nodes, so that a
    /// 3MF of a few MB unpacked to 2 GB of `<vertex/>` asked for tens of GB.
    #[test]
    fn audit_d4_xml_tree_limited() {
        let most = 8 * crate::model_limit::COLLADA.max;
        assert!(xml_options().nodes_limit as usize <= most, "nodes_limit {}: the tree of the XML without a limit", xml_options().nodes_limit);
        // Scaled: a cap of 1000 triangles, so 9000 nodes and 14 000 attributes for a 3MF.
        let limit = crate::model_limit::Limit { max: 1000, format: "three-mf" };
        let model = |n: usize, vertex: &str| format!("<model><resources><object id=\"1\"><mesh><vertices>{}</vertices></mesh></object></resources></model>", vertex.repeat(n));
        for (what, text) in [("nodes", model(10_000, "<vertex/>")), ("attributes", model(5_000, r#"<vertex x="0" y="0" z="0"/>"#))] {
            let (r, peak) = crate::test_alloc::peak(|| parse_xml(&text, limit).map(|d| d.descendants().count()));
            assert!(r.as_ref().is_err_and(|e| e.starts_with(crate::model_limit::MEMORY)) && peak < 1 << 20, "{what}: {r:?}, {peak} bytes");
        }
        assert_eq!(parse_xml(&model(100, r#"<vertex x="0" y="0" z="0"/>"#), limit).map(|d| d.descendants().count()), Ok(106));
    }

    /// Audit 27/11, D.4: in a 3MF of several model files each file has its tree within the cap,
    /// but every step into another file kept its tree while the components went on: two part
    /// files whose components point at each other held a tree at each of the 32 steps allowed.
    /// Measured against the same 3MF whose part does not point back.
    #[test]
    fn audit_d4_trees_of_files_up_the_stack() {
        let filler = "<a/>".repeat(50_000);
        let triangle = r#"<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>"#;
        let part = |next: Option<&str>| {
            let component = next.map_or(String::new(), |n| format!(r#"<components><component objectid="1" p:path="{n}"/></components>"#));
            format!(r#"<model xmlns:p="p"><resources><object id="1">{triangle}{component}</object></resources><build/>{filler}</model>"#)
        };
        let zip3mf = |a: &str, b: &str| {
            let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
            for (name, text) in [("3D/3dmodel.model", r#"<model xmlns:p="p"><resources/><build><item objectid="1" p:path="/3D/a.model"/></build></model>"#), ("3D/a.model", a), ("3D/b.model", b)] {
                w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                w.write_all(text.as_bytes()).unwrap();
            }
            w.finish().unwrap().into_inner()
        };
        let alone = zip3mf(&part(None), &part(None));
        let looped = zip3mf(&part(Some("/3D/b.model")), &part(Some("/3D/a.model")));
        let (r, one) = crate::test_alloc::peak(|| read(&alone, &Unpacked::default()).map(|m| m.tris.len()));
        assert_eq!(r, Ok(1));
        let (r, both) = crate::test_alloc::peak(|| read(&looped, &Unpacked::default()).map(|m| m.tris.len()));
        assert!(both < 4 * one, "{both} bytes held against {one} for one part file ({r:?})");
    }

    /// Audit 27/11, D.4: the limit the model files of a 3MF share is taken once by each file, not
    /// at every parse: a part file placed by many components (one for each of its objects, as some
    /// programs write them) was charged again at each one, and a file that opened was refused.
    /// Here 100 components into one part file whose text has 1.5 million `=` (counted as
    /// attributes before it is parsed): 150 million charged against 140.
    #[test]
    fn audit_d4_each_file_charged_once() {
        let triangle = r#"<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>"#;
        let part = format!(r#"<model><resources><object id="1">{triangle}</object></resources><build/><!--{}--></model>"#, "=".repeat(1_500_000));
        let components: String = (0..100).map(|_| r#"<component objectid="1" p:path="/3D/part.model"/>"#).collect();
        let main = format!(r#"<model xmlns:p="p"><resources><object id="9"><components>{components}</components></object></resources><build><item objectid="9"/></build></model>"#);
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in [("3D/3dmodel.model", main.as_str()), ("3D/part.model", part.as_str())] {
            w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(read(&bytes, &Unpacked::default()).map(|m| m.tris.len()), Ok(100));
    }

    /// Audit 27/11, D.6 and 6.12: a part file placed by many components was parsed again at
    /// each of them, whatever it held: 100 components into one part file, and the part with
    /// an object of no mesh placed 100 more times through it. Each model file is parsed once.
    #[test]
    fn audit_d6_each_model_file_parsed_once() {
        let triangle = r#"<mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh>"#;
        let part = format!(r#"<model><resources><object id="1">{triangle}</object><object id="2"/></resources><build/></model>"#);
        let components: String = (0..100).map(|k| format!(r#"<component objectid="{}" p:path="/3D/part.model"/>"#, 1 + k % 2)).collect();
        let main = format!(r#"<model xmlns:p="p"><resources><object id="9"><components>{components}</components></object></resources><build><item objectid="9"/></build></model>"#);
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in [("3D/3dmodel.model", main.as_str()), ("3D/part.model", part.as_str()), ("3D/unused.model", "not XML")] {
            w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        let bytes = w.finish().unwrap().into_inner();
        PARSES.with(|p| p.set(0));
        assert_eq!(read(&bytes, &Unpacked::default()).map(|m| m.tris.len()), Ok(50));
        let parses = PARSES.with(Cell::get);
        assert_eq!(parses, 2, "{parses} parses of 2 model files");
    }

    /// Audit 27/11, D.4: the nodes and attributes roxmltree builds for the XML of every 3MF, AMF,
    /// Collada and X3D under `EYECANSEE_MODELS` (and its folders), against the triangles read.
    #[test]
    #[ignore]
    fn audit_d4_nodes_live() {
        let Ok(dir) = std::env::var("EYECANSEE_MODELS") else { return };
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for f in std::fs::read_dir(&d).unwrap().flatten() {
                let path = f.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
                let ext = name.rsplit('.').next().unwrap_or("").to_string();
                if !["3mf", "amf", "dae", "zae", "x3d", "x3dz"].contains(&ext.as_str()) {
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap();
                let texts: Vec<String> = if bytes.starts_with(b"PK") {
                    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes[..])).unwrap();
                    let names: Vec<String> = zip.file_names().filter(|n| [".model", ".amf", ".dae", ".rels"].iter().any(|e| n.to_lowercase().ends_with(e))).map(str::to_string).collect();
                    names.iter().map(|n| entry(&mut zip, n, &Unpacked::default()).unwrap()).collect()
                } else if bytes.starts_with(&[0x1f, 0x8b]) {
                    let mut out = Vec::new();
                    flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out).unwrap();
                    vec![text(&out)]
                } else {
                    vec![text(&bytes)]
                };
                let (mut nodes, mut attributes, mut most) = (0, 0, 0);
                for t in &texts {
                    if let Ok(doc) = Document::parse_with_options(t, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }) {
                        let n = doc.descendants().count();
                        (nodes, most) = (nodes + n, most.max(n));
                        attributes += doc.descendants().map(|n| n.attributes().len()).sum::<usize>();
                    }
                }
                let tris = crate::model::parse(&bytes, &name).map_or(0, |m| m.tris.len());
                let per = |n: usize| n as f64 / tris.max(1) as f64;
                println!("NODES {name}: {nodes} nodes (most in one file {most}), {attributes} attributes, {tris} triangles: {:.2} nodes and {:.2} attributes a triangle", per(nodes), per(attributes));
                // None refused by the limits of its format.
                use crate::model_limit::{AMF, COLLADA, THREE_MF, X3D};
                let limit = match ext.as_str() {
                    "3mf" => THREE_MF,
                    "amf" => AMF,
                    "dae" | "zae" => COLLADA,
                    _ => X3D,
                };
                let (most_nodes, most_attributes) = tree_most(limit);
                assert!(most <= most_nodes && texts.iter().all(|t| t.bytes().filter(|&b| b == b'=').count() <= most_attributes), "{name}");
            }
        }
    }
}
