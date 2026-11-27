//! FTPS: the TLS of the control and data connections. The server's certificate is checked against
//! the usual certificate authorities (webpki roots) and the server's name; one that fails (a NAS
//! with its own certificate, an address instead of a name) is accepted only if the user trusted
//! its fingerprint before, as ssh does with known_hosts. Trusted fingerprints: `known_certs` in the
//! configuration folder, one "host:port fingerprint" per line.

use std::sync::{Arc, Mutex};

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

/// A certificate no authority vouches for, to ask the user about.
#[derive(Clone, PartialEq, Debug)]
pub struct Unknown {
    /// The server, as the question names it.
    pub label: String,
    /// "host:port", as `known_certs` writes it.
    key: String,
    /// SHA-256, as browsers show it.
    pub fingerprint: String,
    /// The fingerprint confirmed before for this server, when it is another one.
    pub old: Option<String>,
}

/// A certificate of `label` waiting, `old` the one confirmed before: for the tests of the question.
#[cfg(test)]
pub fn changed(label: &str, fingerprint: &str, old: Option<&str>) -> Unknown {
    Unknown { label: label.to_string(), key: format!("{label}:990"), fingerprint: fingerprint.to_string(), old: old.map(str::to_string) }
}

/// The certificate of the last connection refused for it, waiting for the app to ask (`take`).
static UNKNOWN: Mutex<Option<Unknown>> = Mutex::new(None);

/// The server whose certificate is waiting to be asked about: its name and the fingerprint.
pub fn unknown() -> Option<(String, String)> {
    UNKNOWN.lock().unwrap().as_ref().map(|u| (u.label.clone(), u.fingerprint.clone()))
}

/// The certificate waiting, to be asked about: the question keeps it, and what it shows is what
/// `trust` saves, whatever another connection leaves meanwhile (audit 2.10). It stays waiting
/// until the answer (`forget`): a connection refused meanwhile still says why (`waiting`).
pub fn pending() -> Option<Unknown> {
    UNKNOWN.lock().unwrap().clone()
}

/// After an answer: what connections left meanwhile is not asked about with the next server.
pub fn forget() {
    UNKNOWN.lock().unwrap().take();
}

/// Is a certificate of `key` ("host:port") waiting for the user?
pub(super) fn waiting(key: &str) -> bool {
    UNKNOWN.lock().unwrap().as_ref().is_some_and(|u| u.key == key)
}

/// The user trusts the certificate asked about: its fingerprint is added to `known_certs`.
pub fn trust(u: &Unknown) {
    let Some(path) = crate::settings::config_path("known_certs") else { return };
    let text = with_trusted(&std::fs::read_to_string(&path).unwrap_or_default(), &u.key, &u.fingerprint);
    let r = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&path, text));
    log!("ftps: certificate of {} trusted: {r:?}", u.label);
}

/// `known_certs` as `text`, with the fingerprint `fp` of `key` trusted.
fn with_trusted(text: &str, key: &str, fp: &str) -> String {
    format!("{text}{key} {fp}\n")
}

fn known(key: &str, fp: &str) -> bool {
    let Some(path) = crate::settings::config_path("known_certs") else { return false };
    std::fs::read_to_string(path).unwrap_or_default().lines().any(|l| l.split_once(' ') == Some((key, fp)))
}

/// SHA-256 of the certificate, as browsers show it: "AB:CD:…".
fn fingerprint(cert: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, cert);
    digest.as_ref().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

#[derive(Debug)]
struct Verifier {
    usual: Arc<WebPkiServerVerifier>,
    provider: Arc<CryptoProvider>,
    label: String,
    key: String,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let e = match self.usual.verify_server_cert(cert, intermediates, name, ocsp, now) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        let fp = fingerprint(cert);
        if known(&self.key, &fp) {
            return Ok(ServerCertVerified::assertion());
        }
        log!("ftps: certificate of {} not trusted ({e}): {fp}", self.label);
        *UNKNOWN.lock().unwrap() = Some(Unknown { label: self.label.clone(), key: self.key.clone(), fingerprint: fp, old: None });
        Err(e)
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// The TLS of one server, shared by its control and data connections (so the data connections
/// resume the control connection's session, which some servers require).
pub(super) fn connector(label: &str, key: &str) -> tokio_rustls::TlsConnector {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let roots = Arc::new(rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() });
    let usual = WebPkiServerVerifier::builder_with_provider(roots, provider.clone()).build().expect("webpki roots");
    let verifier = Arc::new(Verifier { usual, provider: provider.clone(), label: label.to_string(), key: key.to_string() });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    tokio_rustls::TlsConnector::from(Arc::new(config))
}

#[cfg(test)]
mod tests {
    /// Audit S.7 (27/11): the certificate of a server confirmed again after it changed was added
    /// beside the old one, which stayed trusted: whoever presented the old one was still let in.
    /// The new line takes the place of the old one, for that server only.
    #[test]
    fn a_certificate_trusted_again_replaces_the_old_one() {
        let now = super::with_trusted("nas:990 AA:01\nother:21 BB:02\n", "nas:990", "CC:03");
        assert_eq!(now.lines().collect::<Vec<_>>(), ["other:21 BB:02", "nas:990 CC:03"]);
    }

    #[test]
    fn fingerprint_as_browsers() {
        let fp = super::fingerprint(b"abc");
        assert!(fp.starts_with("BA:78:16:BF:"), "{fp}");
        assert_eq!(fp.len(), 32 * 3 - 1);
    }
}
