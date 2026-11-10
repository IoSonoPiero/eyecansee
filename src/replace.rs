//! Writing a file that leaves nothing half written at a failure (#90), and saving an image over
//! its original safely: the new file under a hidden name, the original put away as asked, the new
//! one renamed into place, everything put back at any failure. Split from `loader.rs` on 2026-11-01.

use crate::loader::SKIPPED;
use crate::source::{self, Store};

/// Writes a file whole: as `write_file`, nothing half written left at a failure.
pub(crate) async fn write_whole(op: &Store, path: &str, data: Vec<u8>) -> Result<(), source::Error> {
    let size = data.len() as u64;
    write_file(op, path, size, whole(data), None).await
}

/// The chunks of `data`, all of it in one.
fn whole(data: Vec<u8>) -> Chunks {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let _ = tx.try_send(Ok(bytes::Bytes::from(data)));
    rx
}

type Chunks = tokio::sync::mpsc::Receiver<std::io::Result<bytes::Bytes>>;

/// Writes a file of `size` bytes from `chunks` as `path`, leaving nothing half written at a
/// failure (issue #90): a new file is deleted (`Store::write_chunks`); a file already there keeps
/// its content until the new one is whole, written under its hidden name and then put in its
/// place as `replace` does, the old one deleted. `existed`: whether `path` is already there, when
/// the caller knows (`None`: asked). Where renaming is not possible or not safe (gvfs, see
/// `replace`) the new file is written over the old one as before.
pub(crate) async fn write_file(op: &Store, path: &str, size: u64, chunks: Chunks, existed: Option<bool>) -> Result<(), source::Error> {
    let existed = match existed {
        Some(e) => e,
        None => op.exists(path).await,
    };
    if !existed || !op.can_rename() || op.gvfs(path) {
        return op.write_chunks(path, size, chunks).await;
    }
    source::SAVING.lock().unwrap().insert(path.to_string());
    let r = swap(op, path, |new| async move { op.write_chunks(&new, size, chunks).await }, source::Deletion::Permanent, false).await;
    source::SAVING.lock().unwrap().remove(path);
    r
}

/// The image read and turned in its own format, and whether it lost quality.
pub(crate) async fn turned(op: &Store, path: &str, turn: crate::transform::Turn) -> Result<(Vec<u8>, bool), source::Error> {
    rewritten(op, path, move |bytes, name| crate::save_back::turned_file(bytes, name, turn)).await
}

/// The file read and passed to `make` (in the background), which gives the file to write and
/// whether it lost quality; `SKIPPED` if it cannot be written back over itself. An Ultra HDR
/// photo passes when `ultra_hdr`: only flips and quarter turns know how to keep its gain map.
pub(crate) async fn rewritten<T: Send + 'static>(op: &Store, path: &str, make: impl FnOnce(&[u8], &str) -> Result<T, String> + Send + 'static) -> Result<T, source::Error> {
    let bytes = op.read(path).await?;
    let name = source::name(path).to_string();
    tokio::task::spawn_blocking(move || {
        if !crate::save_back::can_turn_in_place(&bytes, &name) && !crate::ultrahdr::is(&bytes) {
            return Err((SKIPPED, name));
        }
        make(&bytes, &name).map_err(|e| ("error-detail", e))
    })
    .await
    .map_err(|e| ("error-detail", e.to_string()))?
}

/// Writes `data` over the image `path`; the original goes as `how` says, under its own name.
/// Nothing is written where renaming is not possible. At every failure, and if the app closes
/// half way (`Store::recover`), the image keeps a visible file under its name: the original, put
/// back from its hidden copy if it had already left.
/// Never under gvfs (issue #45): for seconds after a rename gvfs-mtp still answers that the old
/// name exists, and a rename onto that name can delete the file that left it (seen with test files
/// on a phone, 2026-11-04). The app offers "Save as…" there instead (`Loader::no_overwrite`).
pub(crate) async fn replace(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), source::Error> {
    replace_as(op, path, data, how, false).await
}

/// As `replace`, for the editor's "Overwrite the original": the original must reach its trash.
/// When it cannot go into the `.EyeCanSee-trash` folder (it cannot be made, no permission), the
/// new image leaves its name again and the original gets it back: nothing changed, and the error
/// says why (`replace` leaves it hidden until the next save instead).
pub(crate) async fn replace_keeping(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), source::Error> {
    replace_as(op, path, data, how, true).await
}

async fn replace_as(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion, strict: bool) -> Result<(), source::Error> {
    if !op.can_rename() {
        return Err(("error-rename-unsupported", path.to_string()));
    }
    if op.gvfs(path) {
        return Err(("error-save-gvfs", path.to_string()));
    }
    // Issue #47: the change would stay in a copy Windows can delete, not reach the phone.
    if op.temp_copy(path) {
        return Err(("error-save-temp-copy", path.to_string()));
    }
    source::SAVING.lock().unwrap().insert(path.to_string());
    let size = data.len() as u64;
    let r = swap(op, path, |new| async move { op.write_chunks(&new, size, whole(data)).await }, how, strict).await;
    source::SAVING.lock().unwrap().remove(path);
    r
}

/// The new file written by `write` (given its hidden name) and put in the place of the original,
/// which goes as `how` says.
async fn swap<W: Future<Output = Result<(), source::Error>>>(op: &Store, path: &str, write: impl FnOnce(String) -> W, how: source::Deletion, strict: bool) -> Result<(), source::Error> {
    let own = source::name(path);
    let (new, old) = source::save_names(path);
    // An original left by an earlier save cut short after its new version took the name: it goes
    // where that save would have sent it.
    if op.names(&source::parent(path)).await?.contains(source::name(&old)) {
        put_away(op, &old, own, how).await?;
    }
    // A failed write leaves nothing (`Store::write_chunks`), the original untouched.
    write(new.clone()).await?;
    // The original leaves its name and waits as `old`. To the system trash it goes now with its
    // own name (restoring it from there gives it back), `old` being a second name of it.
    let left = match how {
        source::Deletion::SystemTrash => match op.link_or_copy(path, &old).await {
            Ok(()) => op.delete(path, how).await.map_err(|e| if strict { trash_failed(e) } else { e }),
            Err(e) => Err(e),
        },
        _ => op.rename(path, source::name(&old)).await.map(|_| ()),
    };
    let named = match left {
        Ok(()) => op.rename(&new, own).await.map(|_| ()),
        Err(e) => Err(e),
    };
    if let Err(e) = named {
        undo(op, path, &new, &old).await;
        return Err(e);
    }
    // The new image is in place: if the original cannot go, it stays hidden until the next save,
    // or (`strict`) everything goes back as it was.
    if let Err(e) = put_away(op, &old, own, how).await {
        log!("save {path:?}: original kept as {old:?}: {e:?}");
        if strict && how == source::Deletion::TrashFolder {
            back(op, path, &new, &old).await;
            return Err(trash_failed(e));
        }
    }
    Ok(())
}

/// The original could not go to the trash: said plainly, what went wrong (id and detail) at the end.
fn trash_failed(e: source::Error) -> source::Error {
    ("error-trash-failed", format!("{}: {}", e.0, e.1))
}

/// The new image, already under the original's name, leaves it again; the original, waiting as
/// `old`, gets it back; the new image goes.
async fn back(op: &Store, path: &str, new: &str, old: &str) {
    let r = async {
        op.rename(path, source::name(new)).await?;
        op.rename(old, source::name(path)).await?;
        op.delete(new, source::Deletion::Permanent).await
    }
    .await;
    log!("save {path:?}: put back as it was: {r:?}");
}

/// After a failed step of `swap`: the original back under its name if that is free (never over
/// another file), the new file gone.
pub(crate) async fn undo(op: &Store, path: &str, new: &str, old: &str) {
    let names = op.names(&source::parent(path)).await.unwrap_or_default();
    if names.contains(source::name(old)) && !names.contains(source::name(path)) {
        let r = op.rename(old, source::name(path)).await;
        log!("save {path:?}: original back: {r:?}");
    }
    if names.contains(source::name(new)) {
        let _ = op.delete(new, source::Deletion::Permanent).await;
    }
}

/// The original, waiting as `old`, goes as `how` says, under its own name `own`. With the system
/// trash it is already there and `old` is only a second name.
async fn put_away(op: &Store, old: &str, own: &str, how: source::Deletion) -> Result<(), source::Error> {
    match how {
        source::Deletion::TrashFolder => op.to_trash_folder(old, own).await,
        _ => op.delete(old, source::Deletion::Permanent).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Deletion;

    /// The editor's overwrite: with a trash folder that cannot be made (a file has its name) the
    /// original stays as it was, under its name, and no hidden file is left; `replace` instead
    /// writes over it and leaves the original hidden. With a trash that works, the original goes
    /// there and the new image takes its name.
    #[test]
    fn overwrite_only_with_the_original_in_its_trash() {
        let dir = std::env::temp_dir().join(format!("eyecansee-keeping-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let blocked = dir.join("blocked");
        std::fs::create_dir_all(&blocked).unwrap();
        std::fs::write(blocked.join("a.jpg"), b"original").unwrap();
        std::fs::write(blocked.join(crate::trash::TRASH_DIR), b"a file, not a folder").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let names = |sub: &str| {
            let mut v: Vec<String> = std::fs::read_dir(dir.join(sub)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        };
        let e = rt.block_on(replace_keeping(&op, "blocked/a.jpg", b"new".to_vec(), Deletion::TrashFolder)).unwrap_err();
        assert_eq!(e.0, "error-trash-failed", "{e:?}");
        assert_eq!(std::fs::read(blocked.join("a.jpg")).unwrap(), b"original");
        assert_eq!(names("blocked"), [crate::trash::TRASH_DIR, "a.jpg"]);
        // `replace` goes on: the new image in place, the original left hidden.
        rt.block_on(replace(&op, "blocked/a.jpg", b"new".to_vec(), Deletion::TrashFolder)).unwrap();
        assert_eq!(std::fs::read(blocked.join("a.jpg")).unwrap(), b"new");
        assert!(names("blocked").iter().any(|n| n.ends_with(".eyecansee-old")), "{:?}", names("blocked"));
        // A trash that can be made: the original goes there.
        std::fs::create_dir_all(dir.join("free")).unwrap();
        std::fs::write(dir.join("free/a.jpg"), b"original").unwrap();
        rt.block_on(replace_keeping(&op, "free/a.jpg", b"new".to_vec(), Deletion::TrashFolder)).unwrap();
        assert_eq!(std::fs::read(dir.join("free/a.jpg")).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join("free").join(crate::trash::TRASH_DIR).join("a.jpg")).unwrap(), b"original");
        assert_eq!(names("free"), [crate::trash::TRASH_DIR, "a.jpg"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A write that fails halfway leaves nothing (#90): a new file is gone, a file it was replacing
    /// stays as it was, and no hidden file is left. The failure comes from the chunks (the reader
    /// stopped) and, on Linux, from the disk (`/dev/full` behind the name being written, as a full
    /// disk answers).
    #[test]
    fn failed_write_leaves_nothing() {
        let dir = std::env::temp_dir().join(format!("eyecansee-failed-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jpg"), b"original").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let names = || {
            let mut v: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
            v.sort();
            v
        };
        let broken = || {
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            tx.try_send(Ok(bytes::Bytes::from(vec![1u8; 4096]))).unwrap();
            tx.try_send(Err(std::io::Error::other("the reader stopped"))).unwrap();
            rx
        };
        for path in ["new.jpg", "a.jpg"] {
            for existed in [None, Some(path == "a.jpg")] {
                let e = rt.block_on(write_file(&op, path, 8192, broken(), existed)).unwrap_err();
                assert_eq!(e.1, "the reader stopped");
                assert_eq!(names(), ["a.jpg"], "{path} {existed:?}");
                assert_eq!(std::fs::read(dir.join("a.jpg")).unwrap(), b"original");
            }
        }
        // A whole write over a file replaces it, nothing hidden left.
        rt.block_on(write_whole(&op, "a.jpg", b"second".to_vec())).unwrap();
        assert_eq!(std::fs::read(dir.join("a.jpg")).unwrap(), b"second");
        assert_eq!(names(), ["a.jpg"]);
        #[cfg(target_os = "linux")]
        {
            let big = vec![7u8; 4 << 20];
            // A new file on a full disk.
            std::os::unix::fs::symlink("/dev/full", dir.join("full.jpg")).unwrap();
            let e = rt.block_on(op.write_chunks("full.jpg", big.len() as u64, whole(big.clone()))).unwrap_err();
            assert!(e.1.to_lowercase().contains("space"), "{e:?}");
            assert_eq!(names(), ["a.jpg"]);
            // Over a file: the new version, under its hidden name, does not fit.
            std::os::unix::fs::symlink("/dev/full", dir.join(source::save_names("a.jpg").0)).unwrap();
            rt.block_on(write_whole(&op, "a.jpg", big)).unwrap_err();
            assert_eq!(names(), ["a.jpg"]);
            assert_eq!(std::fs::read(dir.join("a.jpg")).unwrap(), b"second");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
