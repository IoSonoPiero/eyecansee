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

/// `path` deleted, moved or renamed: forgotten with the paths that start with it.
pub(super) fn forget(handles: &mut Handles, path: &str) {
    handles.retain(|p, _| !p.starts_with(path));
}

/// The children of `dir` remembered before they are read again whole (`Phone::names`).
pub(super) fn forget_children(handles: &mut Handles, dir: &str) {
    handles.retain(|p, _| !(p.starts_with(dir) && source::parent(p) == dir));
}

/// The children of `dir` a listing found (`Phone::list`): their paths and handles.
pub(super) fn listed(handles: &mut Handles, _dir: &str, entries: Vec<(String, ObjectHandle)>) {
    handles.extend(entries);
}

/// An object someone else added on the phone (an event: its folder is not known).
pub(super) fn added(_complete: &mut HashSet<String>) {}
