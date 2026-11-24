//! The font of the field where a text or a bubble's letters are typed on the image (#310): the
//! text's own, as `edit::render` draws it, so the letters keep their look and their width while
//! typed (until 0.77.0 the field showed them in the interface's font, thin, and a wide font was
//! seen only once typing ended). The face is the one the drawing takes (the same query of the
//! system's fonts: the family, then the sans-serif one), given to egui once, under a family of
//! its own with the interface's fonts after it for the characters it lacks.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, FontFamily};
use resvg::usvg::fontdb;

/// The faces egui could not read (a file it does not take): the field keeps the interface's font
/// for them. A face egui cannot read would close the program when its fonts are made again.
fn refused() -> &'static Mutex<HashSet<String>> {
    static REFUSED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    REFUSED.get_or_init(Default::default)
}

/// The family of the field for letters in `font` (`None`: the drawing's sans-serif one). The
/// interface's font until egui has the face, from the next frame, and for a face it cannot read.
pub(super) fn family(ctx: &egui::Context, font: Option<&str>) -> FontFamily {
    let options = crate::decode::usvg_options();
    let db = &options.fontdb;
    let families = [font.map_or(fontdb::Family::SansSerif, fontdb::Family::Name), fontdb::Family::SansSerif];
    let query = fontdb::Query { families: &families, weight: fontdb::Weight::NORMAL, stretch: fontdb::Stretch::Normal, style: fontdb::Style::Normal };
    let Some(id) = db.query(&query) else { return FontFamily::Proportional };
    let Some(face) = db.face(id) else { return FontFamily::Proportional };
    let name = format!("edit-letters:{}:{}", face.post_script_name, face.index);
    let family = FontFamily::Name(name.as_str().into());
    if ctx.fonts(|f| f.definitions().families.contains_key(&family)) {
        return family;
    }
    if refused().lock().is_ok_and(|r| r.contains(&name)) {
        return FontFamily::Proportional;
    }
    let Some((bytes, index)) = db.with_face_data(id, |data, index| (data.to_vec(), index)) else { return FontFamily::Proportional };
    if skrifa::FontRef::from_index(&bytes, index).is_err() {
        log!("edit: the font {name:?} cannot be shown while typing");
        if let Ok(mut r) = refused().lock() {
            r.insert(name);
        }
        return FontFamily::Proportional;
    }
    let mut fonts = ctx.fonts(|f| f.definitions().clone());
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    fonts.font_data.insert(name.clone(), Arc::new(data));
    let mut chain = vec![name];
    chain.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(family, chain);
    ctx.set_fonts(fonts);
    ctx.request_repaint();
    FontFamily::Proportional
}

#[cfg(test)]
mod tests {
    use eframe::egui::{self, FontFamily};

    /// The face of the default letters (`None`: the drawing's sans-serif one) and of a font of
    /// the list is given to egui once, under a family of its own, with the interface's fonts after
    /// it; the interface's font only in the frame it is given.
    #[test]
    fn the_texts_face_in_a_family_of_its_own() {
        let first = crate::decode::font_families().first().cloned();
        for font in [None, first] {
            let ctx = egui::Context::default();
            let mut got = Vec::new();
            for _ in 0..3 {
                ctx.run_ui(Default::default(), |ui| got.push(super::family(ui.ctx(), font.as_deref()))).textures_delta.clear();
            }
            if got[0] == FontFamily::Proportional && got[2] == FontFamily::Proportional {
                eprintln!("no font for {font:?} on this system");
                continue;
            }
            assert_eq!((&got[0], got[1] == got[2]), (&FontFamily::Proportional, true), "{font:?}: {got:?}");
            let chain = ctx.fonts(|f| f.definitions().families[&got[2]].clone());
            let ui_fonts = ctx.fonts(|f| f.definitions().families[&FontFamily::Proportional].clone());
            assert!(chain[0].starts_with("edit-letters:") && chain[1..] == ui_fonts[..], "{font:?}: {chain:?}");
        }
    }
}
