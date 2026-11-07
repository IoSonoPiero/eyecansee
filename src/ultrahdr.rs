//! Ultra HDR JPEGs (Android 14 and later, most phone photos): the photo, then a second JPEG, the
//! gain map, after its end. The MPF segment of the photo says where the gain map starts and how
//! long it is, the XMP container directory says its length again. Turned as a whole: both
//! pictures the same way, then lengths and offsets written again, so the photo stays HDR. The
//! same for Rotate and crop (`edited`), the gain map edited at its own scale.

use image::DynamicImage;

use crate::encode::{Format, Options, Original};
use crate::rotate::{Crop, Edit};
use crate::transform::{self, Turn};

/// Where the parts of an Ultra HDR file are: the gain map from `gain` to `end`; after it zeros
/// (padding) or, in a Motion Photo, its video (`video`).
struct Parts {
    gain: usize,
    end: usize,
    video: bool,
}

/// The two pictures of an Ultra HDR file, if it is one: an MPF segment listing exactly two
/// images, the second starting where the photo ends and followed only by padding or by the video
/// of a Motion Photo (an MP4 or MOV: its first box is `ftyp`; Pixel and Xiaomi, #54).
fn parts(b: &[u8]) -> Option<Parts> {
    let m = mpf(b)?;
    if m.images != 2 {
        return None;
    }
    let size = read_u32(b, m.table + 16 + 4, m.little)? as usize;
    let gain = m.tiff + read_u32(b, m.table + 16 + 8, m.little)? as usize;
    let end = gain.checked_add(size)?;
    let jpeg = b.get(gain..end)?;
    let video = b.get(end + 4..end + 8) == Some(b"ftyp");
    (jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) && (video || b[end..].iter().all(|&x| x == 0))).then_some(Parts { gain, end, video })
}

/// Is this an Ultra HDR JPEG (with a Motion Photo video after it or not) that `turned` can turn?
pub fn is(b: &[u8]) -> bool {
    parts(b).is_some()
}

/// The file turned, photo and gain map, and the video of a Motion Photo after them shown turned
/// the same way (Piero, 2026-11-07: nothing is lost, #54); `None` if it is not an Ultra HDR file.
/// Also says whether it lost quality (a picture whose size is not a multiple of the JPEG blocks
/// is encoded again at quality 95).
pub fn turned(b: &[u8], turn: Turn) -> Option<Result<(Vec<u8>, bool), String>> {
    let p = parts(b)?;
    let (photo, gain) = split(b)?;
    Some((|| {
        // The gain map has the photo's pixels in the photo's orientation: it turns as they do.
        let total = turn.after(transform::exif_turn(photo));
        let (mut new_photo, lossy_photo) = one(photo, turn, |j| transform::jpeg_lossless(j, turn))?;
        let (gain, lossy_gain) = one(gain, total, |j| transform::jpeg_lossless_total(j, total))?;
        if p.video {
            keep_video_xmp(&mut new_photo, photo)?;
        }
        let mut out = assemble(new_photo, &gain)?;
        if p.video {
            out.extend(crate::mp4::turned(&b[p.end..], turn).ok_or("Motion Photo: its video could not be turned")?);
        }
        Ok((out, lossy_photo || lossy_gain))
    })())
}

/// The XMP of the turned `photo` made again from the original's `old`, keeping the description
/// of the video that is written after the gain map (`xmp::fit_keeping_video`).
fn keep_video_xmp(photo: &mut Vec<u8>, old: &[u8]) -> Result<(), String> {
    let xmp = |b: &[u8]| segments(b).into_iter().find(|&(m, s, e)| m == 0xE1 && b[s + 4..e].starts_with(crate::meta_write::JPEG_XMP));
    let (Some((_, os, oe)), Some((_, s, e))) = (xmp(old), xmp(photo)) else { return Err("Motion Photo: XMP lost".into()) };
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&photo[..])).with_guessed_format().map_err(|e| e.to_string())?.into_dimensions().map_err(|e| e.to_string())?;
    let packet = crate::xmp::fit_keeping_video(&old[os + 4 + crate::meta_write::JPEG_XMP.len()..oe], w, h);
    let body = [crate::meta_write::JPEG_XMP, &packet].concat();
    let len = u16::try_from(body.len() + 2).map_err(|_| "Motion Photo: XMP too long")?;
    photo.splice(s..e, [&[0xFF, 0xE1][..], &len.to_be_bytes(), &body].concat());
    Ok(())
}

/// The photo after "Rotate and crop" (`image`: decoded upright and already edited) written back
/// as Ultra HDR: the gain map gets the same edits, scaled to its size, so the photo stays HDR.
/// `holes` (left, top, right, bottom in pixels of the edited photo: the editor's cover boxes and
/// opaque objects) get the lowest gain on every pixel of the gain map they touch even in part.
pub fn edited(b: &[u8], image: DynamicImage, edits: &[Edit], holes: &[[f32; 4]]) -> Result<Vec<u8>, String> {
    let (photo, gain) = split(b).ok_or("not Ultra HDR")?;
    // Each at its own quality and subsampling: not compressed harder a second time.
    let like = |jpeg| crate::encode::jpeg_quality::like(jpeg, Options { format: Format::Jpeg, ..Options::default() });
    // The gain map upright as the photo shown, then the edits at its scale.
    let turn = transform::exif_turn(photo);
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(photo)).with_guessed_format().map_err(|e| e.to_string())?.into_dimensions().map_err(|e| e.to_string())?;
    let mut g = crate::decode::decode_full(gain, "x.jpg")?;
    let gray = !g.color().has_color();
    turn.apply(&mut g);
    // Long side to long side: the same whatever the orientation.
    let k = g.width().max(g.height()) as f32 / w.max(h) as f32;
    for e in edits {
        let crop = Crop { cx: e.crop.cx * k, cy: e.crop.cy * k, w: e.crop.w * k, h: e.crop.h * k };
        // ponytail: corners outside the photo take the lowest gain (0), no boost when GainMapMin
        // is 0 as on the phones seen; read hdrgm:GainMapMin if a photo has another one.
        Edit { crop, fill: Some([0; 3]), ..*e }.apply(&mut g);
    }
    let (kx, ky) = (g.width() as f32 / image.width() as f32, g.height() as f32 / image.height() as f32);
    lowest_gain(&mut g, holes, kx, ky);
    if gray {
        g = DynamicImage::ImageLuma8(g.to_luma8());
    }
    let new_photo = keep_segments(photo, Original::with_meta(image, photo).encode(&like(photo))?);
    let new_gain = keep_segments(gain, Original::with_meta(g, gain).encode(&like(gain))?);
    assemble(new_photo, &new_gain)
}

/// Gain 0 on every pixel of `g` that a hole touches even in part, `kx`, `ky` its pixels per pixel
/// of the photo: the gain map is smaller than the photo, a pixel of it covers several.
fn lowest_gain(g: &mut DynamicImage, holes: &[[f32; 4]], kx: f32, ky: f32) {
    use image::GenericImage;
    for &[l, t, r, b] in holes {
        let x0 = ((l * kx).floor().max(0.0) as u32).min(g.width());
        let y0 = ((t * ky).floor().max(0.0) as u32).min(g.height());
        let x1 = ((r * kx).ceil().max(0.0) as u32).min(g.width());
        let y1 = ((b * ky).ceil().max(0.0) as u32).min(g.height());
        for (x, y) in (x0..x1).flat_map(|x| (y0..y1).map(move |y| (x, y))) {
            g.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
        }
    }
}

/// The photo and the gain map (without the padding or the video after it), if it is an Ultra
/// HDR file.
pub(crate) fn split(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let p = parts(b)?;
    let gain = &b[p.gain..p.end];
    let end = gain.len() - gain.iter().rev().take_while(|&&x| x == 0).count();
    Some((&b[..p.gain], &gain[..end]))
}

/// The new photo (with the old one's XMP and MPF) and the new gain map in one file: lengths
/// and offsets written again.
fn assemble(mut photo: Vec<u8>, gain: &[u8]) -> Result<Vec<u8>, String> {
    set_xmp_length(&mut photo, gain.len());
    let m = mpf(&photo).ok_or("Ultra HDR: MPF lost")?;
    let photo_len = photo.len();
    // First the photo (offset 0), then the gain map right after it.
    write_u32(&mut photo, m.table + 4, photo_len as u32, m.little);
    write_u32(&mut photo, m.table + 8, 0, m.little);
    write_u32(&mut photo, m.table + 16 + 4, gain.len() as u32, m.little);
    write_u32(&mut photo, m.table + 16 + 8, (photo_len - m.tiff) as u32, m.little);
    photo.extend_from_slice(gain);
    Ok(photo)
}

/// One of the two pictures turned: without encoding again when its size allows it, otherwise
/// at its own quality and subsampling; the segments the turn left out (XMP, MPF…) put back.
fn one(jpeg: &[u8], turn: Turn, lossless: impl FnOnce(&[u8]) -> Option<Vec<u8>>) -> Result<(Vec<u8>, bool), String> {
    let (out, lossy) = match lossless(jpeg) {
        Some(out) => (out, false),
        None => {
            let mut img = crate::decode::decode_full(jpeg, "x.jpg")?;
            turn.apply(&mut img);
            let o = crate::encode::jpeg_quality::like(jpeg, Options { format: Format::Jpeg, ..Options::default() });
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
pub(crate) fn segments(b: &[u8]) -> Vec<(u8, usize, usize)> {
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
            "http://ns.adobe.com/xap/1.0/\0<x:xmpmeta><rdf:Description xmlns:exif=\"http://ns.adobe.com/exif/1.0/\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" exif:PixelXDimension=\"{w}\" crs:HasCrop=\"True\"/><rdf:Seq><rdf:li><Container:Item Item:Semantic=\"Primary\" Item:Mime=\"image/jpeg\"/></rdf:li><rdf:li><Container:Item Item:Semantic=\"GainMap\" Item:Mime=\"image/jpeg\" Item:Length=\"{}\"/></rdf:li></rdf:Seq><hdrgm:Version>1.0</hdrgm:Version></x:xmpmeta>",
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
        // IPTC and a Photoshop thumbnail of the photo, as Photoshop or Lightroom would leave.
        let irb = [crate::meta_write::JPEG_PHOTOSHOP, &crate::meta_write::resource(1036, &[7; 9]), &crate::meta_write::irb_of_iim(&iim())].concat();
        let mut photo = [&photo[..2], &segment(0xE1, xmp.as_bytes()), &segment(0xE2, &mpf), &segment(0xED, &irb), &photo[2..]].concat();
        let m = mpf_at(&photo);
        let len = photo.len();
        write_u32(&mut photo, m.table + 4, len as u32, true);
        write_u32(&mut photo, m.table + 16 + 4, gain.len() as u32, true);
        write_u32(&mut photo, m.table + 16 + 8, (len - m.tiff) as u32, true);
        photo.extend_from_slice(&gain);
        photo
    }

    fn iim() -> Vec<u8> {
        crate::xmp::tests::iim(&[(80, "Ann")])
    }

    /// The photo's XMP and IPTC fit it: its width, no Camera Raw, the IPTC without thumbnail.
    fn fitted(photo: &[u8], w: u32) {
        let xmp = String::from_utf8_lossy(photo).into_owned();
        assert!(xmp.contains(&format!("exif:PixelXDimension=\"{w}\"")) && !xmp.contains("crs:"), "{xmp}");
        let (_, s, e) = segments(photo).into_iter().find(|&(m, ..)| m == 0xED).expect("IPTC kept");
        assert_eq!(&photo[s + 4..e], [crate::meta_write::JPEG_PHOTOSHOP, &crate::meta_write::irb_of_iim(&iim())].concat());
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
        fitted(&out[..p.gain], 32);
        // Saving goes through it, and the whole file is still read as the photo.
        let (saved, _) = crate::save_back::turned_file(&file, "a.jpg", Turn::default().then(transform::Op::Cw)).unwrap();
        assert_eq!(saved, out);
        // A size that is not a multiple of the blocks: encoded again, still Ultra HDR.
        let (out, lossy) = turned(&sample(72, 40), Turn::default().then(transform::Op::FlipH)).unwrap().unwrap();
        assert!(lossy && is(&out));
        fitted(&out[..parts(&out).unwrap().gain], 72);
    }

    /// Rotate and crop: the gain map gets the edit at its scale, the file stays Ultra HDR.
    #[test]
    fn edit_ultra_hdr() {
        let file = sample(64, 32);
        let mut img = crate::decode::decode_full(&file, "a.jpg").unwrap();
        let e = Edit { turn: Turn::default().then(transform::Op::Cw), angle: 10.0, crop: Crop::centred(20.0, 40.0), fill: Some([255; 3]) };
        e.apply(&mut img);
        let out = edited(&file, img, &[e], &[]).unwrap();
        let p = parts(&out).expect("still Ultra HDR");
        let photo = image::load_from_memory(&out[..p.gain]).unwrap();
        let gain = image::load_from_memory(&out[p.gain..]).unwrap();
        assert_eq!((photo.dimensions(), gain.dimensions()), ((20, 40), (10, 20)));
        let xmp = String::from_utf8_lossy(&out[..p.gain]).into_owned();
        assert!(xmp.contains(&format!("Item:Length=\"{}\"", out.len() - p.gain)));
        fitted(&out[..p.gain], 20);
    }

    /// A Motion Photo that is also Ultra HDR (Pixel, Xiaomi): its video after the gain map (#54).
    /// The gain map is found; the editor's copy keeps it, without the video; a lossless turn keeps
    /// the video too, shown turned as the photo, with the same length. Random bytes after the gain
    /// map are still not Ultra HDR.
    #[test]
    fn video_after_gain_map() {
        let file = sample(64, 32);
        let video = crate::mp4::tests::video(160, 90);
        let motion = [file.as_slice(), &video].concat();
        assert!(is(&motion) && split(&motion) == split(&file));
        assert!(!is(&[file.as_slice(), &[7; 40]].concat()));
        let img = crate::decode::decode_full(&motion, "a.jpg").unwrap();
        let out = edited(&motion, img, &[Edit { turn: Turn::default(), angle: 0.0, crop: Crop::centred(64.0, 32.0), fill: None }], &[]).unwrap();
        assert!(is(&out) && !out.windows(4).any(|w| w == b"ftyp"));
        let cw = Turn::default().then(transform::Op::Cw);
        let (out, _) = turned(&motion, cw).unwrap().unwrap();
        let p = parts(&out).expect("still Ultra HDR");
        assert!(p.video && out.len() - p.end == video.len());
        assert_eq!(&out[p.end..], crate::mp4::turned(&video, cw).unwrap());
    }

    /// A hole gives gain 0 to every pixel of the gain map it touches even in part, and to no other.
    #[test]
    fn lowest_gain_where_touched() {
        let mut g = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(20, 10, image::Luma([90])));
        // Half the size of the photo: 10.5..21 × 4..8.2 of the photo is 5.25..10.5 × 2..4.1.
        lowest_gain(&mut g, &[[10.5, 4.0, 21.0, 8.2]], 0.5, 0.5);
        let g = g.to_luma8();
        for (x, y, p) in g.enumerate_pixels() {
            let inside = (5..11).contains(&x) && (2..5).contains(&y);
            assert_eq!(p.0[0], if inside { 0 } else { 90 }, "{x},{y}");
        }
    }

    /// Rotate and crop on real phone photos, as `ultra_hdr_live`: 7.5° and a crop, the files in
    /// `EYECANSEE_SAMPLES_OUT` to check with libultrahdr.
    #[test]
    #[ignore]
    fn ultra_hdr_edit_live() {
        let dir = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES");
        let out_dir = std::env::var("EYECANSEE_SAMPLES_OUT").expect("EYECANSEE_SAMPLES_OUT");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let b = std::fs::read(&path).unwrap();
            if !is(&b) {
                continue;
            }
            let mut img = crate::decode::decode_full(&b, "x.jpg").unwrap();
            let (w, h) = (img.width() as f32, img.height() as f32);
            let e = Edit { turn: Turn::default(), angle: 7.5, crop: crate::rotate::inside(w, h, 7.5, w / h), fill: Some([255; 3]) };
            e.apply(&mut img);
            let out = edited(&b, img, &[e], &[]).unwrap();
            assert!(is(&out), "{}", path.display());
            std::fs::write(std::path::Path::new(&out_dir).join(path.file_name().unwrap()), &out).unwrap();
        }
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
