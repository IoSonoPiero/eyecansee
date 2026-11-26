//! The names of a phone's storages, the first part of every path on the phone
//! (`"Internal shared storage/DCIM/…"`), and the storage a path is in.

/// The name of each storage `found` on the phone, as (description, id), in the same order.
pub(super) fn storage_names(found: &[(String, u64)]) -> Vec<String> {
    found
        .iter()
        .map(|(description, id)| match description.trim() {
            "" => format!("{id}"),
            d => d.to_string(),
        })
        .collect()
}

/// The storage `path` is in, by its first part, and the rest of the path after it ("" for the
/// storage itself).
pub(super) fn find<'a, 'p, T>(storages: &'a [(String, T)], path: &'p str) -> Option<(&'a T, &'p str)> {
    let trimmed = path.trim_end_matches('/');
    let (first, rest) = trimmed.split_once('/').unwrap_or((trimmed, ""));
    storages.iter().find(|(name, _)| name == first).map(|(_, s)| (s, rest))
}
