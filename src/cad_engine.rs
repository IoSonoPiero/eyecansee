//! The CAD engine (`component::OCCT`, for STEP, IGES and BREP) offered for download: a card in
//! the viewer on such a file, and a red button in the menu bar while the open folder has some.
//! Once downloaded it is used straight away: the thumbnails of those files are made again, and so
//! is the model open in the viewer.

use eframe::egui::{self, Color32};

use crate::{App, browser};

/// Where the download is.
enum State {
    Missing,
    Downloading(u64),
    Failed(String),
}

impl App {
    fn cad_engine_state(&mut self, ctx: &egui::Context) -> State {
        let Some(d) = &self.model.engine else { return State::Missing };
        let done = d.done.load(std::sync::atomic::Ordering::Relaxed);
        let result = d.result.lock().unwrap().clone();
        match result {
            None => {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
                State::Downloading(done)
            }
            Some(Err(e)) => State::Failed(e),
            Some(Ok(())) => {
                // There now: the thumbnails that failed without it are made again, only those.
                // Redoing them all read every other file of the folder again (a phone, 2026-11-08).
                self.model.engine = None;
                let models = self.files().iter().filter(|f| crate::occt::is_occt(&f.path)).map(|f| f.path.clone()).collect();
                self.on_forgotten(models);
                // The viewer kept its failed load and showed the old error (#102): read again.
                drop_missing(&mut self.full);
                ctx.request_repaint();
                State::Downloading(done)
            }
        }
    }

    fn cad_engine_download(&mut self, ctx: &egui::Context) {
        let Some(c) = crate::component::OCCT else { return };
        let ctx = ctx.clone();
        self.model.engine = Some(crate::component::start(c, move || ctx.request_repaint()));
    }

    /// In the viewer, on a STEP, IGES or BREP file without the engine.
    pub(crate) fn cad_engine_card(&mut self, ui: &mut egui::Ui) {
        let Some(c) = crate::component::OCCT else { return };
        let state = self.cad_engine_state(ui.ctx());
        let t = |id: &str, args: &[(&str, &str)]| self.i18n.t_args(id, args);
        let (size, light) = (self.human_size(c.size), Color32::from_gray(230));
        let mut download = false;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 110.0).max(0.0));
            let card = egui::Frame::new().fill(Color32::from_gray(34)).stroke(egui::Stroke::new(1.0, Color32::from_gray(60))).corner_radius(10.0).inner_margin(24.0);
            card.show(ui, |ui| {
                ui.set_max_width(460.0);
                ui.vertical_centered(|ui| {
                    let title = egui::FontId::new(20.0, egui::FontFamily::Name(crate::BOLD.into()));
                    ui.label(egui::RichText::new(t("cad-engine-title", &[])).font(title).color(light));
                    ui.add_space(10.0);
                    ui.add(egui::Label::new(egui::RichText::new(t("cad-engine-ask", &[("size", &size)])).color(light)).wrap());
                    match &state {
                        State::Downloading(done) => {
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new(t("video-component-progress", &[("done", &self.human_size(*done)), ("total", &size)])).color(light));
                            ui.add(egui::ProgressBar::new(*done as f32 / c.size as f32).desired_width(360.0));
                        }
                        State::Missing | State::Failed(_) => {
                            if let State::Failed(e) = &state {
                                ui.add_space(6.0);
                                ui.add(egui::Label::new(browser::error_text(ui, t("video-component-error", &[("error", e)]))).wrap());
                            }
                            ui.add_space(14.0);
                            let big = egui::Button::new(egui::RichText::new(t("cad-engine-download", &[])).size(15.0)).min_size(egui::vec2(220.0, 34.0));
                            download = ui.add(big).clicked();
                        }
                    }
                });
            });
        });
        if download {
            self.cad_engine_download(ui.ctx());
        }
    }

    /// In the menu bar, as the video engine's: only while the open folder has STEP, IGES or BREP
    /// files and the engine is missing.
    pub(crate) fn cad_engine_bar(&mut self, ui: &mut egui::Ui) {
        let Some(c) = crate::component::OCCT else { return };
        // Every frame (the menu bar is always there): the end of the download makes the folder
        // read again, also when nothing on screen asks for the engine any more.
        if self.model.engine.is_some() {
            self.cad_engine_state(ui.ctx());
        }
        let wanted = self.listings.get(&self.dir).and_then(|l| l.as_ref().ok()).is_some_and(|l| l.files.iter().any(|f| crate::occt::needs_engine(&f.path)));
        if !wanted {
            return;
        }
        let size = self.human_size(c.size);
        match self.cad_engine_state(ui.ctx()) {
            State::Downloading(done) => {
                let text = self.i18n.t_args("video-component-progress", &[("done", &self.human_size(done)), ("total", &size)]);
                ui.add_enabled(false, egui::Button::new(text));
            }
            State::Missing | State::Failed(_) => {
                let hover = self.i18n.t_args("cad-engine-ask", &[("size", &size)]);
                if ui.add(crate::theme::alert_button(self.i18n.t_args("cad-engine-button", &[("size", &size)]))).on_hover_text(hover).clicked() {
                    self.cad_engine_download(ui.ctx());
                }
            }
        }
    }
}

/// The viewer's loads that failed only for want of the engine (`occt::MISSING`): dropped, so
/// that the viewer asks for them again (`prefetch_full`).
fn drop_missing<T>(full: &mut std::collections::HashMap<usize, Result<T, String>>) {
    full.retain(|_, r| !matches!(r, Err(e) if e == crate::occt::MISSING));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_missing_engine_dropped() {
        let mut full = std::collections::HashMap::from([(0, Ok(())), (1, Err(crate::occt::MISSING.to_string())), (2, Err("broken file".to_string()))]);
        drop_missing(&mut full);
        let mut left: Vec<usize> = full.into_keys().collect();
        left.sort_unstable();
        assert_eq!(left, [0, 2]);
    }
}
