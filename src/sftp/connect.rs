//! Opening an SFTP connection as `ssh` would: the host as `~/.ssh/config` describes it (alias,
//! HostName, User, Port, IdentityFile), the server's key checked against `~/.ssh/known_hosts`,
//! then the login: a saved server only with its own password and key; any other address with the
//! key files without a passphrase or the password. Never the system's SSH agent (user's rule).
//! Not supported: ProxyJump and ProxyCommand.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};

use crate::source::{Error, SILENT};

/// Where to connect, as the address gives it; what is missing comes from `~/.ssh/config`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Target {
    /// "" = the one in `~/.ssh/config`, or the user logged in on this computer.
    pub user: String,
    pub host: String,
    pub port: Option<u16>,
    pub password: Option<String>,
    /// A saved server's key file and its passphrase ("" = none).
    pub key: Option<(PathBuf, String)>,
    /// A saved server: only its own password and key, never the keys in ~/.ssh.
    pub saved: bool,
}

/// Servers logged in to with an RSA key in this session, each warned once, and the one whose
/// warning is still to be shown (`rsa_login`): RUSTSEC-2023-0071 has no fix in the `rsa` crate.
static RSA: Mutex<(Vec<String>, Option<String>)> = Mutex::new((Vec::new(), None));

/// The server just logged in to with an RSA key, to warn about it once.
pub fn rsa_login() -> Option<String> {
    RSA.lock().unwrap().1.take()
}

/// A server key not in known_hosts, to ask the user about.
#[derive(Clone, PartialEq, Debug)]
pub struct Unknown {
    /// The server, as the question names it.
    pub label: String,
    host: String,
    port: u16,
    key: PublicKey,
}

impl Unknown {
    /// The key's fingerprint, as ssh shows it.
    pub fn fingerprint(&self) -> String {
        self.key.fingerprint(HashAlg::Sha256).to_string()
    }
}

/// The key of the last connection refused for it, waiting for the app to ask (`take`).
static UNKNOWN: Mutex<Option<Unknown>> = Mutex::new(None);

/// The server whose key is waiting to be asked about: its name and the key's fingerprint.
pub fn unknown() -> Option<(String, String)> {
    UNKNOWN.lock().unwrap().as_ref().map(|u| (u.label.clone(), u.fingerprint()))
}

/// The key waiting, taken to be asked about: what the question shows is what `trust` saves
/// (audit 2.10).
pub fn take() -> Option<Unknown> {
    UNKNOWN.lock().unwrap().take()
}

/// After an answer: what connections left meanwhile is not asked about with the next server.
pub fn forget() {
    UNKNOWN.lock().unwrap().take();
}

/// The user trusts the key asked about: written to `~/.ssh/known_hosts`, as ssh does.
pub fn trust(u: &Unknown) {
    let r = keys::known_hosts::learn_known_hosts(&u.host, u.port, &u.key);
    log!("sftp: key of {} trusted: {r:?}", u.label);
}

pub(super) struct Client {
    label: String,
    host: String,
    port: u16,
    /// The known_hosts file read; `None` = russh's own (`~/.ssh/known_hosts`).
    known_hosts: Option<PathBuf>,
    /// Why the key was refused, read after `connect` fails.
    refused: Arc<Mutex<Option<Error>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            *self.refused.lock().unwrap() = Some(("error-detail", format!("{}: host certificates are not supported", self.label)));
            return Ok(false);
        };
        let known = match &self.known_hosts {
            Some(file) => keys::check_known_hosts_path(&self.host, self.port, key, file),
            None => keys::check_known_hosts(&self.host, self.port, key),
        };
        let refused = match known {
            Ok(true) => return Ok(true),
            Err(keys::Error::KeyChanged { line }) => ("error-host-changed", format!("{} (known_hosts, line {line})", self.label)),
            // Not there, or no known_hosts yet.
            _ => {
                *UNKNOWN.lock().unwrap() = Some(Unknown { label: self.label.clone(), host: self.host.clone(), port: self.port, key: key.clone() });
                ("error-host-unknown", self.label.clone())
            }
        };
        log!("sftp: key of {} refused: {}", self.label, refused.0);
        *self.refused.lock().unwrap() = Some(refused);
        Ok(false)
    }
}

/// Host, port, user and key files after `~/.ssh/config`.
struct Resolved {
    host: String,
    port: u16,
    user: String,
    files: Vec<PathBuf>,
    key: Option<(PathBuf, String)>,
    saved: bool,
}

/// `ssh`: the folder of `config` and of the keys; `None` = `~/.ssh`.
fn resolve(t: &Target, ssh: Option<&Path>) -> Resolved {
    let cfg = match ssh {
        Some(dir) => russh_config::parse_path(dir.join("config"), &t.host),
        None => russh_config::parse_home(&t.host),
    };
    let cfg = cfg.unwrap_or_else(|_| russh_config::Config::default(&t.host));
    let user = if t.user.is_empty() { cfg.user() } else { t.user.clone() };
    let mut files = cfg.host_config.identity_file.clone().unwrap_or_default();
    if let Some(ssh) = ssh.map(Path::to_path_buf).or_else(|| std::env::home_dir().map(|h| h.join(".ssh"))) {
        files.extend(["id_ed25519", "id_ecdsa", "id_rsa"].map(|f| ssh.join(f)));
    }
    Resolved { host: cfg.host().to_string(), port: t.port.unwrap_or_else(|| cfg.port()), user, files, key: t.key.clone(), saved: t.saved }
}

/// The server as the user wrote it, with the address it stands for when it is an alias of
/// `~/.ssh/config`: "nas (192.0.2.10:2222)".
fn label(t: &Target, r: &Resolved) -> String {
    let addr = if r.port == 22 { r.host.clone() } else { format!("{}:{}", r.host, r.port) };
    if t.host == r.host { addr } else { format!("{} ({addr})", t.host) }
}

/// russh's key exchanges with ECDH before Diffie-Hellman, and the Diffie-Hellman group exchange
/// last: with it the SFTP of a Hetzner Storage Box on port 22 (ProFTPD mod_sftp) failed with "Key
/// exchange init failed", while group14, group16 and ECDH worked (2026-10-30).
const KEX: &[russh::kex::Name] = &[
    russh::kex::MLKEM768X25519_SHA256,
    russh::kex::CURVE25519,
    russh::kex::CURVE25519_PRE_RFC_8731,
    russh::kex::ECDH_SHA2_NISTP256,
    russh::kex::ECDH_SHA2_NISTP384,
    russh::kex::ECDH_SHA2_NISTP521,
    russh::kex::DH_G18_SHA512,
    russh::kex::DH_G17_SHA512,
    russh::kex::DH_G16_SHA512,
    russh::kex::DH_G15_SHA512,
    russh::kex::DH_G14_SHA256,
    russh::kex::DH_GEX_SHA256,
    russh::kex::EXTENSION_SUPPORT_AS_CLIENT,
    russh::kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
];

/// The whole login, every key and password tried: a few answers of the server, some of them after
/// its pause for a refused password (2-3 s in OpenSSH [Inference]).
const LOGIN: Duration = Duration::from_secs(30);

/// Connected and logged in. `ssh`: the folder of `config`, `known_hosts` and the keys, instead of
/// `~/.ssh` (the tests' own).
pub(super) async fn connect(t: &Target, ssh: Option<&Path>) -> Result<Handle<Client>, Error> {
    let r = resolve(t, ssh);
    let label = label(t, &r);
    // A saved key that does not open (wrong passphrase, missing file): no login another way.
    let key = match &r.key {
        Some((file, passphrase)) => match keys::load_secret_key(file, (!passphrase.is_empty()).then_some(passphrase.as_str())) {
            Ok(key) => Some(key),
            Err(e) => {
                log!("sftp: saved key {}: {e}", file.display());
                return Err(("error-key-failed", label));
            }
        },
        None => None,
    };
    let refused = Arc::new(Mutex::new(None));
    let known_hosts = ssh.map(|dir| dir.join("known_hosts"));
    let handler = Client { label: label.clone(), host: r.host.clone(), port: r.port, known_hosts: known_hosts.clone(), refused: refused.clone() };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        // russh's 2 MiB (asked again at half) held downloads from a Storage Box (50 ms ping) to
        // 10 MB/s; 8 MiB gave 22-31, like FileZilla.
        window_size: 8 << 20,
        preferred: russh::Preferred { key: known_first(&r.host, r.port, known_hosts.as_deref()).into(), kex: KEX.into(), ..Default::default() },
        ..Default::default()
    });
    let connecting = client::connect(config, (r.host.as_str(), r.port), handler);
    let mut handle = match tokio::time::timeout(Duration::from_secs(20), connecting).await {
        Err(_) => return Err((SILENT, label)),
        Ok(Err(e)) => return Err(refused.lock().unwrap().take().unwrap_or(("error-detail", format!("{label}: {e}")))),
        Ok(Ok(h)) => h,
    };
    // Within a time limit too: the connection's lock is held meanwhile, and every request waits
    // for it (#75).
    let rsa = match tokio::time::timeout(LOGIN, login(&mut handle, &r, t.password.as_deref(), key)).await {
        Err(_) => return Err((SILENT, label)),
        Ok(r) => r,
    };
    let rsa = rsa.map_err(|()| ("error-login-ssh", format!("{}@{label}", r.user)))?;
    log!("sftp: logged in to {label} as {}{}", r.user, if rsa { " with an RSA key" } else { "" });
    let mut warned = RSA.lock().unwrap();
    if rsa && !warned.0.contains(&label) {
        warned.0.push(label.clone());
        warned.1 = Some(label);
    }
    drop(warned);
    Ok(handle)
}

/// The key types known_hosts has for this server first, as ssh does: otherwise a server with an
/// ECDSA line would offer its Ed25519 key and look unknown.
fn known_first(host: &str, port: u16, known_hosts: Option<&Path>) -> Vec<keys::Algorithm> {
    let known = match known_hosts {
        Some(file) => keys::known_hosts::known_host_keys_path(host, port, file),
        None => keys::known_hosts::known_host_keys(host, port),
    };
    let mut algs: Vec<_> = known.unwrap_or_default().into_iter().map(|(_, k)| k.algorithm()).collect();
    for a in russh::Preferred::default().key.iter() {
        if !algs.contains(a) {
            algs.push(a.clone());
        }
    }
    algs
}

/// As ssh: every key until the server accepts one or stops answering (MaxAuthTries). A password
/// in the address goes first: whoever typed it wants to log in with it. `Ok(true)`: with an RSA key.
async fn login(h: &mut Handle<Client>, r: &Resolved, password: Option<&str>, key: Option<keys::PrivateKey>) -> Result<bool, ()> {
    if let Some(p) = password
        && password_login(h, &r.user, p).await
    {
        return Ok(false);
    }
    let rsa_hash = h.best_supported_rsa_hash().await.ok().flatten().flatten();
    if let Some(key) = key
        && let Some(rsa) = key_login(h, &r.user, key, rsa_hash).await
    {
        return Ok(rsa);
    }
    if r.saved {
        return Err(());
    }
    for file in r.files.iter().filter(|f| f.exists()) {
        // A key with a passphrase is skipped: never the system's agent (the user's rule), so a
        // saved server with its passphrase is the way to use it.
        let Ok(key) = keys::load_secret_key(file, None) else { continue };
        if let Some(rsa) = key_login(h, &r.user, key, rsa_hash).await {
            return Ok(rsa);
        }
    }
    Err(())
}

/// A private key: `Some(it is RSA)` when the server accepts it.
async fn key_login(h: &mut Handle<Client>, user: &str, key: keys::PrivateKey, rsa_hash: Option<HashAlg>) -> Option<bool> {
    let rsa = key.algorithm().is_rsa();
    let key = PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash);
    matches!(h.authenticate_publickey(user, key).await, Ok(AuthResult::Success)).then_some(rsa)
}

/// "password", or "keyboard-interactive" answering each question with the password (servers
/// that only allow the latter ask just for it).
async fn password_login(h: &mut Handle<Client>, user: &str, password: &str) -> bool {
    if matches!(h.authenticate_password(user, password).await, Ok(AuthResult::Success)) {
        return true;
    }
    let mut step = h.authenticate_keyboard_interactive_start(user, None).await;
    for _ in 0..3 {
        match step {
            Ok(KeyboardInteractiveAuthResponse::Success) => return true,
            Ok(KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. }) => {
                let answers = prompts.iter().map(|_| password.to_string()).collect();
                step = h.authenticate_keyboard_interactive_respond(answers).await;
            }
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    /// Without a line in ~/.ssh/config the address is used as it is, on port 22.
    #[test]
    fn resolve_without_config() {
        let t = super::Target { user: "me".into(), host: "server.invalid".into(), port: None, password: None, key: None, saved: false };
        let r = super::resolve(&t, None);
        assert_eq!((r.host.as_str(), r.port, r.user.as_str()), ("server.invalid", 22, "me"));
        let t = super::Target { port: Some(2222), ..t };
        assert_eq!(super::resolve(&t, None).port, 2222);
    }

    /// An alias shows the address it stands for; an address is shown once.
    #[test]
    fn label_of_alias() {
        let t = super::Target { user: String::new(), host: "box".into(), port: None, password: None, key: None, saved: false };
        let r = |host: &str, port| super::Resolved { host: host.into(), port, user: String::new(), files: vec![], key: None, saved: false };
        assert_eq!(super::label(&t, &r("10.0.0.1", 2222)), "box (10.0.0.1:2222)");
        assert_eq!(super::label(&t, &r("box", 22)), "box");
    }
}
