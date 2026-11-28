//! The editor's clipboard and eyedropper: Copy (the result, or the part chosen with Selection),
//! Cut, Paste (the part, or an image from another program, as a patch), the colour the screen's
//! eyedropper took. Moved from `edit_page.rs` on 2026-11-05, unchanged.

use eframe::egui::{self, ColorImage};

use super::Tool;
use crate::App;
use crate::actions::Act;
use crate::edit::doc::{Obj, Pt, Shape, Style};

impl App {
    /// Ctrl+C or Copy: the result at full size, made in the background (`Msg::EditCopy`); with
    /// Selection, the part chosen (kept for Paste, and in the clipboard for other programs).
    pub(crate) fn edit_copy(&mut self, ctx: &egui::Context) {
        if self.copy_part(ctx) {
            return;
        }
        let Some(w) = &mut self.editor else { return };
        let Some(Ok((original, ..))) = &w.original else { return };
        if w.copying {
            return;
        }
        w.copying = true;
        let (doc, original, tx, ctx) = (w.doc.clone(), original.clone(), self.tx.clone(), ctx.clone());
        let size = watch_size(w);
        crate::logging::spawn_named("edit-copy", move || {
            // A fault while it is made is said, not a Copy greyed out for ever (audit 5.15).
            let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::edit::save::result(&doc, &original, &crate::decode::usvg_options())));
            let r = made.map_err(|p| super::fault_text(p.as_ref())).map(|img| {
                if let Some(size) = size {
                    let too_large = |i: &image::RgbaImage| crate::edit::clip_limit::too_large(i.as_raw(), i.width(), i.height());
                    if let Some(n) = img.as_rgba8().map_or_else(|| too_large(&img.to_rgba8()), too_large) {
                        let _ = size.send(n);
                    }
                }
                crate::decode::to_color_image(img)
            });
            if tx.send(crate::loader::Msg::EditCopy(r)).is_ok() {
                ctx.request_repaint();
            }
        });
    }

    /// The screen's eyedropper answered: the colour goes to the style shown, or why not.
    pub(crate) fn on_edit_picked(&mut self, ctx: &egui::Context, r: Result<[u8; 3], String>) {
        let Some(w) = &mut self.editor else { return };
        w.screen_picking = false;
        match r {
            Ok(c) => {
                w.picked = Some(c);
                w.just_picked = Some(c);
                ctx.copy_text(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
            }
            Err(e) if e == "cancelled" => {}
            // In the error bar, to close and to copy, as every error: it was a note in the status
            // bar that went after six seconds.
            Err(e) => {
                log!("eyedropper: ERROR {e}");
                self.error = Some(self.i18n.t_args("edit-pick-failed", &[("detail", &e)]));
            }
        }
    }

    /// The part chosen with Selection copied as it is seen: the photo with the filters, the
    /// highlighters, the objects drawn and the cover boxes (it was the photo alone: user,
    /// 2026-11-06), as a magnifier of 1× with its objects makes it. Made in a thread aside from
    /// the document as it is now (audit 5.14: up to 8000 × 8000, the window still meanwhile), the
    /// clipboard and Paste have it once it comes (`part_copied`). `false` when there is none.
    fn copy_part(&mut self, ctx: &egui::Context) -> bool {
        let Some(w) = &mut self.editor else { return false };
        let (Some(area), Some(Ok((original, ..)))) = (w.marquee, &w.original) else { return false };
        let photo = original.clone();
        let (area, doc) = (w.doc.on_pixels(area, w.upright()), w.doc.clone());
        let size = watch_size(w);
        w.aside.copying_part = true;
        w.aside(ctx, "edit-copy-part", move || {
            use crate::edit::doc::{Pixels, Preview};
            // The photo at the document's size, also while it is resampled (`follow_size`).
            let photo = crate::edit::resize::at_size(&photo, doc.scale);
            let filtered: Vec<crate::edit::photo::Filtered> = doc.objects.iter().filter_map(|f| if let Shape::Filter { preview: Preview(Some((a, _, p))), .. } = &f.shape { Some((*a, p.image())) } else { None }).collect();
            let fonts = crate::decode::usvg_options();
            let pixels = Pixels::new(crate::edit::photo::lens(&doc, &photo.image, &area, (1.0, 8000), &filtered, Some((&doc.objects, &fonts))));
            let img = pixels.image();
            if let Some(size) = size
                && let Some(n) = crate::edit::clip_limit::too_large(img.as_raw(), img.width(), img.height())
            {
                let _ = size.send(n);
            }
            let shown = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            super::work::Done::Part(area, pixels, shown)
        });
        true
    }

    /// The part copied (`copy_part`) came: in the clipboard, kept for Paste, and pasted when a
    /// Paste waited for it.
    pub(super) fn part_copied(&mut self, ctx: &egui::Context, w: &mut super::Window, (area, pixels, shown): (crate::edit::doc::Area, crate::edit::doc::Pixels, ColorImage)) {
        ctx.copy_image(shown);
        w.clip = Some((area, pixels));
        w.note = Some((self.i18n.t("edit-part-copied"), ctx.input(|i| i.time) + 4.0));
        w.aside.copying_part = false;
        if std::mem::take(&mut w.aside.paste_after)
            && let Some((area, pixels)) = w.clip_patch(None)
        {
            w.put_patch(area, pixels);
        }
    }

    /// Ctrl+X or Cut: the part copied, then covered with the colour of the empty corners (white
    /// without one); a cover box, so nothing of it is left in the saved file.
    pub(crate) fn edit_cut(&mut self, ctx: &egui::Context) {
        if !self.copy_part(ctx) {
            return;
        }
        let Some(w) = &mut self.editor else { return };
        let Some(area) = w.marquee.take() else { return };
        let color = w.doc.fill.unwrap_or([255, 255, 255]);
        w.doc.objects.push(Obj { shape: Shape::Cover(area), style: Style { color, ..Style::default() } });
    }

    /// The editor's copy, cut and paste among the window's events (Ctrl+C, X, V and the system's
    /// keys), taken out of them. A Ctrl+V that brought nothing is a paste too, so it says why: with
    /// an empty clipboard, or one that could not be read, egui sends no paste, only the key let go.
    pub(crate) fn edit_clip_events(&mut self, events: &mut Vec<egui::Event>) -> Option<Act> {
        let w = self.editor.as_mut()?;
        let mut act = None;
        events.retain(|e| {
            let (a, keep) = clip_event(e, &mut w.pasted);
            act = a.or(act.take());
            keep
        });
        act
    }

    /// Ctrl+V or Paste: the part copied, as a patch a little aside of where it was; without one,
    /// an image from the clipboard of another program, in the middle of the result, at most 80% of
    /// it. Selected, with Select chosen to move it. Nothing to paste: a note says so (it was
    /// silent, as when the clipboard holds text).
    pub(crate) fn edit_paste(&mut self, ctx: &egui::Context, img: Option<std::sync::Arc<ColorImage>>) {
        let Some(w) = &mut self.editor else { return };
        // The part being copied: the clipboard does not have it yet, the Paste waits for it.
        if w.aside.copying_part {
            w.aside.paste_after = true;
            return;
        }
        let (area, pixels) = match (w.clip_patch(img.as_deref()), img) {
            (Some(patch), _) => patch,
            (None, Some(img)) => {
                let [iw, ih] = img.size;
                if !crate::edit::resize::fits(iw, ih) {
                    let (max, mp) = (crate::edit::resize::MAX_SIDE.to_string(), (crate::new_image::MAX_PIXELS / 1_000_000).to_string());
                    self.error = Some(self.i18n.t_args("edit-paste-too-big", &[("w", &iw.to_string()), ("h", &ih.to_string()), ("max", &max), ("mp", &mp)]));
                    return;
                }
                // Its pixels made aside (audit 5.14: up to 200 megapixels), and the PNG that draws
                // them, which the first frame drawing it made (audit 5.12); the patch once they come.
                w.aside(ctx, "edit-paste", move || {
                    let pixels = patch_pixels(&img);
                    pixels.as_ref().map(crate::edit::doc::Pixels::png_base64);
                    super::work::Done::Pasted(pixels)
                });
                return;
            }
            (None, None) => {
                w.note = Some((self.i18n.t("edit-paste-none"), ctx.input(|i| i.time) + 4.0));
                return;
            }
        };
        w.put_patch(area, pixels);
    }

    pub(crate) fn on_edit_copy(&mut self, ctx: &egui::Context, r: Result<ColorImage, String>) {
        let Some(w) = &mut self.editor else { return };
        w.copying = false;
        match r {
            Ok(img) => {
                ctx.copy_image(img);
                w.note = Some((self.i18n.t("edit-copied"), ctx.input(|i| i.time) + 4.0));
            }
            Err(e) => {
                log!("edit: copy failed: {e}");
                self.error = Some(super::result_failed(&self.i18n, &e));
            }
        }
    }
}

impl super::Window {
    /// The part copied as a patch a little aside of where it was; none when `img`, the image in
    /// the clipboard, is of another size: copied elsewhere after the part, it wins.
    fn clip_patch(&self, img: Option<&ColorImage>) -> Option<(crate::edit::doc::Area, crate::edit::doc::Pixels)> {
        let (a, p) = self.clip.as_ref().filter(|(_, p)| img.is_none_or(|i| i.size == [p.image().width() as usize, p.image().height() as usize]))?;
        let step = self.long() / 40.0;
        Some((crate::edit::doc::Area { centre: Pt::new(a.centre.x + step, a.centre.y + step), ..*a }, p.clone()))
    }

    /// A patch pasted: selected, with Select chosen to move it.
    fn put_patch(&mut self, area: crate::edit::doc::Area, pixels: crate::edit::doc::Pixels) {
        self.doc.objects.push(Obj { shape: Shape::Patch { area, pixels, mirrored: false }, style: Style::default() });
        self.selected = Some(self.doc.objects.len() - 1);
        self.choose(Tool::Select);
        self.marquee = None;
    }
}

/// The pixels of an image from the clipboard of another program, for a patch. `None` for one
/// larger than the editor takes (audit 5.10).
fn patch_pixels(img: &ColorImage) -> Option<crate::edit::doc::Pixels> {
    if !crate::edit::resize::fits(img.size[0], img.size[1]) {
        return None;
    }
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba).map(crate::edit::doc::Pixels::new)
}

impl super::Window {
    /// The pixels of an image of another program came (`patch_pixels`): a patch in the middle of
    /// the result, at most 80% of it, upright on the screen.
    pub(super) fn pasted_in(&mut self, pixels: crate::edit::doc::Pixels) {
        let (pw, ph) = (pixels.image().width() as f32, pixels.image().height() as f32);
        let c = self.doc.crop;
        let k = (0.8 * c.w / pw).min(0.8 * c.h / ph).min(1.0);
        let centre = crate::edit::geom::turned(Pt::new(c.cx, c.cy), -self.doc.angle);
        self.put_patch(crate::edit::doc::Area { centre, w: pw * k, h: ph * k, rot: -self.doc.angle }, pixels);
    }
}

/// On X11, where other programs get at most about 16 MB of a copy (`clip_limit`), where to send its
/// size when it is over: the status bar says it.
fn watch_size(w: &mut super::Window) -> Option<std::sync::mpsc::Sender<usize>> {
    w.too_big = None;
    if !crate::edit::clip_limit::through_x11() {
        return None;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    w.too_big = Some(rx);
    Some(tx)
}

/// What one event of the window does in the editor, and whether it stays for the others. `pasted`:
/// the clipboard gave something since the last Ctrl+V let go.
fn clip_event(e: &egui::Event, pasted: &mut bool) -> (Option<Act>, bool) {
    match e {
        egui::Event::Copy => (Some(Act::EditCopy), false),
        egui::Event::Cut => (Some(Act::EditCut), false),
        egui::Event::Paste(_) => {
            *pasted = true;
            (Some(Act::EditPaste(None)), false)
        }
        egui::Event::PasteImage(img) => {
            *pasted = true;
            (Some(Act::EditPaste(Some(img.clone()))), false)
        }
        egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. } if modifiers.command => (if std::mem::take(pasted) { None } else { Some(Act::EditPaste(None)) }, true),
        _ => (None, true),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::{Event, Key, Modifiers};

    use super::*;

    fn v_up(modifiers: Modifiers) -> Event {
        Event::Key { key: Key::V, physical_key: None, pressed: false, repeat: false, modifiers }
    }

    /// Ctrl+V with an image pastes it once; with nothing on the clipboard (no paste event, only the
    /// key let go) it is a paste of nothing, which says so; V alone is not.
    #[test]
    fn paste_of_nothing_is_seen() {
        let img = std::sync::Arc::new(ColorImage::filled([1, 1], egui::Color32::RED));
        let mut pasted = false;
        let (a, keep) = clip_event(&Event::PasteImage(img), &mut pasted);
        assert!(matches!(a, Some(Act::EditPaste(Some(_)))) && !keep);
        assert!(matches!(clip_event(&v_up(Modifiers::COMMAND), &mut pasted), (None, true)));
        assert!(matches!(clip_event(&v_up(Modifiers::COMMAND), &mut pasted), (Some(Act::EditPaste(None)), true)));
        assert!(matches!(clip_event(&v_up(Modifiers::NONE), &mut pasted), (None, true)));
        assert!(matches!(clip_event(&Event::Paste("text".into()), &mut pasted), (Some(Act::EditPaste(None)), false)));
        assert!(matches!(clip_event(&v_up(Modifiers::COMMAND), &mut pasted), (None, true)));
    }

    /// Audit 5.14 (#295): Copy of the part chosen with Selection made it at full size (up to
    /// 8000 × 8000, a pixel at a time, with the objects) inside the frame, the window still
    /// meanwhile. The frame asks; the part comes from a thread aside, for Paste too.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn audit_5_14_the_part_is_copied_aside() {
        use crate::audit_family_1::{Harness, own_folder};
        if own_folder("edit_page::clip::tests::audit_5_14_the_part_is_copied_aside") {
            return;
        }
        let mut h = Harness::new(&super::super::defects::white("five-fourteen-part"));
        super::super::defects::editor(&mut h, "a.png", super::super::Tool::Marquee);
        h.app.editor.as_mut().unwrap().marquee = Some(crate::edit::doc::Area { centre: Pt::new(0.0, 0.0), w: 100.0, h: 80.0, rot: 0.0 });
        crate::edit::photo::LENSES.with(|n| n.set(0));
        h.frame(vec![Event::Copy]);
        let made = crate::edit::photo::LENSES.with(|n| n.get());
        eprintln!("parts made in the frame of Copy: {made}");
        assert_eq!(made, 0, "the part made in the frame");
        h.until(|app| app.editor.as_ref().is_some_and(|w| w.clip.is_some()), "the part copied");
        let w = h.app.editor.as_ref().unwrap();
        assert_eq!((w.clip.as_ref().unwrap().1.image().width(), w.note.is_some()), (100, true));
    }

    /// A Paste while the part is being copied waits for it, and pastes it once it comes: the
    /// clipboard still has what it had before (here an image of another size, which would win).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn a_paste_waits_for_the_part_being_copied() {
        use crate::audit_family_1::{Harness, own_folder};
        if own_folder("edit_page::clip::tests::a_paste_waits_for_the_part_being_copied") {
            return;
        }
        let mut h = Harness::new(&super::super::defects::white("five-fourteen-paste-waits"));
        super::super::defects::editor(&mut h, "a.png", super::super::Tool::Marquee);
        h.app.editor.as_mut().unwrap().aside.copying_part = true;
        let before = std::sync::Arc::new(ColorImage::filled([7, 7], egui::Color32::RED));
        h.app.edit_paste(&h.ctx.clone(), Some(before));
        let mut w = h.app.editor.take().unwrap();
        assert!(w.doc.objects.is_empty() && w.aside.paste_after, "pasted what the clipboard had before");
        let area = crate::edit::doc::Area { centre: Pt::new(0.0, 0.0), w: 30.0, h: 20.0, rot: 0.0 };
        let pixels = crate::edit::doc::Pixels::new(image::RgbaImage::new(30, 20));
        h.app.part_copied(&h.ctx.clone(), &mut w, (area, pixels, ColorImage::filled([30, 20], egui::Color32::RED)));
        let patch = w.doc.objects.last().and_then(|o| if let Shape::Patch { pixels, .. } = &o.shape { Some(pixels.image().width()) } else { None });
        assert_eq!((patch, w.selected, w.aside.copying_part), (Some(30), Some(0), false));
    }

    /// An image pasted from another program larger than the editor takes (20 000 pixels a side)
    /// is refused: it had no limit (audit 5.10). One that fits becomes a patch.
    #[test]
    fn pasted_image_within_the_limit() {
        let max = crate::edit::resize::MAX_SIDE as usize;
        let patch = patch_pixels(&ColorImage::filled([30, 20], egui::Color32::RED)).unwrap();
        assert!(patch_pixels(&ColorImage::filled([max, 2], egui::Color32::RED)).is_some());
        assert!(patch_pixels(&ColorImage::filled([max + 1, 2], egui::Color32::RED)).is_none());
        assert!(patch_pixels(&ColorImage::filled([2, max + 1], egui::Color32::RED)).is_none());
        // In the middle of the result, at most 80% of it.
        let mut w = super::super::Window::new("p/a.png".into(), Default::default(), super::super::Tool::Select);
        w.doc = crate::edit::doc::Doc::new(20, 48, None);
        w.pasted_in(patch);
        let a = *w.doc.objects[0].shape.area().unwrap();
        assert!((a.w - 16.0).abs() < 1e-3 && (a.h - 16.0 * 20.0 / 30.0).abs() < 1e-3, "{a:?}");
    }
}
