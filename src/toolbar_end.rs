//! The end of the main toolbar: the address field, Copy and Paste of the address, and in the grid
//! sorting and the thumbnail slider. What follows the field is summed from the widths it is drawn
//! with in the bar's style (button padding, item spacing, slider width), and the field takes what
//! is left of the row. An estimate before kept 64 points for Copy and Paste, which take 74: Paste
//! went past the right edge of the window in the viewer (Windows at 110 %, a 1400 px window).
//! When the field would be narrower than `FIELD_MIN`, what follows it goes on a row of its own,
//! at the right end: nothing is cut off or left out.

use eframe::egui::{self, Key};

use crate::i18n::I18n;
use crate::loader::THUMB_SIDE;
use crate::menu::address_id;
use crate::settings::SortBy;
use crate::{App, icons, ticked};
use crate::confirm::FollowList;

/// Width of the sort drop-down.
const SORT: f32 = 150.0;
/// Side of the picture before the thumbnail slider.
const THUMB_ICON: f32 = 18.0;
/// The thumbnail slider stops every 32 pixels from 64 to 256: seven sizes, 128 (the least with
/// the details under it) and 160 (the size at start) among them, as the few sizes of a file
/// manager's zoom (Dolphin, Nautilus) rather than a pixel at a time.
const THUMB_STEP: f64 = 32.0;
const THUMB_LEAST: f32 = 64.0;
/// Width of the size written after the slider ("256 px").
const THUMB_VALUE: f32 = 48.0;
/// The address field is never narrower than this with what follows it on its row: about 20
/// characters of a path. Below, what follows goes on the row under it.
pub(crate) const FIELD_MIN: f32 = 160.0;

/// The widths what follows the field is made of, in points.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TailParts {
    /// The room before an item.
    spacing: f32,
    /// Copy or Paste: a small button, its icon with the padding on both sides.
    clip: f32,
    /// The arrow of the sort direction: a square button.
    arrow: f32,
    /// The thumbnail slider, without the size written after it.
    slider: f32,
}

impl TailParts {
    /// From the style: button padding, item spacing, slider width.
    pub(crate) fn new(padding: f32, spacing: f32, slider: f32) -> Self {
        Self { spacing, clip: icons::SMALL_ICON + 2.0 * padding, arrow: icons::BUTTON.x.max(icons::ICON + 2.0 * padding), slider }
    }

    /// Measured in `ui`'s style.
    pub(crate) fn measure(ui: &egui::Ui) -> Self {
        let s = ui.spacing();
        Self::new(s.button_padding.x, s.item_spacing.x, s.slider_width)
    }
}

/// Copy and Paste, each with the room before it.
fn clip_width(p: TailParts) -> f32 {
    2.0 * (p.spacing + p.clip)
}

/// In the grid, after Copy and Paste: the sort drop-down and direction, the picture, the slider
/// and the size, each with the room before it.
pub(crate) fn sort_width(p: TailParts) -> f32 {
    2.0 * p.spacing + SORT + p.arrow + size_width(p)
}

/// The picture, the thumbnail slider and the size, each with the room before it.
fn size_width(p: TailParts) -> f32 {
    3.0 * p.spacing + THUMB_ICON + p.slider + THUMB_VALUE
}

/// All that follows the field, in the grid or in the viewer.
pub(crate) fn tail_width(p: TailParts, grid: bool) -> f32 {
    clip_width(p) + if grid { sort_width(p) } else { 0.0 }
}

/// The field's width in `available` points with `tail` after it on the same row, or None when it
/// would be narrower than `FIELD_MIN`: then the field takes the whole of `available` and what
/// follows goes on the row under it.
pub(crate) fn field_width(available: f32, tail: f32) -> Option<f32> {
    let width = available - tail;
    (width >= FIELD_MIN).then_some(width)
}

/// On a row of its own, the room before what follows the field so that it ends at the right
/// edge: its first item has no room before it there.
pub(crate) fn lead(available: f32, p: TailParts, tail: f32) -> f32 {
    (available - tail + p.spacing).max(0.0)
}

/// The least width of the thumbnail slider, and of the sort drop-down ("Data di modifica" whole).
const SLIDER_LEAST: f32 = 40.0;
const SORT_LEAST: f32 = 128.0;

/// How what follows the field goes under it, on a row of its own narrower than it (a 400 px
/// window): it went past the right edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Under {
    /// On one row, with the thumbnail slider and the sort drop-down this wide: the slider is
    /// shortened by what is missing, to `SLIDER_LEAST`, then the drop-down, to `SORT_LEAST`.
    One { slider: f32, sort: f32 },
    /// Still too narrow (a 400 px window at 110 %): the picture, the slider and the size on a
    /// third row.
    Two,
}

pub(crate) fn under(available: f32, p: TailParts, tail: f32) -> Under {
    let missing = (tail - p.spacing - available).max(0.0);
    let slider = (p.slider - missing).max(SLIDER_LEAST.min(p.slider));
    let sort = SORT - (missing - (p.slider - slider));
    if sort < SORT_LEAST - 0.01 {
        return Under::Two;
    }
    Under::One { slider, sort: sort.min(SORT) }
}

impl App {
    /// The address field, to the end of the toolbar's row, with what follows it when the field
    /// keeps `FIELD_MIN`. Otherwise returns the width of what follows, for `address_tail_row`.
    pub(crate) fn address_field(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> Option<f32> {
        let tail = tail_width(TailParts::measure(ui), self.viewing.is_none());
        let available = ui.available_width();
        let width = field_width(available, tail);
        let address = egui::TextEdit::singleline(&mut self.address)
            .id(address_id())
            .hint_text(self.i18n.t("tb-address-hint"))
            .desired_width(width.unwrap_or(available));
        let response = ui.add(address);
        // The Enter is taken here: left to the grid drawn after, it opened the selected file
        // (an address that does not open left the viewer on it).
        if response.lost_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
            let address = crate::navigation::typed_address(&self.address);
            self.open_source(ctx, &address);
        }
        if width.is_none() {
            return Some(tail);
        }
        self.address_tail(ui, SORT, true);
        None
    }

    /// Under the toolbar's row, what follows the field (`tail` wide), ending at the right edge.
    pub(crate) fn address_tail_row(&mut self, ui: &mut egui::Ui, tail: f32) {
        let (available, p) = (ui.available_width(), TailParts::measure(ui));
        match under(available, p, tail) {
            Under::One { slider, sort } => _ = ui.horizontal(|ui| {
                ui.add_space(lead(available, p, tail));
                ui.spacing_mut().slider_width = slider;
                self.address_tail(ui, sort, true);
            }),
            Under::Two => {
                ui.horizontal(|ui| {
                    ui.add_space(lead(available, p, tail - size_width(p)));
                    self.address_tail(ui, SORT, false);
                });
                ui.horizontal(|ui| {
                    ui.add_space(lead(available, p, size_width(p)));
                    size_slider(ui, &self.i18n, &mut self.thumb_size);
                });
            }
        }
    }

    /// Copy the address shown (never with a password), or paste one in its place: the field
    /// takes the keyboard and gets what the clipboard holds, ready for Enter. Then, in the grid,
    /// sorting (`sort` wide) and, with `size`, the size of the thumbnails.
    fn address_tail(&mut self, ui: &mut egui::Ui, sort: f32, size: bool) {
        let (copy, paste) = clip_buttons(ui, &self.i18n);
        if copy {
            ui.ctx().copy_text(crate::address::without_password(&self.address));
        }
        if paste {
            self.address.clear();
            ui.ctx().memory_mut(|m| m.request_focus(address_id()));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        if self.viewing.is_none() {
            let desc = self.settings.sort_desc;
            let (by, flip) = sort_buttons(ui, &self.i18n, (self.settings.sort_by, sort), desc);
            if by != self.settings.sort_by || flip {
                self.set_sort(by, desc != flip);
            }
            if size {
                size_slider(ui, &self.i18n, &mut self.thumb_size);
            }
        }
    }
}

/// Copy and Paste of the address: whether each was clicked.
fn clip_buttons(ui: &mut egui::Ui, i18n: &I18n) -> (bool, bool) {
    let copy = ui.add(icons::small(ui, icons::Line::Copy)).on_hover_text(i18n.t("tb-address-copy")).clicked();
    let paste = ui.add(icons::small(ui, icons::Line::Paste)).on_hover_text(i18n.t("tb-address-paste")).clicked();
    (copy, paste)
}

/// The sort drop-down (`width` wide) and direction: the order chosen and whether the direction
/// was clicked.
fn sort_buttons(ui: &mut egui::Ui, i18n: &I18n, (mut by, width): (SortBy, f32), desc: bool) -> (SortBy, bool) {
    egui::ComboBox::from_id_salt("sort")
        .width(width)
        .selected_text(i18n.t(by.label()))
        .show_list(ui, |ui| {
            for b in SortBy::ALL {
                ui.selectable_value(&mut by, b, i18n.t(b.label()));
            }
        })
        .response
        .on_hover_text(i18n.t("sort-by"));
    let arrow = icons::button(ui, if desc { icons::Line::SortDesc } else { icons::Line::SortAsc });
    let flip = ui.add(arrow).on_hover_text(i18n.t(if desc { "sort-desc" } else { "sort-asc" })).clicked();
    (by, flip)
}

/// The size of the thumbnails: a picture, the slider by steps with its marks, the size in pixels.
fn size_slider(ui: &mut egui::Ui, i18n: &I18n, thumb_size: &mut f32) {
    let hint = i18n.t("tb-thumb-size");
    ui.add(icons::line(ui, icons::Line::Image, THUMB_ICON)).on_hover_text(&hint);
    let range = THUMB_LEAST..=THUMB_SIDE as f32;
    // A size kept from before (not on a step) goes to the nearest one.
    *thumb_size = ticked::snap(f64::from(*thumb_size), f64::from(THUMB_LEAST)..=f64::from(THUMB_SIDE), THUMB_STEP) as f32;
    ticked::add(ui, egui::Slider::new(thumb_size, range.clone()).show_value(false), &ticked::Marks::new(&range, Some(THUMB_STEP))).on_hover_text(&hint);
    let size = egui::Label::new(format!("{} px", *thumb_size as u32));
    ui.add_sized([THUMB_VALUE, ui.spacing().interact_size.y], size).on_hover_text(hint);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// egui's default style: padding and spacing 8, slider 100.
    fn default_parts() -> TailParts {
        TailParts::new(8.0, 8.0, 100.0)
    }

    /// The sum, part by part: Copy and Paste 29 wide each (13 + 8 + 8), not the 64 points kept
    /// for both before; in the grid the drop-down 150, the arrow 34, the picture 18, the slider
    /// 100, its size 48; 8 before each item.
    #[test]
    fn width_part_by_part() {
        let p = default_parts();
        assert_eq!(tail_width(p, false), 8.0 + 29.0 + 8.0 + 29.0);
        assert_eq!(tail_width(p, true), 74.0 + 8.0 + 150.0 + 8.0 + 34.0 + 8.0 + 18.0 + 8.0 + 100.0 + 8.0 + 48.0);
        // A larger padding widens the buttons; the arrow is never narrower than a square button.
        assert_eq!(tail_width(TailParts::new(10.0, 8.0, 100.0), false), 2.0 * (8.0 + 33.0));
        assert_eq!(TailParts::new(2.0, 8.0, 100.0).arrow, icons::BUTTON.x);
    }

    /// Just room for the field at its least width: one row; half a point less: what follows goes
    /// under it. On its own row it ends at the right edge.
    #[test]
    fn boundary() {
        let p = default_parts();
        let tail = tail_width(p, true);
        assert_eq!(field_width(tail + FIELD_MIN, tail), Some(FIELD_MIN));
        assert_eq!(field_width(tail + FIELD_MIN - 0.5, tail), None);
        assert_eq!(field_width(1000.0, tail), Some(1000.0 - tail));
        // 7 items: the room before the first is not there, the others' is in `tail`.
        let available = 600.0;
        assert_eq!(lead(available, p, tail) + tail - p.spacing, available);
        assert_eq!(lead(100.0, p, tail), 0.0);
        // Room enough: as they are; 24 points short: the slider as much shorter; 72 short (a
        // 400 px window): the slider at its least, the drop-down 12 shorter; shorter still: the
        // thumbnail size on a row of its own.
        assert_eq!(under(available, p, tail), Under::One { slider: 100.0, sort: SORT });
        assert_eq!(under(tail - p.spacing - 24.0, p, tail), Under::One { slider: 76.0, sort: SORT });
        assert_eq!(under(tail - p.spacing - 72.0, p, tail), Under::One { slider: SLIDER_LEAST, sort: SORT - 12.0 });
        assert_eq!(under(tail - p.spacing - 82.0, p, tail), Under::One { slider: SLIDER_LEAST, sort: SORT_LEAST });
        assert_eq!(under(tail - p.spacing - 83.0, p, tail), Under::Two);
    }

    /// Through egui with the app's fonts and the real Copy, Paste, sorting and slider, after a
    /// stand-in as wide as the bar's left part, at 100 % and 110 %, in the viewer and in the grid,
    /// from a 1400 px window down to 400 px: nothing past the right edge, the last item at the
    /// right edge (on the third row too, in a 400 px window at 110 %), the field at least `FIELD_MIN` wide when what follows is on its row, which it
    /// is in a 1400 px window at 110 % (warmachinew).
    #[test]
    fn real_window() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let i18n = I18n::new("it");
        let mut checked = 0;
        for ppp in [1.0, 1.1] {
            ctx.set_pixels_per_point(ppp);
            for px in [1400.0, 1300.0, 1200.0, 1100.0, 1000.0, 900.0, 800.0, 700.0, 600.0, 520.0, 450.0, 400.0] {
                // About the left part with Image and Tools in "More": the grid's and the viewer's.
                for (grid, left) in [(true, 470.0_f32), (false, 380.0)] {
                    // In a narrow window more of the left part is in "More" (`toolbar::fit`): it
                    // leaves the field its least width.
                    let left = left.min(px / ppp - 16.0 - FIELD_MIN - 16.0);
                    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(px / ppp, 800.0))), ..Default::default() };
                    let mut seen = (egui::Rect::NOTHING, 0.0, 0.0, false);
                    // The fonts are ready from the second pass.
                    for _ in 0..3 {
                        // Debug builds check that the texture changes were taken: nobody draws them here.
                        ctx.run_ui(input.clone(), |ui| {
                            egui::Panel::top("toolbar").show(ui, |ui| seen = draw(ui, &i18n, grid, left));
                        })
                        .textures_delta
                        .clear();
                    }
                    let (field, end, right, one_row) = seen;
                    let what = format!("{px} px at {ppp}, grid {grid}");
                    assert!((end - right).abs() < 0.01, "{what}: ends at {end} for {right}");
                    if one_row {
                        assert!(field.width() >= FIELD_MIN - 0.01, "{what}: field {field:?}");
                    } else {
                        assert!((field.right() - right).abs() < 0.01, "{what}: field {field:?}");
                    }
                    if px == 1400.0 {
                        assert!(one_row, "{what}");
                    }
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 48);
    }

    /// The toolbar's end as `address_field` and `address_tail_row` draw it, after `left` points:
    /// the field, where the last item ends, the right edge, whether all is on one row.
    fn draw(ui: &mut egui::Ui, i18n: &I18n, grid: bool, left: f32) -> (egui::Rect, f32, f32, bool) {
        let right = ui.max_rect().right();
        let (mut field, mut end, mut below) = (egui::Rect::NOTHING, 0.0, None);
        let tail_items = |ui: &mut egui::Ui, sort: f32, size: bool| {
            clip_buttons(ui, i18n);
            if grid {
                sort_buttons(ui, i18n, (SortBy::Name, sort), false);
                if size {
                    size_slider(ui, i18n, &mut 128.0);
                }
            }
            ui.min_rect().right()
        };
        ui.horizontal(|ui| {
            ui.allocate_exact_size(egui::vec2(left, icons::BUTTON.y), egui::Sense::hover());
            let tail = tail_width(TailParts::measure(ui), grid);
            let available = ui.available_width();
            let width = field_width(available, tail);
            field = ui.add(egui::TextEdit::singleline(&mut String::from("/home/demo/Immagini")).desired_width(width.unwrap_or(available))).rect;
            match width {
                Some(_) => end = tail_items(ui, SORT, true),
                None => below = Some(tail),
            }
        });
        if let Some(tail) = below {
            let (available, p) = (ui.available_width(), TailParts::measure(ui));
            match under(available, p, tail) {
                Under::One { slider, sort } => _ = ui.horizontal(|ui| {
                    ui.add_space(lead(available, p, tail));
                    ui.spacing_mut().slider_width = slider;
                    end = tail_items(ui, sort, true);
                }),
                Under::Two => {
                    let second = ui.horizontal(|ui| {
                        ui.add_space(lead(available, p, tail - size_width(p)));
                        tail_items(ui, SORT, false)
                    });
                    assert!((second.inner - right).abs() < 0.01, "second row ends at {}", second.inner);
                    ui.horizontal(|ui| {
                        ui.add_space(lead(available, p, size_width(p)));
                        size_slider(ui, i18n, &mut 128.0);
                        end = ui.min_rect().right();
                    });
                }
            }
        }
        (field, end, right, below.is_none())
    }
}
