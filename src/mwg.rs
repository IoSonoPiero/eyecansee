//! The description of a photo for the metadata panel's Description box, read from the three
//! places it can be written (EXIF ImageDescription, IPTC Caption-Abstract 2:120, XMP
//! dc:description) as the Metadata Working Group's "Guidelines for Handling Image Metadata" 2.0
//! (2010) ask a reader to: section 4.2.3 (reconciling EXIF, IPTC-IIM and XMP), 5.2 (Description)
//! and the text encodings of 4.2.4. exiftool's `-mwg -Description` (`MWG.pm`) follows the same
//! rules, but when an application unaware of XMP changed the IPTC it still shows the EXIF; the
//! guidelines show the IPTC then, and so does this module.

use crate::meta_write::{JPEG_PHOTOSHOP, digest_current, resource_data};

/// Photoshop resource with the MD5 of the IPTC-IIM, written by the applications that write the
/// XMP too: a digest that no longer matches says the IPTC was changed by one that does not.
const IPTC_DIGEST: u16 = 1061;
/// Length limit of the IPTC caption (IIM 2:120), in bytes.
const IPTC_LIMIT: usize = 2000;

/// What cameras and encoders write in ImageDescription by themselves, not a person: shown as no
/// description, as gThumb ("stupid comment filter") and KPhotoAlbum do.
const BOILERPLATE: [&str; 8] = [
    "OLYMPUS DIGITAL CAMERA",
    "SONY DSC",
    "KONICA MINOLTA DIGITAL CAMERA",
    "MINOLTA DIGITAL CAMERA",
    "JENOPTIK DIGITAL CAMERA",
    "Exif_JPEG_PICTURE",
    "EPSON DSC Picture",
    "DCF 1.0",
];

/// The description of a photo from the bytes of its EXIF ImageDescription (`exif`), its
/// Photoshop block with the IPTC and its digest (`irb`) and its XMP dc:description (`xmp`,
/// the "x-default" language): EXIF first, then XMP, then IPTC; but the IPTC first when its
/// digest no longer matches and its caption is not what the XMP would have become in it
/// (shortened to 2000 bytes, characters that do not fit). A text of only spaces or NULs, or
/// one a camera writes by itself (`BOILERPLATE`), counts as none.
pub fn description(exif: Option<&[u8]>, irb: Option<&[u8]>, xmp: Option<&str>) -> Option<String> {
    let exif = exif.map(|e| decode(e, Charset::Unknown)).filter(|e| present(e));
    let iim = irb.and_then(crate::meta_write::iim);
    let iptc = iim.and_then(caption).filter(|c| present(&c.text));
    let xmp = xmp.map(|x| x.trim().to_string()).filter(|x| present(x));
    let changed = iim.zip(irb.and_then(|b| resource_data(b, IPTC_DIGEST))).is_some_and(|(i, d)| !digest_current(i, d));
    let edited = changed && iptc.as_ref().is_some_and(|c| xmp.as_deref().is_none_or(|x| !predicted(x, c)));
    if edited {
        return iptc.map(|c| c.text);
    }
    exif.or(xmp).or(iptc.map(|c| c.text))
}

/// The Photoshop block of a JPEG (its APP13 segments), a TIFF (tags 34377 and 33723) or a PSD,
/// where the guidelines read the IPTC from (section 4.2.3.4); `None` for the other formats.
pub fn photoshop_block(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        // A large block goes on in the next segment.
        let block: Vec<u8> = crate::ultrahdr::segments(bytes)
            .into_iter()
            .filter_map(|(m, s, e)| bytes[s + 4..e].strip_prefix(JPEG_PHOTOSHOP).filter(|_| m == 0xED))
            .flatten()
            .copied()
            .collect();
        return (!block.is_empty()).then_some(block);
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return crate::meta_write::tiff_irb(bytes);
    }
    crate::psd::metadata(bytes)?.irb
}

/// The bytes of a text that exist: neither only spaces and NULs nor `BOILERPLATE`.
fn present(text: &str) -> bool {
    let t = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    !t.is_empty() && !BOILERPLATE.iter().any(|b| b.eq_ignore_ascii_case(t)) && !libav(t)
}

/// "Lavc57.107.100": the version of FFmpeg's encoder, written in the place of a description.
fn libav(t: &str) -> bool {
    t.strip_prefix("Lavc").or_else(|| t.strip_prefix("Lavf")).is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Charset {
    Utf8,
    Latin1,
    /// Not said: ASCII or UTF-8 if the bytes are valid UTF-8, otherwise Windows-1252 (guidelines,
    /// "Text encodings": "some reasonable fallback"; exiftool's too).
    Unknown,
}

/// A text of 8-bit characters, without a UTF-8 byte order mark, NULs and spaces around it.
fn decode(raw: &[u8], charset: Charset) -> String {
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(raw);
    let text = match (charset, std::str::from_utf8(raw)) {
        (Charset::Latin1, _) => raw.iter().map(|&b| char::from(b)).collect(),
        (Charset::Utf8, _) => String::from_utf8_lossy(raw).into_owned(),
        (Charset::Unknown, Ok(s)) => s.to_string(),
        (Charset::Unknown, Err(_)) => raw.iter().map(|&b| windows_1252(b)).collect(),
    };
    text.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string()
}

/// The characters Windows-1252 puts at 0x80 to 0x9F, where ISO 8859-1 has control codes (the
/// five unused places stay those codes, as Windows decodes them).
const CP1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', //
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

fn windows_1252(b: u8) -> char {
    if (0x80..0xA0).contains(&b) { CP1252[usize::from(b - 0x80)] } else { char::from(b) }
}

/// The IPTC caption, as the reader of its bytes saw it.
#[derive(Debug)]
struct Caption {
    text: String,
    /// Written in UTF-8 (said by 1:90, or valid UTF-8 when nothing is said).
    utf8: bool,
}

/// The caption (2:120) of an IPTC-IIM block, in the character set its 1:90 dataset names.
fn caption(iim: &[u8]) -> Option<Caption> {
    let mut charset = Charset::Unknown;
    let mut i = 0;
    while let Some(&[0x1C, record, dataset, a, b]) = iim.get(i..i + 5) {
        let mut len = usize::from(u16::from_be_bytes([a, b]));
        let mut start = i + 5;
        // An extended dataset: the length is in the next `len & 0x7FFF` bytes.
        if len & 0x8000 != 0 {
            let n = len & 0x7FFF;
            let size = iim.get(start..start + n).filter(|_| n <= 8)?;
            len = size.iter().fold(0usize, |l, &b| l << 8 | usize::from(b));
            start += n;
        }
        let data = iim.get(start..start.checked_add(len)?)?;
        i = start + len;
        match (record, dataset) {
            (1, 90) => charset = coded_charset(data),
            (2, 120) => {
                let utf8 = charset == Charset::Utf8 || (charset == Charset::Unknown && std::str::from_utf8(data).is_ok());
                return Some(Caption { text: decode(data, charset), utf8 });
            }
            _ => {}
        }
    }
    None
}

/// The character set an IIM 1:90 dataset names (ISO 2022 escape sequences): UTF-8 (ESC % G),
/// ISO 8859-1 put in one of the four places (ESC , A to ESC / A); any other as not said.
fn coded_charset(data: &[u8]) -> Charset {
    match data {
        b"\x1B%G" | b"\x1B%/G" | b"\x1B%/I" => Charset::Utf8,
        [0x1B, b',' | b'-' | b'.' | b'/', b'A'] => Charset::Latin1,
        _ => Charset::Unknown,
    }
}

/// Whether the IPTC caption is what an application writing both would have made of `xmp`: the
/// same text, characters that 8 bits cannot hold as "?", cut at 2000 bytes (section 4.2.3.2,
/// scenario 4: the IPTC changed, but not this field).
fn predicted(xmp: &str, iptc: &Caption) -> bool {
    let fits = |c: char| u32::from(c) <= 0xFF || CP1252.contains(&c);
    let text: String = if iptc.utf8 { xmp.to_string() } else { xmp.chars().map(|c| if fits(c) { c } else { '?' }).collect() };
    let bytes = |s: &str| if iptc.utf8 { s.len() } else { s.chars().count() };
    // Cut at the limit, or a little before it, at the end of a UTF-8 character.
    text == iptc.text || (bytes(&text) > IPTC_LIMIT && bytes(&iptc.text) + 4 > IPTC_LIMIT && text.starts_with(&iptc.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_write::{irb_of_iim, resource};

    /// An IPTC-IIM block: the 1:90 dataset if `charset`, then the caption's bytes.
    fn iim_of(charset: Option<&[u8]>, caption: &[u8]) -> Vec<u8> {
        let set = charset.map(|c| [&[0x1C, 1, 90][..], &(c.len() as u16).to_be_bytes(), c].concat()).unwrap_or_default();
        [set, vec![0x1C, 2, 120], (caption.len() as u16).to_be_bytes().to_vec(), caption.to_vec()].concat()
    }

    /// A Photoshop block with `iim` and a digest: that of `iim` (`Some(true)`), another one
    /// (`Some(false)`, the IPTC changed since the XMP was written) or none.
    fn block(iim: &[u8], digest: Option<bool>) -> Vec<u8> {
        let digest = match digest {
            Some(true) => resource(IPTC_DIGEST, &md5::compute(iim).0),
            Some(false) => resource(IPTC_DIGEST, &[7; 16]),
            None => Vec::new(),
        };
        [irb_of_iim(iim), digest].concat()
    }

    const UTF8: &[u8] = b"\x1B%G";

    #[test]
    fn one_place_only() {
        assert_eq!(description(Some(b"From EXIF\0"), None, None).as_deref(), Some("From EXIF"));
        let iptc = block(&iim_of(None, b"From IPTC"), None);
        assert_eq!(description(None, Some(&iptc), None).as_deref(), Some("From IPTC"));
        assert_eq!(description(None, None, Some(" From XMP ")).as_deref(), Some("From XMP"));
        assert_eq!(description(None, None, None), None);
    }

    /// EXIF first, then XMP, then IPTC, while the digest matches or there is none.
    #[test]
    fn exif_then_xmp_then_iptc() {
        for digest in [Some(true), None] {
            let iptc = block(&iim_of(Some(UTF8), b"IPTC"), digest);
            assert_eq!(description(Some(b"EXIF"), Some(&iptc), Some("XMP")).as_deref(), Some("EXIF"), "{digest:?}");
            assert_eq!(description(None, Some(&iptc), Some("XMP")).as_deref(), Some("XMP"), "{digest:?}");
            assert_eq!(description(None, Some(&iptc), None).as_deref(), Some("IPTC"), "{digest:?}");
        }
    }

    /// A digest that no longer matches: the IPTC was changed by an application unaware of XMP,
    /// its caption counts over the XMP and the EXIF; unless it is the XMP's own, shortened.
    #[test]
    fn iptc_changed() {
        let iptc = block(&iim_of(Some(UTF8), b"Edited in IPTC"), Some(false));
        assert_eq!(description(None, Some(&iptc), Some("Old")).as_deref(), Some("Edited in IPTC"));
        assert_eq!(description(Some(b"Camera"), Some(&iptc), Some("Old")).as_deref(), Some("Edited in IPTC"));
        // Another field changed: the caption is still the XMP's, cut at 2000 bytes.
        let long = "é".repeat(1500);
        let cut = &long[..2000];
        let iptc = block(&iim_of(Some(UTF8), cut.as_bytes()), Some(false));
        assert_eq!(description(None, Some(&iptc), Some(&long)).as_deref(), Some(long.as_str()));
        assert_eq!(description(Some(b"EXIF"), Some(&iptc), Some(&long)).as_deref(), Some("EXIF"));
        // Written in Latin-1 without 1:90: "€" fits (Windows-1252), "→" became "?".
        let iptc = block(&iim_of(None, b"5 \x80 \xE0 ?"), Some(false));
        assert_eq!(description(None, Some(&iptc), Some("5 € à →")).as_deref(), Some("5 € à →"));
    }

    /// The character set of the IPTC as 1:90 says, or guessed; the EXIF's guessed too.
    #[test]
    fn encodings() {
        let caption = |set: Option<&[u8]>, bytes: &[u8]| description(None, Some(&block(&iim_of(set, bytes), None)), None);
        assert_eq!(caption(Some(UTF8), "Città, 東京".as_bytes()).as_deref(), Some("Città, 東京"));
        assert_eq!(caption(None, "Città".as_bytes()).as_deref(), Some("Città"));
        assert_eq!(caption(None, b"Citt\xE0 \x80").as_deref(), Some("Città €"));
        assert_eq!(caption(Some(b"\x1B-A"), "Ã©".as_bytes()).as_deref(), Some("Ã\u{83}Â©"));
        assert_eq!(caption(Some(UTF8), b"bad \xE0").as_deref(), Some("bad \u{FFFD}"));
        assert_eq!(description(Some(b"Citt\xE0"), None, None).as_deref(), Some("Città"));
        assert_eq!(description(Some("\u{FEFF}Città".as_bytes()), None, None).as_deref(), Some("Città"));
    }

    /// Only spaces or NULs, or what a camera writes by itself: no description, the next place counts.
    #[test]
    fn empty_and_boilerplate() {
        assert_eq!(description(Some(b"                               "), None, Some("XMP")).as_deref(), Some("XMP"));
        assert_eq!(description(Some(b"\0\0\0\0"), None, None), None);
        assert_eq!(description(Some(b"OLYMPUS DIGITAL CAMERA         "), None, Some("XMP")).as_deref(), Some("XMP"));
        assert_eq!(description(Some(b"Lavc57.107.100"), None, None), None);
        assert_eq!(description(Some(b"Lavc is a word"), None, None).as_deref(), Some("Lavc is a word"));
        let iptc = block(&iim_of(None, b"SONY DSC"), None);
        assert_eq!(description(None, Some(&iptc), Some("  ")), None);
    }

    /// The XMP's "x-default" language, otherwise the first one.
    #[test]
    fn xmp_languages() {
        let xmp = |items: &str| {
            let x = format!(
                r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:description><rdf:Alt>{items}</rdf:Alt></dc:description></rdf:Description></rdf:RDF></x:xmpmeta>"#
            );
            crate::xmp_fields::read(x.as_bytes()).caption
        };
        assert_eq!(xmp(r#"<rdf:li xml:lang="it">Faro</rdf:li><rdf:li xml:lang="x-default">Lighthouse</rdf:li>"#).as_deref(), Some("Lighthouse"));
        assert_eq!(xmp(r#"<rdf:li xml:lang="de">Leuchtturm</rdf:li><rdf:li xml:lang="it">Faro</rdf:li>"#).as_deref(), Some("Leuchtturm"));
    }

    /// The IPTC after an extended dataset, and of a JPEG in two APP13 segments.
    #[test]
    fn where_found() {
        let extended = [&[0x1C, 2, 200, 0x80, 2, 0, 3][..], b"abc"].concat();
        let iim = [extended, iim_of(None, b"After")].concat();
        assert_eq!(description(None, Some(&irb_of_iim(&iim)), None).as_deref(), Some("After"));
        let irb = block(&iim_of(None, b"In two segments"), Some(true));
        let (a, b) = irb.split_at(10);
        let seg = |body: &[u8]| [&[0xFF, 0xED][..], &((body.len() + 2 + JPEG_PHOTOSHOP.len()) as u16).to_be_bytes(), JPEG_PHOTOSHOP, body].concat();
        let jpeg = [&[0xFF, 0xD8][..], &seg(a), &seg(b), &[0xFF, 0xDA, 0, 2]].concat();
        assert_eq!(photoshop_block(&jpeg), Some(irb));
    }

    /// The caption the panel shows for each file of `EYECANSEE_MWG` (its sidecar too), to set
    /// beside `exiftool -s3 -MWG:Description`:
    /// `EYECANSEE_MWG=<folder> cargo test mwg_live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn mwg_live() {
        let dir = std::env::var("EYECANSEE_MWG").unwrap();
        let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmp"))).collect();
        files.sort();
        for p in &files {
            let path = p.to_string_lossy().to_string();
            let raw_beside = files.iter().any(|q| q != p && q.file_stem() == p.file_stem() && crate::decode::is_raw(&q.to_string_lossy()));
            let sidecar = crate::sidecar::names(&path).into_iter().find_map(|name| {
                let xmp = std::fs::read(&name).ok()?;
                crate::sidecar::belongs(&path, &name, crate::sidecar::sidecar_for(&xmp).as_deref(), raw_beside).then_some(crate::sidecar::Sidecar { name, xmp })
            });
            let meta = crate::meta::read_file_with(&std::fs::read(p).unwrap(), &path, sidecar.as_ref());
            let caption = meta.and_then(|m| m.fields.into_iter().find(|(_, l, _)| *l == "meta-xmp-caption").map(|(_, _, v)| v));
            println!("{} | {}", p.file_name().unwrap().to_string_lossy(), caption.unwrap_or_default());
        }
    }
}
