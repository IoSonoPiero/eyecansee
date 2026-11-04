//! What runs at every frame around the drawing: messages, retries, keys, title and window size
//! (`before_frame`), the panels and the windows. Split from `main.rs` on 2026-11-01.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Color32};

use egui_commonmark::CommonMarkViewer;

use crate::{App, browser, meta, save_dialogs, theme};

impl App {
    /// Every frame, before drawing: messages, retries, keys, title and window size.
    pub(crate) fn before_frame(&mut self, ctx: &egui::Context) {
        // Closing during a copy: it is cancelled, and the window closes once the file being copied
        // is finished. Closed in the middle, a phone kept half an operation and the next session
        // got its leftovers ("Transaction ID mismatch", until it was plugged in again).
        if let Some((p, ..)) = &self.transfer
            && ctx.input(|i| i.viewport().close_requested())
        {
            log!("close during a copy: cancelling it first");
            p.cancel.store(true, Ordering::Relaxed);
            self.quit_after_transfer = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        // Closing with an image turned and not saved: asked first, as when leaving the image.
        if self.transfer.is_none()
            && ctx.input(|i| i.viewport().close_requested())
            && self.hold_for_turn(save_dialogs::After::Quit)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.receive(ctx);
        self.phone_events(ctx);
        self.phone_stall();
        self.notice_gone(ctx);
        self.retry(ctx);
        self.reread_changed(ctx);
        // "Save in another format" and the duplicates cover the window: the keys must not reach the grid under them.
        if self.convert.is_none() && self.rotate.is_none() && self.show.is_none() && !self.dup_page_shown() {
            self.global_keys(ctx);
            if self.viewing.is_some() {
                self.viewer_keys(ctx);
            }
        }
        if let Some(current) = self.viewing {
            self.prefetch_full(current);
        }
        self.update_title(ctx);
        // Size to remember: that of the non-maximised window.
        let (size, maximized) = ctx.input(|i| (i.viewport_rect().size(), i.viewport().maximized));
        self.settings.window_maximized = maximized.unwrap_or(false);
        if !self.settings.window_maximized {
            self.settings.window_size = Some([size.x, size.y]);
        }
    }

    /// The panels of the window: the "Save in another format" page, or the bars with the viewer
    /// or with the tree, the metadata and the grid.
    pub(crate) fn panels(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.show.is_some() {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| self.show_page(ui));
        } else if self.convert.is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.convert_page(ui));
        } else if self.rotate.is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.rotate_page(ui));
        } else if self.dup_page_shown() {
            self.dup_page(ui);
        } else {
            egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
            egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, ctx));
            if let Some(e) = self.error.clone() {
                egui::Panel::bottom("error").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(browser::error_text(ui, e));
                        if ui.small_button("✖").clicked() {
                            self.error = None;
                        }
                    });
                });
            }
            match self.viewing {
                Some(current) => {
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    let status = egui::Panel::bottom("viewer-status").frame(bar).show(ui, |ui| self.viewer_status(ui, current));
                    // The keys of the 3D and CAD viewers: listed while the button is pointed at, or
                    // until it is clicked again.
                    if let (Some(button), Some(id)) = (status.inner, self.keys_hint(current)) {
                        if button.clicked() {
                            self.keys_open = !self.keys_open;
                        }
                        if self.keys_open || button.hovered() {
                            self.keys_window(ctx, id, status.response.rect.height());
                        }
                    }
                    egui::CentralPanel::default()
                        .frame(egui::Frame::central_panel(ui.style()).fill(Color32::from_gray(18)))
                        .show(ui, |ui| self.viewer(ui, current));
                }
                None => {
                    self.video = None;
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    egui::Panel::bottom("status").frame(bar).show(ui, |ui| self.status_bar(ui));
                    if meta::Group::ALL.into_iter().any(|g| self.meta_shown(g)) {
                        egui::Panel::bottom("meta").show(ui, |ui| self.meta_panel(ui));
                    }
                    egui::Panel::left("tree").resizable(true).default_size(240.0).show(ui, |ui| self.tree(ui));
                    let canvas = theme::canvas(ui.visuals());
                    egui::CentralPanel::default()
                        .frame(egui::Frame::central_panel(ui.style()).fill(canvas))
                        .show(ui, |ui| self.grid(ui));
                    if let Some(back) = self.tab.take() {
                        self.tab(ctx, back);
                    }
                }
            }
        }
    }

    /// The windows over the panels (manual, dialogs, confirmations) and the chosen menu item.
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        let mut open = self.help;
        egui::Window::new(self.i18n.t("menu-manual"))
            .open(&mut open)
            .default_size([560.0, 480.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    CommonMarkViewer::new().show(ui, &mut self.help_cache, self.i18n.manual);
                });
            });
        self.help = open;
        self.about_window(ctx);
        self.update_window(ctx);
        self.options_window(ctx);
        if let Some(a) = self.act.take() {
            self.run(ctx, a);
        }
        self.duplicates_window(ctx);
        self.batch_window(ctx);
        self.delete_window(ctx);
        self.turn_leave_window(ctx);
        self.save_window(ctx);
        self.batch_turn_window(ctx);
        self.notice_window(ctx);
        self.kept_window(ctx);
        self.kiod_window(ctx);
        self.host_window(ctx);
        self.address_window(ctx);
        self.server_manager_window(ctx);
        self.unlock_window(ctx);
        self.paste_window(ctx);
        self.transfer_window(ctx);
        self.rename_window(ctx);
        self.new_folder_window(ctx);
        self.restore_window(ctx);
        self.next_restore();
    }
}
