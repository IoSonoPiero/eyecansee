//! The layers of a CAD drawing (DWG, DXF): which ones each thing drawn is on, and which are shown.
//! A thing inside blocks is on its own layer and on those of the blocks around it, so a block on
//! a hidden layer is hidden whole, as the file's frozen layers hide it and as LibreCAD does. At
//! first the layers are shown as the file has them (off and frozen ones hidden); the viewer turns
//! them on and off (`cad_layers_panel`) without changing the file.

use std::collections::HashMap;

use acadrust::CadDocument;

use crate::cad::{Drawing, Item, visible};
use crate::cad_geom::P;

/// A layer of the drawing: its name, colour and whether it is shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub color: [u8; 3],
    pub shown: bool,
}

/// The layers of `doc`, in the order of its layer table, then those used by its entities but
/// missing from the table (a DXF may have no table); and their numbers by lower-case name. The
/// same list for the model and every layout, so one choice serves them all.
pub(crate) fn table(doc: &CadDocument) -> (Vec<Layer>, HashMap<String, u32>) {
    let mut layers: Vec<Layer> = doc
        .layers
        .iter()
        .map(|l| {
            let color = l.color.rgb().map_or(crate::cad::WHITE, |(r, g, b)| [r, g, b]);
            Layer { name: l.name.clone(), color: visible(color), shown: l.is_visible() }
        })
        .collect();
    let mut index: HashMap<String, u32> = HashMap::new();
    for (i, l) in layers.iter().enumerate() {
        index.entry(l.name.to_lowercase()).or_insert(i as u32);
    }
    for e in doc.entities() {
        let name = &e.common().layer;
        if let std::collections::hash_map::Entry::Vacant(v) = index.entry(name.to_lowercase()) {
            v.insert(layers.len() as u32);
            layers.push(Layer { name: name.clone(), color: crate::cad::WHITE, shown: true });
        }
    }
    (layers, index)
}

/// The sets of layers the things drawn are on, each different set kept once (set 0: none).
#[derive(Default)]
pub(crate) struct Sets {
    index: HashMap<Vec<u32>, u32>,
}

impl Sets {
    /// The set `outer` with `layer` added, as a number in `d.sets`.
    pub(crate) fn with(&mut self, d: &mut Drawing, outer: u32, layer: u32) -> u32 {
        let mut set = d.sets.get(outer as usize).cloned().unwrap_or_default();
        if let Err(at) = set.binary_search(&layer) {
            set.insert(at, layer);
        }
        if d.sets.is_empty() {
            d.sets.push(Vec::new());
        }
        if let Some(&n) = self.index.get(&set) {
            return n;
        }
        let n = d.sets.len() as u32;
        self.index.insert(set.clone(), n);
        d.sets.push(set);
        n
    }
}

impl Drawing {
    /// Whether each set of layers is shown: all its layers are.
    fn sets_shown(&self) -> Vec<bool> {
        self.sets.iter().map(|s| s.iter().all(|&l| self.layers.get(l as usize).is_none_or(|l| l.shown))).collect()
    }

    /// The things drawn on layers that are shown. A thing without a set (a plot, the sheet of a
    /// layout) is always shown.
    pub(crate) fn shown_items(&self) -> impl Iterator<Item = &Item> {
        self.shown_indexed().map(|(_, item)| item)
    }

    /// `shown_items`, with their place in `items`.
    pub(crate) fn shown_indexed(&self) -> impl Iterator<Item = (usize, &Item)> {
        let shown = self.sets_shown();
        self.items.iter().enumerate().filter(move |(i, _)| self.on.get(*i).is_none_or(|&s| shown.get(s as usize).copied().unwrap_or(true)))
    }

    /// The centres of the circles and arcs on layers that are shown.
    pub(crate) fn shown_centres(&self) -> impl Iterator<Item = P> + '_ {
        let shown = self.sets_shown();
        self.centres.iter().filter(move |(_, s)| shown.get(*s as usize).copied().unwrap_or(true)).map(|(p, _)| *p)
    }

    /// The layers shown as `shown` says (one for each of `layers`; another length is ignored).
    pub fn set_layers(&mut self, shown: &[bool]) {
        if shown.len() == self.layers.len() {
            for (l, &s) in self.layers.iter_mut().zip(shown) {
                l.shown = s;
            }
        }
    }

    /// A copy with the layers shown as `shown` says, framed as before so that the view does not
    /// move (larger if a layer turned on reaches further).
    pub fn with_layers(&self, shown: &[bool]) -> Drawing {
        let mut d = self.clone();
        d.set_layers(shown);
        d.frame = match (self.bounds(), d.bounds_of_shown()) {
            (Some(a), Some(b)) => Some([a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]),
            (a, b) => a.or(b),
        };
        d
    }

    /// Whether each layer has something on it, in the model or in a layout: those worth a switch.
    pub fn used_layers(&self) -> Vec<bool> {
        let mut used = vec![false; self.layers.len()];
        for d in std::iter::once(self).chain(self.sheets.iter().map(|(_, s)| s.as_ref())) {
            for set in &d.sets {
                for &l in set {
                    if let Some(u) = used.get_mut(l as usize) {
                        *u = true;
                    }
                }
            }
        }
        used
    }

    /// Whether each layer is shown.
    pub fn layers_shown(&self) -> Vec<bool> {
        self.layers.iter().map(|l| l.shown).collect()
    }
}

/// The layers listed in the panel: those used, whose name contains `search` (any case), by
/// their number.
pub fn listed(layers: &[Layer], used: &[bool], search: &str) -> Vec<usize> {
    let search = search.trim().to_lowercase();
    (0..layers.len()).filter(|&i| used.get(i).copied().unwrap_or(false) && layers[i].name.to_lowercase().contains(&search)).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Two layers in the table, A shown and B off; a line on each, a block on B holding a line on
    /// A, and a line on C, a layer missing from the table.
    pub(crate) const DXF: &str = "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n0\nLAYER\n2\nA\n70\n0\n62\n1\n6\nCONTINUOUS\n0\nLAYER\n2\nB\n70\n0\n62\n-3\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nK\n70\n0\n10\n0\n20\n0\n30\n0\n3\nK\n0\nLINE\n8\nA\n10\n0\n20\n0\n30\n0\n11\n5\n21\n5\n31\n0\n0\nENDBLK\n8\n0\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nA\n10\n0\n20\n0\n30\n0\n11\n100\n21\n0\n31\n0\n0\nLINE\n8\nB\n10\n0\n20\n0\n30\n0\n11\n0\n21\n100\n31\n0\n0\nINSERT\n8\nB\n2\nK\n10\n50\n20\n50\n30\n0\n0\nLINE\n8\nC\n10\n0\n20\n0\n30\n0\n11\n200\n21\n200\n31\n0\n0\nENDSEC\n0\nEOF\n";

    fn names(d: &Drawing) -> Vec<&str> {
        d.layers.iter().map(|l| l.name.as_str()).collect()
    }

    fn index(d: &Drawing, name: &str) -> usize {
        d.layers.iter().position(|l| l.name == name).unwrap()
    }

    #[test]
    fn layers_off_in_the_file_start_hidden_and_can_be_turned_on() {
        let d = crate::cad::read(DXF.as_bytes(), "l.dxf").unwrap();
        assert!(names(&d).contains(&"A") && names(&d).contains(&"B") && names(&d).contains(&"C"));
        let (a, b, c) = (index(&d, "A"), index(&d, "B"), index(&d, "C"));
        assert!(d.layers[a].shown && !d.layers[b].shown && d.layers[c].shown);
        // Every line is read, those on B hidden: the one on A, and the one on C.
        assert_eq!(d.items.len(), 4);
        assert_eq!(d.shown_items().count(), 2);
        let used = d.used_layers();
        assert!(used[a] && used[b] && used[c]);
        // B on: its line and the block on it, with the line on A inside.
        let mut on = d.layers_shown();
        on[b] = true;
        assert_eq!(d.with_layers(&on).shown_items().count(), 4);
        // A off: its line and the one inside the block go, B's own line stays.
        on[a] = false;
        assert_eq!(d.with_layers(&on).shown_items().count(), 2);
    }

    #[test]
    fn the_view_keeps_its_frame() {
        let d = crate::cad::read(DXF.as_bytes(), "l.dxf").unwrap();
        let before = d.bounds().unwrap();
        // C off: what is left is smaller, the frame stays.
        let mut on = d.layers_shown();
        on[index(&d, "C")] = false;
        let hidden = d.with_layers(&on);
        assert_eq!(hidden.bounds().unwrap(), before);
        assert!(hidden.bounds_of_shown().unwrap()[2] < before[2]);
        // The wrong number of choices changes nothing.
        assert_eq!(d.with_layers(&[false]).shown_items().count(), d.shown_items().count());
    }

    #[test]
    fn the_list_is_searched() {
        let layer = |name: &str| Layer { name: name.into(), color: [0; 3], shown: true };
        let layers = [layer("WALLS"), layer("Doors"), layer("Text-walls"), layer("Unused walls")];
        let used = [true, true, true, false];
        assert_eq!(listed(&layers, &used, ""), vec![0, 1, 2]);
        assert_eq!(listed(&layers, &used, " wall"), vec![0, 2]);
        assert!(listed(&layers, &used, "roof").is_empty());
    }
}
