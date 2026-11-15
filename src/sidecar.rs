//! The `.xmp` file beside a RAW photo, its "sidecar" (#24): where Lightroom, Camera Raw,
//! darktable and digiKam keep what the person wrote about the photo (title, keywords, rating,
//! label, author, copyright, position) and how they develop it, since they do not write into a
//! RAW file. Two names are in use: `foo.CR2.xmp` (darktable, digiKam: one sidecar for each file,
//! even with a `foo.JPG` beside) and `foo.xmp` (Adobe, the XMP specification, RawTherapee). As
//! digiKam does, the first is looked for first; `foo.xmp` is taken unless it says it belongs to a
//! file with another extension (`photoshop:SidecarForExtension`, written by Lightroom). As
//! Lightroom does, the sidecar counts instead of the XMP inside the RAW file. Only RAW photos:
//! Lightroom writes into the other files, and reads sidecars of RAW files only.

use crate::source::Store;

/// A sidecar found: its file name and content.
pub struct Sidecar {
    pub name: String,
    pub xmp: Vec<u8>,
}

/// Larger than any sidecar seen (darktable's with a long history: some hundred KB): not one.
const MAX: u64 = 8 << 20;

/// The paths a RAW photo's sidecar can have, in the order they are looked for (none for a file
/// that is not a RAW photo): `foo.CR2.xmp` first, then `foo.xmp`, each with the extension in
/// small and capital letters.
pub fn names(path: &str) -> Vec<String> {
    if !crate::decode::is_raw(path) {
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

/// Whether `xmp`, found at `found`, is the sidecar of `path`: a `foo.xmp` that names another
/// extension is that of the other file with the same name (a RAW and a JPEG taken together).
pub fn belongs(path: &str, found: &str, xmp: &[u8]) -> bool {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    if found.len() > path.len() && found.starts_with(path) {
        return true;
    }
    crate::xmp_fields::read(xmp).sidecar_for.is_none_or(|e| e.eq_ignore_ascii_case(ext))
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

/// The sidecar of `path`, if it has one that `belongs` to it.
pub async fn read(op: &Store, path: &str) -> Option<Sidecar> {
    for (name, size) in found(op, path).await {
        if size > MAX {
            continue;
        }
        let Ok(xmp) = op.read(&name).await else { continue };
        if belongs(path, &name, &xmp) {
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

    #[test]
    fn names_of_a_raw_photo() {
        assert_eq!(names("d/IMG_1.CR2"), ["d/IMG_1.CR2.xmp", "d/IMG_1.CR2.XMP", "d/IMG_1.xmp", "d/IMG_1.XMP"]);
        assert!(names("d/IMG_1.jpg").is_empty() && names("d/a.mp4").is_empty());
    }

    /// `foo.xmp` naming the JPEG beside is not the RAW's; `foo.CR2.xmp` always is.
    #[test]
    fn whose_sidecar() {
        let for_ext = |e: &str| format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" photoshop:SidecarForExtension="{e}"/></rdf:RDF></x:xmpmeta>"#);
        assert!(belongs("d/a.CR2", "d/a.xmp", for_ext("cr2").as_bytes()));
        assert!(!belongs("d/a.CR2", "d/a.xmp", for_ext("JPG").as_bytes()));
        assert!(belongs("d/a.CR2", "d/a.CR2.xmp", for_ext("JPG").as_bytes()));
        assert!(belongs("d/a.CR2", "d/a.xmp", b"<x:xmpmeta/>"));
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
