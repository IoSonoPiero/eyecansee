//! A 3D model written as OBJ (audit 6.16), as Blender and MeshLab write it: the points shared by
//! the triangles that meet there (split only where their colours differ), the pictures' coordinates
//! (`vt`) and the normals (`vn`) each once, and next to it a `.mtl` with a material for each
//! picture and surface, and the pictures as PNG files (`Made::beside`). The materials keep what
//! `model_mtl` reads: the picture (`map_Kd`), roughness and metal (`Pr`, `Pm`, the PBR
//! extension), their own light (`Ke`, `map_Ke`) and bumps (`norm`). Pictures of roughness and
//! metal, and of the shadows in the creases, have no place in a `.mtl` (said: `LOST_SURFACES`).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use crate::model_export::{Beside, Made, Tick, View, ticked, unit_line};

/// The pictures the `.mtl` names (`named`) as PNG files next to the model, named after it
/// (`stem`): the name of each (index in `textures`), `None` for one not written.
fn pictures(v: &View, named: &[bool], stem: &str, beside: &Beside, files: &mut Vec<(String, Vec<u8>)>) -> Vec<Option<String>> {
    let many = named.iter().filter(|&&n| n).count() > 1;
    let mut number = 0;
    v.mesh
        .textures
        .iter()
        .zip(named)
        .map(|(img, &named)| {
            if !named {
                return None;
            }
            let mut png = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
            number += 1;
            let wanted = if many { format!("{stem}-{number}.png") } else { format!("{stem}.png") };
            let name = (beside.free)(&wanted);
            files.push((name.clone(), png));
            Some(name)
        })
        .collect()
}

/// Which pictures a `.mtl` names: those on triangles with their coordinates, and the bumps and
/// the glow of the shiny surfaces (`shiny`); not those of roughness and metal or of the shadows
/// in the creases, which have no place there (#340).
fn named(v: &View, shiny: impl Fn(u16) -> bool) -> Vec<bool> {
    let m = v.mesh;
    let mut named = vec![false; m.textures.len()];
    let mut mark = |k: u16| {
        if let Some(n) = (k as usize).checked_sub(1).and_then(|i| named.get_mut(i)) {
            *n = true;
        }
    };
    for &k in m.tex.iter().take(m.uv.len()) {
        mark(k);
    }
    for (i, s) in m.surfaces.iter().enumerate() {
        if shiny(i as u16 + 1) && m.surface.contains(&(i as u16 + 1)) {
            mark(s.bumps);
            mark(s.glow_map);
        }
    }
    named
}

/// A value seen once, numbered from 1 as OBJ counts: the points, the coordinates, the normals.
#[derive(Default)]
struct Once<K> {
    seen: HashMap<K, u32>,
    text: String,
}

impl<K: std::hash::Hash + Eq> Once<K> {
    /// The number of `k`, its line written by `line` the first time.
    fn of(&mut self, k: K, line: impl FnOnce(&mut String)) -> u32 {
        let n = self.seen.len() as u32 + 1;
        *self.seen.entry(k).or_insert_with(|| {
            line(&mut self.text);
            n
        })
    }
}

pub(crate) fn obj(v: &View, tick: Tick, beside: &Beside) -> Option<Made> {
    let m = v.mesh;
    let mut files = Vec::new();
    // No spaces in the names next to it: `mtllib` is a list split at spaces, and many readers
    // cut a name there (Piero, 24/11, #340). The OBJ keeps its own name.
    let stem: String = beside.stem.chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect();
    // A matte surface (the one every model without its own gets) needs no material.
    let shiny = |k: u16| (k as usize).checked_sub(1).and_then(|i| m.surfaces.get(i)).is_some_and(|s| *s != crate::model::Surface::default());
    let names = pictures(v, &named(v, shiny), &stem, beside, &mut files);
    let pic = |t: usize| m.tex.get(t).and_then(|&k| (k as usize).checked_sub(1)).and_then(|i| names.get(i).cloned().flatten());
    // A triangle keeps its picture when the picture was written and it has its coordinates.
    let kept = |t: usize| m.uv.get(t).is_some() && pic(t).is_some();
    let mut groups: BTreeMap<(u16, u16), Vec<usize>> = BTreeMap::new();
    for t in 0..m.tris.len() {
        let tex = if kept(t) { m.tex[t] } else { 0 };
        let surface = m.surface.get(t).copied().filter(|&k| shiny(k)).unwrap_or(0);
        groups.entry((tex, surface)).or_default().push(t);
    }
    let materials = groups.keys().any(|&(tex, surface)| tex > 0 || surface > 0);
    let mtl = materials.then(|| (beside.free)(&format!("{stem}.mtl")));
    // Colours on the points when the model has some, or a picture that could not be kept.
    let colored = !m.colors.is_empty() || (0..m.tris.len()).any(|t| v.picture(t).is_some() && !kept(t));
    let (smooth, scale) = (!m.normals.is_empty(), v.scale);
    let (mut points, mut coords, mut normals) = (Once::default(), Once::default(), Once::default());
    if !colored {
        for p in &m.pos {
            let p = v.turn(*p).map(|c| c * scale);
            let _ = writeln!(points.text, "v {} {} {}", p[0], p[1], p[2]);
        }
    }
    let mut faces = String::new();
    let mut done = 0;
    for (i, ((tex, surface), tris)) in groups.iter().enumerate() {
        if materials {
            let _ = writeln!(faces, "usemtl {}", material_name(i, *tex, *surface));
        }
        for &t in tris {
            if !ticked(tick, done, m.tris.len()) {
                return None;
            }
            done += 1;
            let colors = if kept(t) { [v.tint(t); 3] } else { v.corner_colors(t) };
            let corner_normals = smooth.then(|| v.normals(t));
            let _ = write!(faces, "f");
            for k in 0..3 {
                let at = m.tris[t][k];
                let p = if colored {
                    let c = colors[k];
                    points.of((at, c), |s| {
                        let p = v.turn(m.pos[at as usize]).map(|x| x * scale);
                        let c = c.map(|x| f32::from(x) / 255.0);
                        let _ = writeln!(s, "v {} {} {} {:.4} {:.4} {:.4}", p[0], p[1], p[2], c[0], c[1], c[2]);
                    })
                } else {
                    at + 1
                };
                let uv = kept(t).then(|| m.uv[t][k]).map(|[x, y]| coords.of([x.to_bits(), y.to_bits()], |s| {
                    let _ = writeln!(s, "vt {} {}", x, 1.0 - y);
                }));
                let n = corner_normals.map(|n| n[k]).map(|n| normals.of(n.map(f32::to_bits), |s| {
                    let _ = writeln!(s, "vn {:.5} {:.5} {:.5}", n[0], n[1], n[2]);
                }));
                let _ = match (uv, n) {
                    (Some(uv), Some(n)) => write!(faces, " {p}/{uv}/{n}"),
                    (Some(uv), None) => write!(faces, " {p}/{uv}"),
                    (None, Some(n)) => write!(faces, " {p}//{n}"),
                    (None, None) => write!(faces, " {p}"),
                };
            }
            faces.push('\n');
        }
    }
    let mut s = format!("# EyeCanSee\n{}", unit_line(m, "#"));
    if let Some(name) = &mtl {
        let _ = writeln!(s, "mtllib {name}");
        let text = groups.keys().enumerate().map(|(i, &(tex, surface))| material(v, &names, i, tex, surface)).collect::<Vec<_>>().join("\n");
        files.push((name.clone(), format!("# EyeCanSee\n{text}").into_bytes()));
    }
    s.push_str(&points.text);
    s.push_str(&coords.text);
    s.push_str(&normals.text);
    s.push_str(&faces);
    Some(Made { main: s.into_bytes(), beside: files })
}

fn material_name(i: usize, tex: u16, surface: u16) -> String {
    format!("material{i}_{tex}_{surface}")
}

/// The `.mtl` lines of a group's material: its picture and its surface.
fn material(v: &View, names: &[Option<String>], i: usize, tex: u16, surface: u16) -> String {
    let pic = |k: u16| (k as usize).checked_sub(1).and_then(|i| names.get(i).cloned().flatten());
    let mut out = format!("newmtl {}\n", material_name(i, tex, surface));
    if let Some(name) = pic(tex) {
        // White under the picture, as the reader and Blender take it; the tints are on the points.
        let _ = writeln!(out, "Kd 1 1 1\nmap_Kd {name}");
    }
    if let Some(s) = (surface as usize).checked_sub(1).and_then(|i| v.mesh.surfaces.get(i)) {
        let _ = writeln!(out, "Pr {}\nPm {}", s.roughness, s.metallic);
        if s.glow != [0.0; 3] {
            let _ = writeln!(out, "Ke {} {} {}", s.glow[0], s.glow[1], s.glow[2]);
        }
        if let Some(name) = pic(s.glow_map) {
            let _ = writeln!(out, "map_Ke {name}");
        }
        if let Some(name) = pic(s.bumps) {
            let _ = writeln!(out, "norm {name}");
        }
    }
    out
}
