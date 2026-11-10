//! A file of this computer replaced by a new version (#90) in about the time of a plain write:
//! the new file is written whole under its hidden name next to the old one, then takes its name
//! in one step (`rename` on Linux, `ReplaceFileW` on Windows). Until then the old file stays as it
//! was, and nothing is read again in the folder.
//!
//! Up to 0.75.0 the new content was written into the old file, which kept everything that is not
//! content; the same is kept here:
//! - a file that cannot be written (read-only, no permission, held by another program on
//!   Windows) fails the same way, before anything is written: it is opened for writing first;
//! - a link leads to the file it points to, which is the one replaced: the link stays a link;
//! - Linux: permissions, owner and group (where allowed), and the extended attributes, the ACL
//!   (`system.posix_acl_access`) and the SELinux label (`security.selinux`) among them, copied to
//!   the new file before it takes the name. Disks that have none of these (FAT, exFAT) refuse the
//!   copy, which is skipped;
//! - Windows: `ReplaceFileW` gives the new file the old one's attributes, creation time, ACL and
//!   other streams (`Zone.Identifier`);
//! - a file with more than one name (hard links), which a rename would split: the new file is
//!   written whole under the hidden name first, then copied into the old file in place, so every
//!   name sees the new content. Only that local copy can still fail halfway (a full disk), as every
//!   write did before 0.76.0; then the new file takes the name on its own, whole.

use std::fs::{File, Metadata, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::source::{self, Error, Store};

use super::Chunks;

/// `file`, the local path of `path` of `op`, replaced by the file of `chunks`.
pub(super) async fn write_over(op: &Store, path: &str, file: PathBuf, chunks: Chunks) -> Result<(), Error> {
    let real = real_file(&file);
    // `Store::recover` leaves the hidden file alone, also next to the file a link leads to.
    let saving: Vec<String> = [Some(path.to_string()), store_path(op, &real)].into_iter().flatten().collect();
    source::SAVING.lock().unwrap().extend(saving.iter().cloned());
    let r = tokio::task::spawn_blocking(move || replace(&file, &real, chunks)).await;
    let mut s = source::SAVING.lock().unwrap();
    for p in &saving {
        s.remove(p);
    }
    r.map_err(|e| ("error-detail", e.to_string()))?
}

/// Can the file be written? As `replace` asks before a paste over it: a read-only file is not
/// saved over by the editor either, the same on every path (renaming it aside worked, and the
/// original then could not be deleted on Windows).
pub(super) fn writable(file: &Path) -> Result<(), Error> {
    OpenOptions::new().write(true).open(file).map(drop).map_err(at(file, "open"))
}

/// The file a link leads to; any other file as it is.
fn real_file(file: &Path) -> PathBuf {
    match file.symlink_metadata() {
        Ok(m) if m.file_type().is_symlink() => file.canonicalize().unwrap_or_else(|_| file.to_path_buf()),
        _ => file.to_path_buf(),
    }
}

/// `real` as a path of `op`, when it is below its root.
fn store_path(op: &Store, real: &Path) -> Option<String> {
    let Store::Dal(dal) = op else { return None };
    let root = PathBuf::from(dal.info().root());
    let rel = real.strip_prefix(&root).ok()?;
    Some(rel.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// An error of the disk about `file`, as OpenDAL words it: the error bar says it plainly with
/// the image's name (`fault::recognise`).
fn at(file: &Path, what: &'static str) -> impl Fn(std::io::Error) -> Error {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    move |e| ("error-detail", format!("{what}, context: {{ path: {name} }} => {e}"))
}

/// The work of `write_over`, in a thread where waiting on the disk is fine. `file` is the path
/// asked for, `real` the file it leads to.
fn replace(file: &Path, real: &Path, mut chunks: Chunks) -> Result<(), Error> {
    let original = match OpenOptions::new().write(true).open(real) {
        Ok(f) => f,
        // Gone since it was seen, or a link that leads nowhere: a new file, written through the
        // link as before (the file it names is made).
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return write_new(file, &mut chunks, None),
        Err(e) => return Err(at(real, "open")(e)),
    };
    let meta = original.metadata().map_err(at(real, "stat"))?;
    let shared = links(&original, &meta) > 1;
    drop(original);
    let own = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = real.with_file_name(source::save_names(&own).0);
    write_new(&temp, &mut chunks, (!shared).then_some((real, &meta)))?;
    let placed = if shared { copy_in_place(&temp, real) } else { put_over(&temp, real) };
    if placed.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    placed.map_err(at(real, "replace"))
}

/// The file of `chunks` written as `to`, flushed to the disk (as OpenDAL's writer did: a full
/// disk or a network share that fails says so here at the latest) and, with `like`, given what
/// the file it replaces has (`keep`). Deleted at any failure.
fn write_new(to: &Path, chunks: &mut Chunks, like: Option<(&Path, &Metadata)>) -> Result<(), Error> {
    let disk = at(to, "write");
    let r = (|| {
        let mut f = new_file(to, like.is_some()).map_err(&disk)?;
        while let Some(chunk) = chunks.blocking_recv() {
            let chunk = chunk.map_err(|e| ("error-detail", e.to_string()))?;
            f.write_all(&chunk).map_err(&disk)?;
        }
        if let Some((real, meta)) = like {
            keep(&f, real, meta);
        }
        f.sync_all().map_err(&disk)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(to);
    }
    r
}

/// A file opened to be written from the start. One that will replace another (`private`) can be
/// read only by its owner on Linux until `keep` gives it the permissions of that file.
fn new_file(to: &Path, private: bool) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    o.open(to)
}

/// How many names the file has (hard links).
#[cfg(unix)]
fn links(_: &File, meta: &Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::nlink(meta)
}

#[cfg(windows)]
fn links(file: &File, _: &Metadata) -> u64 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
    // SAFETY: a plain struct filled by the call, on a handle that stays open meanwhile.
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) != 0 { info.nNumberOfLinks as u64 } else { 1 }
    }
}

/// The new file `f` given what the file `real` it replaces has: owner and group (only root can
/// give a file away; a group of the user's own is allowed), then the permissions (a change of
/// owner clears setuid), then the extended attributes. What the disk refuses is skipped.
#[cfg(unix)]
fn keep(f: &File, real: &Path, meta: &Metadata) {
    use std::os::unix::fs::MetadataExt;
    if let Ok(mine) = f.metadata()
        && (mine.uid(), mine.gid()) != (meta.uid(), meta.gid())
    {
        let r = std::os::unix::fs::fchown(f, Some(meta.uid()), Some(meta.gid()));
        if r.is_err() {
            let _ = std::os::unix::fs::fchown(f, None, Some(meta.gid()));
        }
    }
    let _ = f.set_permissions(meta.permissions());
    #[cfg(target_os = "linux")]
    copy_xattrs(real, f);
    #[cfg(not(target_os = "linux"))]
    let _ = real;
}

/// On Windows `ReplaceFileW` does it (`put_over`).
#[cfg(windows)]
fn keep(_: &File, _: &Path, _: &Metadata) {}

/// Every extended attribute of `from` set on `to`: user ones, the ACL, the SELinux label. One
/// that cannot be set (`trusted.*` without privileges, a disk without them) is skipped.
#[cfg(target_os = "linux")]
fn copy_xattrs(from: &Path, to: &File) {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let Ok(from) = CString::new(from.as_os_str().as_bytes()) else { return };
    // SAFETY: the buffers passed are as long as said; the names are C strings.
    let Some(list) = xattr_bytes(|buf, len| unsafe { libc::listxattr(from.as_ptr(), buf.cast(), len) }) else { return };
    for name in list.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let Ok(name) = CString::new(name) else { continue };
        let Some(value) = xattr_bytes(|buf, len| unsafe { libc::getxattr(from.as_ptr(), name.as_ptr(), buf.cast(), len) }) else { continue };
        let set = unsafe { libc::fsetxattr(to.as_raw_fd(), name.as_ptr(), value.as_ptr().cast(), value.len(), 0) };
        if set != 0 {
            log!("replace: attribute {name:?} not kept: {}", std::io::Error::last_os_error());
        }
    }
}

/// What a call of the `getxattr` family gives: its size asked first, then the bytes (asked again
/// if they grew in between).
#[cfg(target_os = "linux")]
fn xattr_bytes(call: impl Fn(*mut u8, usize) -> isize) -> Option<Vec<u8>> {
    for _ in 0..3 {
        let size = usize::try_from(call(std::ptr::null_mut(), 0)).ok()?;
        let mut buf = vec![0u8; size];
        if size == 0 {
            return Some(buf);
        }
        if let Ok(got) = usize::try_from(call(buf.as_mut_ptr(), size)) {
            buf.truncate(got);
            return Some(buf);
        }
    }
    None
}

/// The new file `temp` takes the name of `real` in one step; `real` is gone.
#[cfg(not(windows))]
fn put_over(temp: &Path, real: &Path) -> std::io::Result<()> {
    std::fs::rename(temp, real)
}

/// `ReplaceFileW`: the new file takes the name and what is not content (attributes, creation
/// time, ACL, other streams such as `Zone.Identifier`) of `real`, which waits as its hidden
/// ".name.eyecansee-old" until then and is deleted after. A file held open by another program
/// without sharing its deletion cannot be replaced: everything stays as it was and the error says
/// the file is in use (`fault`). Where the call is not supported (some disks or network shares)
/// a plain replace (`MoveFileExW`, what `std::fs::rename` does) follows.
#[cfg(windows)]
fn put_over(temp: &Path, real: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW};
    let wide = |p: &Path| p.as_os_str().encode_wide().chain([0]).collect::<Vec<u16>>();
    let own = real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let backup = real.with_file_name(source::save_names(&own).1);
    // An original left by a save cut short after its new version took the name: it goes now,
    // read-only too (0.76.0 left such a file hidden: renamed aside, it could not be deleted).
    if let Ok(m) = backup.symlink_metadata() {
        if m.permissions().readonly() {
            let mut p = m.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            p.set_readonly(false);
            std::fs::set_permissions(&backup, p)?;
        }
        std::fs::remove_file(&backup)?;
    }
    let (r, t, b) = (wide(real), wide(temp), wide(&backup));
    let flags = REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS;
    // SAFETY: three paths ending in a zero, alive for the call; the reserved pointers are null.
    let done = unsafe { ReplaceFileW(r.as_ptr(), t.as_ptr(), b.as_ptr(), flags, std::ptr::null(), std::ptr::null()) } != 0;
    if done {
        if let Err(e) = std::fs::remove_file(&backup) {
            log!("replace {real:?}: the original left as {backup:?}: {e}");
        }
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    log!("replace {real:?}: ReplaceFileW: {e}");
    // ERROR_UNABLE_TO_MOVE_REPLACEMENT_2: the original went to the backup name, the new file did
    // not take its name. The original gets it back.
    if real.symlink_metadata().is_err() && backup.symlink_metadata().is_ok() {
        std::fs::rename(&backup, real)?;
    }
    match e.raw_os_error() {
        // ERROR_INVALID_FUNCTION, ERROR_NOT_SUPPORTED, ERROR_INVALID_PARAMETER.
        Some(1 | 50 | 87) => std::fs::rename(temp, real),
        _ => Err(e),
    }
}

/// The whole new file `temp` copied into `real` in place, so that every name of `real` sees it;
/// `temp` deleted after. If the copy fails halfway, `temp` takes the name instead: the new
/// content is under it whole, and the other names keep the part copied.
fn copy_in_place(temp: &Path, real: &Path) -> std::io::Result<()> {
    let copied = (|| {
        let mut from = File::open(temp)?;
        let mut to = OpenOptions::new().write(true).truncate(true).open(real)?;
        std::io::copy(&mut from, &mut to)?;
        to.sync_all()
    })();
    match copied {
        Ok(()) => std::fs::remove_file(temp),
        Err(e) => {
            log!("replace {real:?} in place: {e}, the new file takes the name");
            std::fs::rename(temp, real).and(Err(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{whole, write_file, write_whole};
    use crate::source::{FOLDERS_READ, Store};
    use std::path::{Path, PathBuf};

    fn setup(tag: &str) -> (PathBuf, Store, tokio::runtime::Runtime) {
        let dir = std::env::temp_dir().join(format!("eyecansee-replace-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        (dir, op, tokio::runtime::Runtime::new().unwrap())
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    /// A file replaced keeps its permissions and extended attributes; the folder is not read;
    /// nothing hidden is left.
    #[cfg(target_os = "linux")]
    #[test]
    fn replaced_keeps_what_is_not_content() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let (dir, op, rt) = setup("keeps");
        let file = dir.join("a.jpg");
        std::fs::write(&file, b"old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        let c = |s: &str| std::ffi::CString::new(s).unwrap();
        let (path, attr) = (c(file.to_str().unwrap()), c("user.eyecansee"));
        // SAFETY: C strings and buffers of the length given.
        let xattrs = unsafe { libc::setxattr(path.as_ptr(), attr.as_ptr(), b"kept".as_ptr().cast(), 4, 0) } == 0;
        let ino = std::fs::metadata(&file).unwrap().ino();
        FOLDERS_READ.with(|n| n.set(0));
        rt.block_on(write_file(&op, "a.jpg", 3, whole(b"new".to_vec()), Some(true))).unwrap();
        assert_eq!(FOLDERS_READ.with(|n| n.get()), 0, "no folder read");
        let meta = std::fs::metadata(&file).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"new");
        assert_ne!(meta.ino(), ino, "a new file, renamed over the old one");
        assert_eq!(meta.mode() & 0o7777, 0o640);
        if xattrs {
            let mut buf = [0u8; 16];
            let n = unsafe { libc::getxattr(path.as_ptr(), attr.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
            assert_eq!(&buf[..n.max(0) as usize], b"kept");
        } else {
            println!("no user xattrs on {dir:?}: not checked");
        }
        assert_eq!(names(&dir), ["a.jpg"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A link stays a link and the file it leads to gets the new content; a file with two names
    /// is written in place, both names see it; a link that leads nowhere is written through.
    #[cfg(unix)]
    #[test]
    fn links_kept() {
        use std::os::unix::fs::MetadataExt;
        let (dir, op, rt) = setup("links");
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::write(dir.join("real/a.jpg"), b"old").unwrap();
        std::os::unix::fs::symlink(dir.join("real/a.jpg"), dir.join("link.jpg")).unwrap();
        rt.block_on(write_whole(&op, "link.jpg", b"new".to_vec())).unwrap();
        assert!(dir.join("link.jpg").symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(dir.join("real/a.jpg")).unwrap(), b"new");
        assert_eq!(names(&dir.join("real")), ["a.jpg"]);

        std::fs::hard_link(dir.join("real/a.jpg"), dir.join("real/b.jpg")).unwrap();
        let ino = std::fs::metadata(dir.join("real/a.jpg")).unwrap().ino();
        rt.block_on(write_whole(&op, "real/a.jpg", b"newer".to_vec())).unwrap();
        assert_eq!(std::fs::read(dir.join("real/b.jpg")).unwrap(), b"newer");
        assert_eq!(std::fs::metadata(dir.join("real/a.jpg")).unwrap().ino(), ino);
        assert_eq!(names(&dir.join("real")), ["a.jpg", "b.jpg"]);

        std::os::unix::fs::symlink(dir.join("real/gone.jpg"), dir.join("dangling.jpg")).unwrap();
        rt.block_on(write_file(&op, "dangling.jpg", 1, whole(b"x".to_vec()), Some(true))).unwrap();
        assert_eq!(std::fs::read(dir.join("real/gone.jpg")).unwrap(), b"x");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A file that cannot be written fails as before 0.76.0, before anything is written: it stays
    /// as it was, nothing hidden is left, and the error says it plainly with its name.
    #[cfg(unix)]
    #[test]
    fn read_only_stays() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, op, rt) = setup("read-only");
        let file = dir.join("a.jpg");
        std::fs::write(&file, b"old").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        // Not as root, who writes anyway.
        if std::fs::OpenOptions::new().write(true).open(&file).is_err() {
            let e = rt.block_on(write_whole(&op, "a.jpg", b"new".to_vec())).unwrap_err();
            assert_eq!(crate::fault::recognise(&e.1), Some((crate::fault::Kind::Denied, Some("a.jpg".into()))), "{e:?}");
            assert_eq!(std::fs::read(&file).unwrap(), b"old");
            assert_eq!(names(&dir), ["a.jpg"]);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A name so long that ".name.eyecansee-new" would not fit (disks take 255 bytes): replaced
    /// through a cut hidden name, nothing left.
    #[test]
    fn long_name_replaced() {
        let (dir, op, rt) = setup("long");
        let long = format!("{}.jpg", "é".repeat(124));
        assert_eq!(long.len(), 252);
        std::fs::write(dir.join(&long), b"old").unwrap();
        rt.block_on(write_whole(&op, &long, b"new".to_vec())).unwrap();
        assert_eq!(std::fs::read(dir.join(&long)).unwrap(), b"new");
        assert_eq!(names(&dir), [long]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
