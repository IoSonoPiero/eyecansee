//! Files made of ISO boxes (ISO/IEC 14496-12). A HEIC or an AVIF: its `meta` box whole (the
//! items, their properties: size, colour, alpha), then every item but the coded pictures, read
//! through `iloc` (EXIF, XMP, the grid that makes a picture of tiles). A JPEG XL in its container:
//! every box but its codestream, of which only the start (the header with size and colour). A
//! Canon CR3: every box (its `moov` with the EXIF of CMT1-4 and the tracks, the XMP and preview
//! `uuid` boxes) but the samples of its picture tracks (full JPEG, small and full RAW), of which
//! only the start; the timed metadata track (CTMD) is read.

use std::ops::Range;

use super::{PADDING, Plan, Sparse, Stop, coded, merged, plan_of};

/// Item types of coded pictures and their tiles (HEVC, AV1, AVC, JPEG, JPEG 2000, VVC,
/// uncompressed), whose data is skipped.
const CODED: [&[u8; 4]; 9] = [b"hvc1", b"hvt1", b"lhv1", b"av01", b"avc1", b"jpeg", b"j2k1", b"vvc1", b"unci"];

/// The start of a JPEG XL codestream kept: its headers, the colour profile in them.
const CODESTREAM_START: usize = 64 << 10;

/// Boxes walked at most at the top of a file.
const MOST: usize = 4096;

/// A box: its type, where its content starts, where it ends.
pub(crate) struct Boxed {
    pub kind: [u8; 4],
    pub start: usize,
    pub content: usize,
    pub end: usize,
}

/// The box whose header is at `at`, inside `end`.
pub(crate) fn boxed(s: &Sparse, at: usize, end: usize) -> Result<Boxed, Stop> {
    let size = s.u32(at, false)? as usize;
    let kind: [u8; 4] = s.take(at + 4, 4)?.try_into().map_err(|_| Stop::Whole)?;
    let (content, len) = match size {
        0 => (at + 8, end - at),
        1 => (at + 16, usize::try_from(s.u64(at + 8, false)?).map_err(|_| Stop::Whole)?),
        n => (at + 8, n),
    };
    let box_end = at.checked_add(len).ok_or(Stop::Whole)?;
    if len < content - at || box_end > end {
        return Err(Stop::Whole);
    }
    Ok(Boxed { kind, start: at, content, end: box_end })
}

/// The boxes between `from` and `to`, their headers read.
pub(crate) fn children(s: &Sparse, from: usize, to: usize) -> Result<Vec<Boxed>, Stop> {
    let mut out = Vec::new();
    let mut at = from;
    while at + 8 <= to {
        if out.len() >= MOST {
            return Err(Stop::Whole);
        }
        let b = boxed(s, at, to)?;
        at = b.end;
        out.push(b);
    }
    Ok(out)
}

pub(super) fn plan_heif(s: &Sparse) -> Plan {
    plan_of(Vec::new(), heif(s))
}

fn heif(s: &Sparse) -> Result<Vec<Range<usize>>, Stop> {
    let top = children(s, 0, s.len())?;
    if top.first().is_none_or(|b| &b.kind != b"ftyp") {
        return Err(Stop::Whole);
    }
    let meta = top.iter().find(|b| &b.kind == b"meta").ok_or(Stop::Whole)?;
    s.at(meta.start..meta.end)?;
    // A full box: version and flags first.
    let inside = children(s, meta.content + 4, meta.end)?;
    let (Some(iinf), Some(iloc)) = (inside.iter().find(|b| &b.kind == b"iinf"), inside.iter().find(|b| &b.kind == b"iloc")) else { return Ok(Vec::new()) };
    let coded = coded_items(s, iinf)?;
    let skip: Vec<_> = locations(s, iloc)?.into_iter().filter(|(id, _)| coded.contains(id)).map(|(_, r)| r).collect();
    Ok(super::tiff::subtract(&merged(skip, PADDING), &[meta.start..meta.end]))
}

/// The items of `iinf` that are coded pictures.
fn coded_items(s: &Sparse, iinf: &Boxed) -> Result<Vec<u32>, Stop> {
    let version = s.take(iinf.content, 1)?[0];
    let first = iinf.content + if version == 0 { 6 } else { 8 };
    let mut out = Vec::new();
    for infe in children(s, first, iinf.end)?.iter().filter(|b| &b.kind == b"infe") {
        let v = s.take(infe.content, 1)?[0];
        let (id, kind_at) = match v {
            2 => (u32::from(s.u16(infe.content + 4, false)?), infe.content + 8),
            3 => (s.u32(infe.content + 4, false)?, infe.content + 10),
            _ => continue,
        };
        if CODED.iter().any(|c| s.take(kind_at, 4).is_ok_and(|k| k == *c)) {
            out.push(id);
        }
    }
    Ok(out)
}

/// The extents in the file of the items of `iloc` (ISO/IEC 14496-12, 8.11.3), by item; those
/// in `idat` or made from other items, and those whose length is "to the end", left out.
fn locations(s: &Sparse, iloc: &Boxed) -> Result<Vec<(u32, Range<usize>)>, Stop> {
    let v = s.take(iloc.content, 1)?[0];
    let sizes = s.take(iloc.content + 4, 2)?;
    let (offset_size, length_size, base_size) = ((sizes[0] >> 4) as usize, (sizes[0] & 15) as usize, (sizes[1] >> 4) as usize);
    let index_size = if v >= 1 { (sizes[1] & 15) as usize } else { 0 };
    let mut at = iloc.content + 6;
    let count = if v < 2 { s.u16(at, false)? as usize } else { s.u32(at, false)? as usize };
    at += if v < 2 { 2 } else { 4 };
    let number = |at: &mut usize, n: usize| -> Result<usize, Stop> {
        let b = s.take(*at, n)?;
        *at += n;
        Ok(b.iter().fold(0u64, |acc, &x| acc << 8 | u64::from(x)) as usize)
    };
    let mut out = Vec::new();
    for _ in 0..count {
        let id = number(&mut at, if v < 2 { 2 } else { 4 })? as u32;
        let method = if v >= 1 { number(&mut at, 2)? & 15 } else { 0 };
        number(&mut at, 2)?;
        let base = number(&mut at, base_size)?;
        let extents = number(&mut at, 2)?;
        for _ in 0..extents {
            number(&mut at, index_size)?;
            let (offset, length) = (number(&mut at, offset_size)?, number(&mut at, length_size)?);
            if method == 0 && length > 0 {
                let start = base.saturating_add(offset);
                out.push((id, start..start.saturating_add(length).min(s.len())));
            }
        }
        if at > iloc.end {
            return Err(Stop::Whole);
        }
    }
    Ok(out)
}

pub(super) fn plan_jxl(s: &Sparse) -> Plan {
    plan_of(Vec::new(), jxl(s))
}

fn jxl(s: &Sparse) -> Result<Vec<Range<usize>>, Stop> {
    // A bare codestream has nowhere to keep metadata, but the panel looks for a packet in it.
    if s.take(0, 12)? != b"\0\0\0\x0CJXL \r\n\x87\n" {
        return Err(Stop::Whole);
    }
    let mut skip = Vec::new();
    let mut first = true;
    for b in children(s, 0, s.len())? {
        // Partial codestreams start with their index.
        let data = match &b.kind {
            b"jxlc" => b.content,
            b"jxlp" => b.content + 4,
            _ => continue,
        };
        let from = if first { data + CODESTREAM_START } else { data };
        first = false;
        if from < b.end {
            skip.push(from..b.end);
        }
    }
    Ok(skip)
}

pub(super) fn plan_cr3(s: &Sparse) -> Plan {
    let mut needs = Vec::new();
    let skip = cr3(s, &mut needs);
    plan_of(needs, skip)
}

fn cr3(s: &Sparse, needs: &mut Vec<Range<usize>>) -> Result<Vec<Range<usize>>, Stop> {
    let top = children(s, 0, s.len())?;
    if top.first().is_none_or(|b| &b.kind != b"ftyp") {
        return Err(Stop::Whole);
    }
    let moov = top.iter().find(|b| &b.kind == b"moov").ok_or(Stop::Whole)?;
    s.at(moov.start..moov.end)?;
    let mut skip = Vec::new();
    for trak in children(s, moov.content, moov.end)?.iter().filter(|b| &b.kind == b"trak") {
        let Some(stbl) = inside(s, trak, &[b"mdia", b"minf", b"stbl"])? else { continue };
        let boxes = children(s, stbl.content, stbl.end)?;
        let find = |k: &[u8; 4]| boxes.iter().find(|b| &b.kind == k);
        // The sample entry's type, after the full box header and the count.
        if find(b"stsd").is_none_or(|d| s.take(d.content + 12, 4).is_ok_and(|k| k == b"CTMD")) {
            continue;
        }
        let (Some(stsz), Some(offsets)) = (find(b"stsz"), find(b"stco").or_else(|| find(b"co64"))) else { return Err(Stop::Whole) };
        for sample in samples(s, stsz, offsets)? {
            if let Some(r) = coded(s, sample, needs)? {
                skip.push(r);
            }
        }
    }
    Ok(super::tiff::subtract(&merged(skip, PADDING), &[moov.start..moov.end]))
}

/// The box at the end of `path` inside `b`, each the first of its type.
fn inside(s: &Sparse, b: &Boxed, path: &[&[u8; 4]]) -> Result<Option<Boxed>, Stop> {
    let mut found: Option<Boxed> = None;
    for kind in path {
        let (from, to) = found.as_ref().map_or((b.content, b.end), |f| (f.content, f.end));
        match children(s, from, to)?.into_iter().find(|c| &c.kind == *kind) {
            Some(c) => found = Some(c),
            None => return Ok(None),
        }
    }
    Ok(found)
}

/// The samples of a track, one per chunk as Canon writes them: where each starts (`stco` or
/// `co64`) and its size (`stsz`).
fn samples(s: &Sparse, stsz: &Boxed, offsets: &Boxed) -> Result<Vec<Range<usize>>, Stop> {
    let (fixed, count) = (s.u32(stsz.content + 4, false)? as usize, s.u32(stsz.content + 8, false)? as usize);
    let wide = &offsets.kind == b"co64";
    if s.u32(offsets.content + 4, false)? as usize != count || count > 64 {
        return Err(Stop::Whole);
    }
    let mut out = Vec::new();
    for i in 0..count {
        let size = if fixed > 0 { fixed } else { s.u32(stsz.content + 12 + 4 * i, false)? as usize };
        let at = if wide { usize::try_from(s.u64(offsets.content + 8 + 8 * i, false)?).map_err(|_| Stop::Whole)? } else { s.u32(offsets.content + 8 + 4 * i, false)? as usize };
        out.push(at..at.saturating_add(size).min(s.len()));
    }
    Ok(out)
}
