//! A copy the phone made that Windows does not show (#354, prova W5 on Windows, 27/11): after
//! Windows (WPD) stopped waiting for the phone's copy, the listing of the destination never
//! showed the file, though the phone had it whole. The files that can take that long go through
//! the computer there (`across::alone_limit`); this is for a phone or a card slower than
//! foreseen. The end of the copy is then seen in the free space of the storage, which is asked
//! to the phone: once it is less by the size of the file, the phone has made the copy, and the
//! wait ends by saying that Windows does not show it. [Unverified on Windows: whether the free
//! space is asked to the phone at every reading; if it is not, the wait ends at its deadline,
//! and says the same with other words.]

use mtp_rs::mtp::{ObjectHandle, Storage};

use super::Phone;

/// The phone made the copy and Windows does not show it: seen in the free space.
pub(super) const UNSEEN: &str = "error-phone-copy-unseen";
/// Windows stopped waiting for the copy and never showed it, nor its space taken.
pub(super) const LOST: &str = "error-phone-copy-lost";

/// What the wait for the phone found (`Phone::finished_by_phone`).
#[derive(Debug)]
pub(super) enum Found {
    /// The item, in the listing of the destination.
    There(ObjectHandle),
    /// No item in the listing, and its bytes taken from the free space (`made`).
    Unseen,
}

/// Whether a storage that had `before` bytes free and now has `free` of `capacity` holds a new
/// file of `size` bytes: at least nine tenths of it less (something else may free a little
/// meanwhile). A reading that failed gives no capacity (mtp-rs then says 0 and 0): it tells
/// nothing. Never the other way round: space not taken is no proof of a copy not made.
pub(super) fn made(before: u64, (free, capacity): (u64, u64), size: u64) -> bool {
    capacity > 0 && size > 0 && before.saturating_sub(free) >= size - size / 10
}

impl Phone {
    /// The free bytes and the capacity of `storage`, asked now. Not in the phone's turn: the wait
    /// for the phone holds it (`finished_by_phone`).
    pub(super) async fn space(&self, storage: &Storage) -> Option<(u64, u64)> {
        let (device, _) = self.open.get()?;
        let fresh = device.storage(storage.id()).await.ok()?;
        Some((fresh.info().free_space, fresh.info().total_capacity))
    }

    /// Before a copy of `size` bytes into `to` that Windows may stop waiting for: the free bytes
    /// of `to`, to see the end of the copy by (`made_unseen`). Only on Windows, and only for a
    /// file that may take long: on Linux the listing shows the copy, and nothing more is asked.
    pub(super) async fn free_before(&self, path: &str, to: &Storage, size: u64) -> Option<u64> {
        if !cfg!(windows) || size < super::LONG_COPY {
            return None;
        }
        self.queued("storage_info", path, self.space(to)).await.filter(|(_, capacity)| *capacity > 0).map(|(free, _)| free)
    }

    /// Whether the phone has made its copy of `size` bytes into `to`, which had `before` bytes
    /// free (`made`), with what was read in the log.
    pub(super) async fn made_unseen(&self, path: &str, to: &Storage, (before, size): (u64, u64)) -> bool {
        let t = std::time::Instant::now();
        let space = self.space(to).await;
        log!("phone {}: copy {path}: free space read in {:.1} s: {space:?} (free, capacity), {before} free before, {size} bytes to come", self.serial, t.elapsed().as_secs_f32());
        space.is_some_and(|now| made(before, now, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    /// #354 on Windows: the copy the listing never shows is seen in the free space, less by the
    /// size of the file once the phone is done. Not while the phone copies (it answers nothing,
    /// and a reading that failed tells nothing), not for space that did not go down, not for a
    /// part of the file only.
    #[test]
    fn a_copy_made_is_seen_in_the_free_space() {
        let (before, capacity) = (20 * GB, 64 * GB);
        assert!(made(before, (before - 6 * GB, capacity), 6 * GB), "6 GB less");
        assert!(made(before, (before - 6 * GB - 4096, capacity), 6 * GB), "a little more taken");
        assert!(made(before, (before - 6 * GB + (100 << 20), capacity), 6 * GB), "something else freed 100 MB meanwhile");
        assert!(!made(before, (before, capacity), 6 * GB), "nothing taken");
        assert!(!made(before, (before - 3 * GB, capacity), 6 * GB), "half of it");
        assert!(!made(before, (before + GB, capacity), 6 * GB), "more free than before");
        assert!(!made(before, (0, 0), 6 * GB), "a reading that failed: mtp-rs gives 0 and 0");
        assert!(!made(before, (before, capacity), 0), "an empty file is never waited for this way");
    }
}
