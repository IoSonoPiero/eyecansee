//! Thumbnails saved to disk in an SQLite database, so reopening a folder does not regenerate them.
//! Linux: `$XDG_CACHE_HOME/eyecansee/thumbs.db` (usually `~/.cache/...`),
//! Windows: `%LOCALAPPDATA%\eyecansee\thumbs.db`.
//! One row per (source, path) with the file's size and modification date: if the image
//! changes, the row no longer matches and the thumbnail is regenerated.
//! Database separate from the future edit catalogue: this one can be thrown away, that one cannot.
//!
//! Reads: one connection per thread (WAL, in parallel). Writes: a single thread, in batches.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat};
use rusqlite::{Connection, params};

use crate::source::File;

/// The thumbnail folder inside the cache folder `base`.
pub(crate) fn dir_in(base: &std::path::Path) -> PathBuf {
    base.join(crate::test_copy::NAME)
}

/// The app's cache folder (`~/.cache/eyecansee`, `%LOCALAPPDATA%\eyecansee`, the test copy's own),
/// made if missing: the thumbnails, and the copies of the pictures open (`open_copy`).
pub(crate) fn cache_dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        // Tests never open the user's cache: one of their own for each run of the test binary.
        // Opening the user's wrote its WAL at every `cargo test`, and a test saved a row there
        // (2026-11-21). ponytail: left in the temporary folder, which the system clears.
        let base = if cfg!(test) {
            Some(std::env::temp_dir().join(format!("eyecansee-test-cache-{}", std::process::id())))
        } else if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::home_dir().map(|h| h.join(".cache")))
        };
        let dir = dir_in(&base?);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    })
    .as_ref()
}

fn db_path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| cache_dir().map(|d| d.join("thumbs.db"))).as_ref()
}

fn open() -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path().ok_or(rusqlite::Error::InvalidQuery)?)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    create(&conn)?;
    Ok(conn)
}

fn create(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS thumbs (
            source TEXT NOT NULL,
            path TEXT NOT NULL,
            size INTEGER NOT NULL,
            modified TEXT NOT NULL,
            accessed INTEGER NOT NULL,
            data BLOB NOT NULL,
            PRIMARY KEY (source, path)
        );
        CREATE TABLE IF NOT EXISTS meta (
            source TEXT NOT NULL,
            path TEXT NOT NULL,
            size INTEGER NOT NULL,
            modified TEXT NOT NULL,
            data TEXT NOT NULL,
            PRIMARY KEY (source, path)
        );
        CREATE TABLE IF NOT EXISTS info (
            source TEXT NOT NULL,
            path TEXT NOT NULL,
            size INTEGER NOT NULL,
            modified TEXT NOT NULL,
            data TEXT NOT NULL,
            PRIMARY KEY (source, path)
        );",
    )
}

/// Keeps the most recently used thumbnails as long as the total fits in `limit` bytes.
fn trim_rows(conn: &Connection, limit: u64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM thumbs WHERE rowid IN (
            SELECT rowid FROM (
                SELECT rowid, SUM(length(data)) OVER (ORDER BY accessed DESC, rowid) AS total
                FROM thumbs
            ) WHERE total > ?1
        )",
        [limit as i64],
    )
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

enum Write {
    Save { source: String, file: File, data: Vec<u8> },
    /// The metadata panel of a file (`meta::Meta::to_json`), with the same key as its thumbnail.
    SaveMeta { source: String, file: File, json: String },
    /// What the file says of itself for the line under its thumbnail (`thumb_details::Info`),
    /// with the same key as its thumbnail. A cache made before has no such rows: they are read
    /// from the file when wanted.
    SaveInfo { source: String, file: File, json: String },
    Touch { source: String, path: String },
    Delete { source: String, path: String },
    Trim { limit: u64 },
    /// Deleted like `Delete`, then `done` is called once they are committed.
    Forget { source: String, paths: Vec<String>, done: Box<dyn FnOnce() + Send> },
}

/// Channel to the writer thread, started on first use.
fn writer() -> Option<&'static Sender<Write>> {
    static TX: OnceLock<Option<Sender<Write>>> = OnceLock::new();
    TX.get_or_init(|| {
        let conn = open().ok()?;
        let (tx, rx) = channel();
        std::thread::Builder::new().name("thumbcache".into()).spawn(move || write_loop(conn, rx)).ok()?;
        Some(tx)
    })
    .as_ref()
}

/// Everything that arrived in the meantime goes into a single transaction.
// ponytail: writes still queued when the app closes are lost; they are regenerated on the next opening
fn write_loop(mut conn: Connection, rx: Receiver<Write>) {
    while let Ok(first) = rx.recv() {
        let Ok(tx) = conn.transaction() else { continue };
        let mut trimmed = 0;
        let mut done = Vec::new();
        for w in std::iter::once(first).chain(rx.try_iter()) {
            let _ = match w {
                Write::Save { source, file, data } => tx.execute(
                    "INSERT OR REPLACE INTO thumbs (source, path, size, modified, accessed, data)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![source, file.path, file.size as i64, file.modified, now(), data],
                ),
                Write::SaveMeta { source, file, json } => tx.execute(
                    "INSERT OR REPLACE INTO meta (source, path, size, modified, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![source, file.path, file.size as i64, file.modified, json],
                ),
                Write::SaveInfo { source, file, json } => tx.execute(
                    "INSERT OR REPLACE INTO info (source, path, size, modified, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![source, file.path, file.size as i64, file.modified, json],
                ),
                // At most one write a day per thumbnail: enough to evict the oldest ones.
                Write::Touch { source, path } => tx.execute(
                    "UPDATE thumbs SET accessed = ?3 WHERE source = ?1 AND path = ?2 AND accessed < ?3 - 86400",
                    params![source, path, now()],
                ),
                Write::Delete { source, path } => forget_row(&tx, &source, &path),
                // Emptying the cache empties the metadata too.
                Write::Trim { limit } => trim_rows(&tx, limit)
                    .and_then(|n| if limit == 0 { tx.execute_batch("DELETE FROM meta; DELETE FROM info").map(|()| n) } else { Ok(n) })
                    .inspect(|n| trimmed += n),
                Write::Forget { source, paths, done: d } => {
                    done.push(d);
                    paths.iter().try_fold(0, |n, path| forget_row(&tx, &source, path).map(|m| n + m))
                }
            };
        }
        let committed = tx.commit().is_ok();
        done.into_iter().for_each(|d| d());
        // Without VACUUM the file does not shrink: the freed space would only be reusable.
        if committed && trimmed > 0 {
            let _ = conn.execute_batch("VACUUM");
        }
    }
}

/// A file's thumbnail, metadata and details go together.
fn forget_row(conn: &Connection, source: &str, path: &str) -> rusqlite::Result<usize> {
    let n = conn.execute("DELETE FROM thumbs WHERE source = ?1 AND path = ?2", params![source, path])?;
    let m = conn.execute("DELETE FROM meta WHERE source = ?1 AND path = ?2", params![source, path])?;
    Ok(n + m + conn.execute("DELETE FROM info WHERE source = ?1 AND path = ?2", params![source, path])?)
}

fn send(w: Write) {
    if let Some(tx) = writer() {
        let _ = tx.send(w);
    }
}

/// Thumbnails saved for a source, with the file's size and date when they were created.
pub fn entries(source: &str) -> Vec<File> {
    let rows = || -> rusqlite::Result<Vec<File>> {
        let conn = open()?;
        let mut stmt = conn.prepare("SELECT path, size, modified FROM thumbs WHERE source = ?1")?;
        let rows = stmt.query_map([source], |r| {
            Ok(File { path: r.get(0)?, size: r.get::<_, i64>(1)? as u64, modified: r.get(2)? })
        })?;
        rows.collect()
    };
    rows().unwrap_or_default()
}

pub fn delete(source: &str, path: &str) {
    send(Write::Delete { source: source.to_string(), path: path.to_string() });
}

/// Removes the thumbnails through the writer, so the interface does not wait for it (a VACUUM can
/// take seconds); `done` is called once they are gone. Only then may the grid ask for them again,
/// or the old ones would come back from the cache. Without the cache `done` is called at once.
pub fn forget(source: &str, paths: Vec<String>, done: impl FnOnce() + Send + 'static) {
    match writer() {
        Some(tx) => {
            if let Err(e) = tx.send(Write::Forget { source: source.to_string(), paths, done: Box::new(done) })
                && let Write::Forget { done, .. } = e.0
            {
                done();
            }
        }
        None => done(),
    }
}

pub fn trim(limit: u64) {
    send(Write::Trim { limit });
}

/// Empties the cache: it is the space limit set to zero (and then VACUUM). `done` is called once
/// the thumbnails are gone (`forget` after the trim, in the writer's order): only then may the
/// grid read them again, or the old ones came back from the cache (#326).
pub fn purge(done: impl FnOnce() + Send + 'static) {
    trim(0);
    forget("", Vec::new(), done);
}

/// Number of thumbnails and bytes used, for the confirmation before emptying.
pub fn stats() -> (u64, u64) {
    open()
        .and_then(|c| {
            c.query_row("SELECT count(*), coalesce(sum(length(data)), 0) FROM thumbs", [], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
            })
        })
        .unwrap_or((0, 0))
}

thread_local! {
    /// Readers, one per thread (WAL: they do not wait for the writer).
    static READ: Option<Connection> = open().ok();
}

/// The metadata saved for this version of the file (`meta::Meta::to_json`).
pub fn load_meta(source: &str, file: &File) -> Option<String> {
    READ.with(|conn| {
        conn.as_ref()?
            .query_row(
                "SELECT data FROM meta WHERE source = ?1 AND path = ?2 AND size = ?3 AND modified = ?4",
                params![source, file.path, file.size as i64, file.modified],
                |r| r.get(0),
            )
            .ok()
    })
}

/// The details saved for this version of the file (`thumb_details::Info`); `None` for a file
/// changed since, or never read (a cache made before them).
pub fn load_info(source: &str, file: &File) -> Option<crate::thumb_details::Info> {
    READ.with(|conn| {
        conn.as_ref()?
            .query_row(
                "SELECT data FROM info WHERE source = ?1 AND path = ?2 AND size = ?3 AND modified = ?4",
                params![source, file.path, file.size as i64, file.modified],
                |r| r.get::<_, String>(0),
            )
            .ok()
    })
    .as_deref()
    .and_then(crate::thumb_details::Info::from_json)
}

pub fn save_info(source: &str, file: &File, info: &crate::thumb_details::Info) {
    send(Write::SaveInfo { source: source.to_string(), file: file.clone(), json: info.to_json() });
}

pub fn save_meta(source: &str, file: &File, json: String) {
    send(Write::SaveMeta { source: source.to_string(), file: file.clone(), json });
}

/// Thumbnails waiting for the listing of a file just written (a converted video, a copied or
/// moved file): path in the destination, size, encoded image. `loader::cached` takes them when
/// the folder is read again and saves them under the listing's key, so nothing is read back from
/// the device (on the POCO that read competed with the next file's transfer and timed out).
// ponytail: keyed by path and size, not by source; two devices with the same path and size are unlikely
static PENDING: LazyLock<Mutex<Pending>> = LazyLock::new(Default::default);
type Pending = HashMap<String, (u64, Vec<u8>)>;

pub fn keep(path: &str, size: u64, data: Vec<u8>) {
    let mut pending = PENDING.lock().unwrap();
    // ponytail: never taken (the folder was never shown again): dropped in bulk
    if pending.len() >= 4096 {
        pending.clear();
    }
    pending.insert(path.to_string(), (size, data));
}

/// The thumbnail kept for `path`, if the file still has `size`; taken once.
pub fn take(path: &str, size: u64) -> Option<Vec<u8>> {
    let mut pending = PENDING.lock().unwrap();
    match pending.get(path) {
        Some((s, _)) if *s == size => pending.remove(path).map(|(_, data)| data),
        _ => None,
    }
}

/// The thumbnail saved for `path` of `source`, whichever version: the file's size and the
/// encoded image. For a copy or a move: the new file has the same content.
pub fn stored(source: &str, path: &str) -> Option<(u64, Vec<u8>)> {
    READ.with(|conn| {
        conn.as_ref()?
            .query_row("SELECT size, data FROM thumbs WHERE source = ?1 AND path = ?2", params![source, path], |r| Ok((r.get::<_, i64>(0)? as u64, r.get(1)?)))
            .ok()
    })
}

pub fn load(source: &str, file: &File) -> Option<DynamicImage> {
    let data: Vec<u8> = READ.with(|conn| {
        conn.as_ref()?
            .query_row(
                "SELECT data FROM thumbs WHERE source = ?1 AND path = ?2 AND size = ?3 AND modified = ?4",
                params![source, file.path, file.size as i64, file.modified],
                |r| r.get(0),
            )
            .ok()
    })?;
    send(Write::Touch { source: source.to_string(), path: file.path.clone() });
    image::load_from_memory(&data).ok()
}

/// JPEG for opaque images (small), PNG if there is transparency, always 8-bit: the PNG encoder
/// refuses float images (OpenEXR decodes as Rgba32F) and they were never cached (issue #2).
/// An error is only logged: without the cache the thumbnail is simply regenerated.
pub fn save(source: &str, file: &File, img: &DynamicImage) {
    match encode(img) {
        Ok(data) => save_data(source, file, data),
        Err(e) => log!("thumbnail of {}: not cached: {e}", file.path),
    }
}

/// A thumbnail as the cache stores it (`encode`, `stored`, `take`).
pub fn save_data(source: &str, file: &File, data: Vec<u8>) {
    send(Write::Save { source: source.to_string(), file: file.clone(), data });
}

pub fn encode(img: &DynamicImage) -> Result<Vec<u8>, image::ImageError> {
    let mut data = Vec::new();
    let mut out = std::io::Cursor::new(&mut data);
    if img.color().has_alpha() {
        DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut out, ImageFormat::Png)?;
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut out, ImageFormat::Jpeg)?;
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache made before #139 (thumbnails and metadata only) opens as it is: its rows stay, the
    /// table of details is added empty, and they are read from the files when wanted.
    #[test]
    fn cache_from_before_the_details() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE thumbs (source TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL, modified TEXT NOT NULL,
                accessed INTEGER NOT NULL, data BLOB NOT NULL, PRIMARY KEY (source, path));
             CREATE TABLE meta (source TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL, modified TEXT NOT NULL,
                data TEXT NOT NULL, PRIMARY KEY (source, path));
             INSERT INTO thumbs VALUES ('s', 'a.jpg', 10, 'm', 0, x'00');",
        )
        .unwrap();
        create(&conn).unwrap();
        let count = |table: &str| conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((count("thumbs"), count("info")), (1, 0));
        // And again: nothing changes.
        create(&conn).unwrap();
        assert_eq!((count("thumbs"), count("info")), (1, 0));
    }

    /// The details of a file kept with its thumbnail: read back for that version of the file only,
    /// and gone with it.
    #[test]
    fn details_roundtrip() {
        use crate::thumb_details::Info;
        let source = format!("test-info-{}://", std::process::id());
        let file = File { path: "b.jpg".into(), size: 20, modified: "yesterday".into() };
        let info = Info { pixels: Some([3000, 4000]), taken: Some("2026-11-14 21:00:00".into()), duration: None, drawing: None };
        save_info(&source, &file, &info);
        let mut back = None;
        for _ in 0..100 {
            back = load_info(&source, &file);
            if back.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(back, Some(info));
        assert!(load_info(&source, &File { size: 21, ..file.clone() }).is_none(), "changed file: read again");
        let (tx, rx) = channel();
        forget(&source, vec!["b.jpg".into()], move || tx.send(()).unwrap());
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(load_info(&source, &file).is_none(), "forgotten with its thumbnail");
    }

    #[test]
    fn roundtrip() {
        let source = format!("test-{}://", std::process::id());
        let file = File { path: "a.jpg".into(), size: 10, modified: "yesterday".into() };
        save(&source, &file, &DynamicImage::new_rgb8(4, 3));
        // The write is asynchronous: wait for the writer thread to do it.
        let mut back = None;
        for _ in 0..100 {
            back = load(&source, &file);
            if back.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let back = back.expect("thumbnail saved");
        assert_eq!((back.width(), back.height()), (4, 3));
        // File changed: the old thumbnail is no longer valid.
        assert!(load(&source, &File { modified: "today".into(), ..file.clone() }).is_none());
        // Copied elsewhere: the same thumbnail waits for the new file, whatever its date; once.
        let (size, data) = stored(&source, "a.jpg").expect("stored");
        assert_eq!(size, 10);
        keep("new/a.jpg", size, data.clone());
        assert!(take("new/a.jpg", 11).is_none(), "another size: another file");
        assert_eq!(take("new/a.jpg", 10), Some(data));
        assert!(take("new/a.jpg", 10).is_none(), "taken");
        assert!(stored(&source, "none.jpg").is_none());
        assert_eq!(entries(&source).iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.jpg"]);
        delete(&source, "a.jpg");
        for _ in 0..100 {
            if entries(&source).is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(entries(&source).is_empty(), "thumbnail not deleted");
        // Forgotten: gone as soon as `done` is called.
        save(&source, &file, &DynamicImage::new_rgb8(4, 3));
        for _ in 0..100 {
            if load(&source, &file).is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        forget(&source, vec!["a.jpg".into()], move || tx.send(()).unwrap());
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(load(&source, &file).is_none());
    }

    /// #335: before it, a phone's date without its zone was kept as Unix seconds, read in the
    /// computer's time zone; now it is the phone's clock time as it is. With the computer still in
    /// the same zone, the thumbnail, details and metadata saved under the old key are the same
    /// file's: found under the new one, nothing read again from the phone.
    #[test]
    fn phone_thumbnails_kept_with_the_new_date_key() {
        let source = format!("mtp://TEST335-{}/", std::process::id());
        let clock = "2026-11-24T10:30:05";
        let seconds = crate::human::parse_modified(clock).unwrap().as_second().to_string();
        let before = File { path: "Internal storage/DCIM/a.jpg".into(), size: 10, modified: seconds };
        save(&source, &before, &DynamicImage::new_rgb8(4, 3));
        save_info(&source, &before, &crate::thumb_details::Info { pixels: Some([4, 3]), ..Default::default() });
        save_meta(&source, &before, "{}".into());
        for _ in 0..100 {
            if load(&source, &before).is_some() && load_info(&source, &before).is_some() && load_meta(&source, &before).is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(load(&source, &before).is_some(), "saved under the old key");
        let now = File { modified: clock.into(), ..before.clone() };
        assert!(load(&source, &now).is_some(), "the thumbnail is made again from the phone");
        assert!(load_info(&source, &now).is_some(), "the details are read again from the phone");
        assert!(load_meta(&source, &now).is_some(), "the metadata are read again from the phone");
    }

    /// A float image with alpha (what OpenEXR decodes to) must be cached too (issue #2).
    #[test]
    fn float_image() {
        let source = format!("test-exr-{}://", std::process::id());
        let file = File { path: "x.exr".into(), size: 10, modified: "yesterday".into() };
        save(&source, &file, &DynamicImage::new_rgba32f(4, 3));
        let mut back = None;
        for _ in 0..100 {
            back = load(&source, &file);
            if back.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let back = back.expect("float thumbnail saved");
        assert_eq!((back.width(), back.height()), (4, 3));
        delete(&source, "x.exr");
    }

    #[test]
    fn trim() {
        let conn = Connection::open_in_memory().unwrap();
        create(&conn).unwrap();
        // Three 10-byte thumbnails, accessed at times 1, 3, 2.
        for (path, accessed) in [("a", 1), ("b", 3), ("c", 2)] {
            conn.execute(
                "INSERT INTO thumbs VALUES ('s', ?1, 0, '', ?2, zeroblob(10))",
                params![path, accessed],
            )
            .unwrap();
        }
        assert_eq!(trim_rows(&conn, 25).unwrap(), 1);
        let left: Vec<String> =
            conn.prepare("SELECT path FROM thumbs ORDER BY path").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!(left, ["b", "c"]); // the least recent goes
        assert_eq!(trim_rows(&conn, 25).unwrap(), 0);
        // Zero limit = complete emptying (`purge`).
        assert_eq!(trim_rows(&conn, 0).unwrap(), 2);
    }
}
