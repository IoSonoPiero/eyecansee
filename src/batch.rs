//! Converting and resizing several images at once, without the interface: what each image
//! becomes (`convert`), and the whole job in the background (`run`). The originals are never
//! touched: the new files get free names, next to them, in a subfolder or in another folder,
//! also on another device.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use image::imageops::FilterType;
use serde::{Deserialize, Serialize};

use crate::encode::{self, Format, Options};
use crate::encoders::LeftOut;
use crate::meta_strip::Keep;
use crate::source::{self, Store};
use crate::transform::Turn;

/// New size of the images. Kept in the settings by the presets of Resize (`presets`).
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Same,
    /// Percent of the original sides.
    Percent(u32),
    /// Inside this box, keeping the proportions.
    Fit(u32, u32),
    /// The long side, the short side, the width or the height this many pixels, keeping the
    /// proportions.
    Long(u32),
    Short(u32),
    Width(u32),
    Height(u32),
    /// Exactly this size: the middle of the image, cut to these proportions (`crop`).
    Crop(u32, u32),
    /// Exactly this size, stretched.
    Stretch(u32, u32),
}

impl Size {
    /// Every kind with its first measures, in the order the windows show them.
    pub const KINDS: [Size; 9] = [
        Size::Same,
        Size::Percent(50),
        Size::Fit(1920, 1080),
        Size::Long(1920),
        Size::Short(1080),
        Size::Width(1920),
        Size::Height(1080),
        Size::Crop(1920, 1080),
        Size::Stretch(1920, 1080),
    ];

    /// Fluent id of the kind; without its "batch-size-", the key of the kind (`presets::describe`).
    pub fn label(self) -> &'static str {
        match self {
            Size::Same => "batch-size-same",
            Size::Percent(_) => "batch-size-percent",
            Size::Fit(..) => "batch-size-fit",
            Size::Long(_) => "batch-size-long",
            Size::Short(_) => "batch-size-short",
            Size::Width(_) => "batch-size-width",
            Size::Height(_) => "batch-size-height",
            Size::Crop(..) => "batch-size-crop",
            Size::Stretch(..) => "batch-size-stretch",
        }
    }

    /// The same kind, whatever the measures.
    pub fn same_kind(self, other: Size) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    /// The kinds ffmpeg resizes videos to (`ffmpeg::scale`); the others are for pictures only.
    pub fn for_videos(self) -> bool {
        matches!(self, Size::Same | Size::Percent(_) | Size::Fit(..))
    }

    /// What the name of a copy says of its size, as "photo (1920 px).jpg" (`tagged`).
    pub fn tag(self) -> String {
        match self {
            Size::Same => String::new(),
            Size::Percent(p) => format!(" ({p}%)"),
            Size::Long(n) | Size::Short(n) | Size::Width(n) | Size::Height(n) => format!(" ({n} px)"),
            Size::Fit(w, h) | Size::Crop(w, h) | Size::Stretch(w, h) => format!(" ({w}x{h})"),
        }
    }
}

/// How the pixels are worked out when the size changes, from the sharpest to the plainest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    Lanczos3,
    CatmullRom,
    Gaussian,
    Bilinear,
    /// No smoothing: for pixel art.
    Nearest,
}

impl Filter {
    pub const ALL: [Filter; 5] = [Filter::Lanczos3, Filter::CatmullRom, Filter::Gaussian, Filter::Bilinear, Filter::Nearest];

    /// Fluent id of the name.
    pub fn label(self) -> &'static str {
        match self {
            Filter::Lanczos3 => "batch-filter-lanczos",
            Filter::CatmullRom => "batch-filter-catmull",
            Filter::Gaussian => "batch-filter-gaussian",
            Filter::Bilinear => "batch-filter-bilinear",
            Filter::Nearest => "batch-filter-nearest",
        }
    }

    pub(crate) fn kind(self) -> FilterType {
        match self {
            Filter::Lanczos3 => FilterType::Lanczos3,
            Filter::CatmullRom => FilterType::CatmullRom,
            Filter::Gaussian => FilterType::Gaussian,
            Filter::Bilinear => FilterType::Triangle,
            Filter::Nearest => FilterType::Nearest,
        }
    }
}

/// What to do with every image.
#[derive(Clone, PartialEq, Debug)]
pub struct Job {
    /// `None`: the format of each original, written as "Save" writes it.
    pub format: Option<Format>,
    pub options: Options,
    pub size: Size,
    pub filter: Filter,
    /// Flips and quarter turns, before the size changes.
    pub turn: Turn,
    /// Images smaller than the size asked for are made larger too.
    pub enlarge: bool,
    /// What the videos become, with the same size and turn; `None`: videos are left out.
    pub video: Option<crate::ffmpeg::Video>,
    /// Only the audio of the videos taken out or extracted, nothing else done (`audio_job`).
    pub audio: Option<crate::audio_job::AudioJob>,
    /// What the audio files become (#120); `None`: they are left out.
    pub sound: Option<crate::audio_convert::Sound>,
    /// The originals' source in the thumbnail cache: a video without its audio takes its thumbnail.
    pub base: String,
    /// "Export drawing": CAD drawings drawn again (`drawing`), nothing else done.
    pub drawing: Option<Drawn>,
    /// The layers chosen in the viewer's panel, by drawing; and those of the drawing being done
    /// (`cad_layers`). The others as their file has them.
    pub layers: Arc<std::collections::HashMap<String, Vec<bool>>>,
    pub shown: Option<Vec<bool>>,
    /// Added to the names of the pictures really resized, before the extension (Resize:
    /// `Size::tag`); a picture left at its size keeps its name (`Job::named`).
    pub tag: String,
    /// Added instead to the names of the pictures turned or flipped but not resized (Custom…).
    pub tag_turned: String,
}

/// What a CAD drawing becomes: an SVG, or a picture in `Job::format` drawn at this many pixels
/// on its long side (sharp at any size: a drawing has no pixels of its own to enlarge).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Drawn {
    Svg,
    Side(u32),
    /// The other AutoCAD format (`cad_convert`), only DWG and DXF files.
    Cad(crate::cad_convert::Target),
}

impl Default for Job {
    fn default() -> Self {
        Job { format: Some(Format::Jpeg), options: Options::default(), size: Size::Same, filter: Filter::Lanczos3, turn: Turn::default(), enlarge: false, video: None, audio: None, sound: None, base: String::new(), drawing: None, layers: Default::default(), shown: None, tag: String::new(), tag_turned: String::new() }
    }
}

impl Job {
    /// Same format, same size, not turned: every image would only be encoded again, losing quality.
    pub fn does_nothing(&self) -> bool {
        self.format.is_none() && self.size == Size::Same && self.turn.is_none()
    }

    /// The name of the copy of a picture: with `tag` when it was `resized`, with `tag_turned`
    /// when it was only turned, its own name when it was left at its size (#119).
    pub fn named(&self, name: String, resized: bool) -> String {
        let tag = if resized { &self.tag } else if self.turn.is_none() { "" } else { &self.tag_turned };
        if tag.is_empty() { name } else { tagged(&name, tag) }
    }
}

/// Size of an image of `w` × `h` pixels after the job; never below 1 pixel. Without `enlarge`
/// no side grows: a box larger than the image is cut down to it.
pub fn target(w: u32, h: u32, size: Size, enlarge: bool) -> (u32, u32) {
    let ratio = |to: u32, from: u32| to as f64 / from as f64;
    let scale = match size {
        Size::Same => return (w, h),
        Size::Percent(p) => p as f64 / 100.0,
        Size::Fit(bw, bh) => ratio(bw, w).min(ratio(bh, h)),
        Size::Long(n) => ratio(n, w.max(h)),
        Size::Short(n) => ratio(n, w.min(h)),
        Size::Width(n) => ratio(n, w),
        Size::Height(n) => ratio(n, h),
        Size::Crop(bw, bh) => ratio(bw, w).max(ratio(bh, h)),
        Size::Stretch(bw, bh) if enlarge => return (bw.max(1), bh.max(1)),
        Size::Stretch(bw, bh) => return (bw.min(w).max(1), bh.min(h).max(1)),
    };
    let scale = if enlarge { scale } else { scale.min(1.0) };
    let side = |v: u32| ((v as f64 * scale).round() as u32).max(1);
    match size {
        Size::Crop(bw, bh) => (side(w).min(bw.max(1)), side(h).min(bh.max(1))),
        _ => (side(w), side(h)),
    }
}

/// The part of an image of `w` × `h` pixels that `Size::Crop` keeps, in the middle: left, top,
/// width, height. `None` for the other sizes, or when nothing is cut.
pub fn crop(w: u32, h: u32, size: Size, enlarge: bool) -> Option<(u32, u32, u32, u32)> {
    let Size::Crop(..) = size else { return None };
    let (tw, th) = target(w, h, size, enlarge);
    // The side that fills the box sets the scale; the other side is cut.
    let scale = (tw as f64 / w as f64).max(th as f64 / h as f64);
    let (cw, ch) = (((tw as f64 / scale).round() as u32).clamp(1, w), ((th as f64 / scale).round() as u32).clamp(1, h));
    ((cw, ch) != (w, h)).then_some(((w - cw) / 2, (h - ch) / 2, cw, ch))
}

/// `name` with `tag` before its extension: "photo.jpg" and " (50%)" make "photo (50%).jpg".
pub fn tagged(name: &str, tag: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem}{tag}.{ext}"),
        _ => format!("{name}{tag}"),
    }
}

/// Why an image was left out (Fluent ids), with its name.
pub const SKIP_FORMAT: &str = "batch-skip-format";
pub const SKIP_INSIDE: &str = "batch-skip-inside";
pub const SKIP_SAME_SIZE: &str = "batch-skip-same-size";
pub const SKIP_VIDEO: &str = "batch-skip-video";
pub const SKIP_NOT_AUTOCAD: &str = "drawing-skip-not-autocad";
pub const SKIP_WOULD_LOSE: &str = "drawing-skip-would-lose";
/// Over half the RAM once decoded (`view_memory`): left out, not asked about (Piero, 20/11/2026).
pub const SKIP_TOO_LARGE: &str = "batch-skip-too-large";
/// Saved, but its EXIF, XMP or IPTC was too large for a JPEG (#217).
pub const META_TOO_LARGE: &str = "batch-meta-too-large";

/// The file `bytes` (named `name`) after the job: the new name, without the folder, and its
/// content. `Err((id, detail))`: a Fluent id with the name or the error.
#[cfg(test)]
pub fn convert(bytes: &[u8], name: &str, job: &Job) -> Result<(String, Vec<u8>), (&'static str, String)> {
    convert_with(bytes, name, job, &|_| None, None).map(|(name, data, _)| (name, data))
}

/// A file made by the job: its name, its content, and what its metadata left out, too large for
/// a JPEG (#217).
pub type Made = (String, Vec<u8>, LeftOut);

/// Like `convert`, reading the files next to a 3D model or a CAD drawing through `sibling`; a RAW
/// photo's copy has the XMP of its `sidecar` (#24). Also says what a JPEG made leaves out.
pub fn convert_with(bytes: &[u8], name: &str, job: &Job, sibling: crate::model::Sibling, sidecar: Option<&[u8]>) -> Result<Made, (&'static str, String)> {
    if let Some(drawn) = job.drawing {
        return drawing(bytes, name, job, drawn, sibling).map(|(name, data)| (name, data, LeftOut::default()));
    }
    // Copy with the metadata: the picture as it is, everything in it kept.
    if job.does_nothing() {
        return Ok((name.to_string(), bytes.to_vec(), LeftOut::default()));
    }
    if job.format.is_none() && !encode::saves_back(name) {
        return Err((SKIP_FORMAT, name.to_string()));
    }
    // Only turned, in its own format: as "Save" does it, a JPEG without losing quality when
    // its size allows it.
    if job.format.is_none()
        && job.size == Size::Same
        && (crate::save_back::can_turn_in_place(bytes, name) || crate::ultrahdr::turns_whole(bytes))
        && let Ok((data, _)) = crate::save_back::turned_file_beside(bytes, name, job.turn, sidecar)
    {
        return Ok((job.named(name.to_string(), false), data, LeftOut::default()));
    }
    let mut image = crate::decode::decode_full_with(bytes, name, sibling).map_err(|e| match e.split_once('\u{1}') {
        Some((_, size)) if crate::pixel_limit::is_too_large(&e) => (crate::pixel_limit::TOO_LARGE, format!("{name}, {size}")),
        Some((id, file)) if crate::model_gltf::is_error(id) => crate::model_gltf::convert_error(id, file, name),
        _ => crate::fault::not_opened(name, e),
    })?;
    // The other frames of an animation, the other pages would be lost. After decoding, so that
    // a file that is not an image says so. A phone JPEG (HDR gain map, motion photo video) is
    // not left out: the new file is the picture, the original keeps the rest.
    if Format::of_file(name).is_some_and(|f| f != Format::Jpeg) && !crate::save_back::can_turn_in_place(bytes, name) {
        return Err((SKIP_INSIDE, name.to_string()));
    }
    job.turn.apply(&mut image);
    let (w, h) = target(image.width(), image.height(), job.size, job.enlarge);
    let cut = crop(image.width(), image.height(), job.size, job.enlarge);
    let resized = cut.is_some() || (w, h) != (image.width(), image.height());
    // Already of the size asked in its own format (or smaller, not enlarged): a copy would be
    // the original again, under a longer name at each Resize. None is made.
    if job.format.is_none() && job.turn.is_none() && !resized {
        return Err((SKIP_SAME_SIZE, name.to_string()));
    }
    if let Some((x, y, cw, ch)) = cut {
        image = image.crop_imm(x, y, cw, ch);
    }
    if (w, h) != (image.width(), image.height()) {
        image = image.resize_exact(w, h, job.filter.kind());
    }
    encoded(bytes, name, job, image, cut, sidecar).map(|(out, data, left)| (job.named(out, resized), data, left))
}

/// The picture `image` made from `bytes` (named `name`, cut by `cut`, with the XMP of the RAW
/// photo's `sidecar`) written as `job` asks: the new name, without its tag, and the content.
fn encoded(bytes: &[u8], name: &str, job: &Job, image: image::DynamicImage, cut: Option<(u32, u32, u32, u32)>, sidecar: Option<&[u8]>) -> Result<Made, (&'static str, String)> {
    let failed = |e: String| ("batch-failed", format!("{name}: {e}"));
    // An Ultra HDR photo written as JPEG with its metadata stays Ultra HDR, its gain map turned
    // and resized with it (#65); without metadata there is nothing to hold the gain map.
    let to_jpeg = job.format.map_or(Format::of_file(name) == Some(Format::Jpeg), |f| f == Format::Jpeg);
    if to_jpeg && job.options.meta != Keep::Nothing && crate::ultrahdr::is(bytes) {
        let (out, options) = match job.format {
            None => (name.to_string(), crate::encode::jpeg_quality::like(bytes, Options::default())),
            Some(f) => (with_ext(name, f), Options { format: f, turn: Turn::default(), ..job.options }),
        };
        return Ok((out, crate::ultrahdr::resized(bytes, image, job.turn, cut, &options, sidecar).map_err(failed)?, LeftOut::default()));
    }
    // Encoded again, the file is the picture alone: the XMP of a gain map or a motion photo
    // video it no longer has goes (`Original::alone`).
    let (out, options) = match job.format {
        // As "Save" writes them, at the original's quality (which would refuse a phone JPEG: it
        // writes over the original).
        None if Format::of_file(name) == Some(Format::Jpeg) => (name.to_string(), crate::encode::jpeg_quality::like(bytes, Options::default())),
        None => return crate::save_back::encoded_beside(bytes, name, image, sidecar).map(|(data, _)| (name.to_string(), data, LeftOut::default())).map_err(|_| (SKIP_INSIDE, name.to_string())),
        Some(format) => (with_ext(name, format), Options { format, turn: Turn::default(), ..job.options }),
    };
    let original = encode::Original::with_meta_beside(image, bytes, name, sidecar).alone().turned(job.turn);
    Ok((out, original.encode(&options).map_err(failed)?, original.left_out(&options)))
}

/// A CAD drawing as `drawn` says; its pictures read through `sibling`.
fn drawing(bytes: &[u8], name: &str, job: &Job, drawn: Drawn, sibling: crate::model::Sibling) -> Result<(String, Vec<u8>), (&'static str, String)> {
    let failed = |e: String| ("batch-failed", format!("{name}: {e}"));
    let side = match drawn {
        Drawn::Cad(_) if !crate::cad_convert::can_convert(name) => return Err((SKIP_NOT_AUTOCAD, name.to_string())),
        Drawn::Cad(to) => {
            use crate::cad_convert::Refused;
            return match crate::cad_convert::convert_checked(bytes, name, to) {
                Ok(data) => Ok((ext(name, to.ext()), data)),
                // A drawing that does not open is said as everywhere, not with its library's text.
                Err(Refused::NotRead(e)) => Err(crate::fault::not_opened(name, e)),
                Err(Refused::NotMade(e)) => Err(failed(e)),
                Err(Refused::WouldLose(lost)) => Err((SKIP_WOULD_LOSE, format!("{name} ({lost})"))),
            };
        }
        Drawn::Svg => {
            let d = shown(crate::cad::read_with(bytes, name, sibling).map_err(|e| crate::fault::not_opened(name, e))?, job);
            return Ok((ext(name, "svg"), d.svg(crate::decode::CAD_SIDE, 1.0).map_err(failed)?.into_bytes()));
        }
        Drawn::Side(side) => side,
    };
    let d = shown(crate::cad::read_with(bytes, name, sibling).map_err(|e| crate::fault::not_opened(name, e))?, job);
    let image = image::DynamicImage::ImageRgba8(crate::decode::cad_render(&d, side, None).map_err(failed)?);
    let format = job.format.unwrap_or(Format::Png);
    let options = Options { format, turn: Turn::default(), ..job.options };
    Ok((with_ext(name, format), encode::Original::with_meta(image, &[]).encode(&options).map_err(failed)?))
}

/// The drawing with the layers chosen in the viewer, if any.
fn shown(mut d: crate::cad::Drawing, job: &Job) -> crate::cad::Drawing {
    if let Some(on) = &job.shown {
        d.set_layers(on);
    }
    d
}

/// `name` with the extension of `format` in place of its own.
pub fn with_ext(name: &str, format: Format) -> String {
    ext(name, format.ext())
}

/// The name typed in Save as…, with the extension of `format` added when it has none ("photo"
/// → "photo.jpg"): a file without one is not known as a picture. One typed is kept.
pub fn typed_name(name: &str, format: Format) -> String {
    let name = name.trim().trim_end_matches('.');
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => name.to_string(),
        _ => format!("{name}.{}", format.ext()),
    }
}

fn ext(name: &str, ext: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    format!("{stem}.{ext}")
}

/// Local files ffmpeg is writing right now: the folder watcher ignores their changes and the
/// listing leaves them out, so no thumbnail is tried on a file half written (it was tried every
/// 0.3 s, "mpv cannot play the file"). The folder is read again once the file is closed.
static WRITING: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Default::default);

/// Whether a conversion is writing `path` at this moment.
pub fn writing(path: &std::path::Path) -> bool {
    let writing = WRITING.lock().unwrap();
    !writing.is_empty() && writing.contains(&same_form(path))
}

/// The same path whoever wrote it: OpenDAL canonicalizes its root (on Windows `\\?\C:\…` with
/// long names), the folder watcher and the temp folder do not (`C:\Users\RUNNER~1\…`). The
/// folder canonicalized, the name kept: the file may not exist yet.
fn same_form(path: &std::path::Path) -> PathBuf {
    match (path.parent().and_then(|p| p.canonicalize().ok()), path.file_name()) {
        (Some(dir), Some(name)) => dir.join(name),
        _ => path.to_path_buf(),
    }
}

/// The thumbnail of the video written into `file`, kept for `to` until the loader asks for it
/// (`thumbcache::keep`): made here, from the local file, before it goes to a phone or a server.
pub(crate) async fn keep_thumbnail(to: &str, file: &std::path::Path) {
    let input = crate::mpv::Input::File(file.to_path_buf());
    let (Ok(size), Ok(Ok(img))) = (
        std::fs::metadata(file).map(|m| m.len()),
        tokio::task::spawn_blocking(move || crate::mpv::still(input, crate::loader::THUMB_SIDE)).await,
    ) else {
        return;
    };
    if let Ok(data) = crate::thumbcache::encode(&image::DynamicImage::ImageRgba8(img)) {
        crate::thumbcache::keep(to, size, data);
    }
}

/// `path` marked as being written until the guard is dropped.
pub(crate) struct Writing(PathBuf);

impl Writing {
    pub(crate) fn start(path: PathBuf) -> Self {
        let path = same_form(&path);
        WRITING.lock().unwrap().insert(path.clone());
        Writing(path)
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        WRITING.lock().unwrap().remove(&self.0);
    }
}

/// A video converted by ffmpeg into the folder `dest` of `out`, with a name free in `taken`,
/// returned. ffmpeg reads and writes files of this computer: a video on a phone or a server is
/// copied into a temporary file first, a destination there gets the temporary result at the end.
async fn video(op: &Store, path: &str, (out, dest, taken): (&Store, &str, &HashSet<String>), job: &Job, p: &Arc<Progress>) -> Result<String, source::Error> {
    use crate::ffmpeg_files::{Input, Output, deliver};
    let name = source::name(path).to_string();
    let failed = |e: String| crate::ffmpeg_files::failed(&name, e);
    // Copied streams: an FFmpeg without the phones' decoders does it too (`ffmpeg::copier`).
    let probe = job.video.as_ref().and_then(|v| if v.encoder == "copy" { crate::ffmpeg::copy_probed() } else { crate::ffmpeg::probed() });
    let (Some(mut v), Some(probe)) = (job.video.clone(), probe) else { return Err((SKIP_VIDEO, name)) };
    let input = Input::fetch(op, path, (out, dest), p).await?;
    // Copied as it is: into a container that holds its streams (`video_copy`), MKV otherwise.
    if v.encoder == "copy" {
        v.container = crate::video_copy::for_file(v.container, &name, &probe.program, &input.file);
    }
    let new_name = source::free_name(taken, &ext(&name, crate::ffmpeg::CONTAINERS[v.container].ext));
    let output = Output::new(out, format!("{dest}{new_name}"), &name);
    let tags = if v.meta { crate::video_read::track_tags(op, path).await } else { Vec::new() };
    let args = |on_card| crate::ffmpeg::args(&v, job.size, job.enlarge, job.turn, on_card, (&input.file.to_string_lossy(), &output.file.to_string_lossy()), &probe.audio, &tags);
    let on_card = probe.card.contains(&v.encoder);
    let mut r = input.ffmpeg(&probe.program, args(on_card)).await;
    // A video the card cannot read (a codec or a format it lacks): again as before, on the processor.
    if on_card && r.is_err() && !input.cancelled() {
        log!("ffmpeg on the card failed ({}), again on the processor", r.as_ref().unwrap_err());
        r = input.ffmpeg(&probe.program, args(false)).await;
    }
    let phases = input.phases.clone();
    drop(input);
    deliver(out, &output, &phases, r.map_err(failed), failed).await?;
    Ok(new_name)
}

/// The parts of a video's conversion on the bar: `set(phase, 0 to 1)`.
#[derive(Clone)]
pub(crate) struct Phases {
    pub(crate) p: Arc<Progress>,
    pub(crate) count: u32,
}

impl Phases {
    pub(crate) fn set(&self, phase: u32, f: f32) {
        let whole = (phase as f32 + f.clamp(0.0, 1.0)) / self.count as f32;
        self.p.part.store((whole * 1000.0) as u32, Ordering::Relaxed);
    }
}

/// `path` of `op` into the local `file`, 4 MB at a time; `progress` gets 0 to 1.
pub(crate) async fn download(op: &Store, path: &str, file: &std::path::Path, cancelled: impl Fn() -> bool, progress: impl Fn(f32)) -> std::io::Result<()> {
    let r = download_pieces(op, path, file, cancelled, progress).await;
    // Cancelled or failed: nothing half copied is left behind.
    if r.is_err() {
        let _ = std::fs::remove_file(file);
    }
    r
}

/// The error of a transfer stopped by Cancel, piece by piece (#74).
fn stopped() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled")
}

async fn download_pieces(op: &Store, path: &str, file: &std::path::Path, cancelled: impl Fn() -> bool, progress: impl Fn(f32)) -> std::io::Result<()> {
    use std::io::Write;
    let size = op.size(path).await.map_err(|(_, d)| std::io::Error::other(d))?;
    let mut f = std::fs::File::create(file)?;
    let mut at = 0;
    while at < size {
        if cancelled() {
            return Err(stopped());
        }
        let piece = op.read_range(path, at, (size - at).min(4 << 20)).await.map_err(|(_, d)| std::io::Error::other(d))?;
        if piece.is_empty() {
            return Err(std::io::Error::other("short read"));
        }
        f.write_all(&piece)?;
        at += piece.len() as u64;
        progress(at as f32 / size as f32);
    }
    Ok(())
}

/// The local `file` written to `to` of `out`, 1 MB at a time; `progress` gets 0 to 1 as it is
/// read (a few MB ahead of what the server has). Cancelled, it stops at the next MB and the
/// destination deletes what it had (`write_chunks` does on an error).
pub(crate) async fn upload(out: &Store, to: &str, file: &std::path::Path, cancelled: impl Fn() -> bool + Send + 'static, progress: impl Fn(f32) + Send + 'static) -> std::io::Result<()> {
    use std::io::Read;
    let size = std::fs::metadata(file)?.len();
    let mut f = std::fs::File::open(file)?;
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let reader = tokio::task::spawn_blocking(move || {
        let mut sent = 0;
        loop {
            if cancelled() {
                let _ = tx.blocking_send(Err(stopped()));
                return;
            }
            let mut buf = vec![0; 1 << 20];
            let n = match f.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    return;
                }
            };
            buf.truncate(n);
            if tx.blocking_send(Ok(bytes::Bytes::from(buf))).is_err() {
                return;
            }
            sent += n as u64;
            progress(sent as f32 / size.max(1) as f32);
        }
    });
    let r = out.write_chunks(to, size, rx).await.map_err(|(_, d)| std::io::Error::other(d));
    let _ = reader.await;
    r
}

/// How the job is going, read by the window; `cancel` stops it after the image under way, and
/// a video or audio file between two pieces of its download or upload (#74).
#[derive(Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub current: Mutex<String>,
    /// How far the video under way is, in thousandths.
    pub part: AtomicU32,
    pub cancel: AtomicBool,
    /// Paths of the new files.
    pub saved: Mutex<Vec<String>>,
    /// How many of `saved` the window has shown already (folder read again).
    pub listed: AtomicUsize,
    /// Images left out or failed: Fluent id and detail.
    pub failed: Mutex<Vec<(&'static str, String)>>,
    /// Names of the copies without audio saved without the camera's preview, which could not be
    /// written into them (#216).
    pub no_preview: Mutex<Vec<String>>,
    /// Images saved as JPEG without a part of their metadata, too large for it (#217): the id
    /// `META_TOO_LARGE` and the name with what was left out.
    pub left_out_meta: Mutex<Vec<(&'static str, String)>>,
    pub finished: AtomicBool,
    /// Bytes of the originals of the files saved, and of the new files (the summary at the end).
    pub bytes_before: AtomicU64,
    pub bytes_after: AtomicU64,
    /// A size could not be read: the summary leaves the sizes out.
    pub bytes_unknown: AtomicBool,
}

impl Progress {
    /// The sizes of an original and of what it became, added; one unknown makes all unknown.
    fn sized(&self, before: Option<u64>, after: Option<u64>) {
        match (before, after) {
            (Some(b), Some(a)) => {
                self.bytes_before.fetch_add(b, Ordering::Relaxed);
                self.bytes_after.fetch_add(a, Ordering::Relaxed);
            }
            _ => self.bytes_unknown.store(true, Ordering::Relaxed),
        }
    }

    /// The originals' bytes and the new files', for the summary; `None` when one was not read.
    pub fn sizes(&self) -> Option<(u64, u64)> {
        (!self.bytes_unknown.load(Ordering::Relaxed)).then(|| (self.bytes_before.load(Ordering::Relaxed), self.bytes_after.load(Ordering::Relaxed)))
    }
}

/// The sizes of the original `path` and of the files `saved` from it (a video, its audio) added
/// to `p`, read back where they are: what FFmpeg wrote is not in memory.
async fn add_sizes(p: &Progress, op: &Store, path: &str, out: &Store, saved: &[String]) {
    let mut after = Some(0);
    for s in saved {
        after = match (after, out.size(s).await) {
            (Some(a), Ok(n)) => Some(a + n),
            _ => None,
        };
    }
    p.sized(op.size(path).await.ok(), after);
}

/// Where the new files go: a folder of a source, made first if `create`. `home`: a server address
/// without a folder, whose files go into the folder the server starts in (its root may refuse them).
pub struct Dest {
    pub op: Store,
    pub dir: String,
    pub create: bool,
    pub home: bool,
}

/// The folder the files go into: a server's starting folder for `home`, made first if `create`.
async fn dest_dir(out: &Store, dest: String, create: bool, home: bool) -> Result<String, source::Error> {
    let dest = match (out, home) {
        (Store::Sftp(server), true) => server.home().await?,
        (Store::Ftp(server), true) => server.home().await?,
        _ => dest,
    };
    if create {
        out.create_dir(&dest).await?;
    }
    Ok(dest)
}

/// Every image of `paths` (read from `op`) after `job`, written into `to` with a free name;
/// never over a file.
pub async fn run(op: &Store, paths: Vec<String>, to: Dest, job: Job, p: Arc<Progress>) {
    p.total.store(paths.len(), Ordering::Relaxed);
    let Dest { op: out, dir: dest, create, home } = to;
    let dest = match dest_dir(&out, dest, create, home).await {
        Ok(d) => d,
        Err(e) => {
            p.failed.lock().unwrap().push(e);
            p.finished.store(true, Ordering::Relaxed);
            return;
        }
    };
    // Without the names that are there nothing can be written "never over a file": the work stops.
    let mut taken: HashSet<String> = match out.names(&dest).await {
        Ok(names) => names,
        Err(e) => {
            p.failed.lock().unwrap().push(e);
            p.finished.store(true, Ordering::Relaxed);
            return;
        }
    };
    let job = std::sync::Arc::new(job);
    for path in paths {
        if p.cancel.load(Ordering::Relaxed) {
            break;
        }
        let name = source::name(&path).to_string();
        *p.current.lock().unwrap() = name.clone();
        p.part.store(0, Ordering::Relaxed);
        if let Some(audio) = job.audio {
            match crate::audio_job::run(op, (&job.base, &path), &out, &dest, audio, &mut taken, &p).await {
                Ok(saved) => {
                    add_sizes(&p, op, &path, &out, &saved).await;
                    p.saved.lock().unwrap().extend(saved);
                }
                Err(_) if p.cancel.load(Ordering::Relaxed) => break,
                Err(e) => p.failed.lock().unwrap().push(e),
            }
            p.done.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        if crate::decode::is_played(&path) {
            let r = match job.sound {
                _ if crate::decode::is_video(&path) => video(op, &path, (&out, &dest, &taken), &job, &p).await,
                Some(s) => crate::audio_convert::run(op, &path, (&out, &dest, &taken), s, &p).await,
                None => Err((crate::audio_convert::SKIP_AUDIO, name.clone())),
            };
            match r {
                Ok(new_name) => {
                    let to = format!("{dest}{new_name}");
                    add_sizes(&p, op, &path, &out, std::slice::from_ref(&to)).await;
                    p.saved.lock().unwrap().push(to);
                    taken.insert(new_name);
                }
                // Cancelled: neither saved nor "left out", the summary says where it stopped.
                Err(_) if p.cancel.load(Ordering::Relaxed) => break,
                Err(e) => p.failed.lock().unwrap().push(e),
            }
            p.done.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        match picture(op, &path, name.clone(), &job).await {
            Ok(made) => saved_picture(&p, (&out, &dest, &mut taken), &name, made).await,
            Err(e) => p.failed.lock().unwrap().push(e),
        }
        p.done.fetch_add(1, Ordering::Relaxed);
    }
    p.finished.store(true, Ordering::Relaxed);
}

/// The picture `name` made by the job (new name, content, size of the original, what a JPEG
/// left out) written into `dest` of `out` under a name not `taken`, and counted in `p`.
async fn saved_picture(p: &Progress, (out, dest, taken): (&Store, &str, &mut HashSet<String>), name: &str, (new_name, data, before, left): (String, Vec<u8>, u64, LeftOut)) {
    let new_name = source::free_name(taken, &new_name);
    let to = format!("{dest}{new_name}");
    let after = data.len() as u64;
    match crate::replace::write_whole(out, &to, data).await {
        Ok(()) => {
            taken.insert(new_name);
            p.sized(Some(before), Some(after));
            p.saved.lock().unwrap().push(to);
            // Saved, without what a JPEG has no room for (#217): the summary says it.
            if left.any() {
                p.left_out_meta.lock().unwrap().push((META_TOO_LARGE, format!("{name} ({})", left.parts(crate::encoders::AND))));
            }
        }
        Err(e) => p.failed.lock().unwrap().push(e),
    }
}

/// The job decodes its pictures at full resolution: not Copy with the metadata, nor Export
/// drawing (drawings have limits of their own). A picture too large for the memory is then left
/// out, told before it is decoded (`read_decoded`), unless opened anyway before.
fn decodes(job: &Job) -> bool {
    !job.does_nothing() && job.drawing.is_none()
}

/// The bytes of the picture `path` that `job` decodes, unless too large for the memory
/// (`view_memory::read_unless_too_large`: over 32 MB from the start of its file when it can be,
/// `too_large`); all of it when opened anyway before (`view_memory::anyway`).
async fn read_decoded(op: &Store, path: &str, job: &Job) -> Result<Vec<u8>, crate::view_memory::NotRead> {
    if crate::view_memory::own_limits(path) || crate::view_memory::anyway(&job.base, path, false) {
        return op.read(path).await.map_err(crate::view_memory::NotRead::Failed);
    }
    let (name, ram) = (source::name(path), crate::thumb_memory::ram());
    crate::view_memory::read_unless_too_large(op, path, None, |bytes, size| too_large(job, (bytes, size, name), ram)).await
}

/// The memory the picture `bytes` (the start of a file of `size` bytes, or all of it) needs for
/// `job`, when it is over half of `ram`: only turned, as `convert_with` turns it, so a JPEG turned
/// without re-encoding never (`save_back::turn_too_large`, Piero, 21/11/2026); else decoded whole.
fn too_large(job: &Job, (bytes, size, name): (&[u8], u64, &str), ram: u64) -> Option<u64> {
    if job.format.is_none() && job.size == Size::Same {
        return crate::save_back::turn_too_large(bytes, size, name, job.turn, ram);
    }
    crate::view_memory::estimate(bytes, size, name, crate::view_memory::Use::Whole).over_half(bytes.len(), size, name, ram)
}

/// The picture at `path` (named `name`) after `job`, with its photo's sidecar (#24) and the
/// layers shown of a drawing: the new name, the content and the size of the original.
async fn picture(op: &Store, path: &str, name: String, job: &Job) -> Result<(String, Vec<u8>, u64, LeftOut), (&'static str, String)> {
    let bytes = match decodes(job) {
        true => read_decoded(op, path, job).await.map_err(|e| match e {
            crate::view_memory::NotRead::TooLarge(_) => (SKIP_TOO_LARGE, name.clone()),
            crate::view_memory::NotRead::Failed(e) => e,
        })?,
        false => op.read(path).await?,
    };
    let (mut job, before) = (job.clone(), bytes.len() as u64);
    job.shown = job.layers.get(path).cloned();
    let (sibling, local) = (crate::loader::siblings(op.clone(), path.to_string()), crate::occt::local(op, path));
    let sidecar = crate::sidecar::read(op, path).await.map(|s| s.xmp);
    let made = tokio::task::spawn_blocking(move || crate::occt::from_file(local, || convert_with(&bytes, &name, &job, &sibling, sidecar.as_deref()))).await.unwrap_or_else(|e| Err(("batch-failed", e.to_string())));
    made.map(|(name, data, left)| (name, data, before, left))
}

#[cfg(test)]
mod tests {
    use image::DynamicImage;

    /// Every file of `EYECANSEE_META_IN` converted to JPEG, PNG, WebP, TIFF and AVIF into
    /// `EYECANSEE_META_OUT`, as Convert and resize writes them, to compare the metadata with exiftool:
    /// `EYECANSEE_META_IN=<dir> EYECANSEE_META_OUT=<dir> cargo test --release meta_files_live -- --ignored --nocapture`
    /// (`EYECANSEE_META_FORMATS=jpg,tif` for only some of them). A RAW photo's sidecar is read as
    /// the app reads it (#24).
    #[test]
    #[ignore]
    fn meta_files_live() {
        let (src, out) = (std::env::var("EYECANSEE_META_IN").unwrap(), std::env::var("EYECANSEE_META_OUT").unwrap());
        for entry in std::fs::read_dir(src).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let bytes = std::fs::read(entry.path()).unwrap();
            let only = std::env::var("EYECANSEE_META_FORMATS").unwrap_or_default();
            for format in [super::Format::Jpeg, super::Format::Png, super::Format::Webp, super::Format::Tiff, super::Format::Avif] {
                if !only.is_empty() && !only.split(',').any(|e| e == format.ext()) {
                    continue;
                }
                let path = entry.path().to_string_lossy().to_string();
                let sidecar = crate::sidecar::names(&path).into_iter().find_map(|p| std::fs::read(&p).ok().filter(|x| crate::sidecar::belongs(&path, &p, crate::sidecar::sidecar_for(x).as_deref(), false)));
                match super::convert_with(&bytes, &name, &super::Job { format: Some(format), ..Default::default() }, &|_| None, sidecar.as_deref()) {
                    // The original's extension kept in the name: a.miff and a.psb do not overwrite each other.
                    Ok((new, data, _)) => std::fs::write(format!("{out}/{name}.{}", new.rsplit_once('.').unwrap().1), data).unwrap(),
                    Err(e) => println!("{name} -> {format:?}: {e:?}"),
                }
            }
        }
    }

    /// Download, ffmpeg and upload each take a third of the video's bar.
    #[test]
    fn video_phases_share_the_bar() {
        let ph = super::Phases { p: std::sync::Arc::default(), count: 3 };
        let part = |phase, f| {
            ph.set(phase, f);
            ph.p.part.load(std::sync::atomic::Ordering::Relaxed)
        };
        assert_eq!((part(0, 0.0), part(0, 1.0), part(1, 0.5), part(2, 1.0)), (0, 333, 500, 1000));
        assert_eq!(part(1, 7.0), 666);
    }

    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(target(4000, 3000, Size::Same, false), (4000, 3000));
        assert_eq!(target(4000, 3000, Size::Percent(50), false), (2000, 1500));
        assert_eq!(target(4000, 3000, Size::Percent(200), false), (4000, 3000));
        assert_eq!(target(4000, 3000, Size::Percent(200), true), (8000, 6000));
        // Inside the box, the proportions kept: here the height decides.
        assert_eq!(target(4000, 3000, Size::Fit(1920, 1080), false), (1440, 1080));
        assert_eq!(target(3000, 4000, Size::Fit(1920, 1920), false), (1440, 1920));
        assert_eq!(target(800, 600, Size::Fit(1920, 1080), false), (800, 600));
        assert_eq!(target(800, 600, Size::Fit(1920, 1080), true), (1440, 1080));
        assert_eq!(target(5000, 10, Size::Percent(1), false), (50, 1));
        assert!(Job { format: None, ..Default::default() }.does_nothing());
        assert_eq!(with_ext("a.b.jpeg", Format::Png), "a.b.png");
        assert_eq!(with_ext("noext", Format::Webp), "noext.webp");
    }

    /// The sizes of Resize on a landscape and a portrait photo, smaller ones left as they are.
    #[test]
    fn sizes_of_each_kind() {
        for (w, h, size, out) in [
            (4000, 3000, Size::Long(1920), (1920, 1440)),
            (3000, 4000, Size::Long(1920), (1440, 1920)),
            (4000, 3000, Size::Short(1080), (1440, 1080)),
            (3000, 4000, Size::Short(1080), (1080, 1440)),
            (4000, 3000, Size::Width(1000), (1000, 750)),
            (3000, 4000, Size::Height(600), (450, 600)),
            (800, 600, Size::Long(1920), (800, 600)),
            (4000, 3000, Size::Stretch(500, 500), (500, 500)),
            // Not enlarged: each side at most the original's.
            (400, 300, Size::Stretch(500, 200), (400, 200)),
            (4000, 3000, Size::Crop(1920, 1920), (1920, 1920)),
            (3000, 4000, Size::Crop(1920, 1080), (1920, 1080)),
            (800, 600, Size::Crop(1920, 1080), (800, 600)),
            (3000, 500, Size::Crop(1920, 1080), (1920, 500)),
        ] {
            assert_eq!(target(w, h, size, false), out, "{w}x{h} {size:?}");
        }
        assert_eq!(target(800, 600, Size::Long(1920), true), (1920, 1440));
        assert_eq!(target(400, 300, Size::Stretch(500, 200), true), (500, 200));
        assert_eq!(target(800, 600, Size::Crop(1920, 1080), true), (1920, 1080));
        // The middle kept, cut on the side that is too long for the box.
        assert_eq!(crop(4000, 3000, Size::Crop(1920, 1920), false), Some((500, 0, 3000, 3000)));
        assert_eq!(crop(3000, 4000, Size::Crop(1920, 1080), false), Some((0, 1156, 3000, 1688)));
        assert_eq!(crop(3000, 500, Size::Crop(1920, 1080), false), Some((540, 0, 1920, 500)));
        assert_eq!(crop(800, 600, Size::Crop(1920, 1080), true), Some((0, 75, 800, 450)));
        assert_eq!(crop(800, 600, Size::Crop(1920, 1080), false), None);
        assert_eq!(crop(4000, 3000, Size::Fit(100, 100), false), None);
        // The names of the copies.
        assert_eq!(tagged("photo.jpg", &Size::Long(1920).tag()), "photo (1920 px).jpg");
        assert_eq!(tagged("a.b.png", &Size::Percent(50).tag()), "a.b (50%).png");
        assert_eq!(tagged("noext", &Size::Crop(800, 600).tag()), "noext (800x600)");
        assert_eq!(tagged(".hidden", " (1 px)"), ".hidden (1 px)");
        assert!(Size::KINDS.iter().all(|k| k.same_kind(*k) && k.for_videos() == matches!(k, Size::Same | Size::Percent(_) | Size::Fit(..))));
    }

    /// A destination whose names cannot be read stops the work: with no names every name would
    /// look free, and the file that is there would be written over. A folder that lets files in
    /// but does not let itself be listed (as a write-only folder of a server).
    #[cfg(unix)]
    #[test]
    fn a_destination_that_cannot_be_listed_stops_the_work() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("eyecansee-unlisted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([x as u8, y as u8, 9])));
        std::fs::write(dir.join("a.png"), file(&img, image::ImageFormat::Png)).unwrap();
        std::fs::write(dir.join("out/a.png"), b"the one that is there").unwrap();
        std::fs::set_permissions(dir.join("out"), std::fs::Permissions::from_mode(0o300)).unwrap();
        // Who may read every folder whatever its permissions (root, as in a container) cannot
        // make this one unreadable: nothing to try there.
        if std::fs::read_dir(dir.join("out")).is_ok() {
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        }
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let p = Arc::new(Progress::default());
        let to = Dest { op: op.clone(), dir: "out/".into(), create: false, home: false };
        let job = Job { format: None, ..Default::default() };
        tokio::runtime::Runtime::new().unwrap().block_on(run(&op, vec!["a.png".into()], to, job, p.clone()));
        std::fs::set_permissions(dir.join("out"), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(p.finished.load(Ordering::Relaxed));
        assert_eq!(p.failed.lock().unwrap().len(), 1);
        assert!(p.saved.lock().unwrap().is_empty());
        assert_eq!(std::fs::read(dir.join("out/a.png")).unwrap(), b"the one that is there");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Convert and Resize leave out a picture too large for the memory of this computer, told from
    /// the start of its file, over `view_memory::ONCE` (a RAW, of bytes at random, whose estimate
    /// goes by its size: 64 bytes for every 72 of RAM it needs), and go on with the others; Copy
    /// with the metadata, which decodes nothing, copies it.
    #[test]
    fn too_large_left_out() {
        let dir = std::env::temp_dir().join(format!("eyecansee-batch-large-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let size = crate::thumb_memory::ram() / 64;
        assert!(size > crate::view_memory::ONCE, "read from its start");
        std::fs::File::create(dir.join("huge.cr3")).unwrap().set_len(size).unwrap();
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(40, 30, |x, y| image::Rgb([x as u8, y as u8, 9])));
        std::fs::write(dir.join("a.png"), file(&img, image::ImageFormat::Png)).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let paths = vec!["huge.cr3".to_string(), "a.png".to_string()];
        let p = Arc::new(Progress::default());
        let to = Dest { op: op.clone(), dir: "out/".into(), create: true, home: false };
        crate::source::BYTES_READ.with(|n| n.set(0));
        rt.block_on(run(&op, paths, to, Job { format: Some(Format::Jpeg), ..Default::default() }, p.clone()));
        assert_eq!(*p.failed.lock().unwrap(), [(SKIP_TOO_LARGE, "huge.cr3".to_string())]);
        assert_eq!(*p.saved.lock().unwrap(), ["out/a.jpg"]);
        let read = crate::source::BYTES_READ.with(std::cell::Cell::get);
        assert!(read < 2 * crate::view_memory::HEAD, "{read} bytes read");
        let copy = Job { format: None, ..Default::default() };
        assert!(copy.does_nothing() && !decodes(&copy));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Convert with only a turn (#177): a JPEG turned without re-encoding is never left out for
    /// the memory, and is looked at only whole; one encoded again, a JPEG converted to another
    /// format, a PNG turned, are when over half the RAM (here none).
    #[test]
    fn only_a_decoding_too_large() {
        let jpeg = |w: usize, h: usize| {
            let rgb = vec![90u8; w * h * 3];
            let mut c = turbojpeg::Compressor::new().unwrap();
            c.set_subsamp(turbojpeg::Subsamp::Sub2x2).unwrap();
            c.compress_to_vec(turbojpeg::Image { pixels: rgb.as_slice(), width: w, pitch: w * 3, height: h, format: turbojpeg::PixelFormat::RGB }).unwrap()
        };
        let turned = Job { format: None, turn: Turn::default().then(crate::transform::Op::Cw), ..Default::default() };
        let converted = Job { format: Some(Format::Png), ..turned.clone() };
        let (fits, not) = (jpeg(32, 32), jpeg(32, 40));
        let len = |b: &[u8]| b.len() as u64;
        assert_eq!(too_large(&turned, (&fits, len(&fits), "a.jpg"), 0), None);
        assert!(too_large(&turned, (&not, len(&not), "a.jpg"), 0).is_some());
        assert_eq!(too_large(&turned, (&not, len(&not) + 1, "a.jpg"), 0), None, "its start only");
        assert!(too_large(&converted, (&fits, len(&fits), "a.jpg"), 0).is_some());
        assert_eq!(too_large(&converted, (&fits, len(&fits), "a.jpg"), 1 << 40), None);
        let png = file(&DynamicImage::new_rgb8(8, 8), image::ImageFormat::Png);
        assert!(too_large(&turned, (&png, len(&png), "a.png"), 0).is_some());
    }

    /// Resize with a preset: copies next to the originals with the size in their names, a name
    /// taken gets " (2)", an image already of that size (or smaller) and a format not written are
    /// left out.
    #[test]
    fn resized_next_to_the_originals() {
        let dir = std::env::temp_dir().join(format!("eyecansee-resize-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(400, 300, |x, y| image::Rgb([x as u8, y as u8, 9])));
        let png = file(&img, image::ImageFormat::Png);
        std::fs::write(dir.join("a.png"), &png).unwrap();
        std::fs::write(dir.join("a (200 px).png"), b"taken").unwrap();
        std::fs::write(dir.join("t.tga"), file(&img, image::ImageFormat::Tga)).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let resize = |size: Size| {
            let p = Arc::new(Progress::default());
            let to = Dest { op: op.clone(), dir: String::new(), create: false, home: false };
            let job = Job { format: None, size, tag: size.tag(), ..Default::default() };
            rt.block_on(run(&op, vec!["a.png".into(), "t.tga".into()], to, job, p.clone()));
            let failed = p.failed.lock().unwrap().clone();
            (p.saved.lock().unwrap().clone(), failed)
        };
        let (saved, failed) = resize(Size::Long(200));
        assert_eq!(saved, ["a (200 px) (2).png"]);
        assert_eq!(failed, [(SKIP_FORMAT, "t.tga".to_string())]);
        let back = crate::decode::decode_full(&std::fs::read(dir.join(&saved[0])).unwrap(), "x.png").unwrap();
        assert_eq!((back.width(), back.height()), (200, 150));
        // Larger than the image, or its size: no copy, said by name (#119).
        for size in [Size::Long(1920), Size::Long(400), Size::Fit(400, 400), Size::Stretch(400, 300), Size::Crop(400, 300), Size::Percent(100)] {
            let (saved, failed) = resize(size);
            assert!(saved.is_empty(), "{size:?}: {saved:?}");
            assert_eq!(failed[0], (SKIP_SAME_SIZE, "a.png".to_string()), "{size:?}");
        }
        assert!(!dir.join("a (1920 px).png").exists());
        // Custom… (#119): the size in the name only when the picture was resized; left at its
        // size in another format it keeps its name, and a name taken gets " (2)"; only turned,
        // what was done.
        let custom = |format: Option<Format>, size: Size, turn: Turn| {
            let p = Arc::new(Progress::default());
            let to = Dest { op: op.clone(), dir: String::new(), create: false, home: false };
            let job = Job { format, size, turn, tag: size.tag(), tag_turned: " (rotated)".into(), ..Default::default() };
            rt.block_on(run(&op, vec!["a.png".into()], to, job, p.clone()));
            assert!(p.failed.lock().unwrap().is_empty(), "{size:?}");
            p.saved.lock().unwrap().clone()
        };
        let none = Turn::default();
        assert_eq!(custom(Some(Format::Jpeg), Size::Long(1920), none), ["a.jpg"]);
        assert_eq!(custom(Some(Format::Jpeg), Size::Long(1920), none), ["a (2).jpg"]);
        assert_eq!(custom(Some(Format::Jpeg), Size::Long(200), none), ["a (200 px).jpg"]);
        let cw = none.then(crate::transform::Op::Cw);
        assert_eq!(custom(None, Size::Long(1920), cw), ["a (rotated).png"]);
        assert_eq!(custom(None, Size::Long(200), cw), ["a (200 px) (3).png"]);
        for f in ["a.jpg", "a (2).jpg", "a (200 px).jpg", "a (rotated).png", "a (200 px) (3).png"] {
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        // Copy with the metadata: the file as it is, also in a format not written (TGA).
        let p = Arc::new(Progress::default());
        let job = Job { format: None, ..Default::default() };
        rt.block_on(run(&op, vec!["a.png".into(), "t.tga".into()], Dest { op: op.clone(), dir: String::new(), create: false, home: false }, job, p.clone()));
        assert_eq!(*p.saved.lock().unwrap(), ["a (2).png", "t (2).tga"]);
        assert_eq!(std::fs::read(dir.join("a (2).png")).unwrap(), png);
        assert!(p.failed.lock().unwrap().is_empty());
        // The summary's sizes (#134): a copy as it is weighs what its original does.
        let both = png.len() as u64 + std::fs::metadata(dir.join("t.tga")).unwrap().len();
        assert_eq!(p.sizes(), Some((both, both)));
        // Cropped in the middle: the left column of the copy comes from x = 50.
        let (saved, _) = resize(Size::Crop(100, 100));
        let back = crate::decode::decode_full(&std::fs::read(dir.join(&saved[0])).unwrap(), "x.png").unwrap().to_rgb8();
        assert_eq!((saved[0].as_str(), back.dimensions()), ("a (100x100).png", (100, 100)));
        assert!((48..=56).contains(&back.get_pixel(0, 50)[0]), "{:?}", back.get_pixel(0, 50));
        assert_eq!(std::fs::read(dir.join("a.png")).unwrap(), png, "the original untouched");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn file(img: &DynamicImage, format: image::ImageFormat) -> Vec<u8> {
        let mut v = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut v), format).unwrap();
        v
    }

    #[test]
    fn convert_images() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(400, 300, |x, y| image::Rgb([x as u8, y as u8, 9])));
        let png = file(&img, image::ImageFormat::Png);
        // Another format and a smaller size.
        let job = Job { format: Some(Format::Webp), size: Size::Fit(200, 200), ..Default::default() };
        let (name, data) = convert(&png, "a.png", &job).unwrap();
        assert_eq!(name, "a.webp");
        let back = crate::decode::decode_full(&data, &name).unwrap();
        assert_eq!((back.width(), back.height()), (200, 150));
        // Its own format, half the size: still a PNG with the same name.
        let job = Job { format: None, size: Size::Percent(50), filter: Filter::Nearest, ..Default::default() };
        let (name, data) = convert(&png, "a.png", &job).unwrap();
        assert_eq!(name, "a.png");
        let back = crate::decode::decode_full(&data, &name).unwrap();
        assert_eq!((back.width(), back.height()), (200, 150));
        // Its own format cannot be written: left out, and said why.
        let tga = file(&img, image::ImageFormat::Tga);
        assert_eq!(convert(&tga, "a.tga", &job).unwrap_err(), (SKIP_FORMAT, "a.tga".to_string()));
        // An animation would lose its frames: left out, whatever the format.
        let apng = crate::encode::tests::apng(&png);
        assert_eq!(convert(&apng, "b.png", &Job::default()).unwrap_err(), (SKIP_INSIDE, "b.png".to_string()));
        // A phone JPEG with more after the picture (here: a motion photo video): converted.
        let jpeg = [file(&img, image::ImageFormat::Jpeg).as_slice(), b"\0\0\0\x18ftypmp42"].concat();
        let job = Job { format: None, size: Size::Percent(50), ..Default::default() };
        let (name, data) = convert(&jpeg, "p.jpg", &job).unwrap();
        assert_eq!(name, "p.jpg");
        assert!(crate::save_back::can_turn_in_place(&data, &name));
        // Turned a quarter and fitted: the box applies to the turned image.
        let job = Job { format: Some(Format::Png), size: Size::Fit(150, 1000), filter: Filter::Nearest, turn: Turn::default().then(crate::transform::Op::Cw), ..Default::default() };
        let (_, data) = convert(&png, "a.png", &job).unwrap();
        let back = crate::decode::decode_full(&data, "a.png").unwrap();
        assert_eq!((back.width(), back.height()), (150, 200));
        // The top left corner of the original (0, 0) is now at the top right.
        let corner = back.to_rgb8().get_pixel(149, 0).0;
        assert!(corner[0] <= 2 && corner[1] <= 2, "{corner:?}");
        // Only turned, own format: a JPEG of 16-pixel multiples turned without encoding again.
        let job = Job { format: None, turn: Turn::default().then(crate::transform::Op::Half), ..Default::default() };
        assert!(!job.does_nothing());
        let jpeg = file(&img.crop_imm(0, 0, 320, 240), image::ImageFormat::Jpeg);
        let (_, data) = convert(&jpeg, "q.jpg", &job).unwrap();
        assert_eq!(Some(data), crate::transform::jpeg_lossless(&jpeg, job.turn));
        // Not an image.
        assert_eq!(convert(b"nothing", "c.jpg", &Job::default()).unwrap_err().0, crate::fault::NOT_OPENED);
    }

    /// JPEG chosen in the window: the quality set there wins over the original's; Same as the
    /// original: the original's quality (there is no field for it).
    #[test]
    fn jpeg_quality_chosen_or_kept() {
        let jpeg = crate::encode::jpeg_quality::tests::jpeg(70, crate::encode::Chroma::Half);
        let quality = |data: &[u8]| crate::encode::jpeg_quality::of(data).map(|(q, _)| q);
        let chosen = Job { format: Some(Format::Jpeg), options: Options { format: Format::Jpeg, jpeg_quality: 40, ..Options::default() }, ..Default::default() };
        assert_eq!(quality(&convert(&jpeg, "a.jpg", &chosen).unwrap().1), Some(40));
        let kept = Job { format: None, size: Size::Fit(32, 32), ..Default::default() };
        assert_eq!(quality(&convert(&jpeg, "a.jpg", &kept).unwrap().1), Some(70));
    }

    /// An Ultra HDR photo resized as JPEG, or converted to JPEG, stays Ultra HDR (#65); in PNG, or
    /// without its metadata, it is the picture alone. Cropped, its gain map is cropped too.
    #[test]
    fn ultra_hdr_kept_in_jpeg() {
        let bytes = crate::ultrahdr::tests::sample(64, 32);
        let (_, cropped) = convert(&bytes, "a.jpg", &Job { format: None, size: Size::Crop(16, 16), ..Default::default() }).unwrap();
        let (photo, gain) = crate::ultrahdr::split(&cropped).expect("still Ultra HDR");
        let side = |b: &[u8]| image::load_from_memory(b).map(|i| (i.width(), i.height())).unwrap();
        assert_eq!(side(photo), (16, 16));
        let g = side(gain);
        assert_eq!(g.0, g.1, "the gain map square as the photo: {g:?}");
        let fit = Job { format: None, size: Size::Fit(32, 32), ..Default::default() };
        let jpeg = Job { format: Some(Format::Jpeg), options: Options { format: Format::Jpeg, jpeg_quality: 70, ..Options::default() }, ..Default::default() };
        for job in [fit, jpeg] {
            let (_, data) = convert(&bytes, "a.jpg", &job).unwrap();
            assert!(crate::ultrahdr::is(&data), "{:?}", job.format);
        }
        let (_, png) = convert(&bytes, "a.jpg", &Job { format: Some(Format::Png), ..Default::default() }).unwrap();
        assert!(!crate::ultrahdr::is(&png) && png.starts_with(b"\x89PNG"));
        let bare = Job { format: Some(Format::Jpeg), options: Options { format: Format::Jpeg, meta: Keep::Nothing, ..Options::default() }, ..Default::default() };
        assert!(!crate::ultrahdr::is(&convert(&bytes, "a.jpg", &bare).unwrap().1));
    }

    /// The real Ultra HDR photos halved as JPEG (#65): the Apple one of the test files
    /// (`notes/test-files/online/telefono`) and the Pixel 7a Motion Photo of `EYECANSEE_SAMPLES`
    /// (the same folder), written to `EYECANSEE_SAMPLES_OUT` to look at. Each stays Ultra HDR with
    /// its gain map halved, MPF lengths adding up to the file, and no video after it.
    #[test]
    #[ignore]
    fn ultra_hdr_resized_live() {
        use image::GenericImageView;
        let out_dir = std::env::var("EYECANSEE_SAMPLES_OUT").expect("EYECANSEE_SAMPLES_OUT");
        let samples = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES");
        let apple = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("notes/test-files/online/telefono/apple_gainmap_new.jpg");
        for path in [apple, std::path::Path::new(&samples).join("PXL_20250327_052203484.MP.jpg")] {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let bytes = std::fs::read(&path).unwrap();
            let (w, h) = image::load_from_memory(&bytes).unwrap().dimensions();
            let (gw, _) = image::load_from_memory(crate::ultrahdr::split(&bytes).unwrap().1).unwrap().dimensions();
            let job = Job { format: None, size: Size::Percent(50), ..Default::default() };
            let (_, d) = convert(&bytes, &name, &job).unwrap();
            let (photo, gain) = crate::ultrahdr::split(&d).expect("still Ultra HDR");
            assert_eq!(photo.len() + gain.len(), d.len(), "{name}: nothing after the gain map");
            let p = image::load_from_memory(photo).unwrap().dimensions();
            let g = image::load_from_memory(gain).unwrap().dimensions();
            assert_eq!(p, (w / 2, h / 2), "{name}");
            assert!(g.0.abs_diff(gw / 2) <= 1, "{name}: gain map {g:?}, was {gw} wide");
            let text = String::from_utf8_lossy(photo);
            assert!(!text.contains("MotionPhoto") && !text.contains("MicroVideo"), "{name}: video described");
            std::fs::write(std::path::Path::new(&out_dir).join(&name), &d).unwrap();
            println!("{name}: {w}x{h} -> {p:?}, gain map {gw} -> {g:?}, {} bytes", d.len());
        }
    }

    /// A phone photo converted (resized in JPEG, or PNG) is the picture alone: the XMP no longer
    /// describes a gain map (`Container:Directory`, `hdrgm:`), the rest of it stays.
    #[test]
    fn converted_without_gain_map_xmp() {
        let xmp = concat!(
            "http://ns.adobe.com/xap/1.0/\0<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\" ",
            "xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\" hdrgm:Version=\"1.0\">",
            "<dc:creator><rdf:Seq><rdf:li>Ann</rdf:li></rdf:Seq></dc:creator>",
            "<Container:Directory><rdf:Seq><rdf:li><Container:Item Item:Semantic=\"Primary\" Item:Mime=\"image/jpeg\"/></rdf:li>",
            "<rdf:li><Container:Item Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\" Item:Length=\"100\"/></rdf:li></rdf:Seq></Container:Directory>",
            "</rdf:Description></rdf:RDF></x:xmpmeta>"
        );
        let jpeg = crate::encode::jpeg_quality::tests::jpeg(90, crate::encode::Chroma::Quarter);
        let len = (xmp.len() + 2) as u16;
        let bytes = [&jpeg[..2], &[0xFF, 0xE1], &len.to_be_bytes(), xmp.as_bytes(), &jpeg[2..]].concat();
        for job in [Job { format: None, size: Size::Fit(32, 32), ..Default::default() }, Job { format: Some(Format::Png), ..Default::default() }] {
            let (name, data) = convert(&bytes, "a.jpg", &job).unwrap();
            let text = String::from_utf8_lossy(&data);
            for gone in ["Container:Directory", "Container:Item", "hdrgm:Version"] {
                assert!(!text.contains(gone), "{name}: {gone} still there");
            }
            assert!(text.contains("<rdf:li>Ann</rdf:li>"), "{name}: the rest of the XMP");
        }
    }

    #[test]
    fn a_model_with_the_files_next_to_it() {
        // An OBJ square whose material, in the .mtl next to it, is red.
        let obj = b"mtllib m.mtl\nusemtl red\nv -1 -1 0\nv 1 -1 0\nv 1 1 0\nv -1 1 0\nf 1 2 3 4\n";
        let next = |rel: &str| (rel == "m.mtl").then(|| b"newmtl red\nKd 1 0 0\n".to_vec());
        let job = Job { format: Some(Format::Png), ..Default::default() };
        let red = |data: &[u8]| {
            let img = crate::decode::decode_full(data, "x.png").unwrap().to_rgb8();
            img.pixels().filter(|p| p[0] > 120 && p[1] < 60 && p[2] < 60).count()
        };
        assert!(red(&convert_with(obj, "q.obj", &job, &next, None).unwrap().1) > 1000);
        assert_eq!(red(&convert(obj, "q.obj", &job).unwrap().1), 0);
    }

    #[test]
    fn a_drawing_drawn_again_at_the_size_asked() {
        let dxf = b"0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n100\n21\n50\n31\n0\n0\nENDSEC\n0\nEOF\n";
        let job = Job { format: Some(Format::Png), drawing: Some(Drawn::Side(6000)), ..Default::default() };
        let (name, data) = convert(dxf, "plan.dxf", &job).unwrap();
        let back = crate::decode::decode_full(&data, &name).unwrap();
        assert_eq!((name.as_str(), back.width()), ("plan.png", 6000));
        let job = Job { drawing: Some(Drawn::Svg), ..job };
        let (name, data) = convert(dxf, "plan.dxf", &job).unwrap();
        assert!(name == "plan.svg" && data.starts_with(b"<svg"));
        // As the other AutoCAD format; a plotter file has no document to save so.
        let job = Job { drawing: Some(Drawn::Cad(crate::cad_convert::Target::Dwg)), ..job };
        let (name, data) = convert(dxf, "plan.dxf", &job).unwrap();
        assert_eq!((name.as_str(), crate::cad::read(&data, "x.dwg").unwrap().items.len()), ("plan.dwg", 1));
        assert_eq!(convert(b"IN;PU0,0;PD100,100;", "p.plt", &job).unwrap_err().0, SKIP_NOT_AUTOCAD);
        // A drawing that does not open (a text file renamed, R13 of 2026-11-21) is said as by
        // the other exports: its name and the library's text apart, for the summary.
        let (id, detail) = convert(b"Questo non e un disegno DWG.\n", "rotto.dwg", &Job { drawing: Some(Drawn::Cad(crate::cad_convert::Target::Dxf)), ..Default::default() }).unwrap_err();
        assert!(id == crate::fault::NOT_OPENED && detail.starts_with("rotto.dwg\u{1}"), "{id} {detail}");
        let (shown, copied) = crate::batch_summary::item(&crate::i18n::I18n::new("it"), id, &detail, false);
        assert!(crate::save_dialogs::plain(&shown) == "Impossibile aprire rotto.dwg" && copied.len() > shown.len(), "{shown} / {copied}");
    }

    #[test]
    fn a_drawing_exported_with_the_layers_chosen() {
        // Layer A red and shown, B green and off in the file: as the file has them, then A
        // hidden and B shown, as chosen in the viewer.
        let dxf = crate::cad_layers::tests::DXF.as_bytes();
        let job = Job { drawing: Some(Drawn::Svg), ..Default::default() };
        let svg = String::from_utf8(convert(dxf, "l.dxf", &job).unwrap().1).unwrap();
        assert!(svg.contains("#ff0000") && !svg.contains("#00ff00"));
        let d = crate::cad::read(dxf, "l.dxf").unwrap();
        let shown = d.layers.iter().map(|l| l.name != "A").collect();
        let job = Job { shown: Some(shown), ..job };
        let svg = String::from_utf8(convert(dxf, "l.dxf", &job).unwrap().1).unwrap();
        assert!(!svg.contains("#ff0000") && svg.contains("#00ff00"));
    }

    /// A name typed without extension gets the format's; one typed is kept.
    #[test]
    fn typed_name_gets_the_extension() {
        use super::{Format, typed_name};
        assert_eq!(typed_name("salva-come", Format::Jpeg), "salva-come.jpg");
        assert_eq!(typed_name(" salva-come. ", Format::Png), "salva-come.png");
        assert_eq!(typed_name(".hidden", Format::Png), ".hidden.png");
        assert_eq!(typed_name("foto.JPEG", Format::Jpeg), "foto.JPEG");
        assert_eq!(typed_name("v1.2", Format::Webp), "v1.2");
    }
    /// A picture converted to JPEG whose EXIF has no room in it is saved, and said (#217); the
    /// same picture to PNG keeps it.
    #[test]
    fn meta_too_large_for_a_jpeg_said() {
        use image::ImageEncoder;
        let mut png = Vec::new();
        let mut e = image::codecs::png::PngEncoder::new(&mut png);
        e.set_exif_metadata(crate::encode::tests::caption_exif(70_000)).unwrap();
        e.write_image(&[90; 8 * 6 * 3], 8, 6, image::ExtendedColorType::Rgb8).unwrap();
        let to = |format| convert_with(&png, "a.png", &Job { format: Some(format), ..Job::default() }, &|_| None, None).unwrap();
        let (name, jpeg, left) = to(Format::Jpeg);
        assert_eq!((name.as_str(), left.parts("and").as_str()), ("a.jpg", "EXIF"));
        assert_eq!(crate::decode::decode_full(&jpeg, "a.jpg").unwrap().width(), 8);
        assert!(!to(Format::Png).2.any());
    }
}
