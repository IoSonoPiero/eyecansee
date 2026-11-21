//! A JPEG: its segments up to the coded data (EXIF, XMP and its extended part, ICC, IPTC, MPF and
//! the frame header the panel's size comes from). Without an XMP of its own, the panel looks for
//! a packet in the whole file: then the segments of the other pictures MPF lists (an Ultra HDR
//! gain map, a second view) are read too. The coded data, and whatever follows the pictures
//! (a Motion Photo's video), is not read.

use std::ops::Range;

use super::{Plan, Sparse, Stop, noted, plan_of};

const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub(super) fn plan(s: &Sparse) -> Plan {
    let mut needs = Vec::new();
    let skip = walk(s, &mut needs);
    plan_of(needs, skip)
}

fn walk(s: &Sparse, needs: &mut Vec<Range<usize>>) -> Result<Vec<Range<usize>>, Stop> {
    let (end, segments) = head(s, 0)?;
    let own_xmp = segments.iter().any(|&(m, at, e)| m == 0xE1 && s.bytes[at + 4..e].starts_with(XMP));
    let mut read = vec![0..end];
    if !own_xmp {
        for at in mpf_images(s, &segments)? {
            if let Some((e, _)) = noted(head(s, at), needs)? {
                read.push(at..e);
            }
        }
    }
    Ok(super::complement(&read, s.len()))
}

/// The segments of a JPEG: marker, start, end.
pub(crate) type Segments = Vec<(u8, usize, usize)>;

/// The segments of the JPEG starting at `at` (marker, start, end), and where its coded data
/// starts: after the header of its first scan (`ultrahdr::segments` and the decoders stop there;
/// the EXIF reader goes on looking for markers, but a photo has its EXIF before). A JPEG that
/// is not so (bytes between its segments, which the EXIF reader passes over, an end before the
/// first scan) is read whole.
pub(crate) fn head(s: &Sparse, at: usize) -> Result<(usize, Segments), Stop> {
    if s.take(at, 2)? != [0xFF, 0xD8] {
        return Err(Stop::Whole);
    }
    let mut segments = Vec::new();
    let mut i = at + 2;
    loop {
        let b = s.take(i, 2)?;
        if b[0] != 0xFF {
            return Err(Stop::Whole);
        }
        match b[1] {
            // Fill bytes before a marker.
            0xFF => i += 1,
            // Markers without a length.
            0x01 | 0xD0..=0xD7 => i += 2,
            0x00 | 0xD8 | 0xD9 => return Err(Stop::Whole),
            marker => {
                let len = s.u16(i + 2, false)? as usize;
                let end = i + 2 + len;
                if len < 2 || end > s.len() {
                    return Err(Stop::Whole);
                }
                s.at(i..end)?;
                if marker == 0xDA {
                    return Ok((end, segments));
                }
                segments.push((marker, i, end));
                i = end;
            }
        }
    }
}

/// Where the other pictures of an MPF file start (Multi-Picture Format, CIPA DC-007): the
/// table of its first directory, offsets from the MPF header; the first picture is this one.
fn mpf_images(s: &Sparse, segments: &[(u8, usize, usize)]) -> Result<Vec<usize>, Stop> {
    let Some(&(_, start, end)) = segments.iter().find(|&&(m, at, e)| m == 0xE2 && s.bytes[at + 4..e].starts_with(b"MPF\0")) else { return Ok(Vec::new()) };
    let b = &s.bytes[..end];
    let tiff = start + 8;
    let little = match b.get(tiff..tiff + 2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Ok(Vec::new()),
    };
    let u16_at = |at: usize| b.get(at..at + 2).map(|x| if little { u16::from_le_bytes([x[0], x[1]]) } else { u16::from_be_bytes([x[0], x[1]]) });
    let u32_at = |at: usize| b.get(at..at + 4).map(|x| if little { u32::from_le_bytes([x[0], x[1], x[2], x[3]]) } else { u32::from_be_bytes([x[0], x[1], x[2], x[3]]) } as usize);
    let Some(ifd) = u32_at(tiff + 4).map(|o| tiff + o) else { return Ok(Vec::new()) };
    let count = u16_at(ifd).unwrap_or(0) as usize;
    let Some(entry) = (0..count).map(|k| ifd + 2 + k * 12).find(|&e| u16_at(e) == Some(0xB002)) else { return Ok(Vec::new()) };
    let (Some(len), Some(table)) = (u32_at(entry + 4), u32_at(entry + 8).map(|o| tiff + o)) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    // The first picture's offset is 0: this one.
    for k in 1..len / 16 {
        let offset = u32_at(table + k * 16 + 8).ok_or(Stop::Whole)?;
        if offset > 0 && tiff + offset < s.len() {
            out.push(tiff + offset);
        }
    }
    Ok(out)
}
