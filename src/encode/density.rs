//! The pixel density of a picture (dots per inch or per centimetre, or only the shape of its
//! pixels) read from wherever the original keeps it and written into whatever the new file can
//! hold it in. A resize changes the pixels, not the density: a 600 dpi scan stays 600 dpi. An
//! original without a density gives a file without one.
//!
//! Read from: the JFIF segment of a JPEG, the pHYs chunk of a PNG, the header of a BMP, the EXIF
//! (the TIFF tags of a TIFF; the EXIF of a JPEG, PNG, WebP, HEIC, AVIF, JPEG XL or RAW) and the
//! Photoshop block (PSD). Written into: the JFIF segment of a JPEG, the pHYs chunk of a PNG, the
//! tags of a TIFF, the EXIF of a WebP or AVIF (made for it when the original had none) and the
//! header of a BMP; an EXIF carried over gets the same density.

use exif::{In, Tag, Value};

use super::jfif;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    /// No unit: the two numbers only give the shape of the pixels.
    Aspect,
    Inch,
    Cm,
}

/// A number as a fraction (numerator, denominator), as TIFF and EXIF keep it: what is read is
/// written back as it was, without rounding.
pub type Ratio = (u32, u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Density {
    pub unit: Unit,
    /// Across.
    pub x: Ratio,
    /// Down.
    pub y: Ratio,
}

const METRES_PER_INCH: f64 = 0.0254;
/// Resolution tags of TIFF and EXIF: XResolution, YResolution, ResolutionUnit.
pub const TAGS: [u16; 3] = [282, 283, 296];

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

fn reduce((n, d): Ratio) -> Ratio {
    let g = gcd(n as u64, d as u64) as u32;
    (n / g, d / g)
}

fn value((n, d): Ratio) -> f64 {
    n as f64 / d as f64
}

impl Density {
    /// `None` for a zero or a fraction over zero, and for square pixels without a unit, which
    /// say nothing a reader does not assume already.
    pub fn new(unit: Unit, x: Ratio, y: Ratio) -> Option<Density> {
        if x.0 == 0 || x.1 == 0 || y.0 == 0 || y.1 == 0 {
            return None;
        }
        let d = Density { unit, x: reduce(x), y: reduce(y) };
        (unit != Unit::Aspect || d.x != d.y).then_some(d)
    }

    /// From dots per metre (PNG, BMP): per inch when a whole number of dots per inch is less
    /// than one dot per metre away (23622 per metre is 600 dpi, and 600 dpi is 23622 again;
    /// ImageMagick writes 72 dpi as 2834, cut instead of rounded), otherwise per centimetre,
    /// exactly.
    fn per_metre(x: u32, y: u32) -> Option<Density> {
        let inch = |m: u32| {
            let dpi = (m as f64 * METRES_PER_INCH).round();
            (dpi >= 1.0 && (dpi / METRES_PER_INCH - m as f64).abs() < 1.0).then_some((dpi as u32, 1))
        };
        match (inch(x), inch(y)) {
            (Some(x), Some(y)) => Density::new(Unit::Inch, x, y),
            _ => Density::new(Unit::Cm, (x, 100), (y, 100)),
        }
    }

    /// Dots per metre across and down (PNG, BMP); `None` without a unit.
    pub fn per_metre_counts(self) -> Option<(u32, u32)> {
        let k = match self.unit {
            Unit::Inch => 1.0 / METRES_PER_INCH,
            Unit::Cm => 100.0,
            Unit::Aspect => return None,
        };
        let m = |r: Ratio| (value(r) * k).round().clamp(1.0, i32::MAX as f64) as u32;
        Some((m(self.x), m(self.y)))
    }

    /// The shape of the pixels as two whole numbers, each at most `max`.
    fn aspect(self, max: u64) -> (u64, u64) {
        let (x, y) = (self.x.0 as u64 * self.y.1 as u64, self.y.0 as u64 * self.x.1 as u64);
        let g = gcd(x, y);
        let (x, y) = (x / g, y / g);
        let k = (x.max(y) as f64 / max as f64).max(1.0);
        let fit = |v: u64| ((v as f64 / k).round() as u64).clamp(1, max);
        (fit(x), fit(y))
    }

    /// Across and down exchanged: the picture was turned a quarter.
    pub fn swapped(self) -> Density {
        Density { x: self.y, y: self.x, ..self }
    }
}

/// The density of the original: its file's own place for it (JFIF, pHYs, BMP header), then its
/// EXIF, then its Photoshop block; a shape without unit only when nothing gives a unit.
pub fn read(bytes: &[u8], exif: Option<&[u8]>, irb: Option<&[u8]>) -> Option<Density> {
    let file = jfif::read(bytes).and_then(of_jfif).or_else(|| png(bytes)).or_else(|| bmp(bytes));
    match file {
        Some(d) if d.unit != Unit::Aspect => Some(d),
        _ => exif.and_then(of_exif).or_else(|| irb.and_then(photoshop)).or(file),
    }
}

fn of_jfif(j: jfif::Density) -> Option<Density> {
    let unit = [Unit::Aspect, Unit::Inch, Unit::Cm][j.unit as usize];
    Density::new(unit, (j.x as u32, 1), (j.y as u32, 1))
}

/// A JFIF density for `d`: whole numbers of at most 65535.
pub fn to_jfif(d: Density) -> jfif::Density {
    let (unit, x, y) = match d.unit {
        Unit::Aspect => {
            let (x, y) = d.aspect(u16::MAX as u64);
            (0, x as f64, y as f64)
        }
        Unit::Inch => (1, value(d.x), value(d.y)),
        Unit::Cm => (2, value(d.x), value(d.y)),
    };
    let n = |v: f64| v.round().clamp(1.0, u16::MAX as f64) as u16;
    jfif::Density { unit, x: n(x), y: n(y) }
}

/// The chunks of a PNG up to its first IDAT (included): where each starts, its kind and data.
fn png_chunks(png: &[u8]) -> Vec<(usize, &[u8], &[u8])> {
    let mut v = Vec::new();
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return v;
    }
    let mut i = 8;
    while let Some(head) = png.get(i..i + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let Some(data) = png.get(i + 8..i + 8 + len) else { break };
        v.push((i, &head[4..], data));
        if &head[4..] == b"IDAT" {
            break;
        }
        i += 12 + len;
    }
    v
}

/// The pHYs chunk of a PNG: dots per metre (unit 1), or only the shape (unit 0).
fn png(bytes: &[u8]) -> Option<Density> {
    let (_, _, p) = png_chunks(bytes).into_iter().find(|(_, kind, data)| *kind == b"pHYs" && data.len() == 9)?;
    let (x, y) = (u32::from_be_bytes([p[0], p[1], p[2], p[3]]), u32::from_be_bytes([p[4], p[5], p[6], p[7]]));
    match p[8] {
        0 => Density::new(Unit::Aspect, (x, 1), (y, 1)),
        1 => Density::per_metre(x, y),
        _ => None,
    }
}

/// `png` with a pHYs chunk for `d` before its image data (in place of one it had).
pub fn set_png(png: Vec<u8>, d: Density) -> Vec<u8> {
    let chunks = png_chunks(&png);
    let Some(&(at, ..)) = chunks.iter().find(|(_, kind, _)| *kind == b"IDAT") else { return png };
    let old = chunks.iter().find(|(_, kind, _)| *kind == b"pHYs").map(|&(s, _, data)| s..s + 12 + data.len());
    let (x, y, unit) = match d.per_metre_counts() {
        Some((x, y)) => (x, y, 1),
        None => {
            let (x, y) = d.aspect(i32::MAX as u64);
            (x as u32, y as u32, 0)
        }
    };
    let mut chunk = Vec::new();
    crate::meta_write::chunk(&mut chunk, b"pHYs", &[&x.to_be_bytes()[..], &y.to_be_bytes(), &[unit]].concat());
    let mut out = png;
    out.splice(at..at, chunk);
    if let Some(old) = old {
        out.drain(old);
    }
    out
}

/// Where a BMP header keeps its dots per metre, across then down; `None` for a header too old
/// (OS/2) to have them.
fn bmp_at(bytes: &[u8]) -> Option<usize> {
    let size = u32::from_le_bytes(bytes.get(14..18)?.try_into().ok()?);
    (bytes.starts_with(b"BM") && size >= 40 && bytes.len() >= 46).then_some(38)
}

fn bmp(bytes: &[u8]) -> Option<Density> {
    let at = bmp_at(bytes)?;
    let n = |i: usize| i32::from_le_bytes(bytes[i..i + 4].try_into().unwrap_or_default());
    let (x, y) = (n(at), n(at + 4));
    (x > 0 && y > 0).then(|| Density::per_metre(x as u32, y as u32)).flatten()
}

/// `d` written into the header of the BMP `bmp`; a BMP holds no shape without unit.
pub fn set_bmp(bmp: &mut [u8], d: Density) {
    if let (Some(at), Some((x, y))) = (bmp_at(bmp), d.per_metre_counts()) {
        bmp[at..at + 4].copy_from_slice(&x.to_le_bytes());
        bmp[at + 4..at + 8].copy_from_slice(&y.to_le_bytes());
    }
}

/// The resolution in the main directory of an EXIF block (TIFF data). Without ResolutionUnit
/// the unit is the inch, as TIFF says.
fn of_exif(exif: &[u8]) -> Option<Density> {
    let mut r = exif::Reader::new();
    r.continue_on_error(true);
    let e = r.read_raw(exif.to_vec()).or_else(|e| e.distill_partial_result(|_| {})).ok()?;
    let ratio = |tag: Tag| match &e.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(v) => v.first().map(|r| (r.num, r.denom)),
        _ => None,
    };
    let unit = match e.get_field(Tag::ResolutionUnit, In::PRIMARY).and_then(|f| f.value.get_uint(0)) {
        Some(1) => Unit::Aspect,
        None | Some(2) => Unit::Inch,
        Some(3) => Unit::Cm,
        _ => return None,
    };
    Density::new(unit, ratio(Tag::XResolution)?, ratio(Tag::YResolution)?)
}

fn unit_code(u: Unit) -> u16 {
    match u {
        Unit::Aspect => 1,
        Unit::Inch => 2,
        Unit::Cm => 3,
    }
}

/// The three resolution tags of a TIFF for `d`, in this machine's byte order (as the `tiff`
/// encoder takes them).
pub fn tiff_fields(d: Density) -> Vec<(u16, tiff::tags::Type, Vec<u8>)> {
    use tiff::tags::Type;
    let rational = |(n, d): Ratio| [n.to_ne_bytes(), d.to_ne_bytes()].concat();
    vec![(TAGS[0], Type::RATIONAL, rational(d.x)), (TAGS[1], Type::RATIONAL, rational(d.y)), (TAGS[2], Type::SHORT, unit_code(d.unit).to_ne_bytes().to_vec())]
}

/// The EXIF `exif` with the density `d`: unchanged when it already says so; made with the
/// density alone when there is none and `make` asks for it (WebP and AVIF, where the EXIF is the
/// only place for it). A shape without unit is not put into an EXIF.
pub fn set_exif(exif: Option<Vec<u8>>, d: Density, make: bool) -> Option<Vec<u8>> {
    match exif {
        _ if d.unit == Unit::Aspect => exif,
        Some(e) if of_exif(&e) == Some(d) => Some(e),
        Some(e) => Some(exif_with(&e, d).unwrap_or(e)),
        None if make => {
            let field = |tag: Tag, value: Value| exif::Field { tag, ifd_num: In::PRIMARY, value };
            let rational = |(num, denom): Ratio| Value::Rational(vec![exif::Rational { num, denom }]);
            let fields = [field(Tag::XResolution, rational(d.x)), field(Tag::YResolution, rational(d.y)), field(Tag::ResolutionUnit, Value::Short(vec![unit_code(d.unit)]))];
            crate::meta::write_exif(&fields, true)
        }
        None => None,
    }
}

/// `exif` with a new main directory at its end: the old one's entries without their resolution,
/// and the three resolution tags of `d`. Nothing else moves, so every offset (the maker note's
/// too) stays right; the old directory stays behind, unused. `None` for a block not readable.
fn exif_with(t: &[u8], d: Density) -> Option<Vec<u8>> {
    let little = match t.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_of = |b: &[u8]| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let short = |v: u16| if little { v.to_le_bytes() } else { v.to_be_bytes() };
    let long = |v: u32| if little { v.to_le_bytes() } else { v.to_be_bytes() };
    let b = t.get(4..8)?;
    let ifd = if little { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) } as usize;
    let n = u16_of(t.get(ifd..ifd + 2)?) as usize;
    let entries = t.get(ifd + 2..ifd + 2 + 12 * n)?;
    let next = t.get(ifd + 2 + 12 * n..ifd + 6 + 12 * n)?;
    let mut out = t.to_vec();
    if out.len() % 2 == 1 {
        out.push(0);
    }
    let at = out.len() as u32;
    for (n, d) in [d.x, d.y] {
        out.extend(long(n));
        out.extend(long(d));
    }
    let entry = |tag: u16, ty: u16, value: [u8; 4]| [&short(tag)[..], &short(ty), &long(1), &value].concat();
    let mut unit = [0; 4];
    unit[..2].copy_from_slice(&short(unit_code(d.unit)));
    let mut list: Vec<Vec<u8>> = entries.chunks(12).filter(|e| !TAGS.contains(&u16_of(e))).map(<[u8]>::to_vec).collect();
    list.extend([entry(TAGS[0], 5, long(at)), entry(TAGS[1], 5, long(at + 8)), entry(TAGS[2], 3, unit)]);
    list.sort_by_key(|e| u16_of(e));
    let new_ifd = out.len() as u32;
    out.extend(short(list.len() as u16));
    out.extend(list.concat());
    out.extend_from_slice(next);
    out[4..8].copy_from_slice(&long(new_ifd));
    Some(out)
}

/// The resolution info of a Photoshop block (resource 1005): always in dots per inch, as 16.16
/// fixed-point numbers (the unit it names is only the one Photoshop shows).
fn photoshop(irb: &[u8]) -> Option<Density> {
    let r = crate::meta_write::resource_data(irb, 1005)?;
    let fixed = |i: usize| r.get(i..i + 4).map(|b| (u32::from_be_bytes([b[0], b[1], b[2], b[3]]), 65536));
    Density::new(Unit::Inch, fixed(0)?, fixed(8)?)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{Format, Options, Original};
    use crate::transform::{Op, Turn};
    use image::DynamicImage;

    const DPI600: Density = Density { unit: Unit::Inch, x: (600, 1), y: (600, 1) };
    const DPI72: Density = Density { unit: Unit::Inch, x: (72, 1), y: (72, 1) };
    const CM118: Density = Density { unit: Unit::Cm, x: (118, 1), y: (118, 1) };
    /// Pixels twice as wide as high, without unit.
    const WIDE: Density = Density { unit: Unit::Aspect, x: (2, 1), y: (1, 1) };
    const WRITTEN: [Format; 6] = [Format::Jpeg, Format::Png, Format::Tiff, Format::Webp, Format::Avif, Format::Bmp];

    fn photo(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 5) as u8, (y * 5) as u8, 90])))
    }

    fn opts(format: Format) -> Options {
        Options { format, ..Options::default() }
    }

    fn name(f: Format) -> String {
        format!("a.{}", f.ext())
    }

    /// A `f` file with the density `d`, where `f` can hold it.
    fn source(f: Format, d: Option<Density>) -> Vec<u8> {
        Original { density: d, ..Original::with_meta(photo(40, 30), &[]) }.encode(&opts(f)).unwrap()
    }

    /// The original as Save, Save as, Convert and the editor read it.
    fn open(bytes: &[u8], f: Format) -> Original {
        Original::with_meta_of(crate::decode::decode_full(bytes, &name(f)).unwrap(), bytes, &name(f))
    }

    /// The density a later save reads from `bytes`.
    fn of(bytes: &[u8], f: Format) -> Option<Density> {
        open(bytes, f).density
    }

    /// What a `f` file keeps of `d`: a shape without unit only in JPEG, PNG and TIFF.
    fn held(d: Option<Density>, f: Format) -> Option<Density> {
        d.filter(|d| d.unit != Unit::Aspect || matches!(f, Format::Jpeg | Format::Png | Format::Tiff))
    }

    fn png_with_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut png = source(Format::Png, None);
        let at = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
        let mut c = Vec::new();
        crate::meta_write::chunk(&mut c, kind, data);
        png.splice(at..at, c);
        png
    }

    fn exif_of(fields: &[(Tag, Value)]) -> Vec<u8> {
        let fields: Vec<exif::Field> = fields.iter().map(|(tag, value)| exif::Field { tag: *tag, ifd_num: In::PRIMARY, value: value.clone() }).collect();
        crate::meta::write_exif(&fields, false).unwrap()
    }

    fn rational(n: u32, d: u32) -> Value {
        Value::Rational(vec![exif::Rational { num: n, denom: d }])
    }

    /// Each place a density is read from, as other programs write it.
    #[test]
    fn reads() {
        // pHYs: 23622 dots per metre are 600 dpi; 11800 are 118 per cm; unit 0 is a shape.
        let phys = |x: u32, y: u32, unit: u8| png_with_chunk(b"pHYs", &[&x.to_be_bytes()[..], &y.to_be_bytes(), &[unit]].concat());
        assert_eq!(png(&phys(23622, 23622, 1)), Some(DPI600));
        assert_eq!(png(&phys(11800, 11800, 1)), Some(CM118));
        assert_eq!(png(&phys(2, 1, 0)), Some(WIDE));
        assert_eq!(png(&phys(1, 1, 0)), None);
        assert_eq!(png(&phys(0, 2835, 1)), None);
        // BMP: 2835 dots per metre are 72 dpi; 0 is none.
        let mut b = source(Format::Bmp, None);
        assert_eq!(bmp(&b), None);
        b[38..42].copy_from_slice(&2835i32.to_le_bytes());
        b[42..46].copy_from_slice(&2835i32.to_le_bytes());
        assert_eq!(bmp(&b), Some(DPI72));
        // EXIF: inch without unit, centimetres, a shape (unit 1), square pixels without unit.
        assert_eq!(of_exif(&exif_of(&[(Tag::XResolution, rational(600, 1)), (Tag::YResolution, rational(600, 1))])), Some(DPI600));
        let cm = exif_of(&[(Tag::XResolution, rational(1180, 10)), (Tag::YResolution, rational(118, 1)), (Tag::ResolutionUnit, Value::Short(vec![3]))]);
        assert_eq!(of_exif(&cm), Some(CM118));
        let shape = |x| exif_of(&[(Tag::XResolution, rational(x, 1)), (Tag::YResolution, rational(1, 1)), (Tag::ResolutionUnit, Value::Short(vec![1]))]);
        assert_eq!(of_exif(&shape(2)), Some(WIDE));
        assert_eq!(of_exif(&shape(1)), None);
        assert_eq!(of_exif(&exif_of(&[(Tag::Make, Value::Ascii(vec![b"Cam".to_vec()]))])), None);
        // Photoshop: 300 dpi as 16.16 fixed point, whatever unit it shows.
        let fixed = [&(300u32 << 16).to_be_bytes()[..], &[0, 2, 0, 1]].concat();
        let irb = crate::meta_write::resource(1005, &[&fixed[..], &fixed].concat());
        assert_eq!(photoshop(&irb), Density::new(Unit::Inch, (300, 1), (300, 1)));
        // JPEG XL: the EXIF in its "Exif" box, after an offset of 4 bytes.
        let tiff = exif_of(&[(Tag::XResolution, rational(300, 1)), (Tag::YResolution, rational(300, 1))]);
        let exif_box = [&(8 + 8 + tiff.len() as u32).to_be_bytes()[..], b"Exif", &4u32.to_be_bytes(), &[9; 4], &tiff].concat();
        let jxl = [&b"\0\0\0\x0CJXL \r\n\x87\n"[..], &[0, 0, 0, 20], b"ftypjxl \0\0\0\0jxl ", &exif_box].concat();
        let exif = crate::meta::jxl_exif(&jxl).unwrap();
        assert_eq!(read(&jxl, Some(&exif), None), Density::new(Unit::Inch, (300, 1), (300, 1)));
        assert_eq!(crate::meta::jxl_exif(&jxl[..jxl.len() - 3]), None);
        // JFIF: the unit-less 1:1 libjpeg-turbo writes is no density.
        assert_eq!(of(&source(Format::Jpeg, None), Format::Jpeg), None);
    }

    /// The file's own place first, then the EXIF, then the Photoshop block; a shape without unit
    /// only when nothing has a unit.
    #[test]
    fn which_first() {
        let exif = exif_of(&[(Tag::XResolution, rational(72, 1)), (Tag::YResolution, rational(72, 1))]);
        let fixed = [&(300u32 << 16).to_be_bytes()[..], &[0, 1, 0, 1]].concat();
        let irb = crate::meta_write::resource(1005, &[&fixed[..], &fixed].concat());
        let jpeg600 = source(Format::Jpeg, Some(DPI600));
        assert_eq!(read(&jpeg600, Some(&exif), Some(&irb)), Some(DPI600));
        let wide = source(Format::Jpeg, Some(WIDE));
        assert_eq!(read(&wide, Some(&exif), Some(&irb)), Some(DPI72));
        assert_eq!(read(&wide, None, Some(&irb)), Density::new(Unit::Inch, (300, 1), (300, 1)));
        assert_eq!(read(&wide, None, None), Some(WIDE));
        assert_eq!(read(b"not an image", None, None), None);
    }

    /// Dots per inch through dots per metre and back without drift, and per centimetre exact.
    #[test]
    fn per_metre_round_trip() {
        for dpi in [1, 72, 96, 150, 300, 600, 1200, 2400, 4800, 9600] {
            let d = Density::new(Unit::Inch, (dpi, 1), (dpi, 1)).unwrap();
            let (x, y) = d.per_metre_counts().unwrap();
            assert_eq!(Density::per_metre(x, y), Some(d), "{dpi} dpi");
        }
        assert_eq!(DPI600.per_metre_counts(), Some((23622, 23622)));
        assert_eq!(Density::per_metre(2834, 2834), Some(DPI72));
        assert_eq!(Density::per_metre(11800, 11800), Some(CM118));
        // Not a whole number of dots per inch: per centimetre, the count kept exactly.
        let odd = Density::per_metre(5005, 5005).unwrap();
        assert_eq!(odd, Density::new(Unit::Cm, (5005, 100), (5005, 100)).unwrap());
        assert_eq!(odd.per_metre_counts(), Some((5005, 5005)));
        assert_eq!(WIDE.per_metre_counts(), None);
    }

    /// Every format EyeCanSee writes with a density, saved as every other one: 600 and 72 dpi,
    /// 118 dots per centimetre, none, a shape without unit (only in JPEG, PNG and TIFF).
    #[test]
    fn every_source_to_every_target() {
        for d in [Some(DPI600), Some(DPI72), Some(CM118), None, Some(WIDE)] {
            for s in WRITTEN {
                let file = source(s, d);
                assert_eq!(of(&file, s), held(d, s), "{d:?} in {s:?}");
                let original = open(&file, s);
                for t in WRITTEN {
                    let out = original.encode(&opts(t)).unwrap();
                    // A TIFF's tags come over as EXIF, a shape without unit too: WebP and AVIF keep it there.
                    let kept = if s == Format::Tiff && matches!(t, Format::Webp | Format::Avif) { held(d, s) } else { held(held(d, s), t) };
                    assert_eq!(of(&out, t), kept, "{d:?}: {s:?} to {t:?}");
                }
            }
        }
    }

    /// The carriers themselves: the JFIF segment, the pHYs chunk, the TIFF tags, the BMP header.
    #[test]
    fn written_where_each_format_keeps_it() {
        assert_eq!(jfif::read(&source(Format::Jpeg, Some(DPI600))), Some(jfif::Density { unit: 1, x: 600, y: 600 }));
        assert_eq!(jfif::read(&source(Format::Jpeg, Some(CM118))), Some(jfif::Density { unit: 2, x: 118, y: 118 }));
        assert_eq!(jfif::read(&source(Format::Jpeg, Some(WIDE))), Some(jfif::Density { unit: 0, x: 2, y: 1 }));
        let png = source(Format::Png, Some(DPI600));
        assert_eq!(png_chunks(&png).iter().filter(|c| c.1 == b"pHYs").count(), 1);
        assert_eq!(png_chunks(&set_png(png.clone(), DPI72)).iter().filter(|c| c.1 == b"pHYs").count(), 1);
        assert_eq!(self::png(&set_png(png, DPI72)), Some(DPI72));
        let tiff = source(Format::Tiff, Some(CM118));
        let mut t = tiff::decoder::Decoder::new(std::io::Cursor::new(&tiff)).unwrap();
        assert_eq!(t.get_tag_u32_vec(tiff::tags::Tag::XResolution).unwrap(), [118, 1]);
        assert_eq!(t.get_tag_u32(tiff::tags::Tag::ResolutionUnit).unwrap(), 3);
        let bmp = source(Format::Bmp, Some(DPI72));
        assert_eq!(i32::from_le_bytes(bmp[38..42].try_into().unwrap()), 2835);
        // A WebP from an original without EXIF gets one with the density alone.
        let webp = source(Format::Webp, Some(DPI600));
        let exif = open(&webp, Format::Webp).exif.unwrap();
        assert_eq!(of_exif(&exif), Some(DPI600));
    }

    /// A resize (the editor's, Convert and resize's) changes the pixels, not the density.
    #[test]
    fn resize_keeps_dpi() {
        for s in WRITTEN {
            let original = open(&source(s, Some(DPI600)), s).with_image(photo(20, 15));
            for t in WRITTEN {
                let out = original.encode(&opts(t)).unwrap();
                assert_eq!(crate::decode::decode_full(&out, &name(t)).unwrap().width(), 20);
                assert_eq!(of(&out, t), Some(DPI600), "{s:?} to {t:?}");
            }
        }
    }

    /// Saved again and again through every format, the density does not drift: 600 dpi comes
    /// back 600, and an odd count per metre comes back the same count.
    #[test]
    fn no_drift() {
        let mut file = source(Format::Jpeg, Some(DPI600));
        let mut from = Format::Jpeg;
        for t in [Format::Png, Format::Tiff, Format::Bmp, Format::Webp, Format::Avif, Format::Png, Format::Jpeg] {
            file = open(&file, from).encode(&opts(t)).unwrap();
            from = t;
            assert_eq!(of(&file, t), Some(DPI600), "{t:?}");
        }
        let odd = png_with_chunk(b"pHYs", &[&5005u32.to_be_bytes()[..], &5005u32.to_be_bytes(), &[1]].concat());
        let tiff = open(&odd, Format::Png).encode(&opts(Format::Tiff)).unwrap();
        let back = open(&tiff, Format::Tiff).encode(&opts(Format::Png)).unwrap();
        assert_eq!(png(&back).and_then(Density::per_metre_counts), Some((5005, 5005)));
    }

    /// The EXIF carried over says the same density as the rest, everything else in it kept; one
    /// that already says it is not touched.
    #[test]
    fn exif_set() {
        let exif = exif_of(&[(Tag::Make, Value::Ascii(vec![b"Cam".to_vec()])), (Tag::XResolution, rational(72, 1)), (Tag::YResolution, rational(72, 1)), (Tag::ResolutionUnit, Value::Short(vec![2]))]);
        let set = set_exif(Some(exif.clone()), DPI600, false).unwrap();
        assert_eq!(of_exif(&set), Some(DPI600));
        let e = exif::Reader::new().read_raw(set).unwrap();
        assert_eq!(e.get_field(Tag::Make, In::PRIMARY).unwrap().display_value().to_string(), "\"Cam\"");
        assert_eq!(set_exif(Some(exif.clone()), DPI72, false), Some(exif.clone()));
        assert_eq!(set_exif(Some(exif.clone()), WIDE, true), Some(exif));
        assert_eq!(set_exif(None, DPI600, false), None);
        assert_eq!(set_exif(None, WIDE, true), None);
        // A JPEG with JFIF at 600 dpi and an EXIF at 72: the WebP made from it says 600.
        let jpeg = Original { density: Some(DPI600), exif: Some(exif_of(&[(Tag::XResolution, rational(72, 1)), (Tag::YResolution, rational(72, 1))])), ..Original::with_meta(photo(40, 30), &[]) };
        let webp = open(&jpeg.encode(&opts(Format::Jpeg)).unwrap(), Format::Jpeg).encode(&opts(Format::Webp)).unwrap();
        assert_eq!(of_exif(&open(&webp, Format::Webp).exif.unwrap()), Some(DPI600));
    }

    /// Without "Keep the metadata" no density is written, as no other metadata; an original
    /// with a density alone has metadata to keep (Save in another format lets it be left out).
    #[test]
    fn not_without_metadata() {
        let original = open(&source(Format::Png, Some(DPI600)), Format::Png);
        assert!(original.has_meta());
        assert!(!open(&source(Format::Png, None), Format::Png).has_meta());
        for t in WRITTEN {
            let out = original.encode(&Options { keep_meta: false, ..opts(t) }).unwrap();
            assert_eq!(of(&out, t), None, "{t:?}");
        }
    }

    /// A quarter turn exchanges the density across and down; a half turn does not.
    #[test]
    fn quarter_turn() {
        let d = Density::new(Unit::Cm, (118, 1), (59, 1)).unwrap();
        let original = open(&source(Format::Png, Some(d)), Format::Png);
        let cw = Turn::default().then(Op::Cw);
        for t in [Format::Jpeg, Format::Png, Format::Tiff] {
            assert_eq!(of(&original.encode(&Options { turn: cw, ..opts(t) }).unwrap(), t), Some(d.swapped()), "{t:?}");
            assert_eq!(of(&original.encode(&Options { turn: Turn::default().then(Op::Half), ..opts(t) }).unwrap(), t), Some(d), "{t:?}");
        }
        assert_eq!(original.clone().turned(cw).density, Some(d.swapped()));
    }
}
