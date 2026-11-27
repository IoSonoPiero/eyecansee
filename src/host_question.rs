//! "Do you trust this server?": the SSH key of an SFTP server never used from this computer, or
//! the certificate of an FTPS server no authority vouches for. The connection refused leaves it
//! in `sftp` or `ftp`; the question keeps a copy, shows it and, trusted, saves exactly that
//! one (`App::host_window`).

/// What the window asks about.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Question {
    Cert(crate::ftp::Unknown),
    Key(crate::sftp::Unknown),
}

impl Question {
    /// The question for the error `id` of a listing: the certificate or the key its connection
    /// left. Before, the window looked at the certificate first, whatever the error: one
    /// refused before was asked about again with an SFTP server (audit 2.10).
    pub(crate) fn of(id: &str) -> Option<Self> {
        match id {
            "error-cert-unknown" => crate::ftp::pending().map(Question::Cert),
            "error-host-unknown" => crate::sftp::pending().map(Question::Key),
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

    /// The window's title, of the Fluent id `id`.
    pub(crate) fn title(&self, i18n: &crate::i18n::I18n, id: &str) -> String {
        i18n.t(id)
    }

    /// The window's question, of the Fluent id `id` with `args`.
    pub(crate) fn ask(&self, i18n: &crate::i18n::I18n, id: &str, args: &[(&str, &str)]) -> String {
        i18n.t_args(id, args)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    /// Audit S.7 (27/11): an FTPS server whose certificate is not the one confirmed before was
    /// asked about as at the first connection ("New FTPS server", "First connection to…"):
    /// nothing said that something had changed. Its title and its question say so, with the
    /// fingerprint confirmed before and the new one. In both languages.
    #[test]
    fn a_changed_certificate_is_not_a_first_connection() {
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            let args = [("host", "nas"), ("fingerprint", "CC:03")];
            let first = Question::Cert(crate::ftp::changed("nas", "CC:03", None));
            assert_eq!((first.title(&i18n, "cert-title"), first.ask(&i18n, "cert-ask", &args)), (i18n.t("cert-title"), i18n.t_args("cert-ask", &args)), "{lang}: the first time");
            let changed = Question::Cert(crate::ftp::changed("nas", "CC:03", Some("AA:01")));
            let (title, ask) = (changed.title(&i18n, "cert-title"), changed.ask(&i18n, "cert-ask", &args));
            assert_ne!(title, i18n.t("cert-title"), "{lang}: a changed certificate titled as a new server");
            assert!(ask.contains("AA:01") && ask.contains("CC:03") && ask != i18n.t_args("cert-ask", &args), "{lang}: {ask}");
        }
    }
}
