//! The lines of a folder's listing (MLSD or LIST) as items, with the links told apart.
//!
//! A link can hide in MLSD: the server describes the link itself (its size is the length of the
//! path it holds) and MLSD has no standard word for a link, so SFTPGo, on a Hetzner Storage Box,
//! says "Type=file;Size=1" for a link to "." (2026-11-21). Its LIST says it, with the letter Go
//! gives a link: "Lrwxrwxrwx" (`io/fs`, `FileMode::String`: "L: symbolic link"; Go's "l" is
//! something else, exclusive use), which the parser of `suppaftp` knew only as "l". So when MLSD
//! shows a small file, one LIST of the same folder tells which items are links (`mark_links`).

use std::time::SystemTime;

use suppaftp::list::{File, ListParser};

/// A file of MLSD up to this size may be a link (the length of a path, at most `PATH_MAX` on
/// Linux): then the folder is listed again with LIST (`may_hide_links`).
const LINK_SIZE: u64 = 4096;

/// An item of a folder, "." and ".." left out.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Item {
    pub name: String,
    pub dir: bool,
    /// A link: to a folder or to a file, which only the server can tell (`Server::entry_is_dir`).
    pub link: bool,
    /// Of a link, the length of what it holds.
    pub size: u64,
    pub modified: SystemTime,
}

impl From<File> for Item {
    fn from(f: File) -> Self {
        Item { name: f.name().to_string(), dir: f.is_directory(), link: f.is_symlink(), size: f.size() as u64, modified: f.modified() }
    }
}

/// A line of LIST as `suppaftp` reads it: a link with Go's letter ("Lrwxrwxrwx") as one with the
/// usual "l". Only an "L" followed by the nine letters of the permissions.
fn usual(line: &str) -> std::borrow::Cow<'_, str> {
    let perms = line.get(1..10).is_some_and(|p| p.chars().all(|c| "-rwxsStT".contains(c)));
    match line.strip_prefix('L') {
        Some(rest) if perms => format!("l{rest}").into(),
        _ => line.into(),
    }
}

/// A listed item: a line of MLSD, or of LIST (Unix or DOS). A line that cannot be read is
/// written in the log (it was dropped without a word). Not LIST as `File::from_str` reads it:
/// that ends with the reader of MLSD, which takes any text for a file ("total 12" too).
pub(super) fn parse(line: &str, mlsd: bool) -> Option<Item> {
    let f = match mlsd {
        true => ListParser::parse_mlsd(line).ok(),
        false => ListParser::parse_posix(&usual(line)).or_else(|_| ListParser::parse_dos(line)).ok(),
    };
    if f.is_none() {
        log!("ftp: listed line not understood, left out: {line:?}");
    }
    f.map(Item::from)
}

/// The items of a listing's text, "." and ".." left out.
pub(super) fn items(text: &str, mlsd: bool) -> Vec<Item> {
    let lines = text.lines().map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty());
    lines.filter_map(|l| parse(l, mlsd)).filter(|f| f.name != "." && f.name != "..").collect()
}

/// Whether a listing of MLSD may hold a link seen as a file: a file small enough to be one.
pub(super) fn may_hide_links(items: &[Item]) -> bool {
    items.iter().any(|f| !f.dir && !f.link && f.size <= LINK_SIZE)
}

/// The items of MLSD that the LIST of the same folder (`list`) says are links, marked so. An item
/// LIST does not show (or a LIST not understood) stays as MLSD gave it.
pub(super) fn mark_links(items: &mut [Item], list: &str) {
    let links: Vec<String> = self::items(list, false).into_iter().filter(|f| f.link).map(|f| f.name).collect();
    for f in items.iter_mut().filter(|f| !f.dir && links.contains(&f.name)) {
        log!("ftp: {:?} is a link (LIST), MLSD said a file", f.name);
        f.link = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The folder of the L17 test of 2026-11-21, as the Storage Box (SFTPGo 2.7.5) lists it.
    const MLSD: &str = "Type=file;Size=482493;Modify=20261121082501; uno.jpg\r\nType=file;Size=1;Modify=20261121082504; self\r\nType=file;Size=570040;Modify=20261121082501; due.jpg\r\n";
    const LIST: &str = "-rw-r--r-- 1 ftp ftp       482493 Nov 21 08:25 uno.jpg\r\nLrwxrwxrwx 1 ftp ftp            1 Nov 21 08:25 self\r\n-rw-r--r-- 1 ftp ftp       570040 Nov 21 08:25 due.jpg\r\n";

    /// Go's "L" is a link, with no "-> where" after the name; the usual "l" still is.
    #[test]
    fn go_letter_for_a_link() {
        let link = parse("Lrwxrwxrwx 1 ftp ftp            1 Nov 21 08:25 self", false).unwrap();
        assert!(link.link && !link.dir);
        assert_eq!((link.name.as_str(), link.size), ("self", 1));
        let usual = parse("lrwxrwxrwx    1 me       users           1 Oct 30 09:10 again -> /loop", false).unwrap();
        assert!(usual.link);
        assert_eq!(usual.name, "again");
        // A name that starts with "L" is not touched.
        let file = parse("-rw-r--r-- 1 ftp ftp 10 Nov 21 08:25 Lrwxrwxrwx", false).unwrap();
        assert!(!file.link && file.name == "Lrwxrwxrwx");
        assert_eq!(parse("Lxyz", false), None);
        assert_eq!(parse("total 12", false), None);
        let dos = parse("10-19-20  03:19PM       <DIR>          pub", false).unwrap();
        assert!(dos.dir && dos.name == "pub");
    }

    /// The link of the L17 test: MLSD says a file of 1 byte, LIST says a link, and it is one.
    #[test]
    fn link_hidden_in_mlsd() {
        let mut found = items(MLSD, true);
        assert!(found.iter().all(|f| !f.link && !f.dir), "MLSD alone does not tell it: {found:?}");
        assert!(may_hide_links(&found));
        mark_links(&mut found, LIST);
        let links: Vec<&str> = found.iter().filter(|f| f.link).map(|f| f.name.as_str()).collect();
        assert_eq!(links, ["self"]);
        assert_eq!(found.len(), 3);
    }

    /// Only big files and folders: no LIST asked for.
    #[test]
    fn no_list_for_photos_and_folders() {
        let found = items("Type=file;Size=482493;Modify=20261121082501; uno.jpg\r\nType=dir;Size=0;Modify=20261121082501; sub\r\n", true);
        assert!(!may_hide_links(&found));
        assert!(may_hide_links(&items("Type=file;Size=4096;Modify=20261121082501; a.xmp\r\n", true)));
    }
}
