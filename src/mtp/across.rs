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

    /// The size of `handle` (on `from`), asked to the phone.
    pub(super) async fn size_of(&self, path: &str, (from, handle): (&Storage, ObjectHandle)) -> Result<u64, Error> {
        Ok(self.queued("info", path, from.get_object_info(handle)).await.map_err(error)?.size)
    }

    /// After a copy or a move of `path` (begun at `began`) into `folder` of the storage `to`, given up by mtp-rs at
    /// 30 s: the phone is still at it. Its turn is kept, so nothing else is asked meanwhile but the
    /// destination, read every few seconds (a reading while the phone works is given up at 30 s
    /// too, and mtp-rs goes on), until `name` is there with `size` bytes and, for a move, the item
    /// `moved` is no longer on its storage. The handle of what arrived.
    pub(super) async fn finished_by_phone(&self, (what, began): (&str, std::time::Instant), path: &str, (to, folder): (&Storage, ObjectHandle), (name, size): (&str, u64), moved: Option<(&Storage, ObjectHandle)>) -> Result<ObjectHandle, Error> {
        let start = std::time::Instant::now();
        let _turn = self.turn.lock().await;
        let _waiting = Waiting::start_at(what, path, began);
        log!("phone {}: {what} {path}: the phone is still at it ({size} bytes), its result looked for", self.serial);
        let parent = (folder != ObjectHandle::ROOT).then_some(folder);
        while start.elapsed() < deadline(size) {
            tokio::time::sleep(LOOK).await;
            let t = std::time::Instant::now();
            let found = match to.list_objects(parent).await {
                Ok(objects) => objects.into_iter().map(|o| (o.filename, o.size, o.handle)).collect::<Vec<_>>(),
                Err(e) if e.is_disconnected() => return Err(error(e)),
                // The phone is still copying: this reading was given up too.
                Err(e) => {
                    log!("phone {}: {what} {path}: destination not read ({e}) in {:.1} s, at {:.0} s", self.serial, t.elapsed().as_secs_f32(), start.elapsed().as_secs_f32());
                    continue;
                }
            };
            let there = found.iter().find(|(n, _, _)| n == name).map(|(_, s, _)| *s);
            log!("phone {}: {what} {path}: destination read in {:.1} s, at {:.0} s: {there:?} of {size} bytes", self.serial, t.elapsed().as_secs_f32(), start.elapsed().as_secs_f32());
            let Some(h) = arrived(&found, name, size) else { continue };
            if let Some((from, old)) = moved
                && from.get_object_info(old).await.is_ok_and(|i| i.storage_id == from.id())
            {
                continue;
            }
            log!("phone {}: {what} {path}: done by the phone in {:.0} s", self.serial, start.elapsed().as_secs_f32());
            return Ok(h);
        }
        log!("phone {}: {what} {path}: not done by the phone in {:.0} s", self.serial, start.elapsed().as_secs_f32());
        Err(("error-phone-timeout", path.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
