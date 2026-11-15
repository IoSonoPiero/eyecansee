//! The 3D viewer without a graphics card EyeCanSee can use (#145): the model drawn by `render3d`
//! on the processor, in the background, again whenever the view changes (turned, zoomed, cut by
//! a section), the last picture shown meanwhile. Before, such a computer saw a still picture.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, ColorImage, TextureHandle};

use crate::App;
use crate::model::Mesh;
use crate::render3d::View;

/// What a picture shows: the model, the view and the size in pixels.
type Key = (String, View, [u32; 2]);

#[derive(Default)]
pub(crate) struct CpuView {
    shown: Option<(Key, TextureHandle)>,
    job: Option<Key>,
    done: Arc<Mutex<Option<(Key, ColorImage)>>>,
}

impl App {
    /// The picture of `mesh` as `view` shows it at `size`, for the viewer: the one drawn last
    /// while the new one is drawn; `None` before the first.
    pub(crate) fn model_cpu_picture(&mut self, ctx: &egui::Context, path: &str, mesh: &Arc<Mesh>, view: View, size: [u32; 2]) -> Option<egui::TextureId> {
        let c = &mut self.model.cpu;
        if let Some((key, img)) = c.done.lock().unwrap().take() {
            c.job = None;
            c.shown = Some((key, ctx.load_texture("model-cpu", img, egui::TextureOptions::LINEAR)));
        }
        let key = (path.to_string(), view, size);
        let current = c.shown.as_ref().is_some_and(|(k, _)| *k == key);
        if !current && c.job.is_none() && size[0] > 0 && size[1] > 0 {
            c.job = Some(key.clone());
            let (done, ctx, mesh) = (c.done.clone(), ctx.clone(), mesh.clone());
            std::thread::spawn(move || {
                let img = crate::render3d::render(&mesh, &key.1, key.2[0], key.2[1], 1);
                let img = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                *done.lock().unwrap() = Some((key, img));
                ctx.request_repaint();
            });
        }
        // Another model's picture is not shown while this one is drawn.
        c.shown.as_ref().filter(|(k, _)| k.0 == path).map(|(_, t)| t.id())
    }
}
