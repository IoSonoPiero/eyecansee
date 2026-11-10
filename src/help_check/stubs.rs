//! Stub until guida-A/guida-C merge: stand-ins for `crate::help::{PAGES, IMAGES}` (guida-A) and
//! `crate::help_icons::icon_exists` (guida-C), so the checks compile on their own branch.
//! At the merge delete this file and, in `help_check.rs`, replace `mod stubs;` and its `use` with
//! `use crate::help::{IMAGES, PAGES};` and `use crate::help_icons::icon_exists;`.

/// Stub until guida-A merge: (language, page id, text) of every embedded page.
pub(super) const PAGES: &[(&str, &str, &str)] = &[];

/// Stub until guida-A merge: (image name, bytes) of every embedded image.
pub(super) const IMAGES: &[(&str, &[u8])] = &[];

/// Stub until guida-C merge: whether `icon://<name>` is a known icon.
pub(super) fn icon_exists(_name: &str) -> bool {
    true
}
