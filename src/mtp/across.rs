//! A copy the phone makes itself, also inside one storage, or a move from one of its storages to
//! the other (the internal storage and the memory card): the phone copies the data inside that
//! one call, about 2 minutes for 2 GB to an SD card or one for 6 GB inside the internal storage
//! (Redmi Note 9 Pro, 25/11/2026), past the 30 s mtp-rs waits for an answer.

use super::*;

/// How often the destination is read while the phone works.
const LOOK: std::time::Duration = std::time::Duration::from_secs(3);

/// The most a phone may take for `size` bytes between its storages: a minute, plus 1 MB/s (the
/// card of the Redmi took 16 MB/s).
fn deadline(size: u64) -> std::time::Duration {
    std::time::Duration::from_secs(60 + size / (1 << 20))
}

/// How long Windows (WPD) waits for a copy before it answers: `Copy` came back after 30.4 s with
/// the phone still copying 6 GB (Redmi Note 9 Pro, prova W5, 27/11/2026). An answer before this is
/// the phone's own: a copy refused at once (#59) is an error.
const WPD_GAVE_UP: std::time::Duration = std::time::Duration::from_secs(25);

/// A copy given up while the phone is still making it, so its result is looked for in the
/// destination (`finished_by_phone`): past the 30 s mtp-rs waits for an answer (`Timeout`, USB),
/// or, on Windows, WPD's `Copy` back near 30 s with no new object there yet (`InvalidData`, #354),
/// only for a file of `LONG_COPY` and a call that long.
pub(super) fn still_at_it(e: &mtp_rs::Error, size: u64, took: std::time::Duration) -> bool {
    match e {
        mtp_rs::Error::Timeout => true,
        mtp_rs::Error::InvalidData { .. } => size >= LONG_COPY && took >= WPD_GAVE_UP,
        _ => false,
    }
}

/// The error of a copy the phone did not make, for the window: on Windows mtp-rs says it with
/// `InvalidData` (no new object), in English, which goes to the log only.
pub(super) fn copy_error(e: mtp_rs::Error, path: &str) -> Error {
    match e {
        mtp_rs::Error::InvalidData { message } => {
            log!("copy of {path} not made: {message}");
            (NOT_MADE, path.to_string())
        }
        e => error(e),
    }
}

/// Whether the item looked for is where the phone puts it: `name` with `size` bytes among `found`.
fn arrived(found: &[(String, u64, ObjectHandle)], name: &str, size: u64) -> Option<ObjectHandle> {
    found.iter().find(|(n, s, _)| n == name && *s == size).map(|(_, _, h)| *h)
}

/// The phone's events stopped while it copies or moves between its storages, started again when
/// dropped: with them waited for, mtp-rs did not get the session back after giving up the call at
/// 30 s, and every next call stayed waiting for ever, Cancel too; without them the destination
/// answers again once the phone is done (Redmi Note 9 Pro, 2 GB to the SD card, 25/11/2026).
pub(super) struct Quiet<'a>(&'a Phone);

impl Drop for Quiet<'_> {
    fn drop(&mut self) {
        let phone = self.0;
        if let Some((device, _)) = phone.open.get() {
            let watch = events::Watch { serial: phone.serial.clone(), device: device.clone(), handles: phone.handles.clone(), complete: phone.complete.clone(), own: phone.own.clone() };
            *phone._events.lock().unwrap() = Some(events::start(watch));
        }
    }
}

impl Phone {
    /// Stops the phone's events until the guard is dropped (`Quiet`).
    pub(super) fn quiet(&self) -> Quiet<'_> {
        if let Some(stop) = self._events.lock().unwrap().take() {
            let _ = stop.send(());
        }
        Quiet(self)
    }

    /// The size of `handle` (on `from`) when it goes to the other storage `to`: what is waited for
    /// if the phone takes longer than mtp-rs waits (`finished_by_phone`). `None` on one storage.
    pub(super) async fn size_across(&self, path: &str, (from, handle): (&Storage, ObjectHandle), to: &Storage) -> Result<Option<u64>, Error> {
        if from.id() == to.id() {
            return Ok(None);
        }
        Ok(Some(self.size_of(path, (from, handle)).await?))
    }

    /// The free bytes of the storage of `path`, asked to the phone now (the ones read when it was
    /// opened are old): a big file through the computer checks there is room before it is read.
    pub async fn free_space(&self, path: &str) -> Result<u64, Error> {
        let (storage, _) = storages::find(self.storages().await?, path).ok_or((NOT_FOUND, path.to_string()))?;
        let (device, _) = self.open.get().ok_or((NOT_FOUND, path.to_string()))?;
        let fresh = self.queued("storage_info", path, device.storage(storage.id())).await.map_err(error)?;
        Ok(fresh.info().free_space)
    }

    /// The size of `handle` (on `from`), asked to the phone.
    pub(super) async fn size_of(&self, path: &str, (from, handle): (&Storage, ObjectHandle)) -> Result<u64, Error> {
        Ok(self.queued("info", path, from.get_object_info(handle)).await.map_err(error)?.size)
    }

    /// After a copy or a move of `path` (begun at `began`) into `folder` of the storage `to`, given up by mtp-rs at
    /// 30 s: the phone is still at it. Its turn, held since the call (`queued_kept`), is kept, so nothing else is asked meanwhile but the
    /// destination, read every few seconds (`wait::until_arrived`; a reading while the phone works is given up at 30 s
    /// too, and mtp-rs goes on), until `name` is there with `size` bytes and, for a move, the item
    /// `moved` is no longer on its storage. The handle of what arrived.
    pub(super) async fn finished_by_phone(&self, (what, began, _turn): (&str, std::time::Instant, turn::Turn<'_>), path: &str, (to, folder): (&Storage, ObjectHandle), (name, size): (&str, u64), moved: Option<(&Storage, ObjectHandle)>) -> Result<ObjectHandle, Error> {
        let start = std::time::Instant::now();
        let (_waiting, _deaf) = (Waiting::start_at(what, path, began), turn::Deaf::start(&self.serial, path));
        log!("phone {}: {what} {path}: the phone is still at it ({size} bytes), its result looked for", self.serial);
        let parent = (folder != ObjectHandle::ROOT).then_some(folder);
        let pace = wait::Pace { look: LOOK, most: deadline(size) };
        let look = |at: std::time::Duration| async move {
            let t = std::time::Instant::now();
            let found = match to.list_objects(parent).await {
                Ok(objects) => objects.into_iter().map(|o| (o.filename, o.size, o.handle)).collect::<Vec<_>>(),
                // The phone is still copying: this reading was given up too.
                Err(e) => {
                    log!("phone {}: {what} {path}: destination not read ({e}) in {:.1} s, at {:.0} s", self.serial, t.elapsed().as_secs_f32(), at.as_secs_f32());
                    return Err(e);
                }
            };
            let there = found.iter().find(|(n, _, _)| n == name).map(|(_, s, _)| *s);
            log!("phone {}: {what} {path}: destination read in {:.1} s, at {:.0} s: {there:?} of {size} bytes", self.serial, t.elapsed().as_secs_f32(), at.as_secs_f32());
            let Some(h) = arrived(&found, name, size) else { return Ok(None) };
            if let Some((from, old)) = moved
                && from.get_object_info(old).await.is_ok_and(|i| i.storage_id == from.id())
            {
                return Ok(None);
            }
            Ok(Some(h))
        };
        match wait::until_arrived(pace, look).await {
            wait::End::Arrived(h) => {
                log!("phone {}: {what} {path}: done by the phone in {:.0} s", self.serial, start.elapsed().as_secs_f32());
                Ok(h)
            }
            wait::End::Gone(e) => Err(error(e)),
            wait::End::Late => {
                log!("phone {}: {what} {path}: not done by the phone in {:.0} s", self.serial, start.elapsed().as_secs_f32());
                Err(("error-phone-timeout", path.to_string()))
            }
        }
    }

    /// `CopyObject` of `path` into the folder `dir`. A handle equal to the original's means a copy
    /// not made (see `copy_inside`): it is never renamed or moved.
    pub(super) async fn copy_object(&self, path: &str, dir: &str) -> Result<ObjectHandle, Error> {
        let (storage, handle) = self.locate(path).await?;
        let handle = handle.ok_or((NOT_FOUND, path.to_string()))?;
        // Into the storage of `dir`, which may not be the one of `path` (audit 2.7).
        let (to, folder) = match self.locate(dir).await? {
            (to, Some(h)) => (to, h),
            (to, None) => (to, ObjectHandle::ROOT),
        };
        // A copy is made by the phone also inside one storage: a big one takes longer than mtp-rs
        // waits there too (6 GB, about a minute). The events stop for a file that may (`Quiet`).
        let size = self.size_of(path, (&storage, handle)).await?;
        let _quiet = (size >= LONG_COPY).then(|| (self.quiet(), turn::Deaf::start(&self.serial, path)));
        let began = std::time::Instant::now();
        let h = match self.queued_kept("copy", path, self.marked(storage.copy_object(handle, folder, Some(to.id())))).await {
            // Given up while the phone is still copying it (`still_at_it`).
            (Err(e), turn) if still_at_it(&e, size, began.elapsed()) => {
                let h = self.finished_by_phone(("copy", began, turn), path, (&to, folder), (source::name(path), size), None).await?;
                self.own.mark(h);
                h
            }
            (r, _) => r.map_err(|e| copy_error(e, path))?,
        };
        log!("phone {}: copy {path} into {dir}: handle {} -> {}", self.serial, handle.0, h.0);
        if h == handle {
            return Err((NOT_MADE, path.to_string()));
        }
        Ok(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What WPD answers on Windows when the phone copies a big file inside a storage: `Copy` back
    /// after about 30 s, no new object in the destination yet, and mtp-rs gives `InvalidData` (6 GB,
    /// Redmi Note 9 Pro, prova W5, 27/11/2026). The phone finished the copy all the same.
    fn wpd_no_new_object() -> mtp_rs::Error {
        mtp_rs::Error::invalid_data("the device reported a copy it didn't make: no new object appeared in the destination")
    }

    /// #354 on Windows: a long copy WPD gave up is still being made by the phone; the same answer
    /// at once, or for a small file, is a copy the phone refused (#59) and stays an error.
    #[test]
    fn a_copy_wpd_gave_up_is_still_being_made() {
        let s = std::time::Duration::from_secs;
        assert!(still_at_it(&mtp_rs::Error::Timeout, 1, s(30)), "USB: the 30 s of mtp-rs");
        assert!(still_at_it(&wpd_no_new_object(), 6 << 30, s(30)), "WPD: 6 GB, back after 30 s");
        assert!(!still_at_it(&wpd_no_new_object(), 6 << 30, s(1)), "WPD: refused at once");
        assert!(!still_at_it(&wpd_no_new_object(), 1 << 20, s(30)), "WPD: a small file");
        assert!(!still_at_it(&mtp_rs::Error::Disconnected, 6 << 30, s(30)), "unplugged");
    }

    /// #354 on Windows: a copy the phone did not make says so in the user's language, with the file;
    /// mtp-rs's English text only in the log.
    #[test]
    fn a_copy_not_made_says_so_in_the_users_language() {
        assert_eq!(copy_error(wpd_no_new_object(), "Download/a.mp4"), (NOT_MADE, "Download/a.mp4".to_string()));
        assert_eq!(copy_error(mtp_rs::Error::Disconnected, "Download/a.mp4").0, "error-phone-disconnected");
    }

    #[test]
    fn what_arrived_and_how_long() {
        let found = vec![("a.mp4".to_string(), 10, ObjectHandle(1)), ("b.mp4".to_string(), 5, ObjectHandle(2))];
        assert_eq!(arrived(&found, "a.mp4", 10), Some(ObjectHandle(1)));
        // Still being written: not there yet.
        assert_eq!(arrived(&found, "b.mp4", 20), None);
        assert_eq!(arrived(&found, "c.mp4", 10), None);
        assert_eq!(deadline(0).as_secs(), 60);
        assert_eq!(deadline(2 << 30).as_secs(), 60 + 2048);
    }
}
