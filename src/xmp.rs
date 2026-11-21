//! The XMP of an image being written again (Rotate and crop, Save, Convert and resize, Save in
//! another format), with `quick-xml`: sizes and orientation made true for the pixels written, the
//! XMP thumbnail and the description of a Motion Photo's video (never written again) taken out,
//! the Camera Raw / Lightroom settings (`crs:`) marked as already in the pixels (`fix`),
//! everything else (author, copyright, licence, descriptions, keywords…) kept as it was. For WebP and AVIF, which have no place for IPTC, the IPTC fields go into the
//! XMP as Adobe and the IPTC Core map them, when the XMP does not have them already; so do the
//! texts of a PNG and the comments of a JPEG or a GIF in a file of another format (#218).

use quick_xml::Writer;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::name::{PrefixDeclaration, ResolveResult};
use quick_xml::reader::NsReader;

use crate::encode::texts::Field;

pub(crate) const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const XAP: &str = "http://ns.adobe.com/xap/1.0/";
const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
pub(crate) const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
pub(crate) const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
pub(crate) const XMP_NOTE: &str = "http://ns.adobe.com/xmp/note/";
pub(crate) const GCAMERA: &str = "http://ns.google.com/photos/1.0/camera/";
pub(crate) const CONTAINER: &str = "http://ns.google.com/photos/1.0/container/";
pub(crate) const ITEM: &str = "http://ns.google.com/photos/1.0/container/item/";
pub(crate) const HDRGM: &str = "http://ns.adobe.com/hdr-gain-map/1.0/";
const IPTC_CORE: &str = "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/";
const IPTC_EXT: &str = "http://iptc.org/std/Iptc4xmpExt/2008-02-29/";
const DJI: &str = "http://www.dji.com/drone-dji/1.0/";

/// What happens to a property: dropped, or its value replaced.
enum Fix {
    Drop,
    Set(String),
}

/// How a packet is written again (`fix`).
#[derive(Clone, Copy, Default)]
struct How {
    merged: bool,
    alone: bool,
    /// The video of a Motion Photo is written too (a lossless turn of an Ultra HDR one, #54): its
    /// description stays.
    video: bool,
    /// Without the place (#55): GPS, city, state, country, the IPTC locations (`place`).
    no_place: bool,
    /// The XMP of a RAW file or of its sidecar (#24): its Camera Raw settings and darktable's
    /// history go (`without_camera_raw`).
    raw: bool,
}

/// A property that says where the photo was taken: the GPS of the EXIF, the place of Photoshop
/// and of the IPTC Core and Extension, the position of a DJI drone.
fn place(ns: &str, local: &str) -> bool {
    match ns {
        EXIF => local.starts_with("GPS"),
        PHOTOSHOP => matches!(local, "City" | "State" | "Country"),
        IPTC_CORE => matches!(local, "Location" | "CountryCode"),
        IPTC_EXT => matches!(local, "LocationCreated" | "LocationShown"),
        DJI => local.starts_with("Gps") || local.ends_with("Altitude"),
        _ => false,
    }
}

/// `merged`: the extended XMP is in the packet, the GUID that pointed to it goes. The video of a
/// Motion Photo is written again only by a lossless turn (`video`), so what describes it goes (also when the original
/// says so without having one, as a Pixel 8 photo seen). `alone`: the picture is written without
/// the gain map of an Ultra HDR photo either, so its description and the container directory go.
///
/// Camera Raw / Lightroom settings (`crs:`, the crop too): in a photo that is not RAW they describe
/// how it was developed and stay, with `crs:AlreadyApplied` "True" (added by `rewrite` when
/// missing), as Lightroom writes them when it exports with "All Metadata": with "False" Camera Raw
/// and Lightroom would apply them again to pixels that already have them (DxO forum, "Remove
/// xmp-crs data in EXIF when exporting to DNG"). The tag is Adobe's use, not a standard: exiv2
/// lists it as "Not in XMP Specification. Found in sample files". In a RAW file they are settings
/// EyeCanSee does not apply when it develops it: they go (`raw`), as darktable's history in a
/// sidecar (#24).
fn fix(ns: &str, local: &str, w: u32, h: u32, how: How) -> Option<Fix> {
    match (ns, local) {
        _ if how.no_place && place(ns, local) => Some(Fix::Drop),
        (XMP_NOTE, "HasExtendedXMP") if how.merged => Some(Fix::Drop),
        // Only the lossless turn of an Ultra HDR Motion Photo keeps the video (`how.video`).
        (GCAMERA, l) if !how.video && (l.starts_with("MotionPhoto") || l.starts_with("MicroVideo")) => Some(Fix::Drop),
        (CONTAINER, "Directory") | (HDRGM, _) if how.alone => Some(Fix::Drop),
        (CRS, _) if how.raw => Some(Fix::Drop),
        // darktable's history, as Camera Raw's settings; its colour labels are the person's.
        (crate::xmp_fields::DARKTABLE, l) if how.raw && l != "colorlabels" => Some(Fix::Drop),
        // A sidecar's (#24): the copy has its XMP inside.
        (PHOTOSHOP, "SidecarForExtension") if how.raw => Some(Fix::Drop),
        (CRS, "AlreadyApplied") => Some(Fix::Set("True".into())),
        (XAP, "Thumbnails") => Some(Fix::Drop),
        (TIFF, "ImageWidth") | (EXIF, "PixelXDimension") => Some(Fix::Set(w.to_string())),
        (TIFF, "ImageLength") | (EXIF, "PixelYDimension") => Some(Fix::Set(h.to_string())),
        (TIFF, "Orientation") => Some(Fix::Set("1".into())),
        _ => None,
    }
}

fn bound(r: &ResolveResult) -> String {
    match r {
        ResolveResult::Bound(ns) => ns.as_ref().to_string(),
        _ => String::new(),
    }
}

/// An empty XMP packet, for an IPTC with nothing to go into.
const EMPTY: &str = "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"></rdf:RDF></x:xmpmeta>\n<?xpacket end=\"w\"?>";

/// `xmp` (or a new packet when there is none and `iim` has fields) made to fit `w` × `h` pixels,
/// with the fields of the IPTC-IIM `iim` it does not have. A packet that is not well-formed XML is
/// returned as it was.
pub fn fit(xmp: Option<&[u8]>, w: u32, h: u32, iim: Option<&[u8]>) -> Option<Vec<u8>> {
    fit_with(xmp, w, h, iim, None, &[])
}

/// As `fit`, with the descriptions of the extended XMP `ext` of a JPEG (`meta_write::xmp_ext_packet`)
/// put into the main packet (a JPEG splits it again when it is too large: `xmp_ext::split`); if
/// that cannot be read, the main packet alone. The `texts` of a PNG, a JPEG or a GIF (#218) are
/// added as the IPTC fields, after them.
pub fn fit_with(xmp: Option<&[u8]>, w: u32, h: u32, iim: Option<&[u8]>, ext: Option<&[u8]>, texts: &[(Field, String)]) -> Option<Vec<u8>> {
    fit_how(xmp, w, h, (iim, texts), ext, How::default())
}

/// As `fit_with`, without the place (#55): a packet that is not well-formed XML goes whole, since
/// what it holds cannot be told.
pub fn fit_without_place(xmp: Option<&[u8]>, w: u32, h: u32, iim: Option<&[u8]>, ext: Option<&[u8]>, texts: &[(Field, String)]) -> Option<Vec<u8>> {
    fit_how(xmp, w, h, (iim, texts), ext, How { no_place: true, ..How::default() })
}

fn fit_how(xmp: Option<&[u8]>, w: u32, h: u32, (iim, texts): (Option<&[u8]>, &[(Field, String)]), ext: Option<&[u8]>, how: How) -> Option<Vec<u8>> {
    let mut props = iim.map(iim_props).unwrap_or_default();
    add_texts(&mut props, texts);
    let xmp = xmp.map(without_video_item);
    let xmp = match xmp.as_deref() {
        Some(x) => x,
        None if props.is_empty() => return None,
        None => EMPTY.as_bytes(),
    };
    // Each property once, the main packet's first (`xmp_ext::with_extended`).
    let merged = ext.and_then(|e| crate::xmp_ext::with_extended(xmp, e));
    if let Some(out) = merged.and_then(|m| rewrite(&m, w, h, &props, How { merged: true, ..how })) {
        return Some(out);
    }
    match rewrite(xmp, w, h, &props, how) {
        Some(out) => Some(out),
        None if how.no_place => None,
        None => Some(xmp.to_vec()),
    }
}

/// As `fit`, for a lossless turn that writes the Motion Photo's video again after the picture
/// (#54): its description and its item in the container directory stay. A packet that is not
/// well-formed XML is returned as it was.
pub fn fit_keeping_video(xmp: &[u8], w: u32, h: u32) -> Vec<u8> {
    rewrite(xmp, w, h, &[], How { video: true, ..How::default() }).unwrap_or_else(|| xmp.to_vec())
}

/// `xmp` without the item of a Motion Photo's video in the container directory (the video is
/// never written again, as `fix` says), the other items (the gain map) kept: an `rdf:li` holding
/// `Item:Semantic="MotionPhoto"` goes, with the blanks before it.
// ponytail: by text, with the prefixes the phones write (Pixel); an item written as elements
// (`<Item:Semantic>MotionPhoto</Item:Semantic>`) stays.
fn without_video_item(xmp: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let rfind = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).rposition(|w| w == needle);
    let Some(at) = find(xmp, b"Item:Semantic=\"MotionPhoto\"") else { return xmp.into() };
    let (Some(open), Some(close)) = (rfind(&xmp[..at], b"<rdf:li"), find(&xmp[at..], b"</rdf:li>")) else { return xmp.into() };
    let start = open - xmp[..open].iter().rev().take_while(|c| c.is_ascii_whitespace()).count();
    [&xmp[..start], &xmp[at + close + b"</rdf:li>".len()..]].concat().into()
}

/// The XMP of a RAW file (or of its sidecar, #24) without its Camera Raw settings and darktable's
/// history (`fix`), for a copy of what EyeCanSee develops from it; sizes set to `w` × `h` as `fit` does. A packet that is not well-formed XML is
/// returned as it was.
pub fn without_camera_raw(xmp: &[u8], w: u32, h: u32) -> Vec<u8> {
    rewrite(xmp, w, h, &[], How { raw: true, ..How::default() }).unwrap_or_else(|| xmp.to_vec())
}

/// The XMP of a photo's sidecar (#24) as it joins a copy's: without its Camera Raw settings and
/// darktable's history (as `without_camera_raw`), and without what describes the parts of a file
/// (Ultra HDR gain map and container directory, Motion Photo video, as `alone`): the copy's own
/// XMP says those, never the sidecar.
pub fn of_sidecar(xmp: &[u8], w: u32, h: u32) -> Vec<u8> {
    rewrite(xmp, w, h, &[], How { raw: true, alone: true, ..How::default() }).unwrap_or_else(|| xmp.to_vec())
}

/// The XMP of a photo that is not RAW (`inside`, its own) with its `sidecar` joined as
/// `with_sidecar` does it, the sidecar's part made as `of_sidecar` says for `w` × `h` pixels.
pub fn joined(inside: Option<&[u8]>, sidecar: &[u8], w: u32, h: u32) -> Vec<u8> {
    with_sidecar(inside, &of_sidecar(sidecar, w, h))
}

/// The XMP of a photo that is not RAW with what its sidecar (#24) says: the sidecar's packet, with
/// the properties only `inside` (the file's own XMP) has added after it, as digiKam merges them
/// (the sidecar's values count first). Either one not well-formed: the sidecar as it is.
pub fn with_sidecar(inside: Option<&[u8]>, sidecar: &[u8]) -> Vec<u8> {
    let text = |x: &[u8]| String::from_utf8_lossy(x).trim_start_matches(|c| c != '<').trim_end_matches(|c| c != '>').to_string();
    let (outer, own) = (text(sidecar), inside.map(text).unwrap_or_default());
    let (Ok(side), Ok(doc)) = (roxmltree::Document::parse(&outer), roxmltree::Document::parse(&own)) else { return sidecar.to_vec() };
    let Some(end) = sidecar.windows(10).rposition(|w| w == b"</rdf:RDF>") else { return sidecar.to_vec() };
    let key = |ns: Option<&str>, local: &str| (ns.unwrap_or("").to_string(), local.to_string());
    fn descriptions<'a, 'i>(d: &'a roxmltree::Document<'i>) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> {
        d.descendants().filter(|n| n.has_tag_name((RDF, "Description")))
    }
    let mut seen = std::collections::HashSet::new();
    for d in descriptions(&side) {
        seen.extend(d.attributes().map(|a| key(a.namespace(), a.name())));
        seen.extend(d.children().filter(|n| n.is_element()).map(|n| key(n.tag_name().namespace(), n.tag_name().name())));
    }
    let esc = |v: &str| quick_xml::escape::escape(v).into_owned();
    let mut added = String::new();
    for d in descriptions(&doc) {
        let prefix = |uri: &str| d.namespaces().find(|n| n.uri() == uri).and_then(|n| n.name());
        let attrs: Vec<String> = d
            .attributes()
            .filter(|a| a.namespace().is_some_and(|ns| ns != RDF) && !seen.contains(&key(a.namespace(), a.name())))
            .filter_map(|a| Some(format!(" {}:{}=\"{}\"", prefix(a.namespace()?)?, a.name(), esc(a.value()))))
            .collect();
        let elements: Vec<&str> = d.children().filter(|n| n.is_element() && !seen.contains(&key(n.tag_name().namespace(), n.tag_name().name()))).map(|n| &own[n.range()]).collect();
        if attrs.is_empty() && elements.is_empty() {
            continue;
        }
        let decls: String = d.namespaces().filter_map(|n| Some(format!(" xmlns:{}=\"{}\"", n.name()?, esc(n.uri())))).filter(|x| !x.starts_with(" xmlns:xml=")).collect();
        added += &format!("<rdf:Description rdf:about=\"\"{decls}{}>{}</rdf:Description>", attrs.concat(), elements.concat());
    }
    [&sidecar[..end], added.as_bytes(), &sidecar[end..]].concat()
}

/// `xmp` without what describes the parts after the picture in the file (Motion Photo video,
/// Ultra HDR gain map and its container directory), for a picture written alone; sizes set to
/// `w` × `h` as `fit` does. A packet that is not well-formed XML is returned as it was.
pub fn alone(xmp: &[u8], w: u32, h: u32) -> Vec<u8> {
    rewrite(xmp, w, h, &[], How { alone: true, ..How::default() }).unwrap_or_else(|| xmp.to_vec())
}

fn rewrite(xmp: &[u8], w: u32, h: u32, props: &[Prop], how: How) -> Option<Vec<u8>> {
    let mut r = NsReader::from_reader(xmp);
    let mut out = Writer::new(Vec::with_capacity(xmp.len()));
    // Depth inside a dropped element; the new value for the text of the element being read, and
    // whether its old text is being replaced.
    let (mut skip, mut set, mut replacing) = (0usize, None::<String>, false);
    // Camera Raw settings kept without `crs:AlreadyApplied`: it goes on the first description that
    // declares their namespace.
    let has = |s: &[u8]| xmp.windows(s.len()).any(|w| w == s);
    let mut applied = how.raw || !has(CRS.as_bytes()) || has(b"AlreadyApplied");
    let mut seen: Vec<(String, String)> = Vec::new();
    loop {
        let (ns, event) = r.read_resolved_event().ok()?;
        let ns = bound(&ns);
        match event {
            Event::Start(_) | Event::Empty(_) if skip > 0 => skip += matches!(event, Event::Start(_)) as usize,
            Event::End(_) if skip > 0 => skip -= 1,
            _ if skip > 0 => {}
            Event::Start(ref e) | Event::Empty(ref e) if matches!(fix(&ns, e.local_name().as_ref(), w, h, how), Some(Fix::Drop)) => {
                skip = matches!(event, Event::Start(_)) as usize;
            }
            Event::Start(ref e) | Event::Empty(ref e) => {
                seen.push((ns.clone(), e.local_name().as_ref().to_string()));
                if let Some(Fix::Set(v)) = fix(&ns, e.local_name().as_ref(), w, h, how)
                    && matches!(event, Event::Start(_))
                {
                    (set, replacing) = (Some(v), true);
                }
                let e = attributes(&r, e, w, h, how, &mut seen, &mut applied)?;
                out.write_event(if matches!(event, Event::Start(_)) { Event::Start(e) } else { Event::Empty(e) }).ok()?;
            }
            Event::Text(_) | Event::GeneralRef(_) if replacing => {
                // The whole text, entities included, becomes the new value once.
                if let Some(v) = set.take() {
                    out.write_event(Event::Text(BytesText::new(&v))).ok()?;
                }
            }
            Event::End(e) => {
                replacing = false;
                if let Some(v) = set.take() {
                    out.write_event(Event::Text(BytesText::new(&v))).ok()?;
                }
                if ns == RDF && e.local_name().as_ref() == "RDF" {
                    out.get_mut().extend(description(props, &seen).as_bytes());
                }
                out.write_event(Event::End(e)).ok()?;
            }
            Event::Eof => break,
            e => out.write_event(e).ok()?,
        }
    }
    Some(out.into_inner())
}

/// The attributes of an element, with the sizes and orientation set, `crs:AlreadyApplied` added
/// when `applied` says it is still missing and this is a description declaring the Camera Raw
/// namespace, without Camera Raw (and its namespace) for a RAW file; the element as it was when
/// nothing changes.
fn attributes<'a>(r: &NsReader<&[u8]>, e: &BytesStart<'a>, w: u32, h: u32, how: How, seen: &mut Vec<(String, String)>, applied: &mut bool) -> Option<BytesStart<'a>> {
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut changed = false;
    let description = e.local_name().as_ref() == "Description";
    for a in e.attributes() {
        let a = a.ok()?;
        let value = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()?.into_owned();
        let key = a.key.as_ref().to_string();
        if let Some(binding) = a.key.as_namespace_binding() {
            // Their declarations go with what they named: a reader of Ultra HDR (Skia) takes the
            // first description declaring the container's namespaces as the one describing it.
            if (value == CRS && how.raw) || (how.alone && [CONTAINER, ITEM, HDRGM].contains(&value.as_str())) {
                changed = true;
                continue;
            }
            kept.push((key, value.clone()));
            if let PrefixDeclaration::Named(prefix) = binding
                && value == CRS
                && description
                && !*applied
            {
                *applied = true;
                changed = true;
                kept.push((format!("{prefix}:AlreadyApplied"), "True".into()));
            }
            continue;
        }
        let (ns, local) = r.resolver().resolve_attribute(a.key);
        let ns = bound(&ns);
        seen.push((ns.clone(), local.as_ref().to_string()));
        match fix(&ns, local.as_ref(), w, h, how) {
            Some(Fix::Drop) => changed = true,
            Some(Fix::Set(v)) => {
                changed |= v != value;
                kept.push((key, v));
            }
            None => kept.push((key, value)),
        }
    }
    if !changed {
        return Some(e.clone());
    }
    let mut new = BytesStart::new(e.name().as_ref().to_string());
    for (k, v) in &kept {
        new.push_attribute((k.as_str(), v.as_str()));
    }
    Some(new)
}

/// How an IPTC field is written in XMP.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Text,
    /// Ordered list (authors).
    Seq,
    /// Unordered list (keywords).
    Bag,
    /// Text in languages, here only the default one.
    Alt,
}

#[derive(PartialEq, Debug)]
struct Prop {
    /// "dc", "photoshop" or "exif".
    prefix: &'static str,
    name: &'static str,
    kind: Kind,
    values: Vec<String>,
}

/// IPTC-IIM datasets of record 2 and the XMP property each one is in the IPTC Core.
const MAP: [(u8, bool, &str, Kind); 17] = [
    (5, true, "title", Kind::Alt),
    (15, false, "Category", Kind::Text),
    (25, true, "subject", Kind::Bag),
    (40, false, "Instructions", Kind::Text),
    (55, false, "DateCreated", Kind::Text),
    (80, true, "creator", Kind::Seq),
    (85, false, "AuthorsPosition", Kind::Text),
    (90, false, "City", Kind::Text),
    (95, false, "State", Kind::Text),
    (101, false, "Country", Kind::Text),
    (103, false, "TransmissionReference", Kind::Text),
    (105, false, "Headline", Kind::Text),
    (110, false, "Credit", Kind::Text),
    (115, false, "Source", Kind::Text),
    (116, true, "rights", Kind::Alt),
    (120, true, "description", Kind::Alt),
    (122, false, "CaptionWriter", Kind::Text),
];

/// The fields of an IPTC-IIM block that have a place in XMP.
fn iim_props(iim: &[u8]) -> Vec<Prop> {
    let mut props: Vec<Prop> = Vec::new();
    let mut i = 0;
    while let Some(&[0x1C, record, dataset, a, b]) = iim.get(i..i + 5) {
        let len = u16::from_be_bytes([a, b]) as usize;
        // Lengths of 32 KB and more (extended datasets) are never text.
        let Some(data) = iim.get(i + 5..i + 5 + len).filter(|_| len < 0x8000) else { break };
        i += 5 + len;
        let Some(&(_, dc, name, kind)) = MAP.iter().find(|m| record == 2 && m.0 == dataset) else { continue };
        // UTF-8 when the file says so (1:90) or it is valid UTF-8, otherwise Latin-1.
        let mut text = String::from_utf8(data.to_vec()).unwrap_or_else(|_| data.iter().map(|&c| c as char).collect());
        if dataset == 55 && text.len() == 8 && text.bytes().all(|c| c.is_ascii_digit()) {
            text = format!("{}-{}-{}", &text[..4], &text[4..6], &text[6..]);
        }
        match props.iter_mut().find(|p| p.name == name) {
            Some(p) if matches!(kind, Kind::Seq | Kind::Bag) => p.values.push(text),
            Some(_) => {}
            None => props.push(Prop { prefix: if dc { "dc" } else { "photoshop" }, name, kind, values: vec![text] }),
        }
    }
    props
}

/// The texts of a PNG, a JPEG or a GIF (#218) as properties, after those of the IPTC: title,
/// authors, description and copyright in Dublin Core, the comment as exif:UserComment. A
/// property the IPTC already gives stays the IPTC's.
fn add_texts(props: &mut Vec<Prop>, texts: &[(Field, String)]) {
    let from_iptc = props.len();
    for (field, text) in texts {
        let (prefix, name, kind) = match field {
            Field::Title => ("dc", "title", Kind::Alt),
            Field::Author => ("dc", "creator", Kind::Seq),
            Field::Description => ("dc", "description", Kind::Alt),
            Field::Copyright => ("dc", "rights", Kind::Alt),
            Field::Comment => ("exif", "UserComment", Kind::Alt),
        };
        match props.iter().position(|p| p.prefix == prefix && p.name == name) {
            Some(at) if at >= from_iptc && kind == Kind::Seq => props[at].values.push(text.clone()),
            Some(_) => {}
            None => props.push(Prop { prefix, name, kind, values: vec![text.clone()] }),
        }
    }
}

/// The namespace of a property's prefix (`Prop`).
fn namespace(prefix: &str) -> &'static str {
    match prefix {
        "dc" => DC,
        "photoshop" => PHOTOSHOP,
        _ => EXIF,
    }
}

/// An `rdf:Description` with the IPTC fields the XMP does not have yet (empty if none).
fn description(props: &[Prop], seen: &[(String, String)]) -> String {
    let missing: Vec<&Prop> = props.iter().filter(|p| !seen.iter().any(|(ns, l)| ns == namespace(p.prefix) && l == p.name)).collect();
    if missing.is_empty() {
        return String::new();
    }
    let esc = |s: &str| quick_xml::escape::escape(s).into_owned();
    let mut d = format!("<rdf:Description rdf:about=\"\" xmlns:dc=\"{DC}\" xmlns:photoshop=\"{PHOTOSHOP}\" xmlns:exif=\"{EXIF}\">");
    for p in missing {
        let name = format!("{}:{}", p.prefix, p.name);
        let items = |tag: &str, lang: &str| format!("<rdf:{tag}>{}</rdf:{tag}>", p.values.iter().map(|v| format!("<rdf:li{lang}>{}</rdf:li>", esc(v))).collect::<String>());
        let inner = match p.kind {
            Kind::Text => esc(&p.values[0]),
            Kind::Seq => items("Seq", ""),
            Kind::Bag => items("Bag", ""),
            Kind::Alt => items("Alt", " xml:lang=\"x-default\""),
        };
        d.push_str(&format!("<{name}>{inner}</{name}>"));
    }
    d + "</rdf:Description>"
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:tiff="http://ns.adobe.com/tiff/1.0/" xmlns:exif="http://ns.adobe.com/exif/1.0/"
 xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"
 xmlns:Item="http://ns.google.com/photos/1.0/container/item/"
 tiff:Orientation="6" exif:PixelXDimension="4000" crs:CropTop="0.1" crs:HasCrop="True" xmp:CreatorTool="Lightroom &amp; co">
<exif:PixelYDimension>3000</exif:PixelYDimension>
<tiff:ImageWidth>4000</tiff:ImageWidth>
<crs:ToneCurve><rdf:Seq><rdf:li>0, 0</rdf:li></rdf:Seq></crs:ToneCurve>
<crs:Exposure2012/>
<xmp:Thumbnails><rdf:Alt><rdf:li rdf:parseType="Resource"><xmpGImg:image xmlns:xmpGImg="http://ns.adobe.com/xap/1.0/g/img/">/9j/4AAQ</xmpGImg:image></rdf:li></rdf:Alt></xmp:Thumbnails>
<dc:creator><rdf:Seq><rdf:li>Ann &lt;Author&gt;</rdf:li></rdf:Seq></dc:creator>
<dc:rights><rdf:Alt><rdf:li xml:lang="x-default">CC BY-SA 4.0</rdf:li></rdf:Alt></dc:rights>
<Item:Entry Item:Semantic="GainMap" Item:Length="1234"/>
</rdf:Description></rdf:RDF></x:xmpmeta>
<?xpacket end="w"?>"#;

    /// An IPTC-IIM block of `(dataset, text)` in record 2.
    pub(crate) fn iim(fields: &[(u8, &str)]) -> Vec<u8> {
        fields.iter().flat_map(|(d, t)| [&[0x1C, 2, *d][..], &(t.len() as u16).to_be_bytes(), t.as_bytes()].concat()).collect()
    }

    /// Sizes and orientation set in attributes and elements; the thumbnail gone; Camera Raw kept
    /// and marked as applied; author, rights, the Ultra HDR item and the packet wrapper as they were.
    #[test]
    fn fitted() {
        let out = String::from_utf8(fit(Some(SAMPLE.as_bytes()), 300, 200, None).unwrap()).unwrap();
        for gone in ["Thumbnails", "/9j/", "4000", "3000", "\"6\""] {
            assert!(!out.contains(gone), "{gone} in {out}");
        }
        for kept in [
            "tiff:Orientation=\"1\"",
            "exif:PixelXDimension=\"300\"",
            "<exif:PixelYDimension>200</exif:PixelYDimension>",
            "<tiff:ImageWidth>300</tiff:ImageWidth>",
            "Lightroom &amp; co",
            "Ann &lt;Author&gt;",
            "CC BY-SA 4.0",
            "Item:Semantic=\"GainMap\" Item:Length=\"1234\"",
            "<?xpacket end=\"w\"?>",
            "xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:AlreadyApplied=\"True\"",
            "crs:CropTop=\"0.1\" crs:HasCrop=\"True\"",
            "<crs:ToneCurve><rdf:Seq><rdf:li>0, 0</rdf:li></rdf:Seq></crs:ToneCurve>",
        ] {
            assert!(out.contains(kept), "{kept} not in {out}");
        }
        assert!(roxmltree::Document::parse(out.trim_start_matches(|c| c != '<')).is_ok());
        // Nothing to change: the same bytes. Not XML: as it was.
        let plain = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/></x:xmpmeta>"#;
        assert_eq!(fit(Some(plain), 1, 1, None).unwrap(), plain);
        assert_eq!(fit(Some(b"<a><b></a>"), 1, 1, None).unwrap(), b"<a><b></a>");
        assert_eq!(fit(None, 1, 1, None), None);
    }

    /// Camera Raw settings: in a RAW file's XMP they go with their namespace; elsewhere an
    /// `AlreadyApplied` there already is set to "True" and none is added twice.
    #[test]
    fn camera_raw() {
        let raw = String::from_utf8(without_camera_raw(SAMPLE.as_bytes(), 300, 200)).unwrap();
        assert!(!raw.contains("crs") && raw.contains("CC BY-SA 4.0") && raw.contains("exif:PixelXDimension=\"300\""), "{raw}");
        assert!(roxmltree::Document::parse(raw.trim_start_matches(|c| c != '<')).is_ok());
        let two = SAMPLE.replace("<crs:Exposure2012/>", "<crs:AlreadyApplied>False</crs:AlreadyApplied>").replace(
            "</rdf:Description></rdf:RDF>",
            "</rdf:Description><rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:Tint=\"5\"/></rdf:RDF>",
        );
        let out = String::from_utf8(fit(Some(two.as_bytes()), 1, 1, None).unwrap()).unwrap();
        assert!(out.contains("<crs:AlreadyApplied>True</crs:AlreadyApplied>") && out.matches("AlreadyApplied").count() == 2 && out.contains("crs:Tint=\"5\""), "{out}");
        let one = SAMPLE.replace("</rdf:Description></rdf:RDF>", "</rdf:Description><rdf:Description rdf:about=\"\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" crs:Tint=\"5\"/></rdf:RDF>");
        let out = String::from_utf8(fit(Some(one.as_bytes()), 1, 1, None).unwrap()).unwrap();
        assert_eq!(out.matches("crs:AlreadyApplied=\"True\"").count(), 1, "{out}");
        assert!(roxmltree::Document::parse(out.trim_start_matches(|c| c != '<')).is_ok());
    }

    /// A sidecar (#24) in a RAW photo's copy: Camera Raw settings, darktable's history and the
    /// extension it was for go; what the person wrote, darktable's colour labels too, stays.
    #[test]
    fn sidecar_in_a_copy() {
        let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:darktable="http://darktable.sf.net/" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" xmlns:xmp="http://ns.adobe.com/xap/1.0/"
 darktable:history_end="2" darktable:xmp_version="5" photoshop:SidecarForExtension="NEF" xmp:Rating="3">
<darktable:history><rdf:Seq><rdf:li darktable:operation="exposure" darktable:params="0000"/></rdf:Seq></darktable:history>
<darktable:colorlabels><rdf:Seq><rdf:li>2</rdf:li></rdf:Seq></darktable:colorlabels>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let out = String::from_utf8(without_camera_raw(xmp.as_bytes(), 10, 10)).unwrap();
        for gone in ["history", "xmp_version", "SidecarForExtension", "exposure"] {
            assert!(!out.contains(gone), "{gone} in {out}");
        }
        assert!(out.contains("xmp:Rating=\"3\"") && out.contains("<darktable:colorlabels><rdf:Seq><rdf:li>2</rdf:li></rdf:Seq></darktable:colorlabels>"), "{out}");
        assert_eq!(crate::xmp_fields::read(out.as_bytes()).labels, ["Green"]);
    }

    /// A JPEG's sidecar with its own XMP: the sidecar's values first, what only the file has added,
    /// its namespaces declared; still XML.
    #[test]
    fn sidecar_with_the_file() {
        let side = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmp:Rating="5">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">From the sidecar</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let inside = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:xap="http://ns.adobe.com/xap/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:Iptc4xmpCore="http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/"
 xap:Rating="1" xap:Label="Blue &amp; co" Iptc4xmpCore:Location="Lecce">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">From the file</rdf:li></rdf:Alt></dc:title>
<dc:creator><rdf:Seq><rdf:li>Ann</rdf:li></rdf:Seq></dc:creator></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#;
        let out = String::from_utf8(with_sidecar(Some(inside.as_bytes()), side.as_bytes())).unwrap();
        let f = crate::xmp_fields::read(out.as_bytes());
        assert_eq!((f.title.as_deref(), f.rating), (Some("From the sidecar"), Some(5)), "{out}");
        assert_eq!((f.labels, f.creators), (vec!["Blue & co".to_string()], vec!["Ann".to_string()]), "{out}");
        assert!(out.contains("Iptc4xmpCore:Location=\"Lecce\"") && !out.contains("From the file") && !out.contains("\"1\""), "{out}");
        assert_eq!(with_sidecar(None, side.as_bytes()), side.as_bytes());
        assert_eq!(with_sidecar(Some(b"<broken"), side.as_bytes()), side.as_bytes());
    }

    /// IPTC fields go into the XMP when it lacks them, into a new packet when there is none.
    #[test]
    fn iptc_into_xmp() {
        let block = iim(&[(80, "Ann <Author>"), (80, "Bob"), (116, "© Ann"), (25, "a"), (25, "b"), (90, "Québec"), (55, "20240102"), (200, "x")]);
        let out = String::from_utf8(fit(Some(SAMPLE.as_bytes()), 1, 1, Some(&block)).unwrap()).unwrap();
        // dc:creator and dc:rights are there already: not twice.
        assert_eq!(out.matches("dc:creator>").count(), 2, "{out}");
        assert!(out.contains("<dc:subject><rdf:Bag><rdf:li>a</rdf:li><rdf:li>b</rdf:li></rdf:Bag></dc:subject>"), "{out}");
        assert!(out.contains("<photoshop:City>Québec</photoshop:City>") && out.contains("<photoshop:DateCreated>2024-01-02</photoshop:DateCreated>"), "{out}");
        let new = String::from_utf8(fit(None, 1, 1, Some(&block)).unwrap()).unwrap();
        let doc = roxmltree::Document::parse(new.trim_start_matches(|c| c != '<')).unwrap();
        let creators: Vec<_> = doc.descendants().filter(|n| n.tag_name().name() == "li" && n.parent().unwrap().tag_name().name() == "Seq").map(|n| n.text().unwrap()).collect();
        assert_eq!(creators, ["Ann <Author>", "Bob"]);
        assert!(new.contains("<rdf:li xml:lang=\"x-default\">© Ann</rdf:li>"));
    }

    /// The description of a Motion Photo's video goes from every packet written again (no save
    /// writes the video), attributes, elements and its item in the container directory (the gain
    /// map's stays, #54); with `alone` the container directory and the
    /// gain map's description go too; the rest stays.
    #[test]
    fn motion_photo_described_no_more() {
        let xmp = concat!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\" xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" GCamera:MotionPhoto=\"1\" GCamera:MotionPhotoVersion=\"1\" hdrgm:Version=\"1.0\">",
            "<GCamera:MicroVideoOffset>40</GCamera:MicroVideoOffset><dc:creator><rdf:Seq><rdf:li>Ann</rdf:li></rdf:Seq></dc:creator>",
            "<Container:Directory><rdf:Seq><rdf:li rdf:parseType=\"Resource\"><Container:Item Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\"/></rdf:li>",
            "\n  <rdf:li rdf:parseType=\"Resource\"><Container:Item Item:Mime=\"video/mp4\" Item:Semantic=\"MotionPhoto\" Item:Length=\"40\"/></rdf:li></rdf:Seq></Container:Directory>",
            "</rdf:Description></rdf:RDF></x:xmpmeta>"
        );
        let kept = String::from_utf8(fit(Some(xmp.as_bytes()), 8, 6, None).unwrap()).unwrap();
        assert!(!kept.contains("MotionPhoto") && !kept.contains("MicroVideo"), "{kept}");
        assert!(kept.contains("Container:Directory") && kept.contains("Item:Semantic=\"GainMap\"") && kept.contains("hdrgm:Version") && kept.contains("Ann"), "{kept}");
        assert!(!kept.contains("video/mp4") && kept.contains("</rdf:li></rdf:Seq>"), "{kept}");
        let alone = String::from_utf8(alone(xmp.as_bytes(), 8, 6)).unwrap();
        assert!(!alone.contains("Container:") && !alone.contains("hdrgm:Version") && alone.contains("Ann"), "{alone}");
    }
}
