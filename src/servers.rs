//! Saved servers (File > Open location): address, user, password, folder and, for SFTP, a key
//! file with its passphrase, so a server opens without typing them again.
//! Linux: `$XDG_CONFIG_HOME/eyecansee/servers.json` (only the user can read it), Windows:
//! `%APPDATA%\eyecansee\servers.json`. Kept apart from `settings.json`, which is not secret.
//! ponytail: passwords in plain text, as the user asked; to be encrypted with a master password.

use std::path::PathBuf;

use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};

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
    path().map(|p| p.display().to_string()).unwrap_or_default()
}

fn path() -> Option<PathBuf> {
    crate::settings::config_path("servers.json")
}

/// Missing file: no servers. Unreadable file: set aside as `.bad`, as for the settings.
pub(crate) fn load() -> Vec<Saved> {
    let Some(path) = path() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_else(|_| {
        let _ = std::fs::rename(&path, path.with_extension("json.bad"));
        Vec::new()
    })
}

/// Written to a temporary file and renamed, readable only by the user on Linux.
pub(crate) fn save(servers: &[Saved]) -> std::io::Result<()> {
    let path = path().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(path.parent().ok_or(std::io::ErrorKind::NotFound)?)?;
    let text = serde_json::to_string_pretty(servers)?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&tmp)?, text.as_bytes())?;
    std::fs::rename(&tmp, &path)
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

    #[test]
    fn missing_fields_are_empty() {
        let s: Vec<Saved> = serde_json::from_str(r#"[{"name": "NAS", "scheme": "ftp", "host": "nas.lan"}]"#).unwrap();
        assert_eq!((s[0].port, s[0].password.as_str()), (None, ""));
    }
}
