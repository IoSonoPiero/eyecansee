//! AMF (Additive Manufacturing File, 3D printing), plain or zipped XML: objects with their
//! vertices and volumes of triangles, placed by the constellation when there is one, with the
//! colours of triangles, vertices, volumes, objects and materials.

use std::collections::HashMap;
use roxmltree::{Document, Node};

use crate::model::{Builder, Mesh};

/// Constellations inside constellations; deeper is a loop.
const MAX_DEPTH: u32 = 32;

pub fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let text = if bytes.starts_with(b"PK") { crate::model_3mf::unzipped(bytes, ".amf")? } else { crate::model_3mf::text(bytes) };
    let doc = Document::parse_with_options(&text, crate::model_3mf::xml_options()).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let ids = |tag: &str| -> HashMap<&str, Node> { root.children().filter(|n| n.has_tag_name(tag)).filter_map(|n| Some((n.attribute("id")?, n))).collect() };
    let (objects, materials, constellations) = (ids("object"), ids("material"), ids("constellation"));
    let mut b = Builder::default();
    let mut amf = Amf { b: &mut b, objects: &objects, materials: &materials, constellations: &constellations };
    match root.children().find(|n| n.has_tag_name("constellation")) {
        Some(c) => amf.constellation(c, &IDENTITY, 0)?,
        None => {
            for o in root.children().filter(|n| n.has_tag_name("object")) {
                amf.object(o, &IDENTITY)?;
            }
        }
    }
    b.finish(true)
}

/// Row-major 3×4: rotation and translation.
type M = [[f32; 4]; 3];
const IDENTITY: M = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

fn then(a: &M, b: &M) -> M {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum::<f32>() + if c == 3 { a[r][3] } else { 0.0 }))
}

fn apply(m: &M, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3])
}

fn child<'a>(n: Node<'a, 'a>, tag: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn number(n: Node, tag: &str) -> f32 {
    child(n, tag).and_then(|c| c.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(0.0)
}

fn color(n: Node) -> Option<[u8; 3]> {
    let c = child(n, "color")?;
    Some(["r", "g", "b"].map(|k| (number(c, k).clamp(0.0, 1.0) * 255.0) as u8))
}

/// An instance's place: rotations in degrees about X, then Y, then Z, then the shift.
fn placement(i: Node) -> M {
    let [rx, ry, rz] = ["rx", "ry", "rz"].map(|k| number(i, k).to_radians());
    let (sx, cx, sy, cy, sz, cz) = (rx.sin(), rx.cos(), ry.sin(), ry.cos(), rz.sin(), rz.cos());
    let x = [[1.0, 0.0, 0.0, 0.0], [0.0, cx, -sx, 0.0], [0.0, sx, cx, 0.0]];
    let y = [[cy, 0.0, sy, 0.0], [0.0, 1.0, 0.0, 0.0], [-sy, 0.0, cy, 0.0]];
    let z = [[cz, -sz, 0.0, 0.0], [sz, cz, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
    let mut m = then(&z, &then(&y, &x));
    for (r, k) in ["deltax", "deltay", "deltaz"].into_iter().enumerate() {
        m[r][3] = number(i, k);
    }
    m
}

struct Amf<'a, 'd> {
    b: &'a mut Builder,
    objects: &'a HashMap<&'d str, Node<'d, 'd>>,
    materials: &'a HashMap<&'d str, Node<'d, 'd>>,
    constellations: &'a HashMap<&'d str, Node<'d, 'd>>,
}

impl Amf<'_, '_> {
    fn constellation(&mut self, c: Node, m: &M, depth: u32) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("constellations nested too deep".into());
        }
        for i in c.children().filter(|n| n.has_tag_name("instance")) {
            let id = i.attribute("objectid").unwrap_or("");
            let m = then(m, &placement(i));
            if let Some(&o) = self.objects.get(id) {
                self.object(o, &m)?;
            } else if let Some(&inner) = self.constellations.get(id) {
                self.constellation(inner, &m, depth + 1)?;
            }
        }
        Ok(())
    }

    fn object(&mut self, o: Node, m: &M) -> Result<(), String> {
        let Some(mesh) = child(o, "mesh") else { return Ok(()) };
        let vertices: Vec<Node> = child(mesh, "vertices").map(|v| v.children().filter(|n| n.has_tag_name("vertex")).collect()).unwrap_or_default();
        let pos: Vec<[f32; 3]> = vertices.iter().map(|v| child(*v, "coordinates").map(|c| apply(m, ["x", "y", "z"].map(|k| number(c, k)))).unwrap_or([f32::NAN; 3])).collect();
        let vcolor: Vec<Option<[u8; 3]>> = vertices.iter().map(|v| color(*v)).collect();
        let (mut tris, mut colors) = (Vec::new(), Vec::new());
        for vol in mesh.children().filter(|n| n.has_tag_name("volume")) {
            let fallback = color(vol).or_else(|| color(o)).or_else(|| vol.attribute("materialid").and_then(|id| self.materials.get(id)).and_then(|&mat| color(mat)));
            for t in vol.children().filter(|n| n.has_tag_name("triangle")) {
                let v = ["v1", "v2", "v3"].map(|k| number(t, k) as u32);
                tris.push(v);
                colors.push(color(t).or_else(|| vcolor.get(v[0] as usize).copied().flatten()).or(fallback));
            }
        }
        self.b.add(pos, tris, |i| colors[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_triangle_placed_twice() {
        let amf = r#"<amf><object id="1"><mesh><vertices>
            <vertex><coordinates><x>0</x><y>0</y><z>0</z></coordinates></vertex>
            <vertex><coordinates><x>1</x><y>0</y><z>0</z></coordinates></vertex>
            <vertex><coordinates><x>0</x><y>1</y><z>0</z></coordinates></vertex></vertices>
            <volume materialid="2"><triangle><v1>0</v1><v2>1</v2><v3>2</v3></triangle></volume></mesh></object>
            <material id="2"><color><r>1</r><g>0</g><b>0</b></color></material>
            <constellation id="3"><instance objectid="1"/><instance objectid="1"><deltax>10</deltax><rz>90</rz></instance></constellation></amf>"#;
        let m = read(amf.as_bytes()).unwrap();
        assert_eq!(m.tris.len(), 2);
        assert_eq!(m.colors[1], [255, 0, 0]);
        // Turned by 90° about Z and moved: (1, 0, 0) lands on (10, 1, 0).
        let p = m.pos[4];
        assert!((p[0] - 10.0).abs() < 1e-5 && (p[1] - 1.0).abs() < 1e-5);
    }
}
