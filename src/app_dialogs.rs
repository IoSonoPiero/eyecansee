//! Program windows: Options, update and About.

use eframe::egui::{self, TextureOptions};

use crate::{App, OptTab, confirm, decode, mtp, theme, thumbcache, update, version};
use crate::i18n::I18n;
use crate::loader::Msg;
use crate::settings::{AfterSave, Graphics, RemoteDelete, Settings, SmoothFilter, ThemeChoice, WindowStart};

/// The Options window: the width of a tab's page (the rows of Resize are the widest: name, kind,
/// two sizes) and the space between the column of tabs and the page.
const PAGE: f32 = 572.0;
const GAP: f32 = 14.0;
/// Its height, the column of the tabs' (taller than the longest tab, General, since Diagnostics
/// came, #189), the same for every tab: changing tab does not resize the window.
pub(crate) const OPTIONS_HEIGHT: f32 = 357.0;

impl App {
    /// Every change is saved immediately.
    pub(crate) fn options_window(&mut self, ctx: &egui::Context) {
        let mut open = self.options;
        let before = self.settings.clone();
        let mut purged = false;
        let (i18n, languages) = (&self.i18n, &self.languages);
        let tab = &mut self.options_tab;
        let confirm = &mut self.purge_confirm;
        let s = &mut self.settings;
        let (addons, downloads) = (&mut self.addons_tab, &self.downloads);
        let mut download = None;
        // Fixed id: changing language changes the title, and the window must not move.
        // One tab per subject, in a column on the left (eight or more tabs do not fit in a row in
        // every language; `options_tabs.rs`). Fixed size and in the middle, so changing tab does
        // not make it jump.
        let id = egui::Id::unique("options");
        in_ring(ctx, egui::LayerId::new(egui::Order::Middle, id), || egui::Window::new(i18n.t("options-title"))
            .id(id)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size([crate::options_tabs::WIDTH + GAP + PAGE, OPTIONS_HEIGHT])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_height(OPTIONS_HEIGHT);
                ui.horizontal_top(|ui| {
                    crate::options_tabs::column(ui, tab, i18n, OPTIONS_HEIGHT);
                    ui.add_space(GAP - ui.spacing().item_spacing.x);
                    ui.vertical(|ui| match tab {
                        OptTab::Slideshow => crate::slideshow::options_ui(ui, s, i18n),
                        OptTab::Video => crate::engine::options_ui(ui, s, i18n),
                        OptTab::General => general_tab(ui, s, i18n, languages),
                        OptTab::Look => look_tab(ui, s, i18n),
                        OptTab::Viewer => viewer_tab(ui, s, i18n),
                        OptTab::Deleting => deleting_tab(ui, s, i18n),
                        OptTab::Thumbnails => purged = thumbnails_tab(ui, s, i18n, confirm),
                        OptTab::Editor => editor_tab(ui, s, i18n),
                        OptTab::Resize => crate::presets::options_ui(ui, s, i18n),
                        OptTab::Diagnostics => crate::log_options::options_ui(ui, s, i18n),
                        OptTab::Addons => download = addons.ui(ui, downloads, i18n),
                    });
                });
            }));
        if !open {
            self.purge_confirm = None;
            // Read again at the next opening: something may have changed meanwhile.
            self.addons_tab = Default::default();
        }
        // The same download the red buttons and the first-start window start (#212).
        if let Some(c) = download
            && self.downloads.start(c, ctx)
        {
            log!("options: downloading {}", c.kind);
        }
        if purged {
            // The thumbnails in memory are still the old ones: the open folder redoes them once the
            // cache is empty (`Msg::Purged`); read before, the old ones came back from it (#326).
            let (tx, ctx) = (self.tx.clone(), ctx.clone());
            thumbcache::purge(move || {
                let _ = tx.send(crate::loader::Msg::Purged);
                ctx.request_repaint();
            });
        }
        self.options = open;
        self.options_changed(ctx, &before);
    }

    /// Before the pages read the keys: Esc with Options open is the window's, and reaches nothing
    /// under it (it closed the viewer and left Options open, #163). The question on emptying the
    /// thumbnail cache is answered No first, as its No button.
    /// With nothing focused Esc comes as `key_answer` (`asking_keys`, #311); under a modal window
    /// (Rename from the grid's menu) it is that window's.
    pub(crate) fn options_esc(&mut self, ctx: &egui::Context) {
        if !self.options || ctx.memory(|m| m.top_modal_layer().is_some()) {
            return;
        }
        let key = self.key_answer == Some(false);
        if key {
            self.key_answer = None;
        } else if !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return;
        }
        if self.purge_confirm.take().is_none() {
            log!("options: closed with Esc");
            self.options = false;
        }
    }

    /// After the Options window: what a changed setting needs at once, then the settings saved.
    pub(crate) fn options_changed(&mut self, ctx: &egui::Context, before: &Settings) {
        if self.settings.language != before.language {
            log!("language {:?}", self.settings.language);
            self.i18n = I18n::detect(self.settings.language.as_deref());
            // The guide shows the pages of the new language.
            self.help.language(self.i18n.code);
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
    /// by EyeCanSee (Flatpak, RPM, development build).
    pub(crate) fn check_updates(&self, ctx: &egui::Context) -> bool {
        let Some(kind) = update::Kind::detect() else { return false };
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        crate::logging::spawn_named("update-check", move || {
            if tx.send(Msg::Update(update::check(kind, version()))).is_ok() {
                ctx.request_repaint();
            }
        });
        true
    }

    /// Help > Check for updates: the outcome, until closed (also with Esc, and with Enter on Close,
    /// highlighted, as the other windows that tell how something went: issues #12, #311).
    /// Drawn as every message window (issue #11: one style for all of them).
    pub(crate) fn update_window(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.update_note else { return };
        let title = self.i18n.t("menu-check-updates");
        let id = "update";
        let mut open = crate::confirm::answer(ctx, &mut self.key_answer, id) != Some(false);
        let tip = self.i18n.t("notice-copy");
        crate::save_dialogs::message(ctx, id, crate::save_dialogs::Head::new(&title), text, &tip, |ui| {
            if crate::confirm::buttons(ui, id, &[(self.i18n.t("notice-close"), true)], 0).is_some() {
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
        // In the Flatpak the user must close Dolphin first: with the phone open there, KDE takes it back.
        let (ask, ok) = if crate::update::packaged() { ("kiod-ask-flatpak", "kiod-free") } else { ("kiod-ask", "kiod-ok") };
        let (text, ok, cancel) = (self.i18n.t(ask), self.i18n.t(ok), self.i18n.t("delete-no"));
        let answer = crate::confirm::answer(ctx, &mut self.key_answer, "kiod");
        match confirm::yes_no(ctx, "kiod", 460.0, (self.i18n.t("kiod-title"), text), ok, true, cancel, answer, false) {
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
    /// key goes to known_hosts and the folder is read again (the server's starting folder if it
    /// had none).
    /// Also the warning, once per server, after a login with an RSA key (RUSTSEC-2023-0071).
    pub(crate) fn host_window(&mut self, ctx: &egui::Context) {
        if self.notice.is_none()
            && let Some(host) = crate::sftp::rsa_login()
        {
            let text = self.i18n.t_args("sftp-rsa-key", &[("host", &host)]);
            self.notice = Some(crate::save_dialogs::Notice { warning: true, ..crate::save_dialogs::Notice::new(self.i18n.t("sftp-rsa-title"), text) });
        }
        if !self.host_ask {
            return;
        }
        // An FTPS certificate or an SSH key, the same question.
        let Some(question) = &self.host_question else {
            self.host_ask = false;
            return;
        };
        let (cert, (host, fingerprint)) = (question.cert(), question.shown());
        let text = self.i18n.t_args(if cert { "cert-ask" } else { "host-ask" }, &[("host", &host), ("fingerprint", &fingerprint)]);
        let title = self.i18n.t(if cert { "cert-title" } else { "host-title" });
        let (ok, cancel) = (self.i18n.t("host-ok"), self.i18n.t("delete-no"));
        let answer = crate::confirm::answer(ctx, &mut self.key_answer, "host");
        // Cancel highlighted: trusting a server is for good, its fingerprint is to be read (#311).
        match confirm::yes_no(ctx, "host", 460.0, (title, text), ok, true, cancel, answer, true) {
            Some(true) => {
                if let Some(q) = self.host_question.take() {
                    q.trust();
                }
                crate::host_question::forget_all();
                self.host_ask = false;
                // The same connection again, with the password it was opened with: the address
                // never holds it, and the login went without it (audit 2.14).
                if self.dir.is_empty()
                    && let Some(loader) = &self.loader
                {
                    loader.home();
                }
                self.refresh();
            }
            Some(false) => {
                self.host_question = None;
                crate::host_question::forget_all();
                self.host_ask = false;
            }
            None => {}
        }
    }

    /// Help > About: closes with Enter or Esc too, as the other windows.
    pub(crate) fn about_window(&mut self, ctx: &egui::Context) {
        // Not a modal: it answers only with no modal over it (`answer`).
        let mut open = self.about && crate::confirm::answer(ctx, &mut self.key_answer, "about").is_none();
        let icon = self.about_icon.get_or_insert_with(|| {
            let img = decode::app_icon(96).unwrap_or_default();
            ctx.load_texture("about-icon", decode::to_color_image(img.into()), TextureOptions::LINEAR)
        });
        let t = |id| self.i18n.t(id);
        let card = crate::render_gpu::card_text(&self.i18n);
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
                    // Tab goes through the links and the copy of the card, Enter or Space opens or
                    // presses the one highlighted; none is at first, so Enter closes (#311).
                    crate::confirm::join("about", &ui.hyperlink_to(t("about-license-link"), "https://www.gnu.org/licenses/gpl-3.0.html"));
                    crate::confirm::join("about", &ui.hyperlink_to(t("about-source"), env!("CARGO_PKG_REPOSITORY")));
                    ui.add_space(8.0);
                    ui.weak(t("about-third-party"));
                    // The card the window is drawn with, to copy into a report of a problem (#319).
                    if let Some(card) = &card {
                        ui.add_space(8.0);
                        ui.label(t("about-gpu"));
                        ui.label(card);
                        let copy = ui.button(t("about-gpu-copy"));
                        crate::confirm::join("about", &copy);
                        if copy.clicked() {
                            ui.ctx().copy_text(card.clone());
                        }
                    }
                    crate::confirm::keys(ui, "about", &[], usize::MAX);
                });
            });
        self.about = open;
    }
}

fn general_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n, languages: &[(&'static str, String)]) {
    let t = |id| i18n.t(id);
    ui.checkbox(&mut s.remember_location, t("opt-remember-location")).on_hover_text(t("opt-remember-location-hint"));
    ui.checkbox(&mut s.remember_scroll, t("opt-remember-scroll")).on_hover_text(t("opt-remember-scroll-hint"));
    ui.checkbox(&mut s.tips, t("opt-tips")).on_hover_text(t("opt-tips-hint"));
    ui.add_space(6.0);
    ui.label(t("opt-window"));
    ui.radio_value(&mut s.window_start, WindowStart::Normal, t("opt-window-normal"));
    ui.radio_value(&mut s.window_start, WindowStart::Maximized, t("opt-window-maximized"));
    ui.radio_value(&mut s.window_start, WindowStart::Remember, t("opt-window-remember"));
    ui.add_space(6.0);
    language_rows(ui, &mut s.language, i18n, languages);
    // The Flatpak and the Fedora package are updated by the system; the Store copy asks the Store
    // (`update::Kind::Store`).
    if !crate::update::by_system() {
        ui.add_space(6.0);
        ui.checkbox(&mut s.check_updates, t("opt-check-updates")).on_hover_text(t("opt-check-updates-hint"));
    }
}

/// The language, as in Options > General.
pub(crate) fn language_rows(ui: &mut egui::Ui, language: &mut Option<String>, i18n: &I18n, languages: &[(&'static str, String)]) {
    ui.label(i18n.t("opt-language"));
    for (value, name) in language_choices(i18n, languages) {
        ui.radio_value(language, value, name);
    }
}

/// The languages to choose from, in Options > General and on the first-start window's welcome
/// page (#212): what the settings keep, and its name. "Same as the system" first.
pub(crate) fn language_choices(i18n: &I18n, languages: &[(&'static str, String)]) -> Vec<(Option<String>, String)> {
    let system = (None, i18n.t("opt-language-system"));
    std::iter::once(system).chain(languages.iter().map(|(code, name)| (Some(code.to_string()), name.clone()))).collect()
}

/// The colors, as in Options > Look.
pub(crate) fn theme_rows(ui: &mut egui::Ui, theme: &mut ThemeChoice, i18n: &I18n) {
    ui.label(i18n.t("opt-theme"));
    for (value, name) in theme_choices(i18n) {
        ui.radio_value(theme, value, name);
    }
}

/// The colors to choose from, in Options > Look and on the first-start window's welcome page
/// (#212).
pub(crate) fn theme_choices(i18n: &I18n) -> [(ThemeChoice, String); 3] {
    [(ThemeChoice::System, i18n.t("opt-theme-system")), (ThemeChoice::Light, i18n.t("opt-theme-light")), (ThemeChoice::Dark, i18n.t("opt-theme-dark"))]
}

fn look_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    theme_rows(ui, &mut s.theme, i18n);
    ui.add_space(6.0);
    ui.checkbox(&mut s.smooth, t("opt-smooth")).on_hover_text(t("opt-smooth-hint"));
    ui.add_enabled_ui(s.smooth, |ui| {
        ui.indent("smooth-filter", |ui| {
            for (f, id) in [(SmoothFilter::Halves, "opt-smooth-halves"), (SmoothFilter::Whole, "opt-smooth-whole"), (SmoothFilter::Mitchell, "opt-smooth-mitchell")] {
                ui.radio_value(&mut s.smooth_filter, f, t(id)).on_hover_text(i18n.t(&format!("{id}-hint")));
            }
        });
    });
    // DirectX 12 or Vulkan: a choice only on Windows (#171).
    if cfg!(windows) {
        ui.add_space(6.0);
        let hint = t("opt-graphics-hint");
        ui.label(t("opt-graphics")).on_hover_text(&hint);
        ui.radio_value(&mut s.graphics, Graphics::Directx12, t("opt-graphics-directx12")).on_hover_text(&hint);
        ui.radio_value(&mut s.graphics, Graphics::Vulkan, t("opt-graphics-vulkan")).on_hover_text(&hint);
        ui.weak(t("opt-engine-restart"));
    }
}

fn viewer_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
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

/// After saving (issue #95) and the main save button (issue #96).
fn editor_tab(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    // How to save first, then what happens after saving.
    let hint = t("opt-edit-main-hint");
    ui.label(t("opt-edit-main")).on_hover_text(&hint);
    ui.radio_value(&mut s.edit_main_overwrite, false, t("edit-save-copy")).on_hover_text(&hint);
    ui.radio_value(&mut s.edit_main_overwrite, true, t("opt-edit-main-overwrite")).on_hover_text(&hint);
    ui.add_space(6.0);
    let hint = t("opt-edit-after-hint");
    ui.label(t("opt-edit-after")).on_hover_text(&hint);
    for (value, id) in [(AfterSave::Stay, "opt-edit-after-stay"), (AfterSave::Back, "opt-edit-after-back"), (AfterSave::Thumbnails, "opt-edit-after-grid")] {
        ui.radio_value(&mut s.edit_after_save, value, t(id)).on_hover_text(&hint);
    }
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
    // What goes under each thumbnail (#139).
    ui.label(t("opt-under-thumbs")).on_hover_text(t("opt-under-thumbs-hint"));
    let d = &mut s.thumb_details;
    ui.checkbox(&mut d.name, t("opt-under-name"));
    ui.checkbox(&mut d.pixels, t("opt-under-pixels")).on_hover_text(t("opt-under-pixels-hint"));
    ui.checkbox(&mut d.size, t("opt-under-size"));
    ui.checkbox(&mut d.date, t("opt-under-date")).on_hover_text(t("opt-under-date-hint"));
    ui.checkbox(&mut d.duration, t("opt-under-duration")).on_hover_text(t("opt-under-duration-hint"));
    ui.checkbox(&mut d.drawing, t("opt-under-drawing")).on_hover_text(t("opt-under-drawing-hint"));
    ui.add_space(6.0);
    ui.separator();
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
    // On Empty: the thumbnails are made again (#311).
    let labels = [(t("opt-purge-yes"), true), (t("opt-purge-no"), true)];
    let answer = ui.horizontal(|ui| crate::confirm::buttons(ui, "options", &labels, 0)).inner;
    if answer.is_some() {
        *confirm = None;
    }
    answer == Some(0)
}

/// Where the keyboard was at the end of a window's frame before, which of its controls take it
/// (in the order Tab goes), and whether an arrow came then (`in_ring`).
#[derive(Clone, Default)]
struct Ring {
    frame: u64,
    was: Option<egui::Id>,
    ids: Vec<egui::Id>,
    arrow: bool,
}

/// A window that is not modal (New image, Options), drawn by `show` on `layer`: Tab past its last
/// control went on to the menus, the toolbar and the grid under it, and Enter pressed what it had
/// reached there; Shift+Tab before the first one and the arrows too (#311). The keyboard goes round
/// inside the window instead, as in a modal one, which would keep the mouse from what is under it.
pub(crate) fn in_ring<R>(ctx: &egui::Context, layer: egui::LayerId, show: impl FnOnce() -> R) -> R {
    use egui::Key::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Tab};
    let (key, frame) = (layer.id.with("focus-ring"), ctx.cumulative_frame_nr());
    let ring = ctx.data(|d| d.get_temp::<Ring>(key)).filter(|r| r.frame + 1 == frame).unwrap_or_default();
    // From the events: the harness of the tests gives Shift with the key only.
    let tab = |back: bool| ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Tab, pressed: true, modifiers, .. } if modifiers.shift == back)));
    let (next, back) = (tab(false), tab(true));
    let arrow = ctx.input(|i| [ArrowDown, ArrowLeft, ArrowRight, ArrowUp].iter().any(|k| i.key_pressed(*k)));
    let now = ctx.memory(|m| m.focused());
    // Shift+Tab on the first control goes to the last, as if it were drawn just before it.
    if back && now.is_some() && now == ring.ids.first().copied() && let Some(&last) = ring.ids.last() {
        ctx.memory_mut(|m| m.interested_in_focus(last, layer));
    }
    // An arrow took the keyboard under the window at the end of the frame before: back.
    if let Some(was) = ring.was.filter(|w| ring.arrow && ring.ids.contains(w) && now.is_some_and(|n| under(ctx, n))) {
        ctx.memory_mut(|m| m.request_focus(was));
    }
    let shown = show();
    let ids: Vec<egui::Id> = ctx.viewport(|v| v.this_pass.widgets.get_layer(layer).filter(|w| w.enabled && w.sense.is_focusable()).map(|w| w.id).collect());
    let now = ctx.memory(|m| m.focused());
    if let (Some(&first), Some(&last)) = (ids.first(), ids.last()) {
        // A row of buttons may have taken it back meanwhile (`confirm::keys`, nothing focused).
        let from_last = ring.was.is_some() && ring.was == ring.ids.last().copied();
        if next && (now.is_none() || from_last) {
            // Tab on the last control: egui gives the keyboard to the next one drawn, the first.
            ctx.memory_mut(|m| m.interested_in_focus(first, layer));
        } else if (next || back) && now.is_some_and(|n| under(ctx, n)) {
            ctx.memory_mut(|m| m.request_focus(if next { first } else { last }));
        }
    }
    let now = ctx.memory(|m| m.focused());
    let arrow = arrow && now.is_some_and(|n| ids.contains(&n));
    if arrow {
        // egui moves it at the end of the frame: the next one, right away, brings it back.
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(key, Ring { frame, was: now, ids, arrow }));
    shown
}

/// The widget `id` is under the windows: the menus, the toolbar, the tree, the grid.
fn under(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.read_response(id).is_some_and(|r| r.layer_id.order == egui::Order::Background)
}

#[cfg(test)]
mod tests {
    use eframe::egui::Key;

    use crate::OptTab;
    use crate::audit_family_1::{Harness, folder, keys_stay_in_window, own_folder};

    /// Options open on the folder `name`, with nothing focused (a click on its body).
    fn options(name: &str) -> Harness {
        let mut h = Harness::new(&folder(name, &["a.png", "b.png", "c.png"]));
        h.app.options = true;
        h.frame(Vec::new());
        h
    }

    /// #311: with Options open the keys never reach the grid, the viewer or the shortcuts under
    /// it, also with nothing focused; Esc still closes it.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn options_keep_their_keys() {
        if own_folder("app_dialogs::tests::options_keep_their_keys") {
            return;
        }
        let mut h = options("opt-keys");
        keys_stay_in_window(&mut h, |a| a.options);
        h.blur();
        h.key(Key::Enter);
        assert!(h.app.options && h.app.viewing.is_none(), "Enter in Options opened the picture under it");
        h.blur();
        h.key(Key::Escape);
        assert!(!h.app.options, "Esc did not close Options");
    }

    /// #311: Tab past the last control of Options went on to the menus under it, and Enter pressed
    /// what it reached there; Shift+Tab and the arrows too. The keyboard goes round inside.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn tab_goes_round_options() {
        if own_folder("app_dialogs::tests::tab_goes_round_options") {
            return;
        }
        let mut h = options("opt-round");
        // Every page: its first and last controls differ (lists, fields, buttons greyed out).
        for tab in [OptTab::General, OptTab::Look, OptTab::Viewer, OptTab::Slideshow, OptTab::Video, OptTab::Editor, OptTab::Resize, OptTab::Deleting, OptTab::Thumbnails, OptTab::Diagnostics, OptTab::Addons] {
            h.app.options_tab = tab;
            h.blur();
            crate::audit_family_1::focus_stays_in_window(&mut h);
            assert!(h.app.options && h.app.options_tab == tab, "Options closed, or left the page {tab:?}");
        }
    }

    /// #311: Options open with nothing focused (as they open, or after a click on their body): Tab
    /// gives the keyboard to their first control, Shift+Tab to their last one. Both did nothing:
    /// the keys of the window went to rows of buttons, and Options have none.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn tab_enters_options() {
        use eframe::egui::{Modifiers, Order};
        if own_folder("app_dialogs::tests::tab_enters_options") {
            return;
        }
        let mut h = options("opt-enter");
        (h.app.cursor, h.app.selected, h.app.focus) = (true, 0, crate::Focus::Grid);
        let layer = |h: &Harness| h.ctx.memory(|m| m.focused()).and_then(|id| h.ctx.read_response(id)).map(|r| r.layer_id);
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
            h.blur();
            h.frame(Vec::new());
            h.key_with(Key::Tab, modifiers);
            h.frame(Vec::new());
            assert_eq!(layer(&h).map(|l| l.order), Some(Order::Middle), "Tab {modifiers:?} with nothing focused did not reach Options");
            let a = &h.app;
            assert!(a.options && a.viewing.is_none() && (a.cursor, a.selected, a.focus) == (true, 0, crate::Focus::Grid), "Options closed, or the grid under them moved");
        }
        // With the guide over them, the guide has the keys first: Tab does not go into Options.
        h.key(Key::F1);
        assert!(h.app.help.open, "F1 did not open the guide");
        h.blur();
        h.key(Key::Tab);
        h.frame(Vec::new());
        let options = eframe::egui::Id::unique("options");
        assert!(layer(&h).is_none_or(|l| l.id != options), "Tab under the guide went into Options");
    }

    /// #311: in Server manager, with nothing focused (as it opens, or after a click on a button),
    /// Tab gives the keyboard to its first control and goes on through them; the arrows too. Tab
    /// did nothing: it went to a row of buttons that the manager has not (Piero, 24/11).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn tab_goes_through_the_server_manager() {
        use eframe::egui::Key;
        if own_folder("app_dialogs::tests::tab_goes_through_the_server_manager") {
            return;
        }
        let mut h = Harness::new(&folder("srv-tab", &["a.png"]));
        h.app.server_manager = Some(crate::address::AddressForm::manager());
        h.frame(Vec::new());
        h.blur();
        h.frame(Vec::new());
        let focused = |h: &Harness| h.ctx.memory(|m| m.focused());
        let mut seen = std::collections::HashSet::new();
        for i in 1..=4 {
            h.key(Key::Tab);
            h.frame(Vec::new());
            let now = focused(&h);
            assert!(now.is_some(), "Tab pressed {i} times: nothing has the keyboard");
            seen.extend(now);
        }
        assert!(seen.len() >= 3, "Tab went through {} controls", seen.len());
        let before = focused(&h);
        h.key(Key::ArrowDown);
        h.frame(Vec::new());
        assert!(focused(&h).is_some() && focused(&h) != before, "the arrow moved nothing");
        assert!(h.app.server_manager.is_some(), "the manager closed");
    }

    /// #311: Options over the viewer keep the keys of the viewer and the shortcuts, with the
    /// keyboard nowhere or on a control: the picture, its zoom, its turn not saved and full screen
    /// stay as they were.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn options_over_the_viewer_keep_their_keys() {
        if own_folder("app_dialogs::tests::options_over_the_viewer_keep_their_keys") {
            return;
        }
        let mut h = crate::audit_family_1::turned_middle("opt-viewer");
        h.app.options = true;
        h.frame(Vec::new());
        h.frame(Vec::new());
        let first = h.ctx.viewport(|v| v.prev_pass.widgets.layers().filter(|(l, _)| l.order == eframe::egui::Order::Middle).find_map(|(_, w)| w.iter().find(|w| w.enabled && w.sense.is_focusable()).map(|w| w.id)));
        for focus in [None, first] {
            crate::audit_family_1::keys_stay_over_viewer(&mut h, |a| a.options, focus);
        }
    }

    /// #311: "Empty the thumbnail cache?" opens on Empty: Enter empties it. Esc cancels the
    /// question first, then closes Options.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn emptying_the_cache_opens_on_empty() {
        if own_folder("app_dialogs::tests::emptying_the_cache_opens_on_empty") {
            return;
        }
        let mut h = options("opt-purge");
        h.until(|_| crate::thumbcache::stats().0 == 3, "the thumbnails in the cache");
        // Gone from the disk: emptying the cache reads the folder again and must make none anew.
        for f in ["a.png", "b.png", "c.png"] {
            std::fs::remove_file(crate::audit_family_1::guard().join("opt-purge").join(f)).unwrap();
        }
        h.app.options_tab = OptTab::Thumbnails;
        // The cache writes in a thread of its own (`thumbcache::purge` only sends the order): what
        // was sent is in once `forget` is done. Read before, the count raced the writer.
        let settled = || {
            let (tx, rx) = std::sync::mpsc::channel();
            crate::thumbcache::forget("", Vec::new(), move || {
                let _ = tx.send(());
            });
            rx.recv().expect("the cache did not write");
        };
        let ask = |h: &mut Harness| {
            h.focus_somewhere();
            h.app.purge_confirm = Some(crate::thumbcache::stats());
            h.frame(Vec::new());
        };
        ask(&mut h);
        h.blur();
        h.key(Key::Escape);
        settled();
        assert!(h.app.options && h.app.purge_confirm.is_none() && crate::thumbcache::stats().0 > 0, "Esc did not cancel the question only");
        ask(&mut h);
        h.key(Key::Enter);
        assert!(h.app.options && h.app.purge_confirm.is_none() && h.app.viewing.is_none(), "Enter did not answer the question, or reached the grid");
        settled();
        assert_eq!(crate::thumbcache::stats().0, 0, "Enter did not empty the cache");
        h.blur();
        h.key(Key::Escape);
        assert!(!h.app.options, "Esc did not close Options");
    }

    /// What the grid shows right after "Empty the thumbnail cache". The cache holds an old
    /// thumbnail (5x5) for each picture (8x6): emptying it must make the grid show thumbnails made
    /// again from the files. `hold`: the cache's writer is kept busy (a `forget` whose `done`
    /// waits) while the grid reads again; `busy`: thumbnails of another folder queued before.
    fn stale_after_emptying(name: &str, hold: bool, busy: usize) -> Vec<[usize; 2]> {
        let mut h = options(name);
        h.until(|_| crate::thumbcache::stats().0 == 3, "the thumbnails in the cache");
        let settled = || {
            let (tx, rx) = std::sync::mpsc::channel();
            crate::thumbcache::forget("", Vec::new(), move || {
                let _ = tx.send(());
            });
            rx.recv().expect("the cache did not write");
        };
        // The oracle: the grid shows the thumbnails made from the files, 8x6.
        h.until(|a| (0..3).all(|i| matches!(a.thumbs.get(&i), Some(Ok(_)))), "the three thumbnails shown");
        let shown: Vec<[usize; 2]> = (0..3).map(|i| h.app.thumbs[&i].as_ref().unwrap().texture_at(1.0).size()).collect();
        assert_eq!(shown, vec![[8, 6]; 3], "the thumbnails of the files are not 8x6");
        let old = image::DynamicImage::new_rgb8(5, 5);
        for f in h.app.files().to_vec() {
            crate::thumbcache::save(&h.app.base, &f, &old);
        }
        settled();
        let (release, held) = std::sync::mpsc::channel::<()>();
        if hold {
            crate::thumbcache::forget("", Vec::new(), move || {
                let _ = held.recv();
            });
        }
        h.app.options_tab = OptTab::Thumbnails;
        h.focus_somewhere();
        h.app.purge_confirm = Some(crate::thumbcache::stats());
        // The writer busy with ordinary work: thumbnails of another folder still being saved.
        for i in 0..busy {
            let f = crate::source::File { path: format!("other/{i}.jpg"), size: 1, modified: "0".into() };
            crate::thumbcache::save_data("test-busy://", &f, vec![7; 64 << 10]);
        }
        h.frame(Vec::new());
        h.key(Key::Enter);
        assert!(h.app.purge_confirm.is_none(), "Enter did not empty the cache");
        h.until(|a| (0..3).all(|i| matches!(a.thumbs.get(&i), Some(Ok(_)))), "the three thumbnails shown again");
        let _ = release.send(());
        settled();
        (0..3).map(|i| h.app.thumbs[&i].as_ref().unwrap().texture_at(1.0).size()).collect()
    }

    /// #326: after "Empty the thumbnail cache" the grid shows thumbnails made again from the
    /// files, also while the cache's writer is busy: it got the old ones back from the cache.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn emptying_the_cache_shows_new_thumbnails_writer_busy() {
        if own_folder("app_dialogs::tests::emptying_the_cache_shows_new_thumbnails_writer_busy") {
            return;
        }
        let sizes = stale_after_emptying("opt-stale-held", true, 0);
        assert_eq!(sizes, vec![[8, 6]; 3], "after emptying the cache the grid shows the old thumbnails");
    }

    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn emptying_the_cache_shows_new_thumbnails() {
        if own_folder("app_dialogs::tests::emptying_the_cache_shows_new_thumbnails") {
            return;
        }
        assert_eq!(stale_after_emptying("opt-stale", false, 0), vec![[8, 6]; 3], "after emptying the cache the grid shows the old thumbnails");
    }

    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn emptying_the_cache_shows_new_thumbnails_writer_saving() {
        if own_folder("app_dialogs::tests::emptying_the_cache_shows_new_thumbnails_writer_saving") {
            return;
        }
        assert_eq!(stale_after_emptying("opt-stale-busy", false, 300), vec![[8, 6]; 3], "after emptying the cache the grid shows the old thumbnails");
    }

    /// #311: the outcome of Check for updates opens with Close highlighted, which Enter presses;
    /// Esc closes it too.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn update_note_opens_on_close() {
        if own_folder("app_dialogs::tests::update_note_opens_on_close") {
            return;
        }
        let mut h = Harness::new(&folder("update-note", &["a.png"]));
        for key in [Key::Enter, Key::Escape] {
            h.frame(Vec::new());
            h.app.update_note = Some("EyeCanSee is up to date.".into());
            h.frame(Vec::new());
            h.frame(Vec::new());
            assert!(h.ctx.memory(|m| m.focused()).is_some(), "Close not highlighted");
            h.key(key);
            assert!(h.app.update_note.is_none(), "{key:?} did not close the window");
        }
    }
}
