//! Addresses and paths: an address opened as a `Source` (local path, `file://`, `ftp://`, `ftps://`,
//! `sftp://`, `mtp://`), the folder and file in a path, names and parents, and what a path says
//! about where it is (a gvfs mount, a temporary copy made by Windows).

use std::path::Path;

use opendal::{Operator, services};
use percent_encoding::percent_decode_str;
use url::Url;

use super::{BAD_NAME, Error, Source, Store};
use crate::decode::is_media;

pub fn in_gvfs(root: &str, path: &str) -> bool {
    format!("{root}/{path}").contains("/gvfs/")
}

/// A temporary copy Windows made of a file it cannot hand over as a path, kept in its internet
/// cache: a phone photo opened with a double click in File Explorer arrives as
/// `C:\Users\<user>\AppData\Local\Microsoft\Windows\INetCache\IE\<random>\<name>[1].jpg`
/// (seen on Windows 11 with a phone in "File transfer" mode, issue #47). Saving over it would
/// change only the copy, not the file on the phone.
pub fn windows_temp_copy(path: &str) -> bool {
    path.replace('\\', "/").to_ascii_lowercase().contains("/microsoft/windows/inetcache/")
}

/// Splits a relative path into (folder, file) if the last element is an image or a video.
fn split(path: &str) -> (String, Option<String>) {
    let path = path.trim_matches('/');
    let dir = |d: &str| if d.is_empty() { String::new() } else { format!("{d}/") };
    match path.rsplit_once('/').unwrap_or(("", path)) {
        (d, name) if is_media(name) => (dir(d), Some(path.to_string())),
        _ => (dir(path), None),
    }
}

fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// Last element of a relative path, for labels and titles.
pub fn name(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

/// Folder containing `dir` ("" if `dir` is already the root).
pub fn parent(dir: &str) -> String {
    match dir.trim_end_matches('/').rsplit_once('/') {
        Some((p, _)) => format!("{p}/"),
        None => String::new(),
    }
}

/// Accepts a local path or `file://`, `ftp://`, `ftps://`, `sftp://`, `mtp://`.
pub fn open(arg: &str) -> Result<Source, Error> {
    // "C:\photos" would be read as a URL with scheme "c": one-letter schemes are paths.
    let url = Url::parse(arg).ok().filter(|u| u.scheme().len() > 1);
    let Some(url) = url else { return local(Path::new(arg)) };
    if url.scheme() == "file" {
        let path = url.to_file_path().map_err(|_| ("error-path", arg.to_string()))?;
        return local(&path);
    }

    let host = url.host_str().ok_or(("error-no-host", arg.to_string()))?;
    let (dir, start) = split(&decode(url.path()));
    if url.scheme() == "mtp" {
        let serial = decode(host);
        let base = format!("mtp://{serial}/");
        let op = Store::Mtp(crate::mtp::Phone::shared(&serial));
        return Ok(Source { op, base, dir, start, home: false });
    }
    let user = match url.username() {
        "" => String::new(),
        u => format!("{}@", decode(u)),
    };
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    // The password never appears in the address bar.
    let base = format!("{}://{user}{host}{port}/", url.scheme());
    // A saved server gives what the address leaves out: password and key, and only those.
    let found = crate::servers::find(arg);
    let saved = found.clone().unwrap_or_default();
    let password = url.password().map(decode).or_else(|| Some(saved.password.clone()).filter(|p| !p.is_empty()));
    if url.scheme() == "sftp" {
        let (user, port, key) = (decode(url.username()), url.port(), saved.key());
        let target = crate::sftp::Target { user, host: host.to_string(), port, password, key, saved: found.is_some() };
        let home = dir.is_empty() && start.is_none();
        return Ok(Source { op: Store::Sftp(crate::sftp::Server::shared(target)), base, dir, start, home });
    }
    if let scheme @ ("ftp" | "ftps") = url.scheme() {
        let (user, port, secure) = (decode(url.username()), url.port_or_known_default().unwrap_or(21), scheme == "ftps");
        let target = crate::ftp::Target { host: host.to_string(), port, user, password: password.unwrap_or_default(), secure };
        let home = dir.is_empty() && start.is_none();
        return Ok(Source { op: Store::Ftp(crate::ftp::Server::shared(target)), base, dir, start, home });
    }
    Err(("error-unsupported", url.scheme().to_string()))
}

fn local(path: &Path) -> Result<Source, Error> {
    let lossy = path.to_string_lossy();
    let path = path.canonicalize().map_err(|e| match lossy.contains(char::REPLACEMENT_CHARACTER) && e.kind() == std::io::ErrorKind::NotFound {
        true => (BAD_NAME, lossy.to_string()), // an argument whose name is not valid UTF-8 (`bad_name`)
        false => ("error-path", format!("{}: {e}", path.display())),
    })?;
    // "/" on Linux, "\\?\C:\" on Windows.
    let root = path.ancestors().last().unwrap_or(&path);
    let rel = path.strip_prefix(root).unwrap_or(&path);
    let rel: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let (dir, start) = split(&rel.join("/"));
    let op = Operator::new(services::Fs::default().root(&root.to_string_lossy()))
        .map_err(|e| ("error-source", e.to_string()))?;
    // ponytail: Windows' "\\?\" prefix is hidden only in the bar; it is really needed only for paths over 260 characters
    let base = root.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    Ok(Source { op: Store::Dal(op), base, dir, start, home: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_open() {
        assert_eq!(split("/photos/sea/"), ("photos/sea/".into(), None));
        assert_eq!(split("/photos/a.JPG"), ("photos/".into(), Some("photos/a.JPG".into())));
        assert_eq!(split("/a.png"), ("".into(), Some("a.png".into())));
        assert_eq!(split("/"), ("".into(), None));
        assert_eq!((parent("a/b/"), parent("a/"), parent("")), ("a/".into(), "".into(), "".into()));
        assert_eq!((name("a/b/"), name("a/c.jpg")), ("b", "c.jpg"));
        // Building the FTP operator does not open connections.
        let s = open("ftp://me:p%40ss@nas.lan/Sea%20photos/a.png").unwrap();
        assert_eq!((s.base.as_str(), s.dir.as_str()), ("ftp://me@nas.lan/", "Sea photos/"));
        assert_eq!(s.start.as_deref(), Some("Sea photos/a.png"));
        assert_eq!(open("gopher://x/").err().map(|e| e.0), Some("error-unsupported"));
        // The checkout folder can have any name (/src in the AppImage container).
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).file_name().unwrap().to_string_lossy();
        let l = open(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(l.dir.ends_with(&format!("{here}/")) && l.start.is_none());
        assert!(in_gvfs("/", "run/user/1000/gvfs/mtp:host=X/DCIM/") && in_gvfs("/run/user/1000/gvfs/", "a.jpg"));
        assert!(!in_gvfs("/", "home/me/a.jpg"));
        // Issue #47: the path Windows 11 handed over for a phone photo opened from File Explorer.
        assert!(windows_temp_copy(r"C:\Users\Ann\AppData\Local\Microsoft\Windows\INetCache\IE\OC6EAUQ4\photo-47[1].jpg"));
        assert!(windows_temp_copy("C:\\/Users/Ann/AppData/Local/Microsoft/Windows/INetCache/IE/OC6EAUQ4/photo-47[1].jpg"));
        for normal in [r"C:\Users\Ann\Pictures\IMG_0001.jpg", r"D:\Photos\Microsoft\Windows\a.jpg", r"C:\Users\Ann\INetCache\a.jpg", "/home/me/Microsoft/a.jpg"] {
            assert!(!windows_temp_copy(normal), "{normal}");
        }
    }
}
