//! Opening an SFTP connection as `ssh` would: the host as `~/.ssh/config` describes it (alias,
//! HostName, User, Port, IdentityFile), the server's key checked against `~/.ssh/known_hosts`,
//! then the login with the saved server's key, the agent's keys, key files without a passphrase,
//! or the password.
//! Not supported: ProxyJump and ProxyCommand.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, AuthResult, Handle, KeyboardInteractiveAuthResponse};
use russh::keys::agent::AgentIdentity;
use russh::keys::agent::client::{AgentClient, AgentStream};
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};

use crate::source::Error;

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
}

/// Servers logged in to with an RSA key in this session, each warned once, and the one whose
/// warning is still to be shown (`rsa_login`): RUSTSEC-2023-0071 has no fix in the `rsa` crate.
static RSA: Mutex<(Vec<String>, Option<String>)> = Mutex::new((Vec::new(), None));

/// A server whose saved key could not be opened (wrong passphrase, missing file), to say so.
static KEY_FAILED: Mutex<Option<String>> = Mutex::new(None);

pub fn key_failed() -> Option<String> {
    KEY_FAILED.lock().unwrap().take()
}

/// The server just logged in to with an RSA key, to warn about it once.
pub fn rsa_login() -> Option<String> {
    RSA.lock().unwrap().1.take()
}

/// A server key not in known_hosts, waiting for the user's answer (`trust`): host label, host,
/// port and key.
static UNKNOWN: Mutex<Option<(String, String, u16, PublicKey)>> = Mutex::new(None);

/// The server whose key the app has to ask about: its name and the key's fingerprint.
pub fn unknown() -> Option<(String, String)> {
    UNKNOWN.lock().unwrap().as_ref().map(|(label, _, _, key)| (label.clone(), key.fingerprint(HashAlg::Sha256).to_string()))
}

/// The user trusts the key asked about: written to `~/.ssh/known_hosts`, as ssh does.
pub fn trust() {
    if let Some((label, host, port, key)) = UNKNOWN.lock().unwrap().take() {
        let r = keys::known_hosts::learn_known_hosts(&host, port, &key);
        log!("sftp: key of {label} trusted: {r:?}");
    }
}

pub(super) struct Client {
    label: String,
    host: String,
    port: u16,
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
        let refused = match keys::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => return Ok(true),
            Err(keys::Error::KeyChanged { line }) => ("error-host-changed", format!("{} (known_hosts, line {line})", self.label)),
            // Not there, or no known_hosts yet.
            _ => {
                *UNKNOWN.lock().unwrap() = Some((self.label.clone(), self.host.clone(), self.port, key.clone()));
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
}

fn resolve(t: &Target) -> Resolved {
    let cfg = russh_config::parse_home(&t.host).unwrap_or_else(|_| russh_config::Config::default(&t.host));
    let user = if t.user.is_empty() { cfg.user() } else { t.user.clone() };
    let mut files = cfg.host_config.identity_file.clone().unwrap_or_default();
    if let Some(ssh) = std::env::home_dir().map(|h| h.join(".ssh")) {
        files.extend(["id_ed25519", "id_ecdsa", "id_rsa"].map(|f| ssh.join(f)));
    }
    Resolved { host: cfg.host().to_string(), port: t.port.unwrap_or_else(|| cfg.port()), user, files, key: t.key.clone() }
}

/// The server as the user wrote it, with the address it stands for when it is an alias of
/// `~/.ssh/config`: "ironmanl (100.64.10.6:40222)".
fn label(t: &Target, r: &Resolved) -> String {
    let addr = if r.port == 22 { r.host.clone() } else { format!("{}:{}", r.host, r.port) };
    if t.host == r.host { addr } else { format!("{} ({addr})", t.host) }
}

/// Connected and logged in.
pub(super) async fn connect(t: &Target) -> Result<Handle<Client>, Error> {
    let r = resolve(t);
    let label = label(t, &r);
    let refused = Arc::new(Mutex::new(None));
    let handler = Client { label: label.clone(), host: r.host.clone(), port: r.port, refused: refused.clone() };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        preferred: russh::Preferred { key: known_first(&r.host, r.port).into(), ..Default::default() },
        ..Default::default()
    });
    let connecting = client::connect(config, (r.host.as_str(), r.port), handler);
    let mut handle = match tokio::time::timeout(Duration::from_secs(20), connecting).await {
        Err(_) => return Err(("error-detail", format!("{label}: no answer"))),
        Ok(Err(e)) => return Err(refused.lock().unwrap().take().unwrap_or(("error-detail", format!("{label}: {e}")))),
        Ok(Ok(h)) => h,
    };
    let key = r.key.as_ref().and_then(|(file, passphrase)| {
        keys::load_secret_key(file, (!passphrase.is_empty()).then_some(passphrase.as_str()))
            .inspect_err(|e| {
                log!("sftp: saved key {}: {e}", file.display());
                *KEY_FAILED.lock().unwrap() = Some(label.clone());
            })
            .ok()
    });
    let rsa = login(&mut handle, &r, t.password.as_deref(), key).await.map_err(|()| ("error-login-ssh", format!("{}@{label}", r.user)))?;
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
fn known_first(host: &str, port: u16) -> Vec<keys::Algorithm> {
    let mut algs: Vec<_> = keys::known_hosts::known_host_keys(host, port).unwrap_or_default().into_iter().map(|(_, k)| k.algorithm()).collect();
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
    // The keys named in ~/.ssh/config, as public keys next to their files.
    let named: Vec<PublicKey> = r.files.iter().filter_map(|f| PublicKey::read_openssh_file(pub_file(f)).ok()).collect();
    if let Some(rsa) = agent_login(h, &r.user, &named, rsa_hash).await {
        return Ok(rsa);
    }
    for file in r.files.iter().filter(|f| f.exists()) {
        // A key with a passphrase is skipped: the agent is the way to use it.
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

/// "key" → "key.pub" (not `with_extension`: key names can have dots).
fn pub_file(key: &std::path::Path) -> PathBuf {
    let mut name = key.as_os_str().to_owned();
    name.push(".pub");
    name.into()
}

/// The agent's keys, those named in ~/.ssh/config first. Linux: `SSH_AUTH_SOCK`; Windows: the
/// OpenSSH agent's pipe, then Pageant. `Some(it is RSA)` when the server accepts a key.
async fn agent_login(h: &mut Handle<Client>, user: &str, named: &[PublicKey], rsa_hash: Option<HashAlg>) -> Option<bool> {
    #[cfg(unix)]
    if let Ok(agent) = AgentClient::connect_env().await {
        return with_agent(agent, h, user, named, rsa_hash).await;
    }
    #[cfg(windows)]
    {
        if let Ok(agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            return with_agent(agent, h, user, named, rsa_hash).await;
        }
        if let Ok(agent) = AgentClient::connect_pageant().await {
            return with_agent(agent, h, user, named, rsa_hash).await;
        }
    }
    None
}

async fn with_agent<S: AgentStream + Unpin + Send + 'static>(
    mut agent: AgentClient<S>,
    h: &mut Handle<Client>,
    user: &str,
    named: &[PublicKey],
    rsa_hash: Option<HashAlg>,
) -> Option<bool> {
    let mut ids: Vec<AgentIdentity> = agent.request_identities().await.unwrap_or_default();
    ids.sort_by_key(|id| !named.contains(&id.public_key()));
    for id in ids {
        let key = id.public_key().into_owned();
        let rsa = key.algorithm().is_rsa();
        let hash = if rsa { rsa_hash } else { None };
        if matches!(h.authenticate_publickey_with(user, key, hash, &mut agent).await, Ok(AuthResult::Success)) {
            return Some(rsa);
        }
    }
    None
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
        let t = super::Target { user: "me".into(), host: "server.invalid".into(), port: None, password: None, key: None };
        let r = super::resolve(&t);
        assert_eq!((r.host.as_str(), r.port, r.user.as_str()), ("server.invalid", 22, "me"));
        let t = super::Target { port: Some(2222), ..t };
        assert_eq!(super::resolve(&t).port, 2222);
    }

    /// An alias shows the address it stands for; an address is shown once.
    #[test]
    fn label_of_alias() {
        let t = super::Target { user: String::new(), host: "box".into(), port: None, password: None, key: None };
        let r = |host: &str, port| super::Resolved { host: host.into(), port, user: String::new(), files: vec![], key: None };
        assert_eq!(super::label(&t, &r("10.0.0.1", 2222)), "box (10.0.0.1:2222)");
        assert_eq!(super::label(&t, &r("box", 22)), "box");
    }
}
