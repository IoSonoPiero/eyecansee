//! The editor's work done aside (audit 5.14, #295): the photo resampled after Resize, Undo or
//! Redo (`follow_size`), the part chosen copied (Copy, Cut), an image of another program pasted.
//! The frame asks, a thread of its own does it, a later frame takes the result; meanwhile the
//! status bar says "Preparing…". Until 0.78.0 all of it was done inside the frame, the window
//! still for that time (a photo of 20 000 pixels a side, a part of 8000, 200 megapixels pasted).

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui::{self, ColorImage};

use super::Window;
use crate::App;
use crate::edit::doc::{Area, Pixels};
use crate::edit::resize::Scale;
use crate::encode::Original;

/// A result of the work aside.
pub(super) enum Done {
    /// The photo at the size of `Scale` (`None`: the original's), its preview and the preview's
    /// halves (`mipmap`).
    Photo(Option<Scale>, Arc<Original>, ColorImage, Vec<ColorImage>),
    /// The part chosen copied: where it was, its pixels, and as the clipboard takes them.
    Part(Area, Pixels, ColorImage),
    /// The pixels of an image pasted from another program (`None`: it could not be read).
    Pasted(Option<Pixels>),
}

/// The work under way.
#[derive(Default)]
pub(super) struct Aside {
    jobs: Vec<Receiver<Done>>,
    /// The size the photo is being resampled to.
    pub resampling: Option<Option<Scale>>,
    /// A part is being copied (`copy_part`): a Paste meanwhile waits for it (`paste_after`), or it
    /// would paste what the clipboard held before.
    pub copying_part: bool,
    pub paste_after: bool,
}

impl Window {
    /// `job` done in a thread of its own; its result taken by `arrived`.
    pub(super) fn aside(&mut self, ctx: &egui::Context, name: &str, job: impl FnOnce() -> Done + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        crate::logging::spawn_named(name, move || {
            if tx.send(job()).is_ok() {
                ctx.request_repaint();
            }
        });
        self.aside.jobs.push(rx);
    }

    /// The jobs under way (tests).
    #[cfg(test)]
    pub(super) fn jobs_under_way(&self) -> usize {
        self.aside.jobs.len()
    }

    /// Some work aside is not done yet: the status bar says so.
    pub(super) fn working(&self) -> bool {
        !self.aside.jobs.is_empty()
    }

    /// The results arrived since the last frame: the photo shown, the others returned. A job
    /// that stopped without one (a fault) is dropped with what waited for it.
    pub(super) fn arrived(&mut self, ctx: &egui::Context) -> Vec<Done> {
        let mut done = Vec::new();
        let mut lost = false;
        self.aside.jobs.retain(|rx| match rx.try_recv() {
            Ok(d) => {
                done.push(d);
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                lost = true;
                false
            }
        });
        if lost {
            log!("edit: a work aside stopped without its result");
            if self.aside.jobs.is_empty() {
                self.aside = Aside::default();
            }
        }
        done.into_iter()
            .filter_map(|d| match d {
                Done::Photo(scale, photo, preview, halves) => {
                    self.photo_arrived(ctx, scale, photo, (preview, halves));
                    None
                }
                d => Some(d),
            })
            .collect()
    }
}

impl App {
    /// The work aside done since the last frame (`Window::arrived`), taken in: the part copied
    /// goes to the clipboard, the image pasted becomes a patch.
    pub(super) fn edit_work(&mut self, ctx: &egui::Context, w: &mut Window) {
        for done in w.arrived(ctx) {
            match done {
                Done::Part(area, pixels, shown) => self.part_copied(ctx, w, (area, pixels, shown)),
                Done::Pasted(pixels) => pixels.into_iter().for_each(|p| w.pasted_in(p)),
                Done::Photo(..) => {}
            }
        }
    }
}
