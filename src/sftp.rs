//! SFTP servers with `russh` and `russh-sftp` (pure Rust, Linux and Windows). One SSH connection
//! per server, shared by every request: `RawSftpSession` sends many requests at once and matches
//! the answers by id, so a file is read in pieces fetched together, not one after the other.
//! A connection that dropped is opened again at the next request.
//!
//! Paths as for the other sources, relative to the server's "/" (folders end in "/").

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use bytes::Bytes;
use russh::client::Handle;
use russh_sftp::client::RawSftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};

use crate::source::{Error, Listing, NOT_FOUND, TRASH_DIR};

mod connect;
pub use connect::{Target, rsa_login, trust, unknown};

/// Requests in flight for one read or write.
const IN_FLIGHT: usize = 16;

pub struct Server {
    target: Target,
    conn: tokio::sync::Mutex<Option<Arc<Conn>>>,
}

struct Conn {
    handle: Handle<connect::Client>,
    raw: RawSftpSession,
    /// Largest read and write the server accepts (`limits@openssh.com`, else the 32 KiB every
    /// server must accept).
    read_len: u32,
    write_len: usize,
}

/// Servers in use: the clipboard can keep the source of a copy, which then shares the connection.
static SERVERS: Mutex<Vec<Weak<Server>>> = Mutex::new(Vec::new());

/// Absolute path on the server.
fn abs(path: &str) -> String {
    format!("/{}", path.trim_end_matches('/'))
}

fn error(e: SftpError, path: &str) -> Error {
    match &e {
        SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile => (NOT_FOUND, path.to_string()),
        SftpError::Status(s) if s.status_code == StatusCode::PermissionDenied => ("error-permission", path.to_string()),
        _ => ("error-detail", format!("{path}: {e}")),
    }
}

fn is_eof(e: &SftpError) -> bool {
    matches!(e, SftpError::Status(s) if s.status_code == StatusCode::Eof)
}

/// An open file, closed when dropped (the server limits how many stay open).
pub struct Opened {
    conn: Arc<Conn>,
    handle: String,
}

impl Drop for Opened {
    fn drop(&mut self) {
        let (conn, handle) = (self.conn.clone(), std::mem::take(&mut self.handle));
        if handle.is_empty() {
            return; // closed already
        }
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move { conn.raw.close(handle).await });
        }
    }
}

/// `$op`, an expression using the connection `$c`, run once more on a new connection if the
/// first one had dropped. A macro, not a closure: async closures over references are not `Send`
/// for the tasks that run them.
macro_rules! retry {
    ($server:expr, $path:expr, |$c:ident| $op:expr) => {{
        let conn = $server.session().await?;
        let first = {
            let $c: &Conn = &conn;
            $op
        };
        match first {
            Err(e) if $server.dropped(&conn, &e).await => {
                let conn = $server.session().await?;
                let $c: &Conn = &conn;
                $op.map_err(|e| error(e, $path))
            }
            r => r.map_err(|e| error(e, $path)),
        }
    }};
}

impl Server {
    /// The server already in use for this address, or a new one (connected at the first request).
    pub fn shared(target: Target) -> Arc<Server> {
        let mut servers = SERVERS.lock().unwrap();
        servers.retain(|s| s.strong_count() > 0);
        if let Some(s) = servers.iter().filter_map(Weak::upgrade).find(|s| s.target == target) {
            return s;
        }
        let s = Arc::new(Server { target, conn: tokio::sync::Mutex::const_new(None) });
        servers.push(Arc::downgrade(&s));
        s
    }

    async fn session(&self) -> Result<Arc<Conn>, Error> {
        let mut conn = self.conn.lock().await;
        if let Some(c) = conn.as_ref().filter(|c| !c.handle.is_closed()) {
            return Ok(c.clone());
        }
        let t = std::time::Instant::now();
        let c = Arc::new(open(&self.target).await?);
        log!("sftp {}: connected in {:.2} s", self.target.host, t.elapsed().as_secs_f32());
        *conn = Some(c.clone());
        Ok(c)
    }

    /// After a request failed on `c`: was it the connection that dropped? Then it is forgotten,
    /// and the next request connects again.
    async fn dropped(&self, c: &Conn, e: &SftpError) -> bool {
        let dropped = c.handle.is_closed() || matches!(e, SftpError::IO(_) | SftpError::Timeout);
        if dropped {
            log!("sftp {}: {e}, connecting again", self.target.host);
            self.conn.lock().await.take();
        }
        dropped
    }

    /// The folder the server starts in (usually the user's home), as a source path.
    pub async fn home(&self) -> Result<String, Error> {
        let name = retry!(self, ".", |c| c.raw.realpath(".").await)?;
        let home = name.files.first().map(|f| f.filename.trim_matches('/').to_string()).unwrap_or_default();
        Ok(if home.is_empty() { home } else { format!("{home}/") })
    }

    async fn stat(&self, path: &str) -> Result<FileAttributes, Error> {
        retry!(self, path, |c| c.raw.stat(abs(path)).await.map(|a| a.attrs))
    }

    /// Everything in a folder but "." and "..", with its attributes.
    async fn entries(&self, dir: &str) -> Result<Vec<(String, FileAttributes)>, Error> {
        retry!(self, dir, |c| read_dir(c, dir).await)
    }

    /// Is this entry a folder? A symlink is followed.
    async fn entry_is_dir(&self, path: &str, a: &FileAttributes) -> bool {
        match a.file_type() {
            t if t.is_symlink() => self.stat(path).await.is_ok_and(|a| a.file_type().is_dir()),
            t => t.is_dir(),
        }
    }

    pub async fn list(&self, dir: &str) -> Result<Listing, Error> {
        let mut listing = Listing::default();
        for (name, a) in self.entries(dir).await? {
            let is_dir = self.entry_is_dir(&format!("{dir}{name}"), &a).await;
            let path = format!("{dir}{name}{}", if is_dir { "/" } else { "" });
            let modified = a.mtime.map(|t| t.to_string()).unwrap_or_default();
            listing.add(path, &name, is_dir, a.size.unwrap_or(0), modified, true);
        }
        listing.sort();
        Ok(listing)
    }

    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        Ok(self.entries(dir).await?.into_iter().map(|(n, _)| n).collect())
    }

    pub async fn size(&self, path: &str) -> Result<u64, Error> {
        Ok(self.stat(path).await?.size.unwrap_or(0))
    }

    pub async fn is_dir(&self, path: &str) -> bool {
        self.stat(path).await.is_ok_and(|a| a.file_type().is_dir())
    }

    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        match self.stat(dir).await? {
            a if a.file_type().is_dir() => Ok(()),
            _ => Err((NOT_FOUND, dir.to_string())),
        }
    }

    pub async fn open_read(&self, path: &str) -> Result<Opened, Error> {
        let c = self.session().await?;
        let handle = c.raw.open(abs(path), OpenFlags::READ, FileAttributes::default()).await.map_err(|e| error(e, path))?.handle;
        Ok(Opened { conn: c, handle })
    }

    /// `len` bytes from `offset`, fewer at the end of the file: pieces fetched together.
    pub async fn read_at(&self, f: &Opened, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let piece = u64::from(f.conn.read_len);
        let pieces: Vec<(u64, u32)> = (offset..offset + len).step_by(piece as usize).map(|at| (at, piece.min(offset + len - at) as u32)).collect();
        let mut data = Vec::with_capacity(len as usize);
        for batch in pieces.chunks(IN_FLIGHT) {
            let mut reads = Vec::with_capacity(batch.len());
            for &(at, n) in batch {
                reads.push(read_piece(&f.conn, &f.handle, at, n));
            }
            for part in futures_util::future::try_join_all(reads).await.map_err(|e| error(e, ""))? {
                let short = part.len() < piece as usize;
                data.extend(part);
                if short {
                    return Ok(data); // end of the file
                }
            }
        }
        Ok(data)
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let f = self.open_read(path).await?;
        let size = f.conn.raw.fstat(f.handle.clone()).await.map_err(|e| error(e, path))?.attrs.size;
        match size {
            Some(size) => self.read_at(&f, 0, size).await,
            // ponytail: size unknown (rare servers): one piece at a time until the end
            None => {
                let mut data = Vec::new();
                loop {
                    let part = read_piece(&f.conn, &f.handle, data.len() as u64, f.conn.read_len).await.map_err(|e| error(e, path))?;
                    if part.is_empty() {
                        return Ok(data);
                    }
                    data.extend(part);
                }
            }
        }
    }

    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let f = self.open_read(path).await?;
        self.read_at(&f, offset, len).await
    }

    /// Writes a file from the chunks of a copy, replacing one with the same name; removed if the
    /// chunks end with an error or a write fails.
    pub async fn write_chunks(&self, path: &str, mut chunks: tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Error> {
        let c = self.session().await?;
        let flags = OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE;
        let handle = c.raw.open(abs(path), flags, FileAttributes::default()).await.map_err(|e| error(e, path))?.handle;
        let mut f = Opened { conn: c, handle };
        let failed = write_all(&f, &mut chunks).await.err().map(|e| match e {
            Ok(e) => error(e, path),
            Err(e) => ("error-detail", e.to_string()),
        });
        let closed = f.conn.raw.close(std::mem::take(&mut f.handle)).await;
        match (failed, closed) {
            (None, Ok(_)) => Ok(()),
            (failed, closed) => {
                let _ = self.delete(path).await;
                Err(failed.unwrap_or_else(|| error(closed.unwrap_err(), path)))
            }
        }
    }

    /// Creates a folder whose parent exists; nothing if it is already there.
    pub async fn create_dir(&self, dir: &str) -> Result<(), Error> {
        let made = retry!(self, dir, |c| c.raw.mkdir(abs(dir), FileAttributes::default()).await);
        match made {
            Err(_) if self.is_dir(dir).await => Ok(()),
            r => r.map(|_| ()),
        }
    }

    /// Creates a folder and the ones above it that are missing.
    pub async fn create_dir_all(&self, dir: &str) -> Result<(), Error> {
        let mut at = String::new();
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            at = format!("{at}{part}/");
            if !self.is_dir(&at).await {
                self.create_dir(&at).await?;
            }
        }
        Ok(())
    }

    /// Renames or moves a file or folder on the server itself; `to` must be free.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), Error> {
        retry!(self, from, |c| c.raw.rename(abs(from), abs(to)).await).map(|_| ())
    }

    pub async fn exists(&self, path: &str) -> bool {
        let r: Result<_, Error> = (async { retry!(self, path, |c| c.raw.lstat(abs(path)).await) }).await;
        r.is_ok()
    }

    /// Deletes a file, or a folder with everything inside it.
    pub async fn delete(&self, path: &str) -> Result<(), Error> {
        if !path.ends_with('/') {
            return retry!(self, path, |c| c.raw.remove(abs(path)).await).map(|_| ());
        }
        for (name, a) in self.entries(path).await? {
            let child = format!("{path}{name}");
            // A symlink is removed, never followed.
            if a.file_type().is_dir() {
                Box::pin(self.delete(&format!("{child}/"))).await?;
            } else {
                retry!(self, &child, |c| c.raw.remove(abs(&child)).await)?;
            }
        }
        retry!(self, path, |c| c.raw.rmdir(abs(path)).await).map(|_| ())
    }

    /// Everything below a folder, hidden items too, folders before what they contain: path and
    /// size (folders end in "/", size 0). EyeCanSee's trash folders stay out.
    pub async fn walk(&self, dir: &str, p: &crate::transfer::Progress) -> Result<Vec<(String, u64)>, Error> {
        let mut found = Vec::new();
        let mut todo = vec![dir.to_string()];
        while let Some(d) = todo.pop() {
            if p.cancel.load(Ordering::Relaxed) {
                return Err(("error-cancelled", String::new()));
            }
            *p.current.lock().unwrap() = d.clone();
            for (name, a) in self.entries(&d).await? {
                let path = format!("{d}{name}");
                if self.entry_is_dir(&path, &a).await {
                    if name != TRASH_DIR {
                        found.push((format!("{path}/"), 0));
                        todo.push(format!("{path}/"));
                    }
                } else {
                    found.push((path, a.size.unwrap_or(0)));
                }
                p.found.fetch_add(1, Ordering::Relaxed);
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(found)
    }

    /// Reads a file of `size` bytes in chunks of `chunk` bytes, handing each to `push`.
    pub async fn send<F, Fut>(&self, path: &str, size: u64, chunk: u64, cancel: &AtomicBool, push: F) -> Result<(), Error>
    where
        F: Fn(Bytes) -> Fut,
        Fut: std::future::Future<Output = Result<(), Error>>,
    {
        let f = self.open_read(path).await?;
        let mut at = 0;
        while at < size {
            if cancel.load(Ordering::Relaxed) {
                return Err(("error-cancelled", String::new()));
            }
            let data = self.read_at(&f, at, chunk.min(size - at)).await?;
            if data.is_empty() {
                return Err(("error-detail", format!("{path}: file shorter than expected")));
            }
            at += data.len() as u64;
            push(Bytes::from(data)).await?;
        }
        Ok(())
    }
}

/// Connected, logged in, with the SFTP subsystem started.
async fn open(t: &Target) -> Result<Conn, Error> {
    let handle = connect::connect(t).await?;
    let detail = |e: &dyn std::fmt::Display| ("error-detail", format!("{}: {e}", t.host));
    let channel = handle.channel_open_session().await.map_err(|e| detail(&e))?;
    // A server with SSH but no SFTP (a Synology with SFTP off) refuses here.
    channel.request_subsystem(true, "sftp").await.map_err(|e| ("error-no-sftp", format!("{}: {e}", t.host)))?;
    let mut raw = RawSftpSession::new(channel.into_stream());
    raw.init().await.map_err(|e| ("error-no-sftp", format!("{}: {e}", t.host)))?;
    let (mut read_len, mut write_len) = (32 << 10, 32 << 10);
    if let Ok(limits) = raw.limits().await {
        read_len = limits.max_read_len.clamp(32 << 10, 256 << 10);
        write_len = limits.max_write_len.clamp(32 << 10, 256 << 10);
        raw.set_limits(limits.into());
    }
    raw.set_timeout(60);
    Ok(Conn { handle, raw, read_len: read_len as u32, write_len: write_len as usize })
}

/// Everything in a folder but "." and "..", with its attributes.
async fn read_dir(c: &Conn, dir: &str) -> Result<Vec<(String, FileAttributes)>, SftpError> {
    let handle = c.raw.opendir(abs(dir)).await?.handle;
    let mut all = Vec::new();
    let r = loop {
        match c.raw.readdir(handle.as_str()).await {
            Ok(name) => all.extend(name.files.into_iter().filter(|f| f.filename != "." && f.filename != "..").map(|f| (f.filename, f.attrs))),
            Err(e) if is_eof(&e) => break Ok(all),
            Err(e) => break Err(e),
        }
    };
    let _ = c.raw.close(handle).await;
    r
}

/// `n` bytes from `at`, fewer only at the end of the file (servers may answer with less).
async fn read_piece(c: &Conn, handle: &str, at: u64, n: u32) -> Result<Vec<u8>, SftpError> {
    let mut data = Vec::with_capacity(n as usize);
    while data.len() < n as usize {
        match c.raw.read(handle, at + data.len() as u64, n - data.len() as u32).await {
            Ok(d) if d.data.is_empty() => break,
            Ok(d) => data.extend(d.data),
            Err(e) if is_eof(&e) => break,
            Err(e) => return Err(e),
        }
    }
    Ok(data)
}

/// Writes the chunks of a copy in pieces, up to `IN_FLIGHT` sent and not yet confirmed: the
/// next chunk goes out while the server confirms the previous one. Errors: the server's, or the
/// one that ended the chunks.
async fn write_all(f: &Opened, chunks: &mut tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Result<SftpError, std::io::Error>> {
    use futures_util::StreamExt;
    let mut sent = futures_util::stream::FuturesUnordered::new();
    let mut at = 0u64;
    while let Some(chunk) = chunks.recv().await {
        let chunk = chunk.map_err(Err)?;
        for part in chunk.chunks(f.conn.write_len) {
            sent.push(f.conn.raw.write(f.handle.clone(), at, part.to_vec()));
            at += part.len() as u64;
            if sent.len() >= IN_FLIGHT
                && let Some(r) = sent.next().await
            {
                r.map_err(Ok)?;
            }
        }
    }
    while let Some(r) = sent.next().await {
        r.map_err(Ok)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::source::{self, Deletion, Store};

    #[test]
    fn absolute_paths() {
        assert_eq!(super::abs(""), "/");
        assert_eq!(super::abs("home/me/Photos/"), "/home/me/Photos");
        assert_eq!(super::abs("home/me/a.jpg"), "/home/me/a.jpg");
    }

    /// Writes 10 MB in chunks as a copy does, reads it back; `Store` operations on a real server.
    async fn write(op: &Store, path: &str, data: &[u8]) {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let chunks: Vec<bytes::Bytes> = data.chunks(1 << 20).map(bytes::Bytes::copy_from_slice).collect();
        let feed = async move {
            for c in chunks {
                tx.send(Ok(c)).await.unwrap();
            }
        };
        let (r, ()) = tokio::join!(op.write_chunks(path, data.len() as u64, rx), feed);
        r.unwrap();
    }

    /// Every operation on a real server, in a new folder that is deleted at the end.
    /// `EYECANSEE_SFTP=sftp://user@host:port/folder/ cargo test --release sftp_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn sftp_live() {
        let url = std::env::var("EYECANSEE_SFTP").expect("EYECANSEE_SFTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let s = source::open(&url).unwrap();
            let Store::Sftp(server) = &s.op else { panic!("not sftp") };
            let t = std::time::Instant::now();
            let home = server.home().await.unwrap();
            println!("home: {home:?} ({:?}, connection included)", t.elapsed());
            let t = std::time::Instant::now();
            let names = s.op.names(&s.dir).await.map(|n| n.len());
            println!("names of {:?}: {names:?} in {:?}", s.dir, t.elapsed());
            let t = std::time::Instant::now();
            let listed = s.op.list(&s.dir).await.map(|l| l.files.len());
            println!("list: {listed:?} in {:?}", t.elapsed());
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            // As the app: an address without a folder opens in the server's starting folder.
            let dir = format!("{}eyecansee-test-{secs}/", if s.home { &home } else { &s.dir });
            let op = &s.op;
            op.create_dir(&dir).await.unwrap();
            let data: Vec<u8> = (0..10u32 << 20).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
            let t = std::time::Instant::now();
            write(op, &format!("{dir}a.bin"), &data).await;
            println!("write 10 MB: {:?}", t.elapsed());
            assert_eq!(op.size(&format!("{dir}a.bin")).await.unwrap(), data.len() as u64);
            let t = std::time::Instant::now();
            assert!(op.read(&format!("{dir}a.bin")).await.unwrap() == data, "read back differs");
            println!("read 10 MB: {:?}", t.elapsed());
            let part = op.read_range(&format!("{dir}a.bin"), 5_000_000, 300_000).await.unwrap();
            assert!(part == data[5_000_000..5_300_000], "range differs");
            write(op, &format!("{dir}x.jpg"), b"not really a jpeg").await;
            let l = op.list(&dir).await.unwrap();
            assert_eq!(l.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), [format!("{dir}x.jpg")]);
            // Renames: a file, never over another item, a folder; moves into new folders.
            op.rename(&format!("{dir}x.jpg"), "y.jpg").await.unwrap();
            assert_eq!(op.rename(&format!("{dir}y.jpg"), "a.bin").await.unwrap_err().0, "error-exists");
            op.create_dir(&format!("{dir}sub/")).await.unwrap();
            op.create_dir(&format!("{dir}sub/")).await.unwrap(); // already there: fine
            op.move_into(&format!("{dir}y.jpg"), &format!("{dir}sub/inner/"), "z.jpg").await.unwrap();
            op.rename(&format!("{dir}sub/"), "sub2").await.unwrap();
            assert!(op.is_dir(&format!("{dir}sub2/inner/")).await);
            // The trash folder: a file, then a whole folder.
            op.delete(&format!("{dir}sub2/inner/z.jpg"), Deletion::TrashFolder).await.unwrap();
            assert_eq!(op.size(&format!("{dir}sub2/inner/{}/z.jpg", source::TRASH_DIR)).await.unwrap(), 17);
            op.delete(&format!("{dir}sub2/"), Deletion::TrashFolder).await.unwrap();
            assert!(op.is_dir(&format!("{dir}{}/sub2/inner/", source::TRASH_DIR)).await);
            let walked = op.walk(&dir, &crate::transfer::Progress::default()).await.unwrap();
            println!("walk: {walked:?}");
            assert_eq!(walked.len(), 1, "the trash stays out of a walk");
            assert_eq!(op.read(&format!("{dir}nothing.jpg")).await.unwrap_err().0, source::NOT_FOUND);
            op.delete(&dir, Deletion::Permanent).await.unwrap();
            assert_eq!(op.check_dir(&dir).await.unwrap_err().0, source::NOT_FOUND);
        });
    }
}
