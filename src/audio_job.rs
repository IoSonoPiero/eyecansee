//! Taking the audio out of a video, or the audio out into files of its own, with the streams
//! copied as they are (`-c copy`): no re-encoding, no loss, the metadata kept. Without the
//! interface: the arguments of ffmpeg, the audio tracks of a file, the names of the new files and
//! one video's job for `batch::run`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::batch::{Phases, Progress, SKIP_VIDEO, Writing, download, keep_thumbnail, upload};
use crate::source::{self, Store};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioJob {
    /// The video and its other tracks, without the audio, in the original's format.
    Strip,
    /// Every audio track in a file of its own, in the format of its codec.
    Extract,
}

/// The video has no audio track: nothing to take out.
pub const NO_AUDIO: &str = "batch-no-audio";

const START: [&str; 6] = ["-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-nostats"];

fn ext_of(file: &str) -> String {
    file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// The metadata copied, then the file. MP4, MOV and their kin write only a few tags unless asked,
/// as in `ffmpeg::args`.
fn output(a: &mut Vec<String>, file: &str) {
    a.extend(["-map_metadata".into(), "0".into()]);
    if matches!(ext_of(file).as_str(), "mp4" | "m4v" | "mov" | "3gp" | "3g2" | "m4a") {
        a.extend(["-movflags".into(), "+use_metadata_tags".into()]);
    }
    a.push(file.into());
}

/// The video without its audio, every other track copied. `data` false leaves out the data
/// tracks too: MP4 cannot always take them as they are (a timecode track: "Could not find tag
/// for codec none", 2026-10-30), and the muxer writes the timecode again from its tag.
pub fn strip_args(input: &str, file: &str, data: bool) -> Vec<String> {
    let mut a: Vec<String> = START.map(String::from).to_vec();
    a.extend(["-i", input, "-map", "0", "-map", "-0:a"].map(String::from));
    if !data {
        a.extend(["-map".into(), "-0:d".into()]);
    }
    a.extend(["-c".into(), "copy".into()]);
    output(&mut a, file);
    a
}

/// Audio track `i` of the input into `files[i]`, all in one run.
pub fn extract_args(input: &str, files: &[String]) -> Vec<String> {
    let mut a: Vec<String> = START.map(String::from).to_vec();
    a.extend(["-i".into(), input.into()]);
    for (i, file) in files.iter().enumerate() {
        a.extend(["-vn".into(), "-map".into(), format!("0:a:{i}"), "-c:a".into(), "copy".into()]);
        output(&mut a, file);
    }
    a
}

/// The codecs of the audio tracks, in their order, from what `ffmpeg -i` writes
/// ("Stream #0:1(und): Audio: aac (LC) (mp4a / 0x6134706D), …").
pub fn audio_codecs(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| l.trim_start().starts_with("Stream #0:"))
        .filter_map(|l| l.split_once(": Audio: "))
        .map(|(_, rest)| rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).next().unwrap_or_default().to_string())
        .collect()
}

/// The extension of a file holding one track of `codec` as it is.
pub fn audio_ext(codec: &str) -> &'static str {
    match codec {
        "aac" | "alac" => "m4a",
        "opus" => "opus",
        "mp3" => "mp3",
        "flac" => "flac",
        "vorbis" => "ogg",
        "ac3" => "ac3",
        "eac3" => "eac3",
        "amr_nb" | "amr_wb" => "amr",
        // WAV takes little-endian PCM only.
        c if c == "pcm_u8" || (c.starts_with("pcm_") && c.ends_with("le")) => "wav",
        // Matroska takes almost any codec.
        _ => "mka",
    }
}

/// The names of the new files, free in `taken`: for `Strip` the original's own, for `Extract`
/// one per track, `stem.m4a`, `stem-2.m4a`…
pub fn new_names(job: AudioJob, name: &str, codecs: &[String], taken: &HashSet<String>) -> Vec<String> {
    if job == AudioJob::Strip {
        return vec![crate::trash::free_name(taken, name)];
    }
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let mut taken = taken.clone();
    codecs
        .iter()
        .enumerate()
        .map(|(i, codec)| {
            let number = if i == 0 { String::new() } else { format!("-{}", i + 1) };
            let free = crate::trash::free_name(&taken, &format!("{stem}{number}.{}", audio_ext(codec)));
            taken.insert(free.clone());
            free
        })
        .collect()
}

/// The codecs of the audio tracks of a local file. `ffmpeg -i` without an output ends with an
/// error, after listing the streams.
fn tracks(program: &Path, input: &Path) -> Result<Vec<String>, String> {
    let out = crate::ffmpeg::command(program).args(["-hide_banner", "-i"]).arg(input).output().map_err(|e| e.to_string())?;
    Ok(audio_codecs(&String::from_utf8_lossy(&out.stderr)))
}

/// One video of `batch::run`: the new files written into the folder `dest` of `out` with free
/// names (added to `taken`), their paths returned. As `batch::video`, a video on a phone or a
/// server is copied into a temporary file first, and the new files go there at the end.
pub async fn run(op: &Store, path: &str, out: &Store, dest: &str, job: AudioJob, taken: &mut HashSet<String>, p: &Arc<Progress>) -> Result<Vec<String>, source::Error> {
    let name = source::name(path).to_string();
    let failed = |e: String| ("batch-failed", format!("{name}: {e}"));
    if crate::ffmpeg::probed().is_none() {
        return Err((SKIP_VIDEO, name));
    }
    let temp = |what: &str, n: &str| std::env::temp_dir().join(format!("eyecansee-{what}-{}-{n}", std::process::id()));
    let remote_in = op.local_file(path).is_none();
    let remote_out = out.local_file(&format!("{dest}{name}")).is_none();
    let phases = Phases { p: p.clone(), count: 1 + remote_in as u32 + remote_out as u32 };
    let input = match op.local_file(path) {
        Some(file) => (file, false),
        None => {
            let file = temp("in", &name);
            download(op, path, &file, |f| phases.set(0, f)).await.map_err(|e| failed(e.to_string()))?;
            (file, true)
        }
    };
    let r = write(&input.0, out, dest, job, taken, &phases, (remote_in as u32, temp)).await.map_err(|e| e.unwrap_or_else(failed));
    if input.1 {
        let _ = std::fs::remove_file(&input.0);
    }
    r
}

/// The work of `run` once the video is a local file. `Err(Ok)`: an error with its Fluent id,
/// `Err(Err)`: what ffmpeg or the copy said.
async fn write(
    input: &Path,
    out: &Store,
    dest: &str,
    job: AudioJob,
    taken: &mut HashSet<String>,
    phases: &Phases,
    (first, temp): (u32, impl Fn(&str, &str) -> PathBuf),
) -> Result<Vec<String>, Result<source::Error, String>> {
    let name = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let probe = crate::ffmpeg::probed().ok_or(Err(String::new()))?;
    let (program, file) = (probe.program.clone(), input.to_path_buf());
    let codecs = tokio::task::spawn_blocking(move || tracks(&program, &file)).await.unwrap_or_else(|e| Err(e.to_string())).map_err(Err)?;
    if codecs.is_empty() {
        return Err(Ok((NO_AUDIO, name)));
    }
    let names = new_names(job, &name, &codecs, taken);
    let tos: Vec<String> = names.iter().map(|n| format!("{dest}{n}")).collect();
    let files: Vec<(PathBuf, bool)> = tos.iter().zip(&names).map(|(to, n)| out.local_file(to).map_or_else(|| (temp("out", n), true), |f| (f, false))).collect();
    let _writing: Vec<Writing> = files.iter().map(|f| Writing::start(f.0.clone())).collect();
    let paths: Vec<String> = files.iter().map(|f| f.0.to_string_lossy().into_owned()).collect();
    let input = input.to_string_lossy().into_owned();
    let attempt = |args: Vec<String>| {
        log!("ffmpeg {args:?}");
        let (program, ph) = (probe.program.clone(), phases.clone());
        async move { tokio::task::spawn_blocking(move || crate::ffmpeg::run(&program, &args, |f| ph.set(first, f), &ph.p.cancel)).await.unwrap_or_else(|e| Err(e.to_string())) }
    };
    let mut r = match job {
        AudioJob::Strip => attempt(strip_args(&input, &paths[0], true)).await,
        AudioJob::Extract => attempt(extract_args(&input, &paths)).await,
    };
    if job == AudioJob::Strip && r.is_err() && !phases.p.cancel.load(Ordering::Relaxed) {
        log!("ffmpeg: again without the data tracks");
        r = attempt(strip_args(&input, &paths[0], false)).await;
    }
    let mut r = r.map_err(Err);
    if r.is_ok() && job == AudioJob::Strip {
        keep_thumbnail(&tos[0], &files[0].0).await;
    }
    for (to, (file, remote)) in tos.iter().zip(&files) {
        if r.is_ok() && *remote {
            let (ph, last) = (phases.clone(), phases.count - 1);
            r = upload(out, to, file, move |f| ph.set(last, f)).await.map_err(|e| Err(e.to_string()));
        }
    }
    for (file, remote) in &files {
        if *remote || r.is_err() {
            let _ = std::fs::remove_file(file);
        }
    }
    r?;
    taken.extend(names);
    Ok(tos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codecs_of_the_streams() {
        let text = "Input #0, matroska,webm, from 'm.mkv':\n  Duration: 00:00:02.00, start: 0.000000, bitrate: 77 kb/s\n  Stream #0:0: Video: h264 (High), yuv444p(progressive), 320x240\n  Stream #0:1(eng): Audio: aac (LC), 44100 Hz, mono, fltp (default)\n  Stream #0:2: Audio: opus, 48000 Hz, mono, fltp\n  Stream #0:3: Subtitle: subrip (srt)\n  Stream #0:4: Audio: pcm_s16le ([1][0][0][0] / 0x0001), 8000 Hz\nAt least one output file must be specified\n";
        assert_eq!(audio_codecs(text), ["aac", "opus", "pcm_s16le"]);
        assert!(audio_codecs("  Stream #0:0: Video: hevc (Main)\n").is_empty());
    }

    #[test]
    fn audio_extensions() {
        let ext = |c: &str| audio_ext(c);
        assert_eq!([ext("aac"), ext("opus"), ext("mp3"), ext("flac"), ext("vorbis"), ext("amr_nb")], ["m4a", "opus", "mp3", "flac", "ogg", "amr"]);
        assert_eq!([ext("pcm_s16le"), ext("pcm_u8"), ext("pcm_s16be"), ext("truehd")], ["wav", "wav", "mka", "mka"]);
    }

    #[test]
    fn names_of_the_new_files() {
        let taken: HashSet<String> = ["v.mp4", "v.m4a"].map(String::from).into();
        let codecs = ["aac", "aac", "opus"].map(String::from);
        assert_eq!(new_names(AudioJob::Strip, "v.mp4", &codecs, &taken), ["v (2).mp4"]);
        assert_eq!(new_names(AudioJob::Extract, "v.mp4", &codecs, &taken), ["v (2).m4a", "v-2.m4a", "v-3.opus"]);
        assert_eq!(new_names(AudioJob::Extract, "w.mkv", &codecs[..1], &taken), ["w.m4a"]);
    }

    #[test]
    fn arguments() {
        let a = strip_args("in.mp4", "out.mp4", true).join(" ");
        assert!(a.ends_with("-i in.mp4 -map 0 -map -0:a -c copy -map_metadata 0 -movflags +use_metadata_tags out.mp4"), "{a}");
        let a = strip_args("in.mkv", "out.mkv", false).join(" ");
        assert!(a.ends_with("-map 0 -map -0:a -map -0:d -c copy -map_metadata 0 out.mkv"), "{a}");
        let a = extract_args("in.mkv", &["a.m4a".into(), "b.opus".into()]).join(" ");
        assert!(a.ends_with("-i in.mkv -vn -map 0:a:0 -c:a copy -map_metadata 0 -movflags +use_metadata_tags a.m4a -vn -map 0:a:1 -c:a copy -map_metadata 0 b.opus"), "{a}");
    }

    /// What `ffmpeg -i` says of a file: its streams and tags.
    fn about(file: &Path) -> String {
        let program = crate::ffmpeg::probed().unwrap().program.clone();
        let out = crate::ffmpeg::command(&program).args(["-hide_banner", "-i"]).arg(file).output().unwrap();
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Both jobs through `batch::run` on clips made by FFmpeg with its own encoders, in MP4 (with
    /// subtitles and a timecode track), MKV (two audio tracks and subtitles), MOV and AVI: the
    /// stripped videos keep every other track and the tags, each audio track comes out in the
    /// format of its codec. Run on every system after a change to the arguments:
    /// `cargo test --release audio_jobs_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn audio_jobs_live() {
        crate::ffmpeg::start_probe(|| {});
        while crate::ffmpeg::probed().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let program = crate::ffmpeg::probed().unwrap().program.clone();
        println!("{program:?}");
        let dir = std::env::temp_dir().join(format!("eyecansee-audio-jobs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("s.srt"), "1\n00:00:00,000 --> 00:00:00,800\nCiao\n").unwrap();
        let clip = |name: &str, extra: &[&str]| {
            let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=1", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-f", "lavfi", "-i", "sine=frequency=880:duration=1"].map(String::from).to_vec();
            a.extend(extra.iter().map(|s| s.replace("{dir}", &dir.to_string_lossy())));
            a.extend(["-metadata", "title=Prova", "-metadata", "location=+45.0+009.0/"].map(String::from));
            a.push(dir.join(name).to_string_lossy().into());
            let r = crate::ffmpeg::command(&program).args(&a).output().unwrap();
            assert!(r.status.success(), "{name}: {}", String::from_utf8_lossy(&r.stderr));
        };
        let mp4 = ["-movflags", "+use_metadata_tags"];
        clip("a.mp4", &[&["-i", "{dir}/s.srt", "-map", "0", "-map", "1", "-map", "3", "-c:v", "mpeg4", "-c:a", "aac", "-c:s", "mov_text", "-timecode", "00:00:00:00"], &mp4[..]].concat());
        clip("b.mkv", &["-i", "{dir}/s.srt", "-map", "0", "-map", "1", "-map", "2", "-map", "3", "-c:v", "mpeg4", "-c:a:0", "aac", "-c:a:1", "flac", "-c:s", "srt"]);
        clip("c.mov", &[&["-map", "0", "-map", "1", "-c:v", "mpeg4", "-c:a", "aac", "-timecode", "00:00:00:00"], &mp4[..]].concat());
        clip("d.avi", &["-map", "0", "-map", "1", "-c:v", "mpeg4", "-c:a", "pcm_s16le"]);
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths: Vec<String> = ["a.mp4", "b.mkv", "c.mov", "d.avi"].map(String::from).into();
        for (job, sub) in [(AudioJob::Strip, "strip/"), (AudioJob::Extract, "extract/")] {
            let p = Arc::new(Progress::default());
            let dest = crate::batch::Dest { op: op.clone(), dir: sub.into(), create: true };
            rt.block_on(crate::batch::run(&op, paths.clone(), dest, crate::batch::Job { audio: Some(job), ..Default::default() }, p.clone()));
            println!("{job:?}: saved {:?}, failed {:?}", p.saved.lock().unwrap(), p.failed.lock().unwrap());
            assert!(p.failed.lock().unwrap().is_empty());
        }
        for name in ["a.mp4", "b.mkv", "c.mov", "d.avi"] {
            let text = about(&dir.join("strip").join(name));
            assert!(audio_codecs(&text).is_empty() && text.contains("Video: mpeg4"), "{name}: {text}");
            assert!(text.contains("Prova"), "{name}: the title is lost\n{text}");
            if name != "d.avi" {
                assert!(text.contains("+45.0"), "{name}: the place is lost\n{text}");
            }
            if name != "c.mov" && name != "d.avi" {
                assert!(text.contains("Subtitle:"), "{name}: the subtitles are lost\n{text}");
            }
        }
        for (name, codec) in [("a.m4a", "aac"), ("b.m4a", "aac"), ("b-2.flac", "flac"), ("c.m4a", "aac"), ("d.wav", "pcm_s16le")] {
            let text = about(&dir.join("extract").join(name));
            assert_eq!(audio_codecs(&text), [codec], "{name}: {text}");
            assert!(!text.contains("Video:"), "{name}: {text}");
            if name != "d.wav" {
                assert!(text.contains("Prova"), "{name}: the title is lost\n{text}");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
