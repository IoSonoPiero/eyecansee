//! New files in the system's temporary folder that only the user can read (#176), and the ones a
//! copy of the app closed by force left there, removed at the next start. Without the interface.

use std::path::{Path, PathBuf};

/// A new file in the temporary folder, `{prefix}{process}-{random}.{ext}`: only the user can read
/// it, its name nobody can work out beforehand, never written through a link or over a file
/// already there (`create_new`).
pub(crate) fn create(prefix: &str, ext: &str) -> std::io::Result<(PathBuf, std::fs::File)> {
    use std::hash::{BuildHasher, Hasher};
    for _ in 0..8 {
        // Keys seeded from the system's random source, new for each `RandomState`.
        let n = std::hash::RandomState::new().build_hasher().finish();
        let path = std::env::temp_dir().join(format!("{prefix}{}-{n:016x}.{ext}", std::process::id()));
        match new_private(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no new name for the temporary copy"))
}

/// A new file only the user can read; an error when the name is taken, even by a link.
pub(crate) fn new_private(path: &Path) -> std::io::Result<std::fs::File> {
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
    open.open(path)
}

/// Whether `name`, in the temporary folder, is one of the app's temporary files: the copies of
/// videos (`video_read`) and of CAD models (`occt`). The test copy has its own names.
fn old(name: &str) -> bool {
    (name.starts_with(&crate::video_read::temp_prefix()) && name.ends_with(".tmp")) || name.starts_with(&crate::occt::temp_prefix())
}

/// At start (after `single::hand_over`, so no other copy of the app is using them): the copies a
/// copy of the app closed by force or crashed left in the temporary folder. Not with
/// `EYECANSEE_MULTI`: another instance may be using them.
pub(crate) fn remove_old_copies() {
    if !crate::single::multi() {
        remove_old_in(&std::env::temp_dir());
    }
}

fn remove_old_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if old(&name) {
            log!("old temporary copy removed: {name}");
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file made for the copy is never one already there: a link under its name is refused,
    /// and the file it points to stays as it was.
    #[test]
    #[cfg(unix)]
    fn new_private_refuses_a_link() {
        let dir = std::env::temp_dir().join(format!("eyecansee-occt-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ours"), b"ours").unwrap();
        std::os::unix::fs::symlink(dir.join("ours"), dir.join("copy.step")).unwrap();
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("dangling.step")).unwrap();
        let link = new_private(&dir.join("copy.step")).map(|_| ()).map_err(|e| e.kind());
        let dangling = new_private(&dir.join("dangling.step")).map(|_| ()).map_err(|e| e.kind());
        let (ours, nowhere) = (std::fs::read(dir.join("ours")).unwrap(), dir.join("nowhere").exists());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(link, Err(std::io::ErrorKind::AlreadyExists));
        assert_eq!(dangling, Err(std::io::ErrorKind::AlreadyExists));
        assert_eq!(ours, b"ours");
        assert!(!nowhere);
    }

    /// The copies left by an earlier run go at start, and nothing else in the folder.
    #[test]
    fn old_copies_removed() {
        let dir = std::env::temp_dir().join(format!("eyecansee-temp-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = [dir.join(format!("{}123-00ff.step", crate::occt::temp_prefix())), dir.join(format!("{}123-4.tmp", crate::video_read::temp_prefix()))];
        let others = [dir.join(format!("{}1-0.mp4", crate::video_read::temp_prefix())), dir.join("model.step"), dir.join("cad-1.step"), dir.join("video-1-0.tmp")];
        for f in others.iter().chain(&old) {
            std::fs::write(f, b"x").unwrap();
        }
        remove_old_in(&dir);
        let (gone, kept) = (old.iter().all(|f| !f.exists()), others.iter().all(|f| f.exists()));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(gone && kept);
    }

    /// #190 (audit 4.15 b): FFmpeg's temporary files (the copy of an original, a new file, a
    /// turned video, the clip of the card's probe) left by a run closed by force were never
    /// removed. They go at start too, and nothing else.
    #[test]
    fn ffmpeg_files_removed_at_start() {
        let dir = std::env::temp_dir().join(format!("eyecansee-temp-ffmpeg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = crate::test_copy::NAME;
        let old = ["in-12-ab.mp4", "out-12-ab.mkv", "turn-in-12-ab.mov", "turn-out-12-ab.mov", "card-12-h264_qsv.mkv"].map(|n| dir.join(format!("{name}-{n}")));
        let others = [dir.join(format!("{name}-inside.mp4")), dir.join("in-12-ab.mp4"), dir.join("clip-out-1.mkv")];
        for f in others.iter().chain(&old) {
            std::fs::write(f, b"x").unwrap();
        }
        remove_old_in(&dir);
        let left: Vec<_> = old.iter().filter(|f| f.exists()).map(|f| f.file_name().unwrap().to_owned()).collect();
        let kept = others.iter().all(|f| f.exists());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(left.is_empty(), "left at start: {left:?}");
        assert!(kept);
    }
}
