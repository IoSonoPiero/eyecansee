//! The videos' part of the Convert window: container, codec, encoder, quality, audio and
//! metadata, FFmpeg downloaded first where there is none, the audio jobs, and the audio files'
//! part (#120). Split from
//! `batch_page.rs` on 2026-11-02.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui;

use crate::audio_job::AudioJob;
use crate::ffmpeg::{CONTAINERS, Codec, Quality};
use crate::i18n::I18n;


/// The choices for the videos.
#[derive(Clone)]
pub(crate) struct VideoForm {
    /// Index in `CONTAINERS`.
    pub(crate) container: usize,
    pub(crate) codec: Codec,
    /// One of `codec`'s encoders that works here, once checked.
    pub(crate) encoder: Option<&'static str>,
    pub(crate) quality: Quality,
    pub(crate) audio: bool,
    pub(crate) meta: bool,
    /// FFmpeg being downloaded (`component::FFMPEG`).
    pub(crate) download: Option<Arc<crate::component::Download>>,
    /// Copied as it is (`Codec::Copy`): the containers offered for the videos chosen, and the one
    /// it starts on (`video_copy::targets`, `default`), set when the window opens.
    pub(crate) copy_into: Vec<usize>,
    pub(crate) copy_own: usize,
}

impl VideoForm {
    /// The videos chosen (`names`), for the containers of a copy.
    pub(crate) fn files<'a>(&mut self, names: impl IntoIterator<Item = &'a str> + Clone) {
        self.copy_into = crate::video_copy::targets(names.clone());
        self.copy_own = crate::video_copy::default(names);
    }

    /// "Same as the original (no re-encoding)" chosen: the copy starts on the originals'
    /// container, unless the one chosen is offered for them too.
    pub(crate) fn copy_container(&mut self) {
        if self.codec == Codec::Copy && !self.copy_into.contains(&self.container) {
            self.container = self.copy_own;
        }
    }
}

/// The audio jobs: FFmpeg ready, and what the job does; nothing else to choose. They copy the
/// streams: an FFmpeg without the phones' decoders does them too (`ffmpeg::copier`), no encoder
/// to try.
pub(crate) fn audio_rows(ui: &mut egui::Ui, v: &mut VideoForm, job: AudioJob, i18n: &I18n) {
    if crate::ffmpeg::copier().is_none() {
        ffmpeg_download(ui, v, i18n);
        return;
    }
    ui.label(i18n.t(match job {
        AudioJob::Strip => "batch-strip-hint",
        AudioJob::Extract => "batch-extract-hint",
    }));
}

/// The choices for the audio files (#120).
#[derive(Clone)]
pub(crate) struct SoundForm {
    /// Index in `audio_convert::FORMATS`.
    pub(crate) format: usize,
    pub(crate) quality: Quality,
    /// FFmpeg found and the format's encoder there (`sound_rows`).
    pub(crate) ready: bool,
}

/// The format of the audio files (only those whose encoder this FFmpeg has) and its quality;
/// FFmpeg downloaded first where there is none. No video decoder is needed: an FFmpeg without
/// the phones' decoders does it too (`ffmpeg::copier`).
pub(crate) fn sound_rows(ui: &mut egui::Ui, s: &mut SoundForm, v: &mut VideoForm, i18n: &I18n) {
    use crate::audio_convert::{FORMATS, available, hint};
    let t = |id| i18n.t(id);
    s.ready = false;
    if crate::ffmpeg::copier().is_none() {
        ffmpeg_download(ui, v, i18n);
        return;
    }
    let ctx = ui.ctx().clone();
    crate::ffmpeg::start_copy_probe(move || ctx.request_repaint());
    let Some(probe) = crate::ffmpeg::copy_probed() else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(t("batch-video-checking"));
        });
        return;
    };
    let formats = available(&probe.audio);
    let Some(&first) = formats.first() else {
        ui.label(crate::browser::error_text(ui, t("batch-video-none")));
        return;
    };
    if !formats.contains(&s.format) {
        s.format = first;
    }
    egui::ComboBox::from_label(t("batch-sound-format")).selected_text(FORMATS[s.format].name).show_ui(ui, |ui| {
        for &i in &formats {
            ui.selectable_value(&mut s.format, i, FORMATS[i].name);
        }
    });
    if !FORMATS[s.format].lossless {
        ui.horizontal(|ui| {
            ui.label(t("batch-video-quality"));
            for (q, id) in [(Quality::High, "batch-video-high"), (Quality::Medium, "batch-video-medium"), (Quality::Small, "batch-video-small")] {
                ui.radio_value(&mut s.quality, q, t(id));
            }
        });
    }
    ui.weak(t(hint(s.format)));
    s.ready = true;
}

/// FFmpeg found and its encoders tried: downloaded first where there is none, a spinner while
/// it is tried. With only a `copier` (an FFmpeg without the phones' decoders), the probe for the
/// copied streams and `true`: the other codecs ask for the download when chosen.
fn ffmpeg_ready(ui: &mut egui::Ui, v: &mut VideoForm, i18n: &I18n) -> Option<(&'static crate::ffmpeg::Probe, bool)> {
    let ctx = ui.ctx().clone();
    let (probe, copy_only) = if crate::ffmpeg::program().is_some() {
        crate::ffmpeg::start_probe(move || ctx.request_repaint());
        (crate::ffmpeg::probed(), false)
    } else if crate::ffmpeg::copier().is_some() {
        crate::ffmpeg::start_copy_probe(move || ctx.request_repaint());
        (crate::ffmpeg::copy_probed(), true)
    } else {
        ffmpeg_download(ui, v, i18n);
        return None;
    };
    if probe.is_none() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(i18n.t("batch-video-checking"));
        });
    }
    probe.map(|p| (p, copy_only))
}

/// Container, codec, encoder (only those that work here), quality, sound and metadata; FFmpeg
/// downloaded first where there is none.
pub(crate) fn video_rows(ui: &mut egui::Ui, v: &mut VideoForm, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let Some((probe, copy_only)) = ffmpeg_ready(ui, v, i18n) else { return };
    if crate::ffmpeg::download_better(v.download.is_some()) {
        better_download(ui, v, i18n);
    }
    container_codec_rows(ui, v, probe, copy_only, i18n);
    let c = &CONTAINERS[v.container];
    let encoders = probe.of(v.codec);
    if encoders.is_empty() {
        v.encoder = None;
        if copy_only {
            ffmpeg_download(ui, v, i18n);
        } else {
            ui.label(crate::browser::error_text(ui, t("batch-video-none")));
        }
        return;
    }
    if !v.encoder.is_some_and(|e| encoders.contains(&e)) {
        v.encoder = Some(encoders[0]);
    }
    // Copied streams: nothing to choose about the encoder or the quality.
    if v.codec != Codec::Copy {
        // The graphics card's encoders in green, with a line that says so.
        let good = crate::theme::good(ui.visuals());
        let name = |e: &str| {
            let text = egui::RichText::new(format!("{} ({e})", crate::ffmpeg::maker(e)));
            if crate::ffmpeg::hardware(e) { text.color(good) } else { text }
        };
        egui::ComboBox::from_label(t("batch-video-encoder")).selected_text(name(v.encoder.unwrap_or_default())).show_ui(ui, |ui| {
            for &e in &encoders {
                ui.selectable_value(&mut v.encoder, Some(e), name(e));
            }
        });
        ui.weak(t(crate::ffmpeg::hint(v.encoder.unwrap_or_default())));
        if encoders.iter().any(|e| crate::ffmpeg::hardware(e)) {
            ui.weak(t("batch-video-hw-note"));
        }
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

/// The container and the codec. Copied as it is, only the containers that hold the originals'
/// streams (`video_copy`); choosing the copy from another one moves to the originals' own.
fn container_codec_rows(ui: &mut egui::Ui, v: &mut VideoForm, probe: &crate::ffmpeg::Probe, copy_only: bool, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let copy = v.codec == Codec::Copy;
    egui::ComboBox::from_label(t("batch-video-container")).selected_text(CONTAINERS[v.container].name).show_ui(ui, |ui| {
        for (i, c) in CONTAINERS.iter().enumerate().filter(|(i, _)| !copy || v.copy_into.contains(i)) {
            ui.selectable_value(&mut v.container, i, c.name);
        }
    });
    let c = &CONTAINERS[v.container];
    // With only a copier the codecs that encode stay open: choosing one offers the download.
    let usable = |k: Codec| copy_only || !probe.of(k).is_empty();
    if !c.codecs.contains(&v.codec) || !usable(v.codec) {
        v.codec = c.codecs.iter().copied().find(|k| usable(*k)).unwrap_or(c.codecs[0]);
    }
    let label = |k: Codec| if k == Codec::Copy { t("batch-video-copy") } else { k.label().to_string() };
    egui::ComboBox::from_label(t("batch-video-codec")).selected_text(label(v.codec)).show_ui(ui, |ui| {
        for &k in c.codecs {
            let usable = usable(k);
            let label = if usable { label(k) } else { i18n.t_args("batch-video-unavailable", &[("codec", k.label())]) };
            ui.add_enabled_ui(usable, |ui| ui.selectable_value(&mut v.codec, k, label));
        }
    });
    v.copy_container();
    ui.weak(t(v.codec.hint()));
}

/// No usable FFmpeg on this computer (none, too old, or without the phones' decoders): what it is, its size and the button;
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
        crate::engine::Status::Lacks { found, missing } => {
            ui.label(i18n.t_args("batch-video-lacks-ffmpeg", &[("found", &found), ("missing", &missing), ("size", &size)]));
        }
        _ => {
            ui.label(i18n.t_args("batch-video-need-ffmpeg", &[("size", &size)]));
        }
    }
    // Done: `ffmpeg::program` finds it at the next frame.
    if download_row(ui, v, i18n, c) {
        ui.ctx().request_repaint();
    }
}

/// The system's FFmpeg works, but the downloaded one would also keep the reading, turning and
/// resizing on the card ("Automatic", `ffmpeg::download_better`): said, with the button.
fn better_download(ui: &mut egui::Ui, v: &mut VideoForm, i18n: &I18n) {
    let Some(c) = crate::component::FFMPEG else { return };
    ui.label(i18n.t_args("batch-video-better-ffmpeg", &[("size", &format!("{} MB", c.size / 1_000_000))]));
    if download_row(ui, v, i18n, c) {
        ui.label(i18n.t("opt-engine-restart"));
    }
    ui.add_space(4.0);
}

/// The progress of the download, or its button (with the error of the last try): true once done.
fn download_row(ui: &mut egui::Ui, v: &mut VideoForm, i18n: &I18n, c: crate::component::Component) -> bool {
    let state = v.download.as_ref().map(|d| (d.done.load(Ordering::Relaxed), d.result.lock().unwrap().clone()));
    match state {
        Some((done, None)) => {
            ui.add(egui::ProgressBar::new(done as f32 / c.size as f32).show_percentage());
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
        }
        Some((_, Some(Ok(())))) => return true,
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
    false
}
