//! A PSD or a PSB (Adobe's Photoshop File Format specification): header, colour mode data, the
//! image resources (EXIF, XMP, IPTC, ICC), the layer records and the blocks after them; not the
//! pixels of the layers (also those of a 16- or 32-bit file, in its Lr16 or Lr32 block) nor those
//! of the merged picture at the end.

use std::ops::Range;

use super::{Plan, Sparse, Stop, plan_of};

/// Blocks read at most after the layers: a broken file could have many.
const MOST: usize = 4096;

pub(super) fn plan(s: &Sparse) -> Plan {
    plan_of(Vec::new(), walk(s))
}

fn walk(s: &Sparse) -> Result<Vec<Range<usize>>, Stop> {
    let head = s.take(0, 26)?;
    if &head[..4] != b"8BPS" {
        return Err(Stop::Whole);
    }
    let big = match u16::from_be_bytes([head[4], head[5]]) {
        1 => false,
        2 => true,
        _ => return Err(Stop::Whole),
    };
    let modes = 30 + s.u32(26, false)? as usize;
    let layers = modes + 4 + s.u32(modes, false)? as usize;
    let (len, start) = length(s, layers, big)?;
    let pixels = start.checked_add(len).ok_or(Stop::Whole)?;
    // The merged picture: its compression, then its data.
    let mut skip = vec![(pixels + 2).min(s.len())..s.len()];
    if len > 0 {
        let (info, at) = length(s, start, big)?;
        skip.extend(layer_info(s, at, info, big)?);
        let mask = at + info;
        blocks(s, mask + 4 + s.u32(mask, false)? as usize, pixels, big, &mut skip)?;
    }
    Ok(skip)
}

/// A section's length (4 bytes, 8 in a PSB) at `at`, and where its content starts.
fn length(s: &Sparse, at: usize, big: bool) -> Result<(usize, usize), Stop> {
    let n = if big { usize::try_from(s.u64(at, false)?).map_err(|_| Stop::Whole)? } else { s.u32(at, false)? as usize };
    let start = at + if big { 8 } else { 4 };
    if start.checked_add(n).is_none_or(|e| e > s.len()) {
        return Err(Stop::Whole);
    }
    Ok((n, start))
}

/// The pixels of the layer info of `len` bytes at `at`: after the records, as long as their
/// channels say.
fn layer_info(s: &Sparse, at: usize, len: usize, big: bool) -> Result<Option<Range<usize>>, Stop> {
    if len < 2 {
        return Ok(None);
    }
    let count = (s.u16(at, false)? as i16).unsigned_abs() as usize;
    let (mut i, mut data) = (at + 2, 0usize);
    let channel = if big { 10 } else { 6 };
    for _ in 0..count {
        let channels = s.u16(i + 16, false)? as usize;
        for c in 0..channels {
            let n = i + 18 + c * channel + 2;
            data = data.saturating_add(if big { usize::try_from(s.u64(n, false)?).map_err(|_| Stop::Whole)? } else { s.u32(n, false)? as usize });
        }
        let extra = i + 18 + channels * channel + 12;
        i = extra + 4 + s.u32(extra, false)? as usize;
        if i > at + len {
            return Err(Stop::Whole);
        }
    }
    let end = i.saturating_add(data);
    if end > at + len {
        return Err(Stop::Whole);
    }
    Ok(Some(i..end))
}

/// The blocks after the global mask, up to `end`: the pixels of the layers in a Lr16, Lr32 or
/// Layr block added to `skip`.
fn blocks(s: &Sparse, mut at: usize, end: usize, big: bool, skip: &mut Vec<Range<usize>>) -> Result<(), Stop> {
    for _ in 0..MOST {
        if at + 12 > end {
            return Ok(());
        }
        let head = s.take(at, 8)?;
        if &head[..4] != b"8BIM" && &head[..4] != b"8B64" {
            return Ok(());
        }
        let key: [u8; 4] = head[4..8].try_into().map_err(|_| Stop::Whole)?;
        // As `psd::layers`: these blocks have 8-byte lengths in a PSB.
        let long = big && matches!(&key, b"LMsk" | b"Lr16" | b"Lr32" | b"Layr" | b"Mt16" | b"Mt32" | b"Mtrn" | b"Alph" | b"FMsk" | b"lnk2" | b"FEid" | b"FXid" | b"PxSD");
        let (len, data) = length(s, at + 8, long)?;
        if matches!(&key, b"Lr16" | b"Lr32" | b"Layr") {
            skip.extend(layer_info(s, data, len, big)?);
        }
        at = data + len;
    }
    Err(Stop::Whole)
}
