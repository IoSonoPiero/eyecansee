//! The scene graph of VRML 97 and X3D, in one form for `model_x3d`: VRML's text (also X3D's
//! classic encoding, `.x3dv`) and X3D's XML both become `SceneNode`s, with DEF and USE resolved
//! (shared, not copied).

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::model_3mf::Unpacked;

/// Nodes inside nodes; deeper is a damaged file.
const MAX_DEPTH: u32 = 256;

#[derive(Debug, Default)]
pub struct SceneNode {
    pub kind: String,
    /// Field name and value; in X3D's XML the child elements are under the name "".
    pub fields: Vec<(String, Value)>,
}

#[derive(Debug)]
pub enum Value {
    Nums(Vec<f32>),
    Strs(Vec<String>),
    Bool(bool),
    Nodes(Vec<Rc<SceneNode>>),
}

impl SceneNode {
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|f| f.0 == name).map(|f| &f.1)
    }

    pub fn nums(&self, name: &str) -> &[f32] {
        match self.field(name) {
            Some(Value::Nums(v)) => v,
            _ => &[],
        }
    }

    pub fn num(&self, name: &str, default: f32) -> f32 {
        self.nums(name).first().copied().unwrap_or(default)
    }

    pub fn bool(&self, name: &str, default: bool) -> bool {
        match self.field(name) {
            Some(Value::Bool(b)) => *b,
            _ => default,
        }
    }

    pub fn strs(&self, name: &str) -> &[String] {
        match self.field(name) {
            Some(Value::Strs(v)) => v,
            _ => &[],
        }
    }

    /// Every node held by the fields, in order.
    pub fn children(&self) -> impl Iterator<Item = &Rc<SceneNode>> {
        self.fields.iter().flat_map(|(_, v)| match v {
            Value::Nodes(n) => n.as_slice(),
            _ => &[],
        })
    }

    /// The nodes of one field (VRML) or, in X3D's XML, the child elements.
    pub fn field_nodes(&self, name: &str) -> &[Rc<SceneNode>] {
        match self.field(name).or_else(|| self.field("")) {
            Some(Value::Nodes(n)) => n,
            _ => &[],
        }
    }
}

/// The scene of a file: a root holding the top nodes. A gzipped file is unpacked within what
/// `unpacked` still allows.
pub fn parse(bytes: &[u8], unpacked: &Unpacked) -> Result<SceneNode, String> {
    // The text as it is, without a copy, when it is UTF-8 without its mark (nearly always).
    let text: Cow<str> = if bytes.starts_with(&[0x1f, 0x8b]) {
        let v = unpacked.read(flate2::read::GzDecoder::new(bytes))?;
        match v.as_slice() {
            [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..] | [0xEF, 0xBB, 0xBF, ..] => Cow::Owned(crate::model_3mf::text(&v)),
            _ => Cow::Owned(String::from_utf8(v).unwrap_or_else(|e| crate::model_3mf::text(e.as_bytes()))),
        }
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) if !s.starts_with('\u{FEFF}') => Cow::Borrowed(s),
            _ => Cow::Owned(crate::model_3mf::text(bytes)),
        }
    };
    let head = text.trim_start();
    if head.starts_with("#VRML V1.0") {
        return Err("VRML 1.0 is not supported (only VRML 97 and X3D)".into());
    }
    if head.starts_with('<') { xml(&text) } else { Classic { text: &text, at: 0, ahead: None, defs: HashMap::new() }.scene() }
}

/// The word of `text` from `i` on, after spaces, commas and comments, and where it ends. Read one
/// at a time: a list of all of them took 16 bytes for each, 8 times a text of one-digit numbers.
fn token(text: &str, mut i: usize) -> (Option<&str>, usize) {
    let b = text.as_bytes();
    while i < b.len() {
        match b[i] {
            b'#' => i += b[i..].iter().position(|&c| c == b'\n').unwrap_or(b.len() - i),
            c if c.is_ascii_whitespace() || c == b',' => i += 1,
            b'{' | b'}' | b'[' | b']' => return (Some(&text[i..i + 1]), i + 1),
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                return (Some(&text[i..j.min(b.len())]), j + 1);
            }
            _ => {
                let j = b[i..].iter().position(|&c| c.is_ascii_whitespace() || b",{}[]\"#".contains(&c)).map_or(b.len(), |p| i + p);
                return (Some(&text[i..j]), j);
            }
        }
    }
    (None, i)
}

fn number(t: &str) -> Option<f32> {
    if !t.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.')) {
        return None;
    }
    t.parse().ok().or_else(|| i64::from_str_radix(t.strip_prefix("0x").or_else(|| t.strip_prefix("0X"))?, 16).ok().map(|v| v as f32))
}

/// A token `"..."` without its quotes and escapes.
fn unquoted(t: &str) -> String {
    t.trim_start_matches('"').replace("\\\"", "\"").replace("\\\\", "\\")
}

struct Classic<'t> {
    text: &'t str,
    /// Where the next word starts to be looked for, and that word once `peek` has found it.
    at: usize,
    ahead: Option<(Option<&'t str>, usize)>,
    defs: HashMap<&'t str, Rc<SceneNode>>,
}

impl<'t> Classic<'t> {
    fn next(&mut self) -> Option<&'t str> {
        let (t, end) = self.ahead.take().unwrap_or_else(|| token(self.text, self.at));
        self.at = end;
        t
    }

    fn peek(&mut self) -> Option<&'t str> {
        self.ahead.get_or_insert_with(|| token(self.text, self.at)).0
    }

    fn skip(&mut self, n: usize) {
        for _ in 0..n {
            self.next();
        }
    }

    fn scene(mut self) -> Result<SceneNode, String> {
        let mut top = Vec::new();
        while self.peek().is_some() {
            if !self.statement()? {
                top.extend(self.node(0)?);
            }
        }
        Ok(SceneNode { kind: "Scene".into(), fields: vec![("children".into(), Value::Nodes(top))] })
    }

    /// Skips what is not a node or a field (prototypes, routes, the X3D header); false if none.
    fn statement(&mut self) -> Result<bool, String> {
        let skip = match self.peek() {
            Some("PROFILE" | "COMPONENT") => 2,
            Some("META") => 3,
            // ROUTE a.b TO c.d, IMPORT a.b AS c, EXPORT a AS b, UNIT kind name factor.
            Some("ROUTE" | "IMPORT" | "EXPORT" | "UNIT") => 4,
            Some("PROTO") => {
                self.skip(2);
                self.skip_block()?;
                self.skip_block()?;
                return Ok(true);
            }
            Some("EXTERNPROTO") => {
                self.skip(2);
                self.skip_block()?;
                if self.peek() == Some("[") { self.skip_block()? } else { self.skip(1) }
                return Ok(true);
            }
            _ => return Ok(false),
        };
        self.skip(skip);
        Ok(true)
    }

    /// A `[...]` or `{...}` with what is nested in it.
    fn skip_block(&mut self) -> Result<(), String> {
        let mut depth = 0i32;
        loop {
            match self.next().ok_or("unexpected end of file")? {
                "[" | "{" => depth += 1,
                "]" | "}" => depth -= 1,
                _ => {}
            }
            if depth <= 0 {
                return Ok(());
            }
        }
    }

    fn node(&mut self, depth: u32) -> Result<Option<Rc<SceneNode>>, String> {
        if depth > MAX_DEPTH {
            return Err("nodes nested too deep".into());
        }
        let t = self.next().ok_or("unexpected end of file")?;
        match t {
            "NULL" => Ok(None),
            "USE" => Ok(self.next().and_then(|n| self.defs.get(n)).cloned()),
            "DEF" => {
                let name = self.next().ok_or("unexpected end of file")?;
                let n = self.node(depth + 1)?;
                if let Some(n) = &n {
                    self.defs.insert(name, n.clone());
                }
                Ok(n)
            }
            kind => {
                if self.next() != Some("{") {
                    return Err(format!("'{{' expected after {kind}"));
                }
                let mut node = SceneNode { kind: kind.to_string(), fields: Vec::new() };
                while self.peek() != Some("}") {
                    if self.peek().is_none() {
                        return Err("unexpected end of file".into());
                    }
                    if self.statement()? {
                        continue;
                    }
                    let name = self.next().unwrap_or_default();
                    // Fields declared by a Script: kind, type and name, then a value for some.
                    match name {
                        "eventIn" | "eventOut" | "inputOnly" | "outputOnly" => {
                            self.skip(2);
                            continue;
                        }
                        "field" | "exposedField" | "initializeOnly" | "inputOutput" => {
                            self.skip(2);
                            self.value(depth)?;
                            continue;
                        }
                        _ => {}
                    }
                    let value = self.value(depth)?;
                    node.fields.push((name.to_string(), value));
                }
                self.next();
                Ok(Some(Rc::new(node)))
            }
        }
    }

    fn value(&mut self, depth: u32) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("nodes nested too deep".into());
        }
        let t = self.peek().ok_or("unexpected end of file")?;
        if t == "[" {
            self.next();
            let (mut nums, mut strs, mut nodes) = (Vec::new(), Vec::new(), Vec::new());
            while self.peek() != Some("]") {
                if self.peek().is_none() {
                    return Err("unexpected end of file".into());
                }
                match self.value(depth + 1)? {
                    // The numbers of a list come as one run: taken, not copied.
                    Value::Nums(n) if nums.is_empty() => nums = n,
                    Value::Nums(n) => nums.extend(n),
                    Value::Strs(s) => strs.extend(s),
                    Value::Nodes(n) => nodes.extend(n),
                    Value::Bool(_) => {}
                }
            }
            self.next();
            return Ok(join(nums, strs, nodes));
        }
        if number(t).is_some() {
            let mut v = Vec::new();
            while let Some(n) = self.peek().and_then(number) {
                v.push(n);
                self.next();
            }
            return Ok(Value::Nums(v));
        }
        Ok(match t {
            "TRUE" | "FALSE" => {
                self.next();
                Value::Bool(t == "TRUE")
            }
            "IS" => {
                self.skip(2);
                Value::Nodes(Vec::new())
            }
            _ if t.starts_with('"') => {
                self.next();
                Value::Strs(vec![unquoted(t)])
            }
            _ => Value::Nodes(self.node(depth + 1)?.into_iter().collect()),
        })
    }
}

/// The items of a `[...]`, all of one kind.
fn join(nums: Vec<f32>, strs: Vec<String>, nodes: Vec<Rc<SceneNode>>) -> Value {
    if !nodes.is_empty() {
        Value::Nodes(nodes)
    } else if !strs.is_empty() {
        Value::Strs(strs)
    } else {
        Value::Nums(nums)
    }
}

fn xml(text: &str) -> Result<SceneNode, String> {
    let doc = roxmltree::Document::parse_with_options(text, crate::model_3mf::xml_options()).map_err(|e| e.to_string())?;
    let mut defs = HashMap::new();
    let scene = doc.descendants().find(|n| n.has_tag_name("Scene")).ok_or("no Scene in the X3D file")?;
    Ok(SceneNode { kind: "Scene".into(), fields: vec![("".into(), Value::Nodes(xml_children(scene, &mut defs, 0)?))] })
}

fn xml_children(e: roxmltree::Node, defs: &mut HashMap<String, Rc<SceneNode>>, depth: u32) -> Result<Vec<Rc<SceneNode>>, String> {
    if depth > MAX_DEPTH {
        return Err("nodes nested too deep".into());
    }
    let mut out = Vec::new();
    for c in e.children().filter(|c| c.is_element()) {
        if let Some(used) = c.attribute("USE") {
            out.extend(defs.get(used).cloned());
            continue;
        }
        let mut fields: Vec<(String, Value)> = c.attributes().filter(|a| !matches!(a.name(), "DEF" | "USE" | "containerField")).map(|a| (a.name().to_string(), attribute(a.value()))).collect();
        fields.push((String::new(), Value::Nodes(xml_children(c, defs, depth + 1)?)));
        let node = Rc::new(SceneNode { kind: c.tag_name().name().to_string(), fields });
        if let Some(def) = c.attribute("DEF") {
            defs.insert(def.to_string(), node.clone());
        }
        out.push(node);
    }
    Ok(out)
}

/// An X3D attribute: numbers, a boolean or strings (`"a" "b"`, or the text as it is).
fn attribute(v: &str) -> Value {
    // The words one at a time, not in a list: 16 bytes each.
    let words = v.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty());
    if let Some(nums) = words.map(number).collect::<Option<Vec<f32>>>() {
        return Value::Nums(nums);
    }
    match v.trim() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        s if s.starts_with('"') => Value::Strs(s.split('"').skip(1).step_by(2).map(str::to_string).collect()),
        s => Value::Strs(vec![s.to_string()]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names given one after the other and lists inside lists without end are an error, not a
    /// stack that runs out (a file of a few KB closed the program).
    #[test]
    fn names_and_lists_nested_without_end_are_refused() {
        let names = format!("#VRML V2.0 utf8\n{}Group {{}}", "DEF a ".repeat(100_000));
        assert!(parse(names.as_bytes(), &Unpacked::default()).is_err());
        let lists = format!("#VRML V2.0 utf8\nGroup {{ children {}", "[ ".repeat(100_000));
        assert!(parse(lists.as_bytes(), &Unpacked::default()).is_err());
    }

    #[test]
    fn vrml_and_x3d_give_the_same_scene() {
        let wrl = "#VRML V2.0 utf8\nPROTO P [ field SFFloat a 1 ] { Group {} }\nDEF T Transform { translation 1 2 3 children [ Shape { geometry Box { size 1 1 1 } } ] }\nUSE T\nROUTE A.b TO C.d\n";
        let s = parse(wrl.as_bytes(), &Unpacked::default()).unwrap();
        let top: Vec<_> = s.children().collect();
        assert_eq!(top.len(), 2);
        assert!(Rc::ptr_eq(top[0], top[1]));
        assert_eq!(top[0].nums("translation"), &[1.0, 2.0, 3.0]);
        let x3d = "<X3D><Scene><Transform DEF='T' translation='1 2 3'><Shape><Box size='1 1 1'/></Shape></Transform><Transform USE='T'/></Scene></X3D>";
        let s = parse(x3d.as_bytes(), &Unpacked::default()).unwrap();
        let top: Vec<_> = s.children().collect();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].children().next().unwrap().children().next().unwrap().nums("size"), &[1.0, 1.0, 1.0]);
        assert!(parse(b"#VRML V1.0 ascii\nSeparator {}", &Unpacked::default()).is_err());
        let script = b"#X3D V3.0 utf8\nPROFILE Immersive\nDEF S Script { inputOnly SFTime touchTime initializeOnly SFFloat speed 2 url \"ecmascript:\" }\nShape { geometry Box {} }\n";
        assert_eq!(parse(script, &Unpacked::default()).unwrap().children().count(), 2);
        assert!(parse(b"#VRML V2.0 utf8\nShape {", &Unpacked::default()).is_err());
    }

    /// Audit 6.19 (b): a `.wrz` of one-digit numbers took 14 times its text to read (64 MB of text,
    /// 912 MB): every word went into a list, 16 bytes each, before the scene was made. The words
    /// are now read one at a time; what stays is the text and the numbers, 4 bytes for every 2 of
    /// text. Measured on this thread (`test_alloc`: the memory of the process is shared with the
    /// tests beside), with 1 MB of text. A `.wrz` that unpacks to more than the limit is refused.
    #[test]
    fn audit_6_19_wrz_tokens() {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(b"#VRML V2.0 utf8\nShape { geometry IndexedFaceSet { coord Coordinate { point [\n").unwrap();
        gz.write_all("0 ".repeat(1 << 19).as_bytes()).unwrap();
        gz.write_all(b"] } } }\n").unwrap();
        let wrz = gz.finish().unwrap();
        let text = 1 << 20;
        let (scene, scene_peak) = crate::test_alloc::peak(|| parse(&wrz, &Unpacked::default()).map(|_| ()));
        let (model, peak) = crate::test_alloc::peak(|| crate::model::parse(&wrz, "a.wrz").map(|m| m.tris.len()));
        println!("{scene_peak} bytes for the scene, {peak} in all, for {text} of text: {model:?}");
        assert!(scene.is_ok() && model.is_err_and(|e| e.contains("no triangles")));
        // The scene: the text and its numbers (3 times the text; it was 12). The model then copies
        // the points twice more, as for a .wrl (8 times; 9 leaves room).
        assert!(scene_peak <= 3 * text + (64 << 10) && peak <= 9 * text, "{scene_peak} and {peak} bytes for {text} of text");
        assert!(parse(&wrz, &Unpacked::new(text as u64 / 2)).is_err_and(|e| e.contains("unpacks")));
    }
}
