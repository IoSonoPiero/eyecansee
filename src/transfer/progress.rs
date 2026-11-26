//! What a paste tells the UI while it runs (`Progress`): counts, bytes, the file under way, what
//! was left out, and the phase the copy box shows.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use super::Staging;
use super::by_phone::ByPhone;
use crate::source::Error;

/// Shared with the UI, which shows it while the copy runs.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub files: AtomicUsize,
    pub files_total: AtomicUsize,
    /// Items found while looking inside the folders, before the copy starts.
    pub found: AtomicUsize,
    /// Links to folders the walk of a server left out (`ftp::Server::walk`, `sftp::Server::walk`),
    /// as walked: said at the end of the copy, and left where they were by a move (#183).
    pub links_skipped: Mutex<Vec<String>>,
    /// Destination folders that got something new since the UI last looked: it reads them again,
    /// so a copy shows up while it runs.
    pub touched: Mutex<Vec<String>>,
    /// Seconds spent looking, for the log.
    pub looked: Mutex<f64>,
    /// When the copy itself started, after looking: the UI works out speed and time left from it.
    pub started: OnceLock<Instant>,
    /// When the paste began, counting its looking inside folders and the phone working by itself
    /// (`App::tell_transfer`: a long one says when it ends).
    pub begun: OnceLock<Instant>,
    /// Name of the file being copied.
    pub current: Mutex<String>,
    pub cancel: AtomicBool,
    /// The speed of the last seconds, as the UI shows it.
    pub recent: Mutex<Recent>,
    /// Items of a phone or a server left out because of their name (`allowed`), as found: a
    /// message at the end of the copy, which is not an error (the rest was copied), and a move
    /// keeps the folders that hold them.
    pub refused: Mutex<Vec<String>>,
    /// The items of the plan made in the folder of arrival, by their names there, in the order
    /// made: the UI selects the last one (#323). Not what is copied below a folder.
    pub made: Mutex<Vec<String>>,
    /// What a move left in the folders it moved without copying it (`originals`): items that
    /// arrived while it ran, or a folder holding what cannot be seen. They stay, with their
    /// folders, and a message at the end of the copy names them (#194).
    pub left: Mutex<Vec<String>>,
    /// While a move removes the originals of a folder it copied (`originals::delete`): how many
    /// files are to go, and how many went. 0 when not removing: the copy goes on, or it ended.
    pub removing: AtomicUsize,
    pub removed: AtomicUsize,
    /// Files whose source could not be read, with why, as found: left out, the rest copied, a
    /// message at the end names them, and a move keeps them where they were (L17 of 2026-11-21:
    /// a link a server listed as a file stopped the copy, and the files after it were never
    /// copied, with only the error to say so).
    pub failed: Mutex<Vec<(String, Error)>>,
    /// Items answered Skip, their name already there (`skips`): said at the end (#356).
    pub skipped: AtomicUsize,
    /// Folders a move took whole, as a change of place (`renames`): among the files in the copy
    /// box, said as folders at the end (#356: "1 file moved" for a folder).
    pub folders_moved: AtomicUsize,
    /// A big file of a phone on its way to the same phone through this computer, which half
    /// (`through_pc`): the copy box says it.
    pub staging: Mutex<Option<Staging>>,
    /// A copy the phone makes by itself inside one storage: its window is a modal (`by_phone`).
    pub by_phone: ByPhone,
}

/// What the copy box shows (`Progress::phase`).
#[derive(Debug, PartialEq)]
pub enum Phase {
    /// Looking inside the folders, before the copy: items found.
    Looking(usize),
    /// Copying: files and bytes, copied and in all.
    Copying { files: usize, files_total: usize, done: u64, total: u64 },
    /// Removing the originals of a folder moved: files gone and to go. No bytes move then, so
    /// no speed and no time left (#194: a speed falling to nothing showed at 100%).
    Removing { removed: usize, total: usize },
}

impl Phase {
    /// The part of the bar filled: bytes copied, or originals removed.
    pub fn fraction(&self) -> f32 {
        let part = |n: u64, of: u64| if of > 0 { n as f32 / of as f32 } else { 0.0 };
        match *self {
            Phase::Looking(_) => 0.0,
            Phase::Copying { done, total, .. } => part(done, total),
            Phase::Removing { removed, total } => part(removed as u64, total as u64),
        }
    }
}

impl Progress {
    pub fn phase(&self) -> Phase {
        let removing = self.removing.load(Ordering::Relaxed);
        if removing > 0 {
            return Phase::Removing { removed: self.removed.load(Ordering::Relaxed).min(removing), total: removing };
        }
        if self.started.get().is_none() {
            return Phase::Looking(self.found.load(Ordering::Relaxed));
        }
        Phase::Copying {
            files: self.files.load(Ordering::Relaxed),
            files_total: self.files_total.load(Ordering::Relaxed),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
        }
    }
}

/// Bytes copied over the last `SPAN` seconds: the speed of the moment, as a system network meter
/// shows it, which moves with the line; the average since the start gives the time left.
#[derive(Default)]
pub struct Recent(std::collections::VecDeque<(f64, u64)>);

impl Recent {
    const SPAN: f64 = 4.0;

    /// `done` bytes at `now` seconds; bytes per second once a second has been seen.
    pub fn speed(&mut self, now: f64, done: u64) -> Option<f64> {
        self.0.push_back((now, done));
        while self.0.front().is_some_and(|(t, _)| now - t > Self::SPAN) {
            self.0.pop_front();
        }
        let (t0, d0) = *self.0.front()?;
        (now - t0 >= 1.0).then(|| done.saturating_sub(d0) as f64 / (now - t0))
    }
}
