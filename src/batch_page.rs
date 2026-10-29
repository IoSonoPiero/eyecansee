//! Convert and resize several images: the window with the choices, then how the work is going,
//! then what was done. The work itself is in `batch`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui;

use crate::batch::{Dest, Filter, Job, Progress, Size};
use crate::encode::{Format, Options};
use crate::ffmpeg::{CONTAINERS, Codec, Quality, Video};
use crate::folder_pick::{Picker, pick_ui};
use crate::i18n::I18n;
use crate::transform::{Op, Turn};
use crate::save_dialogs::{md, notice_text};
use crate::{App, source};

/// Where the new files go.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Next,
    Sub,
    /// A folder written or chosen: on this computer or on another device.
    Other,
}

#[derive(Clone, Copy, PartialEq)]
enum SizeKind {
    Same,
    Percent,
    Fit,
}

/// The choices, kept for the next time while the program is open.
pub struct Form {
    format: Option<Format>,
    options: Options,
    size: SizeKind,
    percent: u32,
    fit: (u32, u32),
    filter: Filter,
    enlarge: bool,
    flip_h: bool,
    flip_v: bool,
    /// Clockwise quarter turns, 0 to 3.
    quarter: u8,
    place: Place,
    sub_name: String,
    /// Address of the other folder, as the address bar takes it.
    other: String,
    /// Browse… was pressed: the chooser opens after drawing.
    browse: bool,
    /// The folder chooser, open in place of the choices.
    picker: Option<Picker>,
    /// What the selection holds, set when the window opens.
    images: bool,
    videos: bool,
    video: VideoForm,
}

/// The choices for the videos.
struct VideoForm {
    /// Index in `CONTAINERS`.
    container: usize,
    codec: Codec,
    /// One of `codec`'s encoders that works here, once checked.
    encoder: Option<&'static str>,
    quality: Quality,
    audio: bool,
    meta: bool,
    /// FFmpeg being downloaded (`component::FFMPEG`).
    download: Option<Arc<crate::component::Download>>,
}

impl Form {
    fn new(sub_name: String) -> Form {
        Form {
            format: Some(Format::Jpeg),
            options: Options::default(),
            size: SizeKind::Same,
            percent: 50,
            fit: (1920, 1080),
            filter: Filter::Lanczos3,
            enlarge: false,
            flip_h: false,
            flip_v: false,
            quarter: 0,
            place: Place::Sub,
            sub_name,
            other: String::new(),
            browse: false,
            picker: None,
            images: true,
            videos: false,
            video: VideoForm { container: 0, codec: Codec::H264, encoder: None, quality: Quality::Medium, audio: true, meta: true, download: None },
        }
    }

    fn job(&self) -> Job {
        let size = match self.size {
            SizeKind::Same => Size::Same,
            SizeKind::Percent => Size::Percent(self.percent),
            SizeKind::Fit => Size::Fit(self.fit.0, self.fit.1),
        };
        let v = &self.video;
        let video = v.encoder.filter(|_| self.videos).map(|encoder| Video { container: v.container, encoder, quality: v.quality, audio: v.audio, meta: v.meta });
        Job { format: self.format, options: self.options, size, filter: self.filter, turn: self.turn(), enlarge: self.enlarge, video }
    }

    /// The flips first, then the turn.
    fn turn(&self) -> Turn {
        let mut turn = Turn::default();
        for (on, op) in [(self.flip_h, Op::FlipH), (self.flip_v, Op::FlipV)] {
            if on {
                turn = turn.then(op);
            }
        }
        (0..self.quarter).fold(turn, |t, _| t.then(Op::Cw))
    }

    /// Why Convert cannot start yet (Fluent id), if it cannot.
    fn why_not(&self) -> Option<&'static str> {
        let name = self.sub_name.trim();
        if self.images && self.job().does_nothing() {
            Some("batch-nothing")
        } else if self.videos && self.job().video.is_none() {
            Some("batch-video-not-ready")
        } else if self.place == Place::Sub && (name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\'])) {
            Some("batch-bad-folder")
        } else if self.place == Place::Other && self.other.trim().is_empty() {
            Some("batch-no-dest")
        } else {
            None
        }
    }
}

/// The open window: the images, their folder, and the work once started.
pub struct Window {
    paths: Vec<String>,
    dir: String,
    progress: Option<Arc<Progress>>,
    /// Where the files go, as shown; the folder to read again at the end, if it is on this source.
    dest: (String, Option<String>),
}

impl Window {
    /// The choices are open (keys answer them).
    pub fn asking(&self) -> bool {
        self.progress.is_none()
    }
}

impl App {
    /// From the menus: the marked images of the grid, or `path`, or the selected one.
    pub(crate) fn open_batch(&mut self, path: Option<String>) {
        if self.batch.is_some() {
            return;
        }
        let paths = self.grid_items(path);
        if paths.is_empty() {
            return;
        }
        let form = self.batch_form.get_or_insert_with(|| Form::new(self.i18n.t("batch-subfolder-name")));
        form.videos = paths.iter().any(|p| crate::decode::is_video(p));
        form.images = paths.iter().any(|p| !crate::decode::is_video(p));
        if form.other.is_empty() {
            form.other = crate::navigation::shown(&self.base, &self.dir);
        }
        self.batch = Some(Window { paths, dir: self.dir.clone(), progress: None, dest: Default::default() });
    }

    /// "Copy with the metadata": the window on "Same as the original" (streams copied, metadata
    /// written), the destination left to choose.
    pub(crate) fn open_copy_meta(&mut self, path: Option<String>) {
        self.open_batch(path);
        if let Some(form) = self.batch_form.as_mut().filter(|_| self.batch.is_some()) {
            if !CONTAINERS[form.video.container].codecs.contains(&Codec::Copy) {
                form.video.container = 0;
            }
            form.video.codec = Codec::Copy;
        }
    }

    pub(crate) fn batch_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.batch.take() else { return };
        let keep = match w.progress.clone() {
            None => self.batch_ask(ctx, &mut w),
            Some(p) => self.batch_progress(ctx, &w, &p),
        };
        if keep {
            self.batch = Some(w);
        }
    }

    /// The choices; Convert starts the work. `false` once cancelled.
    fn batch_ask(&mut self, ctx: &egui::Context, w: &mut Window) -> bool {
        let Some(mut form) = self.batch_form.take() else { return false };
        let n = w.paths.len().to_string();
        let intro = self.i18n.t_args("batch-images", &[("n", &n), ("folder", &md(&self.display(&w.dir)))]);
        let keys = self.delete_answer.take();
        let (places, rt) = (self.pick_places(), self.rt.handle().clone());
        let (mut answer, mut picked) = (None, None);
        egui::Modal::new(egui::Id::unique("batch-ask")).show(ctx, |ui| {
            ui.set_width(680.0);
            if let Some(p) = &mut form.picker {
                picked = pick_ui(ui, p, &places, &self.i18n, &rt, keys);
                return;
            }
            crate::theme::title(ui, self.i18n.t("batch-title"));
            ui.add_space(6.0);
            notice_text(ui, &intro);
            answer = form_ui(ui, &mut form, &self.i18n);
        });
        if form.picker.is_some() {
            if let Some(choice) = picked {
                form.other = choice.unwrap_or(form.other);
                form.picker = None;
            }
            self.batch_form = Some(form);
            return true;
        }
        if std::mem::take(&mut form.browse) {
            // From the folder written, or the open one.
            let start = if form.other.trim().is_empty() { self.display(&w.dir) } else { form.other.trim().to_string() };
            form.picker = Some(Picker::open(&start, &rt, ctx));
        }
        let ready = form.why_not().is_none();
        let answer = answer.or(keys).filter(|&yes| !yes || ready);
        let keep = match answer {
            Some(true) => match self.batch_dest(&form, &w.dir) {
                Ok((to, dest)) => {
                    w.dest = dest;
                    self.start_batch(w, form.job(), to, ctx);
                    true
                }
                // The other folder cannot be opened: said in the error bar, the window stays open.
                Err((id, detail)) => {
                    self.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                    true
                }
            },
            Some(false) => false,
            None => true,
        };
        self.batch_form = Some(form);
        keep
    }

    /// Where the files go: the store and folder, how it is shown, and the folder to read again at
    /// the end when it is on the source open.
    fn batch_dest(&self, f: &Form, dir: &str) -> Result<(Dest, (String, Option<String>)), source::Error> {
        let Some(loader) = &self.loader else { return Err(("error-source", String::new())) };
        let here = |dir: String, create: bool| (Dest { op: loader.store(), dir: dir.clone(), create }, (self.display(&dir), Some(dir)));
        match f.place {
            Place::Next => Ok(here(dir.to_string(), false)),
            Place::Sub => Ok(here(format!("{dir}{}/", f.sub_name.trim()), true)),
            Place::Other => {
                let address = f.other.trim();
                let s = source::open(address)?;
                if s.start.is_some() {
                    return Err(("batch-not-folder", address.to_string()));
                }
                let again = (s.base == self.base).then(|| s.dir.clone());
                Ok((Dest { op: s.op, dir: s.dir.clone(), create: false }, (crate::navigation::shown(&s.base, &s.dir), again)))
            }
        }
    }

    fn start_batch(&self, w: &mut Window, job: Job, to: Dest, ctx: &egui::Context) {
        let Some(loader) = &self.loader else { return };
        let p = Arc::new(Progress::default());
        w.progress = Some(p.clone());
        log!("batch: {} images to {:?}, {job:?}", w.paths.len(), w.dest.0);
        let (op, paths, ctx) = (loader.store(), w.paths.clone(), ctx.clone());
        self.rt.spawn(async move {
            crate::batch::run(&op, paths, to, job, p).await;
            ctx.request_repaint();
        });
    }

    /// How the work is going, in a corner (the program can be used meanwhile); at the end what was
    /// done, in a message. `false` once finished.
    fn batch_progress(&mut self, ctx: &egui::Context, w: &Window, p: &Progress) -> bool {
        if p.finished.load(Ordering::Relaxed) {
            self.batch_done(w, p);
            return false;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed).max(1));
        // A video takes long: its own progress moves the bar between two files.
        let part = p.part.load(Ordering::Relaxed) as f32 / 1000.0;
        let counts = self.i18n.t_args("batch-counts", &[("n", &done.to_string()), ("total", &total.to_string())]);
        let current = p.current.lock().unwrap().clone();
        let stopping = p.cancel.load(Ordering::Relaxed);
        egui::Window::new(self.i18n.t("batch-working"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -40.0))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(counts);
                ui.add(egui::ProgressBar::new(((done as f32 + part) / total as f32).min(1.0)).show_percentage());
                ui.label(egui::RichText::new(current).weak());
                if stopping {
                    ui.label(self.i18n.t("batch-stopping"));
                } else if ui.button(self.i18n.t("delete-no")).clicked() {
                    p.cancel.store(true, Ordering::Relaxed);
                }
            });
        true
    }

    /// What was saved and where, what was left out and why; the folders shown again.
    fn batch_done(&mut self, w: &Window, p: &Progress) {
        let saved = p.saved.lock().unwrap().len();
        let failed = std::mem::take(&mut *p.failed.lock().unwrap());
        log!("batch: {saved} saved, {} left out", failed.len());
        let mut text = self.i18n.t_args("batch-result", &[("n", &saved.to_string()), ("folder", &md(&w.dest.0))]);
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
        if done < total {
            text = format!("{text}\n\n{}", self.i18n.t_args("batch-cancelled", &[("n", &done.to_string()), ("total", &total.to_string())]));
        }
        if !failed.is_empty() {
            let lines: String = failed.iter().map(|(id, detail)| format!("\n- {}", self.i18n.t_args(id, &[("detail", &md(detail))]))).collect();
            text = format!("{text}\n\n{}{lines}", self.i18n.t("batch-left-out"));
        }
        self.notice = Some((text, None));
        self.relist(w.dir.clone());
        if let Some(dest) = w.dest.1.clone().filter(|d| *d != w.dir) {
            self.relist(dest);
        }
    }
}

/// The choices of the window; `Some(true)` Convert, `Some(false)` Cancel.
fn form_ui(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) -> Option<bool> {
    let t = |id| i18n.t(id);
    ui.spacing_mut().item_spacing.y = 6.0;
    // On a low screen the choices scroll, the buttons stay in view.
    let max = (ui.ctx().content_rect().height() - 170.0).max(200.0);
    egui::ScrollArea::vertical().max_height(max).show(ui, |ui| {
        if f.images {
            section(ui, t("convert-format"), |ui| format_rows(ui, f, i18n));
        }
        if f.videos {
            section(ui, t("batch-video"), |ui| video_rows(ui, &mut f.video, i18n));
        }
        // Copied streams keep their turn and size: with videos only there is nothing to choose.
        let fixed = f.videos && !f.images && f.video.codec == Codec::Copy;
        ui.add_enabled_ui(!fixed, |ui| {
            section(ui, t("batch-turn"), |ui| turn_rows(ui, f, i18n));
            section(ui, t("batch-size"), |ui| size_rows(ui, f, i18n));
        });
        section(ui, t("batch-where"), |ui| where_rows(ui, f, i18n));
    });
    ui.add_space(14.0);
    let why_not = f.why_not();
    if let Some(id) = why_not {
        ui.label(crate::browser::error_text(ui, t(id)));
        ui.add_space(4.0);
    }
    let mut answer = None;
    ui.horizontal(|ui| {
        let start = crate::theme::main_button(ui, t("batch-start"));
        if ui.add_enabled(why_not.is_none(), start).clicked() {
            answer = Some(true);
        }
        if ui.button(t("delete-no")).clicked() {
            answer = Some(false);
        }
    });
    answer
}

/// A part of the window: its title, then what is in it, set in a little.
fn section(ui: &mut egui::Ui, title: String, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(14.0);
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.add_space(4.0);
    ui.indent(ui.next_auto_id(), add);
}

fn format_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let label = |format: Option<Format>| format.map_or_else(|| t("batch-same-format"), |f| f.label().to_string());
    egui::ComboBox::from_id_salt("batch-format").selected_text(label(f.format)).show_ui(ui, |ui| {
        ui.selectable_value(&mut f.format, None, t("batch-same-format"));
        for format in Format::ALL {
            ui.selectable_value(&mut f.format, Some(format), format.label());
        }
    });
    match f.format {
        None => {
            ui.weak(t("batch-same-format-hint"));
        }
        Some(format) => {
            f.options.format = format;
            // Closed at first: the window stays short enough to show every part.
            let title = i18n.t_args("batch-format-options", &[("format", format.label())]);
            egui::CollapsingHeader::new(title).id_salt("batch-format-options").show(ui, |ui| {
                // Several images: the options shown as for one with transparency and larger than an icon.
                crate::convert::format_options(ui, &mut f.options, true, true, i18n);
                if format.holds_meta() {
                    ui.checkbox(&mut f.options.keep_meta, t("convert-keep-meta"));
                }
            });
        }
    }
}

/// Container, codec, encoder (only those that work here), quality, sound and metadata; FFmpeg
/// downloaded first where there is none.
fn video_rows(ui: &mut egui::Ui, v: &mut VideoForm, i18n: &I18n) {
    let t = |id| i18n.t(id);
    if crate::ffmpeg::program().is_none() {
        return ffmpeg_download(ui, v, i18n);
    }
    let ctx = ui.ctx().clone();
    crate::ffmpeg::start_probe(move || ctx.request_repaint());
    let Some(probe) = crate::ffmpeg::probed() else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(t("batch-video-checking"));
        });
        return;
    };
    egui::ComboBox::from_label(t("batch-video-container")).selected_text(CONTAINERS[v.container].name).show_ui(ui, |ui| {
        for (i, c) in CONTAINERS.iter().enumerate() {
            ui.selectable_value(&mut v.container, i, c.name);
        }
    });
    let c = &CONTAINERS[v.container];
    if !c.codecs.contains(&v.codec) || probe.of(v.codec).is_empty() {
        v.codec = c.codecs.iter().copied().find(|k| !probe.of(*k).is_empty()).unwrap_or(c.codecs[0]);
    }
    let label = |k: Codec| if k == Codec::Copy { t("batch-video-copy") } else { k.label().to_string() };
    egui::ComboBox::from_label(t("batch-video-codec")).selected_text(label(v.codec)).show_ui(ui, |ui| {
        for &k in c.codecs {
            let usable = !probe.of(k).is_empty();
            let label = if usable { label(k) } else { i18n.t_args("batch-video-unavailable", &[("codec", k.label())]) };
            ui.add_enabled_ui(usable, |ui| ui.selectable_value(&mut v.codec, k, label));
        }
    });
    ui.weak(t(v.codec.hint()));
    let encoders = probe.of(v.codec);
    if encoders.is_empty() {
        v.encoder = None;
        ui.label(crate::browser::error_text(ui, t("batch-video-none")));
        return;
    }
    if !v.encoder.is_some_and(|e| encoders.contains(&e)) {
        v.encoder = Some(encoders[0]);
    }
    // Copied streams: nothing to choose about the encoder or the quality.
    if v.codec != Codec::Copy {
        let name = |e: &str| format!("{} ({e})", crate::ffmpeg::maker(e));
        egui::ComboBox::from_label(t("batch-video-encoder")).selected_text(name(v.encoder.unwrap_or_default())).show_ui(ui, |ui| {
            for e in encoders {
                ui.selectable_value(&mut v.encoder, Some(e), name(e));
            }
        });
        ui.weak(t(crate::ffmpeg::hint(v.encoder.unwrap_or_default())));
        ui.horizontal(|ui| {
            ui.label(t("batch-video-quality"));
            for (q, id) in [(Quality::High, "batch-video-high"), (Quality::Medium, "batch-video-medium"), (Quality::Small, "batch-video-small")] {
                ui.radio_value(&mut v.quality, q, t(id));
            }
        });
    }
    // Copied as it is: sound and metadata stay, no choice.
    let copy = v.codec == Codec::Copy;
    if copy {
        (v.audio, v.meta) = (true, true);
    }
    ui.add_enabled_ui(!copy, |ui| {
        if !c.audio.is_empty() {
            ui.checkbox(&mut v.audio, t("batch-video-audio"));
        }
        ui.checkbox(&mut v.meta, t("batch-video-meta"));
    });
}

/// No usable FFmpeg on this computer (none, or too old): what it is, its size and the button;
/// then the progress. With "the system's only" in the Options, just the reason.
fn ffmpeg_download(ui: &mut egui::Ui, v: &mut VideoForm, i18n: &I18n) {
    let Some(c) = crate::component::FFMPEG else { return };
    let size = format!("{} MB", c.size / 1_000_000);
    match crate::ffmpeg::status() {
        crate::engine::Status::SystemOnly => {
            ui.label(crate::browser::error_text(ui, i18n.t("batch-video-system-only")));
            return;
        }
        crate::engine::Status::TooOld { found, required } => {
            ui.label(i18n.t_args("batch-video-old-ffmpeg", &[("found", &found), ("required", required), ("size", &size)]));
        }
        _ => {
            ui.label(i18n.t_args("batch-video-need-ffmpeg", &[("size", &size)]));
        }
    }
    let state = v.download.as_ref().map(|d| (d.done.load(Ordering::Relaxed), d.result.lock().unwrap().clone()));
    match state {
        Some((done, None)) => {
            ui.add(egui::ProgressBar::new(done as f32 / c.size as f32).show_percentage());
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        // Done: `ffmpeg::program` finds it at the next frame.
        Some((_, Some(Ok(())))) => ui.ctx().request_repaint(),
        failed => {
            if let Some((_, Some(Err(e)))) = failed {
                ui.label(crate::browser::error_text(ui, i18n.t_args("batch-video-download-failed", &[("detail", &e)])));
            }
            if ui.button(i18n.t("batch-video-download")).clicked() {
                let ctx = ui.ctx().clone();
                v.download = Some(crate::component::start(c, move || ctx.request_repaint()));
            }
        }
    }
}

fn turn_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.horizontal_wrapped(|ui| {
        for (q, id) in [(0, "batch-turn-none"), (1, "batch-turn-cw"), (2, "batch-turn-180"), (3, "batch-turn-ccw")] {
            ui.radio_value(&mut f.quarter, q, t(id));
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut f.flip_h, t("tf-flip-h"));
        ui.checkbox(&mut f.flip_v, t("tf-flip-v"));
    });
    if f.quarter != 0 && (f.flip_h || f.flip_v) {
        ui.weak(t("batch-turn-order"));
    }
}

fn size_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.radio_value(&mut f.size, SizeKind::Same, t("batch-size-same"));
    ui.horizontal(|ui| {
        ui.radio_value(&mut f.size, SizeKind::Percent, t("batch-size-percent"));
        ui.add_enabled(f.size == SizeKind::Percent, egui::DragValue::new(&mut f.percent).range(1..=1000).suffix(" %"));
    });
    ui.horizontal(|ui| {
        ui.radio_value(&mut f.size, SizeKind::Fit, t("batch-size-fit"));
        let on = f.size == SizeKind::Fit;
        ui.add_enabled(on, egui::DragValue::new(&mut f.fit.0).range(1..=65535));
        ui.label("×");
        ui.add_enabled(on, egui::DragValue::new(&mut f.fit.1).range(1..=65535));
        ui.label(t("batch-pixels"));
    });
    ui.add_space(4.0);
    ui.add_enabled_ui(f.size != SizeKind::Same, |ui| {
        ui.checkbox(&mut f.enlarge, t("batch-enlarge"));
        if !f.images {
            return;
        }
        egui::ComboBox::from_label(t("batch-filter")).selected_text(t(f.filter.label())).show_ui(ui, |ui| {
            for filter in Filter::ALL {
                ui.selectable_value(&mut f.filter, filter, t(filter.label()));
            }
        });
    });
}

fn where_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.radio_value(&mut f.place, Place::Next, t("batch-same-folder"));
    ui.horizontal(|ui| {
        ui.radio_value(&mut f.place, Place::Sub, t("batch-subfolder"));
        ui.add_enabled(f.place == Place::Sub, egui::TextEdit::singleline(&mut f.sub_name).desired_width(200.0));
    });
    ui.radio_value(&mut f.place, Place::Other, t("batch-other"));
    ui.add_enabled_ui(f.place == Place::Other, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.other).desired_width(360.0));
            if ui.button(t("batch-browse")).clicked() {
                f.browse = true;
            }
        });
        ui.weak(t("batch-other-hint"));
    });
    ui.add_space(4.0);
    ui.weak(t("batch-keep"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_checks() {
        let mut f = Form::new("converted".into());
        assert_eq!(f.why_not(), None);
        f.format = None;
        assert_eq!(f.why_not(), Some("batch-nothing"));
        f.size = SizeKind::Percent;
        assert_eq!(f.why_not(), None);
        assert_eq!(f.job().size, Size::Percent(50));
        for bad in ["", "  ", "a/b", "..", "a\\b"] {
            f.sub_name = bad.into();
            assert_eq!(f.why_not(), Some("batch-bad-folder"), "{bad:?}");
        }
        f.place = Place::Next;
        assert_eq!(f.why_not(), None);
        f.place = Place::Other;
        assert_eq!(f.why_not(), Some("batch-no-dest"));
        f.other = "/tmp".into();
        assert_eq!(f.why_not(), None);
        // Only turned: something to do. Flips first, then the turn.
        f.size = SizeKind::Same;
        f.quarter = 1;
        f.flip_h = true;
        assert_eq!(f.why_not(), None);
        assert_eq!(f.job().turn, Turn::default().then(Op::FlipH).then(Op::Cw));
    }
}
