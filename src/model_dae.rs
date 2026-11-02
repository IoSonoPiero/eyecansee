//! Collada (DAE, and ZAE: zipped): the visual scene's nodes with their transforms, the meshes
//! they place (triangles, polylists, polygons, strips, fans; also skinned ones in their rest
//! pose) and the diffuse colours of the bound materials or of the vertices.
// ponytail: polygons become fans, right for convex ones; concave ones may cover their notches

use std::collections::HashMap;

use roxmltree::{Document, Node};

use crate::model::{Builder, IDENTITY, Mat4, Mesh, apply, mul, rotation};
use crate::model_3mf::Local;

/// Nodes inside nodes (and instanced nodes); deeper is a loop.
const MAX_DEPTH: u32 = 64;

pub fn read(bytes: &[u8], zipped: bool) -> Result<Mesh, String> {
    let text = if zipped { crate::model_3mf::unzipped(bytes, ".dae")? } else { crate::model_3mf::text(bytes) };
    let doc = Document::parse_with_options(&text, crate::model_3mf::xml_options()).map_err(|e| e.to_string())?;
    let ids: HashMap<&str, Node> = doc.descendants().filter_map(|n| Some((n.attribute("id")?, n))).collect();
    let z_up = doc.descendants().find(|n| n.has_tag_name_local("up_axis")).and_then(|n| n.text()).is_some_and(|t| t.trim() == "Z_UP");
    let mut b = Builder::default();
    let mut dae = Dae { b: &mut b, ids: &ids };
    let scene = doc.descendants().find(|n| n.has_tag_name_local("instance_visual_scene")).and_then(|i| dae.target(i, "url"));
    let scene = scene.or_else(|| doc.descendants().find(|n| n.has_tag_name_local("visual_scene")));
    match scene {
        Some(s) => {
            for n in s.children().filter(|n| n.has_tag_name_local("node")) {
                dae.node(n, &IDENTITY, 0)?;
            }
        }
        None => {
            for g in doc.descendants().filter(|n| n.has_tag_name_local("geometry")) {
                dae.geometry(g, &IDENTITY, &HashMap::new())?;
            }
        }
    }
    b.finish(z_up)
}

fn tagged<'a>(n: Node<'a, 'a>, tag: &'a str) -> impl Iterator<Item = Node<'a, 'a>> {
    n.children().filter(move |c| c.has_tag_name_local(tag))
}

/// Also with decimal commas, written by some exporters in Italian or German.
fn floats(n: Node) -> Vec<f32> {
    n.text().unwrap_or("").split_whitespace().map(|s| s.parse().or_else(|_| s.replace(',', ".").parse()).unwrap_or(f32::NAN)).collect()
}

fn ints(n: Node) -> Vec<u32> {
    n.text().unwrap_or("").split_whitespace().map(|s| s.parse().unwrap_or(u32::MAX)).collect()
}

/// A transform element as a column-major matrix.
fn transform(n: Node) -> Option<Mat4> {
    let v = floats(n);
    let mut m = IDENTITY;
    match n.tag_name().name() {
        // Row-major in the file.
        "matrix" if v.len() == 16 => m = std::array::from_fn(|c| std::array::from_fn(|r| v[r * 4 + c])),
        "translate" if v.len() == 3 => m[3] = [v[0], v[1], v[2], 1.0],
        "scale" if v.len() == 3 => (0..3).for_each(|k| m[k][k] = v[k]),
        "rotate" if v.len() == 4 => m = rotation([v[0], v[1], v[2]], v[3].to_radians()),
        _ => return None,
    }
    Some(m)
}

struct Dae<'a, 'd> {
    b: &'a mut Builder,
    ids: &'a HashMap<&'d str, Node<'d, 'd>>,
}

impl<'d> Dae<'_, 'd> {
    /// The element an attribute points to (`#id`).
    fn target(&self, n: Node, attr: &str) -> Option<Node<'d, 'd>> {
        self.ids.get(n.attribute(attr)?.trim_start_matches('#')).copied()
    }

    fn node(&mut self, n: Node<'d, 'd>, parent: &Mat4, depth: u32) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("nodes nested too deep".into());
        }
        let m = n.children().filter_map(transform).fold(*parent, |acc, t| mul(&acc, &t));
        for c in n.children() {
            match c.tag_name().name() {
                "node" => self.node(c, &m, depth + 1)?,
                "instance_node" => {
                    if let Some(t) = self.target(c, "url") {
                        self.node(t, &m, depth + 1)?;
                    }
                }
                "instance_geometry" | "instance_controller" => {
                    let mut g = self.target(c, "url");
                    // A skin names the mesh it deforms.
                    if let Some(skin) = g.and_then(|g| g.descendants().find(|s| s.has_tag_name_local("skin"))) {
                        g = self.target(skin, "source");
                    }
                    if let Some(g) = g {
                        let bound = self.bound_colors(c);
                        self.geometry(g, &m, &bound)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The colour of each material symbol of an instance.
    fn bound_colors(&self, inst: Node) -> HashMap<String, [u8; 3]> {
        inst.descendants()
            .filter(|n| n.has_tag_name_local("instance_material"))
            .filter_map(|i| Some((i.attribute("symbol")?.to_string(), self.material_color(self.target(i, "target")?)?)))
            .collect()
    }

    fn material_color(&self, material: Node) -> Option<[u8; 3]> {
        let effect = self.target(material.descendants().find(|n| n.has_tag_name_local("instance_effect"))?, "url")?;
        let diffuse = effect.descendants().find(|n| n.has_tag_name_local("diffuse"))?;
        let c = floats(tagged(diffuse, "color").next()?);
        (c.len() >= 3).then(|| [c[0], c[1], c[2]].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8))
    }

    /// The numbers of a `<source>`, by groups of its accessor's stride.
    fn source(&self, input: Node) -> Option<(Vec<f32>, usize)> {
        let s = self.target(input, "source")?;
        let data = floats(s.descendants().find(|n| n.has_tag_name_local("float_array"))?);
        let stride = s.descendants().find(|n| n.has_tag_name_local("accessor")).and_then(|a| a.attribute("stride")?.parse().ok()).unwrap_or(3usize).max(1);
        Some((data, stride))
    }

    fn geometry(&mut self, g: Node<'d, 'd>, m: &Mat4, bound: &HashMap<String, [u8; 3]>) -> Result<(), String> {
        let Some(mesh) = tagged(g, "mesh").next() else { return Ok(()) };
        let Some(vertices) = tagged(mesh, "vertices").next() else { return Ok(()) };
        let semantic = |n: Node<'d, 'd>, s: &str| tagged(n, "input").find(|i| i.attribute("semantic") == Some(s));
        let Some((p, stride)) = semantic(vertices, "POSITION").and_then(|i| self.source(i)) else { return Ok(()) };
        let pos: Vec<[f32; 3]> = p.chunks(stride).filter(|c| c.len() >= 3).map(|c| apply(m, [c[0], c[1], c[2]])).collect();
        let vertex_colors = semantic(vertices, "COLOR").and_then(|i| self.source(i));
        let (mut tris, mut colors) = (Vec::new(), Vec::new());
        for prim in mesh.children().filter(|n| matches!(n.tag_name().name(), "triangles" | "polylist" | "polygons" | "tristrips" | "trifans")) {
            let inputs: Vec<Node> = tagged(prim, "input").collect();
            let offset = |i: &Node| i.attribute("offset").and_then(|o| o.parse::<usize>().ok()).unwrap_or(0);
            let step = inputs.iter().map(|i| offset(i) + 1).max().unwrap_or(1);
            let Some(vin) = inputs.iter().find(|i| i.attribute("semantic") == Some("VERTEX")) else { continue };
            let color_in = inputs.iter().find(|i| i.attribute("semantic") == Some("COLOR"));
            let colors_src = color_in.and_then(|c| self.source(*c).map(|s| (s, offset(c)))).or_else(|| vertex_colors.clone().map(|s| (s, offset(vin))));
            let material = prim.attribute("material").and_then(|s| bound.get(s)).copied();
            for (corners, idx) in polygons(prim, step) {
                let at = |k: usize, off: usize| idx.get(k * step + off).copied().unwrap_or(u32::MAX);
                for t in corners {
                    tris.push(t.map(|k| at(k, offset(vin))));
                    let vc = colors_src.as_ref().and_then(|((c, s), off)| average(c, *s, t.map(|k| at(k, *off))));
                    colors.push(vc.or(material));
                }
            }
        }
        self.b.add(pos, tris, |i| colors[i])
    }
}

/// The triangles of a primitive as corner numbers, with the index list they refer to (`step`
/// indices per corner).
fn polygons(prim: Node, step: usize) -> Vec<(Vec<[usize; 3]>, Vec<u32>)> {
    let kind = prim.tag_name().name();
    let fan = |n: usize, first: usize| (1..n.saturating_sub(1)).map(move |k| [first, first + k, first + k + 1]);
    tagged(prim, "p")
        .map(|p| {
            let idx = ints(p);
            let n = idx.len() / step;
            let tris: Vec<[usize; 3]> = match kind {
                "triangles" => (0..n / 3).map(|t| [t * 3, t * 3 + 1, t * 3 + 2]).collect(),
                "polylist" => {
                    let counts = tagged(prim, "vcount").next().map(ints).unwrap_or_default();
                    let mut first = 0;
                    counts.iter().flat_map(|&c| { let f = fan(c as usize, first); first += c as usize; f }).filter(|t| t[2] < n).collect()
                }
                "tristrips" => (0..n.saturating_sub(2)).map(|k| if k % 2 == 0 { [k, k + 1, k + 2] } else { [k + 1, k, k + 2] }).collect(),
                // A polygon or a fan per <p>.
                _ => fan(n, 0).collect(),
            };
            (tris, idx)
        })
        .collect()
}

fn average(c: &[f32], stride: usize, t: [u32; 3]) -> Option<[u8; 3]> {
    let get = |i: u32| c.get(i as usize * stride..i as usize * stride + 3);
    let (a, b, d) = (get(t[0])?, get(t[1])?, get(t[2])?);
    Some(std::array::from_fn(|k| ((a[k] + b[k] + d[k]) / 3.0 * 255.0).clamp(0.0, 255.0) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quad_moved_by_its_node_in_its_material() {
        let dae = r##"<COLLADA><asset><up_axis>Z_UP</up_axis></asset>
            <library_effects><effect id="e"><profile_COMMON><technique><lambert><diffuse><color>1 0 0 1</color></diffuse></lambert></technique></profile_COMMON></effect></library_effects>
            <library_materials><material id="m"><instance_effect url="#e"/></material></library_materials>
            <library_geometries><geometry id="g"><mesh>
              <source id="p"><float_array count="12">0 0 0 1 0 0 1 1 0 0 1 0</float_array><technique_common><accessor stride="3"/></technique_common></source>
              <vertices id="v"><input semantic="POSITION" source="#p"/></vertices>
              <polylist material="sym" count="1"><input semantic="VERTEX" source="#v" offset="0"/><vcount>4</vcount><p>0 1 2 3</p></polylist>
            </mesh></geometry></library_geometries>
            <library_visual_scenes><visual_scene id="s"><node><translate>5 0 0</translate>
              <instance_geometry url="#g"><bind_material><technique_common><instance_material symbol="sym" target="#m"/></technique_common></bind_material></instance_geometry>
            </node></visual_scene></library_visual_scenes>
            <scene><instance_visual_scene url="#s"/></scene></COLLADA>"##;
        let m = read(dae.as_bytes(), false).unwrap();
        assert_eq!((m.tris.len(), m.colors[0], m.z_up), (2, [255, 0, 0], true));
        assert_eq!(m.pos[1], [6.0, 0.0, 0.0]);
    }
}
