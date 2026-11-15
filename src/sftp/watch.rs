//! A connection that stopped answering, told from a slow one (#75): a request fails with
//! `Timeout` only when nothing at all has come back on its connection for `SILENCE`. A long queue
//! of reads on a slow link goes on as long as answers keep coming; a link whose data stopped
//! arriving (Windows to a NAS through a VPN) is left after `SILENCE`, not after
//! russh-sftp's 60 s per request, and the request is tried again on a new connection.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use russh_sftp::client::error::Error as SftpError;

/// No answer at all on a connection for this long, while a request waits: the connection has
/// stopped. A working connection answers in well under a second, in a few seconds on a slow link
/// (one piece of 32-256 KiB) [Inference]; a sleeping disk of a NAS takes some 10 s to spin up
/// [Inference]. The request then goes again on a new connection, so even a longer spin-up costs
/// one new connection, not an error.
pub(super) const SILENCE: Duration = Duration::from_secs(20);

/// Microseconds since the first call: one clock for every connection.
fn now() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_micros() as u64
}

fn at(us: u64) -> tokio::time::Instant {
    tokio::time::Instant::now() - Duration::from_micros(now().saturating_sub(us))
}

/// When a connection last answered.
#[derive(Default)]
pub(super) struct Watch {
    heard: AtomicU64,
}

impl Watch {
    /// `req`, a request on the watched connection: its answer, or `Timeout` when neither it nor
    /// any other request on the connection got an answer for `silence` since it was sent.
    pub(super) async fn ask<T>(&self, silence: Duration, req: impl Future<Output = Result<T, SftpError>>) -> Result<T, SftpError> {
        let sent = now();
        tokio::pin!(req);
        loop {
            let since = sent.max(self.heard.load(Ordering::Relaxed));
            tokio::select! {
                biased;
                r = &mut req => {
                    if !matches!(r, Err(SftpError::Timeout | SftpError::IO(_))) {
                        self.heard.fetch_max(now(), Ordering::Relaxed);
                    }
                    return r;
                }
                () = tokio::time::sleep_until(at(since) + silence) => {
                    if self.heard.load(Ordering::Relaxed) <= since {
                        return Err(SftpError::Timeout);
                    }
                }
            }
        }
    }
}

/// After a failed request: is its connection gone (closed, an I/O error, no answer in time, or
/// the SFTP channel ended under russh-sftp), so that it is forgotten and the request goes again on
/// a new one? A refusal of the server (no such file, no permission) is not.
pub(super) fn lost(closed: bool, e: &SftpError) -> bool {
    closed
        || match e {
            SftpError::IO(_) | SftpError::Timeout => true,
            // russh-sftp 3.0: a request sent on an ended channel, or waiting when it ended.
            SftpError::UnexpectedBehavior(m) => m == "session closed" || m == "sender dropped",
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use super::{SftpError, Watch, lost};
    use russh_sftp::protocol::{Status, StatusCode};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap()
    }

    fn status(code: StatusCode) -> SftpError {
        SftpError::Status(Status { id: 1, status_code: code, error_message: String::new(), language_tag: String::new() })
    }

    /// Only a connection that is gone is left for a new one; the server's refusals stay errors.
    #[test]
    fn lost_connections() {
        assert!(lost(false, &SftpError::Timeout));
        assert!(lost(false, &SftpError::IO("broken pipe".into())));
        assert!(lost(false, &SftpError::UnexpectedBehavior("session closed".into())));
        assert!(lost(false, &SftpError::UnexpectedBehavior("sender dropped".into())));
        assert!(lost(true, &status(StatusCode::Failure)));
        assert!(!lost(false, &status(StatusCode::NoSuchFile)));
        assert!(!lost(false, &status(StatusCode::PermissionDenied)));
        assert!(!lost(false, &SftpError::UnexpectedPacket));
        assert!(!lost(false, &SftpError::Limited("read limit reached".into())));
    }

    /// An answer comes back as it is; a request with no answer at all ends in `Timeout` after the
    /// silence, not later.
    #[test]
    fn answer_or_timeout() {
        rt().block_on(async {
            let w = Watch::default();
            let silence = Duration::from_millis(300);
            assert_eq!(w.ask(silence, async { Ok::<_, SftpError>(7) }).await.unwrap(), 7);
            let t = Instant::now();
            let r = w.ask(silence, std::future::pending::<Result<(), SftpError>>()).await;
            assert!(matches!(r, Err(SftpError::Timeout)), "{r:?}");
            assert!(t.elapsed() >= silence && t.elapsed() < silence * 3, "{:?}", t.elapsed());
            // A refusal of the server is an answer, returned as it is.
            let r = w.ask(silence, async { Err::<(), _>(status(StatusCode::NoSuchFile)) }).await;
            assert!(matches!(r, Err(SftpError::Status(_))));
        });
    }

    /// A request that waits much longer than the silence lives on while other requests on the
    /// same connection get answers (a slow link with a long queue); when they stop, it ends.
    #[test]
    fn slow_but_answering() {
        rt().block_on(async {
            let w = Arc::new(Watch::default());
            let silence = Duration::from_millis(300);
            let others = {
                let w = w.clone();
                tokio::spawn(async move {
                    for _ in 0..20 {
                        let answer = async {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            Ok::<_, SftpError>(())
                        };
                        let _ = w.ask(silence, answer).await;
                    }
                })
            };
            let slow = async {
                tokio::time::sleep(Duration::from_millis(900)).await;
                Ok::<_, SftpError>("late")
            };
            assert_eq!(w.ask(silence, slow).await.unwrap(), "late");
            others.await.unwrap();
            // The others are done: nothing answers any more.
            let r = w.ask(silence, std::future::pending::<Result<(), SftpError>>()).await;
            assert!(matches!(r, Err(SftpError::Timeout)));
        });
    }
}
