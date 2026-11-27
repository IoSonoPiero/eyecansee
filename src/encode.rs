//! Encoding an image again in another format, without the interface: every option each encoder
//! offers is there: libjpeg-turbo for JPEG, `image` for PNG, GIF, BMP and QOI, libwebp for WebP,
//! ravif for AVIF, `tiff` for TIFF, `image` for ICO, TGA, PNM, OpenEXR, Radiance HDR and Farbfeld. Writing back over the original: `save_back`.


pub mod density;
pub mod jfif;
pub mod jpeg_quality;
pub mod texts;
use std::io::Cursor;

use image::codecs::{bmp, farbfeld, gif, hdr, ico, openexr, png, pnm, qoi, tga};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageReader};

use crate::encoders;
use crate::meta_strip::Keep;
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

    /// Of the metadata, keeps only the pixel density (with a unit), in its header: BMP. The
    /// other formats outside `holds_meta` keep nothing.
    pub fn holds_density_only(self) -> bool {
        self == Format::Bmp
    }

    /// The Fluent id of the "Keep the metadata" box for this format: it names what is kept.
    pub fn meta_label(self) -> &'static str {
        if self.holds_density_only() { "convert-keep-density" } else { "convert-keep-meta" }
    }

    /// The format a file can be saved back in after a turn ("Save"), from its name (what is
    /// inside, animated or not, is looked at by `save_back::can_turn_in_place`). The other names
    /// of a JPEG (`.jpeg`, `.jfif`, `.jpe`) and of a TIFF (`.tiff`) count as theirs.
    pub fn of_file(name: &str) -> Option<Format> {
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        let ext = match ext.as_str() {
            "jpeg" | "jfif" | "jpe" => "jpg",
            "tiff" => "tif",
            e => e,
        };
        Format::BACK.into_iter().find(|f| f.ext() == ext)
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
    /// What the new file keeps of the original's metadata (#55).
    pub meta: Keep,
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
            meta: Keep::All,
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
    /// The pixel density and its unit (`density::read`), written into every file made from it
    /// that can hold one.
    density: Option<density::Density>,
    /// An Ultra HDR photo opened in Save in another format: what writes it as JPEG still Ultra
    /// HDR (#66). Not used by `encode`, which writes the picture alone.
    pub hdr: Option<std::sync::Arc<crate::ultrahdr::Source>>,
    /// The XMP of the sidecar of a photo that is not RAW (#24), as read: for the copies written
    /// from the file itself rather than from these blocks (an Ultra HDR photo, a lossless turn),
    /// which join it to the file's XMP (`xmp::joined`).
    pub sidecar: Option<Vec<u8>>,
    /// What was read differently from a 3D model's file (`Mesh::warnings`): Save as… says the
    /// new file has it so (#179). Not written into the file.
    pub notes: Vec<&'static str>,
    /// The texts of a PNG and its colour chunks, the comments of a JPEG or a GIF (#218).
    texts: texts::Texts,
}

/// The metadata of a file being written, made to fit its pixels (`Original::blocks`).
#[derive(Default)]
pub(crate) struct Blocks {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
    pub irb: Option<Vec<u8>>,
    /// The original's pixel density, turned with the pixels; each encoder writes it where its
    /// format keeps it (the EXIF already has it).
    pub density: Option<density::Density>,
    /// The text chunks of a PNG for a PNG, its colour chunks (`texts::Texts`).
    pub png_text: Vec<([u8; 4], Vec<u8>)>,
    pub png_color: Vec<([u8; 4], Vec<u8>)>,
    /// The comments of a JPEG for a JPEG.
    pub jpeg_comments: Vec<Vec<u8>>,
}

impl Original {

    /// `image` with the metadata of the file `bytes` (not decoded again).
    pub fn with_meta(image: DynamicImage, bytes: &[u8]) -> Original {
        Original::with_meta_of(image, bytes, "")
    }

    /// As `with_meta`; the file's `name` tells the formats ImageMagick reads, whose metadata only
    /// ImageMagick gives.
    pub fn with_meta_of(image: DynamicImage, bytes: &[u8], name: &str) -> Original {
        Original::with_meta_beside(image, bytes, name, None)
    }

    /// As `with_meta_of`, with the XMP of the photo's `sidecar` (#24): for a RAW photo in place of
    /// the file's own, as Lightroom reads it, and its position in the EXIF; for another photo
    /// added to the file's own, its values first, as digiKam reads it (`xmp::with_sidecar`).
    pub fn with_meta_beside(image: DynamicImage, bytes: &[u8], name: &str, sidecar: Option<&[u8]>) -> Original {
        // RAW: the EXIF of every maker (`raw_meta`), the profile of the decoded pixels, the XMP,
        // the IPTC where it is in a TIFF tag. Only for a RAW file by its name, as it is decoded:
        // rawler also takes for a RAW a TIFF with a camera's make and model (#215).
        let place = sidecar.map(crate::xmp_fields::read).and_then(|f| Some((f.gps?, f.altitude)));
        if let Some(raw) = crate::decode::is_raw(name).then(|| crate::raw_meta::read_placed(bytes, place)).flatten() {
            let x = crate::meta_write::read(bytes, None);
            let density = density::read(bytes, raw.exif.as_deref(), x.irb.as_deref());
            // Its Camera Raw settings (and darktable's history) are not in the pixels EyeCanSee
            // develops: they go.
            let xmp = sidecar.map(<[u8]>::to_vec).or(raw.xmp).or(x.xmp).map(|m| crate::xmp::without_camera_raw(&m, image.width(), image.height()));
            return Original { density, ..Original::upright(image, raw.exif, raw.icc, xmp, x.irb) };
        }
        // As found in the formats `image` reads (JPEG, PNG, WebP, TIFF), then in the file itself
        // (`meta_write::read`, JPEG XL too), then from ImageMagick for the formats it reads.
        let mut d = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok().and_then(|r| r.into_decoder().ok());
        let (exif, icc) = d.as_mut().map(|d| (d.exif_metadata().ok().flatten(), d.icc_profile().ok().flatten())).unwrap_or_default();
        let mut x = crate::meta_write::read(bytes, d.as_mut().map(|d| d as &mut dyn ImageDecoder));
        if crate::decode::is_magick(name) {
            let m = crate::decode::magick_meta(bytes);
            x = crate::meta_write::Extra { xmp: x.xmp.or(m.xmp), irb: x.irb.or(m.irb), icc: x.icc.or(m.icc), exif: x.exif.or(m.exif) };
        }
        let exif = exif.or(x.exif).or_else(|| crate::meta::exif_chunk(bytes));
        // The sidecar's position in the EXIF too, as for a RAW photo: one place in the copy.
        let exif = match (exif, place) {
            (Some(e), Some(p)) => Some(crate::exif_place::placed(&e, p).unwrap_or(e)),
            (e, _) => e,
        };
        let xmp_ext = x.xmp.as_deref().map_or_else(Vec::new, |m| crate::meta_write::jpeg_xmp_ext(bytes, m));
        // The sidecar's development settings (darktable's history) are not in these pixels either.
        if let Some(side) = sidecar {
            x.xmp = Some(crate::xmp::joined(x.xmp.as_deref(), side, image.width(), image.height()));
        }
        let density = density::read(bytes, exif.as_deref(), x.irb.as_deref());
        let sidecar = sidecar.map(<[u8]>::to_vec);
        let texts = texts::read(bytes);
        Original { xmp_ext, jpeg: jpeg_quality::of(bytes), density, sidecar, texts, ..Original::upright(image, exif, icc.or(x.icc), x.xmp, x.irb) }
    }

    /// A picture made here (a video's frame, #16) with the EXIF made for it.
    pub fn with_exif(image: DynamicImage, exif: Option<Vec<u8>>) -> Original {
        Original::upright(image, exif, None, None, None)
    }

    /// The pixels are already turned upright: the EXIF orientation is set back to "upright".
    fn upright(image: DynamicImage, mut exif: Option<Vec<u8>>, icc: Option<Vec<u8>>, xmp: Option<Vec<u8>>, irb: Option<Vec<u8>>) -> Original {
        if let Some(chunk) = &mut exif {
            let _ = Orientation::remove_from_exif_chunk(chunk);
        }
        Original { image, exif, icc, xmp, irb, xmp_ext: Vec::new(), jpeg: None, density: None, hdr: None, sidecar: None, notes: Vec::new(), texts: texts::Texts::default() }
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
        let texts = self.texts.clone();
        Original { image, exif, icc, xmp, irb, xmp_ext, jpeg: self.jpeg, density: self.density, hdr: self.hdr.clone(), sidecar: self.sidecar.clone(), notes: self.notes.clone(), texts }
    }

    /// The pixels were turned by `turn` after this was made (the editor, Convert and resize):
    /// a quarter turn exchanges the density across and down.
    pub fn turned(mut self, turn: Turn) -> Original {
        if turn.sideways() {
            self.density = self.density.map(density::Density::swapped);
        }
        self.upside(turn)
    }

    /// The pixels were turned by `turn`: upside down, the Photo Sphere tags of a panorama go
    /// (#188, `xmp::without_pano`); turned on its side, its size says it (`xmp_pano`).
    pub fn upside(mut self, turn: Turn) -> Original {
        if turn.upside_down() {
            self.xmp = self.xmp.map(|x| crate::xmp::without_pano(&x));
        }
        self
    }

    /// Has something "Keep the metadata" keeps: a pixel density alone counts too, so that it can
    /// be left out.
    pub fn has_meta(&self) -> bool {
        self.exif.is_some() || self.icc.is_some() || self.xmp.is_some() || self.irb.is_some() || self.density.is_some() || !self.texts.is_empty()
    }

    /// Records the place where the photo was taken, in a `format` file: a position in the EXIF, or
    /// a place that "All but the place" would take out of the XMP or the IPTC. An XMP that cannot
    /// be read counts, as that choice leaves it out whole.
    pub fn has_place(&self, format: Format) -> bool {
        if self.exif.as_deref().is_some_and(crate::meta_strip::exif_has_position) {
            return true;
        }
        let (w, h) = (self.image.width(), self.image.height());
        let (all, other) = (self.blocks(format, w, h, false, Keep::All), self.blocks(format, w, h, false, Keep::NoPlace));
        (all.xmp, all.irb) != (other.xmp, other.irb)
    }

    /// "Keep the metadata" for a `format` file: whether it can be changed (something of this
    /// original would be kept) and the Fluent id of what pointing at it says. A BMP keeps a
    /// density with a unit and nothing else.
    pub fn meta_box(&self, format: Format) -> (bool, Option<&'static str>) {
        if !self.has_meta() {
            (false, Some("convert-no-meta"))
        } else if format.holds_meta() {
            (true, None)
        } else if format.holds_density_only() {
            (self.density.is_some_and(|d| d.per_metre_counts().is_some()), Some("convert-meta-density-only"))
        } else {
            (false, Some("convert-meta-unsupported"))
        }
    }

    /// The metadata for a `format` file of `w` × `h` pixels: sizes set, the old thumbnails out
    /// (they showed the original), the Camera Raw settings of the XMP marked as already in the
    /// pixels (`xmp::fix`). As `keep` says (#55): without the place, the GPS of the EXIF and the
    /// place in the XMP and the IPTC go (an XMP that cannot be read goes whole); with nothing, only
    /// the colour profile stays, which the picture needs to look right (and the colour chunks of
    /// a PNG, #218). The texts of a PNG and the comments of a JPEG stay in a file of the same
    /// format, and go into the XMP of the others (`texts::Texts::for_xmp`).
    fn blocks(&self, format: Format, w: u32, h: u32, sideways: bool, keep: Keep) -> Blocks {
        let png_color = self.texts.png_color.clone();
        if keep == Keep::Nothing {
            return Blocks { icc: self.icc.clone(), png_color, ..Blocks::default() };
        }
        let no_place = keep == Keep::NoPlace;
        let mut exif = self.exif.clone();
        if let Some(e) = &mut exif {
            crate::meta::fit_exif(e, w, h);
            if no_place {
                crate::meta_strip::exif_without_gps(e);
            }
        }
        let density = self.density.map(|d| if sideways { d.swapped() } else { d });
        if let Some(d) = density {
            let with = density::set_exif(exif.clone(), d, matches!(format, Format::Webp | Format::Avif));
            // A JPEG's EXIF that its new directory makes too large (#217): first without the
            // thumbnail's bytes at its end, which the directory would follow (`meta::without_tail`).
            exif = match with {
                Some(e) if format == Format::Jpeg && e.len() > crate::meta::JPEG_EXIF_MAX => density::set_exif(exif.map(crate::meta::without_tail), d, false),
                with => with,
            };
        }
        let irb = self.irb.as_deref().and_then(crate::meta_write::fit_irb);
        let irb = if no_place { irb.and_then(|i| crate::meta_write::irb_without_place(&i)) } else { irb };
        // WebP and AVIF have no place for IPTC: its fields go into the XMP.
        let iim = if matches!(format, Format::Webp | Format::Avif) { irb.as_deref().and_then(crate::meta_write::iim) } else { None };
        // The extended XMP of a JPEG joins the main packet, fitted and without the place as it; a
        // JPEG written splits it again when it is too large for one segment (`xmp_ext::split`).
        let ext = crate::meta_write::xmp_ext_packet(&self.xmp_ext);
        let fields = self.texts.for_xmp(format);
        let xmp = if no_place { crate::xmp::fit_without_place(self.xmp.as_deref(), w, h, iim, ext.as_deref(), fields) } else { crate::xmp::fit_with(self.xmp.as_deref(), w, h, iim, ext.as_deref(), fields) };
        let (png_text, jpeg_comments) = (self.texts.png_text.clone(), self.texts.jpeg_comments.clone());
        Blocks { exif, icc: self.icc.clone(), xmp, irb, density, png_text, png_color, jpeg_comments }
    }

    /// `blocks` for the pixels turned by `o.turn`: upside down, without the Photo Sphere tags (#188).
    fn blocks_turned(&self, format: Format, w: u32, h: u32, o: &Options) -> Blocks {
        let mut b = self.blocks(format, w, h, o.turn.sideways(), o.meta);
        if o.turn.upside_down() {
            b.xmp = b.xmp.map(|x| crate::xmp::without_pano(&x));
        }
        b
    }

    /// What a file written with `o` leaves out of this original's metadata, too large for a
    /// JPEG's segments (#217); nothing for the other formats. Known before encoding: the
    /// metadata do not need the pixels.
    pub fn left_out(&self, o: &Options) -> encoders::LeftOut {
        if o.format != Format::Jpeg || o.meta == Keep::Nothing {
            return encoders::LeftOut::default();
        }
        let (w, h) = (self.image.width(), self.image.height());
        let (w, h) = if o.turn.sideways() { (h, w) } else { (w, h) };
        encoders::jpeg_left_out(&self.blocks_turned(Format::Jpeg, w, h, o))
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
        let mut b = self.blocks_turned(o.format, img.width(), img.height(), o);
        // The profile only with pixels of its colour space (#214): a CMYK JPEG is decoded to RGB,
        // a grey picture becomes RGB in most formats, a JPEG in greys has one channel.
        let grey = !img.color().has_color() || (o.format == Format::Jpeg && o.chroma == Chroma::Grey && !o.jpeg_lossless);
        b.icc = b.icc.filter(|p| profile_fits(p, grey));
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
                let file = crate::meta_write::png_chunks(out.into_inner(), b.xmp.as_deref(), b.irb.as_deref());
                // The colour chunks only without a profile: the profile says it all.
                let file = texts::set_png(file, &b.png_text, if b.icc.is_none() { &b.png_color } else { &[] });
                return Ok(match b.density {
                    Some(d) => density::set_png(file, d),
                    None => file,
                });
            }
            Format::Bmp => {
                img.write_with_encoder(bmp::BmpEncoder::new(&mut out)).map_err(|e| e.to_string())?;
                if let Some(d) = b.density {
                    density::set_bmp(out.get_mut(), d);
                }
            }
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

/// Whether the colour profile `icc` describes grey pixels (`grey`) or RGB ones: its colour space,
/// bytes 16-20 of its header. A CMYK or Lab profile describes neither.
pub(crate) fn profile_fits(icc: &[u8], grey: bool) -> bool {
    icc.get(16..20) == Some(if grey { b"GRAY" } else { b"RGB " })
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
    /// format holds them, and are gone when not kept. A JPEG in greys has one channel: the RGB
    /// profile stays out (#214).
    #[test]
    fn every_format() {
        // 1200 different colours: the GIF must bring them down to 256.
        let mut img = image::RgbaImage::new(40, 30);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([(x * 6) as u8, (y * 8) as u8, (x * y) as u8, if x < 20 { 255 } else { 100 }]);
        }
        let mut exif = b"II*\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00".to_vec();
        assert_eq!(Orientation::remove_from_exif_chunk(&mut exif), Some(Orientation::Rotate90));
        // Only bytes to carry, with "RGB " where a profile names its colour space (#214).
        let icc = b"not a real one: RGB bytes to carry".to_vec();
        // XMP with sizes, orientation and Camera Raw settings; a Photoshop block with a thumbnail
        // and IPTC.
        let xmp = crate::xmp::tests::SAMPLE.as_bytes().to_vec();
        let iim = crate::xmp::tests::iim(&[(80, "Ann"), (90, "Québec")]);
        let irb = [crate::meta_write::resource(1036, &[1; 9]), crate::meta_write::irb_of_iim(&iim)].concat();
        let original = Original { image: DynamicImage::ImageRgba8(img), exif: Some(exif.clone()), icc: Some(icc.clone()), xmp: Some(xmp), irb: Some(irb), xmp_ext: Vec::new(), jpeg: None, density: None, hdr: None, sidecar: None, notes: Vec::new(), texts: texts::Texts::default() };
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
            let ((e, i), icc_kept) = (read_meta(&bytes), (format != Format::Jpeg || o.chroma != Chroma::Grey || o.jpeg_lossless).then(|| icc.clone()));
            if matches!(format, Format::Jpeg | Format::Png | Format::Webp) {
                assert_eq!(e.as_ref(), Some(&exif), "{o:?}");
            }
            if matches!(format, Format::Jpeg | Format::Png | Format::Webp | Format::Tiff) {
                assert_eq!(i, icc_kept, "{o:?}");
                kept_xmp_and_iptc(&bytes, format, &iim);
            }
            place_left_out(&original, o);
            // Nothing (#55): only the colour profile, which the picture needs.
            let bare = original.encode(&Options { meta: Keep::Nothing, ..o }).unwrap();
            if format != Format::Gif {
                assert_eq!(read_meta(&bare), (None, icc_kept.filter(|_| matches!(format, Format::Jpeg | Format::Png | Format::Webp | Format::Tiff))), "{o:?}");
                let back = Original::with_meta(original.image.clone(), &bare);
                assert!(back.xmp.is_none() && back.irb.is_none(), "{o:?}");
            }
        }
    }

    /// Without the place (#55), in a format with metadata: the city of the IPTC gone from the IPTC
    /// and the XMP, the author kept.
    fn place_left_out(original: &Original, o: Options) {
        if !o.format.holds_meta() {
            return;
        }
        let shared = original.encode(&Options { meta: Keep::NoPlace, ..o }).unwrap();
        let back = Original::with_meta(original.image.clone(), &shared);
        let text = format!("{:?} {:?}", back.xmp.as_deref().map(String::from_utf8_lossy), back.irb.as_deref().map(String::from_utf8_lossy));
        assert!(!text.contains("bec") && !text.contains("City"), "{o:?}: {text}");
        assert!(text.contains("Ann"), "{o:?}: {text}");
        // The Camera Raw settings of a photo stay, marked as applied (they go only with None).
        assert!(text.contains("crs:AlreadyApplied"), "{o:?}: {text}");
    }

    /// #359: a photo records its place when "All but the place" would take something out: a GPS
    /// position, or a city with no position; not a photo with a title only, nor the copy saved
    /// without the place.
    #[test]
    fn records_the_place() {
        let xmp = |props: &str| format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/" {props}>
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Sea</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>"#);
        let img = DynamicImage::new_rgb8(8, 8);
        let jpeg = Original::with_meta(img.clone(), &[]).encode(&Options::default()).unwrap();
        assert!(!Original::with_meta(img.clone(), &jpeg).has_place(Format::Jpeg), "no metadata");
        for (what, props, place) in [("GPS", r#"exif:GPSLatitude="40,21.5N" exif:GPSLongitude="18,10.2E""#, true), ("city", r#"photoshop:City="Lecce""#, true), ("title", "", false)] {
            let side = xmp(props);
            let original = Original::with_meta_beside(img.clone(), &jpeg, "a.jpg", Some(side.as_bytes()));
            for format in [Format::Jpeg, Format::Png, Format::Webp] {
                assert_eq!(original.has_place(format), place, "{what}, {format:?}");
            }
            let shared = original.encode(&Options { meta: Keep::NoPlace, ..Options::default() }).unwrap();
            assert!(!Original::with_meta(img.clone(), &shared).has_place(Format::Jpeg), "{what}: saved without the place");
        }
    }

    /// A photo's sidecar (#24) follows the metadata choice (#55): its title kept without the
    /// place, its position and city gone; with nothing, no XMP at all.
    #[test]
    fn sidecar_without_the_place() {
        let side = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"
 exif:GPSLatitude="40,21.5N" exif:GPSLongitude="18,10.2E" photoshop:City="Lecce">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Sea</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let img = DynamicImage::new_rgb8(8, 8);
        let jpeg = Original::with_meta(img.clone(), &[]).encode(&Options::default()).unwrap();
        let original = Original::with_meta_beside(img.clone(), &jpeg, "a.jpg", Some(side.as_bytes()));
        let xmp = |keep: Keep| Original::with_meta(img.clone(), &original.encode(&Options { meta: keep, ..Options::default() }).unwrap()).xmp.map(|x| String::from_utf8(x).unwrap());
        let all = xmp(Keep::All).unwrap();
        assert!(all.contains("GPSLatitude") && all.contains("Lecce") && all.contains("Sea"), "{all}");
        let shared = xmp(Keep::NoPlace).unwrap();
        assert!(!shared.contains("GPS") && !shared.contains("Lecce") && shared.contains("Sea"), "{shared}");
        assert_eq!(xmp(Keep::Nothing), None);
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

    /// The XMP of `xmp::tests::SAMPLE` in a written file: sizes `w` × `h`, upright, no thumbnail,
    /// the author kept, the Camera Raw settings kept and marked as already applied.
    pub(crate) fn kept_xmp_like(bytes: &[u8], w: u32, h: u32) -> String {
        let back = Original::with_meta(DynamicImage::new_rgb8(1, 1), bytes);
        let xmp = String::from_utf8(back.xmp.clone().expect("no XMP")).unwrap();
        for kept in [format!("exif:PixelXDimension=\"{w}\""), format!("<exif:PixelYDimension>{h}</exif:PixelYDimension>"), "tiff:Orientation=\"1\"".into(), "Ann &lt;Author&gt;".into()] {
            assert!(xmp.contains(&kept), "{kept} not in {xmp}");
        }
        assert!(xmp.contains("crs:HasCrop=\"True\"") && xmp.contains("crs:AlreadyApplied=\"True\"") && !xmp.contains("Thumbnails"), "{xmp}");
        xmp
    }

    /// The extended XMP of a JPEG: put into the main packet (without the GUID, which would point
    /// to nothing), in a JPEG too when it fits in one segment.
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
        assert!(back.xmp_ext.is_empty());
        let png = back.encode(&Options { format: Format::Png, ..Default::default() }).unwrap();
        for file in [jpeg, png] {
            let xmp = String::from_utf8(Original::with_meta(DynamicImage::new_rgb8(1, 1), &file).xmp.unwrap()).unwrap();
            assert!(xmp.contains(&format!("<dc:source>{}</dc:source>", "a".repeat(100))) && !xmp.contains(guid), "{xmp}");
        }
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
        // A maker note that can move (`meta::movable_maker_note`) goes in too.
        let note = b"Apple iOS\0\0\x01MM\0\0".to_vec();
        let maker = Field { tag: Tag::MakerNote, ifd_num: In::PRIMARY, value: Value::Undefined(note.clone(), 0) };
        w.push_field(&maker);
        for little in [false, true] {
            let mut exif = Cursor::new(Vec::new());
            w.write(&mut exif, little).unwrap();
            let original = Original { image: DynamicImage::new_rgb8(8, 6), exif: Some(exif.into_inner()), icc: None, xmp: None, irb: None, xmp_ext: Vec::new(), jpeg: None, density: None, hdr: None, sidecar: None, notes: Vec::new(), texts: texts::Texts::default() };
            let bytes = original.encode(&Options { format: Format::Tiff, ..Default::default() }).unwrap();
            let back = exif::Reader::new().read_from_container(&mut Cursor::new(&bytes)).unwrap();
            for f in &fields {
                assert_eq!(back.get_field(f.tag, In::PRIMARY).map(|b| format!("{:?}", b.value)), Some(format!("{:?}", f.value)), "{:?}", f.tag);
            }
            assert!(matches!(back.get_field(Tag::MakerNote, In::PRIMARY).map(|f| &f.value), Some(Value::Undefined(v, _)) if *v == note), "maker note");
            assert_eq!(crate::decode::decode_full(&bytes, "x.tif").unwrap().width(), 8);
        }
    }

    /// ICO holds at most 256 × 256 pixels: a larger image is reduced, keeping its shape. The
    /// formats only "Save as…" writes are not saved back over an original.
    #[test]
    fn ico_and_save_back() {
        let original = Original { image: DynamicImage::ImageRgb8(image::RgbImage::new(600, 300)), exif: None, icc: None, xmp: None, irb: None, xmp_ext: Vec::new(), jpeg: None, density: None, hdr: None, sidecar: None, notes: Vec::new(), texts: texts::Texts::default() };
        let bytes = original.encode(&Options { format: Format::Ico, ..Default::default() }).unwrap();
        let back = crate::decode::decode_full(&bytes, "x.ico").unwrap();
        assert_eq!((back.width(), back.height()), (256, 128));
        assert_eq!(Format::of_file("a.TIFF"), Some(Format::Tiff));
        for name in ["a.jpg", "a.JPEG", "a.jfif", "a.JPE"] {
            assert_eq!(Format::of_file(name), Some(Format::Jpeg), "{name}");
            assert!(saves_back(name), "{name}");
        }
        for name in ["a.ico", "a.tga", "a.pnm", "a.exr", "a.hdr", "a.ff"] {
            assert_eq!(Format::of_file(name), None, "{name}");
        }
    }

    /// A TIFF (not a RAW file) with the make and model of a camera rawler knows keeps its own
    /// colour profile and XMP when saved (#215): it is not taken for a RAW file by its content.
    #[test]
    fn camera_tiff_is_not_raw() {
        let fields = [(exif::Tag::Make, "Hasselblad"), (exif::Tag::Model, "Hasselblad X1D")].map(|(tag, v)| exif::Field { tag, ifd_num: exif::In::PRIMARY, value: exif::Value::Ascii(vec![v.as_bytes().to_vec()]) });
        let exif = crate::meta::write_exif(&fields, false).unwrap();
        let icc = include_bytes!("../assets/icc/AdobeCompat-v2.icc").to_vec();
        let xmp = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/" crs:Exposure2012="+0.50"/></rdf:RDF></x:xmpmeta>"#.to_vec();
        // Large enough that `image` hands out a TIFF's profile: it gives none when the profile is
        // not much smaller than the pixel data.
        let image = DynamicImage::ImageRgb16(image::ImageBuffer::new(64, 48));
        let made = Original { exif: Some(exif), icc: Some(icc.clone()), xmp: Some(xmp), ..Original::with_meta(image.clone(), &[]) };
        let tiff = made.encode(&Options { format: Format::Tiff, ..Default::default() }).unwrap();
        let own = crate::meta_write::read(&tiff, None).xmp.unwrap();
        assert!(String::from_utf8_lossy(&own).contains("crs:Exposure2012"));
        // rawler takes it for a Hasselblad 3FR.
        assert!(crate::raw_meta::read(&tiff).is_some());
        let saved = Original::with_meta_beside(image, &tiff, "a.tif", None);
        assert_eq!(saved.icc, Some(icc));
        assert_eq!(saved.xmp, Some(own));
    }

    /// A colour profile goes into a file only when its colour space is that of the pixels written
    /// (#214): a CMYK JPEG is decoded to RGB, a grey picture becomes RGB in WebP or a colour JPEG.
    #[test]
    fn profile_fits_the_pixels() {
        let space = |name: &[u8; 4]| {
            let mut icc = include_bytes!("../assets/icc/sRGB-v2-micro.icc").to_vec();
            icc[16..20].copy_from_slice(name);
            icc
        };
        let (rgb, cmyk, gray) = (space(b"RGB "), space(b"CMYK"), space(b"GRAY"));
        let back = |file: &[u8]| Original::with_meta(DynamicImage::new_rgb8(1, 1), file).icc;
        let to = |f: Format| Options { format: f, avif_speed: 10, ..Default::default() };
        // A CMYK JPEG with its profile in APP2, as Photoshop writes it.
        let mut c = turbojpeg::Compressor::new().unwrap();
        c.set_subsamp(turbojpeg::Subsamp::None).unwrap();
        let ink = vec![40u8; 160 * 160 * 4];
        let mut jpeg = c.compress_to_vec(turbojpeg::Image { pixels: ink.as_slice(), width: 160, pitch: 160 * 4, height: 160, format: turbojpeg::PixelFormat::CMYK }).unwrap();
        let body = [&b"ICC_PROFILE\0\x01\x01"[..], &cmyk].concat();
        jpeg.splice(2..2, [&[0xFF, 0xE2][..], &(body.len() as u16 + 2).to_be_bytes(), &body].concat());
        let original = Original::with_meta(crate::decode::decode_full(&jpeg, "a.jpg").unwrap(), &jpeg);
        assert_eq!(original.icc.as_ref(), Some(&cmyk));
        for f in [Format::Png, Format::Jpeg, Format::Webp, Format::Avif, Format::Tiff] {
            assert_eq!(back(&original.encode(&to(f)).unwrap()), None, "CMYK in {f:?}");
        }
        // A grey picture with a grey profile: kept in a grey PNG, TIFF or JPEG, not in an RGB file.
        // Large enough that `image` hands out a TIFF's profile: it gives none when the profile is
        // not much smaller than the pixel data.
        let grey = Original { icc: Some(gray.clone()), ..Original::with_meta(DynamicImage::new_luma8(160, 160), &[]) };
        for f in [Format::Png, Format::Tiff] {
            assert_eq!(back(&grey.encode(&to(f)).unwrap()).as_ref(), Some(&gray), "grey in {f:?}");
        }
        let grey_jpeg = Options { chroma: Chroma::Grey, ..to(Format::Jpeg) };
        assert_eq!(back(&grey.encode(&grey_jpeg).unwrap()).as_ref(), Some(&gray), "grey JPEG");
        for f in [Format::Webp, Format::Avif, Format::Jpeg] {
            assert_eq!(back(&grey.encode(&to(f)).unwrap()), None, "grey in {f:?}");
        }
        // An RGB profile stays in an RGB file.
        let colour = Original { icc: Some(rgb.clone()), ..Original::with_meta(DynamicImage::new_rgb8(160, 160), &[]) };
        for f in [Format::Png, Format::Jpeg, Format::Webp, Format::Tiff] {
            assert_eq!(back(&colour.encode(&to(f)).unwrap()).as_ref(), Some(&rgb), "RGB in {f:?}");
        }
    }

    /// Every chunk of a PNG up to IEND: kind and data.
    pub(crate) fn png_all_chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        let mut v = Vec::new();
        let mut i = 8;
        while let Some(head) = png.get(i..i + 8) {
            let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
            v.push((head[4..8].try_into().unwrap(), png[i + 8..i + 8 + len].to_vec()));
            i += 12 + len;
        }
        v
    }

    /// A PNG with the texts a program writes (a title in tEXt, a description compressed in zTXt,
    /// an author in UTF-8 in iTXt, a copyright, the parameters of an image generator) and a gamma
    /// chunk, without a colour profile.
    pub(crate) fn png_with_texts() -> Vec<u8> {
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(8, 6)).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut z, b"A walk by the sea").unwrap();
        let mut extra = Vec::new();
        crate::meta_write::chunk(&mut extra, b"gAMA", &45455u32.to_be_bytes());
        crate::meta_write::chunk(&mut extra, b"tEXt", b"Title\0Sea \xe0 l'aube");
        crate::meta_write::chunk(&mut extra, b"zTXt", &[&b"Description\0\0"[..], &z.finish().unwrap()].concat());
        crate::meta_write::chunk(&mut extra, b"iTXt", "Author\0\0\0it\0Autore\0Zoë Rossi".as_bytes());
        crate::meta_write::chunk(&mut extra, b"tEXt", b"Copyright\0CC BY 4.0");
        crate::meta_write::chunk(&mut extra, b"tEXt", b"parameters\0a lighthouse, steps 30");
        crate::meta_write::chunk(&mut extra, b"tEXt", b"Comment\0made at dawn");
        let at = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
        png.splice(at..at, extra);
        png
    }

    /// The texts of a PNG and its gamma reach a PNG made from it as they were, and the XMP of
    /// the other formats as title, author, description, copyright and comment (#218). A JPEG's
    /// comment stays in a JPEG written again, and goes into the XMP of the other formats, as a
    /// GIF's.
    #[test]
    fn texts_and_comments_kept() {
        let png = png_with_texts();
        let original = Original::with_meta(crate::decode::decode_full(&png, "x.png").unwrap(), &png);
        let copy = original.encode(&Options { format: Format::Png, ..Default::default() }).unwrap();
        let kept = png_all_chunks(&copy);
        for c in png_all_chunks(&png).into_iter().filter(|(k, _)| matches!(k, b"gAMA" | b"tEXt" | b"zTXt" | b"iTXt")) {
            assert!(kept.contains(&c), "{} {:?} not in the copy", String::from_utf8_lossy(&c.0), String::from_utf8_lossy(&c.1));
        }
        for format in [Format::Jpeg, Format::Webp, Format::Tiff, Format::Avif] {
            let file = original.encode(&Options { format, ..Default::default() }).unwrap();
            let xmp = Original::with_meta(DynamicImage::new_rgb8(1, 1), &file).xmp.map(|x| String::from_utf8(x).unwrap()).unwrap_or_default();
            for text in ["<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Sea à l&apos;aube</rdf:li>", "<dc:creator><rdf:Seq><rdf:li>Zoë Rossi</rdf:li>", ">A walk by the sea</rdf:li>", ">CC BY 4.0</rdf:li>", "<exif:UserComment><rdf:Alt><rdf:li xml:lang=\"x-default\">made at dawn</rdf:li>"] {
                assert!(xmp.contains(text), "{format:?}: {text} not in {xmp}");
            }
        }
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(8, 6)).write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        let jpeg = [&jpeg[..2], &[0xFF, 0xFE, 0, 8], b"a note", &jpeg[2..]].concat();
        let again = Original::with_meta(crate::decode::decode_full(&jpeg, "x.jpg").unwrap(), &jpeg).encode(&Options::default()).unwrap();
        assert!(crate::ultrahdr::segments(&again).iter().any(|&(m, s, e)| m == 0xFE && &again[s + 4..e] == b"a note"), "the comment is lost");
        let mut gif = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(8, 6)).write_to(&mut Cursor::new(&mut gif), image::ImageFormat::Gif).unwrap();
        let end = gif.len() - 1;
        gif.splice(end..end, [&[0x21, 0xFE, 7][..], b"a frame", &[0]].concat());
        for (file, name, comment) in [(jpeg, "x.jpg", "a note"), (gif, "x.gif", "a frame")] {
            let other = Original::with_meta(crate::decode::decode_full(&file, name).unwrap(), &file).encode(&Options { format: Format::Png, ..Default::default() }).unwrap();
            let xmp = Original::with_meta(DynamicImage::new_rgb8(1, 1), &other).xmp.map(|x| String::from_utf8(x).unwrap()).unwrap_or_default();
            assert!(xmp.contains(&format!("<exif:UserComment><rdf:Alt><rdf:li xml:lang=\"x-default\">{comment}</rdf:li>")), "{name}: {xmp}");
        }
    }

    /// A JPEG comment and a PNG text with characters XML does not allow (binary bytes some
    /// cameras write, a control character): the XMP of a copy in another format is still XML,
    /// with the text but them (audit E.5).
    #[test]
    fn control_characters_out_of_the_xmp() {
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(8, 6)).write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        let jpeg = [&jpeg[..2], &[0xFF, 0xFE, 0, 5], b"a\x01b", &jpeg[2..]].concat();
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(8, 6)).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut extra = Vec::new();
        crate::meta_write::chunk(&mut extra, b"tEXt", b"Comment\0a\x0bb");
        let at = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
        png.splice(at..at, extra);
        for (file, name, format) in [(jpeg, "x.jpg", Format::Png), (png, "x.png", Format::Jpeg)] {
            let copy = Original::with_meta(crate::decode::decode_full(&file, name).unwrap(), &file).encode(&Options { format, ..Default::default() }).unwrap();
            let xmp = Original::with_meta(DynamicImage::new_rgb8(1, 1), &copy).xmp.map(|x| String::from_utf8(x).unwrap()).unwrap_or_default();
            let parsed = roxmltree::Document::parse(xmp.trim_end_matches(['\0', ' ', '\n']));
            assert!(parsed.is_ok(), "{name}: {parsed:?} in {xmp:?}");
            assert!(xmp.contains(">ab</rdf:li>"), "{name}: {xmp}");
        }
    }

    /// An EXIF with the camera, the date, a GPS position, a maker note that can move (Apple's) of
    /// `note` bytes and, if `thumb` is not 0, a thumbnail of `thumb` bytes.
    pub(crate) fn large_exif(note: usize, thumb: usize) -> Vec<u8> {
        large_exif_of(b"Apple iOS\0\0\x01MM", note, thumb)
    }

    /// As `large_exif`, with a maker note that begins with `maker`.
    pub(crate) fn large_exif_of(maker: &[u8], note: usize, thumb: usize) -> Vec<u8> {
        use exif::{Field, In, Tag, Value};
        let mut w = exif::experimental::Writer::new();
        let maker = [maker, &vec![7; note]].concat();
        let fields = [
            Field { tag: Tag::Make, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"Maker".to_vec()]) },
            Field { tag: Tag::DateTimeOriginal, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"2024:01:02 03:04:05".to_vec()]) },
            Field { tag: Tag::GPSLatitudeRef, ifd_num: In::PRIMARY, value: Value::Ascii(vec![b"N".to_vec()]) },
            Field { tag: Tag::GPSLatitude, ifd_num: In::PRIMARY, value: Value::Rational(vec![(45, 1).into(), (0, 1).into(), (0, 1).into()]) },
            Field { tag: Tag::MakerNote, ifd_num: In::PRIMARY, value: Value::Undefined(maker, 0) },
        ];
        fields.iter().for_each(|f| w.push_field(f));
        let thumbnail = [&[0xFF, 0xD8][..], &vec![1; thumb.saturating_sub(4)], &[0xFF, 0xD9]].concat();
        if thumb > 0 {
            w.set_jpeg(&thumbnail, In::THUMBNAIL);
        }
        let mut out = Cursor::new(Vec::new());
        w.write(&mut out, false).unwrap();
        out.into_inner()
    }

    /// An EXIF too large for a JPEG segment (64 KB) is made smaller, not left out (#217): first
    /// without its thumbnail, then without the maker note; the camera, the date and the GPS
    /// stay, and without the place the GPS goes as ever. So for an EXIF at the limit that the
    /// pixel density makes larger.
    #[test]
    fn large_exif_in_a_jpeg() {
        use exif::{In, Tag};
        let jpeg = |exif: Vec<u8>, density: Option<density::Density>, meta: Keep| {
            let original = Original { density, ..Original::upright(DynamicImage::new_rgb8(8, 6), Some(exif), None, None, None) };
            let file = original.encode(&Options { meta, ..Default::default() }).unwrap();
            exif::Reader::new().read_from_container(&mut Cursor::new(&file)).ok()
        };
        let has = |e: &exif::Exif, tag| e.get_field(tag, In::PRIMARY).is_some();
        let with_thumbnail = large_exif(60_000, 10_000);
        assert!(with_thumbnail.len() > 70_000);
        let e = jpeg(with_thumbnail, None, Keep::All).expect("EXIF left out");
        assert!(has(&e, Tag::Make) && has(&e, Tag::DateTimeOriginal) && has(&e, Tag::GPSLatitude) && has(&e, Tag::MakerNote));
        assert!(e.fields().all(|f| f.ifd_num != In::THUMBNAIL));
        let without = large_exif(70_000, 0);
        let e = jpeg(without.clone(), None, Keep::All).expect("EXIF left out");
        assert!(has(&e, Tag::Make) && has(&e, Tag::DateTimeOriginal) && has(&e, Tag::GPSLatitude) && !has(&e, Tag::MakerNote));
        let e = jpeg(without, None, Keep::NoPlace).expect("EXIF left out");
        assert!(has(&e, Tag::Make) && !has(&e, Tag::GPSLatitude));
        // At the limit with a small thumbnail; the density adds a directory after it.
        let mut note = 65_000;
        let at_limit = loop {
            let e = large_exif(note, 300);
            if e.len() >= 65_520 {
                break e;
            }
            note += 65_520 - e.len();
        };
        assert!(at_limit.len() <= 65_527);
        let dpi = density::Density::new(density::Unit::Inch, (300, 1), (300, 1));
        let mut fitted = at_limit.clone();
        crate::meta::fit_exif(&mut fitted, 8, 6);
        assert!(density::set_exif(Some(fitted), dpi.unwrap(), false).unwrap().len() > 65_527, "the density no longer makes it too large");
        let e = jpeg(at_limit, dpi, Keep::All).expect("EXIF left out");
        assert!(has(&e, Tag::Make) && has(&e, Tag::MakerNote) && has(&e, Tag::XResolution));
    }
    /// An EXIF with a caption of `len` letters: it cannot be made smaller.
    pub(crate) fn caption_exif(len: usize) -> Vec<u8> {
        use exif::{Field, In, Tag, Value};
        let mut w = exif::experimental::Writer::new();
        let caption = Field { tag: Tag::ImageDescription, ifd_num: In::PRIMARY, value: Value::Ascii(vec![vec![b'a'; len]]) };
        w.push_field(&caption);
        let mut exif = Cursor::new(Vec::new());
        w.write(&mut exif, false).unwrap();
        exif.into_inner()
    }

    /// What cannot fit even made smaller is said before the file is written, for a JPEG only
    /// and never when nothing is kept: a caption of 70 KB, an XMP that is not XML of 70 KB, a
    /// Photoshop block of 70 KB.
    #[test]
    fn too_large_said() {
        use crate::encoders::LeftOut;
        let irb = crate::meta_write::irb_of_iim(&vec![0x1C; 70_000]);
        let original = Original::upright(DynamicImage::new_rgb8(8, 6), Some(caption_exif(70_000)), None, Some(vec![b'<'; 70_000]), Some(irb));
        let jpeg = Options::default();
        assert_eq!(original.left_out(&jpeg), LeftOut { exif: true, xmp: true, iptc: true });
        assert_eq!(original.left_out(&jpeg).parts("and"), "EXIF, XMP and IPTC");
        assert_eq!(original.left_out(&Options { meta: Keep::Nothing, ..jpeg }), LeftOut::default());
        assert_eq!(original.left_out(&Options { format: Format::Png, ..jpeg }), LeftOut::default());
        let small = Original::upright(DynamicImage::new_rgb8(8, 6), Some(large_exif(70_000, 0)), None, None, None);
        assert!(!small.left_out(&jpeg).any());
    }
    /// A maker note that cannot move (Canon's and most others count from the start of the EXIF)
    /// stays when the thumbnail's bytes at the end are enough (#217): left out, not moved, also
    /// when the pixel density adds its directory to an EXIF at the limit.
    #[test]
    fn fixed_maker_note_kept_in_a_jpeg() {
        use exif::{In, Tag};
        let jpeg = |exif: Vec<u8>, density: Option<density::Density>| {
            let original = Original { density, ..Original::upright(DynamicImage::new_rgb8(8, 6), Some(exif), None, None, None) };
            let file = original.encode(&Options::default()).unwrap();
            exif::Reader::new().read_from_container(&mut Cursor::new(&file)).expect("EXIF left out")
        };
        let note = |e: &exif::Exif| e.get_field(Tag::MakerNote, In::PRIMARY).is_some();
        let with_thumbnail = large_exif_of(b"Canon", 60_000, 10_000);
        assert!(with_thumbnail.len() > 70_000);
        let e = jpeg(with_thumbnail, None);
        assert!(note(&e) && e.fields().all(|f| f.ifd_num != In::THUMBNAIL));
        let mut size = 65_000;
        let at_limit = loop {
            let e = large_exif_of(b"Canon", size, 300);
            if e.len() >= 65_520 {
                break e;
            }
            size += 65_520 - e.len();
        };
        let e = jpeg(at_limit, density::Density::new(density::Unit::Inch, (300, 1), (300, 1)));
        assert!(note(&e) && e.get_field(Tag::XResolution, In::PRIMARY).is_some());
    }
}
