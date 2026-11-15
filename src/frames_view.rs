//! Animated images and pages in the viewer (issues #121 and #122): the pictures read by
//! `frames::read` shown in turn in the open image's texture, and a small bar over the image to
//! play and pause an animation or to turn the pages.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, Key, Modifiers, TextureOptions};

use crate::App;
use crate::frames::Sequence;
use crate::icons::Line;

#[derive(Default)]
pub(crate) struct State {
    /// The frames or pages of the images read for the viewer (and around it), by path.
    pub(crate) read: HashMap<String, Arc<Sequence>>,
    /// The picture shown of the open image; another image starts again from the first.
    shown: Option<Shown>,
}

struct Shown {
    path: String,
    at: usize,
    playing: bool,
    /// When the next frame is due, in egui's time.
    next: f64,
    /// The picture the texture holds now.
    drawn: Option<usize>,
}

/// What the bar was asked for.
enum Click {
    Toggle,
    Page(usize),
}

impl App {
    /// The frames or pages of the open image, and its path.
    fn sequence(&self) -> Option<(String, Arc<Sequence>)> {
        let path = &self.files().get(self.viewing?)?.path;
        Some((path.clone(), self.frames.read.get(path)?.clone()))
    }

    /// Before the open image is drawn: its frame or page of the moment in its texture, and the
    /// next frame of a playing animation asked for in time. Nothing plays once the viewer shows
    /// something else: the frames move only while this runs.
    pub(crate) fn frames_tick(&mut self, ctx: &egui::Context, current: usize) {
        let Some((path, seq)) = self.sequence() else {
            self.frames.shown = None;
            return;
        };
        let now = ctx.input(|i| i.time);
        let shown = match &mut self.frames.shown {
            Some(s) if s.path == path => s,
            slot => slot.insert(Shown { path, at: 0, playing: true, next: now + delay(&seq, 0), drawn: None }),
        };
        if let Sequence::Animation(frames) = &*seq
            && shown.playing
        {
            if now >= shown.next {
                shown.at = (shown.at + 1) % frames.len();
                // Late by more than a second (the window was hidden): from now, not a rush of frames.
                let from = if now - shown.next > 1.0 { now } else { shown.next };
                shown.next = from + frames[shown.at].1.as_secs_f64();
            }
            ctx.request_repaint_after(Duration::from_secs_f64((shown.next - now).max(0.0)));
        }
        if shown.drawn != Some(shown.at)
            && let Some(Ok(tex)) = self.full.get_mut(&current)
            && let Some(image) = seq.image(shown.at)
        {
            tex.set(image, TextureOptions::LINEAR);
            shown.drawn = Some(shown.at);
        }
    }

    /// Space plays and pauses an animation; Page Up and Page Down turn the pages, and past the
    /// last or before the first go to the next or previous file as without pages. Taken from the
    /// input, so that `viewer_keys` does not see them.
    pub(crate) fn frames_keys(&mut self, ctx: &egui::Context) {
        let Some((path, seq)) = self.sequence() else { return };
        let Some(at) = self.frames.shown.as_ref().filter(|s| s.path == path).map(|s| s.at) else { return };
        let click = ctx.input_mut(|i| match &*seq {
            Sequence::Animation(_) => i.consume_key(Modifiers::NONE, Key::Space).then_some(Click::Toggle),
            Sequence::Pages(p) if at + 1 < p.len() && i.consume_key(Modifiers::NONE, Key::PageDown) => Some(Click::Page(at + 1)),
            Sequence::Pages(_) if at > 0 && i.consume_key(Modifiers::NONE, Key::PageUp) => Some(Click::Page(at - 1)),
            _ => None,
        });
        if let Some(click) = click {
            self.frames_click(ctx, click);
        }
    }

    fn frames_click(&mut self, ctx: &egui::Context, click: Click) {
        let Some((_, seq)) = self.sequence() else { return };
        let Some(shown) = &mut self.frames.shown else { return };
        match click {
            Click::Toggle => {
                shown.playing = !shown.playing;
                shown.next = ctx.input(|i| i.time) + delay(&seq, shown.at);
            }
            Click::Page(n) => {
                shown.at = n;
                // Another page may have another size: fitted again.
                self.zoom = None;
            }
        }
        ctx.request_repaint();
    }

    /// The bar over the bottom of the open image: play/pause and the frame of an animation, the
    /// arrows and the page of a TIFF; for one too large to play, why only the first picture shows.
    pub(crate) fn frames_bar(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        let Some((_, seq)) = self.sequence() else { return };
        let Some(shown) = &self.frames.shown else { return };
        let (at, playing) = (shown.at, shown.playing);
        let light = Color32::from_gray(225);
        let area = egui::Area::new(egui::Id::unique("frames-bar")).pivot(egui::Align2::CENTER_BOTTOM).fixed_pos(egui::pos2(rect.center().x, rect.bottom() - 12.0));
        let frame = egui::Frame::new().fill(Color32::from_black_alpha(170)).corner_radius(6.0).inner_margin(6.0);
        let i18n = &self.i18n;
        let click = area
            .show(ui.ctx(), |ui| {
                frame
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let button = |ui: &mut egui::Ui, icon, tip: &str, on: bool| ui.add_enabled(on, egui::Button::image(crate::icons::line_in(ui.ctx(), icon, 18.0, light)).fill(Color32::TRANSPARENT)).on_hover_text(i18n.t(tip)).clicked();
                            let text = |s: String| egui::RichText::new(s).color(light).monospace();
                            let n = |v: usize| (v + 1).to_string();
                            let total = seq.len().to_string();
                            match &*seq {
                                Sequence::Animation(_) => {
                                    let (icon, tip) = if playing { (Line::Pause, "video-pause") } else { (Line::Play, "video-play") };
                                    let toggle = button(ui, icon, tip, true);
                                    ui.label(text(i18n.t_args("frames-frame", &[("n", &n(at)), ("total", &total)])));
                                    toggle.then_some(Click::Toggle)
                                }
                                Sequence::Pages(p) => {
                                    let back = button(ui, Line::Back, "frames-page-prev", at > 0);
                                    ui.label(text(i18n.t_args("frames-page", &[("n", &n(at)), ("total", &total)])));
                                    let on = button(ui, Line::Forward, "frames-page-next", at + 1 < p.len());
                                    if back { Some(Click::Page(at - 1)) } else { on.then_some(Click::Page(at + 1)) }
                                }
                                Sequence::TooLarge => {
                                    ui.label(egui::RichText::new(i18n.t("frames-too-large")).color(light));
                                    None
                                }
                            }
                        })
                        .inner
                    })
                    .inner
            })
            .inner;
        if let Some(click) = click {
            self.frames_click(ui.ctx(), click);
        }
    }

    /// "Page 2 / 5" or "Frame 3 / 20" for the status bar, when `path` is the open image.
    pub(crate) fn frames_status(&self, path: &str) -> Option<String> {
        let (open, seq) = self.sequence()?;
        let shown = self.frames.shown.as_ref().filter(|s| s.path == path && open == path)?;
        let id = match &*seq {
            Sequence::Animation(_) => "frames-frame",
            Sequence::Pages(_) => "frames-page",
            Sequence::TooLarge => return None,
        };
        Some(self.i18n.t_args(id, &[("n", &(shown.at + 1).to_string()), ("total", &seq.len().to_string())]))
    }
}

fn delay(seq: &Sequence, at: usize) -> f64 {
    match seq {
        Sequence::Animation(f) => f.get(at).map_or(0.1, |f| f.1.as_secs_f64()),
        _ => 0.0,
    }
}
