//! 3DS (3D Studio, still exported by many tools): binary chunks with the objects' triangles,
//! already in world coordinates, and the diffuse colour or picture of the materials of each face.

use std::collections::HashMap;

use crate::model::{Builder, Mesh, Sibling};

const MAIN: u16 = 0x4D4D;
const EDITOR: u16 = 0x3D3D;
const OBJECT: u16 = 0x4000;
const TRIANGLES: u16 = 0x4100;
const VERTICES: u16 = 0x4110;
const FACES: u16 = 0x4120;
const FACE_MATERIAL: u16 = 0x4130;
const MAP_COORDS: u16 = 0x4140;
const MATERIAL: u16 = 0xAFFF;
const MATERIAL_NAME: u16 = 0xA000;
const DIFFUSE: u16 = 0xA020;
const COLOR_F32: u16 = 0x0010;
const COLOR_U8: u16 = 0x0011;
const TEXTURE: u16 = 0xA200;
const MAP_NAME: u16 = 0xA300;

/// A material: its colour, and its picture (`Builder::texture`).
type Look = (Option<[u8; 3]>, u16);

/// The chunks inside `data`: (id, content).
fn chunks(data: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let head = data.get(at..at + 6)?;
        let id = u16::from_le_bytes([head[0], head[1]]);
        let len = u32::from_le_bytes([head[2], head[3], head[4], head[5]]) as usize;
        let body = data.get(at + 6..at.checked_add(len)?)?;
        at += len.max(6);
        Some((id, body))
    })
}

fn u16_at(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(i..i + 2)?.try_into().ok()?))
}

/// A zero-terminated name and what follows it.
fn name(d: &[u8]) -> (String, &[u8]) {
    let end = d.iter().position(|&c| c == 0).unwrap_or(d.len());
    (String::from_utf8_lossy(&d[..end]).into_owned(), d.get(end + 1..).unwrap_or(&[]))
}

pub fn read(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
    let main = chunks(bytes).find(|c| c.0 == MAIN).ok_or("not a 3DS file")?.1;
    let editor = chunks(main).find(|c| c.0 == EDITOR).ok_or("no 3D data in the file")?.1;
    let mut b = Builder::default();
    let mut materials: HashMap<String, Look> = HashMap::new();
    for (_, d) in chunks(editor).filter(|c| c.0 == MATERIAL) {
        let Some((mat, color, map)) = material(d) else { continue };
        // DOS names, often in capitals while the file is not.
        let tex = map.map_or(0, |f| match b.texture_file(&f, sibling) {
            0 => b.texture_file(&f.to_lowercase(), sibling),
            k => k,
        });
        materials.insert(mat, (color, tex));
    }
    for (_, obj) in chunks(editor).filter(|c| c.0 == OBJECT) {
        for (_, tri) in chunks(name(obj).1).filter(|c| c.0 == TRIANGLES) {
            object(&mut b, tri, &materials)?;
        }
    }
    b.finish(true)
}

fn object(b: &mut Builder, tri: &[u8], materials: &HashMap<String, Look>) -> Result<(), String> {
    let mut pos = Vec::new();
    let mut faces: Vec<[u32; 3]> = Vec::new();
    let mut looks: Vec<Look> = Vec::new();
    let mut uv: Vec<[f32; 2]> = Vec::new();
    for (id, d) in chunks(tri) {
        match id {
            VERTICES => {
                let n = u16_at(d, 0).unwrap_or(0) as usize;
                pos = d.get(2..).unwrap_or(&[]).as_chunks::<12>().0.iter().take(n).map(|v| std::array::from_fn(|k| f32::from_le_bytes(v[k * 4..k * 4 + 4].try_into().unwrap()))).collect();
            }
            FACES => {
                let n = u16_at(d, 0).unwrap_or(0) as usize;
                let list = d.get(2..).unwrap_or(&[]);
                faces = list.as_chunks::<8>().0.iter().take(n).map(|f| std::array::from_fn(|k| u16::from_le_bytes([f[k * 2], f[k * 2 + 1]]) as u32)).collect();
                looks = vec![(None, 0); faces.len()];
                // The material groups follow the faces' list, inside the same chunk.
                for (id, g) in chunks(list.get(n * 8..).unwrap_or(&[])) {
                    if id == FACE_MATERIAL {
                        let (mat, rest) = name(g);
                        let count = u16_at(rest, 0).unwrap_or(0) as usize;
                        for i in (0..count).filter_map(|i| u16_at(rest, 2 + i * 2)) {
                            if let (Some(l), Some(m)) = (looks.get_mut(i as usize), materials.get(&mat)) {
                                *l = *m;
                            }
                        }
                    }
                }
            }
            MAP_COORDS => {
                let n = u16_at(d, 0).unwrap_or(0) as usize;
                // V upwards, as rows go down: turned.
                uv = d.get(2..).unwrap_or(&[]).as_chunks::<8>().0.iter().take(n).map(|c| [f32::from_le_bytes(c[..4].try_into().unwrap()), 1.0 - f32::from_le_bytes(c[4..].try_into().unwrap())]).collect();
            }
            _ => {}
        }
    }
    let corners = |i: usize| {
        let c = faces[i].map(|v| uv.get(v as usize).copied());
        c.iter().all(Option::is_some).then(|| c.map(Option::unwrap_or_default))
    };
    // The picture alone where it can be placed, otherwise the material's colour.
    let place = |i: usize| corners(i).filter(|_| looks[i].1 > 0).map(|c| (c, looks[i].1));
    b.add_textured(pos, faces.clone(), |i| if place(i).is_some() { Some([255; 3]) } else { looks[i].0 }, place)
}

/// Name, diffuse colour and picture's file.
fn material(d: &[u8]) -> Option<(String, Option<[u8; 3]>, Option<String>)> {
    let mat = name(chunks(d).find(|c| c.0 == MATERIAL_NAME)?.1).0;
    let map = chunks(d).find(|c| c.0 == TEXTURE).and_then(|t| chunks(t.1).find(|c| c.0 == MAP_NAME)).map(|c| name(c.1).0).filter(|n| !n.is_empty());
    let diffuse = chunks(d).find(|c| c.0 == DIFFUSE).map_or(&[][..], |c| c.1);
    let color = chunks(diffuse).find_map(|(id, c)| match id {
        COLOR_U8 => Some([*c.first()?, *c.get(1)?, *c.get(2)?]),
        COLOR_F32 => Some(std::array::from_fn(|k| (f32::from_le_bytes(c.get(k * 4..k * 4 + 4).unwrap_or(&[0; 4]).try_into().unwrap()).clamp(0.0, 1.0) * 255.0) as u8)),
        _ => None,
    });
    Some((mat, color, map))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
        [&id.to_le_bytes()[..], &(body.len() as u32 + 6).to_le_bytes(), body].concat()
    }

    #[test]
    fn a_red_triangle() {
        let v: Vec<u8> = [3u16.to_le_bytes().to_vec(), [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect()].concat();
        let group = chunk(FACE_MATERIAL, b"red\0\x01\x00\x00\x00");
        let f = [&[1, 0, 0, 0, 1, 0, 2, 0, 0, 0][..], &group].concat();
        let tri = chunk(TRIANGLES, &[chunk(VERTICES, &v), chunk(FACES, &f)].concat());
        let mat = chunk(MATERIAL, &[chunk(MATERIAL_NAME, b"red\0"), chunk(DIFFUSE, &chunk(COLOR_U8, &[255, 0, 0]))].concat());
        let file = chunk(MAIN, &chunk(EDITOR, &[mat, chunk(OBJECT, &[b"tri\0".to_vec(), tri].concat())].concat()));
        let m = read(&file, &|_| None).unwrap();
        assert_eq!((m.tris.len(), m.colors.clone()), (1, vec![[255, 0, 0]]));
        assert!(read(b"\x4d\x4d\xff\xff\xff\xff", &|_| None).is_err());
        // With a picture (named in capitals, the file is not) and its coordinates.
        let uv = [3u16.to_le_bytes().to_vec(), [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0].iter().flat_map(|f| f.to_le_bytes()).collect()].concat();
        let tri = chunk(TRIANGLES, &[chunk(VERTICES, &v), chunk(MAP_COORDS, &uv), chunk(FACES, &f)].concat());
        let mat = chunk(MATERIAL, &[chunk(MATERIAL_NAME, b"red\0"), chunk(TEXTURE, &chunk(MAP_NAME, b"WOOD.PNG\0"))].concat());
        let file = chunk(MAIN, &chunk(EDITOR, &[mat, chunk(OBJECT, &[b"tri\0".to_vec(), tri].concat())].concat()));
        let mut png = Vec::new();
        image::RgbaImage::new(2, 2).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let m = read(&file, &|n| (n == "wood.png").then(|| png.clone())).unwrap();
        assert_eq!((m.tex.clone(), m.uv[0][1]), (vec![1], [1.0, 1.0]));
    }
}
