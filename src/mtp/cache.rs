//! What a phone remembers of its folders: path → handle (`Phone::handles`, folders end in "/")
//! and the folders whose every child is there (`Phone::complete`). Plain functions over the map
//! and the set, without the phone, so that they can be tested.

use std::collections::{HashMap, HashSet};

use mtp_rs::mtp::ObjectHandle;

use crate::source;

pub(super) type Handles = HashMap<String, ObjectHandle>;

/// The path of `name` in `dir`.
pub(super) fn child(dir: &str, name: &str, folder: bool) -> String {
    format!("{dir}{name}{}", if folder { "/" } else { "" })
}

/// `path` deleted, moved or renamed: forgotten, a folder with the paths below it. A file only
/// by its exact path: "a.jpg.xmp" starts with "a.jpg" and is still there (audit 2.12).
pub(super) fn forget(handles: &mut Handles, path: &str) {
    handles.retain(|p, _| if path.ends_with('/') { !p.starts_with(path) } else { p != path });
}

/// The children of `dir` remembered before they are read again whole (`Phone::names`).
pub(super) fn forget_children(handles: &mut Handles, dir: &str) {
    handles.retain(|p, _| !(p.starts_with(dir) && source::parent(p) == dir));
}

/// The children of `dir` a listing found (`Phone::list`): they take the place of those
/// remembered, which may be gone meanwhile.
pub(super) fn listed(handles: &mut Handles, dir: &str, entries: Vec<(String, ObjectHandle)>) {
    forget_children(handles, dir);
    handles.extend(entries);
}

/// An object someone else added on the phone: its folder may have a new name. The event has
/// only the handle, which no listing has seen yet, so no folder stays read whole; the next
/// `Phone::taken` lists its folder once.
pub(super) fn added(complete: &mut HashSet<String>) {
    complete.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder read whole: its files, a sidecar next to one, a subfolder with a file in it.
    fn camera() -> Handles {
        ["S/DCIM/", "S/DCIM/a.jpg", "S/DCIM/a.jpg.xmp", "S/DCIM/b.jpg", "S/DCIM/old/", "S/DCIM/old/c.jpg"]
            .iter()
            .enumerate()
            .map(|(i, p)| (p.to_string(), ObjectHandle(i as u64 + 1)))
            .collect()
    }

    fn paths(handles: &Handles) -> Vec<&str> {
        let mut p: Vec<&str> = handles.keys().map(String::as_str).collect();
        p.sort();
        p
    }

    /// Audit 2.12: deleting, moving or renaming "a.jpg" forgot "a.jpg.xmp" too, which is still
    /// on the phone; its folder stayed read whole, so the name was then taken for free.
    #[test]
    fn a_file_forgets_only_itself() {
        let mut handles = camera();
        forget(&mut handles, "S/DCIM/a.jpg");
        assert_eq!(paths(&handles), ["S/DCIM/", "S/DCIM/a.jpg.xmp", "S/DCIM/b.jpg", "S/DCIM/old/", "S/DCIM/old/c.jpg"]);
    }

    #[test]
    fn a_folder_forgets_what_is_below_it() {
        let mut handles = camera();
        forget(&mut handles, "S/DCIM/old/");
        assert_eq!(paths(&handles), ["S/DCIM/", "S/DCIM/a.jpg", "S/DCIM/a.jpg.xmp", "S/DCIM/b.jpg"]);
    }

    /// Audit 2.12: a listing added the children it found and kept the ones gone meanwhile (deleted
    /// on the phone, say): the name stayed taken in a folder read whole.
    #[test]
    fn a_listing_replaces_the_children_of_its_folder() {
        let mut handles = camera();
        let found = vec![("S/DCIM/a.jpg".to_string(), ObjectHandle(2)), ("S/DCIM/d.jpg".to_string(), ObjectHandle(9)), ("S/DCIM/old/".to_string(), ObjectHandle(5))];
        listed(&mut handles, "S/DCIM/", found);
        // What is below a subfolder is not a child of the folder listed: it stays.
        assert_eq!(paths(&handles), ["S/DCIM/", "S/DCIM/a.jpg", "S/DCIM/d.jpg", "S/DCIM/old/", "S/DCIM/old/c.jpg"]);
        assert_eq!(handles["S/DCIM/d.jpg"], ObjectHandle(9));
    }

    /// Audit 2.12: a photo taken on the phone left its folder read whole, so its name was taken
    /// for free by a copy. The event does not say the folder: none stays read whole.
    #[test]
    fn an_added_object_leaves_no_folder_read_whole() {
        let mut complete = HashSet::from(["S/DCIM/".to_string(), "S/Pictures/".to_string()]);
        added(&mut complete);
        assert!(complete.is_empty());
    }
}
