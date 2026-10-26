//! Ultra HDR JPEGs (Android 14 and later, most phone photos): the photo, then a second JPEG, the
//! gain map, after its end. The MPF segment of the photo says where the gain map starts and how
//! long it is, the XMP container directory says its length again. Turned as a whole: both
//! pictures the same way, then lengths and offsets written again, so the photo stays HDR.

use crate::encode::{Format, Options, Original};
use crate::transform::{self, Turn};

/// Where the parts of an Ultra HDR file are: the gain map from `gain` to the end (zeros after
/// it are padding).
struct Parts {
    gain: usize,
}

/// The two pictures of an Ultra HDR file, if it is one: an MPF segment listing exactly two
/// images, the second starting where the photo ends and reaching the end of the file.
fn parts(b: &[u8]) -> Option<Parts> {
    let m = mpf(b)?;
    if m.images != 2 {
        return None;
    }
    let size = read_u32(b, m.table + 16 + 4, m.little)? as usize;
    let gain = m.tiff + read_u32(b, m.table + 16 + 8, m.little)? as usize;
    let end = gain.checked_add(size)?;
    let jpeg = b.get(gain..end)?;
    (jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) && b[end..].iter().all(|&x| x == 0)).then_some(Parts { gain })
}

/// Is this an Ultra HDR JPEG that `turned` can turn?
pub fn is(b: &[u8]) -> bool {
    parts(b).is_some()
}

/// The file turned, photo and gain map; `None` if it is not an Ultra HDR file. Also says
/// whether it lost quality (a picture whose size is not a multiple of the JPEG blocks is encoded
/// again at quality 95).
pub fn turned(b: &[u8], turn: Turn) -> Option<Result<(Vec<u8>, bool), String>> {
    let p = parts(b)?;
    let (photo, gain) = (&b[..p.gain], &b[p.gain..]);
    let end = gain.len() - gain.iter().rev().take_while(|&&x| x == 0).count();
    let gain = &gain[..end];
    Some((|| {
        // The gain map has the photo's pixels in the photo's orientation: it turns as they do.
        let total = turn.after(transform::exif_turn(photo));
        let (mut photo, lossy_photo) = one(photo, turn, |j| transform::jpeg_lossless(j, turn))?;
        let (gain, lossy_gain) = one(gain, total, |j| transform::jpeg_lossless_total(j, total))?;
        set_xmp_length(&mut photo, gain.len());
        let m = mpf(&photo).ok_or("Ultra HDR: MPF lost")?;
        let photo_len = photo.len();
        // First the photo (offset 0), then the gain map right after it.
        write_u32(&mut photo, m.table + 4, photo_len as u32, m.little);
        write_u32(&mut photo, m.table + 8, 0, m.little);
        write_u32(&mut photo, m.table + 16 + 4, gain.len() as u32, m.little);
        write_u32(&mut photo, m.table + 16 + 8, (photo_len - m.tiff) as u32, m.little);
        photo.extend_from_slice(&gain);
        Ok((photo, lossy_photo || lossy_gain))
    })())
}

/// One of the two pictures turned: without encoding again when its size allows it, otherwise
/// at quality 95; the segments the turn left out (XMP, MPF…) put back.
fn one(jpeg: &[u8], turn: Turn, lossless: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> Result<(Vec<u8>, bool), String> {
    let (out, lossy) = match lossless(jpeg) {
        Some(out) => (out, false),
        None => {
            let mut img = crate::decode::decode_full(jpeg, "x.jpg")?;
            turn.apply(&mut img);
            let o = Options { format: Format::Jpeg, jpeg_quality: 95, ..Options::default() };
            (Original::with_meta(img, jpeg).encode(&o)?, true)
        }
    };
    Ok((keep_segments(jpeg, out), lossy))
}

/// `out` with the APP segments of `from` it does not have, after its own: the encoder writes
/// only JFIF, EXIF and the colour profile.
fn keep_segments(from: &[u8], mut out: Vec<u8>) -> Vec<u8> {
    // A segment is known by its marker and signature ("Exif", "MPF", the XMP namespace…), up to
    // the first zero byte: the EXIF of the turned photo has another orientation.
    let key = |b: &[u8], (m, s, e): (u8, usize, usize)| (m, b[s + 4..e].iter().take(40).take_while(|&&c| c != 0).copied().collect::<Vec<u8>>());
    let have: Vec<_> = segments(&out).into_iter().map(|s| key(&out, s)).collect();
    let missing: Vec<u8> = segments(from)
        .into_iter()
        .filter(|&(m, ..)| (0xE0..=0xEF).contains(&m))
        .filter(|&s| !have.contains(&key(from, s)))
        .flat_map(|(_, s, e)| from[s..e].to_vec())
        .collect();
    let at = segments(&out).into_iter().take_while(|&(m, ..)| (0xE0..=0xEF).contains(&m)).last().map_or(2, |(_, _, e)| e);
    out.splice(at..at, missing);
    out
}

/// The segments before the coded data: marker, start (at 0xFF) and end.
fn segments(b: &[u8]) -> Vec<(u8, usize, usize)> {
    let mut v = Vec::new();
    let mut i = 2;
    while i + 4 <= b.len() && b[i] == 0xFF {
        let marker = b[i + 1];
        let end = i + 2 + u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if marker == 0xDA || end > b.len() {
            break;
        }
        v.push((marker, i, end));
        i = end;
    }
    v
}

/// The MPF segment of a JPEG.
struct Mpf {
    /// Start of its TIFF header: the offsets of the images count from here.
    tiff: usize,
    /// Start of the table of images, 16 bytes each.
    table: usize,
    images: usize,
    little: bool,
}

fn mpf(b: &[u8]) -> Option<Mpf> {
    let (_, s, _) = segments(b).into_iter().find(|&(m, s, e)| m == 0xE2 && b[s + 4..e].starts_with(b"MPF\0"))?;
    let tiff = s + 8;
    let little = match b.get(tiff..tiff + 2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let ifd = tiff + read_u32(b, tiff + 4, little)? as usize;
    let count = read_u16(b, ifd, little)? as usize;
    // MPEntry: the table's length in the count field, its place where the offset says.
    let e = (0..count).map(|k| ifd + 2 + k * 12).find(|&e| read_u16(b, e, little) == Some(0xB002))?;
    let len = read_u32(b, e + 4, little)? as usize;
    let table = tiff + read_u32(b, e + 8, little)? as usize;
    (len.is_multiple_of(16) && table + len <= b.len()).then_some(Mpf { tiff, table, images: len / 16, little })
}

/// In the XMP of the photo, the length of the gain map (`Item:Length` of the item whose
/// `Item:Semantic` is "GainMap"); the segment's length follows.
fn set_xmp_length(b: &mut Vec<u8>, len: usize) {
    let Some((_, s, e)) = segments(b).into_iter().find(|&(m, s, e)| m == 0xE1 && b[s + 4..e].starts_with(b"http://ns.adobe.com/xap/1.0/\0")) else { return };
    let body = &b[s + 4..e];
    let Some(at) = find(body, b"Item:Semantic=\"GainMap\"") else { return };
    // The element around it: from its '<' to its '>'.
    let open = body[..at].iter().rposition(|&c| c == b'<').unwrap_or(0);
    let close = at + find(&body[at..], b">").unwrap_or(body.len() - at);
    let Some(l) = find(&body[open..close], b"Item:Length=\"") else { return };
    let start = open + l + b"Item:Length=\"".len();
    let Some(q) = find(&body[start..], b"\"") else { return };
    let mut new_body = body[..start].to_vec();
    new_body.extend_from_slice(len.to_string().as_bytes());
    new_body.extend_from_slice(&body[start + q..]);
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&((new_body.len() + 2) as u16).to_be_bytes());
    segment.extend_from_slice(&new_body);
    b.splice(s..e, segment);
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn read_u16(b: &[u8], at: usize, little: bool) -> Option<u16> {
    let x: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
    Some(if little { u16::from_le_bytes(x) } else { u16::from_be_bytes(x) })
}

fn read_u32(b: &[u8], at: usize, little: bool) -> Option<u32> {
    let x: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
    Some(if little { u32::from_le_bytes(x) } else { u32::from_be_bytes(x) })
}

fn write_u32(b: &mut [u8], at: usize, v: u32, little: bool) {
    b[at..at + 4].copy_from_slice(&if little { v.to_le_bytes() } else { v.to_be_bytes() });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView};

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 4) as u8, (y * 4) as u8, 50])));
        let mut v = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Jpeg).unwrap();
        v
    }

    fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
        [&[0xFF, marker][..], &((body.len() + 2) as u16).to_be_bytes(), body].concat()
    }

    /// An Ultra HDR file as phones write it: XMP with the container directory and MPF in the
    /// photo, the gain map after it.
    pub(crate) fn sample(w: u32, h: u32) -> Vec<u8> {
        let gain = jpeg(w / 2, h / 2);
        let xmp = format!(
            "http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><rdf:Seq><rdf:li><Container:Item Item:Semantic=\"Primary\" Item:Mime=\"image/jpeg\"/></rdf:li><rdf:li><Container:Item Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\" Item:Length=\"{}\"/></rdf:li></rdf:Seq><hdrgm:Version>1.0</hdrgm:Version></x:xmpmeta>",
            gain.len()
        );
        // MPF: TIFF header, one IFD entry (MPEntry, 32 bytes at offset 26), then the table.
        let mut mpf = b"MPF\0II*\0".to_vec();
        mpf.extend_from_slice(&8u32.to_le_bytes());
        mpf.extend_from_slice(&1u16.to_le_bytes());
        mpf.extend_from_slice(&[0x02, 0xB0, 7, 0]);
        mpf.extend_from_slice(&32u32.to_le_bytes());
        mpf.extend_from_slice(&26u32.to_le_bytes());
        mpf.extend_from_slice(&0u32.to_le_bytes());
        mpf.extend_from_slice(&[0; 32]);
        let photo = jpeg(w, h);
        let mut photo = [&photo[..2], &segment(0xE1, xmp.as_bytes()), &segment(0xE2, &mpf), &photo[2..]].concat();
        let m = mpf_at(&photo);
        let len = photo.len();
        write_u32(&mut photo, m.table + 4, len as u32, true);
        write_u32(&mut photo, m.table + 16 + 4, gain.len() as u32, true);
        write_u32(&mut photo, m.table + 16 + 8, (len - m.tiff) as u32, true);
        photo.extend_from_slice(&gain);
        photo
    }

    fn mpf_at(b: &[u8]) -> Mpf {
        mpf(b).unwrap()
    }

    /// Lengths and offsets after a turn point at the turned gain map, both pictures turned the
    /// same way, without encoding again (sizes multiple of 16); padding after it is fine. A plain
    /// JPEG is not an Ultra HDR one.
    #[test]
    fn turn_ultra_hdr() {
        let file = sample(64, 32);
        assert!(is(&file) && is(&[file.as_slice(), &[0; 8]].concat()));
        assert!(!is(&jpeg(64, 32)));
        let (out, lossy) = turned(&file, Turn::default().then(transform::Op::Cw)).unwrap().unwrap();
        assert!(!lossy);
        let p = parts(&out).expect("still Ultra HDR");
        let photo = image::load_from_memory(&out[..p.gain]).unwrap();
        let gain = image::load_from_memory(&out[p.gain..]).unwrap();
        assert_eq!((photo.dimensions(), gain.dimensions()), ((32, 64), (16, 32)));
        let xmp = String::from_utf8_lossy(&out[..p.gain]).into_owned();
        assert!(xmp.contains(&format!("Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\" Item:Length=\"{}\"", out.len() - p.gain)));
        // Saving goes through it, and the whole file is still read as the photo.
        let (saved, _) = crate::encode::turned_file(&file, "a.jpg", Turn::default().then(transform::Op::Cw)).unwrap();
        assert_eq!(saved, out);
        // A size that is not a multiple of the blocks: encoded again, still Ultra HDR.
        let (out, lossy) = turned(&sample(72, 40), Turn::default().then(transform::Op::FlipH)).unwrap().unwrap();
        assert!(lossy && is(&out));
    }

    /// Real phone photos: `EYECANSEE_SAMPLES=<folder>` (never changed), the turned files in
    /// `EYECANSEE_SAMPLES_OUT`, to look at. Every Ultra HDR photo stays one, both pictures turned.
    #[test]
    #[ignore]
    fn ultra_hdr_live() {
        let dir = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES");
        let out_dir = std::env::var("EYECANSEE_SAMPLES_OUT").expect("EYECANSEE_SAMPLES_OUT");
        let (mut n, mut lossless) = (0, 0);
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let b = std::fs::read(&path).unwrap();
            if !is(&b) {
                println!("not Ultra HDR: {}", path.display());
                continue;
            }
            let p = parts(&b).unwrap();
            let before = (image::load_from_memory(&b[..p.gain]).unwrap().dimensions(), image::load_from_memory(&b[p.gain..]).unwrap().dimensions());
            let (out, lossy) = turned(&b, Turn::default().then(transform::Op::Cw)).unwrap().unwrap();
            let q = parts(&out).expect("still Ultra HDR");
            let photo = crate::decode::decode_full(&out[..q.gain], "x.jpg").unwrap().dimensions();
            let gain = image::load_from_memory(&out[q.gain..]).unwrap().dimensions();
            assert_eq!((photo, gain), ((before.0.1, before.0.0), (before.1.1, before.1.0)), "{}", path.display());
            n += 1;
            lossless += usize::from(!lossy);
            std::fs::write(std::path::Path::new(&out_dir).join(path.file_name().unwrap()), &out).unwrap();
        }
        println!("{n} Ultra HDR turned, {lossless} without encoding again");
    }
}
