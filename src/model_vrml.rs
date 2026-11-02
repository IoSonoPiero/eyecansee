//! The scene graph of VRML 97 and X3D, in one form for `model_x3d`: VRML's text (also X3D's
//! classic encoding, `.x3dv`) and X3D's XML both become `SceneNode`s, with DEF and USE resolved
//! (shared, not copied).

use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;

/// Nodes inside nodes; deeper is a damaged file.
const MAX_DEPTH: u32 = 256;
/// Unzipped size of a `.wrz` or `.x3dz`.
const MAX_TEXT: u64 = 2 << 30;

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

/// The scene of a file: a root holding the top nodes.
pub fn parse(bytes: &[u8]) -> Result<SceneNode, String> {
    let unzipped;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut v = Vec::new();
        flate2::read::GzDecoder::new(bytes).take(MAX_TEXT).read_to_end(&mut v).map_err(|e| e.to_string())?;
        unzipped = v;
        &unzipped[..]
    } else {
        bytes
    };
    let text = crate::model_3mf::text(bytes);
    let head = text.trim_start();
    if head.starts_with("#VRML V1.0") {
        return Err("VRML 1.0 is not supported (only VRML 97 and X3D)".into());
    }
    if head.starts_with('<') { xml(&text) } else { Classic { tokens: tokens(&text), at: 0, defs: HashMap::new() }.scene() }
}

fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'#' => i += b[i..].iter().position(|&c| c == b'\n').unwrap_or(b.len() - i),
            c if c.is_ascii_whitespace() || c == b',' => i += 1,
            b'{' | b'}' | b'[' | b']' => {
                out.push(&text[i..i + 1]);
                i += 1;
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                out.push(&text[i..j.min(b.len())]);
                i = j + 1;
            }
            _ => {
                let j = b[i..].iter().position(|&c| c.is_ascii_whitespace() || b",{}[]\"#".contains(&c)).map_or(b.len(), |p| i + p);
                out.push(&text[i..j]);
                i = j;
            }
        }
    }
    out
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
    tokens: Vec<&'t str>,
    at: usize,
    defs: HashMap<&'t str, Rc<SceneNode>>,
}

impl<'t> Classic<'t> {
    fn next(&mut self) -> Option<&'t str> {
        let t = self.tokens.get(self.at).copied();
        self.at += 1;
        t
    }

    fn peek(&self) -> Option<&'t str> {
        self.tokens.get(self.at).copied()
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
                self.at += 2;
                self.skip_block()?;
                self.skip_block()?;
                return Ok(true);
            }
            Some("EXTERNPROTO") => {
                self.at += 2;
                self.skip_block()?;
                if self.peek() == Some("[") { self.skip_block()? } else { self.at += 1 }
                return Ok(true);
            }
            _ => return Ok(false),
        };
        self.at += skip;
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
                let n = self.node(depth)?;
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
                            self.at += 2;
                            continue;
                        }
                        "field" | "exposedField" | "initializeOnly" | "inputOutput" => {
                            self.at += 2;
                            self.value(depth)?;
                            continue;
                        }
                        _ => {}
                    }
                    let value = self.value(depth)?;
                    node.fields.push((name.to_string(), value));
                }
                self.at += 1;
                Ok(Some(Rc::new(node)))
            }
        }
    }

    fn value(&mut self, depth: u32) -> Result<Value, String> {
        let t = self.peek().ok_or("unexpected end of file")?;
        if t == "[" {
            self.at += 1;
            let mut items = Vec::new();
            while self.peek() != Some("]") {
                if self.peek().is_none() {
                    return Err("unexpected end of file".into());
                }
                items.push(self.value(depth)?);
            }
            self.at += 1;
            return Ok(join(items));
        }
        if number(t).is_some() {
            let mut v = Vec::new();
            while let Some(n) = self.peek().and_then(number) {
                v.push(n);
                self.at += 1;
            }
            return Ok(Value::Nums(v));
        }
        self.at += 1;
        Ok(match t {
            "TRUE" => Value::Bool(true),
            "FALSE" => Value::Bool(false),
            "IS" => {
                self.at += 1;
                Value::Nodes(Vec::new())
            }
            _ if t.starts_with('"') => Value::Strs(vec![unquoted(t)]),
            _ => {
                self.at -= 1;
                Value::Nodes(self.node(depth + 1)?.into_iter().collect())
            }
        })
    }
}

/// The items of a `[...]`, all of one kind.
fn join(items: Vec<Value>) -> Value {
    let mut nums = Vec::new();
    let mut strs = Vec::new();
    let mut nodes = Vec::new();
    for v in items {
        match v {
            Value::Nums(n) => nums.extend(n),
            Value::Strs(s) => strs.extend(s),
            Value::Nodes(n) => nodes.extend(n),
            Value::Bool(_) => {}
        }
    }
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
    let words: Vec<&str> = v.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect();
    if let Some(nums) = words.iter().map(|w| number(w)).collect::<Option<Vec<f32>>>() {
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

    #[test]
    fn vrml_and_x3d_give_the_same_scene() {
        let wrl = "#VRML V2.0 utf8\nPROTO P [ field SFFloat a 1 ] { Group {} }\nDEF T Transform { translation 1 2 3 children [ Shape { geometry Box { size 1 1 1 } } ] }\nUSE T\nROUTE A.b TO C.d\n";
        let s = parse(wrl.as_bytes()).unwrap();
        let top: Vec<_> = s.children().collect();
        assert_eq!(top.len(), 2);
        assert!(Rc::ptr_eq(top[0], top[1]));
        assert_eq!(top[0].nums("translation"), &[1.0, 2.0, 3.0]);
        let x3d = "<X3D><Scene><Transform DEF='T' translation='1 2 3'><Shape><Box size='1 1 1'/></Shape></Transform><Transform USE='T'/></Scene></X3D>";
        let s = parse(x3d.as_bytes()).unwrap();
        let top: Vec<_> = s.children().collect();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].children().next().unwrap().children().next().unwrap().nums("size"), &[1.0, 1.0, 1.0]);
        assert!(parse(b"#VRML V1.0 ascii\nSeparator {}").is_err());
        let script = b"#X3D V3.0 utf8\nPROFILE Immersive\nDEF S Script { inputOnly SFTime touchTime initializeOnly SFFloat speed 2 url \"ecmascript:\" }\nShape { geometry Box {} }\n";
        assert_eq!(parse(script).unwrap().children().count(), 2);
        assert!(parse(b"#VRML V2.0 utf8\nShape {").is_err());
    }
}
