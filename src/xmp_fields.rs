//! What a person wrote about a photo in its XMP (title, caption, keywords, rating, colour label,
//! author, copyright) and where it was taken, for the metadata panel's Description box: from the
//! sidecar of a RAW photo (`sidecar`) or the XMP inside the file (#24). Read with `roxmltree`; a
//! property can be an attribute of `rdf:Description` or an element, a list (`rdf:Seq`,
//! `rdf:Bag`) or text in languages (`rdf:Alt`).

use roxmltree::{Document, Node};

use crate::meta::{Group, Meta};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const XAP: &str = "http://ns.adobe.com/xap/1.0/";
const EXIF: &str = "http://ns.adobe.com/exif/1.0/";
const PHOTOSHOP: &str = "http://ns.adobe.com/photoshop/1.0/";
pub(crate) const DARKTABLE: &str = "http://darktable.sf.net/";

#[derive(Debug, Default, PartialEq)]
pub struct Fields {
    pub title: Option<String>,
    pub caption: Option<String>,
    pub keywords: Vec<String>,
    /// 1 to 5 stars, -1 rejected; 0 (no rating) is left out.
    pub rating: Option<i32>,
    /// Lightroom's label (its name, as the label set writes it) or darktable's colours.
    pub labels: Vec<String>,
    pub creators: Vec<String>,
    pub rights: Option<String>,
    /// Latitude and longitude in decimal degrees (negative to the south and west).
    pub gps: Option<(f64, f64)>,
    /// Metres above the sea (negative below).
    pub altitude: Option<f64>,
    /// `photoshop:SidecarForExtension`: the extension of the file a `foo.xmp` belongs to.
    pub sidecar_for: Option<String>,
    /// `photoshop:DateCreated`, as written: when the photo was taken, for the Shot box (`mwg`).
    pub taken: Option<String>,
}

impl Fields {
    /// Nothing for the Description box (the date goes to the Shot box).
    pub fn is_empty(&self) -> bool {
        self == &Fields { sidecar_for: self.sidecar_for.clone(), taken: self.taken.clone(), ..Fields::default() }
    }
}

/// darktable's colour labels, by their number in `darktable:colorlabels`.
const DARKTABLE_LABELS: [&str; 5] = ["red", "yellow", "green", "blue", "purple"];

/// The fields of an XMP packet; empty if it is not XML.
pub fn read(xmp: &[u8]) -> Fields {
    let text = String::from_utf8_lossy(xmp);
    // What comes before the first tag (a byte order mark, the padding of a packet) is not XML.
    let text = text.trim_start_matches(|c| c != '<').trim_end_matches(|c| c != '>');
    let Ok(doc) = Document::parse(text) else { return Fields::default() };
    let mut f = Fields::default();
    let first = |ns: &str, name: &str| values(&doc, ns, name).into_iter().find(|v| !v.is_empty());
    f.title = first(DC, "title");
    f.caption = first(DC, "description");
    f.keywords = values(&doc, DC, "subject").into_iter().filter(|v| !v.is_empty()).collect();
    f.creators = values(&doc, DC, "creator").into_iter().filter(|v| !v.is_empty()).collect();
    f.rights = first(DC, "rights");
    f.rating = first(XAP, "Rating").and_then(|r| r.parse::<f64>().ok()).map(|r| r.round() as i32).filter(|&r| r != 0 && (-1..=5).contains(&r));
    f.labels = first(XAP, "Label").into_iter().collect();
    for n in values(&doc, DARKTABLE, "colorlabels") {
        if let Some(name) = n.parse::<usize>().ok().and_then(|i| DARKTABLE_LABELS.get(i)) {
            let name = format!("{}{}", name[..1].to_uppercase(), &name[1..]);
            if !f.labels.iter().any(|l| l.eq_ignore_ascii_case(&name)) {
                f.labels.push(name);
            }
        }
    }
    let lat = first(EXIF, "GPSLatitude").and_then(|v| coord(&v));
    let lon = first(EXIF, "GPSLongitude").and_then(|v| coord(&v));
    f.gps = lat.zip(lon);
    let below = first(EXIF, "GPSAltitudeRef").is_some_and(|r| r == "1");
    f.altitude = first(EXIF, "GPSAltitude").and_then(|v| rational(&v)).map(|a| if below { -a } else { a });
    f.sidecar_for = first(PHOTOSHOP, "SidecarForExtension");
    f.taken = first(PHOTOSHOP, "DateCreated");
    f
}

/// Every value of the property `ns`:`name`, wherever a description has it: an attribute, an
/// element's text, or the items of its list (for `rdf:Alt`, the default language first).
fn values(doc: &Document, ns: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for d in doc.descendants().filter(|n| n.has_tag_name((RDF, "Description"))) {
        if let Some(v) = d.attribute((ns, name)) {
            out.push(v.trim().to_string());
        }
        for p in d.children().filter(|n| n.has_tag_name((ns, name))) {
            out.extend(items(p));
        }
    }
    out
}

/// The text of a property element, or of its list's items.
fn items(p: Node) -> Vec<String> {
    let Some(list) = p.children().find(|n| n.is_element() && n.tag_name().namespace() == Some(RDF)) else {
        return p.text().map(|t| t.trim().to_string()).into_iter().collect();
    };
    let mut li: Vec<Node> = list.children().filter(|n| n.has_tag_name((RDF, "li"))).collect();
    if list.has_tag_name((RDF, "Alt")) {
        // The default language first ("x-default"), as the XMP specification asks readers.
        li.sort_by_key(|n| n.attribute(("http://www.w3.org/XML/1998/namespace", "lang")) != Some("x-default"));
        li.truncate(1);
    }
    li.into_iter().map(|n| n.text().unwrap_or("").trim().to_string()).collect()
}

/// An XMP GPS coordinate, "DDD,MM,SSk" or "DDD,MM.mmk" (k: N, S, E or W), in decimal degrees.
fn coord(v: &str) -> Option<f64> {
    let v = v.trim();
    let last = v.chars().last()?;
    let dir = last.to_ascii_uppercase();
    let parts: Vec<f64> = v[..v.len() - last.len_utf8()].split(',').map(|p| p.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
    let (&d, rest) = parts.split_first()?;
    let deg = d + rest.first().unwrap_or(&0.0) / 60.0 + rest.get(1).unwrap_or(&0.0) / 3600.0;
    match dir {
        'N' | 'E' => Some(deg),
        'S' | 'W' => Some(-deg),
        _ => None,
    }
}

/// "1234/10" or "123.4".
fn rational(v: &str) -> Option<f64> {
    match v.split_once('/') {
        Some((n, d)) => Some(n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok().filter(|d| *d != 0.0)?),
        None => v.trim().parse().ok(),
    }
}

/// The Description box of the panel from `f`, and its position where the file's EXIF has none
/// (`sidecar`: the name of the sidecar it was read from, whose position counts over the EXIF's:
/// it is what the person set in Lightroom or darktable).
pub fn add_to(meta: &mut Meta, f: &Fields, sidecar: Option<&str>) {
    let mut add = |label: &'static str, value: String| meta.fields.push((Group::Info, label, value));
    if let Some(t) = &f.title {
        add("meta-xmp-title", t.clone());
    }
    if let Some(c) = &f.caption {
        add("meta-xmp-caption", c.clone());
    }
    if !f.keywords.is_empty() {
        add("meta-xmp-keywords", f.keywords.join(", "));
    }
    match f.rating {
        Some(-1) => add("meta-xmp-rating", "meta-xmp-rejected".into()),
        Some(r) => add("meta-xmp-rating", format!("{r} / 5")),
        None => {}
    }
    if !f.labels.is_empty() {
        add("meta-xmp-label", f.labels.iter().map(|l| label_id(l).map_or_else(|| l.clone(), str::to_string)).collect::<Vec<_>>().join(", "));
    }
    if !f.creators.is_empty() {
        add("meta-xmp-creator", f.creators.join(", "));
    }
    if let Some(r) = &f.rights {
        add("meta-xmp-rights", r.clone());
    }
    if let Some(s) = sidecar {
        add("meta-xmp-sidecar", s.to_string());
    }
    if f.gps.is_some() && (sidecar.is_some() || meta.gps.is_none()) {
        meta.gps = f.gps;
        if let Some(a) = f.altitude {
            meta.fields.retain(|(_, l, _)| *l != "meta-altitude");
            meta.fields.push((Group::Gps, "meta-altitude", format!("{} m", (a * 10.0).round() / 10.0)));
        }
    }
}

/// The Fluent id of a colour label Lightroom or darktable names in English.
fn label_id(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "red" => "meta-label-red",
        "yellow" => "meta-label-yellow",
        "green" => "meta-label-green",
        "blue" => "meta-label-blue",
        "purple" => "meta-label-purple",
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// As Lightroom Classic writes a CR2's sidecar: properties as attributes and as elements.
    pub(crate) const LIGHTROOM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 7.0">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/"
    xmlns:exif="http://ns.adobe.com/exif/1.0/"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   xmp:Rating="4" xmp:Label="Red" photoshop:SidecarForExtension="CR2"
   exif:GPSLatitude="40,21.0678N" exif:GPSLongitude="18,10.2W" exif:GPSAltitude="1234/10" exif:GPSAltitudeRef="1"
   crs:Exposure2012="+0.50">
   <dc:title><rdf:Alt><rdf:li xml:lang="it">Faro</rdf:li><rdf:li xml:lang="x-default">Lighthouse</rdf:li></rdf:Alt></dc:title>
   <dc:description><rdf:Alt><rdf:li xml:lang="x-default">At dawn &amp; calm sea</rdf:li></rdf:Alt></dc:description>
   <dc:subject><rdf:Bag><rdf:li>sea</rdf:li><rdf:li>lighthouse</rdf:li></rdf:Bag></dc:subject>
   <dc:creator><rdf:Seq><rdf:li>Ann Author</rdf:li></rdf:Seq></dc:creator>
   <dc:rights><rdf:Alt><rdf:li xml:lang="x-default">CC BY-SA 4.0</rdf:li></rdf:Alt></dc:rights>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn lightroom_sidecar() {
        let f = read(LIGHTROOM.as_bytes());
        assert_eq!(f.title.as_deref(), Some("Lighthouse"));
        assert_eq!(f.caption.as_deref(), Some("At dawn & calm sea"));
        assert_eq!(f.keywords, ["sea", "lighthouse"]);
        assert_eq!((f.rating, f.labels.clone(), f.creators.clone()), (Some(4), vec!["Red".to_string()], vec!["Ann Author".to_string()]));
        assert_eq!(f.rights.as_deref(), Some("CC BY-SA 4.0"));
        assert_eq!(f.sidecar_for.as_deref(), Some("CR2"));
        let (lat, lon) = f.gps.unwrap();
        assert!((lat - (40.0 + 21.0678 / 60.0)).abs() < 1e-9 && (lon + (18.0 + 10.2 / 60.0)).abs() < 1e-9, "{lat} {lon}");
        assert_eq!(f.altitude, Some(-123.4));
    }

    /// As darktable writes `foo.NEF.xmp`: a packet, the colour labels as numbers, a rejected photo.
    #[test]
    fn darktable_sidecar() {
        let xmp = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:darktable="http://darktable.sf.net/" xmlns:dc="http://purl.org/dc/elements/1.1/"
 xmp:Rating="-1" darktable:history_end="2">
 <darktable:colorlabels><rdf:Seq><rdf:li>0</rdf:li><rdf:li>3</rdf:li></rdf:Seq></darktable:colorlabels>
 <dc:subject><rdf:Bag><rdf:li>darktable|format|nef</rdf:li></rdf:Bag></dc:subject>
</rdf:Description></rdf:RDF></x:xmpmeta>
<?xpacket end="w"?>"#;
        let f = read(xmp.as_bytes());
        assert_eq!((f.rating, f.labels.clone()), (Some(-1), vec!["Red".to_string(), "Blue".to_string()]));
        let mut m = Meta::default();
        add_to(&mut m, &f, Some("a.NEF.xmp"));
        let rows: Vec<_> = m.fields.iter().map(|(g, l, v)| (*g, *l, v.as_str())).collect();
        assert!(rows.contains(&(Group::Info, "meta-xmp-rating", "meta-xmp-rejected")), "{rows:?}");
        assert!(rows.contains(&(Group::Info, "meta-xmp-label", "meta-label-red, meta-label-blue")), "{rows:?}");
        assert!(rows.contains(&(Group::Info, "meta-xmp-sidecar", "a.NEF.xmp")), "{rows:?}");
    }

    /// The sidecar's position counts over the EXIF's; the file's own XMP only fills in.
    #[test]
    fn position() {
        let f = read(LIGHTROOM.as_bytes());
        let exif = Meta { gps: Some((1.0, 2.0)), fields: vec![(Group::Gps, "meta-altitude", "5 m".into())] };
        let mut m = exif.clone();
        add_to(&mut m, &f, None);
        assert_eq!(m.gps, Some((1.0, 2.0)));
        let mut m = exif.clone();
        add_to(&mut m, &f, Some("a.xmp"));
        assert_eq!(m.gps, f.gps);
        assert!(m.fields.contains(&(Group::Gps, "meta-altitude", "-123.4 m".into())), "{:?}", m.fields);
        assert_eq!(m.fields.iter().filter(|(_, l, _)| *l == "meta-altitude").count(), 1);
    }

    #[test]
    fn nothing() {
        assert!(read(b"not xml").is_empty());
        assert!(read(b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>").is_empty());
        assert_eq!(coord("40,21,4.07N").map(|c| (c * 1e6).round()), Some(40351131.0));
        assert_eq!(coord("12,30.5X"), None);
        // A last character of more than one byte: not a coordinate, and no cut in the middle of it.
        assert_eq!(coord("12,30.5É"), None);
        assert_eq!(coord("°"), None);
        assert_eq!(rational("7/0"), None);
    }
}
