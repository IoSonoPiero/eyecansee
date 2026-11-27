//! Probes with a real phone for #354 on Windows: what Windows (WPD) shows of a copy the phone
//! goes on making after Windows stopped waiting for it. Not run by `cargo test` (`#[ignore]`).
//! The files to copy are the tester's, left as they are; the copies go in a folder of their own,
//! `EyeCanSee-prova-wpd/`, next to nothing else, removed at the end when the phone lets it.
//! Start with a small file, which the phone copies in a few seconds (the probe then only checks
//! that everything works), and go up to one that takes more than 30 s only when that passes.

use std::time::{Duration, Instant};

use mtp_rs::mtp::{DeviceEvent, MtpDevice, ObjectHandle, Storage};

use super::{Phone, source};

/// The folder of the probes, in the storage of the file copied.
const DIR: &str = "EyeCanSee-prova-wpd";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} not set"))
}

/// Seconds since `t`, for the lines printed.
fn at(t: Instant) -> String {
    format!("{:6.1} s", t.elapsed().as_secs_f32())
}

/// `name` in the folder `parent` of `storage`: its size, if a listing shows it, and how long the
/// listing took; the error of a listing that failed.
async fn listed(storage: &Storage, parent: Option<ObjectHandle>, name: &str) -> (Result<Option<u64>, String>, Duration) {
    let t = Instant::now();
    let r = storage.list_objects(parent).await.map(|objects| objects.into_iter().find(|o| o.filename == name).map(|o| o.size)).map_err(|e| e.to_string());
    (r, t.elapsed())
}

/// `name` in the folder `DIR` of the storage `id`, looked for from another session, `device`:
/// its storages, the top of the storage and the folder are read anew.
async fn listed_anew(device: &MtpDevice, id: mtp_rs::mtp::StorageId, name: &str) -> Result<Option<u64>, String> {
    let storages = device.storages().await.map_err(|e| format!("storages: {e}"))?;
    let storage = storages.iter().find(|s| s.id() == id).ok_or("the storage is not there")?;
    let top = storage.list_objects(None).await.map_err(|e| format!("top: {e}"))?;
    let dir = top.iter().find(|o| o.filename == DIR).ok_or("the folder of the probe is not there")?;
    listed(storage, Some(dir.handle), name).await.0
}

/// What a real phone and Windows show while and after the phone copies a file inside one storage,
/// asked to mtp-rs directly (no wait of EyeCanSee in between), every 3 s: the listing of the
/// destination from the session that asked the copy, the free space of the storage, the events,
/// and the listing from a second session opened after the copy was given up. It answers, with
/// one run: which of them shows the copy on Windows, and when.
/// `EYECANSEE_MTP=<serial> EYECANSEE_MTP_BIG="<storage>/Download/prova-512MB.mp4" [EYECANSEE_PROBE_SECS=120] cargo test --release mtp_wpd_copy_probe_live -- --ignored --nocapture`
#[test]
#[ignore]
fn mtp_wpd_copy_probe_live() {
    let (serial, big) = (env("EYECANSEE_MTP"), env("EYECANSEE_MTP_BIG"));
    let secs: u64 = std::env::var("EYECANSEE_PROBE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let phone = Phone::new(&serial);
        phone.list("").await.expect("the phone opens");
        let (storage, handle) = phone.locate(&big).await.expect("EYECANSEE_MTP_BIG is there");
        let handle = handle.expect("EYECANSEE_MTP_BIG is a file");
        let (name, size) = (source::name(&big), storage.get_object_info(handle).await.unwrap().size);
        let dir = format!("{}/{DIR}/", big.split('/').next().unwrap());
        assert!(!phone.taken(&dir).await.unwrap(), "{dir} already exists: remove it first");
        phone.create_dir(&dir).await.unwrap();
        let folder = phone.locate(&dir).await.unwrap().1.unwrap();
        let (device, _) = phone.open.get().unwrap();
        // The events to this probe, not to the phone's own task.
        let _quiet = phone.quiet();
        let t = Instant::now();
        let events = tokio::spawn({
            let device = device.clone();
            async move {
                loop {
                    match device.next_event().await {
                        Ok(DeviceEvent::ObjectAdded { handle }) => println!("{} EVENT object added, handle {}", at(t), handle.0),
                        Ok(e) => println!("{} EVENT {e:?}", at(t)),
                        Err(e) => return println!("{} events end: {e}", at(t)),
                    }
                }
            }
        });
        let before = phone.space(&storage).await;
        println!("{name}: {size} bytes; free and capacity before {before:?}; copying into {dir}");
        let copied = storage.copy_object(handle, folder, Some(storage.id())).await;
        println!("{} copy_object ended: {copied:?}", at(t));
        let mut second = None;
        let mut seen = [None::<f32>; 3];
        while t.elapsed() < Duration::from_secs(secs) {
            let (old, took) = listed(&storage, Some(folder), name).await;
            println!("{} listing, session of the copy ({:.1} s): {old:?}", at(t), took.as_secs_f32());
            let t1 = Instant::now();
            let space = phone.space(&storage).await;
            println!("{} free space and capacity ({:.1} s): {space:?}, taken {:?}", at(t), t1.elapsed().as_secs_f32(), before.zip(space).map(|(b, s)| b.0.saturating_sub(s.0)));
            if second.is_none() {
                second = super::open(&serial).await.map_err(|e| println!("{} second session not opened: {e:?}", at(t))).ok();
            }
            let anew = match &second {
                Some(device) => listed_anew(device, storage.id(), name).await,
                None => Err("no second session".into()),
            };
            println!("{} listing, second session: {anew:?}", at(t));
            let whole = |r: &Result<Option<u64>, String>| matches!(r, Ok(Some(s)) if *s == size);
            let now = [whole(&old), before.zip(space).is_some_and(|(b, s)| super::unseen::made(b.0, s, size)), whole(&anew)];
            for (first, _) in seen.iter_mut().zip(now).filter(|(first, now)| first.is_none() && *now) {
                *first = Some(t.elapsed().as_secs_f32());
            }
            if now[0] {
                break;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        println!("RESULT copy whole seen at (s): listing of the session of the copy {:?}, free space {:?}, listing of a second session {:?}", seen[0], seen[1], seen[2]);
        events.abort();
        drop(second);
        println!("removing {dir}: {:?} (if the listing never showed the copy, check on the phone that the folder is gone)", phone.delete(&dir).await);
    });
}

/// The same copy as a paste makes it (`Phone::copy_inside`), on Windows past the size a paste
/// leaves to the phone there (`across::alone_limit`): with a file the phone takes more than 30 s
/// to copy, Windows gives the copy up and EyeCanSee waits for the phone; the test prints how the
/// wait ended (the copy there, "error-phone-copy-unseen" once its space is taken, or
/// "error-phone-copy-lost" at the deadline) and how long it took. With `EYECANSEE_LOG=1` the log has
/// every look of the wait.
/// `EYECANSEE_MTP=<serial> EYECANSEE_MTP_BIG="<storage>/Download/prova-4GB.mp4" cargo test --release mtp_wpd_copy_inside_live -- --ignored --nocapture`
#[test]
#[ignore]
fn mtp_wpd_copy_inside_live() {
    let (serial, big) = (env("EYECANSEE_MTP"), env("EYECANSEE_MTP_BIG"));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let phone = Phone::new(&serial);
        phone.list("").await.expect("the phone opens");
        let dir = format!("{}/{DIR}/", big.split('/').next().unwrap());
        assert!(!phone.taken(&dir).await.unwrap(), "{dir} already exists: remove it first");
        phone.create_dir(&dir).await.unwrap();
        let dest = format!("{dir}{}", source::name(&big));
        let t = Instant::now();
        let r = phone.copy_inside(&big, &dest, &std::sync::atomic::AtomicBool::new(false)).await;
        println!("{} copy_inside ended: {r:?}", at(t));
        println!("{} the copy in a listing: {:?}", at(t), phone.names(&dir).await.map(|n| n.contains(source::name(&big))));
        println!("removing {dir}: {:?} (if the listing did not show the copy, check on the phone that the folder is gone)", phone.delete(&dir).await);
    });
}
