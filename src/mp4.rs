//! The preview some cameras store inside their MP4 files, which gives a thumbnail without opening
//! the video: `moov/udta/thmb` (the Umino UM-D1M dashcam: 320×180, ~11 KB) or the standard cover
//! `moov/udta/meta/ilst/covr` (DJI Mini 4 Pro: 1280×720, ~625 KB; music and films too). 2026-10-27.

/// Bytes read from the start of a video to look for the preview: the whole `moov` of a
/// fragmented dashcam video fits (12 KB for 3 minutes).
pub const HEAD: u64 = 64 << 10;

/// Largest preview read.
pub const IMAGE_MAX: u64 = 4 << 20;

/// Type and whole size of the box whose header starts `header` (at least 16 bytes, or what is
/// left of the file); size 0 means "to the end of the file" and is returned as 0.
pub fn box_header(header: &[u8]) -> Option<([u8; 4], u64)> {
    let size = u32::from_be_bytes(header.get(..4)?.try_into().ok()?) as u64;
    let kind = header.get(4..8)?.try_into().ok()?;
    let size = if size == 1 { u64::from_be_bytes(header.get(8..16)?.try_into().ok()?) } else { size };
    (size == 0 || size >= 8).then_some((kind, size))
}

/// The preview, if `head` holds the whole `moov`.
pub fn thumbnail(head: &[u8]) -> Option<&[u8]> {
    let udta = child(child(head, b"moov")?, b"udta")?;
    match child(udta, b"thmb") {
        Some(thmb) => image(thmb),
        None => image(child(child(child(udta, b"meta")?.get(4..)?, b"ilst")?, b"covr")?),
    }
}

/// A JPEG or PNG: the content itself (`thmb`), or inside a `data` box after its type and
/// locale (`covr`).
pub fn image(content: &[u8]) -> Option<&[u8]> {
    let picture = |b: &[u8]| b.starts_with(&[0xFF, 0xD8]) || b.starts_with(b"\x89PNG");
    if picture(content) {
        return Some(content);
    }
    let data = child(content, b"data")?.get(8..)?;
    picture(data).then_some(data)
}

/// The content of the first box of type `kind` among the boxes in `data`.
fn child<'a>(mut data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    while data.len() >= 8 {
        let size = u32::from_be_bytes(data[..4].try_into().ok()?) as usize;
        let (header, size) = match size {
            1 => (16, usize::try_from(u64::from_be_bytes(data.get(8..16)?.try_into().ok()?)).ok()?),
            0 => (8, data.len()), // to the end
            s => (8, s),
        };
        if size < header || size > data.len() {
            return None; // cut off: the box goes past what was read
        }
        if &data[4..8] == kind {
            return Some(&data[header..size]);
        }
        data = &data[size..];
    }
    None
}

#[cfg(test)]
mod tests {
    use super::thumbnail;

    fn mp4_box(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut b = ((content.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(kind);
        b.extend_from_slice(content);
        b
    }

    #[test]
    fn box_headers() {
        assert_eq!(super::box_header(&mp4_box(b"ftyp", b"isomisom")), Some((*b"ftyp", 16)));
        let big = [&1u32.to_be_bytes()[..], b"mdat", &(5u64 << 32).to_be_bytes()].concat();
        assert_eq!(super::box_header(&big), Some((*b"mdat", 5 << 32)));
        assert_eq!(super::box_header(&[0, 0, 0, 3, b'x', b'x', b'x', b'x']), None);
    }

    #[test]
    fn preview_in_moov() {
        let jpeg = [0xFF, 0xD8, 1, 2, 3];
        let udta = mp4_box(b"udta", &mp4_box(b"thmb", &jpeg));
        let moov = mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 20]), udta].concat());
        let file = [mp4_box(b"ftyp", b"isom"), mp4_box(b"free", &[0; 10]), moov.clone(), mp4_box(b"mdat", &[9; 50])].concat();
        assert_eq!(thumbnail(&file), Some(&jpeg[..]));
        // Cut inside the moov: not there.
        assert_eq!(thumbnail(&file[..30 + moov.len() / 2]), None);
        // A cover in udta/meta/ilst/covr, as DJI drones and music files have it.
        let covr = mp4_box(b"covr", &mp4_box(b"data", &[&[0, 0, 0, 13, 0, 0, 0, 0][..], &jpeg].concat()));
        let meta = mp4_box(b"meta", &[&[0u8; 4][..], &mp4_box(b"hdlr", &[0; 24]), &mp4_box(b"ilst", &covr)].concat());
        let moov = mp4_box(b"moov", &mp4_box(b"udta", &meta));
        assert_eq!(thumbnail(&[mp4_box(b"ftyp", b"isom"), moov].concat()), Some(&jpeg[..]));
        // No preview.
        assert_eq!(thumbnail(&[mp4_box(b"ftyp", b"isom"), mp4_box(b"moov", &mp4_box(b"mvhd", &[0; 20]))].concat()), None);
    }
}
