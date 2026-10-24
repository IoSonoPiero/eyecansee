//! "Save in another format": the image decoded in full, encoded again with the chosen format and
//! options. The encoded bytes are both what gets saved and, decoded back, the preview: what the
//! window shows is what the file will contain.
//! Every option each encoder offers is there: libjpeg-turbo for JPEG, `image` for PNG, GIF, BMP
//! and QOI, libwebp for WebP, ravif for AVIF, `tiff` for TIFF.

use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Arc;

use eframe::egui::{self, ColorImage, TextureHandle};
use image::codecs::{bmp, gif, png, qoi};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};

use crate::transform::Turn;

/// Output formats, all with encoders in the program. No HEIC (patents) and no JPEG XL (no
/// encoder yet).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Jpeg,
    Png,
    Webp,
    Avif,
    Tiff,
    Bmp,
    Gif,
    Qoi,
}

impl Format {
    pub const ALL: [Format; 8] = [Format::Jpeg, Format::Png, Format::Webp, Format::Avif, Format::Tiff, Format::Bmp, Format::Gif, Format::Qoi];

    pub fn ext(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::Webp => "webp",
            Format::Avif => "avif",
            Format::Tiff => "tif",
            Format::Bmp => "bmp",
            Format::Gif => "gif",
            Format::Qoi => "qoi",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Jpeg => "JPEG",
            Format::Png => "PNG",
            Format::Webp => "WebP",
            Format::Avif => "AVIF",
            Format::Tiff => "TIFF",
            Format::Bmp => "BMP",
            Format::Gif => "GIF",
            Format::Qoi => "QOI",
        }
    }

    /// Can keep EXIF or the colour profile (the others drop them): EXIF in JPEG, PNG, WebP, AVIF,
    /// the profile in JPEG, PNG, WebP, TIFF.
    pub fn holds_meta(self) -> bool {
        matches!(self, Format::Jpeg | Format::Png | Format::Webp | Format::Avif | Format::Tiff)
    }

    /// The format a file can be saved back in after a turn ("Save"), from its name. Not GIF: its
    /// animation would be lost, since only the first frame is decoded (the other formats are
    /// looked at inside by `can_turn_in_place`).
    pub fn of_file(name: &str) -> Option<Format> {
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        Format::ALL.into_iter().filter(|f| *f != Format::Gif).find(|f| f.ext() == ext || (ext == "jpeg" && *f == Format::Jpeg) || (ext == "tiff" && *f == Format::Tiff))
    }
}

/// JPEG colour resolution: full (4:4:4), half across (4:2:2), half both ways (4:2:0), or none (greys).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chroma {
    Full,
    Half,
    Quarter,
    Grey,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PngFilter {
    Adaptive,
    None,
    Sub,
    Up,
    Avg,
    Paeth,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TiffCompression {
    None,
    Lzw,
    DeflateFast,
    Deflate,
    DeflateBest,
    PackBits,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Options {
    pub format: Format,
    pub keep_meta: bool,
    pub jpeg_quality: u8,
    pub chroma: Chroma,
    pub progressive: bool,
    pub optimize: bool,
    pub arithmetic: bool,
    pub jpeg_lossless: bool,
    /// 0 = no compression, 9 = the most.
    pub png_level: u8,
    pub png_filter: PngFilter,
    pub webp_lossless: bool,
    pub webp_quality: u8,
    /// libwebp "method": 0 fast … 6 slow and smaller.
    pub webp_effort: u8,
    /// Lossless only: 100 = off, lower = smaller file with slight changes.
    pub near_lossless: u8,
    pub sharp_yuv: bool,
    pub avif_quality: u8,
    pub avif_alpha_quality: u8,
    /// 1 slow and smaller … 10 fast.
    pub avif_speed: u8,
    pub avif_ten_bit: bool,
    /// RGB instead of YCbCr inside the file: larger, for graphics.
    pub avif_rgb: bool,
    pub tiff: TiffCompression,
    pub tiff_predictor: bool,
    /// GIF colour reduction: 1 fast … 30 best.
    pub gif_quality: u8,
    /// Flips and quarter turns made in the viewer.
    pub turn: Turn,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            format: Format::Jpeg,
            keep_meta: true,
            jpeg_quality: 90,
            chroma: Chroma::Quarter,
            progressive: false,
            optimize: true,
            arithmetic: false,
            jpeg_lossless: false,
            png_level: 6,
            png_filter: PngFilter::Adaptive,
            webp_lossless: false,
            webp_quality: 80,
            webp_effort: 4,
            near_lossless: 100,
            sharp_yuv: false,
            avif_quality: 80,
            avif_alpha_quality: 80,
            avif_speed: 6,
            avif_ten_bit: false,
            avif_rgb: false,
            tiff: TiffCompression::Lzw,
            tiff_predictor: true,
            gif_quality: 21,
            turn: Turn::default(),
        }
    }
}

/// The original, decoded once, with its metadata.
pub struct Original {
    /// Already turned upright (EXIF orientation applied).
    pub image: DynamicImage,
    /// EXIF with the orientation set back to "upright", since the pixels are already turned.
    exif: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
}

impl Original {
    pub fn open(bytes: &[u8], name: &str) -> Result<Original, String> {
        let image = crate::decode::decode_full(bytes, name)?;
        // As found in the formats `image` reads (JPEG, PNG, WebP, TIFF). The others (RAW, HEIC…)
        // get the EXIF the metadata panel shows; their colour profile is still lost.
        let (exif, icc) = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_decoder().ok())
            .map(|mut d| (d.exif_metadata().ok().flatten(), d.icc_profile().ok().flatten()))
            .unwrap_or_default();
        let mut exif = exif.or_else(|| crate::meta::exif_chunk(bytes));
        if let Some(chunk) = &mut exif {
            let _ = Orientation::remove_from_exif_chunk(chunk);
        }
        Ok(Original { image, exif, icc })
    }

    pub fn has_meta(&self) -> bool {
        self.exif.is_some() || self.icc.is_some()
    }

    /// The file as it would be saved.
    pub fn encode(&self, o: &Options) -> Result<Vec<u8>, String> {
        let (exif, icc) = if o.keep_meta { (self.exif.as_deref(), self.icc.as_deref()) } else { (None, None) };
        let turned;
        let image = if o.turn.is_none() {
            &self.image
        } else {
            let mut img = self.image.clone();
            o.turn.apply(&mut img);
            turned = img;
            &turned
        };
        let img = pixels(image, o.format);
        let mut out = Cursor::new(Vec::new());
        match o.format {
            Format::Jpeg => return jpeg(&img, o, exif, icc),
            Format::Webp => return webp(&img, o, exif, icc),
            Format::Avif => return avif(&img, o, exif),
            Format::Tiff => tiff(&img, o, icc, &mut out).map_err(|e| e.to_string())?,
            Format::Png => {
                let level = if o.png_level == 0 { png::CompressionType::Uncompressed } else { png::CompressionType::Level(o.png_level) };
                let filter = match o.png_filter {
                    PngFilter::Adaptive => png::FilterType::Adaptive,
                    PngFilter::None => png::FilterType::NoFilter,
                    PngFilter::Sub => png::FilterType::Sub,
                    PngFilter::Up => png::FilterType::Up,
                    PngFilter::Avg => png::FilterType::Avg,
                    PngFilter::Paeth => png::FilterType::Paeth,
                };
                let mut e = png::PngEncoder::new_with_quality(&mut out, level, filter);
                if let Some(icc) = icc {
                    let _ = e.set_icc_profile(icc.to_vec());
                }
                if let Some(exif) = exif {
                    let _ = e.set_exif_metadata(exif.to_vec());
                }
                img.write_with_encoder(e).map_err(|e| e.to_string())?;
            }
            Format::Bmp => img.write_with_encoder(bmp::BmpEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Gif => img.write_with_encoder(gif::GifEncoder::new_with_speed(&mut out, 31 - o.gif_quality.clamp(1, 30) as i32)).map_err(|e| e.to_string())?,
            Format::Qoi => img.write_with_encoder(qoi::QoiEncoder::new(&mut out)).map_err(|e| e.to_string())?,
        }
        Ok(out.into_inner())
    }
}

/// The file turned, in its own format: a JPEG without re-encoding when its size allows it,
/// otherwise at quality 95; the others encoded again, a lossless WebP still lossless. Also says
/// whether it was encoded again with loss (the confirmation warns about it).
pub fn turned_file(bytes: &[u8], name: &str, turn: Turn) -> Result<(Vec<u8>, bool), String> {
    let format = Format::of_file(name).filter(|_| can_turn_in_place(bytes, name)).ok_or_else(|| name.to_string())?;
    if format == Format::Jpeg
        && let Some(out) = crate::transform::jpeg_lossless(bytes, turn)
    {
        return Ok((out, false));
    }
    let webp_lossless = format == Format::Webp && bytes.windows(4).any(|w| w == b"VP8L");
    let o = Options { format, turn, jpeg_quality: 95, webp_lossless, ..Default::default() };
    let lossy = matches!(format, Format::Jpeg | Format::Avif) || (format == Format::Webp && !webp_lossless);
    Ok((Original::open(bytes, name)?.encode(&o)?, lossy))
}

/// Can the file be turned and written back over itself, losing at most a little quality? Its
/// format must be one EyeCanSee writes, and the file a single picture: not animated (APNG, WebP,
/// AVIF sequence), not several pages (TIFF), not a JPEG carrying more than the picture (the video
/// of a Motion Photo after its end, MPF secondary images or depth maps), which would be lost.
pub fn can_turn_in_place(bytes: &[u8], name: &str) -> bool {
    let cursor = || Cursor::new(bytes);
    match Format::of_file(name) {
        Some(Format::Jpeg) => plain_jpeg(bytes),
        Some(Format::Png) => png::PngDecoder::new(cursor()).is_ok_and(|d| d.is_apng().is_ok_and(|a| !a)),
        Some(Format::Webp) => image::codecs::webp::WebPDecoder::new(cursor()).is_ok_and(|d| !d.has_animation()),
        Some(Format::Tiff) => tiff::decoder::Decoder::new(cursor()).is_ok_and(|d| !d.more_images()),
        Some(Format::Avif) => !avif_sequence(bytes),
        Some(_) => true,
        None => false,
    }
}

/// A JPEG that ends at its EOI marker (zeros after it are padding) and has no MPF segment.
/// libjpeg-turbo drops what follows EOI (tried: 27 bytes after it, gone after a turn).
fn plain_jpeg(b: &[u8]) -> bool {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return false;
    }
    let mut i = 2;
    while i + 1 < b.len() {
        if b[i] != 0xFF {
            return false;
        }
        let marker = b[i + 1];
        match marker {
            0xFF => {
                i += 1; // fill byte
                continue;
            }
            0xD9 => return b[i + 2..].iter().all(|&x| x == 0),
            0x01 | 0xD0..=0xD7 => {
                i += 2;
                continue;
            }
            _ => {}
        }
        let Some(len) = b.get(i + 2..i + 4).map(|l| u16::from_be_bytes([l[0], l[1]]) as usize).filter(|l| *l >= 2) else { return false };
        if marker == 0xE2 && b.get(i + 4..i + 2 + len).is_some_and(|body| body.starts_with(b"MPF\0")) {
            return false;
        }
        i += 2 + len;
        if marker == 0xDA {
            // Coded data, up to the next marker: not a stuffed 0xFF00, not a restart.
            while i + 1 < b.len() && !(b[i] == 0xFF && b[i + 1] != 0 && !(0xD0..=0xD7).contains(&b[i + 1])) {
                i += 1;
            }
        }
    }
    false
}

/// AVIF image sequence: brand "avis" in the ftyp box.
fn avif_sequence(b: &[u8]) -> bool {
    let size = b.get(..4).map_or(0, |s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize);
    b.get(4..8) == Some(b"ftyp") && b.get(8..size.min(b.len())).is_some_and(|brands| brands.as_chunks::<4>().0.contains(b"avis"))
}

/// Pixels the format can store: JPEG has no transparency (laid on white, not on black), and
/// only PNG and TIFF take 16 bits; floating point (EXR, HDR) becomes 8 bits.
fn pixels(img: &DynamicImage, format: Format) -> DynamicImage {
    let alpha = img.color().has_alpha();
    let deep = matches!(img, DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_) | DynamicImage::ImageLuma16(_));
    match format {
        Format::Jpeg if alpha => {
            let mut rgba = img.to_rgba8();
            for p in rgba.pixels_mut() {
                let a = p[3] as u16;
                for c in 0..3 {
                    p[c] = ((p[c] as u16 * a + 255 * (255 - a)) / 255) as u8;
                }
            }
            DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
        }
        Format::Png | Format::Tiff if deep || matches!(img, DynamicImage::ImageLuma8(_)) => img.clone(),
        Format::Png if matches!(img, DynamicImage::ImageLumaA8(_) | DynamicImage::ImageLumaA16(_)) => img.clone(),
        _ if alpha && format != Format::Jpeg => DynamicImage::ImageRgba8(img.to_rgba8()),
        _ => DynamicImage::ImageRgb8(img.to_rgb8()),
    }
}

/// libjpeg-turbo; EXIF and colour profile go in as APP1 and APP2 segments, which it does not write.
fn jpeg(img: &DynamicImage, o: &Options, exif: Option<&[u8]>, icc: Option<&[u8]>) -> Result<Vec<u8>, String> {
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
    Ok(jpeg_meta(file, exif, icc))
}

/// After SOI (and the JFIF APP0, which must come first): APP1 "Exif", then the profile in APP2
/// "ICC_PROFILE" pieces of at most 65519 bytes. EXIF too big for one segment is left out.
fn jpeg_meta(file: Vec<u8>, exif: Option<&[u8]>, icc: Option<&[u8]>) -> Vec<u8> {
    if exif.is_none() && icc.is_none() || !file.starts_with(&[0xFF, 0xD8]) {
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
    if let Some(exif) = exif.filter(|e| e.len() <= 65533 - 6) {
        segment(0xE1, &[b"Exif\0\0", exif]);
    }
    if let Some(icc) = icc {
        let pieces: Vec<&[u8]> = icc.chunks(65519).collect();
        for (i, piece) in pieces.iter().enumerate() {
            segment(0xE2, &[b"ICC_PROFILE\0", &[i as u8 + 1, pieces.len() as u8], piece]);
        }
    }
    out.extend_from_slice(&file[at..]);
    out
}

/// libwebp, lossy or lossless; metadata added by rewriting the container (`webp_meta`).
fn webp(img: &DynamicImage, o: &Options, exif: Option<&[u8]>, icc: Option<&[u8]>) -> Result<Vec<u8>, String> {
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
    Ok(webp_meta(&file, w, h, alpha, exif, icc))
}

/// The libwebp file inside an extended container (VP8X) with ICCP and EXIF chunks.
fn webp_meta(file: &[u8], w: u32, h: u32, alpha: bool, exif: Option<&[u8]>, icc: Option<&[u8]>) -> Vec<u8> {
    if exif.is_none() && icc.is_none() || file.len() < 12 {
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
    let flags = (if icc.is_some() { 0x20 } else { 0 }) | (if alpha { 0x10 } else { 0 }) | (if exif.is_some() { 0x08 } else { 0 });
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
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

/// ravif (rav1e); it keeps EXIF but has no place for a colour profile.
fn avif(img: &DynamicImage, o: &Options, exif: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut e = ravif::Encoder::new()
        .with_quality(o.avif_quality as f32)
        .with_alpha_quality(o.avif_alpha_quality as f32)
        .with_speed(o.avif_speed)
        .with_bit_depth(if o.avif_ten_bit { ravif::BitDepth::Ten } else { ravif::BitDepth::Eight })
        .with_internal_color_model(if o.avif_rgb { ravif::ColorModel::RGB } else { ravif::ColorModel::YCbCr });
    if let Some(exif) = exif {
        e = e.with_exif(exif.to_vec());
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

/// `tiff` with the chosen compression; the colour profile as tag 34675.
fn tiff(img: &DynamicImage, o: &Options, icc: Option<&[u8]>, out: &mut Cursor<Vec<u8>>) -> tiff::TiffResult<()> {
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
    macro_rules! write {
        ($c:ty, $data:expr) => {{
            let mut image = enc.new_image::<$c>(w, h)?;
            if let Some(icc) = icc {
                image.encoder().write_tag(tiff::tags::Tag::Unknown(34675), icc)?;
            }
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

/// The original decoded, its texture and file size.
type Loaded = Result<(Arc<Original>, TextureHandle, u64), String>;
/// The encoded file and, decoded back, its texture.
type Encoded = Result<(Arc<Vec<u8>>, Option<TextureHandle>), String>;

/// The open "Save in another format" window.
pub struct Window {
    /// The original, relative to the source.
    pub path: String,
    original: Option<Loaded>,
    options: Options,
    /// Options of the encoding under way or shown; a change starts a new one when it ends.
    encoded_with: Option<Options>,
    pub generation: u64,
    busy: bool,
    result: Option<Encoded>,
    name: String,
    /// 100%: one image pixel per screen pixel, moved by dragging (`pan` = centre, in image pixels).
    actual: bool,
    pan: egui::Vec2,
    saving: bool,
    /// Height of the part under the images, measured at the last frame.
    bottom_h: f32,
    error: Option<String>,
}

impl Window {
    pub fn new(path: String) -> Self {
        let options = Options::default();
        let name = with_ext(crate::source::name(&path), options.format);
        Window {
            path,
            original: None,
            options,
            encoded_with: None,
            generation: 0,
            busy: false,
            result: None,
            name,
            actual: false,
            pan: egui::Vec2::ZERO,
            saving: false,
            bottom_h: 300.0,
            error: None,
        }
    }
}

fn with_ext(name: &str, format: Format) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    format!("{stem}.{}", format.ext())
}

impl crate::App {
    pub(crate) fn open_convert(&mut self, path: Option<String>) {
        let Some(path) = path.or_else(|| self.target()).filter(|p| !p.ends_with('/')) else { return };
        let Some(loader) = &self.loader else { return };
        loader.load_original(path.clone());
        let mut w = Window::new(path);
        // From the viewer: saved as it is shown there.
        if self.viewing.is_some() {
            w.options.turn = self.turn;
        }
        self.convert = Some(w);
    }

    pub(crate) fn convert_original(&mut self, ctx: &egui::Context, r: Result<(Arc<Original>, ColorImage, u64), String>) {
        let Some(w) = &mut self.convert else { return };
        w.original = Some(r.map(|(original, preview, size)| {
            w.pan = egui::vec2(preview.size[0] as f32, preview.size[1] as f32) / 2.0;
            (original, ctx.load_texture("convert-original", preview, egui::TextureOptions::LINEAR), size)
        }));
    }

    pub(crate) fn convert_encoded(&mut self, ctx: &egui::Context, generation: u64, r: Result<(Arc<Vec<u8>>, Option<ColorImage>), String>) {
        let Some(w) = &mut self.convert else { return };
        if generation != w.generation {
            return;
        }
        w.busy = false;
        w.result = Some(r.map(|(bytes, preview)| (bytes, preview.map(|p| ctx.load_texture("convert-result", p, egui::TextureOptions::LINEAR)))));
    }

    pub(crate) fn convert_saved(&mut self, r: Result<String, crate::source::Error>) {
        match r {
            Ok(path) => {
                log!("saved as {path:?}");
                self.convert = None;
                self.relist(crate::source::parent(&path));
            }
            Err((id, detail)) => {
                if let Some(w) = &mut self.convert {
                    w.saving = false;
                    w.error = Some(self.i18n.t_args(id, &[("detail", &detail)]));
                }
            }
        }
    }

    /// Drawn in place of the whole interface, as a page of its own: a window could not cover the
    /// menu, tool and status bars, and the two images need all the room there is.
    pub(crate) fn convert_page(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        let Some(mut w) = self.convert.take() else { return };
        // One encoding at a time (AVIF takes seconds): a change made meanwhile starts when it ends.
        if !w.busy
            && w.encoded_with != Some(w.options)
            && let Some(Ok((original, _, _))) = &w.original
        {
            w.generation += 1;
            w.busy = true;
            // Another format: the old preview would pass for the new one while it is being made.
            if w.encoded_with.map(|o| o.format) != Some(w.options.format) {
                w.result = None;
            }
            w.encoded_with = Some(w.options);
            let (original, options, generation, tx, ctx2) = (original.clone(), w.options, w.generation, self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let r = original.encode(&options).map(|bytes| {
                    let preview = crate::decode::decode(&bytes, &format!("x.{}", options.format.ext())).ok();
                    (Arc::new(bytes), preview)
                });
                if tx.send(crate::loader::Msg::Encoded(generation, r)).is_ok() {
                    ctx2.request_repaint();
                }
            });
        }

        let i18n = &self.i18n;
        let t = |id| i18n.t(id);
        let human = |b: u64| self.human_size(b);
        let dir = crate::source::parent(&w.path);
        let taken: HashSet<String> = match self.listings.get(&dir) {
            Some(Ok(l)) => l.dirs.iter().chain(l.files.iter().map(|f| &f.path)).map(|p| crate::source::name(p).trim_end_matches('/').to_string()).collect(),
            _ => HashSet::new(),
        };
        let final_name = crate::source::free_name(&taken, w.name.trim());
        let mut open = true;
        let mut save = false;
        ui.horizontal(|ui| {
            ui.heading(t("convert-title"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✕").on_hover_text("Esc").clicked() {
                    open = false;
                }
            });
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        ui.separator();
        (|ui: &mut egui::Ui| {
                let original = match &w.original {
                    None => {
                        ui.centered_and_justified(|ui| ui.spinner());
                        return;
                    }
                    Some(Err(e)) => {
                        ui.label(crate::browser::error_text(ui, i18n.t_args("cannot-load", &[("file", crate::source::name(&w.path)), ("error", e)])));
                        return;
                    }
                    Some(Ok(o)) => o.clone(),
                };
                let (image, tex, size) = (&original.0.image, &original.1, original.2);
                const GAP: f32 = 16.0;
                let half = (ui.available_width() - GAP) / 2.0;
                // The images get what the options below leave (their height measured at the last frame).
                let pane = egui::vec2(half, (ui.available_height() - 22.0 - 16.0 - w.bottom_h).max(120.0));
                let result_tex = match &w.result {
                    Some(Ok((_, Some(tex)))) => Some(tex),
                    _ => None,
                };
                // Titles in rows of the same height (the spinner is taller than the text), so the
                // two images start at the same height.
                let title = |ui: &mut egui::Ui, text: String, busy: bool| {
                    ui.allocate_ui_with_layout(egui::vec2(half, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_size(egui::vec2(half, 22.0));
                        ui.strong(text);
                        if busy {
                            ui.spinner();
                            ui.weak(t("convert-working"));
                        }
                    });
                };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    ui.vertical(|ui| {
                        title(ui, t("convert-original"), false);
                        let r = preview(ui, pane, Some(tex), None, w.actual, &mut w.pan);
                        drag(&r, w.actual, &mut w.pan);
                    });
                    ui.vertical(|ui| {
                        title(ui, t("convert-result"), w.busy);
                        let note = match &w.result {
                            _ if w.busy && result_tex.is_none() => Some(t("convert-working")),
                            Some(Ok((_, None))) => Some(t("convert-no-preview")),
                            _ => None,
                        };
                        let r = preview(ui, pane, result_tex, note.as_deref(), w.actual, &mut w.pan);
                        drag(&r, w.actual, &mut w.pan);
                    });
                });
                ui.add_space(4.0);
                let pixels = i18n.t_args("convert-pixels", &[("w", &image.width().to_string()), ("h", &image.height().to_string())]);
                let bottom = ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    ui.vertical(|ui| {
                        ui.set_width(half);
                        let ext = crate::source::name(&w.path).rsplit_once('.').map_or(String::new(), |(_, e)| e.to_uppercase());
                        ui.label(format!("{ext} · {pixels}"));
                        ui.label(i18n.t_args("convert-file-size", &[("size", &human(size))]));
                        ui.add_space(6.0);
                        ui.checkbox(&mut w.actual, t("convert-actual"));
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.set_width(half);
                        match &w.result {
                            Some(Ok((bytes, _))) => {
                                let percent = (bytes.len() as u64 * 100 / size.max(1)).to_string();
                                ui.label(format!("{} · {pixels}", w.options.format.label()));
                                ui.label(format!(
                                    "{} ({})",
                                    i18n.t_args("convert-file-size", &[("size", &human(bytes.len() as u64))]),
                                    i18n.t_args("convert-ratio", &[("percent", &percent)])
                                ));
                            }
                            Some(Err(e)) => {
                                ui.label(crate::browser::error_text(ui, i18n.t_args("convert-error", &[("detail", e)])));
                            }
                            None => {
                                ui.label(" ");
                                ui.label(" ");
                            }
                        }
                        ui.add_space(6.0);
                        let o = &mut w.options;
                        let before = o.format;
                        egui::ComboBox::from_label(t("convert-format")).selected_text(o.format.label()).show_ui(ui, |ui| {
                            for f in Format::ALL {
                                ui.selectable_value(&mut o.format, f, f.label());
                            }
                        });
                        if o.format != before {
                            w.name = with_ext(&w.name, o.format);
                        }
                        let alpha = original.0.image.color().has_alpha();
                        let quality = |ui: &mut egui::Ui, v: &mut u8, label: String| {
                            ui.add(egui::Slider::new(v, 1..=100).text(label));
                        };
                        match o.format {
                            Format::Jpeg => {
                                ui.add_enabled_ui(!o.jpeg_lossless, |ui| {
                                    quality(ui, &mut o.jpeg_quality, t("convert-quality"));
                                    egui::ComboBox::from_label(t("convert-chroma"))
                                        .selected_text(t(match o.chroma {
                                            Chroma::Full => "convert-chroma-full",
                                            Chroma::Half => "convert-chroma-half",
                                            Chroma::Quarter => "convert-chroma-quarter",
                                            Chroma::Grey => "convert-chroma-grey",
                                        }))
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut o.chroma, Chroma::Full, t("convert-chroma-full"));
                                            ui.selectable_value(&mut o.chroma, Chroma::Half, t("convert-chroma-half"));
                                            ui.selectable_value(&mut o.chroma, Chroma::Quarter, t("convert-chroma-quarter"));
                                            ui.selectable_value(&mut o.chroma, Chroma::Grey, t("convert-chroma-grey"));
                                        });
                                });
                                ui.checkbox(&mut o.progressive, t("convert-progressive"));
                                ui.checkbox(&mut o.optimize, t("convert-optimize"));
                                ui.checkbox(&mut o.arithmetic, t("convert-arithmetic"));
                                ui.checkbox(&mut o.jpeg_lossless, t("convert-jpeg-lossless"));
                            }
                            Format::Png => {
                                ui.add(egui::Slider::new(&mut o.png_level, 0..=9).text(t("convert-png-level")));
                                let filters = [
                                    (PngFilter::Adaptive, t("convert-filter-adaptive")),
                                    (PngFilter::None, t("convert-none")),
                                    (PngFilter::Sub, "Sub".to_string()),
                                    (PngFilter::Up, "Up".to_string()),
                                    (PngFilter::Avg, "Average".to_string()),
                                    (PngFilter::Paeth, "Paeth".to_string()),
                                ];
                                let shown = filters.iter().find(|(f, _)| *f == o.png_filter).map(|(_, l)| l.clone()).unwrap_or_default();
                                egui::ComboBox::from_label(t("convert-png-filter")).selected_text(shown).show_ui(ui, |ui| {
                                    for (f, label) in filters {
                                        ui.selectable_value(&mut o.png_filter, f, label);
                                    }
                                });
                            }
                            Format::Webp => {
                                ui.horizontal(|ui| {
                                    ui.radio_value(&mut o.webp_lossless, false, t("convert-lossy"));
                                    ui.radio_value(&mut o.webp_lossless, true, t("convert-lossless-mode"));
                                });
                                if o.webp_lossless {
                                    ui.add(egui::Slider::new(&mut o.near_lossless, 0..=100).text(t("convert-near-lossless")));
                                } else {
                                    quality(ui, &mut o.webp_quality, t("convert-quality"));
                                    ui.checkbox(&mut o.sharp_yuv, t("convert-sharp-yuv"));
                                }
                                ui.add(egui::Slider::new(&mut o.webp_effort, 0..=6).text(t("convert-effort"))).on_hover_text(t("convert-speed-hint"));
                            }
                            Format::Avif => {
                                quality(ui, &mut o.avif_quality, t("convert-quality"));
                                if alpha {
                                    quality(ui, &mut o.avif_alpha_quality, t("convert-alpha-quality"));
                                }
                                ui.add(egui::Slider::new(&mut o.avif_speed, 1..=10).text(t("convert-speed"))).on_hover_text(t("convert-speed-hint"));
                                ui.checkbox(&mut o.avif_ten_bit, t("convert-ten-bit"));
                                ui.checkbox(&mut o.avif_rgb, t("convert-avif-rgb"));
                            }
                            Format::Tiff => {
                                let kinds = [
                                    (TiffCompression::None, t("convert-none")),
                                    (TiffCompression::Lzw, "LZW".to_string()),
                                    (TiffCompression::DeflateFast, t("convert-deflate-fast")),
                                    (TiffCompression::Deflate, "Deflate".to_string()),
                                    (TiffCompression::DeflateBest, t("convert-deflate-best")),
                                    (TiffCompression::PackBits, "PackBits".to_string()),
                                ];
                                let shown = kinds.iter().find(|(k, _)| *k == o.tiff).map(|(_, l)| l.clone()).unwrap_or_default();
                                egui::ComboBox::from_label(t("convert-compression")).selected_text(shown).show_ui(ui, |ui| {
                                    for (k, label) in kinds {
                                        ui.selectable_value(&mut o.tiff, k, label);
                                    }
                                });
                                let predictor = matches!(o.tiff, TiffCompression::Lzw | TiffCompression::DeflateFast | TiffCompression::Deflate | TiffCompression::DeflateBest);
                                ui.add_enabled(predictor, egui::Checkbox::new(&mut o.tiff_predictor, t("convert-predictor")));
                            }
                            Format::Gif => {
                                ui.add(egui::Slider::new(&mut o.gif_quality, 1..=30).text(t("convert-gif-quality")));
                            }
                            Format::Bmp | Format::Qoi => {}
                        }
                        let lossless = match o.format {
                            Format::Png | Format::Tiff | Format::Bmp | Format::Qoi => true,
                            Format::Webp => o.webp_lossless && o.near_lossless == 100,
                            Format::Jpeg => o.jpeg_lossless,
                            _ => false,
                        };
                        if lossless {
                            ui.weak(t("convert-lossless"));
                        } else if o.format == Format::Gif {
                            ui.weak(t("convert-gif"));
                        }
                        let can_meta = original.0.has_meta() && o.format.holds_meta();
                        let hint = if !original.0.has_meta() { t("convert-no-meta") } else if !o.format.holds_meta() { t("convert-meta-unsupported") } else { String::new() };
                        let r = ui.add_enabled(can_meta, egui::Checkbox::new(&mut o.keep_meta, t("convert-keep-meta")));
                        if !hint.is_empty() {
                            r.on_disabled_hover_text(hint);
                        }
                        // The name belongs to the new image, so it sits on its side.
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(t("convert-name"));
                            ui.add(egui::TextEdit::singleline(&mut w.name).desired_width((half - 200.0).clamp(160.0, 420.0)));
                            let ready = matches!(w.result, Some(Ok(_))) && !w.busy && !w.saving && !w.name.trim().is_empty() && !w.name.contains(['/', '\\']);
                            if ui.add_enabled(ready, egui::Button::new(t("convert-save"))).clicked() {
                                save = true;
                            }
                            if w.saving {
                                ui.spinner();
                            }
                        });
                        if final_name != w.name.trim() && !w.name.trim().is_empty() {
                            ui.weak(i18n.t_args("convert-exists", &[("name", &final_name)]));
                        }
                        if let Some(e) = &w.error {
                            ui.label(crate::browser::error_text(ui, e.as_str()));
                        }
                    });
                })
                .response;
                // Measured, for the next frame: the options change with the format.
                let used = bottom.rect.height() + 8.0;
                if (used - w.bottom_h).abs() > 1.0 {
                    w.bottom_h = used;
                    ui.ctx().request_repaint();
                }
            })(ui);
        if save
            && let (Some(Ok((bytes, _))), Some(loader)) = (&w.result, &self.loader)
        {
            w.saving = true;
            w.error = None;
            loader.save(format!("{dir}{final_name}"), bytes.clone());
        }
        // Taken out while drawn (it needs the rest of the app), put back unless closed.
        if open {
            self.convert = Some(w);
        }
    }
}

/// The image fitted into `size` (never enlarged), or at 100% around `pan`; grey when none.
/// `note` is written in the middle when there is no image (converting, no preview for the format).
fn preview(ui: &mut egui::Ui, size: egui::Vec2, tex: Option<&TextureHandle>, note: Option<&str>, actual: bool, pan: &mut egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
    let painter = ui.painter_at(rect);
    // Grey ground and a border: on the window's own colour the two images ran into each other.
    let ground = if ui.visuals().dark_mode { egui::Color32::from_gray(24) } else { egui::Color32::from_gray(200) };
    painter.rect_filled(rect, 2.0, ground);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
    let Some(tex) = tex else {
        if let Some(note) = note {
            let font = egui::TextStyle::Body.resolve(ui.style());
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, note, font, ui.visuals().text_color());
        }
        return response;
    };
    let ppp = ui.ctx().pixels_per_point();
    let t = tex.size_vec2();
    if actual {
        paint_zoomed(&painter, rect, tex, 1.0, pan, Turn::default());
    } else {
        let scale = (rect.width() / t.x).min(rect.height() / t.y).min(1.0 / ppp);
        let dest = egui::Rect::from_center_size(rect.center(), t * scale);
        paint_turned(&painter, tex, dest, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Turn::default());
    }
    response
}

/// `tex` drawn in `rect` at `scale` screen pixels per image pixel, around `pan` (the centre of the
/// view, in image pixels). `pan` is kept inside the image; a side smaller than the view stays centred.
/// `turn`: flipped and turned as in the viewer (sizes and `pan` are those of the turned image).
pub fn paint_zoomed(painter: &egui::Painter, rect: egui::Rect, tex: &TextureHandle, scale: f32, pan: &mut egui::Vec2, turn: Turn) {
    let ppp = painter.ctx().pixels_per_point();
    let t = turned_size(tex, turn);
    let half = rect.size() * ppp / scale / 2.0;
    let clamp = |c: f32, h: f32, full: f32| if full <= 2.0 * h { full / 2.0 } else { c.clamp(h, full - h) };
    *pan = egui::vec2(clamp(pan.x, half.x, t.x), clamp(pan.y, half.y, t.y));
    let min = (*pan - half).max(egui::Vec2::ZERO);
    let max = (*pan + half).min(t);
    let dest = egui::Rect::from_center_size(rect.center() + ((min + max) / 2.0 - *pan) * scale / ppp, (max - min) * scale / ppp);
    let uv = egui::Rect::from_min_max((min / t).to_pos2(), (max / t).to_pos2());
    paint_turned(painter, tex, dest, uv, turn);
}

pub fn turned_size(tex: &TextureHandle, turn: Turn) -> egui::Vec2 {
    let t = tex.size_vec2();
    if turn.sideways() { egui::vec2(t.y, t.x) } else { t }
}

/// The part `uv` of the turned image (0 to 1 on each side) in `dest`: each corner takes the
/// texture point it comes from, so the GPU turns it.
pub fn paint_turned(painter: &egui::Painter, tex: &TextureHandle, dest: egui::Rect, uv: egui::Rect, turn: Turn) {
    let mut mesh = egui::Mesh::with_texture(tex.id());
    for (pos, shown) in [(dest.left_top(), uv.left_top()), (dest.right_top(), uv.right_top()), (dest.right_bottom(), uv.right_bottom()), (dest.left_bottom(), uv.left_bottom())] {
        mesh.vertices.push(egui::epaint::Vertex { pos, uv: turn.source_uv(shown), color: egui::Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Dragging either preview moves both, at 100%.
fn drag(response: &egui::Response, actual: bool, pan: &mut egui::Vec2) {
    if actual && response.dragged() {
        *pan -= response.drag_delta() * response.ctx.pixels_per_point();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG made animated (APNG): an acTL chunk after IHDR.
    pub(crate) fn apng(png: &[u8]) -> Vec<u8> {
        let crc = |data: &[u8]| {
            let mut c = !0u32;
            for &b in data {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
            }
            !c
        };
        let body = [&b"acTL"[..], &1u32.to_be_bytes(), &0u32.to_be_bytes()].concat();
        let chunk = [&8u32.to_be_bytes()[..], &body, &crc(&body).to_be_bytes()].concat();
        let mut apng = png.to_vec();
        apng.splice(33..33, chunk);
        apng
    }

    /// An animated WebP of one frame: the lossless frame of `img` (8×8) inside VP8X, ANIM and ANMF chunks.
    pub(crate) fn animated_webp(img: &DynamicImage) -> Vec<u8> {
        let mut still = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut still).write_image(img.as_bytes(), 8, 8, image::ExtendedColorType::Rgb8).unwrap();
        let frame = &still[12..];
        let chunk = |id: &[u8], data: &[u8]| {
            let pad: &[u8] = if data.len() % 2 == 1 { &[0] } else { &[] };
            [id, &(data.len() as u32).to_le_bytes(), data, pad].concat()
        };
        let side = [7, 0, 0, 7, 0, 0];
        let anmf = [&[0, 0, 0, 0, 0, 0][..], &side, &[100, 0, 0, 0], frame].concat();
        let chunks = [chunk(b"VP8X", &[&[0x02, 0, 0, 0][..], &side].concat()), chunk(b"ANIM", &[0; 6]), chunk(b"ANMF", &anmf)].concat();
        [&b"RIFF"[..], &(4 + chunks.len() as u32).to_le_bytes(), b"WEBP", &chunks].concat()
    }

    /// A TIFF with two pages of `img`.
    pub(crate) fn tiff_pages(img: &DynamicImage) -> Vec<u8> {
        let mut pages = Vec::new();
        let mut enc = tiff::encoder::TiffEncoder::new(Cursor::new(&mut pages)).unwrap();
        for _ in 0..2 {
            enc.write_image::<tiff::encoder::colortype::RGB8>(img.width(), img.height(), img.to_rgb8().as_raw()).unwrap();
        }
        pages
    }

    fn read_meta(bytes: &[u8]) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let mut d = ImageReader::new(Cursor::new(bytes)).with_guessed_format().unwrap().into_decoder().unwrap();
        (d.exif_metadata().unwrap(), d.icc_profile().unwrap())
    }

    /// Every format, with its main variants, encodes; what is written decodes back to the same
    /// size (and, when lossless, the same pixels); EXIF and colour profile survive where the
    /// format holds them, and are gone when not kept.
    #[test]
    fn every_format() {
        // 1200 different colours: the GIF must bring them down to 256.
        let mut img = image::RgbaImage::new(40, 30);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 6) as u8, (y * 8) as u8, (x * y) as u8, if x < 20 { 255 } else { 100 }]);
        }
        let mut exif = b"II*\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00".to_vec();
        assert_eq!(Orientation::remove_from_exif_chunk(&mut exif), Some(Orientation::Rotate90));
        let icc = b"not a real profile, only bytes to carry".to_vec();
        let original = Original { image: DynamicImage::ImageRgba8(img), exif: Some(exif.clone()), icc: Some(icc.clone()) };
        let d = Options::default();
        let variants = [
            Options { format: Format::Jpeg, ..d },
            Options { format: Format::Jpeg, progressive: true, chroma: Chroma::Full, optimize: false, ..d },
            Options { format: Format::Jpeg, chroma: Chroma::Grey, ..d },
            Options { format: Format::Png, ..d },
            Options { format: Format::Png, png_level: 0, png_filter: PngFilter::Paeth, ..d },
            Options { format: Format::Webp, ..d },
            Options { format: Format::Webp, webp_lossless: true, ..d },
            Options { format: Format::Avif, ..d },
            Options { format: Format::Tiff, ..d },
            Options { format: Format::Tiff, tiff: TiffCompression::DeflateBest, ..d },
            Options { format: Format::Tiff, tiff: TiffCompression::PackBits, ..d },
            Options { format: Format::Tiff, tiff: TiffCompression::None, ..d },
            Options { format: Format::Bmp, ..d },
            Options { format: Format::Gif, ..d },
            Options { format: Format::Qoi, ..d },
        ];
        for o in variants {
            let format = o.format;
            let bytes = original.encode(&o).unwrap_or_else(|e| panic!("{o:?}: {e}"));
            if format == Format::Avif {
                assert_eq!(&bytes[4..12], b"ftypavif");
                // `image` has no AVIF decoder: it is read back through libheif, when built with it.
                if cfg!(feature = "heif") {
                    let back = crate::decode::decode_full(&bytes, "x.avif").unwrap_or_else(|e| panic!("{o:?}: {e}"));
                    assert_eq!((back.width(), back.height()), (40, 30), "{o:?}");
                }
                continue;
            }
            let back = image::load_from_memory(&bytes).unwrap_or_else(|e| panic!("{o:?}: {e}"));
            assert_eq!((back.width(), back.height()), (40, 30), "{o:?}");
            if format == Format::Gif {
                let colours: HashSet<[u8; 4]> = back.to_rgba8().pixels().map(|p| p.0).collect();
                assert!(colours.len() <= 256, "GIF with {} colours", colours.len());
            }
            if matches!(format, Format::Png | Format::Tiff | Format::Qoi) || (format == Format::Webp && o.webp_lossless) {
                assert_eq!(back.to_rgba8(), original.image.to_rgba8(), "{o:?}");
            }
            let (e, i) = read_meta(&bytes);
            if matches!(format, Format::Jpeg | Format::Png | Format::Webp) {
                assert_eq!(e.as_ref(), Some(&exif), "{o:?}");
            }
            if matches!(format, Format::Jpeg | Format::Png | Format::Webp | Format::Tiff) {
                assert_eq!(i.as_ref(), Some(&icc), "{o:?}");
            }
            let bare = original.encode(&Options { keep_meta: false, ..o }).unwrap();
            if format != Format::Gif {
                assert_eq!(read_meta(&bare), (None, None), "{o:?}");
            }
        }
    }

    /// What cannot be saved over itself, found inside the file: animations, pages, what a JPEG
    /// carries besides its picture. The plain files of the same formats can.
    #[test]
    fn turn_in_place() {
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_fn(8, 8, |x, y| image::Rgb([x as u8 * 30, y as u8 * 30, 0])));
        let file = |f| {
            let mut v = Vec::new();
            img.write_to(&mut Cursor::new(&mut v), f).unwrap();
            v
        };
        // JPEG: plain, with zero padding, with a video after EOI, with an MPF segment.
        let jpeg = file(image::ImageFormat::Jpeg);
        assert!(can_turn_in_place(&jpeg, "a.jpg"));
        assert!(can_turn_in_place(&[jpeg.as_slice(), &[0; 16]].concat(), "a.jpeg"));
        assert!(!can_turn_in_place(&[jpeg.as_slice(), b"\0\0\0\x18ftypmp42"].concat(), "a.jpg"));
        let mut mpf = jpeg.clone();
        mpf.splice(2..2, [&[0xFF, 0xE2, 0, 10][..], b"MPF\0II*\0"].concat());
        assert!(!can_turn_in_place(&mpf, "a.jpg"));

        let png = file(image::ImageFormat::Png);
        assert!(can_turn_in_place(&png, "a.png"));
        let apng = apng(&png);
        assert!(png::PngDecoder::new(Cursor::new(&apng)).is_ok_and(|d| d.is_apng().unwrap()));
        assert!(!can_turn_in_place(&apng, "a.png"));

        assert!(can_turn_in_place(&file(image::ImageFormat::WebP), "a.webp"));
        let animated = animated_webp(&img);
        assert!(image::codecs::webp::WebPDecoder::new(Cursor::new(&animated)).is_ok_and(|d| d.has_animation()));
        assert!(!can_turn_in_place(&animated, "a.webp"));

        // TIFF of two pages.
        assert!(can_turn_in_place(&file(image::ImageFormat::Tiff), "a.tif"));
        assert!(!can_turn_in_place(&tiff_pages(&img), "a.tiff"));

        // AVIF sequence: brand "avis".
        let ftyp = |major: &[u8]| [&24u32.to_be_bytes()[..], b"ftyp", major, &[0; 4], major, b"mif1"].concat();
        assert!(can_turn_in_place(&ftyp(b"avif"), "a.avif"));
        assert!(!can_turn_in_place(&ftyp(b"avis"), "a.avif"));

        assert!(!can_turn_in_place(&file(image::ImageFormat::Gif), "a.gif"));
        assert!(!can_turn_in_place(&jpeg, "a.heic"));
    }
}
