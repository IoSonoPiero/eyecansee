//! The editor's clipboard and eyedropper: Copy (the result, or the part chosen with Selection),
//! Cut, Paste (the part, or an image from another program, as a patch), the colour the screen's
//! eyedropper took. Moved from `edit_page.rs` on 2026-11-05, unchanged.

use eframe::egui::{self, ColorImage};

use super::Tool;
use crate::App;
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
        std::thread::spawn(move || {
            let img = crate::edit::save::result(&doc, &original, &crate::decode::usvg_options());
            if tx.send(crate::loader::Msg::EditCopy(crate::decode::to_color_image(img))).is_ok() {
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
            Err(e) => w.note = Some((self.i18n.t_args("edit-pick-failed", &[("detail", &e)]), ctx.input(|i| i.time) + 6.0)),
        }
    }

    /// The part chosen with Selection copied as it is seen: the photo with the filters, the
    /// highlighters, the objects drawn and the cover boxes (it was the photo alone: user,
    /// 2026-11-06), as a magnifier of 1× with its objects makes it. `false` when there is none.
    fn copy_part(&mut self, ctx: &egui::Context) -> bool {
        use crate::edit::doc::{Pixels, Preview};
        let Some(w) = &mut self.editor else { return false };
        let (Some(area), Some(Ok((original, ..)))) = (w.marquee, &w.original) else { return false };
        let area = w.doc.on_pixels(area, (original.image.width(), original.image.height()));
        let filtered: Vec<crate::edit::photo::Filtered> = w.doc.objects.iter().filter_map(|f| if let Shape::Filter { preview: Preview(Some((a, _, p))), .. } = &f.shape { Some((*a, p.image())) } else { None }).collect();
        let fonts = crate::decode::usvg_options();
        let pixels = Pixels::new(crate::edit::photo::lens(&w.doc, &original.image, &area, (1.0, 8000), &filtered, Some((&w.doc.objects, &fonts))));
        let img = pixels.image();
        ctx.copy_image(ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()));
        w.clip = Some((area, pixels));
        w.note = Some((self.i18n.t("edit-part-copied"), ctx.input(|i| i.time) + 4.0));
        true
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

    /// Ctrl+V or Paste: the part copied, as a patch a little aside of where it was; without one,
    /// an image from the clipboard of another program, in the middle of the result, at most 80% of
    /// it. Selected, with Select chosen to move it.
    pub(crate) fn edit_paste(&mut self, img: Option<std::sync::Arc<ColorImage>>) {
        let Some(w) = &mut self.editor else { return };
        // An image of another size in the clipboard was copied elsewhere after the part: it wins.
        let clip = w.clip.as_ref().filter(|(_, p)| img.as_ref().is_none_or(|i| i.size == [p.image().width() as usize, p.image().height() as usize]));
        let (area, pixels) = match (clip, img) {
            (Some((a, p)), _) => {
                let step = w.long() / 40.0;
                (crate::edit::doc::Area { centre: Pt::new(a.centre.x + step, a.centre.y + step), ..*a }, p.clone())
            }
            (None, Some(img)) => {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                let Some(image) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba) else { return };
                let c = w.doc.crop;
                let k = (0.8 * c.w / image.width() as f32).min(0.8 * c.h / image.height() as f32).min(1.0);
                // Upright on the screen, in the middle of what is shown.
                let centre = crate::edit::geom::turned(Pt::new(c.cx, c.cy), -w.doc.angle);
                (crate::edit::doc::Area { centre, w: image.width() as f32 * k, h: image.height() as f32 * k, rot: -w.doc.angle }, crate::edit::doc::Pixels::new(image))
            }
            (None, None) => return,
        };
        w.doc.objects.push(Obj { shape: Shape::Patch { area, pixels }, style: Style::default() });
        w.selected = Some(w.doc.objects.len() - 1);
        w.choose(Tool::Select);
        w.marquee = None;
    }

    pub(crate) fn on_edit_copy(&mut self, ctx: &egui::Context, img: ColorImage) {
        let Some(w) = &mut self.editor else { return };
        w.copying = false;
        ctx.copy_image(img);
        w.note = Some((self.i18n.t("edit-copied"), ctx.input(|i| i.time) + 4.0));
    }
}
