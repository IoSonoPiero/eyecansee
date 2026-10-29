//! Changes made on the phone itself (a photo taken, one deleted in the gallery): the MTP events
//! ObjectAdded, ObjectRemoved and ObjectInfoChanged, which the window takes with `take_changes`
//! (`App::phone_events`). One task per open phone waits for them, without polling: the USB
//! interrupt endpoint on Linux, the WPD callbacks on Windows (mtp-rs 0.36.0). It ends when the
//! phone is unplugged or let go. Without events nothing changes: the app finds out when a read
//! fails (`source::noticed`).
//!
//! An event never calls the phone, so an added object has no path: the window reads its open
//! folder again. Asking WPD about an object just added (`get_object_info`) was followed by a
//! stall of every call to that phone for ~340 s twice out of two, none in 7 tries without it
//! (2026-10-29). Windows stalls that way by itself too: a photo taken on a POCO X7 Pro while the
//! phone is open, with no call at all from the app (FATTI, issue #52); only unplugging frees it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures_util::future::{Either, select};
use mtp_rs::mtp::{DeviceEvent, MtpDevice, ObjectHandle};

/// What changed on a phone; a path (folders end in "/") when a listing had seen the object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Something new, anywhere on the phone.
    Added,
    Removed(String),
    /// Size or date: a file still being written when it was added, say.
    Updated(String),
}

/// Changes not yet taken by the window: (phone serial, change).
static CHANGES: Mutex<Vec<(String, Change)>> = Mutex::new(Vec::new());
static WAKE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// `wake` is called at every change, from the phone's task: it must only wake the window up.
pub fn on_change(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Box::new(wake));
}

/// The changes since the last call.
pub fn take_changes() -> Vec<(String, Change)> {
    std::mem::take(&mut *CHANGES.lock().unwrap())
}

fn push(serial: &str, change: Change) {
    log!("phone {serial}: event {change:?}");
    CHANGES.lock().unwrap().push((serial.to_string(), change));
    if let Some(wake) = WAKE.get() {
        wake();
    }
}

/// How long an object EyeCanSee itself added or removed keeps its events silent. WPD can hold a
/// request for minutes (FATTI, issue #52): a late event of our own would only read the folder again.
const OWN_SECS: u64 = 600;

/// Objects EyeCanSee itself added, removed or changed (copy, move, delete, conversion to the
/// phone): their events change nothing, the grid is already up to date.
#[derive(Default)]
pub(super) struct Own(Mutex<HashMap<ObjectHandle, Instant>>);

impl Own {
    pub(super) fn mark(&self, h: ObjectHandle) {
        let mut own = self.0.lock().unwrap();
        own.retain(|_, at| at.elapsed() < Duration::from_secs(OWN_SECS));
        own.insert(h, Instant::now());
    }

    /// Ours: a move can send two events (removed and added), so the mark stays until it expires.
    fn is_own(&self, h: ObjectHandle) -> bool {
        self.0.lock().unwrap().get(&h).is_some_and(|at| at.elapsed() < Duration::from_secs(OWN_SECS))
    }
}

/// What the task needs of its `Phone`, shared with it.
pub(super) struct Watch {
    pub serial: String,
    pub device: MtpDevice,
    pub handles: Arc<Mutex<HashMap<String, ObjectHandle>>>,
    pub own: Arc<Own>,
}

/// Starts the task of a phone just opened. It stops when the returned sender is dropped (the
/// `Phone` let go) or the phone is unplugged.
pub(super) fn start(w: Watch) -> tokio::sync::oneshot::Sender<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if w.device.capabilities().supports_events {
        log!("phone {}: events on: changes made on the phone show by themselves", w.serial);
        tokio::spawn(run(w, rx));
    } else {
        log!("phone {}: events off (the device announces none): changes made on the phone show at the next reading", w.serial);
    }
    tx
}

async fn run(w: Watch, mut stop: tokio::sync::oneshot::Receiver<()>) {
    loop {
        match select(Box::pin(w.device.next_event()), &mut stop).await {
            Either::Left((Ok(event), _)) => w.event(event),
            Either::Left((Err(mtp_rs::Error::Disconnected), _)) => {
                log!("phone {}: events end: disconnected", w.serial);
                return;
            }
            // Never in a loop: an error that came back at once would spin.
            Either::Left((Err(e), _)) => {
                log!("phone {}: events end: ERROR {e}", w.serial);
                return;
            }
            // On Windows also an unplugged phone ends here: WPD never says Disconnected
            // (2026-10-29), the app lets the phone go when it leaves its folder.
            Either::Right(_) => {
                log!("phone {}: events end: let go", w.serial);
                return;
            }
        }
    }
}

impl Watch {
    fn event(&self, event: DeviceEvent) {
        match event {
            DeviceEvent::ObjectAdded { handle } if !self.own.is_own(handle) => push(&self.serial, Change::Added),
            DeviceEvent::ObjectRemoved { handle } if !self.own.is_own(handle) => {
                // Gone from the phone: its path is known only if a listing saw it.
                let mut handles = self.handles.lock().unwrap();
                let Some(path) = path_of(&handles, handle) else { return };
                // A folder takes the paths below it too.
                handles.retain(|p, _| if path.ends_with('/') { !p.starts_with(&path) } else { *p != path });
                drop(handles);
                push(&self.serial, Change::Removed(path));
            }
            DeviceEvent::ObjectInfoChanged { handle } if !self.own.is_own(handle) => {
                let path = path_of(&self.handles.lock().unwrap(), handle);
                if let Some(path) = path {
                    push(&self.serial, Change::Updated(path));
                }
            }
            _ => {}
        }
    }
}

/// The path a listing recorded for `handle`.
fn path_of(handles: &HashMap<String, ObjectHandle>, handle: ObjectHandle) -> Option<String> {
    handles.iter().find(|(_, h)| **h == handle).map(|(p, _)| p.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_and_paths() {
        let own = Own::default();
        own.mark(ObjectHandle(7));
        assert!(own.is_own(ObjectHandle(7)) && !own.is_own(ObjectHandle(8)));
        let handles = HashMap::from([("Storage/DCIM/".to_string(), ObjectHandle(1)), ("Storage/DCIM/a.jpg".to_string(), ObjectHandle(2))]);
        assert_eq!(path_of(&handles, ObjectHandle(1)).as_deref(), Some("Storage/DCIM/"));
        assert_eq!(path_of(&handles, ObjectHandle(3)), None);
    }

    #[test]
    fn changes_taken_once() {
        let _ = take_changes();
        push("X", Change::Removed("S/a.jpg".into()));
        assert_eq!(take_changes(), [("X".to_string(), Change::Removed("S/a.jpg".into()))]);
        assert!(take_changes().is_empty());
    }
}
