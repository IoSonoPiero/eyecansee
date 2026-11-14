//! The video of a Motion Photo (Google, Samsung and Xiaomi phones), an MP4 after the end of the
//! JPEG, found for the viewer to play it (issue #123) through the video engine, from memory.

use std::sync::Arc;

/// Where the video starts in a Motion Photo: from the XMP container directory (`Item:Length` of
/// the `MotionPhoto` item, the last part of the file), from the older `GCamera:MicroVideoOffset`
/// (bytes from the video's start to the end of the file), or after Samsung's `MotionPhoto_Data`.
/// Only where an MP4 or MOV really starts (its first box is `ftyp`).
pub fn video_start(b: &[u8]) -> Option<usize> {
    if !b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return None;
    }
    let from_end = |n: usize| b.len().checked_sub(n);
    // The XMP is near the start: the whole file (the video too) is not searched for it.
    let head = &b[..b.len().min(HEAD)];
    let container = || {
        let at = find(head, b"Item:Semantic=\"MotionPhoto\"")?;
        // The attributes of that item only: between the `<` before and the `>` after.
        let open = head[..at].iter().rposition(|&c| c == b'<')?;
        let close = at + head[at..].iter().position(|&c| c == b'>')?;
        number(&head[open..close], b"Item:Length=\"").and_then(from_end)
    };
    let micro = || number(head, b"GCamera:MicroVideoOffset=\"").and_then(from_end);
    // Samsung's trailer ends with `SEFT`.
    let samsung = || b.ends_with(b"SEFT").then(|| find_last(b, b"MotionPhoto_Data")).flatten().map(|at| at + b"MotionPhoto_Data".len());
    let video = |s: &usize| *s > 0 && b.get(s + 4..s + 8) == Some(b"ftyp");
    container().filter(video).or_else(|| micro().filter(video)).or_else(|| samsung().filter(video))
}

/// How far from the start the XMP of a Motion Photo is looked for.
const HEAD: usize = 256 << 10;

/// The video of a Motion Photo, played by the engine from memory.
pub struct Video(pub Arc<Vec<u8>>);

impl crate::mpv::Reader for Video {
    fn size(&self) -> u64 {
        self.0.len() as u64
    }

    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        let start = (offset as usize).min(self.0.len());
        let end = start.saturating_add(len as usize).min(self.0.len());
        Ok(self.0[start..end].to_vec())
    }
}

/// The digits after `key` (an attribute's name with `="`), as a number.
fn number(hay: &[u8], key: &[u8]) -> Option<usize> {
    let at = find(hay, key)? + key.len();
    let digits = hay[at..].iter().take_while(|c| c.is_ascii_digit()).count();
    std::str::from_utf8(&hay[at..at + digits]).ok()?.parse().ok()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn find_last(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpv::Reader;

    const MP4: &[u8] = b"\0\0\0\x18ftypmp42\0\0\0\0isommp42moov";

    fn photo(xmp: &str) -> Vec<u8> {
        [&[0xFF, 0xD8, 0xFF, 0xE1][..], xmp.as_bytes(), &[0xFF, 0xD9]].concat()
    }

    #[test]
    fn container_item_length() {
        // A gain map item before the video's, with its own length: the video's is taken.
        let xmp = format!(
            "<Container:Item Item:Mime=\"image/jpeg\" Item:Semantic=\"GainMap\" Item:Length=\"9\"/><Container:Item Item:Mime=\"video/mp4\" Item:Semantic=\"MotionPhoto\" Item:Length=\"{}\" Item:Padding=\"0\"/>",
            MP4.len()
        );
        let p = photo(&xmp);
        let b = [p.as_slice(), &[1; 9], MP4].concat();
        assert_eq!(video_start(&b), Some(p.len() + 9));
    }

    #[test]
    fn micro_video_offset() {
        let p = photo(&format!("GCamera:MicroVideo=\"1\" GCamera:MicroVideoOffset=\"{}\"", MP4.len()));
        let b = [p.as_slice(), MP4].concat();
        assert_eq!(video_start(&b), Some(p.len()));
    }

    #[test]
    fn samsung_marker() {
        let p = photo("nothing");
        let b = [p.as_slice(), b"MotionPhoto_Data", MP4, b"trailer SEFT"].concat();
        assert_eq!(video_start(&b), Some(p.len() + 16));
        // Without Samsung's trailer the marker is not looked for.
        let b = [p.as_slice(), b"MotionPhoto_Data", MP4].concat();
        assert_eq!(video_start(&b), None);
    }

    #[test]
    fn no_video_where_said() {
        // Said to be a Motion Photo, with no MP4 after it (a Pixel photo without its video).
        let p = photo("GCamera:MotionPhoto=\"1\" GCamera:MicroVideoOffset=\"40\"");
        assert_eq!(video_start(&[p.as_slice(), &[0; 40]].concat()), None);
        assert_eq!(video_start(MP4), None);
    }

    #[test]
    fn reader_inside_the_video() {
        let v = Video(Arc::new(MP4.to_vec()));
        assert_eq!(v.size(), MP4.len() as u64);
        assert_eq!(v.read(4, 4).unwrap(), b"ftyp");
        assert_eq!(v.read(MP4.len() as u64 - 2, 100).unwrap().len(), 2);
        assert!(v.read(1000, 4).unwrap().is_empty());
    }
}
