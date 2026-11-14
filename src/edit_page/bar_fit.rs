//! Whether the editor's top bar fits on one row: its width summed from the parts it is drawn
//! with (the square buttons, the separators, the spacing of the style, the label of the main save
//! button measured in the bar's font). An estimate before put the bar on two rows in a 1200 px
//! window where one was enough (issue #114).

use eframe::egui::{self, Color32};

use super::tools::TOOLS;
use crate::icons;

/// Side of the icon of the arrow next to Save copy, with Save as… and Overwrite in its menu.
pub(super) const ARROW_ICON: f32 = 16.0;
/// Least width of that arrow.
pub(super) const ARROW_MIN: f32 = 24.0;
/// Room between the arrow, Save copy and Close: they stay together.
pub(super) const SAVE_GAP: f32 = 1.0;
/// Width of a separator of the bar.
pub(super) const SEPARATOR: f32 = 6.0;
/// The square buttons on the left: the tools, the two quarter turns, Resize, Filters, Undo, Redo.
const LEFT_BUTTONS: usize = TOOLS.len() + 6;
/// The separators on the left: before Crop, before Undo.
const LEFT_SEPARATORS: usize = 2;

/// The widths the top bar is made of, in points.
#[derive(Clone, Copy, Debug)]
pub(super) struct BarParts {
    /// A square icon button: a tool, a turn, Undo, Close, Copy.
    button: f32,
    /// The arrow of Save copy's menu.
    arrow: f32,
    /// The main save button, with the label it has now.
    save: f32,
    /// The room after an item.
    spacing: f32,
    /// The spinner while a copy is being made, with its room; 0 when there is none.
    spinner: f32,
}

impl BarParts {
    /// From the style (button padding, item spacing, height of a control), the width of the main
    /// save button's label and whether the spinner of Copy is shown. A button is as wide as its
    /// icon or label with the padding on both sides, and at least its least width.
    pub(super) fn new(padding: f32, spacing: f32, control: f32, label: f32, copying: bool) -> Self {
        Self {
            button: icons::BUTTON.x.max(icons::ICON + 2.0 * padding),
            arrow: ARROW_MIN.max(ARROW_ICON + 2.0 * padding),
            save: label + 2.0 * padding,
            spacing,
            spinner: if copying { control + spacing } else { 0.0 },
        }
    }

    /// Measured in `ui`: its style, and `label` laid out in the font of the buttons.
    pub(super) fn measure(ui: &egui::Ui, label: String, copying: bool) -> Self {
        let s = ui.spacing();
        let font = egui::TextStyle::Body.resolve(ui.style());
        let label = ui.painter().layout_no_wrap(label, font, Color32::WHITE).size().x;
        Self::new(s.button_padding.x, s.item_spacing.x, s.interact_size.y, label, copying)
    }
}

/// The width the bar needs on one row. On the left the buttons and the separators, each with the
/// room after it; on the right, drawn right to left, the arrow and Save copy, Close, the spinner
/// while copying, Copy (`App::right_row`).
pub(super) fn one_row_width(p: BarParts) -> f32 {
    let items = (LEFT_BUTTONS + LEFT_SEPARATORS) as f32;
    let left = LEFT_BUTTONS as f32 * p.button + LEFT_SEPARATORS as f32 * SEPARATOR + items * p.spacing;
    let right = p.arrow + SAVE_GAP + p.save + SAVE_GAP + p.button + p.spacing + p.spinner + p.button;
    left + right
}

/// The bar fits on one row in `available` points.
pub(super) fn fits(available: f32, p: BarParts) -> bool {
    available >= one_row_width(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    /// egui's default style: padding and spacing 8, controls 20 high.
    fn default_parts(label: f32, copying: bool) -> BarParts {
        BarParts::new(8.0, 8.0, 20.0, label, copying)
    }

    /// The sum, part by part: 22 buttons 34 wide (18 + 8 + 8), 2 separators, 24 spacings; the
    /// arrow 32 wide, Save copy, Close, Copy.
    #[test]
    fn width_part_by_part() {
        let left = 22.0 * 34.0 + 2.0 * 6.0 + 24.0 * 8.0;
        let right = 32.0 + 1.0 + (60.0 + 16.0) + 1.0 + 34.0 + 8.0 + 34.0;
        assert_eq!(one_row_width(default_parts(60.0, false)), left + right);
        // The spinner while copying: its 20 points and the room after it.
        assert_eq!(one_row_width(default_parts(60.0, true)), left + right + 28.0);
    }

    /// Just as wide as the bar: one row; half a point less: two.
    #[test]
    fn boundary() {
        let p = default_parts(70.0, false);
        let need = one_row_width(p);
        assert!(fits(need, p));
        assert!(!fits(need - 0.5, p));
    }

    /// Through egui with the app's fonts, every label the main save button can have in every
    /// language: one row in a 1200 px window, two in a 900 px one.
    #[test]
    fn real_window() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let labels = ["edit-save-copy", "edit-update-copy", "menu-save-copy"];
        for (width, one_row) in [(1200.0, true), (900.0, false)] {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))), ..Default::default() };
            let mut seen = Vec::new();
            // The fonts are ready from the second pass.
            for _ in 0..2 {
                seen.clear();
                let _ = ctx.run_ui(input.clone(), |ui| {
                    egui::Panel::top("edit-tools").show(ui, |ui| {
                        for (lang, _) in crate::i18n::languages() {
                            let i18n = I18n::new(lang);
                            for id in labels {
                                let p = BarParts::measure(ui, i18n.t(id), false);
                                seen.push((format!("{lang} {id}"), ui.available_width(), one_row_width(p)));
                            }
                        }
                    });
                });
            }
            assert!(!seen.is_empty());
            for (what, available, need) in &seen {
                assert_eq!(available >= need, one_row, "{width} px, {what}: {need} needed, {available} available");
            }
        }
    }
}
