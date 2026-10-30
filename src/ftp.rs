//! FTP servers with `suppaftp` (tokio): up to `POOL` connections per server, each with one
//! transfer at a time, kept open between requests. Passive mode, with the data connection to the
//! address of the control connection (`set_passive_nat_workaround`), as curl and FileZilla do: a
//! server behind NAT or a VPN often announces an address that cannot be reached (a Synology
//! reached over Tailscale says 127.0.0.1).
//!
//! Paths as for the other sources, relative to the server's "/" (folders end in "/").

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bytes::Bytes;
use suppaftp::list::{File, ListParser};
use suppaftp::tokio::AsyncRustlsFtpStream;
use suppaftp::types::FileType;
use suppaftp::{FtpError, Mode, Status};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::source::{Error, Listing, NOT_FOUND, TRASH_DIR};

/// Connections per server: the thumbnails are read four at a time, as on SFTP.
pub const POOL: usize = 4;

type Stream = AsyncRustlsFtpStream;

/// Where to connect, as the address and the saved server give it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Target {
    pub host: String,
    pub port: u16,
    /// "" = anonymous.
    pub user: String,
    pub password: String,
}

pub struct Server {
    target: Target,
    idle: Mutex<Vec<Stream>>,
    permits: tokio::sync::Semaphore,
}

/// A connection taken from the pool; `ok` false: not given back (broken, or a transfer left in
/// an unknown state).
struct Conn {
    s: Stream,
    ok: bool,
}

/// Servers in use: the clipboard can keep the source of a copy, which then shares the connections.
static SERVERS: Mutex<Vec<Weak<Server>>> = Mutex::new(Vec::new());

/// Absolute path on the server.
fn abs(path: &str) -> String {
    format!("/{}", path.trim_end_matches('/'))
}

fn error(e: FtpError, path: &str) -> Error {
    match &e {
        FtpError::UnexpectedResponse(r) if r.status == Status::NotLoggedIn => ("error-login", path.to_string()),
        FtpError::UnexpectedResponse(r) if r.status == Status::FileUnavailable => {
            let body = String::from_utf8_lossy(&r.body).to_lowercase();
            if body.contains("permission") || body.contains("denied") {
                ("error-permission", path.to_string())
            } else {
                (NOT_FOUND, path.to_string())
            }
        }
        _ => ("error-detail", format!("{path}: {e}")),
    }
}

/// The connection is gone (closed by the server after its idle timeout, network down).
fn broken(e: &FtpError) -> bool {
    matches!(e, FtpError::ConnectionError(_) | FtpError::BadResponse)
        || matches!(e, FtpError::UnexpectedResponse(r) if r.status == Status::NotAvailable)
}

/// The servers in use at this host, port and user.
fn at(host: &str, port: u16, user: &str) -> Vec<Arc<Server>> {
    let servers = SERVERS.lock().unwrap();
    let same = |s: &Arc<Server>| s.target.host.eq_ignore_ascii_case(host) && s.target.port == port && s.target.user == user;
    servers.iter().filter_map(Weak::upgrade).filter(same).collect()
}

/// Whether a connection to this server is open.
pub fn connected(host: &str, port: u16, user: &str) -> bool {
    at(host, port, user).iter().any(|s| !s.idle.lock().unwrap().is_empty() || s.permits.available_permits() < POOL)
}

/// Closes the idle connections to this server ("Close connection"); a later request opens a new one.
pub async fn disconnect(host: String, port: u16, user: String) {
    for s in at(&host, port, &user) {
        let idle: Vec<Stream> = std::mem::take(&mut *s.idle.lock().unwrap());
        for mut c in idle {
            let _ = c.quit().await;
        }
        log!("ftp: disconnected from {host}");
    }
}

/// A transfer whose data has started moving cannot be tried again on a new connection: the
/// chunks already sent are gone, and the file would end up short.
fn once(began: &AtomicBool) -> Result<(), FtpError> {
    match began.swap(true, Ordering::Relaxed) {
        true => Err(FtpError::ConnectionError(std::io::Error::other("connection lost during the transfer"))),
        false => Ok(()),
    }
}

/// RETR of `path` from `offset`. suppaftp sends REST before PASV, and a Synology forgets the
/// REST at the PASV and sends the file from the start: here PASV (or EPSV), data connection,
/// REST, RETR, in this order.
async fn retr_from(s: &mut Stream, path: &str, offset: u64) -> Result<tokio::net::TcpStream, FtpError> {
    let at = data_address(s).await?;
    let data = tokio::net::TcpStream::connect(at).await.map_err(FtpError::ConnectionError)?;
    s.custom_command(format!("REST {offset}"), &[Status::RequestFilePending]).await?;
    s.custom_command(format!("RETR {}", abs(path)), &[Status::AboutToSend, Status::AlreadyOpen]).await?;
    Ok(data)
}

/// Where to open a data connection: the port from EPSV (IPv6) or PASV (IPv4), at the address of
/// the control connection, not the one in the answer (see the top of the file).
async fn data_address(s: &mut Stream) -> Result<std::net::SocketAddr, FtpError> {
    let peer = s.get_ref().peer_addr().map_err(FtpError::ConnectionError)?;
    let port = if peer.is_ipv6() {
        let r = s.custom_command("EPSV", &[Status::ExtendedPassiveMode]).await?;
        epsv_port(&String::from_utf8_lossy(&r.body))
    } else {
        let r = s.custom_command("PASV", &[Status::PassiveMode]).await?;
        pasv_port(&String::from_utf8_lossy(&r.body))
    };
    Ok(std::net::SocketAddr::new(peer.ip(), port.ok_or(FtpError::BadResponse)?))
}

/// The port of an EPSV answer: "229 Entering Extended Passive Mode (|||55677|)" → 55677.
fn epsv_port(answer: &str) -> Option<u16> {
    answer.split_once("|||")?.1.split_once('|')?.0.trim().parse().ok()
}

/// The port of a PASV answer: "227 Entering Passive Mode (127,0,0,1,217,125)" → 55677.
fn pasv_port(answer: &str) -> Option<u16> {
    let inside = answer.split_once('(')?.1.split_once(')')?.0;
    let n: Vec<u16> = inside.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    (n.len() == 6).then(|| n[4] * 256 + n[5])
}

/// A listed item: a line of MLSD, or of LIST (Unix or DOS).
fn parse(line: &str, mlsd: bool) -> Option<File> {
    if mlsd { ListParser::parse_mlsd(line).ok() } else { line.parse().ok() }
}

/// `$op`, an expression using the pooled connection `$c` (`&mut Conn`) and giving a
/// `Result<_, FtpError>`, as `Result<_, Error>`. An idle connection the server has closed in the
/// meantime is replaced once by a new one. A macro, not an async closure: those futures are not
/// `Send` for the tasks that run them.
macro_rules! with {
    ($server:expr, $path:expr, |$c:ident| $op:expr) => {{
        let server: &Server = $server;
        let _permit = server.permits.acquire().await.map_err(|e| ("error-detail", e.to_string()))?;
        loop {
            let idle = server.idle.lock().unwrap().pop();
            let fresh = idle.is_none();
            let s = match idle {
                Some(s) => s,
                None => server.connect().await?,
            };
            let mut conn = Conn { s, ok: true };
            let r = {
                let $c: &mut Conn = &mut conn;
                $op
            };
            if let Err(e) = &r {
                conn.ok &= !broken(e);
                if !fresh && !conn.ok {
                    continue;
                }
            }
            if conn.ok {
                server.idle.lock().unwrap().push(conn.s);
            }
            break r.map_err(|e| error(e, $path));
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
        let s = Arc::new(Server { target, idle: Mutex::new(Vec::new()), permits: tokio::sync::Semaphore::new(POOL) });
        servers.push(Arc::downgrade(&s));
        s
    }

    fn label(&self) -> String {
        let t = &self.target;
        if t.port == 21 { t.host.clone() } else { format!("{}:{}", t.host, t.port) }
    }

    /// A new connection, logged in, binary transfers, passive mode to the server's own address.
    async fn connect(&self) -> Result<Stream, Error> {
        let t = &self.target;
        let label = self.label();
        let started = std::time::Instant::now();
        let connecting = Stream::connect(format!("{}:{}", t.host, t.port));
        let mut s = match tokio::time::timeout(Duration::from_secs(20), connecting).await {
            Err(_) => return Err(("error-detail", format!("{label}: no answer"))),
            Ok(Err(e)) => return Err(("error-detail", format!("{label}: {e}"))),
            Ok(Ok(s)) => s,
        };
        // EPSV in IPv6, where PASV cannot work (a Storage Box answers it with 421); PASV in IPv4,
        // where old servers may lack EPSV.
        if s.get_ref().peer_addr().is_ok_and(|a| a.is_ipv6()) {
            s.set_mode(Mode::ExtendedPassive);
        } else {
            s.set_mode(Mode::Passive);
            s.set_passive_nat_workaround(true);
        }
        let (user, password) = if t.user.is_empty() { ("anonymous", "anonymous@") } else { (t.user.as_str(), t.password.as_str()) };
        s.login(user, password).await.map_err(|e| match e {
            FtpError::UnexpectedResponse(r) if r.status == Status::NotLoggedIn => ("error-login", format!("{user}@{label}")),
            e => ("error-detail", format!("{label}: {e}")),
        })?;
        s.transfer_type(FileType::Binary).await.map_err(|e| error(e, &label))?;
        log!("ftp {label}: connected as {user} in {:.2} s", started.elapsed().as_secs_f64());
        Ok(s)
    }

    /// The folder the server starts in (usually the user's home), as a source path.
    pub async fn home(&self) -> Result<String, Error> {
        let home = with!(self, "", |c| c.s.pwd().await)?;
        let home = home.trim_matches('/');
        Ok(if home.is_empty() { String::new() } else { format!("{home}/") })
    }

    /// Everything in a folder but "." and "..": MLSD where the server has it (exact times),
    /// otherwise LIST.
    async fn entries(&self, dir: &str) -> Result<Vec<File>, Error> {
        let (lines, mlsd) = with!(self, dir, |c| match c.s.mlsd(Some(&abs(dir))).await {
            Ok(lines) => Ok((lines, true)),
            Err(FtpError::UnexpectedResponse(r)) if matches!(r.status, Status::NotImplemented | Status::BadCommand | Status::BadArguments) => {
                c.s.list(Some(&abs(dir))).await.map(|l| (l, false))
            }
            Err(e) => Err(e),
        })?;
        Ok(lines.iter().filter_map(|l| parse(l, mlsd)).filter(|f| f.name() != "." && f.name() != "..").collect())
    }

    /// Is this entry a folder? A symlink is followed.
    async fn entry_is_dir(&self, path: &str, f: &File) -> bool {
        if f.is_symlink() { self.is_dir(path).await } else { f.is_directory() }
    }

    pub async fn list(&self, dir: &str) -> Result<Listing, Error> {
        let mut listing = Listing::default();
        for f in self.entries(dir).await? {
            let is_dir = self.entry_is_dir(&format!("{dir}{}", f.name()), &f).await;
            let path = format!("{dir}{}{}", f.name(), if is_dir { "/" } else { "" });
            let modified = f.modified().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default();
            listing.add(path, f.name(), is_dir, f.size() as u64, modified, true);
        }
        listing.sort();
        Ok(listing)
    }

    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        Ok(self.entries(dir).await?.into_iter().map(|f| f.name().to_string()).collect())
    }

    pub async fn size(&self, path: &str) -> Result<u64, Error> {
        with!(self, path, |c| c.s.size(abs(path)).await).map(|n| n as u64)
    }

    pub async fn is_dir(&self, path: &str) -> bool {
        self.check_dir(path).await.is_ok()
    }

    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        with!(self, dir, |c| c.s.cwd(abs(dir)).await)
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        with!(self, path, |c| async {
            let mut data = Vec::new();
            let mut r = c.s.retr_as_stream(abs(path)).await?;
            r.read_to_end(&mut data).await.map_err(FtpError::ConnectionError)?;
            c.s.finalize_retr_stream(r).await?;
            Ok::<_, FtpError>(data)
        }
        .await)
    }

    /// `len` bytes from `offset`, fewer at the end of the file. A transfer stopped before the end
    /// is aborted; a connection whose abort fails is not used again.
    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        with!(self, path, |c| async {
            let mut r = retr_from(&mut c.s, path, offset).await?;
            let mut data = Vec::with_capacity(len as usize);
            (&mut r).take(len).read_to_end(&mut data).await.map_err(FtpError::ConnectionError)?;
            if (data.len() as u64) < len {
                drop(r);
                c.s.read_response_in(&[Status::ClosingDataConnection, Status::RequestedFileActionOk]).await?;
            } else if let Err(e) = c.s.abort(r).await {
                log!("ftp: abort after a partial read of {path}: {e}");
                c.ok = false;
            }
            Ok::<_, FtpError>(data)
        }
        .await)
    }

    /// Writes a file from the chunks of a copy, replacing one with the same name; removed if the
    /// chunks end with an error or the transfer fails.
    pub async fn write_chunks(&self, path: &str, mut chunks: tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>) -> Result<(), Error> {
        let began = AtomicBool::new(false);
        let r: Result<(), Error> = (async {
            with!(self, path, |c| async {
                let mut w = c.s.put_with_stream(abs(path)).await?;
                once(&began)?;
                while let Some(chunk) = chunks.recv().await {
                    let chunk = chunk.map_err(FtpError::ConnectionError)?;
                    w.write_all(&chunk).await.map_err(FtpError::ConnectionError)?;
                }
                c.s.finalize_put_stream(w).await
            }
            .await)
        })
        .await;
        if r.is_err() {
            let _ = self.delete(path).await;
        }
        r
    }

    /// Creates a folder whose parent exists; nothing if it is already there.
    pub async fn create_dir(&self, dir: &str) -> Result<(), Error> {
        match with!(self, dir, |c| c.s.mkdir(abs(dir)).await) {
            Err(_) if self.is_dir(dir).await => Ok(()),
            r => r,
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

    /// Renames or moves a file or folder on the server itself (RNFR, RNTO); `to` must be free.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), Error> {
        with!(self, from, |c| c.s.rename(abs(from), abs(to)).await)
    }

    pub async fn exists(&self, path: &str) -> bool {
        let name = crate::source::name(path);
        self.names(&crate::source::parent(path)).await.is_ok_and(|n| n.contains(name))
    }

    /// Deletes a file, or a folder with everything inside it.
    pub async fn delete(&self, path: &str) -> Result<(), Error> {
        if !path.ends_with('/') {
            return with!(self, path, |c| c.s.rm(abs(path)).await);
        }
        for f in self.entries(path).await? {
            let child = format!("{path}{}", f.name());
            // A symlink is removed, never followed.
            if f.is_directory() {
                Box::pin(self.delete(&format!("{child}/"))).await?;
            } else {
                with!(self, &child, |c| c.s.rm(abs(&child)).await)?;
            }
        }
        with!(self, path, |c| c.s.rmdir(abs(path)).await)
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
            for f in self.entries(&d).await? {
                let path = format!("{d}{}", f.name());
                if self.entry_is_dir(&path, &f).await {
                    if f.name() != TRASH_DIR {
                        found.push((format!("{path}/"), 0));
                        todo.push(format!("{path}/"));
                    }
                } else {
                    found.push((path, f.size() as u64));
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
        // Errors of `push` (the destination) travel apart from the server's.
        let pushed: Mutex<Option<Error>> = Mutex::new(None);
        let began = AtomicBool::new(false);
        let r: Result<(), Error> = (async {
            with!(self, path, |c| async {
                let mut r = c.s.retr_as_stream(abs(path)).await?;
                once(&began)?;
                let mut at = 0;
                while at < size {
                    let stop = if cancel.load(Ordering::Relaxed) { Some(("error-cancelled", String::new())) } else { None };
                    let mut data = vec![0; chunk.min(size - at) as usize];
                    let read = if stop.is_none() { r.read_exact(&mut data).await.map(|_| ()) } else { Ok(()) };
                    let failed = match (stop, read) {
                        (Some(e), _) => Some(e),
                        (None, Err(e)) => Some(("error-detail", format!("{path}: {e}"))),
                        (None, Ok(())) => {
                            at += data.len() as u64;
                            push(Bytes::from(data)).await.err()
                        }
                    };
                    if let Some(e) = failed {
                        *pushed.lock().unwrap() = Some(e);
                        if c.s.abort(r).await.is_err() {
                            c.ok = false;
                        }
                        return Ok(());
                    }
                }
                c.s.finalize_retr_stream(r).await
            }
            .await)
        })
        .await;
        match pushed.into_inner().unwrap() {
            Some(e) => Err(e),
            None => r,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn listed_items() {
        let unix = super::parse("-rw-r--r--    1 me       users     1234 Oct 30 09:10 a b.jpg", false).unwrap();
        assert_eq!((unix.name(), unix.size(), unix.is_directory()), ("a b.jpg", 1234, false));
        let dir = super::parse("drwxr-xr-x    2 me       users     4096 Jan  1  2025 Foto", false).unwrap();
        assert!(dir.is_directory());
        let mlsd = super::parse("type=file;size=42;modify=20261030091000; x.mp4", true).unwrap();
        assert_eq!((mlsd.name(), mlsd.size()), ("x.mp4", 42));
        assert_eq!(super::abs("Foto/2026/"), "/Foto/2026");
        assert_eq!(super::abs(""), "/");
        assert_eq!(super::pasv_port("Entering Passive Mode (127,0,0,1,217,125)."), Some(55677));
        assert_eq!(super::pasv_port("Entering Passive Mode"), None);
        assert_eq!(super::epsv_port("Entering Extended Passive Mode (|||55677|)"), Some(55677));
        assert_eq!(super::epsv_port("Entering Extended Passive Mode"), None);
    }
}
