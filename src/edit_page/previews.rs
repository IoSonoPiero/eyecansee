//! The editor: the previews of the filters, highlighters and lenses, made again from the photo
//! when they changed, and a filter added from the menu.

use super::{Tool, Window};
use crate::edit::doc::{Obj, Pt, Shape, Style};

impl Window {
    /// The filters, highlighters and lenses whose preview is missing or was made for another box
    /// or amount (colour, zoom, what is under it: `photo::refresh_layers`): made again from the
    /// photo, at most 1200 pixels a side; while a drag is under way, or a button `held` (the
    /// slider of a filter's amount: audit 5.14), at most 400, to follow the pointer (the old one,
    /// stretched over the new place, showed the part it came from: user, 2026-11-06), and sharp
    /// again once let go. The lenses last, with the filters' previews and
    /// the cover boxes on them, and again whenever those changed.
    pub(super) fn refresh_previews(&mut self, held: bool) {
        use crate::edit::doc::{Pixels, Preview};
        use crate::edit::photo;
        let Some(Ok((original, ..))) = &self.original else { return };
        // Not from the photo of another size, still being resampled (`follow_size`).
        if self.doc.scale != self.scaled {
            return;
        }
        let still = self.hold.is_none() && !held;
        // A highlighter is quick to make and is over text, read closely: sharper.
        let side = |s: &Shape| match (still, s) {
            (false, _) => 400.0,
            (true, Shape::Highlight { .. }) => 3000.0,
            _ => 1200.0,
        };
        self.filters_made += photo::refresh_layers(&mut self.doc, &original.image, side) as u64;
        if still && std::mem::replace(&mut self.hiders, photo::hiders_key(&self.doc)) != self.hiders {
            for o in &mut self.doc.objects {
                if let Shape::Lens { preview, .. } = &mut o.shape {
                    *preview = Preview::default();
                }
            }
        }
        for i in 0..self.doc.objects.len() {
            let o = &self.doc.objects[i];
            let Shape::Lens { area, preview } = &o.shape else { continue };
            // Its key: the zoom, a half more when it shows the objects.
            let (area, amount) = (*area, o.style.zoom.max(1) as f32 + if o.style.with_objects { 0.5 } else { 0.0 });
            let key = amount.to_bits() as u64;
            let side = side(&o.shape);
            let scale = (side / area.w.max(area.h)).min(1.0);
            // Made for this box and zoom, and as sharp as it is to be now.
            let sharp = |p: &Pixels| p.image().width() + 1 >= (area.w * scale).round() as u32;
            if preview.0.as_ref().is_some_and(|(a, m, p)| *a == area && *m == key && sharp(p)) {
                continue;
            }
            let filtered: Vec<photo::Filtered> = self.doc.objects.iter().filter_map(|f| if let Shape::Filter { preview: Preview(Some((a, _, p))), .. } = &f.shape { Some((*a, p.image())) } else { None }).collect();
            let fonts = crate::decode::usvg_options();
            let before = o.style.with_objects.then_some((&self.doc.objects[..i], &fonts));
            let pixels = photo::lens(&self.doc, &original.image, &area, (amount.floor(), side as u32), &filtered, before);
            if let Shape::Lens { preview, .. } = &mut self.doc.objects[i].shape {
                *preview = Preview(Some((area, key, Pixels::new(pixels))));
            }
            self.filters_made += 1;
        }
    }

    /// After Undo or Redo: the lenses made again at once. A step keeps the previews of the frame
    /// it was taken in, and the step of a cover is taken as the mouse lets go of it, before the
    /// lenses are made again with it: back to that step, a lens showed what the cover hides
    /// (audit 5.8), and nothing made it again, as the covers were the same as before Undo.
    pub(super) fn lenses_again(&mut self) {
        for o in &mut self.doc.objects {
            if let Shape::Lens { preview, .. } = &mut o.shape {
                *preview = crate::edit::doc::Preview::default();
            }
        }
        self.refresh_previews(false);
    }

    /// A filter from the menu: on the part chosen with Selection, or on the whole image; selected,
    /// with Select chosen to move it or change its amount.
    pub(super) fn add_filter(&mut self, filter: crate::edit::filter::Filter) {
        let (iw, ih) = self.size();
        let area = self.marquee.unwrap_or(crate::edit::doc::Area { centre: Pt::new(0.0, 0.0), w: iw, h: ih, rot: 0.0 });
        let amount = filter.amount(self.long()).map_or(0.0, |(_, _, start)| start);
        self.doc.objects.push(Obj { shape: Shape::Filter { area, filter, amount, preview: Default::default() }, style: Style::default() });
        self.selected = Some(self.doc.objects.len() - 1);
        self.choose(Tool::Select);
        // `choose` keeps it when Select was already the tool.
        self.marquee = None;
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Event, Modifiers, PointerButton, pos2};

    use super::super::Tool;
    use crate::audit_family_1::{Harness, own_folder};
    use crate::edit::doc::{Area, Obj, Preview, Pt, Shape, Style};

    /// The width of the preview of the filter, the first object.
    fn preview_width(h: &Harness) -> u32 {
        match &h.app.editor.as_ref().unwrap().doc.objects[0].shape {
            Shape::Filter { preview: Preview(Some((_, _, p))), .. } => p.image().width(),
            _ => 0,
        }
    }

    /// Audit 5.14 (#295): the previews of the filters were made again at up to 1200 pixels in the
    /// frame at every step of the slider of the amount, as the pointer moved it. Now at 400 while
    /// a button is held, as during a drag on the image, and sharp again once let go.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn audit_5_14_a_filter_follows_its_slider_small() {
        if own_folder("edit_page::previews::tests::audit_5_14_a_filter_follows_its_slider_small") {
            return;
        }
        let dir = crate::audit_family_1::guard().join("five-fourteen-filter");
        std::fs::create_dir_all(&dir).unwrap();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2000, 1500, image::Rgb([200, 120, 40]))).save(dir.join("a.png")).unwrap();
        let mut h = Harness::new(&dir);
        super::super::defects::editor(&mut h, "a.png", Tool::Select);
        let area = Area { centre: Pt::new(0.0, 0.0), w: 2000.0, h: 1500.0, rot: 0.0 };
        let filter = Shape::Filter { area, filter: crate::edit::filter::Filter::Blur, amount: 4.0, preview: Default::default() };
        h.app.editor.as_mut().unwrap().doc.objects.push(Obj { shape: filter, style: Style::default() });
        h.frame(Vec::new());
        assert_eq!(preview_width(&h), 1200);
        // The button held on the slider (here on the status bar, which takes no drag), the amount moved.
        let at = pos2(4.0, 796.0);
        let button = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        h.frame(vec![Event::PointerMoved(at), button(true)]);
        if let Shape::Filter { amount, .. } = &mut h.app.editor.as_mut().unwrap().doc.objects[0].shape {
            *amount = 6.0;
        }
        h.frame(Vec::new());
        let held = preview_width(&h);
        eprintln!("preview made while the slider is held: {held} pixels wide");
        assert!(held <= 400, "the preview made sharp at every step of the slider: {held}");
        h.frame(vec![button(false)]);
        h.frame(Vec::new());
        assert_eq!(preview_width(&h), 1200, "sharp again once let go");
    }
}
