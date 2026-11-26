//! A paste the phone copies by itself inside one of its storages (#354): while it copies a file
//! it answers nothing and says nothing of how far it is, so the copy window becomes a modal in
//! the middle of the app for the whole paste (Piero, 26/11 night). It shows only what is known:
//! the file under way with its seconds, the files and bytes done (counted at the end of each
//! file), and a speed only once a big file of this paste has been copied.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::through_pc::storage;

/// After this long on one file the window says the phone goes on until it is done.
pub const LONG_SECS: u64 = 180;

/// Whether a paste is one the phone copies by itself inside one storage (its window modal): a
/// copy, not a move (inside one storage a move only changes the folder, at once; one that has to
/// merge a folder stays in the corner box too), on the same phone (`same_phone`), with an item
/// whose storage is the one of `to_dir`. One such item is enough: the phone is busy then.
pub(super) fn by_phone(cut: bool, same_phone: bool, paths: &[String], to_dir: &str) -> bool {
    !cut && same_phone && paths.iter().any(|p| storage(p) == storage(to_dir))
}

/// What the window of such a paste reads while it runs, shared with the copy (`Progress::by_phone`).
#[derive(Default)]
pub struct ByPhone {
    /// The paste is one (`by_phone`).
    on: AtomicBool,
    /// The phone went from the devices during the paste: the window goes back to the corner, so
    /// the app is never held by a phone that is not there (Windows does not say it went at once).
    unplugged: AtomicBool,
    /// The file the phone was last given: name, size, since when. Kept after its end: the window
    /// names it after Cancel.
    file: Mutex<Option<(String, u64, Instant)>>,
    /// Big files (`mtp::LONG_COPY` and more) the phone finished in this paste: bytes and seconds.
    big: Mutex<(u64, f64)>,
}

impl ByPhone {
    pub(crate) fn decide(&self, on: bool) {
        self.on.store(on, Ordering::Relaxed);
    }

    /// The window of this paste is the modal one.
    pub fn modal(&self) -> bool {
        self.on.load(Ordering::Relaxed) && !self.unplugged.load(Ordering::Relaxed)
    }

    /// The phone of the paste is no longer among the devices.
    pub fn unplugged(&self) {
        self.unplugged.store(true, Ordering::Relaxed);
    }

    /// The phone is given the file `name` of `size` bytes to copy.
    pub(crate) fn begin(&self, name: &str, size: u64) {
        *self.file.lock().unwrap() = Some((name.to_string(), size, Instant::now()));
    }

    /// The phone finished the file of `size` bytes it was given last.
    pub(super) fn end(&self, size: u64) {
        let began = self.file.lock().unwrap().as_ref().map(|(_, _, at)| at.elapsed().as_secs_f64());
        if let Some(secs) = began.filter(|_| size >= crate::mtp::LONG_COPY) {
            let mut big = self.big.lock().unwrap();
            *big = (big.0 + size, big.1 + secs);
        }
    }

    /// The file under way: name, size, whole seconds since the phone was given it.
    pub fn file(&self) -> Option<(String, u64, u64)> {
        self.file.lock().unwrap().as_ref().map(|(n, s, at)| (n.clone(), *s, at.elapsed().as_secs()))
    }

    /// Bytes and seconds of the big files finished in this paste; `None` before the first.
    pub fn big(&self) -> Option<(u64, f64)> {
        Some(*self.big.lock().unwrap()).filter(|(bytes, secs)| *bytes > 0 && *secs > 0.0)
    }
}

/// What the window knows at a moment, as plain numbers (`lines`).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// The file under way: name, size, seconds.
    pub file: Option<(String, u64, u64)>,
    /// Files finished, and in all.
    pub files: usize,
    pub files_total: usize,
    /// Bytes of the files finished, and in all.
    pub done: u64,
    pub total: u64,
    /// `ByPhone::big`.
    pub big: Option<(u64, f64)>,
    /// Cancel was pressed.
    pub cancelling: bool,
}

/// A line of the window, in the order shown; the UI gives it its words.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// The file under way, its size and its seconds.
    File { name: String, size: u64, secs: u64 },
    /// "File n of total": `n` is the file under way (the last one once all are done).
    Counts { n: usize, total: usize, done: u64, size: u64 },
    /// The part done, by bytes of whole files (by files when they hold no bytes).
    Bar(f32),
    /// One file only: the phone does not say how far it is.
    NoBar,
    /// Bytes per second of the big files done, and seconds left at that speed.
    Speed { speed: u64, left: u64 },
    /// More than `LONG_SECS` on the file under way.
    Long,
    /// Cancel pressed: stopping after the file named.
    Stopping(String),
}

/// The lines of the window for `s`: only what is known, nothing guessed.
pub fn lines(s: &Seen) -> Vec<Line> {
    let mut out = Vec::new();
    if let Some((name, size, secs)) = &s.file {
        out.push(Line::File { name: name.clone(), size: *size, secs: *secs });
    }
    let n = (s.files + 1).min(s.files_total.max(1));
    out.push(Line::Counts { n, total: s.files_total, done: s.done, size: s.total });
    out.push(if s.files_total > 1 { Line::Bar(part(s)) } else { Line::NoBar });
    if let Some((bytes, secs)) = s.big {
        let speed = bytes as f64 / secs;
        out.push(Line::Speed { speed: speed as u64, left: (s.total.saturating_sub(s.done) as f64 / speed).round() as u64 });
    }
    if s.file.as_ref().is_some_and(|(_, _, secs)| *secs >= LONG_SECS) {
        out.push(Line::Long);
    }
    if s.cancelling
        && let Some((name, _, _)) = &s.file
    {
        out.push(Line::Stopping(name.clone()));
    }
    out
}

/// The part of the bar filled: bytes of whole files, or files when none holds a byte.
fn part(s: &Seen) -> f32 {
    match (s.total, s.files_total) {
        (0, 0) => 0.0,
        (0, files) => s.files as f32 / files as f32,
        (total, _) => s.done as f32 / total as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    /// #354: modal for a copy inside one storage of the same phone, of any size; not for a move,
    /// another device, or a copy to the other storage (big files there go through the computer,
    /// small ones take seconds). One item of the storage is enough.
    #[test]
    fn modal_for_a_copy_inside_one_storage() {
        let inside = paths(&["Internal storage/DCIM/a.jpg"]);
        assert!(by_phone(false, true, &inside, "Internal storage/Download/"));
        assert!(!by_phone(true, true, &inside, "Internal storage/Download/"), "a move is a change of folder");
        assert!(!by_phone(false, false, &inside, "Internal storage/Download/"), "another device");
        assert!(!by_phone(false, true, &inside, "SD card/Download/"), "the other storage");
        let both = paths(&["SD card/", "Internal storage/"]);
        assert!(by_phone(false, true, &both, "Internal storage/Download/"), "one item of that storage");
    }

    fn seen(files: usize, files_total: usize, done: u64, total: u64) -> Seen {
        Seen { file: Some(("big.mp4".into(), 2048 * MB, 12)), files, files_total, done, total, ..Seen::default() }
    }

    /// Several files: the file under way with its seconds, "File 2 of 5" while the second is
    /// copied, the bar by whole files; no speed before a big file is done.
    #[test]
    fn several_files() {
        let l = lines(&seen(1, 5, 100 * MB, 400 * MB));
        assert_eq!(l[0], Line::File { name: "big.mp4".into(), size: 2048 * MB, secs: 12 });
        assert_eq!(l[1], Line::Counts { n: 2, total: 5, done: 100 * MB, size: 400 * MB });
        assert_eq!(l[2], Line::Bar(0.25));
        assert_eq!(l.len(), 3, "nothing guessed: {l:?}");
        // All done, waiting for the end: the last file, not "6 of 5".
        assert!(lines(&seen(5, 5, 400 * MB, 400 * MB)).contains(&Line::Counts { n: 5, total: 5, done: 400 * MB, size: 400 * MB }));
        // Empty files only: the bar goes by files.
        assert!(lines(&seen(2, 4, 0, 0)).contains(&Line::Bar(0.5)));
    }

    /// One file: no bar, the line that says why.
    #[test]
    fn one_file_has_no_bar() {
        let l = lines(&seen(0, 1, 0, 2048 * MB));
        assert!(l.contains(&Line::NoBar) && !l.iter().any(|x| matches!(x, Line::Bar(_))), "{l:?}");
        assert!(l.contains(&Line::Counts { n: 1, total: 1, done: 0, size: 2048 * MB }));
    }

    /// The speed only from the big files done in this paste: small ones never count (the average
    /// over them misled, P6), and before the first big one there is none. The time left is what is
    /// left at that speed.
    #[test]
    fn speed_only_from_big_files_done() {
        let b = ByPhone::default();
        b.begin("small.jpg", 4 * MB);
        b.end(4 * MB);
        assert_eq!(b.big(), None, "a small file gives no speed");
        b.begin("big.mp4", 64 * MB);
        b.end(64 * MB);
        let (bytes, secs) = b.big().expect("a big file done");
        assert_eq!(bytes, 64 * MB);
        assert!(secs >= 0.0);
        let s = Seen { big: Some((200 * MB, 2.0)), ..seen(1, 3, 200 * MB, 600 * MB) };
        assert!(lines(&s).contains(&Line::Speed { speed: 100 * MB, left: 4 }));
    }

    /// After 3 minutes on one file, and after Cancel, their lines; not before.
    #[test]
    fn long_file_and_cancel() {
        let mut s = seen(0, 2, 0, 4096 * MB);
        assert!(!lines(&s).contains(&Line::Long));
        s.file = Some(("big.mp4".into(), 2048 * MB, LONG_SECS));
        assert!(lines(&s).contains(&Line::Long));
        assert!(!lines(&s).iter().any(|l| matches!(l, Line::Stopping(_))));
        s.cancelling = true;
        assert_eq!(lines(&s).last(), Some(&Line::Stopping("big.mp4".into())));
    }

    /// The phone gone from the devices: the window is no longer modal.
    #[test]
    fn unplugged_is_not_modal() {
        let b = ByPhone::default();
        assert!(!b.modal());
        b.decide(true);
        assert!(b.modal());
        b.unplugged();
        assert!(!b.modal());
    }
}
