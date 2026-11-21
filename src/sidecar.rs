//! The `.xmp` file beside a photo, its "sidecar" (#24): where Lightroom, Camera Raw, darktable
//! and digiKam keep what the person wrote about the photo (title, keywords, rating, label,
//! author, copyright, position) and how they develop it, always for a RAW file, which they do not
//! write into, and for any photo with darktable and with digiKam when set so. Two names are in
//! use: `foo.CR2.xmp` (darktable, digiKam: one sidecar for each file, even with a `foo.JPG`
//! beside) and `foo.xmp` (Adobe, the XMP specification, digiKam "compatible with commercial
//! programs"). As digiKam does, the first is looked for first. `foo.xmp` is that of the file whose
//! extension its `photoshop:SidecarForExtension` names (Lightroom writes it); without it, of the
//! RAW photo with that name if there is one (Lightroom takes a JPEG shot with a RAW as part of
//! the RAW), otherwise of the photo. A RAW photo's sidecar counts instead of the XMP inside it,
//! as in Lightroom; another photo's adds to it, its own values counting first, as in digiKam
//! (`xmp::with_sidecar`).

use std::collections::HashSet;

use crate::source::Store;

/// A sidecar found: its file name and content.
pub struct Sidecar {
    pub name: String,
    pub xmp: Vec<u8>,
}

/// Larger than any sidecar seen (darktable's with a long history: some hundred KB): not one.
const MAX: u64 = 8 << 20;

/// The paths a photo's sidecar can have, in the order they are looked for (none for a file that
/// is not a photo): `foo.CR2.xmp` first, then `foo.xmp`, each with the extension in small and
/// capital letters.
pub fn names(path: &str) -> Vec<String> {
    if !crate::decode::is_image(path) || path.ends_with('/') {
        return Vec::new();
    }
    let stem = path.rsplit_once('.').map_or(path, |(s, _)| s);
    let mut out: Vec<String> = Vec::new();
    for base in [path, stem] {
        for ext in ["xmp", "XMP"] {
            let name = format!("{base}.{ext}");
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// The sidecars of the RAW photo named `own` among `names`, the names in its folder (no path),
/// in the order of `names`.
pub fn among(own: &str, taken: &HashSet<String>) -> Vec<String> {
    names(own).into_iter().filter(|n| taken.contains(n)).collect()
}

/// A name without its extension.
fn stem(own: &str) -> &str {
    own.rsplit_once('.').map_or(own, |(s, _)| s)
}

/// The name the sidecar `side` of the photo named `own` takes when the photo becomes `new`, with
/// the same pattern: `a.CR2.xmp` of `a.CR2` → `a (2).CR2.xmp`, `a.xmp` → `a (2).xmp`.
pub fn renamed(own: &str, side: &str, new: &str) -> String {
    if let Some(rest) = side.strip_prefix(own).filter(|r| r.starts_with('.')) {
        return format!("{new}{rest}");
    }
    match side.strip_prefix(stem(own)) {
        Some(rest) => format!("{}{rest}", stem(new)),
        None => side.to_string(),
    }
}

/// As `source::free_name` for a photo and its sidecars `sides`: `own` if it and the names its
/// sidecars would take are all free (`taken` says which are not), otherwise "own (2).ext" and so
/// on, the sidecars renamed with it ("a (2).CR2.xmp").
pub fn free_pair(taken: impl Fn(&str) -> bool, own: &str, sides: &[String]) -> String {
    let free = |n: &str| !taken(n) && sides.iter().all(|s| !taken(&renamed(own, s, n)));
    if free(own) {
        return own.to_string();
    }
    let ext = &own[stem(own).len()..];
    let stem = if stem(own).is_empty() { own } else { stem(own) };
    let ext = if stem == own { "" } else { ext };
    (2..).map(|n| format!("{stem} ({n}){ext}")).find(|n| free(n)).unwrap_or_default()
}

/// Whether a sidecar named `found` is that of `path` without looking inside: `foo.CR2.xmp` is
/// always that of `foo.CR2`; `None` for a `foo.xmp`, which may be that of another photo named
/// `foo` (`belongs`).
pub fn surely(path: &str, found: &str) -> Option<bool> {
    let (path, found) = (crate::source::name(path), crate::source::name(found));
    (found.len() > path.len() && found.starts_with(path)).then_some(true)
}

/// Whether the sidecar `found` (a `foo.xmp`, with `photoshop:SidecarForExtension` `for_ext`)
/// is that of `path`, `raw_beside` telling if a RAW photo of the same name is in its folder.
pub fn belongs(path: &str, found: &str, for_ext: Option<&str>, raw_beside: bool) -> bool {
    if let Some(sure) = surely(path, found) {
        return sure;
    }
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    match for_ext {
        Some(e) => e.eq_ignore_ascii_case(ext),
        None => crate::decode::is_raw(path) || !raw_beside,
    }
}

/// `photoshop:SidecarForExtension` of an XMP packet.
pub fn sidecar_for(xmp: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(xmp);
    let text = text.trim_start_matches(|c| c != '<').trim_end_matches(|c| c != '>');
    let doc = roxmltree::Document::parse(text).ok()?;
    let key = ("http://ns.adobe.com/photoshop/1.0/", "SidecarForExtension");
    doc.descendants().find_map(|n| n.attribute(key).map(str::to_string).or_else(|| n.has_tag_name(key).then(|| n.text().unwrap_or("").trim().to_string())))
}

/// Whether a RAW photo other than `path` has its name, without the extension, among `names`.
pub fn raw_beside(path: &str, names: &HashSet<String>) -> bool {
    let own = crate::source::name(path);
    names.iter().any(|n| n != own && stem(n) == stem(own) && crate::decode::is_raw(n))
}

/// Whether the sidecar `found` with content `xmp` is that of `path` (`belongs`); the names of its
/// folder are read only when they decide it (a `foo.xmp` naming no extension, beside a photo that
/// is not RAW), or taken from `names` when given.
pub async fn is_its(op: &Store, path: &str, found: &str, xmp: &[u8], names: Option<&HashSet<String>>) -> bool {
    if let Some(sure) = surely(path, found) {
        return sure;
    }
    let for_ext = sidecar_for(xmp);
    let beside = if for_ext.is_some() || crate::decode::is_raw(path) {
        false
    } else if let Some(names) = names {
        raw_beside(path, names)
    } else {
        raw_beside(path, &op.names(&crate::source::parent(path)).await.unwrap_or_default())
    };
    belongs(path, found, for_ext.as_deref(), beside)
}

/// The sidecars of `path` there are (`names`), with their size. Asked with `exists` first: on a
/// phone it answers from the folder's listing (the size of a missing file would list the folder
/// again, each time), on a server it is one request.
async fn found(op: &Store, path: &str) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for name in names(path) {
        if op.exists(&name).await
            && let Ok(size) = op.size(&name).await
        {
            out.push((name, size));
        }
    }
    out
}

/// The sidecar of `path`, if it has one that is its own (`is_its`).
pub async fn read(op: &Store, path: &str) -> Option<Sidecar> {
    for (name, size) in found(op, path).await {
        if size > MAX {
            continue;
        }
        let Ok(xmp) = op.read(&name).await else { continue };
        if is_its(op, path, &name, &xmp, None).await {
            return Some(Sidecar { name: crate::source::name(&name).to_string(), xmp });
        }
    }
    None
}

/// What the metadata saved for `path` depend on beside it: its sidecar's name, size and, for a
/// file on this computer, date ("" without one). A rating changed from 3 to 4 stars keeps the
/// size: on phones and servers it is seen when the folder is read again.
pub async fn key(op: &Store, path: &str) -> String {
    let Some((name, size)) = found(op, path).await.into_iter().next() else { return String::new() };
    let date = op
        .local_file(&name)
        .and_then(|f| std::fs::metadata(f).ok()?.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("xmp {} {size} {date}", crate::source::name(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sidecars follow the photo's name, each in its own pattern, and a free name is free
    /// for the photo and its sidecars together.
    #[test]
    fn names_that_follow() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<std::collections::HashSet<_>>();
        let folder = set(&["a.CR2", "a.CR2.xmp", "a.xmp", "a.JPG", "b.NEF", "ab.xmp", "a.jpg.xmp"]);
        assert_eq!(among("a.CR2", &folder), ["a.CR2.xmp", "a.xmp"]);
        assert!(among("b.NEF", &folder).is_empty());
        // By name only: whose `a.xmp` is, `is_its` says.
        assert_eq!(among("a.JPG", &folder), ["a.xmp"]);
        assert_eq!(among("a.jpg", &folder), ["a.jpg.xmp", "a.xmp"]);
        assert_eq!(renamed("a.CR2", "a.CR2.xmp", "a (2).CR2"), "a (2).CR2.xmp");
        assert_eq!(renamed("a.CR2", "a.xmp", "a (2).CR2"), "a (2).xmp");
        assert_eq!(renamed("a.CR2", "a.CR2.XMP", "sea.cr2"), "sea.cr2.XMP");
        assert_eq!(renamed("a.CR2", "a.XMP", "sea.cr2"), "sea.XMP");
        let sides = ["a.CR2.xmp".to_string(), "a.xmp".to_string()];
        let has = |names: &[&str]| {
            let names = set(names);
            move |n: &str| names.contains(n)
        };
        assert_eq!(free_pair(has(&["b.CR2"]), "a.CR2", &sides), "a.CR2");
        assert_eq!(free_pair(has(&["a.CR2"]), "a.CR2", &sides), "a (2).CR2");
        // The photo's name is free, its sidecar's is not: both change.
        assert_eq!(free_pair(has(&["a.xmp"]), "a.CR2", &sides), "a (2).CR2");
        assert_eq!(free_pair(has(&["a.CR2", "a (2).xmp", "a (3).CR2.xmp"]), "a.CR2", &sides), "a (4).CR2");
        assert_eq!(free_pair(has(&["raw"]), "raw", &[]), "raw (2)");
    }

    #[test]
    fn names_of_a_raw_photo() {
        assert_eq!(names("d/IMG_1.CR2"), ["d/IMG_1.CR2.xmp", "d/IMG_1.CR2.XMP", "d/IMG_1.xmp", "d/IMG_1.XMP"]);
        assert_eq!(names("d/IMG_1.jpg")[..2], ["d/IMG_1.jpg.xmp", "d/IMG_1.jpg.XMP"]);
        assert!(names("d/a.mp4").is_empty() && names("d/a.txt").is_empty() && names("d/").is_empty());
    }

    /// `foo.CR2.xmp` is always the RAW's; `foo.xmp` that of the extension it names, otherwise of
    /// the RAW photo of that name, otherwise of the photo.
    #[test]
    fn whose_sidecar() {
        let for_ext = |e: &str| format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" photoshop:SidecarForExtension="{e}"/></rdf:RDF></x:xmpmeta>"#);
        assert_eq!(sidecar_for(for_ext("cr2").as_bytes()).as_deref(), Some("cr2"));
        assert_eq!(sidecar_for(b"<x:xmpmeta/>"), None);
        assert!(belongs("d/a.CR2", "d/a.xmp", Some("cr2"), false));
        assert!(!belongs("d/a.CR2", "d/a.xmp", Some("JPG"), false));
        assert!(belongs("d/a.CR2", "d/a.CR2.xmp", Some("JPG"), false));
        assert!(belongs("d/a.CR2", "d/a.xmp", None, true));
        assert!(belongs("d/a.jpg", "d/a.xmp", None, false) && !belongs("d/a.jpg", "d/a.xmp", None, true));
        assert!(belongs("d/a.jpg", "d/a.jpg.xmp", None, true) && belongs("d/a.JPG", "d/a.xmp", Some("JPG"), true));
        let folder: HashSet<String> = ["a.jpg", "a.NEF", "b.png", "b.xmp"].map(String::from).into();
        assert!(raw_beside("d/a.jpg", &folder) && !raw_beside("d/b.png", &folder) && !raw_beside("d/a.NEF", &folder));
    }

    /// Looked for on a source: the darktable name wins, the key follows the sidecar.
    #[test]
    fn found_on_a_source() {
        let dir = std::env::temp_dir().join(format!("eyecansee-sidecar-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("d")).unwrap();
        let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&dir.to_string_lossy())).unwrap());
        let rt = tokio::runtime::Runtime::new().unwrap();
        std::fs::write(dir.join("d/a.NEF"), b"raw").unwrap();
        assert!(rt.block_on(read(&op, "d/a.NEF")).is_none());
        assert_eq!(rt.block_on(key(&op, "d/a.NEF")), "");
        std::fs::write(dir.join("d/a.xmp"), b"<x:xmpmeta/>").unwrap();
        assert_eq!(rt.block_on(read(&op, "d/a.NEF")).unwrap().name, "a.xmp");
        let lightroom = rt.block_on(key(&op, "d/a.NEF"));
        std::fs::write(dir.join("d/a.NEF.xmp"), b"<x:xmpmeta/> ").unwrap();
        let s = rt.block_on(read(&op, "d/a.NEF")).unwrap();
        assert_eq!((s.name.as_str(), s.xmp.len()), ("a.NEF.xmp", 13));
        let darktable = rt.block_on(key(&op, "d/a.NEF"));
        assert!(lightroom.starts_with("xmp a.xmp 12 ") && darktable.starts_with("xmp a.NEF.xmp 13 "), "{lightroom} {darktable}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
