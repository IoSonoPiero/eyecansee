//! Measures of a 3D model (#141): its overall size along X, Y and Z, its surface and, when it is
//! closed (every edge shared by two triangles) and its faces turn the same way, its volume; and
//! the point of the model under the pointer, for a distance between two points. In the file's units.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use crate::model::Mesh;

/// The sizes of the models seen, each measured once in the background (`get`). Kept for the
/// model read, not for its path: a file saved again under the same name and read again is
/// measured again (audit 6.8). `Weak` keeps the model's place, not its points: no other model
/// can take that place while the sizes are kept, and they go when the model has gone.
#[derive(Default)]
pub(crate) struct Memo {
    /// By path, with the model they are of: `None` while being measured.
    by: HashMap<String, (Weak<Mesh>, Option<Sizes>)>,
    done: Arc<Mutex<Vec<Measured>>>,
}

/// Sizes measured in the background: the path and the model they are of.
type Measured = (String, Weak<Mesh>, Sizes);

impl Memo {
    /// The sizes of `mesh`, read from `path`: measured in a thread of their own the first time,
    /// `None` until then; `wake` when they are ready.
    pub(crate) fn get(&mut self, path: &str, mesh: &Arc<Mesh>, wake: impl FnOnce() + Send + 'static) -> Option<Sizes> {
        for (p, of, s) in self.done.lock().unwrap().drain(..) {
            // A late result of a model read before under the same path is dropped.
            if let Some((was, sizes)) = self.by.get_mut(&p)
                && was.ptr_eq(&of)
            {
                *sizes = Some(s);
            }
        }
        if let Some((_, s)) = self.by.get(path).filter(|(of, _)| std::ptr::eq(of.as_ptr(), Arc::as_ptr(mesh))) {
            return s.clone();
        }
        self.by.retain(|_, (of, _)| of.strong_count() > 0);
        self.by.insert(path.to_string(), (Arc::downgrade(mesh), None));
        let (done, path, mesh) = (self.done.clone(), path.to_string(), mesh.clone());
        crate::logging::spawn_named("model-measure", move || {
            let s = sizes(&mesh);
            done.lock().unwrap().push((path, Arc::downgrade(&mesh), s));
            wake();
        });
        None
    }

    /// The sizes of `mesh`, read from `path`, if they are measured already (`get`).
    pub(crate) fn known(&self, path: &str, mesh: &Arc<Mesh>) -> Option<Sizes> {
        self.by.get(path).filter(|(of, _)| std::ptr::eq(of.as_ptr(), Arc::as_ptr(mesh))).and_then(|(_, s)| s.clone())
    }
}

/// What is measured once for a model.
#[derive(Clone, Debug, PartialEq)]
pub struct Sizes {
    /// Along X, Y and Z of the file.
    pub size: [f64; 3],
    pub surface: f64,
    /// `None` for a model that is not closed (`open_edges` edges have one triangle only), or
    /// whose faces do not all turn the same way (`flipped` edges between two faces turned
    /// differently): the sum with its sign would be wrong.
    pub volume: Option<f64>,
    pub open_edges: usize,
    pub flipped: usize,
}

/// Points closer than this share a corner, relative to the model's size: files repeat the corners
/// of each triangle (STL), with rounding.
const WELD: f64 = 1e-6;

pub fn sizes(mesh: &Mesh) -> Sizes {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    // Only the points of the triangles, as the camera and the section: a file may keep others
    // (OBJ lines and points, PLY points no face uses).
    for p in mesh.tris.iter().flatten().map(|&v| mesh.pos[v as usize]) {
        for k in 0..3 {
            lo[k] = lo[k].min(f64::from(p[k]));
            hi[k] = hi[k].max(f64::from(p[k]));
        }
    }
    let size = if mesh.tris.is_empty() { [0.0; 3] } else { [0, 1, 2].map(|k| hi[k] - lo[k]) };
    let step = size.iter().copied().fold(0.0, f64::max).max(f64::MIN_POSITIVE) * WELD;
    // Each corner by its welded place.
    let mut ids: HashMap<[i64; 3], u32> = HashMap::new();
    let corner: Vec<u32> = mesh
        .pos
        .iter()
        .map(|p| {
            let key = p.map(|v| (f64::from(v) / step).round() as i64);
            let n = ids.len() as u32;
            *ids.entry(key).or_insert(n)
        })
        .collect();
    let mut surface = 0.0;
    let mut signed = 0.0;
    // Each edge: how many triangles have it, and which way they go along it (+1 from the lower
    // corner, −1 back): two faces turned the same way go along it the other way round, 0 in all.
    let mut edges: HashMap<(u32, u32), (i32, i32)> = HashMap::new();
    for t in &mesh.tris {
        let [a, b, c] = t.map(|i| mesh.pos[i as usize].map(f64::from));
        let (u, v) = (sub(b, a), sub(c, a));
        let n = cross(u, v);
        surface += dot(n, n).sqrt() / 2.0;
        signed += dot(a, cross(b, c)) / 6.0;
        let w = t.map(|i| corner[i as usize]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue;
        }
        for (x, y) in [(w[0], w[1]), (w[1], w[2]), (w[2], w[0])] {
            let e = edges.entry((x.min(y), x.max(y))).or_insert((0, 0));
            *e = (e.0 + 1, e.1 + if x < y { 1 } else { -1 });
        }
    }
    let open_edges = edges.values().filter(|e| e.0 != 2).count();
    // ponytail: a whole part turned inside out, on its own, still turns one way: not seen here.
    let flipped = edges.values().filter(|e| e.0 == 2 && e.1 != 0).count();
    let volume = (open_edges == 0 && flipped == 0 && !mesh.tris.is_empty()).then_some(signed.abs());
    Sizes { size, surface, volume, open_edges, flipped }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Where the ray from `origin` along `dir` (model coordinates) first meets the model.
pub fn hit(mesh: &Mesh, origin: [f32; 3], dir: [f32; 3]) -> Option<[f32; 3]> {
    let (o, d) = (origin.map(f64::from), dir.map(f64::from));
    let mut best: Option<f64> = None;
    for t in &mesh.tris {
        let [a, b, c] = t.map(|i| mesh.pos[i as usize].map(f64::from));
        // Möller–Trumbore, both faces.
        let (e1, e2) = (sub(b, a), sub(c, a));
        let p = cross(d, e2);
        let det = dot(e1, p);
        if det.abs() < 1e-18 {
            continue;
        }
        let s = sub(o, a);
        let u = dot(s, p) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = cross(s, e1);
        let v = dot(d, q) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let k = dot(e2, q) / det;
        if k > 0.0 && best.is_none_or(|b| k < b) {
            best = Some(k);
        }
    }
    best.map(|k| [0, 1, 2].map(|i| (o[i] + d[i] * k) as f32))
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A closed cube of side 2 written as an ASCII STL: each triangle with its own corners.
    fn cube(open: bool) -> Mesh {
        let c = |i: usize| [0, 1, 2].map(|k| if i >> k & 1 == 1 { 2.0 } else { 0.0 });
        let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        let mut stl = String::from("solid c\n");
        for f in faces.iter().take(if open { 5 } else { 6 }) {
            for t in [[f[0], f[1], f[2]], [f[0], f[2], f[3]]] {
                stl += "facet normal 0 0 0\nouter loop\n";
                for v in t {
                    let p: [f64; 3] = c(v);
                    stl += &format!("vertex {} {} {}\n", p[0], p[1], p[2]);
                }
                stl += "endloop\nendfacet\n";
            }
        }
        stl += "endsolid c\n";
        crate::model::parse(stl.as_bytes(), "c.stl").unwrap()
    }

    #[test]
    fn size_surface_and_volume() {
        let s = sizes(&cube(false));
        assert_eq!(s.size, [2.0, 2.0, 2.0]);
        assert!((s.surface - 24.0).abs() < 1e-6 && (s.volume.unwrap() - 8.0).abs() < 1e-6, "{s:?}");
        assert_eq!(s.open_edges, 0);
        // Without its top: no volume, and the four edges around the hole said.
        let open = sizes(&cube(true));
        assert!(open.volume.is_none() && open.open_edges == 4 && (open.surface - 20.0).abs() < 1e-6, "{open:?}");
        assert_eq!(cube(false).unit, None);
    }

    /// `memo.get` until the sizes are measured, 30 s at most.
    fn measured(memo: &mut Memo, path: &str, mesh: &Arc<Mesh>) -> Sizes {
        let start = std::time::Instant::now();
        loop {
            if let Some(s) = memo.get(path, mesh, || {}) {
                return s;
            }
            assert!(start.elapsed().as_secs() < 30, "not measured in 30 s");
            std::thread::yield_now();
        }
    }

    /// Audit 6.8 (a): the file saved again under the same name and read again is measured
    /// again, and the sizes of the model before, if they come late, are not taken for it.
    #[test]
    fn audit_6_8_a_new_model_same_path() {
        let mut memo = Memo::default();
        let (small, large) = (Arc::new(cube(false)), Arc::new(crate::model::parse(b"v 0 0 0\nv 5 0 0\nv 0 5 0\nf 1 2 3\n", "c.obj").unwrap()));
        assert_eq!(measured(&mut memo, "d/c.stl", &small).size, [2.0; 3]);
        assert_eq!(measured(&mut memo, "d/c.stl", &large).size, [5.0, 5.0, 0.0]);
        // Read again while the model before is still being measured: its result, late, is not
        // taken for the one read now.
        let mut late = Memo::default();
        assert_eq!(late.get("d/c.stl", &small, || {}), None);
        assert_eq!(measured(&mut late, "d/c.stl", &large).size, [5.0, 5.0, 0.0]);
    }

    /// Audit 6.8 (b): a point that no triangle uses, far away, is not in the size.
    #[test]
    fn audit_6_8_b_points_not_used() {
        let ply = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n100 100 100\n3 0 1 2\n";
        let m = crate::model::parse(ply.as_bytes(), "t.ply").unwrap();
        assert_eq!(m.pos.len(), 4);
        assert_eq!(sizes(&m).size, [1.0, 1.0, 0.0]);
    }

    /// Audit 6.8 (c): a closed cube with one face turned the other way has no volume (the sum
    /// with its sign would give a wrong one), and the edges around that face are counted.
    #[test]
    fn audit_6_8_c_faces_turned_differently() {
        let mut m = cube(false);
        // The top (z = 2): turned, it takes its part of the volume away instead of adding it.
        for t in &mut m.tris[2..4] {
            t.swap(1, 2);
        }
        let s = sizes(&m);
        assert_eq!((s.open_edges, s.volume, s.flipped), (0, None, 4), "{s:?}");
        let whole = sizes(&cube(false));
        assert_eq!((whole.flipped, whole.volume.is_some()), (0, true));
    }

    #[test]
    fn the_point_under_the_pointer() {
        let m = cube(false);
        // From above, down through the middle: the top face.
        assert_eq!(hit(&m, [1.0, 1.0, 10.0], [0.0, 0.0, -1.0]), Some([1.0, 1.0, 2.0]));
        assert_eq!(hit(&m, [5.0, 5.0, 10.0], [0.0, 0.0, -1.0]), None);
        // The camera's ray through the middle of the picture meets the model, and the point found
        // goes back to the middle of the picture.
        let view = crate::render3d::View::default();
        let (o, d) = crate::render3d::ray(&m, &view, [800.0, 600.0], [400.0, 300.0]);
        let p = hit(&m, o, d).unwrap();
        let back = crate::render3d::to_screen(&m, &view, [800.0, 600.0], p).unwrap();
        assert!((back[0] - 400.0).abs() < 0.01 && (back[1] - 300.0).abs() < 0.01, "{back:?}");
    }
}
