//! Pictures attached to a CAD drawing (IMAGE entities): the file they name, looked for next to
//! the drawing (by its relative path, then by its bare name: drawings keep absolute paths of the
//! computer they were made on), made smaller and kept as PNG for the drawing's SVG.
// ponytail: brightness, contrast, fade and clipping outlines of the picture not applied

use acadrust::entities::RasterImage;
use acadrust::objects::ObjectType;
use acadrust::CadDocument;
use base64::Engine;

use crate::cad_geom::P;
use crate::model::Sibling;

/// Long side of a picture in the drawing: enough for the 4096-pixel view.
const SIDE: u32 = 2048;

/// No files next to the drawing (tests, *Convert and resize*).
pub(crate) fn none(_: &str) -> Option<Vec<u8>> {
    None
}

/// The pictures of a drawing made so far, by their definition: one placed several times (a logo
/// in a block) is read, decoded and made PNG once (audit 6.13).
pub(crate) type Pictures = std::collections::HashMap<acadrust::types::Handle, Option<String>>;

/// The picture of `r` as a base64 PNG, if its file is found and can be read; once for each
/// definition (`seen`).
pub(crate) fn picture(doc: &CadDocument, r: &RasterImage, sibling: Sibling, seen: &mut Pictures) -> Option<String> {
    let handle = r.definition_handle?;
    seen.entry(handle).or_insert_with(|| made(doc, handle, sibling)).clone()
}

fn made(doc: &CadDocument, handle: acadrust::types::Handle, sibling: Sibling) -> Option<String> {
    let ObjectType::ImageDefinition(def) = doc.objects.get(&handle)? else { return None };
    let name = def.file_name.trim();
    let bare = name.rsplit(['/', '\\']).next()?;
    let relative = !(name.starts_with(['/', '\\']) || name.contains(':'));
    let bytes = relative.then(|| sibling(name)).flatten().or_else(|| sibling(bare))?;
    let img = crate::decode::decode_full(&bytes, bare).ok()?;
    let img = if img.width().max(img.height()) > SIDE { img.thumbnail(SIDE, SIDE) } else { img };
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(png))
}

/// The SVG `matrix` that puts a 1×1 picture (y down) on its corners in the picture's pixels:
/// bottom-left, bottom-right, top-right, top-left.
pub(crate) fn matrix(c: [(f64, f64); 4]) -> [f64; 6] {
    let (tl, tr, bl) = (c[3], c[2], c[0]);
    [tr.0 - tl.0, tr.1 - tl.1, bl.0 - tl.0, bl.1 - tl.1, tl.0, tl.1]
}

/// Corners of the picture in drawing units, as `RasterImage::corners`.
pub(crate) fn corners(r: &RasterImage) -> [P; 4] {
    r.corners().map(|v| [v.x, v.y])
}

#[cfg(test)]
mod tests {
    /// Audit 6.13: a picture placed several times (a logo in a block) was read, decoded and made
    /// PNG again for each place. Three places of one picture: one read of its file.
    #[test]
    fn audit_6_13_picture_read_once() {
        use acadrust::entities::{EntityType, RasterImage};
        use acadrust::objects::{ImageDefinition, ObjectType};
        let mut doc = acadrust::CadDocument::new();
        let h = doc.allocate_handle();
        let mut def = ImageDefinition::new("logo.png");
        def.handle = h;
        doc.objects.insert(h, ObjectType::ImageDefinition(def));
        for i in 0..3 {
            let mut r = RasterImage::new("logo.png", acadrust::types::Vector3::new(f64::from(i) * 20.0, 0.0, 0.0), 10.0, 10.0);
            r.definition_handle = Some(h);
            doc.add_entity(EntityType::RasterImage(r)).unwrap();
        }
        let dxf = acadrust::DxfWriter::new(&doc).write_to_vec().unwrap();
        let mut png = Vec::new();
        image::RgbaImage::new(2, 2).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let asked = std::cell::Cell::new(0);
        let next = |n: &str| {
            let logo = n == "logo.png";
            asked.set(asked.get() + usize::from(logo));
            logo.then(|| png.clone())
        };
        let d = crate::cad::read_with(&dxf, "a.dxf", &next).unwrap();
        assert_eq!(d.items.iter().filter(|i| matches!(i, crate::cad::Item::Image { .. })).count(), 3);
        assert_eq!(asked.get(), 1, "reads of a picture placed three times");
    }

    #[test]
    fn the_unit_square_lands_on_the_corners() {
        // A picture from (10, 50) to (30, 40) in pixels (y down): top-left (10, 40).
        let m = super::matrix([(10.0, 50.0), (30.0, 50.0), (30.0, 40.0), (10.0, 40.0)]);
        assert_eq!(m, [20.0, 0.0, 0.0, 10.0, 10.0, 40.0]);
    }
}
