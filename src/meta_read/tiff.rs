//! A TIFF, a DNG and the RAW files built on TIFF: every directory and the values it points at
//! (EXIF, GPS, XMP, IPTC, Photoshop resources, the maker's note with what it points at, a DNG's
//! own tags) are read; not the strips and tiles of the pictures. Of each picture its start is
//! read: a JPEG's segments (which can hold an XMP packet the panel looks for), or the first bytes
//! of a RAW file's sensor data, where a decoder may read the picture's shape.

use std::collections::HashSet;
use std::ops::Range;

use super::{FIRST, PADDING, Plan, Sparse, Stop, merged, noted, plan_of};

/// The TIFF of a file and what its directories hold.
pub(crate) struct Dialect {
    /// The first four bytes, little-endian or not.
    magic: &'static [(&'static [u8; 4], bool)],
    /// Tags of the directories followed.
    pointers: &'static [u16],
    /// Tags of the pictures: offsets and lengths.
    pictures: &'static [(u16, u16)],
    /// Tags of a picture that goes on to the end of the file, without a length.
    to_the_end: &'static [u16],
    /// Tags whose value is a JPEG (the preview of a Panasonic RW2).
    jpegs: &'static [u16],
    /// The start of the sensor data is read even when not compressed.
    raw: bool,
}

/// SubIFDs, EXIF, GPS, Interoperability.
const POINTERS: &[u16] = &[0x014A, 0x8769, 0x8825, 0xA005];

/// Strips, tiles, a JPEG (thumbnail or preview).
const PICTURES: &[(u16, u16)] = &[(0x0111, 0x0117), (0x0144, 0x0145), (0x0201, 0x0202)];

pub(crate) const TIFF: Dialect = Dialect { magic: &[(b"II*\0", true), (b"MM\0*", false)], pointers: POINTERS, pictures: PICTURES, to_the_end: &[], jpegs: &[], raw: false };

/// Olympus ORF and Panasonic RW2 have another number in the header; RW2 says where its sensor
/// data starts (0x0118) and keeps its preview, with the EXIF, in a tag of its own (0x002E).
pub(crate) const RAW: Dialect = Dialect {
    magic: &[(b"II*\0", true), (b"MM\0*", false), (b"IIRO", true), (b"MMOR", false), (b"IISR", true), (b"IIU\0", true)],
    pointers: POINTERS,
    pictures: PICTURES,
    to_the_end: &[0x0118],
    jpegs: &[0x002E],
    raw: true,
};

/// The TIFF in a Fujifilm RAF: its own directory (0xF000) with the sensor data (0xF007, 0xF008).
pub(crate) const FUJI: Dialect = Dialect {
    magic: &[(b"II*\0", true), (b"MM\0*", false)],
    pointers: &[0x014A, 0x8769, 0x8825, 0xA005, 0xF000],
    pictures: &[(0x0111, 0x0117), (0x0144, 0x0145), (0x0201, 0x0202), (0xF007, 0xF008)],
    to_the_end: &[],
    jpegs: &[],
    raw: true,
};

/// Directories read at most: a broken file could point at many. Also the most directories one
/// entry may point at, and the most waiting to be read (real files: a few, audit E.1).
const MOST: usize = 256;

/// Strips or tiles noted at most, of all the pictures of a file: a large tiled TIFF has hundreds
/// of thousands, a file made to point many directories at the same table many more (audit E.4).
const STRIPS: usize = 1 << 20;

/// TIFF compressions coded as JPEG: old and new JPEG, a lossy DNG's.
const JPEG_CODED: [usize; 3] = [6, 7, 34892];

pub(super) fn plan(s: &Sparse) -> Plan {
    plan_as(s, &TIFF)
}

pub(super) fn plan_raw(s: &Sparse) -> Plan {
    plan_as(s, &RAW)
}

fn plan_as(s: &Sparse, d: &Dialect) -> Plan {
    let mut needs = Vec::new();
    let skip = walk(s, 0, d, &mut needs);
    plan_of(needs, skip)
}

/// One directory entry: tag, type, count, and where its value is (in the entry or out of it).
struct Entry {
    tag: u16,
    kind: u16,
    count: usize,
    value: usize,
}

/// What a walk found: the bytes of the structure, the pictures' data, the starts kept.
#[derive(Default)]
struct Found {
    structure: Vec<Range<usize>>,
    pixels: Vec<Range<usize>>,
    kept: Vec<Range<usize>>,
}

/// The ranges to skip of the TIFF at `base` (its offsets count from there).
pub(crate) fn walk(s: &Sparse, base: usize, d: &Dialect, needs: &mut Vec<Range<usize>>) -> Result<Vec<Range<usize>>, Stop> {
    let head = s.take(base, 4)?;
    let little = d.magic.iter().find(|(m, _)| head == *m).ok_or(Stop::Whole)?.1;
    let mut found = Found { structure: vec![base..base + 8], ..Default::default() };
    let mut todo = vec![(base + s.u32(base + 4, little)? as usize, true)];
    let mut seen = HashSet::new();
    while let Some((at, main)) = todo.pop() {
        if at == base || !seen.insert(at) {
            continue;
        }
        if seen.len() > MOST {
            return Err(Stop::Whole);
        }
        if let Some((next, inner)) = noted(directory(s, (base, at, main), little, d, &mut found, needs), needs)? {
            todo.extend(next.map(|n| (n, true)));
            todo.extend(inner.into_iter().map(|n| (n, false)));
            if todo.len() > MOST {
                return Err(Stop::Whole);
            }
        }
    }
    Ok(subtract(&subtract(&merged(found.pixels, PADDING), &found.structure), &found.kept))
}

/// Entries of a directory at most: more is not one (a Kodak KDC's Interoperability pointer
/// leads to other data), and it is left as it is, read.
const ENTRIES: usize = 1000;

/// The entries of the directory at `at`, its size and the values it holds noted in `found` (but
/// a JPEG of `d.jpegs`, a picture), and the next directory.
fn entries(s: &Sparse, (base, at): (usize, usize), little: bool, d: &Dialect, found: &mut Found) -> Result<(Vec<Entry>, usize), Stop> {
    let n = s.u16(at, little)? as usize;
    if n > ENTRIES {
        return Ok((Vec::new(), 0));
    }
    let end = at + 2 + 12 * n;
    s.at(at..end + 4)?;
    found.structure.push(at..end + 4);
    let mut out = Vec::new();
    for k in 0..n {
        let e = at + 2 + 12 * k;
        let (tag, kind, count) = (s.u16(e, little)?, s.u16(e + 2, little)?, s.u32(e + 4, little)? as usize);
        let Some(size) = unit(kind).and_then(|u| u.checked_mul(count)) else { continue };
        let value = if size <= 4 { e + 8 } else { base + s.u32(e + 8, little)? as usize };
        if size > 4 && !d.jpegs.contains(&tag) {
            found.structure.push(value..value.saturating_add(size).min(s.len()));
        }
        out.push(Entry { tag, kind, count, value });
    }
    Ok((out, s.u32(end, little)? as usize))
}

/// The next main directory, and the directories one points at.
type Next = (Option<usize>, Vec<usize>);

/// The directory at `at` (`main`: in the chain of the main directories): its entries noted in
/// `found`; the next main directory, and the directories it points at. The "next" link of
/// those is not followed: the EXIF reader does not either, and a Mamiya MEF has it pointing into
/// its pixels.
fn directory(s: &Sparse, (base, at, main): (usize, usize, bool), little: bool, d: &Dialect, found: &mut Found, needs: &mut Vec<Range<usize>>) -> Result<Next, Stop> {
    let (entries, next_ifd) = entries(s, (base, at), little, d, found)?;
    let next = (main && next_ifd != 0).then_some(base + next_ifd);
    let mut inner = Vec::new();
    let get = |tag: u16| entries.iter().find(|e| e.tag == tag);
    for e in entries.iter().filter(|e| d.pointers.contains(&e.tag)) {
        if e.count > MOST {
            return Err(Stop::Whole);
        }
        if let Some(v) = noted(values(s, e, little), needs)? {
            inner.extend(v.into_iter().map(|o| base + o));
        }
    }
    let compression = get(0x0103).and_then(|e| values(s, e, little).ok()?.first().copied()).unwrap_or(1);
    let start = if JPEG_CODED.contains(&compression) {
        Start::Jpeg
    } else if d.raw {
        Start::Some
    } else {
        Start::Nothing
    };
    for &(offsets, counts) in d.pictures {
        let (Some(o), Some(c)) = (get(offsets), get(counts)) else { continue };
        if o.count.max(c.count) > STRIPS {
            return Err(Stop::Whole);
        }
        let (Some(o), Some(c)) = (noted(values(s, o, little), needs)?, noted(values(s, c, little), needs)?) else { continue };
        // A thumbnail or preview JPEG, whatever the compression of the directory says.
        let start = if offsets == 0x0201 { Start::Jpeg } else { start };
        let parts = o.iter().zip(&c).map(|(&o, &c)| base + o..(base + o).saturating_add(c).min(s.len())).collect();
        pixels(s, parts, start, found, needs)?;
    }
    for &tag in d.to_the_end {
        if let Some(o) = get(tag).and_then(|e| values(s, e, little).ok()?.first().copied()) {
            pixels(s, vec![base + o..s.len()], Start::Some, found, needs)?;
        }
    }
    for e in entries.iter().filter(|e| d.jpegs.contains(&e.tag) && e.count > 4) {
        pixels(s, vec![e.value..e.value.saturating_add(e.count).min(s.len())], Start::Jpeg, found, needs)?;
    }
    Ok((next, inner))
}

/// What is read of the start of a picture.
#[derive(Clone, Copy)]
enum Start {
    /// Its segments, if it is a JPEG (`FIRST` bytes if not).
    Jpeg,
    /// `FIRST` bytes.
    Some,
    Nothing,
}

/// A picture's strips or tiles, skipped but for the start of the first as `start` says.
fn pixels(s: &Sparse, mut parts: Vec<Range<usize>>, start: Start, found: &mut Found, needs: &mut Vec<Range<usize>>) -> Result<(), Stop> {
    parts.retain(|r| r.start < r.end);
    let Some(first) = parts.first().cloned() else { return Ok(()) };
    let some = first.start..(first.start + FIRST).min(first.end);
    let kept = match start {
        Start::Nothing => Some(first.start..first.start),
        Start::Some => Some(some),
        Start::Jpeg => match noted(s.take(first.start, 2), needs)? {
            Some([0xFF, 0xD8]) => noted(super::jpeg::head(s, first.start), needs)?.map(|(end, _)| first.start..end),
            Some(_) => Some(some),
            None => None,
        },
    };
    // Until its start is read, none of it is skipped.
    if let Some(kept) = kept {
        if found.pixels.len() + parts.len() > STRIPS {
            return Err(Stop::Whole);
        }
        found.kept.push(kept);
        found.pixels.append(&mut parts);
    }
    Ok(())
}

/// The numbers of an entry of offsets or counts (SHORT, LONG or IFD); others none.
fn values(s: &Sparse, e: &Entry, little: bool) -> Result<Vec<usize>, Stop> {
    let size = match e.kind {
        3 => 2,
        4 | 13 => 4,
        _ => return Ok(Vec::new()),
    };
    let bytes = s.take(e.value, size * e.count)?;
    Ok(bytes
        .chunks_exact(size)
        .map(|b| match (size, little) {
            (2, true) => u16::from_le_bytes([b[0], b[1]]) as usize,
            (2, false) => u16::from_be_bytes([b[0], b[1]]) as usize,
            (_, true) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize,
            (_, false) => u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize,
        })
        .collect())
}

/// Bytes of one value of a TIFF type; `None` for a type unknown.
fn unit(kind: u16) -> Option<usize> {
    match kind {
        1 | 2 | 6 | 7 => Some(1),
        3 | 8 => Some(2),
        4 | 9 | 11 | 13 => Some(4),
        5 | 10 | 12 => Some(8),
        _ => None,
    }
}

/// `from` without what is in `out`. `out` is joined first, so each range of `from` looks only at
/// the parts it meets (one against each took hours, audit E.4).
pub(crate) fn subtract(from: &[Range<usize>], out: &[Range<usize>]) -> Vec<Range<usize>> {
    let cut = merged(out.iter().filter(|r| r.start < r.end).cloned().collect(), 0);
    let mut result = Vec::new();
    for r in from {
        let mut start = r.start;
        let first = cut.partition_point(|c| c.end <= r.start);
        for c in cut[first..].iter().take_while(|c| c.start < r.end) {
            if c.start > start {
                result.push(start..c.start);
            }
            start = start.max(c.end);
        }
        if start < r.end {
            result.push(start..r.end);
        }
    }
    result
}
