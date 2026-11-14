//! Convert and resize several images: the window with the choices, then how the work is going,
//! then what was done. The work itself is in `batch`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui;

use crate::audio_job::AudioJob;
use crate::batch::{Dest, Drawn, Filter, Job, Progress, Size};
use crate::cad_convert::Target;
use crate::encode::{Format, Options};
use crate::audio_convert::Sound;
use crate::batch_video::{SoundForm, VideoForm, audio_rows, sound_rows, video_rows};
use crate::ffmpeg::{Codec, Quality, Video};
use crate::folder_pick::{Picker, pick_ui};
use crate::i18n::I18n;
use crate::transform::{Op, Turn};
use crate::save_dialogs::{md, notice_text};
use crate::selection::{CONVERTED, Kind, PLAYED, left_out_id, split};
use crate::{App, source};

mod rows;

use rows::form_ui;
pub(crate) use rows::size_fields;

/// Where the new files go.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Place {
    Next,
    Sub,
    /// A folder written or chosen: on this computer or on another device.
    Other,
}

/// The choices, kept for the next time while the program is open; those of a window cancelled
/// are forgotten (`Form::cancelled`).
#[derive(Clone)]
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
    /// Audio files among them, and what they become (#120).
    audios: bool,
    sound: SoundForm,
    /// CAD drawings exported (`Drawn`): the window shows just their format, size and destination.
    drawing: bool,
    /// Not a picture: SVG or the other AutoCAD format (`None`: a picture of `side` pixels).
    kept: Option<Drawn>,
    /// Long side of the pictures, in pixels.
    side: u32,
    /// Only the audio of the videos: the window shows just FFmpeg and the destination.
    audio: Option<AudioJob>,
    /// What the window was last opened for (`Form::open_on`); `scroll`: it scrolls to its part
    /// when it opens.
    part: Part,
    scroll: bool,
    /// The format and the destination of Convert…, given back after another use of the window
    /// changed them.
    convert: (Option<Format>, Place),
    /// The size (in `sizes`), the measures and the turn of the last Resize > Custom…: quarter
    /// turns, flips.
    resize: (usize, [Size; 9], (u8, bool, bool)),
}

/// What the window is opened for.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Part {
    /// Convert… (and Export drawing): on the format.
    Convert,
    /// Resize > Custom…: on the size.
    Resize,
    CopyMeta,
    /// Copy without sound, Save the sound as a file.
    Audio,
}

/// The index in `Form::sizes` of Long side, the first size of Resize > Custom….
const LONG: usize = 3;

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
            video: VideoForm { container: 0, codec: Codec::H264, encoder: None, quality: Quality::Medium, audio: true, meta: true, download: None, copy_into: Vec::new(), copy_own: 0 },
            audios: false,
            sound: SoundForm { format: 0, quality: Quality::Medium, ready: false },
            drawing: false,
            kept: None,
            side: 4096,
            audio: None,
            part: Part::Convert,
            scroll: false,
            convert: (Some(Format::Jpeg), Place::Sub),
            resize: (LONG, Size::KINDS, (0, false, false)),
        }
    }

    /// Opened for `to`, after `files`: the choices of the use before kept (Convert…'s format and
    /// destination, the size, measures and turn of Custom…), those of `to` set. Custom… on its
    /// last size, measures and turn, the others always on the size of the original (Unchanged),
    /// not turned nor flipped (#119). A size for pictures only is off with videos: only here,
    /// after the choices of Custom… are kept, so that a video never changes them.
    /// Convert… in its own format; Custom… and Copy with the metadata in the format of the
    /// originals; Custom… and the sound of the videos next to the originals.
    fn open_on(&mut self, to: Part) {
        match self.part {
            Part::Convert => self.convert = (self.format, self.place),
            Part::Resize => self.resize = (self.size, self.sizes, (self.quarter, self.flip_h, self.flip_v)),
            Part::CopyMeta | Part::Audio => {}
        }
        (self.part, self.scroll) = (to, true);
        (self.format, self.place) = self.convert;
        if to == Part::Resize {
            (self.size, self.sizes, (self.quarter, self.flip_h, self.flip_v)) = self.resize;
        } else {
            (self.size, (self.quarter, self.flip_h, self.flip_v)) = (0, (0, false, false));
        }
        if self.videos && !self.sizes[self.size].for_videos() {
            self.size = 0;
        }
        match to {
            Part::Convert => {}
            Part::Resize => (self.format, self.place) = (None, Place::Next),
            Part::CopyMeta => self.format = None,
            Part::Audio => self.place = Place::Next,
        }
    }

    /// Copy with the metadata: the videos copied on the originals' container (a .webm as WebM),
    /// MKV for different ones (`video_copy`).
    fn open_copy(&mut self) {
        self.open_on(Part::CopyMeta);
        (self.video.codec, self.video.container) = (Codec::Copy, self.video.copy_own);
    }

    /// What the files chosen are (`paths`; CAD drawings exported with `drawings`), before
    /// `open_on`.
    fn files(&mut self, paths: &[String], drawings: bool) {
        self.audio = None;
        self.drawing = drawings;
        self.videos = paths.iter().any(|p| crate::decode::is_video(p));
        self.video.files(paths.iter().map(String::as_str).filter(|p| crate::decode::is_video(p)));
        self.audios = !drawings && paths.iter().any(|p| crate::decode::is_audio(p));
        self.images = !drawings && paths.iter().any(|p| !crate::decode::is_played(p));
        self.jpegs = paths.iter().any(|p| Format::of_file(p) == Some(Format::Jpeg));
    }

    /// Cancel pressed: the choices as they were `before` the window opened, as if it had never
    /// been touched; only an FFmpeg download started in it goes on.
    fn cancelled(self, before: Form) -> Form {
        let mut form = before;
        form.video.download = self.video.download.or(form.video.download);
        form.picker = None;
        form
    }

    fn job(&self) -> Job {
        let size = self.sizes[self.size];
        let v = &self.video;
        let video = v.encoder.filter(|_| self.videos).map(|encoder| Video { container: v.container, encoder, quality: v.quality, audio: v.audio, meta: v.meta });
        let drawing = self.drawing.then_some(self.kept.unwrap_or(Drawn::Side(self.side)));
        let sound = (self.audios && self.sound.ready).then_some(Sound { format: self.sound.format, quality: self.sound.quality });
        Job { format: self.format, options: self.options, size, filter: self.filter, turn: self.turn(), enlarge: self.enlarge, video, audio: self.audio, sound, base: String::new(), drawing, tag: String::new(), tag_turned: String::new() }
    }

    /// The job with what the names of the copies of Custom… say (`Job::named`): the size of the
    /// pictures resized, as the presets ("photo (600 px).jpg"); for those only turned what was
    /// done, in the language of `i18n` ("photo (rotated).jpg", or "flipped" without a turn).
    /// Nothing for the other uses of the window.
    fn job_named(&self, i18n: &I18n) -> Job {
        let job = self.job();
        if self.part != Part::Resize {
            return job;
        }
        let done = if self.quarter != 0 { "batch-tag-rotated" } else { "batch-tag-flipped" };
        Job { tag: job.size.tag(), tag_turned: format!(" ({})", i18n.t(done)), ..job }
    }

    /// On videos or audio files only, or only the audio of videos: F1 opens the page on
    /// converting videos.
    pub(crate) fn videos_only(&self) -> bool {
        self.audio.is_some() || (self.videos || self.audios) && !self.images
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
            // Copy with the metadata copies the pictures as they are.
            None if self.images && self.part != Part::CopyMeta && self.job().does_nothing() => Some("batch-nothing"),
            None if self.videos && self.job().video.is_none() => Some("batch-video-not-ready"),
            None if self.audios && self.job().sound.is_none() => Some("batch-sound-not-ready"),
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
    /// Resize with a preset, without the window.
    preset: bool,
    /// The choices before the window opened, given back by Cancel (`Form::cancelled`).
    before: Option<Form>,
}

/// The Fluent ids of the title of the window (and of the message at the end), of its button that
/// starts the work, and of the box that shows how the work is going.
fn titles(audio: Option<AudioJob>, drawing: bool, part: Part) -> (&'static str, &'static str, &'static str) {
    match audio {
        Some(AudioJob::Strip) => ("batch-title-strip", "batch-start-copy", "batch-working-copy"),
        Some(AudioJob::Extract) => ("batch-title-extract", "save-yes", "batch-working"),
        None if drawing => ("drawing-title", "drawing-start", "batch-working"),
        None => match part {
            Part::Resize => ("batch-title-resize", "batch-start-resize", "batch-working-resize"),
            Part::CopyMeta => ("batch-title-copy-meta", "batch-start-copy", "batch-working-copy"),
            Part::Convert | Part::Audio => ("batch-title", "batch-start", "batch-working"),
        },
    }
}

impl Form {
    fn titles(&self) -> (&'static str, &'static str, &'static str) {
        titles(self.audio, self.drawing, self.part)
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
    /// selected one. Pictures, videos and audio files (#120); CAD drawings are exported
    /// (`open_drawings`), 3D models too.
    pub(crate) fn open_batch(&mut self, path: Option<String>) {
        if !self.open_batch_of(path, CONVERTED) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            form.open_on(Part::Convert);
        }
    }

    /// "Export drawing": the CAD drawings among the marked files, drawn again, in the format and
    /// to the destination of Convert….
    pub(crate) fn open_drawings(&mut self, path: Option<String>) {
        if self.open_batch_of(path, &[Kind::Drawing])
            && let Some(form) = self.batch_form.as_mut()
        {
            form.open_on(Part::Convert);
        }
    }

    /// On the files of `kinds`. `false`: not opened (one already open, or no file of its kinds).
    fn open_batch_of(&mut self, path: Option<String>, kinds: &'static [Kind]) -> bool {
        if self.batch.is_some() {
            return false;
        }
        let drawings = kinds == [Kind::Drawing];
        // In the viewer the file shown (Export drawing…).
        let (paths, left_out) = split(self.files_chosen(path), kinds);
        if paths.is_empty() {
            return false;
        }
        let form = self.batch_form.get_or_insert_with(|| Form::new(self.i18n.t("batch-subfolder-name")));
        let before = Some(form.clone());
        form.files(&paths, drawings);
        if form.other.is_empty() {
            form.other = crate::navigation::shown(&self.base, &self.dir);
        }
        self.batch = Some(Window { paths, dir: self.dir.clone(), progress: None, dest: Default::default(), left_out, kinds, preset: false, before });
        true
    }

    /// "Copy with the metadata": the window on the pictures and videos chosen, on "Same as the
    /// original" (video streams copied, metadata written; pictures copied as they are), the
    /// destination left to choose.
    pub(crate) fn open_copy_meta(&mut self, path: Option<String>) {
        if !self.open_batch_of(path, PLAYED) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            form.open_copy();
        }
    }

    /// "Copy without sound" and "Save the sound as a file": the window on the videos chosen, with only the
    /// destination to choose, next to the originals at first.
    pub(crate) fn open_audio(&mut self, job: AudioJob, path: Option<String>) {
        if !self.open_batch_of(path, PLAYED) || !self.batch_only(&[Kind::Video]) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            form.open_on(Part::Audio);
            form.audio = Some(job);
            form.images = false;
        }
    }

    /// Resize > Custom…: the window on the pictures chosen, on the part of the size, in their own
    /// format, next to the originals, on the size of the last Custom… (Long side at first).
    pub(crate) fn open_resize(&mut self, path: Option<String>) {
        if !self.open_batch_of(path, PLAYED) || !self.batch_only(&[Kind::Picture]) {
            return;
        }
        if let Some(form) = self.batch_form.as_mut() {
            (form.images, form.videos) = (true, false);
            form.open_on(Part::Resize);
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
        let (paths, left_out) = split(self.grid_chosen(path), kinds);
        if paths.is_empty() {
            return;
        }
        let dir = self.dir.clone();
        let to = Dest { op: loader.store(), dir: dir.clone(), create: false };
        let mut w = Window { paths, dir: dir.clone(), progress: None, dest: (self.display(&dir), Some(dir)), left_out, kinds, preset: true, before: None };
        let job = Job { format: None, size, tag: size.tag(), ..Job::default() };
        self.start_batch(&mut w, job, to, ctx);
        self.batch = Some(w);
    }

    /// The titles of the work of `w` (`titles`): of its window, or of Resize for a preset.
    fn batch_titles(&self, w: &Window) -> (&'static str, &'static str, &'static str) {
        match &self.batch_form {
            _ if w.preset => titles(None, false, Part::Resize),
            Some(form) => form.titles(),
            None => titles(None, false, Part::Convert),
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
        let title = self.i18n.t(form.titles().0);
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
                    self.start_batch(w, form.job_named(&self.i18n), to, ctx);
                    true
                }
                // The other folder cannot be opened: said in the error bar, the window stays open.
                Err((id, detail)) => {
                    self.error = Some(self.i18n.error(id, &detail));
                    true
                }
            },
            Some(false) => {
                if let Some(before) = w.before.take() {
                    form = form.cancelled(before);
                }
                false
            }
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
        egui::Window::new(self.i18n.t(self.batch_titles(w).2))
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
                    log!("batch: Cancel pressed, {done} of {total} done");
                    p.cancel.store(true, Ordering::Relaxed);
                }
            });
        true
    }

    /// What was saved and where, what was left out and why; the folders shown again.
    fn batch_done(&mut self, w: &Window, p: &Progress) {
        let saved = p.saved.lock().unwrap().len();
        // The files of another kind first, then those the work left out.
        let other_kind = w.left_out.iter().map(|path| (left_out_id(w.kinds, path), source::name(path).to_string()));
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
            let said = |id: &'static str| if w.preset && id == crate::batch::SKIP_FORMAT { "preset-skip-format" } else { id };
            let lines: String = failed.iter().map(|(id, detail)| format!("\n- {}", self.i18n.error_md(said(id), detail))).collect();
            text = format!("{text}\n\n{}{lines}", self.i18n.t("batch-left-out"));
        }
        let title = self.batch_titles(w).0;
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

    /// Copied as it is, on the originals' container: a .webm as WebM (it failed in MP4), MKV for
    /// different ones; the copy chosen in Convert… from a container that cannot hold them moves
    /// there.
    #[test]
    fn copy_on_the_originals_container() {
        let named = |c: &str| crate::ffmpeg::CONTAINERS.iter().position(|x| x.name == c).unwrap();
        let mut f = Form::new("x".into());
        f.files(&["/v/Bees on flowers.webm".into(), "/v/a.jpg".into()], false);
        f.open_copy();
        assert_eq!((f.video.codec, f.video.container), (Codec::Copy, named("WebM")));
        f.files(&["/v/a.mp4".into()], false);
        f.open_copy();
        assert_eq!(f.video.container, named("MP4"));
        f.files(&["/v/a.mp4".into(), "/v/b.webm".into()], false);
        f.open_copy();
        assert_eq!((f.video.container, &f.video.copy_into), (named("MKV"), &vec![named("MKV")]));
        f.files(&["/v/b.webm".into()], false);
        f.open_on(Part::Convert);
        (f.video.codec, f.video.container) = (Codec::Copy, named("MP4"));
        f.video.copy_container();
        assert_eq!(f.video.container, named("WebM"));
        f.video.container = named("MKV");
        f.video.copy_container();
        assert_eq!(f.video.container, named("MKV"));
        // Converted: any container.
        (f.video.codec, f.video.container) = (Codec::H264, named("MP4"));
        f.video.copy_container();
        assert_eq!(f.video.container, named("MP4"));
    }

    /// Each window is called as the menu entry that opens it, in every language: "Convert", not
    /// "Convert and resize"; Resize > Custom… opens "Resize".
    #[test]
    fn titles_follow_the_menu() {
        let pairs = [
            ("batch-title", "menu-batch"),
            ("batch-title-resize", "menu-resize"),
            ("batch-title-copy-meta", "menu-copy-meta"),
            ("batch-title-strip", "menu-strip-audio"),
            ("batch-title-extract", "menu-extract-audio"),
            ("drawing-title", "menu-export-drawings"),
        ];
        for lang in ["en", "it"] {
            let i18n = crate::i18n::I18n::new(lang);
            for (title, menu) in pairs {
                assert_eq!(i18n.t(title), i18n.t(menu).trim_end_matches('…'), "{lang}: {title}");
            }
        }
    }

    /// Audio files chosen (#120): their part of the window, ready once FFmpeg is; alone, F1
    /// opens the page on videos and there is no picture to turn or resize.
    #[test]
    fn audio_files_converted() {
        let mut f = Form::new("converted".into());
        f.files(&["/a/song.flac".into(), "/a/p.jpg".into()], false);
        assert!(f.audios && f.images && !f.videos);
        f.open_on(Part::Convert);
        assert_eq!((f.job().sound, f.why_not()), (None, Some("batch-sound-not-ready")));
        f.sound.ready = true;
        assert_eq!((f.job().sound, f.why_not()), (Some(Sound { format: 0, quality: Quality::Medium }), None));
        f.files(&["/a/song.flac".into()], false);
        assert!(!f.images && !f.videos && f.videos_only());
        // Exported drawings are never audio files.
        f.files(&["/a/song.flac".into(), "/a/d.dxf".into()], true);
        assert!(!f.audios);
    }

    /// In English always "audio", never "sound": the strings and the guide.
    #[test]
    fn english_says_audio() {
        // The text only: the ids may say "sound" (`batch-sound`).
        let shown = |l: &str| l.split_once(" = ").map_or(l, |(_, text)| text).to_lowercase();
        let sound = |text: &str| text.lines().filter(|l| !l.starts_with('#')).any(|l| shown(l).split(|c: char| !c.is_alphabetic()).any(|w| w == "sound"));
        assert!(!sound(include_str!("../locales/en.ftl")));
        for (lang, id, text) in crate::help::PAGES {
            assert!(*lang != "en" || !sound(text), "docs/help/en/{id}.md");
        }
    }

    /// One window for Convert…, Resize > Custom…, Copy with the metadata and the sound of the
    /// videos (#119): each opens with its own choices, and gives Convert…'s back.
    #[test]
    fn each_use_its_choices() {
        let mut f = Form::new("converted".into());
        assert!(matches!(f.sizes[LONG], Size::Long(_)));
        f.open_on(Part::Convert);
        (f.format, f.place) = (Some(Format::Png), Place::Sub);
        // Custom…: in the format of the originals, next to them, Long side at first.
        f.open_on(Part::Resize);
        assert_eq!((f.format, f.place == Place::Next, f.job().size), (None, true, Size::Long(1920)));
        assert_eq!(f.titles(), ("batch-title-resize", "batch-start-resize", "batch-working-resize"));
        f.size = 4;
        f.sizes[4] = Size::Short(600);
        // Convert…: its format and destination back, always on the size of the original.
        f.open_on(Part::Convert);
        assert_eq!((f.format, f.place == Place::Sub, f.job().size), (Some(Format::Png), true, Size::Same));
        assert_eq!(f.titles(), ("batch-title", "batch-start", "batch-working"));
        // Custom… again: the size of the last one.
        f.open_on(Part::Resize);
        assert_eq!((f.job().size, f.place == Place::Next), (Size::Short(600), true));
        // Copy with the metadata: the pictures as they are, which Convert would refuse.
        f.open_on(Part::CopyMeta);
        assert!(f.job().does_nothing());
        assert_eq!(f.titles(), ("batch-title-copy-meta", "batch-start-copy", "batch-working-copy"));
        assert_eq!((f.why_not(), f.place == Place::Sub), (None, true));
        f.open_on(Part::Convert);
        assert_eq!((f.format, f.job().size, f.why_not()), (Some(Format::Png), Size::Same, None));
        f.format = None;
        assert_eq!(f.why_not(), Some("batch-nothing"));
        f.format = Some(Format::Webp);
        // The sound of the videos: next to them; Convert… still in its own place after.
        f.open_on(Part::Audio);
        f.audio = Some(AudioJob::Strip);
        assert_eq!((f.place == Place::Next, f.titles()), (true, ("batch-title-strip", "batch-start-copy", "batch-working-copy")));
        // Its buttons say what they do (#119): Copy, Save.
        f.audio = Some(AudioJob::Extract);
        assert_eq!(f.titles(), ("batch-title-extract", "save-yes", "batch-working"));
        f.audio = None;
        f.open_on(Part::Convert);
        assert_eq!((f.format, f.place == Place::Sub), (Some(Format::Webp), true));
        // Export drawing, and the presets of Resize (no window).
        f.drawing = true;
        assert_eq!(f.titles(), ("drawing-title", "drawing-start", "batch-working"));
        assert_eq!(titles(None, false, Part::Resize), ("batch-title-resize", "batch-start-resize", "batch-working-resize"));
    }

    /// Resize > Custom… names its copies as the presets do, one name for each kind of size
    /// (#119), or with Unchanged what was done; a name taken gets " (2)" after it; the other uses
    /// of the window add nothing.
    #[test]
    fn custom_names_its_copies() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let mut f = Form::new("converted".into());
        f.open_on(Part::Resize);
        let kinds = [
            (Size::Long(1920), "Sunset (1920 px).jpg"),
            (Size::Short(600), "Sunset (600 px).jpg"),
            (Size::Width(800), "Sunset (800 px).jpg"),
            (Size::Height(600), "Sunset (600 px).jpg"),
            (Size::Fit(800, 600), "Sunset (800x600).jpg"),
            (Size::Percent(50), "Sunset (50%).jpg"),
            (Size::Crop(800, 600), "Sunset (800x600).jpg"),
            (Size::Stretch(800, 600), "Sunset (800x600).jpg"),
            (Size::Same, "Sunset.jpg"),
        ];
        // As a picture larger than the size asked is named (`batch::convert`); with Unchanged it is
        // only turned.
        let named = |f: &Form, i18n: &I18n, name: &str| f.job_named(i18n).named(name.to_string(), f.sizes[f.size] != Size::Same);
        for (size, name) in kinds {
            let i = Size::KINDS.iter().position(|k| k.same_kind(size)).unwrap();
            (f.size, f.sizes[i]) = (i, size);
            assert_eq!(named(&f, &en, "Sunset.jpg"), name, "{size:?}");
            // In the format chosen: the extension changes, the size stays in the name.
            f.format = Some(Format::Png);
            assert_eq!(named(&f, &en, &crate::batch::with_ext("Sunset.jpg", Format::Png)), name.replace(".jpg", ".png"));
            f.format = None;
            // Turned with a size: only the size.
            f.quarter = 1;
            if size != Size::Same {
                assert_eq!(named(&f, &it, "Sunset.jpg"), name, "{size:?}");
            }
            f.quarter = 0;
        }
        // Unchanged: what was done, in the language of the interface; a turn says it before a flip.
        f.size = 0;
        f.quarter = 1;
        assert_eq!((named(&f, &en, "Sunset.jpg"), named(&f, &it, "Sunset.jpg")), ("Sunset (rotated).jpg".into(), "Sunset (ruotata).jpg".into()));
        f.flip_v = true;
        assert_eq!(named(&f, &it, "Sunset.jpg"), "Sunset (ruotata).jpg");
        f.quarter = 0;
        assert_eq!((named(&f, &en, "Sunset.jpg"), named(&f, &it, "Sunset.jpg")), ("Sunset (flipped).jpg".into(), "Sunset (riflessa).jpg".into()));
        (f.flip_v, f.flip_h) = (false, true);
        assert_eq!(named(&f, &it, "Sunset.jpg"), "Sunset (riflessa).jpg");
        // The rule of the names taken, as for the presets.
        let taken: std::collections::HashSet<String> = ["Sunset (riflessa).jpg".to_string(), "Sunset (600 px).jpg".to_string()].into();
        assert_eq!(source::free_name(&taken, &named(&f, &it, "Sunset.jpg")), "Sunset (riflessa) (2).jpg");
        (f.size, f.sizes[4]) = (4, Size::Short(600));
        assert_eq!(source::free_name(&taken, &named(&f, &it, "Sunset.jpg")), "Sunset (600 px) (2).jpg");
        // Convert… and Copy with the metadata: nothing in the names, even resized or turned.
        for part in [Part::Convert, Part::CopyMeta] {
            f.open_on(part);
            (f.size, f.quarter) = (4, 1);
            let job = f.job_named(&en);
            assert_eq!((job.tag.as_str(), job.tag_turned.as_str()), ("", ""), "{part:?}");
        }
    }

    /// What a window shows and does, to compare two forms.
    fn seen(f: &Form) -> String {
        format!("{:?}", (f.job(), f.place, &f.sub_name, &f.other, f.sizes, f.size, f.enlarge, f.part, f.convert, f.resize, f.video.codec, f.video.container))
    }

    /// Convert… and Copy with the metadata never start turned or flipped, whatever the last
    /// Custom… did (#119, seen in winvm: photos copied mirrored); Custom… keeps its own.
    #[test]
    fn convert_and_copy_start_not_turned() {
        let mut f = Form::new("converted".into());
        f.open_on(Part::Resize);
        (f.quarter, f.flip_h, f.size) = (1, true, 4);
        for part in [Part::Convert, Part::CopyMeta, Part::Audio] {
            f.open_on(part);
            assert_eq!((f.quarter, f.flip_h, f.flip_v, f.job().size), (0, false, false, Size::Same), "{part:?}");
            assert!(f.job().turn.is_none(), "{part:?}");
        }
        f.open_on(Part::CopyMeta);
        assert!(f.job().does_nothing());
        f.open_on(Part::Resize);
        assert_eq!((f.quarter, f.flip_h, f.size), (1, true, 4));
    }

    /// Custom… opens again on its own size, with its measure, and its turn after Copy with the
    /// metadata of a video and a Convert… that changed the measures (#119, seen on IronManL:
    /// Unchanged with the turn kept). Convert… and Copy with the metadata still start unturned.
    #[test]
    fn custom_keeps_its_size_after_a_video() {
        let mut f = Form::new("converted".into());
        let (photo, clip) = (vec!["a.jpg".to_string()], vec!["clip.mp4".to_string()]);
        // As `open_resize`: the pictures only.
        let resize = |f: &mut Form| {
            f.files(&photo, false);
            (f.images, f.videos) = (true, false);
            f.open_on(Part::Resize);
        };
        resize(&mut f);
        (f.size, f.sizes[LONG], f.quarter) = (LONG, Size::Long(1920), 1);
        f.files(&clip, false);
        f.open_on(Part::CopyMeta);
        assert_eq!((f.job().size, f.quarter), (Size::Same, 0));
        resize(&mut f);
        assert_eq!((f.job().size, f.quarter), (Size::Long(1920), 1));
        // Convert… of a picture and a video, on Long side changed to 800.
        f.files(&[photo[0].clone(), clip[0].clone()], false);
        f.open_on(Part::Convert);
        assert_eq!((f.job().size, f.quarter), (Size::Same, 0));
        (f.size, f.sizes[LONG]) = (LONG, Size::Long(800));
        resize(&mut f);
        assert_eq!((f.job().size, f.quarter), (Size::Long(1920), 1));
    }

    /// Cancel forgets what was chosen in the window, whichever use opened it: opened again, it
    /// is as if never touched (#119). What was confirmed stays.
    #[test]
    fn cancel_forgets_the_choices() {
        let mut f = Form::new("converted".into());
        // Confirmed before: Convert… to PNG in the subfolder, a Custom… of 600 px turned.
        f.open_on(Part::Convert);
        f.format = Some(Format::Png);
        f.open_on(Part::Resize);
        (f.size, f.sizes[4], f.quarter) = (4, Size::Short(600), 1);
        for part in [Part::Convert, Part::Resize, Part::CopyMeta, Part::Audio] {
            let before = f.clone();
            let mut untouched = f.clone();
            untouched.open_on(part);
            f.open_on(part);
            (f.format, f.place, f.size, f.sizes[2], f.quarter, f.flip_h, f.flip_v, f.enlarge) = (Some(Format::Webp), Place::Other, 2, Size::Fit(10, 10), 3, true, true, true);
            (f.sub_name, f.other, f.video.codec, f.video.container) = ("x".into(), "/tmp".into(), Codec::Vp9, 1);
            f = f.cancelled(before.clone());
            assert_eq!(seen(&f), seen(&before), "{part:?}");
            f.open_on(part);
            assert_eq!(seen(&f), seen(&untouched), "{part:?}");
        }
        // Confirmed (no Cancel): kept for the next time.
        f.open_on(Part::Resize);
        (f.size, f.flip_v) = (1, true);
        f.open_on(Part::Convert);
        f.open_on(Part::Resize);
        assert_eq!((f.size, f.flip_v, f.quarter), (1, true, 1));
    }
}
