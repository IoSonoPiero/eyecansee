//! IFC building models (BIM: IFC 2x3, 4, 4.3) through `ifc-lite` (pure Rust, MPL-2.0): every
//! element with geometry (walls, slabs, windows, beams, pipes, roads…) as triangles in the colour
//! of its style or, without one, of its kind. Openings are cut out of walls by the library.

use crate::model::{Builder, Mesh};

/// Elements drawn see-through in the IFC (glass, spaces) above this opacity are kept, the
/// others left out: they would hide what is inside them.
const MIN_ALPHA: f32 = 0.2;

pub fn read(bytes: &[u8]) -> Result<Mesh, String> {
    if !bytes.starts_with(b"ISO-10303-21") {
        return Err("not an IFC file".into());
    }
    let result = ifc_lite_processing::process_geometry(bytes);
    let mut b = Builder::default();
    for m in result.meshes {
        if m.color[3] < MIN_ALPHA || m.ifc_type.eq_ignore_ascii_case("IfcSpace") {
            continue;
        }
        let pos = m.positions.as_chunks::<3>().0.to_vec();
        let tris = m.indices.as_chunks::<3>().0.iter().copied();
        let rgb = [m.color[0], m.color[1], m.color[2]].map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8);
        b.add(pos, tris, |_| Some(rgb))?;
    }
    // Buildings keep Z up.
    b.finish(true).map_err(|_| "no geometry in the IFC file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_ifc_refused() {
        assert!(read(b"<?xml").is_err());
        assert!(read(b"ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n").is_err());
    }
}
