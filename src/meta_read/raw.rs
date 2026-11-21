//! RAW files not built on TIFF. A Fujifilm RAF: its header gives where its JPEG preview (with the
//! EXIF and the XMP), its own metadata and its sensor data are; the sensor data starts with a
//! TIFF of Fujifilm's that says where the pixels are. A Minolta MRW: its metadata blocks come
//! first, the pixels after them.

use std::ops::Range;

use super::{FIRST, Plan, Sparse, Stop, coded, plan_of, tiff};

pub(super) fn plan_raf(s: &Sparse) -> Plan {
    let mut needs = Vec::new();
    let skip = raf(s, &mut needs);
    plan_of(needs, skip)
}

fn raf(s: &Sparse, needs: &mut Vec<Range<usize>>) -> Result<Vec<Range<usize>>, Stop> {
    if s.take(0, 16)? != b"FUJIFILMCCD-RAW " {
        return Err(Stop::Whole);
    }
    let at = |i: usize| s.u32(i, false).map(|v| v as usize);
    let (jpeg, jpeg_len, cfa) = (at(84)?, at(88)?, at(100)?);
    let mut skip = Vec::new();
    skip.extend(coded(s, jpeg..jpeg.saturating_add(jpeg_len).min(s.len()), needs)?);
    if cfa == 0 || cfa >= s.len() {
        return Err(Stop::Whole);
    }
    // Older cameras have the pixels themselves there: no such file was tried, read whole.
    skip.extend(tiff::walk(s, cfa, &tiff::FUJI, needs)?);
    Ok(skip)
}

pub(super) fn plan_mrw(s: &Sparse) -> Plan {
    plan_of(Vec::new(), mrw(s))
}

fn mrw(s: &Sparse) -> Result<Vec<Range<usize>>, Stop> {
    if s.take(0, 4)? != b"\0MRM" {
        return Err(Stop::Whole);
    }
    // The blocks (PRD, TTW with the EXIF, WBG, RIF) are in the first one's length.
    let pixels = 8usize.checked_add(s.u32(4, false)? as usize).ok_or(Stop::Whole)?;
    Ok(vec![(pixels + FIRST).min(s.len())..s.len()])
}
