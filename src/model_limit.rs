//! The most triangles EyeCanSee reads in one 3D model, by format: what a model may have so that
//! reading it fits in the memory of the computer (`Builder::up_to`).

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
pub(crate) const PLY_BINARY: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "ply-binary" };
pub(crate) const STL: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "stl" };
pub(crate) const GLB: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "glb" };
pub(crate) const OBJ: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "obj" };
pub(crate) const PLY_TEXT: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "ply-text" };
pub(crate) const THREE_MF: Limit = Limit { max: crate::model::MAX_TRIANGLES, format: "3mf" };

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
        format!("more than {} triangles", self.max)
    }

    /// Points and triangles the parts of one model may bring (`Builder::add_shiny`), kept or
    /// dropped: a triangle with its three points, as STL gives them. A part placed again and
    /// again counts at every placement, so that it cannot take forever or fill the memory even
    /// when its points are not used or its triangles are dropped.
    pub(crate) fn offered(self) -> usize {
        4 * self.max
    }
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

    /// The error said in the viewer, in the lists of Export and Convert (a few words, and the
    /// whole sentence), and in a message window, in English and in Italian.
    #[test]
    fn said_where_the_model_does_not_open_d1() {
        let e = OBJ.error();
        let en = I18n::new("en");
        let said = "The model has more than 10 million triangles.\nEyeCanSee does not read OBJ files with more than 10 million triangles, so that reading them does not fill the computer's memory.";
        assert_eq!(en.load_error(&e), said);
        assert_eq!(en.cannot_load("a.obj", &e), format!("Cannot open a.obj\n{said}"));
        assert_eq!(en.brief_load(&e).as_deref(), Some("more than 10 million triangles"));
        let (line, _) = crate::batch_summary::item(&en, crate::fault::NOT_OPENED, &format!("a.obj\u{1}{e}"), false);
        assert_eq!(line, format!("Cannot open a.obj: {}", said.replace('\n', " ")));
        let (id, detail) = e.split_once('\u{1}').unwrap();
        assert_eq!(en.error_md(id, detail), crate::save_dialogs::md(said));
        let it = I18n::new("it");
        assert_eq!(it.load_error(&PLY_TEXT.error()), "Il modello ha più di 10 milioni di triangoli.\nEyeCanSee non legge i file PLY in testo con più di 10 milioni di triangoli, perché la lettura non riempia la memoria del computer.");
        assert_eq!(it.load_error(&OTHER.error()), "Il modello ha più di 50 milioni di triangoli.\nEyeCanSee non legge i modelli 3D con più di 50 milioni di triangoli, perché la lettura non riempia la memoria del computer.");
        assert_eq!(it.brief_load(&STL.error()).as_deref(), Some("più di 20 milioni di triangoli"));
        assert_eq!(en.load_error(&GLB.error()).lines().nth(1), Some("EyeCanSee does not read GLB files with more than 20 million triangles, so that reading them does not fill the computer's memory."));
    }
}
