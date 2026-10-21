//! Where the images come from: local disk, FTP, SFTP (Unix only) through OpenDAL, and
//! phones connected directly over MTP (`mtp://<serial>/`), which OpenDAL does not cover.
//! Every source has its root at the top, so the folder tree can climb all the way up.
//! Folders are paths relative to the root ending in `/` ("" is the root).

use std::path::Path;
use std::sync::Arc;

use opendal::{Operator, services};
use percent_encoding::percent_decode_str;
use url::Url;

use crate::decode::is_image;
use crate::settings::SortBy;

/// Translatable error: Fluent message id and technical detail.
pub type Error = (&'static str, String);

/// Id of the "not found" error, recognised by cache maintenance.
pub const NOT_FOUND: &str = "error-not-found";

/// The operations the app needs, the same for all sources.
#[derive(Clone)]
pub enum Store {
    Dal(Operator),
    Mtp(Arc<crate::mtp::Phone>),
}

/// Reads under gvfs (phones, cameras, shares mounted by the desktop), two at a time like the
/// workers of `Store::Mtp`. With too many requests at once gvfs stalls: 12 file opens while a
/// 796-entry folder was being listed gave 12 files out of 30 in 31 s and the listing never ended;
/// with 8 or fewer everything took 3-6 s (script, 2026-10-20; gvfsd-fuse has 10 worker threads
/// [Inference: the limit comes from there]). More parallel reads are not faster: gvfsd-mtp serves
/// one request at a time and in a large folder every file costs ~1.5 s (FUSE_LOOKUP + FUSE_OPEN,
/// kernel trace). Listings stay out, otherwise the tree waits for the thumbnails (Pictures after 14 s).
static GVFS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

async fn gvfs_permit(op: &Operator, path: &str) -> Option<tokio::sync::SemaphorePermit<'static>> {
    if op.info().scheme() == "fs" && in_gvfs(&op.info().root(), path) { GVFS.acquire().await.ok() } else { None }
}

fn in_gvfs(root: &str, path: &str) -> bool {
    format!("{root}/{path}").contains("/gvfs/")
}

fn dal_error(e: opendal::Error) -> Error {
    let id = if e.kind() == opendal::ErrorKind::NotFound { NOT_FOUND } else { "error-detail" };
    (id, e.to_string())
}

impl Store {
    pub async fn list(&self, dir: &str) -> Result<Listing, Error> {
        match self {
            Store::Dal(op) => list(op, dir).await.map_err(dal_error),
            Store::Mtp(phone) => phone.list(dir).await,
        }
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        match self {
            Store::Dal(op) => {
                let _permit = gvfs_permit(op, path).await;
                op.read(path).await.map(|b| b.to_vec()).map_err(dal_error)
            }
            Store::Mtp(phone) => phone.read(path).await,
        }
    }

    /// Does the folder exist? OpenDAL (at least `fs`) lists a non-existent folder as empty.
    pub async fn check_dir(&self, dir: &str) -> Result<(), Error> {
        match self {
            Store::Dal(_) if dir.is_empty() => Ok(()),
            Store::Dal(op) => op.stat(dir).await.map(|_| ()).map_err(dal_error),
            Store::Mtp(phone) => phone.check_dir(dir).await,
        }
    }
}

pub struct Source {
    pub op: Store,
    /// Prefix shown in the address bar: "/", "C:\", "ftp://user@host/".
    pub base: String,
    /// Initial folder.
    pub dir: String,
    /// Image to open straight away, if the address pointed to a file.
    pub start: Option<String>,
}

#[derive(Default)]
pub struct Listing {
    pub dirs: Vec<String>,
    pub files: Vec<File>,
}

#[derive(Clone)]
pub struct File {
    pub path: String,
    /// Size and modification date: they identify the file's version for the thumbnail cache.
    pub size: u64,
    /// As the source gives it ("" if it does not): only used for comparison.
    pub modified: String,
}

/// Splits a relative path into (folder, file) if the last element is an image.
fn split(path: &str) -> (String, Option<String>) {
    let path = path.trim_matches('/');
    let dir = |d: &str| if d.is_empty() { String::new() } else { format!("{d}/") };
    match path.rsplit_once('/').unwrap_or(("", path)) {
        (d, name) if is_image(name) => (dir(d), Some(path.to_string())),
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

/// Accepts a local path or `file://`, `ftp://`, `ftps://`, `sftp://` (Unix only).
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
        let op = Store::Mtp(Arc::new(crate::mtp::Phone::new(&serial)));
        return Ok(Source { op, base, dir, start });
    }
    let user = match url.username() {
        "" => String::new(),
        u => format!("{}@", decode(u)),
    };
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    // The password never appears in the address bar.
    let base = format!("{}://{user}{host}{port}/", url.scheme());
    let op = match url.scheme() {
        scheme @ ("ftp" | "ftps") => {
            let port = url.port_or_known_default().unwrap_or(21);
            let mut b = services::Ftp::default().endpoint(&format!("{scheme}://{host}:{port}")).root("/");
            if !url.username().is_empty() {
                b = b.user(&decode(url.username()));
            }
            if let Some(p) = url.password() {
                b = b.password(&decode(p));
            }
            Operator::new(b)
        }
        // ponytail: OpenDAL uses the openssh crate, which does not compile on Windows (apache/opendal#2963).
        // SFTP on Windows will have to be done with russh-sftp.
        #[cfg(unix)]
        "sftp" => Operator::new(services::Sftp::default().endpoint(&format!("ssh://{user}{host}{port}")).root("/")),
        s => return Err(("error-unsupported", s.to_string())),
    };
    Ok(Source { op: Store::Dal(op.map_err(|e| ("error-source", e.to_string()))?), base, dir, start })
}

fn local(path: &Path) -> Result<Source, Error> {
    let path = path.canonicalize().map_err(|e| ("error-path", format!("{}: {e}", path.display())))?;
    // "/" on Linux, "\\?\C:\" on Windows.
    let root = path.ancestors().last().unwrap_or(&path);
    let rel = path.strip_prefix(root).unwrap_or(&path);
    let rel: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let (dir, start) = split(&rel.join("/"));
    let op = Operator::new(services::Fs::default().root(&root.to_string_lossy()))
        .map_err(|e| ("error-source", e.to_string()))?;
    // ponytail: Windows' "\\?\" prefix is hidden only in the bar; it is really needed only for paths over 260 characters
    let base = root.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    Ok(Source { op: Store::Dal(op), base, dir, start })
}

impl Listing {
    // ponytail: plain alphabetical order, "img10" before "img2"; natural sort if it bothers anyone
    pub fn sort(&mut self) {
        self.dirs.sort_by_key(|n| n.to_lowercase());
        self.files.sort_by_key(|f| f.path.to_lowercase());
    }

    /// Images in the order chosen by the user; ties broken by name. Those without a date (or with
    /// zero size) go last, in descending order too.
    pub fn sort_files(&mut self, by: SortBy, desc: bool) {
        use std::cmp::Ordering;
        let name = |f: &File| f.path.to_lowercase();
        let ext = |f: &File| f.path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
        let date = |f: &File| crate::parse_modified(&f.modified);
        self.files.sort_by(|a, b| {
            let flip = |o: Ordering| if desc { o.reverse() } else { o };
            let primary = match by {
                SortBy::Name => flip(name(a).cmp(&name(b))),
                SortBy::Type => flip(ext(a).cmp(&ext(b))),
                SortBy::Size => (a.size == 0).cmp(&(b.size == 0)).then(flip(a.size.cmp(&b.size))),
                SortBy::Date => match (date(a), date(b)) {
                    (Some(x), Some(y)) => flip(x.cmp(&y)),
                    (x, y) => x.is_none().cmp(&y.is_none()),
                },
            };
            primary.then_with(|| name(a).cmp(&name(b)))
        });
    }
}

/// Subfolders (without hidden ones) and images of `dir`, sorted by name.
async fn list(op: &Operator, dir: &str) -> opendal::Result<Listing> {
    let mut listing = Listing::default();
    for e in op.list(if dir.is_empty() { "/" } else { dir }).await? {
        let path = e.path().trim_start_matches('/');
        if path == dir || e.name().starts_with('.') || hidden(op, path) {
            continue;
        }
        if e.metadata().is_dir() {
            // Symlinks to folders arrive without a trailing "/" (see Cargo.toml).
            let path = if path.ends_with('/') { path.to_string() } else { format!("{path}/") };
            listing.dirs.push(path);
        } else if is_image(e.name()) {
            let m = e.metadata();
            let modified = m.last_modified().map(|t| t.to_string()).unwrap_or_default();
            listing.files.push(File { path: path.to_string(), size: m.content_length(), modified });
        }
    }
    listing.sort();
    Ok(listing)
}

/// On Windows, local files with the "hidden" attribute, which File Explorer does not show. Among
/// them the compatibility junctions such as `Documents\My Pictures`, which deny listing to everyone.
#[cfg(windows)]
fn hidden(op: &Operator, path: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    op.info().scheme() == "fs"
        && std::fs::symlink_metadata(Path::new(&op.info().root()).join(path.trim_end_matches('/')))
            .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(windows))]
fn hidden(_: &Operator, _: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_files() {
        let f = |path: &str, size, modified: &str| File { path: path.into(), size, modified: modified.into() };
        let mut l = Listing {
            files: vec![
                f("b.png", 300, "1776333600"),
                f("A.jpg", 100, ""),
                f("c.jpg", 200, "2026-01-01T00:00:00Z"),
            ],
            ..Default::default()
        };
        let names = |l: &Listing| l.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        l.sort_files(SortBy::Name, false);
        assert_eq!(names(&l), ["A.jpg", "b.png", "c.jpg"]);
        l.sort_files(SortBy::Size, true);
        assert_eq!(names(&l), ["b.png", "c.jpg", "A.jpg"]);
        // Without a date last, in reverse order too.
        l.sort_files(SortBy::Date, false);
        assert_eq!(names(&l), ["c.jpg", "b.png", "A.jpg"]);
        l.sort_files(SortBy::Date, true);
        assert_eq!(names(&l), ["b.png", "c.jpg", "A.jpg"]);
        l.sort_files(SortBy::Type, false);
        assert_eq!(names(&l), ["A.jpg", "c.jpg", "b.png"]);
    }

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
        let l = open(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(l.dir.ends_with("eyecansee/") && l.start.is_none());
        assert!(in_gvfs("/", "run/user/1000/gvfs/mtp:host=X/DCIM/") && in_gvfs("/run/user/1000/gvfs/", "a.jpg"));
        assert!(!in_gvfs("/", "home/me/a.jpg"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_hidden() {
        let dir = std::env::temp_dir().join("eyecansee-hidden-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("visible")).unwrap();
        std::fs::create_dir_all(dir.join("hidden")).unwrap();
        let ok = std::process::Command::new("attrib").arg("+h").arg(dir.join("hidden")).status().unwrap();
        assert!(ok.success());
        let s = open(&dir.to_string_lossy()).unwrap();
        let Store::Dal(op) = &s.op else { unreachable!() };
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let l = rt.block_on(list(op, &s.dir)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(l.dirs, [format!("{}visible/", s.dir)]);
    }
}
