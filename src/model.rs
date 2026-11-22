//! 3D models made of triangles (STL, OBJ, PLY, glTF/GLB, 3MF in `model_3mf`), read into one
//! `Mesh` that `render3d` draws: in the grid, in the viewer and in screenshots.

use std::io::Cursor;

/// Extensions read as 3D models.
const MODEL: &[&str] = &["stl", "obj", "ply", "glb", "gltf", "3mf", "ifc", "fbx", "3ds", "off", "amf", "dae", "zae", "wrl", "wrz", "x3d", "x3dv", "x3dz", "x3dvz"];

/// Beyond this the file is refused instead of filling the memory (a 3D scan of a statue is ~10 M),
/// in the formats without a cap of their own (`model_limit`).
pub(crate) const MAX_TRIANGLES: usize = 50_000_000;
/// Nodes, instances and blocks placed while reading one model or drawing (`Builder::visit`,
/// `cad::Walk::block`). A part can be placed by many parents, each placed by many more: a file of
/// a few KB can ask for 10^32 placements. A million is far more than a scene places (a 3MF plate
/// places hundreds); an AMF reaches it in 3 s in a debug build.
pub(crate) const MAX_VISITS: usize = 1_000_000;
/// The error of a model whose points are too far apart to be drawn (`Builder::finish`).
pub(crate) const TOO_FAR: &str = "points too far apart to be drawn";
/// Normals are worked out (`model_smooth`) up to this many triangles: 36 bytes each.
const SMOOTH_MAX: usize = 20_000_000;

/// Also STEP, IGES and BREP (`occt`), read by the CAD engine.
pub fn is_model(name: &str) -> bool {
    ext(name).is_some_and(|e| MODEL.contains(&e.as_str())) || crate::occt::is_occt(name)
}

fn ext(name: &str) -> Option<String> {
    Some(name.rsplit_once('.')?.1.to_lowercase())
}

/// Triangles in one list, with the box around them for the camera.
#[derive(Debug)]
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    /// Every index is within `pos` and on a finite point: `Builder` drops the other triangles
    /// (`add_shiny`, `finish`), and a `Mesh` is made only there, so the renderers, the exports and
    /// the measures index `pos` without checking (audit 4.15 e, `triangles_point_inside`).
    pub tris: Vec<[u32; 3]>,
    /// One per triangle, or empty: all in the default colour.
    pub colors: Vec<[u8; 3]>,
    /// Z points up (3D printing, CAD: STL, 3MF), otherwise Y (glTF, OBJ, scans in PLY).
    pub z_up: bool,
    pub center: [f32; 3],
    pub radius: f32,
    /// Half the size of its box along X, Y and Z: how far a section's plane goes from the centre.
    pub half: [f32; 3],
    /// Per triangle, or empty without textures: where its corners are on its picture (u to the
    /// right, v down from the top, repeating outside 0-1), and which picture (0 = none, else
    /// the index in `textures` plus one).
    pub uv: Vec<[[f32; 2]; 3]>,
    pub tex: Vec<u16>,
    pub textures: Vec<std::sync::Arc<image::RgbaImage>>,
    /// Per triangle, or empty when no triangle has one: its surface (0 = matte, as without; else
    /// the index in `surfaces` plus one) and its corners' normals (zero: the face's own).
    pub surface: Vec<u16>,
    pub surfaces: Vec<Surface>,
    pub normals: Vec<[[f32; 3]; 3]>,
    /// The unit of its coordinates when the format or the file says it (`unit_of`), for the
    /// measures; `None` for the formats without one (STL, OBJ, PLY…).
    pub unit: Option<&'static str>,
    /// What was read differently from the file, for the viewer's status bar: Fluent ids, each once
    /// (`Builder::warn`).
    pub warnings: Vec<&'static str>,
}

/// How a surface takes the light beyond its colour, as glTF describes it (metallic-roughness).
/// The renderers light it the same way (`render3d::shade`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub metallic: f32,
    pub roughness: f32,
    /// How much of the occlusion picture darkens the ambient light (0 = none).
    pub occlusion: f32,
    /// Light of its own, added to the lit colour (sRGB values, 0-1).
    pub glow: [f32; 3],
    /// Pictures (`Builder::texture`, 0 = none) on the triangle's texture coordinates: bumps as a
    /// normal map, roughness (green) and metal (blue), occlusion (red), glow.
    pub bumps: u16,
    pub rough_metal: u16,
    pub occlusion_map: u16,
    pub glow_map: u16,
}

impl Default for Surface {
    fn default() -> Self {
        Surface { metallic: 0.0, roughness: 1.0, occlusion: 0.0, glow: [0.0; 3], bumps: 0, rough_metal: 0, occlusion_map: 0, glow_map: 0 }
    }
}

/// Pictures in one model, and their long side: enough for the screen, not too much memory.
pub(crate) const MAX_TEXTURES: usize = 64;
pub(crate) const TEXTURE_SIDE: u32 = 2048;

/// A triangle's texture: its corners on the picture and which picture (`Builder::texture`).
pub(crate) type Textured = ([[f32; 2]; 3], u16);

/// What a triangle has beyond its colour: its texture (the picture may be 0 when only the surface
/// has pictures), its surface (`Builder::surface`, 0 = matte) and its corners' normals.
pub(crate) type Shine = (Option<Textured>, u16, Option<[[f32; 3]; 3]>);

/// Collects the parts of a model (glTF primitives, OBJ groups, 3MF objects) into one mesh.
#[derive(Default)]
pub(crate) struct Builder {
    pos: Vec<[f32; 3]>,
    tris: Vec<[u32; 3]>,
    /// Per triangle, from the first triangle that has one; shorter (or empty) when the last ones
    /// have none: a scan of 28 million triangles does not take 80 bytes for each of them (#191).
    colors: Vec<Option<[u8; 3]>>,
    uv: Vec<Option<Textured>>,
    textures: Vec<std::sync::Arc<image::RgbaImage>>,
    /// Pictures already read by `texture_file`, by the name the model gives them.
    named: std::collections::HashMap<String, u16>,
    surfaces: Vec<Surface>,
    /// Per triangle, as `colors`: surface and normals (`Shine`).
    shine: Vec<(u16, Option<[[f32; 3]; 3]>)>,
    /// Nodes and instances placed so far (`visit`), points and triangles brought (`Limit::offered`).
    visits: usize,
    offered: usize,
    warnings: Vec<&'static str>,
    /// The cap of the format read (`model_limit`).
    limit: crate::model_limit::Limit,
}

impl Builder {
    /// For a format with a cap of its own (`model_limit`); `default` has the others'.
    pub(crate) fn up_to(limit: crate::model_limit::Limit) -> Self {
        Builder { limit, ..Default::default() }
    }

    pub(crate) fn limit(&self) -> crate::model_limit::Limit {
        self.limit
    }

    /// Something read differently from the file, said once in the viewer (`Mesh::warnings`).
    pub(crate) fn warn(&mut self, id: &'static str) {
        if !self.warnings.contains(&id) {
            self.warnings.push(id);
        }
    }

    /// Adds a part; its triangles index its own `pos`. Triangles outside `pos` are dropped
    /// (files are not trusted). `colors`: one per triangle, or `None` for the default.
    pub(crate) fn add(&mut self, pos: Vec<[f32; 3]>, tris: impl IntoIterator<Item = [u32; 3]>, color: impl FnMut(usize) -> Option<[u8; 3]>) -> Result<(), String> {
        self.add_textured(pos, tris, color, |_| None)
    }

    /// As `add`, with the texture of each triangle (`None` for none).
    pub(crate) fn add_textured(&mut self, pos: Vec<[f32; 3]>, tris: impl IntoIterator<Item = [u32; 3]>, color: impl FnMut(usize) -> Option<[u8; 3]>, mut uv: impl FnMut(usize) -> Option<Textured>) -> Result<(), String> {
        self.add_shiny(pos, tris, color, |i| (uv(i), 0, None))
    }

    /// As `add`, with texture, surface and normals of each triangle.
    pub(crate) fn add_shiny(&mut self, pos: Vec<[f32; 3]>, tris: impl IntoIterator<Item = [u32; 3]>, mut color: impl FnMut(usize) -> Option<[u8; 3]>, mut shine: impl FnMut(usize) -> Shine) -> Result<(), String> {
        let offered = self.limit.offered();
        self.offered = self.offered.saturating_add(pos.len());
        if self.offered > offered {
            return Err(format!("more than {offered} points and triangles"));
        }
        let base = self.pos.len() as u32;
        let n = pos.len() as u32;
        let tris = tris.into_iter();
        // As many as given at once (a list), not twice as many by growing.
        self.tris.reserve(tris.size_hint().0.min(self.limit.max + 1));
        for (i, t) in tris.enumerate() {
            if t.iter().all(|&v| v < n) {
                self.tris.push(t.map(|v| v + base));
                let (uv, surface, normals) = shine(i);
                let uv = uv.filter(|(c, k)| (*k > 0 || surface > 0) && c.iter().flatten().all(|v| v.is_finite()));
                let normals = normals.filter(|n| n.iter().flatten().all(|v| v.is_finite()));
                let k = self.tris.len() - 1;
                pad(&mut self.colors, k, color(i));
                pad(&mut self.uv, k, uv);
                pad(&mut self.shine, k, (surface, normals));
            }
            self.offered += 1;
            self.limit.check(self.tris.len())?;
            if self.offered > offered {
                return Err(format!("more than {offered} points and triangles"));
            }
        }
        self.pos.extend(pos);
        Ok(())
    }

    /// One more node or instance placed, counted before walking into it (`MAX_VISITS`).
    pub(crate) fn visit(&mut self) -> Result<(), String> {
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return Err(format!("more than {MAX_VISITS} parts placed"));
        }
        Ok(())
    }

    /// A picture for the triangles that follow, made smaller beyond `TEXTURE_SIDE`: its number
    /// for `add_textured`, or 0 when the model has too many already.
    pub(crate) fn texture(&mut self, img: image::RgbaImage) -> u16 {
        if self.textures.len() >= MAX_TEXTURES || img.width() == 0 || img.height() == 0 {
            return 0;
        }
        let img = if img.width().max(img.height()) > TEXTURE_SIDE {
            image::imageops::thumbnail(&img, TEXTURE_SIDE.min(img.width() * TEXTURE_SIDE / img.width().max(img.height())).max(1), (img.height() * TEXTURE_SIDE / img.width().max(img.height())).max(1))
        } else {
            img
        };
        self.textures.push(std::sync::Arc::new(img));
        self.textures.len() as u16
    }

    /// A surface for the triangles that follow: its number for `add_shiny`, the same for the same.
    pub(crate) fn surface(&mut self, s: Surface) -> u16 {
        match self.surfaces.iter().position(|o| *o == s) {
            Some(i) => i as u16 + 1,
            None if self.surfaces.len() < u16::MAX as usize - 1 => {
                self.surfaces.push(s);
                self.surfaces.len() as u16
            }
            None => 0,
        }
    }

    /// The picture in the file `name` next to the model, read once: as the model writes it, or
    /// just its file name (paths of the author's computer). As `texture`, 0 when unreadable.
    pub(crate) fn texture_file(&mut self, name: &str, sibling: Sibling) -> u16 {
        if let Some(&k) = self.named.get(name) {
            return k;
        }
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let k = [name, base].into_iter().find_map(sibling).and_then(|d| image::load_from_memory(&d).ok()).map_or(0, |i| self.texture(i.to_rgba8()));
        self.named.insert(name.to_owned(), k);
        k
    }

    /// The box around the points added so far (all of them, used or not).
    pub(crate) fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let finite = self.pos.iter().filter(|p| p.iter().all(|c| c.is_finite()));
        finite.fold(None, |acc, p| {
            let (mut lo, mut hi) = acc.unwrap_or((*p, *p));
            for k in 0..3 {
                (lo[k], hi[k]) = (lo[k].min(p[k]), hi[k].max(p[k]));
            }
            Some((lo, hi))
        })
    }

    pub(crate) fn finish(self, z_up: bool) -> Result<Mesh, String> {
        let Builder { pos, tris, colors, uv, textures, surfaces, shine, warnings, .. } = self;
        let finite = |p: &[f32; 3]| p.iter().all(|c| c.is_finite());
        let tris: Vec<(usize, [u32; 3])> = tris.into_iter().enumerate().filter(|(_, t)| t.iter().all(|&v| finite(&pos[v as usize]))).collect();
        if tris.is_empty() {
            return Err("no triangles in the file".into());
        }
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &(_, t) in &tris {
            for v in t {
                let p = pos[v as usize];
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        let center = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
        let half = [0, 1, 2].map(|k| (hi[k] - lo[k]) / 2.0);
        let radius = ((0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt() / 2.0).max(f32::MIN_POSITIVE);
        // Points so far apart (beyond about 10^19) that the box is endless: nothing could be drawn.
        if !(radius.is_finite() && center.iter().chain(&half).all(|v| v.is_finite())) {
            return Err(TOO_FAR.into());
        }
        let colors = if colors.iter().any(Option::is_some) { tris.iter().map(|&(i, _)| colors.get(i).copied().flatten().unwrap_or(DEFAULT)).collect() } else { Vec::new() };
        let textured = uv.iter().any(Option::is_some);
        let (uv, tex) = if textured { tris.iter().map(|&(i, _)| uv.get(i).copied().flatten().unwrap_or(([[0.0; 2]; 3], 0))).unzip() } else { (Vec::new(), Vec::new()) };
        let shine_of = |i: usize| shine.get(i).copied().unwrap_or((0, None));
        let textures = if textured { textures } else { Vec::new() };
        // Normals worked out where the file gives none, and those triangles matte (as before, smooth).
        // ponytail: not beyond SMOOTH_MAX triangles, where the normals would take gigabytes
        let mut surfaces = surfaces;
        let missing = tris.iter().any(|&(i, _)| shine_of(i).1.is_none());
        let smooth = if missing && tris.len() <= SMOOTH_MAX { crate::model_smooth::normals(&pos, &tris.iter().map(|&(_, t)| t).collect::<Vec<_>>()) } else { Vec::new() };
        let matte = if smooth.is_empty() { 0 } else { surfaces.iter().position(|s| *s == Surface::default()).unwrap_or_else(|| { surfaces.push(Surface::default()); surfaces.len() - 1 }) as u16 + 1 };
        let pick = |k: usize, i: usize| match shine_of(i) {
            (s, Some(n)) => (s, n),
            (s, None) => (if s > 0 { s } else { matte }, smooth.get(k).copied().unwrap_or_default()),
        };
        let shiny = matte > 0 || shine.iter().any(|s| s.0 > 0);
        let (surface, normals) = if shiny { tris.iter().enumerate().map(|(k, &(i, _))| pick(k, i)).unzip() } else { (Vec::new(), Vec::new()) };
        let surfaces = if shiny { surfaces } else { Vec::new() };
        Ok(Mesh { pos, tris: tris.into_iter().map(|(_, t)| t).collect(), colors, z_up, center, radius, half, uv, tex, textures, surface, surfaces, normals, unit: None, warnings })
    }
}

/// Puts `v` at `k` of a list kept per triangle (`Builder`): nothing while the list is empty and
/// `v` the default, else after the defaults for the triangles before.
fn pad<T: Default + PartialEq + Clone>(list: &mut Vec<T>, k: usize, v: T) {
    if list.is_empty() && v == T::default() {
        return;
    }
    list.resize(k, T::default());
    list.push(v);
}

/// The Fluent id that says warning `id` (`Mesh::warnings`) of a file made from the model, with its
/// `$name` (#179): Save as… says it on its page; Export as a 3D model… has its own message
/// (`model_view::export_notice`), the thumbnails say nothing.
pub fn saved_warning(id: &'static str) -> &'static str {
    match id {
        crate::model_gltf::NODE_AGAIN => crate::model_gltf::NODE_AGAIN_SAVED,
        other => other,
    }
}

/// The lines that say the `warnings` of the model `name` of a file made from it (`saved_warning`).
pub fn saved_lines(i18n: &crate::i18n::I18n, name: &str, warnings: &[&'static str]) -> Vec<String> {
    warnings.iter().map(|&id| i18n.t_args(saved_warning(id), &[("name", name)])).collect()
}

/// Light grey with a touch of blue, as in CAD viewers.
pub const DEFAULT: [u8; 3] = [200, 205, 214];

/// Reads a file next to the model by the name the model gives it (a glTF's `.bin`, an OBJ's
/// `.mtl`), from the same source; `None` when missing or refused (`sibling_path`).
pub type Sibling<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

#[cfg(test)]
pub fn parse(bytes: &[u8], name: &str) -> Result<Mesh, String> {
    parse_with(bytes, name, &|_| None)
}

/// Like `parse`, reading the files next to the model through `sibling`.
pub fn parse_with(bytes: &[u8], name: &str, sibling: Sibling) -> Result<Mesh, String> {
    let mesh = read_as(bytes, name, sibling);
    if bytes.len() >= GIVE_BACK_BYTES || mesh.as_ref().is_ok_and(|m| m.tris.len() >= GIVE_BACK_TRIANGLES) {
        give_back();
    }
    let mut mesh = mesh?;
    if mesh.unit.is_none() {
        mesh.unit = unit_of(name);
    }
    Ok(mesh)
}

/// After reading a file this large, or a model of this many triangles, the memory freed is given
/// back to the system (`give_back`): reading takes several times what the model keeps (#191).
const GIVE_BACK_BYTES: usize = 32 << 20;
const GIVE_BACK_TRIANGLES: usize = 1_000_000;

/// The memory freed by the reading given back to the system at once. glibc keeps it for the next
/// allocations, and a model of 28 million triangles left 6 GB to the process after a reading
/// of which 0.5 GB were still used (#191, measured on Linux). Elsewhere nothing: other allocators
/// give memory back by themselves [Unverified on Windows and musl].
fn give_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only goes through glibc's free lists; it takes no pointer.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// The unit a format fixes: glTF, IFC (as `ifc-lite` gives it), X3D and VRML in metres; STEP and
/// IGES in millimetres, as the CAD engine reads them. 3MF and AMF say it in the file (their readers).
fn unit_of(name: &str) -> Option<&'static str> {
    match ext(name)?.as_str() {
        "glb" | "gltf" | "ifc" | "wrl" | "wrz" | "x3d" | "x3dv" | "x3dz" | "x3dvz" => Some("m"),
        "step" | "stp" | "iges" | "igs" => Some("mm"),
        _ => None,
    }
}

/// A unit as 3MF and AMF name it, millimetres when not said.
pub(crate) fn named_unit(name: Option<&str>) -> Option<&'static str> {
    Some(match name.unwrap_or("millimeter").to_ascii_lowercase().as_str() {
        "micron" | "micrometer" => "µm",
        "millimeter" | "millimetre" => "mm",
        "centimeter" | "centimetre" => "cm",
        "meter" | "metre" => "m",
        "inch" => "in",
        "foot" | "feet" => "ft",
        _ => return None,
    })
}

fn read_as(bytes: &[u8], name: &str, sibling: Sibling) -> Result<Mesh, String> {
    // What the file unpacks (zips, gzip, compressed glTF data), in all.
    let unpacked = &crate::model_3mf::Unpacked::for_file(bytes.len());
    match ext(name).as_deref() {
        Some("stl") => stl(bytes),
        Some("obj") => obj(bytes, sibling),
        Some("ply") => crate::model_ply::read(bytes),
        Some("glb" | "gltf") => crate::model_gltf::read(bytes, sibling, unpacked),
        Some("3mf") => crate::model_3mf::read(bytes, unpacked),
        Some("ifc") => crate::model_ifc::read(bytes),
        Some("fbx") => crate::model_fbx::read(bytes, sibling),
        Some("3ds") => crate::model_3ds::read(bytes, sibling),
        Some("off") => crate::model_off::read(bytes),
        Some("amf") => crate::model_amf::read(bytes, unpacked),
        Some(e @ ("dae" | "zae")) => crate::model_dae::read(bytes, e == "zae", sibling, unpacked),
        Some("wrl" | "wrz" | "x3d" | "x3dv" | "x3dz" | "x3dvz") => crate::model_x3d::read(bytes, sibling, unpacked),
        _ if crate::occt::is_occt(name) => crate::occt::read(bytes, name),
        _ => Err("not a 3D model".into()),
    }
}

fn stl(bytes: &[u8]) -> Result<Mesh, String> {
    let n = bytes.get(80..84).map_or(0, |c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize);
    // Binary when the size matches its count, even if the header starts with "solid".
    if bytes.len() >= 84 && n.checked_mul(50).and_then(|b| b.checked_add(84)) == Some(bytes.len()) {
        return stl_binary(bytes, n);
    }
    // Counted before stl_io reads it all (`model_limit`).
    crate::model_limit::STL_TEXT.check(crate::model_limit::stl_text_triangles(bytes))?;
    // "solid" then a line break or a tab, without a name: stl_io takes it for binary.
    let named;
    let bytes = match bytes.strip_prefix(b"solid") {
        Some(rest) if rest.first().is_some_and(|c| *c != b' ') => {
            named = [&b"solid model"[..], rest].concat();
            &named[..]
        }
        _ => bytes,
    };
    let m = stl_io::read_stl(&mut Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut b = Builder::up_to(crate::model_limit::STL_TEXT);
    let tris = m.faces.iter().map(|f| f.vertices.map(|v| v as u32));
    b.add(m.vertices.iter().map(|v| v.0).collect(), tris, |_| None)?;
    b.finish(true)
}

/// Binary STL, with the colours of its triangles: Materialise's ("COLOR=" in the header, red in
/// the low bits, bit 15 off when the triangle has one) or VisCAM's (bit 15 on, blue in the low bits).
fn stl_binary(bytes: &[u8], n: usize) -> Result<Mesh, String> {
    crate::model_limit::STL.check(n)?;
    let materialise = bytes[..80].windows(6).any(|w| w == b"COLOR=");
    let f = |c: &[u8], i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
    let five = |v: u16| ((v & 31) as u32 * 255 / 31) as u8;
    let mut pos = Vec::with_capacity(n * 3);
    let mut colors = Vec::with_capacity(n);
    for c in bytes[84..].as_chunks::<50>().0.iter().take(n) {
        for k in 0..3 {
            pos.push([f(c, 12 + k * 12), f(c, 16 + k * 12), f(c, 20 + k * 12)]);
        }
        let a = u16::from_le_bytes([c[48], c[49]]);
        colors.push(match (materialise, a & 0x8000 != 0) {
            (true, false) => Some([five(a), five(a >> 5), five(a >> 10)]),
            (false, true) => Some([five(a >> 10), five(a >> 5), five(a)]),
            _ => None,
        });
    }
    let mut b = Builder::up_to(crate::model_limit::STL);
    b.add(pos, (0..n as u32).map(|i| [i * 3, i * 3 + 1, i * 3 + 2]), |i| colors[i])?;
    b.finish(true)
}

/// The file a model names next to itself, as a path of the same source: `rel` is relative to
/// the model's folder, may be percent-encoded (glTF URIs), and may not leave that folder.
pub fn sibling_path(model: &str, rel: &str) -> Option<String> {
    let rel = percent_decoded(rel)?.replace('\\', "/");
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let refused = outside(&rel) || parts.is_empty();
    (!refused).then(|| format!("{}{}", crate::source::parent(model), parts.join("/")))
}

/// Whether a name a model gives for a file next to it points out of the model's folder, which
/// `sibling_path` refuses: a folder above (`..`), a whole path, or an address (`http:`, `C:`).
pub(crate) fn leaves_folder(rel: &str) -> bool {
    percent_decoded(rel).is_some_and(|r| outside(&r.replace('\\', "/")))
}

/// `leaves_folder` for a name already decoded, with `/` between its folders.
fn outside(rel: &str) -> bool {
    rel.starts_with('/') || rel.contains(':') || rel.split('/').any(|p| p == "..")
}

pub(crate) fn percent_decoded(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex = [bytes.next()?, bytes.next()?];
            out.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).ok()
}

/// With the colours of its materials (`Kd` of the `.mtl` files it names) or of its vertices,
/// their pictures (`map_Kd`) and the rest of the material (`model_mtl`: highlight, glow, normal map).
fn obj(bytes: &[u8], sibling: Sibling) -> Result<Mesh, String> {
    // Counted before tobj reads it all (`model_limit`).
    crate::model_limit::OBJ.check(crate::model_limit::obj_triangles(bytes))?;
    let options = tobj::LoadOptions { triangulate: true, single_index: true, ignore_points: true, ignore_lines: true };
    let mtl = |p: &std::path::Path| match p.to_str().and_then(sibling) {
        Some(data) => tobj::load_mtl_buf(&mut Cursor::new(utf8(&data).as_bytes())),
        None => Err(tobj::LoadError::OpenFileFailed),
    };
    let (models, materials) = tobj::load_obj_buf(&mut Cursor::new(utf8(bytes).as_bytes()), &options, mtl).map_err(|e| e.to_string())?;
    let materials = materials.unwrap_or_default();
    let mut b = Builder::up_to(crate::model_limit::OBJ);
    let mut surfaces: std::collections::HashMap<usize, u16> = std::collections::HashMap::new();
    for m in models {
        let id = m.mesh.material_id;
        let surface = id.and_then(|i| Some((i, materials.get(i)?))).map_or(0, |(i, mat)| *surfaces.entry(i).or_insert_with(|| crate::model_mtl::surface(mat, &mut b, sibling).map_or(0, |s| b.surface(s))));
        let m = m.mesh;
        let pos = m.positions.as_chunks::<3>().0.to_vec();
        let tris: Vec<[u32; 3]> = m.indices.as_chunks::<3>().0.to_vec();
        let vc: Vec<[f32; 3]> = m.vertex_color.as_chunks::<3>().0.to_vec();
        let mat = m.material_id.and_then(|i| materials.get(i));
        // "map_Kd -s 1 1 1 wood.png": the options before the name.
        let map = mat.and_then(|mat| mat.diffuse_texture.as_deref()).map(|t| if t.starts_with('-') { t.rsplit(' ').next().unwrap_or(t) } else { t });
        let tex = map.map_or(0, |t| b.texture_file(t, sibling));
        let uv: Vec<[f32; 2]> = m.texcoords.as_chunks::<2>().0.iter().map(|&[u, v]| [u, 1.0 - v]).collect();
        // The picture alone, as 3D programs show it (Kd is often a leftover grey).
        let kd = if tex > 0 && !uv.is_empty() { Some([1.0; 3]) } else { mat.and_then(|mat| mat.diffuse) };
        let to_u8 = |c: [f32; 3]| c.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
        let colors = |i: usize| average(&vc, tris[i]).or(kd).map(to_u8);
        let corners = |i: usize| {
            let c = tris[i].map(|v| uv.get(v as usize).copied());
            c.iter().all(Option::is_some).then(|| (c.map(Option::unwrap_or_default), tex))
        };
        // The file's normals with a material that has a surface; matte, as before, worked out.
        let normals: Vec<[f32; 3]> = if surface > 0 { m.normals.as_chunks::<3>().0.to_vec() } else { Vec::new() };
        let shine = |i: usize| {
            let n = tris[i].map(|v| normals.get(v as usize).copied());
            (corners(i), surface, n.iter().all(Option::is_some).then(|| n.map(Option::unwrap_or_default)))
        };
        b.add_shiny(pos, tris.clone(), colors, shine)?;
    }
    b.finish(false)
}

/// The average colour of a triangle's vertices, if they have colours.
pub(crate) fn average(colors: &[[f32; 3]], t: [u32; 3]) -> Option<[f32; 3]> {
    let c = t.map(|v| colors.get(v as usize));
    let [Some(a), Some(b), Some(c)] = c else { return None };
    Some([0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0))
}

/// Text files of old programs: UTF-8, UTF-16 with its mark, or else Latin-1 (3ds Max's OBJ).
fn utf8(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return crate::model_3mf::text(bytes).into();
    }
    match std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)) {
        Ok(s) => s.into(),
        Err(_) => bytes.iter().map(|&b| b as char).collect::<String>().into(),
    }
}

/// Column-major, as glTF writes it.
pub(crate) type Mat4 = [[f32; 4]; 4];
pub(crate) const IDENTITY: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

pub(crate) fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..4).map(|k| a[k][r] * b[c][k]).sum()))
}

pub(crate) fn apply(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r])
}

/// A turn of `angle` radians about `axis` (right-handed).
pub(crate) fn rotation(axis: [f32; 3], angle: f32) -> Mat4 {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt().max(f32::MIN_POSITIVE);
    let [x, y, z] = axis.map(|v| v / len);
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    [[t * x * x + c, t * x * y + s * z, t * x * z - s * y, 0.0], [t * x * y - s * z, t * y * y + c, t * y * z + s * x, 0.0], [t * x * z + s * y, t * y * z - s * x, t * z * z + c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Every 3D model and CAD extension EyeCanSee reads is in Windows' "Open with" (the installer,
    /// and through it the MSIX): STEP, IGES and BREP were missing until 2026-11-02.
    #[test]
    fn open_with_on_windows() {
        let iss = include_str!("../tools/eyecansee.iss");
        for e in MODEL.iter().chain(crate::cad::CAD).chain(crate::occt::OCCT) {
            assert!(iss.contains(&format!("Software\\Classes\\.{e}\\OpenWithProgids")), "{e} missing from eyecansee.iss");
        }
    }

    const CUBE_OBJ: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\nf 1 2 3 4\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";

    #[test]
    fn obj_quads_become_triangles() {
        let m = parse(CUBE_OBJ.as_bytes(), "cube.OBJ").unwrap();
        assert_eq!((m.pos.len(), m.tris.len()), (8, 12));
        assert_eq!(m.center, [0.5; 3]);
        assert!((m.radius - 3f32.sqrt() / 2.0).abs() < 1e-6);
        assert!(m.colors.is_empty() && !m.z_up);
    }

    #[test]
    fn ascii_stl_and_ply_with_colours_and_strips() {
        let stl = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        let m = parse(stl.as_bytes(), "t.stl").unwrap();
        assert_eq!((m.tris.len(), m.z_up), (1, true));
        let ply = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nelement tristrips 1\nproperty list int int vertex_indices\nend_header\n0 0 0 255 0 0\n1 0 0 255 0 0\n1 1 0 255 0 0\n0 1 0 0 0 255\n4 0 1 2 3\n5 0 1 2 -1 3\n";
        let m = parse(ply.as_bytes(), "q.ply").unwrap();
        // The quad fan (2) and the strip 0 1 2 (1); "-1 3" alone is no triangle.
        assert_eq!(m.tris, vec![[0, 1, 2], [0, 2, 3], [0, 1, 2]]);
        assert_eq!(m.colors[0], [255, 0, 0]);
        assert_eq!(m.colors[1], [170, 0, 85]);
    }

    #[test]
    fn files_next_to_the_model() {
        assert_eq!(sibling_path("phone/DCIM/a.gltf", "a%20b.bin").as_deref(), Some("phone/DCIM/a b.bin"));
        assert_eq!(sibling_path("d/a.obj", "./tex\\m.mtl").as_deref(), Some("d/tex/m.mtl"));
        for bad in ["../x.bin", "a/../../x", "/etc/passwd", "C:/x.bin", "https://h/x.bin", "%2", ""] {
            assert_eq!(sibling_path("d/a.gltf", bad), None, "{bad}");
        }
        // An OBJ with its materials: each group in the colour of its material.
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl red\nf 1 2 3\no b\nusemtl blue\nf 1 3 2\n";
        let mtl = |name: &str| (name == "m.mtl").then(|| b"newmtl red\nKd 1 0 0\nnewmtl blue\nKd 0 0 1\n".to_vec());
        let m = parse_with(obj.as_bytes(), "x.obj", &mtl).unwrap();
        assert_eq!(m.colors, vec![[255, 0, 0], [0, 0, 255]]);
        // Without the .mtl it still opens, uncoloured.
        assert!(parse(obj.as_bytes(), "x.obj").unwrap().colors.is_empty());
        // Its picture (map_Kd, with options and a Windows path), upside down as OBJ counts V.
        let obj = "mtllib m.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl t\nf 1/1 2/2 3/3\n";
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 255, 0, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let next = |name: &str| match name {
            "m.mtl" => Some(b"newmtl t\nKd 0.5 0.5 0.5\nmap_Kd -s 1 1 1 .\\tex\\grass.png\n".to_vec()),
            "grass.png" => Some(png.clone()),
            _ => None,
        };
        let m = parse_with(obj.as_bytes(), "x.obj", &next).unwrap();
        assert_eq!((m.tex.clone(), m.textures.len(), m.colors.clone()), (vec![1], 1, vec![[255, 255, 255]]));
        assert_eq!(m.uv[0], [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0]]);
        // A .gltf whose triangle is in the .bin next to it.
        let gltf = r#"{"asset":{"version":"2.0"},"buffers":[{"uri":"tri%20data.bin","byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#;
        let bin: Vec<u8> = [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let next = |name: &str| (name == "tri%20data.bin").then(|| bin.clone());
        assert_eq!(parse_with(gltf.as_bytes(), "t.gltf", &next).unwrap().tris.len(), 1);
        assert!(parse(gltf.as_bytes(), "t.gltf").is_err());
    }

    #[test]
    fn a_point_cloud_becomes_dots() {
        let ply = "ply\nformat ascii 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n0 0 0 255 0 0\n1 0 0 0 0 255\n";
        let m = parse(ply.as_bytes(), "scan.ply").unwrap();
        assert_eq!((m.tris.len(), m.colors[0], m.colors[7]), (8, [255, 0, 0], [0, 0, 255]));
    }

    #[test]
    fn stl_without_a_name_and_with_colours() {
        let ascii = b"solid\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n";
        assert_eq!(parse(ascii, "a.stl").unwrap().tris.len(), 1);
        // A binary file whose header starts with "solid", one red triangle (VisCAM: bit 15, red high).
        let mut bin = b"solid but binary".to_vec();
        bin.resize(80, 0);
        bin.extend(1u32.to_le_bytes());
        bin.extend([0.0f32, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0].iter().flat_map(|v| v.to_le_bytes()));
        bin.extend((0x8000u16 | (31 << 10)).to_le_bytes());
        let m = parse(&bin, "b.stl").unwrap();
        assert_eq!((m.tris.len(), m.colors[0]), (1, [255, 0, 0]));
    }

    #[test]
    fn bad_indices_and_empty_files_refused() {
        // Audit 6.21: points so far apart that the box around them is endless.
        assert!(parse(b"v -1e38 0 0\nv 1e38 0 0\nv 0 1e38 0\nf 1 2 3\n", "far.obj").is_err());
        assert!(parse(b"v 0 0 0\nf 1 2 3\n", "x.obj").is_err());
        assert!(parse(b"", "x.stl").is_err());
        assert!(parse(b"{}", "x.gltf").is_err());
        assert!(is_model("a.GLB") && is_model("b.3mf") && !is_model("c.png"));
    }

    /// Audit 4.15 (e): the renderers (`render_gpu`, `render3d`), the exports and the measures index
    /// `Mesh::pos` with the triangles' numbers as they are. Each reader, given triangles that point
    /// outside its points (or at a point that is not a number), gives only triangles inside.
    #[test]
    fn triangles_point_inside() {
        let files: [(&str, &[u8]); 7] = [
            ("x.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nv nan 0 0\nf 1 2 3\nf 1 2 4\n"),
            ("x.ply", b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 2\nproperty list uchar int vertex_indices\nend_header\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n3 0 1 9\n"),
            ("x.off", b"OFF\n3 2 0\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n3 0 1 9\n"),
            ("x.amf", b"<amf><object id='1'><mesh><vertices><vertex><coordinates><x>0</x><y>0</y><z>0</z></coordinates></vertex><vertex><coordinates><x>1</x><y>0</y><z>0</z></coordinates></vertex><vertex><coordinates><x>0</x><y>1</y><z>0</z></coordinates></vertex></vertices><volume><triangle><v1>0</v1><v2>1</v2><v3>2</v3></triangle><triangle><v1>0</v1><v2>1</v2><v3>9</v3></triangle></volume></mesh></object></amf>"),
            ("x.x3d", b"<X3D><Scene><Shape><IndexedFaceSet coordIndex='0 1 2 -1 0 1 9 -1 0 1 16777217 -1'><Coordinate point='0 0 0 1 0 0 0 1 0'/></IndexedFaceSet></Shape></Scene></X3D>"),
            ("x.wrl", b"#VRML V2.0 utf8\nShape { geometry IndexedTriangleSet { index [ 0 1 2 0 1 9 ] coord Coordinate { point [ 0 0 0, 1 0 0, 0 1 0 ] } } }"),
            ("x.dae", b"<COLLADA><library_geometries><geometry id='g'><mesh><source id='p'><float_array id='pa' count='9'>0 0 0 1 0 0 0 1 0</float_array><technique_common><accessor source='#pa' count='3' stride='3'/></technique_common></source><vertices id='v'><input semantic='POSITION' source='#p'/></vertices><triangles count='2'><input semantic='VERTEX' source='#v' offset='0'/><p>0 1 2 0 1 9</p></triangles></mesh></geometry></library_geometries><library_visual_scenes><visual_scene id='s'><node><instance_geometry url='#g'/></node></visual_scene></library_visual_scenes><scene><instance_visual_scene url='#s'/></scene></COLLADA>"),
        ];
        for (name, bytes) in files {
            let m = parse(bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(m.tris.len(), 1, "{name}");
            assert!(m.tris.iter().flatten().all(|&v| (v as usize) < m.pos.len() && m.pos[v as usize].iter().all(|c| c.is_finite())), "{name}");
        }
    }

    /// Every model under `EYECANSEE_MODELS` read and drawn into `EYECANSEE_MODELS_OUT` (PNG), with times.
    #[test]
    #[ignore]
    fn models_live() {
        let (Ok(dir), Ok(out)) = (std::env::var("EYECANSEE_MODELS"), std::env::var("EYECANSEE_MODELS_OUT")) else { return };
        let mut failed = Vec::new();
        for sub in std::fs::read_dir(&dir).unwrap().flatten().filter(|s| s.path().is_dir()) {
            for f in std::fs::read_dir(sub.path()).unwrap().flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if !is_model(&name) {
                    continue;
                }
                let bytes = std::fs::read(f.path()).unwrap();
                let path = f.path().to_string_lossy().to_string();
                let next = |rel: &str| sibling_path(&path, rel).and_then(|p| std::fs::read(p).ok());
                let t = std::time::Instant::now();
                match parse_with(&bytes, &name, &next) {
                    Ok(m) => {
                        let read = t.elapsed();
                        let t = std::time::Instant::now();
                        let img = crate::render3d::render(&m, &Default::default(), 800, 600, 2);
                        println!("{name}: {} triangles, {} pictures, read {read:.0?}, drawn {:.0?}", m.tris.len(), m.textures.len(), t.elapsed());
                        img.save(format!("{out}/{name}.png")).unwrap();
                    }
                    Err(e) => {
                        println!("{name}: ERROR {e}");
                        failed.push(name);
                    }
                }
            }
        }
        println!("failed: {failed:?}");
    }

    #[test]
    fn gltf_matrices() {
        let t: Mat4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [5.0, 0.0, 0.0, 1.0]];
        let s: Mat4 = [[2.0, 0.0, 0.0, 0.0], [0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 2.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        // Scaled first, then moved.
        assert_eq!(apply(&mul(&t, &s), [1.0, 1.0, 1.0]), [7.0, 2.0, 2.0]);
    }

    /// Audit 6.6: `Builder` limits the triangles, not the points. A .gltf of 2 KB with a mesh of
    /// 100 points and one triangle that points outside them (dropped), placed 10^7 times by seven
    /// levels of nodes with the next one as child ten times: 12 GB of points and no triangle.
    /// It must be refused before the memory is full. `AUDIT_N` = fewer levels, to measure how the
    /// memory grows. Run alone, with a ceiling on the memory.
    #[test]
    fn audit_6_6_builder_points() {
        let levels: u32 = std::env::var("AUDIT_N").ok().and_then(|v| v.parse().ok()).unwrap_or(7);
        // 1200 bytes of zeros (100 points) and 12 of 0xFF (three indices of 4 294 967 295), in base64.
        let data = format!("{}{}", "A".repeat(1600), "/".repeat(16));
        let nodes: Vec<String> = (1..=levels).map(|k| format!(r#"{{"children":[{}]}}"#, vec![k.to_string(); 10].join(","))).collect();
        let gltf = format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":1212,"uri":"data:application/octet-stream;base64,{data}"}}],"bufferViews":[{{"buffer":0,"byteLength":1200}},{{"buffer":0,"byteOffset":1200,"byteLength":12}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":100,"type":"VEC3","min":[0,0,0],"max":[0,0,0]}},{{"bufferView":1,"componentType":5125,"count":3,"type":"SCALAR"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"nodes":[{},{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#,
            nodes.join(",")
        );
        println!("glTF of {} bytes, 10^{levels} placements of 100 points", gltf.len());
        let t = std::time::Instant::now();
        let r = parse(gltf.as_bytes(), "a.gltf");
        println!("ended in {:.2?}: {:?}", t.elapsed(), r.as_ref().map(|m| (m.pos.len(), m.tris.len())));
        assert!(r.is_err());
    }
}
