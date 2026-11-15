//! The zoom in the status bars of the viewer and of the editor (issue #152), as in XnView MP and
//! Windows Photos: the percentage, which a click turns into a field to type another (Enter sets
//! it, Esc or a click elsewhere leaves it as it was), and beside it a slider on a logarithmic
//! scale with marks on round zooms (`ticked`). Both ask for a zoom: the viewer reaches it in the
//! same pass (`zoom::Bar::ask`), as the status bar is drawn before it, with the limits of the last
//! pass; the editor at once. The logic is in `zoom`.

use eframe::egui;

use crate::i18n::I18n;
use crate::zoom::{self, Ask};
use crate::App;

/// The slider's length, in points: room for a mark every 6 points over the zooms of the Options.
const SLIDER: f32 = 120.0;
/// The field's width, in points: "10000%" and a little more; the percentage takes as much, so
/// that the slider does not move when the field opens.
const FIELD: f32 = 52.0;

/// The field open: what is typed, and whether it has just opened (focus and all selected).
#[derive(Clone, Default)]
struct Typing {
    text: String,
    new: bool,
}

impl App {
    /// The viewer's zoom group, at the right of its status bar beside Keys: the zoom asked, of
    /// the file's pixels.
    pub(crate) fn zoom_group(&self, ui: &mut egui::Ui) -> Option<Ask> {
        group(ui, &self.i18n, "viewer-zoom", self.zoom_shown, self.zoom_bar.range)
    }
}

/// −, the percentage, the slider and + (#152), as in Paint, Paint.NET, Windows Photos and Word:
/// at the right of a status bar drawn from the right, beside Keys.
pub(crate) fn group(ui: &mut egui::Ui, i18n: &I18n, salt: &str, shown: f32, range: [f32; 2]) -> Option<Ask> {
    let ctx = ui.ctx().clone();
    let white = |icon| egui::Button::image(crate::icons::line_in(&ctx, icon, 16.0, egui::Color32::WHITE)).frame(false);
    let hint = |id: &str, key: &str| format!("{} ({key})", i18n.t(id));
    let zoom_in = ui.add(white(crate::icons::Line::ZoomIn)).on_hover_text(hint("edit-zoom-in", "+")).clicked();
    let asked = controls(ui, i18n, salt, shown, range).map(Ask::To);
    let zoom_out = ui.add(white(crate::icons::Line::ZoomOut)).on_hover_text(hint("edit-zoom-out", "−")).clicked();
    match () {
        _ if zoom_in => Some(Ask::In),
        _ if zoom_out => Some(Ask::Out),
        _ => asked,
    }
}

/// The percentage of `shown` (1 = 100%) and the slider from `range[0]` to `range[1]`, kept
/// together on a row of the bar (`bar_rows`), in reading order also in a bar drawn from the
/// right: the zoom asked by either. `salt` tells the field of one bar from another's.
fn controls(ui: &mut egui::Ui, i18n: &I18n, salt: &str, shown: f32, range: [f32; 2]) -> Option<f32> {
    ui.horizontal(|ui| {
        if ui.layout().prefer_right_to_left() {
            let slid = slider(ui, i18n, shown, range);
            field(ui, i18n, salt, shown).or(slid)
        } else {
            let typed = field(ui, i18n, salt, shown);
            typed.or(slider(ui, i18n, shown, range))
        }
    })
    .inner
}

/// The field of `salt` is taking the keys: Esc is its, not the page's.
pub(crate) fn typing(ctx: &egui::Context, salt: &str) -> bool {
    ctx.memory(|m| m.has_focus(field_id(salt).with("edit")))
}

fn field_id(salt: &str) -> egui::Id {
    egui::Id::unique(("zoom-field", salt))
}

/// The percentage, or the field it turns into when clicked.
fn field(ui: &mut egui::Ui, i18n: &I18n, salt: &str, zoom: f32) -> Option<f32> {
    let id = field_id(salt);
    let shown = zoom::percent_text(zoom, &i18n.t("decimal-separator"));
    let Some(mut typing) = ui.data(|d| d.get_temp::<Typing>(id)) else {
        let r = ui.add(egui::Button::new(&shown).frame(false).min_size(egui::vec2(FIELD, 0.0))).on_hover_cursor(egui::CursorIcon::Text).on_hover_text(i18n.t("zoom-field-hint"));
        if r.clicked() {
            let text = shown.trim_end_matches('%').to_string();
            ui.data_mut(|d| d.insert_temp(id, Typing { text, new: true }));
            ui.ctx().request_repaint();
        }
        return None;
    };
    // The field on the theme's background, its text in the theme's colour: the bar makes
    // every text white, and it was white on white in the light theme.
    let colour = ui.visuals().widgets.inactive.text_color();
    let filter = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, escape: true, ..Default::default() };
    let edit = egui::TextEdit::singleline(&mut typing.text).id(id.with("edit")).desired_width(FIELD).char_limit(9).text_color(colour);
    let mut out = edit.event_filter(filter).show(ui);
    let r = out.response.response.clone().on_hover_text(i18n.t("zoom-field-hint"));
    if std::mem::take(&mut typing.new) {
        // Just opened: the field takes the keys, its text all selected to be typed over.
        r.request_focus();
        let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(typing.text.chars().count()));
        out.state.cursor.set_char_range(Some(all));
        out.state.store(ui.ctx(), r.id);
        ui.data_mut(|d| d.insert_temp(id, typing));
        return None;
    }
    // Enter ends the typing (the field lets the focus go), Esc is kept by the field and taken
    // here, so that it does not also close the viewer or the editor.
    let esc = r.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if r.has_focus() && !esc {
        ui.data_mut(|d| d.insert_temp(id, typing));
        return None;
    }
    // Enter sets what was typed; Esc, a click elsewhere or Tab leave the zoom as it was.
    r.surrender_focus();
    ui.data_mut(|d| d.remove::<Typing>(id));
    if enter { zoom::typed(&typing.text) } else { None }
}

/// The slider from the smallest zoom to the largest, at `zoom`.
fn slider(ui: &mut egui::Ui, i18n: &I18n, zoom: f32, range: [f32; 2]) -> Option<f32> {
    if range[1] <= range[0] {
        return None;
    }
    let mut at = zoom::to_slider(zoom, range);
    ui.spacing_mut().slider_width = SLIDER;
    // Room for the marks under the rail: in a row of the bar as tall as the text they were cut.
    let thick = ui.text_style_height(&egui::TextStyle::Body).max(ui.spacing().interact_size.y);
    let slider = egui::Slider::new(&mut at, 0.0..=1.0).show_value(false);
    let r = ui.add_sized([SLIDER, thick + 6.0], slider).on_hover_text(i18n.t("zoom-slider-hint"));
    let marks: Vec<f64> = zoom::slider_marks(range).into_iter().map(|z| f64::from(zoom::to_slider(z, range))).collect();
    crate::ticked::paint(ui, &r, 0.0..=1.0, &marks);
    // The arrows change the image (or move what is selected): a focused slider would move as well.
    if r.has_focus() {
        r.surrender_focus();
    }
    r.changed().then(|| zoom::from_slider(at, range))
}
