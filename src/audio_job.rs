//! Taking the audio out of a video, or the audio out into files of its own, with the streams
//! copied as they are (`-c copy`): no re-encoding, no loss, the metadata kept. Without the
//! interface: the arguments of ffmpeg, the audio tracks of a file, the names of the new files and
//! one video's job for `batch::run`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use crate::batch::{Progress, SKIP_VIDEO, keep_thumbnail};
use crate::ffmpeg_files::{Input, Output, failed, send};
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
    crate::video_copy::codecs(text, "Audio")
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
/// server is copied into a temporary file first, and the new files go there at the end
/// (`ffmpeg_files`).
pub async fn run(op: &Store, (base, path): (&str, &str), out: &Store, dest: &str, job: AudioJob, taken: &mut HashSet<String>, p: &Arc<Progress>) -> Result<Vec<String>, source::Error> {
    let name = source::name(path).to_string();
    // Copied streams: an FFmpeg without the phones' decoders does it too (`ffmpeg::copier`).
    let Some(program) = crate::ffmpeg::copier() else { return Err((SKIP_VIDEO, name)) };
    let input = Input::fetch(op, path, (out, dest), p).await?;
    // The same pictures without the audio: the original's thumbnail, not a frame made again.
    let thumb = (job == AudioJob::Strip).then(|| crate::thumbcache::stored(base, path)).flatten();
    write(&program, &input, (out, dest), job, taken, thumb, crate::mp4::copy_preview).await.map_err(|e| e.unwrap_or_else(|e| failed(&name, e)))
}

/// What writes the camera's preview of a video into its copy (`mp4::copy_preview`).
type PutPreview = fn(&Path, &Path) -> std::io::Result<bool>;

/// The work of `run` once the video is a local file, the preview written by `put`. `Err(Ok)`: an
/// error with its Fluent id, `Err(Err)`: what ffmpeg or the copy said.
async fn write(program: &Path, input: &Input, (out, dest): (&Store, &str), job: AudioJob, taken: &mut HashSet<String>, thumb: Option<(u64, Vec<u8>)>, put: PutPreview) -> Result<Vec<String>, Result<source::Error, String>> {
    // The original's name: a temporary copy has another one.
    let name = input.name.clone();
    let (p, file) = (program.to_path_buf(), input.file.clone());
    let codecs = tokio::task::spawn_blocking(move || tracks(&p, &file)).await.unwrap_or_else(|e| Err(e.to_string())).map_err(Err)?;
    if codecs.is_empty() {
        return Err(Ok((NO_AUDIO, name)));
    }
    let names = new_names(job, &name, &codecs, taken);
    let outputs: Vec<Output> = names.iter().map(|n| Output::new(out, format!("{dest}{n}"), n)).collect();
    let paths: Vec<String> = outputs.iter().map(|o| o.file.to_string_lossy().into_owned()).collect();
    let source = input.file.to_string_lossy().into_owned();
    // With the data tracks, unless FFmpeg could not copy them.
    let mut data = true;
    let mut r = match job {
        AudioJob::Strip => input.ffmpeg(program, strip_args(&source, &paths[0], data)).await,
        AudioJob::Extract => input.ffmpeg(program, extract_args(&source, &paths)).await,
    };
    if job == AudioJob::Strip && r.is_err() && !input.cancelled() {
        log!("ffmpeg: again without the data tracks");
        data = false;
        r = input.ffmpeg(program, strip_args(&source, &paths[0], data)).await;
    }
    let mut without_preview = false;
    if r.is_ok() && job == AudioJob::Strip {
        (r, without_preview) = with_preview(program, input, &outputs[0], (&source, data), thumb, put).await;
    }
    send(out, &outputs, &input.phases, r.map_err(Err), Err).await?;
    if without_preview {
        input.phases.p.no_preview.lock().unwrap().push(names[0].clone());
    }
    taken.extend(names);
    Ok(outputs.iter().map(|o| o.to.clone()).collect())
}

/// The video without its audio, made by FFmpeg from `source` into `output` (with the data tracks
/// when `data`): the camera's preview put in (`preview`) and the thumbnail kept. A write of the
/// preview that fails may leave the copy cut off half way (a full disk): the copy is made again
/// without the preview, and `true` says so (#216). The result is that of FFmpeg's new run.
async fn with_preview(program: &Path, input: &Input, output: &Output, (source, data): (&str, bool), thumb: Option<(u64, Vec<u8>)>, put: PutPreview) -> (Result<(), String>, bool) {
    let (copied, without) = match preview(&input.file, output, put).await {
        Ok(copied) => (copied, false),
        Err(e) => {
            log!("ffmpeg: the camera's preview not copied ({e}): the copy made again without it");
            (false, true)
        }
    };
    let r = if without { input.ffmpeg(program, strip_args(source, &output.file.to_string_lossy(), data)).await } else { Ok(()) };
    if r.is_ok() {
        keep_thumb(output, thumb, copied).await;
    }
    (r, without)
}

/// The camera's preview, which FFmpeg leaves out, copied by `put` from the original into the
/// video without its audio; `false` when there was none to copy.
async fn preview(original: &Path, output: &Output, put: PutPreview) -> Result<bool, String> {
    let (original, to) = (original.to_path_buf(), output.file.clone());
    let copied = tokio::task::spawn_blocking(move || put(&original, &to)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    if copied {
        log!("ffmpeg: the camera's preview copied into {}", output.to);
    }
    Ok(copied)
}

/// The thumbnail of the video without its audio: the original's, the same pictures; without it,
/// the camera's preview read from the file when `copied`, otherwise a frame of the video.
async fn keep_thumb(output: &Output, thumb: Option<(u64, Vec<u8>)>, copied: bool) {
    let size = std::fs::metadata(&output.file).map(|m| m.len());
    match (thumb, size) {
        (Some((_, data)), Ok(size)) => crate::thumbcache::keep(&output.to, size, data),
        _ if !copied => keep_thumbnail(&output.to, &output.file).await,
        _ => {}
    }
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

    /// A video of a phone or a server: the new files named after it, not after its temporary
    /// copy ("eyecansee-in-<process>-v.m4a" before). A script stands for FFmpeg: it lists one
    /// audio track, and copies the input into the last file it is given.
    #[cfg(unix)]
    #[test]
    fn named_after_a_remote_original() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("eyecansee-audio-job-name-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("ffmpeg");
        let text = "#!/bin/sh\nprev=\nfor a; do [ \"$prev\" = -i ] && input=$a; prev=$a; last=$a; done\nif [ $# -le 3 ]; then echo '  Stream #0:1: Audio: aac (LC)' >&2; exit 1; fi\ncp \"$input\" \"$last\"\n";
        std::fs::write(&script, text).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Memory::default()).unwrap());
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let Store::Dal(o) = &op else { unreachable!() };
            o.write("trip/v-name.mp4", b"the video".to_vec()).await.unwrap();
            let input = Input::fetch(&op, "trip/v-name.mp4", (&op, "out/"), &Arc::new(Progress::default())).await.unwrap();
            let mut taken = HashSet::new();
            let saved = write(&script, &input, (&op, "out/"), AudioJob::Extract, &mut taken, None, crate::mp4::copy_preview).await.unwrap();
            assert_eq!(saved, ["out/v-name.m4a"]);
            assert_eq!(op.read("out/v-name.m4a").await.unwrap(), b"the video");
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// What the test of a full disk writes into: only where it runs (Unix, it needs a script).
    #[cfg(unix)]
    mod full {
        use std::path::Path;

        /// A file whose writes stop after `left` more bytes, as on a disk that fills up.
        struct Filling {
            f: std::fs::File,
            left: usize,
        }

        impl std::io::Read for Filling {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                self.f.read(b)
            }
        }

        impl std::io::Seek for Filling {
            fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
                self.f.seek(to)
            }
        }

        impl std::io::Write for Filling {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if self.left == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "no space left on device"));
                }
                let n = self.f.write(&b[..b.len().min(self.left)])?;
                self.left -= n;
                Ok(n)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.f.flush()
            }
        }

        impl crate::mp4::Rewritten for Filling {
            fn set_len(&mut self, len: u64) -> std::io::Result<()> {
                self.f.set_len(len)
            }
        }

        /// `mp4::copy_preview` on a disk that is full after 20 more bytes.
        pub(super) fn full_disk(original: &Path, copy: &Path) -> std::io::Result<bool> {
            crate::mp4::copy_preview_with(original, copy, |path| Ok(Filling { f: std::fs::OpenOptions::new().read(true).write(true).open(path)?, left: 20 }))
        }
    }

    /// Copy without audio of a camera's video with its own preview, when the disk fills up while
    /// the preview is written into the copy: the job delivered the copy with its `moov` cut off
    /// half way, a video that does not play, as saved (audit 3.8, #216). A script stands for
    /// FFmpeg: it lists one audio track, and writes the copy as FFmpeg does, `moov` last and
    /// without the preview.
    #[cfg(unix)]
    #[test]
    fn preview_not_written_leaves_a_whole_video() {
        use crate::mp4::tests::mp4_box;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("eyecansee-audio-job-preview-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("out")).unwrap();
        let (ftyp, mdat, trak) = (mp4_box(b"ftyp", b"isomisom"), mp4_box(b"mdat", &[7; 100]), mp4_box(b"trak", &[1; 20]));
        let thmb = mp4_box(b"udta", &mp4_box(b"thmb", &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4]));
        let camera = mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 12]), trak.clone(), thmb].concat());
        std::fs::write(dir.join("cam.mp4"), [ftyp.clone(), camera, mdat.clone()].concat()).unwrap();
        let made = [ftyp, mdat, mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 12]), trak].concat())].concat();
        let moov_at = made.len() - 8 - 12 - 8 - 28;
        std::fs::write(dir.join("made.mp4"), &made).unwrap();
        let script = dir.join("ffmpeg");
        let text = format!("#!/bin/sh\nfor a; do last=$a; done\nif [ $# -le 3 ]; then echo '  Stream #0:1: Audio: aac (LC)' >&2; exit 1; fi\ncp '{}' \"$last\"\n", dir.join("made.mp4").display());
        std::fs::write(&script, text).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let p = Arc::new(Progress::default());
        let r = tokio::runtime::Runtime::new().unwrap().block_on(async {
            let input = Input::fetch(&op, "cam.mp4", (&op, "out/"), &p).await.unwrap();
            write(&script, &input, (&op, "out/"), AudioJob::Strip, &mut HashSet::new(), None, full::full_disk).await
        });
        let delivered = std::fs::read(dir.join("out").join("cam.mp4")).ok();
        std::fs::remove_dir_all(&dir).unwrap();
        // Saved: the copy is whole, as FFmpeg made it, without the preview. Or not saved at all.
        match r {
            Ok(saved) => {
                assert_eq!(saved, ["out/cam.mp4"]);
                assert_eq!(*p.no_preview.lock().unwrap(), ["cam.mp4"], "the summary names it");
                let delivered = delivered.unwrap();
                let moov = crate::mp4::box_header(&delivered[moov_at..]);
                assert!(delivered == made, "a broken copy delivered as saved: its moov {moov:?} in {} bytes left", delivered.len() - moov_at);
            }
            Err(e) => assert!(delivered.is_none(), "{e:?}: the broken copy left in the folder"),
        }
    }

    /// The jobs that copy the streams (both audio jobs and "Same as the original") on
    /// `EYECANSEE_VIDEO`, a phone's H.264 or HEVC video with sound, with the FFmpeg of `copier`
    /// and no probe of the encoders. Where the system's FFmpeg has no H.264 and HEVC decoders
    /// (Fedora's ffmpeg-free, issue #67) they work without the download:
    /// `EYECANSEE_VIDEO=… cargo test copy_jobs_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn copy_jobs_live() {
        let video = std::env::var("EYECANSEE_VIDEO").expect("EYECANSEE_VIDEO: a phone's video");
        println!("program {:?}, copier {:?}", crate::ffmpeg::program(), crate::ffmpeg::copier());
        crate::ffmpeg::copier().expect("no FFmpeg that copies");
        crate::ffmpeg::start_copy_probe(|| {});
        while crate::ffmpeg::copy_probed().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let dir = std::env::temp_dir().join(format!("eyecansee-copy-jobs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&video, dir.join("v.mp4")).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let same = crate::ffmpeg::Video { container: 0, encoder: "copy", quality: crate::ffmpeg::Quality::Medium, audio: true, meta: true };
        let jobs = [
            ("strip/", crate::batch::Job { audio: Some(AudioJob::Strip), ..Default::default() }),
            ("extract/", crate::batch::Job { audio: Some(AudioJob::Extract), ..Default::default() }),
            ("same/", crate::batch::Job { video: Some(same), ..Default::default() }),
        ];
        for (sub, job) in jobs {
            let p = Arc::new(Progress::default());
            let dest = crate::batch::Dest { op: op.clone(), dir: sub.into(), create: true, home: false };
            rt.block_on(crate::batch::run(&op, vec!["v.mp4".into()], dest, job, p.clone()));
            println!("{sub}: saved {:?}, failed {:?}", p.saved.lock().unwrap(), p.failed.lock().unwrap());
            assert!(p.failed.lock().unwrap().is_empty() && !p.saved.lock().unwrap().is_empty(), "{sub}");
        }
        let strip = about(&dir.join("strip").join("v.mp4"));
        assert!(audio_codecs(&strip).is_empty() && strip.contains("Video:"), "{strip}");
        let same = about(&dir.join("same").join("v.mp4"));
        assert!(!audio_codecs(&same).is_empty() && same.contains("Video:"), "{same}");
        let extracted = std::fs::read_dir(dir.join("extract")).unwrap().count();
        assert!(extracted > 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// What `ffmpeg -i` says of a file: its streams and tags.
    fn about(file: &Path) -> String {
        let program = crate::ffmpeg::copier().unwrap();
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
        // The clips are made by the FFmpeg that encodes, the jobs run with `copier`.
        crate::ffmpeg::start_probe(|| {});
        while crate::ffmpeg::probed().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let program = crate::ffmpeg::probed().unwrap().program.clone();
        println!("{program:?}, copies with {:?}", crate::ffmpeg::copier());
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
            let dest = crate::batch::Dest { op: op.clone(), dir: sub.into(), create: true, home: false };
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
        // A camera's video with its own preview (`EYECANSEE_PREVIEW_VIDEO`, a dashcam's):
        // the copy without audio has the same preview.
        if let Ok(video) = std::env::var("EYECANSEE_PREVIEW_VIDEO") {
            let original = std::fs::read(&video).unwrap();
            std::fs::write(dir.join("cam.mp4"), &original).unwrap();
            let p = Arc::new(Progress::default());
            let dest = crate::batch::Dest { op: op.clone(), dir: "cam/".into(), create: true, home: false };
            rt.block_on(crate::batch::run(&op, vec!["cam.mp4".into()], dest, crate::batch::Job { audio: Some(AudioJob::Strip), ..Default::default() }, p.clone()));
            let copy = std::fs::read(dir.join("cam").join("cam.mp4")).unwrap();
            let preview = crate::mp4::thumbnail(&original[..original.len().min(crate::mp4::HEAD as usize)]).expect("the original has a preview");
            assert!(copy.windows(preview.len()).any(|w| w == preview), "the preview is not in the copy");
            println!("preview of {} bytes copied", preview.len());
            // Still a whole video: its start and its end decode without errors.
            for at in [["-ss", "0"], ["-sseof", "-1"]] {
                let r = crate::ffmpeg::command(&program).args(["-v", "error"]).args(at).arg("-i").arg(dir.join("cam").join("cam.mp4")).args(["-t", "1", "-f", "null", "-"]).output().unwrap();
                assert!(r.status.success() && r.stderr.is_empty(), "{at:?}: {}", String::from_utf8_lossy(&r.stderr));
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
