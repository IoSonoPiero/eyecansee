//! What runs at every frame around the drawing: messages, retries, keys, title and window size
//! (`before_frame`), the panels and the windows. Split from `main.rs` on 2026-11-01.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Color32};

use crate::window_close::Step;
use crate::{App, save_dialogs, theme};

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
        // The editor closed after the ✕ with the option off: EyeCanSee now.
        self.quit_after_editor(ctx);
        // The ✕ (`window_close`): over the editor the page takes it (`edit_page`); in the viewer
        // back to the grid, as Esc, when chosen in the Options (on at first), or EyeCanSee; a
        // turned image not saved is asked about first, as when leaving it.
        if self.transfer.is_none() && self.editor.is_none() && ctx.input(|i| i.viewport().close_requested()) {
            let close = self.on_close(self.turn_unsaved() && !self.quit_dropped_turn);
            match close.step {
                Step::Viewer => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.close_viewer();
                }
                Step::Quit if close.ask && self.hold_for_turn(save_dialogs::After::Quit) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                _ => {}
            }
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
        // Esc with a menu open closes the menu only: left in the input, the page under it read it
        // too and closed the viewer or the editor with the menu.
        if egui::Popup::is_any_open(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            egui::Popup::close_all(ctx);
        }
        crate::browser::menu_keys(ctx);
        self.options_esc(ctx);
        self.first_start_esc(ctx);
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
        let (size, viewport) = ctx.input(|i| (i.viewport_rect().size(), i.viewport().clone()));
        self.settings.note_window(size, &viewport);
    }

    /// The panels of the window: the "Save in another format" page, or the bars with the viewer
    /// or with the tree, the metadata and the grid.
    pub(crate) fn panels(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // On Windows, one black frame before the window goes to full screen (#171).
        crate::viewer::full_step(ctx);
        // First, so that it is at the very bottom on every page, under their own bars (issue #85).
        self.error_bar(ui);
        if self.show.is_some() {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(egui::Color32::BLACK)).show(ui, |ui| crate::viewer::ahead(ui, |ui| self.show_page(ui)));
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
            let bare = self.viewing.is_some() && crate::viewer::bare(ctx);
            if !bare {
                crate::browser::menu_bar_at(ctx, egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui)).response.rect);
                egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, ctx));
            }
            match self.viewing {
                Some(current) if bare => {
                    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ui, |ui| crate::viewer::ahead(ui, |ui| self.viewer(ui, current)));
                }
                Some(current) => {
                    // The frame of an animation moves before the status bar says which it is (the
                    // viewer calls it again: a second call in the same frame changes nothing).
                    self.frames_tick(ctx, current);
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    let status = egui::Panel::bottom("viewer-status").frame(bar).show(ui, |ui| self.viewer_status(ui, current));
                    // The keys of the viewer: listed while the button is pointed at, or until it is
                    // clicked again.
                    let ([keys, layers, copy], zoom_asked) = status.inner;
                    if zoom_asked.is_some() {
                        self.zoom_bar.ask = zoom_asked;
                    }
                    self.section_slider(ctx);
                    if copy.is_some_and(|b| b.clicked()) {
                        self.copy_measure(ctx);
                    }
                    if layers.is_some_and(|b| b.clicked()) {
                        self.model.cad.panel = !self.model.cad.panel;
                    }
                    if let (Some(button), Some(id)) = (keys, self.keys_hint(current)) {
                        if button.clicked() {
                            self.keys_open = !self.keys_open;
                        }
                        if self.keys_open || button.hovered() {
                            self.keys_window(ctx, id, status.response.rect.height());
                        }
                    }
                    if self.cad_layers_shown() {
                        self.cad_layers_panel(ui);
                    }
                    egui::CentralPanel::default()
                        .frame(egui::Frame::central_panel(ui.style()).fill(Color32::from_gray(18)))
                        .show(ui, |ui| self.viewer(ui, current));
                }
                None => {
                    self.video = None;
                    let bar = egui::Frame::side_top_panel(ui.style()).fill(theme::solid(ui.visuals()));
                    // The grid's keys too (issue #150): the button on the right, the rest of the bar
                    // on its left.
                    let status = egui::Panel::bottom("status").frame(bar).show(ui, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let button = self.keys_button(ui);
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| self.status_bar(ui));
                            button
                        })
                        .inner
                    });
                    if status.inner.clicked() {
                        self.keys_open = !self.keys_open;
                    }
                    if self.keys_open || status.inner.hovered() {
                        self.keys_window(ctx, "hint-grid", status.response.rect.height());
                    }
                    self.meta_bottom(ui, ctx);
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

    /// The windows over the panels (guide, dialogs, confirmations) and the chosen menu item.
    pub(crate) fn windows(&mut self, ctx: &egui::Context) {
        self.help_window(ctx);
        self.about_window(ctx);
        self.update_window(ctx);
        self.options_window(ctx);
        self.tip_window(ctx);
        self.first_start_window(ctx);
        if let Some(a) = self.act.take() {
            self.run(ctx, a);
        }
        self.duplicates_window(ctx);
        self.batch_window(ctx);
        self.delete_window(ctx);
        self.turn_leave_window(ctx);
        self.save_window(ctx);
        self.save_big_window(ctx);
        self.ffmpeg_window(ctx);
        self.batch_turn_window(ctx);
        self.notice_window(ctx);
        self.batch_summary_window(ctx);
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
        // Enter or Esc that no window took (the one on top was not asking): forgotten, or it would
        // answer the window under it once that one is on top (#196).
        self.key_answer = None;
    }
}

/// Closing EyeCanSee: a save under way is waited for; anything else in the background (the
/// decoding of a large image, the neighbours read ahead, thumbnails) gets half a second, then is
/// left: the runtime waited for every one of them, and the window stayed frozen for seconds on
/// Windows after a 45-megapixel HEIC. The video players closing on threads of their own (#276)
/// end first, 2 s at most: mpv's threads cut off by the end of the process crashed it (a test's
/// own process, 15 times in 20, 22/11).
impl Drop for crate::App {
    fn drop(&mut self) {
        log!("quitting");
        self.video = None;
        crate::mpv::wait_closed(std::time::Duration::from_secs(2));
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
