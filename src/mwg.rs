//! The fields of a photo that can be written in more than one place, read as the Metadata Working
//! Group's "Guidelines for Handling Image Metadata" 2.0 (2010) ask a reader to (section 4.2.3,
//! reconciling EXIF, IPTC-IIM and XMP, and the text encodings of 4.2.4), for the metadata panel:
//! description (5.2: EXIF ImageDescription, IPTC Caption-Abstract 2:120, XMP dc:description),
//! date taken (5.3: EXIF DateTimeOriginal, IPTC DateCreated 2:55 and TimeCreated 2:60, XMP
//! photoshop:DateCreated), copyright (5.6: EXIF Copyright, IPTC CopyrightNotice 2:116, XMP
//! dc:rights), author (5.7: EXIF Artist, IPTC By-line 2:80, XMP dc:creator) and keywords (5.1:
//! IPTC Keywords 2:25, XMP dc:subject). exiftool's MWG tags (`MWG.pm`) follow the same rules, but
//! when an application unaware of XMP changed the IPTC they still show the EXIF; the guidelines
//! show the IPTC then (4.2.3.3), and so does this module (Piero, 15/11).

use crate::meta_write::{JPEG_PHOTOSHOP, digest_current, resource_data};

/// Photoshop resource with the MD5 of the IPTC-IIM, written by the applications that write the
/// XMP too: a digest that no longer matches says the IPTC was changed by one that does not.
const IPTC_DIGEST: u16 = 1061;

/// The fields as the EXIF holds them: the bytes of its texts.
#[derive(Default, Debug)]
pub struct ExifText {
    pub description: Option<Vec<u8>>,
    pub artist: Option<Vec<u8>>,
    /// The photographer's and the editor's copyright, the two texts the tag can hold.
    pub copyright: Vec<Vec<u8>>,
    /// DateTimeOriginal as written: "2026:10:09 17:48:35".
    pub taken: Option<Vec<u8>>,
}

/// The fields as the panel shows them.
#[derive(Default, Debug, PartialEq)]
pub struct Mapped {
    pub description: Option<String>,
    pub creators: Vec<String>,
    pub rights: Option<String>,
    pub keywords: Vec<String>,
    /// ISO, as the panel has dates: "2026-10-09 17:48:35", or only the date ("2026-10-09").
    pub taken: Option<String>,
}

/// Each field from the EXIF (`exif`), the IPTC of the Photoshop block (`irb`, with its digest)
/// and the XMP (`xmp`): EXIF first, then XMP, then IPTC; but the IPTC first when its digest no
/// longer matches and its value is not what the XMP's would have become in it (cut to the IPTC's
/// length, characters that do not fit in 8 bits). A text of only spaces or NULs counts as none.
pub fn reconcile(exif: &ExifText, irb: Option<&[u8]>, xmp: &crate::xmp_fields::Fields) -> Mapped {
    let iim = irb.and_then(crate::meta_write::iim);
    let changed = iim.zip(irb.and_then(|b| resource_data(b, IPTC_DIGEST))).is_some_and(|(i, d)| !digest_current(i, d));
    let iptc = iim.map(Iptc::read).unwrap_or_default();
    let utf8 = iptc.utf8;
    let text = |t: &str| decode(t.as_bytes(), Charset::Utf8);
    let one = |t: &Option<String>| t.as_deref().map(text).filter(|t| present(t));
    let list = |l: &[String]| Some(l.iter().map(|t| text(t)).filter(|t| present(t)).collect::<Vec<_>>()).filter(|l| !l.is_empty());
    let fits = move |limit: usize| move |x: &String, i: &String| predicted(x, i, utf8, limit);
    let fit_all = move |limit: usize| move |x: &Vec<String>, i: &Vec<String>| x.len() == i.len() && x.iter().zip(i).all(|(x, i)| predicted(x, i, utf8, limit));
    let exif_one = |b: &Option<Vec<u8>>| b.as_deref().map(|b| decode(b, Charset::Unknown)).filter(|t| present(t));
    let copyright = Some(exif.copyright.iter().map(|b| decode(b, Charset::Unknown)).filter(|t| present(t)).collect::<Vec<_>>().join("\n")).filter(|t| !t.is_empty());
    let artist = exif_one(&exif.artist).map(|a| split_artist(&a)).filter(|a| !a.is_empty());
    let iptc_date = iptc.one(55).and_then(|d| iptc_date(&d, iptc.one(60).as_deref()));
    Mapped {
        description: choose(exif_one(&exif.description), iptc.one(120), one(&xmp.caption), changed, fits(2000)),
        creators: choose(artist, Some(iptc.all(80)).filter(|l| !l.is_empty()), list(&xmp.creators), changed, fit_all(32)).unwrap_or_default(),
        rights: choose(copyright, iptc.one(116), one(&xmp.rights), changed, fits(128)),
        keywords: choose(None, Some(iptc.all(25)).filter(|l| !l.is_empty()), list(&xmp.keywords), changed, fit_all(64)).unwrap_or_default(),
        taken: choose(exif.taken.as_deref().and_then(exif_date), iptc_date, xmp.taken.as_deref().and_then(xmp_date), changed, |x, i| x == i),
    }
}

/// The texts of `exif` that `reconcile` reads.
pub fn exif_text(exif: Option<&exif::Exif>) -> ExifText {
    let parts = |tag| match exif.and_then(|e| e.get_field(tag, exif::In::PRIMARY)).map(|f| &f.value) {
        Some(exif::Value::Ascii(parts)) => parts.clone(),
        _ => Vec::new(),
    };
    let first = |tag| parts(tag).into_iter().find(|p| !p.is_empty());
    use exif::Tag;
    ExifText { description: first(Tag::ImageDescription), artist: first(Tag::Artist), copyright: parts(Tag::Copyright), taken: first(Tag::DateTimeOriginal) }
}

/// The Description box's `fields` with what `found` read from the file: in place of the XMP's,
/// which it already counts, or, with a `sidecar`, where the sidecar has nothing (as its other
/// fields, it counts: Lightroom keeps there what the person set). Gives back the date taken.
pub fn apply(found: Mapped, fields: &mut crate::xmp_fields::Fields, sidecar: bool) -> Option<String> {
    let own = |v: &mut Option<String>| std::mem::take(v).filter(|t| sidecar && present(t));
    let own_list = |v: &mut Vec<String>| Some(std::mem::take(v)).filter(|l| sidecar && !l.is_empty());
    fields.caption = own(&mut fields.caption).or(found.description);
    fields.rights = own(&mut fields.rights).or(found.rights);
    fields.creators = own_list(&mut fields.creators).unwrap_or(found.creators);
    fields.keywords = own_list(&mut fields.keywords).unwrap_or(found.keywords);
    own(&mut fields.taken).and_then(|t| xmp_date(&t)).or(found.taken)
}

/// `fields` (a sidecar's added to the file's XMP, `xmp::with_sidecar`) with the fields `apply`
/// chooses as the `sidecar` alone has them: what the file's XMP has of them is in `reconcile`'s
/// already, where the EXIF or the IPTC may count before it.
pub fn sidecar_only(fields: &mut crate::xmp_fields::Fields, sidecar: &crate::xmp_fields::Fields) {
    fields.caption.clone_from(&sidecar.caption);
    fields.rights.clone_from(&sidecar.rights);
    fields.creators.clone_from(&sidecar.creators);
    fields.keywords.clone_from(&sidecar.keywords);
    fields.taken.clone_from(&sidecar.taken);
}

/// The Shot box's date taken as `taken`, where the EXIF's was (first, if it had none).
pub fn set_taken(meta: &mut crate::meta::Meta, taken: Option<String>) {
    use crate::meta::Group;
    let at = meta.fields.iter().position(|(_, l, _)| *l == "meta-taken");
    match (at, taken) {
        (Some(i), Some(t)) => meta.fields[i].2 = t,
        (Some(i), None) => {
            meta.fields.remove(i);
        }
        (None, Some(t)) => {
            let i = meta.fields.iter().position(|(g, _, _)| *g == Group::Shot).unwrap_or(meta.fields.len());
            meta.fields.insert(i, (Group::Shot, "meta-taken", t));
        }
        (None, None) => {}
    }
}

/// The value of one field: EXIF, XMP or IPTC (`reconcile`). `predicted`: whether the IPTC's
/// value is the XMP's as an application writing both would have put it there (section 4.2.3.2,
/// scenario 4: the IPTC changed since, but not this field).
fn choose<T>(exif: Option<T>, iptc: Option<T>, xmp: Option<T>, changed: bool, predicted: impl Fn(&T, &T) -> bool) -> Option<T> {
    let edited = changed && iptc.as_ref().is_some_and(|i| xmp.as_ref().is_none_or(|x| !predicted(x, i)));
    if edited { iptc } else { exif.or(xmp).or(iptc) }
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

/// A text that exists: not only spaces and NULs.
fn present(text: &str) -> bool {
    !text.trim_matches(|c: char| c.is_whitespace() || c == '\0').is_empty()
}

/// EXIF Artist as the list of names it can hold (guidelines 5.7): "A; B" is two names, a name
/// in quotes can hold "; " and doubled quotes. As exiftool's `MWG::StringToList`.
fn split_artist(artist: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut quoted = false;
    for part in artist.split("; ") {
        let was = quoted;
        let mut part = part.to_string();
        if !quoted && let Some(rest) = part.strip_prefix('"') {
            (quoted, part) = (true, rest.to_string());
        }
        if quoted {
            // An odd number of quotes at its end closes it.
            let tail = part.len() - part.trim_end_matches('"').len();
            if tail % 2 == 1 {
                part.pop();
                quoted = false;
            }
            part = part.replace("\"\"", "\"");
        }
        match names.last_mut() {
            Some(last) if was => *last = format!("{last}; {part}"),
            _ => names.push(part),
        }
    }
    names.into_iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect()
}

/// EXIF "2026:10:09 17:48:35" as "2026-10-09 17:48:35" (a Hasselblad writes "2017-09-02T09:34:10");
/// `None` if only colons and spaces.
fn exif_date(raw: &[u8]) -> Option<String> {
    let s = decode(raw, Charset::Unknown);
    if s.chars().all(|c| c == ':' || c == ' ') {
        return None;
    }
    Some(s.char_indices().map(|(i, c)| if i < 10 && c == ':' { '-' } else if i == 10 && c == 'T' { ' ' } else { c }).collect())
}

/// XMP "2026-10-09T17:48:35.12+02:00" (or "2026-10-09T17:48Z", "2026-10-09", "2026") as the
/// panel writes it: the time to the second, without its fraction and zone.
pub fn xmp_date(s: &str) -> Option<String> {
    let s = s.trim();
    let (date, time) = s.split_once('T').unwrap_or((s, ""));
    if date.is_empty() || !date.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return None;
    }
    let time: String = time.chars().take_while(|c| c.is_ascii_digit() || *c == ':').collect();
    Some(match time.split(':').count() {
        _ if time.is_empty() => date.to_string(),
        2 => format!("{date} {time}:00"),
        _ => format!("{date} {}", &time[..time.len().min(8)]),
    })
}

/// IPTC "20261009" and "174835+0200" as "2026-10-09 17:48:35".
fn iptc_date(date: &str, time: Option<&str>) -> Option<String> {
    let d = date.trim();
    if d.len() != 8 || !d.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let date = format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]);
    Some(match time.map(str::trim).filter(|t| t.len() >= 6 && t.as_bytes()[..6].iter().all(u8::is_ascii_digit)) {
        Some(t) => format!("{date} {}:{}:{}", &t[..2], &t[2..4], &t[4..6]),
        None => date,
    })
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

/// The texts of record 2 of an IPTC-IIM block (dataset, text), in the character set its 1:90
/// dataset names.
#[derive(Default, Debug)]
struct Iptc {
    values: Vec<(u8, String)>,
    /// Written in UTF-8: said by 1:90, or all its texts valid UTF-8 when nothing is said.
    utf8: bool,
}

impl Iptc {
    fn read(iim: &[u8]) -> Iptc {
        let mut charset = Charset::Unknown;
        let mut raw: Vec<(u8, &[u8])> = Vec::new();
        let mut i = 0;
        while let Some(&[0x1C, record, dataset, a, b]) = iim.get(i..i + 5) {
            let mut len = usize::from(u16::from_be_bytes([a, b]));
            let mut start = i + 5;
            // An extended dataset: the length is in the next `len & 0x7FFF` bytes.
            if len & 0x8000 != 0 {
                let n = len & 0x7FFF;
                let Some(size) = iim.get(start..start + n).filter(|_| n <= 8) else { break };
                len = size.iter().fold(0usize, |l, &b| l << 8 | usize::from(b));
                start += n;
            }
            let Some(data) = start.checked_add(len).and_then(|end| iim.get(start..end)) else { break };
            i = start + len;
            match record {
                1 if dataset == 90 => charset = coded_charset(data),
                2 => raw.push((dataset, data)),
                _ => {}
            }
        }
        let utf8 = charset == Charset::Utf8 || (charset == Charset::Unknown && raw.iter().all(|(_, d)| std::str::from_utf8(d).is_ok()));
        let charset = if charset == Charset::Unknown && utf8 { Charset::Utf8 } else { charset };
        Iptc { values: raw.into_iter().map(|(d, b)| (d, decode(b, charset))).filter(|(_, t)| present(t)).collect(), utf8 }
    }

    /// The first text of `dataset`.
    fn one(&self, dataset: u8) -> Option<String> {
        self.values.iter().find(|(d, _)| *d == dataset).map(|(_, t)| t.clone())
    }

    /// All the texts of a repeated `dataset` (By-line, Keywords).
    fn all(&self, dataset: u8) -> Vec<String> {
        self.values.iter().filter(|(d, _)| *d == dataset).map(|(_, t)| t.clone()).collect()
    }
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

/// Whether the IPTC text `iptc` is what an application writing both would have made of `xmp`:
/// the same text, characters that 8 bits cannot hold as "?", cut at `limit` bytes.
fn predicted(xmp: &str, iptc: &str, utf8: bool, limit: usize) -> bool {
    let fits = |c: char| u32::from(c) <= 0xFF || CP1252.contains(&c);
    let text: String = if utf8 { xmp.to_string() } else { xmp.chars().map(|c| if fits(c) { c } else { '?' }).collect() };
    let bytes = |s: &str| if utf8 { s.len() } else { s.chars().count() };
    // Cut at the limit, or a little before it, at the end of a UTF-8 character.
    text == iptc || (bytes(&text) > limit && bytes(iptc) + 4 > limit && text.starts_with(iptc))
}

#[cfg(test)]
mod tests;
