//! What runs at every frame around the drawing: messages, retries, keys, title and window size
//! (`before_frame`), the panels and the windows. Split from `main.rs` on 2026-11-01.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Color32};

use crate::{App, meta, save_dialogs, theme};

impl App {
    /// Every frame, before drawing: messages, retries, keys, title and window size.
    pub(crate) fn before_frame(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            log!("close requested (the window's ✕ or the system), editor open {}", self.editor.is_some());
        }
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
        // The ✕ in the viewer goes back to the grid, as Esc, when chosen in the Options (on at
        // first); a turned image not saved is asked about as when leaving it.
        if self.transfer.is_none() && self.settings.close_to_grid && self.viewing.is_some() && self.editor.is_none() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_viewer();
        } else if self.transfer.is_none()
            && ctx.input(|i| i.viewport().close_requested())
            && self.hold_for_turn(save_dialogs::After::Quit)
        {
            // Closing with an image turned and not saved: asked first, as when leaving the image.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.receive(ctx);
        self.open_held(ctx);
        self.phone_events(ctx);
        self.phone_stall();
        self.notice_gone(ctx);
        self.error_leaves_with_page(ctx);
        self.retry(ctx);
        self.reread_changed(ctx);
        // F1 everywhere, also over the pages, the dialogs and in a field: the guide on the page of
        // what is in front (`help_spot`).
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            let spot = self.help_spot(ctx);
            log!("help spot {spot}");
            self.toggle_help(spot);
        }
        // Esc over the guide is the guide's: taken here, before the pages under it read it.
        self.help_esc(ctx);
        // "Save in another format" and the duplicates cover the window: the keys must not reach the grid under them.
        if self.convert.is_none() && self.editor.is_none() && self.show.is_none() && !self.dup_page_shown() {
            self.global_keys(ctx);
            if self.viewing.is_some() {
                self.viewer_keys(ctx);
            }
        }
        if let Some(current) = self.viewing {
            self.prefetch_full(current);
        }
        self.probe_chosen(ctx);
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
        // First, so that it is at the very bottom on every page, under their own bars (issue #85).
        self.error_bar(ui);
        if self.show.is_some() {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| self.show_page(ui));
        } else if self.convert.is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.convert_page(ui));
        } else if self.editor.is_some() {
            // The editor's bars are panels of their own: no margin around them.
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.edit_page(ui));
        } else if self.dup_page_shown() {
            self.dup_page(ui);
        } else {
            // Full screen in the viewer: the picture (or video, model, drawing) alone, no bars
            // (issue #43 asked it for videos).
            let bare = self.viewing.is_some() && ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
            if !bare {
                egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
                egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, ctx));
            }
            match self.viewing {
                Some(current) if bare => {
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ui, |ui| self.viewer(ui, current));
                }
                Some(current) => {
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    let status = egui::Panel::bottom("viewer-status").frame(bar).show(ui, |ui| self.viewer_status(ui, current));
                    // The keys of the viewer: listed while the button is pointed at, or until it is
                    // clicked again.
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

    /// The error goes away with the page it was shown on: the editor, Save in another format, the
    /// duplicates, the slideshow, the viewer, or the folder of the grid (it
    /// stayed until its ✕). The page it was first shown on is kept in egui's memory.
    fn error_leaves_with_page(&mut self, ctx: &egui::Context) {
        let id = egui::Id::unique("page-of-the-error");
        let Some(error) = self.error.clone() else {
            ctx.data_mut(|d| d.remove::<(String, String)>(id));
            return;
        };
        let page = self.page_key();
        match ctx.data(|d| d.get_temp::<(String, String)>(id)) {
            Some(seen) if seen.0 == error => {
                if left_page(&seen, &error, &page) {
                    self.error = None;
                    ctx.data_mut(|d| d.remove::<(String, String)>(id));
                }
            }
            _ => ctx.data_mut(|d| {
                d.insert_temp(id, (error, page));
            }),
        }
    }

    /// The page in front, as a key: the pages that cover the window, the viewer, or the folder.
    fn page_key(&self) -> String {
        // Save in another format first: it can be open over the editor.
        if self.convert.is_some() {
            "convert".into()
        } else if self.editor.is_some() {
            "editor".into()
        } else if self.show.is_some() {
            "slideshow".into()
        } else if self.dup_page_shown() {
            "duplicates".into()
        } else if self.viewing.is_some() {
            "viewer".into()
        } else {
            format!("grid {}{}", self.base, self.dir)
        }
    }

    /// The last error, at the bottom of the window with a ✕ to close it. Over the pages that
    /// cover the window too (editor, Save in another format, duplicates, slideshow): drawn only
    /// in the grid and the viewer, a save that failed in the editor was told once it was closed.
    /// The slideshow goes on above it: the error is seen, and its ✕ closes it.
    /// A band in the warning colours with the triangle and the text on as many lines as it needs:
    /// red text on one long line, on the dark bar, was hard to read. It stays
    /// at the bottom, where the status bar is: at the top it would push the toolbar and the
    /// address bar down every time an error comes and goes.
    fn error_bar(&mut self, ui: &mut egui::Ui) {
        let Some(e) = self.error.clone() else { return };
        let (frame, ink) = (theme::warning_frame(ui.visuals()), theme::warning_text(ui.visuals()));
        egui::Panel::bottom("error").frame(frame).show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(crate::icons::small(ui, crate::icons::Line::Close)).on_hover_text(self.i18n.t("notice-close")).clicked() {
                    self.error = None;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    theme::warning_icon(ui, 22.0);
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(egui::RichText::new(e).color(ink).size(15.0)).wrap());
                });
            });
        });
    }

    /// The windows over the panels (guide, dialogs, confirmations) and the chosen menu item.
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        self.help_window(ctx);
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
        self.bookmarks_window(ctx);
        self.unlock_window(ctx);
        self.paste_window(ctx);
        self.transfer_window(ctx);
        self.rename_window(ctx);
        self.new_folder_window(ctx);
        self.new_image_window(ctx);
        self.model_export_window(ctx);
        self.restore_window(ctx);
        self.next_restore();
    }
}

/// Closing EyeCanSee: a save under way is waited for; anything else in the background (the
/// decoding of a large image, the neighbours read ahead, thumbnails) gets half a second, then is
/// left: the runtime waited for every one of them, and the window stayed frozen for seconds on
/// Windows after a 45-megapixel HEIC.
impl Drop for crate::App {
    fn drop(&mut self) {
        log!("quitting");
        if self.saving.is_empty()
            && let Ok(small) = tokio::runtime::Builder::new_current_thread().build()
        {
            std::mem::replace(&mut self.rt, small).shutdown_timeout(std::time::Duration::from_millis(500));
        }
    }
}

/// The error first shown on `seen.1` is still `error`, and the page in front is another one.
fn left_page(seen: &(String, String), error: &str, page: &str) -> bool {
    seen.0 == error && seen.1 != page
}

#[cfg(test)]
mod tests {
    use super::left_page;

    #[test]
    fn error_goes_with_its_page() {
        let seen = ("Cannot save".to_string(), "editor".to_string());
        assert!(!left_page(&seen, "Cannot save", "editor"));
        assert!(left_page(&seen, "Cannot save", "viewer"));
        // Another error is a new one, shown on the page in front.
        assert!(!left_page(&seen, "Disk full", "viewer"));
    }
}
