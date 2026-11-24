//! How wide a text's letters are, measured with its font (#174): until 0.77.0 the box of a text
//! (its frame, the area that takes a click, its turn handle, the line of On a curve) was an
//! estimate from the number of characters (0.6 of the height each), and never looked at the
//! font: capitals and wide fonts came out of it, small letters left it half empty. The letters
//! are laid out as `render::text` draws them (its SVG alone), once per text and font: the
//! measure is kept for the next frames.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use resvg::usvg;

use super::doc::Pt;

/// The size the letters are measured at: the measure goes with the size, so one serves them all.
const AT: f32 = 100.0;
/// The measures kept at most: past them, all go (a text typed adds one a letter).
const MOST: usize = 512;

/// The text and its font, hashed: looked up every frame, without copying them.
type Key = u64;

/// Where the letters begin and end, left and right of their anchor.
type Extent = Option<(f32, f32)>;

fn kept() -> &'static Mutex<HashMap<Key, Extent>> {
    static KEPT: OnceLock<Mutex<HashMap<Key, Extent>>> = OnceLock::new();
    KEPT.get_or_init(Default::default)
}

/// Where the letters of `text` in `font` begin and end, from their anchor (`render::text`
/// centres the lines on it; a slanted font's letters may lean past it on one side), for letters of
/// height 1; `None` for a text with no letters to draw, or no font to draw them.
pub fn extent(text: &str, font: Option<&str>) -> Extent {
    if text.trim().is_empty() {
        return None;
    }
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        (text, font).hash(&mut h);
        h.finish()
    };
    if let Some(h) = kept().lock().ok().and_then(|k| k.get(&key).copied()) {
        return h;
    }
    let h = measured(text, font);
    if let Ok(mut k) = kept().lock() {
        if k.len() >= MOST {
            k.clear();
        }
        k.insert(key, h);
    }
    h
}

/// The letters laid out in an SVG of their own, with the system's fonts.
fn measured(text: &str, font: Option<&str>) -> Extent {
    let mut alone = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">"#);
    super::render::text(&mut alone, (Pt::new(0.0, 0.0), AT, 0.0), text, font, "").ok()?;
    alone.push_str("</svg>");
    let tree = usvg::Tree::from_str(&alone, &crate::decode::usvg_options()).ok()?;
    // The outlines of the letters, not the boxes of their advances (wider in a font with fixed
    // widths: an "i" takes as much room as an "M").
    let (l, r) = tree.root().children().iter().filter_map(|n| if let usvg::Node::Text(t) = n { Some(t.flattened().abs_bounding_box()) } else { None }).fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), b| (l.min(b.left()), r.max(b.right())));
    (r > l).then_some((l / AT, r / AT))
}
