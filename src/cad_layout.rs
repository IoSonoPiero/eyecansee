//! The layouts of a CAD drawing (paper space): the sheet, what is drawn on it (title block,
//! border, notes) and its viewports, windows on the model space at their scale, cut to their
//! frame and without the layers frozen in them. Each becomes a `cad::Drawing` of its own.
// ponytail: viewports seen from above (no 3D views) and rectangular (no clipping outlines)

use acadrust::entities::{EntityType, Viewport};
use acadrust::objects::{Layout, ObjectType};
use acadrust::CadDocument;

use crate::cad::{CONTINUOUS, Context, Drawing, Stroke, WHITE, Walk};
use crate::cad_geom::Affine;

/// The sheet, a little lighter than the background.
const PAPER: [u8; 3] = [46, 54, 64];
/// The viewport of the sheet itself: not a window on the model.
const SHEET_VIEWPORT: i16 = 1;

/// Every layout with something on it, in tab order; a layout that cannot be drawn is left out.
pub(crate) fn sheets(doc: &CadDocument) -> Vec<(String, std::sync::Arc<Drawing>)> {
    let mut layouts: Vec<&Layout> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            ObjectType::Layout(l) if !l.name.eq_ignore_ascii_case("Model") => Some(l),
            _ => None,
        })
        .collect();
    layouts.sort_by_key(|l| l.tab_order);
    layouts.into_iter().filter_map(|l| Some((l.name.clone(), std::sync::Arc::new(sheet(doc, l)?)))).collect()
}

fn viewport(e: &EntityType) -> Option<&Viewport> {
    match e {
        EntityType::Viewport(v) => Some(v),
        _ => None,
    }
}

fn sheet(doc: &CadDocument, l: &Layout) -> Option<Drawing> {
    let name = &doc.block_records.iter().find(|b| b.handle == l.block_record)?.name;
    let entities: Vec<&EntityType> = doc.entities_in_block(name).collect();
    // A layout with only its own viewport is empty (Layout1 and Layout2 of most drawings).
    if entities.iter().all(|e| viewport(e).is_some_and(|v| v.id == SHEET_VIEWPORT)) {
        return None;
    }
    let mut d = Drawing::default();
    let mut walk = Walk::new(doc, &mut d);
    let (lo, hi) = (l.min_limits, l.max_limits);
    if hi.0 > lo.0 && hi.1 > lo.1 {
        let paper = vec![[lo.0, lo.1], [hi.0, lo.1], [hi.0, hi.1], [lo.0, hi.1], [lo.0, lo.1]];
        walk.line(paper, &Affine::IDENTITY, PAPER, Some(1.0), Stroke::default()).ok()?;
    }
    let top = Context { t: Affine::IDENTITY, layer: None, color: WHITE, style: CONTINUOUS, depth: 0 };
    for e in &entities {
        match viewport(e) {
            Some(v) if v.id != SHEET_VIEWPORT => window(doc, &mut walk, v).ok()?,
            Some(_) => {}
            None => walk.entity(e, &top).ok()?,
        }
    }
    drop(walk);
    (!d.items.is_empty()).then_some(d)
}

/// A viewport: its frame, then the model space placed at its scale and cut to it.
fn window(doc: &CadDocument, walk: &mut Walk, v: &Viewport) -> Result<(), String> {
    if v.status.is_off || v.width <= 0.0 || v.height <= 0.0 || v.view_height <= 0.0 {
        return Ok(());
    }
    let (c, half) = ([v.center.x, v.center.y], [v.width / 2.0, v.height / 2.0]);
    let frame = vec![[c[0] - half[0], c[1] - half[1]], [c[0] + half[0], c[1] - half[1]], [c[0] + half[0], c[1] + half[1]], [c[0] - half[0], c[1] + half[1]], [c[0] - half[0], c[1] - half[1]]];
    walk.line(frame, &Affine::IDENTITY, [150, 150, 150], None, Stroke::default())?;
    let k = v.height / v.view_height;
    let t = Affine::translate(c[0], c[1])
        .then_inner(&Affine::rotate(v.twist_angle))
        .then_inner(&Affine::scale(k, k))
        .then_inner(&Affine::translate(-(v.view_center.x + v.view_target.x), -(v.view_center.y + v.view_target.y)));
    walk.clip = Some([c[0] - half[0], c[1] - half[1], c[0] + half[0], c[1] + half[1]]);
    walk.frozen = doc.layers.iter().filter(|l| v.frozen_layers.contains(&l.handle)).map(|l| l.name.to_lowercase()).collect();
    let ctx = Context { t, layer: None, color: WHITE, style: CONTINUOUS, depth: 0 };
    let r = doc.model_space_entities().try_for_each(|e| walk.entity(e, &ctx));
    (walk.clip, walk.frozen) = (None, Vec::new());
    r
}

#[cfg(test)]
mod tests {
    /// A drawing with two layouts made by ezdxf (`notes/test-files/cad/genera_cad.py`): the sheet,
    /// its frame and title, and viewports showing the model at their scale.
    #[test]
    #[ignore]
    fn layouts_live() {
        let Ok(path) = std::env::var("EYECANSEE_LAYOUT") else { return };
        let d = crate::cad::read(&std::fs::read(&path).unwrap(), &path).unwrap();
        for (name, sheet) in &d.sheets {
            println!("{name}: {} items", sheet.items.len());
            if let Ok(out) = std::env::var("EYECANSEE_LAYOUT_OUT") {
                crate::decode::cad_render(sheet, 1600, None).unwrap().save(format!("{out}/{name}.png")).unwrap();
            }
        }
        assert!(!d.sheets.is_empty());
    }
}
