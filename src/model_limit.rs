//! The most triangles EyeCanSee reads in one 3D model, by format (D1, Piero 22/11/2026): what a
//! model may have so that reading it fits in the memory of the computer (`Builder::up_to`).
//! Reading a format in text takes far more memory than a binary one: at the peak, with the same
//! model of 7.2 million triangles (Linux, release build), a binary PLY took 155 bytes a triangle,
//! STL 300, GLB 335, 3MF 657, OBJ 706, a PLY in text 873; the caps keep the peak under about
//! 8 GB, half of a computer of 16 GB. Where a library reads the whole file before `Builder` sees
//! a triangle (OBJ, PLY in text, 3MF, STL in text), the triangles are counted first on the bytes
//! already read (`obj_triangles` and the others), and a file beyond its cap is not read at all.

/// The cap of one format: its triangles, and the format as the message names it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Limit {
    pub(crate) max: usize,
    /// The selector of the message's `$format`.
    pub(crate) format: &'static str,
}

/// The formats without a cap of their own: glTF with its data apart, FBX, 3DS, OFF, AMF,
/// Collada, VRML, X3D, IFC and the CAD engine's.
pub(crate) const OTHER: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "other" };
pub(crate) const PLY_BINARY: Limit = Limit { max: 50_000_000, format: "ply-binary" };
pub(crate) const STL: Limit = Limit { max: 20_000_000, format: "stl" };
pub(crate) const GLB: Limit = Limit { max: 20_000_000, format: "glb" };
pub(crate) const OBJ: Limit = Limit { max: 10_000_000, format: "obj" };
pub(crate) const PLY_TEXT: Limit = Limit { max: 10_000_000, format: "ply-text" };
pub(crate) const THREE_MF: Limit = Limit { max: 10_000_000, format: "three-mf" };

/// Fluent id of the error, sent as `id\u{1}format max` (`said`).
pub const TOO_MANY: &str = "error-model-too-many-triangles";
/// Fluent id of the error of a point cloud (PLY), each point drawn as 4 triangles: sent as
/// `id\u{1}format points` (Piero, 22/11/2026: a message that speaks of points).
pub const TOO_MANY_POINTS: &str = "error-model-too-many-points";

/// What the early count of a PLY in text gives: the triangles of its faces, or the points of a
/// cloud (`ply_text_count`).
#[derive(Debug, PartialEq)]
pub(crate) enum Count {
    Triangles(usize),
    Points(usize),
}

impl Default for Limit {
    fn default() -> Self {
        OTHER
    }
}

impl Limit {
    /// `n` triangles, refused beyond the cap.
    pub(crate) fn check(self, n: usize) -> Result<(), String> {
        if n > self.max { Err(self.error()) } else { Ok(()) }
    }

    pub(crate) fn error(self) -> String {
        format!("{TOO_MANY}\u{1}{} {}", self.format, self.max)
    }

    /// The points of a cloud, 4 triangles each (`model_ply::point_cloud`): measured in text,
    /// 1177 bytes a point at the peak against 857 a face with its points (22/11/2026).
    pub(crate) fn points(self) -> usize {
        self.max / 4
    }

    /// A cloud of `n` points, refused beyond `points`, with the error that speaks of points.
    pub(crate) fn check_points(self, n: usize) -> Result<(), String> {
        if n > self.points() { Err(format!("{TOO_MANY_POINTS}\u{1}{} {}", self.format, self.points())) } else { Ok(()) }
    }

    /// `check` or `check_points`, as the count says.
    pub(crate) fn check_count(self, count: Count) -> Result<(), String> {
        match count {
            Count::Triangles(n) => self.check(n),
            Count::Points(n) => self.check_points(n),
        }
    }

    /// Points and triangles the parts of one model may bring (`Builder::add_shiny`), kept or
    /// dropped: a triangle with its three points, as STL gives them. A part placed again and
    /// again counts at every placement, so that it cannot take forever or fill the memory even
    /// when its points are not used or its triangles are dropped.
    pub(crate) fn offered(self) -> usize {
        4 * self.max
    }
}

/// The sentence of the error `id` with its `detail` (`Limit::error`), or its few words for the
/// lists (`brief`); `None` for another error.
pub(crate) fn said(i18n: &crate::i18n::I18n, id: &str, detail: &str, brief: bool) -> Option<String> {
    if !is_error(id) {
        return None;
    }
    let (format, max) = detail.split_once(' ')?;
    let max = max.parse::<usize>().ok()?;
    // Millions, with a tenth when there is one: 10, 2.5 (2,5 in Italian).
    let millions = if max.is_multiple_of(1_000_000) { (max / 1_000_000).to_string() } else { format!("{:.1}", max as f64 / 1e6).replace('.', &i18n.t("decimal-separator")) };
    let id = if brief { format!("{id}-brief") } else { id.to_string() };
    Some(i18n.t_args(&id, &[("format", format), ("max", &millions)]))
}

/// Whether `id` is one of the errors of a cap (`said`).
pub(crate) fn is_error(id: &str) -> bool {
    id == TOO_MANY || id == TOO_MANY_POINTS
}

/// The lines of a file in text, as `BufRead::lines` gives them (tobj, stl_io), split into words.
fn lines(bytes: &[u8]) -> impl Iterator<Item = impl Iterator<Item = &[u8]>> {
    bytes.split(|&b| b == b'\n').map(|l| l.split(u8::is_ascii_whitespace).filter(|w| !w.is_empty()))
}

/// The triangles tobj makes of an OBJ: a fan of each face (`f`, and `l` of three points or more,
/// as tobj reads it) of n corners, n - 2. A face it cannot read fails the whole file, so this is
/// never more than what it gives.
pub(crate) fn obj_triangles(bytes: &[u8]) -> usize {
    lines(bytes)
        .map(|mut words| match words.next() {
            Some(b"f" | b"l") => words.count().saturating_sub(2),
            _ => 0,
        })
        .sum()
}

/// The triangles of an STL in text: one a facet.
pub(crate) fn stl_text_triangles(bytes: &[u8]) -> usize {
    lines(bytes).filter_map(|mut words| words.next()).filter(|w| *w == b"facet").count()
}

/// The triangles of a 3MF's model in XML: one a `<triangle>` element, with or without the
/// prefix of its namespace (`<m:triangle`); the `<triangles>` around them are not counted.
pub(crate) fn xml_triangles(text: &str) -> usize {
    let b = text.as_bytes();
    let tag = |&(at, word): &(usize, &str)| b.get(at + word.len()).is_some_and(|c| c.is_ascii_whitespace() || matches!(c, b'/' | b'>')) && opened(&b[..at]);
    text.match_indices("triangle").filter(tag).count()
}

/// Whether a tag's name starts after `before`: right after `<`, or after `<prefix:`.
fn opened(before: &[u8]) -> bool {
    match before.split_last() {
        Some((b'<', _)) => true,
        Some((b':', rest)) => {
            let prefix = rest.iter().rev().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')).count();
            prefix > 0 && rest[..rest.len() - prefix].ends_with(b"<")
        }
        _ => false,
    }
}

/// An element of a PLY's header: its name, its rows, and its properties (name, list or not,
/// whole numbers or not).
struct Element<'a> {
    name: &'a [u8],
    count: usize,
    props: Vec<(&'a [u8], bool, bool)>,
}

/// The elements of a PLY's header and where its rows start; `None` without `end_header`.
fn ply_header(bytes: &[u8]) -> Option<(Vec<Element<'_>>, usize)> {
    let end = bytes.windows(10).position(|w| w == b"end_header")?;
    let body = bytes[end..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |n| end + n + 1);
    let whole = |t: &[u8]| !matches!(t, b"float" | b"float32" | b"double" | b"float64");
    let mut elements: Vec<Element> = Vec::new();
    for words in lines(&bytes[..end]) {
        let w: Vec<&[u8]> = words.collect();
        match w.as_slice() {
            [b"element", name, count] => elements.push(Element { name, count: std::str::from_utf8(count).ok()?.parse().ok()?, props: Vec::new() }),
            [b"property", b"list", _, t, name] => elements.last_mut()?.props.push((name, true, whole(t))),
            [b"property", t, name] => elements.last_mut()?.props.push((name, false, whole(t))),
            _ => {}
        }
    }
    Some((elements, body))
}

/// Where the property `name` is in a row: its word, when only single values come before it.
fn word_of(props: &[(&[u8], bool, bool)], name: &[u8]) -> Option<usize> {
    let k = props.iter().position(|p| p.0 == name)?;
    props[..k].iter().all(|p| !p.1).then_some(k)
}

/// The triangles the parser of a PLY in text gives (`model_ply::parsed`) and `Builder` keeps: a
/// fan of each face, without the triangles with a corner outside the points; without any, the
/// points with three finite coordinates (a cloud, `point_cloud`). Rows it cannot follow (a list
/// before the corners) stop the count there: the triangle strips are not counted either, so
/// this is never more than what it gives.
pub(crate) fn ply_text_count(bytes: &[u8]) -> Count {
    let Some((elements, body)) = ply_header(bytes) else { return Count::Triangles(0) };
    // Two elements of the same name: the parser keeps the last one, not counted.
    if elements.iter().enumerate().any(|(i, e)| elements[..i].iter().any(|o| o.name == e.name)) {
        return Count::Triangles(0);
    }
    let points = elements.iter().find(|e| e.name == b"vertex").map_or(0, |e| e.count);
    // As the parser takes the corners: `vertex_indices` when there is one, else `vertex_index`;
    // not a list of whole numbers, no triangle (`model_ply::list`).
    let corners_of = |e: &Element| {
        let name: &[u8] = if e.props.iter().any(|p| p.0 == b"vertex_indices") { b"vertex_indices" } else { b"vertex_index" };
        let listed = e.props.iter().find(|p| p.0 == name).is_some_and(|p| p.1 && p.2);
        listed.then(|| word_of(&e.props, name))
    };
    // A cloud only without faces that could have triangles, nor strips: the points' numbers are
    // read only then.
    let cloud = !elements.iter().any(|e| e.count > 0 && (e.name == b"tristrips" || e.name == b"face" && corners_of(e).is_some()));
    let mut rows = lines(&bytes[body..]).map(|w| w.collect::<Vec<_>>());
    let (mut faces, mut finite) = (0usize, 0usize);
    for e in &elements {
        let corners = corners_of(e);
        let xyz = [b"x" as &[u8], b"y", b"z"].map(|n| word_of(&e.props, n).filter(|&k| !e.props[k].1));
        for _ in 0..e.count {
            let Some(row) = rows.next() else { break };
            match (e.name, corners) {
                (b"face", Some(Some(k))) => match fan(&row, k, points) {
                    Some(n) => faces = faces.saturating_add(n),
                    None => return Count::Triangles(faces),
                },
                (b"face", Some(None)) => return Count::Triangles(faces),
                (b"vertex", _) if cloud => {
                    let number = |k: Option<usize>| k.and_then(|k| std::str::from_utf8(row.get(k)?).ok()?.parse::<f32>().ok()).is_some_and(f32::is_finite);
                    finite += usize::from(xyz.into_iter().all(number));
                }
                _ => {}
            }
        }
    }
    if cloud { Count::Points(finite) } else { Count::Triangles(faces) }
}

/// The triangles of the fan of a face whose corners' count is the word `k` of `row`, those with
/// every corner among the `points`; `None` when the row is not one the parser reads.
fn fan(row: &[&[u8]], k: usize, points: usize) -> Option<usize> {
    let n: usize = std::str::from_utf8(row.get(k)?).ok()?.parse().ok()?;
    let inside: Vec<bool> = row.get(k + 1..k + 1 + n)?.iter().map(|w| std::str::from_utf8(w).ok().and_then(|w| w.parse::<i64>().ok()).is_some_and(|v| (0..points as i64).contains(&v))).collect();
    Some((1..n.saturating_sub(1)).filter(|&i| inside[0] && inside[i] && inside[i + 1]).count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    /// D1 (Piero, 22/11/2026): the caps by format.
    #[test]
    fn caps_by_format_d1() {
        assert_eq!([PLY_BINARY, STL, GLB, OBJ, PLY_TEXT, THREE_MF, OTHER].map(|l| l.max), [50_000_000, 20_000_000, 20_000_000, 10_000_000, 10_000_000, 10_000_000, 50_000_000]);
    }

    /// `n` + 2 corners of one face, `first` to `first` + 2 over and over: `n` triangles in a fan,
    /// about 2 bytes a triangle (a file of 7 GB of reading for 10 million triangles otherwise).
    fn corners(n: usize, first: usize) -> String {
        let mut s = String::with_capacity((n + 2) * 2);
        for i in 0..n + 2 {
            s.push_str(&(first + i % 3).to_string());
            s.push(' ');
        }
        s
    }

    /// The error of `r` if it is one, else how many triangles were read (not the whole mesh).
    fn refused(r: Result<crate::model::Mesh, String>) -> Result<String, usize> {
        r.map(|m| m.tris.len()).map_or_else(Ok, Err)
    }

    /// D1's cap of the formats in text, written out: the files below are made with it, not with
    /// the constants, so that they stay of this size whatever the constants say.
    const TEXT_CAP: usize = 10_000_000;

    /// The corners of the files below: too far to be drawn, so that a reading that goes to the
    /// end drops every triangle at once (`Builder::finish`) instead of working out the normals of
    /// ten million triangles around one point; a count before the reading refuses the file all
    /// the same.
    const FAR: &str = "1e39 0 0\n1e39 1 0\n1e39 0 1\n";

    /// An OBJ of one triangle more than its cap is refused, with the error of the cap.
    #[test]
    fn obj_over_its_cap_d1() {
        let obj = format!("{}f {}\n", FAR.lines().map(|l| format!("v {l}\n")).collect::<String>(), corners(TEXT_CAP + 1, 1));
        assert_eq!(refused(crate::model::parse(obj.as_bytes(), "a.obj")), Ok(OBJ.error()));
    }

    /// A PLY in text of one triangle more than its cap, the same.
    #[test]
    fn ply_text_over_its_cap_d1() {
        let n = TEXT_CAP + 1;
        let head = "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uint uint vertex_indices\nend_header\n";
        let ply = format!("{head}{FAR}{} {}\n", n + 2, corners(n, 0));
        assert_eq!(refused(crate::model::parse(ply.as_bytes(), "a.ply")), Ok(PLY_TEXT.error()));
    }

    /// A point cloud in a PLY in text of one point more than 2.5 million (10 million triangles,
    /// 4 a point) is refused with an error of its own, which speaks of points (Piero, 22/11/2026).
    #[test]
    fn cloud_in_text_over_its_cap_d1() {
        let n = 2_500_001;
        let mut ply = format!("ply\nformat ascii 1.0\nelement vertex {n}\nproperty float x\nproperty float y\nproperty float z\nend_header\n");
        for i in 0..n {
            ply.push_str(&format!("{i} 0 0\n"));
        }
        let e = refused(crate::model::parse(ply.as_bytes(), "a.ply")).unwrap();
        assert_eq!(e, "error-model-too-many-points\u{1}ply-text 2500000");
        assert_eq!(I18n::new("en").load_error(&e), "The point cloud has more than 2.5 million points.\nEyeCanSee does not open text PLY files with more than 2.5 million points, so that reading them does not fill the computer's memory.");
        assert_eq!(I18n::new("it").load_error(&e), "La nuvola di punti ha più di 2,5 milioni di punti.\nEyeCanSee non apre i file PLY in testo con più di 2,5 milioni di punti, perché la lettura non riempia la memoria del computer.");
        assert_eq!(I18n::new("en").brief_load(&e).as_deref(), Some("more than 2.5 million points"));
        assert_eq!(I18n::new("it").brief_load(&e).as_deref(), Some("più di 2,5 milioni di punti"));
    }

    /// An STL in text of one facet more than 10 million, the cap of the formats in text (Piero,
    /// 22/11/2026), is refused by its early count, said as a text STL file; the facets are bare,
    /// about 6 bytes each, as the count reads only the first word of each line.
    #[test]
    fn stl_text_over_its_cap_d1() {
        let mut stl = String::with_capacity(6 * (TEXT_CAP + 2));
        stl.push_str("solid a\n");
        for _ in 0..=TEXT_CAP {
            stl.push_str("facet\n");
        }
        let e = refused(crate::model::parse(stl.as_bytes(), "a.stl")).unwrap();
        assert_eq!(e, "error-model-too-many-triangles\u{1}stl-text 10000000");
        assert_eq!(I18n::new("en").load_error(&e).lines().nth(1), Some("EyeCanSee does not open text STL files with more than 10 million triangles, so that reading them does not fill the computer's memory."));
        assert_eq!(I18n::new("it").load_error(&e).lines().nth(1), Some("EyeCanSee non apre i file STL in testo con più di 10 milioni di triangoli, perché la lettura non riempia la memoria del computer."));
    }

    /// The error said in the viewer, in the lists of Export and Convert (a few words, and the
    /// whole sentence), and in a message window, in English and in Italian.
    #[test]
    fn said_where_the_model_does_not_open_d1() {
        let e = OBJ.error();
        let en = I18n::new("en");
        let said = "The model has more than 10 million triangles.\nEyeCanSee does not open OBJ files with more than 10 million triangles, so that reading them does not fill the computer's memory.";
        assert_eq!(en.load_error(&e), said);
        assert_eq!(en.cannot_load("a.obj", &e), format!("Cannot open a.obj\n{said}"));
        assert_eq!(en.brief_load(&e).as_deref(), Some("more than 10 million triangles"));
        let (line, _) = crate::batch_summary::item(&en, crate::fault::NOT_OPENED, &format!("a.obj\u{1}{e}"), false);
        assert_eq!(line, crate::save_dialogs::md(&format!("Cannot open a.obj: {}", said.replace('\n', " "))));
        // Not taken for a picture too large for the memory (`error_bar`), and given back as it is
        // by the CAD engine's wrapper (`occt::from_file`), as the loader and the thumbnails call it.
        assert_eq!(crate::view_memory::need_of(&e), None);
        assert_eq!(crate::occt::from_file(None, || OBJ.check(OBJ.max + 1)), Err(e.clone()));
        let (id, detail) = e.split_once('\u{1}').unwrap();
        assert_eq!(en.error_md(id, detail), crate::save_dialogs::md(said));
        let it = I18n::new("it");
        assert_eq!(it.load_error(&PLY_TEXT.error()), "Il modello ha più di 10 milioni di triangoli.\nEyeCanSee non apre i file PLY in testo con più di 10 milioni di triangoli, perché la lettura non riempia la memoria del computer.");
        assert_eq!(it.load_error(&OTHER.error()), "Il modello ha più di 50 milioni di triangoli.\nEyeCanSee non apre i modelli 3D con più di 50 milioni di triangoli.");
        assert_eq!(it.brief_load(&STL.error()).as_deref(), Some("più di 20 milioni di triangoli"));
        // The formats read whole before the cap is met (AMF, Collada, X3D…): without the why, which
        // would be true only in part (Piero, 22/11/2026).
        assert_eq!(en.load_error(&OTHER.error()), "The model has more than 50 million triangles.\nEyeCanSee does not open 3D models with more than 50 million triangles.");
        assert_eq!(en.load_error(&PLY_TEXT.error()).lines().nth(1), Some("EyeCanSee does not open text PLY files with more than 10 million triangles, so that reading them does not fill the computer's memory."));
        assert_eq!(en.brief_load(&OTHER.error()).as_deref(), Some("more than 50 million triangles"));
        assert_eq!(en.load_error(&GLB.error()).lines().nth(1), Some("EyeCanSee does not open GLB files with more than 20 million triangles, so that reading them does not fill the computer's memory."));
    }

    /// The early counts give what the readers give, on small files of every kind they count.
    #[test]
    fn counted_as_read_d1() {
        let tris = |bytes: &[u8], name: &str| crate::model::parse(bytes, name).map(|m| m.tris.len());
        // Triangle, quad, a line of two points (left out), "l" of three (a triangle for tobj), a
        // pentagon with tabs and CRLF, a comment.
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 2 0\n# f 1 2 3\nf 1 2 3\nf 1 2 3 4\r\nl 1 2\nl 1 2 3\n  f\t1 2 3 4 5\r\n";
        assert_eq!(obj_triangles(obj), 7);
        assert_eq!(tris(obj, "a.obj"), Ok(7));
        let facet = "facet normal 0 0 1\n outer loop\n  vertex 0 0 0\n  vertex 1 0 0\n  vertex 0 1 0\n endloop\nendfacet\n";
        let stl = format!("solid a\n{facet}  {facet}endsolid a\n");
        assert_eq!(stl_text_triangles(stl.as_bytes()), 2);
        assert_eq!(tris(stl.as_bytes(), "a.stl"), Ok(2));
        // A quad and a triangle, after a property of the faces; corners as floats are not read.
        let head = "ply\nformat ascii 1.0\ncomment made by hand\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nelement face 2\nproperty uchar flags\nproperty list uchar int vertex_indices\nend_header\n";
        let ply = format!("{head}0 0 0\n1 0 0\n1 1 0\n0 1 0\n0 4 0 1 2 3\n0 3 0 1 2\n");
        assert_eq!(ply_text_count(ply.as_bytes()), Count::Triangles(3));
        assert_eq!(tris(ply.as_bytes(), "a.ply"), Ok(3));
        // A corner outside the points drops the triangles of the fan that use it, as Builder does.
        let outside = ply.replace("0 4 0 1 2 3\n", "0 4 0 1 2 9\n");
        assert_eq!(ply_text_count(outside.as_bytes()), Count::Triangles(2));
        assert_eq!(tris(outside.as_bytes(), "a.ply"), Ok(2));
        let floats = ply.replace("list uchar int", "list uchar float");
        assert_eq!(ply_text_count(floats.as_bytes()), Count::Points(4));
        // A cloud: 4 triangles a point, the point too far to be drawn left out.
        let cloud = "ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nend_header\n0 0 0\n1 0 0\n1e39 1 0\n";
        assert_eq!(ply_text_count(cloud.as_bytes()), Count::Points(2));
        assert_eq!(tris(cloud.as_bytes(), "a.ply"), Ok(8));
        let xml = r#"<model xmlns:m="x"><resources><object id="1"><mesh><vertices/><triangles><triangle v1="0" v2="1" v3="2"/><m:triangle v1="0" v2="1" v3="2"></m:triangle>
            <triangle
            v1="0" v2="1" v3="2"/></triangles></mesh><trianglesets/></object></resources><!-- a triangle --></model>"#;
        assert_eq!(xml_triangles(xml), 3);
    }

    /// Builder refuses one triangle beyond the cap it was given, with that cap's error.
    #[test]
    fn builder_up_to_its_cap_d1() {
        let small = Limit { max: 2, format: "obj" };
        let mut b = crate::model::Builder::up_to(small);
        assert_eq!(b.add(vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], [[0, 1, 2]; 2], |_| None), Ok(()));
        assert_eq!(b.add(vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], [[0, 1, 2]], |_| None), Err(small.error()));
        assert_eq!(OBJ.check(OBJ.max), Ok(()));
        assert_eq!(OBJ.check(OBJ.max + 1), Err(OBJ.error()));
        assert_eq!(PLY_BINARY.check_points(12_500_000), Ok(()));
        assert_eq!(PLY_BINARY.check_points(12_500_001), Err("error-model-too-many-points\u{1}ply-binary 12500000".into()));
        assert_eq!(I18n::new("it").load_error(&PLY_BINARY.check_points(12_500_001).unwrap_err()).lines().nth(1), Some("EyeCanSee non apre i file PLY binari con più di 12,5 milioni di punti, perché la lettura non riempia la memoria del computer."));
    }

    /// The time of the early count against the whole reading, three rounds each, of the models in
    /// text named by `EYECANSEE_COUNT_FILES` (paths apart by `:`). In a release build:
    /// `cargo test --release count_time_live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn count_time_live() {
        for path in std::env::var("EYECANSEE_COUNT_FILES").unwrap_or_default().split(':').filter(|p| !p.is_empty()) {
            let bytes = std::fs::read(path).unwrap();
            for _ in 0..3 {
                let t = std::time::Instant::now();
                let n = match path.rsplit('.').next() {
                    Some("obj") => obj_triangles(&bytes),
                    Some("stl") => stl_text_triangles(&bytes),
                    _ => match ply_text_count(&bytes) {
                        Count::Triangles(n) | Count::Points(n) => n,
                    },
                };
                let count = t.elapsed();
                let t = std::time::Instant::now();
                let read = crate::model::parse(&bytes, path).map(|m| m.tris.len());
                println!("{path}: {} MB, counted {n} in {count:.2?}, read {read:?} in {:.2?}", bytes.len() >> 20, t.elapsed());
            }
        }
    }
}
