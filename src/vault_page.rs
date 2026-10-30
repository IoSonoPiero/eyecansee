//! The master password of the saved servers: asked once per session before the first one is used
//! (the window `Unlock`), and set, changed or removed at the top of File > Server manager, with a
//! strength meter and the password typed twice.

use eframe::egui::{self, Color32, Key};

use crate::address::AddressForm;
use crate::{App, browser, servers};

/// What waits for the master password.
#[derive(Clone)]
pub(crate) enum Then {
    Open(String),
    Manager,
    /// The left panel's list of servers.
    Panel,
}

/// The window asking for the master password.
#[derive(Clone)]
pub(crate) struct Unlock {
    password: String,
    wrong: Option<String>,
    then: Then,
    first: bool,
}

impl Unlock {
    pub(crate) fn new(then: Then) -> Self {
        Unlock { password: String::new(), wrong: None, then, first: true }
    }
}

/// Set, change or remove, at the top of the server manager.
#[derive(Clone, Default, PartialEq)]
pub(crate) enum Change {
    #[default]
    None,
    /// A new password (also in place of the old one), typed twice.
    New { first: String, again: String },
    Remove,
}

/// The strength a new master password needs ("strong" for zxcvbn).
const STRONG: u8 = 3;

/// Is it an address of a server (the master password may hold its password and key)?
pub(crate) fn remote(address: &str) -> bool {
    ["ftp://", "ftps://", "sftp://"].iter().any(|s| address.trim().to_ascii_lowercase().starts_with(s))
}

fn strength_color(score: u8) -> Color32 {
    match score {
        0 | 1 => Color32::from_rgb(0xd3, 0x2f, 0x2f),
        2 => Color32::from_rgb(0xf5, 0x7c, 0x00),
        3 => Color32::from_rgb(0x7c, 0xb3, 0x42),
        _ => Color32::from_rgb(0x2e, 0x7d, 0x32),
    }
}

impl App {
    /// The master password before what waits for it; locked servers: nothing else happens.
    pub(crate) fn ask_master(&mut self, then: Then) {
        self.unlock = Some(Unlock::new(then));
    }

    pub(crate) fn unlock_window(&mut self, ctx: &egui::Context) {
        let Some(mut u) = self.unlock.clone() else { return };
        let t = |id| self.i18n.t(id);
        let keys = self.delete_answer.take();
        let mut answer = ctx.input(|i| if i.key_pressed(Key::Escape) || keys == Some(false) { Some(false) } else { None });
        egui::Modal::new(egui::Id::unique("unlock")).show(ctx, |ui| {
            ui.set_width(420.0);
            crate::theme::title(ui, t("vault-unlock-title"));
            ui.label(t("vault-unlock-text"));
            ui.add_space(6.0);
            let field = ui.add(egui::TextEdit::singleline(&mut u.password).password(true).desired_width(f32::INFINITY));
            if std::mem::take(&mut u.first) {
                field.request_focus();
            }
            if let Some(e) = &u.wrong {
                ui.label(browser::error_text(ui, e.clone()));
            }
            ui.add_space(8.0);
            let enter = keys == Some(true) || ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                if ui.add_enabled(!u.password.is_empty(), egui::Button::new(t("vault-unlock"))).clicked() || (enter && !u.password.is_empty()) {
                    answer = Some(true);
                }
                if ui.button(t("delete-no")).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => match servers::unlock(&u.password) {
                Ok(()) => {
                    log!("servers unlocked");
                    self.unlock = None;
                    self.saved_servers = servers::load();
                    match u.then {
                        Then::Open(address) => self.open_source(ctx, &address),
                        Then::Manager => self.server_manager = Some(AddressForm::manager()),
                        Then::Panel => {}
                    }
                }
                Err(e) => {
                    u.wrong = Some(e.map_or(t("vault-wrong"), |e| self.i18n.t_args("vault-unreadable", &[("detail", &e)])));
                    u.password.clear();
                    u.first = true;
                    self.unlock = Some(u);
                }
            },
            Some(false) => self.unlock = None,
            None => self.unlock = Some(u),
        }
    }

    /// The master password at the top of the server manager, standing out: its state, and set,
    /// change or remove.
    pub(crate) fn master_section(&self, ui: &mut egui::Ui, f: &mut AddressForm) {
        let t = |id| self.i18n.t(id);
        let encrypted = servers::encrypted();
        let accent = if encrypted { crate::theme::good(ui.visuals()) } else { strength_color(2) };
        egui::Frame::group(ui.style()).stroke(egui::Stroke::new(2.0, accent)).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(t(if encrypted { "vault-on" } else { "vault-off" })).strong().color(accent));
            match f.master.clone() {
                Change::None => {
                    ui.label(t(if encrypted { "vault-on-hint" } else { "vault-off-hint" }));
                    ui.horizontal(|ui| {
                        if ui.button(t(if encrypted { "vault-change" } else { "vault-set" })).clicked() {
                            f.master = Change::New { first: String::new(), again: String::new() };
                        }
                        if encrypted && ui.button(t("vault-remove")).clicked() {
                            f.master = Change::Remove;
                        }
                    });
                }
                Change::New { .. } => self.new_master(ui, f),
                Change::Remove => {
                    ui.label(browser::error_text(ui, t("vault-remove-warning")));
                    ui.horizontal(|ui| {
                        if ui.button(t("vault-remove-yes")).clicked() {
                            f.error = servers::remove_password().err().map(|e| e.to_string());
                            log!("master password removed: {:?}", f.error);
                            f.master = Change::None;
                        }
                        if ui.button(t("delete-no")).clicked() {
                            f.master = Change::None;
                        }
                    });
                }
            }
        });
    }

    /// A new master password: typed twice, its strength in colour, saved only when strong.
    fn new_master(&self, ui: &mut egui::Ui, f: &mut AddressForm) {
        let t = |id| self.i18n.t(id);
        let Change::New { mut first, mut again } = f.master.clone() else { return };
        egui::Grid::new("vault-new").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t("vault-new"));
            ui.add(egui::TextEdit::singleline(&mut first).password(true).desired_width(260.0));
            ui.end_row();
            ui.label(t("vault-again"));
            ui.add(egui::TextEdit::singleline(&mut again).password(true).desired_width(260.0));
            ui.end_row();
        });
        let score = if first.is_empty() { 0 } else { servers::vault::strength(&first) };
        let color = strength_color(score);
        let label = self.i18n.t(&format!("vault-strength-{score}"));
        ui.add(egui::ProgressBar::new((score as f32 + 1.0) / 5.0).fill(color).desired_width(380.0).text(label));
        let same = !first.is_empty() && first == again;
        if score < STRONG {
            ui.label(egui::RichText::new(t("vault-too-weak")).color(color));
        } else if !again.is_empty() && !same {
            ui.label(browser::error_text(ui, t("vault-different")));
        }
        ui.label(egui::RichText::new(t("vault-forget")).small());
        ui.horizontal(|ui| {
            if ui.add_enabled(same && score >= STRONG, egui::Button::new(t("vault-save"))).clicked() {
                f.error = servers::set_password(&first).err().map(|e| e.to_string());
                log!("master password set: {:?}", f.error);
                f.master = Change::None;
                return;
            }
            if ui.button(t("delete-no")).clicked() {
                f.master = Change::None;
                return;
            }
            f.master = Change::New { first: std::mem::take(&mut first), again: std::mem::take(&mut again) };
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn server_addresses() {
        for a in ["sftp://nas/", "ftp://me@nas.lan/Foto/", " FTPS://nas:21/"] {
            assert!(super::remote(a), "{a}");
        }
        for a in ["/home/me/Foto/", "mtp://X/", "C:\\Foto"] {
            assert!(!super::remote(a), "{a}");
        }
    }
}
