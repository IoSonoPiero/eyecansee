//! The XMP of an image being written again (Rotate and crop, Save, Convert and resize, Save in
//! another format), with `quick-xml`: sizes and orientation made true for the pixels written, the
//! XMP thumbnail, the Camera Raw / Lightroom settings (`crs:`, crop and adjustments already in
//! the pixels) and the description of a Motion Photo's video (never written again) taken out, everything else (author, copyright, licence, descriptions, keywords…)
//! kept as it was. For WebP and AVIF, which have no place for IPTC, the IPTC fields go into the
//! XMP as Adobe and the IPTC Core map them, when the XMP does not have them already.

use quick_xml::Writer;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;

const CRS: &str = "http://ns.adobe.com/camera-raw-settings/1.0/";
const XAP: &str = "http://ns.adobe.com/xap/1.0/";
const TIFF: &str = "http://ns.adobe.com/tiff/1.0/";
const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
const XMP_NOTE: &str = "http://ns.adobe.com/xmp/note/";
const GCAMERA: &str = "http://ns.google.com/photos/1.0/camera/";
const CONTAINER: &str = "http://ns.google.com/photos/1.0/container/";
const HDRGM: &str = "http://ns.adobe.com/hdr-gain-map/1.0/";

/// What happens to a property: dropped, or its value replaced.
enum Fix {
    Drop,
    Set(String),
}

/// `merged`: the extended XMP is in the packet, the GUID that pointed to it goes. The video of a
/// Motion Photo is never written again, so what describes it always goes (also when the original
/// says so without having one, as a Pixel 8 photo seen). `alone`: the picture is written without
/// the gain map of an Ultra HDR photo either, so its description and the container directory go.
fn fix(ns: &str, local: &str, w: u32, h: u32, merged: bool, alone: bool) -> Option<Fix> {
    match (ns, local) {
        (XMP_NOTE, "HasExtendedXMP") if merged => Some(Fix::Drop),
        (GCAMERA, l) if l.starts_with("MotionPhoto") || l.starts_with("MicroVideo") => Some(Fix::Drop),
        (CONTAINER, "Directory") | (HDRGM, _) if alone => Some(Fix::Drop),
        (CRS, _) | (XAP, "Thumbnails") => Some(Fix::Drop),
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
    fit_with(xmp, w, h, iim, None)
}

/// As `fit`, with the descriptions of the extended XMP `ext` of a JPEG (`meta_write::xmp_ext_packet`)
/// put into the main packet, for a format whose XMP has no size limit; if that cannot be read,
/// the main packet alone.
pub fn fit_with(xmp: Option<&[u8]>, w: u32, h: u32, iim: Option<&[u8]>, ext: Option<&[u8]>) -> Option<Vec<u8>> {
    let props = iim.map(iim_props).unwrap_or_default();
    let xmp = match xmp {
        Some(x) => x,
        None if props.is_empty() => return None,
        None => EMPTY.as_bytes(),
    };
    // ponytail: by text, with the "rdf" prefix every writer uses; a packet using another one keeps only its main part.
    let inner = |x: &[u8]| {
        let start = x.windows(8).position(|w| w == b"<rdf:RDF")?;
        let start = start + x[start..].iter().position(|&b| b == b'>')? + 1;
        let end = x.windows(10).rposition(|w| w == b"</rdf:RDF>")?;
        (start <= end).then_some((start, end))
    };
    let merged = ext.zip(inner(xmp)).zip(ext.and_then(inner)).map(|((ext, (_, at)), (s, e))| [&xmp[..at], &ext[s..e], &xmp[at..]].concat());
    if let Some(out) = merged.and_then(|m| rewrite(&m, w, h, &props, true, false)) {
        return Some(out);
    }
    Some(rewrite(xmp, w, h, &props, false, false).unwrap_or_else(|| xmp.to_vec()))
}

/// `xmp` without what describes the parts after the picture in the file (Motion Photo video,
/// Ultra HDR gain map and its container directory), for a picture written alone; sizes set to
/// `w` × `h` as `fit` does. A packet that is not well-formed XML is returned as it was.
pub fn alone(xmp: &[u8], w: u32, h: u32) -> Vec<u8> {
    rewrite(xmp, w, h, &[], false, true).unwrap_or_else(|| xmp.to_vec())
}

fn rewrite(xmp: &[u8], w: u32, h: u32, props: &[Prop], merged: bool, alone: bool) -> Option<Vec<u8>> {
    let mut r = NsReader::from_reader(xmp);
    let mut out = Writer::new(Vec::with_capacity(xmp.len()));
    // Depth inside a dropped element; the new value for the text of the element being read, and
    // whether its old text is being replaced.
    let (mut skip, mut set, mut replacing) = (0usize, None::<String>, false);
    let mut seen: Vec<(String, String)> = Vec::new();
    loop {
        let (ns, event) = r.read_resolved_event().ok()?;
        let ns = bound(&ns);
        match event {
            Event::Start(_) | Event::Empty(_) if skip > 0 => skip += matches!(event, Event::Start(_)) as usize,
            Event::End(_) if skip > 0 => skip -= 1,
            _ if skip > 0 => {}
            Event::Start(ref e) | Event::Empty(ref e) if matches!(fix(&ns, e.local_name().as_ref(), w, h, merged, alone), Some(Fix::Drop)) => {
                skip = matches!(event, Event::Start(_)) as usize;
            }
            Event::Start(ref e) | Event::Empty(ref e) => {
                seen.push((ns.clone(), e.local_name().as_ref().to_string()));
                if let Some(Fix::Set(v)) = fix(&ns, e.local_name().as_ref(), w, h, merged, alone)
                    && matches!(event, Event::Start(_))
                {
                    (set, replacing) = (Some(v), true);
                }
                let e = attributes(&r, e, w, h, (merged, alone), &mut seen)?;
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

/// The attributes of an element, without those of Camera Raw (and its namespace) and with the
/// sizes and orientation set; the element as it was when nothing changes.
fn attributes<'a>(r: &NsReader<&[u8]>, e: &BytesStart<'a>, w: u32, h: u32, (merged, alone): (bool, bool), seen: &mut Vec<(String, String)>) -> Option<BytesStart<'a>> {
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut changed = false;
    for a in e.attributes() {
        let a = a.ok()?;
        let value = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok()?.into_owned();
        let key = a.key.as_ref().to_string();
        if a.key.as_namespace_binding().is_some() {
            if value == CRS {
                changed = true;
            } else {
                kept.push((key, value));
            }
            continue;
        }
        let (ns, local) = r.resolver().resolve_attribute(a.key);
        let ns = bound(&ns);
        seen.push((ns.clone(), local.as_ref().to_string()));
        match fix(&ns, local.as_ref(), w, h, merged, alone) {
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
    dc: bool,
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
            None => props.push(Prop { dc, name, kind, values: vec![text] }),
        }
    }
    props
}

/// An `rdf:Description` with the IPTC fields the XMP does not have yet (empty if none).
fn description(props: &[Prop], seen: &[(String, String)]) -> String {
    let missing: Vec<&Prop> = props.iter().filter(|p| !seen.iter().any(|(ns, l)| ns == if p.dc { DC } else { PHOTOSHOP } && l == p.name)).collect();
    if missing.is_empty() {
        return String::new();
    }
    let esc = |s: &str| quick_xml::escape::escape(s).into_owned();
    let mut d = format!("<rdf:Description rdf:about=\"\" xmlns:dc=\"{DC}\" xmlns:photoshop=\"{PHOTOSHOP}\">");
    for p in missing {
        let name = format!("{}:{}", if p.dc { "dc" } else { "photoshop" }, p.name);
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

    /// Sizes and orientation set in attributes and elements; Camera Raw, its namespace and the
    /// thumbnail gone; author, rights, the Ultra HDR item and the packet wrapper as they were.
    #[test]
    fn fitted() {
        let out = String::from_utf8(fit(Some(SAMPLE.as_bytes()), 300, 200, None).unwrap()).unwrap();
        for gone in ["crs", "Thumbnails", "/9j/", "4000", "3000", "\"6\""] {
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
    /// writes the video), attributes and elements; with `alone` the container directory and the
    /// gain map's description go too; the rest stays.
    #[test]
    fn motion_photo_described_no_more() {
        let xmp = concat!(
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
            "<rdf:Description xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\" xmlns:hdrgm=\"http://ns.adobe.com/hdr-gain-map/1.0/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" GCamera:MotionPhoto=\"1\" GCamera:MotionPhotoVersion=\"1\" hdrgm:Version=\"1.0\">",
            "<GCamera:MicroVideoOffset>40</GCamera:MicroVideoOffset><dc:creator><rdf:Seq><rdf:li>Ann</rdf:li></rdf:Seq></dc:creator>",
            "<Container:Directory><rdf:Seq><rdf:li rdf:parseType=\"Resource\"><Container:Item Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\"/></rdf:li></rdf:Seq></Container:Directory>",
            "</rdf:Description></rdf:RDF></x:xmpmeta>"
        );
        let kept = String::from_utf8(fit(Some(xmp.as_bytes()), 8, 6, None).unwrap()).unwrap();
        assert!(!kept.contains("MotionPhoto") && !kept.contains("MicroVideo"), "{kept}");
        assert!(kept.contains("Container:Directory") && kept.contains("hdrgm:Version") && kept.contains("Ann"), "{kept}");
        let alone = String::from_utf8(alone(xmp.as_bytes(), 8, 6)).unwrap();
        assert!(!alone.contains("Container:") && !alone.contains("hdrgm:Version") && alone.contains("Ann"), "{alone}");
    }
}
