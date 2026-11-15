//! Draws a `Mesh` into an image, on the CPU: perspective camera turning around the model, depth
//! buffer, one light from the viewer's side, bands of rows in parallel. The same picture serves
//! the grid, the viewer and the screenshots, on every system and without a GPU.
// ponytail: CPU rasteriser, ~20 ms for 1 M triangles on 32 cores; wgpu if big models feel slow

use image::{RgbaImage, imageops::FilterType};
use rayon::prelude::*;

use crate::model::{DEFAULT, Mesh};

/// Vertical field of view: narrow, so that a mechanical part does not look distorted.
const FOV: f32 = 0.5;
/// Rows of a band drawn by one thread.
const BAND: usize = 32;

/// Where the camera looks from: how the model is turned (rows of a rotation); `zoom` 1 = the
/// whole model fits; `pan` in fractions of the shorter side of the image, so the view is the same
/// at any size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub turned: [[f32; 3]; 3],
    pub zoom: f32,
    pub pan: [f32; 2],
    /// A section (#145): what is beyond its plane is cut away.
    pub cut: Option<Cut>,
}

/// A cutting plane across an axis of the file (0 X, 1 Y, 2 Z), at `at` from one side of the
/// model's sphere (0) to the other (1); `flip` keeps the other side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub axis: usize,
    pub at: f32,
    pub flip: bool,
}

impl Cut {
    /// Through the middle of the model, across Z for a model with Z up (a plan, for a building),
    /// otherwise across Y.
    pub fn middle(z_up: bool) -> Cut {
        Cut { axis: if z_up { 2 } else { 1 }, at: 0.5, flip: false }
    }

    /// The plane in the model's coordinates: its normal and distance; a point `p` with
    /// `n · p > d` is cut away.
    pub fn plane(&self, mesh: &Mesh) -> [f32; 4] {
        let axis = self.axis.min(2);
        let sign = if self.flip { -1.0 } else { 1.0 };
        let at = mesh.center[axis] + (self.at.clamp(0.0, 1.0) * 2.0 - 1.0) * mesh.radius;
        let mut p = [0.0; 4];
        p[axis] = sign;
        p[3] = sign * at;
        p
    }
}

/// The colour of the cut faces: the inside of the model seen through the cut, as the section
/// fill of CAD programs.
pub const CAP: [u8; 3] = [214, 96, 74];

/// Whether the model's point `p` is cut away by `plane`.
pub(crate) fn cut_away(plane: &[f32; 4], p: [f32; 3]) -> bool {
    plane[0] * p[0] + plane[1] * p[1] + plane[2] * p[2] > plane[3]
}

impl Default for View {
    /// From the front, a little from the right and from above.
    fn default() -> Self {
        View::angles(-0.6, 0.45)
    }
}

type M3 = [[f32; 3]; 3];

fn mul3(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

/// About the screen's vertical axis (`dx`) and horizontal axis (`dy`).
fn screen_turn(dx: f32, dy: f32) -> M3 {
    let (sy, cy) = dx.sin_cos();
    let (sp, cp) = dy.sin_cos();
    mul3(&[[1.0, 0.0, 0.0], [0.0, cp, -sp], [0.0, sp, cp]], &[[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]])
}

impl View {
    /// Turned sideways by `yaw`, then up or down by `pitch` (radians), not zoomed or moved.
    pub fn angles(yaw: f32, pitch: f32) -> View {
        View { turned: screen_turn(yaw, pitch), zoom: 1.0, pan: [0.0; 2], cut: None }
    }

    /// Turned by `dx`, `dy` radians about the axes of the screen, like a trackball: the same way
    /// whatever the model shows, all the way round, and both at once along the diagonal.
    pub fn turn(&mut self, dx: f32, dy: f32) {
        let m = mul3(&screen_turn(dx, dy), &self.turned);
        // Kept a true rotation, rounding errors of many small turns taken out (Gram-Schmidt).
        let norm = |v: [f32; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(f32::MIN_POSITIVE);
            v.map(|x| x / l)
        };
        let x = norm(m[0]);
        let d = x[0] * m[1][0] + x[1] * m[1][1] + x[2] * m[1][2];
        let y = norm([m[1][0] - d * x[0], m[1][1] - d * x[1], m[1][2] - d * x[2]]);
        let z = [x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]];
        self.turned = [x, y, z];
    }
    pub fn zoom_by(&mut self, f: f32) {
        self.zoom = (self.zoom * f).clamp(0.05, 200.0);
    }
}

/// `w`×`h`, drawn `aa`×`aa` larger and scaled down for smooth edges.
pub fn render(mesh: &Mesh, view: &View, w: u32, h: u32, aa: u32) -> RgbaImage {
    let (w, h) = (w.max(1), h.max(1));
    if aa <= 1 {
        return draw(mesh, view, w, h);
    }
    let big = draw(mesh, view, w * aa, h * aa);
    image::imageops::resize(&big, w, h, FilterType::Triangle)
}

/// Vertex on the screen: x, y in pixels and 1/depth (larger = closer).
type Screen = [f32; 3];

/// Row-major 4×4.
type M4 = [[f32; 4]; 4];

fn mul(a: &M4, b: &M4) -> M4 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}

fn transposed(m: &M4) -> M4 {
    std::array::from_fn(|c| std::array::from_fn(|r| m[r][c]))
}

/// Distance of the camera: far enough for the bounding sphere to fit the field of view, so
/// nothing is ever behind it.
fn distance(mesh: &Mesh) -> f32 {
    mesh.radius / (FOV / 2.0).sin()
}

/// Pixels per unit at depth 1.
fn focal(view: &View, short: f32) -> f32 {
    view.zoom * 0.95 * (short / 2.0) / (FOV / 2.0).tan()
}

/// From the model to the camera (looking down −Z, Y up): centred, Z up turned to Y up, turned
/// as the view says, moved back by `distance`.
fn eye_matrix(mesh: &Mesh, view: &View) -> M4 {
    let [cx, cy, cz] = mesh.center;
    let centre = [[1.0, 0.0, 0.0, -cx], [0.0, 1.0, 0.0, -cy], [0.0, 0.0, 1.0, -cz], [0.0, 0.0, 0.0, 1.0]];
    let up = if mesh.z_up { [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, -1.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] } else { IDENTITY };
    let t = view.turned;
    let turned = [[t[0][0], t[0][1], t[0][2], 0.0], [t[1][0], t[1][1], t[1][2], 0.0], [t[2][0], t[2][1], t[2][2], 0.0], [0.0, 0.0, 0.0, 1.0]];
    let back = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -distance(mesh)], [0.0, 0.0, 0.0, 1.0]];
    [up, turned, back].iter().fold(centre, |m, next| mul(next, &m))
}

const IDENTITY: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

fn apply(m: &M4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3])
}

/// The camera for the graphics card (`render_gpu`), column-major as WGSL wants: model to clip
/// space (the same picture as `draw`, depth 0-1 across the bounding sphere) and model to camera.
pub fn camera(mesh: &Mesh, view: &View, w: u32, h: u32) -> (M4, M4) {
    let (wf, hf) = (w as f32, h as f32);
    let short = wf.min(hf);
    let f = focal(view, short);
    let d = distance(mesh);
    let (near, far) = ((d - mesh.radius * 1.01).max(d * 1e-3), d + mesh.radius * 1.01);
    let (a, b) = (far / (far - near), -far * near / (far - near));
    let projection = [
        [2.0 * f / wf, 0.0, -2.0 * view.pan[0] * short / wf, 0.0],
        [0.0, 2.0 * f / hf, 2.0 * view.pan[1] * short / hf, 0.0],
        [0.0, 0.0, -a, b],
        [0.0, 0.0, -1.0, 0.0],
    ];
    let eye = eye_matrix(mesh, view);
    (transposed(&mul(&projection, &eye)), transposed(&eye))
}

/// The ray through the point (`x`, `y`) of a picture `w` × `h` of the model in `view`, in the
/// model's coordinates: where the camera is, and the direction (for the measures, #141).
pub fn ray(mesh: &Mesh, view: &View, [w, h]: [f32; 2], [x, y]: [f32; 2]) -> ([f32; 3], [f32; 3]) {
    let short = w.min(h);
    let f = focal(view, short);
    let (cx, cy) = (w / 2.0 + view.pan[0] * short, h / 2.0 + view.pan[1] * short);
    let m = eye_matrix(mesh, view);
    // The eye matrix only turns and moves: its inverse turns back with the transpose.
    let back = |v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|c| (0..3).map(|r| m[r][c] * v[r]).sum()) };
    let origin = back([m[0][3], m[1][3], m[2][3]]).map(|v| -v);
    (origin, back([(x - cx) / f, -(y - cy) / f, -1.0]))
}

/// Where the model's point `p` is in a picture `w` × `h` in `view`; `None` behind the camera.
pub fn to_screen(mesh: &Mesh, view: &View, [w, h]: [f32; 2], p: [f32; 3]) -> Option<[f32; 2]> {
    let short = w.min(h);
    let f = focal(view, short);
    let (cx, cy) = (w / 2.0 + view.pan[0] * short, h / 2.0 + view.pan[1] * short);
    let [x, y, z] = apply(&eye_matrix(mesh, view), p);
    (z < 0.0).then(|| [cx + f * x / -z, cy - f * y / -z])
}

fn draw(mesh: &Mesh, view: &View, w: u32, h: u32) -> RgbaImage {
    let (wf, hf) = (w as f32, h as f32);
    let short = wf.min(hf);
    let focal = focal(view, short);
    let (cx, cy) = (wf / 2.0 + view.pan[0] * short, hf / 2.0 + view.pan[1] * short);
    let m = eye_matrix(mesh, view);
    let eye: Vec<[f32; 3]> = mesh.pos.par_iter().map(|&p| apply(&m, p)).collect();
    let screen: Vec<Screen> = eye.par_iter().map(|&[x, y, z]| [cx + focal * x / -z, cy - focal * y / -z, 1.0 / -z]).collect();
    let shade: Vec<[u8; 3]> = mesh.tris.par_iter().enumerate().map(|(i, t)| lit(&eye, *t, mesh.colors.get(i).copied().unwrap_or(DEFAULT))).collect();
    // Directions (normals) into the camera's space.
    let origin = apply(&m, [0.0; 3]);
    let turn = |v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|k| apply(&m, v)[k] - origin[k]) };

    let plane = view.cut.map(|c| c.plane(mesh));
    let face = plane.map(|p| Face::new(p, &turn, origin, [cx, cy, focal]));
    let bands = (h as usize).div_ceil(BAND);
    let mut binned: Vec<Vec<u32>> = vec![Vec::new(); bands];
    for (i, t) in mesh.tris.iter().enumerate() {
        // Wholly beyond the cutting plane: not drawn.
        if plane.is_some_and(|p| t.iter().all(|&v| cut_away(&p, mesh.pos[v as usize]))) {
            continue;
        }
        let ys = t.map(|v| screen[v as usize][1]);
        let (lo, hi) = (ys[0].min(ys[1]).min(ys[2]), ys[0].max(ys[1]).max(ys[2]));
        if hi < 0.0 || lo >= hf || !lo.is_finite() || !hi.is_finite() {
            continue;
        }
        let (first, last) = ((lo.max(0.0) as usize) / BAND, ((hi.min(hf - 1.0)) as usize) / BAND);
        for band in &mut binned[first..=last.min(bands - 1)] {
            band.push(i as u32);
        }
    }

    let mut img = RgbaImage::new(w, h);
    let row = w as usize * 4;
    img.par_chunks_mut(row * BAND).zip(binned.par_iter()).enumerate().for_each(|(b, (pixels, tris))| {
        let y0 = b * BAND;
        let rows = pixels.len() / row;
        background(pixels, row, y0, h as usize);
        let mut depth = vec![0.0f32; rows * w as usize];
        // With a section, which pixels see the inside of the model through the cut (`Face`).
        let mut inside = if plane.is_some() { vec![false; rows * w as usize] } else { Vec::new() };
        for &i in tris {
            let i = i as usize;
            let t = mesh.tris[i].map(|v| screen[v as usize]);
            // With a section, the parts beyond the plane left out pixel by pixel.
            let corners = mesh.tris[i].map(|v| mesh.pos[v as usize]);
            let keep = |k: [f32; 3]| plane.is_none_or(|p| !cut_away(&p, std::array::from_fn(|j| k[0] * corners[0][j] + k[1] * corners[1][j] + k[2] * corners[2][j])));
            if plane.is_some() {
                cover(&mut inside, w as usize, y0, rows, t, &keep);
            }
            let at = |k: [f32; 3], uv: &[[f32; 2]; 3]| [0, 1].map(|j| k[0] * uv[0][j] + k[1] * uv[1][j] + k[2] * uv[2][j]);
            let texture = mesh.tex.get(i).filter(|&&k| k > 0).and_then(|&k| mesh.textures.get(k as usize - 1)).map(|img| (img.as_ref(), mesh.uv[i]));
            if let Some(s) = crate::render_shine::Shiny::new(mesh, &eye, &turn, i) {
                fill(pixels, &mut depth, w as usize, y0, rows, t, |k| keep(k).then(|| s.pixel(k)));
            } else if let Some((img, uv)) = texture {
                // The base colour lit, tinted by the picture (both sRGB, as everywhere here).
                let c = shade[i];
                fill(pixels, &mut depth, w as usize, y0, rows, t, |k| {
                    let tx = texel(img, at(k, &uv));
                    keep(k).then(|| std::array::from_fn(|j| (c[j] as f32 * tx[j] as f32 / 255.0).min(255.0) as u8))
                });
            } else {
                fill(pixels, &mut depth, w as usize, y0, rows, t, |k| keep(k).then_some(shade[i]));
            }
        }
        if let Some(face) = &face {
            face.fill(pixels, &depth, &inside, w as usize, y0);
        }
    });
    img
}

/// The face of a section on the screen: the plane in the camera's space, and the camera.
struct Face {
    normal: [f32; 3],
    distance: f32,
    /// Centre x, y and focal length of the picture.
    camera: [f32; 3],
    color: [u8; 3],
}

impl Face {
    /// The model's `plane` seen by the camera that turns directions with `turn` and puts the
    /// model's origin at `origin`.
    fn new(plane: [f32; 4], turn: &impl Fn([f32; 3]) -> [f32; 3], origin: [f32; 3], camera: [f32; 3]) -> Face {
        let normal = turn([plane[0], plane[1], plane[2]]);
        let distance = plane[3] + normal[0] * origin[0] + normal[1] * origin[1] + normal[2] * origin[2];
        // Lit as the graphics card lights it: lighter than the model's faces.
        let l = crate::render_shine::LIGHT;
        let lit = 0.65 + 0.35 * (normal[0] * l[0] + normal[1] * l[1] + normal[2] * l[2]).abs();
        Face { normal, distance, camera, color: CAP.map(|c| (f32::from(c) * lit).min(255.0) as u8) }
    }

    /// Where the pixels that see the inside (`inside`) meet the plane, if nothing is closer: the
    /// cut's colour. `depth` as `fill` keeps it, 1/distance.
    fn fill(&self, pixels: &mut [u8], depth: &[f32], inside: &[bool], w: usize, y0: usize) {
        let [cx, cy, f] = self.camera;
        for (at, _) in inside.iter().enumerate().filter(|(_, i)| **i) {
            let (x, y) = ((at % w) as f32 + 0.5, (y0 + at / w) as f32 + 0.5);
            let dir = [(x - cx) / f, -(y - cy) / f, -1.0];
            let along = self.normal[0] * dir[0] + self.normal[1] * dir[1] + self.normal[2] * dir[2];
            let s = self.distance / along;
            if s > 0.0 && s.is_finite() && 1.0 / s > depth[at] {
                pixels[at * 4..at * 4 + 3].copy_from_slice(&self.color);
            }
        }
    }
}

/// Each pixel the triangle covers (where `keep` keeps it), flipped in `inside`, whatever is in
/// front: odd where the ray from the camera crosses the model's inside, so the cut there shows
/// it, whichever way the triangles turn (as the stencil of `render_gpu`).
fn cover(inside: &mut [bool], w: usize, y0: usize, rows: usize, t: [Screen; 3], keep: &impl Fn([f32; 3]) -> bool) {
    let [a, b, c] = t;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area == 0.0 || !area.is_finite() {
        return;
    }
    let x_lo = a[0].min(b[0]).min(c[0]).max(0.0).floor() as usize;
    let x_hi = (a[0].max(b[0]).max(c[0]).ceil().max(0.0) as usize).min(w);
    let y_lo = (a[1].min(b[1]).min(c[1]).floor().max(y0 as f32) as usize).max(y0);
    let y_hi = (a[1].max(b[1]).max(c[1]).ceil().max(0.0) as usize).min(y0 + rows);
    let edge = |p: Screen, q: Screen, x: f32, y: f32| ((q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])) / area;
    for y in y_lo..y_hi {
        for x in x_lo..x_hi {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (wa, wb, wc) = (edge(b, c, px, py), edge(c, a, px, py), edge(a, b, px, py));
            if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                continue;
            }
            let z = wa * a[2] + wb * b[2] + wc * c[2];
            if keep([wa * a[2] / z, wb * b[2] / z, wc * c[2] / z]) {
                let at = (y - y0) * w + x;
                inside[at] = !inside[at];
            }
        }
    }
}

/// Colour of a triangle: a soft ambient part plus light from the viewer's side, upper left;
/// both faces lit (files often have triangles turned the wrong way).
fn lit(eye: &[[f32; 3]], t: [u32; 3], color: [u8; 3]) -> [u8; 3] {
    let [a, b, c] = t.map(|v| eye[v as usize]);
    let (u, v) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let light = crate::render_shine::LIGHT;
    let d = if len > 0.0 { ((n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len).abs() } else { 0.0 };
    let i = 0.28 + 0.72 * d;
    color.map(|c| (c as f32 * i).min(255.0) as u8)
}

/// The texel at `uv` (repeating), nearest.
pub(crate) fn texel(img: &image::RgbaImage, uv: [f32; 2]) -> [u8; 3] {
    let (w, h) = (img.width(), img.height());
    let x = ((uv[0] - uv[0].floor()) * w as f32) as u32;
    let y = ((uv[1] - uv[1].floor()) * h as f32) as u32;
    let p = img.get_pixel(x.min(w - 1), y.min(h - 1));
    [p[0], p[1], p[2]]
}

/// Dark blue-grey, lighter at the top, as in CAD programs: top and bottom.
pub const BACKGROUND: [[u8; 3]; 2] = [[74, 82, 96], [30, 33, 40]];

fn background(pixels: &mut [u8], row: usize, y0: usize, h: usize) {
    let [top, bottom] = BACKGROUND;
    for (r, line) in pixels.chunks_exact_mut(row).enumerate() {
        let t = (y0 + r) as f32 / h as f32;
        let c = [0, 1, 2].map(|k| (top[k] as f32 * (1.0 - t) + bottom[k] as f32 * t) as u8);
        for p in line.as_chunks_mut::<4>().0 {
            p.copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

/// One triangle into a band starting at row `y0`, with `rows` rows: pixel centres inside the
/// three edges, kept where closer than what is already there, in the colour `paint` gives for
/// the corners' weights there (counting the perspective, so pictures follow it); `None` leaves
/// the pixel as it is (cut away by a section).
#[allow(clippy::too_many_arguments)]
fn fill(pixels: &mut [u8], depth: &mut [f32], w: usize, y0: usize, rows: usize, t: [Screen; 3], paint: impl Fn([f32; 3]) -> Option<[u8; 3]>) {
    let [a, b, c] = t;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area == 0.0 || !area.is_finite() {
        return;
    }
    let x_lo = a[0].min(b[0]).min(c[0]).max(0.0).floor() as usize;
    let x_hi = (a[0].max(b[0]).max(c[0]).ceil().max(0.0) as usize).min(w);
    let y_lo = (a[1].min(b[1]).min(c[1]).floor().max(y0 as f32) as usize).max(y0);
    let y_hi = (a[1].max(b[1]).max(c[1]).ceil().max(0.0) as usize).min(y0 + rows);
    let edge = |p: Screen, q: Screen, x: f32, y: f32| ((q[0] - p[0]) * (y - p[1]) - (q[1] - p[1]) * (x - p[0])) / area;
    for y in y_lo..y_hi {
        let py = y as f32 + 0.5;
        let line = (y - y0) * w;
        for x in x_lo..x_hi {
            let px = x as f32 + 0.5;
            let (wa, wb, wc) = (edge(b, c, px, py), edge(c, a, px, py), edge(a, b, px, py));
            if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                continue;
            }
            let z = wa * a[2] + wb * b[2] + wc * c[2];
            let at = line + x;
            if z > depth[at] {
                // Interpolated over 1/depth.
                let Some(c) = paint([wa * a[2] / z, wb * b[2] / z, wc * c[2] / z]) else { continue };
                depth[at] = z;
                pixels[at * 4..at * 4 + 3].copy_from_slice(&c);
            }
        }
    }
}

/// Where the file's X, Y and Z axes point on the screen in `view`: right, down (as egui) and
/// towards the viewer, for the axes in the corner of the viewer.
pub fn axes(view: &View, z_up: bool) -> [[f32; 3]; 3] {
    let t = view.turned;
    std::array::from_fn(|axis| {
        let mut v = [0.0f32; 3];
        v[axis] = 1.0;
        // Z up turned to Y up, as `eye_matrix` does.
        let v = if z_up { [v[0], v[2], -v[1]] } else { v };
        let e: [f32; 3] = std::array::from_fn(|r| t[r][0] * v[0] + t[r][1] * v[1] + t[r][2] * v[2]);
        [e[0], -e[1], e[2]]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model;

    fn cube() -> Mesh {
        let obj = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 2 3 4\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        model::parse(obj.as_bytes(), "c.obj").unwrap()
    }

    fn lit_pixels(img: &RgbaImage) -> usize {
        img.pixels().filter(|p| p[0] > 120).count()
    }

    #[test]
    fn the_model_is_drawn_in_the_middle_and_zoom_enlarges_it() {
        let m = cube();
        let img = render(&m, &View::default(), 200, 100, 1);
        assert_eq!(img.dimensions(), (200, 100));
        // The centre is the cube, the corners the background.
        assert!(img.get_pixel(100, 50)[0] > 120);
        assert!(img.get_pixel(1, 1)[0] < 120 && img.get_pixel(198, 98)[0] < 120);
        let mut near = View::default();
        near.zoom_by(1.3);
        assert!(lit_pixels(&render(&m, &near, 200, 100, 1)) * 10 > lit_pixels(&img) * 14);
        // Smoothed copy, same size.
        assert_eq!(render(&m, &View::default(), 60, 40, 2).dimensions(), (60, 40));
    }

    #[test]
    fn pan_moves_it_and_the_closest_face_wins() {
        let m = cube();
        let mut v = View::angles(0.0, 0.0);
        // From straight in front only the front face (+Z) is seen, evenly lit.
        let img = render(&m, &v, 100, 100, 1);
        assert_eq!(img.get_pixel(50, 50), img.get_pixel(45, 55));
        v.pan = [0.4, 0.0];
        let img = render(&m, &v, 100, 100, 1);
        assert!(img.get_pixel(15, 50)[0] < 120 && img.get_pixel(90, 50)[0] > 120);
        // Over the top and round again: no stop. Sideways after half a turn up still goes the
        // same way on the screen, and many small turns stay a rotation.
        let mut a = View::angles(0.0, 0.0);
        for _ in 0..1000 {
            a.turn(0.0, std::f32::consts::TAU / 1000.0);
        }
        let near = |a: &View, b: &View| (0..3).all(|r| (0..3).all(|c| (a.turned[r][c] - b.turned[r][c]).abs() < 1e-3));
        assert!(near(&a, &View::angles(0.0, 0.0)));
        for start in [View::angles(0.0, std::f32::consts::PI), View::angles(0.4, 2.0)] {
            let mut after = start;
            after.turn(0.3, 0.0);
            let back: M3 = std::array::from_fn(|r| std::array::from_fn(|c| start.turned[c][r]));
            let step = View { turned: mul3(&after.turned, &back), ..start };
            assert!(near(&step, &View::angles(0.3, 0.0)));
        }
    }

    #[test]
    fn the_card_sees_what_the_cpu_draws() {
        let mut m = cube();
        m.z_up = true;
        let v = View { zoom: 1.7, pan: [0.1, -0.2], ..View::angles(0.7, -0.3) };
        let (w, h) = (300.0, 200.0);
        let (clip, eye) = camera(&m, &v, w as u32, h as u32);
        let at = |m: &M4, p: [f32; 3]| -> [f32; 4] { std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r]) };
        let e = eye_matrix(&m, &v);
        for p in &m.pos {
            let c = at(&clip, *p);
            let (x, y, z) = (c[0] / c[3], c[1] / c[3], c[2] / c[3]);
            let q = apply(&e, *p);
            assert!((at(&eye, *p)[2] - q[2]).abs() < 1e-4);
            let f = focal(&v, h);
            let (sx, sy) = (w / 2.0 + v.pan[0] * h + f * q[0] / -q[2], h / 2.0 + v.pan[1] * h - f * q[1] / -q[2]);
            assert!(((x + 1.0) * w / 2.0 - sx).abs() < 1e-2 && ((1.0 - y) * h / 2.0 - sy).abs() < 1e-2);
            assert!((0.0..=1.0).contains(&z));
        }
    }

    #[test]
    fn a_section_shows_the_inside_in_the_colour_of_the_cut() {
        // A cube with its faces turned outwards, seen from above as the viewer starts.
        let obj = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 3 4 8 7\nf 2 3 7 6\nf 4 1 5 8\n";
        let m = model::parse(obj.as_bytes(), "c.obj").unwrap();
        let centre = |view: &View| *render(&m, view, 200, 200, 1).get_pixel(100, 100);
        let whole = centre(&View::default());
        assert_ne!([whole[0], whole[1], whole[2]], CAP);
        // The top half cut away: through the cut, the inside.
        let cut = View { cut: Some(Cut::middle(false)), ..View::default() };
        let red = |c: image::Rgba<u8>| c[0] > c[1].saturating_add(60) && c[0] > c[2].saturating_add(60);
        assert!(red(centre(&cut)), "{:?}", centre(&cut));
        // The other side kept: the top of the cube, as before.
        let flipped = View { cut: Some(Cut { flip: true, ..Cut::middle(false) }), ..View::default() };
        assert_eq!(centre(&flipped), whole);
        // Cut away whole: the background only.
        let none = View { cut: Some(Cut { at: 0.0, ..Cut::middle(false) }), ..View::default() };
        assert_eq!(lit_pixels(&render(&m, &none, 200, 200, 1)), 0);
        assert!(cut_away(&Cut::middle(false).plane(&m), [0.0, 0.5, 0.0]) && !cut_away(&Cut::middle(false).plane(&m), [0.0, -0.5, 0.0]));
        // Its faces turned inwards (a file written the wrong way round): the same pictures, the
        // outside never red.
        let inward = model::parse(obj.replace("f 1 4 3 2", "f 2 3 4 1").replace("f 5 6 7 8", "f 8 7 6 5").replace("f 1 2 6 5", "f 5 6 2 1").replace("f 3 4 8 7", "f 7 8 4 3").replace("f 2 3 7 6", "f 6 7 3 2").replace("f 4 1 5 8", "f 8 5 1 4").as_bytes(), "c.obj").unwrap();
        let on = |m: &Mesh, view: &View| *render(m, view, 200, 200, 1).get_pixel(100, 100);
        assert_eq!(on(&inward, &cut), centre(&cut));
        assert_eq!(on(&inward, &flipped), whole);
        assert!(!red(on(&inward, &View::default())));
    }

    #[test]
    fn the_axes_follow_the_view() {
        // From the front: X right, Y up (down is +y on screen), Z towards the viewer.
        let a = axes(&View::angles(0.0, 0.0), false);
        assert_eq!(a, [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]]);
        // A CAD model with Z up: Z points up on the screen, Y away from the viewer.
        let a = axes(&View::angles(0.0, 0.0), true);
        assert_eq!(a[2], [0.0, -1.0, 0.0]);
        assert_eq!(a[1], [0.0, 0.0, -1.0]);
    }
}
