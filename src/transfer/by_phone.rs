//! A paste the phone copies by itself inside one of its storages (#354): while it copies a file
//! it answers nothing and says nothing of how far it is, so the copy window becomes a modal in
//! the middle of the app for the whole paste (Piero, 26/11 night). It shows only what is known
//! (`view`): the file under way with its seconds, with several files the files and bytes done
//! (counted at the end of each file), and a speed only once a big file of this paste has been
//! copied. Cancel during a file: the phone ends it, then the copy it made goes (`ByPhone::undone`).

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

/// Whether the items pasted may be several files: more than one, or a folder. Known before
/// anything is counted, so the window has the rows of the counts from its first frame, and one
/// size until it closes.
pub(super) fn many(paths: &[String]) -> bool {
    paths.len() > 1 || paths.iter().any(|p| p.ends_with('/'))
}

/// What the window of such a paste reads while it runs, shared with the copy (`Progress::by_phone`).
#[derive(Default)]
pub struct ByPhone {
    /// The paste is one (`by_phone`).
    on: AtomicBool,
    /// Its items may be several files (`many`).
    many: AtomicBool,
    /// The phone went from the devices during the paste: the window goes back to the corner, so
    /// the app is never held by a phone that is not there (Windows does not say it went at once).
    unplugged: AtomicBool,
    /// The file the phone was last given: name, size, since when. Kept after its end, until the
    /// next one: the window does not blink between two files.
    file: Mutex<Option<(String, u64, Instant)>>,
    /// Big files (`mtp::LONG_COPY` and more) the phone finished in this paste: bytes and seconds.
    big: Mutex<(u64, f64)>,
}

impl ByPhone {
    pub(crate) fn decide(&self, on: bool, many: bool) {
        self.on.store(on, Ordering::Relaxed);
        self.many.store(many, Ordering::Relaxed);
    }

    /// The items pasted may be several files (`many`).
    pub fn many(&self) -> bool {
        self.many.load(Ordering::Relaxed)
    }

    /// The window of this paste is the modal one.
    pub fn modal(&self) -> bool {
        self.on.load(Ordering::Relaxed) && !self.unplugged.load(Ordering::Relaxed)
    }

    /// The phone of the paste is no longer among the devices.
    pub fn unplugged(&self) {
        self.unplugged.store(true, Ordering::Relaxed);
    }

    /// Whether Cancel during the copy of `src` to `dest` removes the copy the phone made: a paste
    /// of this kind, and a copy that stays inside one storage (Piero, 26/11 night: Cancel means
    /// "not the file under way"; the files done before stay).
    pub(super) fn undone(&self, src: &str, dest: &str) -> bool {
        self.on.load(Ordering::Relaxed) && storage(src) == storage(dest)
    }

    /// The phone is given the file `name` of `size` bytes to copy.
    pub(crate) fn begin(&self, name: &str, size: u64) {
        *self.file.lock().unwrap() = Some((name.to_string(), size, Instant::now()));
    }

    /// `begin`, `secs` seconds ago: for the tests of the window.
    #[cfg(test)]
    pub(crate) fn begin_ago(&self, name: &str, size: u64, secs: u64) {
        let at = Instant::now().checked_sub(std::time::Duration::from_secs(secs)).expect("a clock that old");
        *self.file.lock().unwrap() = Some((name.to_string(), size, at));
    }

    /// `big` as given: for the tests of the window.
    #[cfg(test)]
    pub(crate) fn set_big(&self, bytes: u64, secs: f64) {
        *self.big.lock().unwrap() = (bytes, secs);
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

/// From this many seconds on one file the window says them: under it they only blink by.
pub const SAY_SECS: u64 = 2;

/// What the window knows at a moment, as plain numbers (`view`).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// The file the phone was given last: name, size, seconds.
    pub file: Option<(String, u64, u64)>,
    /// The name of the file of the paste under way (`Progress::current`).
    pub current: String,
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
    /// A big file of the other storage is on its way through this computer now (a paste of items
    /// of both storages): its bytes are counted as they go, and the phone is not copying alone.
    pub through_pc: bool,
    /// The items pasted may be several files (`ByPhone::many`).
    pub many: bool,
    /// The files are still being counted, before the copy: items found so far.
    pub counting: Option<usize>,
    /// The wait for the phone was asked to end (`mtp::leaving`).
    pub leaving: bool,
}

/// The way out of the wait for the phone, once Cancel is pressed (#354: the wait could last
/// very long, and nothing ended it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Leave {
    /// Not there: Cancel not pressed, or no file given to the phone yet.
    No,
    /// "Stop waiting" is to press.
    Offered,
    /// Pressed: the wait ends at its next look.
    Asked,
}

/// The bar of the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bar {
    /// One file: nothing is known of how far it is; a strip that moves says the phone works.
    Busy,
    /// Several files: the part done, by bytes of whole files (by files when they hold no bytes);
    /// `busy`: the phone is on a file now.
    Part { part: f32, busy: bool },
}

/// The bytes line of several files.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Data {
    /// No file finished yet: only how much there is in all ("0 KB of 6 GB" told nothing true).
    Total(u64),
    /// Bytes of the files finished, and in all.
    Done { done: u64, total: u64 },
}

/// What the explanation at the bottom says, one at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Note {
    /// Why the app waits.
    Waiting,
    /// More than `LONG_SECS` on the file under way.
    Long,
    /// Cancel pressed: the phone ends the file, then its copy goes; `others` files never start.
    Cancelled { others: usize },
    /// "Stop waiting" pressed too: what the phone makes of the file stays.
    Left,
}

/// What the window shows for a `Seen`; the UI gives it its words and its place. With one file
/// `data` and `speed` have no row; with several (`rows`) their rows are there even when empty.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    /// The files are still being counted (the window is there from the start of the paste: the
    /// copy box in the corner showed first, for a second; Piero, 26/11): items found so far.
    pub counting: Option<usize>,
    /// The rows of the bytes and of the speed are there: the items may be several files.
    pub rows: bool,
    /// The file under way.
    pub name: String,
    /// With several files: the file under way among them, from 1, and how many.
    pub nth: Option<(usize, usize)>,
    /// The size of the file the phone is copying, and its seconds from `SAY_SECS` on.
    pub file: Option<(u64, Option<u64>)>,
    pub bar: Bar,
    /// With several files that hold bytes.
    pub data: Option<Data>,
    /// Bytes per second of the big files done, and seconds left at that speed.
    pub speed: Option<(u64, u64)>,
    pub note: Note,
    /// Cancel is still to press (`false` once pressed: its button gives way to what happens).
    pub cancel: bool,
    pub leave: Leave,
}

/// The window for `s`: only what is known, nothing guessed.
pub fn view(s: &Seen) -> View {
    let rows = s.many || s.files_total > 1;
    if s.counting.is_some() {
        // Nothing of the copy is known yet, and Cancel stops before the first file.
        let (file, bar, note) = (None, Bar::Busy, Note::Waiting);
        return View { counting: s.counting, rows, name: String::new(), nth: None, file, bar, data: None, speed: None, note, cancel: !s.cancelling, leave: Leave::No };
    }
    let several = s.files_total > 1;
    // The file under way among all: the last one once all are done (a sidecar after its photo).
    let n = (s.files + 1).min(s.files_total.max(1));
    let phone = s.file.as_ref().filter(|_| !s.through_pc);
    let speed = s.big.filter(|_| several).map(|(bytes, secs)| {
        let speed = bytes as f64 / secs;
        (speed as u64, (s.total.saturating_sub(s.done) as f64 / speed).round() as u64)
    });
    let leave = match (s.cancelling && phone.is_some(), s.leaving) {
        (false, _) => Leave::No,
        (true, false) => Leave::Offered,
        (true, true) => Leave::Asked,
    };
    let note = if leave == Leave::Asked {
        Note::Left
    } else if s.cancelling {
        Note::Cancelled { others: s.files_total.saturating_sub(n) }
    } else if phone.is_some_and(|(_, _, secs)| *secs >= LONG_SECS) {
        Note::Long
    } else {
        Note::Waiting
    };
    View {
        counting: None,
        rows,
        name: phone.map_or_else(|| s.current.clone(), |(name, _, _)| name.clone()),
        nth: several.then_some((n, s.files_total)),
        file: phone.map(|(_, size, secs)| (*size, Some(*secs).filter(|secs| *secs >= SAY_SECS))),
        bar: if rows || s.through_pc { Bar::Part { part: part(s), busy: phone.is_some() } } else { Bar::Busy },
        data: (several && s.total > 0).then_some(if s.done > 0 { Data::Done { done: s.done, total: s.total } } else { Data::Total(s.total) }),
        speed,
        note,
        cancel: !s.cancelling,
        leave,
    }
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
    const GB: u64 = 1 << 30;

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
        Seen { file: Some(("big.mp4".into(), 2 * GB, 12)), current: "big.mp4".into(), files, files_total, done, total, ..Seen::default() }
    }

    /// One file (Piero, 26/11: "why do I see 0 KB?" at "File 1 of 1, 0 KB of 6144,0 MB"): its
    /// name, its size and its seconds, the strip that moves; no count of files, no bytes done, no
    /// speed, whatever the numbers say. Cancel is there: it removes the copy at its end.
    #[test]
    fn one_file_shows_no_counts() {
        let v = view(&Seen { big: Some((GB, 10.0)), ..seen(0, 1, 0, 6 * GB) });
        assert_eq!(v, View { counting: None, rows: false, name: "big.mp4".into(), nth: None, file: Some((2 * GB, Some(12))), bar: Bar::Busy, data: None, speed: None, note: Note::Waiting, cancel: true, leave: Leave::No });
    }

    /// Several files: "File 2 of 5" while the second is copied, the bar by whole files with the
    /// phone busy; the bytes done only once a file is done (before: how much in all); no speed
    /// before a big file is done.
    #[test]
    fn several_files() {
        let v = view(&seen(1, 5, 100 * MB, 400 * MB));
        assert_eq!((v.nth, v.bar), (Some((2, 5)), Bar::Part { part: 0.25, busy: true }));
        assert_eq!((v.data, v.speed), (Some(Data::Done { done: 100 * MB, total: 400 * MB }), None));
        assert!(v.rows && v.cancel);
        let first = view(&seen(0, 5, 0, 400 * MB));
        assert_eq!((first.nth, first.data), (Some((1, 5)), Some(Data::Total(400 * MB))), "nothing done yet: no \"0 KB of\"");
        // All done, waiting for the end: the last file, not "6 of 5".
        assert_eq!(view(&seen(5, 5, 400 * MB, 400 * MB)).nth, Some((5, 5)));
        // Empty files only: the bar goes by files, and there are no bytes to say.
        let empty = view(&seen(2, 4, 0, 0));
        assert_eq!((empty.bar, empty.data), (Bar::Part { part: 0.5, busy: true }, None));
    }

    /// The seconds of a file only from `SAY_SECS` on: small files go by in less.
    #[test]
    fn seconds_only_once_worth_saying() {
        let mut s = seen(0, 3, 0, 3 * GB);
        s.file = Some(("a.jpg".into(), 4 * MB, SAY_SECS - 1));
        assert_eq!(view(&s).file, Some((4 * MB, None)));
        s.file = Some(("a.jpg".into(), 4 * MB, SAY_SECS));
        assert_eq!(view(&s).file, Some((4 * MB, Some(SAY_SECS))));
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
        assert_eq!(view(&s).speed, Some((100 * MB, 4)));
    }

    /// After 3 minutes on one file the explanation says so; not before.
    #[test]
    fn long_file() {
        let mut s = seen(0, 1, 0, 2 * GB);
        assert_eq!(view(&s).note, Note::Waiting);
        s.file = Some(("big.mp4".into(), 2 * GB, LONG_SECS));
        assert_eq!(view(&s).note, Note::Long);
    }

    /// After Cancel: no button any more, and what happens, with the files that never start (the
    /// file under way is not one of them: the phone ends it and its copy goes). One file, or the
    /// last one: none. It comes before the 3 minutes.
    #[test]
    fn after_cancel() {
        let cancelled = |files, total| view(&Seen { cancelling: true, ..seen(files, total, 0, 6 * GB) });
        let v = cancelled(1, 5);
        assert_eq!((v.note, v.cancel, v.leave), (Note::Cancelled { others: 3 }, false, Leave::Offered));
        assert_eq!(cancelled(3, 5).note, Note::Cancelled { others: 1 });
        assert_eq!(cancelled(4, 5).note, Note::Cancelled { others: 0 }, "the last file");
        assert_eq!(cancelled(5, 5).note, Note::Cancelled { others: 0 }, "all done");
        assert_eq!(cancelled(0, 1).note, Note::Cancelled { others: 0 }, "one file");
        let long = Seen { cancelling: true, file: Some(("big.mp4".into(), 2 * GB, LONG_SECS)), ..seen(0, 2, 0, 4 * GB) };
        assert_eq!(view(&long).note, Note::Cancelled { others: 1 });
    }

    /// #354: once Cancel is pressed the wait for the phone has a way out, "Stop waiting", until
    /// it is pressed too. Not before Cancel, nor while the files are counted or a file goes
    /// through the computer (Cancel stops those by itself).
    #[test]
    fn the_way_out_of_the_wait() {
        let s = |cancelling, leaving| Seen { cancelling, leaving, ..seen(0, 1, 0, 6 * GB) };
        assert_eq!(view(&s(false, false)).leave, Leave::No);
        assert_eq!(view(&s(false, true)).leave, Leave::No, "asked during another paste");
        assert_eq!(view(&s(true, false)).leave, Leave::Offered);
        let asked = view(&s(true, true));
        assert_eq!((asked.leave, asked.note), (Leave::Asked, Note::Left), "the copy is no longer removed: said");
        assert_eq!(view(&Seen { counting: Some(3), ..s(true, false) }).leave, Leave::No, "counting");
        assert_eq!(view(&Seen { through_pc: true, ..s(true, false) }).leave, Leave::No, "through the computer");
    }

    /// A big file of the other storage going through this computer in the same paste: its name,
    /// no seconds of the phone's last file, the bar by the bytes that move, no strip.
    #[test]
    fn a_file_through_the_computer() {
        let s = Seen { through_pc: true, current: "other.mkv".into(), file: Some(("big.mp4".into(), 2 * GB, LONG_SECS + 5)), ..seen(1, 2, GB, 4 * GB) };
        let v = view(&s);
        assert_eq!((v.name.as_str(), v.file, v.bar, v.note), ("other.mkv", None, Bar::Part { part: 0.25, busy: false }, Note::Waiting));
    }

    /// Before the phone is given its first file (folders made first): the name of the paste's file.
    #[test]
    fn before_the_first_file() {
        let v = view(&Seen { file: None, current: "a.jpg".into(), files_total: 1, ..Seen::default() });
        assert_eq!((v.name.as_str(), v.file, v.bar), ("a.jpg", None, Bar::Busy));
    }

    /// While the files are counted (the window is there from the start of the paste): nothing of
    /// the copy, the strip that moves, the items found; the rows of the counts already there when
    /// the items may be several files (more than one, or a folder), so the window keeps its size
    /// when the count ends, also for a folder that holds one file. Cancel there promises nothing
    /// about a file.
    #[test]
    fn counting_the_files() {
        assert!(!many(&paths(&["Internal storage/DCIM/a.mp4"])));
        assert!(many(&paths(&["Internal storage/DCIM/a.mp4", "Internal storage/DCIM/b.mp4"])));
        assert!(many(&paths(&["Internal storage/DCIM/Camera/"])));
        let counting = |many, cancelling| view(&Seen { counting: Some(7), many, cancelling, current: "Camera".into(), ..Seen::default() });
        let one = counting(false, false);
        assert_eq!(one, View { counting: Some(7), rows: false, name: String::new(), nth: None, file: None, bar: Bar::Busy, data: None, speed: None, note: Note::Waiting, cancel: true, leave: Leave::No });
        assert!(counting(true, false).rows);
        let cancelled = counting(true, true);
        assert_eq!((cancelled.note, cancelled.cancel), (Note::Waiting, false));
        // Counted: a folder that held one file keeps the rows, with nothing in them.
        let v = view(&Seen { many: true, ..seen(0, 1, 0, 2 * GB) });
        assert_eq!((v.rows, v.nth, v.data, v.bar), (true, None, None, Bar::Part { part: 0.0, busy: true }));
    }

    /// Cancel removes the copy under way only in a paste of this kind, for a copy that stays
    /// inside one storage: a small file the phone copies to its other storage is as before.
    #[test]
    fn cancel_removes_only_copies_inside_one_storage() {
        let b = ByPhone::default();
        assert!(!b.undone("Internal storage/DCIM/a.mp4", "Internal storage/Download/a.mp4"), "not a paste of this kind");
        b.decide(true, false);
        assert!(b.undone("Internal storage/DCIM/a.mp4", "Internal storage/Download/a.mp4"));
        assert!(!b.undone("SD card/DCIM/a.jpg", "Internal storage/Download/a.jpg"));
    }

    /// The phone gone from the devices: the window is no longer modal.
    #[test]
    fn unplugged_is_not_modal() {
        let b = ByPhone::default();
        assert!(!b.modal());
        b.decide(true, true);
        assert!(b.modal() && b.many());
        b.unplugged();
        assert!(!b.modal());
    }
}
