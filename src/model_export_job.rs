//! Export as a 3D model while it runs (#312): the models read, written and put in place one after
//! the other in the background, with how far it is in a box in the corner, as a copy's
//! (`file_dialogs::transfer_window`): the model under way (N of M), a bar, and Cancel, which
//! stops the model being written (its file removed) and those still to come. Its Cancel is
//! reached with F6 and Tab as the other boxes' (`box_keys`).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::App;
use crate::model_export::Kind;
use crate::source::Store;

/// The detail of a model not exported because Cancel was pressed: not said as a failure.
pub(crate) const CANCELLED: &str = "\u{1}cancelled";

/// Where the model under way is, in thousandths of its part of the bar: read, then turned into
/// the new format (`model_export::export_file`), then written.
const READ: f32 = 0.05;
const BUILT: f32 = 0.7;

/// The pieces a new file is written in: Cancel is seen between two of them.
const CHUNK: usize = if cfg!(test) { 4096 } else { 8 << 20 };

/// What an export did with one model: its new file, the original and what to say of the new
/// file; or the original and why not.
pub(crate) type Exported = Result<(String, String, Vec<&'static str>), (String, String)>;

/// How far an export is: the models done of all, the one under way and how far it is, and Cancel.
#[derive(Default)]
pub(crate) struct Progress {
    pub(crate) total: AtomicUsize,
    pub(crate) done: AtomicUsize,
    /// Of the model under way, in thousandths.
    pub(crate) part: AtomicU32,
    pub(crate) current: Mutex<String>,
    pub(crate) cancel: AtomicBool,
    /// Called after each piece written: the tests stop an export there.
    #[cfg(test)]
    pub(crate) on_chunk: Mutex<Option<OnChunk>>,
}

#[cfg(test)]
pub(crate) type OnChunk = Box<dyn Fn(&Progress) + Send>;

impl Progress {
    fn stopped(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// The model under way at `f` (0 to 1) of the stretch from `from` to `to` of its part.
    fn at(&self, from: f32, to: f32, f: f32) {
        self.part.store(((from + (to - from) * f.clamp(0.0, 1.0)) * 1000.0) as u32, Ordering::Relaxed);
    }

    /// The whole export, 0 to 1, for the bar.
    pub(crate) fn fraction(&self) -> f32 {
        let (done, total) = (self.done.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed).max(1));
        ((done as f32 + self.part.load(Ordering::Relaxed) as f32 / 1000.0) / total as f32).min(1.0)
    }
}

/// One export at a time: a second one waits for the first, so the box shows one model at a time.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The models `jobs` (original, new file) exported as `kind`, one after the other, each told to
/// `send` once done, also those Cancel left out (`CANCELLED`).
pub(crate) async fn run(op: Store, jobs: Vec<(String, String)>, kind: Kind, p: Arc<Progress>, send: impl Fn(Exported)) {
    let _turn = ONE_AT_A_TIME.lock().await;
    for (path, to) in jobs {
        // Asked of the folder too: the grid may not have read it yet, and a file of that name
        // (the original itself, exported into its own format) is never written over.
        let to = free_in_folder(&op, to).await;
        if !p.stopped() {
            *p.current.lock().unwrap() = crate::source::name(&path).to_string();
            p.part.store(0, Ordering::Relaxed);
        }
        let r = if p.stopped() { Err(CANCELLED.to_string()) } else { one(&op, &path, &to, kind, &p).await };
        p.done.fetch_add(1, Ordering::Relaxed);
        p.part.store(0, Ordering::Relaxed);
        send(r.map(|notes| (to, path.clone(), notes)).map_err(|e| (path, e)));
    }
}

/// One model read, turned into `kind` and written as `to`, with the files that go next to it
/// (an OBJ's materials and pictures) before it; what to say of the new file.
async fn one(op: &Store, path: &str, to: &str, kind: Kind, p: &Arc<Progress>) -> Result<Vec<&'static str>, String> {
    let bytes = op.read(path).await.map_err(|(_, detail)| detail)?;
    p.at(0.0, READ, 1.0);
    let (sibling, local, name, q) = (crate::loader::siblings(op.clone(), path.to_string()), crate::occt::local(op, path), path.to_string(), p.clone());
    let free = free_names(op.clone(), to);
    let stem = crate::source::name(to).rsplit_once('.').map_or(crate::source::name(to), |(s, _)| s).to_string();
    let made = tokio::task::spawn_blocking(move || {
        let tick = |f: f32| {
            q.at(READ, BUILT, f);
            !q.stopped()
        };
        let beside = crate::model_export::Beside { stem: &stem, free: &free };
        crate::occt::from_file(local, || crate::model_export::export_file(&bytes, &name, &sibling, kind, &tick, &beside))
    });
    let (made, notes) = made.await.unwrap_or_else(|e| Err(e.to_string()))?;
    let dir = crate::source::parent(to);
    let total = (made.beside.iter().map(|(_, d)| d.len()).sum::<usize>() + made.main.len()).max(1) as f32;
    let (mut at, mut written) = (0, Vec::<String>::new());
    let files = made.beside.into_iter().map(|(name, data)| (format!("{dir}{name}"), data)).chain([(to.to_string(), made.main)]);
    for (path, data) in files {
        let span = (BUILT + (1.0 - BUILT) * at as f32 / total, BUILT + (1.0 - BUILT) * (at + data.len()) as f32 / total);
        at += data.len();
        if let Err(e) = write(op, &path, data, p, span).await {
            // Nothing of a model half there: the files next to it written so far go too.
            for path in written {
                let gone = op.delete(&path, crate::source::Deletion::Permanent).await;
                log!("export: {path:?} removed after {e:?}: {gone:?}");
            }
            return Err(e);
        }
        written.push(path);
    }
    Ok(notes)
}

/// `to`, or the next free name in its folder when a file has that name.
async fn free_in_folder(op: &Store, to: String) -> String {
    let dir = crate::source::parent(&to);
    let mut given = std::collections::HashSet::new();
    let mut name = crate::source::name(&to).to_string();
    while op.exists(&format!("{dir}{name}")).await {
        given.insert(name.clone());
        name = crate::source::free_name(&given, crate::source::name(&to));
    }
    format!("{dir}{name}")
}

/// Free names in the folder of `to` for the files next to it, asked of the folder itself: a
/// file already there is never written over (the user's `a.png` next to `a.glb`), nor `to`,
/// nor a name given before.
fn free_names(op: Store, to: &str) -> impl Fn(&str) -> String + Send + 'static {
    let rt = tokio::runtime::Handle::current();
    let dir = crate::source::parent(to);
    let given = Mutex::new(std::collections::HashSet::from([crate::source::name(to).to_string()]));
    move |wanted| {
        let mut given = given.lock().unwrap();
        loop {
            let name = crate::source::free_name(&given, wanted);
            given.insert(name.clone());
            if !rt.block_on(op.exists(&format!("{dir}{name}"))) {
                return name;
            }
        }
    }
}

/// `data` written as `path` in pieces, telling how far along `span` of the model's part; at
/// Cancel the file is removed, as a write that fails (`replace::write_file`), and the error is
/// `CANCELLED`.
async fn write(op: &Store, to: &str, data: Vec<u8>, p: &Arc<Progress>, span: (f32, f32)) -> Result<(), String> {
    let size = data.len() as u64;
    let data = bytes::Bytes::from(data);
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let feed = async move {
        let mut at = 0;
        while at < data.len() {
            if p.stopped() {
                let _ = tx.send(Err(std::io::Error::other("cancelled"))).await;
                return true;
            }
            let end = (at + CHUNK).min(data.len());
            if tx.send(Ok(data.slice(at..end))).await.is_err() {
                return false;
            }
            at = end;
            p.at(span.0, span.1, at as f32 / data.len() as f32);
            #[cfg(test)]
            if let Some(f) = &*p.on_chunk.lock().unwrap() {
                f(p);
            }
        }
        false
    };
    let (written, cancelled) = tokio::join!(crate::replace::write_file(op, to, size, rx, None), feed);
    match written {
        Err(_) if cancelled => Err(CANCELLED.to_string()),
        Err((_, detail)) => Err(detail),
        Ok(()) => Ok(()),
    }
}

impl App {
    /// The box of an export under way, in the bottom right corner above the others
    /// (`box_keys::offset`): the model under way, N of M, the bar and Cancel.
    pub(crate) fn export_window(&mut self, ctx: &egui::Context) {
        let Some(p) = self.export_said.progress() else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let (done, total) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed));
        let counts = self.i18n.t_args("model-export-counts", &[("n", &(done + 1).min(total).to_string()), ("total", &total.to_string())]);
        let current = p.current.lock().unwrap().clone();
        let stopping = p.stopped();
        let shown = egui::Window::new(self.i18n.t("model-export-running"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, crate::box_keys::offset(ctx, "export"))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.add(egui::Label::new(egui::RichText::new(current).strong()).truncate());
                ui.label(counts);
                ui.scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = ui.visuals().widgets.inactive.bg_fill;
                    let fill = crate::theme::accent(ui.visuals());
                    ui.add(egui::ProgressBar::new(p.fraction()).fill(fill).desired_height(16.0).show_percentage());
                });
                if stopping {
                    ui.label(self.i18n.t("batch-stopping"));
                } else {
                    // Reached with F6 and Tab, as Cancel of a copy (`box_keys`).
                    let cancel = ui.button(self.i18n.t("delete-no"));
                    crate::box_keys::mark(&cancel);
                    if cancel.clicked() {
                        log!("export: Cancel pressed, {done} of {total} done");
                        p.cancel.store(true, Ordering::Relaxed);
                    }
                }
            });
        if let Some(r) = shown {
            crate::box_keys::drawn(ctx, "export", r.response.rect.height());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A binary STL of `n` triangles in a row.
    fn stl(n: usize) -> Vec<u8> {
        let mut out = vec![0u8; 80];
        out.extend((n as u32).to_le_bytes());
        for i in 0..n {
            let x = i as f32;
            for f in [0.0, 0.0, 1.0, x, 0.0, 0.0, x + 1.0, 0.0, 0.0, x, 1.0, 0.0] {
                out.extend(f32::to_le_bytes(f));
            }
            out.extend([0, 0]);
        }
        out
    }

    /// The export of `names` (STL, in a folder of their own) as OBJ, with `on_chunk` called after
    /// each piece written: what each model gave, the files left in the folder, the progress.
    fn export(folder: &str, names: &[&str], on_chunk: impl Fn(&Progress) + Send + 'static) -> (Vec<Exported>, Vec<String>, Arc<Progress>) {
        let files = names.iter().map(|n| (format!("{n}.stl"), stl(300))).collect();
        let jobs = names.iter().map(|n| (format!("{n}.stl"), format!("{n}.obj"))).collect();
        export_in(folder, files, jobs, on_chunk)
    }

    /// As `export`, from the folder with `files` in it, the models `jobs` (original, new file).
    fn export_in(folder: &str, files: Vec<(String, Vec<u8>)>, jobs: Vec<(String, String)>, on_chunk: impl Fn(&Progress) + Send + 'static) -> (Vec<Exported>, Vec<String>, Arc<Progress>) {
        let dir = std::env::temp_dir().join(format!("eyecansee-export-job-{}-{folder}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, data) in files {
            std::fs::write(dir.join(name), data).unwrap();
        }
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let p = Arc::new(Progress::default());
        p.total.store(jobs.len(), Ordering::Relaxed);
        *p.on_chunk.lock().unwrap() = Some(Box::new(on_chunk));
        let got = Mutex::new(Vec::new());
        tokio::runtime::Runtime::new().unwrap().block_on(run(op, jobs, Kind::Obj, p.clone(), |r| got.lock().unwrap().push(r)));
        let mut files: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|f| f.file_name().to_string_lossy().to_string()).collect();
        files.sort();
        std::fs::remove_dir_all(&dir).unwrap();
        (got.into_inner().unwrap(), files, p)
    }

    /// #312: the progress of an export of three models goes from the first to the last, each
    /// written in more than one piece with the bar going forward, and ends with all done.
    #[test]
    fn progress_from_the_first_to_the_last() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let (got, files, p) = export("progress", &["a", "b", "c"], move |p| log.lock().unwrap().push((p.current.lock().unwrap().clone(), p.fraction())));
        assert!(got.iter().all(Result::is_ok), "{got:?}");
        assert_eq!(files, ["a.obj", "a.stl", "b.obj", "b.stl", "c.obj", "c.stl"]);
        let seen = seen.lock().unwrap();
        let mut names: Vec<&str> = seen.iter().map(|(n, _)| n.as_str()).collect();
        names.dedup();
        assert_eq!(names, ["a.stl", "b.stl", "c.stl"]);
        assert!(seen.len() > 6, "each model in more than one piece: {}", seen.len());
        assert!(seen.windows(2).all(|w| w[1].1 >= w[0].1), "the bar went back: {seen:?}");
        assert_eq!((p.done.load(Ordering::Relaxed), p.fraction()), (3, 1.0));
    }

    /// #312: Cancel while the second of three models is written: its file is removed (nothing
    /// half written, nothing hidden left), the first stays, the third is not exported; both said
    /// as cancelled, not as failures.
    #[test]
    fn cancel_removes_the_file_being_written() {
        let (got, files, p) = export("cancel", &["a", "b", "c"], |p| {
            if p.done.load(Ordering::Relaxed) == 1 {
                p.cancel.store(true, Ordering::Relaxed);
            }
        });
        assert_eq!(files, ["a.obj", "a.stl", "b.stl", "c.stl"]);
        assert!(got[0].is_ok(), "{got:?}");
        assert_eq!(got[1..].iter().map(|r| r.as_ref().map_err(|(from, e)| (from.as_str(), e.as_str()))).collect::<Vec<_>>(), [Err(("b.stl", CANCELLED)), Err(("c.stl", CANCELLED))]);
        assert_eq!(p.done.load(Ordering::Relaxed), 3);
    }

    /// Audit 6.16: a model with a picture exported as OBJ in a folder that has a picture named
    /// as the model already: the user's picture stays as it was, the model's goes next to it with
    /// a free name, with the `.mtl`. Cancel while the OBJ is written removes it and the files
    /// written next to it.
    #[test]
    fn obj_files_beside_free_and_removed_at_cancel() {
        let glb = crate::model_export::export(&crate::model_export::tests::pictured(), Kind::Glb);
        let mine = b"the user's picture".to_vec();
        let files = || vec![("a.glb".to_string(), glb.clone()), ("a.png".to_string(), mine.clone())];
        let jobs = || vec![("a.glb".to_string(), "a.obj".to_string())];
        let (got, names, _) = export_in("beside", files(), jobs(), |_| {});
        assert!(got[0].is_ok(), "{got:?}");
        assert_eq!(names, ["a (2).png", "a.glb", "a.mtl", "a.obj", "a.png"]);
        // Stopped after the picture and the `.mtl` (a piece each), before the OBJ.
        let pieces = AtomicUsize::new(0);
        let (got, names, _) = export_in("beside-cancel", files(), jobs(), move |p| {
            if pieces.fetch_add(1, Ordering::Relaxed) + 1 == 2 {
                p.cancel.store(true, Ordering::Relaxed);
            }
        });
        assert_eq!(names, ["a.glb", "a.png"], "{got:?}");
    }

    /// #312: a new name already taken in the folder (the grid had not read it yet: the PLY
    /// exported as PLY at once wrote over its original) is never written over: the next free
    /// name, which the end says.
    #[test]
    fn a_name_taken_is_never_written_over() {
        let files = vec![("a.stl".to_string(), stl(10)), ("a.obj".to_string(), b"mine".to_vec())];
        let (got, names, _) = export_in("taken", files, vec![("a.stl".to_string(), "a.obj".to_string())], |_| {});
        assert_eq!(names, ["a (2).obj", "a.obj", "a.stl"], "{got:?}");
        assert!(matches!(&got[0], Ok((to, _, _)) if to == "a (2).obj"), "{got:?}");
    }
}
