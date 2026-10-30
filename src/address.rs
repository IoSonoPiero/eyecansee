//! File > Open location: the window with a whole address or its parts, the fields shared with
//! the server manager, and the address for the log.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Key};
use percent_encoding::percent_decode_str;

use crate::servers::{self, Saved};
use crate::{App, browser};

/// Protocols of File > Open location.
const SCHEMES: &[&str] = &["ftp", "ftps", "sftp"];

/// File > Open location: a whole address, or its parts; File > Server manager: the saved servers.
#[derive(Clone, Default)]
pub(crate) struct AddressForm {
    pub(crate) url: String,
    pub(crate) scheme: usize,
    pub(crate) host: String,
    pub(crate) port: String,
    pub(crate) user: String,
    pub(crate) password: String,
    pub(crate) folder: String,
    /// The name it is saved with ("" = the server).
    pub(crate) name: String,
    pub(crate) key_file: String,
    pub(crate) key_passphrase: String,
    pub(crate) servers: Vec<Saved>,
    /// The saved server the fields come from; `None`: a new one.
    pub(crate) chosen: Option<usize>,
    /// Asking whether to delete the chosen server.
    pub(crate) deleting: bool,
    /// Why the servers could not be saved.
    pub(crate) error: Option<String>,
    /// The key file chosen in the system window, filled in when it closes.
    pub(crate) key_pick: Arc<Mutex<Option<PathBuf>>>,
    /// Server manager: setting, changing or removing the master password.
    pub(crate) master: crate::vault_page::Change,
    /// Just opened: the address field takes the focus.
    pub(crate) first: bool,
    /// Opened to ask for the password (`asking_password`): the password field takes the focus.
    pub(crate) password_focus: bool,
}

impl AddressForm {
    /// File > Open location.
    pub(crate) fn new() -> Self {
        Self { first: true, ..Default::default() }
    }

    /// File > Server manager: the saved servers, the first one chosen.
    pub(crate) fn manager() -> Self {
        let mut f = Self { servers: servers::load(), ..Default::default() };
        f.choose((!f.servers.is_empty()).then_some(0));
        f
    }

    /// An FTP or FTPS address with a user but no password, as the last location is saved: the
    /// form filled in with its parts, waiting for the password. `None` for any other address,
    /// and for a saved server with its password.
    pub(crate) fn asking_password(address: &str) -> Option<Self> {
        let u = url::Url::parse(address).ok()?;
        if u.scheme() == "sftp" || u.username().is_empty() || u.password().is_some() || servers::find(address).is_some_and(|s| !s.password.is_empty()) {
            return None;
        }
        Self::for_password(address)
    }

    /// A server that refused the login: the form filled in with the address's parts, waiting for
    /// the password. `None` for an address that is not a server's.
    pub(crate) fn for_password(address: &str) -> Option<Self> {
        let u = url::Url::parse(address).ok()?;
        let scheme = SCHEMES.iter().position(|s| *s == u.scheme())?;
        let decode = |s: &str| percent_decode_str(s).decode_utf8_lossy().into_owned();
        Some(Self {
            scheme,
            host: u.host_str()?.to_string(),
            port: u.port().map(|p| p.to_string()).unwrap_or_default(),
            user: decode(u.username()),
            folder: decode(u.path()),
            password_focus: true,
            ..Default::default()
        })
    }

    /// The address to open: the one typed whole, otherwise built from the parts (user and
    /// password escaped). `None`: nothing to open yet, or a port that is not a number.
    pub(crate) fn address(&self) -> Option<String> {
        if !self.url.trim().is_empty() {
            return Some(self.url.trim().to_string());
        }
        let host = self.host.trim();
        if host.is_empty() {
            return None;
        }
        let mut u = url::Url::parse(&format!("{}://{host}/", SCHEMES[self.scheme])).ok()?;
        if !self.port.trim().is_empty() {
            u.set_port(Some(self.port.trim().parse().ok()?)).ok()?;
        }
        if !self.user.is_empty() {
            u.set_username(&self.user).ok()?;
        }
        if !self.password.is_empty() {
            u.set_password(Some(&self.password)).ok()?;
        }
        // A folder: with "/" at the end, or its last part would be taken for a file.
        let folder = self.folder.trim().trim_matches('/');
        u.set_path(&if folder.is_empty() { "/".to_string() } else { format!("/{folder}/") });
        Some(u.to_string())
    }

    /// The fields of a saved server, or empty ones for a new server.
    pub(crate) fn choose(&mut self, chosen: Option<usize>) {
        let s = chosen.and_then(|i| self.servers.get(i)).cloned().unwrap_or_default();
        self.scheme = SCHEMES.iter().position(|x| *x == s.scheme).unwrap_or(0);
        self.port = s.port.map(|p| p.to_string()).unwrap_or_default();
        (self.name, self.host, self.user, self.password, self.folder) = (s.name, s.host, s.user, s.password, s.folder);
        (self.key_file, self.key_passphrase) = (s.key_file, s.key_passphrase);
        (self.chosen, self.deleting, self.error) = (chosen, false, None);
        self.url.clear();
    }

    /// The fields as a saved server: `None` without a server or with a bad port.
    pub(crate) fn saved(&self) -> Option<Saved> {
        let host = self.host.trim();
        let port = self.port.trim();
        let port = if port.is_empty() { None } else { Some(port.parse().ok()?) };
        let sftp = SCHEMES[self.scheme] == "sftp";
        (self.url.trim().is_empty() && !host.is_empty()).then(|| Saved {
            name: if self.name.trim().is_empty() { host.to_string() } else { self.name.trim().to_string() },
            scheme: SCHEMES[self.scheme].to_string(),
            host: host.to_string(),
            port,
            user: self.user.clone(),
            password: self.password.clone(),
            folder: self.folder.trim().to_string(),
            key_file: if sftp { self.key_file.trim().to_string() } else { String::new() },
            key_passphrase: if sftp { self.key_passphrase.clone() } else { String::new() },
        })
    }

    /// Save: the chosen server changed, or a new one added; then the file written.
    pub(crate) fn store(&mut self) {
        let Some(s) = self.saved() else { return };
        self.name = s.name.clone();
        match self.chosen.filter(|i| *i < self.servers.len()) {
            Some(i) => self.servers[i] = s,
            None => {
                self.servers.push(s);
                self.chosen = Some(self.servers.len() - 1);
            }
        }
        self.error = servers::save(&self.servers).err().map(|e| e.to_string());
    }

    /// The chosen server deleted, the fields emptied for a new one.
    pub(crate) fn remove(&mut self) {
        if let Some(i) = self.chosen.filter(|i| *i < self.servers.len()) {
            self.servers.remove(i);
            let error = servers::save(&self.servers).err().map(|e| e.to_string());
            self.choose(None);
            self.error = error;
        }
    }
}

/// A row of the grid: label and one-line field.
fn field(ui: &mut egui::Ui, label: String, text: &mut String, hint: String, password: bool) -> egui::Response {
    ui.label(label);
    let response = ui.add(egui::TextEdit::singleline(text).hint_text(hint).password(password).desired_width(300.0));
    ui.end_row();
    response
}

impl App {
    /// File > Open location: an address typed whole, or protocol, server, port, user, password
    /// and folder in their own fields.
    pub(crate) fn address_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.open_address.clone() else { return };
        let t = |id| self.i18n.t(id);
        let address = f.address();
        let bad_port = f.url.trim().is_empty() && !f.port.trim().is_empty() && f.port.trim().parse::<u16>().is_err();
        // In a field Enter and Esc reach the window; elsewhere they arrive as `delete_answer`.
        let keys = self.delete_answer.take();
        let mut answer = ctx.input(|i| if i.key_pressed(Key::Escape) || keys == Some(false) { Some(false) } else { None });
        egui::Modal::new(egui::Id::unique("open-address")).show(ctx, |ui| {
            ui.set_width(460.0);
            crate::theme::title(ui, t("addr-title"));
            ui.add_space(4.0);
            let url = ui.add(egui::TextEdit::singleline(&mut f.url).hint_text(t("tb-address-hint")).desired_width(f32::INFINITY));
            if f.first {
                url.request_focus();
                f.first = false;
            }
            ui.add_space(8.0);
            ui.label(t("addr-or"));
            ui.add_space(4.0);
            ui.add_enabled_ui(f.url.trim().is_empty(), |ui| {
                egui::Grid::new("addr-parts").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| self.address_grid(ui, &mut f, false));
            });
            if bad_port {
                ui.label(browser::error_text(ui, t("addr-bad-port")));
            }
            ui.add_space(8.0);
            let enter = keys == Some(true) || ui.input(|i| i.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                if ui.add_enabled(address.is_some(), egui::Button::new(t("addr-open"))).clicked() || (enter && address.is_some()) {
                    answer = Some(true);
                }
                if ui.button(t("delete-no")).clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                self.open_address = None;
                if let Some(a) = address {
                    self.open_source(ctx, &a);
                }
            }
            Some(false) => self.open_address = None,
            None => self.open_address = Some(f),
        }
    }

    /// The fields of an address; for the server manager (`saved`) also name, key file and passphrase.
    pub(crate) fn address_grid(&self, ui: &mut egui::Ui, f: &mut AddressForm, saved: bool) {
        let t = |id| self.i18n.t(id);
        if saved {
            let host_hint = f.host.clone();
            field(ui, t("addr-name"), &mut f.name, host_hint, false);
        }
        ui.label(t("addr-protocol"));
        egui::ComboBox::from_id_salt("addr-scheme").selected_text(SCHEMES[f.scheme].to_uppercase()).show_ui(ui, |ui| {
            for (i, s) in SCHEMES.iter().enumerate() {
                ui.selectable_value(&mut f.scheme, i, s.to_uppercase());
            }
        });
        ui.end_row();
        field(ui, t("addr-host"), &mut f.host, String::new(), false);
        let sftp = SCHEMES[f.scheme] == "sftp";
        field(ui, t("addr-port"), &mut f.port, if sftp { "22" } else { "21" }.to_string(), false);
        field(ui, t("addr-user"), &mut f.user, t("addr-user-hint"), false);
        let password = field(ui, t("addr-password"), &mut f.password, String::new(), true);
        if f.password_focus {
            password.request_focus();
            f.password_focus = false;
        }
        field(ui, t("addr-folder"), &mut f.folder, "/".to_string(), false);
        if saved && sftp {
            ui.label(t("addr-key"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.key_file).hint_text(t("addr-key-hint")).desired_width(210.0));
                if ui.button(t("batch-browse")).clicked() {
                    self.pick_key(ui.ctx(), f.key_pick.clone());
                }
                if let Some(p) = f.key_pick.lock().unwrap().take() {
                    f.key_file = p.to_string_lossy().into_owned();
                }
            });
            ui.end_row();
            field(ui, t("addr-passphrase"), &mut f.key_passphrase, String::new(), true);
        }
    }

    /// The system window for the key file, opening in ~/.ssh, or in the home folder without it.
    fn pick_key(&self, ctx: &egui::Context, slot: Arc<Mutex<Option<PathBuf>>>) {
        let ctx = ctx.clone();
        let mut dialog = rfd::AsyncFileDialog::new().set_title(self.i18n.t("addr-key"));
        if let Some(home) = std::env::home_dir() {
            let ssh = home.join(".ssh");
            dialog = dialog.set_directory(if ssh.is_dir() { ssh } else { home });
        }
        self.rt.spawn(async move {
            if let Some(file) = dialog.pick_file().await {
                *slot.lock().unwrap() = Some(file.path().to_path_buf());
                ctx.request_repaint();
            }
        });
    }
}

/// The address for the log: a password becomes "***".
pub(crate) fn without_password(address: &str) -> String {
    match url::Url::parse(address.trim()) {
        Ok(mut u) if u.password().is_some() => {
            let _ = u.set_password(Some("***"));
            u.to_string()
        }
        _ => address.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn open_location_address() {
        let f = |host: &str, port: &str, user: &str, password: &str, folder: &str| {
            super::AddressForm { host: host.into(), port: port.into(), user: user.into(), password: password.into(), folder: folder.into(), ..Default::default() }.address()
        };
        assert_eq!(f("", "", "", "", ""), None);
        assert_eq!(f("nas.lan", "", "", "", ""), Some("ftp://nas.lan/".into()));
        assert_eq!(f("127.0.0.1", "2121", "", "", "/Foto/2026/"), Some("ftp://127.0.0.1:2121/Foto/2026/".into()));
        assert_eq!(f("nas.lan", "", "me@home", "p:w/d", "Foto"), Some("ftp://me%40home:p%3Aw%2Fd@nas.lan/Foto/".into()));
        assert_eq!(f("nas.lan", "porta", "", "", ""), None);
        let whole = super::AddressForm { url: " C:\\Foto ".into(), host: "ignored".into(), ..Default::default() };
        assert_eq!(whole.address(), Some("C:\\Foto".into()));
    }

    #[test]
    fn password_asked_at_start() {
        let f = super::AddressForm::asking_password("ftp://me%40home@nas.lan:2121/Sea%20photos/").unwrap();
        assert_eq!((f.scheme, f.host.as_str(), f.port.as_str(), f.user.as_str()), (0, "nas.lan", "2121", "me@home"));
        assert_eq!((f.folder.as_str(), f.password.as_str(), f.password_focus), ("/Sea photos/", "", true));
        let typed = super::AddressForm { password: "p:w".into(), ..f };
        assert_eq!(typed.address(), Some("ftp://me%40home:p%3Aw@nas.lan:2121/Sea%20photos/".into()));
        assert_eq!(super::AddressForm::asking_password("ftps://me@nas.lan/").map(|f| f.scheme), Some(1));
        // Anonymous FTP, a password already there, SFTP (keys), local folders: nothing to ask.
        for a in ["ftp://nas.lan/Foto/", "ftp://me:secret@nas.lan/", "sftp://me@nas.lan/", "/home/me/Foto/", "mtp://X/"] {
            assert!(super::AddressForm::asking_password(a).is_none(), "{a}");
        }
    }

    #[test]
    fn saved_server_fields() {
        let nas = crate::servers::Saved { name: "NAS".into(), scheme: "sftp".into(), host: "nas.lan".into(), port: Some(2222), user: "me".into(), password: "pw".into(), key_file: "~/.ssh/id".into(), ..Default::default() };
        let mut f = super::AddressForm { servers: vec![nas.clone()], url: "typed".into(), ..Default::default() };
        f.choose(Some(0));
        assert_eq!((f.scheme, f.port.as_str(), f.url.as_str(), f.chosen), (2, "2222", "", Some(0)));
        assert_eq!(f.saved(), Some(nas));
        assert_eq!(f.address(), Some("sftp://me:pw@nas.lan:2222/".into()));
        // No name: the server's; FTP keeps no key; a bad port or a whole address saves nothing.
        f.name.clear();
        f.scheme = 0;
        assert_eq!(f.saved().map(|s| (s.name, s.key_file)), Some(("nas.lan".into(), String::new())));
        let bad = super::AddressForm { port: "x".into(), ..f.clone() };
        assert_eq!(bad.saved(), None);
        let whole = super::AddressForm { url: "ftp://other/".into(), ..f.clone() };
        assert_eq!(whole.saved(), None);
        f.choose(None);
        assert_eq!((f.host.as_str(), f.password.as_str(), f.chosen, f.servers.len()), ("", "", None, 1));
    }

    #[test]
    fn password_out_of_log() {
        assert_eq!(super::without_password("ftp://me:secret@nas.lan/Foto/"), "ftp://me:***@nas.lan/Foto/");
        assert_eq!(super::without_password("ftp://nas.lan/"), "ftp://nas.lan/");
        assert_eq!(super::without_password("C:\\Foto"), "C:\\Foto");
    }
}
