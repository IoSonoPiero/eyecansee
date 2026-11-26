//! The hidden folder of a copy inside one phone folder (`COPY_DIR`, `Phone::copy_aside`): the copy
//! is made there and moved in with its new name. A copy broken off before the move (the cable
//! unplugged while the phone copied, P7 of 26/11/2026: the phone finished the 6 GB copy by itself)
//! left the whole file there, hidden, until the next copy into that folder. Now the next listing of
//! the folder deletes it, unless a copy of this process is using it: a phone talks to one program
//! at a time, so no other one can.

use std::collections::HashSet;
use std::sync::Mutex;

use super::COPY_DIR;

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
}
