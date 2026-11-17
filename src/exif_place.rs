//! The position of a photo's sidecar (#24) in the EXIF of its copy, for the photos that are not
//! RAW (those have it from `raw_meta::read_placed`): the copy's XMP had the sidecar's position and
//! its EXIF still the camera's, two places in one file, and most programs read the EXIF (Piero,
//! 17/11, altitude). Only the GPS directory changes: a new one is written at the end of the block
//! and pointed to, nothing else moves, so every offset (the maker note's too) stays right, as
//! `density::exif_with` does for the resolution.

use crate::raw_meta::Place;

const GPS_POINTER: u16 = 0x8825;
/// Latitude and longitude, with their N/S and E/W.
const POSITION: [u16; 4] = [1, 2, 3, 4];
/// Altitude and above or below sea level.
const ALTITUDE: [u16; 2] = [5, 6];
const BYTE: u16 = 1;
const ASCII: u16 = 2;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;

/// Nearer than this (degrees, about 30 m) the sidecar's position is the camera's rounded: the XMP
/// writes minutes with two decimals (`40,21.09N`, 18 m), a phone's EXIF to the hundredth of a
/// second. The camera's, more exact, stays.
const SAME_PLACE: f64 = 0.0003;
/// The same for the altitude, in metres (`413/5` in a sidecar for the camera's 82.69).
const SAME_HEIGHT: f64 = 0.5;

/// The TIFF block `t` with the position `place` where it differs from the camera's: the camera's
/// altitude kept when the sidecar has none, or only rounds it. `None` for a block not readable,
/// and when nothing would change.
pub fn placed(t: &[u8], place: Place) -> Option<Vec<u8>> {
    let (camera, height) = camera_place(t);
    let ((lat, lon), alt) = place;
    let moved = camera.is_none_or(|(a, b)| (a - lat).abs() > SAME_PLACE || (b - lon).abs() > SAME_PLACE);
    let alt = alt.filter(|a| height.is_none_or(|h| (h - a).abs() > SAME_HEIGHT));
    if !moved && alt.is_none() {
        return None;
    }
    // The camera's position kept where only the altitude changes.
    let (lat, lon) = if moved { (lat, lon) } else { camera.unwrap_or((lat, lon)) };
    write(t, ((lat, lon), alt))
}

/// The position and altitude the EXIF `t` has, as `kamadak-exif` reads them.
fn camera_place(t: &[u8]) -> (Option<(f64, f64)>, Option<f64>) {
    let Ok(e) = exif::Reader::new().read_raw(t.to_vec()) else { return (None, None) };
    let num = |tag: exif::Tag| e.get_field(tag, exif::In::PRIMARY).map(|f| f.value.clone());
    let degrees = |tag, side: exif::Tag, minus: &str| match num(tag) {
        Some(exif::Value::Rational(v)) if v.len() == 3 => {
            let d = v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0;
            let neg = matches!(num(side), Some(exif::Value::Ascii(a)) if a.first().is_some_and(|s| s.starts_with(minus.as_bytes())));
            Some(if neg { -d } else { d })
        }
        _ => None,
    };
    let lat = degrees(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef, "S");
    let lon = degrees(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef, "W");
    let below = matches!(num(exif::Tag::GPSAltitudeRef), Some(exif::Value::Byte(b)) if b.first() == Some(&1));
    let height = match num(exif::Tag::GPSAltitude) {
        Some(exif::Value::Rational(v)) if !v.is_empty() => Some(if below { -v[0].to_f64() } else { v[0].to_f64() }),
        _ => None,
    };
    (lat.zip(lon), height)
}

/// `t` with the position and, if any, the altitude written.
fn write(t: &[u8], ((lat, lon), alt): Place) -> Option<Vec<u8>> {
    let little = match t.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_of = |b: &[u8]| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let u32_of = |b: &[u8]| if little { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) };
    let short = |v: u16| if little { v.to_le_bytes() } else { v.to_be_bytes() };
    let long = |v: u32| if little { v.to_le_bytes() } else { v.to_be_bytes() };
    let entries_at = |ifd: usize| -> Option<Vec<Vec<u8>>> {
        let n = u16_of(t.get(ifd..ifd + 2)?) as usize;
        Some(t.get(ifd + 2..ifd + 2 + 12 * n)?.chunks(12).map(<[u8]>::to_vec).collect())
    };
    let ifd0 = u32_of(t.get(4..8)?) as usize;
    let main = entries_at(ifd0)?;
    let pointer = main.iter().position(|e| u16_of(e) == GPS_POINTER);
    let old = match pointer {
        Some(i) => entries_at(u32_of(&main[i][8..]) as usize).unwrap_or_default(),
        None => Vec::new(),
    };
    let mut out = t.to_vec();
    if out.len() % 2 == 1 {
        out.push(0);
    }
    // The values that do not fit in an entry, then the entries.
    let mut data = |values: Vec<u32>| {
        let at = out.len() as u32;
        out.extend(values.into_iter().flat_map(long));
        at
    };
    let dms = |v: f64| {
        let v = v.abs();
        let (d, m) = (v.trunc(), (v.fract() * 60.0).trunc());
        let s = ((v * 3600.0 - d * 3600.0 - m * 60.0) * 1000.0).round().max(0.0);
        vec![d as u32, 1, m as u32, 1, s as u32, 1000]
    };
    let (lat_at, lon_at) = (data(dms(lat)), data(dms(lon)));
    let alt_at = alt.map(|a| data(vec![(a.abs() * 100.0).round() as u32, 100]));
    let entry = |tag: u16, ty: u16, count: u32, value: [u8; 4]| [&short(tag)[..], &short(ty), &long(count), &value].concat();
    let side = |v: f64, plus: u8, minus: u8| [if v < 0.0 { minus } else { plus }, 0, 0, 0];
    let gone = |tag: u16| POSITION.contains(&tag) || (alt.is_some() && ALTITUDE.contains(&tag));
    let mut list: Vec<Vec<u8>> = old.into_iter().filter(|e| !gone(u16_of(e))).collect();
    if !list.iter().any(|e| u16_of(e) == 0) {
        list.push(entry(0, BYTE, 4, [2, 3, 0, 0]));
    }
    list.extend([entry(1, ASCII, 2, side(lat, b'N', b'S')), entry(2, RATIONAL, 3, long(lat_at)), entry(3, ASCII, 2, side(lon, b'E', b'W')), entry(4, RATIONAL, 3, long(lon_at))]);
    if let (Some(a), Some(at)) = (alt, alt_at) {
        list.extend([entry(5, BYTE, 1, [u8::from(a < 0.0), 0, 0, 0]), entry(6, RATIONAL, 1, long(at))]);
    }
    list.sort_by_key(|e| u16_of(e));
    let gps = out.len() as u32;
    out.extend(short(list.len() as u16));
    out.extend(list.concat());
    out.extend(long(0));
    match pointer {
        // The pointer's value is in its entry: changed where it is.
        Some(i) => {
            let at = ifd0 + 2 + 12 * i + 8;
            out[at..at + 4].copy_from_slice(&long(gps));
        }
        // No GPS before: a new main directory with the pointer, at the end.
        None => {
            let next = t.get(ifd0 + 2 + 12 * main.len()..ifd0 + 6 + 12 * main.len())?.to_vec();
            let mut list = main;
            list.push(entry(GPS_POINTER, LONG, 1, long(gps)));
            list.sort_by_key(|e| u16_of(e));
            let new_ifd = out.len() as u32;
            out.extend(short(list.len() as u16));
            out.extend(list.concat());
            out.extend(next);
            out[4..8].copy_from_slice(&long(new_ifd));
        }
    }
    Some(out)
}

/// The place of the XMP of a `sidecar`, as `placed` takes it; `None` without a position.
pub fn of_sidecar(sidecar: &[u8]) -> Option<Place> {
    let f = crate::xmp_fields::read(sidecar);
    Some((f.gps?, f.altitude))
}

#[cfg(test)]
mod tests {
    use super::*;
    use exif::{Field, In, Rational, Tag, Value};

    fn field(tag: Tag, value: Value) -> Field {
        Field { tag, ifd_num: In::PRIMARY, value }
    }

    fn get(t: &[u8], tag: Tag) -> Option<String> {
        let e = exif::Reader::new().read_raw(t.to_vec()).ok()?;
        e.get_field(tag, In::PRIMARY).map(|f| f.display_value().to_string())
    }

    /// The camera's position replaced by the sidecar's, the altitude too; the other fields (one
    /// with its value after the entries, as the maker's name) still read.
    #[test]
    fn the_sidecar_position_in_the_exif() {
        for little in [true, false] {
            let ascii = |s: &str| Value::Ascii(vec![s.as_bytes().to_vec()]);
            let r = |n, d| Rational { num: n, denom: d };
            let fields = [
                field(Tag::Make, ascii("Google camera")),
                field(Tag::GPSLatitudeRef, ascii("N")),
                field(Tag::GPSLatitude, Value::Rational(vec![r(45, 1), r(25, 1), r(2543, 100)])),
                field(Tag::GPSLongitudeRef, ascii("W")),
                field(Tag::GPSLongitude, Value::Rational(vec![r(75, 1), r(41, 1), r(147, 10)])),
                field(Tag::GPSAltitude, Value::Rational(vec![r(8269, 100)])),
                field(Tag::GPSImgDirection, Value::Rational(vec![r(312, 1)])),
            ];
            let t = crate::meta::write_exif(&fields, little).unwrap();
            let out = placed(&t, ((40.351131, 18.175), Some(95.0))).unwrap();
            assert_eq!(get(&out, Tag::GPSLatitude).unwrap(), "40 deg 21 min 4.072 sec");
            assert_eq!((get(&out, Tag::GPSLatitudeRef).unwrap(), get(&out, Tag::GPSLongitudeRef).unwrap()), ("N".to_string(), "E".to_string()));
            assert_eq!(get(&out, Tag::GPSLongitude).unwrap(), "18 deg 10 min 30 sec");
            assert_eq!(get(&out, Tag::GPSAltitude).unwrap(), "95");
            // An altitude that only rounds the camera's (82.6 for 82.69): the camera's stays.
            assert_eq!(get(&placed(&t, ((40.351131, 18.175), Some(82.6))).unwrap(), Tag::GPSAltitude).unwrap(), "82.69");
            assert_eq!(get(&out, Tag::GPSImgDirection).unwrap(), "312", "the other GPS fields stay");
            assert_eq!(get(&out, Tag::Make).unwrap(), "\"Google camera\"");
            // Without an altitude in the sidecar, the camera's stays.
            assert_eq!(get(&placed(&t, ((-1.5, -2.0), None)).unwrap(), Tag::GPSAltitude).unwrap(), "82.69");
        }
    }

    /// A sidecar that only rounds the camera's values (`40,21.09N`, `413/5`) changes nothing: the
    /// camera's are more exact. A new altitude alone keeps the camera's position.
    #[test]
    fn a_rounded_copy_changes_nothing() {
        let r = |n, d| Rational { num: n, denom: d };
        let ascii = |s: &str| Value::Ascii(vec![s.as_bytes().to_vec()]);
        let fields = [
            field(Tag::GPSLatitudeRef, ascii("N")),
            field(Tag::GPSLatitude, Value::Rational(vec![r(40, 1), r(21, 1), r(543, 100)])),
            field(Tag::GPSLongitudeRef, ascii("E")),
            field(Tag::GPSLongitude, Value::Rational(vec![r(18, 1), r(10, 1), r(3012, 100)])),
            field(Tag::GPSAltitude, Value::Rational(vec![r(8269, 100)])),
        ];
        let t = crate::meta::write_exif(&fields, true).unwrap();
        let (lat, lon) = (40.0 + 21.09 / 60.0, 18.0 + 10.5 / 60.0);
        assert!(placed(&t, ((lat, lon), Some(82.6))).is_none());
        let higher = placed(&t, ((lat, lon), Some(120.0))).unwrap();
        assert_eq!(get(&higher, Tag::GPSAltitude).unwrap(), "120");
        assert_eq!(get(&higher, Tag::GPSLatitude).unwrap(), "40 deg 21 min 5.43 sec", "the camera's position, exact");
    }

    /// An EXIF without GPS gets one, its other fields kept.
    #[test]
    fn a_gps_directory_where_there_was_none() {
        let t = crate::meta::write_exif(&[field(Tag::Make, Value::Ascii(vec![b"Maker name".to_vec()]))], true).unwrap();
        let out = placed(&t, ((-33.5, 151.25), Some(-3.0))).unwrap();
        assert_eq!((get(&out, Tag::GPSLatitudeRef).unwrap(), get(&out, Tag::GPSLongitudeRef).unwrap()), ("S".to_string(), "E".to_string()));
        assert_eq!(get(&out, Tag::GPSAltitudeRef).unwrap(), "below sea level");
        assert_eq!(get(&out, Tag::Make).unwrap(), "\"Maker name\"");
        assert!(placed(b"not a tiff", ((0.0, 0.0), None)).is_none());
    }
}
