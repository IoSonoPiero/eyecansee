//! The hidden folder of a copy inside one phone folder (`COPY_DIR`, `Phone::copy_aside`): the copy
//! is made there and moved in with its new name. A copy broken off before the move (the cable
//! unplugged while the phone copied, P7 of 26/11/2026: the phone finished the 6 GB copy by itself)
//! left the whole file there, hidden, until the next copy into that folder. Now the next listing of
//! the folder deletes it, unless a copy of this process is using it: a phone talks to one program
//! at a time, so no other one can.

use std::collections::HashSet;
use std::sync::Mutex;

use super::{Phone, COPY_DIR};
use crate::source::{self, Error};

/// The folders (`"<storage>/<folder>/"`) whose `COPY_DIR` a copy of this process is using.
static IN_USE: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// `dir`'s hidden folder in use while it lives.
pub(super) struct InUse(String);

impl InUse {
    pub(super) fn start(dir: &str) -> Self {
        IN_USE.lock().unwrap().get_or_insert_with(HashSet::new).insert(dir.to_string());
        InUse(dir.to_string())
    }
}

impl Drop for InUse {
    fn drop(&mut self) {
        if let Some(set) = IN_USE.lock().unwrap().as_mut() {
            set.remove(&self.0);
        }
    }
}

impl Phone {
    /// Whether a copy of `path` into `dir` goes through the hidden folder (`copy_aside`): into
    /// its own folder (Android does not copy a file there), or onto a name of `dir` that differs
    /// only by case. A folder read whole answers from what the phone remembers, as `taken`
    /// does: it was listed once for each file (audit 2.26).
    pub(super) async fn clash(&self, path: &str, dir: &str) -> Result<bool, Error> {
        if dir == source::parent(path) {
            return Ok(true);
        }
        if !self.complete.lock().unwrap().contains(dir) {
            self.names(dir).await?;
        }
        Ok(super::cache::same_name(&self.handles.lock().unwrap(), dir, source::name(path)))
    }
}

/// The hidden folder of `dir` to delete: there among `names` (the folder's items) and no copy of
/// this process using it.
pub(super) fn left_behind<'a>(dir: &str, mut names: impl Iterator<Item = &'a str>) -> Option<String> {
    let in_use = IN_USE.lock().unwrap().as_ref().is_some_and(|s| s.contains(dir));
    (!in_use && names.any(|n| n == COPY_DIR)).then(|| format!("{dir}{COPY_DIR}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hidden folder left by a copy broken off goes at the next listing; one a copy is using
    /// stays; a folder without one has nothing to delete.
    #[test]
    fn hidden_copy_folder_left_behind() {
        let dir = "Internal storage/Aside test/";
        let names = || [".EyeCanSee-copy", "a.jpg"].into_iter();
        assert_eq!(left_behind(dir, names()).as_deref(), Some("Internal storage/Aside test/.EyeCanSee-copy/"));
        {
            let _copying = InUse::start(dir);
            assert_eq!(left_behind(dir, names()), None, "a copy is using it");
        }
        assert!(left_behind(dir, names()).is_some(), "the copy ended");
        assert_eq!(left_behind(dir, ["a.jpg"].into_iter()), None);
    }

    /// Audit 2.26: a copy into another folder of the phone listed that folder whole for every
    /// file, to find a name that differs only by case, though `taken` had just read it whole
    /// (500 photos into a folder of 5,000 files: 500 listings of 5,000 objects). A folder read
    /// whole answers from what the phone remembers: no listing, no phone (none here).
    #[test]
    fn audit_2_26_case_clash_from_a_folder_read_whole() {
        use mtp_rs::mtp::ObjectHandle;
        let phone = Phone::new("eyecansee-test-no-phone");
        let known = ["S/DCIM/a.jpg", "S/DCIM/b.jpg", "S/Pictures/", "S/Pictures/B.JPG", "S/Pictures/old/"];
        phone.handles.lock().unwrap().extend(known.iter().enumerate().map(|(i, p)| (p.to_string(), ObjectHandle(i as u64 + 1))));
        phone.complete.lock().unwrap().insert("S/Pictures/".to_string());
        super::super::cache::LISTED.with(|n| n.set(0));
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let clash = |path: &str, dir: &str| rt.block_on(phone.clash(path, dir));
        let found = [clash("S/DCIM/a.jpg", "S/Pictures/"), clash("S/DCIM/b.jpg", "S/Pictures/"), clash("S/DCIM/a.jpg", "S/DCIM/")];
        assert_eq!(found, [Ok(false), Ok(true), Ok(true)], "free; B.JPG there; its own folder");
        assert_eq!(super::super::cache::LISTED.with(|n| n.get()), 0, "folders listed");
    }
}
