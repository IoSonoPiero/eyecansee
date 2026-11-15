//! The files FFmpeg works on, all on this computer: a video or an audio file of a phone or a
//! server is copied into a temporary file first, the new files are written into temporary files
//! and sent there at the end, and every temporary file is removed, also after an error or Cancel.
//! Shared by the conversion of videos (`batch::video`), the audio jobs (`audio_job`) and the
//! conversion of audio files (`audio_convert`). Without the interface.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::batch::{Phases, Progress, Writing, download, keep_thumbnail, upload};
use crate::source::{self, Store};

/// A temporary file of this process: `eyecansee-in-<process>-<name>` for the copy of an
/// original, `eyecansee-out-…` for a new file.
pub(crate) fn temp(what: &str, name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("eyecansee-{what}-{}-{name}", std::process::id()))
}

/// The error of a file of the job: its name and what went wrong.
pub(crate) fn failed(name: &str, e: String) -> source::Error {
    ("batch-failed", format!("{name}: {e}"))
}

/// The original as a local file for FFmpeg; a temporary copy is removed when this is dropped.
pub(crate) struct Input {
    /// The original's name.
    pub(crate) name: String,
    pub(crate) file: PathBuf,
    /// `file` is a temporary copy of a file of a phone or a server.
    copy: bool,
    /// The file's bar: the download, ffmpeg and the upload, each where it is needed.
    pub(crate) phases: Phases,
}

impl Input {
    /// `path` of `op`, copied into a temporary file where it is not on this computer; the bar
    /// has a part for the upload where the folder `dest` of `out` is not on this computer either.
    pub(crate) async fn fetch(op: &Store, path: &str, (out, dest): (&Store, &str), p: &Arc<Progress>) -> Result<Input, source::Error> {
        let name = source::name(path).to_string();
        // Download, ffmpeg and upload share the file's bar in equal parts.
        // ponytail: equal parts, not by time: a "copy" conversion jumps through its third.
        let (remote_in, remote_out) = (op.local_file(path).is_none(), out.local_file(&format!("{dest}{name}")).is_none());
        let phases = Phases { p: p.clone(), count: 1 + remote_in as u32 + remote_out as u32 };
        let (file, copy) = match op.local_file(path) {
            Some(file) => (file, false),
            None => {
                let file = temp("in", &name);
                download(op, path, &file, || phases.p.cancel.load(Ordering::Relaxed), |f| phases.set(0, f)).await.map_err(|e| failed(&name, e.to_string()))?;
                (file, true)
            }
        };
        Ok(Input { name, file, copy, phases })
    }

    /// Cancel was pressed.
    pub(crate) fn cancelled(&self) -> bool {
        self.phases.p.cancel.load(Ordering::Relaxed)
    }

    /// `program` (an FFmpeg) run with `args`, written in the log, on the bar after the
    /// download, stopped by Cancel.
    pub(crate) async fn ffmpeg(&self, program: &Path, args: Vec<String>) -> Result<(), String> {
        log!("ffmpeg {args:?}");
        let (program, ph, first) = (program.to_path_buf(), self.phases.clone(), self.copy as u32);
        tokio::task::spawn_blocking(move || crate::ffmpeg::run(&program, &args, |f| ph.set(first, f), &ph.p.cancel)).await.unwrap_or_else(|e| Err(e.to_string()))
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if self.copy {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

/// A new file: written by FFmpeg into `file`, marked as being written (`Writing`) until dropped.
pub(crate) struct Output {
    /// Its path in the destination.
    pub(crate) to: String,
    pub(crate) file: PathBuf,
    /// `file` is a temporary file, sent to `to` at the end.
    remote: bool,
    _writing: Writing,
}

impl Output {
    /// The new file `to` of `out`: the file itself on this computer, otherwise a temporary file
    /// named after `temp_name`.
    pub(crate) fn new(out: &Store, to: String, temp_name: &str) -> Output {
        let (file, remote) = out.local_file(&to).map_or_else(|| (temp("out", temp_name), true), |f| (f, false));
        let _writing = Writing::start(file.clone());
        Output { to, file, remote, _writing }
    }
}

/// The one new file of a conversion after ffmpeg: its thumbnail kept, then `send`.
pub(crate) async fn deliver<E>(out: &Store, output: &Output, phases: &Phases, r: Result<(), E>, failed: impl Fn(String) -> E) -> Result<(), E> {
    if r.is_ok() {
        keep_thumbnail(&output.to, &output.file).await;
    }
    send(out, std::slice::from_ref(output), phases, r, failed).await
}

/// The new files sent to their phone or server while `r` is good (Cancel stops an upload),
/// `failed` making the error of an upload; then the temporary ones removed, and every one after
/// an error: nothing half written stays.
pub(crate) async fn send<E>(out: &Store, outputs: &[Output], phases: &Phases, mut r: Result<(), E>, failed: impl Fn(String) -> E) -> Result<(), E> {
    for o in outputs {
        if r.is_ok() && o.remote {
            let (ph, last, p) = (phases.clone(), phases.count - 1, phases.p.clone());
            r = upload(out, &o.to, &o.file, move || p.cancel.load(Ordering::Relaxed), move |f| ph.set(last, f)).await.map_err(|e| failed(e.to_string()));
        }
    }
    for o in outputs {
        if o.remote || r.is_err() {
            let _ = std::fs::remove_file(&o.file);
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(dir: &Path) -> Store {
        Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap())
    }

    /// A store that is not on this computer, like a phone or a server.
    fn remote() -> Store {
        Store::Dal(opendal::Operator::new(opendal::services::Memory::default()).unwrap())
    }

    /// A file put into `store` for the test.
    async fn put(store: &Store, path: &str, data: &'static [u8]) {
        let Store::Dal(op) = store else { unreachable!() };
        op.write(path, data).await.unwrap();
    }

    fn progress() -> Arc<Progress> {
        Arc::new(Progress::default())
    }

    /// ffmpeg's work, done here: the input copied into the output.
    fn convert(input: &Input, output: &Output) -> Result<(), String> {
        std::fs::copy(&input.file, &output.file).map(|_| ()).map_err(|e| e.to_string())
    }

    #[test]
    fn local_files_used_as_they_are() {
        let dir = std::env::temp_dir().join(format!("eyecansee-ffmpeg-files-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(dir.join("a.mp4"), b"video").unwrap();
        let op = local(&dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let input = Input::fetch(&op, "a.mp4", (&op, "out/"), &progress()).await.unwrap();
            assert_eq!((input.name.as_str(), input.phases.count), ("a.mp4", 1));
            assert_eq!(input.file.canonicalize().unwrap(), dir.join("a.mp4").canonicalize().unwrap());
            let output = Output::new(&op, "out/a.mkv".into(), "a.mp4");
            assert!(!output.remote && crate::batch::writing(&dir.join("out").join("a.mkv")));
            convert(&input, &output).unwrap();
            send(&op, std::slice::from_ref(&output), &input.phases, Ok::<(), String>(()), |e| e).await.unwrap();
            // Failed: what ffmpeg wrote is removed.
            let half = Output::new(&op, "out/b.mkv".into(), "a.mp4");
            std::fs::write(&half.file, b"half").unwrap();
            let r = send(&op, std::slice::from_ref(&half), &input.phases, Err("ffmpeg failed".to_string()), |e| e).await;
            assert_eq!(r, Err("ffmpeg failed".into()));
            assert!(!half.file.exists());
            drop(half);
            drop(output);
            assert!(!crate::batch::writing(&dir.join("out").join("a.mkv")));
        });
        // The original and the new file stay.
        assert_eq!(std::fs::read(dir.join("a.mp4")).unwrap(), b"video");
        assert_eq!(std::fs::read(dir.join("out").join("a.mkv")).unwrap(), b"video");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remote_files_through_temporary_ones() {
        let (op, out) = (remote(), remote());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            put(&op, "v/clip-remote.mp4", b"the video").await;
            let input = Input::fetch(&op, "v/clip-remote.mp4", (&out, "dest/"), &progress()).await.unwrap();
            assert_eq!(input.file, temp("in", "clip-remote.mp4"));
            assert_eq!((std::fs::read(&input.file).unwrap(), input.phases.count), (b"the video".to_vec(), 3));
            let output = Output::new(&out, "dest/clip-remote.mkv".into(), "clip-remote.mp4");
            assert_eq!((&output.file, output.remote), (&temp("out", "clip-remote.mp4"), true));
            convert(&input, &output).unwrap();
            let phases = input.phases.clone();
            drop(input);
            assert!(!temp("in", "clip-remote.mp4").exists());
            send(&out, std::slice::from_ref(&output), &phases, Ok::<(), String>(()), |e| e).await.unwrap();
            assert!(!output.file.exists());
            assert_eq!(out.read("dest/clip-remote.mkv").await.unwrap(), b"the video");
            assert_eq!(phases.p.part.load(Ordering::Relaxed), 1000);
        });
    }

    #[test]
    fn errors_and_cancel_leave_nothing() {
        let (op, out) = (remote(), remote());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            put(&op, "clip-cancel.mp4", b"the video").await;
            let p = progress();
            let input = Input::fetch(&op, "clip-cancel.mp4", (&out, ""), &p).await.unwrap();
            // ffmpeg failed: nothing sent, the temporary file removed.
            let output = Output::new(&out, "a.mkv".into(), "clip-cancel.mp4");
            convert(&input, &output).unwrap();
            let r = send(&out, std::slice::from_ref(&output), &input.phases, Err(failed("clip-cancel.mp4", "no".into())), |e| failed("x", e)).await;
            assert_eq!(r, Err(("batch-failed", "clip-cancel.mp4: no".into())));
            assert!(!output.file.exists() && !out.exists("a.mkv").await);
            // Cancel before the upload: stopped, nothing half written there or here.
            let output = Output::new(&out, "b.mkv".into(), "clip-cancel.mp4");
            convert(&input, &output).unwrap();
            p.cancel.store(true, Ordering::Relaxed);
            let r = send(&out, std::slice::from_ref(&output), &input.phases, Ok(()), |e| failed("clip-cancel.mp4", e)).await;
            assert_eq!(r, Err(("batch-failed", "clip-cancel.mp4: cancelled".into())));
            assert!(!output.file.exists() && !out.exists("b.mkv").await);
            // Cancel during the download: the half copy removed.
            assert!(Input::fetch(&op, "clip-cancel.mp4", (&out, ""), &p).await.is_err());
            drop(input);
            assert!(!temp("in", "clip-cancel.mp4").exists());
        });
    }

    /// The "command" run as ffmpeg is: `cp` in place of FFmpeg, and `false` for one that fails.
    #[cfg(unix)]
    #[test]
    fn the_command_run() {
        let dir = std::env::temp_dir().join(format!("eyecansee-ffmpeg-command-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.wav"), b"sound").unwrap();
        let op = local(&dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let input = Input::fetch(&op, "a.wav", (&op, ""), &progress()).await.unwrap();
            let output = Output::new(&op, "b.flac".into(), "a.wav");
            let args = vec![input.file.to_string_lossy().into_owned(), output.file.to_string_lossy().into_owned()];
            input.ffmpeg(Path::new("cp"), args).await.unwrap();
            assert!(input.ffmpeg(Path::new("false"), Vec::new()).await.is_err());
            input.phases.p.cancel.store(true, Ordering::Relaxed);
            assert_eq!(input.ffmpeg(Path::new("true"), Vec::new()).await, Err("cancelled".into()));
        });
        assert_eq!(std::fs::read(dir.join("b.flac")).unwrap(), b"sound");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
