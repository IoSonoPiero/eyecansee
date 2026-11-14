//! A video copied as it is ("Same as the original (no re-encoding)", `Codec::Copy`), without the
//! interface: which containers the window offers for the videos chosen, and which one each file
//! really goes in. FFmpeg copies streams only into a container that can hold their codecs: VP8 in
//! MP4 fails ("Could not find tag for codec vp8", a .webm, 2026-11-14).
//!
//! The window starts from the originals' own container (a .webm copied as WebM); videos of
//! different containers chosen together are copied as MKV, which holds them all (chosen over
//! "each in its own" to keep one container per job, as for the other codecs). Then each file is
//! checked by its streams (`for_file`): one the chosen container cannot hold is copied as MKV,
//! never a combination that fails.

use std::path::Path;

use crate::ffmpeg::CONTAINERS;

fn index(muxer: &str) -> usize {
    CONTAINERS.iter().position(|c| c.muxer == muxer).expect("a container of CONTAINERS")
}

/// Matroska: it holds every codec, the fallback.
pub fn mkv() -> usize {
    index("matroska")
}

/// The container of a video, by its extension; `None` for the others (.ts, .3gp, .wmv…).
pub fn of_name(name: &str) -> Option<usize> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    Some(index(match ext.as_str() {
        "mp4" | "m4v" => "mp4",
        "mov" | "qt" => "mov",
        "mkv" => "matroska",
        "webm" => "webm",
        "avi" => "avi",
        _ => return None,
    }))
}

/// The containers a copy of a video in `own` goes in for sure, by its usual codecs: its own and
/// MKV (MP4 and MOV hold each other's).
fn targets_of(own: Option<usize>) -> Vec<usize> {
    let own = own.map(|c| CONTAINERS[c].muxer);
    let mut t: Vec<usize> = match own {
        Some("mp4") | Some("mov") => vec![index("mp4"), index("mov")],
        Some(m) => vec![index(m)],
        None => Vec::new(),
    };
    t.push(mkv());
    t.dedup();
    t
}

/// The containers the window offers for copying every one of `names` (videos), in the order of
/// `CONTAINERS`; MKV always.
pub fn targets<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let mut all: Vec<usize> = (0..CONTAINERS.len()).collect();
    for name in names {
        let these = targets_of(of_name(name));
        all.retain(|c| these.contains(c));
    }
    all
}

/// The container the copy starts on: the originals' own when they all share it, else MKV.
pub fn default<'a>(names: impl IntoIterator<Item = &'a str>) -> usize {
    let mut owns = names.into_iter().map(of_name);
    match owns.next() {
        Some(Some(first)) if owns.all(|c| c == Some(first)) => first,
        _ => mkv(),
    }
}

/// Whether FFmpeg copies a `video` and an `audio` stream (codecs as `ffmpeg -i` names them) of a
/// file in `source` into `container`. Only codecs tried (FFmpeg 8.1, 2026-11-14) and accepted
/// without `-strict` by older FFmpegs: Opus, FLAC and Vorbis in MP4 were experimental there.
pub fn holds(container: usize, source: Option<usize>, video: Option<&str>, audio: Option<&str>) -> bool {
    const AVI_AUDIO: &[&str] = &["mp3", "mp2", "ac3", "pcm_s16le", "pcm_s24le", "pcm_u8"];
    let (videos, audios): (&[&str], &[&str]) = match CONTAINERS[container].muxer {
        "matroska" => return true,
        "mp4" => (&["h264", "hevc", "av1", "vp9", "mpeg4", "mpeg2video", "mjpeg"], &["aac", "mp3", "mp2", "ac3", "eac3", "alac"]),
        "mov" => (&["h264", "hevc", "prores", "mpeg4", "mpeg2video", "mjpeg"], &["aac", "mp3", "mp2", "ac3", "eac3", "alac", "pcm_s16le", "pcm_s16be", "pcm_s24le"]),
        "webm" => (&["vp8", "vp9", "av1"], &["opus", "vorbis"]),
        // H.264 only as an AVI has it: from MP4 or MKV "no startcode found".
        "avi" if source == Some(container) => (&["mpeg4", "mjpeg", "mpeg2video", "h264"], AVI_AUDIO),
        "avi" => (&["mpeg4", "mjpeg", "mpeg2video"], AVI_AUDIO),
        _ => return false,
    };
    video.is_none_or(|c| videos.contains(&c)) && audio.is_none_or(|c| audios.contains(&c))
}

/// `chosen` when it holds the streams, else MKV.
pub fn choose(chosen: usize, source: Option<usize>, video: Option<&str>, audio: Option<&str>) -> usize {
    if holds(chosen, source, video, audio) { chosen } else { mkv() }
}

/// The codecs of the streams of `kind` ("Video", "Audio") in what `ffmpeg -i` writes
/// ("Stream #0:1(und): Audio: aac (LC) (mp4a / 0x6134706D), …"), in their order; a cover picture
/// ("(attached pic)") is not the video.
pub fn codecs(text: &str, kind: &str) -> Vec<String> {
    let tag = format!(": {kind}: ");
    text.lines()
        .filter(|l| l.trim_start().starts_with("Stream #0:") && !l.contains("(attached pic)"))
        .filter_map(|l| l.split_once(tag.as_str()))
        .map(|(_, rest)| rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).next().unwrap_or_default().to_string())
        .collect()
}

/// The container the copy of the local `input` (named `name`) goes in, `chosen` in the window:
/// its streams read by `ffmpeg -i`, which ends with an error after listing them. MKV when they
/// cannot be read.
pub fn for_file(chosen: usize, name: &str, program: &Path, input: &Path) -> usize {
    let Ok(out) = crate::ffmpeg::command(program).args(["-hide_banner", "-i"]).arg(input).output() else { return mkv() };
    let text = String::from_utf8_lossy(&out.stderr);
    let (video, audio) = (codecs(&text, "Video"), codecs(&text, "Audio"));
    let c = choose(chosen, of_name(name), video.first().map(String::as_str), audio.first().map(String::as_str));
    log!("copy of {name}: video {video:?}, audio {audio:?}: {}", CONTAINERS[c].name);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> usize {
        CONTAINERS.iter().position(|c| c.name == name).unwrap()
    }

    /// The window: the originals' own container, MKV for different ones.
    #[test]
    fn window_starts_on_the_originals_container() {
        let (mp4, mkv, webm, mov, avi) = (named("MP4"), named("MKV"), named("WebM"), named("MOV"), named("AVI"));
        assert_eq!(default(["Bees on flowers.webm"]), webm);
        assert_eq!(targets(["Bees on flowers.webm"]), [mkv, webm]);
        assert_eq!(default(["a.MP4", "b.m4v"]), mp4);
        assert_eq!(targets(["a.mp4"]), [mp4, mkv, mov]);
        assert_eq!(default(["a.mkv"]), mkv);
        assert_eq!(targets(["a.mkv"]), [mkv]);
        assert_eq!(default(["a.mov"]), mov);
        assert_eq!(default(["a.avi"]), avi);
        assert_eq!(targets(["a.avi"]), [mkv, avi]);
        // Mixed: MKV, the one that holds both.
        assert_eq!(default(["a.mp4", "b.webm"]), mkv);
        assert_eq!(targets(["a.mp4", "b.webm"]), [mkv]);
        assert_eq!(default(["a.mp4", "b.mov"]), mkv);
        assert_eq!(targets(["a.mp4", "b.mov"]), [mp4, mkv, mov]);
        // Others: MKV only.
        assert_eq!((default(["a.mts"]), targets(["a.mts"])), (mkv, vec![mkv]));
        // Every one offered can take a copy.
        assert!(targets(["a.mp4", "a.webm", "a.mov", "a.avi", "a.mkv"]).iter().all(|&c| CONTAINERS[c].codecs.contains(&crate::ffmpeg::Codec::Copy)));
    }

    /// Each file: the chosen container when it holds the streams, else MKV.
    #[test]
    fn each_file_goes_where_its_streams_fit() {
        let (mp4, mkv, webm, mov, avi) = (named("MP4"), named("MKV"), named("WebM"), named("MOV"), named("AVI"));
        // webm → WebM; VP8 and Vorbis are no MP4 (the bug), VP9 and Opus neither by this list.
        assert_eq!(choose(webm, Some(webm), Some("vp8"), Some("vorbis")), webm);
        assert_eq!(choose(mp4, Some(webm), Some("vp8"), Some("vorbis")), mkv);
        assert_eq!(choose(mp4, Some(webm), Some("vp9"), Some("opus")), mkv);
        assert_eq!(choose(webm, Some(webm), Some("av1"), None), webm);
        // mp4 H.264 → MP4, MOV; not WebM.
        assert_eq!(choose(mp4, Some(mp4), Some("h264"), Some("aac")), mp4);
        assert_eq!(choose(mov, Some(mp4), Some("hevc"), Some("aac")), mov);
        assert_eq!(choose(webm, Some(mp4), Some("h264"), Some("aac")), mkv);
        // mkv → MKV, whatever it holds.
        assert_eq!(choose(mkv, Some(mkv), Some("vp8"), Some("flac")), mkv);
        // Mixed selection copied as MKV: both stay MKV.
        assert_eq!(choose(mkv, Some(mp4), Some("h264"), Some("aac")), mkv);
        assert_eq!(choose(mkv, Some(webm), Some("vp8"), Some("vorbis")), mkv);
        // mov: ProRes and PCM stay MOV; MP4 does not take ProRes.
        assert_eq!(choose(mov, Some(mov), Some("prores"), Some("pcm_s16le")), mov);
        assert_eq!(choose(mp4, Some(mov), Some("prores"), Some("pcm_s16le")), mkv);
        // avi: MPEG-4 and MP3 stay AVI; H.264 only from an AVI.
        assert_eq!(choose(avi, Some(avi), Some("mpeg4"), Some("mp3")), avi);
        assert_eq!(choose(avi, Some(avi), Some("h264"), Some("mp3")), avi);
        assert_eq!(choose(avi, Some(mp4), Some("h264"), Some("aac")), mkv);
        // Unknown codecs: MKV.
        assert_eq!(choose(mp4, None, Some("wmv3"), Some("wmav2")), mkv);
    }

    #[test]
    fn streams_read() {
        let text = "Input #0, matroska,webm, from 'Bees on flowers.webm':\n  Stream #0:0(eng): Video: vp8, yuv420p(tv, bt470bg/unknown/unknown, progressive), 1920x1080\n  Stream #0:1(eng): Audio: vorbis, 44100 Hz, stereo, fltp\n";
        assert_eq!((codecs(text, "Video"), codecs(text, "Audio")), (vec!["vp8".to_string()], vec!["vorbis".to_string()]));
        let cover = "  Stream #0:0: Video: mjpeg (Baseline), yuvj420p (attached pic)\n  Stream #0:1[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p\n";
        assert_eq!(codecs(cover, "Video"), ["h264"]);
    }
}
