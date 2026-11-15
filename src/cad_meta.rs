//! The data that identify a CAD drawing (#144): the drawing properties stored in DWG and DXF
//! (title, subject, author, keywords, comments, revision, custom properties, as AutoCAD's
//! DWGPROPS) and the attributes of its title block, for the details panel and the line under
//! the thumbnail.
// ponytail: the title block is the block with the most filled attributes on the first layout that has one

use acadrust::CadDocument;
use acadrust::entities::EntityType;

/// What a drawing says of itself, in order: the properties (Fluent id of the label, value), then
/// the title block's attributes (its tag, value).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Props {
    pub properties: Vec<(&'static str, String)>,
    pub custom: Vec<(String, String)>,
    pub title_block: Vec<(String, String)>,
}

/// The properties and the title block of `doc`.
pub(crate) fn props(doc: &CadDocument) -> Props {
    let s = &doc.summary_info;
    let properties = [
        ("cad-prop-title", &s.title),
        ("cad-prop-subject", &s.subject),
        ("cad-prop-author", &s.author),
        ("cad-prop-keywords", &s.keywords),
        ("cad-prop-comments", &s.comments),
        ("cad-prop-revision", &s.revision_number),
        ("cad-prop-saved-by", &s.last_saved_by),
    ]
    .into_iter()
    .filter(|(_, v)| !v.trim().is_empty())
    .map(|(l, v)| (l, v.trim().to_string()))
    .collect();
    let custom = s.custom_properties.iter().filter(|(n, v)| !n.trim().is_empty() && !v.trim().is_empty()).map(|(n, v)| (n.trim().to_string(), v.trim().to_string())).collect();
    Props { properties, custom, title_block: title_block(doc) }
}

/// The attributes (tag, value) of the title block: on the first layout, in tab order, with a
/// block that has some, the one with the most filled in; without layouts, in the model space.
fn title_block(doc: &CadDocument) -> Vec<(String, String)> {
    let mut layouts: Vec<&acadrust::objects::Layout> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            acadrust::objects::ObjectType::Layout(l) if !l.name.eq_ignore_ascii_case("Model") => Some(l),
            _ => None,
        })
        .collect();
    layouts.sort_by_key(|l| l.tab_order);
    let sheets = layouts.iter().filter_map(|l| doc.block_records.iter().find(|b| b.handle == l.block_record)).map(|b| richest(doc.entities_in_block(&b.name)));
    sheets.chain(std::iter::once_with(|| richest(doc.model_space_entities()))).find(|a| !a.is_empty()).unwrap_or_default()
}

/// Of the blocks among `entities`, the attributes of the one with the most filled in.
fn richest<'a>(entities: impl Iterator<Item = &'a EntityType>) -> Vec<(String, String)> {
    entities
        .filter_map(|e| match e {
            EntityType::Insert(i) => Some(i.attributes.iter().filter_map(|a| Some((a.tag.trim().to_string(), plain(&a.value)?))).collect::<Vec<_>>()),
            _ => None,
        })
        .max_by_key(Vec::len)
        .unwrap_or_default()
}

/// An attribute's value without formatting codes; `None` when empty.
fn plain(v: &str) -> Option<String> {
    let v = if v.contains('\\') || v.contains('{') { acadrust::entities::mtext_format::parse_mtext(v, true).to_plain_text() } else { v.to_string() };
    let v = v.trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// The short line under the thumbnail: the drawing's number and revision, from the title block
/// ("A-101 rev. C"), else from the properties (title, revision number); `None` without either.
pub fn summary(p: &Props) -> Option<String> {
    let key = |tag: &str| tag.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_uppercase();
    let attr = |wanted: fn(&str) -> bool| p.title_block.iter().find(|(t, _)| wanted(&key(t))).map(|(_, v)| v.clone());
    let number = attr(is_number).or_else(|| p.properties.iter().find(|(l, _)| *l == "cad-prop-title").map(|(_, v)| v.clone()));
    let rev = attr(|k| k.starts_with("REV")).or_else(|| p.properties.iter().find(|(l, _)| *l == "cad-prop-revision").map(|(_, v)| v.clone()));
    match (number, rev) {
        (Some(n), Some(r)) => Some(format!("{n} rev. {r}")),
        (Some(n), None) => Some(n),
        (None, Some(r)) => Some(format!("rev. {r}")),
        (None, None) => None,
    }
}

/// Tags that name the drawing's number, as title blocks write them in English and Italian:
/// DWG_NO, DRAWING_NUMBER, SHEET_NO, NUMERO, TAVOLA, N…
fn is_number(k: &str) -> bool {
    const TAGS: &[&str] = &["DWGNO", "DWGNUM", "DWGNUMBER", "DRAWINGNO", "DRAWINGNUM", "DRAWINGNUMBER", "DRGNO", "SHEETNO", "SHEETNUMBER", "NUMBER", "NO", "NR", "N", "NUM", "NUMERO", "NDISEGNO", "NUMERODISEGNO", "DISEGNO", "DISEGNON", "TAV", "TAVOLA", "TAVN", "TAVOLAN", "ELABORATO"];
    TAGS.contains(&k)
}

/// The document of a DWG or DXF, without drawing it.
fn document(bytes: &[u8], name: &str) -> Option<CadDocument> {
    let cursor = std::io::Cursor::new(bytes);
    if name.to_lowercase().ends_with(".dwg") {
        acadrust::DwgReader::from_stream(cursor).read().ok()
    } else {
        acadrust::DxfReader::from_reader(std::io::Cursor::new(bytes.to_vec())).and_then(|r| r.read()).ok()
    }
}

/// The Drawing box of the details panel (`meta::Group::Drawing`): the properties, then the
/// custom ones and the title block's attributes, which have no Fluent id: their names are written
/// as they are (`=` in front, see `meta_panel`). `None` for a drawing without any.
pub fn meta(bytes: &[u8], name: &str) -> Option<crate::meta::Meta> {
    let p = props(&document(bytes, name)?);
    let g = crate::meta::Group::Drawing;
    let mut fields: Vec<(crate::meta::Group, &'static str, String)> = p.properties.iter().map(|(l, v)| (g, *l, v.clone())).collect();
    fields.extend(p.custom.iter().map(|(n, v)| (g, crate::meta::intern(&format!("={n}")), v.clone())));
    if !p.title_block.is_empty() {
        fields.push((g, "cad-title-block", String::new()));
        fields.extend(p.title_block.iter().map(|(t, v)| (g, crate::meta::intern(&format!("={t}")), v.clone())));
    }
    (!fields.is_empty()).then_some(crate::meta::Meta { fields, gps: None })
}

/// The line under the thumbnail, from the file (`summary`).
pub fn summary_of(bytes: &[u8], name: &str) -> Option<String> {
    summary(&props(&document(bytes, name)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawing properties in the header, a custom one, and a title block with four attributes.
    const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$TITLE\n1\nGround floor plan\n9\n$SUBJECT\n1\nHouse Rossi\n9\n$AUTHOR\n1\nA. Bianchi\n9\n$KEYWORDS\n1\nplan, ground floor\n9\n$COMMENTS\n1\nFor the building permit\n9\n$REVISIONNUMBER\n1\n3\n9\n$CUSTOMPROPERTYTAG\n1\nClient\n9\n$CUSTOMPROPERTY\n1\nRossi family\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n8\n0\n2\nTITLE\n70\n2\n10\n0\n20\n0\n30\n0\n3\nTITLE\n0\nLINE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n180\n21\n0\n31\n0\n0\nLINE\n8\n0\n10\n180\n20\n0\n30\n0\n11\n180\n21\n40\n31\n0\n0\nLINE\n8\n0\n10\n180\n20\n40\n30\n0\n11\n0\n21\n40\n31\n0\n0\nLINE\n8\n0\n10\n0\n20\n40\n30\n0\n11\n0\n21\n0\n31\n0\n0\nLINE\n8\n0\n10\n0\n20\n20\n30\n0\n11\n180\n21\n20\n31\n0\n0\nLINE\n8\n0\n10\n90\n20\n0\n30\n0\n11\n90\n21\n40\n31\n0\n0\nENDBLK\n8\n0\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n420\n21\n0\n31\n0\n0\nLINE\n8\n0\n10\n420\n20\n0\n30\n0\n11\n420\n21\n297\n31\n0\n0\nLINE\n8\n0\n10\n420\n20\n297\n30\n0\n11\n0\n21\n297\n31\n0\n0\nLINE\n8\n0\n10\n0\n20\n297\n30\n0\n11\n0\n21\n0\n31\n0\n0\nLINE\n8\n0\n10\n100\n20\n100\n30\n0\n11\n300\n21\n100\n31\n0\n0\nLINE\n8\n0\n10\n300\n20\n100\n30\n0\n11\n300\n21\n250\n31\n0\n0\nLINE\n8\n0\n10\n300\n20\n250\n30\n0\n11\n100\n21\n250\n31\n0\n0\nLINE\n8\n0\n10\n100\n20\n250\n30\n0\n11\n100\n21\n100\n31\n0\n0\nINSERT\n8\n0\n66\n1\n2\nTITLE\n10\n235\n20\n5\n30\n0\n0\nATTRIB\n8\n0\n10\n240\n20\n30\n30\n0\n40\n5\n1\nA-101\n2\nDWG_NO\n70\n0\n0\nATTRIB\n8\n0\n10\n330\n20\n30\n30\n0\n40\n5\n1\nC\n2\nREV\n70\n0\n0\nATTRIB\n8\n0\n10\n240\n20\n10\n30\n0\n40\n5\n1\n2026-11-15\n2\nDATE\n70\n0\n0\nATTRIB\n8\n0\n10\n330\n20\n10\n30\n0\n40\n5\n1\n1:50\n2\nSCALE\n70\n0\n0\nSEQEND\n8\n0\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn properties_and_title_block_read() {
        let p = props(&document(DXF.as_bytes(), "t.dxf").unwrap());
        assert!(p.properties.contains(&("cad-prop-title", "Ground floor plan".to_string())));
        assert!(p.properties.contains(&("cad-prop-revision", "3".to_string())));
        assert_eq!(p.custom, vec![("Client".to_string(), "Rossi family".to_string())]);
        let tags: Vec<&str> = p.title_block.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(tags, ["DWG_NO", "REV", "DATE", "SCALE"]);
        assert_eq!(summary(&p).as_deref(), Some("A-101 rev. C"));
        let m = meta(DXF.as_bytes(), "t.dxf").unwrap();
        assert!(m.fields.iter().any(|(_, l, v)| *l == "=DWG_NO" && v == "A-101"));
        assert!(m.fields.iter().all(|(g, _, _)| *g == crate::meta::Group::Drawing));
    }

    #[test]
    fn the_line_from_what_there_is() {
        let p = |title: &str, rev: &str, block: &[(&str, &str)]| Props {
            properties: [("cad-prop-title", title), ("cad-prop-revision", rev)].into_iter().filter(|(_, v)| !v.is_empty()).map(|(l, v)| (l, v.to_string())).collect(),
            custom: Vec::new(),
            title_block: block.iter().map(|(t, v)| (t.to_string(), v.to_string())).collect(),
        };
        assert_eq!(summary(&p("Plan", "2", &[])).as_deref(), Some("Plan rev. 2"));
        assert_eq!(summary(&p("Plan", "", &[("Tavola", "T3"), ("Rev.", "B")])).as_deref(), Some("T3 rev. B"));
        assert_eq!(summary(&p("", "", &[("SCALE", "1:50")])), None);
        assert_eq!(summary(&p("", "", &[("REVISION", "4")])).as_deref(), Some("rev. 4"));
        // A drawing without any of them, as most: nothing.
        assert_eq!(summary_of(crate::cad_layers::tests::DXF.as_bytes(), "l.dxf"), None);
    }
}
