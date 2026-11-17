//! The extended XMP of a JPEG (XMP Specification Part 3, "JPEG"): an XMP too large for one APP1
//! segment is written as a main packet whose `xmpNote:HasExtendedXMP` names the rest by the MD5
//! digest of its text (the GUID), and that rest in segments of its own (`meta_write`). What goes
//! into the rest, as Adobe's XMP Toolkit chooses it (`PackageForJPEG`): the Camera Raw settings,
//! then Photoshop's history, then the largest properties first, until the main packet fits. What
//! a reader needs to find the parts of the file (an Ultra HDR gain map, a Motion Photo's video)
//! stays in the main packet. Also the two parts read back as one packet (`with_extended`).

use std::cmp::Reverse;
use std::ops::Range;

use crate::xmp::{CONTAINER, CRS, GCAMERA, HDRGM, ITEM, PHOTOSHOP, RDF, XMP_NOTE};

/// A property of an `rdf:Description`: where it is in the text, its namespace and name, the
/// description it belongs to, its value when it is an attribute.
struct Prop {
    range: Range<usize>,
    ns: String,
    local: String,
    desc: usize,
    value: Option<String>,
}

/// The XML of a packet: from its first `<` to its last `>` (a BOM or padding around it), and
/// where that starts.
fn xml(xmp: &[u8]) -> Option<(&str, usize)> {
    let text = std::str::from_utf8(xmp).ok()?;
    let start = text.find('<')?;
    let end = text.rfind('>')? + 1;
    (start < end).then(|| (&text[start..end], start))
}

/// The descriptions of `doc` and their properties: attributes (not the RDF ones) and elements.
fn props<'a, 'i>(doc: &'a roxmltree::Document<'i>) -> (Vec<roxmltree::Node<'a, 'i>>, Vec<Prop>) {
    let descs: Vec<_> = doc.descendants().filter(|n| n.has_tag_name((RDF, "Description")) && n.parent().is_some_and(|p| p.has_tag_name((RDF, "RDF")))).collect();
    let mut out = Vec::new();
    for (k, d) in descs.iter().enumerate() {
        for a in d.attributes().filter(|a| a.namespace().is_some_and(|ns| ns != RDF)) {
            out.push(Prop { range: a.range(), ns: a.namespace().unwrap_or("").into(), local: a.name().into(), desc: k, value: Some(a.value().into()) });
        }
        for c in d.children().filter(|c| c.is_element()) {
            out.push(Prop { range: c.range(), ns: c.tag_name().namespace().unwrap_or("").into(), local: c.tag_name().name().into(), desc: k, value: None });
        }
    }
    (descs, out)
}

fn has_extended(p: &Prop) -> bool {
    p.ns == XMP_NOTE && p.local == "HasExtendedXMP"
}

/// What stays in the main packet: what describes the parts of the file, found there by Android
/// and Skia.
fn stays(p: &Prop) -> bool {
    [CONTAINER, ITEM, HDRGM].contains(&p.ns.as_str()) || (p.ns == GCAMERA && (p.local.starts_with("MotionPhoto") || p.local.starts_with("MicroVideo")))
}

/// `text` without `cut`, each with the blanks before it.
fn without(text: &str, mut cut: Vec<Range<usize>>) -> String {
    cut.sort_by_key(|r| r.start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for r in cut {
        let start = r.start - text[..r.start].chars().rev().take_while(|c| c.is_ascii_whitespace()).count();
        out.push_str(&text[at..start.max(at)]);
        at = r.end;
    }
    out.push_str(&text[at..]);
    out
}

/// The main packet `main` with its extended part `ext` (both as read from the JPEG) put back
/// into it, the main packet's values first, its GUID gone; `None` if either cannot be read.
pub fn with_extended(main: &[u8], ext: &[u8]) -> Option<Vec<u8>> {
    let (text, start) = xml(main)?;
    let doc = roxmltree::Document::parse(text).ok()?;
    roxmltree::Document::parse(xml(ext)?.0).ok()?;
    let guid = props(&doc).1.into_iter().filter(has_extended).map(|p| p.range).collect();
    let main = [&main[..start], without(text, guid).as_bytes(), &main[start + text.len()..]].concat();
    Some(crate::xmp::with_sidecar(Some(ext), &main))
}

/// `xmp` as a main packet of at most `limit` bytes, naming its extended part, and that part;
/// `None` when it cannot be read, or the main packet would not fit even with all it can lose.
pub fn split(xmp: &[u8], limit: usize) -> Option<(Vec<u8>, Vec<u8>)> {
    let (text, start) = xml(xmp)?;
    let doc = roxmltree::Document::parse(text).ok()?;
    let (descs, props) = props(&doc);
    let note = |guid: &str| format!("<rdf:Description rdf:about=\"\" xmlns:xmpNote=\"{XMP_NOTE}\" xmpNote:HasExtendedXMP=\"{guid}\"/>");
    let mut size = xmp.len() + note(&"0".repeat(32)).len();
    // An old GUID goes in any case: it would name another extended part.
    let mut cut: Vec<Range<usize>> = props.iter().filter(|p| has_extended(p)).map(|p| p.range.clone()).collect();
    size -= cut.iter().map(|r| r.len()).sum::<usize>();
    let rank = |p: &Prop| if p.ns == CRS { 0 } else if p.ns == PHOTOSHOP && p.local == "History" { 1 } else { 2 };
    let mut order: Vec<&Prop> = props.iter().filter(|p| !has_extended(p) && !stays(p)).collect();
    order.sort_by_key(|p| (rank(p), Reverse(p.range.len())));
    let mut moved = Vec::new();
    for p in order {
        if size <= limit {
            break;
        }
        size -= p.range.len();
        cut.push(p.range.clone());
        moved.push(p);
    }
    if size > limit {
        return None;
    }
    let ext = extended(&descs, text, &moved);
    let guid: String = md5::compute(&ext).0.iter().map(|b| format!("{b:02X}")).collect();
    let main = without(text, cut);
    let at = main.rfind("</rdf:RDF>")?;
    let main = [&xmp[..start], &main.as_bytes()[..at], note(&guid).as_bytes(), &main.as_bytes()[at..], &xmp[start + text.len()..]].concat();
    Some((main, ext.into_bytes()))
}

/// The extended part: the `moved` properties, in a description for each of theirs with its
/// namespaces, without a packet wrapper (as the specification asks).
fn extended(descs: &[roxmltree::Node], text: &str, moved: &[&Prop]) -> String {
    let esc = |v: &str| quick_xml::escape::escape(v).into_owned();
    let mut out = format!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"{RDF}\">");
    for (k, d) in descs.iter().enumerate() {
        let mine: Vec<&&Prop> = moved.iter().filter(|p| p.desc == k).collect();
        if mine.is_empty() {
            continue;
        }
        let prefix = |uri: &str| d.namespaces().find(|n| n.uri() == uri).and_then(|n| n.name());
        let decls: String = d.namespaces().filter_map(|n| Some(format!(" xmlns:{}=\"{}\"", n.name()?, esc(n.uri())))).filter(|x| !x.starts_with(" xmlns:xml=") && !x.starts_with(" xmlns:rdf=")).collect();
        let attrs: String = mine.iter().filter_map(|p| Some(format!(" {}:{}=\"{}\"", prefix(&p.ns)?, p.local, esc(p.value.as_deref()?)))).collect();
        let elements: String = mine.iter().filter(|p| p.value.is_none()).map(|p| &text[p.range.clone()]).collect();
        out += &format!("<rdf:Description rdf:about=\"\"{decls}{attrs}>{elements}</rdf:Description>");
    }
    out + "</rdf:RDF></x:xmpmeta>"
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A packet of a phone photo with a large note of the maker, Camera Raw settings, a title, and
    /// the description of its gain map.
    pub(crate) fn large() -> String {
        format!(
            concat!(
                "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"{rdf}\">",
                "<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:crs=\"{crs}\" xmlns:GCamera=\"{gc}\" ",
                "xmlns:hdrgm=\"{hdrgm}\" xmlns:Container=\"{container}\" xmlns:Item=\"{item}\" xmlns:xmpNote=\"{note}\" ",
                "hdrgm:Version=\"1.0\" GCamera:MotionPhoto=\"1\" GCamera:HDRPlusMakerNote=\"{big}\" crs:Exposure2012=\"+0.5\" xmpNote:HasExtendedXMP=\"0123456789ABCDEF0123456789ABCDEF\">",
                "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Sea &amp; sun</rdf:li></rdf:Alt></dc:title>",
                "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{long}</rdf:li></rdf:Alt></dc:description>",
                "<Container:Directory><rdf:Seq><rdf:li rdf:parseType=\"Resource\"><Container:Item Item:Semantic=\"GainMap\" Item:Length=\"100\"/></rdf:li></rdf:Seq></Container:Directory>",
                "</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>"
            ),
            rdf = RDF, crs = CRS, gc = GCAMERA, hdrgm = HDRGM, container = CONTAINER, item = ITEM, note = XMP_NOTE,
            big = "A".repeat(80_000),
            long = "b".repeat(30_000)
        )
    }

    /// Split for a 64 KB segment: the main packet fits and names the extended part by its MD5;
    /// the largest property and Camera Raw moved, the description of the gain map and the video
    /// kept; put back together, every property is there once.
    #[test]
    fn split_and_back() {
        let xmp = large();
        let (main, ext) = split(xmp.as_bytes(), 65502).expect("split");
        let (m, e) = (String::from_utf8(main.clone()).unwrap(), String::from_utf8(ext.clone()).unwrap());
        assert!(main.len() <= 65502, "{}", main.len());
        let guid: String = md5::compute(&ext).0.iter().map(|b| format!("{b:02X}")).collect();
        assert!(m.contains(&format!("xmpNote:HasExtendedXMP=\"{guid}\"")) && !m.contains("0123456789ABCDEF0123456789ABCDEF"), "old GUID");
        for kept in ["hdrgm:Version=\"1.0\"", "GCamera:MotionPhoto=\"1\"", "Item:Semantic=\"GainMap\"", "Sea &amp; sun", "bbbb"] {
            assert!(m.contains(kept), "{kept} not in the main packet");
        }
        assert!(e.contains("HDRPlusMakerNote") && e.contains("crs:Exposure2012=\"+0.5\"") && !m.contains("HDRPlusMakerNote") && !m.contains("Exposure2012"));
        assert!(!e.contains("<?xpacket") && roxmltree::Document::parse(&e).is_ok() && roxmltree::Document::parse(xml(&main).unwrap().0).is_ok());
        let whole = String::from_utf8(with_extended(&main, &ext).unwrap()).unwrap();
        for once in ["HDRPlusMakerNote=", "Exposure2012=", "<dc:title>", "<dc:description>", "<Container:Directory>"] {
            assert_eq!(whole.matches(once).count(), 1, "{once}");
        }
        assert!(!whole.contains("HasExtendedXMP"), "the GUID names nothing once put back");
        let f = crate::xmp_fields::read(&whole.into_bytes());
        assert_eq!(f.title.as_deref(), Some("Sea & sun"));
        // Too large even without what can go: none.
        assert!(split(b"<x:xmpmeta/>", 10).is_none());
    }

    /// A photo whose XMP is larger than a JPEG segment, written as JPEG: the main packet and its
    /// extended part, each property read back once, in a JPEG and from it in a PNG (before, the
    /// XMP was left out whole).
    #[test]
    fn large_xmp_in_a_jpeg() {
        use crate::encode::{Format, Options, Original};
        use image::DynamicImage;
        let photo = DynamicImage::new_rgb8(8, 6);
        let mut png = Vec::new();
        photo.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let png = crate::meta_write::png_chunks(png, Some(large().as_bytes()), None);
        let jpeg = Original::with_meta(photo.clone(), &png).encode(&Options { format: Format::Jpeg, ..Default::default() }).unwrap();
        let ext = crate::ultrahdr::segments(&jpeg).into_iter().filter(|&(m, s, e)| m == 0xE1 && jpeg[s + 4..e].starts_with(crate::meta_write::JPEG_XMP_EXT)).count();
        assert_eq!(ext, 2, "80 KB in two pieces");
        let whole = String::from_utf8(crate::meta_write::jpeg_xmp_whole(&jpeg).unwrap()).unwrap();
        assert_eq!(whole.matches("HDRPlusMakerNote=").count(), 1);
        assert_eq!(crate::xmp_fields::read(whole.as_bytes()).title.as_deref(), Some("Sea & sun"));
        let back = Original::with_meta(photo, &jpeg).encode(&Options { format: Format::Png, ..Default::default() }).unwrap();
        let text = String::from_utf8_lossy(&back);
        assert!(text.matches("HDRPlusMakerNote=").count() == 1 && text.contains(&"A".repeat(80_000)) && !text.contains("HasExtendedXMP"));
    }
}
