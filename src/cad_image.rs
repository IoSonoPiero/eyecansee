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

/// The picture of `r` as a base64 PNG, if its file is found and can be read.
pub(crate) fn picture(doc: &CadDocument, r: &RasterImage, sibling: Sibling) -> Option<String> {
    let ObjectType::ImageDefinition(def) = doc.objects.get(&r.definition_handle?)? else { return None };
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
    #[test]
    fn the_unit_square_lands_on_the_corners() {
        // A picture from (10, 50) to (30, 40) in pixels (y down): top-left (10, 40).
        let m = super::matrix([(10.0, 50.0), (30.0, 50.0), (30.0, 40.0), (10.0, 40.0)]);
        assert_eq!(m, [20.0, 0.0, 0.0, 10.0, 10.0, 40.0]);
    }
}
