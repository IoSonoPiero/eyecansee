//! The metadata besides EXIF and the colour profile that an image keeps when it is written again:
//! XMP (`xmp.rs`) and the Photoshop block with the IPTC, read from every format EyeCanSee writes
//! back (and from HEIC and PSD) and put in the containers of the new file: JPEG segments, PNG
//! chunks, TIFF tags. Also the EXIF of a TIFF, which needs directories of its own.

use std::io::Cursor;

/// Photoshop resources that describe the original's pixels: its thumbnails (1033, 1036), grid and
/// guides (1032), slices (1050). Left out of the file written; the rest (IPTC, copyright flag,
/// URL, resolution…) stays.
const PIXEL_RESOURCES: [u16; 4] = [1032, 1033, 1036, 1050];
/// IPTC-IIM inside a Photoshop block.
const IPTC: u16 = 1028;
/// MD5 of the IPTC-IIM, kept by Photoshop to know whether the XMP still matches it.
const IPTC_DIGEST: u16 = 1061;
pub const JPEG_XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
pub const JPEG_PHOTOSHOP: &[u8] = b"Photoshop 3.0\0";
/// Name of the segments of a JPEG's extended XMP (XMP specification, part 3): the part of an XMP
/// too large for one segment, named in the main packet by its GUID (`xmpNote:HasExtendedXMP`).
pub const JPEG_XMP_EXT: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";

/// The resources of a Photoshop block: id and the whole resource (header and padding included).
fn resources(irb: &[u8]) -> Vec<(u16, &[u8])> {
    let mut v = Vec::new();
    let mut i = 0;
    while let Some(head) = irb.get(i..i + 7) {
        // "8BIM", or older signatures of the same layout.
        let id = u16::from_be_bytes([head[4], head[5]]);
        let name = head[6] as usize;
        let at = i + 6 + (1 + name).next_multiple_of(2);
        let Some(size) = irb.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize) else { break };
        let end = (at + 4).saturating_add(size.next_multiple_of(2)).min(irb.len());
        if at + 4 + size > irb.len() {
            break;
        }
        v.push((id, &irb[i..end]));
        i = end;
    }
    v
}

/// The Photoshop block without the resources about the original's pixels; `None` if nothing else.
pub fn fit_irb(irb: &[u8]) -> Option<Vec<u8>> {
    let kept: Vec<u8> = resources(irb).into_iter().filter(|(id, _)| !PIXEL_RESOURCES.contains(id)).flat_map(|(_, r)| r.to_vec()).collect();
    (!kept.is_empty()).then_some(kept)
}

/// The IPTC-IIM of a Photoshop block.
pub fn iim(irb: &[u8]) -> Option<&[u8]> {
    resources(irb).into_iter().find(|(id, _)| *id == IPTC).and_then(|(_, r)| data(r))
}

/// The data of one resource (`resources`).
fn data(r: &[u8]) -> Option<&[u8]> {
    let at = 6 + (1 + r[6] as usize).next_multiple_of(2);
    let size = u32::from_be_bytes(r[at..at + 4].try_into().ok()?) as usize;
    r.get(at + 4..at + 4 + size)
}

/// The IPTC-IIM for a TIFF and its Photoshop block, as Photoshop writes them: the IIM padded with
/// zeros to a multiple of 4 bytes (the tag is a list of LONGs), the same in the block, and the
/// digest made that of the padded IIM, if it was that of the IIM (a digest already out of date
/// stays, it says the XMP may not match). `None` without IPTC.
pub fn tiff_iptc(irb: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let old = iim(irb)?;
    let padded = [old, &[0; 3][..(4 - old.len() % 4) % 4]].concat();
    let current = md5::compute(old).0;
    let block = resources(irb)
        .into_iter()
        .flat_map(|(id, r)| match id {
            IPTC => resource(IPTC, &padded),
            IPTC_DIGEST if data(r) == Some(&current[..]) => resource(IPTC_DIGEST, &md5::compute(&padded).0),
            _ => r.to_vec(),
        })
        .collect();
    Some((padded, block))
}

/// A Photoshop block holding only `iim`.
pub fn irb_of_iim(iim: &[u8]) -> Vec<u8> {
    resource(IPTC, iim)
}

/// One resource: signature, id, empty name, size, data padded to an even length.
pub fn resource(id: u16, data: &[u8]) -> Vec<u8> {
    let mut out = b"8BIM".to_vec();
    out.extend(id.to_be_bytes());
    out.extend([0, 0]);
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// IPTC as found: a Photoshop block (JPEG, the PNG "Raw profile" of exiftool) or IPTC-IIM alone
/// (TIFF, ImageMagick), always given back as a Photoshop block.
pub fn as_irb(data: Vec<u8>) -> Option<Vec<u8>> {
    let data = data.strip_prefix(JPEG_PHOTOSHOP).map(<[u8]>::to_vec).unwrap_or(data);
    match data.first()? {
        b'8' if data.starts_with(b"8BIM") => Some(data),
        0x1C => Some(irb_of_iim(&data)),
        _ => None,
    }
}

/// The text of a PNG "Raw profile type …" chunk (ImageMagick's convention, exiftool's too): a
/// line with the name, one with the length, then the bytes in hexadecimal.
pub fn from_raw_profile(text: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(text).ok()?;
    let mut lines = text.trim_start_matches('\n').splitn(3, '\n');
    let (_, len) = (lines.next()?, lines.next()?.trim().parse::<usize>().ok()?);
    let hex: Vec<u8> = lines.next()?.bytes().filter(u8::is_ascii_hexdigit).collect();
    let bytes: Vec<u8> = hex.as_chunks::<2>().0.iter().filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok()).collect();
    (bytes.len() >= len).then(|| bytes[..len].to_vec())
}

fn raw_profile(name: &str, data: &[u8]) -> String {
    let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
    let lines: Vec<&str> = hex.as_bytes().chunks(72).map(|l| std::str::from_utf8(l).unwrap_or_default()).collect();
    format!("\n{name}\n{:8}\n{}\n", data.len(), lines.join("\n"))
}

/// `png` with the XMP (iTXt "XML:com.adobe.xmp", uncompressed as the XMP specification asks) and
/// the Photoshop block with the IPTC (zTXt "Raw profile type iptc") before its image data.
pub fn png_chunks(png: Vec<u8>, xmp: Option<&[u8]>, irb: Option<&[u8]>) -> Vec<u8> {
    let Some(at) = png.windows(4).position(|w| w == b"IDAT").map(|p| p - 4) else { return png };
    let mut extra = Vec::new();
    if let Some(xmp) = xmp {
        chunk(&mut extra, b"iTXt", &[b"XML:com.adobe.xmp\0\0\0\0\0", xmp].concat());
    }
    if let Some(irb) = irb {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        let compressed = std::io::Write::write_all(&mut z, raw_profile("IPTC profile", irb).as_bytes()).and_then(|()| z.finish());
        if let Ok(z) = compressed {
            chunk(&mut extra, b"zTXt", &[b"Raw profile type iptc\0\0", z.as_slice()].concat());
        }
    }
    let mut out = png;
    out.splice(at..at, extra);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(kind);
    out.extend(data);
    out.extend(crc.sum().to_be_bytes());
}

/// A JPEG whose segments were copied from its original (lossless turn): XMP made to fit `w` × `h`
/// and the Photoshop block without the original's thumbnails.
pub fn fit_jpeg_segments(jpeg: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = jpeg[..2.min(jpeg.len())].to_vec();
    let mut rest = 2;
    for (marker, s, e) in crate::ultrahdr::segments(jpeg) {
        let body = &jpeg[s + 4..e];
        let new = if marker == 0xE1 && body.starts_with(JPEG_XMP) {
            crate::xmp::fit(Some(&body[JPEG_XMP.len()..]), w, h, None).map(|x| [JPEG_XMP, &x].concat())
        } else if marker == 0xED && body.starts_with(JPEG_PHOTOSHOP) {
            fit_irb(&body[JPEG_PHOTOSHOP.len()..]).map(|x| [JPEG_PHOTOSHOP, &x].concat())
        } else {
            Some(body.to_vec())
        };
        if let Some(new) = new.filter(|n| n.len() <= 65533) {
            out.extend([0xFF, marker]);
            out.extend(((new.len() + 2) as u16).to_be_bytes());
            out.extend(new);
        }
        rest = e;
    }
    out.extend_from_slice(&jpeg[rest..]);
    out
}

/// The extended XMP of a JPEG whose main packet is `xmp`: the segments after their name (GUID,
/// full length, offset, piece), only those of the GUID the main packet names.
pub fn jpeg_xmp_ext(jpeg: &[u8], xmp: &[u8]) -> Vec<Vec<u8>> {
    let Some(guid) = ext_guid(xmp).filter(|_| jpeg.starts_with(&[0xFF, 0xD8])) else { return Vec::new() };
    crate::ultrahdr::segments(jpeg)
        .into_iter()
        .filter(|&(marker, ..)| marker == 0xE1)
        .filter_map(|(_, s, e)| jpeg[s + 4..e].strip_prefix(JPEG_XMP_EXT))
        .filter(|body| body.len() > 40 && body.starts_with(guid))
        .map(<[u8]>::to_vec)
        .collect()
}

/// The GUID an XMP packet gives for its extended part: 32 hexadecimal digits.
fn ext_guid(xmp: &[u8]) -> Option<&[u8]> {
    let at = xmp.windows(14).position(|w| w == b"HasExtendedXMP")? + 14;
    let start = at + xmp[at..].iter().take(4).position(u8::is_ascii_hexdigit)?;
    xmp.get(start..start + 32).filter(|g| g.iter().all(u8::is_ascii_hexdigit))
}

/// The extended XMP put back together from its segments (`jpeg_xmp_ext`), each piece at its
/// offset; `None` without segments or when they do not make up the full length they declare.
pub fn xmp_ext_packet(segments: &[Vec<u8>]) -> Option<Vec<u8>> {
    let full = u32::from_be_bytes(segments.first()?.get(32..36)?.try_into().ok()?) as usize;
    // At most the pieces there are: a wrong length in a file cannot ask for more memory.
    if full != segments.iter().map(|s| s.len() - 40).sum::<usize>() {
        return None;
    }
    let mut out = vec![0; full];
    for s in segments {
        let offset = u32::from_be_bytes(s[36..40].try_into().ok()?) as usize;
        out.get_mut(offset..offset.checked_add(s.len() - 40)?)?.copy_from_slice(&s[40..]);
    }
    Some(out)
}

/// XMP, Photoshop block (with the IPTC) and colour profile of a file, from what the decoder of
/// `image` gives and, where it gives nothing, from the file itself (TIFF tags, HEIC/AVIF items,
/// PSD resources).
#[derive(Default)]
pub struct Extra {
    pub xmp: Option<Vec<u8>>,
    pub irb: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    /// EXIF, only where `image` cannot find it (PSD).
    pub exif: Option<Vec<u8>>,
}

pub fn read(bytes: &[u8], d: Option<&mut dyn image::ImageDecoder>) -> Extra {
    let mut x = Extra::default();
    if let Some(d) = d {
        x.xmp = d.xmp_metadata().ok().flatten();
        // JPEG: the Photoshop block; PNG: the text of the raw profile.
        x.irb = d.iptc_metadata().ok().flatten().and_then(|i| as_irb(from_raw_profile(&i).unwrap_or(i)));
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        // `image` refuses the XMP of small TIFFs (its memory limit follows the image size).
        if x.xmp.is_none() {
            x.xmp = tiff::decoder::Decoder::new(Cursor::new(bytes)).ok().and_then(|mut d| d.get_tag_u8_vec(tiff::tags::Tag::Unknown(700)).ok());
        }
        x.irb = x.irb.or_else(|| tiff_irb(bytes));
    }
    if let Some(p) = crate::psd::metadata(bytes) {
        (x.xmp, x.irb, x.icc, x.exif) = (p.xmp, p.irb, p.icc, p.exif);
    }
    if x.icc.is_none() {
        x.icc = xcf_icc(bytes);
    }
    #[cfg(feature = "heif")]
    if x.xmp.is_none() && bytes.get(4..8) == Some(b"ftyp") {
        (x.xmp, x.icc) = heif(bytes);
    }
    x
}

/// The colour profile of a GIMP file (XCF), kept by GIMP as the "icc-profile" parasite among the
/// image's properties; only for an RGB image (ImageMagick hands its pixels over unconverted). Its
/// EXIF, XMP and IPTC are left out: GIMP keeps them as text of its own, tag by tag.
fn xcf_icc(bytes: &[u8]) -> Option<Vec<u8>> {
    let u32_at = |i: usize| Some(u32::from_be_bytes(bytes.get(i..i + 4)?.try_into().ok()?) as usize);
    // "gimp xcf " and the version ("file" or "v011"), NUL; width, height, base type (0 = RGB);
    // from version 4 the precision.
    let version = bytes.strip_prefix(b"gimp xcf ")?.get(..4)?;
    let mut i = 14 + 8;
    if u32_at(i)? != 0 {
        return None;
    }
    i += 4;
    if version != b"file" && std::str::from_utf8(&version[1..]).ok()?.parse::<u32>().ok()? >= 4 {
        i += 4;
    }
    // Properties (type, length, data) up to PROP_END (0); PROP_PARASITES is 21: each parasite a
    // name (length with its NUL, text), flags, size and data.
    loop {
        let (kind, len) = (u32_at(i)?, u32_at(i + 4)?);
        let data = bytes.get(i + 8..i + 8 + len)?;
        match kind {
            0 => return None,
            21 => {
                let mut j = 0;
                while j + 4 <= data.len() {
                    let n = u32::from_be_bytes(data.get(j..j + 4)?.try_into().ok()?) as usize;
                    let name = data.get(j + 4..j + 4 + n)?;
                    let size = u32::from_be_bytes(data.get(j + 8 + n..j + 12 + n)?.try_into().ok()?) as usize;
                    let value = data.get(j + 12 + n..j + 12 + n + size)?;
                    if name == b"icc-profile\0" {
                        return value.get(16..20).filter(|&c| c == b"RGB ").map(|_| value.to_vec());
                    }
                    j += 12 + n + size;
                }
            }
            _ => {}
        }
        i += 8 + len;
    }
}

/// Photoshop block of a TIFF (34377) with its IPTC (33723), which is often only in the tag.
fn tiff_irb(bytes: &[u8]) -> Option<Vec<u8>> {
    use tiff::tags::Tag;
    let mut d = tiff::decoder::Decoder::new(Cursor::new(bytes)).ok()?;
    let irb = d.get_tag_u8_vec(Tag::Unknown(34377)).ok();
    match (irb, tiff_iim(&mut d, bytes)) {
        (Some(irb), Some(iim)) if self::iim(&irb).is_none() => Some([irb, iim].concat()),
        (Some(irb), _) => Some(irb),
        (None, iim) => iim,
    }
}

/// The IPTC tag of a TIFF, as a Photoshop block.
fn tiff_iim(d: &mut tiff::decoder::Decoder<Cursor<&[u8]>>, bytes: &[u8]) -> Option<Vec<u8>> {
    use tiff::tags::Tag;
    // IPTC is often written as LONGs: their bytes, in the file's order.
    let iim = match d.get_tag(Tag::Unknown(33723)).ok()? {
        tiff::decoder::ifd::Value::List(v) => v
            .into_iter()
            .flat_map(|v| match v {
                tiff::decoder::ifd::Value::Unsigned(n) if bytes.starts_with(b"II") => n.to_le_bytes().to_vec(),
                tiff::decoder::ifd::Value::Unsigned(n) => n.to_be_bytes().to_vec(),
                tiff::decoder::ifd::Value::Byte(b) => vec![b],
                _ => Vec::new(),
            })
            .collect(),
        _ => return None,
    };
    as_irb(iim)
}

/// XMP and colour profile of a HEIC or AVIF image.
#[cfg(feature = "heif")]
fn heif(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let Some(h) = libheif_rs::HeifContext::read_from_bytes(bytes).ok().and_then(|c| c.primary_image_handle().ok()) else { return (None, None) };
    let mut ids = [0; 8];
    let n = h.metadata_block_ids(&mut ids, b"mime");
    let xmp = ids[..n].iter().find(|&&id| h.metadata_content_type(id) == Some("application/rdf+xml")).and_then(|&id| h.metadata(id).ok());
    (xmp, h.color_profile_raw().map(|p| p.data))
}

/// Which directory of a TIFF an EXIF field goes in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Main,
    Exif,
    Gps,
}

/// The fields of an EXIF block (as `meta::exif_chunk` writes it) for a TIFF written by the `tiff`
/// crate: directory, tag, type and value in this machine's byte order. Left out: the tags of the
/// image data (the encoder writes its own), the directory links (written again) and the maker note,
/// whose offsets point into the original file.
pub fn tiff_fields(exif: &[u8]) -> Vec<(Dir, u16, tiff::tags::Type, Vec<u8>)> {
    use exif::{Context, Tag, Value};
    use tiff::tags::Type;
    const MAIN: [Tag; 11] = [Tag::Make, Tag::Model, Tag::Software, Tag::DateTime, Tag::Artist, Tag::Copyright, Tag::ImageDescription, Tag::Orientation, Tag::XResolution, Tag::YResolution, Tag::ResolutionUnit];
    // Directories with a "next" link where none is expected (seen in a Lightroom JPEG) are read
    // all the same.
    let mut r = exif::Reader::new();
    r.continue_on_error(true);
    let Ok(e) = r.read_raw(exif.to_vec()).or_else(|e| e.distill_partial_result(|_| {})) else { return Vec::new() };
    e.fields()
        .filter(|f| f.ifd_num == exif::In::PRIMARY)
        .filter_map(|f| {
            let dir = match f.tag.context() {
                Context::Tiff if MAIN.contains(&f.tag) => Dir::Main,
                Context::Exif if ![Tag::MakerNote, Tag::InteropIFDPointer].contains(&f.tag) => Dir::Exif,
                Context::Gps => Dir::Gps,
                _ => return None,
            };
            let (ty, b): (Type, Vec<u8>) = match &f.value {
                Value::Byte(v) => (Type::BYTE, v.clone()),
                Value::Ascii(v) => (Type::ASCII, v.iter().flat_map(|s| s.iter().copied().chain([0])).collect()),
                Value::Short(v) => (Type::SHORT, v.iter().flat_map(|x| x.to_ne_bytes()).collect()),
                Value::Long(v) => (Type::LONG, v.iter().flat_map(|x| x.to_ne_bytes()).collect()),
                Value::Rational(v) => (Type::RATIONAL, v.iter().flat_map(|x| [x.num, x.denom]).flat_map(u32::to_ne_bytes).collect()),
                Value::SByte(v) => (Type::SBYTE, v.iter().map(|&x| x as u8).collect()),
                Value::Undefined(v, _) => (Type::UNDEFINED, v.clone()),
                Value::SShort(v) => (Type::SSHORT, v.iter().flat_map(|x| x.to_ne_bytes()).collect()),
                Value::SLong(v) => (Type::SLONG, v.iter().flat_map(|x| x.to_ne_bytes()).collect()),
                Value::SRational(v) => (Type::SRATIONAL, v.iter().flat_map(|x| [x.num, x.denom]).flat_map(i32::to_ne_bytes).collect()),
                Value::Float(v) => (Type::FLOAT, v.iter().flat_map(|x| x.to_ne_bytes()).collect()),
                Value::Double(v) => (Type::DOUBLE, v.iter().flat_map(|x| x.to_ne_bytes()).collect()),
                Value::Unknown(..) => return None,
            };
            (!b.is_empty()).then_some((dir, f.tag.number(), ty, b))
        })
        .collect()
}

#[cfg(test)]
mod tests {

    /// In a TIFF the IPTC is padded to 4 bytes, in the Photoshop block too, and a current digest
    /// stays current; one already out of date is left as it was.
    #[test]
    fn tiff_iptc_padded() {
        use super::{IPTC, IPTC_DIGEST, iim, resource, resources, tiff_iptc};
        let old = [0x1C, 2, 80, 0, 4, b'A', b'n', b'n', b'a'];
        let digest = |irb: &[u8]| resources(irb).into_iter().find(|(id, _)| *id == IPTC_DIGEST).and_then(|(_, r)| super::data(r).map(<[u8]>::to_vec));
        let (padded, block) = tiff_iptc(&[resource(IPTC, &old), resource(IPTC_DIGEST, &md5::compute(old).0)].concat()).unwrap();
        assert_eq!(padded, [&old[..], &[0, 0, 0]].concat());
        assert_eq!(iim(&block), Some(&padded[..]));
        assert_eq!(digest(&block), Some(md5::compute(&padded).0.to_vec()));
        let (_, block) = tiff_iptc(&[resource(IPTC, &old), resource(IPTC_DIGEST, &[9; 16])].concat()).unwrap();
        assert_eq!(digest(&block), Some(vec![9; 16]));
        assert!(tiff_iptc(&resource(1036, &[1; 4])).is_none());
    }

    /// An XCF by hand: header, the parasites property with an "icc-profile", PROP_END.
    #[test]
    fn xcf_profile() {
        let xcf = |base: u32, icc: &[u8]| {
            let mut p = Vec::new();
            for (name, data) in [(&b"gimp-comment\0"[..], &b"hi"[..]), (b"icc-profile\0", icc)] {
                p.extend((name.len() as u32).to_be_bytes());
                p.extend(name);
                p.extend(1u32.to_be_bytes());
                p.extend((data.len() as u32).to_be_bytes());
                p.extend(data);
            }
            let mut f = b"gimp xcf v011\0".to_vec();
            for v in [64u32, 48, base, 150] {
                f.extend(v.to_be_bytes());
            }
            f.extend(21u32.to_be_bytes());
            f.extend((p.len() as u32).to_be_bytes());
            f.extend(p);
            f.extend([0; 8]);
            f
        };
        let rgb = [&[0u8; 16][..], b"RGB ", &[0; 20]].concat();
        assert_eq!(super::xcf_icc(&xcf(0, &rgb)), Some(rgb.clone()));
        // A grey image, or a profile not for RGB: the pixels go out as RGB, so none.
        assert_eq!(super::xcf_icc(&xcf(1, &rgb)), None);
        assert_eq!(super::xcf_icc(&xcf(0, &[&[0u8; 16][..], b"GRAY", &[0; 20]].concat())), None);
        assert_eq!(super::xcf_icc(b"not a GIMP file"), None);
    }

    use super::*;

    /// Thumbnails and slices out, the rest of the Photoshop block (IPTC and its digest) as it was;
    /// IPTC found again from the block, and from a raw profile as exiftool writes it in PNG.
    #[test]
    fn photoshop_block() {
        let iim = crate::xmp::tests::iim(&[(80, "Ann")]);
        let irb = [resource(1033, &[1, 2, 3]), resource(IPTC, &iim), resource(1050, b"slices"), resource(1061, &[9; 16]), resource(1036, &[4; 5])].concat();
        let fitted = fit_irb(&irb).unwrap();
        assert_eq!(fitted, [resource(IPTC, &iim), resource(1061, &[9; 16])].concat());
        assert_eq!(super::iim(&fitted), Some(iim.as_slice()));
        assert_eq!(fit_irb(&resource(1036, &[1])), None);
        assert_eq!(as_irb(iim.clone()), Some(irb_of_iim(&iim)));
        assert_eq!(as_irb([JPEG_PHOTOSHOP, &fitted].concat()), Some(fitted.clone()));
        let text = raw_profile("IPTC profile", &fitted);
        assert!(text.starts_with("\nIPTC profile\n      "));
        assert_eq!(from_raw_profile(text.as_bytes()), Some(fitted));
    }

    /// A JPEG turned without loss keeps XMP and IPTC, made to fit: sizes, no Camera Raw, no thumbnail.
    #[test]
    fn lossless_turn() {
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(32, 16)).write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        let iim = crate::xmp::tests::iim(&[(80, "Ann")]);
        let irb = [resource(1036, &[1; 9]), irb_of_iim(&iim)].concat();
        let seg = |m: u8, parts: &[&[u8]]| [&[0xFF, m][..], &((parts.iter().map(|p| p.len()).sum::<usize>() + 2) as u16).to_be_bytes(), &parts.concat()].concat();
        let file = [&jpeg[..2], &seg(0xE1, &[JPEG_XMP, crate::xmp::tests::SAMPLE.as_bytes()]), &seg(0xED, &[JPEG_PHOTOSHOP, &irb]), &jpeg[2..]].concat();
        let (out, lossy) = crate::save_back::turned_file(&file, "a.jpg", crate::transform::Turn::default().then(crate::transform::Op::Cw)).unwrap();
        assert!(!lossy);
        crate::encode::tests::kept_xmp_like(&out, 16, 32);
        let x = read(&out, Some(&mut image::codecs::jpeg::JpegDecoder::new(Cursor::new(&out)).unwrap()));
        assert_eq!(x.irb, Some(irb_of_iim(&iim)));
    }

    /// XMP and IPTC put in a PNG are read back by `image`, the PNG still decodes.
    #[test]
    fn png_with_xmp_and_iptc() {
        use image::ImageDecoder;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 3)).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let irb = irb_of_iim(&crate::xmp::tests::iim(&[(116, "© Ann")]));
        let out = png_chunks(png, Some(b"<x:xmpmeta/>"), Some(&irb));
        let mut d = image::codecs::png::PngDecoder::new(Cursor::new(&out)).unwrap();
        assert_eq!(d.xmp_metadata().unwrap().unwrap(), b"<x:xmpmeta/>");
        let x = read(&out, Some(&mut d));
        assert_eq!(x.irb, Some(irb));
        assert_eq!(crate::decode::decode_full(&out, "x.png").unwrap().width(), 4);
    }

    /// Manual, `cargo test --release meta_live -- --ignored --nocapture`: every image in
    /// `EYECANSEE_META` through each save path, written to `EYECANSEE_META_OUT` as
    /// `<name>.<path>.<ext>`: crop (Rotate and crop), turn (Save), and conversion at half size to
    /// JPEG, PNG, WebP, TIFF and AVIF (Convert and resize, Save in another format). The tags lost
    /// are counted with exiftool (notes, DIARIO 0.73.1).
    #[test]
    #[ignore]
    fn meta_live() {
        use crate::encode::{Format, Options, Original};
        let (src, dst) = (std::env::var("EYECANSEE_META").unwrap(), std::env::var("EYECANSEE_META_OUT").unwrap());
        let dst = std::path::Path::new(&dst);
        for e in std::fs::read_dir(src).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let ext = name.rsplit_once('.').unwrap().1.to_string();
            let bytes = std::fs::read(&path).unwrap();
            let out = |what: &str, ext: &str, r: Result<Vec<u8>, String>| match r {
                Ok(b) => std::fs::write(dst.join(format!("{name}.{what}.{ext}")), b).unwrap(),
                Err(e) => println!("{name} {what}: ERROR {e}"),
            };
            let img = crate::decode::decode_full(&bytes, &name).unwrap();
            let (w, h) = (img.width(), img.height());
            out("crop", &ext, crate::save_back::encoded_back(&bytes, &name, img.crop_imm(w / 10, h / 10, w * 7 / 10, h * 7 / 10)).map(|r| r.0));
            out("turn", &ext, crate::save_back::turned_file(&bytes, &name, crate::transform::Turn::default().then(crate::transform::Op::Cw)).map(|r| r.0));
            let half = Original::with_meta(img.resize(w / 2, h / 2, image::imageops::FilterType::Triangle), &bytes);
            for f in [Format::Jpeg, Format::Png, Format::Webp, Format::Tiff, Format::Avif] {
                out("to", f.ext(), half.encode(&Options { format: f, ..Default::default() }));
            }
        }
    }
}
