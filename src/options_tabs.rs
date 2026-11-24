//! The column of tabs at the left of the Options window: a card as those of the first-start
//! window, a row per tab with its icon and its name, the open one lit as a chosen card. The
//! add-ons are not settings: their row comes last, under a line.

use eframe::egui;

use crate::OptTab;
use crate::i18n::I18n;
use crate::icons::Line;

/// Width of the column, its edge included: the names of the settings fit on one line beside
/// their icons. A longer name (the add-ons in Italian) goes on two lines beside its icon, and
/// the column stays narrow: sized for the longest name it was wide and mostly empty.
pub(crate) const WIDTH: f32 = 150.0;
/// Inside the card's edge, around the rows; with the edge, on both sides.
const MARGIN: f32 = 6.0;
const EDGE: f32 = 2.0 * (MARGIN + 1.0);
/// Height of a row with a name on one line, the space between two rows, and the air above and
/// under a name on two lines.
const ROW: f32 = 28.0;
const GAP: f32 = 1.0;
const AIR: f32 = 4.0;
/// Height of the line before the add-ons, with its air.
const RULE: f32 = 14.0;
/// Side of a row's icon; the space at its sides, and after the name.
const ICON: f32 = 16.0;
const PAD: f32 = 8.0;

/// The tabs in their order, each with the id of its name and its icon. General first, upkeep
/// (the thumbnail cache) last; between them the order of a visit: look, watch, edit, delete
/// (Microsoft's guidelines for options windows). Then the add-ons.
const TABS: [(OptTab, &str, Line); 11] = [
    (OptTab::General, "opt-tab-general", Line::Settings),
    (OptTab::Look, "opt-tab-look", Line::Palette),
    (OptTab::Viewer, "opt-tab-viewer", Line::Image),
    (OptTab::Slideshow, "opt-tab-slideshow", Line::Slideshow),
    (OptTab::Video, "opt-tab-video", Line::Play),
    (OptTab::Editor, "opt-tab-editor", Line::Edit),
    (OptTab::Resize, "opt-tab-resize", Line::Batch),
    (OptTab::Deleting, "opt-tab-deleting", Line::Delete),
    (OptTab::Thumbnails, "opt-tab-thumbnails", Line::Grid),
    (OptTab::Diagnostics, "opt-tab-diagnostics", Line::Info),
    (OptTab::Addons, "opt-tab-addons", Line::Puzzle),
];

/// The column, as tall as the window (`height`): a click on a row opens its tab.
pub(crate) fn column(ui: &mut egui::Ui, open: &mut OptTab, i18n: &I18n, height: f32) {
    let card = crate::theme::card(ui.visuals()).inner_margin(MARGIN);
    // The window lays its parts in a row: the rows of the column go one under the other.
    ui.vertical(|ui| {
        card.show(ui, |ui| {
            ui.set_width(WIDTH - EDGE);
            ui.set_min_height(height - EDGE);
            ui.spacing_mut().item_spacing.y = GAP;
            for (tab, id, icon) in TABS {
                if tab == OptTab::Addons {
                    rule(ui);
                }
                if row(ui, *open == tab, &i18n.t(id), icon) {
                    *open = tab;
                }
            }
        })
    });
}

/// A thin line across the column, with air above and under it.
fn rule(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), RULE), egui::Sense::hover());
    ui.painter().hline(rect.shrink2(egui::vec2(PAD, 0.0)).x_range(), rect.center().y, ui.visuals().widgets.noninteractive.bg_stroke);
}

/// A tab: its icon and its name, on two lines when one is too short for it. True when clicked
/// (Space and Enter too, with the keyboard on it). Lit as a choice of the first-start window.
fn row(ui: &mut egui::Ui, selected: bool, name: &str, icon: Line) -> bool {
    let (width, v) = (ui.available_width(), ui.visuals().clone());
    let ink = if selected { v.strong_text_color() } else { v.text_color() };
    let room = width - (PAD + ICON + PAD) - PAD;
    let galley = egui::WidgetText::from(egui::RichText::new(name).color(ink)).into_galley(ui, Some(egui::TextWrapMode::Wrap), room, egui::TextStyle::Body);
    let height = ROW.max(galley.size().y + 2.0 * AIR);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, name));
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let accent = crate::theme::accent(&v);
    if selected {
        ui.painter().rect(rect, 6, crate::theme::tile_selected(&v), egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    } else if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 6, v.widgets.hovered.weak_bg_fill);
    }
    let at = egui::Rect::from_center_size(egui::pos2(rect.left() + PAD + ICON / 2.0, rect.center().y), egui::Vec2::splat(ICON));
    crate::icons::line_in(ui.ctx(), icon, ICON, if selected { accent } else { v.text_color() }).paint_at(ui, at);
    ui.painter().galley(egui::pos2(at.right() + PAD, rect.center().y - galley.size().y / 2.0), galley, ink);
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons_ui::tests::{click, frame, headless};
    use crate::app_dialogs::OPTIONS_HEIGHT as HEIGHT;

    /// The middle of row `i` of the column drawn at `area`, names on one line; the add-ons (10)
    /// are under the line.
    fn middle(area: egui::Rect, i: usize) -> egui::Pos2 {
        let line = if i == 10 { RULE + GAP } else { 0.0 };
        area.min + egui::vec2(WIDTH / 2.0, EDGE / 2.0 + i as f32 * (ROW + GAP) + line + ROW / 2.0)
    }

    /// Every tab has its row, once, General first and the add-ons last.
    #[test]
    fn a_row_for_every_tab() {
        let tabs: Vec<_> = TABS.iter().map(|t| t.0).collect();
        for (i, tab) in tabs.iter().enumerate() {
            assert!(!tabs[..i].contains(tab), "{tab:?} twice");
        }
        assert_eq!((tabs[0], tabs[10]), (OptTab::General, OptTab::Addons));
    }

    /// In every language the column is as wide and as tall as it is meant to be: no name is cut,
    /// and none goes on a line more than the window has room for (the settings on one line, the
    /// add-ons on two at most).
    #[test]
    fn the_names_fit_in_every_language() {
        let (ctx, _) = headless();
        for (lang, _) in crate::i18n::languages() {
            let i18n = I18n::new(lang);
            let mut size = egui::Vec2::ZERO;
            frame(&ctx, Vec::new(), |ui| size = ui.scope(|ui| column(ui, &mut OptTab::General, &i18n, HEIGHT)).response.rect.size());
            assert_eq!(size, egui::vec2(WIDTH, HEIGHT), "{lang}");
        }
    }

    /// A click on a row opens its tab; one on the line before the add-ons opens nothing.
    #[test]
    fn a_click_on_a_row_opens_its_tab() {
        let (ctx, area) = headless();
        let i18n = I18n::new("en");
        let mut open = OptTab::General;
        for (i, wanted) in [(2, OptTab::Viewer), (10, OptTab::Addons), (9, OptTab::Diagnostics), (6, OptTab::Resize), (0, OptTab::General)] {
            click(&ctx, middle(area, i), |ui| column(ui, &mut open, &i18n, HEIGHT));
            assert_eq!(open, wanted);
        }
        click(&ctx, middle(area, 10) - egui::vec2(0.0, (ROW + RULE) / 2.0), |ui| column(ui, &mut open, &i18n, HEIGHT));
        assert_eq!(open, OptTab::General, "a click on the line");
    }

    /// With the keyboard as the buttons the column had: Tab goes from a row to the next, Enter
    /// and Space open the tab of the row it is on.
    #[test]
    fn enter_and_space_open_the_row_with_the_keyboard() {
        let (ctx, _) = headless();
        let i18n = I18n::new("en");
        let mut open = OptTab::General;
        let key = |key| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
        let press = |k, open: &mut OptTab| {
            for events in [vec![key(k)], vec![]] {
                frame(&ctx, events, |ui| column(ui, open, &i18n, HEIGHT));
            }
        };
        press(egui::Key::Tab, &mut open);
        press(egui::Key::Tab, &mut open);
        press(egui::Key::Enter, &mut open);
        assert_eq!(open, OptTab::Look);
        press(egui::Key::Tab, &mut open);
        press(egui::Key::Space, &mut open);
        assert_eq!(open, OptTab::Viewer);
    }

    /// #311: a row reached with Tab has a ring as strong as the selected one's, also in the light
    /// theme: the pale fill of hovering, alone, could hardly be seen on white (Piero, 24/11).
    #[test]
    fn the_row_of_the_keyboard_has_a_ring() {
        let (ctx, _) = headless();
        ctx.set_visuals(egui::Visuals::light());
        let mut id = None;
        let run = |ctx: &egui::Context, id: &mut Option<egui::Id>| {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                *id = Some(ui.next_auto_id());
                row(ui, false, "Visore", Line::Settings);
            });
            out.textures_delta.clear();
            out
        };
        run(&ctx, &mut id);
        ctx.memory_mut(|m| m.request_focus(id.unwrap()));
        run(&ctx, &mut id);
        let out = run(&ctx, &mut id);
        let ring = out.shapes.iter().any(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.stroke.width >= 2.0 && r.stroke.color == crate::theme::accent(&egui::Visuals::light())));
        assert!(ring, "the row with the keyboard has a ring of the accent colour");
    }
}
