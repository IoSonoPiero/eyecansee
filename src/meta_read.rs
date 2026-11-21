//! What the metadata panel reads of a picture (#177): not the whole file, but the parts its
//! readers look at (`meta::read_file_with`: EXIF, XMP, IPTC, the header's size and colour), on a
//! phone or a server where every read is a round trip. Each format has a plan that walks its
//! structure over what was read so far and asks for what it still needs; when the structure is
//! known, everything is read but the coded pixels. The bytes go to the panel in a buffer the
//! size of the file, the parts not read left at zero, so every offset stays right (a PNG is put
//! back together without its pixel chunks instead, `png`). A file of an unknown shape, a small
//! one, or one the plan cannot read safely is read whole, as before.
//! Not looked at any more: an XMP packet that lives only inside the coded pixels or in a video
//! added after a JPEG photo (the panel's packet scanning found it there; Piero, 21/11/2026).

// Lists of byte ranges with one range in them are meant: not a range collected into a list.
#![allow(clippy::single_range_in_vec_init)]

mod bmff;
mod jpeg;
mod png;
mod psd;
mod raw;
mod tiff;
mod webp;

use std::ops::Range;

use crate::source::{Error, Store};

/// Files up to this size are read whole, in one read: on a server a read costs about 200 ms, the
/// time of 2.5-3 MB (SFTP on a Hetzner Storage Box, 13-14 MB/s, 21/11/2026), and a plan needs one
/// to three reads; on a phone a read costs about 3 ms (33 MB/s).
const WHOLE_UNDER: u64 = 2 << 20;

/// What one more read costs, in bytes, on a server: pieces closer than this are read as one, and
/// pieces that would cost more than the rest of the file are not read apart.
const READ_COST: u64 = 2 << 20;

/// The first read, the start of the file: in the common cases it holds all the metadata (a JPEG's
/// segments, a TIFF's directories, a HEIC's meta box).
const HEAD: u64 = 512 << 10;

/// A read asked for by a plan is at least this long: the next structure is often right after.
const MIN_READ: usize = 64 << 10;

/// The start of a picture's coded data read when it is not a JPEG: what a RAW decoder may read
/// there to learn the picture's shape.
const FIRST: usize = 4096;

/// Bytes between two parts of a picture taken as part of it: the padding that puts each part at
/// an even or aligned place (a DNG's tiles), which nothing reads.
const PADDING: usize = 16;

/// Rounds of reads at most: each structure found (a directory, an embedded JPEG) can point at others.
const ROUNDS: usize = 8;

/// The file as read so far: its bytes where read, `fill` elsewhere.
pub(crate) struct Sparse {
    pub bytes: Vec<u8>,
    /// What was read, in order, not overlapping.
    have: Vec<Range<usize>>,
}

/// Why a plan stopped walking.
#[derive(Debug, PartialEq)]
pub(crate) enum Stop {
    /// These bytes are needed to go on.
    Need(Range<usize>),
    /// The file is not as the plan expects (or is broken): read whole, as before.
    Whole,
}

/// What a plan says once it has walked the file.
#[derive(Debug, PartialEq)]
pub(crate) enum Plan {
    /// These bytes are still needed to know the structure.
    Need(Vec<Range<usize>>),
    /// The structure is known: everything but these ranges (coded pixels) is read.
    Skip(Vec<Range<usize>>),
    /// The bytes for the panel, made from what was read.
    Built(Vec<u8>),
    /// Read whole.
    Whole,
}

impl Sparse {
    pub fn new(size: usize, fill: u8) -> Sparse {
        Sparse { bytes: vec![fill; size], have: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// `data` read at `at`.
    pub fn fill(&mut self, at: usize, data: &[u8]) {
        let end = (at + data.len()).min(self.len());
        self.bytes[at..end].copy_from_slice(&data[..end - at]);
        self.have.push(at..end);
        self.have.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in self.have.drain(..) {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        self.have = merged;
    }

    /// Whether all of `r` was read.
    pub fn has(&self, r: &Range<usize>) -> bool {
        r.is_empty() || self.have.iter().any(|h| h.start <= r.start && r.end <= h.end)
    }

    /// The bytes of `r`: `Need` if not all read, `Whole` if past the end of the file.
    pub fn at(&self, r: Range<usize>) -> Result<&[u8], Stop> {
        if r.start > r.end || r.end > self.len() {
            return Err(Stop::Whole);
        }
        if !self.has(&r) {
            return Err(Stop::Need(r));
        }
        Ok(&self.bytes[r])
    }

    /// `at(at..at + len)` with the length checked.
    pub fn take(&self, at: usize, len: usize) -> Result<&[u8], Stop> {
        self.at(at..at.checked_add(len).ok_or(Stop::Whole)?)
    }

    pub fn u16(&self, at: usize, little: bool) -> Result<u16, Stop> {
        let b: [u8; 2] = self.take(at, 2)?.try_into().map_err(|_| Stop::Whole)?;
        Ok(if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    }

    pub fn u32(&self, at: usize, little: bool) -> Result<u32, Stop> {
        let b: [u8; 4] = self.take(at, 4)?.try_into().map_err(|_| Stop::Whole)?;
        Ok(if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    }

    pub fn u64(&self, at: usize, little: bool) -> Result<u64, Stop> {
        let b: [u8; 8] = self.take(at, 8)?.try_into().map_err(|_| Stop::Whole)?;
        Ok(if little { u64::from_le_bytes(b) } else { u64::from_be_bytes(b) })
    }

    /// Where the part read that ends with the file starts, if its end was read.
    pub fn read_to_end(&self) -> Option<usize> {
        self.have.last().filter(|h| h.end == self.len()).map(|h| h.start)
    }

    /// The parts of `wanted` not read yet.
    fn missing(&self, wanted: &[Range<usize>]) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        for w in wanted {
            let mut start = w.start;
            for h in self.have.iter().filter(|h| h.end > w.start && h.start < w.end) {
                if h.start > start {
                    out.push(start..h.start);
                }
                start = start.max(h.end);
            }
            if start < w.end {
                out.push(start..w.end);
            }
        }
        out
    }
}

/// `Plan` from a walk: what it still needs, or what to skip.
pub(crate) fn plan_of(mut needs: Vec<Range<usize>>, skip: Result<Vec<Range<usize>>, Stop>) -> Plan {
    match skip {
        Err(Stop::Whole) => Plan::Whole,
        Err(Stop::Need(r)) => {
            needs.push(r);
            Plan::Need(needs)
        }
        Ok(_) if !needs.is_empty() => Plan::Need(needs),
        Ok(skip) => Plan::Skip(skip),
    }
}

/// `Stop` of a walk that can go on elsewhere: a need is noted, the rest goes up.
pub(crate) fn noted<T>(r: Result<T, Stop>, needs: &mut Vec<Range<usize>>) -> Result<Option<T>, Stop> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(Stop::Need(n)) => {
            needs.push(n);
            Ok(None)
        }
        Err(Stop::Whole) => Err(Stop::Whole),
    }
}

/// The part of the coded picture `r` that is skipped: all but its start, a JPEG's segments or
/// `FIRST` bytes; `None` until that start is read (noted in `needs`).
pub(crate) fn coded(s: &Sparse, r: Range<usize>, needs: &mut Vec<Range<usize>>) -> Result<Option<Range<usize>>, Stop> {
    if r.start >= r.end {
        return Ok(None);
    }
    let start = match noted(s.take(r.start, 2), needs)? {
        Some([0xFF, 0xD8]) => noted(jpeg::head(s, r.start), needs)?.map(|(end, _)| end),
        Some(_) => Some(r.start + FIRST),
        None => None,
    };
    Ok(start.filter(|&e| e < r.end).map(|e| e..r.end))
}

/// A plan of a format, by its name; `None` for the formats read whole.
type Planner = fn(&Sparse) -> Plan;

fn planner(name: &str) -> Option<Planner> {
    let ext = name.rsplit_once('.')?.1.to_lowercase();
    Some(match ext.as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" | "mpo" => jpeg::plan,
        "png" => png::plan,
        "tif" | "tiff" | "dng" => tiff::plan,
        "cr2" | "nef" | "nrw" | "arw" | "sr2" | "srw" | "pef" | "orf" | "rw2" | "rwl" | "raw" | "erf" | "mef" | "mos" | "dcr" | "3fr" | "fff" => tiff::plan_raw,
        "raf" => raw::plan_raf,
        "mrw" => raw::plan_mrw,
        "cr3" => bmff::plan_cr3,
        "heic" | "heif" | "hif" | "avif" => bmff::plan_heif,
        "jxl" => bmff::plan_jxl,
        "webp" => webp::plan,
        "psd" | "psb" => psd::plan,
        _ => return None,
    })
}

/// How the pieces are read: the panel's way (`PANEL`), or another in the tests.
#[derive(Clone, Copy)]
pub(crate) struct Reading {
    /// The bytes not read.
    pub fill: u8,
    /// The first read.
    pub head: u64,
    /// What one more read costs, in bytes.
    pub cost: u64,
}

const PANEL: Reading = Reading { fill: 0, head: HEAD, cost: READ_COST };

/// The bytes the metadata panel reads of `path` (`size` bytes, from the listing): the parts its
/// readers need, or the whole file when it is small, of a format without a plan, or on a phone
/// that reads only from the start (`video_read::whole_only`).
pub async fn read(op: &Store, path: &str, size: u64) -> Result<Vec<u8>, Error> {
    if size <= WHOLE_UNDER || crate::video_read::whole_only(op) {
        return op.read(path).await;
    }
    match pieces(op, path, size, PANEL).await {
        Some(bytes) => Ok(bytes),
        None => op.read(path).await,
    }
}

/// The parts `path` needs, the rest `how.fill`; `None` when it is read whole instead.
pub(crate) async fn pieces(op: &Store, path: &str, size: u64, how: Reading) -> Option<Vec<u8>> {
    let plan = planner(path)?;
    let mut s = Sparse::new(usize::try_from(size).ok()?, how.fill);
    fetch(op, path, &mut s, vec![0..how.head.min(size) as usize], how.cost).await?;
    for _ in 0..ROUNDS {
        let wanted: Vec<Range<usize>> = match plan(&s) {
            Plan::Need(needs) => needs.into_iter().map(|r| r.start..r.end.max(r.start + MIN_READ).min(s.len())).collect(),
            Plan::Skip(skip) => {
                let rest = s.missing(&complement(&skip, s.len()));
                if !rest.is_empty() {
                    fetch(op, path, &mut s, rest, how.cost).await?;
                }
                return Some(s.bytes);
            }
            Plan::Built(bytes) => return Some(bytes),
            Plan::Whole => return None,
        };
        let missing = s.missing(&wanted);
        if missing.is_empty() {
            return None;
        }
        fetch(op, path, &mut s, missing, how.cost).await?;
    }
    None
}

/// Everything in `0..len` but `skip`.
fn complement(skip: &[Range<usize>], len: usize) -> Vec<Range<usize>> {
    let mut skip: Vec<_> = skip.iter().filter(|r| r.start < r.end).cloned().collect();
    skip.sort_by_key(|r| r.start);
    let (mut out, mut at) = (Vec::new(), 0);
    for r in skip {
        if r.start > at {
            out.push(at..r.start.min(len));
        }
        at = at.max(r.end);
    }
    if at < len {
        out.push(at..len);
    }
    out
}

/// `ranges` merged when closer than `gap`, in order.
fn merged(mut ranges: Vec<Range<usize>>, gap: usize) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end + gap => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Reads `ranges` into `s`, as few reads as they make merged when closer than a read `cost`s;
/// `None` when reading them would cost more than reading the file whole, or a read fails or
/// comes back short (the file changed).
async fn fetch(op: &Store, path: &str, s: &mut Sparse, ranges: Vec<Range<usize>>, cost: u64) -> Option<()> {
    let reads = merged(ranges, cost as usize);
    let bytes: usize = reads.iter().map(|r| r.len()).sum();
    if !s.have.is_empty() && reads.len() as u64 * cost + bytes as u64 >= cost + s.len() as u64 {
        return None;
    }
    for r in reads {
        let data = op.read_range(path, r.start as u64, r.len() as u64).await.ok()?;
        if data.len() != r.len() {
            return None;
        }
        s.fill(r.start, &data);
    }
    Some(())
}

#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod tests;
