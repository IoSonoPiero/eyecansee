//! Motion Photos in the viewer (issue #123): a "Play motion" button over the photo, and Space;
//! the video found by `motion::video_start` plays once through the video engine, from memory,
//! with the video's bar, then the photo is back.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Key, Modifiers};

use crate::App;

#[derive(Default)]
pub(crate) struct State {
    /// The videos of the Motion Photos read for the viewer (and around it), by path.
    pub(crate) read: HashMap<String, Arc<Vec<u8>>>,
    /// The photo whose video plays.
    on: Option<String>,
}

impl App {
    /// The open image's path, when it is a Motion Photo with its video read.
    fn motion_path(&self) -> Option<String> {
        let path = &self.files().get(self.viewing?)?.path;
        self.motion.read.contains_key(path).then(|| path.clone())
    }

    /// The video of the open Motion Photo is playing (or paused), not over.
    pub(crate) fn playing_motion(&self) -> bool {
        let Some(path) = self.motion.on.as_ref() else { return false };
        self.viewing.and_then(|i| self.files().get(i)).is_some_and(|f| &f.path == path) && self.video_playing(path)
    }

    /// Called by the viewer on an image: once the video is over (or another image is open) the
    /// photo is back.
    pub(crate) fn motion_done(&mut self) {
        if self.motion.on.is_some() && !self.playing_motion() {
            self.motion.on = None;
        }
    }

    fn play_motion(&mut self, ctx: &egui::Context) {
        let Some(path) = self.motion_path() else { return };
        let video = self.motion.read[&path].clone();
        log!("motion photo {path:?}: playing its video ({} bytes)", video.len());
        self.video = None;
        self.video = Some(self.open_motion(ctx, path.clone(), video));
        self.motion.on = Some(path);
        ctx.request_repaint();
    }

    /// Space on a Motion Photo plays its video; while it plays Space pauses it, as on a video.
    pub(crate) fn motion_keys(&mut self, ctx: &egui::Context) {
        if self.motion_path().is_none() || self.playing_motion() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) {
            self.play_motion(ctx);
        }
    }

    /// The "Play motion" button over the bottom of a Motion Photo.
    pub(crate) fn motion_bar(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        if self.motion_path().is_none() {
            return;
        }
        let light = Color32::from_gray(225);
        let area = egui::Area::new(egui::Id::unique("motion-bar")).pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(egui::pos2(rect.center().x, rect.bottom() - 12.0));
        let frame = egui::Frame::new().fill(Color32::from_black_alpha(170)).corner_radius(6.0).inner_margin(6.0);
        let text = egui::RichText::new(self.i18n.t("motion-play")).color(light);
        let icon = crate::icons::line_in(ui.ctx(), crate::icons::Line::Play, 18.0, light);
        let clicked = area.show(ui.ctx(), |ui| frame.show(ui, |ui| ui.add(egui::Button::image_and_text(icon, text).fill(Color32::TRANSPARENT)).clicked()).inner).inner;
        if clicked {
            self.play_motion(ui.ctx());
        }
    }
}
