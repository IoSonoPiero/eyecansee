//! A WebP: its RIFF chunks (VP8X, ICCP, EXIF, XMP) but the coded picture and its alpha, of which
//! only the start (the frame header with the size). An animated one, whose frames are chunks of
//! their own, is read whole.

use std::ops::Range;

use super::{Plan, Sparse, Stop, plan_of};

/// The start of a coded picture kept: its frame header.
const PICTURE_START: usize = 4096;

/// Chunks walked at most.
const MOST: usize = 1024;

pub(super) fn plan(s: &Sparse) -> Plan {
    plan_of(Vec::new(), walk(s))
}

fn walk(s: &Sparse) -> Result<Vec<Range<usize>>, Stop> {
    let head = s.take(0, 12)?;
    if &head[..4] != b"RIFF" || &head[8..] != b"WEBP" {
        return Err(Stop::Whole);
    }
    // Bytes after the RIFF are left to the readers as they are.
    let end = 8usize.checked_add(s.u32(4, true)? as usize).ok_or(Stop::Whole)?;
    if end != s.len() {
        return Err(Stop::Whole);
    }
    let (mut at, mut skip) = (12, Vec::new());
    for _ in 0..MOST {
        if at >= end {
            return Ok(skip);
        }
        let kind: [u8; 4] = s.take(at, 4)?.try_into().map_err(|_| Stop::Whole)?;
        let len = s.u32(at + 4, true)? as usize;
        let data = at + 8;
        let next = data.checked_add(len + len % 2).ok_or(Stop::Whole)?;
        if next > end + len % 2 {
            return Err(Stop::Whole);
        }
        match &kind {
            b"ANIM" | b"ANMF" => return Err(Stop::Whole),
            b"VP8 " | b"VP8L" | b"ALPH" if len > PICTURE_START => skip.push(data + PICTURE_START..data + len),
            _ => {}
        }
        at = next;
    }
    Err(Stop::Whole)
}
