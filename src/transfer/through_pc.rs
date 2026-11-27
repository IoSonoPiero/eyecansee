//! A big file of a phone copied or moved to its other storage through this computer (#354): read
//! into a file of the app's cache folder, then written to its place. Done by the phone itself
//! (`Phone::copy_inside`, `move_into`), a big one took minutes in one call that tells nothing of
//! how far it is, with no bar and no Cancel ("without progress bars it is a real mess", Piero,
//! 26/11). This way every byte is counted, both ways, and Cancel stops the reading at once.
//! Slower: 512 MB to the SD card of a Redmi Note 9 Pro 47 s instead of about 28. Inside one
//! storage it was 6 times slower (32 s instead of about 5), so there the phone keeps it
//! (`by_computer`; measured 26/11).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use bytes::Bytes;
use tokio::sync::mpsc::{Receiver, Sender};

use super::{Fail, Progress};
use crate::source::{Error, Store, name};

/// From this size a file goes through the computer: smaller ones the phone copies in a few seconds,
/// well within the 30 s mtp-rs waits for its answer.
pub(super) const BIG: u64 = crate::mtp::LONG_COPY;

/// Left free on this computer beyond the file: the system and the thumbnails keep working.
const PC_MARGIN: u64 = 512 << 20;
/// Left free on the phone beyond the file.
const PHONE_MARGIN: u64 = 32 << 20;

/// The half of the copy under way, for the copy box.
#[derive(Clone, Debug, PartialEq)]
pub enum Staging {
    /// From the phone into the computer.
    Reading,
    /// From the computer to the phone's storage named here.
    Writing(String),
}

/// Whether a file of `size` bytes goes from `src` to `dest` of the same phone through this
/// computer: a big one between its two storages. Inside one storage the phone keeps it at any
/// size: a move is a change of folder, done at once, and a copy is fast there (2 GB in about
/// 20 s on a Redmi Note 9 Pro, 6 times slower through the computer; Piero, 26/11: the short wait
/// with the phone's folders closed is better). Paths start with their storage (`"SD card/DCIM/…"`).
/// `alone`: the biggest file the phone is left to copy by itself inside the storage of `dest`
/// (`mtp::Phone::alone_limit`), `None` for any size.
pub(super) fn by_computer(size: u64, src: &str, dest: &str, alone: Option<u64>) -> bool {
    let _ = alone;
    size >= BIG && storage(src) != storage(dest)
}

/// The storage a path of a phone is on: its first name (`"SD card"` of `"SD card/DCIM/a.jpg"`).
pub(super) fn storage(path: &str) -> &str {
    path.split('/').next().unwrap_or_default()
}

/// Whether `free` bytes hold `size` more and still leave `margin`.
fn enough(free: u64, size: u64, margin: u64) -> bool {
    free >= size.saturating_add(margin)
}

/// The free bytes of the disk holding `dir`, for this user; `None` when not known.
fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` is a valid C string, `st` a writable statvfs.
        (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
        let mut free = 0u64;
        // SAFETY: `wide` ends with 0, the other two may be null.
        let ok = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
        (ok != 0).then_some(free)
    }
}

/// The folder of the files read from a phone, in the app's cache folder (not the temporary one:
/// on many systems it is in memory); only the user can open it.
fn folder() -> Option<PathBuf> {
    let dir = crate::thumbcache::cache_dir()?.join("phone-copy");
    std::fs::create_dir_all(&dir).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Some(dir)
}

/// At start, from `temp_file::remove_old_copies`: the files a copy of the app closed during such a
/// copy left (closed by force, the guard below never ran). At exit (`own`): this copy's, as the
/// copy under way may never get to drop its guard (on Windows a file still open stays, for the
/// next start).
pub(crate) fn remove_left(own: bool) {
    if let Some(dir) = crate::thumbcache::cache_dir() {
        remove_left_in(&dir.join("phone-copy"), own.then(std::process::id));
    }
}

fn remove_left_in(dir: &Path, only: Option<u32>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if only.is_none_or(|pid| name.starts_with(&format!("{pid}-"))) {
            log!("phone copy left on this computer: {name} removed");
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The file on this computer, removed when dropped: after the copy, at an error, at Cancel.
struct Staged {
    store: Store,
    name: String,
    file: PathBuf,
}

impl Staged {
    /// A new empty file in `dir` for `size` bytes, if `free` (of its disk, when known) holds them
    /// (`error-pc-space`).
    fn new(dir: &Path, (size, free): (u64, Option<u64>), shown: &str) -> Result<Self, Error> {
        if free.is_some_and(|free| !enough(free, size, PC_MARGIN)) {
            return Err(("error-pc-space", shown.to_string()));
        }
        let (file, _) = new_file(dir).map_err(|e| ("error-detail", e.to_string()))?;
        let store = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).map_err(|e| ("error-detail", e.to_string()))?);
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Staged { store, name, file })
    }
}

/// `{process}-{random}` in `dir`, a name nobody can work out beforehand, made empty and private.
fn new_file(dir: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    use std::hash::{BuildHasher, Hasher};
    let n = std::hash::RandomState::new().build_hasher().finish();
    let file = dir.join(format!("{}-{n:016x}", std::process::id()));
    crate::temp_file::new_private(&file).map(|f| (file, f))
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// The half of a copy shown in the box (`Progress::staging`), cleared however the copy ends.
struct Shown<'a>(&'a Progress);

impl Shown<'_> {
    fn set(&self, s: Staging) {
        *self.0.staging.lock().unwrap() = Some(s);
    }
}

impl Drop for Shown<'_> {
    fn drop(&mut self) {
        *self.0.staging.lock().unwrap() = None;
    }
}

/// The bytes counted for the `n` first of a half: each half counts for half the file, so the
/// bar, the speed and the time left move all the way and the file counts once ("2 GB of 2 GB").
fn half(n: u64, second: bool) -> u64 {
    if second { n.div_ceil(2) } else { n / 2 }
}

/// Passes the chunks on, adding their `half` to `done`; what it added.
async fn counted(mut from: Receiver<std::io::Result<Bytes>>, to: Sender<std::io::Result<Bytes>>, done: &AtomicU64, second: bool) -> u64 {
    let mut seen = 0;
    while let Some(chunk) = from.recv().await {
        if let Ok(b) = &chunk {
            let before = half(seen, second);
            seen += b.len() as u64;
            done.fetch_add(half(seen, second) - before, Ordering::Relaxed);
        }
        if to.send(chunk).await.is_err() {
            break;
        }
    }
    half(seen, second)
}

/// `src` of the phone `from` to `dest` of the same phone `to` (`existed`: whether something is
/// there, `replace::write_file`), `size` bytes, through this computer. Cancel stops the reading at
/// once; the writing, at the end of the file (a phone must receive a file whole, `copy_file`).
/// The copy must be there with all its bytes, or it is an error, and a move keeps its original.
pub(super) async fn copy(from: &Store, src: &str, to: (&Store, &str, Option<bool>), size: u64, p: &Progress) -> Result<(), Fail> {
    let dir = folder().ok_or(Fail::Stop(("error-pc-space", name(src).to_string())))?;
    copy_via(&dir, from, src, to, size, p).await
}

/// `copy`, through a file of `dir`.
async fn copy_via(dir: &Path, from: &Store, src: &str, (to, dest, existed): (&Store, &str, Option<bool>), size: u64, p: &Progress) -> Result<(), Fail> {
    let shown = name(src);
    let staged = Staged::new(dir, (size, free_space(dir)), shown).map_err(Fail::Stop)?;
    if let Store::Mtp(phone) = to
        && phone.free_space(dest).await.is_ok_and(|free| !enough(free, size, PHONE_MARGIN))
    {
        return Err(Fail::Stop(("error-phone-space", shown.to_string())));
    }
    log!("paste: {src:?} to {dest:?} through this computer ({size} bytes)");
    let box_line = Shown(p);
    box_line.set(Staging::Reading);
    let t = std::time::Instant::now();
    let unused = AtomicU64::new(0);
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let (relay, chunks) = tokio::sync::mpsc::channel(4);
    let (read, first, write) = tokio::join!(from.send_chunks(src, size, tx, &unused, &p.cancel), counted(rx, relay, &p.done, false), staged.store.write_chunks(&staged.name, size, chunks));
    let undo = |n: u64| p.done.fetch_sub(n, Ordering::Relaxed);
    if let Err(e) = super::outcome(read, write, &p.cancel) {
        undo(first);
        return Err(e);
    }
    log!("paste: {src:?} read in {:.1} s", t.elapsed().as_secs_f32());
    box_line.set(Staging::Writing(dest.split('/').next().unwrap_or_default().to_string()));
    let never = AtomicBool::new(false);
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let (relay, chunks) = tokio::sync::mpsc::channel(4);
    let (read, second, write) = tokio::join!(staged.store.send_chunks(&staged.name, size, tx, &unused, &never), counted(rx, relay, &p.done, true), crate::replace::write_file(to, dest, size, chunks, existed));
    drop(staged);
    let whole = match super::outcome(read, write, &never) {
        Ok(()) => to.size(dest).await.map_err(Fail::Stop).and_then(|s| if s == size { Ok(()) } else { Err(Fail::Stop(("error-copy-not-whole", shown.to_string()))) }),
        Err(e) => Err(e),
    };
    if let Err(e) = whole {
        log!("paste: {src:?} not written whole: {:?}", match &e { Fail::Stop(e) | Fail::Source(e) => e });
        undo(first + second);
        return Err(e);
    }
    log!("paste: {src:?} through this computer in {:.1} s", t.elapsed().as_secs_f32());
    // Cancel pressed while it was written: the file is whole, and the copy stops after it.
    if p.cancel.load(Ordering::Relaxed) {
        p.files.fetch_add(1, Ordering::Relaxed);
        return Err(Fail::Stop(("error-cancelled", String::new())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A big file goes through the computer between the two storages and in a copy inside one;
    /// a move inside one storage stays a change of folder on the phone; small files stay the phone's.
    #[test]
    fn the_way_of_a_file() {
        let (card, inner) = ("SD card/DCIM/a.mp4", "Internal shared storage/Movies/a.mp4");
        assert!(by_computer(BIG, inner, card, None), "big copy or move across");
        assert!(by_computer(BIG, card, inner, None), "big copy or move back");
        assert!(!by_computer(2 << 30, inner, "Internal shared storage/Download/a.mp4", None), "big copy or move inside: the phone's");
        assert!(!by_computer(BIG - 1, inner, card, None), "small across");
        // A storage whose name another one starts with is another storage.
        assert!(by_computer(BIG, "SD/a.mp4", "SD card/a.mp4", None));
    }

    /// #354 on Windows (prova W5, 27/11): Windows (WPD) waits about 30 s for the phone to copy a
    /// file inside one storage, then loses track of the copy: the phone finished 6 GB by itself
    /// and neither EyeCanSee nor File Explorer saw the file until the phone was plugged in again.
    /// There a file the phone cannot be sure to copy in that time goes through the computer: more
    /// than 1 GB inside the internal storage (the Redmi copies 100 MB/s there: 4 GB took 40 s),
    /// more than 128 MB inside a memory card (16 MB/s to it). Smaller ones stay the phone's, and
    /// on Linux, where the wait for the phone works, every size.
    #[test]
    fn on_windows_a_copy_the_phone_cannot_end_in_time_goes_through_the_computer() {
        use crate::mtp::alone_limit;
        let (inner, card) = ("Internal shared storage/Download/a.mp4", "SD card/Download/a.mp4");
        let way = |size, src, dest: &str, wpd| by_computer(size, src, dest, alone_limit(wpd, dest.starts_with("Internal")));
        for size in [4 << 30, 6 << 30, (1 << 30) + 1] {
            assert!(way(size, "Internal shared storage/DCIM/a.mp4", inner, true), "Windows, {size} bytes inside the internal storage");
            assert!(!way(size, "Internal shared storage/DCIM/a.mp4", inner, false), "Linux, {size} bytes inside the internal storage");
        }
        for size in [1 << 30, 512 << 20, BIG] {
            assert!(!way(size, "Internal shared storage/DCIM/a.mp4", inner, true), "Windows, {size} bytes inside the internal storage: the phone's");
        }
        assert!(way(512 << 20, "SD card/DCIM/a.mp4", card, true), "Windows, 512 MB inside the card");
        assert!(!way(128 << 20, "SD card/DCIM/a.mp4", card, true), "Windows, 128 MB inside the card: the phone's");
        assert!(!way(512 << 20, "SD card/DCIM/a.mp4", card, false), "Linux, inside the card");
        // Between the storages nothing changes: big ones through the computer everywhere.
        assert!(way(BIG, inner, card, true) && way(BIG, inner, card, false) && !way(BIG - 1, inner, card, true));
    }

    #[test]
    fn free_space_needed() {
        assert!(enough(3 << 30, 2 << 30, PC_MARGIN));
        assert!(!enough((2 << 30) + 100, 2 << 30, PC_MARGIN), "the file, but not the margin");
        assert!(free_space(&std::env::temp_dir()).is_some_and(|f| f > 0), "this computer's disk is read");
    }

    /// Both halves together count the file once, whatever the chunks.
    #[test]
    fn halves_count_the_file_once() {
        for size in [0u64, 1, 2, 7, 1001] {
            assert_eq!(half(size, false) + half(size, true), size);
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let done = AtomicU64::new(0);
        let added = rt.block_on(async {
            let mut added = 0;
            for second in [false, true] {
                let (tx, rx) = tokio::sync::mpsc::channel(4);
                let (relay, mut out) = tokio::sync::mpsc::channel(4);
                let send = async move {
                    for n in [3usize, 4, 1, 5] {
                        tx.send(Ok(Bytes::from(vec![0; n]))).await.unwrap();
                    }
                };
                let take = async move { while out.recv().await.is_some() {} };
                added += tokio::join!(send, counted(rx, relay, &done, second), take).1;
            }
            added
        });
        assert_eq!((added, done.load(Ordering::Relaxed)), (13, 13));
    }

    /// A folder of its own for each test (the cache folder of the tests is shared).
    fn own_dir(what: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eyecansee-through-pc-{what}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn files_in(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    /// The file on this computer goes when the copy ends, however it ends; one left by a copy of
    /// the app closed goes at the next start, and this copy's own at exit.
    #[test]
    fn the_file_on_this_computer_is_removed() {
        let dir = own_dir("guard");
        let staged = Staged::new(&dir, (1, None), "a.mp4").unwrap();
        let file = staged.file.clone();
        assert!(file.exists() && file.starts_with(&dir));
        drop(staged);
        assert!(!file.exists(), "removed with its guard");
        let (mine, _) = new_file(&dir).unwrap();
        let other = dir.join("1-0123456789abcdef");
        std::fs::write(&other, b"").unwrap();
        remove_left_in(&dir, Some(std::process::id()));
        assert!(!mine.exists() && other.exists(), "at exit only this copy's");
        remove_left_in(&dir, None);
        assert!(!other.exists(), "at start all of them");
        // Not enough room: said before anything is read or made.
        assert_eq!(Staged::new(&dir, (2 << 30, Some((2 << 30) + 100)), "a.mp4").err(), Some(("error-pc-space", "a.mp4".to_string())));
        assert_eq!(files_in(&dir), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The two halves on two folders of this computer (no phone: the same way, `Store::Dal`):
    /// the copy whole and counted once; Cancel, a source missing and a destination that cannot be
    /// written each leave nothing on this computer and nothing counted.
    #[test]
    fn a_copy_through_this_computer_and_how_it_can_end() {
        let dir = own_dir("copy");
        let (from_dir, to_dir, stage) = (dir.join("from"), dir.join("to"), dir.join("stage"));
        for d in [&from_dir, &to_dir, &stage] {
            std::fs::create_dir_all(d).unwrap();
        }
        let data: Vec<u8> = (0..300_001u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(from_dir.join("a.mp4"), &data).unwrap();
        std::fs::create_dir_all(to_dir.join("folder.mp4")).unwrap();
        let store = |d: &Path| Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&d.to_string_lossy())).unwrap());
        let (from, to) = (store(&from_dir), store(&to_dir));
        let size = data.len() as u64;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let run = |src: &str, dest: &str, cancel: bool| {
            let p = Progress::default();
            p.cancel.store(cancel, Ordering::Relaxed);
            let r = rt.block_on(copy_via(&stage, &from, src, (&to, dest, Some(false)), size, &p));
            let what = match r {
                Ok(()) => "copied",
                Err(Fail::Source(_)) => "source",
                Err(Fail::Stop(e)) if e.0 == "error-cancelled" => "cancelled",
                Err(Fail::Stop(_)) => "stopped",
            };
            assert!(p.staging.lock().unwrap().is_none(), "{what}: the box's line is cleared");
            assert_eq!(files_in(&stage), 0, "{what}: nothing left on this computer");
            (what, p.done.load(Ordering::Relaxed))
        };
        assert_eq!(run("a.mp4", "b.mp4", false), ("copied", size));
        assert_eq!(std::fs::read(to_dir.join("b.mp4")).unwrap(), data);
        assert_eq!(run("a.mp4", "c.mp4", true), ("cancelled", 0));
        assert!(!to_dir.join("c.mp4").exists());
        assert_eq!(run("missing.mp4", "d.mp4", false), ("source", 0));
        assert_eq!(run("a.mp4", "folder.mp4", false), ("stopped", 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A big file to the memory card, copied and moved, and copied inside the first storage, the
    /// way a paste does it (`copy_file`, then the original deleted for a move): each time the
    /// copy is whole, the bytes counted are the file's, the box's line is gone, nothing is left on
    /// this computer and the phone answers after it. A phone with a memory card;
    /// `EYECANSEE_MTP_BIG` a big file on its first storage, left as it is; what the test makes is
    /// deleted at the end. It stops at the first step that fails or takes more than 10 minutes:
    /// `EYECANSEE_MTP=<serial> EYECANSEE_MTP_BIG="<storage>/Download/big.mp4" cargo test --release mtp_big_file_across_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_big_file_across_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        let big = std::env::var("EYECANSEE_MTP_BIG").expect("EYECANSEE_MTP_BIG not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Store::Mtp(crate::mtp::Phone::shared(&serial));
            let root = phone.list("").await.unwrap();
            let (first, second) = (root.dirs[0].clone(), root.dirs[1].clone());
            let size = phone.size(&big).await.unwrap();
            assert!(size >= BIG, "EYECANSEE_MTP_BIG: {size} bytes, not a big file");
            let (copied, here, moved) = (format!("{second}EyeCanSee-big-copy.mp4"), format!("{first}EyeCanSee-big-here.mp4"), format!("{second}EyeCanSee-big-moved.mp4"));
            let run = async {
                live_copy(&phone, "copy to the card", &big, &copied, size).await?;
                live_copy(&phone, "copy inside the first storage", &big, &here, size).await?;
                assert!(by_computer(size, &here, &moved, None), "a move across goes through the computer");
                live_copy(&phone, "move to the card", &here, &moved, size).await?;
                step("delete the original of the move", phone.delete(&here, crate::source::Deletion::Permanent)).await?;
                let left = phone.exists(&here).await;
                if left {
                    return Err(format!("after the move: {here} left"));
                }
                Ok(())
            };
            let r = run.await;
            for p in [&copied, &here, &moved] {
                if phone.exists(p).await {
                    let _ = phone.delete(p, crate::source::Deletion::Permanent).await;
                }
            }
            assert!(r.is_ok(), "{r:?}");
        });
    }

    /// A big file copied inside its own storage, by the phone itself as a paste does it (fast
    /// there: 6 GB in about a minute on a Redmi Note 9 Pro, past the 30 s mtp-rs waits): the copy
    /// ends up whole, counted once, and the phone answers after it. `EYECANSEE_MTP_BIG` is a big file, left as it is; the copy is deleted:
    /// `EYECANSEE_MTP=<serial> EYECANSEE_MTP_BIG="<storage>/Download/big.mp4" cargo test --release mtp_big_file_inside_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_big_file_inside_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        let big = std::env::var("EYECANSEE_MTP_BIG").expect("EYECANSEE_MTP_BIG not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Store::Mtp(crate::mtp::Phone::shared(&serial));
            phone.list("").await.unwrap();
            let size = phone.size(&big).await.unwrap();
            let copy = format!("{}EyeCanSee-big-inside.mp4", crate::source::parent(&big));
            assert!(!by_computer(size, &big, &copy, None), "inside one storage: the phone's");
            let p = Progress::default();
            let r = async {
                step("copy inside, by the phone", async { super::super::copy_file(&phone, &big, (&phone, &copy, Some(false)), size, &p).await.map_err(|(Fail::Stop(e) | Fail::Source(e))| e) }).await?;
                let there = step("copy inside: the size there", phone.size(&copy)).await?;
                let counted = p.done.load(Ordering::Relaxed);
                if there != size || counted != size {
                    return Err(format!("copy inside: {there} bytes there of {size}, {counted} counted"));
                }
                let listed = async { phone.list(&crate::source::parent(&copy)).await.map(|l| l.files.len()) };
                step("copy inside: the phone answers", listed).await.map(|_| ())
            }
            .await;
            if phone.exists(&copy).await {
                let _ = phone.delete(&copy, crate::source::Deletion::Permanent).await;
            }
            assert!(r.is_ok(), "{r:?}");
        });
    }

    /// Cancel while a big file is read from the phone (P5 of 26/11: every listing after it stayed
    /// waiting for ever): the copy stops, and the phone answers the next call. A phone with a
    /// memory card; `EYECANSEE_MTP_BIG` a big file on its first storage, left as it is:
    /// `EYECANSEE_MTP=<serial> EYECANSEE_MTP_BIG="<storage>/Download/big.mp4" cargo test --release mtp_cancel_while_reading_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn mtp_cancel_while_reading_live() {
        let serial = std::env::var("EYECANSEE_MTP").expect("EYECANSEE_MTP not set");
        let big = std::env::var("EYECANSEE_MTP_BIG").expect("EYECANSEE_MTP_BIG not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let phone = Store::Mtp(crate::mtp::Phone::shared(&serial));
            let root = phone.list("").await.unwrap();
            let size = phone.size(&big).await.unwrap();
            let dest = format!("{}EyeCanSee-big-cancelled.mp4", root.dirs[1]);
            let p = Progress::default();
            let cancel = async {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                p.cancel.store(true, Ordering::Relaxed);
            };
            let (r, ()) = tokio::join!(copy(&phone, &big, (&phone, &dest, Some(false)), size, &p), cancel);
            assert!(matches!(r, Err(Fail::Stop((id, _))) if id == "error-cancelled"), "{:?}", r.err().map(|(Fail::Stop(e) | Fail::Source(e))| e));
            let listed = tokio::time::timeout(std::time::Duration::from_secs(20), phone.list(&crate::source::parent(&big))).await;
            assert!(matches!(listed, Ok(Ok(_))), "the phone did not answer after Cancel: {:?}", listed.map(|l| l.map(|l| l.files.len())));
            assert!(!phone.exists(&dest).await, "nothing left at the destination");
        });
    }

    /// One step of a live test: its result, or why it stopped (an error, or 10 minutes).
    async fn step<T: std::fmt::Debug>(what: &str, call: impl std::future::Future<Output = Result<T, Error>>) -> Result<T, String> {
        let t = std::time::Instant::now();
        let r = tokio::time::timeout(std::time::Duration::from_secs(600), call).await;
        println!("{what}: {r:?} in {:?}", t.elapsed());
        r.map_err(|_| format!("{what}: no answer in 10 minutes"))?.map_err(|e| format!("{what}: {e:?}"))
    }

    /// `src` copied to `dest` on the same phone as a paste does it, checked: whole, counted once,
    /// nothing left on this computer, the phone answering.
    async fn live_copy(phone: &Store, what: &str, src: &str, dest: &str, size: u64) -> Result<(), String> {
        let p = Progress::default();
        let copied = async { copy(phone, src, (phone, dest, Some(false)), size, &p).await.map_err(|(Fail::Stop(e) | Fail::Source(e))| e) };
        step(what, copied).await?;
        let there = step(&format!("{what}: the size there"), phone.size(dest)).await?;
        let left = std::fs::read_dir(folder().unwrap()).unwrap().count();
        let counted = p.done.load(Ordering::Relaxed);
        if there != size || counted != size || left != 0 || p.staging.lock().unwrap().is_some() {
            return Err(format!("{what}: {there} bytes there of {size}, {counted} counted, {left} files left on this computer"));
        }
        let listed = async { phone.list(&crate::source::parent(dest)).await.map(|l| l.files.len()) };
        step(&format!("{what}: the phone answers"), listed).await.map(|_| ())
    }
}
