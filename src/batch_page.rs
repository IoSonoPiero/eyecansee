//! Convert and resize several images: the window with the choices, then how the work is going,
//! then what was done. The work itself is in `batch`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui;

use crate::audio_job::AudioJob;
use crate::batch::{Dest, Drawn, Filter, Job, Progress, Size};
use crate::cad_convert::Target;
use crate::encode::{Format, Options};
use crate::batch_video::{VideoForm, audio_rows, video_rows};
use crate::ffmpeg::{CONTAINERS, Codec, Quality, Video};
use crate::folder_pick::{Picker, pick_ui};
use crate::i18n::I18n;
use crate::transform::{Op, Turn};
use crate::save_dialogs::{md, notice_text};
use crate::selection::{Kind, PLAYED, left_out_id, split};
use crate::{App, source};

mod rows;

use rows::form_ui;
pub(crate) use rows::size_fields;

/// Where the new files go.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    Next,
    Sub,
    /// A folder written or chosen: on this computer or on another device.
    Other,
}

/// The choices, kept for the next time while the program is open.
pub struct Form {
    format: Option<Format>,
    options: Options,
    /// One size of each kind with its measures (`Size::KINDS`), and which one is chosen.
    sizes: [Size; 9],
    size: usize,
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
    /// JPEGs among them: in another format an Ultra HDR one loses its HDR (#65).
    jpegs: bool,
    videos: bool,
    video: VideoForm,
    /// CAD drawings exported (`Drawn`): the window shows just their format, size and destination.
    drawing: bool,
    /// Not a picture: SVG or the other AutoCAD format (`None`: a picture of `side` pixels).
    kept: Option<Drawn>,
    /// Long side of the pictures, in pixels.
    side: u32,
    /// Only the audio of the videos: the window shows just FFmpeg and the destination.
    audio: Option<AudioJob>,
    /// The part the window was last opened on: the format (Convert…) or the size (Resize >
    /// Custom…); `scroll`: it scrolls there when it opens.
    part: Part,
    scroll: bool,
    /// The format chosen for Convert…, given back after Resize > Custom… set Same as the original.
    format_before: Option<Format>,
}

/// Where the window opens.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    Format,
    Size,
}

impl Form {
    fn new(sub_name: String) -> Form {
        Form {
            format: Some(Format::Jpeg),
            options: Options::default(),
            sizes: Size::KINDS,
            size: 0,
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
            jpegs: false,
            videos: false,
            video: VideoForm { container: 0, codec: Codec::H264, encoder: None, quality: Quality::Medium, audio: true, meta: true, download: None },
            drawing: false,
            kept: None,
            side: 4096,
            audio: None,
            part: Part::Format,
            scroll: false,
            format_before: Some(Format::Jpeg),
        }
    }

    fn job(&self) -> Job {
        let size = self.sizes[self.size];
        let v = &self.video;
        let video = v.encoder.filter(|_| self.videos).map(|encoder| Video { container: v.container, encoder, quality: v.quality, audio: v.audio, meta: v.meta });
        let drawing = self.drawing.then_some(self.kept.unwrap_or(Drawn::Side(self.side)));
        Job { format: self.format, options: self.options, size, filter: self.filter, turn: self.turn(), enlarge: self.enlarge, video, audio: self.audio, base: String::new(), drawing, tag: String::new() }
    }

    /// On videos only, or only their sound: F1 opens the page on converting videos.
    pub(crate) fn videos_only(&self) -> bool {
        self.audio.is_some() || self.videos && !self.images
    }

    /// On CAD drawings, exported.
    pub(crate) fn drawings(&self) -> bool {
        self.drawing
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
        let media = match self.audio {
            Some(_) => crate::ffmpeg::copier().is_none().then_some("batch-audio-not-ready"),
            None if self.images && self.job().does_nothing() => Some("batch-nothing"),
            None if self.videos && self.job().video.is_none() => Some("batch-video-not-ready"),
            None => None,
        };
        if media.is_some() {
            media
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
    /// Files chosen that this window does not work on (not of `kinds`): their number said under
    /// the count, their names in the message at the end.
    left_out: Vec<String>,
    kinds: &'static [Kind],
    /// Resize with a preset (no window): the title of the message at the end.
    title: Option<&'static str>,
}

impl Form {
    /// The Fluent id of the window's title, and of the message at the end.
    fn title(&self) -> &'static str {
        match self.audio {
            Some(AudioJob::Strip) => "batch-title-strip",
            Some(AudioJob::Extract) => "batch-title-extract",
            None if self.drawing => "drawing-title",
            None => "batch-title",
        }
    }
}

impl Window {
    /// The choices are open (keys answer them).
    pub fn asking(&self) -> bool {
        self.progress.is_none()
    }
}

impl App {
    /// Convert…, on the part of the format: the marked images of the grid, or `path`, or the
    /// selected one. Pictures and videos only: audio files are not converted ("Save the sound as
    /// a file" makes them as they are), CAD drawings are exported (`open_drawings`), 3D models too.
    pub(crate) fn open_batch(&mut self, path: Option<String>) {
        if !self.open_batch_of(path, false) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            if form.part == Part::Size {
                form.format = form.format_before;
            }
            (form.part, form.scroll) = (Part::Format, true);
        }
    }

    /// "Export drawing": the CAD drawings among the marked files, drawn again.
    pub(crate) fn open_drawings(&mut self, path: Option<String>) {
        self.open_batch_of(path, true);
    }

    /// `false`: not opened (one already open, or no file of its kinds).
    fn open_batch_of(&mut self, path: Option<String>, drawings: bool) -> bool {
        if self.batch.is_some() {
            return false;
        }
        let kinds: &'static [Kind] = if drawings { &[Kind::Drawing] } else { PLAYED };
        let (paths, left_out) = split(self.grid_items(path), kinds);
        if paths.is_empty() {
            return false;
        }
        let form = self.batch_form.get_or_insert_with(|| Form::new(self.i18n.t("batch-subfolder-name")));
        form.audio = None;
        form.drawing = drawings;
        form.videos = paths.iter().any(|p| crate::decode::is_video(p));
        form.images = !drawings && paths.iter().any(|p| !crate::decode::is_video(p));
        form.jpegs = paths.iter().any(|p| Format::of_file(p) == Some(Format::Jpeg));
        // The sizes for pictures only are off with videos.
        if form.videos && !form.sizes[form.size].for_videos() {
            form.size = 0;
        }
        if form.other.is_empty() {
            form.other = crate::navigation::shown(&self.base, &self.dir);
        }
        self.batch = Some(Window { paths, dir: self.dir.clone(), progress: None, dest: Default::default(), left_out, kinds, title: None });
        true
    }

    /// "Copy with the metadata": the window on "Same as the original" (streams copied, metadata
    /// written), the destination left to choose.
    pub(crate) fn open_copy_meta(&mut self, path: Option<String>) {
        if !self.open_batch_of(path, false) || !self.batch_only(&[Kind::Video]) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            form.images = false;
            if !CONTAINERS[form.video.container].codecs.contains(&Codec::Copy) {
                form.video.container = 0;
            }
            form.video.codec = Codec::Copy;
        }
    }

    /// "Copy without sound" and "Save the sound as a file": the window on the videos chosen, with only the
    /// destination to choose, next to the originals at first.
    pub(crate) fn open_audio(&mut self, job: AudioJob, path: Option<String>) {
        if !self.open_batch_of(path, false) || !self.batch_only(&[Kind::Video]) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            form.audio = Some(job);
            form.images = false;
            form.place = Place::Next;
        }
    }

    /// Resize > Custom…: the window on the pictures chosen, on the part of the size, in their own
    /// format, on a size of Long side at first.
    pub(crate) fn open_resize(&mut self, path: Option<String>) {
        if !self.open_batch_of(path, false) || !self.batch_only(&[Kind::Picture]) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            if form.part == Part::Format {
                form.format_before = form.format;
            }
            (form.images, form.videos, form.format, form.part, form.scroll) = (true, false, None, Part::Size, true);
            if form.size == 0 {
                form.size = Size::KINDS.iter().position(|k| matches!(k, Size::Long(_))).unwrap_or(0);
            }
        }
    }

    /// The window just opened kept on the files of `kinds`, the others left out.
    /// `false`: none left, the window closed.
    fn batch_only(&mut self, kinds: &'static [Kind]) -> bool {
        let Some(w) = self.batch.as_mut() else { return false };
        let (paths, out) = split(std::mem::take(&mut w.paths), kinds);
        (w.paths, w.kinds) = (paths, kinds);
        w.left_out.extend(out);
        if w.paths.is_empty() {
            self.batch = None;
        }
        self.batch.is_some()
    }

    /// Resize > a preset: the pictures chosen resized at once, without a window, next to the
    /// originals, in their format with their metadata, the size in their names ("photo (1920 px).jpg").
    pub(crate) fn resize_now(&mut self, ctx: &egui::Context, size: Size, path: Option<String>) {
        let Some(loader) = &self.loader else { return };
        if self.batch.is_some() {
            return;
        }
        let kinds: &'static [Kind] = &[Kind::Picture];
        let (paths, left_out) = split(self.grid_items(path), kinds);
        if paths.is_empty() {
            return;
        }
        let dir = self.dir.clone();
        let to = Dest { op: loader.store(), dir: dir.clone(), create: false };
        let mut w = Window { paths, dir: dir.clone(), progress: None, dest: (self.display(&dir), Some(dir)), left_out, kinds, title: Some("preset-title") };
        let job = Job { format: None, size, tag: size.tag(), ..Job::default() };
        self.start_batch(&mut w, job, to, ctx);
        self.batch = Some(w);
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
        let title = self.i18n.t(form.title());
        let mut intro = self.i18n.t_args("batch-images", &[("n", &n), ("folder", &md(&self.display(&w.dir)))]);
        if !w.left_out.is_empty() {
            intro = format!("{intro} {}", self.i18n.t_args("batch-left-out-kind", &[("n", &w.left_out.len().to_string())]));
        }
        let keys = self.delete_answer.take();
        let (places, rt) = (self.pick_places(), self.rt.handle().clone());
        let (mut answer, mut picked) = (None, None);
        egui::Modal::new(egui::Id::unique("batch-ask")).show(ctx, |ui| {
            ui.set_width(680.0);
            if let Some(p) = &mut form.picker {
                picked = pick_ui(ui, p, &places, &self.i18n, &rt, keys);
                return;
            }
            crate::theme::title(ui, title);
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
                    self.error = Some(self.i18n.error(id, &detail));
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

    fn start_batch(&self, w: &mut Window, mut job: Job, to: Dest, ctx: &egui::Context) {
        let Some(loader) = &self.loader else { return };
        job.base = self.base.clone();
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
        // A file just finished: its folder read again, so its thumbnail (`batch::ready`) shows now.
        let saved = p.saved.lock().unwrap().len();
        if p.listed.swap(saved, Ordering::Relaxed) < saved
            && let Some(dest) = w.dest.1.clone()
        {
            self.relist(dest);
        }
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
        // The files of another kind first, then those the work left out.
        let other_kind = w.left_out.iter().map(|path| (left_out_id(w.kinds), source::name(path).to_string()));
        let failed: Vec<_> = other_kind.chain(std::mem::take(&mut *p.failed.lock().unwrap())).collect();
        log!("batch: {saved} saved, {} left out", failed.len());
        for (id, detail) in &failed {
            log!("batch: left out: {id} {detail}");
        }
        let mut text = self.i18n.t_args("batch-result", &[("n", &saved.to_string()), ("folder", &md(&w.dest.0))]);
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
        if done < total {
            text = format!("{text}\n\n{}", self.i18n.t_args("batch-cancelled", &[("n", &done.to_string()), ("total", &total.to_string())]));
        }
        if !failed.is_empty() {
            // Resize has no format to choose: its own words for a format not written.
            let said = |id: &'static str| if w.title.is_some() && id == crate::batch::SKIP_FORMAT { "preset-skip-format" } else { id };
            let lines: String = failed.iter().map(|(id, detail)| format!("\n- {}", self.i18n.error_md(said(id), detail))).collect();
            text = format!("{text}\n\n{}{lines}", self.i18n.t("batch-left-out"));
        }
        let title = w.title.unwrap_or_else(|| self.batch_form.as_ref().map_or("batch-title", Form::title));
        self.tell(title, text);
        self.relist(w.dir.clone());
        if let Some(dest) = w.dest.1.clone().filter(|d| *d != w.dir) {
            self.relist(dest);
        }
    }
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
        f.size = 1;
        assert_eq!(f.why_not(), None);
        assert_eq!(f.job().size, Size::Percent(50));
        f.size = 7;
        assert_eq!(f.job().size, Size::Crop(1920, 1080));
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
        f.size = 0;
        f.quarter = 1;
        f.flip_h = true;
        assert_eq!(f.why_not(), None);
        assert_eq!(f.job().turn, Turn::default().then(Op::FlipH).then(Op::Cw));
    }
}
