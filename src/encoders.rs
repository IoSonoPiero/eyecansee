//! The encoders that need more than `image` gives: libjpeg-turbo for JPEG, libwebp for WebP, ravif
//! for AVIF, `tiff` for TIFF, with the metadata written into their containers. Split from
//! `encode.rs` on 2026-11-04.

use std::io::Cursor;

use image::DynamicImage;

use crate::encode::{Blocks, Chroma, Options, TiffCompression};
use crate::meta_write::{Dir, JPEG_PHOTOSHOP, JPEG_XMP, JPEG_XMP_EXT};
use tiff::tags::Type;

/// libjpeg-turbo; the metadata go in as APP segments, which it does not write.
pub(crate) fn jpeg(img: &DynamicImage, o: &Options, b: &Blocks) -> Result<Vec<u8>, String> {
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let tj = |e: turbojpeg::Error| e.to_string();
    let mut c = turbojpeg::Compressor::new().map_err(tj)?;
    c.set_quality(o.jpeg_quality as i32).map_err(tj)?;
    let chroma = match o.chroma {
        // Lossless JPEG keeps every colour sample.
        _ if o.jpeg_lossless => turbojpeg::Subsamp::None,
        Chroma::Full => turbojpeg::Subsamp::None,
        Chroma::Half => turbojpeg::Subsamp::Sub2x1,
        Chroma::Quarter => turbojpeg::Subsamp::Sub2x2,
        Chroma::Grey => turbojpeg::Subsamp::Gray,
    };
    c.set_subsamp(chroma).map_err(tj)?;
    c.set_progressive(o.progressive).map_err(tj)?;
    c.set_optimize(o.optimize).map_err(tj)?;
    c.set_arithmetic(o.arithmetic).map_err(tj)?;
    c.set_lossless(o.jpeg_lossless).map_err(tj)?;
    let image = turbojpeg::Image { pixels: rgb.as_raw().as_slice(), width: w, pitch: w * 3, height: h, format: turbojpeg::PixelFormat::RGB };
    let file = c.compress_to_vec(image).map_err(tj)?;
    Ok(jpeg_meta(file, b))
}

/// After SOI (and the JFIF APP0, which must come first): APP1 "Exif", APP1 XMP and those of its
/// extended part, the profile in
/// APP2 "ICC_PROFILE" pieces of at most 65519 bytes, APP13 Photoshop with the IPTC. EXIF, XMP or
/// Photoshop block too big for one segment is left out.
fn jpeg_meta(file: Vec<u8>, b: &Blocks) -> Vec<u8> {
    if !file.starts_with(&[0xFF, 0xD8]) {
        return file;
    }
    let mut at = 2;
    if file.get(2..4) == Some(&[0xFF, 0xE0]) {
        at = 4 + u16::from_be_bytes([file[4], file[5]]) as usize;
    }
    let mut out = file[..at].to_vec();
    let mut segment = |marker: u8, parts: &[&[u8]]| {
        let len: usize = 2 + parts.iter().map(|p| p.len()).sum::<usize>();
        out.extend([0xFF, marker]);
        out.extend((len as u16).to_be_bytes());
        for p in parts {
            out.extend_from_slice(p);
        }
    };
    if let Some(exif) = b.exif.as_deref().filter(|e| e.len() <= 65533 - 6) {
        segment(0xE1, &[b"Exif\0\0", exif]);
    }
    if let Some(xmp) = b.xmp.as_deref().filter(|x| x.len() <= 65533 - JPEG_XMP.len()) {
        segment(0xE1, &[JPEG_XMP, xmp]);
        for piece in &b.xmp_ext {
            segment(0xE1, &[JPEG_XMP_EXT, piece]);
        }
    }
    if let Some(icc) = &b.icc {
        let pieces: Vec<&[u8]> = icc.chunks(65519).collect();
        for (i, piece) in pieces.iter().enumerate() {
            segment(0xE2, &[b"ICC_PROFILE\0", &[i as u8 + 1, pieces.len() as u8], piece]);
        }
    }
    if let Some(irb) = b.irb.as_deref().filter(|x| x.len() <= 65533 - JPEG_PHOTOSHOP.len()) {
        segment(0xED, &[JPEG_PHOTOSHOP, irb]);
    }
    out.extend_from_slice(&file[at..]);
    out
}

/// libwebp, lossy or lossless; metadata added by rewriting the container (`webp_meta`).
pub(crate) fn webp(img: &DynamicImage, o: &Options, b: &Blocks) -> Result<Vec<u8>, String> {
    let alpha = img.color().has_alpha();
    let (w, h) = (img.width(), img.height());
    let raw = if alpha { img.to_rgba8().into_raw() } else { img.to_rgb8().into_raw() };
    let encoder = if alpha { webp::Encoder::from_rgba(&raw, w, h) } else { webp::Encoder::from_rgb(&raw, w, h) };
    let mut config = webp::WebPConfig::new().map_err(|()| "WebPConfig".to_string())?;
    config.lossless = o.webp_lossless as i32;
    config.method = o.webp_effort as i32;
    if o.webp_lossless {
        // In lossless mode "quality" is how hard it tries to compress.
        config.quality = o.webp_effort as f32 * 100.0 / 6.0;
        config.near_lossless = o.near_lossless as i32;
        config.exact = 1;
    } else {
        config.quality = o.webp_quality as f32;
        config.use_sharp_yuv = o.sharp_yuv as i32;
    }
    let file = encoder.encode_advanced(&config).map_err(|e| format!("{e:?}"))?;
    Ok(webp_meta(&file, w, h, alpha, b))
}

/// The libwebp file inside an extended container (VP8X) with ICCP, EXIF and XMP chunks.
fn webp_meta(file: &[u8], w: u32, h: u32, alpha: bool, b: &Blocks) -> Vec<u8> {
    let (exif, icc, xmp) = (b.exif.as_deref(), b.icc.as_deref(), b.xmp.as_deref());
    if exif.is_none() && icc.is_none() && xmp.is_none() || file.len() < 12 {
        return file.to_vec();
    }
    let mut chunks = Vec::new();
    let mut i = 12;
    while i + 8 <= file.len() {
        let id = &file[i..i + 4];
        let len = u32::from_le_bytes([file[i + 4], file[i + 5], file[i + 6], file[i + 7]]) as usize;
        let end = (i + 8 + len).min(file.len());
        if id != b"VP8X" {
            chunks.push((id, &file[i + 8..end]));
        }
        i += 8 + len + (len & 1);
    }
    let push = |out: &mut Vec<u8>, id: &[u8], data: &[u8]| {
        out.extend_from_slice(id);
        out.extend((data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
    };
    let alpha = alpha || chunks.iter().any(|(id, _)| *id == b"ALPH");
    let flags = (if icc.is_some() { 0x20 } else { 0 }) | (if alpha { 0x10 } else { 0 }) | (if exif.is_some() { 0x08 } else { 0 }) | (if xmp.is_some() { 0x04 } else { 0 });
    let mut vp8x = vec![flags, 0, 0, 0];
    vp8x.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
    vp8x.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
    let mut body = b"WEBP".to_vec();
    push(&mut body, b"VP8X", &vp8x);
    if let Some(icc) = icc {
        push(&mut body, b"ICCP", icc);
    }
    for (id, data) in chunks {
        push(&mut body, id, data);
    }
    if let Some(exif) = exif {
        push(&mut body, b"EXIF", exif);
    }
    if let Some(xmp) = xmp {
        push(&mut body, b"XMP ", xmp);
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

/// ravif (rav1e), with EXIF, colour profile and XMP (the last two through the copies of ravif and
/// avif-serialize in vendor/).
pub(crate) fn avif(img: &DynamicImage, o: &Options, b: &Blocks) -> Result<Vec<u8>, String> {
    let mut e = ravif::Encoder::new()
        .with_quality(o.avif_quality as f32)
        .with_alpha_quality(o.avif_alpha_quality as f32)
        .with_speed(o.avif_speed)
        .with_bit_depth(if o.avif_ten_bit { ravif::BitDepth::Ten } else { ravif::BitDepth::Eight })
        .with_internal_color_model(if o.avif_rgb { ravif::ColorModel::RGB } else { ravif::ColorModel::YCbCr });
    if let Some(exif) = &b.exif {
        e = e.with_exif(exif.clone());
    }
    if let Some(icc) = &b.icc {
        e = e.with_icc_profile(icc.clone());
    }
    if let Some(xmp) = &b.xmp {
        e = e.with_xmp(xmp.clone());
    }
    let (w, h) = (img.width() as usize, img.height() as usize);
    let r = if img.color().has_alpha() {
        let px: Vec<ravif::RGBA8> = img.to_rgba8().pixels().map(|p| ravif::RGBA8::new(p[0], p[1], p[2], p[3])).collect();
        e.encode_rgba(ravif::Img::new(&px[..], w, h))
    } else {
        let px: Vec<ravif::RGB8> = img.to_rgb8().pixels().map(|p| ravif::RGB8::new(p[0], p[1], p[2])).collect();
        e.encode_rgb(ravif::Img::new(&px[..], w, h))
    };
    r.map(|r| r.avif_file).map_err(|e| e.to_string())
}

/// `tiff` with the chosen compression; the colour profile as tag 34675, XMP as 700, IPTC as
/// 33723 and the Photoshop block as 34377, EXIF in the main directory and in Exif and GPS
/// directories of their own (written first, the image links them).
pub(crate) fn tiff(img: &DynamicImage, o: &Options, b: &Blocks, out: &mut Cursor<Vec<u8>>) -> tiff::TiffResult<()> {
    use tiff::encoder::{Compression, DeflateLevel, Predictor, TiffEncoder, colortype};
    let compression = match o.tiff {
        TiffCompression::None => Compression::Uncompressed,
        TiffCompression::Lzw => Compression::Lzw,
        TiffCompression::DeflateFast => Compression::Deflate(DeflateLevel::Fast),
        TiffCompression::Deflate => Compression::Deflate(DeflateLevel::Balanced),
        TiffCompression::DeflateBest => Compression::Deflate(DeflateLevel::Best),
        TiffCompression::PackBits => Compression::Packbits,
    };
    let compressed = !matches!(o.tiff, TiffCompression::None | TiffCompression::PackBits);
    let predictor = if o.tiff_predictor && compressed { Predictor::Horizontal } else { Predictor::None };
    let mut enc = TiffEncoder::new(out)?.with_compression(compression).with_predictor(predictor);
    let (w, h) = (img.width(), img.height());
    let fields = b.exif.as_deref().map(crate::meta_write::tiff_fields).unwrap_or_default();
    let exif_at = sub_directory(&mut enc, &fields, Dir::Exif)?;
    let gps_at = sub_directory(&mut enc, &fields, Dir::Gps)?;
    let mut main: Vec<(u16, Type, Vec<u8>)> = fields.into_iter().filter(|f| f.0 == Dir::Main).map(|(_, t, ty, v)| (t, ty, v)).collect();
    main.extend(exif_at.map(|at| (34665, Type::LONG, at.to_ne_bytes().to_vec())));
    main.extend(gps_at.map(|at| (34853, Type::LONG, at.to_ne_bytes().to_vec())));
    main.extend(b.icc.clone().map(|v| (34675, Type::UNDEFINED, v)));
    main.extend(b.xmp.clone().map(|v| (700, Type::BYTE, v)));
    // IPTC as LONGs, as Photoshop writes it: its bytes in their order, padded to a multiple of 4.
    let (iim, irb) = match b.irb.as_deref().and_then(crate::meta_write::tiff_iptc) {
        Some((iim, irb)) => (Some(iim), Some(irb)),
        None => (None, b.irb.clone()),
    };
    main.extend(iim.map(|v| (33723, Type::LONG, v)));
    main.extend(irb.map(|v| (34377, Type::BYTE, v)));
    macro_rules! write {
        ($c:ty, $data:expr) => {{
            let mut image = enc.new_image::<$c>(w, h)?;
            entries(image.encoder(), main.iter().map(|(t, ty, v)| (*t, *ty, v.as_slice())))?;
            image.write_data($data)
        }};
    }
    match img {
        DynamicImage::ImageRgb16(i) => write!(colortype::RGB16, i.as_raw()),
        DynamicImage::ImageRgba16(i) => write!(colortype::RGBA16, i.as_raw()),
        DynamicImage::ImageLuma16(i) => write!(colortype::Gray16, i.as_raw()),
        DynamicImage::ImageLuma8(i) => write!(colortype::Gray8, i.as_raw()),
        DynamicImage::ImageRgba8(i) => write!(colortype::RGBA8, i.as_raw()),
        i => write!(colortype::RGB8, i.to_rgb8().as_raw()),
    }
}

type Tiff<'a> = tiff::encoder::TiffEncoder<&'a mut Cursor<Vec<u8>>>;

/// An EXIF directory of its own (Exif or GPS) with its fields, written before the image; where
/// it starts, `None` without fields.
fn sub_directory(enc: &mut Tiff, fields: &[(Dir, u16, Type, Vec<u8>)], which: Dir) -> tiff::TiffResult<Option<u32>> {
    let mut list = fields.iter().filter(|f| f.0 == which).map(|(_, t, ty, v)| (*t, *ty, v.as_slice())).peekable();
    if list.peek().is_none() {
        return Ok(None);
    }
    let mut d = enc.extra_directory()?;
    entries(&mut d, list)?;
    Ok(Some(d.finish_with_offsets()?.offset))
}

/// Fields of a directory from their bytes (in this machine's order, as the encoder writes); a
/// field the encoder had already set (sizes, resolution) is replaced.
fn entries<'a>(d: &mut tiff::encoder::DirectoryEncoder<&mut Cursor<Vec<u8>>, tiff::encoder::TiffKindStandard>, list: impl Iterator<Item = (u16, Type, &'a [u8])>) -> tiff::TiffResult<()> {
    let mut dir = tiff::Directory::empty();
    for (tag, ty, bytes) in list {
        dir.extend([(tiff::tags::Tag::Unknown(tag), d.write_entry_bytes(ty, bytes)?)]);
    }
    d.extend_from(&dir);
    Ok(())
}
