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
