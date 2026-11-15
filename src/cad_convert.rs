//! An AutoCAD drawing saved in the other AutoCAD format, DWG ↔ DXF, with `acadrust`: the whole
//! document as read is written again, so its layers, blocks, layouts, styles and objects stay
//! (as many of the original's attributes as possible).

use std::io::Cursor;

use acadrust::{CadDocument, DwgReader, DwgWriter, DxfReader, DxfWriter};

/// The two formats a drawing can be saved in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Dwg,
    Dxf,
}

impl Target {
    pub fn ext(self) -> &'static str {
        match self {
            Target::Dwg => "dwg",
            Target::Dxf => "dxf",
        }
    }
}

/// DWG and DXF files: the others (HP-GL plots) have no document to save again.
pub fn can_convert(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".dwg") || lower.ends_with(".dxf")
}

fn document(bytes: &[u8], name: &str) -> Result<CadDocument, String> {
    if name.to_lowercase().ends_with(".dwg") {
        DwgReader::from_stream(Cursor::new(bytes)).read().map_err(|e| e.to_string())
    } else {
        DxfReader::from_reader(Cursor::new(bytes.to_vec())).and_then(|r| r.read()).map_err(|e| e.to_string())
    }
}

/// As `convert`, read back and compared with the original: refused when it shows less than the
/// original (`Err(None)`, with what would be lost), since `acadrust` drops some things on the way
/// (layouts of DXF files saved as DWG, some table cells; 84 of 102 real drawings came out the
/// same, 2026-11-02). More is accepted: a DWG's tables showed more of themselves as DXF.
pub fn convert_checked(bytes: &[u8], name: &str, to: Target) -> Result<Vec<u8>, (Option<String>, String)> {
    let before = crate::cad::read(bytes, name).map_err(|e| (Some(e), String::new()))?;
    let out = convert(bytes, name, to).map_err(|e| (Some(e), String::new()))?;
    let after = crate::cad::read(&out, &format!("x.{}", to.ext())).map_err(|e| (Some(e), String::new()))?;
    let (items, sheets) = ((before.items.len(), after.items.len()), (before.sheets.len(), after.sheets.len()));
    if items.1 < items.0 || sheets.1 < sheets.0 {
        return Err((None, format!("{} → {} elements, {} → {} layouts", items.0, items.1, sheets.0, sheets.1)));
    }
    Ok(out)
}

/// The drawing `bytes` (named `name`) as `to`, in the AutoCAD version it was saved with.
pub fn convert(bytes: &[u8], name: &str, to: Target) -> Result<Vec<u8>, String> {
    let doc = document(bytes, name)?;
    match to {
        Target::Dwg => DwgWriter::write_to_vec(&doc).map_err(|e| e.to_string()),
        Target::Dxf => {
            let mut out = Vec::new();
            DxfWriter::new(&doc).write_to_writer(&mut out).map_err(|e| e.to_string())?;
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DXF: &str = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n62\n1\n10\n0\n20\n0\n30\n0\n11\n100\n21\n50\n31\n0\n0\nCIRCLE\n8\n0\n10\n50\n20\n25\n30\n0\n40\n10\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn dxf_to_dwg_and_back() {
        let before = crate::cad::read(DXF.as_bytes(), "a.dxf").unwrap();
        let dwg = convert(DXF.as_bytes(), "a.dxf", Target::Dwg).unwrap();
        let back = crate::cad::read(&dwg, "a.dwg").unwrap();
        assert_eq!(back.items.len(), before.items.len());
        let dxf = convert(&dwg, "a.dwg", Target::Dxf).unwrap();
        assert_eq!(crate::cad::read(&dxf, "a.dxf").unwrap().items.len(), before.items.len());
    }

    /// Every DWG and DXF under `EYECANSEE_CAD` (subfolders too) saved as the other format and
    /// read back: what is drawn before and after, layouts, and the pictures in
    /// `EYECANSEE_CAD_OUT` to compare by eye.
    #[test]
    #[ignore]
    fn cad_convert_live() {
        let Ok(dir) = std::env::var("EYECANSEE_CAD") else { return };
        let out = std::env::var("EYECANSEE_CAD_OUT").ok();
        let mut stack = vec![std::path::PathBuf::from(dir)];
        let (mut same, mut total) = (0, 0);
        while let Some(d) = stack.pop() {
            for f in std::fs::read_dir(&d).unwrap().flatten() {
                let path = f.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = f.file_name().to_string_lossy().to_string();
                if !can_convert(&name) {
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap();
                let Ok(before) = crate::cad::read(&bytes, &name) else { continue };
                let to = if name.to_lowercase().ends_with(".dwg") { Target::Dxf } else { Target::Dwg };
                total += 1;
                let result = std::panic::catch_unwind(|| convert(&bytes, &name, to));
                let converted = match result {
                    Ok(Ok(c)) => c,
                    Ok(Err(e)) => {
                        println!("{name} -> {}: WRITE ERROR {e}", to.ext());
                        continue;
                    }
                    Err(_) => {
                        println!("{name} -> {}: WRITE PANIC", to.ext());
                        continue;
                    }
                };
                let new_name = format!("{name}.{}", to.ext());
                match std::panic::catch_unwind(|| crate::cad::read(&converted, &new_name)) {
                    Ok(Ok(after)) => {
                        let ok = after.items.len() == before.items.len() && after.sheets.len() == before.sheets.len();
                        same += usize::from(ok);
                        println!("{name} -> {}: items {} -> {}, layouts {} -> {}{}", to.ext(), before.items.len(), after.items.len(), before.sheets.len(), after.sheets.len(), if ok { "" } else { "  <-- DIFFERENT" });
                        if let Some(out) = &out {
                            for (d, tag) in [(&before, "a"), (&after, "b")] {
                                if let Ok(img) = crate::decode::cad_render(d, 1200, None) {
                                    let _ = img.save(format!("{out}/{name}.{tag}.png"));
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => println!("{name} -> {}: READ BACK ERROR {e}", to.ext()),
                    Err(_) => println!("{name} -> {}: READ BACK PANIC", to.ext()),
                }
            }
        }
        println!("same drawing: {same} of {total}");
    }
}
