//! Two versions of a CAD drawing compared (#143): one drawing with what is in both in grey, what
//! is only in the first in red and only in the second in blue, each on a layer of its own, so the
//! viewer's layers panel shows the first, the second or both and is the legend. Matched by
//! geometry: the same kind of thing at the same place within a tolerance (a moved block counts as
//! removed and added; colours and layers are not compared).

use std::collections::HashMap;

use crate::cad::{Drawing, Item};
use crate::cad_geom::P;
use crate::cad_layers::Layer;

/// Two drawings compared, for the viewer: their paths, as read, and the comparison with the places
/// where they differ (`changes`).
#[derive(Debug)]
pub struct Compared {
    pub paths: [String; 2],
    pub first: std::sync::Arc<Drawing>,
    pub second: std::sync::Arc<Drawing>,
    pub drawing: std::sync::Arc<Drawing>,
    pub changes: Vec<[f64; 4]>,
}

/// In both, only in the first, only in the second.
pub const COLORS: [[u8; 3]; 3] = [[120, 124, 132], [255, 90, 80], [70, 160, 255]];
/// Points closer than this, relative to the size of the two drawings together, are the same.
const TOLERANCE: f64 = 1e-6;

/// What a thing drawn is, for matching: its kind and its points on a grid of the tolerance,
/// a line the same whichever way it goes; a text with its words.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Line(Vec<[i64; 2]>, bool),
    Text([i64; 2], String),
    Image([[i64; 2]; 4]),
}

fn key(item: &Item, step: f64) -> Key {
    let q = |p: &P| [(p[0] / step).round() as i64, (p[1] / step).round() as i64];
    match item {
        Item::Line { pts, fill, .. } => {
            let mut a: Vec<[i64; 2]> = pts.iter().filter(|p| !p[0].is_nan()).map(q).collect();
            let mut b = a.clone();
            b.reverse();
            if b < a {
                a = b;
            }
            Key::Line(a, fill.is_some())
        }
        Item::Text { at, text, .. } => Key::Text(q(at), text.clone()),
        Item::Image { corners, .. } => Key::Image(corners.each_ref().map(q)),
    }
}

/// `first` and `second` as one drawing, things in both once; `names` of the three layers (in both,
/// only in the first, only in the second), in the language of the window.
pub fn compare(first: &Drawing, second: &Drawing, names: [String; 3]) -> Drawing {
    let span = [first.bounds_of_shown(), second.bounds_of_shown()].into_iter().flatten().map(|b| (b[2] - b[0]).max(b[3] - b[1])).fold(0.0, f64::max);
    let step = (span * TOLERANCE).max(f64::MIN_POSITIVE);
    let mut left: HashMap<Key, usize> = HashMap::new();
    for item in second.shown_items() {
        *left.entry(key(item, step)).or_insert(0) += 1;
    }
    let mut d = Drawing { unit: first.unit.or(second.unit), ..Default::default() };
    d.layers = names.into_iter().zip(COLORS).map(|(name, color)| Layer { name, color, shown: true }).collect();
    d.sets = vec![Vec::new(), vec![0], vec![1], vec![2]];
    let mut put = |item: &Item, which: u32| {
        d.items.push(recolored(item, COLORS[which as usize]));
        d.on.push(which + 1);
    };
    for item in first.shown_items() {
        match left.get_mut(&key(item, step)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                put(item, 0);
            }
            _ => put(item, 1),
        }
    }
    for item in second.shown_items() {
        if let Some(n) = left.get_mut(&key(item, step))
            && *n > 0
        {
            *n -= 1;
            put(item, 2);
        }
    }
    d
}

/// The thing in the colour of its side; a picture stays as it is.
fn recolored(item: &Item, color: [u8; 3]) -> Item {
    let mut item = item.clone();
    match &mut item {
        Item::Line { color: c, .. } | Item::Text { color: c, .. } => *c = color,
        Item::Image { .. } => {}
    }
    item
}

/// The places where the two drawings differ, from the top left: the boxes of the things only in
/// one of them, those close together (within 2% of the drawing) in one box.
pub fn changes(d: &Drawing) -> Vec<[f64; 4]> {
    let Some(all) = d.bounds() else { return Vec::new() };
    let near = (all[2] - all[0]).max(all[3] - all[1]) * 0.02;
    let mut boxes: Vec<[f64; 4]> = Vec::new();
    for (item, &set) in d.items.iter().zip(&d.on) {
        if set < 2 {
            continue;
        }
        let Some(b) = item_box(item) else { continue };
        match boxes.iter_mut().find(|o| o[0] - near <= b[2] && b[0] <= o[2] + near && o[1] - near <= b[3] && b[1] <= o[3] + near) {
            Some(o) => *o = [o[0].min(b[0]), o[1].min(b[1]), o[2].max(b[2]), o[3].max(b[3])],
            None => boxes.push(b),
        }
    }
    boxes.sort_by(|a, b| b[3].total_cmp(&a[3]).then(a[0].total_cmp(&b[0])));
    boxes
}

fn item_box(item: &Item) -> Option<[f64; 4]> {
    let pts: Vec<P> = match item {
        Item::Line { pts, .. } => pts.iter().copied().filter(|p| !p[0].is_nan()).collect(),
        Item::Text { at, height, angle, text, anchor, .. } => crate::cad_svg::text_box(*at, *height, *angle, text, *anchor).to_vec(),
        Item::Image { corners, .. } => corners.to_vec(),
    };
    let first = *pts.first()?;
    Some(pts.iter().fold([first[0], first[1], first[0], first[1]], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dxf(lines: &[[f64; 4]]) -> Drawing {
        let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
        for [x0, y0, x1, y1] in lines {
            s += &format!("0\nLINE\n8\n0\n10\n{x0}\n20\n{y0}\n30\n0\n11\n{x1}\n21\n{y1}\n31\n0\n");
        }
        s += "0\nENDSEC\n0\nEOF\n";
        crate::cad::read(s.as_bytes(), "x.dxf").unwrap()
    }

    fn names() -> [String; 3] {
        ["both".into(), "first".into(), "second".into()]
    }

    #[test]
    fn the_same_in_grey_the_rest_by_side() {
        // A square; the second has its right side moved and a line drawn the other way round.
        let a = dxf(&[[0.0, 0.0, 10.0, 0.0], [10.0, 0.0, 10.0, 10.0], [10.0, 10.0, 0.0, 10.0], [0.0, 10.0, 0.0, 0.0]]);
        let b = dxf(&[[10.0, 0.0, 0.0, 0.0], [12.0, 0.0, 12.0, 10.0], [10.0, 10.0, 0.0, 10.0], [0.0, 10.0, 0.0, 0.0]]);
        let d = compare(&a, &b, names());
        let count = |set: u32| d.on.iter().filter(|&&s| s == set).count();
        assert_eq!((count(1), count(2), count(3)), (3, 1, 1));
        assert!(d.items.iter().zip(&d.on).all(|(i, &s)| matches!(i, Item::Line { color, .. } if *color == COLORS[s as usize - 1])));
        // The first alone: the layer of the second off.
        assert_eq!(d.with_layers(&[true, true, false]).shown_items().count(), 4);
        // The side that moved, before and after, 2 apart: two places (beyond 2% of the drawing),
        // from the left.
        assert_eq!(changes(&d), vec![[10.0, 0.0, 10.0, 10.0], [12.0, 0.0, 12.0, 10.0]]);
        // Closer together, one place around both.
        let near = dxf(&[[0.0, 0.0, 10.0, 0.0], [10.1, 0.0, 10.1, 10.0], [10.0, 10.0, 0.0, 10.0], [0.0, 10.0, 0.0, 0.0]]);
        assert_eq!(changes(&compare(&a, &near, names())), vec![[10.0, 0.0, 10.1, 10.0]]);
        // The same drawing: no change.
        assert!(changes(&compare(&a, &a, names())).is_empty());
    }

    #[test]
    fn a_line_twice_is_matched_twice() {
        let a = dxf(&[[0.0, 0.0, 5.0, 5.0], [0.0, 0.0, 5.0, 5.0]]);
        let b = dxf(&[[0.0, 0.0, 5.0, 5.0]]);
        let d = compare(&a, &b, names());
        assert_eq!(d.on, vec![1, 2]);
    }
}
