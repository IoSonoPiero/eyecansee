//! "Save in another format": the image decoded in full, encoded again with the chosen format and
//! options (`encode`). The encoded bytes are both what gets saved and, decoded back, the preview: what the
//! window shows is what the file will contain.

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui::{self, ColorImage, TextureHandle};

use crate::i18n::I18n;
use crate::batch::with_ext;
use crate::encode::{Chroma, Format, Options, Original, PngFilter, TiffCompression};
use crate::paint::{paint_turned, paint_zoomed};
use crate::transform::Turn;

/// The original decoded, its texture and file size.
type Loaded = Result<(Arc<Original>, TextureHandle, u64), String>;
/// The encoded file and, decoded back, its texture.
type Encoded = Result<(Arc<Vec<u8>>, Option<TextureHandle>), String>;

/// The open "Save in another format" window.
pub struct Window {
    /// The original, relative to the source.
    pub path: String,
    original: Option<Loaded>,
    pub(crate) options: Options,
    /// Options of the encoding under way or shown; a change starts a new one when it ends.
    encoded_with: Option<Options>,
    pub generation: u64,
    busy: bool,
    result: Option<Encoded>,
    name: String,
    /// 100%: one image pixel per screen pixel, moved by dragging (`pan` = centre, in image pixels).
    actual: bool,
    pan: egui::Vec2,
    saving: bool,
    /// Height of the part under the images, measured at the last frame.
    bottom_h: f32,
    error: Option<String>,
}

impl Window {
    pub fn new(path: String) -> Self {
        let options = Options::default();
        let name = with_ext(crate::source::name(&path), options.format);
        Window {
            path,
            original: None,
            options,
            encoded_with: None,
            generation: 0,
            busy: false,
            result: None,
            name,
            actual: false,
            pan: egui::Vec2::ZERO,
            saving: false,
            bottom_h: 300.0,
            error: None,
        }
    }
}

impl crate::App {
    pub(crate) fn open_convert(&mut self, path: Option<String>) {
        let Some(path) = path.or_else(|| self.target()).filter(|p| !p.ends_with('/')) else { return };
        let Some(loader) = &self.loader else { return };
        loader.load_original(path.clone());
        let mut w = Window::new(path);
        // From the viewer: saved as it is shown there.
        if self.viewing.is_some() {
            w.options.turn = self.turn;
        }
        self.convert = Some(w);
    }

    pub(crate) fn convert_original(&mut self, ctx: &egui::Context, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        let Some(w) = &mut self.convert else { return };
        w.original = Some(r.map(|(original, preview, size)| {
            w.pan = egui::vec2(preview.size[0] as f32, preview.size[1] as f32) / 2.0;
            (original, ctx.load_texture("convert-original", preview, egui::TextureOptions::LINEAR), size)
        }));
    }

    pub(crate) fn convert_encoded(&mut self, ctx: &egui::Context, generation: u64, r: Result<(Arc<Vec<u8>>, Option<ColorImage>), String>) {
        let Some(w) = &mut self.convert else { return };
        if generation != w.generation {
            return;
        }
        w.busy = false;
        w.result = Some(r.map(|(bytes, preview)| (bytes, preview.map(|p| ctx.load_texture("convert-result", p, egui::TextureOptions::LINEAR)))));
    }

    pub(crate) fn convert_saved(&mut self, r: Result<String, crate::source::Error>) {
        match r {
            Ok(path) => {
                log!("saved as {path:?}");
                self.convert = None;
                self.relist(crate::source::parent(&path));
            }
            Err((id, detail)) => {
                if let Some(w) = &mut self.convert {
                    w.saving = false;
                    w.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                }
            }
        }
    }

    /// Drawn in place of the whole interface, as a page of its own: a window could not cover the
    /// menu, tool and status bars, and the two images need all the room there is.
    pub(crate) fn convert_page(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let Some(mut w) = self.convert.take() else { return };
        self.start_encoding(&mut w, ctx);
        let dir = crate::source::parent(&w.path);
        let taken: HashSet<String> = match self.listings.get(&dir) {
            Some(Ok(l)) => l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).map(|p| crate::source::name(p).trim_end_matches('/').to_string()).collect(),
            _ => HashSet::new(),
        };
        let final_name = crate::source::free_name(&taken, w.name.trim());
        let open = page_header(ui, &self.i18n.t("convert-title"));
        let save = self.convert_body(ui, &mut w, &final_name);
        if save
            && let (Some(Ok((bytes, _))), Some(loader)) = (&w.result, &self.loader)
        {
            w.saving = true;
            w.error = None;
            loader.save(format!("{dir}{final_name}"), bytes.clone());
        }
        // Taken out while drawn (it needs the rest of the app), put back unless closed.
        if open {
            self.convert = Some(w);
        }
    }

    /// One encoding at a time (AVIF takes seconds): a change made meanwhile starts when it ends.
    fn start_encoding(&self, w: &mut Window, ctx: &egui::Context) {
        if !w.busy
            && w.encoded_with != Some(w.options)
            && let Some(Ok((original, _, _))) = &w.original
        {
            w.generation += 1;
            w.busy = true;
            // Another format: the old preview would pass for the new one while it is being made.
            if w.encoded_with.map(|o| o.format) != Some(w.options.format) {
                w.result = None;
            }
            w.encoded_with = Some(w.options);
            let (original, options, generation, tx, ctx2) = (original.clone(), w.options, w.generation, self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = original.encode(&options).map(|bytes| {
                    let preview = crate::decode::decode(&bytes, &format!("x.{}", options.format.ext())).ok();
                    (Arc::new(bytes), preview)
                });
                if tx.send(crate::loader::Msg::Encoded(generation, r)).is_ok() {
                    ctx2.request_repaint();
                }
            });
        }
    }

    /// Everything under the title: the two images, then what each one is, with the options and
    /// the name on the side of the new one. `true`: Save clicked.
    fn convert_body(&self, ui: &mut egui::Ui, w: &mut Window, final_name: &str) -> bool {
        let i18n = &self.i18n;
        let t = |id| i18n.t(id);
        let mut save = false;
        let original = match &w.original {
            None => {
                ui.centered_and_justified(|ui| ui.spinner());
                return save;
            }
            Some(Err(e)) => {
                ui.label(crate::browser::error_text(ui, i18n.t_args("cannot-load", &[("file", crate::source::name(&w.path)), ("error", e)])));
                return save;
            }
            Some(Ok(o)) => o.clone(),
        };
        let (image, tex, size) = (&original.0.image, &original.1, original.2);
        let half = (ui.available_width() - GAP) / 2.0;
        // The images get what the options below leave (their height measured at the last frame).
        let pane = egui::vec2(half, (ui.available_height() - 22.0 - 16.0 - w.bottom_h).max(120.0));
        convert_images(ui, w, i18n, tex, half, pane);
        ui.add_space(4.0);
        let pixels = i18n.t_args("convert-pixels", &[("w", &image.width().to_string()), ("h", &image.height().to_string())]);
        let bottom = ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.vertical(|ui| {
                ui.set_width(half);
                let ext = crate::source::name(&w.path).rsplit_once('.').map_or(String::new(), |(_, e)| e.to_uppercase());
                ui.label(format!("{ext} · {pixels}"));
                ui.label(i18n.t_args("convert-file-size", &[("size", &self.human_size(size))]));
                ui.add_space(6.0);
                ui.checkbox(&mut w.actual, t("convert-actual"));
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.set_width(half);
                self.result_info(ui, w, &pixels, size);
                ui.add_space(6.0);
                format_choice(ui, w, i18n);
                let image = &original.0.image;
                format_options(ui, &mut w.options, image.color().has_alpha(), image.width().max(image.height()) > crate::encode::ICO_SIDE, i18n);
                meta_options(ui, &mut w.options, &original.0, i18n);
                // The name belongs to the new image, so it sits on its side.
                ui.add_space(6.0);
                save = name_row(ui, w, i18n, half, final_name);
            });
        })
        .response;
        // Measured, for the next frame: the options change with the format.
        let used = bottom.rect.height() + 8.0;
        if (used - w.bottom_h).abs() > 1.0 {
            w.bottom_h = used;
            ui.ctx().request_repaint();
        }
        save
    }

    /// Format and size of the new file, against the original's; its error if it failed.
    fn result_info(&self, ui: &mut egui::Ui, w: &Window, pixels: &str, size: u64) {
        let i18n = &self.i18n;
        match &w.result {
            // Options changed, new file not made yet: the old size would pass for the new one
            // (the user saw 100 KB, saved, and got 150).
            Some(Ok(_)) if w.busy || w.encoded_with != Some(w.options) => {
                ui.label(format!("{} · {pixels}", w.options.format.label()));
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(i18n.t("convert-calculating"));
                });
            }
            Some(Ok((bytes, _))) => {
                let percent = (bytes.len() as u64 * 100 / size.max(1)).to_string();
                ui.label(format!("{} · {pixels}", w.options.format.label()));
                ui.label(format!(
                    "{} ({})",
                    i18n.t_args("convert-file-size", &[("size", &self.human_size(bytes.len() as u64))]),
                    i18n.t_args("convert-ratio", &[("percent", &percent)])
                ));
            }
            Some(Err(e)) => {
                ui.label(crate::browser::error_text(ui, i18n.t_args("convert-error", &[("detail", e)])));
            }
            None => {
                ui.label(" ");
                ui.label(" ");
            }
        }
    }
}

/// Space between the two halves of the page.
const GAP: f32 = 16.0;

/// Title and ✕ of a page covering the window; `false` once closed (✕ or Esc).
pub(crate) fn page_header(ui: &mut egui::Ui, title: &str) -> bool {
    let mut open = true;
    ui.horizontal(|ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✕").on_hover_text("Esc").clicked() {
                open = false;
            }
        });
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    ui.separator();
    open
}

/// The original and the new image side by side, each under its title.
fn convert_images(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n, tex: &TextureHandle, half: f32, pane: egui::Vec2) {
    let t = |id| i18n.t(id);
    let result_tex = match &w.result {
        Some(Ok((_, Some(tex)))) => Some(tex),
        _ => None,
    };
    // Titles in rows of the same height (the spinner is taller than the text), so the
    // two images start at the same height.
    let title = |ui: &mut egui::Ui, text: String, busy: bool| {
        ui.allocate_ui_with_layout(egui::vec2(half, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_size(egui::vec2(half, 22.0));
            ui.strong(text);
            if busy {
                ui.spinner();
                ui.weak(t("convert-working"));
            }
        });
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.vertical(|ui| {
            title(ui, t("convert-original"), false);
            let r = preview(ui, pane, Some(tex), None, w.actual, &mut w.pan);
            drag(&r, w.actual, &mut w.pan);
        });
        ui.vertical(|ui| {
            title(ui, t("convert-result"), w.busy);
            let note = match &w.result {
                _ if w.busy && result_tex.is_none() => Some(t("convert-working")),
                Some(Ok((_, None))) => Some(t("convert-no-preview")),
                _ => None,
            };
            let r = preview(ui, pane, result_tex, note.as_deref(), w.actual, &mut w.pan);
            drag(&r, w.actual, &mut w.pan);
        });
    });
}

/// The output format; the name takes its extension.
fn format_choice(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n) {
    let o = &mut w.options;
    let before = o.format;
    egui::ComboBox::from_label(i18n.t("convert-format")).selected_text(o.format.label()).show_ui(ui, |ui| {
        for f in Format::ALL {
            ui.selectable_value(&mut o.format, f, f.label());
        }
    });
    if o.format != before {
        w.name = with_ext(&w.name, o.format);
    }
}

fn quality(ui: &mut egui::Ui, v: &mut u8, label: String) {
    ui.add(egui::Slider::new(v, 1..=100).text(label));
}

/// The options of the chosen format's encoder, for an image with transparency (`alpha`) or larger
/// than an icon (`big`); with several images, as if they all were.
pub(crate) fn format_options(ui: &mut egui::Ui, o: &mut Options, alpha: bool, big: bool, i18n: &I18n) {
    let t = |id| i18n.t(id);
    if alpha && matches!(o.format, Format::Jpeg | Format::Hdr) {
        ui.weak(t("convert-on-white"));
    }
    match o.format {
        Format::Jpeg => jpeg_options(ui, o, i18n),
        Format::Png => png_options(ui, o, i18n),
        Format::Webp => webp_options(ui, o, i18n),
        Format::Avif => {
            quality(ui, &mut o.avif_quality, t("convert-quality"));
            if alpha {
                quality(ui, &mut o.avif_alpha_quality, t("convert-alpha-quality"));
            }
            ui.add(egui::Slider::new(&mut o.avif_speed, 1..=10).text(t("convert-speed"))).on_hover_text(t("convert-speed-hint"));
            ui.checkbox(&mut o.avif_ten_bit, t("convert-ten-bit"));
            ui.checkbox(&mut o.avif_rgb, t("convert-avif-rgb"));
        }
        Format::Tiff => tiff_options(ui, o, i18n),
        Format::Gif => {
            ui.add(egui::Slider::new(&mut o.gif_quality, 1..=30).text(t("convert-gif-quality")));
        }
        Format::Tga => {
            ui.checkbox(&mut o.tga_rle, t("convert-tga-rle"));
        }
        Format::Ico if big => {
            ui.weak(t("convert-ico-reduced"));
        }
        Format::Bmp | Format::Qoi | Format::Ico | Format::Pnm | Format::Exr | Format::Hdr | Format::Farbfeld => {}
    }
}

fn jpeg_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.add_enabled_ui(!o.jpeg_lossless, |ui| {
        quality(ui, &mut o.jpeg_quality, t("convert-quality"));
        egui::ComboBox::from_label(t("convert-chroma"))
            .selected_text(t(match o.chroma {
                Chroma::Full => "convert-chroma-full",
                Chroma::Half => "convert-chroma-half",
                Chroma::Quarter => "convert-chroma-quarter",
                Chroma::Grey => "convert-chroma-grey",
            }))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut o.chroma, Chroma::Full, t("convert-chroma-full"));
                ui.selectable_value(&mut o.chroma, Chroma::Half, t("convert-chroma-half"));
                ui.selectable_value(&mut o.chroma, Chroma::Quarter, t("convert-chroma-quarter"));
                ui.selectable_value(&mut o.chroma, Chroma::Grey, t("convert-chroma-grey"));
            });
    });
    ui.checkbox(&mut o.progressive, t("convert-progressive"));
    ui.checkbox(&mut o.optimize, t("convert-optimize"));
    ui.checkbox(&mut o.arithmetic, t("convert-arithmetic"));
    ui.checkbox(&mut o.jpeg_lossless, t("convert-jpeg-lossless"));
}

fn png_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.add(egui::Slider::new(&mut o.png_level, 0..=9).text(t("convert-png-level")));
    let filters = [
        (PngFilter::Adaptive, t("convert-filter-adaptive")),
        (PngFilter::None, t("convert-none")),
        (PngFilter::Sub, "Sub".to_string()),
        (PngFilter::Up, "Up".to_string()),
        (PngFilter::Avg, "Average".to_string()),
        (PngFilter::Paeth, "Paeth".to_string()),
    ];
    let shown = filters.iter().find(|(f, _)| *f == o.png_filter).map(|(_, l)| l.clone()).unwrap_or_default();
    egui::ComboBox::from_label(t("convert-png-filter")).selected_text(shown).show_ui(ui, |ui| {
        for (f, label) in filters {
            ui.selectable_value(&mut o.png_filter, f, label);
        }
    });
}

fn webp_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.horizontal(|ui| {
        ui.radio_value(&mut o.webp_lossless, false, t("convert-lossy"));
        ui.radio_value(&mut o.webp_lossless, true, t("convert-lossless-mode"));
    });
    if o.webp_lossless {
        ui.add(egui::Slider::new(&mut o.near_lossless, 0..=100).text(t("convert-near-lossless")));
    } else {
        quality(ui, &mut o.webp_quality, t("convert-quality"));
        ui.checkbox(&mut o.sharp_yuv, t("convert-sharp-yuv"));
    }
    ui.add(egui::Slider::new(&mut o.webp_effort, 0..=6).text(t("convert-effort"))).on_hover_text(t("convert-speed-hint"));
}

fn tiff_options(ui: &mut egui::Ui, o: &mut Options, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let kinds = [
        (TiffCompression::None, t("convert-none")),
        (TiffCompression::Lzw, "LZW".to_string()),
        (TiffCompression::DeflateFast, t("convert-deflate-fast")),
        (TiffCompression::Deflate, "Deflate".to_string()),
        (TiffCompression::DeflateBest, t("convert-deflate-best")),
        (TiffCompression::PackBits, "PackBits".to_string()),
    ];
    let shown = kinds.iter().find(|(k, _)| *k == o.tiff).map(|(_, l)| l.clone()).unwrap_or_default();
    egui::ComboBox::from_label(t("convert-compression")).selected_text(shown).show_ui(ui, |ui| {
        for (k, label) in kinds {
            ui.selectable_value(&mut o.tiff, k, label);
        }
    });
    let predictor = matches!(o.tiff, TiffCompression::Lzw | TiffCompression::DeflateFast | TiffCompression::Deflate | TiffCompression::DeflateBest);
    ui.add_enabled(predictor, egui::Checkbox::new(&mut o.tiff_predictor, t("convert-predictor")));
}

/// Whether the format loses quality, and the metadata kept when both the original and the format have some.
fn meta_options(ui: &mut egui::Ui, o: &mut Options, original: &Original, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let lossless = match o.format {
        Format::Png | Format::Tiff | Format::Bmp | Format::Qoi | Format::Tga | Format::Pnm | Format::Exr | Format::Farbfeld => true,
        Format::Ico => original.image.width().max(original.image.height()) <= crate::encode::ICO_SIDE,
        Format::Webp => o.webp_lossless && o.near_lossless == 100,
        Format::Jpeg => o.jpeg_lossless,
        _ => false,
    };
    if lossless {
        ui.weak(t("convert-lossless"));
    } else if o.format == Format::Gif {
        ui.weak(t("convert-gif"));
    }
    let can_meta = original.has_meta() && o.format.holds_meta();
    let hint = if !original.has_meta() { t("convert-no-meta") } else if !o.format.holds_meta() { t("convert-meta-unsupported") } else { String::new() };
    let r = ui.add_enabled(can_meta, egui::Checkbox::new(&mut o.keep_meta, t("convert-keep-meta")));
    if !hint.is_empty() {
        r.on_disabled_hover_text(hint);
    }
}

/// Name of the new file and Save (`true` when clicked); the name it will really get if taken.
fn name_row(ui: &mut egui::Ui, w: &mut Window, i18n: &I18n, half: f32, final_name: &str) -> bool {
    let t = |id| i18n.t(id);
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label(t("convert-name"));
        ui.add(egui::TextEdit::singleline(&mut w.name).desired_width((half - 200.0).clamp(160.0, 420.0)));
        let ready = matches!(w.result, Some(Ok(_))) && !w.busy && !w.saving && !w.name.trim().is_empty() && !w.name.contains(['/', '\\']);
        if ui.add_enabled(ready, egui::Button::new(t("convert-save"))).clicked() {
            save = true;
        }
        if w.saving {
            ui.spinner();
        }
    });
    if final_name != w.name.trim() && !w.name.trim().is_empty() {
        ui.weak(i18n.t_args("convert-exists", &[("name", final_name)]));
    }
    if let Some(e) = &w.error {
        ui.label(crate::browser::error_text(ui, e.as_str()));
    }
    save
}

/// The image fitted into `size` (never enlarged), or at 100% around `pan`; grey when none.
/// `note` is written in the middle when there is no image (converting, no preview for the format).
fn preview(ui: &mut egui::Ui, size: egui::Vec2, tex: Option<&TextureHandle>, note: Option<&str>, actual: bool, pan: &mut egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let painter = ui.painter_at(rect);
    // Grey ground and a border: on the window's own colour the two images ran into each other.
    let ground = if ui.visuals().dark_mode { egui::Color32::from_gray(24) } else { egui::Color32::from_gray(200) };
    painter.rect_filled(rect, 2.0, ground);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
    let Some(tex) = tex else {
        if let Some(note) = note {
            let font = egui::TextStyle::Body.resolve(ui.style());
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, note, font, ui.visuals().text_color());
        }
        return response;
    };
    let ppp = ui.ctx().pixels_per_point();
    let t = tex.size_vec2();
    if actual {
        paint_zoomed(&painter, rect, tex, 1.0, pan, Turn::default());
    } else {
        let scale = (rect.width() / t.x).min(rect.height() / t.y).min(1.0 / ppp);
        let dest = egui::Rect::from_center_size(rect.center(), t * scale);
        paint_turned(&painter, tex, dest, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Turn::default());
    }
    response
}


/// Dragging either preview moves both, at 100%.
fn drag(response: &egui::Response, actual: bool, pan: &mut egui::Vec2) {
    if actual && response.dragged() {
        *pan -= response.drag_delta() * response.ctx.pixels_per_point();
    }
}
