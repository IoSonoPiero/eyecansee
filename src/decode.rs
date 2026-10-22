//! Decoding: one dedicated decoder per family of formats, chosen from the extension,
//! and ImageMagick as a fallback when the dedicated decoder fails or does not exist.

use std::error::Error;
use std::io::Cursor;
use std::sync::{Arc, OnceLock};

use eframe::egui::ColorImage;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage, imageops::FilterType};
use rawler::decoders::RawDecodeParams;
use rawler::imgop::develop::RawDevelop;
use rawler::rawsource::RawSource;
use resvg::{tiny_skia, usvg};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

// ponytail: 8192 is supported by any recent GPU; viewing larger photos at 100% needs tiling
const MAX_SIDE: u32 = 8192;

/// Vector images without a useful size (24 px icons) are rasterised at this long side.
const SVG_SIDE: f32 = 2048.0;

const HEIF: &[&str] = &["heic", "heif", "hif", "avif"];
const SVG: &[&str] = &["svg", "svgz"];

/// Rare formats with a decoder written in Rust (`image-extras`, `psd`, `hayro-jpeg2000`): they
/// work on Windows too, and on Linux they come before ImageMagick, which stays the fallback.
const EXTRA: &[&str] = &[
    "psd", "ora", "pcx", "sgi", "rgb", "rgba", "bw", "wbmp", "xbm", "xpm", "otb", "dds", "icns",
    "jp2", "j2k", "j2c", "jpf", "jpx",
];
const JPEG2000: &[&str] = &["jp2", "j2k", "j2c", "jpf", "jpx"];

/// Extensions of formats that `image` reads under another name: cursors are icons, MPO
/// (multi-picture) and JPS (stereo) start with an ordinary JPEG, which is the one shown.
fn image_alias(ext: &str) -> &str {
    match ext {
        "cur" => "ico",
        "mpo" | "jps" => "jpg",
        e => e,
    }
}

/// Formats handed directly to ImageMagick, checked for reading with `magick -list format`
/// (ImageMagick 7.1.2, 2026-10-19). Documents (PDF, EPS) and non-photographic formats excluded.
const MAGICK: &[&str] = &[
    "psb", "xcf", "ras", "sun", "pict", "pct", "miff", "mng", "jng", "dpx", "cin", "fits", "xwd",
    "pfm", "pgx", "wpg", "viff", "vicar",
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Image,
    Jxl,
    Heif,
    Svg,
    Raw,
    Extra,
    Magick,
}

fn ext(name: &str) -> Option<String> {
    Some(name.rsplit_once('.')?.1.to_lowercase())
}

fn kind(name: &str) -> Option<Kind> {
    let ext = ext(name)?;
    let e = ext.as_str();
    Some(if e == "jxl" {
        Kind::Jxl
    } else if cfg!(feature = "heif") && HEIF.contains(&e) {
        Kind::Heif
    } else if SVG.contains(&e) {
        Kind::Svg
    } else if rawler::decoders::supported_extensions().iter().any(|r| r.eq_ignore_ascii_case(e)) {
        Kind::Raw
    } else if image::ImageFormat::from_extension(image_alias(e)).is_some_and(|f| f.reading_enabled()) {
        Kind::Image
    } else if EXTRA.contains(&e) {
        Kind::Extra
    } else if cfg!(feature = "magick") && MAGICK.contains(&e) {
        Kind::Magick
    } else {
        return None;
    })
}

pub fn is_image(name: &str) -> bool {
    kind(name).is_some()
}

/// Recognises from the content the formats that `image` cannot identify by itself.
fn sniff(bytes: &[u8]) -> Option<Kind> {
    const HEIF_BRANDS: &[&[u8]] = &[b"heic", b"heix", b"hevc", b"heim", b"heis", b"mif1", b"msf1", b"avif", b"avis"];
    if bytes.get(4..8) == Some(b"ftyp") && bytes.get(8..12).is_some_and(|b| HEIF_BRANDS.contains(&b)) {
        Some(Kind::Heif)
    } else if bytes.starts_with(&[0xFF, 0x0A]) || bytes.starts_with(b"\0\0\0\x0CJXL ") {
        Some(Kind::Jxl)
    } else {
        None
    }
}

fn run(kind: Kind, bytes: &[u8], name: &str) -> Result<DynamicImage> {
    match kind {
        Kind::Image => with_image(bytes, name),
        Kind::Jxl => jxl(bytes),
        Kind::Heif => heif(bytes),
        Kind::Svg => svg(bytes),
        Kind::Raw => raw(bytes),
        Kind::Extra => extra(bytes, name),
        Kind::Magick => magick(bytes),
    }
}

/// Image for the viewer, scaled down only beyond the texture limit.
/// JPEGs go through libjpeg-turbo, ~18% faster than `image` (measured on UHD photos); the formats
/// it does not decode to RGB (CMYK) fall back to `image`.
pub fn decode(bytes: &[u8], name: &str) -> std::result::Result<ColorImage, String> {
    let turbo = bytes.starts_with(&[0xFF, 0xD8, 0xFF]).then(|| jpeg_scaled(bytes, u32::MAX).ok()).flatten();
    let mut img = match turbo {
        Some(img) => img,
        None => decode_any(bytes, name)?,
    };
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, FilterType::Triangle);
    }
    Ok(to_color_image(img))
}

/// Thumbnail for the grid, long side at most `side`.
/// JPEGs are decoded already scaled down (down to 1/8): measured on UHD photos, full decoding
/// was 80% of the time.
// ponytail: other formats are decoded in full; embedded previews of RAW and HEIC when needed
pub fn thumbnail(bytes: &[u8], name: &str, side: u32) -> std::result::Result<DynamicImage, String> {
    let small = bytes.starts_with(&[0xFF, 0xD8, 0xFF]).then(|| jpeg_scaled(bytes, side).ok()).flatten();
    let mut img = match small {
        Some(img) => img,
        None => decode_any(bytes, name)?,
    };
    if img.width().max(img.height()) > side {
        img = img.thumbnail(side, side);
    }
    Ok(img)
}

fn decode_any(bytes: &[u8], name: &str) -> std::result::Result<DynamicImage, String> {
    let kind = kind(name).unwrap_or(Kind::Image);
    // Wrong extension (e.g. an iPhone HEIC saved as .jpg): look at the content, then try
    // ImageMagick. The error shown is still the one from the decoder chosen by the extension.
    let img = run(kind, bytes, name)
        .or_else(|e| match sniff(bytes) {
            Some(k) if k != kind => run(k, bytes, name).map_err(|_| e),
            _ => Err(e),
        })
        .or_else(|e| magick(bytes).map_err(|_| e))
        .map_err(|e| e.to_string())?;
    Ok(img)
}

/// Without an RGBA copy in between: on a UHD photo the copy took as long as a third of the decoding.
pub fn to_color_image(img: DynamicImage) -> ColorImage {
    let size = [img.width() as usize, img.height() as usize];
    match img {
        DynamicImage::ImageRgb8(rgb) => ColorImage::from_rgb(size, rgb.as_raw()),
        img => ColorImage::from_rgba_unmultiplied(size, img.into_rgba8().as_raw()),
    }
}

/// Decodes applying the EXIF orientation, shared by `image` and `jxl-oxide`.
fn oriented(mut decoder: impl ImageDecoder) -> Result<DynamicImage> {
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn with_image(bytes: &[u8], name: &str) -> Result<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    // Formats without a signature (TGA): the extension counts.
    if reader.format().is_none()
        && let Some(f) = ext(name).and_then(|e| image::ImageFormat::from_extension(image_alias(&e)))
    {
        reader.set_format(f);
    }
    oriented(reader.into_decoder()?)
}

/// JPEG decoded with libjpeg-turbo at the smallest scale (from 1/8 up) whose long side is
/// still ≥ `side`. CMYK and the like fail and fall back to normal decoding.
fn jpeg_scaled(bytes: &[u8], side: u32) -> Result<DynamicImage> {
    let mut dec = turbojpeg::Decompressor::new()?;
    // Defence against progressive JPEGs crafted to stall the decoder (files from FTP, phones, cloud).
    dec.set_scan_limit(500)?;
    let header = dec.read_header(bytes)?;
    let mut factors = turbojpeg::Decompressor::supported_scaling_factors();
    factors.sort_by_key(|f| f.num() * 1000 / f.denom());
    let factor = factors
        .into_iter()
        .find(|f| f.scale(header.width.max(header.height)) >= side as usize)
        .unwrap_or(turbojpeg::ScalingFactor::ONE);
    dec.set_scaling_factor(factor)?;
    let size = header.scaled(factor);
    let mut out = turbojpeg::Image {
        pixels: vec![0; size.width * size.height * 3],
        width: size.width,
        pitch: size.width * 3,
        height: size.height,
        format: turbojpeg::PixelFormat::RGB,
    };
    dec.decompress(bytes, out.as_deref_mut())?;
    let rgb = image::RgbImage::from_raw(size.width as u32, size.height as u32, out.pixels).ok_or("JPEG: buffer")?;
    let mut img = DynamicImage::ImageRgb8(rgb);
    // libjpeg-turbo does not read EXIF: the orientation comes from `image`, which parses only the headers.
    if let Ok(o) = image::codecs::jpeg::JpegDecoder::new(Cursor::new(bytes)).and_then(|mut d| d.orientation()) {
        img.apply_orientation(o);
    }
    Ok(img)
}

fn jxl(bytes: &[u8]) -> Result<DynamicImage> {
    oriented(jxl_oxide::integration::JxlDecoder::new(Cursor::new(bytes))?)
}

/// HEIC and AVIF. libheif applies the container's rotations and mirroring by itself.
#[cfg(feature = "heif")]
fn heif(bytes: &[u8]) -> Result<DynamicImage> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let ctx = HeifContext::read_from_bytes(bytes)?;
    let handle = ctx.primary_image_handle()?;
    let image = LibHeif::new().decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;
    let plane = image.planes().interleaved.ok_or("HEIF without an RGBA plane")?;
    let (w, h, stride) = (plane.width, plane.height, plane.stride);
    let row = w as usize * 4;
    let data = plane.data.chunks(stride).take(h as usize).flat_map(|r| &r[..row]).copied().collect();
    Ok(DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, data).ok_or("HEIF: inconsistent dimensions")?))
}

fn svg(bytes: &[u8]) -> Result<DynamicImage> {
    // Loading the system fonts is expensive: only once for the whole session.
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    });
    let opt = usvg::Options { fontdb: fontdb.clone(), ..Default::default() };
    let tree = usvg::Tree::from_data(bytes, &opt)?;
    let size = tree.size();
    let scale = SVG_SIDE / size.width().max(size.height());
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("SVG: invalid dimensions")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let data = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Ok(DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, data.collect()).ok_or("SVG: buffer")?))
}

/// App icon (`assets/eyecansee.svg`, the same as on Linux) in RGBA, `side`×`side`. Without the
/// system fonts, which the icon does not use: at startup they would cost time.
pub fn app_icon(side: u32) -> Option<RgbaImage> {
    let tree = usvg::Tree::from_data(include_bytes!("../assets/eyecansee.svg"), &usvg::Options::default()).ok()?;
    let scale = side as f32 / tree.size().width().max(tree.size().height());
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let data = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    RgbaImage::from_raw(side, side, data.collect())
}

/// RAW: the full-resolution preview embedded by the camera if present (fast, camera colours),
/// otherwise full development of the sensor data.
fn raw(bytes: &[u8]) -> Result<DynamicImage> {
    let src = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&src)?;
    let params = RawDecodeParams::default();
    let mut img = match decoder.full_image(&src, &params) {
        Ok(Some(img)) => img,
        _ => {
            let raw = decoder.raw_image(&src, &params, false)?;
            RawDevelop::default().develop_intermediate(&raw)?.to_dynamic_image().ok_or("RAW: development failed")?
        }
    };
    let exif = decoder.raw_metadata(&src, &params).ok().and_then(|m| m.exif.orientation);
    if let Some(o) = exif.and_then(|o| Orientation::from_exif(o as u8)) {
        img.apply_orientation(o);
    }
    Ok(img)
}

fn extra(bytes: &[u8], name: &str) -> Result<DynamicImage> {
    use image_extras::*;
    let e = ext(name).unwrap_or_default();
    let r = Cursor::new(bytes);
    Ok(match e.as_str() {
        "psd" => {
            // The composite image saved by Photoshop ("maximize compatibility"), not the layers.
            // Without that option the composite can be blank (all white, with the only layer
            // marked hidden: Regeneration.psd, 2016): then the layers are merged, all of them if
            // none is visible. The result matched ImageMagick's pixel for pixel.
            // `psd` 0.3.5 panics on what it does not support (e.g. ZIP-compressed layers, some
            // channel layouts): an error instead, and on Linux ImageMagick takes over.
            let (w, h, rgba) = std::panic::catch_unwind(|| {
                psd::Psd::from_bytes(bytes).map(|p| {
                    let mut rgba = p.rgba();
                    if !p.layers().is_empty() && rgba.chunks(4).all(|c| c == &rgba[..4]) {
                        let any_visible = p.layers().iter().any(|l| l.visible());
                        if let Ok(merged) = p.flatten_layers_rgba(&|(_, l)| l.visible() || !any_visible) {
                            rgba = merged;
                        }
                    }
                    (p.width(), p.height(), rgba)
                })
            })
            .map_err(|_| "PSD: unsupported variant")??;
            DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, rgba).ok_or("PSD: buffer")?)
        }
        e if JPEG2000.contains(&e) => {
            DynamicImage::from_decoder(hayro_jpeg2000::Image::new(bytes, &Default::default())?)?
        }
        "ora" => DynamicImage::from_decoder(ora::OpenRasterDecoder::with_limits(r, image::Limits::default())?)?,
        "pcx" => DynamicImage::from_decoder(pcx::PCXDecoder::new(r)?)?,
        "sgi" | "rgb" | "rgba" | "bw" => DynamicImage::from_decoder(sgi::SgiDecoder::new(r)?)?,
        "wbmp" => DynamicImage::from_decoder(wbmp::WbmpDecoder::new(r)?)?,
        "xbm" => DynamicImage::from_decoder(xbm::XbmDecoder::new(r)?)?,
        "xpm" => DynamicImage::from_decoder(xpm::XpmDecoder::new(r)?)?,
        "otb" => DynamicImage::from_decoder(otb::OtbDecoder::new(r)?)?,
        "dds" => DynamicImage::from_decoder(dds::DdsDecoder::new(r)?)?,
        "icns" => DynamicImage::from_decoder(icns::IcnsDecoder::new(r)?)?,
        _ => return Err(format!("{e}: no decoder").into()),
    })
}

/// Universal fallback. Recognises the format from the content: formats without a signature
/// (e.g. TGA) only get here if their dedicated decoder already exists.
#[cfg(feature = "magick")]
fn magick(bytes: &[u8]) -> Result<DynamicImage> {
    use magick_rust::{MagickWand, magick_wand_genesis};
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(magick_wand_genesis);
    let wand = MagickWand::new();
    wand.read_image_blob(bytes)?;
    // Files with several images (GIF, MNG, layered PSD): the first is shown.
    wand.set_first_iterator();
    wand.auto_orient();
    let (w, h) = (wand.get_image_width(), wand.get_image_height());
    let data = wand.export_image_pixels(0, 0, w, h, "RGBA").ok_or("ImageMagick: pixel export failed")?;
    Ok(DynamicImage::ImageRgba8(RgbaImage::from_raw(w as u32, h as u32, data).ok_or("ImageMagick: buffer")?))
}

// Builds without libheif or ImageMagick (features `heif`, `magick`), for example the first one on
// Windows: those formats stay out of the list and a file recognised from its content gives this error.
#[cfg(not(feature = "heif"))]
fn heif(_: &[u8]) -> Result<DynamicImage> {
    Err("HEIF/AVIF: not included in this build".into())
}

#[cfg(not(feature = "magick"))]
fn magick(_: &[u8]) -> Result<DynamicImage> {
    Err("ImageMagick: not included in this build".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoders() {
        assert!(is_image("a.JXL") && is_image("c.CR3"));
        assert_eq!(is_image("b.heic"), cfg!(feature = "heif"));
        assert!(is_image("d.psd") && is_image("g.pcx") && is_image("h.jp2") && is_image("i.cur"));
        assert_eq!(is_image("d.xcf"), cfg!(feature = "magick"));
        assert!(!is_image("e.txt") && !is_image("f.pdf") && !is_image("no_extension"));

        let svg_src = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;
        let img = decode(svg_src, "x.svg").unwrap();
        assert_eq!(img.size, [2048, 1024]);
        assert_eq!(img.pixels[0], eframe::egui::Color32::RED);

        let mut png = Vec::new();
        RgbaImage::new(3, 2).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        // Wrong extension: the PNG content is recognised anyway.
        assert_eq!(decode(&png, "photo.heic").unwrap().size, [3, 2]);

        // TGA has no signature: the extension must be enough.
        let mut tga = Vec::new();
        RgbaImage::new(4, 1).write_to(&mut Cursor::new(&mut tga), image::ImageFormat::Tga).unwrap();
        assert_eq!(decode(&tga, "x.tga").unwrap().size, [4, 1]);

        assert!(decode(b"I am not an image", "x.jpg").is_err());
        assert_eq!(thumbnail(&png, "x.png", 2).unwrap().width(), 2);
    }

    /// Regenerates `assets/eyecansee.ico` (icon of the Windows executable, see `build.rs`) from the
    /// SVG, to be run again if the SVG changes: `cargo test --no-default-features make_ico -- --ignored`.
    #[test]
    #[ignore]
    fn make_ico() {
        use image::codecs::ico::{IcoEncoder, IcoFrame};
        let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256]
            .into_iter()
            .map(|side| {
                let img = app_icon(side).unwrap();
                IcoFrame::as_png(img.as_raw(), side, side, image::ExtendedColorType::Rgba8).unwrap()
            })
            .collect();
        let file = std::fs::File::create(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/eyecansee.ico")).unwrap();
        IcoEncoder::new(file).encode_images(&frames).unwrap();
    }

    /// Where thumbnail time goes, on one thread:
    /// `EYECANSEE_SAMPLES=/folder cargo test --release thumb_timing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn thumb_timing() {
        use std::time::{Duration, Instant};
        let dir = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES not set");
        let (mut n, mut dec, mut resize, mut fast) = (0, Duration::ZERO, Duration::ZERO, Duration::ZERO);
        for p in std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()) {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let Ok(bytes) = std::fs::read(&p) else { continue };
            let t = Instant::now();
            let Ok(img) = decode_any(&bytes, &name) else { continue };
            dec += t.elapsed();
            let t = Instant::now();
            let small = img.thumbnail(256, 256);
            resize += t.elapsed();
            let t = Instant::now();
            let thumb = thumbnail(&bytes, &name, 256).unwrap();
            fast += t.elapsed();
            assert_eq!((thumb.width(), thumb.height()), (small.width(), small.height()), "{name}");
            n += 1;
        }
        let n = n.max(1);
        println!("{n} files, per file: full decoding {:?} + resize {:?}; thumbnail() {:?}", dec / n, resize / n, fast / n);
    }

    /// Manual check on real files:
    /// `EYECANSEE_SAMPLES=/folder cargo test samples -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn samples() {
        let dir = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES not set");
        let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        paths.sort();
        for p in paths {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let t = std::time::Instant::now();
            match decode(&std::fs::read(&p).unwrap(), &name) {
                Ok(img) => println!("OK   {name:28} {:?} {:?}", img.size, t.elapsed()),
                Err(e) => println!("ERR  {name:28} {e}"),
            }
        }
    }
}
