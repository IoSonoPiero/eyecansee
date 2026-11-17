//! The materials of OBJ models (`.mtl`) beyond their colour and its picture (#23): shininess and
//! highlights (`Ns`, `Ks`, the PBR extension's `Pr` and `Pm`), light of their own (`Ke`,
//! `map_Ke`) and normal maps (`norm`, or `map_Bump` when its picture is a normal map, as Blender
//! writes it), as a `Surface` lit the same way as glTF's.

use crate::model::{Builder, Sibling, Surface};

/// The surface of an OBJ material, `None` for a matte one (no highlight, no glow, no normal map):
/// the model then looks as before.
pub(crate) fn surface(mat: &tobj::Material, b: &mut Builder, sibling: Sibling) -> Option<Surface> {
    let param = |k: &str| mat.unknown_param.get(k).map(String::as_str);
    let number = |k: &str| param(k).and_then(|v| v.split_whitespace().next()?.parse::<f32>().ok()).filter(|v| v.is_finite());
    let bumps = param("norm").or(mat.normal_texture.as_deref()).map_or(0, |t| normal_map(b, file_name(t), sibling));
    let glow_map = param("map_Ke").map_or(0, |t| b.texture_file(file_name(t), sibling));
    let glow = match mat.emissive {
        Some(k) => k.map(|v| v.clamp(0.0, 1.0)),
        None if glow_map > 0 => [1.0; 3],
        None => [0.0; 3],
    };
    // Classic materials: a highlight when they have one (illumination 2 and up, a specular colour).
    let specular = mat.specular.is_some_and(|k| k.iter().any(|&v| v > 0.01)) && mat.illumination_model.is_none_or(|m| m >= 2);
    let (pr, pm) = (number("Pr"), number("Pm"));
    let roughness = pr.or_else(|| mat.shininess.filter(|_| specular).map(roughness_of)).unwrap_or(1.0).clamp(0.0, 1.0);
    let shiny = bumps > 0 || glow_map > 0 || glow.iter().any(|&v| v > 0.0) || pr.is_some() || pm.is_some() || roughness < 1.0;
    shiny.then(|| Surface { metallic: pm.unwrap_or(0.0).clamp(0.0, 1.0), roughness, glow, bumps, glow_map, ..Surface::default() })
}

/// Shininess (`Ns`, 0 to 1000) as roughness, as Blender reads it: 1 − √(Ns / 1000).
fn roughness_of(ns: f32) -> f32 {
    1.0 - (ns.clamp(0.0, 1000.0) / 1000.0).sqrt()
}

/// "-bm 0.5 wood_normal.png": the options before the name left out.
fn file_name(t: &str) -> &str {
    if t.starts_with('-') { t.rsplit(' ').next().unwrap_or(t) } else { t }
}

/// The picture `name` as a normal map (`Builder::texture`), 0 when missing or when it is a plain
/// height map in grey (classic `map_Bump`), which would light the model wrongly.
fn normal_map(b: &mut Builder, name: &str, sibling: Sibling) -> u16 {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let Some(img) = [name, base].into_iter().find_map(sibling).and_then(|d| image::load_from_memory(&d).ok()) else { return 0 };
    let img = img.to_rgba8();
    if is_normal_map(&img) { b.texture(img) } else { 0 }
}

/// Bluish on average, as normal maps are (pointing out of the surface is blue), and not grey.
fn is_normal_map(img: &image::RgbaImage) -> bool {
    let step = (img.width() as usize * img.height() as usize / 4096).max(1);
    let (mut sum, mut colour, mut n) = ([0.0f64; 3], 0.0, 0.0);
    for p in img.pixels().step_by(step) {
        let [r, g, b] = [p[0], p[1], p[2]].map(f64::from);
        sum = [sum[0] + r, sum[1] + g, sum[2] + b];
        colour += (r - g).abs() + (g - b).abs();
        n += 1.0;
    }
    if n == 0.0 {
        return false;
    }
    let [r, g, b] = sum.map(|v| v / n);
    b > 160.0 && b > r && b > g && colour / n > 20.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material(mtl: &str) -> tobj::Material {
        tobj::load_mtl_buf(&mut std::io::Cursor::new(mtl.as_bytes())).unwrap().0.remove(0)
    }

    fn png(c: [u8; 3]) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([c[0], c[1], c[2], 255])).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn shininess_glow_and_metal() {
        let none = |_: &str| None;
        let mut b = Builder::default();
        // A colour alone, or a highlight switched off by the illumination model: matte as before.
        assert_eq!(surface(&material("newmtl a\nKd 1 0 0\n"), &mut b, &none), None);
        assert_eq!(surface(&material("newmtl a\nKd 1 0 0\nKs 0.5 0.5 0.5\nNs 250\nillum 1\n"), &mut b, &none), None);
        let s = surface(&material("newmtl a\nKs 0.5 0.5 0.5\nNs 250\nillum 2\n"), &mut b, &none).unwrap();
        assert!((s.roughness - 0.5).abs() < 1e-6 && s.metallic == 0.0, "{s:?}");
        let s = surface(&material("newmtl a\nPr 0.2\nPm 1\nKe 0 0.5 0\n"), &mut b, &none).unwrap();
        assert_eq!((s.roughness, s.metallic, s.glow), (0.2, 1.0, [0.0, 0.5, 0.0]));
    }

    #[test]
    fn an_obj_takes_the_surface_of_its_material() {
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nusemtl lamp\nf 1 2 3\nusemtl wall\nf 2 4 3\n";
        let mtl = "newmtl lamp\nKd 0.8 0.8 0.8\nKe 17 12 4\nnewmtl wall\nKd 0.5 0.1 0.1\nKs 0 0 0\nNs 10\nillum 2\n";
        let files = |name: &str| (name == "m.mtl").then(|| mtl.as_bytes().to_vec());
        let m = crate::model::parse_with(obj.as_bytes(), "box.obj", &files).unwrap();
        // The lamp glows; the wall, without a highlight, stays matte.
        let lamp = m.surfaces.iter().find(|s| s.glow == [1.0, 1.0, 1.0]);
        assert!(lamp.is_some(), "{:?}", m.surfaces);
        assert_eq!(m.surfaces.iter().filter(|s| s.glow == [0.0; 3] && s.roughness < 1.0).count(), 0);
    }

    #[test]
    fn only_normal_maps_for_bumps() {
        let files = |name: &str| match name {
            "normal.png" => Some(png([128, 128, 255])),
            "height.png" => Some(png([128, 128, 128])),
            _ => None,
        };
        let mut b = Builder::default();
        let s = surface(&material("newmtl a\nmap_Bump -bm 1.0 normal.png\n"), &mut b, &files).unwrap();
        assert!(s.bumps > 0);
        // A grey height map is not one: matte.
        assert_eq!(surface(&material("newmtl a\nbump height.png\n"), &mut b, &files), None);
        assert!(surface(&material("newmtl a\nnorm textures/normal.png\n"), &mut b, &files).is_some_and(|s| s.bumps > 0));
    }
}
