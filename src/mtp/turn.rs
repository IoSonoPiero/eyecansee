//! The phone's turn: one mtp-rs call at a time, in order of arrival (`Phone::turn`), and the log
//! line of a call that waited or ran long.

use super::*;

/// The last call answered (what, path, end): named in the log line of a slow call, to see what
/// came before it without logging every call.
static LAST: Mutex<Option<(String, String, std::time::Instant)>> = Mutex::new(None);

impl Phone {
    /// Runs one mtp-rs call in its turn (see `turn`). A call that waits or runs for more than
    /// 2 s goes in the log with `what` and `path`: the phone takes one call at a time, and a
    /// slow one holds every other one back.
    pub(super) async fn queued<T>(&self, what: &str, path: &str, call: impl std::future::Future<Output = T>) -> T {
        let t = std::time::Instant::now();
        let _turn = self.turn.lock().await;
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
        r
    }
}
