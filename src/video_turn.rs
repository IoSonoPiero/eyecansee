//! Turning a video as a photo is turned (#125): only the rotation the player reads is changed,
//! the streams are copied as they are (`-c copy`), nothing is encoded again. Without the
//! interface: the rotation a video has, the new one after quarter turns, which containers hold
//! one, and the arguments of ffmpeg.

use std::path::Path;

/// The containers whose rotation FFmpeg writes on a copy, by the file's extension: the display
/// matrix of MP4 and MOV, the projection of Matroska and WebM (FFmpeg 8.1, 2026-11-14; `write`
/// checks it on each file, an older FFmpeg may not write it).
pub fn holds_rotation(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "mp4" | "m4v" | "mov" | "qt" | "3gp" | "3g2" | "mkv" | "webm")
}

/// The rotation of the video stream as `ffmpeg -i` prints it ("displaymatrix: rotation of
/// -90.00 degrees"), counter-clockwise in degrees; 0 without one.
pub fn rotation_of(text: &str) -> f64 {
    text.lines()
        .find_map(|l| l.split_once("rotation of ").and_then(|(_, r)| r.split_whitespace().next()?.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

/// The rotation after `quarters` clockwise quarter turns, counter-clockwise in degrees as
/// `-display_rotation` takes it, from -90 to 180.
pub fn turned(rotation: f64, quarters: u8) -> i32 {
    let r = (rotation.round() as i32 - 90 * quarters as i32).rem_euclid(360);
    if r > 180 { r - 360 } else { r }
}

/// The arguments of ffmpeg: every stream copied with the new rotation, the metadata kept.
/// `data` false leaves out the data tracks, which MP4 cannot always take as they are (as in
/// `audio_job::strip_args`). FFmpeg 7 and later (`-display_rotation`); `old` uses the "rotate"
/// tag of the FFmpegs before it, clockwise.
pub fn args(input: &str, output: &str, rotation: i32, data: bool, old: bool) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-nostats"].map(String::from).to_vec();
    if !old {
        a.extend(["-display_rotation:v:0".into(), rotation.to_string()]);
    }
    a.extend(["-i", input, "-map", "0"].map(String::from));
    if !data {
        a.extend(["-map".into(), "-0:d".into()]);
    }
    a.extend(["-c".into(), "copy".into()]);
    if old {
        a.extend(["-metadata:s:v:0".into(), format!("rotate={}", (-rotation).rem_euclid(360))]);
    }
    a.extend(["-map_metadata", "0", "-movflags", "+use_metadata_tags", output].map(String::from));
    a
}

/// The rotation of a local video, read by `ffmpeg -i` (which ends with an error after listing
/// the streams).
fn read_rotation(program: &Path, file: &Path) -> Result<f64, String> {
    let out = crate::ffmpeg::command(program).args(["-hide_banner", "-i"]).arg(file).output().map_err(|e| e.to_string())?;
    Ok(rotation_of(&String::from_utf8_lossy(&out.stderr)))
}

/// The local `input` written into `output` turned by `quarters` clockwise quarter turns: its
/// rotation read first, then copied with the new one; again without the data tracks, then with
/// the old tag, when FFmpeg refuses or the new file does not have it.
pub fn write(program: &Path, input: &Path, output: &Path, quarters: u8, cancel: &std::sync::atomic::AtomicBool, progress: impl Fn(f32)) -> Result<(), String> {
    let rotation = turned(read_rotation(program, input)?, quarters);
    let (from, to) = (input.to_string_lossy(), output.to_string_lossy());
    let mut r = Err(String::new());
    for (data, old) in [(true, false), (false, false), (true, true)] {
        let a = args(&from, &to, rotation, data, old);
        log!("ffmpeg {a:?}");
        r = crate::ffmpeg::run(program, &a, &progress, cancel).and_then(|()| match read_rotation(program, output) {
            Ok(got) if turned(got, 0) == rotation => Ok(()),
            Ok(got) => Err(format!("rotation {got} written, {rotation} wanted")),
            Err(e) => Err(e),
        });
        if r.is_ok() || cancel.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
    }
    r
}

/// The video `path` of `op` saved over itself turned by `quarters` clockwise quarter turns, the
/// original going as `how` says (as a photo's Save). A video on a phone or a server is copied
/// here first; the new one is written into a temporary file, then sent into place.
pub(crate) async fn replace(op: &crate::source::Store, path: &str, quarters: u8, how: crate::source::Deletion) -> Result<(), crate::source::Error> {
    let name = crate::source::name(path).to_string();
    let Some(program) = crate::ffmpeg::copier() else { return Err((crate::batch::SKIP_VIDEO, name)) };
    let temp = |what: &str| std::env::temp_dir().join(format!("eyecansee-turn-{what}-{}-{name}", std::process::id()));
    let input = match op.local_file(path) {
        Some(file) => (file, false),
        None => {
            let file = temp("in");
            crate::batch::download(op, path, &file, || false, |_| {}).await.map_err(|e| ("error-detail", e.to_string()))?;
            (file, true)
        }
    };
    let output = temp("out");
    let (from, to) = (input.0.clone(), output.clone());
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let written = tokio::task::spawn_blocking(move || write(&program, &from, &to, quarters, &cancel, |_| {})).await.unwrap_or_else(|e| Err(e.to_string()));
    if input.1 {
        let _ = std::fs::remove_file(&input.0);
    }
    let r = match written {
        Ok(()) => crate::replace::replace_with_file(op, path, &output, how).await,
        Err(e) => Err(("batch-failed", format!("{name}: {e}"))),
    };
    let _ = std::fs::remove_file(&output);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations() {
        assert_eq!(rotation_of("    Side data:\n      displaymatrix: rotation of -90.00 degrees\n"), -90.0);
        assert_eq!(rotation_of("  Stream #0:0: Video: h264\n"), 0.0);
        // A phone's portrait video (-90) turned once more clockwise: upside down.
        assert_eq!(turned(-90.0, 1), 180);
        assert_eq!(turned(0.0, 1), -90);
        assert_eq!(turned(0.0, 3), 90);
        assert_eq!(turned(90.0, 1), 0);
        assert_eq!(turned(180.0, 2), 0);
        assert_eq!(turned(-90.0, 0), -90);
    }

    #[test]
    fn arguments() {
        let a = args("in.mp4", "out.mp4", -90, true, false).join(" ");
        assert!(a.contains("-display_rotation:v:0 -90 -i in.mp4 -map 0 -c copy -map_metadata 0 -movflags +use_metadata_tags out.mp4"), "{a}");
        let a = args("in.mp4", "out.mp4", -90, false, true).join(" ");
        assert!(a.contains("-i in.mp4 -map 0 -map -0:d -c copy -metadata:s:v:0 rotate=90 -map_metadata 0"), "{a}");
        assert!(!a.contains("display_rotation"), "{a}");
        assert!(holds_rotation("a.MP4") && holds_rotation("b.mov") && holds_rotation("c.webm") && !holds_rotation("d.avi"));
    }

    /// `cargo test video_turn_live -- --ignored --nocapture`: a clip of every container made,
    /// turned by one, two and three quarters, and its rotation read back; those of
    /// `holds_rotation` must keep it, the others are listed.
    #[test]
    #[ignore]
    fn video_turn_live() {
        let program = crate::ffmpeg::copier().expect("an FFmpeg");
        let dir = std::env::temp_dir().join(format!("eyecansee-video-turn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut failed = Vec::new();
        for (name, codec) in [("a.mp4", "libx264"), ("a.mov", "libx264"), ("a.mkv", "libx264"), ("a.webm", "libvpx-vp9")] {
            let input = dir.join(name);
            let make = ["-hide_banner", "-nostdin", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=1", "-f", "lavfi", "-i", "sine=duration=1", "-c:v", codec, "-shortest", "-metadata", "title=turn test"];
            let a: Vec<String> = make.iter().map(|s| s.to_string()).chain([input.to_string_lossy().into()]).collect();
            crate::ffmpeg::run(&program, &a, |_| {}, &cancel).unwrap();
            for quarters in 1..=3u8 {
                let output = dir.join(format!("turned-{name}"));
                let r = write(&program, &input, &output, quarters, &cancel, |_| {});
                let out = crate::ffmpeg::command(&program).args(["-hide_banner", "-i"]).arg(&output).output().unwrap();
                let text = String::from_utf8_lossy(&out.stderr);
                let got = turned(rotation_of(&text), 0);
                let want = turned(0.0, quarters);
                let ok = r.is_ok() && got == want && text.contains("turn test");
                println!("{name} {quarters}: {r:?}, rotation {got} (wanted {want}), title {}", text.contains("turn test"));
                if ok != holds_rotation(name) {
                    failed.push(format!("{name} {quarters}: rotation {got}, wanted {want}"));
                }
                let _ = std::fs::remove_file(&output);
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    /// `cargo test video_saved_turned_live -- --ignored`: a video saved over itself turned once,
    /// then once more; the original goes into the trash folder, the new one has the rotation.
    #[test]
    #[ignore]
    fn video_saved_turned_live() {
        let program = crate::ffmpeg::copier().expect("an FFmpeg");
        let dir = std::env::temp_dir().join(format!("eyecansee-video-saved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("v.mp4");
        let make = ["-hide_banner", "-nostdin", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=1", "-c:v", "libx264"];
        let a: Vec<String> = make.iter().map(|s| s.to_string()).chain([clip.to_string_lossy().into()]).collect();
        crate::ffmpeg::run(&program, &a, |_| {}, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let op = crate::source::Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(replace(&op, "v.mp4", 1, crate::source::Deletion::TrashFolder)).unwrap();
        assert_eq!(turned(read_rotation(&program, &clip).unwrap(), 0), -90);
        assert!(dir.join(crate::trash::TRASH_DIR).join("v.mp4").is_file());
        // Once more: from the rotation it has now, upside down.
        rt.block_on(replace(&op, "v.mp4", 1, crate::source::Deletion::Permanent)).unwrap();
        assert_eq!(turned(read_rotation(&program, &clip).unwrap(), 0), 180);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
