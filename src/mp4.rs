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

/// A box of 32-bit size: header and content.
fn boxed(kind: &[u8; 4], content: &[u8]) -> Option<Vec<u8>> {
    let size = u32::try_from(content.len() + 8).ok()?;
    Some([&size.to_be_bytes()[..], kind, content].concat())
}

/// The camera's own preview, `moov/udta/thmb`, as a whole box; `moov` is the box's content.
fn thmb_box(moov: &[u8]) -> Option<Vec<u8>> {
    boxed(b"thmb", child(child(moov, b"udta")?, b"thmb")?)
}

/// `moov` (the whole box) with the boxes `extra` at the end of its `udta`, made if missing.
/// `None` for 64-bit sizes, a box cut off, or a `thmb` already there.
fn with_in_udta(moov: &[u8], extra: &[u8]) -> Option<Vec<u8>> {
    if u32::from_be_bytes(moov.get(..4)?.try_into().ok()?) as usize != moov.len() || moov.len() < 8 {
        return None;
    }
    let (mut content, mut rest, mut found) = (Vec::new(), &moov[8..], false);
    while !rest.is_empty() {
        let size = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        if size < 8 || size > rest.len() {
            return None;
        }
        let b = &rest[..size];
        if &b[4..8] == b"udta" {
            if child(&b[8..], b"thmb").is_some() {
                return None;
            }
            content.extend(boxed(b"udta", &[&b[8..], extra].concat())?);
            found = true;
        } else {
            content.extend_from_slice(b);
        }
        rest = &rest[size..];
    }
    if !found {
        content.extend(boxed(b"udta", extra)?);
    }
    boxed(b"moov", &content)
}

/// Offset and size of the top-level box `kind` of a file, reading only the headers.
fn find_in_file(f: &mut std::fs::File, len: u64, kind: &[u8; 4]) -> std::io::Result<Option<(u64, u64)>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut at = 0;
    while at + 8 <= len {
        let mut header = [0u8; 16];
        f.seek(SeekFrom::Start(at))?;
        let n = f.read(&mut header)?;
        let Some((k, size)) = box_header(&header[..n]) else { return Ok(None) };
        let size = if size == 0 { len - at } else { size };
        if &k == kind {
            return Ok((at + size <= len).then_some((at, size)));
        }
        at += size;
    }
    Ok(None)
}

/// The whole box at `at`, of `size` bytes (at most 64 MB: a `moov` of sample tables).
fn read_at(f: &mut std::fs::File, at: u64, size: u64) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::{Read, Seek, SeekFrom};
    if size > 64 << 20 {
        return Ok(None);
    }
    let mut b = vec![0; size as usize];
    f.seek(SeekFrom::Start(at))?;
    f.read_exact(&mut b)?;
    Ok(Some(b))
}

/// The camera's preview (`moov/udta/thmb`, the Umino dashcam) of `original` written into `copy`,
/// which FFmpeg made without it (it does not know the box). Only when the copy's `moov` is the
/// last box of the file, as FFmpeg writes it without `+faststart`: the sample tables point to the
/// media data before it, so `moov` can grow. `false` when there was nothing to do.
pub fn copy_preview(original: &std::path::Path, copy: &std::path::Path) -> std::io::Result<bool> {
    use std::io::{Seek, SeekFrom, Write};
    let mut o = std::fs::File::open(original)?;
    let len = o.metadata()?.len();
    let Some((at, size)) = find_in_file(&mut o, len, b"moov")? else { return Ok(false) };
    let Some(thmb) = read_at(&mut o, at, size)?.and_then(|m| thmb_box(m.get(8..)?)) else { return Ok(false) };
    let mut c = std::fs::OpenOptions::new().read(true).write(true).open(copy)?;
    let len = c.metadata()?.len();
    let Some((at, size)) = find_in_file(&mut c, len, b"moov")?.filter(|(at, size)| at + size == len) else { return Ok(false) };
    let Some(moov) = read_at(&mut c, at, size)?.and_then(|m| with_in_udta(&m, &thmb)) else { return Ok(false) };
    c.seek(SeekFrom::Start(at))?;
    c.write_all(&moov)?;
    c.set_len(at + moov.len() as u64)?;
    Ok(true)
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

    #[test]
    fn preview_copied() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4];
        let ftyp = mp4_box(b"ftyp", b"isomisom");
        let mdat = mp4_box(b"mdat", &[7; 100]);
        let trak = mp4_box(b"trak", &[1; 20]);
        let camera = mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 12]), trak.clone(), mp4_box(b"udta", &mp4_box(b"thmb", &jpeg))].concat());
        let dir = std::env::temp_dir().join(format!("eyecansee-mp4-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (original, copy) = (dir.join("o.mp4"), dir.join("c.mp4"));
        // The camera's file with its moov first, the copy with moov last and a udta of FFmpeg's.
        std::fs::write(&original, [ftyp.clone(), camera.clone(), mdat.clone()].concat()).unwrap();
        let tags = mp4_box(b"udta", &mp4_box(b"meta", &[0; 10]));
        let ffmpeg = mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 12]), trak.clone(), tags.clone()].concat());
        std::fs::write(&copy, [ftyp.clone(), mdat.clone(), ffmpeg.clone()].concat()).unwrap();
        assert!(super::copy_preview(&original, &copy).unwrap());
        let written = std::fs::read(&copy).unwrap();
        let moov = &written[ftyp.len() + mdat.len()..];
        let udta = super::child(super::child(moov, b"moov").unwrap(), b"udta").unwrap();
        assert!(udta.starts_with(&mp4_box(b"meta", &[0; 10])), "FFmpeg's tags kept first");
        assert_eq!(super::child(udta, b"thmb"), Some(&jpeg[..]));
        assert_eq!(&written[..ftyp.len() + mdat.len()], &[ftyp.clone(), mdat.clone()].concat()[..], "media data untouched");
        assert!(!super::copy_preview(&original, &copy).unwrap(), "only once");
        // Without a udta in the copy one is made; with moov not last nothing is done.
        std::fs::write(&copy, [ftyp.clone(), mdat.clone(), mp4_box(b"moov", &trak)].concat()).unwrap();
        assert!(super::copy_preview(&original, &copy).unwrap());
        let written = std::fs::read(&copy).unwrap();
        assert_eq!(thumbnail(&written[ftyp.len() + mdat.len()..]), Some(&jpeg[..]), "the preview is found as EyeCanSee reads it");
        std::fs::write(&copy, [ftyp.clone(), ffmpeg.clone(), mdat.clone()].concat()).unwrap();
        assert!(!super::copy_preview(&original, &copy).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
