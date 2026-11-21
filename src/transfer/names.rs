//! The names in a destination folder, compared as the destination compares them (#204): on
//! Windows, on FAT and exFAT, on phones "IMG_001.JPG" and "img_001.jpg" are one file, and a paste
//! that took the second for a free name wrote over the first without a question.

use std::collections::{HashMap, HashSet};

use crate::source::{Error, Store};

#[derive(Clone)]
pub(super) struct Names {
    exact: HashSet<String>,
    /// Each name without capitals, with the name as it is there.
    lower: HashMap<String, String>,
    /// The destination ignores capitals (`ignores_case`).
    folds: bool,
}

fn lower(n: &str) -> String {
    n.chars().flat_map(char::to_lowercase).collect()
}

impl Names {
    /// The names of `dir` of `to`, and whether it ignores capitals.
    pub(super) async fn read(to: &Store, dir: &str) -> Result<Names, Error> {
        let names = to.names(dir).await?;
        let folds = ignores_case(to, dir, &names).await;
        Ok(Names::new(names, folds))
    }

    pub(super) fn new(exact: HashSet<String>, folds: bool) -> Names {
        let lower = exact.iter().map(|n| (lower(n), n.clone())).collect();
        Names { exact, lower, folds }
    }

    /// The names of a folder about to be made, on the same destination.
    pub(super) fn empty(&self) -> Names {
        Names::new(HashSet::new(), self.folds)
    }

    /// The item that `n` names there, as the destination tells names: its name as it is there.
    pub(super) fn get<'a>(&'a self, n: &'a str) -> Option<&'a str> {
        match self.exact.contains(n) {
            true => Some(n),
            false if self.folds => self.lower.get(&lower(n)).map(String::as_str),
            false => None,
        }
    }

    pub(super) fn contains(&self, n: &str) -> bool {
        self.get(n).is_some()
    }

    /// Taken for a new name: also with other capitals, on any destination (as `source::free_name`).
    pub(super) fn taken(&self, n: &str) -> bool {
        self.exact.contains(n) || self.lower.contains_key(&lower(n))
    }

    pub(super) fn insert(&mut self, n: String) {
        self.lower.insert(lower(&n), n.clone());
        self.exact.insert(n);
    }
}

/// Whether `dir` of `to` takes a name with other capitals for the same item. Asked to the
/// destination itself, on this computer (Windows, FAT, exFAT, a folder of Linux that ignores
/// capitals) and on SFTP (a Windows server): one of its names with the capitals turned, not among
/// its names, is there all the same. Phones: yes, as Android's storage [Unverified on every
/// phone; one question more where it does not]. FTP: no (it can be asked only through the names,
/// as they are). A folder with no name to turn: no, and the new files of this computer are written
/// only onto a free name (`claim`).
async fn ignores_case(to: &Store, dir: &str, names: &HashSet<String>) -> bool {
    match to {
        Store::Mtp(_) => true,
        Store::Ftp(_) => false,
        Store::Dal(op) if op.info().scheme() != "fs" => false,
        _ => {
            let Some(turned) = names.iter().map(|n| turn(n)).find(|t| !names.contains(t)) else { return false };
            to.exists(&format!("{dir}{turned}")).await
        }
    }
}

/// `n` with its capitals turned (ASCII letters); a name without any stays the same.
fn turn(n: &str) -> String {
    n.chars().map(|c| if c.is_ascii_lowercase() { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where capitals do not count, a name with other capitals is the item there, under its own
    /// name; where they do, it is free for a paste. A new name is never one of them with other
    /// capitals.
    #[test]
    fn names_with_other_capitals() {
        let names = |folds| Names::new(["img_001.jpg", "Album"].map(String::from).into(), folds);
        let folding = names(true);
        assert_eq!(folding.get("IMG_001.JPG"), Some("img_001.jpg"));
        assert_eq!(folding.get("album"), Some("Album"));
        assert_eq!(folding.get("img_002.jpg"), None);
        let exact = names(false);
        assert_eq!(exact.get("IMG_001.JPG"), None);
        assert_eq!(exact.get("img_001.jpg"), Some("img_001.jpg"));
        assert!(exact.taken("IMG_001.JPG") && folding.taken("IMG_001.JPG") && !exact.taken("x.jpg"));
        let mut more = folding.empty();
        more.insert("A.JPG".into());
        assert_eq!(more.get("a.jpg"), Some("A.JPG"));
        assert_eq!(turn("IMG_001.jpg"), "img_001.JPG");
        assert_eq!(turn("123"), "123");
    }

    /// A folder of this computer: asked to the disk itself. A temporary folder of Linux tells
    /// capitals apart; on Windows (NTFS) it does not.
    #[test]
    fn asked_to_the_disk() {
        let dir = std::env::temp_dir().join(format!("eyecansee-capitals-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("img_001.jpg"), b"there").unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(dir.to_str().unwrap())).unwrap());
        let names = tokio::runtime::Runtime::new().unwrap().block_on(Names::read(&op, "")).unwrap();
        assert_eq!(names.folds, cfg!(windows));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
