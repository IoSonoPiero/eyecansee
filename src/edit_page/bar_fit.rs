//! How the editor's top bar is laid out: its widths summed from the parts it is drawn with (the
//! square buttons, the separators, the spacing of the style, the label of the main save button
//! measured in the bar's font), and the rows it takes. An estimate before put the bar on two rows
//! in a 1200 px window where one was enough (issue #114); when the tools wrapped, Copy, Close and
//! Save copy went alone on a third row although the second had room for them (Windows at 110 %,
//! a 900 px window).

use eframe::egui::{self, Color32};

use crate::icons;

/// Side of the icon of the arrow next to Save copy, with Save as… and Overwrite in its menu.
pub(super) const ARROW_ICON: f32 = 16.0;
/// Least width of that arrow.
pub(super) const ARROW_MIN: f32 = 24.0;
/// Room between the arrow, Save copy and Close: they stay together.
pub(super) const SAVE_GAP: f32 = 1.0;
/// Width of a separator of the bar.
pub(super) const SEPARATOR: f32 = 6.0;

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

/// The widths of the left part, in the order drawn (`tools::tools_row`): the tools, a separator,
/// Crop, the two quarter turns, Resize, Filters, a separator, Undo, Redo.
#[cfg(test)]
fn left_items(p: BarParts) -> impl Iterator<Item = f32> {
    let buttons = |n| std::iter::repeat_n(p.button, n);
    buttons(super::tools::TOOLS.len() - 1).chain([SEPARATOR]).chain(buttons(5)).chain([SEPARATOR]).chain(buttons(2))
}

/// The width of the right part, drawn right to left: the arrow and Save copy, Close, the spinner
/// while copying, Copy (`App::right_row`).
pub(super) fn group_width(p: BarParts) -> f32 {
    p.arrow + SAVE_GAP + p.save + SAVE_GAP + p.button + p.spacing + p.spinner + p.button
}

/// The width the bar needs on one row: the left part, each item with the room after it, and the
/// right part.
#[cfg(test)]
pub(super) fn one_row_width(p: BarParts) -> f32 {
    left_items(p).map(|w| w + p.spacing).sum::<f32>() + group_width(p)
}

/// The rows the bar takes in `available` points: the left part wrapped as egui wraps it (an item
/// wider than what is left of the row starts the next one), then the right part at the end of the
/// last row when it fits in what is left of it, else on a row of its own (`right_end`). The bar
/// itself is laid out by egui, which measures what is left of the row: this is the model the
/// tests check that layout against.
#[cfg(test)]
pub(super) fn rows(available: f32, p: BarParts) -> usize {
    let (mut rows, mut x) = (1, 0.0);
    for w in left_items(p) {
        if x > 0.0 && available - x < w {
            (rows, x) = (rows + 1, 0.0);
        }
        x += w + p.spacing;
    }
    if available - x < group_width(p) { rows + 1 } else { rows }
}

/// In the bar's wrapped row, after the left part: `add` (the right part, `group_width` wide) drawn
/// right to left at the right end of the last row when it fits in what is left of it, else at the
/// right end of a row of its own.
pub(super) fn right_end(ui: &mut egui::Ui, p: BarParts, add: impl FnOnce(&mut egui::Ui)) {
    if ui.available_size_before_wrap().x < group_width(p) {
        ui.end_row();
    }
    let room = egui::vec2(ui.available_size_before_wrap().x, icons::BUTTON.y);
    ui.allocate_ui_with_layout(room, egui::Layout::right_to_left(egui::Align::Center), add);
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

    /// Just as wide as the bar: one row; half a point less: two, the tools on the first and the
    /// right part alone on the second.
    #[test]
    fn boundary() {
        let p = default_parts(70.0, false);
        let need = one_row_width(p);
        assert_eq!(rows(need, p), 1);
        assert_eq!(rows(need - 0.5, p), 2);
        // The tools alone on one row, and no room after them.
        assert_eq!(rows(need - group_width(p), p), 2);
    }

    /// When the tools wrap, the right part goes at the end of their second row if there is room
    /// there: two rows at 815 points (a 900 px window at 110 %), not three. The tools on two rows
    /// and their second row full: three.
    #[test]
    fn group_after_the_tools() {
        let p = default_parts(70.0, false);
        let tools = one_row_width(p) - group_width(p);
        for available in [815.0, 900.0, 700.0, tools - 1.0] {
            assert_eq!(rows(available, p), 2, "{available}");
        }
        // 12 items of 42 on the first row (504), 12 on the second (16 + 13 + 18 = 492 with the
        // separators): no room for the 202 of the right part.
        assert_eq!(rows(505.0, p), 3);
    }

    /// Through egui with the app's fonts, the real tools and a stand-in as wide as the right part,
    /// for every label the main save button can have in every language (Overwrite the original…
    /// as the bar shows it, Overwrite…), at 100 % and 110 %: the rows counted in the layout are
    /// those of `rows`; one in a 1200 px window at 100 %, two in a 900 or 820 px one, nothing past
    /// the right edge in a 700 px one.
    #[test]
    fn real_window() {
        use super::super::{Tool, Window};
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let labels = ["edit-save-copy", "edit-update-copy", "menu-save-copy", super::super::tools::bar_label("edit-overwrite")];
        let mut w = Window::new("p/a.png".into(), Default::default(), Tool::Select);
        let mut checked = 0;
        for (ppp, px, want) in [(1.0, 1200.0, Some(1)), (1.0, 900.0, Some(2)), (1.0, 820.0, Some(2)), (1.1, 900.0, Some(2)), (1.1, 820.0, Some(2)), (1.0, 700.0, None), (1.1, 700.0, None), (1.1, 1200.0, None)] {
            ctx.set_pixels_per_point(ppp);
            let width = px / ppp;
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))), ..Default::default() };
            for (lang, _) in crate::i18n::languages() {
                let i18n = I18n::new(lang);
                for id in labels {
                    let mut seen = (0.0, 0, egui::Rect::NOTHING, egui::Rect::NOTHING);
                    // The fonts are ready from the second pass.
                    for _ in 0..3 {
                        // Debug builds check that the texture changes were taken: nobody draws them here.
                        ctx.run_ui(input.clone(), |ui| {
                            egui::Panel::top("edit-tools").show(ui, |ui| {
                                let p = BarParts::measure(ui, i18n.t(id), false);
                                let available = ui.available_width();
                                let mut group = egui::Rect::NOTHING;
                                let bar = ui.horizontal_wrapped(|ui| {
                                    super::super::tools::tools_row(ui, &mut w, &i18n);
                                    right_end(ui, p, |ui| group = ui.allocate_exact_size(egui::vec2(group_width(p), icons::BUTTON.y), egui::Sense::hover()).0);
                                });
                                seen = (available, rows(available, p), bar.response.rect, group);
                            });
                        })
                        .textures_delta
                        .clear();
                    }
                    let (available, expected, bar, group) = seen;
                    let what = format!("{px} px at {ppp}, {lang} {id}");
                    let row = icons::BUTTON.y + ctx.global_style().spacing.item_spacing.y;
                    let counted = ((bar.height() + ctx.global_style().spacing.item_spacing.y) / row).round() as usize;
                    assert_eq!(counted, expected, "{what}: {bar:?}, {available} available");
                    if let Some(want) = want {
                        assert_eq!(counted, want, "{what}");
                    }
                    assert!(bar.right() <= width && group.right() <= width && group.left() >= bar.left(), "{what}: {bar:?} {group:?}");
                    assert!((group.bottom() - bar.bottom()).abs() < 0.5, "{what}: the right part on the last row");
                    checked += 1;
                }
            }
        }
        assert!(checked >= 8 * 2 * labels.len());
    }
}
