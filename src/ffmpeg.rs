//! Converting videos with the `ffmpeg` program, without the interface: the containers and codecs,
//! which encoders of this computer work (`probe`), the arguments of a job (`args`) and the run
//! with its progress (`run`). FFmpeg is the system's when there is one, otherwise the component
//! downloaded once (`component::FFMPEG`). mpv stays for playing and thumbnails.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::batch::Size;
use crate::transform::Turn;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
    Vp9,
    Vp8,
    Mpeg4,
    ProRes,
    Gif,
    /// The streams copied as they are (`-c copy`): no re-encoding, seconds, no loss; the turn and
    /// the size do not apply. The window names it from Fluent (`batch-video-copy`).
    Copy,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC (H.265)",
            Codec::Av1 => "AV1",
            Codec::Vp9 => "VP9",
            Codec::Vp8 => "VP8",
            Codec::Mpeg4 => "MPEG-4",
            Codec::ProRes => "ProRes",
            Codec::Gif => "GIF",
            Codec::Copy => "copy",
        }
    }

    /// FFmpeg's encoders, the graphics card first (NVIDIA, Intel, AMD, Windows' own, VA-API on
    /// Linux), then the processor.
    pub fn encoders(self) -> &'static [&'static str] {
        match self {
            Codec::H264 => &["h264_nvenc", "h264_qsv", "h264_amf", "h264_mf", "h264_vaapi", "libx264", "libopenh264"],
            Codec::Hevc => &["hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_mf", "hevc_vaapi", "libx265"],
            Codec::Av1 => &["av1_nvenc", "av1_qsv", "av1_amf", "av1_mf", "av1_vaapi", "libsvtav1", "libaom-av1", "librav1e"],
            Codec::Vp9 => &["vp9_qsv", "vp9_vaapi", "libvpx-vp9"],
            Codec::Vp8 => &["libvpx"],
            Codec::Mpeg4 => &["mpeg4"],
            Codec::ProRes => &["prores_ks"],
            Codec::Gif => &["gif"],
            Codec::Copy => &["copy"],
        }
    }
}

pub struct Container {
    pub name: &'static str,
    /// FFmpeg's muxer.
    pub muxer: &'static str,
    pub ext: &'static str,
    pub codecs: &'static [Codec],
    /// Audio encoders, the first one present is used; none: no sound.
    pub audio: &'static [&'static str],
}

pub const CONTAINERS: &[Container] = &[
    Container { name: "MP4", muxer: "mp4", ext: "mp4", codecs: &[Codec::H264, Codec::Hevc, Codec::Av1, Codec::Vp9, Codec::Mpeg4, Codec::Copy], audio: &["aac"] },
    Container {
        name: "MKV",
        muxer: "matroska",
        ext: "mkv",
        codecs: &[Codec::H264, Codec::Hevc, Codec::Av1, Codec::Vp9, Codec::Vp8, Codec::Mpeg4, Codec::ProRes, Codec::Copy],
        audio: &["libopus", "aac"],
    },
    Container { name: "WebM", muxer: "webm", ext: "webm", codecs: &[Codec::Av1, Codec::Vp9, Codec::Vp8, Codec::Copy], audio: &["libopus", "libvorbis"] },
    Container { name: "MOV", muxer: "mov", ext: "mov", codecs: &[Codec::H264, Codec::Hevc, Codec::ProRes, Codec::Copy], audio: &["aac"] },
    Container { name: "AVI", muxer: "avi", ext: "avi", codecs: &[Codec::Mpeg4, Codec::H264, Codec::Copy], audio: &["libmp3lame", "ac3"] },
    Container { name: "GIF", muxer: "gif", ext: "gif", codecs: &[Codec::Gif], audio: &[] },
];

impl Codec {
    /// Fluent id of a line on what it is good for.
    pub fn hint(self) -> &'static str {
        match self {
            Codec::H264 => "video-codec-h264",
            Codec::Hevc => "video-codec-hevc",
            Codec::Av1 => "video-codec-av1",
            Codec::Vp9 => "video-codec-vp9",
            Codec::Vp8 => "video-codec-vp8",
            Codec::Mpeg4 => "video-codec-mpeg4",
            Codec::ProRes => "video-codec-prores",
            Codec::Gif => "video-codec-gif",
            Codec::Copy => "video-codec-copy",
        }
    }
}

/// Fluent id of a line on an encoder: what does the work, how fast, how good.
pub fn hint(encoder: &str) -> &'static str {
    match encoder {
        "libx264" => "video-enc-x264",
        "libx265" => "video-enc-x265",
        "libsvtav1" => "video-enc-svtav1",
        "libaom-av1" => "video-enc-aom",
        "librav1e" => "video-enc-rav1e",
        "libvpx-vp9" | "libvpx" => "video-enc-vpx",
        "libopenh264" => "video-enc-openh264",
        "mpeg4" | "prores_ks" | "gif" => "video-enc-plain",
        "copy" => "video-codec-copy",
        e => match maker(e) {
            "NVIDIA" => "video-enc-nvenc",
            "Intel Quick Sync" => "video-enc-qsv",
            "AMD" => "video-enc-amf",
            "Windows Media Foundation" => "video-enc-mf",
            _ => "video-enc-vaapi",
        },
    }
}

/// Whether the graphics card does the encoding (shown in green in the window). Windows Media
/// Foundation may or may not: not counted.
pub fn hardware(encoder: &str) -> bool {
    matches!(maker(encoder), "NVIDIA" | "Intel Quick Sync" | "AMD" | "VA-API")
}

/// Who makes an encoder, for the list in the window.
pub fn maker(encoder: &str) -> &'static str {
    match encoder.rsplit('_').next() {
        Some("nvenc") => "NVIDIA",
        Some("qsv") => "Intel Quick Sync",
        Some("amf") => "AMD",
        Some("mf") => "Windows Media Foundation",
        Some("vaapi") => "VA-API",
        _ => "CPU",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    High,
    Medium,
    Small,
}

/// What to do with every video; the size and the turn are those of the images (`batch::Job`).
#[derive(Clone, PartialEq, Debug)]
pub struct Video {
    /// Index in `CONTAINERS`.
    pub container: usize,
    pub encoder: &'static str,
    pub quality: Quality,
    pub audio: bool,
    /// Date, place, camera of the original written in the new file.
    pub meta: bool,
}

// ponytail: the first render node only; on a PC with two cards VA-API may sit on the second
const VAAPI_DEVICE: &str = "/dev/dri/renderD128";

/// A graphics card that encodes and can also read, turn and resize the video itself, so the
/// frames never go back to the processor (#33): with NVENC the processor did 16 times less work
/// for the same time (HEVC 1080p, 2026-11-08).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Cuda,
    Vaapi,
    Qsv,
}

impl Card {
    fn of(encoder: &str) -> Option<Card> {
        match encoder.rsplit('_').next() {
            Some("nvenc") => Some(Card::Cuda),
            Some("vaapi") => Some(Card::Vaapi),
            Some("qsv") => Some(Card::Qsv),
            _ => None,
        }
    }

    /// The name FFmpeg gives it, in `-hwaccel` and in its filters (`scale_cuda`, `transpose_vaapi`).
    fn name(self) -> &'static str {
        match self {
            Card::Cuda => "cuda",
            Card::Vaapi => "vaapi",
            Card::Qsv => "qsv",
        }
    }
}

/// The turn as one transposition of the card's filter: the same picture as `Turn::video_filters`.
fn card_turn(turn: Turn) -> Option<&'static str> {
    match turn.parts() {
        (false, 0) => None,
        (false, 1) => Some("clock"),
        (false, 2) => Some("reversal"),
        (false, _) => Some("cclock"),
        (true, 0) => Some("hflip"),
        (true, 1) => Some("clock_flip"),
        (true, 2) => Some("vflip"),
        (true, _) => Some("cclock_flip"),
    }
}

/// The filters done by the card: turn, then size and, for H.264, 8 bits (a phone's 10-bit HDR
/// video as H.264 would be "High 10", which almost nothing plays).
fn card_filters(card: Card, encoder: &str, size: Size, enlarge: bool, turn: Turn) -> Vec<String> {
    let c = card.name();
    let mut f: Vec<String> = card_turn(turn)
        .map(|d| match (card, d) {
            // vpp_qsv names the two flipped quarter turns the other way round (frames compared on an Intel UHD, 2026-11-08).
            (Card::Qsv, "clock_flip") => "vpp_qsv=transpose=cclock_hflip".to_string(),
            (Card::Qsv, "cclock_flip") => "vpp_qsv=transpose=clock_hflip".to_string(),
            (Card::Qsv, d) => format!("vpp_qsv=transpose={d}"),
            _ => format!("transpose_{c}=dir={d}"),
        })
        .into_iter()
        .collect();
    let scaled = if card == Card::Qsv { qsv_scale(size, enlarge) } else { scale(size, enlarge).map(|s| s.trim_start_matches("scale=").to_string()) };
    let parts: Vec<String> = scaled.into_iter().chain(encoder.starts_with("h264_").then(|| "format=nv12".to_string())).collect();
    if !parts.is_empty() {
        f.push(format!("scale_{c}={}", parts.join(":")));
    }
    f
}

/// The encoder's options for a quality, from the best to the smallest file.
fn quality_args(encoder: &str, q: Quality) -> Vec<String> {
    let i = q as usize;
    let pick = |v: [u32; 3]| v[i].to_string();
    let a: Vec<String> = match encoder {
        "libx264" | "libx265" => vec!["-crf".into(), pick([18, 23, 28]), "-preset".into(), "medium".into()],
        "libsvtav1" => vec!["-crf".into(), pick([24, 32, 40]), "-preset".into(), "8".into()],
        "libaom-av1" => vec!["-crf".into(), pick([24, 32, 40]), "-b:v".into(), "0".into(), "-cpu-used".into(), "6".into(), "-row-mt".into(), "1".into()],
        "librav1e" => vec!["-qp".into(), pick([60, 90, 120])],
        "libvpx-vp9" => vec!["-crf".into(), pick([24, 32, 40]), "-b:v".into(), "0".into(), "-row-mt".into(), "1".into(), "-cpu-used".into(), "4".into()],
        "libvpx" => vec!["-crf".into(), pick([8, 16, 30]), "-b:v".into(), "10M".into()],
        "libopenh264" => vec!["-b:v".into(), format!("{}M", pick([8, 4, 2]))],
        "mpeg4" => vec!["-q:v".into(), pick([3, 5, 8])],
        "prores_ks" => vec!["-profile:v".into(), pick([3, 2, 0])],
        e if e.ends_with("_nvenc") => vec!["-rc".into(), "vbr".into(), "-cq".into(), pick([19, 25, 31]), "-b:v".into(), "0".into(), "-preset".into(), "p5".into()],
        // `:v`: a bare `-global_quality` reaches the audio encoder too, and libopus refuses it
        // ("Quality-based encoding not supported"; seen on the POCO test from Windows).
        // ICQ (`-global_quality`) is not there for VP9 on QSV ("device failed", Intel iGPU on
        // Fedora, FFmpeg 8.1): a fixed quantizer instead.
        "vp9_qsv" => vec!["-q:v".into(), pick([20, 25, 30])],
        e if e.ends_with("_qsv") => vec!["-global_quality:v".into(), pick([20, 25, 30])],
        e if e.ends_with("_amf") => ["-rc", "cqp", "-qp_i", "", "-qp_p", "", "-qp_b", ""].iter().map(|s| if s.is_empty() { pick([20, 25, 30]) } else { s.to_string() }).collect(),
        // No `-hw_encoding 1`: "format negotiation failed" on a laptop with two cards (2026-10-29).
        e if e.ends_with("_mf") => vec!["-rate_control".into(), "quality".into(), "-quality".into(), pick([80, 65, 50])],
        e if e.ends_with("_vaapi") => vec!["-rc_mode".into(), "CQP".into(), "-global_quality:v".into(), pick([20, 25, 30])],
        _ => Vec::new(),
    };
    a
}

/// The filters for a size: sides even, as most codecs want them; smaller videos left as they are
/// unless `enlarge`.
fn scale(size: Size, enlarge: bool) -> Option<String> {
    match size {
        Size::Same => None,
        Size::Percent(p) => {
            let p = if enlarge { p } else { p.min(100) };
            (p != 100).then(|| format!("scale=w='trunc(iw*{p}/200)*2':h=-2"))
        }
        Size::Fit(w, h) if enlarge => Some(format!("scale=w={w}:h={h}:force_original_aspect_ratio=decrease:force_divisible_by=2")),
        Size::Fit(w, h) => Some(format!("scale=w='min({w},iw)':h='min({h},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2")),
        // Pictures only (`Size::for_videos`): Convert and resize keeps them off with videos.
        _ => None,
    }
}

/// `scale`'s sizes for `scale_qsv`, which neither keeps the proportions nor evens the sides by itself.
fn qsv_scale(size: Size, enlarge: bool) -> Option<String> {
    let factor = match size {
        Size::Same => return None,
        Size::Percent(p) => {
            let p = if enlarge { p } else { p.min(100) };
            if p == 100 {
                return None;
            }
            format!("{p}/100")
        }
        Size::Fit(w, h) if enlarge => format!("min({w}/iw,{h}/ih)"),
        Size::Fit(w, h) => format!("min(1,min({w}/iw,{h}/ih))"),
        _ => return None,
    };
    // Rounded as `force_divisible_by` does: 405 px becomes 406, as on the processor.
    Some(format!("w='round(iw*{factor}/2)*2':h='round(ih*{factor}/2)*2'"))
}

/// The arguments of ffmpeg for one video. `audio_encoders`: those this FFmpeg has. FFmpeg turns
/// the picture as the phone recorded it by itself; `turn` comes after.
/// `(input, output)`: the files. `tags`: container tags added to the file (`-metadata`) when `v.meta`, such as a dashcam's position.
/// `on_card`: the video read, turned and resized by the encoder's card (`Probe::card` says where it works).
#[allow(clippy::too_many_arguments)]
pub fn args(v: &Video, size: Size, enlarge: bool, turn: Turn, on_card: bool, (input, output): (&str, &str), audio_encoders: &[String], tags: &[(String, String)]) -> Vec<String> {
    let c = &CONTAINERS[v.container];
    let vaapi = v.encoder.ends_with("_vaapi");
    let copy = v.encoder == "copy";
    let card = Card::of(v.encoder).filter(|_| on_card && !copy);
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-nostats"].map(String::from).to_vec();
    if vaapi {
        a.extend(["-vaapi_device".into(), VAAPI_DEVICE.into()]);
    }
    if let Some(card) = card {
        a.extend(["-hwaccel".into(), card.name().into(), "-hwaccel_output_format".into(), card.name().into()]);
    }
    if copy {
        // An AVI's B-frames have no times: Matroska refused them ("unknown timestamp").
        a.extend(["-fflags".into(), "+genpts".into()]);
    }
    a.extend(["-i".into(), input.into(), "-map".into(), "0:V:0".into()]);
    // Copied: the sound as it is, whatever encoders this FFmpeg has (`video_copy` checked that
    // the container holds it).
    let audio = if copy { Some(&"copy") } else { c.audio.iter().find(|e| audio_encoders.iter().any(|a| a == *e)) }.filter(|_| v.audio);
    if audio.is_some() {
        a.extend(["-map".into(), "0:a:0?".into()]);
    }
    let filters: Vec<String> = match card {
        _ if copy => Vec::new(),
        Some(card) => card_filters(card, v.encoder, size, enlarge, turn),
        None => {
            let mut f: Vec<String> = turn.video_filters().into_iter().map(String::from).collect();
            f.extend(scale(size, enlarge));
            if vaapi {
                f.push("format=nv12,hwupload".into());
            }
            f
        }
    };
    if !filters.is_empty() {
        a.extend(["-vf".into(), filters.join(",")]);
    }
    a.extend(["-c:v".into(), v.encoder.into()]);
    a.extend(quality_args(v.encoder, v.quality));
    // 8 bits: a phone's 10-bit HDR video as H.264 would be "High 10", which almost nothing plays.
    // On the card `card_filters` does it.
    if matches!(v.encoder, "libx264" | "libopenh264" | "mpeg4") {
        a.extend(["-pix_fmt".into(), "yuv420p".into()]);
    }
    match audio {
        Some(_) if copy => a.extend(["-c:a".into(), "copy".into()]),
        Some(e) => a.extend(["-c:a".into(), e.to_string(), "-b:a".into(), ["192k", "128k", "96k"][v.quality as usize].into()]),
        None => a.push("-an".into()),
    }
    a.extend(["-sn".into(), "-dn".into(), "-map_metadata".into(), if v.meta { "0" } else { "-1" }.into()]);
    if v.meta {
        for (k, value) in tags {
            a.extend(["-metadata".into(), format!("{k}={value}")]);
        }
    }
    if matches!(c.muxer, "mp4" | "mov") {
        // MP4 writes only a few tags unless asked: the phone's place and model are among the others.
        a.extend(["-movflags".into(), if v.meta { "+faststart+use_metadata_tags" } else { "+faststart" }.into()]);
    }
    a.extend(["-f".into(), c.muxer.into(), output.into()]);
    a
}

pub(crate) fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    c.stdin(Stdio::null());
    // No console window flashing up for every video.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// The oldest FFmpeg EyeCanSee works with. `force_divisible_by` of the `scale` filter (`scale`
/// above) arrived in FFmpeg 4.3 (June 2020); `-progress`, `-map_metadata`, `-movflags
/// use_metadata_tags` and the encoders' options are older. Encoders a build lacks are found out
/// by the probe (SVT-AV1 came with 4.4, the graphics card ones depend on the build), not by the
/// version. Oldest one tried: 4.4 of Ubuntu 22.04, in a container (2026-10-27).
pub const MIN_VERSION: (u32, u32) = (4, 3);
pub const MIN_TEXT: &str = "4.3";

/// The system's `ffmpeg`: where, its version (`None` for a git build without a number, taken as
/// recent), the version as its first line prints it and its video decoders (`None` when the list
/// could not be read).
struct System {
    path: PathBuf,
    version: Option<(u32, u32)>,
    text: String,
    decoders: Option<Vec<String>>,
}

impl System {
    /// Whether "Automatic" takes it: recent enough and able to read the phones' videos (issue #67:
    /// the ffmpeg-free of Fedora has no H.264 and HEVC decoders, so their conversion fails).
    fn fit(&self) -> crate::engine::Fit {
        crate::engine::fit(self.version.is_none_or(|v| v >= MIN_VERSION), self.decoders.as_deref())
    }
}

/// Looked for once: in `PATH`, then `ffmpeg -decoders`, which prints the version first (on the
/// error output, as `-version` does on the normal one) and then the decoders: one run, as before.
fn system() -> Option<&'static System> {
    static SYSTEM: OnceLock<Option<System>> = OnceLock::new();
    SYSTEM
        .get_or_init(|| {
            let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
            let path = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))?;
            let out = command(&path).arg("-decoders").output().ok()?;
            let text = version_text(&String::from_utf8_lossy(&out.stderr));
            let list = String::from_utf8_lossy(&out.stdout);
            let decoders = list.contains("------").then(|| parse_list(&list, 'V'));
            log!("ffmpeg {path:?}: version {text:?}, without {:?}", decoders.as_deref().map(crate::engine::lacking));
            Some(System { path, version: parse_version(&text), text, decoders })
        })
        .as_ref()
}

/// "ffmpeg version 8.1.3 Copyright …" → "8.1.3" (BtbN's builds: "n9.0-20261026", git: "N-121000-g…").
fn version_text(output: &str) -> String {
    output.lines().next().unwrap_or_default().split_whitespace().nth(2).unwrap_or_default().to_string()
}

/// Major and minor of "8.1.3", "n9.0-20261026", "7.1"; `None` for a git build ("N-121000-g…").
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.trim_start_matches('n').split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

fn downloaded() -> Option<PathBuf> {
    crate::component::FFMPEG.as_ref().and_then(crate::component::path).filter(|p| p.is_file())
}

/// The `ffmpeg` to use: the system's or the downloaded component, by the option (`engine.rs`).
pub fn program() -> Option<PathBuf> {
    let fits = system().map(|s| s.fit() == crate::engine::Fit::Yes);
    match crate::engine::pick(crate::engine::choice(), fits, downloaded().is_some())? {
        crate::engine::Source::System => system().map(|s| s.path.clone()),
        crate::engine::Source::Downloaded => downloaded(),
    }
}

/// The `ffmpeg` for the jobs that copy the streams as they are, decoding and encoding nothing
/// ("Same as the original", `audio_job`): `program`, else with "Automatic" the system's that only
/// lacks the phones' decoders (`engine::pick_copy`).
pub fn copier() -> Option<PathBuf> {
    let fit = system().map(System::fit);
    match crate::engine::pick_copy(crate::engine::choice(), fit.as_ref(), downloaded().is_some())? {
        crate::engine::Source::System => system().map(|s| s.path.clone()),
        crate::engine::Source::Downloaded => downloaded(),
    }
}

/// For the conversion page and the Options.
pub fn status() -> crate::engine::Status {
    let version = || crate::component::FFMPEG.map(|c| c.version.to_string()).unwrap_or_default();
    // Kept by "Automatic" after the probe (`start_probe`).
    if probed().is_some_and(|p| Some(&p.program) == downloaded().as_ref()) {
        return crate::engine::Status::Ready(crate::engine::Source::Downloaded, version());
    }
    let found = system().map(|s| (s.fit(), s.text.clone()));
    crate::engine::status(crate::engine::choice(), found, downloaded().is_some(), MIN_TEXT, |src| match src {
        crate::engine::Source::System => system().map(|s| s.text.clone()).unwrap_or_default(),
        crate::engine::Source::Downloaded => version(),
    })
}

/// What FFmpeg can do on this computer.
#[derive(Debug, Default)]
pub struct Probe {
    pub program: PathBuf,
    /// Video encoders that made a file, in the order of `Codec::encoders`.
    pub video: Vec<&'static str>,
    /// Those of `video` whose card also turned and resized the frames itself (`args`' `on_card`).
    pub card: Vec<&'static str>,
    pub audio: Vec<String>,
}

impl Probe {
    /// A card encoder works here but the card cannot also read, turn and resize: the system's
    /// FFmpeg of Fedora 44 (8.1.3) has no `transpose_cuda` (2026-11-09).
    pub fn card_short(&self) -> bool {
        self.video.iter().any(|e| Card::of(e).is_some() && !self.card.contains(e))
    }

    /// Encoders of `codec` that work here (`copy` always: nothing is encoded).
    pub fn of(&self, codec: Codec) -> Vec<&'static str> {
        codec.encoders().iter().copied().filter(|e| *e == "copy" || self.video.contains(e)).collect()
    }
}

static PROBE: OnceLock<Probe> = OnceLock::new();
static PROBING: AtomicBool = AtomicBool::new(false);

/// The result of `start_probe`, once there.
pub fn probed() -> Option<&'static Probe> {
    PROBE.get()
}

static COPY_PROBE: OnceLock<Probe> = OnceLock::new();
static COPY_PROBING: AtomicBool = AtomicBool::new(false);

/// What copies the streams ("Same as the original"): the probe of `start_probe` when there is
/// one, else that of `start_copy_probe`.
pub fn copy_probed() -> Option<&'static Probe> {
    probed().or_else(|| COPY_PROBE.get())
}

/// With no `program` but a `copier`: its audio encoders read in the background (`args` keeps the
/// sound by them), no video encoder tried; `done` when ready.
pub fn start_copy_probe(done: impl Fn() + Send + 'static) {
    if COPY_PROBE.get().is_some() || COPY_PROBING.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(program) = copier() else {
        COPY_PROBING.store(false, Ordering::Relaxed);
        return;
    };
    std::thread::spawn(move || {
        let _ = COPY_PROBE.set(Probe { audio: audio_encoders(&program), program, ..Default::default() });
        done();
    });
}

/// Tries every encoder in the background (a second or two, once per session); `done` when ready.
/// Listed is not enough: NVENC is listed without an NVIDIA card. Nothing without an FFmpeg.
pub fn start_probe(done: impl Fn() + Send + 'static) {
    if PROBE.get().is_some() || PROBING.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(program) = program() else {
        PROBING.store(false, Ordering::Relaxed);
        return;
    };
    std::thread::spawn(move || {
        let mut p = probe(program);
        // "Automatic": the downloaded one too, when the system's leaves work to the processor
        // that the card could do; the one that does more on the card is kept.
        let automatic = crate::engine::choice() == crate::settings::EngineChoice::SystemIfRecent;
        if automatic && p.card_short()
            && let Some(other) = downloaded().filter(|d| *d != p.program).map(probe)
            && other.card.len() > p.card.len()
        {
            p = other;
        }
        let _ = PROBE.set(p);
        done();
    });
}

/// Every encoder of `program` tried, then the card ones with the card doing it all.
fn probe(program: PathBuf) -> Probe {
    let all: Vec<&'static str> = CONTAINERS.iter().flat_map(|c| c.codecs).flat_map(|c| c.encoders()).copied().collect();
    let mut wanted: Vec<&'static str> = Vec::new();
    for e in all {
        if !wanted.contains(&e) {
            wanted.push(e);
        }
    }
    let works: Vec<bool> = std::thread::scope(|s| {
        let jobs: Vec<_> = wanted.iter().map(|e| s.spawn(|| try_encoder(&program, e))).collect();
        jobs.into_iter().map(|j| j.join().unwrap_or(false)).collect()
    });
    let video: Vec<&'static str> = wanted.iter().zip(works).filter(|(_, ok)| *ok).map(|(e, _)| *e).collect();
    let on_card: Vec<&'static str> = video.iter().copied().filter(|e| Card::of(e).is_some()).collect();
    let card_works: Vec<bool> = std::thread::scope(|s| {
        let jobs: Vec<_> = on_card.iter().map(|e| s.spawn(|| try_card(&program, e))).collect();
        jobs.into_iter().map(|j| j.join().unwrap_or(false)).collect()
    });
    let card = on_card.into_iter().zip(card_works).filter(|(_, ok)| *ok).map(|(e, _)| e).collect();
    let probe = Probe { audio: audio_encoders(&program), program, video, card };
    log!("ffmpeg {:?}: {:?}, on the card {:?}", probe.program, probe.video, probe.card);
    probe
}

/// "Automatic", the system's FFmpeg in use and leaving card work to the processor, nothing
/// downloaded (or `downloading` now): the conversion page offers the download, used from the next
/// start (the probe is done once).
pub fn download_better(downloading: bool) -> bool {
    let system = probed().is_some_and(|p| p.card_short() && Some(&p.program) != downloaded().as_ref());
    crate::engine::choice() == crate::settings::EngineChoice::SystemIfRecent && system && (downloading || downloaded().is_none())
}

/// Ten black frames through `encoder`, written nowhere.
// ponytail: no time limit; a driver that hangs would keep the list "checking"
fn try_encoder(program: &Path, encoder: &str) -> bool {
    let mut c = command(program);
    c.args(["-hide_banner", "-loglevel", "error"]);
    let vaapi = encoder.ends_with("_vaapi");
    if vaapi {
        c.args(["-vaapi_device", VAAPI_DEVICE]);
    }
    c.args(["-f", "lavfi", "-i", "color=c=black:s=256x144:r=25:d=0.4"]);
    if vaapi {
        c.args(["-vf", "format=nv12,hwupload"]);
    }
    // With the options of a real conversion: vp9_qsv encoded the probe and refused them.
    c.args(["-c:v", encoder]).args(quality_args(encoder, Quality::Medium)).args(["-f", "null", "-"]);
    c.stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Ten black frames put on `encoder`'s card, turned and halved there, then encoded: whether a
/// conversion can stay on the card. Reading a real video there is tried by the conversion, which
/// falls back on the processor (`batch::video`).
fn try_card(program: &Path, encoder: &str) -> bool {
    let Some(card) = Card::of(encoder) else { return false };
    let ok = |c: &mut Command| c.stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    let filters = card_filters(card, encoder, Size::Percent(50), false, Turn::default().then(crate::transform::Op::Cw)).join(",");
    // Not smaller: turned and halved it must stay 160 px wide or more, the least NVENC takes.
    let black = ["-f", "lavfi", "-i", "color=c=black:s=640x360:r=25:d=0.4"];
    let clip = std::env::temp_dir().join(format!("eyecansee-card-{}-{encoder}.mkv", std::process::id()));
    let mut c = command(program);
    c.args(["-hide_banner", "-loglevel", "error"]);
    match card {
        Card::Cuda => c.args(["-init_hw_device", "cuda=gpu", "-filter_hw_device", "gpu"]).args(black).args(["-vf", &format!("format=nv12,hwupload,{filters}")]),
        Card::Vaapi => c.args(["-vaapi_device", VAAPI_DEVICE]).args(black).args(["-vf", &format!("format=nv12,hwupload,{filters}")]),
        // Frames uploaded to QSV fail on Windows ("Could not create the texture", Intel UHD,
        // 2026-11-08): a clip of this encoder read there instead, as a conversion does.
        Card::Qsv => {
            let mut make = command(program);
            make.args(["-hide_banner", "-loglevel", "error", "-y"]).args(black).args(["-c:v", encoder]).args(quality_args(encoder, Quality::Medium)).arg(&clip);
            if !ok(&mut make) {
                let _ = std::fs::remove_file(&clip);
                return false;
            }
            c.args(["-hwaccel", "qsv", "-hwaccel_output_format", "qsv", "-i"]).arg(&clip).args(["-vf", &filters])
        }
    };
    c.args(["-c:v", encoder]).args(quality_args(encoder, Quality::Medium)).args(["-f", "null", "-"]);
    let works = ok(&mut c);
    let _ = std::fs::remove_file(&clip);
    works
}

/// Audio encoders of `ffmpeg -encoders` (lines " A..... name  description").
fn audio_encoders(program: &Path) -> Vec<String> {
    let Ok(out) = command(program).args(["-hide_banner", "-encoders"]).output() else { return Vec::new() };
    parse_list(&String::from_utf8_lossy(&out.stdout), 'A')
}

/// The names of `kind` ('A' audio, 'V' video) in the list of `ffmpeg -encoders` or `-decoders`.
fn parse_list(list: &str, kind: char) -> Vec<String> {
    // The legend above the dashes has lines like " A..... = Audio".
    let codecs = list.lines().skip_while(|l| !l.trim_start().starts_with("---")).skip(1);
    codecs.filter(|l| l.trim_start().starts_with(kind)).filter_map(|l| l.split_whitespace().nth(1)).map(String::from).collect()
}

/// Runs ffmpeg with `args`; `progress` gets 0 to 1. `cancel` stops it: the caller removes what
/// was written. The error is ffmpeg's last lines.
pub fn run(program: &Path, args: &[String], progress: impl Fn(f32), cancel: &AtomicBool) -> Result<(), String> {
    let mut child = command(program).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    // stderr: the length of the input ("Duration: 00:01:02.03"), and the last lines for an error.
    let length = Arc::new(Mutex::new(None::<f64>));
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));
    let (l, t, err) = (length.clone(), tail.clone(), child.stderr.take().expect("piped"));
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if let Some(d) = line.trim().strip_prefix("Duration: ").and_then(|d| clock(d.split(',').next()?)) {
                l.lock().unwrap().get_or_insert(d);
            }
            let mut t = t.lock().unwrap();
            t.push(line);
            if t.len() > 8 {
                t.remove(0);
            }
        }
    });
    for line in BufReader::new(child.stdout.take().expect("piped")).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }
        if let (Some(us), Some(total)) = (line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()), *length.lock().unwrap()) {
            progress((us / 1e6 / total).clamp(0.0, 1.0) as f32);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    progress(1.0);
    let _ = reader.join();
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if status.success() { Ok(()) } else { Err(tail.lock().unwrap().join("\n")) }
}

/// Seconds of "01:02:03.45".
fn clock(s: &str) -> Option<f64> {
    let mut parts = s.trim().split(':');
    let (h, m, sec) = (parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?);
    Some(h * 3600.0 + m * 60.0 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Op;

    #[test]
    fn versions() {
        assert_eq!(version_text("ffmpeg version 8.1.3 Copyright (c) 2000-2026 the FFmpeg developers\nbuilt with gcc"), "8.1.3");
        assert_eq!(parse_version("8.1.3"), Some((8, 1)));
        assert_eq!(parse_version("n9.0-20261026"), Some((9, 0)));
        assert_eq!(parse_version("7.1"), Some((7, 1)));
        assert_eq!(parse_version("4"), Some((4, 0)));
        assert_eq!(parse_version("N-121000-g1234567"), None, "a git build: taken as recent");
        assert_eq!(parse_version(""), None);
        assert!(Some((4, 4)) >= Some(MIN_VERSION) && (4, 2) < MIN_VERSION);
    }

    #[test]
    fn arguments() {
        let v = Video { container: 0, encoder: "libx264", quality: Quality::Medium, audio: true, meta: true };
        let turn = Turn::default().then(Op::FlipH).then(Op::Cw);
        let a = args(&v, Size::Fit(1920, 1080), false, turn, false, ("in.mov", "out.mp4"), &["aac".into()], &[]).join(" ");
        assert!(a.contains("-i in.mov -map 0:V:0 -map 0:a:0?"), "{a}");
        assert!(a.contains("-vf hflip,transpose=clock,scale=w='min(1920,iw)':h='min(1080,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"), "{a}");
        assert!(a.contains("-c:v libx264 -crf 23 -preset medium -pix_fmt yuv420p -c:a aac -b:a 128k"), "{a}");
        assert!(a.ends_with("-map_metadata 0 -movflags +faststart+use_metadata_tags -f mp4 out.mp4"), "{a}");
        // No audio encoder for WebM here: no sound. No metadata, no filters.
        let v = Video { container: 2, encoder: "libsvtav1", quality: Quality::High, audio: true, meta: false };
        let a = args(&v, Size::Same, false, Turn::default(), false, ("a", "b"), &["aac".into()], &[("location".to_string(), "+42.5+014.1/".to_string())]).join(" ");
        assert!(a.contains("-c:v libsvtav1 -crf 24 -preset 8 -an -sn -dn -map_metadata -1 -f webm b"), "{a}");
        assert!(!a.contains("-vf") && !a.contains("-metadata"), "{a}"); // no tags without meta
        let v = Video { meta: true, ..v };
        let a = args(&v, Size::Same, false, Turn::default(), false, ("a", "b"), &[], &[("location".to_string(), "+42.5+014.1/".to_string())]).join(" ");
        assert!(a.contains("-map_metadata 0 -metadata location=+42.5+014.1/ -f webm b"), "{a}");
        // Copy: no filters, no quality, the sound copied too.
        let v = Video { container: 0, encoder: "copy", quality: Quality::Small, audio: true, meta: true };
        let a = args(&v, Size::Percent(50), false, Turn::default().then(Op::Cw), false, ("a", "b"), &["aac".into()], &[]).join(" ");
        assert!(a.contains("-fflags +genpts -i a -map 0:V:0 -map 0:a:0? -c:v copy -c:a copy -sn -dn -map_metadata 0"), "{a}");
        assert!(!a.contains("-vf"), "{a}");
        // Copied into WebM: the sound kept without an Opus or Vorbis encoder here.
        let v = Video { container: 2, ..v };
        let a = args(&v, Size::Same, false, Turn::default(), false, ("a", "b"), &[], &[]).join(" ");
        assert!(a.contains("-c:v copy -c:a copy -sn -dn -map_metadata 0 -f webm b"), "{a}");
        // VA-API: the device, and frames uploaded to the card.
        let v = Video { container: 1, encoder: "hevc_vaapi", quality: Quality::Small, audio: false, meta: true };
        let a = args(&v, Size::Percent(50), false, Turn::default(), false, ("a", "b"), &[], &[]).join(" ");
        assert!(a.contains("-vaapi_device /dev/dri/renderD128 -i a"), "{a}");
        assert!(a.contains("-vf scale=w='trunc(iw*50/200)*2':h=-2,format=nv12,hwupload -c:v hevc_vaapi -rc_mode CQP -global_quality:v 30 -an"), "{a}");
        // On the card: read there, turned and resized there; H.264 brought to 8 bits there.
        let v = Video { container: 0, encoder: "h264_nvenc", quality: Quality::Medium, audio: false, meta: false };
        let a = args(&v, Size::Fit(1920, 1080), false, Turn::default().then(Op::Cw), true, ("a", "b"), &[], &[]).join(" ");
        assert!(a.contains("-hwaccel cuda -hwaccel_output_format cuda -i a"), "{a}");
        assert!(a.contains("-vf transpose_cuda=dir=clock,scale_cuda=w='min(1920,iw)':h='min(1080,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2:format=nv12 -c:v h264_nvenc"), "{a}");
        let v = Video { container: 1, encoder: "hevc_vaapi", quality: Quality::Small, audio: false, meta: true };
        let a = args(&v, Size::Same, false, Turn::default().then(Op::FlipH), true, ("a", "b"), &[], &[]).join(" ");
        assert!(a.contains("-vaapi_device /dev/dri/renderD128 -hwaccel vaapi -hwaccel_output_format vaapi -i a"), "{a}");
        assert!(a.contains("-vf transpose_vaapi=dir=hflip -c:v hevc_vaapi"), "{a}");
        // Quick Sync: its own turn filter and flip names, sizes worked out by hand.
        let v = Video { container: 0, encoder: "h264_qsv", quality: Quality::Medium, audio: false, meta: false };
        let a = args(&v, Size::Fit(1920, 1080), false, Turn::default().then(Op::FlipH).then(Op::Cw), true, ("a", "b"), &[], &[]).join(" ");
        assert!(a.contains("-hwaccel qsv -hwaccel_output_format qsv -i a"), "{a}");
        assert!(a.contains("-vf vpp_qsv=transpose=cclock_hflip,scale_qsv=w='round(iw*min(1,min(1920/iw,1080/ih))/2)*2':h='round(ih*min(1,min(1920/iw,1080/ih))/2)*2':format=nv12 -c:v h264_qsv"), "{a}");
        let v = Video { encoder: "hevc_qsv", ..v };
        let a = args(&v, Size::Percent(50), false, Turn::default(), true, ("a", "b"), &[], &[]).join(" ");
        assert!(a.contains("-vf scale_qsv=w='round(iw*50/100/2)*2':h='round(ih*50/100/2)*2' -c:v hevc_qsv"), "{a}");
        // A processor's encoder stays on the processor even if asked.
        let v = Video { container: 0, encoder: "libx264", quality: Quality::Medium, audio: false, meta: false };
        assert!(!args(&v, Size::Same, false, Turn::default(), true, ("a", "b"), &[], &[]).join(" ").contains("-hwaccel"));
    }

    /// Every flip and turn done by the card's one transposition gives the same picture as the
    /// processor's filters, on a 2×3 picture whose pixels are all different.
    #[test]
    fn card_turns_match() {
        type Pic = Vec<Vec<u8>>;
        let cw = |p: &Pic| -> Pic { (0..p[0].len()).map(|x| (0..p.len()).rev().map(|y| p[y][x]).collect()).collect() };
        let ccw = |p: &Pic| -> Pic { (0..p[0].len()).rev().map(|x| (0..p.len()).map(|y| p[y][x]).collect()).collect() };
        let hflip = |p: &Pic| -> Pic { p.iter().map(|r| r.iter().rev().copied().collect()).collect() };
        let vflip = |p: &Pic| -> Pic { p.iter().rev().cloned().collect() };
        let cpu = |f: &str, p: &Pic| match f {
            "hflip" => hflip(p),
            "vflip" => vflip(p),
            "transpose=clock" => cw(p),
            "transpose=cclock" => ccw(p),
            _ => panic!("{f}"),
        };
        // FFmpeg's `transpose` directions: the `_flip` ones flip vertically after the turn.
        let card = |d: Option<&str>, p: &Pic| match d {
            None => p.clone(),
            Some("clock") => cw(p),
            Some("cclock") => ccw(p),
            Some("reversal") => vflip(&hflip(p)),
            Some("hflip") => hflip(p),
            Some("vflip") => vflip(p),
            Some("clock_flip") => vflip(&cw(p)),
            Some("cclock_flip") => vflip(&ccw(p)),
            Some(d) => panic!("{d}"),
        };
        let pic: Pic = vec![vec![1, 2, 3], vec![4, 5, 6]];
        for flip in [false, true] {
            for q in 0..4 {
                let mut turn = if flip { Turn::default().then(Op::FlipH) } else { Turn::default() };
                for _ in 0..q {
                    turn = turn.then(Op::Cw);
                }
                let by_cpu = turn.video_filters().iter().fold(pic.clone(), |p, f| cpu(f, &p));
                assert_eq!(card(card_turn(turn), &pic), by_cpu, "{turn:?}");
            }
        }
    }

    #[test]
    fn sizes_and_lists() {
        assert_eq!(scale(Size::Percent(150), false), None);
        assert_eq!(scale(Size::Fit(640, 480), true).unwrap(), "scale=w=640:h=480:force_original_aspect_ratio=decrease:force_divisible_by=2");
        assert_eq!(clock("00:01:02.50"), Some(62.5));
        let list = "Encoders:\n A..... = Audio\n ------\n V....D libx264  libx264 H.264\n A....D aac   AAC (Advanced Audio Coding)\n A....D libopus  Opus\n";
        assert_eq!(parse_list(list, 'A'), ["aac", "libopus"]);
        // `ffmpeg -decoders` of Fedora's ffmpeg-free (shortened): H.264 only through openh264.
        let decoders = "Decoders:\n V..... = Video\n ------\n VFS..D vp9   Google VP9\n V....D libopenh264  OpenH264 H.264\n A....D aac  AAC\n";
        let fedora = parse_list(decoders, 'V');
        assert_eq!(fedora, ["vp9", "libopenh264"]);
        assert_eq!(crate::engine::lacking(&fedora), ["H.264", "HEVC"]);
        assert_eq!(maker("h264_nvenc"), "NVIDIA");
        assert_eq!(maker("libx264"), "CPU");
        assert_eq!(hint("hevc_qsv"), "video-enc-qsv");
        assert_eq!(hint("av1_vaapi"), "video-enc-vaapi");
        assert_eq!(quality_args("h264_amf", Quality::High).join(" "), "-rc cqp -qp_i 20 -qp_p 20 -qp_b 20");
        // Every codec of a container has an encoder.
        assert!(CONTAINERS.iter().all(|c| c.codecs.iter().all(|k| !k.encoders().is_empty())));
    }

    /// Short when a card encoder works but not with the card doing it all.
    #[test]
    fn card_short() {
        let p = |video: Vec<&'static str>, card: Vec<&'static str>| Probe { video, card, ..Default::default() };
        assert!(p(vec!["h264_nvenc", "h264_qsv", "libx264"], vec!["h264_qsv"]).card_short());
        assert!(!p(vec!["h264_nvenc", "h264_qsv", "libx264"], vec!["h264_nvenc", "h264_qsv"]).card_short());
        assert!(!p(vec!["libx264", "mpeg4"], vec![]).card_short());
    }

    /// `cargo test ffmpeg_matrix_live -- --ignored --nocapture`: every container, every encoder
    /// that works here, every quality, with and without sound, on a one-second clip made by FFmpeg
    /// itself: each run must make a file. Run on every system (Linux and Windows, system and
    /// downloaded FFmpeg) after a change to `args` or `quality_args`: a bare `-global_quality`
    /// passed the probe and broke libopus in MKV on Windows (QSV, 2026-10-29).
    #[test]
    #[ignore]
    fn ffmpeg_matrix_live() {
        // The downloaded FFmpeg instead of the system's (`XDG_DATA_HOME`/`LOCALAPPDATA` where it is).
        if std::env::var_os("EYECANSEE_FFMPEG_DOWNLOADED").is_some() {
            crate::engine::set_choice(crate::settings::EngineChoice::Downloaded);
        }
        start_probe(|| {});
        while probed().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let p = probed().unwrap();
        println!("{:?}\nvideo: {:?}\non the card: {:?}\naudio: {:?}", p.program, p.video, p.card, p.audio);
        let dir = std::env::temp_dir().join(format!("eyecansee-ffmpeg-matrix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.mp4");
        // H.264 as a phone's, which the cards read (MPEG-4 Part 2 they mostly do not).
        let h264 = ["libx264", "libopenh264"].into_iter().find(|e| p.video.contains(e)).unwrap_or("mpeg4");
        let clip = ["-hide_banner", "-nostdin", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x480:rate=30:duration=1", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:v", h264, "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"].map(String::from);
        run(&p.program, &[clip.to_vec(), vec![input.to_string_lossy().into()]].concat(), |_| {}, &AtomicBool::new(false)).unwrap();
        let (mut runs, mut failed) = (0, Vec::new());
        for (container, c) in CONTAINERS.iter().enumerate() {
            // The copy: by `copy_matrix`, from sources it can hold.
            for encoder in c.codecs.iter().flat_map(|&codec| p.of(codec)).filter(|e| *e != "copy") {
                for quality in [Quality::High, Quality::Medium, Quality::Small] {
                    // Both ways where the card can do it all (`Probe::card`).
                    for (audio, on_card) in [(true, false), (false, false), (true, true), (false, true)] {
                        if on_card && !p.card.contains(&encoder) {
                            continue;
                        }
                        let out = dir.join(format!("out.{}", c.ext));
                        let v = Video { container, encoder, quality, audio, meta: true };
                        // Not smaller: NVENC refuses frames under 160 px wide (RTX 4070).
                        let a = args(&v, Size::Fit(480, 360), false, Turn::default().then(Op::Cw), on_card, (&input.to_string_lossy(), &out.to_string_lossy()), &p.audio, &[("location".into(), "+45.0+009.0/".into())]);
                        runs += 1;
                        let r = run(&p.program, &a, |_| {}, &AtomicBool::new(false));
                        let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                        let _ = std::fs::remove_file(&out);
                        match r {
                            Ok(()) if size > 0 => {}
                            Ok(()) => failed.push(format!("{} {encoder} {quality:?} audio={audio} card={on_card}: empty file", c.name)),
                            Err(e) => failed.push(format!("{} {encoder} {quality:?} audio={audio} card={on_card}: {}", c.name, e.lines().rev().find(|l| !l.starts_with("frame=") && !l.starts_with("Conversion failed")).unwrap_or_default())),
                        }
                    }
                }
            }
        }
        copy_matrix(p, &dir, &mut runs, &mut failed);
        std::fs::remove_dir_all(&dir).unwrap();
        println!("{runs} runs, {} failed", failed.len());
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// The copy ("Same as the original"): clips of the usual codecs of every container, each
    /// copied into every container that takes a copy, with and without sound, through the
    /// container `video_copy` picks for it (the one chosen, or MKV): each run must make a file.
    fn copy_matrix(p: &Probe, dir: &Path, runs: &mut usize, failed: &mut Vec<String>) {
        let has = |e: &str| p.video.contains(&e) || p.audio.iter().any(|a| a == e);
        // Name, video encoder and its options, audio encoder. MPEG-4 with B-frames: an AVI has no times for them.
        let clips: [(&str, &[&str], &str); 6] = [
            ("h264.mp4", &["libx264", "-pix_fmt", "yuv420p"], "aac"),
            ("hevc.mov", &["libx265", "-pix_fmt", "yuv420p", "-tag:v", "hvc1"], "aac"),
            ("vp8.webm", &["libvpx"], "libvorbis"),
            ("vp9.webm", &["libvpx-vp9"], "libopus"),
            ("mpeg4.avi", &["mpeg4", "-bf", "2"], "libmp3lame"),
            ("prores.mov", &["prores_ks"], "pcm_s16le"),
        ];
        for (name, video, audio) in clips {
            if !has(video[0]) || !(has(audio) || audio.starts_with("pcm")) {
                println!("copy from {name}: skipped, no {} or {audio} here", video[0]);
                continue;
            }
            let input = dir.join(format!("copy-{name}"));
            let make = ["-hide_banner", "-nostdin", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x480:rate=30:duration=1", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:v"];
            let a: Vec<String> = make.iter().chain(video).chain(&["-c:a", audio, "-shortest"]).map(|s| s.to_string()).chain([input.to_string_lossy().into()]).collect();
            run(&p.program, &a, |_| {}, &AtomicBool::new(false)).unwrap();
            for (chosen, c) in CONTAINERS.iter().enumerate().filter(|(_, c)| c.codecs.contains(&Codec::Copy)) {
                let container = crate::video_copy::for_file(chosen, name, &p.program, &input);
                for audio in [true, false] {
                    let out = dir.join(format!("copy-out.{}", CONTAINERS[container].ext));
                    let v = Video { container, encoder: "copy", quality: Quality::Medium, audio, meta: true };
                    let a = args(&v, Size::Same, false, Turn::default(), false, (&input.to_string_lossy(), &out.to_string_lossy()), &p.audio, &[]);
                    *runs += 1;
                    let r = run(&p.program, &a, |_| {}, &AtomicBool::new(false));
                    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                    let _ = std::fs::remove_file(&out);
                    let what = format!("copy {name} into {} (as {}) audio={audio}", c.name, CONTAINERS[container].name);
                    match r {
                        Ok(()) if size > 0 => println!("{what}: ok"),
                        Ok(()) => failed.push(format!("{what}: empty file")),
                        Err(e) => failed.push(format!("{what}: {}", e.lines().rev().find(|l| !l.starts_with("Conversion failed")).unwrap_or_default())),
                    }
                }
            }
        }
    }

    /// The system's FFmpeg and what "Automatic" makes of it, on this computer:
    /// `cargo test system_ffmpeg_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn system_ffmpeg_live() {
        let s = system().expect("no ffmpeg in PATH");
        println!("{:?} {} decoders {:?}: {:?}", s.path, s.text, s.decoders.as_ref().map(Vec::len), s.fit());
        assert!(s.decoders.is_some());
    }

    /// `cargo test ffmpeg_live -- --ignored --nocapture`: every encoder of the system's FFmpeg,
    /// then a real conversion of `EYECANSEE_VIDEO` with the first H.264 encoder that works.
    #[test]
    #[ignore]
    fn ffmpeg_live() {
        start_probe(|| {});
        while probed().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let p = probed().unwrap();
        println!("{:?}\nvideo: {:?}\naudio: {:?}", p.program, p.video, p.audio);
        let Ok(input) = std::env::var("EYECANSEE_VIDEO") else { return };
        let encoder = p.of(Codec::H264)[0];
        let out = std::env::temp_dir().join(format!("eyecansee-ffmpeg-live-{}.mp4", std::process::id()));
        let v = Video { container: 0, encoder, quality: Quality::Medium, audio: true, meta: true };
        let a = args(&v, Size::Fit(1280, 720), false, Turn::default().then(Op::Cw), false, (&input, &out.to_string_lossy()), &p.audio, &[]);
        let t = std::time::Instant::now();
        let last = Mutex::new(0.0);
        run(&p.program, &a, |f| *last.lock().unwrap() = f, &AtomicBool::new(false)).unwrap();
        println!("{encoder}: {} bytes in {:?}, last progress {}", std::fs::metadata(&out).unwrap().len(), t.elapsed(), last.lock().unwrap());
        std::fs::remove_file(out).unwrap();
    }
}
