//! File > Server manager: the saved servers on the left, their fields on the right (the same as
//! File > Open location, plus name, key file and passphrase); new, save, delete, connect.

use eframe::egui;

use crate::address::AddressForm;
use crate::vault_page::Change;
use crate::App;

impl App {
    pub(crate) fn server_manager_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.server_manager.clone() else { return };
        // In a field Esc reaches the window; elsewhere it arrives as `answer`.
        let id = "server-manager";
        let esc = crate::confirm::answer(ctx, &mut self.key_answer, id) == Some(false) || ctx.input(|i| i.key_pressed(egui::Key::Escape));
        // Esc on a question cancels the question only.
        let asked = f.deleting || f.master == Change::Remove;
        let mut close = esc && !asked;
        if esc && asked {
            (f.deleting, f.master) = (false, if f.master == Change::Remove { Change::None } else { f.master.clone() });
        }
        let mut connect = None;
        let before = f.servers.clone();
        egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
            ui.set_width(700.0);
            crate::theme::title(ui, self.i18n.t("srv-title"));
            ui.add_space(4.0);
            self.master_section(ui, &mut f);
            ui.add_space(8.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| self.server_list(ui, &mut f));
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    if let Some((_, problem)) = f.key_problem.as_ref().filter(|(i, _)| f.chosen == Some(*i)) {
                        crate::error_bar::inline(ui, problem);
                        ui.add_space(6.0);
                    }
                    egui::Grid::new("srv-fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| self.address_grid(ui, &mut f, true));
                });
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(self.i18n.t_args(if crate::servers::encrypted() { "srv-file-vault" } else { "srv-file" }, &[("path", &crate::servers::file())])).weak().small());
            if f.url.is_empty() && !f.port.trim().is_empty() && f.port.trim().parse::<u16>().is_err() {
                crate::error_bar::hint(ui, &self.i18n.t("addr-bad-port"));
            }
            if let Some(e) = &f.error {
                crate::error_bar::inline(ui, &crate::error_bar::with_detail(&self.i18n.t_args("srv-save-failed", &[("detail", e.as_str())]), e));
            }
            ui.add_space(8.0);
            if f.deleting {
                self.server_delete_row(ui, &mut f);
            } else {
                (close, connect) = self.server_buttons(ui, &mut f, close);
            }
        });
        if f.servers != before {
            self.plain_servers_notice();
        }
        self.saved_servers = f.servers.clone();
        self.server_manager = (!close).then_some(f);
        if let Some(address) = connect {
            self.server_manager = None;
            self.open_source(ctx, &address);
        }
    }

    /// The servers just saved without a master password, the first time: "Servers saved in plain
    /// text", said once (Server manager and Open location's "Save and open").
    pub(crate) fn plain_servers_notice(&mut self) {
        if crate::servers::encrypted() || self.settings.servers_plain_warned {
            return;
        }
        self.settings.servers_plain_warned = true;
        self.settings.save();
        let (title, text) = (self.i18n.t("vault-plain-title"), self.i18n.t("vault-plain-warning"));
        self.notice = Some(crate::save_dialogs::Notice { warning: true, ..crate::save_dialogs::Notice::new(title, text) });
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
                log!("server manager: save and connect");
                f.store();
                connect = f.chosen.and_then(|i| f.servers.get(i)).and_then(|s| s.address()).filter(|_| f.error.is_none());
            }
            if ui.add_enabled(can_save, egui::Button::new(t("srv-save"))).clicked() {
                log!("server manager: save");
                f.store();
            }
            if ui.add_enabled(f.chosen.is_some(), egui::Button::new(t("srv-delete"))).clicked() {
                // One question at a time: they share the keys of the window.
                f.deleting = true;
                if f.master == Change::Remove {
                    f.master = Change::None;
                }
            }
            if ui.button(t("srv-close")).clicked() {
                log!("server manager: close");
                close = true;
            }
        });
        (close, connect)
    }

    /// "Delete this server?" in place of the buttons, on Cancel: it cannot be undone (#311).
    fn server_delete_row(&self, ui: &mut egui::Ui, f: &mut AddressForm) {
        let name = f.chosen.and_then(|i| f.servers.get(i)).map(|s| s.name.clone()).unwrap_or_default();
        ui.label(self.i18n.t_args("srv-delete-ask", &[("name", &name)]));
        let labels = [(self.i18n.t("delete-yes"), true), (self.i18n.t("delete-no"), true)];
        match ui.horizontal(|ui| crate::confirm::buttons(ui, "server-manager", &labels, 1)).inner {
            Some(0) => f.remove(),
            Some(_) => f.deleting = false,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Key;

    use crate::address::AddressForm;
    use crate::audit_family_1::{Harness, folder, own_folder};
    use crate::vault_page::Change;

    /// The server manager open on one saved server.
    fn manager(name: &str) -> Harness {
        let mut h = Harness::new(&folder(name, &["a.jpg"]));
        let mut f = AddressForm::manager();
        f.servers = vec![crate::servers::Saved { name: "nas".into(), scheme: "ftp".into(), host: "nas.invalid".into(), ..Default::default() }];
        f.choose(Some(0));
        h.app.server_manager = Some(f);
        h.frame(Vec::new());
        h
    }

    fn form(h: &Harness) -> &AddressForm {
        h.app.server_manager.as_ref().expect("the server manager closed")
    }

    /// The question in place of the buttons, as from its button (which had the keyboard).
    fn ask(h: &mut Harness, question: impl Fn(&mut AddressForm)) {
        h.focus_somewhere();
        question(h.app.server_manager.as_mut().unwrap());
        h.frame(Vec::new());
    }

    /// #311: "Delete this server?" opens on Cancel: Enter keeps the server; Tab goes to Delete,
    /// Enter deletes it. Esc cancels the question, the window stays.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn deleting_a_server_opens_on_cancel() {
        if own_folder("server_manager::tests::deleting_a_server_opens_on_cancel") {
            return;
        }
        let mut h = manager("srv-delete");
        ask(&mut h, |f| f.deleting = true);
        h.key(Key::Enter);
        assert!(!form(&h).deleting && form(&h).servers.len() == 1, "Enter on the question deleted the server, or left the question");
        ask(&mut h, |f| f.deleting = true);
        h.key(Key::Escape);
        assert!(!form(&h).deleting && form(&h).servers.len() == 1, "Esc did not cancel the question");
        ask(&mut h, |f| f.deleting = true);
        h.key(Key::Tab);
        h.key(Key::Enter);
        assert!(form(&h).servers.is_empty() && h.app.saved_servers.is_empty(), "Tab and Enter did not delete the server");
    }

    /// #311: "Remove the master password?" opens on Cancel: Enter keeps it; Esc cancels the
    /// question and leaves the window open; Tab goes to Remove, Enter removes it.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn removing_the_master_password_opens_on_cancel() {
        if own_folder("server_manager::tests::removing_the_master_password_opens_on_cancel") {
            return;
        }
        let mut h = manager("srv-vault");
        crate::servers::set_password("correct horse battery staple 311").unwrap();
        ask(&mut h, |f| f.master = Change::Remove);
        h.key(Key::Enter);
        assert!(form(&h).master == Change::None && crate::servers::encrypted(), "Enter on the question removed the master password, or left the question");
        ask(&mut h, |f| f.master = Change::Remove);
        h.key(Key::Escape);
        assert!(form(&h).master == Change::None && crate::servers::encrypted(), "Esc did not cancel the question only");
        ask(&mut h, |f| f.master = Change::Remove);
        h.key(Key::Tab);
        h.key(Key::Enter);
        assert!(!crate::servers::encrypted(), "Tab and Enter did not remove the master password");
    }
}
