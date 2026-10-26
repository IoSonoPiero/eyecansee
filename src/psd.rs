//! A PSD saved over itself after a turn or "Rotate and crop", written by ImageMagick with the colour
//! profile, EXIF and XMP of the original. Always flattened: tried on 12 sample PSDs (notes, PROVE),
//! ImageMagick drops text, effects, smart objects, masks, vectors, fills, groups and notes, and no
//! free program does better. A "simple" PSD (RGB, 8 bit, at most one layer of plain pixels) loses
//! nothing; for the others the confirmation warns that they will be flattened (asked by the user).

use image::DynamicImage;
#[cfg(feature = "magick")]
use image::metadata::Orientation;

/// Extra blocks of a layer that change nothing ImageMagick would lose: unicode name, id, blending
/// switches, colour tag, protection, sheet and metadata settings.
const HARMLESS: [&[u8; 4]; 13] = [b"luni", b"lyid", b"clbl", b"infx", b"knko", b"lspf", b"lclr", b"shmd", b"fxrp", b"iOpa", b"brst", b"tsly", b"cust"];
/// Blocks of the whole file that would be lost: notes, linked files.
const LOST: [&[u8; 4]; 5] = [b"Anno", b"lnk2", b"lnk3", b"lnkD", b"lnkE"];

pub fn is_psd(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("psd"))
}

/// Can a PSD be written at all in this build?
pub fn writable() -> bool {
    cfg!(feature = "magick")
}

/// What a PSD holds, as far as saving it again is concerned.
#[derive(Default)]
struct Info {
    simple: bool,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
}

/// Can this PSD be saved over itself without losing anything but pixels it does not have?
pub fn simple(bytes: &[u8]) -> bool {
    read(bytes).is_some_and(|i| i.simple)
}

/// Big-endian reader over the file; `None` past its end (a broken file is not simple).
struct Cur<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_be_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Option<usize> {
        self.take(4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize)
    }
}

/// Header, image resources, layers and the blocks after them (Adobe's PSD specification).
fn read(bytes: &[u8]) -> Option<Info> {
    let mut c = Cur { b: bytes, i: 0 };
    let head = c.take(26)?;
    // Signature, version 1 (2 is PSB), RGB at 8 bit: what EyeCanSee decodes and writes back.
    let (version, depth, mode) = (u16::from_be_bytes([head[4], head[5]]), u16::from_be_bytes([head[22], head[23]]), u16::from_be_bytes([head[24], head[25]]));
    let mut info = Info::default();
    let n = c.u32()?;
    c.take(n)?;
    let end = c.u32()? + c.i;
    while c.i < end {
        c.take(4)?;
        let id = c.u16()?;
        let name = *c.take(1)?.first()? as usize;
        c.take(name + (name + 1) % 2)?;
        let size = c.u32()?;
        let data = c.take(size)?.to_vec();
        c.take(size % 2)?;
        match id {
            1039 => info.icc = Some(data),
            1058 => info.exif = Some(data),
            1060 => info.xmp = Some(data),
            _ => {}
        }
    }
    let layers_ok = layers_simple(&mut c)?;
    info.simple = &head[..4] == b"8BPS" && version == 1 && depth == 8 && mode == 3 && layers_ok;
    Some(info)
}

/// The layer and mask section: at most one layer, of pixels, without a mask; nothing lost in the
/// blocks after it.
fn layers_simple(c: &mut Cur) -> Option<bool> {
    let section = c.u32()?;
    if section == 0 {
        return Some(true);
    }
    let end = c.i + section;
    let info_len = c.u32()?;
    let info_end = c.i + info_len;
    if info_len > 0 {
        let count = c.u16()? as i16;
        if count.unsigned_abs() > 1 {
            return Some(false);
        }
        if count != 0 && !layer_simple(c)? {
            return Some(false);
        }
    }
    c.i = info_end;
    // Older files end the section here, without the global mask nor blocks.
    if c.i + 4 > end {
        return Some(true);
    }
    let global_mask = c.u32()?;
    c.take(global_mask)?;
    while c.i + 12 <= end {
        let (sig, key, size) = (c.take(4)?, c.take(4)?, c.u32()?);
        if sig != b"8BIM" && sig != b"8B64" {
            break;
        }
        if LOST.iter().any(|k| *k == key) {
            return Some(false);
        }
        c.take(size)?;
    }
    Some(true)
}

/// One layer record: no mask, only harmless extra blocks.
fn layer_simple(c: &mut Cur) -> Option<bool> {
    c.take(16)?;
    let channels = c.u16()? as usize;
    c.take(6 * channels + 12)?;
    let extra = c.u32()?;
    let end = c.i + extra;
    let mask = c.u32()?;
    if mask > 0 {
        return Some(false);
    }
    let ranges = c.u32()?;
    c.take(ranges)?;
    let name = *c.take(1)?.first()? as usize;
    c.take((1 + name).div_ceil(4) * 4 - 1)?;
    while c.i + 12 <= end {
        let (sig, key, size) = (c.take(4)?, c.take(4)?, c.u32()?);
        if (sig != b"8BIM" && sig != b"8B64") || !HARMLESS.iter().any(|k| *k == key) {
            return Some(false);
        }
        c.take(size)?;
    }
    c.i = end;
    Some(true)
}

/// `img` as a PSD, with the colour profile, EXIF (now upright) and XMP of `original`. The result is
/// checked to be a PSD (ImageMagick may write another format without saying so).
#[cfg(feature = "magick")]
pub fn encode(img: &DynamicImage, original: &[u8]) -> Result<Vec<u8>, String> {
    let info = read(original).unwrap_or_default();
    // Opaque stays without an alpha channel, as the original was.
    let img = if img.to_rgba8().pixels().all(|p| p[3] == 255) { DynamicImage::ImageRgb8(img.to_rgb8()) } else { DynamicImage::ImageRgba8(img.to_rgba8()) };
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    crate::decode::magick_init();
    let wand = magick_rust::MagickWand::new();
    wand.read_image_blob(&png).map_err(|e| e.to_string())?;
    if let Some(icc) = &info.icc {
        wand.profile_image("icc", icc.as_slice()).map_err(|e| e.to_string())?;
    }
    // In a PSD ImageMagick writes only the image resources it gets as one "8bim" block (tried:
    // "exif" and "xmp" profiles were dropped), so EXIF and XMP go back as resources 1058 and 1060.
    let mut resources = Vec::new();
    if let Some(mut exif) = info.exif {
        let _ = Orientation::remove_from_exif_chunk(&mut exif);
        push_resource(&mut resources, 1058, &exif);
    }
    if let Some(xmp) = &info.xmp {
        push_resource(&mut resources, 1060, xmp);
    }
    if !resources.is_empty() {
        wand.profile_image("8bim", resources.as_slice()).map_err(|e| e.to_string())?;
    }
    let out = wand.write_image_blob("PSD").map_err(|e| e.to_string())?;
    if !out.starts_with(b"8BPS") {
        return Err("ImageMagick did not write a PSD".into());
    }
    Ok(out)
}

/// One image resource: signature, id, empty name, size, data padded to an even length.
#[cfg(feature = "magick")]
fn push_resource(out: &mut Vec<u8>, id: u16, data: &[u8]) {
    out.extend(b"8BIM");
    out.extend(id.to_be_bytes());
    out.extend([0, 0]);
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(not(feature = "magick"))]
pub fn encode(_: &DynamicImage, _: &[u8]) -> Result<Vec<u8>, String> {
    Err("ImageMagick: not included in this build".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PSD by hand: one layer with the extra blocks `keys`, or `layers` empty layers.
    fn psd(layers: i16, keys: &[&[u8; 4]], mask: bool) -> Vec<u8> {
        let mut layer = vec![0u8; 16];
        layer.extend(3u16.to_be_bytes());
        layer.extend([0u8; 18]);
        layer.extend(b"8BIMnorm");
        layer.extend([255, 0, 0, 0]);
        let mut extra = Vec::new();
        extra.extend(if mask { 20u32 } else { 0 }.to_be_bytes());
        if mask {
            extra.extend([0u8; 20]);
        }
        extra.extend(0u32.to_be_bytes());
        extra.extend([1, b'a', 0, 0]);
        for k in keys {
            extra.extend(b"8BIM");
            extra.extend(*k);
            extra.extend(4u32.to_be_bytes());
            extra.extend([0u8; 4]);
        }
        layer.extend((extra.len() as u32).to_be_bytes());
        layer.extend(extra);
        let mut info = layers.to_be_bytes().to_vec();
        for _ in 0..layers {
            info.extend(&layer);
        }
        let mut section = (info.len() as u32).to_be_bytes().to_vec();
        section.extend(info);
        section.extend(0u32.to_be_bytes());
        let mut out = b"8BPS".to_vec();
        out.extend(1u16.to_be_bytes());
        out.extend([0u8; 6]);
        out.extend(3u16.to_be_bytes());
        out.extend(4u32.to_be_bytes());
        out.extend(4u32.to_be_bytes());
        out.extend(8u16.to_be_bytes());
        out.extend(3u16.to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.extend((section.len() as u32).to_be_bytes());
        out.extend(section);
        out
    }

    #[test]
    fn which_are_simple() {
        assert!(simple(&psd(1, &[b"luni", b"lyid"], false)));
        assert!(simple(&psd(0, &[], false)));
        assert!(!simple(&psd(2, &[], false)), "two layers");
        assert!(!simple(&psd(1, &[b"TySh"], false)), "text");
        assert!(!simple(&psd(1, &[b"lfx2"], false)), "effects");
        assert!(!simple(&psd(1, &[b"SoLd"], false)), "smart object");
        assert!(!simple(&psd(1, &[], true)), "mask");
        assert!(!simple(b"8BPS"), "broken");
        assert!(is_psd("a/B.PSD") && !is_psd("x.psb"));
    }

    /// Written by ImageMagick: a PSD, simple, decoded back with the same size and colours.
    #[cfg(feature = "magick")]
    #[test]
    fn written_back() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(30, 20, |x, _| image::Rgb([(x * 8) as u8, 100, 200])));
        let out = encode(&img, &psd(1, &[], false)).unwrap();
        assert!(simple(&out));
        let back = crate::decode::decode_full(&out, "x.psd").unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (30, 20));
        assert_eq!(back.get_pixel(10, 5), img.to_rgb8().get_pixel(10, 5));
    }

    /// Manual: every PSD in `EYECANSEE_PSD` turned clockwise as "Save" does, written to
    /// `EYECANSEE_PSD_OUT`; prints whether each was flattened.
    #[test]
    #[ignore]
    fn psd_live() {
        let (src, dst) = (std::env::var("EYECANSEE_PSD").unwrap(), std::env::var("EYECANSEE_PSD_OUT").unwrap());
        for e in std::fs::read_dir(src).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let bytes = std::fs::read(&path).unwrap();
            let turn = crate::transform::Turn::default().then(crate::transform::Op::Cw);
            match crate::encode::turned_file(&bytes, &name, turn) {
                Ok((out, flattened)) => {
                    println!("{name}: ok, flattened {flattened}, {} bytes", out.len());
                    std::fs::write(std::path::Path::new(&dst).join(&name), out).unwrap();
                }
                Err(e) => println!("{name}: ERROR {e}"),
            }
        }
    }
}
