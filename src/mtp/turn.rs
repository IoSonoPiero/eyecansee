//! The phone's turn: one mtp-rs call at a time, in order of arrival (`Phone::turn`), and the log
//! line of a call that waited or ran long.

use super::*;

/// The last call answered (what, path, end): named in the log line of a slow call, to see what
/// came before it without logging every call.
static LAST: Mutex<Option<(String, String, std::time::Instant)>> = Mutex::new(None);

/// A phone answering nothing until a long call of it ends, from its start: a big file written to
/// it (the second half of a copy through this computer, `transfer::through_pc`, or from the
/// computer), or copied by the phone itself. Its serial and the file: the status bar says so
/// while that phone is open (`menu::phone_line`), and its folders wait.
static DEAF: Mutex<Option<(String, String)>> = Mutex::new(None);

/// The call in `DEAF` while it lasts.
pub(super) struct Deaf;

impl Deaf {
    pub(super) fn start(serial: &str, path: &str) -> Self {
        *DEAF.lock().unwrap() = Some((serial.to_string(), path.to_string()));
        Deaf
    }
}

impl Drop for Deaf {
    fn drop(&mut self) {
        *DEAF.lock().unwrap() = None;
    }
}

/// The phone answering nothing until it is done with a big file (`DEAF`): its serial, the file's
/// name (the file's own while it is written under its hidden name to replace it).
pub fn deaf() -> Option<(String, String)> {
    DEAF.lock().unwrap().as_ref().map(|(serial, path)| (serial.clone(), source::saved_image(source::shown(path)).to_string()))
}

/// The phone's turn, held (`queued_kept`).
pub(super) type Turn<'a> = tokio::sync::MutexGuard<'a, ()>;

impl Phone {
    /// Runs one mtp-rs call in its turn (see `turn`). A call that waits or runs for more than
    /// 2 s goes in the log with `what` and `path`: the phone takes one call at a time, and a
    /// slow one holds every other one back.
    pub(super) async fn queued<T>(&self, what: &str, path: &str, call: impl std::future::Future<Output = T>) -> T {
        self.queued_kept(what, path, call).await.0
    }

    /// `queued`, with the turn still held when the call ends: what follows it goes next, before any
    /// call waiting meanwhile. A copy or a move given up by mtp-rs at 30 s while the phone still
    /// works hands it to `finished_by_phone`: let go in between, a listing asked during those 30 s
    /// went first to the phone, which answers nothing until done, and failed 30 s later.
    pub(super) async fn queued_kept<T>(&self, what: &str, path: &str, call: impl std::future::Future<Output = T>) -> (T, Turn<'_>) {
        let t = std::time::Instant::now();
        let turn = self.turn.lock().await;
        let waited = t.elapsed();
        let r = {
            let _waiting = Waiting::start(what, path);
            call.await
        };
        let took = t.elapsed() - waited;
        let mut last = LAST.lock().unwrap();
        if waited + took > std::time::Duration::from_secs(2) {
            // The call before this one: a stall came after a delete every time so far (2026-10-28).
            let before = match &*last {
                Some((w, p, end)) => format!(", after {w} {p} ended {:.1} s before", (t + waited).saturating_duration_since(*end).as_secs_f32()),
                None => String::new(),
            };
            log!("phone {}: {what} {path} waited {:.1} s, took {:.1} s{before}", self.serial, waited.as_secs_f32(), took.as_secs_f32());
        }
        *last = Some((what.to_string(), path.to_string(), std::time::Instant::now()));
        (r, turn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// Fable's point 4 (#354): a copy given up at 30 s hands its turn to the wait for the phone;
    /// a listing asked meanwhile comes after that wait, never between the two (it would reach the
    /// phone still copying, and fail after another 30 s). No phone: `Phone::new` opens nothing.
    #[test]
    fn the_turn_is_kept_from_the_copy_to_the_wait() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let phone = Arc::new(Phone::new("no-phone"));
            let order = Arc::new(StdMutex::new(Vec::new()));
            let (p, o) = (phone.clone(), order.clone());
            let copy = tokio::spawn(async move {
                let (_, turn) = p.queued_kept("copy", "a", tokio::time::sleep(std::time::Duration::from_millis(100))).await;
                o.lock().unwrap().push("copy given up");
                // What `finished_by_phone` does with the turn: waits, nothing else in between.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                o.lock().unwrap().push("copy done by the phone");
                drop(turn);
            });
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            let (p, o) = (phone.clone(), order.clone());
            let listing = tokio::spawn(async move { p.queued("names", "b", async { o.lock().unwrap().push("listing") }).await });
            copy.await.unwrap();
            listing.await.unwrap();
            assert_eq!(*order.lock().unwrap(), ["copy given up", "copy done by the phone", "listing"]);
        });
    }
}
