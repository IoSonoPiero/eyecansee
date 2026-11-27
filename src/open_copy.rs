//! The file of a picture open from a phone or a server, kept on this computer while it is open
//! (Piero, 23/11/2026: a picture of 213 MB on a server was downloaded again at the first save,
//! after it had been downloaded to open it). The saves of the editor and of the viewer read it
//! from here; nothing else does, so a picture opened again is still read from its source.
//! Choices (Fable, 23/11/2026, `notes/bozze/issue-editor-riscarica-originale.md`): only phones,
//! servers and gvfs mounts, never this computer's disks; in the cache folder, readable only by
//! the user, under names that say nothing; checked with SHA-256 when read back; dropped when
//! EyeCanSee writes to that file, when newer pictures push it out, at exit and at the next start.
//! Not used once the file's size is another (it changed on the phone or the server, by another
//! program) or the phone or server it came from is gone (audit E.6).

use std::any::Any;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use crate::source::{Error, Store};

/// The copies, newest last: the picture shown and its neighbours (`AROUND` on each side), and
/// one more.
const KEPT: usize = 2 * crate::AROUND + 2;

struct Copy {
    key: String,
    file: PathBuf,
    sha: [u8; 32],
    /// The size of the file it was read from.
    len: u64,
    /// The phone or server of the file: while it is held, no other one takes its address (the
    /// key), and once it is gone the copy is not used.
    owner: Option<Weak<dyn Any + Send + Sync>>,
}

static COPIES: Mutex<Vec<Copy>> = Mutex::new(Vec::new());

/// Which copy is `path` of `op`: `None` on this computer's disks, where reading again costs
/// nothing. A phone or a server is the one object EyeCanSee keeps for it (`Server::shared`,
/// `Phone::shared`) while it is in use: one that comes back new has no copy, and is read again.
fn key(op: &Store, path: &str) -> Option<String> {
    let who = match op {
        Store::Dal(dal) if dal.info().scheme() == "fs" && !op.gvfs(path) => return None,
        Store::Dal(dal) => format!("{}:{}{}", dal.info().scheme(), dal.info().name(), dal.info().root()),
        Store::Mtp(phone) => format!("mtp:{:p}", std::sync::Arc::as_ptr(phone)),
        Store::Sftp(server) => format!("sftp:{:p}", std::sync::Arc::as_ptr(server)),
        Store::Ftp(server) => format!("ftp:{:p}", std::sync::Arc::as_ptr(server)),
    };
    Some(format!("{who}\0{path}"))
}

/// The phone or server of `op`, held weakly.
fn owner(op: &Store) -> Option<Weak<dyn Any + Send + Sync>> {
    let held: Arc<dyn Any + Send + Sync> = match op {
        Store::Dal(_) => return None,
        Store::Mtp(phone) => phone.clone(),
        Store::Sftp(server) => server.clone(),
        Store::Ftp(server) => server.clone(),
    };
    Some(Arc::downgrade(&held))
}

fn sha256(data: &[u8]) -> [u8; 32] {
    ring::digest::digest(&ring::digest::SHA256, data).as_ref().try_into().expect("32 bytes")
}

/// The folder of the copies, inside the cache folder; only the user can open it.
fn folder() -> Option<PathBuf> {
    let dir = crate::thumbcache::cache_dir()?.join("open");
    std::fs::create_dir_all(&dir).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Some(dir)
}

/// The viewer read `bytes` of `path` whole: kept, in place of an older copy of it. Nothing is
/// said when it cannot be written (a full disk): the save reads from the source as before.
pub(crate) fn keep(op: &Store, path: &str, bytes: &[u8]) {
    let Some(key) = key(op, path) else { return };
    forget_key(&key);
    let Some(dir) = folder() else { return };
    // ponytail: written before the picture is decoded (about 0.2 s for 200 MB on a disk of today).
    let made = (|| {
        let n = std::hash::BuildHasher::hash_one(&std::hash::RandomState::new(), &key);
        let file = dir.join(format!("{}-{n:016x}", std::process::id()));
        let mut f = crate::temp_file::new_private(&file)?;
        std::io::Write::write_all(&mut f, bytes).inspect_err(|_| {
            let _ = std::fs::remove_file(&file);
        })?;
        Ok::<_, std::io::Error>(file)
    })();
    match made {
        Ok(file) => {
            let mut copies = COPIES.lock().unwrap();
            copies.push(Copy { key, file, sha: sha256(bytes), len: bytes.len() as u64, owner: owner(op) });
            while copies.len() > KEPT {
                let _ = std::fs::remove_file(copies.remove(0).file);
            }
        }
        Err(e) => log!("open copy of {path:?} not kept: {e}"),
    }
}

/// The bytes of `path`: from the copy kept on this computer when it is there and whole, else from
/// its source.
pub(crate) async fn read(op: &Store, path: &str) -> Result<Vec<u8>, Error> {
    if let Some(bytes) = kept(op, path).await {
        return Ok(bytes);
    }
    op.read(path).await
}

/// The copy of `path`, if kept, still what was written, and of the size the file has now on its
/// phone or server, which is asked (else dropped).
pub(crate) async fn kept(op: &Store, path: &str) -> Option<Vec<u8>> {
    let key = key(op, path)?;
    let (file, sha, len, gone) = {
        let copies = COPIES.lock().unwrap();
        let c = copies.iter().find(|c| c.key == key)?;
        (c.file.clone(), c.sha, c.len, c.owner.as_ref().is_some_and(|o| o.strong_count() == 0))
    };
    let now = if gone { None } else { op.size(path).await.ok() };
    match std::fs::read(&file) {
        Ok(bytes) if now == Some(len) && sha256(&bytes) == sha => {
            log!("{path:?} read from its open copy on this computer");
            Some(bytes)
        }
        _ => {
            log!("open copy of {path:?} not used: the file is not the one read (size {now:?}, was {len})");
            forget_key(&key);
            None
        }
    }
}

/// EyeCanSee writes to `path` (a save, a turn, a rename, a move): its copy is no longer the file.
pub(crate) fn forget(op: &Store, path: &str) {
    if let Some(key) = key(op, path) {
        forget_key(&key);
    }
}

fn forget_key(key: &str) {
    let mut copies = COPIES.lock().unwrap();
    copies.retain(|c| {
        let gone = c.key == key;
        if gone {
            let _ = std::fs::remove_file(&c.file);
        }
        !gone
    });
}

/// At exit, and at start (after `single::start`, not with `EYECANSEE_MULTI`): no copy stays.
pub(crate) fn forget_all() {
    COPIES.lock().unwrap().clear();
    if let Some(dir) = folder()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}
