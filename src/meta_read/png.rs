//! A PNG: its chunks but the pixels (IDAT). They often come in thousands of small chunks, whose
//! headers the readers walk one after the other: so the file is put back together without them,
//! one empty IDAT in their place. The chunks after the pixels (an eXIf, an XMP in iTXt, written
//! there by some programs) are found from the end of the file: the last pixel chunk is the one
//! followed by chunks whose checksums are right up to IEND, the end of the file.
//! An animated PNG (APNG), whose frames are chunks of their own, is read whole.

use super::{Plan, Sparse, Stop};

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The end of the file read at first, after the pixels: doubled until the last IDAT is in it.
const TAIL: usize = 256 << 10;

/// An IDAT without data, with its checksum.
const EMPTY_IDAT: [u8; 12] = [0, 0, 0, 0, b'I', b'D', b'A', b'T', 0x35, 0xAF, 0x06, 0x1E];

pub(super) fn plan(s: &Sparse) -> Plan {
    match rebuilt(s) {
        Ok(bytes) => Plan::Built(bytes),
        Err(Stop::Need(r)) => Plan::Need(vec![r]),
        Err(Stop::Whole) => Plan::Whole,
    }
}

/// A chunk at `at`: its type and where it ends; `Whole` past the end of the file.
fn chunk(s: &Sparse, at: usize) -> Result<([u8; 4], usize), Stop> {
    let len = s.u32(at, false)? as usize;
    let kind: [u8; 4] = s.take(at + 4, 4)?.try_into().map_err(|_| Stop::Whole)?;
    let end = at.checked_add(12).and_then(|e| e.checked_add(len)).filter(|&e| e <= s.len()).ok_or(Stop::Whole)?;
    Ok((kind, end))
}

/// The PNG without its pixels.
fn rebuilt(s: &Sparse) -> Result<Vec<u8>, Stop> {
    if s.take(0, 8)? != SIGNATURE {
        return Err(Stop::Whole);
    }
    let mut out = SIGNATURE.to_vec();
    let mut at = 8;
    let mut pixels = false;
    loop {
        let (kind, end) = match chunk(s, at) {
            Err(Stop::Need(_)) if pixels => return after_pixels(s, at, out),
            r => r?,
        };
        if &kind == b"IDAT" {
            if !pixels {
                out.extend_from_slice(&EMPTY_IDAT);
                pixels = true;
            }
        } else {
            if matches!(&kind, b"acTL" | b"fcTL" | b"fdAT") {
                return Err(Stop::Whole);
            }
            out.extend_from_slice(s.at(at..end)?);
            if &kind == b"IEND" {
                // Bytes after IEND: the readers go on into them.
                return if end == s.len() { Ok(out) } else { Err(Stop::Whole) };
            }
        }
        at = end;
    }
}

/// `out` with the chunks after the pixels, `at` being inside them and not read: the end of the
/// file is read, and in it the last IDAT followed by chunks whose checksums are right.
fn after_pixels(s: &Sparse, at: usize, mut out: Vec<u8>) -> Result<Vec<u8>, Stop> {
    let len = s.len();
    let Some(tail) = s.read_to_end().filter(|&t| t < len) else { return Err(Stop::Need(at.max(len.saturating_sub(TAIL))..len)) };
    let found = (tail..len.saturating_sub(12)).rev().filter(|&p| s.bytes[p + 4..p + 8] == *b"IDAT").find_map(|p| {
        let (_, end) = chunk(s, p).ok()?;
        (end >= tail).then(|| chained(s, end)).flatten()
    });
    match found {
        Some(chunks) => {
            out.extend(chunks);
            Ok(out)
        }
        None if tail > at => Err(Stop::Need(at.max(tail.saturating_sub((len - tail).max(TAIL)))..tail)),
        None => Err(Stop::Whole),
    }
}

/// The chunks from `at` to the end of the file, but the pixels, if each one's checksum is right
/// and the last is IEND.
fn chained(s: &Sparse, mut at: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let (kind, end) = chunk(s, at).ok()?;
        let bytes = s.at(at..end).ok()?;
        if crc(&bytes[4..bytes.len() - 4]) != u32::from_be_bytes(bytes[bytes.len() - 4..].try_into().ok()?) || matches!(&kind, b"fcTL" | b"fdAT") {
            return None;
        }
        if &kind != b"IDAT" {
            out.extend_from_slice(bytes);
        }
        if &kind == b"IEND" {
            return (end == s.len()).then_some(out);
        }
        at = end;
    }
}

/// CRC-32 of a chunk (PNG specification, annex D).
pub(super) fn crc(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            t[n] = c;
            n += 1;
        }
        t
    };
    !bytes.iter().fold(!0u32, |c, &b| TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_empty_idat_has_its_checksum() {
        assert_eq!(super::crc(b"IDAT"), 0x35AF_061E);
        assert_eq!(super::EMPTY_IDAT[8..], 0x35AF_061Eu32.to_be_bytes());
    }
}
