//! The light on triangles with a surface (`model::Surface`), pixel by pixel on the CPU: the
//! corners' normals, bumps, roughness, metal, occlusion and glow. The same formula as the
//! graphics card's shader (`render_gpu::SHADER`): change both together.
// ponytail: one light and no surroundings to reflect: metals keep their colour lit as the rest

use image::RgbaImage;

use crate::model::{DEFAULT, Mesh, Surface};

type V3 = [f32; 3];

/// Towards the light (upper left, on the viewer's side) and towards the viewer.
pub(crate) const LIGHT: V3 = [-0.3, 0.45, 0.84];
const VIEWER: V3 = [0.0, 0.0, 1.0];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: V3, k: f32) -> V3 {
    a.map(|v| v * k)
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Of length 1, or zero when it has none.
fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 { scale(a, 1.0 / l) } else { [0.0; 3] }
}

/// One triangle ready to be lit, in the camera's space.
pub(crate) struct Shiny<'a> {
    base: V3,
    uv: [[f32; 2]; 3],
    normals: Option<[V3; 3]>,
    /// The face's normal, towards the viewer.
    face: V3,
    /// Directions of the picture's u and v on the face, for the bumps.
    tangent: Option<(V3, V3)>,
    surface: &'a Surface,
    /// Colour, bumps, roughness and metal, occlusion, glow.
    pics: [Option<&'a RgbaImage>; 5],
}

impl<'a> Shiny<'a> {
    /// Triangle `i`, its corners in the camera's space (`eye`), `turn` taking a direction there;
    /// `None` without a surface.
    pub(crate) fn new(mesh: &'a Mesh, eye: &[V3], turn: &dyn Fn(V3) -> V3, i: usize) -> Option<Self> {
        let k = *mesh.surface.get(i)? as usize;
        let surface = mesh.surfaces.get(k.checked_sub(1)?)?;
        let [a, b, c] = mesh.tris[i].map(|v| eye[v as usize]);
        let (e1, e2) = (sub(b, a), sub(c, a));
        let face = unit(cross(e1, e2));
        // The camera is at the origin.
        let face = if dot(face, a) > 0.0 { scale(face, -1.0) } else { face };
        let normals = mesh.normals.get(i).filter(|n| n.iter().any(|v| *v != [0.0; 3])).map(|n| n.map(|v| unit(turn(v))));
        let uv = mesh.uv.get(i).copied().unwrap_or_default();
        let pic = |k: u16| mesh.textures.get((k as usize).checked_sub(1)?).map(|p| p.as_ref());
        let (d1, d2) = ([uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]], [uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]]);
        let det = d1[0] * d2[1] - d2[0] * d1[1];
        let tangent = (det.abs() > 1e-12 && pic(surface.bumps).is_some()).then(|| (scale(sub(scale(e1, d2[1]), scale(e2, d1[1])), 1.0 / det), scale(sub(scale(e2, d1[0]), scale(e1, d2[0])), 1.0 / det)));
        let base = mesh.colors.get(i).copied().unwrap_or(DEFAULT).map(|c| c as f32 / 255.0);
        let pics = [pic(mesh.tex.get(i).copied().unwrap_or(0)), pic(surface.bumps), pic(surface.rough_metal), pic(surface.occlusion_map), pic(surface.glow_map)];
        Some(Shiny { base, uv, normals, face, tangent, surface, pics })
    }

    /// The colour where the corners weigh `k` (summing to 1, perspective already counted).
    pub(crate) fn pixel(&self, k: [f32; 3]) -> [u8; 3] {
        let uv = [0, 1].map(|j| k[0] * self.uv[0][j] + k[1] * self.uv[1][j] + k[2] * self.uv[2][j]);
        let [color, bumps, rough_metal, occlusion, glow] = self.pics.map(|p| p.map_or([1.0; 3], |img| crate::render3d::texel(img, uv).map(|c| c as f32 / 255.0)));
        let mut n = match self.normals {
            Some(c) => unit(std::array::from_fn(|j| k[0] * c[0][j] + k[1] * c[1][j] + k[2] * c[2][j])),
            None => self.face,
        };
        if dot(n, self.face) < 0.0 {
            n = scale(n, -1.0);
        }
        if let Some((t, b)) = self.tangent {
            let (t, b) = (unit(sub(t, scale(n, dot(n, t)))), unit(sub(b, scale(n, dot(n, b)))));
            let m = bumps.map(|v| v * 2.0 - 1.0);
            // Green is up on the picture, where v gets smaller.
            n = unit(std::array::from_fn(|j| t[j] * m[0] - b[j] * m[1] + n[j] * m[2]));
        }
        let base = std::array::from_fn(|j| self.base[j] * color[j]);
        light(base, n, self.surface, rough_metal, occlusion[0], glow)
    }
}

/// Ambient and diffuse light as for every model, plus a highlight (GGX) as rough and as metallic
/// as the surface says, the occlusion darkening the ambient part, and the glow. On sRGB values,
/// as the rest of `render3d`.
fn light(base: V3, n: V3, s: &Surface, rough_metal: V3, occlusion: f32, glow: V3) -> [u8; 3] {
    let l = unit(LIGHT);
    let d = dot(n, l).abs();
    let rough = (s.roughness * rough_metal[1]).clamp(0.05, 1.0);
    let metal = (s.metallic * rough_metal[2]).clamp(0.0, 1.0);
    let ambient = 0.28 * (1.0 + s.occlusion * (occlusion - 1.0));
    let nh = dot(n, unit([l[0] + VIEWER[0], l[1] + VIEWER[1], l[2] + VIEWER[2]])).max(0.0);
    let a2 = rough.powi(4);
    let den = nh * nh * (a2 - 1.0) + 1.0;
    let spread = (a2 / (std::f32::consts::PI * den * den)).min(32.0);
    std::array::from_fn(|j| {
        let f0 = 0.04 + (base[j] - 0.04) * metal;
        ((base[j] * (ambient + 0.72 * d) + f0 * spread * 0.25 * d + s.glow[j] * glow[j]) * 255.0).clamp(0.0, 255.0) as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth_shines_rough_does_not_and_glow_adds() {
        let n = unit(LIGHT);
        let grey = [0.5; 3];
        let matte = light(grey, n, &Surface::default(), [1.0; 3], 1.0, [1.0; 3]);
        // Rough: almost the light of a surface without material (0.5 × (0.28 + 0.72)).
        assert!(matte[0].abs_diff(127) <= 2, "{matte:?}");
        let smooth = Surface { roughness: 0.2, ..Surface::default() };
        let half = unit([n[0], n[1], n[2] + 1.0]);
        assert!(light(grey, half, &smooth, [1.0; 3], 1.0, [1.0; 3])[0] > 190);
        let lamp = Surface { glow: [1.0, 0.0, 0.0], ..Surface::default() };
        assert_eq!(light([0.0; 3], n, &lamp, [1.0; 3], 1.0, [1.0; 3]), [255, 0, 0]);
    }
}
