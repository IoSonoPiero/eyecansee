//! Cancel pressed while the phone copies a file by itself (#354; Piero, 26/11 night: the phone
//! finished the file and the paste ended "Copy finished", so Cancel did nothing). The phone cannot
//! be stopped halfway through a file: it ends it, and then the copy it just made is deleted, where
//! it is (still in the hidden folder, or in the folder of arrival under the source's name), before
//! it gets its name or takes the place of an item. The original is never touched, and an item
//! the copy was to replace stays as it was.

use mtp_rs::ObjectHandle;

use super::{Error, Phone, cache, error};

/// The error of a copy cancelled, as everywhere (`transfer`).
const CANCELLED: &str = "error-cancelled";

/// The end of a copy the phone has made (its `CopyObject` done). Not `cancelled`: `place` gives
/// the copy its name and its place. `cancelled`: `remove` deletes it where it is and the copy
/// ends as cancelled; it is never placed, so nothing is replaced by it.
pub(super) async fn finish(cancelled: bool, remove: impl Future<Output = Result<(), Error>>, place: impl Future<Output = Result<(), Error>>) -> Result<(), Error> {
    if !cancelled {
        return place.await;
    }
    remove.await?;
    Err((CANCELLED, String::new()))
}

impl Phone {
    /// The copy `h` the phone just made in the folder `dir`, named `name` there, deleted
    /// (`finish`). By its handle: the original has the same name when `dir` is its folder's
    /// hidden one, and another handle always (`copy_object`).
    pub(super) async fn remove_copy(&self, dir: &str, name: &str, h: ObjectHandle) -> Result<(), Error> {
        let at = format!("{dir}{name}");
        let storage = self.storage_of(dir).await?;
        self.own.mark(h);
        self.queued("delete", &at, storage.delete(h)).await.map_err(error)?;
        cache::forget(&mut self.handles.lock().unwrap(), &at);
        log!("phone {}: copy {at} removed after Cancel (handle {})", self.serial, h.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::*;

    /// A phone's folders as names and contents, for the three ways a copy is made (`Phone::copy_inside`).
    struct Fake(Mutex<BTreeMap<String, &'static str>>);

    impl Fake {
        /// The original `DCIM/a.mp4`, a file `Download/a.mp4` already there, and two copies of this
        /// paste finished before.
        fn new() -> Self {
            let files = [("DCIM/a.mp4", "original"), ("Download/a.mp4", "there before"), ("Download/first.mp4", "copied before"), ("Download/second.mp4", "copied before")];
            Fake(Mutex::new(files.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
        }

        /// `CopyObject`: the copy is there, under the source's name, in `dir`.
        fn copied(&self, dir: &str) -> String {
            let made = format!("{dir}a.mp4");
            self.0.lock().unwrap().insert(made.clone(), "copy");
            made
        }

        async fn remove(&self, path: &str) -> Result<(), Error> {
            self.0.lock().unwrap().remove(path).map(|_| ()).ok_or(("error-not-found", path.to_string()))
        }

        /// The copy takes the name `to`, whatever is there (a rename, a move, `put_in_place`).
        async fn place(&self, from: &str, to: &str) -> Result<(), Error> {
            let mut files = self.0.lock().unwrap();
            let copy = files.remove(from).ok_or(("error-not-found", from.to_string()))?;
            files.insert(to.to_string(), copy);
            Ok(())
        }

        fn names(&self) -> Vec<(String, &'static str)> {
            self.0.lock().unwrap().iter().map(|(k, v)| (k.clone(), *v)).collect()
        }
    }

    fn block<T>(f: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(f)
    }

    /// #354: Cancel while the phone copied. The copy it made goes, wherever it is: in the folder
    /// of arrival under the source's name (a free name there), or in the hidden folder (the same
    /// folder, a name taken, an item to replace). The original, the item that was to be replaced
    /// and the files copied before stay as they were, and the copy ends as cancelled.
    #[test]
    fn a_cancelled_copy_is_removed_where_it_is() {
        for (dir, dest) in [("Pictures/", "Pictures/a.mp4"), ("DCIM/.EyeCanSee-copy/", "DCIM/a (2).mp4"), ("Download/.EyeCanSee-copy/", "Download/a.mp4")] {
            let phone = Fake::new();
            let before = phone.names();
            let made = phone.copied(dir);
            let r = block(finish(true, phone.remove(&made), phone.place(&made, dest)));
            assert_eq!(r, Err((CANCELLED, String::new())), "{dest}");
            assert_eq!(phone.names(), before, "{dest}: only the copy just made goes");
        }
    }

    /// Not cancelled: the copy takes its place, and nothing is removed.
    #[test]
    fn a_copy_not_cancelled_takes_its_place() {
        let phone = Fake::new();
        let made = phone.copied("Download/.EyeCanSee-copy/");
        assert_eq!(block(finish(false, phone.remove(&made), phone.place(&made, "Download/a.mp4"))), Ok(()));
        let names = phone.names();
        assert!(names.contains(&("Download/a.mp4".to_string(), "copy")) && names.contains(&("DCIM/a.mp4".to_string(), "original")), "{names:?}");
        assert!(!names.iter().any(|(n, _)| n.contains(".EyeCanSee-copy")), "{names:?}");
    }

    /// A copy that cannot be removed says why, not "cancelled": it is still on the phone.
    #[test]
    fn a_copy_not_removed_is_an_error() {
        let phone = Fake::new();
        let r = block(finish(true, phone.remove("Pictures/missing.mp4"), phone.place("Pictures/missing.mp4", "Pictures/b.mp4")));
        assert_eq!(r.unwrap_err().0, "error-not-found");
    }
}
