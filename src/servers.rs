//! Saved servers (File > Open location): address, user, password, folder and, for SFTP, a key
//! file with its passphrase, so a server opens without typing them again.
//! Linux: `$XDG_CONFIG_HOME/eyecansee/servers.json` (only the user can read it), Windows:
//! `%APPDATA%\eyecansee\servers.json`. Kept apart from `settings.json`, which is not secret.
//! With a master password the file is `servers.vault`, encrypted (`vault.rs`), and the plain one
//! is removed.

use std::path::PathBuf;
use std::sync::Mutex;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};

pub(crate) mod vault;

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub(crate) name: String,
    /// "ftp", "ftps" or "sftp".
    pub(crate) scheme: String,
    pub(crate) host: String,
    pub(crate) port: Option<u16>,
    pub(crate) user: String,
    pub(crate) password: String,
    pub(crate) folder: String,
    /// SFTP: a private key file ("~/" is the home folder) and its passphrase ("" = none).
    pub(crate) key_file: String,
    pub(crate) key_passphrase: String,
}

impl Saved {
    /// The key file with "~/" expanded; `None` when there is none.
    pub(crate) fn key(&self) -> Option<(PathBuf, String)> {
        let file = self.key_file.trim();
        if file.is_empty() {
            return None;
        }
        let path = match (file.strip_prefix("~/").or_else(|| file.strip_prefix("~\\")), std::env::home_dir()) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(file),
        };
        Some((path, self.key_passphrase.clone()))
    }
}

impl Saved {
    /// Its address without the password (found again by `find` when opened): what the left
    /// panel opens. `None` for a server name that is not valid in an address.
    pub(crate) fn address(&self) -> Option<String> {
        let mut u = url::Url::parse(&format!("{}://{}/", self.scheme, self.host.trim())).ok()?;
        u.set_port(self.port).ok()?;
        if !self.user.is_empty() {
            u.set_username(&self.user).ok()?;
        }
        let folder = self.folder.trim().trim_matches('/');
        u.set_path(&if folder.is_empty() { "/".to_string() } else { format!("/{folder}/") });
        Some(u.to_string())
    }
}

/// Where the saved servers are, as shown in the server manager.
pub(crate) fn file() -> String {
    let file = if encrypted() { vault_path() } else { path() };
    file.map(|p| p.display().to_string()).unwrap_or_default()
}

fn path() -> Option<PathBuf> {
    crate::settings::config_path("servers.json")
}

/// The encrypted file, in place of `servers.json` once there is a master password.
fn vault_path() -> Option<PathBuf> {
    crate::settings::config_path("servers.vault")
}

/// The key of the master password, once given: until the program closes.
static KEY: Mutex<Option<vault::Key>> = Mutex::new(None);

/// Is there a master password (the servers are in the encrypted file)?
pub(crate) fn encrypted() -> bool {
    vault_path().is_some_and(|p| p.exists())
}

/// Encrypted and not unlocked yet in this session: the servers cannot be read.
pub(crate) fn locked() -> bool {
    encrypted() && KEY.lock().unwrap().is_none()
}

/// The master password, to read the encrypted file. `Err(None)`: wrong password.
pub(crate) fn unlock(password: &str) -> Result<(), Option<String>> {
    let data = std::fs::read(vault_path().ok_or(Some(String::new()))?).map_err(|e| Some(e.to_string()))?;
    let (key, _) = vault::open(password, &data)?;
    *KEY.lock().unwrap() = Some(key);
    Ok(())
}

/// Missing file, or encrypted and locked: no servers. Unreadable plain file: set aside as `.bad`,
/// as for the settings.
pub(crate) fn load() -> Vec<Saved> {
    if encrypted() {
        let key = KEY.lock().unwrap().clone();
        let plain = key.zip(vault_path().and_then(|p| std::fs::read(p).ok())).and_then(|(k, data)| vault::open_with(&k, &data));
        return plain.and_then(|p| serde_json::from_slice(&p).ok()).unwrap_or_default();
    }
    let Some(path) = path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_else(|_| {
        let _ = std::fs::rename(&path, path.with_extension("json.bad"));
        Vec::new()
    })
}

/// Every change, at once: encrypted with the master password's key when there is one,
/// otherwise in plain text.
pub(crate) fn save(servers: &[Saved]) -> std::io::Result<()> {
    let text = serde_json::to_vec_pretty(servers)?;
    let key = KEY.lock().unwrap().clone();
    match key {
        Some(key) => write_vault(&key, &text),
        None if encrypted() => Err(std::io::Error::other("the servers are locked")),
        None => write_safely(&path().ok_or(std::io::ErrorKind::NotFound)?, &text, |_| true),
    }
}

/// A new master password (also in place of the old one, or to encrypt the plain file): the
/// servers written encrypted, then the plain file removed.
pub(crate) fn set_password(password: &str) -> std::io::Result<()> {
    let servers = load();
    let key = vault::new_key(password).map_err(std::io::Error::other)?;
    write_vault(&key, &serde_json::to_vec_pretty(&servers)?)?;
    *KEY.lock().unwrap() = Some(key);
    Ok(())
}

/// No master password any more: the servers written in plain text, then the encrypted file removed.
pub(crate) fn remove_password() -> std::io::Result<()> {
    let servers = load();
    write_safely(&path().ok_or(std::io::ErrorKind::NotFound)?, &serde_json::to_vec_pretty(&servers)?, |_| true)?;
    std::fs::remove_file(vault_path().ok_or(std::io::ErrorKind::NotFound)?)?;
    *KEY.lock().unwrap() = None;
    Ok(())
}

/// The encrypted file, checked by opening it again before it replaces the old one; a plain file
/// left over (a switch interrupted halfway) goes after it.
fn write_vault(key: &vault::Key, text: &[u8]) -> std::io::Result<()> {
    let data = vault::seal(key, text).map_err(std::io::Error::other)?;
    write_safely(&vault_path().ok_or(std::io::ErrorKind::NotFound)?, &data, |d| vault::open_with(key, d).as_deref() == Some(text))?;
    if let Some(plain) = path().filter(|p| p.exists()) {
        std::fs::remove_file(plain)?;
    }
    Ok(())
}

/// Written to a temporary file (only the user can read it on Linux), flushed to the disk, read
/// back and checked with `good`, then renamed over `path`: the old file stays whole until the new
/// one is.
fn write_safely(path: &std::path::Path, data: &[u8], good: impl Fn(&[u8]) -> bool) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().ok_or(std::io::ErrorKind::NotFound)?)?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut f = options.open(&tmp)?;
    std::io::Write::write_all(&mut f, data)?;
    f.sync_all()?;
    drop(f);
    let back = std::fs::read(&tmp)?;
    if back != data || !good(&back) {
        let _ = std::fs::remove_file(&tmp);
        return Err(std::io::Error::other("the file written does not read back the same"));
    }
    std::fs::rename(&tmp, path)
}

/// The saved server an address points to: same protocol, server, port and user.
pub(crate) fn find(address: &str) -> Option<Saved> {
    find_in(load(), address)
}

fn find_in(servers: Vec<Saved>, address: &str) -> Option<Saved> {
    let u = url::Url::parse(address).ok()?;
    let host = u.host_str()?;
    let user = percent_decode_str(u.username()).decode_utf8_lossy();
    let wanted = port(u.scheme(), u.port());
    servers.into_iter().find(|s| s.scheme == u.scheme() && s.host.eq_ignore_ascii_case(host) && port(&s.scheme, s.port) == wanted && s.user == user)
}

/// The port, the protocol's own when none is written: an address drops ":21" from ftp://.
fn port(scheme: &str, port: Option<u16>) -> Option<u16> {
    port.or(match scheme {
        "ftp" | "ftps" => Some(21),
        "sftp" => Some(22),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::Saved;

    #[test]
    fn found_by_address() {
        let nas = Saved { name: "NAS".into(), scheme: "sftp".into(), host: "nas.lan".into(), port: Some(2222), user: "me@home".into(), ..Default::default() };
        let ftp = Saved { scheme: "ftp".into(), host: "nas.lan".into(), ..Default::default() };
        let find = |a: &str| super::find_in(vec![nas.clone(), ftp.clone()], a).map(|s| s.scheme);
        assert_eq!(find("sftp://me%40home@NAS.lan:2222/Foto/"), Some("sftp".into()));
        assert_eq!(find("ftp://nas.lan/"), Some("ftp".into()));
        // The protocol's own port, written or not, is the same server.
        let ftp21 = Saved { scheme: "ftp".into(), host: "box".into(), port: Some(21), ..Default::default() };
        assert!(super::find_in(vec![ftp21.clone()], &ftp21.address().unwrap()).is_some());
        assert!(super::find_in(vec![ftp21], "ftp://box:21/").is_some());
        let sftp = Saved { scheme: "sftp".into(), host: "box".into(), ..Default::default() };
        assert!(super::find_in(vec![sftp], "sftp://box:22/").is_some());
        // Another port, user or protocol is another server.
        for a in ["sftp://me%40home@nas.lan/", "sftp://other@nas.lan:2222/", "ftps://nas.lan/", "/home/me/"] {
            assert_eq!(find(a), None, "{a}");
        }
    }

    #[test]
    fn address_without_password() {
        let s = Saved { scheme: "sftp".into(), host: "nas.lan".into(), port: Some(2222), user: "me@home".into(), password: "pw".into(), folder: "/Foto/2026".into(), ..Default::default() };
        assert_eq!(s.address(), Some("sftp://me%40home@nas.lan:2222/Foto/2026/".into()));
        assert_eq!(super::find_in(vec![s.clone()], &s.address().unwrap()).map(|s| s.password), Some("pw".into()));
        let ftp = Saved { scheme: "ftp".into(), host: "nas.lan".into(), ..Default::default() };
        assert_eq!(ftp.address(), Some("ftp://nas.lan/".into()));
    }

    #[test]
    fn key_file() {
        let s = |f: &str| Saved { key_file: f.into(), key_passphrase: "pw".into(), ..Default::default() }.key();
        assert_eq!(s(" "), None);
        assert_eq!(s("/keys/id"), Some(("/keys/id".into(), "pw".into())));
        assert_eq!(s("~/.ssh/id").unwrap().0, std::env::home_dir().unwrap().join(".ssh/id"));
    }

    /// Plain, then encrypted, locked as after a restart, unlocked, changed, plain again; the
    /// right file at every step. Writes the files: only with a temporary configuration,
    /// `XDG_CONFIG_HOME=$(mktemp -d) cargo test --release vault_switch_live -- --ignored`.
    #[test]
    #[ignore]
    fn vault_switch_live() {
        let dir = super::path().unwrap().parent().unwrap().to_path_buf();
        assert!(dir.starts_with(std::env::temp_dir()), "only with a temporary configuration");
        let nas = Saved { name: "NAS".into(), scheme: "ftp".into(), host: "nas.lan".into(), password: "segreta".into(), ..Default::default() };
        let (plain, vault) = (dir.join("servers.json"), dir.join("servers.vault"));
        super::save(std::slice::from_ref(&nas)).unwrap();
        assert!(plain.exists() && !vault.exists());
        super::set_password("Tramonto-viola-sul-Gran-Sasso-1976").unwrap();
        assert!(!plain.exists() && vault.exists() && super::encrypted());
        assert!(!std::fs::read(&vault).unwrap().windows(7).any(|w| w == b"segreta"), "the password is not in clear");
        // As after a restart: locked, nothing to read, nothing written.
        *super::KEY.lock().unwrap() = None;
        assert!(super::locked() && super::load().is_empty() && super::find("ftp://nas.lan/").is_none());
        assert!(super::save(&[]).is_err());
        assert!(matches!(super::unlock("tramonto-viola-sul-Gran-Sasso-1976"), Err(None)));
        super::unlock("Tramonto-viola-sul-Gran-Sasso-1976").unwrap();
        assert_eq!(super::load(), vec![nas.clone()]);
        let moved = Saved { port: Some(2121), ..nas.clone() };
        super::save(std::slice::from_ref(&moved)).unwrap();
        super::set_password("Un-altra-frase-lunga-per-il-NAS-2026").unwrap();
        *super::KEY.lock().unwrap() = None;
        assert!(matches!(super::unlock("Tramonto-viola-sul-Gran-Sasso-1976"), Err(None)));
        super::unlock("Un-altra-frase-lunga-per-il-NAS-2026").unwrap();
        assert_eq!(super::load(), vec![moved.clone()]);
        super::remove_password().unwrap();
        assert!(plain.exists() && !vault.exists() && !super::locked());
        assert_eq!(super::load(), vec![moved]);
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn missing_fields_are_empty() {
        let s: Vec<Saved> = serde_json::from_str(r#"[{"name": "NAS", "scheme": "ftp", "host": "nas.lan"}]"#).unwrap();
        assert_eq!((s[0].port, s[0].password.as_str()), (None, ""));
    }
}
