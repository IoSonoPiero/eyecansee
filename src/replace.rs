//! Writing a file whole, and saving an image over its original safely: the new file under a
//! hidden name, the original put away as asked, the new one renamed into place, everything put
//! back at any failure. Split from `loader.rs` on 2026-11-01.

use crate::loader::SKIPPED;
use crate::source::{self, Store};

pub(crate) async fn write_whole(op: &Store, path: &str, data: Vec<u8>) -> Result<(), source::Error> {
    let (chunks, rx) = tokio::sync::mpsc::channel(1);
    let size = data.len() as u64;
    let _ = chunks.send(Ok(bytes::Bytes::from(data))).await;
    drop(chunks);
    op.write_chunks(path, size, rx).await
}

/// The image read and turned in its own format, and whether it lost quality.
pub(crate) async fn turned(op: &Store, path: &str, turn: crate::transform::Turn) -> Result<(Vec<u8>, bool), source::Error> {
    rewritten(op, path, move |bytes, name| crate::encode::turned_file(bytes, name, turn)).await
}

/// The file read and passed to `make` (in the background), which gives the file to write and
/// whether it lost quality; `SKIPPED` if it cannot be written back over itself. An Ultra HDR
/// photo passes when `ultra_hdr`: only flips and quarter turns know how to keep its gain map.
pub(crate) async fn rewritten<T: Send + 'static>(op: &Store, path: &str, make: impl FnOnce(&[u8], &str) -> Result<T, String> + Send + 'static) -> Result<T, source::Error> {
    let bytes = op.read(path).await?;
    let name = source::name(path).to_string();
    tokio::task::spawn_blocking(move || {
        if !crate::encode::can_turn_in_place(&bytes, &name) && !crate::ultrahdr::is(&bytes) {
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
pub(crate) async fn replace(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), source::Error> {
    if !op.can_rename() {
        return Err(("error-rename-unsupported", path.to_string()));
    }
    source::SAVING.lock().unwrap().insert(path.to_string());
    let r = swap(op, path, data, how).await;
    source::SAVING.lock().unwrap().remove(path);
    r
}

async fn swap(op: &Store, path: &str, data: Vec<u8>, how: source::Deletion) -> Result<(), source::Error> {
    let own = source::name(path);
    let (new, old) = source::save_names(path);
    // An original left by an earlier save cut short after its new version took the name: it goes
    // where that save would have sent it.
    if op.names(&source::parent(path)).await?.contains(source::name(&old)) {
        put_away(op, &old, own, how).await?;
    }
    write_whole(op, &new, data).await?;
    // The original leaves its name and waits as `old`. To the system trash it goes now with its
    // own name (restoring it from there gives it back), `old` being a second name of it.
    let left = match how {
        source::Deletion::SystemTrash => match op.link_or_copy(path, &old).await {
            Ok(()) => op.delete(path, how).await,
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
    // The new image is in place: if the original cannot go, it stays hidden until the next save.
    if let Err(e) = put_away(op, &old, own, how).await {
        log!("save {path:?}: original kept as {old:?}: {e:?}");
    }
    Ok(())
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
