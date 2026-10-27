//! Thumbnails saved to disk in an SQLite database, so reopening a folder does not regenerate them.
//! Linux: `$XDG_CACHE_HOME/eyecansee/thumbs.db` (usually `~/.cache/...`),
//! Windows: `%LOCALAPPDATA%\eyecansee\thumbs.db`.
//! One row per (source, path) with the file's size and modification date: if the image
//! changes, the row no longer matches and the thumbnail is regenerated.
//! Database separate from the future edit catalogue: this one can be thrown away, that one cannot.
//!
//! Reads: one connection per thread (WAL, in parallel). Writes: a single thread, in batches.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat};
use rusqlite::{Connection, params};

use crate::source::File;

fn db_path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        let base = if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::home_dir().map(|h| h.join(".cache")))
        };
        let dir = base?.join("eyecansee");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir.join("thumbs.db"))
    })
    .as_ref()
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
                // At most one write a day per thumbnail: enough to evict the oldest ones.
                Write::Touch { source, path } => tx.execute(
                    "UPDATE thumbs SET accessed = ?3 WHERE source = ?1 AND path = ?2 AND accessed < ?3 - 86400",
                    params![source, path, now()],
                ),
                Write::Delete { source, path } => forget_row(&tx, &source, &path),
                // Emptying the cache empties the metadata too.
                Write::Trim { limit } => trim_rows(&tx, limit)
                    .and_then(|n| if limit == 0 { tx.execute("DELETE FROM meta", []).map(|m| n + m) } else { Ok(n) })
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

/// A file's thumbnail and metadata go together.
fn forget_row(conn: &Connection, source: &str, path: &str) -> rusqlite::Result<usize> {
    let n = conn.execute("DELETE FROM thumbs WHERE source = ?1 AND path = ?2", params![source, path])?;
    Ok(n + conn.execute("DELETE FROM meta WHERE source = ?1 AND path = ?2", params![source, path])?)
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

/// Empties the cache: it is the space limit set to zero (and then VACUUM).
pub fn purge() {
    trim(0);
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

pub fn save_meta(source: &str, file: &File, json: String) {
    send(Write::SaveMeta { source: source.to_string(), file: file.clone(), json });
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

/// JPEG for opaque images (small), PNG if there is transparency. Errors are ignored:
/// without the cache it is simply regenerated.
pub fn save(source: &str, file: &File, img: &DynamicImage) {
    let mut data = Vec::new();
    let mut out = std::io::Cursor::new(&mut data);
    let ok = if img.color().has_alpha() {
        img.write_to(&mut out, ImageFormat::Png).is_ok()
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut out, ImageFormat::Jpeg).is_ok()
    };
    if ok {
        send(Write::Save { source: source.to_string(), file: file.clone(), data });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
