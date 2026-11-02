//! 3DS (3D Studio, still exported by many tools): binary chunks with the objects' triangles,
//! already in world coordinates, and the diffuse colour of the materials of each face.

use std::collections::HashMap;

use crate::model::{Builder, Mesh};

const MAIN: u16 = 0x4D4D;
const EDITOR: u16 = 0x3D3D;
const OBJECT: u16 = 0x4000;
const TRIANGLES: u16 = 0x4100;
const VERTICES: u16 = 0x4110;
const FACES: u16 = 0x4120;
const FACE_MATERIAL: u16 = 0x4130;
const MATERIAL: u16 = 0xAFFF;
const MATERIAL_NAME: u16 = 0xA000;
const DIFFUSE: u16 = 0xA020;
const COLOR_F32: u16 = 0x0010;
const COLOR_U8: u16 = 0x0011;

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

pub fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let main = chunks(bytes).find(|c| c.0 == MAIN).ok_or("not a 3DS file")?.1;
    let editor = chunks(main).find(|c| c.0 == EDITOR).ok_or("no 3D data in the file")?.1;
    let materials: HashMap<String, [u8; 3]> = chunks(editor).filter(|c| c.0 == MATERIAL).filter_map(|c| material(c.1)).collect();
    let mut b = Builder::default();
    for (_, obj) in chunks(editor).filter(|c| c.0 == OBJECT) {
        for (_, tri) in chunks(name(obj).1).filter(|c| c.0 == TRIANGLES) {
            object(&mut b, tri, &materials)?;
        }
    }
    b.finish(true)
}

fn object(b: &mut Builder, tri: &[u8], materials: &HashMap<String, [u8; 3]>) -> Result<(), String> {
    let mut pos = Vec::new();
    let mut faces = Vec::new();
    let mut colors = Vec::new();
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
                colors = vec![None; faces.len()];
                // The material groups follow the faces' list, inside the same chunk.
                for (id, g) in chunks(list.get(n * 8..).unwrap_or(&[])) {
                    if id == FACE_MATERIAL {
                        let (mat, rest) = name(g);
                        let count = u16_at(rest, 0).unwrap_or(0) as usize;
                        for i in (0..count).filter_map(|i| u16_at(rest, 2 + i * 2)) {
                            if let Some(c) = colors.get_mut(i as usize) {
                                *c = materials.get(&mat).copied();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    b.add(pos, faces, |i| colors[i])
}

fn material(d: &[u8]) -> Option<(String, [u8; 3])> {
    let mat = name(chunks(d).find(|c| c.0 == MATERIAL_NAME)?.1).0;
    let diffuse = chunks(d).find(|c| c.0 == DIFFUSE)?.1;
    let color = chunks(diffuse).find_map(|(id, c)| match id {
        COLOR_U8 => Some([*c.first()?, *c.get(1)?, *c.get(2)?]),
        COLOR_F32 => Some(std::array::from_fn(|k| (f32::from_le_bytes(c.get(k * 4..k * 4 + 4).unwrap_or(&[0; 4]).try_into().unwrap()).clamp(0.0, 1.0) * 255.0) as u8)),
        _ => None,
    })?;
    Some((mat, color))
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
        let m = read(&file).unwrap();
        assert_eq!((m.tris.len(), m.colors.clone()), (1, vec![[255, 0, 0]]));
        assert!(read(b"\x4d\x4d\xff\xff\xff\xff").is_err());
    }
}
