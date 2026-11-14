//! The end of the main toolbar: the address field, Copy and Paste of the address, and in the grid
//! sorting and the thumbnail slider.

use eframe::egui::{self, Key};

use crate::loader::THUMB_SIDE;
use crate::menu::address_id;
use crate::{App, icons, settings};

/// Below this width of the address bar the copy and paste buttons are left out.
const ADDRESS_WITH_CLIP: f32 = 420.0;
/// The room the two buttons take at the end of the address bar.
const CLIP_BUTTONS: f32 = 64.0;

impl App {
    /// The address field and what follows it, to the end of the toolbar's row.
    pub(crate) fn address_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Space to the right of the address: sorting (drop-down and direction) and thumbnail slider.
        let slider_width = if self.viewing.is_none() { 370.0 } else { 0.0 };
        // Copy and paste of the address at its end, when the bar has room for them.
        let room = ui.available_width() - slider_width;
        let clip = room > ADDRESS_WITH_CLIP;
        let address = egui::TextEdit::singleline(&mut self.address)
            .id(address_id())
            .hint_text(self.i18n.t("tb-address-hint"))
            .desired_width(if clip { room - CLIP_BUTTONS } else { room });
        let response = ui.add(address);
        if clip {
            self.address_clip(ui);
        }
        // The Enter is taken here: left to the grid drawn after, it opened the selected file
        // (an address that does not open left the viewer on it).
        if response.lost_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
            let address = crate::navigation::typed_address(&self.address);
            self.open_source(ctx, &address);
        }
        if self.viewing.is_none() {
            let (mut by, desc) = (self.settings.sort_by, self.settings.sort_desc);
            egui::ComboBox::from_id_salt("sort")
                .width(150.0)
                .selected_text(self.i18n.t(by.label()))
                .show_ui(ui, |ui| {
                    for b in settings::SortBy::ALL {
                        ui.selectable_value(&mut by, b, self.i18n.t(b.label()));
                    }
                })
                .response
                .on_hover_text(self.i18n.t("sort-by"));
            let arrow = icons::button(ui, if desc { icons::Line::SortDesc } else { icons::Line::SortAsc });
            let flip = ui.add(arrow).on_hover_text(self.i18n.t(if desc { "sort-desc" } else { "sort-asc" })).clicked();
            if by != self.settings.sort_by || flip {
                self.set_sort(by, desc != flip);
            }
            let hint = self.i18n.t("tb-thumb-size");
            ui.add(icons::line(ui, icons::Line::Image, 18.0)).on_hover_text(&hint);
            ui.add(egui::Slider::new(&mut self.thumb_size, 64.0..=THUMB_SIDE as f32).show_value(false))
                .on_hover_text(hint);
        }
    }

    /// Copy the address shown (never with a password), or paste one in its place: the field
    /// takes the keyboard and gets what the clipboard holds, ready for Enter.
    fn address_clip(&mut self, ui: &mut egui::Ui) {
        if ui.add(icons::small(ui, icons::Line::Copy)).on_hover_text(self.i18n.t("tb-address-copy")).clicked() {
            ui.ctx().copy_text(crate::address::without_password(&self.address));
        }
        if ui.add(icons::small(ui, icons::Line::Paste)).on_hover_text(self.i18n.t("tb-address-paste")).clicked() {
            self.address.clear();
            ui.ctx().memory_mut(|m| m.request_focus(address_id()));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
    }
}
