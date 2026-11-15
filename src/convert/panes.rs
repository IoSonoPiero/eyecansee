//! The two images of Save as…: the original and, decoded back, the file it will write; fitted, or
//! both at 100% around the same point, moved together by dragging either.

use eframe::egui::{self, TextureHandle};

use super::{GAP, Window};
use crate::i18n::I18n;
use crate::paint::{paint_turned, paint_zoomed, turned_size};
use crate::transform::Turn;

/// The original and the new image side by side, each under its title.
pub(super) fn convert_images(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n, tex: &TextureHandle, half: f32, pane: egui::Vec2) {
    let t = |id| i18n.t(id);
    let result_tex = match &w.result {
        Some(Ok((_, Some(tex)))) => Some(tex),
        _ => None,
    };
    // Titles in rows of the same height (the spinner is taller than the text), so the
    // two images start at the same height.
    let title = |ui: &mut egui::Ui, text: String, busy: bool| {
        ui.allocate_ui_with_layout(egui::vec2(half, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(egui::vec2(half, 22.0));
            ui.strong(text);
            if busy {
                ui.spinner();
                ui.weak(t("convert-working"));
            }
        });
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.vertical(|ui| {
            title(ui, t("convert-original"), false);
            // Turned and flipped as in the viewer, as the new image is: the same spot side by side.
            let r = preview(ui, pane, Some(tex), None, w.options.turn, w.actual, &mut w.pan);
            drag(&r, w.actual, &mut w.pan);
        });
        ui.vertical(|ui| {
            title(ui, t("convert-result"), w.busy);
            let note = match &w.result {
                _ if w.busy && result_tex.is_none() => Some(t("convert-working")),
                Some(Ok((_, None))) => Some(t("convert-no-preview")),
                _ => None,
            };
            // The file written, decoded back: already turned.
            let r = preview(ui, pane, result_tex, note.as_deref(), Turn::default(), w.actual, &mut w.pan);
            drag(&r, w.actual, &mut w.pan);
        });
    });
}

/// The image turned by `turn`, fitted into `size` (never enlarged), or at 100% around `pan`, a point
/// of the turned image (the same for both images); grey when none.
/// `note` is written in the middle when there is no image (converting, no preview for the format).
fn preview(ui: &mut egui::Ui, size: egui::Vec2, tex: Option<&TextureHandle>, note: Option<&str>, turn: Turn, actual: bool, pan: &mut egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let painter = ui.painter_at(rect);
    // Grey ground and a border: on the window's own colour the two images ran into each other.
    let ground = if ui.visuals().dark_mode { egui::Color32::from_gray(24) } else { egui::Color32::from_gray(200) };
    painter.rect_filled(rect, 2.0, ground);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
    let Some(tex) = tex else {
        if let Some(note) = note {
            let font = egui::TextStyle::Body.resolve(ui.style());
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, note, font, ui.visuals().text_color());
        }
        return response;
    };
    let ppp = ui.ctx().pixels_per_point();
    let t = turned_size(tex, turn);
    if actual {
        paint_zoomed(&painter, rect, tex, 1.0, pan, turn);
    } else {
        let scale = (rect.width() / t.x).min(rect.height() / t.y).min(1.0 / ppp);
        let dest = egui::Rect::from_center_size(rect.center(), t * scale);
        paint_turned(&painter, tex, dest, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), turn);
    }
    response
}

/// Dragging either preview moves both, at 100%.
fn drag(response: &egui::Response, actual: bool, pan: &mut egui::Vec2) {
    if actual && response.dragged() {
        *pan -= response.drag_delta() * response.ctx.pixels_per_point();
    }
}

/// What the viewer did to the image, said under the original (`convert-turned`); none when nothing.
pub(super) fn turn_note(turn: Turn) -> Option<&'static str> {
    Some(match turn.parts() {
        (false, 0) => return None,
        (false, 1) => "cw",
        (false, 2) => "half",
        (false, _) => "ccw",
        (true, 0) => "flip-h",
        (true, 2) => "flip-v",
        (true, 1) => "flip-h-cw",
        (true, _) => "flip-h-ccw",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Op;

    /// The eight orientations a viewer's flips and turns make.
    fn all_turns() -> Vec<Turn> {
        let mut out = vec![Turn::default()];
        for op in [Op::Cw, Op::Cw, Op::Cw, Op::FlipH, Op::Cw, Op::Cw, Op::Cw] {
            out.push(out.last().unwrap().then(op));
        }
        out
    }

    /// The original drawn turned (each point of the pane taking the texture point `source_uv`
    /// gives) shows at every point the pixel the new file has there (pixels turned by `apply`,
    /// as `encode` does): the two panes show the same spot, fitted or at 100% around the same
    /// `pan`, for every turn and flip.
    #[test]
    fn original_shown_as_the_new_file() {
        let (w, h) = (5u32, 3u32);
        let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let turns = all_turns();
        assert_eq!(turns.iter().map(|t| t.parts()).collect::<std::collections::HashSet<_>>().len(), 8);
        for turn in turns {
            let mut saved = image::DynamicImage::ImageRgba8(img.clone());
            turn.apply(&mut saved);
            let saved = saved.to_rgba8();
            let (sw, sh) = saved.dimensions();
            assert_eq!(turn.sideways(), (sw, sh) == (h, w), "{turn:?}");
            for y in 0..sh {
                for x in 0..sw {
                    let shown = egui::pos2((x as f32 + 0.5) / sw as f32, (y as f32 + 0.5) / sh as f32);
                    let src = turn.source_uv(shown);
                    let (ox, oy) = ((src.x * w as f32) as u32, (src.y * h as f32) as u32);
                    assert_eq!(img.get_pixel(ox, oy), saved.get_pixel(x, y), "{turn:?} at {x},{y}");
                }
            }
        }
    }

    /// Under the original: nothing when the viewer did nothing, else what it did, in each language.
    #[test]
    fn note_under_the_original() {
        let say = |lang: &str, turn: Turn| turn_note(turn).map(|k| I18n::new(lang).t_args("convert-turned", &[("turn", k)]));
        let cw = Turn::default().then(Op::Cw);
        assert_eq!(say("en", Turn::default()), None);
        assert_eq!(say("it", cw).as_deref(), Some("ruotata di 90° in senso orario nel visore"));
        assert_eq!(say("en", cw).as_deref(), Some("rotated 90° clockwise in the viewer"));
        assert_eq!(say("it", Turn::default().then(Op::FlipV)).as_deref(), Some("riflessa verticalmente nel visore"));
        let notes: std::collections::HashSet<_> = all_turns().into_iter().skip(1).map(|t| turn_note(t).unwrap()).collect();
        assert_eq!(notes.len(), 7, "one note for each turn");
        for lang in ["en", "it"] {
            for t in all_turns().into_iter().skip(1) {
                let text = say(lang, t).unwrap();
                assert!(!text.contains("convert-turned") && !text.contains('{'), "{lang} {t:?}: {text}");
            }
        }
    }
}
