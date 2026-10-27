//! The JPEG preview some cameras store inside their MP4 files (`moov/udta/thmb`; the Umino
//! UM-D1M dashcam: 320×180, ~11 KB at the start of every video, 2026-10-27). Read from the first
//! bytes of the file, it gives a thumbnail without opening the video.

/// Bytes read from the start of a video to look for the preview: the whole `moov` of a dashcam
/// video fits (12 KB for 3 minutes); a `moov` at the end (phones) is not looked for.
pub const HEAD: u64 = 64 << 10;

/// The JPEG in `moov/udta/thmb`, if `head` holds it whole.
pub fn thumbnail(head: &[u8]) -> Option<&[u8]> {
    let moov = child(head, b"moov")?;
    let jpeg = child(child(moov, b"udta")?, b"thmb")?;
    jpeg.starts_with(&[0xFF, 0xD8]).then_some(jpeg)
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
    fn preview_in_moov() {
        let jpeg = [0xFF, 0xD8, 1, 2, 3];
        let udta = mp4_box(b"udta", &mp4_box(b"thmb", &jpeg));
        let moov = mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 20]), udta].concat());
        let file = [mp4_box(b"ftyp", b"isom"), mp4_box(b"free", &[0; 10]), moov.clone(), mp4_box(b"mdat", &[9; 50])].concat();
        assert_eq!(thumbnail(&file), Some(&jpeg[..]));
        // Cut inside the moov: not there.
        assert_eq!(thumbnail(&file[..30 + moov.len() / 2]), None);
        // No preview.
        assert_eq!(thumbnail(&[mp4_box(b"ftyp", b"isom"), mp4_box(b"moov", &mp4_box(b"mvhd", &[0; 20]))].concat()), None);
    }
}
