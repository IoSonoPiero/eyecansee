use super::*;
use crate::meta_write::{irb_of_iim, resource};
use crate::xmp_fields::Fields;

/// An IPTC-IIM block: the 1:90 dataset if `charset`, then `(dataset, bytes)` of record 2.
fn iim_of(charset: Option<&[u8]>, fields: &[(u8, &[u8])]) -> Vec<u8> {
    let set = charset.map(|c| [&[0x1C, 1, 90][..], &(c.len() as u16).to_be_bytes(), c].concat()).unwrap_or_default();
    let rest = fields.iter().flat_map(|(d, t)| [&[0x1C, 2, *d][..], &(t.len() as u16).to_be_bytes(), t].concat());
    set.into_iter().chain(rest).collect()
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

fn exif_description(d: &[u8]) -> ExifText {
    ExifText { description: Some(d.to_vec()), ..ExifText::default() }
}

fn xmp_caption(c: &str) -> Fields {
    Fields { caption: Some(c.into()), ..Fields::default() }
}

/// The description from `exif`, a Photoshop block `irb` and the XMP's `xmp`.
fn description(exif: Option<&[u8]>, irb: Option<&[u8]>, xmp: Option<&str>) -> Option<String> {
    let exif = exif.map(exif_description).unwrap_or_default();
    reconcile(&exif, irb, &xmp.map(xmp_caption).unwrap_or_default()).description
}

fn caption_block(text: &[u8], digest: Option<bool>) -> Vec<u8> {
    block(&iim_of(Some(UTF8), &[(120, text)]), digest)
}

#[test]
fn one_place_only() {
    assert_eq!(description(Some(b"From EXIF\0"), None, None).as_deref(), Some("From EXIF"));
    let iptc = block(&iim_of(None, &[(120, b"From IPTC")]), None);
    assert_eq!(description(None, Some(&iptc), None).as_deref(), Some("From IPTC"));
    assert_eq!(description(None, None, Some(" From XMP ")).as_deref(), Some("From XMP"));
    assert_eq!(description(None, None, None), None);
}

/// EXIF first, then XMP, then IPTC, while the digest matches or there is none.
#[test]
fn exif_then_xmp_then_iptc() {
    for digest in [Some(true), None] {
        let iptc = caption_block(b"IPTC", digest);
        assert_eq!(description(Some(b"EXIF"), Some(&iptc), Some("XMP")).as_deref(), Some("EXIF"), "{digest:?}");
        assert_eq!(description(None, Some(&iptc), Some("XMP")).as_deref(), Some("XMP"), "{digest:?}");
        assert_eq!(description(None, Some(&iptc), None).as_deref(), Some("IPTC"), "{digest:?}");
    }
}

/// A digest that no longer matches: the IPTC was changed by an application unaware of XMP, its
/// value counts over the XMP and the EXIF; unless it is the XMP's own, shortened.
#[test]
fn iptc_changed() {
    let iptc = caption_block(b"Edited in IPTC", Some(false));
    assert_eq!(description(None, Some(&iptc), Some("Old")).as_deref(), Some("Edited in IPTC"));
    assert_eq!(description(Some(b"Camera"), Some(&iptc), Some("Old")).as_deref(), Some("Edited in IPTC"));
    // Another field changed: the caption is still the XMP's, cut at 2000 bytes.
    let long = "é".repeat(1500);
    let iptc = caption_block(&long.as_bytes()[..2000], Some(false));
    assert_eq!(description(None, Some(&iptc), Some(&long)).as_deref(), Some(long.as_str()));
    assert_eq!(description(Some(b"EXIF"), Some(&iptc), Some(&long)).as_deref(), Some("EXIF"));
    // Written in Latin-1 without 1:90: "€" fits (Windows-1252), "→" became "?".
    let iptc = block(&iim_of(None, &[(120, b"5 \x80 \xE0 ?")]), Some(false));
    assert_eq!(description(None, Some(&iptc), Some("5 € à →")).as_deref(), Some("5 € à →"));
}

/// The character set of the IPTC as 1:90 says, or guessed; the EXIF's guessed too.
#[test]
fn encodings() {
    let caption = |set: Option<&[u8]>, bytes: &[u8]| description(None, Some(&block(&iim_of(set, &[(120, bytes)]), None)), None);
    assert_eq!(caption(Some(UTF8), "Città, 東京".as_bytes()).as_deref(), Some("Città, 東京"));
    assert_eq!(caption(None, "Città".as_bytes()).as_deref(), Some("Città"));
    assert_eq!(caption(None, b"Citt\xE0 \x80").as_deref(), Some("Città €"));
    assert_eq!(caption(Some(b"\x1B-A"), "Ã©".as_bytes()).as_deref(), Some("Ã\u{83}Â©"));
    assert_eq!(caption(Some(UTF8), b"bad \xE0").as_deref(), Some("bad \u{FFFD}"));
    assert_eq!(description(Some(b"Citt\xE0"), None, None).as_deref(), Some("Città"));
    assert_eq!(description(Some("\u{FEFF}Città".as_bytes()), None, None).as_deref(), Some("Città"));
}

/// Only spaces or NULs: none, the next place counts. What a camera writes by itself is shown,
/// as exiftool does (Piero, 15/11).
#[test]
fn empty_and_camera_texts() {
    assert_eq!(description(Some(b"                               "), None, Some("XMP")).as_deref(), Some("XMP"));
    assert_eq!(description(Some(b"\0\0\0\0"), None, None), None);
    assert_eq!(description(Some(b"OLYMPUS DIGITAL CAMERA         "), None, Some("XMP")).as_deref(), Some("OLYMPUS DIGITAL CAMERA"));
    assert_eq!(description(Some(b"Lavc57.107.100"), None, None).as_deref(), Some("Lavc57.107.100"));
    let iptc = block(&iim_of(None, &[(120, b"   ")]), None);
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
    let iim = [extended, iim_of(None, &[(120, b"After")])].concat();
    assert_eq!(description(None, Some(&irb_of_iim(&iim)), None).as_deref(), Some("After"));
    let irb = caption_block(b"In two segments", Some(true));
    let (a, b) = irb.split_at(10);
    let seg = |body: &[u8]| [&[0xFF, 0xED][..], &((body.len() + 2 + JPEG_PHOTOSHOP.len()) as u16).to_be_bytes(), JPEG_PHOTOSHOP, body].concat();
    let jpeg = [&[0xFF, 0xD8][..], &seg(a), &seg(b), &[0xFF, 0xDA, 0, 2]].concat();
    assert_eq!(photoshop_block(&jpeg), Some(irb));
}

/// EXIF Artist split as the guidelines' examples (5.7); the IPTC's repeated By-line; the same
/// order of places as the description.
#[test]
fn authors() {
    assert_eq!(split_artist("Camera owner, John Smith; Photographer, Michael Brown; Image creator, Ken James"), ["Camera owner, John Smith", "Photographer, Michael Brown", "Image creator, Ken James"]);
    let quoted = r#"first; with;semicolon; "with; semicolon-space"; "with; semicolon-space and ""quotes"""; non-leading "quotes"; """leading"" and non-leading ""quotes"""; last"#;
    assert_eq!(
        split_artist(quoted),
        ["first", "with;semicolon", "with; semicolon-space", r#"with; semicolon-space and "quotes""#, r#"non-leading "quotes""#, r#""leading" and non-leading "quotes""#, "last"]
    );
    let iptc = block(&iim_of(Some(UTF8), &[(80, b"Ann"), (80, b"Bob")]), Some(true));
    let xmp = Fields { creators: vec!["Carla".into()], ..Fields::default() };
    let creators = |artist: Option<&[u8]>, xmp: &Fields| reconcile(&ExifText { artist: artist.map(<[u8]>::to_vec), ..ExifText::default() }, Some(&iptc), xmp).creators;
    assert_eq!(creators(Some(b"Dan; Eve"), &xmp), ["Dan", "Eve"]);
    assert_eq!(creators(Some(b"    "), &xmp), ["Carla"]);
    assert_eq!(creators(None, &Fields::default()), ["Ann", "Bob"]);
    // The IPTC changed, but its By-line is the XMP's cut at 32 bytes: the XMP's whole name.
    let long = "A very long name of a photographer";
    let cut = block(&iim_of(Some(UTF8), &[(80, &long.as_bytes()[..32])]), Some(false));
    let xmp = Fields { creators: vec![long.into()], ..Fields::default() };
    assert_eq!(reconcile(&ExifText::default(), Some(&cut), &xmp).creators, [long]);
}

/// The EXIF's two copyrights (photographer, editor) on two lines; then the XMP's, then the IPTC's.
#[test]
fn copyright() {
    let rights = |parts: &[&[u8]], irb: Option<&[u8]>, xmp: Option<&str>| {
        let exif = ExifText { copyright: parts.iter().map(|p| p.to_vec()).collect(), ..ExifText::default() };
        reconcile(&exif, irb, &Fields { rights: xmp.map(Into::into), ..Fields::default() }).rights
    };
    assert_eq!(rights(&[b"(c) Ann", b"(c) Ed"], None, Some("XMP")).as_deref(), Some("(c) Ann\n(c) Ed"));
    assert_eq!(rights(&[b"   ", b" "], None, Some("© XMP")).as_deref(), Some("© XMP"));
    let iptc = block(&iim_of(Some(UTF8), &[(116, "© IPTC".as_bytes())]), Some(false));
    assert_eq!(rights(&[b"(c) Ann"], Some(&iptc), Some("© XMP")).as_deref(), Some("© IPTC"));
    let iptc = block(&iim_of(Some(UTF8), &[(116, "© IPTC".as_bytes())]), None);
    assert_eq!(rights(&[], Some(&iptc), Some("© XMP")).as_deref(), Some("© XMP"));
}

/// Keywords: IPTC and XMP only, the whole list from one place.
#[test]
fn keywords() {
    let iptc = |digest| block(&iim_of(Some(UTF8), &[(25, b"sea"), (25, b"lighthouse")]), digest);
    let xmp = Fields { keywords: vec!["sea".into(), "dawn".into()], ..Fields::default() };
    let words = |irb: &[u8], xmp: &Fields| reconcile(&ExifText::default(), Some(irb), xmp).keywords;
    assert_eq!(words(&iptc(Some(true)), &xmp), ["sea", "dawn"]);
    assert_eq!(words(&iptc(None), &xmp), ["sea", "dawn"]);
    assert_eq!(words(&iptc(Some(false)), &xmp), ["sea", "lighthouse"]);
    assert_eq!(words(&iptc(Some(false)), &Fields::default()), ["sea", "lighthouse"]);
    // The IPTC changed elsewhere: its keywords are the XMP's.
    let same = block(&iim_of(Some(UTF8), &[(25, b"sea"), (25, b"dawn"), (120, b"new caption")]), Some(false));
    assert_eq!(words(&same, &xmp), ["sea", "dawn"]);
}

/// The date taken from the EXIF, the IPTC's date and time, the XMP's, as the panel writes dates.
#[test]
fn date_taken() {
    let taken = |exif: Option<&[u8]>, irb: Option<&[u8]>, xmp: Option<&str>| {
        let exif = ExifText { taken: exif.map(<[u8]>::to_vec), ..ExifText::default() };
        reconcile(&exif, irb, &Fields { taken: xmp.map(Into::into), ..Fields::default() }).taken
    };
    let iptc = |digest| block(&iim_of(Some(UTF8), &[(55, b"20200102"), (60, b"030405+0100")]), digest);
    assert_eq!(taken(Some(b"2026:10:09 17:48:35"), Some(&iptc(Some(false))), None).as_deref(), Some("2020-01-02 03:04:05"));
    assert_eq!(taken(Some(b"2026:10:09 17:48:35"), Some(&iptc(None)), Some("2021")).as_deref(), Some("2026-10-09 17:48:35"));
    assert_eq!(taken(Some(b"    :  :     :  :  "), Some(&iptc(None)), Some("2021-05-06T07:08:09.5+02:00")).as_deref(), Some("2021-05-06 07:08:09"));
    assert_eq!(taken(None, Some(&iptc(Some(true))), None).as_deref(), Some("2020-01-02 03:04:05"));
    // The IPTC changed, but its date is the XMP's: the XMP's.
    assert_eq!(taken(None, Some(&iptc(Some(false))), Some("2020-01-02T03:04:05+01:00")).as_deref(), Some("2020-01-02 03:04:05"));
    assert_eq!(taken(Some(b"2017-09-02T09:34:10"), None, None).as_deref(), Some("2017-09-02 09:34:10"));
    assert_eq!(xmp_date("2021-05-06T07:08Z").as_deref(), Some("2021-05-06 07:08:00"));
    assert_eq!(xmp_date("2021-05-06").as_deref(), Some("2021-05-06"));
    assert_eq!(xmp_date("yesterday"), None);
    assert_eq!(iptc_date("20200102", None).as_deref(), Some("2020-01-02"));
    assert_eq!(iptc_date("2020", None), None);
}

/// What the panel shows for each file of `EYECANSEE_MWG` (its sidecar too), to set beside
/// `exiftool -s3 -MWG:Description -MWG:Creator -MWG:Copyright -MWG:Keywords -MWG:DateTimeOriginal`:
/// `EYECANSEE_MWG=<folder> cargo test mwg_live -- --ignored --nocapture`.
#[test]
#[ignore]
fn mwg_live() {
    let dir = std::env::var("EYECANSEE_MWG").unwrap();
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("xmp"))).collect();
    files.sort();
    for p in files {
        let path = p.to_string_lossy().to_string();
        let sidecar = crate::sidecar::names(&path).into_iter().find_map(|name| {
            let xmp = std::fs::read(&name).ok()?;
            crate::sidecar::belongs(&path, &name, &xmp).then_some(crate::sidecar::Sidecar { name, xmp })
        });
        let meta = crate::meta::read_file_with(&std::fs::read(&p).unwrap(), &path, sidecar.as_ref()).unwrap_or_default();
        let get = |label: &str| meta.fields.iter().find(|(_, l, _)| *l == label).map_or("", |(_, _, v)| v.as_str()).replace('\n', "\\n");
        let labels = ["meta-xmp-caption", "meta-xmp-creator", "meta-xmp-rights", "meta-xmp-keywords", "meta-taken"];
        println!("{} | {}", p.file_name().unwrap().to_string_lossy(), labels.map(get).join(" | "));
    }
}
