//! SFTP servers with `russh` and `russh-sftp` (pure Rust, Linux and Windows). One SSH connection
//! per server, shared by every request: `RawSftpSession` sends many requests at once and matches
//! the answers by id, so a file is read in pieces fetched together, not one after the other.
//! A connection that dropped, or stopped answering (`watch`), is opened again at the next
//! request, and the request that found it gone goes once more on the new one.
//!
//! Paths as for the other sources, relative to the server's "/" (folders end in "/").

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use bytes::Bytes;
use russh::client::Handle;
use russh_sftp::client::RawSftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::protocol::{FileAttributes, OpenFlags, Packet, StatusCode};

use crate::source::{Error, Listing, NOT_FOUND, SILENT};

mod connect;
mod watch;
pub use connect::{Target, rsa_login, trust, unknown};
use watch::{SILENCE, Watch, lost};

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
    /// The server renames a file over another in one request (`posix-rename@openssh.com`).
    posix_rename: bool,
    watch: Watch,
}

impl Conn {
    /// A request on this connection; `Timeout` when the connection stays silent (`watch`).
    async fn ask<T>(&self, req: impl Future<Output = Result<T, SftpError>>) -> Result<T, SftpError> {
        self.watch.ask(SILENCE, req).await
    }
}

/// Servers in use: the clipboard can keep the source of a copy, which then shares the connection.
static SERVERS: Mutex<Vec<Weak<Server>>> = Mutex::new(Vec::new());

/// Whether a connection to this server is open (a busy one counts as open).
pub fn connected(host: &str, port: Option<u16>, user: &str) -> bool {
    Server::at(host, port, user).iter().any(|s| s.conn.try_lock().map_or(true, |c| c.as_ref().is_some_and(|c| !c.handle.is_closed())))
}

/// Closes the connections to this server ("Close connection"); a later request opens a new one.
pub async fn disconnect(host: String, port: Option<u16>, user: String) {
    for s in Server::at(&host, port, &user) {
        if let Some(c) = s.conn.lock().await.take() {
            let r = c.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
            log!("sftp: disconnected from {host}: {r:?}");
        }
    }
}

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

/// Tries of a request that changes nothing on the server (reads, listings, the opening of a
/// file): the first, then on a new connection each time the one before dropped. On the link of
/// #75 a new connection sometimes stopped again within seconds (Windows to a NAS through a VPN);
/// three tries give up after about 3 × `SILENCE`, about the 60 s of a
/// single wait before.
const READ_TRIES: u32 = 3;

/// `$op`, an expression using the connection `$c` (`&Arc<Conn>`), run up to `$tries` times, each
/// on a new connection if the one before had dropped (twice when not given: an operation that
/// changes something, such as a rename, may have been done already). A macro, not a closure:
/// async closures over references are not `Send` for the tasks that run them.
macro_rules! retry {
    ($server:expr, $path:expr, |$c:ident| $op:expr) => {
        retry!($server, $path, 2, |$c| $op)
    };
    ($server:expr, $path:expr, $tries:expr, |$c:ident| $op:expr) => {{
        let mut left: u32 = $tries;
        loop {
            let conn = $server.session().await?;
            let r = {
                let $c = &conn;
                $op
            };
            left -= 1;
            match r {
                // A connection that dropped is forgotten even when no try is left.
                Err(e) if $server.dropped(&conn, &e).await && left > 0 => continue,
                r => break r.map_err(|e| error(e, $path)),
            }
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

    /// The servers in use at this host, port and user, whatever their key or password.
    fn at(host: &str, port: Option<u16>, user: &str) -> Vec<Arc<Server>> {
        let servers = SERVERS.lock().unwrap();
        let same = |s: &Arc<Server>| s.target.host.eq_ignore_ascii_case(host) && s.target.port == port && s.target.user == user;
        servers.iter().filter_map(Weak::upgrade).filter(same).collect()
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

    /// After a request failed on `c`: was it the connection that dropped (`watch::lost`)? Then it
    /// is forgotten, and the next request connects again. Forgotten once: the other requests that
    /// were waiting on it must not drop the new connection.
    async fn dropped(&self, c: &Arc<Conn>, e: &SftpError) -> bool {
        if !lost(c.handle.is_closed(), e) {
            return false;
        }
        let mut conn = self.conn.lock().await;
        if conn.as_ref().is_some_and(|now| Arc::ptr_eq(now, c)) {
            conn.take();
            log!("sftp {}: {e}, connecting again", self.target.host);
        }
        true
    }

    /// The result of a last try on `c`, as the app's error; a connection that dropped is forgotten
    /// all the same.
    async fn checked<T>(&self, c: &Arc<Conn>, r: Result<T, SftpError>, path: &str) -> Result<T, Error> {
        if let Err(e) = &r {
            self.dropped(c, e).await;
        }
        r.map_err(|e| error(e, path))
    }

    /// The folder the server starts in (usually the user's home), as a source path.
    pub async fn home(&self) -> Result<String, Error> {
        let name = retry!(self, ".", READ_TRIES, |c| c.ask(c.raw.realpath(".")).await)?;
        let home = name.files.first().map(|f| f.filename.trim_matches('/').to_string()).unwrap_or_default();
        Ok(if home.is_empty() { home } else { format!("{home}/") })
    }

    /// The absolute path with links resolved (the server's `realpath`).
    pub async fn real_path(&self, path: &str) -> Result<String, Error> {
        let name = retry!(self, path, READ_TRIES, |c| c.ask(c.raw.realpath(abs(path))).await)?;
        Ok(name.files.first().map(|f| f.filename.clone()).unwrap_or_default())
    }

    async fn stat(&self, path: &str) -> Result<FileAttributes, Error> {
        retry!(self, path, READ_TRIES, |c| c.ask(c.raw.stat(abs(path))).await.map(|a| a.attrs))
    }

    /// Everything in a folder but "." and "..", with its attributes.
    async fn entries(&self, dir: &str) -> Result<Vec<(String, FileAttributes)>, Error> {
        retry!(self, dir, READ_TRIES, |c| read_dir(c, dir).await)
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

    async fn open_file(&self, path: &str, flags: OpenFlags) -> Result<Opened, Error> {
        retry!(self, path, READ_TRIES, |c| open_file(c, path, flags).await)
    }

    /// A whole file. The reads below go again on a new connection when one dropped
    /// (`READ_TRIES`): reading the same bytes again changes nothing (#75).
    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        retry!(self, path, READ_TRIES, |c| read_whole(c, path).await)
    }

    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        retry!(self, path, READ_TRIES, |c| read_from(c, path, offset, len).await)
    }

    /// Writes a file from the chunks of a copy, replacing one with the same name; removed if the
    /// chunks end with an error or a write fails. A connection that drops is replaced once per
    /// piece of progress: the pieces it had not confirmed go again on the new one (`Pending`).
    pub async fn write_chunks(&self, path: &str, mut chunks: tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Error> {
        let mut f = self.open_file(path, OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE).await?;
        let mut w = Pending::default();
        let failed = loop {
            match write_all(&f, &mut chunks, &mut w).await {
                Ok(()) => break None,
                // Opened again without truncating it: what got through stays.
                Err(Ok(e)) if self.dropped(&f.conn, &e).await && w.again() => match self.open_file(path, OpenFlags::WRITE).await {
                    Ok(again) => f = again,
                    Err(e) => break Some(e),
                },
                Err(Ok(e)) => break Some(error(e, path)),
                Err(Err(e)) => break Some(("error-detail", e.to_string())),
            }
        };
        let r = match failed {
            // The handle is closed in the background: on a connection that stopped, waiting for
            // the answer would only add the silence again.
            Some(e) => Err(e),
            None => {
                let closed = f.conn.ask(f.conn.raw.close(std::mem::take(&mut f.handle))).await;
                self.checked(&f.conn, closed, path).await.map(|_| ())
            }
        };
        if r.is_err() {
            drop(f);
            let _ = self.delete(path).await;
        }
        r
    }

    /// Creates a folder whose parent exists; nothing if it is already there.
    pub async fn create_dir(&self, dir: &str) -> Result<(), Error> {
        let made = retry!(self, dir, |c| c.ask(c.raw.mkdir(abs(dir), FileAttributes::default())).await);
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
        retry!(self, from, |c| c.ask(c.raw.rename(abs(from), abs(to))).await).map(|_| ())
    }

    /// Renames the file `from` over the file `to` in one request, as `rename(2)` does, where the
    /// server can (`posix-rename@openssh.com`: OpenSSH and most servers); `None` where it cannot.
    pub async fn rename_over(&self, from: &str, to: &str) -> Option<Result<(), Error>> {
        match self.session().await {
            Ok(c) if !c.posix_rename => None,
            Ok(_) => Some(self.posix_rename(from, to).await),
            Err(e) => Some(Err(e)),
        }
    }

    async fn posix_rename(&self, from: &str, to: &str) -> Result<(), Error> {
        retry!(self, from, |c| posix_rename(c, from, to).await)
    }

    pub async fn exists(&self, path: &str) -> bool {
        let r: Result<_, Error> = (async { retry!(self, path, READ_TRIES, |c| c.ask(c.raw.lstat(abs(path))).await) }).await;
        r.is_ok()
    }

    /// Deletes a file, or a folder with everything inside it.
    pub async fn delete(&self, path: &str) -> Result<(), Error> {
        if !path.ends_with('/') {
            return retry!(self, path, |c| c.ask(c.raw.remove(abs(path))).await).map(|_| ());
        }
        for (name, a) in self.entries(path).await? {
            let child = format!("{path}{name}");
            // A symlink is removed, never followed.
            if a.file_type().is_dir() {
                Box::pin(self.delete(&format!("{child}/"))).await?;
            } else {
                retry!(self, &child, |c| c.ask(c.raw.remove(abs(&child))).await)?;
            }
        }
        retry!(self, path, |c| c.ask(c.raw.rmdir(abs(path))).await).map(|_| ())
    }

    /// Everything below a folder, hidden items and EyeCanSee's trash folders too
    /// (`transfer::place`), folders before what they contain: path and size (folders end in "/",
    /// size 0).
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
                    found.push((format!("{path}/"), 0));
                    todo.push(format!("{path}/"));
                } else {
                    found.push((path, a.size.unwrap_or(0)));
                }
                p.found.fetch_add(1, Ordering::Relaxed);
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(found)
    }

    /// Reads a file of `size` bytes from `from` in chunks of `chunk` bytes, handing each to `push`.
    pub async fn send<F, Fut>(&self, path: &str, from: u64, size: u64, chunk: u64, cancel: &AtomicBool, push: F) -> Result<(), Error>
    where
        F: Fn(Bytes) -> Fut,
        Fut: std::future::Future<Output = Result<(), Error>>,
    {
        let mut f = self.open_file(path, OpenFlags::READ).await?;
        let (mut at, mut buf, mut left) = (from, Vec::new(), READ_TRIES);
        loop {
            let from = at;
            match read_ahead(&f, path, size, chunk, cancel, (&mut at, &mut buf), &push).await {
                Ok(()) => return Ok(()),
                Err(Err(e)) => return Err(e),
                // What a dropped connection lost is read again, the file opened on a new one (#75).
                Err(Ok(e)) => {
                    left = if at > from { READ_TRIES } else { left } - 1;
                    if !(self.dropped(&f.conn, &e).await && left > 0) {
                        return Err(error(e, path));
                    }
                    f = self.open_file(path, OpenFlags::READ).await?;
                }
            }
        }
    }
}

/// Reads `f` from `at` to `size` with about 4 MiB always asked ahead, whatever the size of the
/// pieces (asking a chunk at a time and waiting for it slowed downloads), handing `push` `chunk`
/// bytes at a time. `at` follows the bytes received, `buf` holds those not yet pushed. Errors: the
/// server's, or ours (cancelled, file shorter, `push`).
async fn read_ahead<F, Fut>(f: &Opened, path: &str, size: u64, chunk: u64, cancel: &AtomicBool, (at, buf): (&mut u64, &mut Vec<u8>), push: &F) -> Result<(), Result<SftpError, Error>>
where
    F: Fn(Bytes) -> Fut,
    Fut: std::future::Future<Output = Result<(), Error>>,
{
    use futures_util::StreamExt;
    let piece = u64::from(f.conn.read_len);
    let ahead = ((4 << 20) / piece) as usize;
    let mut reads = futures_util::stream::FuturesOrdered::new();
    let mut next = *at;
    while *at < size {
        while reads.len() < ahead && next < size {
            let n = piece.min(size - next);
            reads.push_back(read_piece(&f.conn, &f.handle, next, n as u32));
            next += n;
        }
        let Some(part) = reads.next().await else { break };
        let part = part.map_err(Ok)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(Err(("error-cancelled", String::new())));
        }
        if (part.len() as u64) < piece.min(size - *at) {
            return Err(Err(("error-detail", format!("{path}: file shorter than expected"))));
        }
        *at += part.len() as u64;
        buf.extend(part);
        if buf.len() as u64 >= chunk || *at == size {
            push(Bytes::from(std::mem::take(buf))).await.map_err(Err)?;
        }
    }
    Ok(())
}

/// Connected, logged in, with the SFTP subsystem started. The steps after the login within
/// `SILENCE`: the lock of the connection is held meanwhile, and every request waits for it (#75).
async fn open(t: &Target) -> Result<Conn, Error> {
    let handle = connect::connect(t).await?;
    match tokio::time::timeout(SILENCE, start(handle, t)).await {
        Ok(r) => r,
        Err(_) => Err((SILENT, t.host.clone())),
    }
}

/// The SFTP subsystem started on a connection logged in.
async fn start(handle: Handle<connect::Client>, t: &Target) -> Result<Conn, Error> {
    let detail = |e: &dyn std::fmt::Display| ("error-detail", format!("{}: {e}", t.host));
    let channel = handle.channel_open_session().await.map_err(|e| detail(&e))?;
    // A server with SSH but no SFTP (a Synology with SFTP off) refuses here.
    channel.request_subsystem(true, "sftp").await.map_err(|e| ("error-no-sftp", format!("{}: {e}", t.host)))?;
    let mut raw = RawSftpSession::new(channel.into_stream());
    let version = raw.init().await.map_err(|e| ("error-no-sftp", format!("{}: {e}", t.host)))?;
    let posix_rename = version.extensions.get(POSIX_RENAME).is_some_and(|v| v == "1");
    log!("sftp {}: extensions {:?}", t.host, version.extensions);
    let (mut read_len, mut write_len) = (32 << 10, 32 << 10);
    if let Ok(limits) = raw.limits().await {
        read_len = limits.max_read_len.clamp(32 << 10, 256 << 10);
        write_len = limits.max_write_len.clamp(32 << 10, 256 << 10);
        raw.set_limits(limits.into());
    }
    // Only a last resort: a connection that stopped is found by `Conn::ask` after `SILENCE`.
    raw.set_timeout(60);
    Ok(Conn { handle, raw, read_len: read_len as u32, write_len: write_len as usize, posix_rename, watch: Watch::default() })
}

/// The extension that renames over a file; its request carries the two paths as SFTP strings.
const POSIX_RENAME: &str = "posix-rename@openssh.com";

async fn posix_rename(c: &Conn, from: &str, to: &str) -> Result<(), SftpError> {
    let mut data = Vec::new();
    for p in [abs(from), abs(to)] {
        data.extend_from_slice(&(p.len() as u32).to_be_bytes());
        data.extend_from_slice(p.as_bytes());
    }
    match c.ask(c.raw.extended(POSIX_RENAME, data)).await? {
        Packet::Status(s) if s.status_code == StatusCode::Ok => Ok(()),
        Packet::Status(s) => Err(s.into()),
        _ => Err(SftpError::UnexpectedPacket),
    }
}

/// Everything in a folder but "." and "..", with its attributes.
async fn read_dir(c: &Conn, dir: &str) -> Result<Vec<(String, FileAttributes)>, SftpError> {
    let handle = c.ask(c.raw.opendir(abs(dir))).await?.handle;
    let mut all = Vec::new();
    let r = loop {
        match c.ask(c.raw.readdir(handle.as_str())).await {
            Ok(name) => all.extend(name.files.into_iter().filter(|f| f.filename != "." && f.filename != "..").map(|f| (f.filename, f.attrs))),
            Err(e) if is_eof(&e) => break Ok(all),
            Err(e) => break Err(e),
        }
    };
    // Not on a connection that stopped: that would wait for the silence once more.
    if r.as_ref().err().is_none_or(|e| !lost(false, e)) {
        let _ = c.ask(c.raw.close(handle)).await;
    }
    r
}

async fn open_file(c: &Arc<Conn>, path: &str, flags: OpenFlags) -> Result<Opened, SftpError> {
    let handle = c.ask(c.raw.open(abs(path), flags, FileAttributes::default())).await?.handle;
    Ok(Opened { conn: c.clone(), handle })
}

/// A whole file, on the connection `c`.
async fn read_whole(c: &Arc<Conn>, path: &str) -> Result<Vec<u8>, SftpError> {
    let f = open_file(c, path, OpenFlags::READ).await?;
    match c.ask(c.raw.fstat(f.handle.as_str())).await?.attrs.size {
        Some(size) => read_at(&f, 0, size).await,
        // ponytail: size unknown (rare servers): one piece at a time until the end
        None => {
            let mut data = Vec::new();
            loop {
                let part = read_piece(c, &f.handle, data.len() as u64, c.read_len).await?;
                if part.is_empty() {
                    return Ok(data);
                }
                data.extend(part);
            }
        }
    }
}

/// `len` bytes of a file from `offset`, on the connection `c`.
async fn read_from(c: &Arc<Conn>, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, SftpError> {
    read_at(&open_file(c, path, OpenFlags::READ).await?, offset, len).await
}

/// `len` bytes from `offset`, fewer at the end of the file: pieces fetched together.
async fn read_at(f: &Opened, offset: u64, len: u64) -> Result<Vec<u8>, SftpError> {
    let piece = u64::from(f.conn.read_len);
    let pieces: Vec<(u64, u32)> = (offset..offset + len).step_by(piece as usize).map(|at| (at, piece.min(offset + len - at) as u32)).collect();
    let mut data = Vec::with_capacity(len as usize);
    for batch in pieces.chunks(IN_FLIGHT) {
        let mut reads = Vec::with_capacity(batch.len());
        for &(at, n) in batch {
            reads.push(read_piece(&f.conn, &f.handle, at, n));
        }
        for part in futures_util::future::try_join_all(reads).await? {
            let short = part.len() < piece as usize;
            data.extend(part);
            if short {
                return Ok(data); // end of the file
            }
        }
    }
    Ok(data)
}

/// `n` bytes from `at`, fewer only at the end of the file (servers may answer with less).
async fn read_piece(c: &Conn, handle: &str, at: u64, n: u32) -> Result<Vec<u8>, SftpError> {
    let mut data = Vec::with_capacity(n as usize);
    while data.len() < n as usize {
        match c.ask(c.raw.read(handle, at + data.len() as u64, n - data.len() as u32)).await {
            Ok(d) if d.data.is_empty() => break,
            Ok(d) => data.extend(d.data),
            Err(e) if is_eof(&e) => break,
            Err(e) => return Err(e),
        }
    }
    Ok(data)
}

/// The pieces of a write sent and not yet confirmed, by their place in the file. When the
/// connection drops they go again on a new one, at the same places, in the file opened without
/// truncating it: a piece written twice holds the same bytes, so the file cannot come out wrong,
/// whatever the old connection still delivers (#75).
#[derive(Default)]
struct Pending {
    /// Where the next piece goes.
    at: u64,
    parts: BTreeMap<u64, Bytes>,
    /// A new connection taken, and no piece confirmed since.
    tried: bool,
}

impl Pending {
    /// `part` goes next in the file: its place.
    fn sent(&mut self, part: Bytes) -> u64 {
        let at = self.at;
        self.at += part.len() as u64;
        self.parts.insert(at, part);
        at
    }

    fn confirmed(&mut self, at: u64) {
        self.parts.remove(&at);
        self.tried = false;
    }

    /// Once more on a new connection? Not twice in a row without a piece getting through.
    fn again(&mut self) -> bool {
        !std::mem::replace(&mut self.tried, true)
    }
}

/// Writes the chunks of a copy in pieces, the ones left by a dropped connection first, about
/// `IN_FLIGHT` sent and not yet confirmed: the next chunk goes out while the server confirms the
/// previous one. Errors: the server's, or the one that ended the chunks.
async fn write_all(f: &Opened, chunks: &mut tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>, w: &mut Pending) -> Result<(), Result<SftpError, std::io::Error>> {
    use futures_util::StreamExt;
    let mut sent = futures_util::stream::FuturesUnordered::new();
    for (&at, part) in &w.parts {
        sent.push(write_part(f, at, part.clone()));
    }
    let mut ended = false;
    while !ended || !sent.is_empty() {
        tokio::select! {
            biased;
            Some((at, r)) = sent.next(), if !sent.is_empty() => {
                r.map_err(Ok)?;
                w.confirmed(at);
            }
            chunk = chunks.recv(), if !ended && sent.len() < IN_FLIGHT => match chunk {
                None => ended = true,
                Some(chunk) => {
                    let chunk = chunk.map_err(Err)?;
                    for from in (0..chunk.len()).step_by(f.conn.write_len) {
                        let part = chunk.slice(from..(from + f.conn.write_len).min(chunk.len()));
                        sent.push(write_part(f, w.sent(part.clone()), part));
                    }
                }
            },
        }
    }
    Ok(())
}

/// One piece at its place, and the place back with the answer.
async fn write_part(f: &Opened, at: u64, part: Bytes) -> (u64, Result<(), SftpError>) {
    (at, f.conn.ask(f.conn.raw.write(f.handle.as_str(), at, part.to_vec())).await.map(|_| ()))
}

#[cfg(test)]
mod tests {
    use crate::source::{self, Deletion, Store};
    use bytes::Bytes;

    /// The pieces a dropped connection did not confirm, sent again at their places over what got
    /// through, give the whole file; a new connection only after some progress.
    #[test]
    fn pending_pieces_sent_again() {
        let data: Vec<u8> = (0..100u8).collect();
        let mut w = super::Pending::default();
        let places: Vec<u64> = data.chunks(30).map(|p| w.sent(Bytes::copy_from_slice(p))).collect();
        assert_eq!(places, [0, 30, 60, 90]);
        // The server wrote the pieces at 0 and 60 and confirmed the first; then nothing more.
        let mut file = vec![0u8; 100];
        file[..30].copy_from_slice(&data[..30]);
        file[60..90].copy_from_slice(&data[60..90]);
        w.confirmed(0);
        assert!(w.again());
        assert_eq!(w.parts.keys().copied().collect::<Vec<_>>(), [30, 60, 90]);
        for (&at, part) in &w.parts {
            file[at as usize..at as usize + part.len()].copy_from_slice(part);
        }
        assert_eq!(file, data);
        // The new connection drops too before any piece got through: no third one.
        assert!(!w.again());
        w.confirmed(30);
        assert!(w.again());
    }

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
        server_live("EYECANSEE_SFTP");
    }

    /// The same on an FTP server (`ftp.rs`), with `EYECANSEE_FTP`.
    #[test]
    #[ignore]
    fn ftp_live() {
        server_live("EYECANSEE_FTP");
    }

    /// The connection cut in the middle of a write, a read of a range and a whole read: each goes
    /// on, on a new connection, and the file comes back whole (#75). In a new folder, deleted.
    /// `EYECANSEE_SFTP=sftp://user@host:port/folder/ cargo test --release sftp_cut_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn sftp_cut_live() {
        let url = std::env::var("EYECANSEE_SFTP").expect("EYECANSEE_SFTP not set");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let s = source::open(&url).unwrap();
            let Store::Sftp(server) = &s.op else { panic!("not SFTP") };
            let home = server.home().await.unwrap();
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let dir = format!("{}eyecansee-test-{secs}/", if s.home { &home } else { &s.dir });
            let (op, file) = (&s.op, format!("{dir}cut.bin"));
            op.create_dir(&dir).await.unwrap();
            let data: Vec<u8> = (0..256u32 << 20).map(|i| (i.wrapping_mul(2654435761) >> 11) as u8).collect();
            let conn = || async { server.conn.lock().await.clone().expect("a connection") };
            // The connection in use cut 200 ms after the start, as a stalled one is left; the
            // operation must have gone on, on a new one.
            let cut = |what: &'static str| async move {
                let before = conn().await;
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                println!("{what}: cut: {:?}", before.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await);
                before
            };
            let t = std::time::Instant::now();
            let ((), before) = tokio::join!(write(op, &file, &data), cut("write"));
            println!("write 256 MB with a cut: {:?}", t.elapsed());
            assert!(!std::sync::Arc::ptr_eq(&before, &conn().await), "the write ended before the cut");
            assert_eq!(op.size(&file).await.unwrap(), data.len() as u64);
            let t = std::time::Instant::now();
            let (part, before) = tokio::join!(op.read_range(&file, 1 << 20, 200 << 20), cut("range"));
            println!("range of 200 MB with a cut: {:?}", t.elapsed());
            assert!(!std::sync::Arc::ptr_eq(&before, &conn().await), "the read ended before the cut");
            assert!(part.unwrap() == data[1 << 20..201 << 20], "range differs");
            let t = std::time::Instant::now();
            let (whole, before) = tokio::join!(op.read(&file), cut("read"));
            println!("read 256 MB with a cut: {:?}", t.elapsed());
            assert!(!std::sync::Arc::ptr_eq(&before, &conn().await), "the read ended before the cut");
            assert!(whole.unwrap() == data, "read back differs");
            op.delete(&dir, Deletion::Permanent).await.unwrap();
        });
    }

    fn server_live(var: &str) {
        let url = std::env::var(var).unwrap_or_else(|_| panic!("{var} not set"));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let s = source::open(&url).unwrap();
            let t = std::time::Instant::now();
            let home = match &s.op {
                Store::Sftp(server) => server.home().await.unwrap(),
                Store::Ftp(server) => server.home().await.unwrap(),
                _ => panic!("not a server"),
            };
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
            assert!(walked.iter().any(|(p, _)| p.ends_with(&format!("{}/sub2/inner/{}/z.jpg", source::TRASH_DIR, source::TRASH_DIR))), "the trash is walked too");
            assert_eq!(op.read(&format!("{dir}nothing.jpg")).await.unwrap_err().0, source::NOT_FOUND);
            op.delete(&dir, Deletion::Permanent).await.unwrap();
            assert_eq!(op.check_dir(&dir).await.unwrap_err().0, source::NOT_FOUND);
        });
    }
}
