//! "Do you trust this server?": the SSH key of an SFTP server never used from this computer, or
//! the certificate of an FTPS server no authority vouches for. The connection refused leaves it
//! in `sftp` or `ftp`; the question takes it from there, shows it and, trusted, saves exactly that
//! one (`App::host_window`).

/// What the window asks about.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Question {
    Cert(crate::ftp::Unknown),
    Key(crate::sftp::Unknown),
}

impl Question {
    /// The question for the error `id` of a listing: the certificate or the key its connection
    /// left, taken. Before, the window looked at the certificate first, whatever the error: one
    /// refused before was asked about again with an SFTP server (audit 2.10).
    pub(crate) fn take(id: &str) -> Option<Self> {
        match id {
            "error-cert-unknown" => crate::ftp::take().map(Question::Cert),
            "error-host-unknown" => crate::sftp::take().map(Question::Key),
            _ => None,
        }
    }

    /// An FTPS certificate (otherwise an SSH key).
    pub(crate) fn cert(&self) -> bool {
        matches!(self, Question::Cert(_))
    }

    /// The server and the fingerprint shown.
    pub(crate) fn shown(&self) -> (String, String) {
        match self {
            Question::Cert(u) => (u.label.clone(), u.fingerprint.clone()),
            Question::Key(u) => (u.label.clone(), u.fingerprint()),
        }
    }

    /// Trusted: the certificate or the key shown, and only that one.
    pub(crate) fn trust(&self) {
        match self {
            Question::Cert(u) => crate::ftp::trust(u),
            Question::Key(u) => crate::sftp::trust(u),
        }
    }
}

/// After any answer: what other connections left meanwhile waits no more, so it is never asked
/// about with the next server (audit 2.10). A connection still refused asks again.
pub(crate) fn forget_all() {
    crate::ftp::forget();
    crate::sftp::forget();
}
