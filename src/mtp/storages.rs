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

#[cfg(test)]
mod tests {
    use super::*;

    fn named(found: &[(&str, u64)]) -> Vec<String> {
        storage_names(&found.iter().map(|(d, id)| (d.to_string(), *id)).collect::<Vec<_>>())
    }

    /// Every name finds its own storage by the rule of `locate`, as a folder and further down;
    /// none is empty, none has a "/" that would cut its paths, no two are the same.
    fn reachable(names: &[String]) {
        let storages: Vec<(String, usize)> = names.iter().cloned().zip(0..).collect();
        for (i, name) in names.iter().enumerate() {
            assert!(!name.is_empty() && !name.contains('/'), "{name:?}");
            assert_eq!(find(&storages, &format!("{name}/")), Some((&i, "")), "{name:?}");
            assert_eq!(find(&storages, &format!("{name}/DCIM/Camera/a.jpg")), Some((&i, "DCIM/Camera/a.jpg")), "{name:?}");
        }
        let distinct: std::collections::HashSet<&String> = names.iter().collect();
        assert_eq!(distinct.len(), names.len(), "{names:?}");
    }

    /// A description of its own keeps exactly the name it always had (bookmarks, last folder and
    /// thumbnails hold it); trimmed; an empty one is the number of the storage.
    #[test]
    fn unique_descriptions_keep_their_names() {
        let names = named(&[("Internal shared storage", 0x10001), ("  SD card ", 0x20001), ("", 0x30001)]);
        assert_eq!(names, ["Internal shared storage", "SD card", "196609"]);
        reachable(&names);
        assert_eq!(named(&[("Memoria condivisa interna", 0x10001)]), ["Memoria condivisa interna"]);
    }

    /// Two storages with the same description: both get their number, in hex, as gvfs names
    /// them, so a phone mounted by the desktop has the same paths; the second opens too.
    #[test]
    fn same_description_twice() {
        let names = named(&[("Storage", 0x10001), ("Storage", 0x20001), ("Internal", 0x30001)]);
        assert_eq!(names, ["Storage (10001)", "Storage (20001)", "Internal"]);
        reachable(&names);
        // The same once trimmed, and the large numbers of the storages on Windows (WPD).
        let names = named(&[("Card", 0x8000_0000_0000_00a1), ("Card ", 0x8000_0000_0000_00b2)]);
        assert_eq!(names, ["Card (80000000000000A1)", "Card (80000000000000B2)"]);
        reachable(&names);
    }

    /// A "/" in a description would cut every path of that storage in two: it becomes "-".
    #[test]
    fn slash_in_description() {
        let names = named(&[("Internal", 0x10001), ("SD/USB", 0x20001)]);
        assert_eq!(names, ["Internal", "SD-USB"]);
        reachable(&names);
        // The same as another one only once the "/" is gone.
        let names = named(&[("SD/Card", 0x10001), ("SD-Card", 0x20001)]);
        assert_eq!(names, ["SD-Card (10001)", "SD-Card (20001)"]);
        reachable(&names);
    }
}
