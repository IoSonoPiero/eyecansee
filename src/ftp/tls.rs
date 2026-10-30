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

/// A certificate not trusted yet, waiting for the user's answer (`trust`): label, "host:port"
/// and fingerprint.
static UNKNOWN: Mutex<Option<(String, String, String)>> = Mutex::new(None);

/// The server whose certificate the app has to ask about: its name and the fingerprint.
pub fn unknown() -> Option<(String, String)> {
    UNKNOWN.lock().unwrap().as_ref().map(|(label, _, fp)| (label.clone(), fp.clone()))
}

/// Is a certificate of `key` ("host:port") waiting for the user?
pub(super) fn waiting(key: &str) -> bool {
    UNKNOWN.lock().unwrap().as_ref().is_some_and(|(_, k, _)| k == key)
}

/// The user trusts the certificate asked about: its fingerprint is added to `known_certs`.
pub fn trust() {
    let Some((label, key, fp)) = UNKNOWN.lock().unwrap().take() else { return };
    let Some(path) = crate::settings::config_path("known_certs") else { return };
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    text.push_str(&format!("{key} {fp}\n"));
    let r = path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&path, text));
    log!("ftps: certificate of {label} trusted: {r:?}");
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
        *UNKNOWN.lock().unwrap() = Some((self.label.clone(), self.key.clone(), fp));
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
    #[test]
    fn fingerprint_as_browsers() {
        let fp = super::fingerprint(b"abc");
        assert!(fp.starts_with("BA:78:16:BF:"), "{fp}");
        assert_eq!(fp.len(), 32 * 3 - 1);
    }
}
