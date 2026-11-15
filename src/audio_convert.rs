//! Converting audio files into another audio format with FFmpeg (Convert…, #120), without the
//! interface: the formats, the arguments of ffmpeg and one file's job for `batch::run`. The tags
//! are kept, and the cover where the format holds one.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::batch::{Phases, Progress, Writing, download, keep_thumbnail, upload};
use crate::ffmpeg::Quality;
use crate::source::{self, Store};

/// An audio file, and no FFmpeg to convert it.
pub const SKIP_AUDIO: &str = "batch-skip-audio";

pub struct Format {
    pub name: &'static str,
    pub ext: &'static str,
    /// FFmpeg's muxer.
    muxer: &'static str,
    pub encoder: &'static str,
    /// The cover (an attached picture) can go into the file.
    cover: bool,
    /// No quality to choose: nothing is lost.
    pub lossless: bool,
}

pub const FORMATS: &[Format] = &[
    Format { name: "MP3", ext: "mp3", muxer: "mp3", encoder: "libmp3lame", cover: true, lossless: false },
    Format { name: "AAC (M4A)", ext: "m4a", muxer: "ipod", encoder: "aac", cover: true, lossless: false },
    Format { name: "Opus", ext: "opus", muxer: "ogg", encoder: "libopus", cover: false, lossless: false },
    Format { name: "FLAC", ext: "flac", muxer: "flac", encoder: "flac", cover: true, lossless: true },
    Format { name: "WAV", ext: "wav", muxer: "wav", encoder: "pcm_s16le", cover: false, lossless: true },
    Format { name: "Ogg Vorbis", ext: "ogg", muxer: "ogg", encoder: "libvorbis", cover: false, lossless: false },
];

/// What every audio file becomes: a format of `FORMATS` and its quality.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Sound {
    pub format: usize,
    pub quality: Quality,
}

/// The formats (indexes in `FORMATS`) whose encoder this FFmpeg has (`Probe::audio`).
pub fn available(encoders: &[String]) -> Vec<usize> {
    (0..FORMATS.len()).filter(|&i| encoders.iter().any(|e| e == FORMATS[i].encoder)).collect()
}

/// Fluent id of the line under the format: what stays in the new file.
pub fn hint(format: usize) -> &'static str {
    if FORMATS[format].cover { "batch-sound-keeps-cover" } else { "batch-sound-keeps-tags" }
}

/// The encoder's options for a quality, from the best to the smallest file.
fn quality_args(encoder: &str, q: Quality) -> Vec<String> {
    let i = q as usize;
    match encoder {
        // VBR: V0, V2, V5 (about 245, 190 and 130 kbit/s).
        "libmp3lame" => vec!["-q:a".into(), ["0", "2", "5"][i].into()],
        "aac" => vec!["-b:a".into(), ["256k", "192k", "128k"][i].into()],
        "libopus" => vec!["-b:a".into(), ["160k", "128k", "96k"][i].into()],
        "libvorbis" => vec!["-q:a".into(), ["8", "6", "4"][i].into()],
        _ => Vec::new(),
    }
}

/// The arguments of ffmpeg for one audio file: its first audio track, the tags, and the cover
/// copied as it is when `cover` and the format holds one.
pub fn args(s: Sound, (input, output): (&str, &str), cover: bool) -> Vec<String> {
    let f = &FORMATS[s.format];
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-nostats", "-i", input, "-map", "0:a:0"].map(String::from).to_vec();
    if cover && f.cover {
        // `v`, not `V`: the attached picture is the only "video" of an audio file.
        a.extend(["-map", "0:v:0?", "-c:v", "copy", "-disposition:v:0", "attached_pic"].map(String::from));
    } else {
        a.push("-vn".into());
    }
    a.extend(["-c:a".into(), f.encoder.into()]);
    a.extend(quality_args(f.encoder, s.quality));
    a.extend(["-sn".into(), "-dn".into(), "-map_metadata".into(), "0".into()]);
    if f.muxer == "mp3" {
        // ID3v2.3: what Windows reads; FFmpeg writes 2.4 unless asked.
        a.extend(["-id3v2_version".into(), "3".into()]);
    }
    a.extend(["-f".into(), f.muxer.into(), output.into()]);
    a
}

/// The name of the converted file: the original's with the format's extension.
pub fn new_name(name: &str, s: Sound) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}.{}", FORMATS[s.format].ext)
}

/// One audio file of `batch::run`, converted into the folder `dest` of `out` with a name free in
/// `taken`, returned. As `batch::video`, a file on a phone or a server is copied into a temporary
/// file first, and the result goes there at the end.
pub async fn run(op: &Store, path: &str, (out, dest, taken): (&Store, &str, &HashSet<String>), s: Sound, p: &Arc<Progress>) -> Result<String, source::Error> {
    let name = source::name(path).to_string();
    let failed = |e: String| ("batch-failed", format!("{name}: {e}"));
    // Nothing of the phones' video decoders is needed: an FFmpeg without them does it too.
    let Some(probe) = crate::ffmpeg::copy_probed() else { return Err((SKIP_AUDIO, name)) };
    let temp = |what: &str| std::env::temp_dir().join(format!("eyecansee-{what}-{}-{name}", std::process::id()));
    let (remote_in, remote_out) = (op.local_file(path).is_none(), out.local_file(&format!("{dest}{name}")).is_none());
    let phases = Phases { p: p.clone(), count: 1 + remote_in as u32 + remote_out as u32 };
    let input = match op.local_file(path) {
        Some(file) => (file, false),
        None => {
            let file = temp("in");
            download(op, path, &file, || phases.p.cancel.load(Ordering::Relaxed), |f| phases.set(0, f)).await.map_err(|e| failed(e.to_string()))?;
            (file, true)
        }
    };
    let new_name = source::free_name(taken, &new_name(&name, s));
    let to = format!("{dest}{new_name}");
    let output = out.local_file(&to).map_or_else(|| (temp("out"), true), |f| (f, false));
    let _writing = Writing::start(output.0.clone());
    let mut r = Err(String::new());
    // With the cover, then without it: a cover the format cannot take (a BMP into M4A) does not
    // stop the conversion.
    for cover in [true, false] {
        if p.cancel.load(Ordering::Relaxed) || (!cover && !FORMATS[s.format].cover) {
            break;
        }
        let a = args(s, (&input.0.to_string_lossy(), &output.0.to_string_lossy()), cover);
        log!("ffmpeg {a:?}");
        let (program, ph, first) = (probe.program.clone(), phases.clone(), remote_in as u32);
        r = tokio::task::spawn_blocking(move || crate::ffmpeg::run(&program, &a, |f| ph.set(first, f), &ph.p.cancel)).await.unwrap_or_else(|e| Err(e.to_string()));
        if r.is_ok() {
            break;
        }
        log!("ffmpeg: failed ({}), again without the cover", r.as_ref().unwrap_err());
    }
    if input.1 {
        let _ = std::fs::remove_file(&input.0);
    }
    deliver(out, &to, &output, &phases, r.map_err(failed)).await?;
    Ok(new_name)
}

/// The converted file's thumbnail kept, the file sent to a phone or a server, the temporary
/// copy removed.
async fn deliver(out: &Store, to: &str, (file, remote): &(std::path::PathBuf, bool), phases: &Phases, r: Result<(), source::Error>) -> Result<(), source::Error> {
    if r.is_ok() {
        keep_thumbnail(to, file).await;
    }
    let r = match r {
        Ok(()) if *remote => {
            let (ph, last, p) = (phases.clone(), phases.count - 1, phases.p.clone());
            upload(out, to, file, move || p.cancel.load(Ordering::Relaxed), move |f| ph.set(last, f)).await.map_err(|e| ("batch-failed", format!("{}: {e}", source::name(to))))
        }
        r => r,
    };
    if *remote || r.is_err() {
        let _ = std::fs::remove_file(file);
    }
    r
}

/// `ffmpeg -i` of a file: its streams and tags, as FFmpeg prints them.
#[cfg(test)]
fn described(program: &std::path::Path, file: &std::path::Path) -> String {
    let out = crate::ffmpeg::command(program).args(["-hide_banner", "-i"]).arg(file).output().map(|o| String::from_utf8_lossy(&o.stderr).into_owned());
    out.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        let mp3 = Sound { format: 0, quality: Quality::Medium };
        let a = args(mp3, ("in.flac", "out.mp3"), true).join(" ");
        assert!(a.contains("-i in.flac -map 0:a:0 -map 0:v:0? -c:v copy -disposition:v:0 attached_pic -c:a libmp3lame -q:a 2"), "{a}");
        assert!(a.ends_with("-sn -dn -map_metadata 0 -id3v2_version 3 -f mp3 out.mp3"), "{a}");
        // Without the cover (the second try).
        let a = args(mp3, ("in.flac", "out.mp3"), false).join(" ");
        assert!(a.contains("-map 0:a:0 -vn -c:a libmp3lame"), "{a}");
        // Opus: no cover, the bitrate of the quality.
        let a = args(Sound { format: 2, quality: Quality::High }, ("a.m4a", "b.opus"), true).join(" ");
        assert!(a.contains("-map 0:a:0 -vn -c:a libopus -b:a 160k -sn -dn -map_metadata 0 -f ogg b.opus"), "{a}");
        // Lossless: no quality.
        let a = args(Sound { format: 3, quality: Quality::Small }, ("a.mp3", "b.flac"), true).join(" ");
        assert!(a.contains("-c:a flac -sn"), "{a}");
        let a = args(Sound { format: 1, quality: Quality::Small }, ("a.mp3", "b.m4a"), true).join(" ");
        assert!(a.contains("-c:a aac -b:a 128k") && a.ends_with("-f ipod b.m4a"), "{a}");
    }

    #[test]
    fn formats_and_names() {
        let names: Vec<&str> = FORMATS.iter().map(|f| f.ext).collect();
        assert_eq!(names, ["mp3", "m4a", "opus", "flac", "wav", "ogg"]);
        assert_eq!(available(&["aac".into(), "flac".into(), "pcm_s16le".into()]), [1, 3, 4]);
        assert_eq!(new_name("song.flac", Sound { format: 0, quality: Quality::High }), "song.mp3");
        assert_eq!(new_name("no ext", Sound { format: 4, quality: Quality::High }), "no ext.wav");
        assert_eq!((hint(0), hint(2)), ("batch-sound-keeps-cover", "batch-sound-keeps-tags"));
    }

    /// `cargo test ffmpeg_matrix_live -- --ignored --nocapture` runs this too: every audio format
    /// this FFmpeg can write, every quality, from an MP3 and a FLAC with tags and a cover: each
    /// run must make a file with the title, and with the cover where the format holds one.
    #[test]
    #[ignore]
    fn ffmpeg_matrix_live() {
        if std::env::var_os("EYECANSEE_FFMPEG_DOWNLOADED").is_some() {
            crate::engine::set_choice(crate::settings::EngineChoice::Downloaded);
        }
        crate::ffmpeg::start_copy_probe(|| {});
        while crate::ffmpeg::copy_probed().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let p = crate::ffmpeg::copy_probed().unwrap();
        println!("{:?}\naudio formats: {:?}", p.program, available(&p.audio).iter().map(|&i| FORMATS[i].name).collect::<Vec<_>>());
        let dir = std::env::temp_dir().join(format!("eyecansee-audio-matrix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let (mut runs, mut failed) = (0, Vec::new());
        for (source, encoder) in [("in.mp3", "libmp3lame"), ("in.flac", "flac")] {
            let input = dir.join(source);
            let make = ["-hide_banner", "-nostdin", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-f", "lavfi", "-i", "color=c=red:s=64x64:d=0.04", "-map", "0:a", "-map", "1:v", "-c:v", "mjpeg", "-disposition:v", "attached_pic", "-c:a", encoder, "-metadata", "title=EyeCanSee test", "-metadata", "artist=Nobody"];
            let a: Vec<String> = make.iter().map(|s| s.to_string()).chain([input.to_string_lossy().into()]).collect();
            crate::ffmpeg::run(&p.program, &a, |_| {}, &cancel).unwrap();
            for format in available(&p.audio) {
                for quality in [Quality::High, Quality::Medium, Quality::Small] {
                    let s = Sound { format, quality };
                    let out = dir.join(format!("out.{}", FORMATS[format].ext));
                    runs += 1;
                    let r = crate::ffmpeg::run(&p.program, &args(s, (&input.to_string_lossy(), &out.to_string_lossy()), true), |_| {}, &cancel);
                    let text = described(&p.program, &out).to_lowercase();
                    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                    let _ = std::fs::remove_file(&out);
                    let what = format!("{source} into {} {quality:?}", FORMATS[format].name);
                    match r {
                        Err(e) => failed.push(format!("{what}: {}", e.lines().rev().find(|l| !l.starts_with("Conversion failed")).unwrap_or_default())),
                        Ok(()) if size == 0 => failed.push(format!("{what}: empty file")),
                        Ok(()) if !text.contains("eyecansee test") || !text.contains("nobody") => failed.push(format!("{what}: tags lost")),
                        Ok(()) if FORMATS[format].cover && !text.contains("attached pic") => failed.push(format!("{what}: cover lost")),
                        Ok(()) => println!("{what}: ok, {size} bytes"),
                    }
                }
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
        println!("{runs} audio runs, {} failed", failed.len());
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
