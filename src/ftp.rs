//! FTP and FTPS servers with `suppaftp` (tokio, TLS in `ftp/tls.rs`): up to `POOL` connections per server, each with one
//! transfer at a time, kept open between requests. Passive mode, with the data connection to the
//! address of the control connection (`set_passive_nat_workaround`), as curl and FileZilla do: a
//! server behind NAT or a VPN often announces an address that cannot be reached (a NAS reached
//! through a VPN said 127.0.0.1).
//!
//! Paths as for the other sources, relative to the server's "/" (folders end in "/").

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bytes::Bytes;
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::FileType;
use suppaftp::{FtpError, Mode, Status};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::source::{Error, Listing, NOT_FOUND, SILENT};

mod listing;
mod tls;
use listing::Item;
pub use tls::{trust, unknown};

/// Connections per server: the thumbnails are read four at a time, as on SFTP.
pub const POOL: usize = 4;

/// A server that does not answer for this long is given up: a command or a listing as a whole, a
/// transfer when no byte moves (a server that never sent the data of an FTPS listing kept the app
/// waiting for good).
const WAIT: Duration = Duration::from_secs(30);

fn timed_out() -> FtpError {
    FtpError::ConnectionError(std::io::Error::new(std::io::ErrorKind::TimedOut, "the server does not answer"))
}

/// `f` within `WAIT`.
async fn waited<T>(f: impl std::future::Future<Output = Result<T, FtpError>>) -> Result<T, FtpError> {
    tokio::time::timeout(WAIT, f).await.unwrap_or_else(|_| Err(timed_out()))
}

/// Up to `limit` bytes, until the end; a pause longer than `WAIT` is an error.
async fn read_up_to(r: &mut (impl AsyncRead + Unpin), limit: u64) -> Result<Vec<u8>, FtpError> {
    let mut data = Vec::new();
    let mut buf = vec![0; 256 << 10];
    while (data.len() as u64) < limit {
        let want = buf.len().min((limit - data.len() as u64) as usize);
        let n = waited(async { r.read(&mut buf[..want]).await.map_err(FtpError::ConnectionError) }).await?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    Ok(data)
}

type Stream = AsyncRustlsFtpStream;

/// Where to connect, as the address and the saved server give it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Target {
    pub host: String,
    pub port: u16,
    /// "" = anonymous.
    pub user: String,
    pub password: String,
    /// FTPS: TLS on the control and data connections (explicit, AUTH TLS).
    pub secure: bool,
}

pub struct Server {
    target: Target,
    idle: Mutex<Vec<Stream>>,
    permits: tokio::sync::Semaphore,
    /// FTPS: the TLS shared by the control and data connections.
    tls: Option<tokio_rustls::TlsConnector>,
    /// Whether a rename onto a file replaces it here (`UNKNOWN`, `REPLACES`, `REFUSES`): learnt
    /// at the first try (`rename_over`).
    over: AtomicU8,
}

const UNKNOWN: u8 = 0;
const REPLACES: u8 = 1;
const REFUSES: u8 = 2;

/// A rename of `from` over a file (issue #92): `attempt` renames and gives `Some(status)` when
/// the server answers with a refusal. `None`: renaming over a file is refused here, known from
/// `over` (then not tried again) or from this answer; nothing has changed and the caller does
/// it another way. Learnt at the first answer: a server that has replaced once is not marked as
/// refusing by a later refusal with another cause.
async fn over<F: std::future::Future<Output = Result<Option<Status>, Error>>>(over: &AtomicU8, from: &str, attempt: impl FnOnce() -> F) -> Option<Result<(), Error>> {
    if over.load(Ordering::Relaxed) == REFUSES {
        return None;
    }
    match attempt().await {
        Ok(None) => {
            over.store(REPLACES, Ordering::Relaxed);
            Some(Ok(()))
        }
        Ok(Some(status)) => {
            if over.compare_exchange(UNKNOWN, REFUSES, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                log!("ftp: renaming {from:?} over a file refused ({status:?}): moving the file aside from now on");
            }
            None
        }
        Err(e) => Some(Err(e)),
    }
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

/// A data connection, plain or TLS.
trait DataIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> DataIo for T {}
type Data = Box<dyn DataIo>;

/// Opens a data connection and sends `cmd` (RETR, STOR, MLSD, LIST), after "REST `offset`" if not 0.
/// suppaftp does it in an order some servers do not follow: REST before PASV (a Synology forgets
/// the REST and sends the file from the start) and the TLS of the data connection before the
/// answer (a Storage Box answers "no such file" and never starts TLS: 30 s waiting). Here PASV or
/// EPSV, data connection, REST, then the command and its TLS together, stopping at the first error.
async fn open_data(s: &mut Stream, tls: Option<&tokio_rustls::TlsConnector>, host: &str, cmd: String, offset: u64) -> Result<Data, FtpError> {
    let at = data_address(s).await?;
    let tcp = tokio::net::TcpStream::connect(at).await.map_err(FtpError::ConnectionError)?;
    if offset > 0 {
        s.custom_command(format!("REST {offset}"), &[Status::RequestFilePending]).await?;
    }
    let answer = s.custom_command(cmd, &[Status::AboutToSend, Status::AlreadyOpen]);
    let Some(tls) = tls else {
        answer.await?;
        return Ok(Box::new(tcp));
    };
    let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|e| FtpError::SecureError(e.to_string()))?;
    let handshake = async { tls.connect(name, tcp).await.map_err(FtpError::ConnectionError) };
    let (_, data) = tokio::try_join!(answer, handshake)?;
    Ok(Box::new(data))
}

/// After ABOR some servers send one more line (a Synology: "225 ABOR command successful"), which
/// would be taken for the answer to the next command: NOOP, and every answer up to its "200" read
/// and dropped.
async fn resync(s: &mut Stream) -> Result<(), FtpError> {
    let mut r = s.custom_command("NOOP", &[Status::CommandOk]).await;
    for _ in 0..3 {
        match r {
            Ok(_) => return Ok(()),
            Err(FtpError::UnexpectedResponse(_)) => r = s.read_response_in(&[Status::CommandOk]).await,
            Err(e) => return Err(e),
        }
    }
    Err(FtpError::BadResponse)
}

/// The end of a transfer: the data connection closed (`writing`: flushed and shut first), then
/// the server's "transfer complete".
async fn close_data(s: &mut Stream, mut data: Data, writing: bool) -> Result<(), FtpError> {
    if writing {
        data.flush().await.map_err(FtpError::ConnectionError)?;
        data.shutdown().await.map_err(FtpError::ConnectionError)?;
        // Until the server closes its side too: a socket closed while the server's own TLS
        // close is still arriving is reset, and a Synology then dropped the end of the file
        // (10 MB arrived as 9.3-9.5 MB over FTPS).
        let _ = tokio::time::timeout(WAIT, async { data.read_to_end(&mut Vec::new()).await }).await;
    }
    drop(data);
    s.read_response_in(&[Status::ClosingDataConnection, Status::RequestedFileActionOk]).await.map(|_| ())
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

/// A file whose data ended before its size (`Server::send`): the server's answer to the transfer
/// says why, after a data connection closed early. A Storage Box (SFTPGo) reading a link to a
/// folder sends nothing, then "550 … is a directory"; the bar said "Connection error: early eof"
/// (L17 of 2026-11-21). The error said plainly (`fault::SERVER_CUT`), with the server's answer as
/// the detail; whether the connection can still be used.
async fn cut_short(s: &mut Stream, data: Data, path: &str) -> (Error, bool) {
    drop(data);
    let answer = waited(s.read_response_in(&[Status::ClosingDataConnection, Status::RequestedFileActionOk])).await;
    log!("ftp: {path:?} ended before its size; the server then said {answer:?}");
    let (detail, ok) = match answer {
        Err(FtpError::UnexpectedResponse(r)) => (String::from_utf8_lossy(&r.body).trim().to_string(), r.status != Status::NotAvailable),
        // "Transfer complete": the file is shorter than listed.
        Ok(r) => (String::from_utf8_lossy(&r.body).trim().to_string(), true),
        Err(e) => (e.to_string(), false),
    };
    ((crate::fault::SERVER_CUT, format!("{}\u{1}{detail}", crate::source::name(path))), ok)
}

/// `$op`, an expression using the pooled connection `$c` (`&mut Conn`) and giving a
/// `Result<_, FtpError>`, as `Result<_, Error>`. An idle connection the server has closed in the
/// meantime is replaced once by a new one. A macro, not an async closure: those futures are not
/// `Send` for the tasks that run them.
macro_rules! with {
    // Commands and listings: the whole operation within `WAIT`.
    ($server:expr, $path:expr, |$c:ident| $op:expr) => {
        with!($server, $path, transfer |$c| waited(async { $op }).await)
    };
    // Transfers: as long as they take; their reads and writes have `WAIT` each.
    ($server:expr, $path:expr, transfer |$c:ident| $op:expr) => {{
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
        let key = format!("{}:{}", target.host, target.port);
        let tls = target.secure.then(|| tls::connector(&target.host, &key));
        let s = Arc::new(Server { target, idle: Mutex::new(Vec::new()), permits: tokio::sync::Semaphore::new(POOL), tls, over: AtomicU8::new(UNKNOWN) });
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
            Err(_) => return Err((SILENT, label)),
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
        if let Some(tls) = &self.tls {
            let key = format!("{}:{}", t.host, t.port);
            s = waited(s.into_secure(AsyncRustlsConnector::from(tls.clone()), &t.host)).await.map_err(|e| match tls::waiting(&key) {
                true => ("error-cert-unknown", label.clone()),
                false => ("error-detail", format!("{label}: {e}")),
            })?;
        }
        let (user, password) = if t.user.is_empty() { ("anonymous", "anonymous@") } else { (t.user.as_str(), t.password.as_str()) };
        waited(s.login(user, password)).await.map_err(|e| match e {
            FtpError::UnexpectedResponse(r) if r.status == Status::NotLoggedIn => ("error-login", format!("{user}@{label}")),
            e => ("error-detail", format!("{label}: {e}")),
        })?;
        waited(s.transfer_type(FileType::Binary)).await.map_err(|e| error(e, &label))?;
        log!("ftp {label}: connected as {user} in {:.2} s", started.elapsed().as_secs_f64());
        Ok(s)
    }

    /// `open_data` with this server's TLS.
    async fn data(&self, s: &mut Stream, cmd: String, offset: u64) -> Result<Data, FtpError> {
        open_data(s, self.tls.as_ref(), &self.target.host, cmd, offset).await
    }

    /// The folder the server starts in (usually the user's home), as a source path.
    pub async fn home(&self) -> Result<String, Error> {
        let home = with!(self, "", |c| c.s.pwd().await)?;
        let home = home.trim_matches('/');
        Ok(if home.is_empty() { String::new() } else { format!("{home}/") })
    }

    /// The text of a listing of `dir`: `cmd` is MLSD or LIST.
    async fn listing(&self, s: &mut Stream, cmd: &str, dir: &str) -> Result<String, FtpError> {
        let mut d = self.data(s, format!("{cmd} {}", abs(dir)), 0).await?;
        let bytes = read_up_to(&mut d, u64::MAX).await?;
        close_data(s, d, false).await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Everything in a folder but "." and "..": MLSD where the server has it (exact times),
    /// otherwise LIST. A small file of MLSD may be a link: then one LIST of the folder tells the
    /// links (`listing::mark_links`), never a request for each item.
    async fn entries(&self, dir: &str) -> Result<Vec<Item>, Error> {
        let (text, mlsd) = with!(self, dir, |c| async {
            match self.listing(&mut c.s, "MLSD", dir).await {
                Ok(text) => Ok((text, true)),
                Err(FtpError::UnexpectedResponse(r)) if matches!(r.status, Status::NotImplemented | Status::BadCommand | Status::BadArguments) => {
                    Ok((self.listing(&mut c.s, "LIST", dir).await?, false))
                }
                Err(e) => Err(e),
            }
        }
        .await)?;
        let mut items = listing::items(&text, mlsd);
        if mlsd && listing::may_hide_links(&items) {
            match with!(self, dir, |c| self.listing(&mut c.s, "LIST", dir).await) {
                Ok(list) => listing::mark_links(&mut items, &list),
                Err(e) => log!("ftp: LIST of {dir:?} for the links failed: {e:?}"),
            }
        }
        Ok(items)
    }

    /// Is this entry a folder? A symlink is followed.
    async fn entry_is_dir(&self, path: &str, f: &Item) -> bool {
        if f.link { self.is_dir(path).await } else { f.dir }
    }

    /// The size of a file listed: of a link to a file the size of the file, asked (SIZE), not
    /// the length of the path the link holds, which the listing gives (a copy would be cut short).
    async fn file_size(&self, path: &str, f: &Item) -> u64 {
        if !f.link {
            return f.size;
        }
        self.size(path).await.unwrap_or_else(|e| {
            log!("ftp: size of the link {path:?} not known: {e:?}");
            f.size
        })
    }

    pub async fn list(&self, dir: &str) -> Result<Listing, Error> {
        let mut listing = Listing::default();
        for f in self.entries(dir).await? {
            let at = format!("{dir}{}", f.name);
            let is_dir = self.entry_is_dir(&at, &f).await;
            let size = if is_dir { f.size } else { self.file_size(&at, &f).await };
            let path = format!("{at}{}", if is_dir { "/" } else { "" });
            let modified = f.modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default();
            listing.add(path, &f.name, is_dir, size, modified, true);
        }
        listing.sort();
        Ok(listing)
    }

    pub async fn names(&self, dir: &str) -> Result<HashSet<String>, Error> {
        Ok(self.entries(dir).await?.into_iter().map(|f| f.name).collect())
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
        with!(self, path, transfer |c| async {
            let mut d = waited(self.data(&mut c.s, format!("RETR {}", abs(path)), 0)).await?;
            let data = read_up_to(&mut d, u64::MAX).await?;
            waited(close_data(&mut c.s, d, false)).await?;
            Ok::<_, FtpError>(data)
        }
        .await)
    }

    /// `len` bytes from `offset`, fewer at the end of the file. A transfer stopped before the end
    /// is aborted; a connection whose abort fails is not used again.
    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        with!(self, path, transfer |c| async {
            let mut d = waited(self.data(&mut c.s, format!("RETR {}", abs(path)), offset)).await?;
            let data = read_up_to(&mut d, len).await?;
            if (data.len() as u64) < len {
                waited(close_data(&mut c.s, d, false)).await?;
            } else if let Err(e) = waited(async { c.s.abort(d).await?; resync(&mut c.s).await }).await {
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
            with!(self, path, transfer |c| async {
                let mut d = waited(self.data(&mut c.s, format!("STOR {}", abs(path)), 0)).await?;
                once(&began)?;
                while let Some(chunk) = chunks.recv().await {
                    let chunk = chunk.map_err(FtpError::ConnectionError)?;
                    waited(async { d.write_all(&chunk).await.map_err(FtpError::ConnectionError) }).await?;
                }
                waited(close_data(&mut c.s, d, true)).await
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

    /// `from` renamed `to`, replacing the file there, with one RNFR/RNTO (issue #92): many
    /// servers (vsftpd, ProFTPD, a Storage Box) replace it, others refuse and leave both files as
    /// they were. `None` where it is refused (see `over`): the caller moves the file aside first.
    pub async fn rename_over(&self, from: &str, to: &str) -> Option<Result<(), Error>> {
        over(&self.over, from, || async {
            with!(self, from, |c| match c.s.rename(abs(from), abs(to)).await {
                // An answer of the server: refused. 421 is a connection closing, as for `broken`.
                Err(FtpError::UnexpectedResponse(r)) if r.status != Status::NotAvailable => Ok(Some(r.status)),
                Err(e) => Err(e),
                Ok(()) => Ok(None),
            })
        })
        .await
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
        self.delete_but(path, &[]).await.map(|_| ())
    }

    /// Deletes a folder with everything inside it but the items `kept` (paths as `walk` gives
    /// them: the links to folders a move left out, #183) and the folders that hold one; whether
    /// the folder went.
    pub async fn delete_but(&self, path: &str, kept: &[String]) -> Result<bool, Error> {
        let mut emptied = true;
        for f in self.entries(path).await? {
            let child = format!("{path}{}", f.name);
            if kept.contains(&child) {
                emptied = false;
            } else if f.dir {
                // A symlink is removed, never followed.
                emptied &= Box::pin(self.delete_but(&format!("{child}/"), kept)).await?;
            } else {
                with!(self, &child, |c| c.s.rm(abs(&child)).await)?;
            }
        }
        if emptied {
            self.remove_dir(path).await?;
        }
        Ok(emptied)
    }

    /// Removes an empty folder; the server refuses one that is not.
    pub async fn remove_dir(&self, path: &str) -> Result<(), Error> {
        with!(self, path, |c| c.s.rmdir(abs(path)).await)
    }

    /// Everything below a folder, hidden items and EyeCanSee's trash folders too
    /// (`transfer::place`), folders before what they contain: path and size (folders end in "/",
    /// size 0).
    /// A link to a folder is left out, not gone down (nor do `cp -r` and rsync): FTP cannot tell
    /// where it leads, and one to the folder itself or to a folder above brought the same files
    /// again and again (#183).
    pub async fn walk(&self, dir: &str, p: &crate::transfer::Progress) -> Result<Vec<(String, u64)>, Error> {
        let mut found = Vec::new();
        let mut todo = vec![dir.to_string()];
        while let Some(d) = todo.pop() {
            if p.cancel.load(Ordering::Relaxed) {
                return Err(("error-cancelled", String::new()));
            }
            crate::source::too_deep(dir, &d)?;
            *p.current.lock().unwrap() = d.clone();
            for f in self.entries(&d).await? {
                let path = format!("{d}{}", f.name);
                if self.entry_is_dir(&path, &f).await {
                    if f.link {
                        log!("ftp: {path:?} left out of the walk: a link to a folder");
                        p.links_skipped.lock().unwrap().push(path.clone());
                        continue;
                    }
                    found.push((format!("{path}/"), 0));
                    todo.push(format!("{path}/"));
                } else {
                    let size = self.file_size(&path, &f).await;
                    found.push((path, size));
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
        // Errors of `push` (the destination) travel apart from the server's.
        let pushed: Mutex<Option<Error>> = Mutex::new(None);
        let began = AtomicBool::new(false);
        let r: Result<(), Error> = (async {
            with!(self, path, transfer |c| async {
                let mut r = waited(self.data(&mut c.s, format!("RETR {}", abs(path)), from)).await?;
                once(&began)?;
                let mut at = from;
                while at < size {
                    let stop = if cancel.load(Ordering::Relaxed) { Some(("error-cancelled", String::new())) } else { None };
                    let mut data = vec![0; chunk.min(size - at) as usize];
                    let read = match stop {
                        None => waited(async { r.read_exact(&mut data).await.map_err(FtpError::ConnectionError) }).await.map(|_| ()),
                        Some(_) => Ok(()),
                    };
                    let failed = match (stop, read) {
                        (Some(e), _) => Some(e),
                        (None, Err(FtpError::ConnectionError(e))) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            let (e, ok) = cut_short(&mut c.s, r, path).await;
                            *pushed.lock().unwrap() = Some(e);
                            c.ok &= ok;
                            return Ok(());
                        }
                        (None, Err(e)) => Some(("error-detail", format!("{path}: {e}"))),
                        (None, Ok(())) => {
                            at += data.len() as u64;
                            push(Bytes::from(data)).await.err()
                        }
                    };
                    if let Some(e) = failed {
                        *pushed.lock().unwrap() = Some(e);
                        if waited(async { c.s.abort(r).await?; resync(&mut c.s).await }).await.is_err() {
                            c.ok = false;
                        }
                        return Ok(());
                    }
                }
                waited(close_data(&mut c.s, r, false)).await
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
    mod fake;

    #[test]
    fn listed_items() {
        let unix = super::listing::parse("-rw-r--r--    1 me       users     1234 Oct 30 09:10 a b.jpg", false).unwrap();
        assert_eq!((unix.name.as_str(), unix.size, unix.dir), ("a b.jpg", 1234, false));
        let dir = super::listing::parse("drwxr-xr-x    2 me       users     4096 Jan  1  2025 Foto", false).unwrap();
        assert!(dir.dir);
        let mlsd = super::listing::parse("type=file;size=42;modify=20261030091000; x.mp4", true).unwrap();
        assert_eq!((mlsd.name.as_str(), mlsd.size), ("x.mp4", 42));
        assert_eq!(super::abs("Foto/2026/"), "/Foto/2026");
        assert_eq!(super::abs(""), "/");
        assert_eq!(super::pasv_port("Entering Passive Mode (127,0,0,1,217,125)."), Some(55677));
        assert_eq!(super::pasv_port("Entering Passive Mode"), None);
        assert_eq!(super::epsv_port("Entering Extended Passive Mode (|||55677|)"), Some(55677));
        assert_eq!(super::epsv_port("Entering Extended Passive Mode"), None);
    }

    /// Audit 2.19: the six numbers of a PASV answer are bytes; one beyond 255 is a bad answer,
    /// not a port worked out by overflowing (a panic in a debug build).
    #[test]
    fn audit_2_19_pasv_numbers_over_255() {
        for bad in ["(1,2,3,4,300,0)", "(1,2,3,4,0,256)", "(1,2,3,4,255,65535)", "(999,2,3,4,1,1)"] {
            assert_eq!(std::panic::catch_unwind(|| super::pasv_port(bad)).ok(), Some(None), "{bad}");
        }
        assert_eq!(super::pasv_port("227 Entering Passive Mode (10,0,0,2,255,255)"), Some(65535));
        assert_eq!(super::pasv_port("(10,0,0,2,0,21)"), Some(21));
    }

    /// Renaming over a file (issue #92): done where the server replaces; where it refuses, `None`
    /// (the caller moves the original aside) and never tried again on that server; an error of
    /// the connection is an error. A server that replaced once stays so after a refusal.
    #[test]
    fn rename_over_learnt() {
        use super::*;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tries = std::cell::Cell::new(0);
        let attempt = |answer: Result<Option<Status>, Error>| {
            let tries = &tries;
            move || async move {
                tries.set(tries.get() + 1);
                answer
            }
        };
        // Replaces.
        let memo = AtomicU8::new(UNKNOWN);
        assert_eq!(rt.block_on(over(&memo, "a", attempt(Ok(None)))), Some(Ok(())));
        assert_eq!(memo.load(Ordering::Relaxed), REPLACES);
        assert_eq!(rt.block_on(over(&memo, "a", attempt(Ok(Some(Status::FileUnavailable))))), None);
        assert_eq!(memo.load(Ordering::Relaxed), REPLACES);
        // Refuses: remembered, not tried again.
        let memo = AtomicU8::new(UNKNOWN);
        assert_eq!(rt.block_on(over(&memo, "a", attempt(Ok(Some(Status::BadFilename))))), None);
        assert_eq!(memo.load(Ordering::Relaxed), REFUSES);
        tries.set(0);
        assert_eq!(rt.block_on(over(&memo, "a", attempt(Ok(None)))), None);
        assert_eq!(tries.get(), 0);
        // The connection lost: an error, nothing learnt.
        let memo = AtomicU8::new(UNKNOWN);
        let lost: Error = ("error-detail", "a: connection lost".to_string());
        assert_eq!(rt.block_on(over(&memo, "a", attempt(Err(lost.clone())))), Some(Err(lost)));
        assert_eq!(memo.load(Ordering::Relaxed), UNKNOWN);
    }

    /// Audit 2.16 (FTP): a folder with a link to itself (`loop/again` → `loop/`) must be walked
    /// once. `walk` took a link the server lets it enter (CWD) for a folder and had no memory of
    /// the folders seen, so it went down `again/again/…` until the server refused the path (Linux:
    /// more than 40 links), and a copy of the folder held the same file once per level.
    /// Server: `tests/fake.rs`.
    #[test]
    fn audit_2_16_walk_link_to_itself() {
        let (target, seen) = fake::start();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let server = super::Server::shared(target);
            let t = std::time::Instant::now();
            let walked = server.walk("loop/", &crate::transfer::Progress::default()).await.unwrap();
            let copies = walked.iter().filter(|(p, _)| p.ends_with("/a.jpg")).count();
            let longest = walked.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
            let listed = seen.listed.lock().unwrap().len();
            println!("walk of loop/ in {:.2?}: {} items, a.jpg {copies} times, {listed} folders listed, longest path {longest} characters", t.elapsed(), walked.len());
            assert_eq!(copies, 1, "the only file of the folder found {copies} times");
        });
    }

    /// A link to a folder left out of the walk is counted, for the end of the copy (#183).
    /// Server: `tests/fake.rs` (`loop/again` → `loop/`).
    #[test]
    fn walk_counts_links_left_out() {
        let (target, _) = fake::start();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let p = crate::transfer::Progress::default();
            let walked = super::Server::shared(target).walk("loop/", &p).await.unwrap();
            assert_eq!(walked, vec![("loop/a.jpg".to_string(), 10)]);
            assert_eq!(*p.links_skipped.lock().unwrap(), ["loop/again"]);
        });
    }

    /// A move of a folder with links to folders left out (#183): only what was copied goes from
    /// the server; the links stay where they were, with the folders that hold them, and the
    /// folder is not gone. Server: `tests/fake.rs` (`move/keep/back` → `move/`, `move/out` →
    /// `loop/`: on FTP both left out).
    #[test]
    fn move_keeps_links_left_out() {
        use crate::source::Store;
        use crate::transfer::{Clip, Progress, run};
        let (target, _) = fake::start();
        let dir = std::env::temp_dir().join(format!("eyecansee-ftp-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dest")).unwrap();
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let server = Store::Ftp(super::Server::shared(target));
        let clip = Clip { op: server.clone(), base: "ftp://test-move/".into(), paths: vec!["move/".into()], cut: true, text: String::new() };
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let p = Progress::default();
            let (gone, r) = run(&clip, &local, "test-local://", "dest/", &std::collections::HashMap::new(), &p).await;
            r.clone().unwrap();
            assert!(gone.is_empty(), "the folder that holds the links is not gone: {gone:?}");
            let mut left = p.links_skipped.lock().unwrap().clone();
            left.sort();
            assert_eq!(left, ["move/keep/back", "move/out"]);
            assert_eq!(crate::transfer::links_to_say(&r, &p).map(|l| l.len()), Some(2));
            assert_eq!(std::fs::read(dir.join("dest/move/a.jpg")).unwrap(), b"not a jpeg");
            assert_eq!(std::fs::read(dir.join("dest/move/sub/b.jpg")).unwrap(), b"not a jpeg");
            assert!(dir.join("dest/move/keep").is_dir() && !dir.join("dest/move/out").exists());
            let names = |d: &'static str| async { server.names(d).await.unwrap().into_iter().collect::<std::collections::BTreeSet<_>>() };
            assert_eq!(names("move/").await, ["keep", "out"].map(String::from).into());
            assert_eq!(names("move/keep/").await, ["back"].map(String::from).into());
            assert_eq!(names("loop/").await, ["a.jpg", "again"].map(String::from).into(), "where a link leads is not touched");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// L17 of 2026-11-21 (#183): a copy of a folder of a Storage Box (SFTPGo) with `self`, a link
    /// to the folder itself, brought only `due.jpg`. MLSD told the link as a file of 1 byte, LIST
    /// with Go's "L", which was not read; the copy read it, got no bytes ("Connection error: early
    /// eof") and stopped there, before `uno.jpg`. Now the link is left out and said, the photos are
    /// copied, and `zeta.jpg`, a link to `uno.jpg`, comes whole, not cut to the 7 bytes of the link.
    /// Server: `tests/fake.rs` (`/links/`).
    #[test]
    fn copy_of_a_folder_with_a_link_listed_by_sftpgo() {
        use crate::source::Store;
        use crate::transfer::{Clip, Progress, run};
        let (target, _) = fake::start();
        let dir = std::env::temp_dir().join(format!("eyecansee-ftp-sftpgo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let server = Store::Ftp(super::Server::shared(target));
        let clip = Clip { op: server, base: "ftp://test-sftpgo/".into(), paths: vec!["links/".into()], cut: false, text: String::new() };
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let p = Progress::default();
            let (_, r) = run(&clip, &local, "test-local://", "", &std::collections::HashMap::new(), &p).await;
            let copied: std::collections::BTreeSet<String> = std::fs::read_dir(dir.join("links")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            println!("copy of links/: {r:?}; copied {copied:?}; links left out {:?}", p.links_skipped.lock().unwrap());
            assert_eq!(r, Ok(()));
            assert_eq!(copied, ["due.jpg", "uno.jpg", "zeta.jpg"].map(String::from).into());
            assert_eq!(*p.links_skipped.lock().unwrap(), ["links/self"]);
            assert_eq!(crate::transfer::links_to_say(&r, &p).map(|l| l.len()), Some(1));
            assert_eq!(std::fs::read(dir.join("links/zeta.jpg")).unwrap(), std::fs::read(dir.join("links/uno.jpg")).unwrap());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file the server stops sending before its end, in a folder copied (L17 of 2026-11-21):
    /// the copy stopped there, and the files after it were never copied. Now it is left out, the
    /// others are copied, the message at the end names it with the server's answer (not "early
    /// eof"), and nothing of it is left at the destination. Moved, the files copied go from the
    /// server, the one not read stays, with its folder. Server: `tests/fake.rs` (`/cut/`).
    #[test]
    fn a_file_not_read_is_left_out_and_said() {
        use crate::source::Store;
        use crate::transfer::{Clip, Progress, run};
        let dir = std::env::temp_dir().join(format!("eyecansee-ftp-cut-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for cut in [false, true] {
                let to = if cut { "moved/" } else { "copied/" };
                std::fs::create_dir_all(dir.join(to)).unwrap();
                let (target, _) = fake::start();
                let server = Store::Ftp(super::Server::shared(target));
                let clip = Clip { op: server.clone(), base: "ftp://test-cut/".into(), paths: vec!["cut/".into()], cut, text: String::new() };
                let p = Progress::default();
                let (gone, r) = run(&clip, &local, "test-local://", to, &std::collections::HashMap::new(), &p).await;
                let copied: std::collections::BTreeSet<String> = std::fs::read_dir(dir.join(to).join("cut")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                let failed = p.failed.lock().unwrap().clone();
                println!("cut: {cut}: {r:?}; copied {copied:?}; not read {failed:?}");
                assert_eq!(r, Ok(()), "cut: {cut}");
                assert_eq!(copied, ["a.jpg", "c.jpg"].map(String::from).into(), "cut: {cut}: nothing of b.jpg is left");
                assert_eq!(failed.len(), 1);
                assert_eq!(failed[0].0, "cut/b.jpg");
                let said = crate::i18n::I18n::new("en").error(failed[0].1.0, &failed[0].1.1);
                assert!(said.starts_with("The server stopped sending \"b.jpg\" before its end.\n(") && !said.contains("eof"), "{said}");
                let left = server.names("cut/").await.unwrap().into_iter().collect::<std::collections::BTreeSet<_>>();
                if cut {
                    assert!(gone.is_empty(), "the folder stays: {gone:?}");
                    assert_eq!(left, ["b.jpg"].map(String::from).into());
                    assert!(p.left.lock().unwrap().is_empty(), "b.jpg is said once: {:?}", p.left.lock().unwrap());
                } else {
                    assert_eq!(left.len(), 3);
                }
            }
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The copy of L17 (2026-11-21) from a real server, into a folder of this computer that goes
    /// after: `<dir>collegamenti/` (`due.jpg`, `uno.jpg`, `self` a link to the folder) and
    /// `<dir>link-file/` (`foto.jpg`, a link to `../collegamenti/uno.jpg`, made for the test with
    /// `symlink` of sftp and removed after: on 2026-11-21 it was). Nothing is written on the
    /// server. Plain FTP, the address from the environment:
    /// `EYECANSEE_FTP_HOST=… EYECANSEE_FTP_USER=… EYECANSEE_FTP_PASSWORD=… EYECANSEE_FTP_DIR=prove-078/
    /// cargo test ftp_links_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ftp_links_live() {
        use crate::source::Store;
        use crate::transfer::{Clip, Progress, run};
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
        let target = super::Target { host: var("EYECANSEE_FTP_HOST"), port: 21, user: var("EYECANSEE_FTP_USER"), password: var("EYECANSEE_FTP_PASSWORD"), secure: false };
        let top = var("EYECANSEE_FTP_DIR");
        let dir = std::env::temp_dir().join(format!("eyecansee-ftp-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let server = Store::Ftp(super::Server::shared(target));
        let paths = vec![format!("{top}collegamenti/"), format!("{top}link-file/")];
        let clip = Clip { op: server, base: "ftp://live/".into(), paths, cut: false, text: String::new() };
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let p = Progress::default();
            let t = std::time::Instant::now();
            let (_, r) = run(&clip, &local, "test-local://", "", &std::collections::HashMap::new(), &p).await;
            let size = |f: &str| std::fs::metadata(dir.join(f)).map(|m| m.len()).ok();
            let names = |d: &str| std::fs::read_dir(dir.join(d)).map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect::<std::collections::BTreeSet<_>>()).unwrap_or_default();
            println!("copy in {:.2?}: {r:?}", t.elapsed());
            println!("collegamenti: {:?}; link-file: {:?}", names("collegamenti"), names("link-file"));
            println!("links left out {:?}; not read {:?}", p.links_skipped.lock().unwrap(), p.failed.lock().unwrap());
            println!("uno.jpg {:?} bytes, foto.jpg {:?} bytes", size("collegamenti/uno.jpg"), size("link-file/foto.jpg"));
            assert_eq!(r, Ok(()));
            assert_eq!(names("collegamenti"), ["due.jpg", "uno.jpg"].map(String::from).into());
            assert_eq!(*p.links_skipped.lock().unwrap(), [format!("{top}collegamenti/self")]);
            assert!(p.failed.lock().unwrap().is_empty());
            assert_eq!(std::fs::read(dir.join("link-file/foto.jpg")).unwrap(), std::fs::read(dir.join("collegamenti/uno.jpg")).unwrap());
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Audit 2.3: a name a server lists with a path in it (`album/../victim.txt`), in a folder
    /// pasted over one of the same name (Replace: the two merge), was written outside the folder,
    /// over a file of the computer, and the copy said it worked. Names sent by a server are now
    /// checked before they become local paths: the item is left out, the rest copied, the copy
    /// ends with a message naming it, and a move keeps the folder on the server. Server: `tests/fake.rs`.
    #[test]
    fn audit_2_3_a_name_from_a_server_writes_outside_the_folder() {
        use crate::source::Store;
        use crate::transfer::{Choice, Clip, Progress, run};
        let (target, _) = fake::start();
        let dir = std::env::temp_dir().join(format!("eyecansee-ftp-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Pictures/album")).unwrap();
        std::fs::write(dir.join("Pictures/album/a.jpg"), b"the computer's a.jpg").unwrap();
        std::fs::write(dir.join("Pictures/victim.txt"), b"the computer's file").unwrap();
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let server = Store::Ftp(super::Server::shared(target));
        let choices = std::collections::HashMap::from([("album/".to_string(), Choice::Replace)]);
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for cut in [false, true] {
                let clip = Clip { op: server.clone(), base: "ftp://test-names/".into(), paths: vec!["album/".into()], cut, text: String::new() };
                let p = Progress::default();
                let (gone, r) = run(&clip, &local, "test-local://", "Pictures/", &choices, &p).await;
                assert_eq!(std::fs::read(dir.join("Pictures/victim.txt")).unwrap(), b"the computer's file", "a file outside the folder pasted was written over");
                assert_eq!((r, p.refused.lock().unwrap().clone()), (Ok(()), vec!["album/../victim.txt".to_string()]), "cut: {cut}");
                assert_eq!(std::fs::read(dir.join("Pictures/album/a.jpg")).unwrap(), b"not a jpeg", "the rest is copied");
                assert!(gone.is_empty(), "a folder with an item left out stays: {gone:?}");
                let names = server.names("album/").await.unwrap().into_iter().collect::<std::collections::BTreeSet<_>>();
                assert_eq!(names, ["../victim.txt", "a.jpg"].map(String::from).into());
            }
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Audit 2.4: a name a server takes and Windows does not (`Mare 12:30.jpg`), moved to a folder
    /// of Windows. On NTFS the colon opened the hidden stream `30.jpg` of an empty file `Mare 12`:
    /// the move said it worked, the folder showed a file of 0 bytes and the server's file was
    /// deleted. On Windows such a name is now left out, alone or in a folder moved, the rest is
    /// moved and the copy ends with a message naming it; elsewhere the name is copied as it is. Server:
    /// `tests/fake.rs`.
    #[test]
    fn audit_2_4_windows_colon_in_a_name_from_a_server() {
        use crate::source::Store;
        use crate::transfer::{Clip, Progress, run};
        let dir = std::env::temp_dir().join(format!("eyecansee-ftp-colon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let local = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let shown = |d: &str| std::fs::read_dir(dir.join(d)).map(|e| e.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().unwrap().len())).collect::<std::collections::BTreeSet<_>>()).unwrap_or_default();
        let none = std::collections::HashMap::new();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            // The file alone, then its folder: a server each, as found at the start.
            for (path, to) in [("beach/Mare 12:30.jpg", "alone/"), ("beach/", "folder/")] {
                std::fs::create_dir_all(dir.join(to)).unwrap();
                let (target, _) = fake::start();
                let server = Store::Ftp(super::Server::shared(target));
                let clip = Clip { op: server.clone(), base: "ftp://test-colon/".into(), paths: vec![path.into()], cut: true, text: String::new() };
                let p = Progress::default();
                let (gone, r) = run(&clip, &local, "test-local://", to, &none, &p).await;
                let on_server = server.names("beach/").await.unwrap_or_default();
                let copied = shown(&format!("{to}{}", if path.ends_with('/') { "beach/" } else { "" }));
                let photo = || ("a.jpg".to_string(), 10);
                if cfg!(windows) {
                    assert_eq!((r, p.refused.lock().unwrap().clone()), (Ok(()), vec!["beach/Mare 12:30.jpg".to_string()]), "{path}");
                    assert!(gone.is_empty(), "{path}: {gone:?}");
                    assert!(on_server.contains("Mare 12:30.jpg") && on_server.contains("a.jpg"), "{path}: the server holds {on_server:?}");
                    let expected: std::collections::BTreeSet<_> = if path.ends_with('/') { [photo()].into() } else { Default::default() };
                    assert_eq!(copied, expected, "{path}: no empty \"Mare 12\"");
                } else {
                    assert_eq!(r, Ok(()), "{path}");
                    assert_eq!(gone, [path], "{path}");
                    assert!(!on_server.contains("Mare 12:30.jpg"), "{path}: moved");
                    assert!(copied.contains(&("Mare 12:30.jpg".to_string(), 10)), "{path}: copied as it is: {copied:?}");
                }
            }
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
