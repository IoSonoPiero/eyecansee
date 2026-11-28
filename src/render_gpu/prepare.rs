//! What the graphics card is given for a model, made without it: the triangles' vertices (`STRIDE`
//! bytes each) grouped by their pictures, and the pictures with their smaller copies (mipmaps).

use crate::model::{DEFAULT, Mesh, Surface};
use crate::render3d;

use super::STRIDE;

/// The surface of triangle `i`, if it has one.
fn surface(mesh: &Mesh, i: usize) -> Option<&Surface> {
    mesh.surfaces.get((*mesh.surface.get(i)? as usize).checked_sub(1)?)
}

/// Triangle `i`'s pictures: colour, bumps, roughness and metal, occlusion, glow (0 = none).
fn key(mesh: &Mesh, i: usize) -> [u16; 5] {
    let tex = mesh.tex.get(i).copied().unwrap_or(0);
    surface(mesh, i).map_or([tex, 0, 0, 0, 0], |s| [tex, s.bumps, s.rough_metal, s.occlusion_map, s.glow_map])
}

/// The triangles' pictures (`key`, 0 where it is not among the `pictures`) and the triangles
/// sorted by them.
fn order(mesh: &Mesh, pictures: usize) -> (Vec<[u16; 5]>, Vec<usize>) {
    let keys: Vec<[u16; 5]> = (0..mesh.tris.len()).map(|i| key(mesh, i).map(|k| if (k as usize) < pictures { k } else { 0 })).collect();
    let mut order: Vec<usize> = (0..mesh.tris.len()).collect();
    order.sort_by_key(|&i| keys[i]);
    (keys, order)
}

/// A picture's bytes followed by those of its smaller copies down to 1 × 1, and how many there
/// are with it.
fn mips(img: &image::RgbaImage) -> (u32, Vec<u8>) {
    let levels = 32 - img.width().max(img.height()).leading_zeros();
    let mut data = img.as_raw().clone();
    let mut level = img.clone();
    for _ in 1..levels {
        level = crate::mipmap::half(&level);
        data.extend_from_slice(level.as_raw());
    }
    (levels, data)
}

/// The face of a section: a square on the plane, larger than the model, in the cut's colour and
/// marked (4) as not cut itself; two triangles (`STRIDE` bytes a vertex).
pub(super) fn cap_vertices(mesh: &Mesh, cut: &render3d::Cut, out: &mut Vec<u8>) {
    let plane = cut.plane(mesh);
    let axis = cut.axis.min(2);
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let r = mesh.radius * 2.0;
    let corner = |a: f32, b: f32| {
        let mut p = mesh.center;
        p[axis] = plane[3] * plane[axis];
        p[u] += a * r;
        p[v] += b * r;
        p
    };
    let [cr, cg, cb] = render3d::CAP;
    for p in [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)] {
        out.extend(p.iter().flat_map(|f| f.to_le_bytes()));
        out.extend([cr, cg, cb, 255]);
        out.extend([0.0f32; 5].iter().flat_map(|f| f.to_le_bytes()));
        out.extend([0, 255, 0, 4, 0, 0, 0, 255]);
    }
}

/// The three vertices of triangle `i` (`STRIDE` bytes each).
fn vertices(mesh: &Mesh, i: usize, out: &mut Vec<u8>) {
    let [r, g, b] = mesh.colors.get(i).copied().unwrap_or(DEFAULT);
    let uv = mesh.uv.get(i).copied().unwrap_or_default();
    let normals = mesh.normals.get(i).copied().unwrap_or_default();
    let unorm = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (pbr, glow) = match surface(mesh, i) {
        Some(s) => ([unorm(s.metallic), unorm(s.roughness), unorm(s.occlusion), 1 + if s.bumps > 0 { 2 } else { 0 }], s.glow.map(unorm)),
        None => ([0, 255, 0, 0], [0; 3]),
    };
    for (k, v) in mesh.tris[i].iter().enumerate() {
        out.extend(mesh.pos[*v as usize].iter().flat_map(|f| f.to_le_bytes()));
        out.extend([r, g, b, 255]);
        out.extend(uv[k].iter().chain(&normals[k]).flat_map(|f| f.to_le_bytes()));
        out.extend(pbr);
        out.extend([glow[0], glow[1], glow[2], 255]);
    }
}

/// A picture for the card: its size, how many copies with it (mipmaps) and their bytes.
pub(super) struct Picture {
    pub size: [u32; 2],
    pub levels: u32,
    pub data: Vec<u8>,
}

/// What the card is given for a model, one after the other (`model`).
pub(super) enum Part {
    /// Its pictures, the white one first (`Mesh::textures` after it).
    Pictures(Vec<Picture>),
    /// Triangles that have the same pictures (`key`): how many vertices, and their bytes.
    Piece([u16; 5], u32, Vec<u8>),
}

/// How many triangles a piece holds at most, for buffers of `limit` bytes at most.
pub(super) fn per(limit: u64) -> usize {
    (limit / (STRIDE * 3)).max(1) as usize
}

/// The parts of `mesh` for the card (`Part`), to `send` in order, made outside the window's
/// frame (audit 4.12): its pictures with their copies, then its triangles grouped by pictures,
/// `per` at most in a piece. `send` says `false` when they are no longer wanted: nothing more is
/// made.
pub(super) fn model(mesh: &Mesh, per: usize, mut send: impl FnMut(Part) -> bool) {
    let white = image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]));
    let pictures: Vec<Picture> = std::iter::once(&white).chain(mesh.textures.iter().map(|t| &**t)).map(picture).collect();
    let n = pictures.len();
    if !send(Part::Pictures(pictures)) {
        return;
    }
    let (keys, order) = order(mesh, n);
    for group in order.chunk_by(|&a, &b| keys[a] == keys[b]) {
        for tris in group.chunks(per) {
            let mut bytes = Vec::with_capacity(tris.len() * 3 * STRIDE as usize);
            for &i in tris {
                vertices(mesh, i, &mut bytes);
            }
            if !send(Part::Piece(keys[group[0]], tris.len() as u32 * 3, bytes)) {
                return;
            }
        }
    }
}

/// A picture with its smaller copies.
fn picture(img: &image::RgbaImage) -> Picture {
    let (levels, data) = mips(img);
    Picture { size: [img.width(), img.height()], levels, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model of `n` triangles, read from a binary STL.
    fn stl(n: usize) -> Mesh {
        let mut b = vec![0u8; 80];
        b.extend((n as u32).to_le_bytes());
        for i in 0..n {
            let x = i as f32;
            for v in [[0.0f32; 3], [x, 0.0, 0.0], [x, 1.0, 0.0], [x, 0.0, 1.0]] {
                b.extend(v.iter().flat_map(|c| c.to_le_bytes()));
            }
            b.extend([0, 0]);
        }
        crate::model::parse(&b, "a.stl").unwrap()
    }

    /// Audit 4.12: the parts in order, the pictures first (the white one, then the model's with
    /// their copies), then the triangles grouped by pictures, `per` at most in a piece: all of
    /// them, each vertex `STRIDE` bytes.
    #[test]
    fn the_parts_of_a_model_4_12() {
        let mut mesh = stl(5);
        mesh.textures = vec![std::sync::Arc::new(image::RgbaImage::new(8, 4))];
        mesh.tex = vec![1, 0, 1, 0, 1];
        mesh.uv = vec![[[0.0; 2]; 3]; 5];
        let mut parts = Vec::new();
        model(&mesh, 2, |p| {
            parts.push(match p {
                Part::Pictures(p) => format!("pictures {:?}", p.iter().map(|p| (p.size, p.levels, p.data.len())).collect::<Vec<_>>()),
                Part::Piece(k, n, b) => format!("piece {} {n} {}", k[0], b.len() as u64 / STRIDE),
            });
            true
        });
        assert_eq!(parts, ["pictures [([1, 1], 1, 4), ([8, 4], 4, 172)]", "piece 0 6 6", "piece 1 6 6", "piece 1 3 3"]);
    }

    /// No longer wanted (another model, the viewer closed): nothing more is made.
    #[test]
    fn the_parts_stop_when_not_wanted() {
        let mut sent = 0;
        model(&stl(10), 2, |_| {
            sent += 1;
            sent < 2
        });
        assert_eq!(sent, 2);
    }

    /// The bytes of the vertices are made a piece at a time, never all together: 100 000
    /// triangles (13 MB of vertices) in pieces of 1000.
    #[test]
    fn the_vertices_made_a_piece_at_a_time() {
        let mesh = stl(100_000);
        let ((), held) = crate::test_alloc::peak(|| model(&mesh, 1000, |_| true));
        let all = mesh.tris.len() * 3 * STRIDE as usize;
        assert!(held < all / 4, "held {held} bytes for {all} of vertices");
    }

    /// How long the parts of a model of the private collection take, as the log says it at the end
    /// of `Gpu::take` ("model on the graphics card: … in …"), less the copies onto the card:
    /// `EYECANSEE_MODEL=<file> cargo test --release model_parts_time -- --ignored --nocapture`.
    #[test]
    #[ignore = "a model of the private collection, named by EYECANSEE_MODEL"]
    fn model_parts_time() {
        let path = std::env::var("EYECANSEE_MODEL").expect("EYECANSEE_MODEL=<a model>");
        let bytes = std::fs::read(&path).unwrap();
        let t = std::time::Instant::now();
        let mesh = crate::model::parse(&bytes, &path).unwrap();
        let read = t.elapsed();
        let t = std::time::Instant::now();
        let (mut pieces, mut sum) = (0, 0);
        model(&mesh, per(128 << 20), |p| {
            if let Part::Piece(_, _, b) = &p {
                pieces += 1;
                sum += b.len();
            }
            true
        });
        println!("model on the graphics card: {} triangles, {} pictures, {:.0?} ({pieces} pieces, {} MB; the file read in {read:.0?})", mesh.tris.len(), mesh.textures.len(), t.elapsed(), sum >> 20);
    }
}
