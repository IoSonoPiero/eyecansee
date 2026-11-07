//! Program windows: Options, update and About.

use eframe::egui::{self, TextureOptions};
use egui_commonmark::CommonMarkCache;

use crate::{App, OptTab, confirm, decode, mtp, theme, thumbcache, update, version};
use crate::i18n::I18n;
use crate::loader::Msg;
use crate::settings::{RemoteDelete, Settings, ThemeChoice, WindowStart};

impl App {
    /// Every change is saved immediately.
    pub(crate) fn options_window(&mut self, ctx: &egui::Context) {
        let mut open = self.options;
        let before = self.settings.clone();
        let t = |id| self.i18n.t(id);
        let tab_names = [
            (OptTab::General, t("opt-tab-general")),
            (OptTab::Look, t("opt-tab-look")),
            (OptTab::Browsing, t("opt-tab-browsing")),
            (OptTab::Deleting, t("opt-tab-deleting")),
            (OptTab::Thumbnails, t("opt-tab-thumbnails")),
            (OptTab::Slideshow, t("opt-tab-slideshow")),
            (OptTab::Video, t("opt-tab-video")),
        ];
        let mut purged = false;
        let (i18n, languages) = (&self.i18n, &self.languages);
        let tab = &mut self.options_tab;
        let confirm = &mut self.purge_confirm;
        let s = &mut self.settings;
        // Fixed id: changing language changes the title, and the window must not move.
        // One tab per subject, as in other programs: the window stays short. Fixed size and in the
        // middle, so changing tab does not make it jump.
        egui::Window::new(i18n.t("options-title"))
            .id(egui::Id::unique("options"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            // Wide enough for the seven tabs in every language (Italian is the longest).
            .fixed_size([600.0, 330.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // As tall as the longest tab (General), so changing tab does not resize the window.
                ui.set_min_height(330.0);
                ui.horizontal(|ui| {
                    for (value, name) in tab_names {
                        ui.selectable_value(tab, value, name);
                    }
                });
                ui.separator();
                match tab {
                    OptTab::Slideshow => crate::slideshow::options_ui(ui, s, i18n),
                    OptTab::Video => crate::engine::options_ui(ui, s, i18n),
                    OptTab::General => general_tab(ui, s, i18n, languages),
                    OptTab::Look => look_tab(ui, s, i18n),
                    OptTab::Browsing => browsing_tab(ui, s, i18n),
                    OptTab::Deleting => deleting_tab(ui, s, i18n),
                    OptTab::Thumbnails => purged = thumbnails_tab(ui, s, i18n, confirm),
                }
            });
        if !open {
            self.purge_confirm = None;
        }
        if purged {
            log!("thumbnail cache emptied");
            // The thumbnails in memory are still the old ones: the open folder redoes them right away.
            self.refresh();
        }
        self.options = open;
        self.options_changed(ctx, &before);
    }

    /// After the Options window: what a changed setting needs at once, then the settings saved.
    fn options_changed(&mut self, ctx: &egui::Context, before: &Settings) {
        if self.settings.language != before.language {
            log!("language {:?}", self.settings.language);
            self.i18n = I18n::detect(self.settings.language.as_deref());
            // The cached manual is the one of the previous language.
            self.help_cache = CommonMarkCache::default();
        }
        if self.settings.theme != before.theme {
            theme::choose(ctx, self.settings.theme);
        }
        if self.settings != *before {
            self.settings.save();
            self.maintain();
        }
    }

    /// In the background; the answer comes as `Msg::Update`. `false` = this copy is not updated
    /// by EyeCanSee (RPM, development build).
    pub(crate) fn check_updates(&self, ctx: &egui::Context) -> bool {
        let Some(kind) = update::Kind::detect() else { return false };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            if tx.send(Msg::Update(update::check(kind, version()))).is_ok() {
                ctx.request_repaint();
            }
        });
        true
    }

    /// Help > Check for updates: the outcome, until closed (also with Enter or Esc, as the other
    /// windows that tell how something went: issue #12).
    /// Drawn as every message window (issue #11: one style for all of them).
    pub(crate) fn update_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.update_note else { return };
        let text = format!("**{}**\n\n{text}", self.i18n.t("menu-check-updates"));
        let mut open = self.notice.is_some() || self.delete_answer.take().is_none();
        crate::save_dialogs::message(ctx, "update", &text, |ui| {
            if ui.button(self.i18n.t("notice-close")).clicked() {
                open = false;
            }
        });
        if !open {
            self.update_note = None;
        }
    }

    /// KDE holds the phone: terminating kiod frees it, but KDE also forgets the passwords of network
    /// folders, so the user decides. OK reads the folder again, which opens the phone.
    pub(crate) fn kiod_window(&mut self, ctx: &egui::Context) {
        if !self.kiod_ask {
            return;
        }
        let (text, ok, cancel) = (self.i18n.t("kiod-ask"), self.i18n.t("kiod-ok"), self.i18n.t("delete-no"));
        match confirm::yes_no(ctx, "kiod", 460.0, text, ok, true, cancel, self.delete_answer.take()) {
            Some(true) => {
                log!("kiod: allowed");
                mtp::allow_kiod();
                self.kiod_ask = false;
                self.refresh();
            }
            Some(false) => self.kiod_ask = false,
            None => {}
        }
    }

    /// First connection to an SFTP server (its key's fingerprint, as ssh shows it) or to an FTPS
    /// server whose certificate no known authority vouches for (its fingerprint, as browsers show it). Trusted, the
    /// key goes to known_hosts and the address opens again (in the server's starting folder if
    /// it had none).
    /// Also the warning, once per server, after a login with an RSA key (RUSTSEC-2023-0071).
    pub(crate) fn host_window(&mut self, ctx: &egui::Context) {
        if self.notice.is_none()
            && let Some(host) = crate::sftp::rsa_login()
        {
            self.notice = Some((self.i18n.t_args("sftp-rsa-key", &[("host", &host)]), None));
        }
        if !self.host_ask {
            return;
        }
        // An FTPS certificate or an SSH key, the same question.
        let (cert, asked) = match crate::ftp::unknown() {
            Some(c) => (true, Some(c)),
            None => (false, crate::sftp::unknown()),
        };
        let Some((host, fingerprint)) = asked else {
            self.host_ask = false;
            return;
        };
        let text = self.i18n.t_args(if cert { "cert-ask" } else { "host-ask" }, &[("host", &host), ("fingerprint", &fingerprint)]);
        let (ok, cancel) = (self.i18n.t("host-ok"), self.i18n.t("delete-no"));
        match confirm::yes_no(ctx, "host", 460.0, text, ok, true, cancel, self.delete_answer.take()) {
            Some(true) => {
                if cert {
                    crate::ftp::trust();
                } else {
                    crate::sftp::trust();
                }
                self.host_ask = false;
                let address = format!("{}{}", self.base, self.dir);
                self.open_source(ctx, &address);
            }
            Some(false) => self.host_ask = false,
            None => {}
        }
    }

    /// Help > About: closes with Enter or Esc too, as the other windows (user, 2026-11-06).
    pub(crate) fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about && (self.notice.is_some() || self.update_note.is_some() || self.delete_answer.take().is_none());
        let icon = self.about_icon.get_or_insert_with(|| {
            let img = decode::app_icon(96).unwrap_or_default();
            ctx.load_texture("about-icon", decode::to_color_image(img.into()), TextureOptions::LINEAR)
        });
        let t = |id| self.i18n.t(id);
        egui::Window::new(t("menu-about"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.image((icon.id(), egui::vec2(96.0, 96.0)));
                    ui.heading(format!("EyeCanSee {}", version()));
                    ui.label(t("about-tagline"));
                    ui.add_space(8.0);
                    ui.label("© 2026 Pierluigi Montinaro");
                    ui.label(t("about-license"));
                    ui.hyperlink_to(t("about-license-link"), "https://www.gnu.org/licenses/gpl-3.0.html");
                    ui.hyperlink_to(t("about-source"), env!("CARGO_PKG_REPOSITORY"));
                    ui.add_space(8.0);
                    ui.weak(t("about-third-party"));
                });
            });
        self.about = open;
    }
}

fn general_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n, languages: &[(&'static str, String)]) {
    let t = |id| i18n.t(id);
    ui.checkbox(&mut s.remember_location, t("opt-remember-location")).on_hover_text(t("opt-remember-location-hint"));
    ui.add_space(6.0);
    ui.label(t("opt-window"));
    ui.radio_value(&mut s.window_start, WindowStart::Normal, t("opt-window-normal"));
    ui.radio_value(&mut s.window_start, WindowStart::Maximized, t("opt-window-maximized"));
    ui.radio_value(&mut s.window_start, WindowStart::Remember, t("opt-window-remember"));
    ui.add_space(6.0);
    ui.label(t("opt-language"));
    ui.radio_value(&mut s.language, None, t("opt-language-system"));
    for (code, name) in languages {
        ui.radio_value(&mut s.language, Some(code.to_string()), name);
    }
    // The Store updates its copies itself.
    if !crate::update::packaged() {
        ui.add_space(6.0);
        ui.checkbox(&mut s.check_updates, t("opt-check-updates")).on_hover_text(t("opt-check-updates-hint"));
    }
}

fn look_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.label(t("opt-theme"));
    ui.radio_value(&mut s.theme, ThemeChoice::System, t("opt-theme-system"));
    ui.radio_value(&mut s.theme, ThemeChoice::Light, t("opt-theme-light"));
    ui.radio_value(&mut s.theme, ThemeChoice::Dark, t("opt-theme-dark"));
}

fn browsing_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.checkbox(&mut s.remember_scroll, t("opt-remember-scroll")).on_hover_text(t("opt-remember-scroll-hint"));
    ui.checkbox(&mut s.wrap_around, t("opt-wrap-around")).on_hover_text(t("opt-wrap-around-hint"));
    ui.checkbox(&mut s.close_to_grid, t("opt-close-to-grid")).on_hover_text(t("opt-close-to-grid-hint"));
    ui.add_space(6.0);
    let wheel_hint = t("opt-wheel-hint");
    ui.label(t("opt-wheel")).on_hover_text(&wheel_hint);
    ui.radio_value(&mut s.wheel_zoom, true, t("opt-wheel-zoom")).on_hover_text(&wheel_hint);
    ui.radio_value(&mut s.wheel_zoom, false, t("opt-wheel-next")).on_hover_text(&wheel_hint);
    ui.add_space(6.0);
    let max_zoom_hint = t("opt-max-zoom-hint");
    ui.horizontal(|ui| {
        ui.label(t("opt-max-zoom")).on_hover_text(&max_zoom_hint);
        ui.add(egui::DragValue::new(&mut s.max_zoom).range(100..=10_000).speed(10).suffix("%")).on_hover_text(&max_zoom_hint);
    });
}

fn deleting_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.checkbox(&mut s.local_permanent, t("opt-local-permanent")).on_hover_text(t("opt-local-permanent-hint"));
    ui.add_space(6.0);
    ui.label(t("opt-remote-delete"));
    ui.radio_value(&mut s.remote_delete, RemoteDelete::TrashFolder, t("opt-remote-delete-folder"));
    ui.radio_value(&mut s.remote_delete, RemoteDelete::Permanent, t("opt-remote-delete-permanent"));
}

/// `confirm`: the files and bytes of the cache, while Empty asks to be sure. `true` once emptied.
fn thumbnails_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n, confirm: &mut Option<(u64, u64)>) -> bool {
    let t = |id| i18n.t(id);
    ui.checkbox(&mut s.thumb_maintenance, t("opt-cache-maintenance")).on_hover_text(t("opt-cache-maintenance-hint"));
    ui.add_enabled_ui(s.thumb_maintenance, |ui| {
        ui.horizontal(|ui| {
            ui.label(t("opt-cache-limit"));
            ui.add(egui::DragValue::new(&mut s.thumb_cache_mb).range(16..=1_000_000).suffix(" MB"));
        });
    });
    ui.add_space(6.0);
    let Some((n, bytes)) = *confirm else {
        if ui.button(t("opt-purge")).clicked() {
            *confirm = Some(thumbcache::stats());
        }
        return false;
    };
    let mb = format!("{:.1}", bytes as f64 / 1_048_576.0);
    ui.label(i18n.t_args("opt-purge-confirm", &[("n", &n.to_string()), ("mb", &mb)]));
    let mut purged = false;
    ui.horizontal(|ui| {
        if ui.button(t("opt-purge-yes")).clicked() {
            thumbcache::purge();
            purged = true;
            *confirm = None;
        }
        if ui.button(t("opt-purge-no")).clicked() {
            *confirm = None;
        }
    });
    purged
}
