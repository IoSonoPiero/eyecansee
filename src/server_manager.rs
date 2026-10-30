//! File > Server manager: the saved servers on the left, their fields on the right (the same as
//! File > Open location, plus name, key file and passphrase); new, save, delete, connect.

use eframe::egui;

use crate::address::AddressForm;
use crate::{App, browser};

impl App {
    pub(crate) fn server_manager_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.server_manager.clone() else { return };
        // In a field Esc reaches the window; elsewhere it arrives as `delete_answer`.
        let esc = self.delete_answer.take() == Some(false) || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut close = esc && !f.deleting;
        f.deleting &= !esc;
        let mut connect = None;
        let before = f.servers.clone();
        egui::Modal::new(egui::Id::unique("server-manager")).show(ctx, |ui| {
            ui.set_width(700.0);
            crate::theme::title(ui, self.i18n.t("srv-title"));
            ui.add_space(4.0);
            self.master_section(ui, &mut f);
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| self.server_list(ui, &mut f));
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    egui::Grid::new("srv-fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| self.address_grid(ui, &mut f, true));
                });
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(self.i18n.t_args(if crate::servers::encrypted() { "srv-file-vault" } else { "srv-file" }, &[("path", &crate::servers::file())])).weak().small());
            if f.url.is_empty() && !f.port.trim().is_empty() && f.port.trim().parse::<u16>().is_err() {
                ui.label(browser::error_text(ui, self.i18n.t("addr-bad-port")));
            }
            if let Some(e) = &f.error {
                ui.label(browser::error_text(ui, self.i18n.t_args("srv-save-failed", &[("detail", e.as_str())])));
            }
            ui.add_space(8.0);
            if f.deleting {
                self.server_delete_row(ui, &mut f);
            } else {
                (close, connect) = self.server_buttons(ui, &mut f, close);
            }
        });
        // The first server saved without a master password: said once.
        if f.servers != before && !crate::servers::encrypted() && !self.settings.servers_plain_warned {
            self.settings.servers_plain_warned = true;
            self.settings.save();
            self.notice = Some((self.i18n.t("vault-plain-warning"), None));
        }
        self.saved_servers = f.servers.clone();
        self.server_manager = (!close).then_some(f);
        if let Some(address) = connect {
            self.server_manager = None;
            self.open_source(ctx, &address);
        }
    }

    /// The saved servers, the chosen one highlighted, and "New server" below.
    fn server_list(&self, ui: &mut egui::Ui, f: &mut AddressForm) {
        ui.set_width(190.0);
        egui::ScrollArea::vertical().id_salt("srv-list").max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
            if f.servers.is_empty() {
                ui.label(egui::RichText::new(self.i18n.t("srv-empty")).weak());
            }
            let mut chosen = f.chosen;
            for (i, s) in f.servers.iter().enumerate() {
                let icon = crate::icons::image(ui.ctx(), crate::icons::Icon::Server, 16.0);
                if ui.add(egui::Button::selectable(f.chosen == Some(i), (icon, s.name.as_str()))).clicked() {
                    chosen = Some(i);
                }
            }
            if chosen != f.chosen {
                f.choose(chosen);
            }
        });
        ui.add_space(6.0);
        if ui.button(self.i18n.t("srv-new")).clicked() {
            f.choose(None);
        }
    }

    /// Connect (saving first), Save, Delete, Close: whether to close, and the address to open.
    fn server_buttons(&self, ui: &mut egui::Ui, f: &mut AddressForm, mut close: bool) -> (bool, Option<String>) {
        let t = |id| self.i18n.t(id);
        let mut connect = None;
        let can_save = f.saved().is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(can_save, egui::Button::new(t("srv-connect"))).clicked() {
                f.store();
                connect = f.chosen.and_then(|i| f.servers.get(i)).and_then(|s| s.address()).filter(|_| f.error.is_none());
            }
            if ui.add_enabled(can_save, egui::Button::new(t("srv-save"))).clicked() {
                f.store();
            }
            if ui.add_enabled(f.chosen.is_some(), egui::Button::new(t("srv-delete"))).clicked() {
                f.deleting = true;
            }
            if ui.button(t("srv-close")).clicked() {
                close = true;
            }
        });
        (close, connect)
    }

    /// "Delete this server?" in place of the buttons.
    fn server_delete_row(&self, ui: &mut egui::Ui, f: &mut AddressForm) {
        let name = f.chosen.and_then(|i| f.servers.get(i)).map(|s| s.name.clone()).unwrap_or_default();
        ui.label(self.i18n.t_args("srv-delete-ask", &[("name", &name)]));
        ui.horizontal(|ui| {
            if ui.button(self.i18n.t("delete-yes")).clicked() {
                f.remove();
            }
            if ui.button(self.i18n.t("delete-no")).clicked() {
                f.deleting = false;
            }
        });
    }
}
