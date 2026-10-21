//! EXIF metadata of the selected photo, for the panel below the grid. Read only, with
//! `kamadak-exif` (pure Rust: files also come from phones and servers). Reads JPEG, HEIC/AVIF,
//! PNG, WebP and TIFF.

use std::io::Cursor;

use exif::{Exif, In, Reader, Tag, Value};

/// Boxes of the panel: each one closes and reopens on its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Camera,
    Shot,
    Image,
    Gps,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Camera, Group::Shot, Group::Image, Group::Gps];

    /// Name in the settings and, with `meta-` in front, Fluent id of the title.
    pub fn key(self) -> &'static str {
        match self {
            Group::Camera => "camera",
            Group::Shot => "shot",
            Group::Image => "image",
            Group::Gps => "gps",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    /// (box, Fluent id of the label, value with its unit). A value starting with `meta-` is itself
    /// a Fluent id (flash, colour space): kamadak-exif describes them only in English.
    /// Dates are ISO (`2026-10-09 17:48:35`, `2026-10-09`): the UI formats them.
    pub fields: Vec<(Group, &'static str, String)>,
    /// Latitude and longitude in decimal degrees (negative to the south and west).
    pub gps: Option<(f64, f64)>,
}

const FIELDS: &[(Group, Tag, &str)] = &[
    (Group::Camera, Tag::Make, "meta-make"),
    (Group::Camera, Tag::Model, "meta-model"),
    (Group::Camera, Tag::LensModel, "meta-lens"),
    (Group::Camera, Tag::Software, "meta-software"),
    (Group::Shot, Tag::DateTimeOriginal, "meta-taken"),
    (Group::Shot, Tag::ExposureTime, "meta-exposure"),
    (Group::Shot, Tag::FNumber, "meta-aperture"),
    (Group::Shot, Tag::PhotographicSensitivity, "meta-iso"),
    (Group::Shot, Tag::FocalLength, "meta-focal"),
    (Group::Shot, Tag::Flash, "meta-flash"),
    (Group::Image, Tag::PixelXDimension, "meta-width"),
    (Group::Image, Tag::PixelYDimension, "meta-height"),
    (Group::Image, Tag::ColorSpace, "meta-colorspace"),
    (Group::Gps, Tag::GPSAltitude, "meta-altitude"),
    (Group::Gps, Tag::GPSDateStamp, "meta-gps-date"),
];

/// `None` if the file has no EXIF (or it cannot be read).
pub fn read(bytes: &[u8]) -> Option<Meta> {
    let exif = Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()?;
    let mut meta = Meta::default();
    for &(group, tag, label) in FIELDS {
        if let Some(f) = exif.get_field(tag, In::PRIMARY) {
            let value = value(&exif, f);
            if !value.is_empty() {
                meta.fields.push((group, label, value));
            }
        }
    }
    let lat = coord(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, 'S');
    let lon = coord(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, 'W');
    meta.gps = lat.zip(lon);
    Some(meta)
}

/// Readable value. Texts: only the non-empty parts, without quotes (the POCO X7 Pro writes
/// "MediaTek Camera Application" followed by empty strings). Exposure as 1/33 s (kamadak-exif
/// would write 1/33.31450619539877 s).
fn value(exif: &Exif, f: &exif::Field) -> String {
    match (&f.value, f.tag) {
        // EXIF writes dates as 2026:10:09 17:48:35.
        (Value::Ascii(parts), Tag::DateTimeOriginal | Tag::GPSDateStamp) => {
            let s = parts.first().map(|p| String::from_utf8_lossy(p).trim().to_string()).unwrap_or_default();
            s.char_indices().map(|(i, c)| if i < 10 && c == ':' { '-' } else { c }).collect()
        }
        (Value::Ascii(parts), _) => parts
            .iter()
            .map(|p| String::from_utf8_lossy(p).trim().to_string())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        (Value::Rational(v), Tag::ExposureTime) if !v.is_empty() => {
            let secs = v[0].to_f64();
            if secs > 0.0 && secs < 1.0 { format!("1/{:.0} s", 1.0 / secs) } else { format!("{secs} s") }
        }
        (_, Tag::Flash) => {
            let fired = f.value.get_uint(0).is_some_and(|v| v & 1 == 1);
            (if fired { "meta-flash-yes" } else { "meta-flash-no" }).to_string()
        }
        (_, Tag::ColorSpace) => match f.value.get_uint(0) {
            Some(1) => "sRGB".to_string(),
            _ => "meta-color-uncalibrated".to_string(),
        },
        (_, Tag::PixelXDimension | Tag::PixelYDimension) => format!("{} px", f.display_value()),
        (Value::Rational(v), Tag::GPSAltitude) if !v.is_empty() => {
            // Reference 1 = below sea level.
            let below = exif.get_field(Tag::GPSAltitudeRef, In::PRIMARY).and_then(|r| r.value.get_uint(0)) == Some(1);
            format!("{:.0} m", if below { -v[0].to_f64() } else { v[0].to_f64() })
        }
        _ => f.display_value().with_unit(exif).to_string(),
    }
}

/// Degrees, minutes and seconds to decimal degrees; the reference (N/S, E/W) gives the sign.
fn coord(exif: &Exif, tag: Tag, reference: Tag, negative: char) -> Option<f64> {
    let Value::Rational(ref v) = exif.get_field(tag, In::PRIMARY)?.value else { return None };
    let part = |i: usize, div: f64| v.get(i).map_or(0.0, |r| r.to_f64() / div);
    let degrees = v.first()?.to_f64() + part(1, 60.0) + part(2, 3600.0);
    let south_west = exif.get_field(reference, In::PRIMARY).is_some_and(|f| f.display_value().to_string().contains(negative));
    Some(if south_west { -degrees } else { degrees })
}

#[cfg(test)]
mod tests {
    use exif::experimental::Writer;
    use exif::{Field, In, Rational, Tag, Value};

    fn field(tag: Tag, value: Value) -> Field {
        Field { tag, ifd_num: In::PRIMARY, value }
    }

    #[test]
    fn camera_and_gps() {
        let fields = [
            field(Tag::Make, Value::Ascii(vec![b"Xiaomi".to_vec()])),
            field(Tag::Model, Value::Ascii(vec![b"POCO X7 Pro".to_vec()])),
            field(Tag::Software, Value::Ascii(vec![b"MediaTek Camera Application".to_vec(), vec![], vec![]])),
            field(Tag::ExposureTime, Value::Rational(vec![(1_000_000, 33_314_506).into()])),
            field(Tag::Flash, Value::Short(vec![16])),
            field(Tag::DateTimeOriginal, Value::Ascii(vec![b"2026:10:09 17:48:35".to_vec()])),
            field(Tag::GPSAltitude, Value::Rational(vec![(980999, 10000).into()])),
            field(Tag::GPSLatitudeRef, Value::Ascii(vec![b"N".to_vec()])),
            field(Tag::GPSLatitude, Value::Rational(vec![(45, 1).into(), (30, 1).into(), Rational::from((0, 1))])),
            field(Tag::GPSLongitudeRef, Value::Ascii(vec![b"W".to_vec()])),
            field(Tag::GPSLongitude, Value::Rational(vec![(9, 1).into(), (15, 1).into(), Rational::from((36, 1))])),
        ];
        let mut writer = Writer::new();
        for f in &fields {
            writer.push_field(f);
        }
        let mut tiff = std::io::Cursor::new(Vec::new());
        writer.write(&mut tiff, false).unwrap();

        let meta = super::read(tiff.get_ref()).unwrap();
        let get = |id| meta.fields.iter().find(|(_, l, _)| *l == id).map(|(_, _, v)| v.as_str());
        assert_eq!(get("meta-make"), Some("Xiaomi"));
        assert_eq!(get("meta-model"), Some("POCO X7 Pro"));
        assert_eq!(get("meta-software"), Some("MediaTek Camera Application"));
        assert_eq!(get("meta-exposure"), Some("1/33 s"));
        assert_eq!(get("meta-flash"), Some("meta-flash-no"));
        assert_eq!(get("meta-taken"), Some("2026-10-09 17:48:35"));
        assert_eq!(get("meta-altitude"), Some("98 m"));
        let (lat, lon) = meta.gps.unwrap();
        assert!((lat - 45.5).abs() < 1e-9);
        assert!((lon + 9.26).abs() < 1e-9);
        assert_eq!(super::read(b"not an image"), None);
    }
}
