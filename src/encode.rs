//! Encoding an image again in another format, without the interface: every option each encoder
//! offers is there: libjpeg-turbo for JPEG, `image` for PNG, GIF, BMP and QOI, libwebp for WebP,
//! ravif for AVIF, `tiff` for TIFF, `image` for ICO, TGA, PNM, OpenEXR, Radiance HDR and Farbfeld. Writing back over the original: `save_back`.


pub mod jpeg_quality;
use std::io::Cursor;

use image::codecs::{bmp, farbfeld, gif, hdr, ico, openexr, png, pnm, qoi, tga};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};

use crate::encoders;
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
    Ico,
    Tga,
    Pnm,
    Exr,
    Hdr,
    Farbfeld,
}

impl Format {
    pub const ALL: [Format; 14] = [
        Format::Jpeg,
        Format::Png,
        Format::Webp,
        Format::Avif,
        Format::Tiff,
        Format::Bmp,
        Format::Gif,
        Format::Qoi,
        Format::Ico,
        Format::Tga,
        Format::Pnm,
        Format::Exr,
        Format::Hdr,
        Format::Farbfeld,
    ];
    /// The formats "Save" writes back over an original. The others only as "Save as…": an ICO
    /// holds several sizes, OpenEXR and HDR more than what is shown.
    const BACK: [Format; 8] = [Format::Jpeg, Format::Png, Format::Webp, Format::Avif, Format::Tiff, Format::Bmp, Format::Gif, Format::Qoi];

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
            Format::Ico => "ico",
            Format::Tga => "tga",
            Format::Pnm => "pnm",
            Format::Exr => "exr",
            Format::Hdr => "hdr",
            Format::Farbfeld => "ff",
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
            Format::Ico => "ICO",
            Format::Tga => "TGA",
            Format::Pnm => "PNM",
            Format::Exr => "OpenEXR",
            Format::Hdr => "Radiance HDR",
            Format::Farbfeld => "Farbfeld",
        }
    }

    /// Can keep the metadata (the others drop it): EXIF, colour profile and XMP in all five, IPTC
    /// in JPEG, PNG and TIFF (in WebP and AVIF its fields go into the XMP).
    pub fn holds_meta(self) -> bool {
        matches!(self, Format::Jpeg | Format::Png | Format::Webp | Format::Avif | Format::Tiff)
    }

    /// The format a file can be saved back in after a turn ("Save"), from its name (what is
    /// inside, animated or not, is looked at by `save_back::can_turn_in_place`).
    pub fn of_file(name: &str) -> Option<Format> {
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        Format::BACK.into_iter().find(|f| f.ext() == ext || (ext == "jpeg" && *f == Format::Jpeg) || (ext == "tiff" && *f == Format::Tiff))
    }
}

/// Can an image with this name be saved over itself (its format written back)? What is inside
/// is looked at later, by `save_back::can_turn_in_place`.
pub fn saves_back(name: &str) -> bool {
    Format::of_file(name).is_some() || (crate::psd::writable() && crate::psd::is_psd(name))
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
    /// TGA with run-length compression (smaller; a few old programs read only uncompressed).
    pub tga_rle: bool,
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
            tga_rle: true,
            turn: Turn::default(),
        }
    }
}

/// The original, decoded once, with its metadata.
#[derive(Clone)]
pub struct Original {
    /// Already turned upright (EXIF orientation applied).
    pub image: DynamicImage,
    /// EXIF with the orientation set back to "upright", since the pixels are already turned.
    exif: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
    /// Photoshop block, with the IPTC.
    irb: Option<Vec<u8>>,
    /// The segments of a JPEG's extended XMP (`meta_write::jpeg_xmp_ext`).
    xmp_ext: Vec<Vec<u8>>,
    /// A JPEG's quality and subsampling, when they can be read (`jpeg_quality::of`).
    pub jpeg: Option<(u8, Chroma)>,
    /// An Ultra HDR photo opened in Save in another format: what writes it as JPEG still Ultra
    /// HDR (#66). Not used by `encode`, which writes the picture alone.
    pub hdr: Option<std::sync::Arc<crate::ultrahdr::Source>>,
}

/// The metadata of a file being written, made to fit its pixels (`Original::blocks`).
#[derive(Default)]
pub(crate) struct Blocks {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
    pub irb: Option<Vec<u8>>,
    /// A JPEG's extended XMP, as `Original::xmp_ext`; only for a JPEG.
    pub xmp_ext: Vec<Vec<u8>>,
}

impl Original {

    /// `image` with the metadata of the file `bytes` (not decoded again).
    pub fn with_meta(image: DynamicImage, bytes: &[u8]) -> Original {
        Original::with_meta_of(image, bytes, "")
    }

    /// As `with_meta`; the file's `name` tells the formats ImageMagick reads, whose metadata only
    /// ImageMagick gives.
    pub fn with_meta_of(image: DynamicImage, bytes: &[u8], name: &str) -> Original {
        // RAW: the EXIF of every maker (`raw_meta`), the profile of the decoded pixels, the XMP,
        // the IPTC where it is in a TIFF tag.
        if let Some(raw) = crate::raw_meta::read(bytes) {
            let x = crate::meta_write::read(bytes, None);
            return Original::upright(image, raw.exif, raw.icc, raw.xmp.or(x.xmp), x.irb);
        }
        // As found in the formats `image` reads (JPEG, PNG, WebP, TIFF), then in the file itself
        // (`meta_write::read`), then from ImageMagick for the formats it reads.
        let mut d = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok().and_then(|r| r.into_decoder().ok());
        let (exif, icc) = d.as_mut().map(|d| (d.exif_metadata().ok().flatten(), d.icc_profile().ok().flatten())).unwrap_or_default();
        let mut x = crate::meta_write::read(bytes, d.as_mut().map(|d| d as &mut dyn ImageDecoder));
        if crate::decode::is_magick(name) {
            let m = crate::decode::magick_meta(bytes);
            x = crate::meta_write::Extra { xmp: x.xmp.or(m.xmp), irb: x.irb.or(m.irb), icc: x.icc.or(m.icc), exif: x.exif.or(m.exif) };
        }
        let exif = exif.or(x.exif).or_else(|| crate::meta::exif_chunk(bytes));
        let xmp_ext = x.xmp.as_deref().map_or_else(Vec::new, |m| crate::meta_write::jpeg_xmp_ext(bytes, m));
        Original { xmp_ext, jpeg: jpeg_quality::of(bytes), ..Original::upright(image, exif, icc.or(x.icc), x.xmp, x.irb) }
    }

    /// The pixels are already turned upright: the EXIF orientation is set back to "upright".
    fn upright(image: DynamicImage, mut exif: Option<Vec<u8>>, icc: Option<Vec<u8>>, xmp: Option<Vec<u8>>, irb: Option<Vec<u8>>) -> Original {
        if let Some(chunk) = &mut exif {
            let _ = Orientation::remove_from_exif_chunk(chunk);
        }
        Original { image, exif, icc, xmp, irb, xmp_ext: Vec::new(), jpeg: None, hdr: None }
    }

    /// For a picture written without what followed it in the file (Motion Photo video, Ultra HDR
    /// gain map): the XMP that describes those goes (`xmp::alone`).
    pub fn alone(mut self) -> Original {
        let (w, h) = (self.image.width(), self.image.height());
        self.xmp = self.xmp.map(|x| crate::xmp::alone(&x, w, h));
        self
    }

    /// `image` with this one's metadata, without copying these pixels (the editor's Resize).
    pub fn with_image(&self, image: DynamicImage) -> Original {
        let (exif, icc, xmp, irb, xmp_ext) = (self.exif.clone(), self.icc.clone(), self.xmp.clone(), self.irb.clone(), self.xmp_ext.clone());
        Original { image, exif, icc, xmp, irb, xmp_ext, jpeg: self.jpeg, hdr: self.hdr.clone() }
    }

    pub fn has_meta(&self) -> bool {
        self.exif.is_some() || self.icc.is_some() || self.xmp.is_some() || self.irb.is_some()
    }

    /// The metadata for a `format` file of `w` × `h` pixels: sizes set, the old thumbnails out
    /// (they showed the original), the Camera Raw settings out of the XMP (already in the pixels).
    fn blocks(&self, format: Format, w: u32, h: u32) -> Blocks {
        let mut exif = self.exif.clone();
        if let Some(e) = &mut exif {
            crate::meta::fit_exif(e, w, h);
        }
        let irb = self.irb.as_deref().and_then(crate::meta_write::fit_irb);
        // WebP and AVIF have no place for IPTC: its fields go into the XMP.
        let iim = if matches!(format, Format::Webp | Format::Avif) { irb.as_deref().and_then(crate::meta_write::iim) } else { None };
        // A JPEG keeps the extended XMP in segments of its own, as they were (the GUID is the
        // digest of their content); the other formats hold an XMP of any size: it joins the main one.
        let (ext, xmp_ext) = if format == Format::Jpeg { (None, self.xmp_ext.clone()) } else { (crate::meta_write::xmp_ext_packet(&self.xmp_ext), Vec::new()) };
        let xmp = crate::xmp::fit_with(self.xmp.as_deref(), w, h, iim, ext.as_deref());
        Blocks { exif, icc: self.icc.clone(), xmp, irb, xmp_ext }
    }

    /// The file as it would be saved.
    pub fn encode(&self, o: &Options) -> Result<Vec<u8>, String> {
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
        let b = if o.keep_meta { self.blocks(o.format, img.width(), img.height()) } else { Blocks::default() };
        let mut out = Cursor::new(Vec::new());
        match o.format {
            Format::Jpeg => return encoders::jpeg(&img, o, &b),
            Format::Webp => return encoders::webp(&img, o, &b),
            Format::Avif => return encoders::avif(&img, o, &b),
            Format::Tiff => encoders::tiff(&img, o, &b, &mut out).map_err(|e| e.to_string())?,
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
                if let Some(icc) = &b.icc {
                    let _ = e.set_icc_profile(icc.clone());
                }
                if let Some(exif) = &b.exif {
                    let _ = e.set_exif_metadata(exif.clone());
                }
                img.write_with_encoder(e).map_err(|e| e.to_string())?;
                return Ok(crate::meta_write::png_chunks(out.into_inner(), b.xmp.as_deref(), b.irb.as_deref()));
            }
            Format::Bmp => img.write_with_encoder(bmp::BmpEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Gif => img.write_with_encoder(gif::GifEncoder::new_with_speed(&mut out, 31 - o.gif_quality.clamp(1, 30) as i32)).map_err(|e| e.to_string())?,
            Format::Qoi => img.write_with_encoder(qoi::QoiEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Ico => img.write_with_encoder(ico::IcoEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Tga if o.tga_rle => img.write_with_encoder(tga::TgaEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Tga => img.write_with_encoder(tga::TgaEncoder::new(&mut out).disable_rle()).map_err(|e| e.to_string())?,
            // PPM, PGM or, with transparency, PAM: chosen from the pixels.
            Format::Pnm => img.write_with_encoder(pnm::PnmEncoder::new(&mut out).with_dynamic_header()).map_err(|e| e.to_string())?,
            Format::Exr => img.write_with_encoder(openexr::OpenExrEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Hdr => img.write_with_encoder(hdr::HdrEncoder::new(&mut out)).map_err(|e| e.to_string())?,
            Format::Farbfeld => img.write_with_encoder(farbfeld::FarbfeldEncoder::new(&mut out)).map_err(|e| e.to_string())?,
        }
        Ok(out.into_inner())
    }
}

/// Pixels the format can store: JPEG has no transparency (laid on white, not on black), and
/// only PNG and TIFF take 16 bits; floating point (EXR, HDR) becomes 8 bits.
/// The largest side an ICO holds.
pub const ICO_SIDE: u32 = 256;

fn pixels(img: &DynamicImage, format: Format) -> DynamicImage {
    let alpha = img.color().has_alpha();
    let deep = matches!(img, DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_) | DynamicImage::ImageLuma16(_));
    match format {
        // ICO holds at most 256 × 256 pixels: larger images are reduced (the page says so).
        Format::Ico if img.width().max(img.height()) > ICO_SIDE => pixels(&img.resize(ICO_SIDE, ICO_SIDE, image::imageops::FilterType::Lanczos3), format),
        // Always RGBA: readers of icons (`image` too) expect the PNG inside in RGBA.
        Format::Ico => DynamicImage::ImageRgba8(img.to_rgba8()),
        Format::Exr if alpha => DynamicImage::ImageRgba32F(img.to_rgba32f()),
        Format::Exr => DynamicImage::ImageRgb32F(img.to_rgb32f()),
        // No transparency in HDR: on white, as JPEG.
        Format::Hdr => DynamicImage::ImageRgb32F(pixels(img, Format::Jpeg).to_rgb32f()),
        Format::Farbfeld => DynamicImage::ImageRgba16(img.to_rgba16()),
        Format::Pnm if deep && alpha => DynamicImage::ImageRgba16(img.to_rgba16()),
        Format::Pnm if deep => DynamicImage::ImageRgb16(img.to_rgb16()),
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

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashSet;

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
        // XMP with sizes, orientation and Camera Raw settings; a Photoshop block with a thumbnail
        // and IPTC.
        let xmp = crate::xmp::tests::SAMPLE.as_bytes().to_vec();
        let iim = crate::xmp::tests::iim(&[(80, "Ann"), (90, "Québec")]);
        let irb = [crate::meta_write::resource(1036, &[1; 9]), crate::meta_write::irb_of_iim(&iim)].concat();
        let original = Original { image: DynamicImage::ImageRgba8(img), exif: Some(exif.clone()), icc: Some(icc.clone()), xmp: Some(xmp), irb: Some(irb), xmp_ext: Vec::new(), jpeg: None, hdr: None };
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
            Options { format: Format::Ico, ..d },
            Options { format: Format::Tga, ..d },
            Options { format: Format::Tga, tga_rle: false, ..d },
            Options { format: Format::Pnm, ..d },
            Options { format: Format::Exr, ..d },
            Options { format: Format::Hdr, ..d },
            Options { format: Format::Farbfeld, ..d },
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
                    // Colour profile and XMP (with the IPTC fields) read back, EXIF too.
                    let meta = Original::with_meta(back, &bytes);
                    assert_eq!(meta.icc.as_ref(), Some(&icc), "{o:?}");
                    assert!(meta.exif.is_some(), "{o:?}");
                    kept_xmp_and_iptc(&bytes, format, &iim);
                }
                continue;
            }
            // Read as the program reads it (TGA has no signature to be recognised by).
            let back = crate::decode::decode_full(&bytes, &format!("x.{}", format.ext())).unwrap_or_else(|e| panic!("{o:?}: {e}"));
            assert_eq!((back.width(), back.height()), (40, 30), "{o:?}");
            if format == Format::Gif {
                let colours: HashSet<[u8; 4]> = back.to_rgba8().pixels().map(|p| p.0).collect();
                assert!(colours.len() <= 256, "GIF with {} colours", colours.len());
            }
            let lossless = matches!(format, Format::Png | Format::Tiff | Format::Qoi | Format::Ico | Format::Tga | Format::Pnm | Format::Exr | Format::Farbfeld);
            if lossless || (format == Format::Webp && o.webp_lossless) {
                assert_eq!(back.to_rgba8(), original.image.to_rgba8(), "{o:?}");
            }
            if !format.holds_meta() && format != Format::Gif {
                continue;
            }
            let (e, i) = read_meta(&bytes);
            if matches!(format, Format::Jpeg | Format::Png | Format::Webp) {
                assert_eq!(e.as_ref(), Some(&exif), "{o:?}");
            }
            if matches!(format, Format::Jpeg | Format::Png | Format::Webp | Format::Tiff) {
                assert_eq!(i.as_ref(), Some(&icc), "{o:?}");
                kept_xmp_and_iptc(&bytes, format, &iim);
            }
            let bare = original.encode(&Options { keep_meta: false, ..o }).unwrap();
            if format != Format::Gif {
                assert_eq!(read_meta(&bare), (None, None), "{o:?}");
                let back = Original::with_meta(original.image.clone(), &bare);
                assert!(back.xmp.is_none() && back.irb.is_none(), "{o:?}");
            }
        }
    }

    /// XMP and IPTC of a file written from `every_format`'s original, read back as the program
    /// reads them: sizes of the pixels, no Camera Raw, the author kept; the IPTC without the
    /// thumbnail, or in the XMP where the format has no IPTC (WebP, AVIF).
    pub(crate) fn kept_xmp_and_iptc(bytes: &[u8], format: Format, iim: &[u8]) {
        let back = Original::with_meta(DynamicImage::new_rgb8(1, 1), bytes);
        let xmp = kept_xmp_like(bytes, 40, 30);
        if matches!(format, Format::Webp | Format::Avif) {
            assert!(xmp.contains("<photoshop:City>Québec</photoshop:City>"), "{format:?}: {xmp}");
            assert!(back.irb.is_none(), "{format:?}");
        } else {
            assert_eq!(back.irb.as_deref(), Some(crate::meta_write::irb_of_iim(iim).as_slice()), "{format:?}");
        }
    }

    /// The XMP of `xmp::tests::SAMPLE` in a written file: sizes `w` × `h`, upright, no Camera Raw
    /// nor thumbnail, the author kept.
    pub(crate) fn kept_xmp_like(bytes: &[u8], w: u32, h: u32) -> String {
        let back = Original::with_meta(DynamicImage::new_rgb8(1, 1), bytes);
        let xmp = String::from_utf8(back.xmp.clone().expect("no XMP")).unwrap();
        for kept in [format!("exif:PixelXDimension=\"{w}\""), format!("<exif:PixelYDimension>{h}</exif:PixelYDimension>"), "tiff:Orientation=\"1\"".into(), "Ann &lt;Author&gt;".into()] {
            assert!(xmp.contains(&kept), "{kept} not in {xmp}");
        }
        assert!(!xmp.contains("crs:") && !xmp.contains("Thumbnails"), "{xmp}");
        xmp
    }

    /// The extended XMP of a JPEG: written again as its segments in a JPEG, put into the main
    /// packet in the other formats (without the GUID, which would point to nothing).
    #[test]
    fn extended_xmp() {
        let guid = "0123456789ABCDEF0123456789ABCDEF";
        let rdf = |body: &str| format!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">{body}</rdf:RDF></x:xmpmeta>");
        let main = rdf(&format!("<rdf:Description rdf:about=\"\" xmlns:xmpNote=\"http://ns.adobe.com/xmp/note/\" xmpNote:HasExtendedXMP=\"{guid}\"/>"));
        let ext = rdf(&format!("<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:source>{}</dc:source></rdf:Description>", "a".repeat(100)));
        let (a, b) = ext.as_bytes().split_at(60);
        let piece = |offset: u32, data: &[u8]| [guid.as_bytes(), &(ext.len() as u32).to_be_bytes(), &offset.to_be_bytes(), data].concat();
        let segments = vec![piece(0, a), piece(60, b)];
        let original = Original { xmp: Some(main.into_bytes()), xmp_ext: segments.clone(), ..Original::upright(DynamicImage::new_rgb8(8, 6), None, None, None, None) };
        let jpeg = original.encode(&Options { format: Format::Jpeg, ..Default::default() }).unwrap();
        let back = Original::with_meta(DynamicImage::new_rgb8(8, 6), &jpeg);
        assert_eq!(back.xmp_ext, segments);
        let png = back.encode(&Options { format: Format::Png, ..Default::default() }).unwrap();
        let xmp = String::from_utf8(Original::with_meta(DynamicImage::new_rgb8(1, 1), &png).xmp.unwrap()).unwrap();
        assert!(xmp.contains(&format!("<dc:source>{}</dc:source>", "a".repeat(100))) && !xmp.contains(guid), "{xmp}");
        // Pieces that do not make up the length they declare are not put together.
        assert!(crate::meta_write::xmp_ext_packet(&segments[..1]).is_none());
    }

    /// A TIFF keeps the EXIF in its own directories: camera, shot and GPS fields read back.
    #[test]
    fn tiff_exif() {
        use exif::{Field, In, Tag, Value};
        let mut w = exif::experimental::Writer::new();
        let fields = [
            Field { tag: Tag::Make, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"Maker".to_vec()]) },
            Field { tag: Tag::DateTimeOriginal, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"2024:01:02 03:04:05".to_vec()]) },
            Field { tag: Tag::FNumber, ifd_num: In::PRIMARY, value: Value::Rational(vec![(28, 10).into()]) },
            Field { tag: Tag::GPSLatitudeRef, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"N".to_vec()]) },
            Field { tag: Tag::Orientation, ifd_num: In::PRIMARY, value: Value::Short(vec![1]) },
        ];
        for f in &fields {
            w.push_field(f);
        }
        for little in [false, true] {
            let mut exif = Cursor::new(Vec::new());
            w.write(&mut exif, little).unwrap();
            let original = Original { image: DynamicImage::new_rgb8(8, 6), exif: Some(exif.into_inner()), icc: None, xmp: None, irb: None, xmp_ext: Vec::new(), jpeg: None, hdr: None };
            let bytes = original.encode(&Options { format: Format::Tiff, ..Default::default() }).unwrap();
            let back = exif::Reader::new().read_from_container(&mut Cursor::new(&bytes)).unwrap();
            for f in &fields {
                assert_eq!(back.get_field(f.tag, In::PRIMARY).map(|b| format!("{:?}", b.value)), Some(format!("{:?}", f.value)), "{:?}", f.tag);
            }
            assert_eq!(crate::decode::decode_full(&bytes, "x.tif").unwrap().width(), 8);
        }
    }

    /// ICO holds at most 256 × 256 pixels: a larger image is reduced, keeping its shape. The
    /// formats only "Save as…" writes are not saved back over an original.
    #[test]
    fn ico_and_save_back() {
        let original = Original { image: DynamicImage::ImageRgb8(image::RgbImage::new(600, 300)), exif: None, icc: None, xmp: None, irb: None, xmp_ext: Vec::new(), jpeg: None, hdr: None };
        let bytes = original.encode(&Options { format: Format::Ico, ..Default::default() }).unwrap();
        let back = crate::decode::decode_full(&bytes, "x.ico").unwrap();
        assert_eq!((back.width(), back.height()), (256, 128));
        assert_eq!(Format::of_file("a.TIFF"), Some(Format::Tiff));
        for name in ["a.ico", "a.tga", "a.pnm", "a.exr", "a.hdr", "a.ff"] {
            assert_eq!(Format::of_file(name), None, "{name}");
        }
    }
}
