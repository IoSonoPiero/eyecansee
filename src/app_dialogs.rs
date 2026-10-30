//! Program windows: Options, update and About.

use eframe::egui::{self, TextureOptions};
use egui_commonmark::CommonMarkCache;

use crate::{App, OptTab, confirm, decode, mtp, theme, thumbcache, update, version};
use crate::i18n::I18n;
use crate::loader::Msg;
use crate::settings::{RemoteDelete, ThemeChoice, WindowStart};

impl App {
    /// Every change is saved immediately.
    #[allow(clippy::too_many_lines)] // to be split
    pub(crate) fn options_window(&mut self, ctx: &egui::Context) {
        let mut open = self.options;
        let before = self.settings.clone();
        let t = |id| self.i18n.t(id);
        let (window, normal, maximized, remember_size) =
            (t("opt-window"), t("opt-window-normal"), t("opt-window-maximized"), t("opt-window-remember"));
        let (scroll, scroll_hint) = (t("opt-remember-scroll"), t("opt-remember-scroll-hint"));
        let (wrap, wrap_hint) = (t("opt-wrap-around"), t("opt-wrap-around-hint"));
        let (wheel, wheel_zoom, wheel_next, wheel_hint) = (t("opt-wheel"), t("opt-wheel-zoom"), t("opt-wheel-next"), t("opt-wheel-hint"));
        let (max_zoom, max_zoom_hint) = (t("opt-max-zoom"), t("opt-max-zoom-hint"));
        let (language_label, language_system) = (t("opt-language"), t("opt-language-system"));
        let languages = self.languages.clone();
        let (theme_label, theme_system, theme_light, theme_dark) =
            (t("opt-theme"), t("opt-theme-system"), t("opt-theme-light"), t("opt-theme-dark"));
        let (title, remember, remember_hint, maint, maint_hint, limit) = (
            t("options-title"),
            t("opt-remember-location"),
            t("opt-remember-location-hint"),
            t("opt-cache-maintenance"),
            t("opt-cache-maintenance-hint"),
            t("opt-cache-limit"),
        );
        let (local_permanent, local_permanent_hint) = (t("opt-local-permanent"), t("opt-local-permanent-hint"));
        let (updates, updates_hint) = (t("opt-check-updates"), t("opt-check-updates-hint"));
        let (remote_delete, remote_folder, remote_permanent) =
            (t("opt-remote-delete"), t("opt-remote-delete-folder"), t("opt-remote-delete-permanent"));
        let (purge, purge_no) = (t("opt-purge"), t("opt-purge-no"));
        let purge_text = self.purge_confirm.map(|(n, bytes)| {
            let mb = format!("{:.1}", bytes as f64 / 1_048_576.0);
            (self.i18n.t_args("opt-purge-confirm", &[("n", &n.to_string()), ("mb", &mb)]), self.i18n.t("opt-purge-yes"))
        });
        let mut purged = false;
        let tab_names = [
            (OptTab::General, t("opt-tab-general")),
            (OptTab::Look, t("opt-tab-look")),
            (OptTab::Browsing, t("opt-tab-browsing")),
            (OptTab::Deleting, t("opt-tab-deleting")),
            (OptTab::Thumbnails, t("opt-tab-thumbnails")),
            (OptTab::Slideshow, t("opt-tab-slideshow")),
            (OptTab::Video, t("opt-tab-video")),
        ];
        let tab = &mut self.options_tab;
        let confirm = &mut self.purge_confirm;
        let s = &mut self.settings;
        // Fixed id: changing language changes the title, and the window must not move.
        // One tab per subject, as in other programs: the window stays short. Fixed size and in the
        // middle, so changing tab does not make it jump.
        egui::Window::new(title)
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
                    OptTab::Slideshow => crate::slideshow::options_ui(ui, s, &self.i18n),
                    OptTab::Video => crate::engine::options_ui(ui, s, &self.i18n),
                    OptTab::General => {
                        ui.checkbox(&mut s.remember_location, remember).on_hover_text(remember_hint);
                        ui.add_space(6.0);
                        ui.label(window);
                        ui.radio_value(&mut s.window_start, WindowStart::Normal, normal);
                        ui.radio_value(&mut s.window_start, WindowStart::Maximized, maximized);
                        ui.radio_value(&mut s.window_start, WindowStart::Remember, remember_size);
                        ui.add_space(6.0);
                        ui.label(language_label);
                        ui.radio_value(&mut s.language, None, language_system);
                        for (code, name) in languages {
                            ui.radio_value(&mut s.language, Some(code.to_string()), name);
                        }
                        ui.add_space(6.0);
                        ui.checkbox(&mut s.check_updates, updates).on_hover_text(updates_hint);
                    }
                    OptTab::Look => {
                        ui.label(theme_label);
                        ui.radio_value(&mut s.theme, ThemeChoice::System, theme_system);
                        ui.radio_value(&mut s.theme, ThemeChoice::Light, theme_light);
                        ui.radio_value(&mut s.theme, ThemeChoice::Dark, theme_dark);
                    }
                    OptTab::Browsing => {
                        ui.checkbox(&mut s.remember_scroll, scroll).on_hover_text(scroll_hint);
                        ui.checkbox(&mut s.wrap_around, wrap).on_hover_text(wrap_hint);
                        ui.add_space(6.0);
                        ui.label(wheel).on_hover_text(&wheel_hint);
                        ui.radio_value(&mut s.wheel_zoom, true, wheel_zoom).on_hover_text(&wheel_hint);
                        ui.radio_value(&mut s.wheel_zoom, false, wheel_next).on_hover_text(&wheel_hint);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(max_zoom).on_hover_text(&max_zoom_hint);
                            ui.add(egui::DragValue::new(&mut s.max_zoom).range(100..=10_000).speed(10).suffix("%")).on_hover_text(&max_zoom_hint);
                        });
                    }
                    OptTab::Deleting => {
                        ui.checkbox(&mut s.local_permanent, local_permanent).on_hover_text(local_permanent_hint);
                        ui.add_space(6.0);
                        ui.label(remote_delete);
                        ui.radio_value(&mut s.remote_delete, RemoteDelete::TrashFolder, remote_folder);
                        ui.radio_value(&mut s.remote_delete, RemoteDelete::Permanent, remote_permanent);
                    }
                    OptTab::Thumbnails => {
                        ui.checkbox(&mut s.thumb_maintenance, maint).on_hover_text(maint_hint);
                        ui.add_enabled_ui(s.thumb_maintenance, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(limit);
                                ui.add(egui::DragValue::new(&mut s.thumb_cache_mb).range(16..=1_000_000).suffix(" MB"));
                            });
                        });
                        ui.add_space(6.0);
                        match &purge_text {
                            None => {
                                if ui.button(&purge).clicked() {
                                    *confirm = Some(thumbcache::stats());
                                }
                            }
                            Some((text, yes)) => {
                                ui.label(text);
                                ui.horizontal(|ui| {
                                    if ui.button(yes).clicked() {
                                        thumbcache::purge();
                                        purged = true;
                                        *confirm = None;
                                    }
                                    if ui.button(&purge_no).clicked() {
                                        *confirm = None;
                                    }
                                });
                            }
                        }
                    }
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
        if self.settings.language != before.language {
            log!("language {:?}", self.settings.language);
            self.i18n = I18n::detect(self.settings.language.as_deref());
            // The cached manual is the one of the previous language.
            self.help_cache = CommonMarkCache::default();
        }
        if self.settings.theme != before.theme {
            theme::choose(ctx, self.settings.theme);
        }
        if self.settings != before {
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

    /// Help > Check for updates: the outcome, until closed.
    pub(crate) fn update_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.update_note else { return };
        let mut open = true;
        egui::Window::new(self.i18n.t("menu-check-updates"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.label(text);
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

    pub(crate) fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about;
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
