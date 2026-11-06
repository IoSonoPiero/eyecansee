//! EXIF metadata of the selected photo, for the panel below the grid, with `kamadak-exif` (pure
//! Rust: files also come from phones and servers). Reads JPEG, HEIC/AVIF, PNG, WebP, TIFF and the
//! RAW files built on TIFF. `exif_chunk` hands the same EXIF to Save in another format,
//! `fit_exif` makes it fit the pixels written (sizes, no old thumbnail).

use std::io::Cursor;

use exif::{Exif, In, Reader, Tag, Value};

/// Boxes of the panel: each one closes and reopens on its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    /// Name, size, date and pixels of the file: from the listing, not from its metadata.
    File,
    Camera,
    Shot,
    Image,
    Gps,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::File, Group::Camera, Group::Shot, Group::Image, Group::Gps];

    /// Name in the settings and, with `meta-` in front, Fluent id of the title.
    pub fn key(self) -> &'static str {
        match self {
            Group::File => "file",
            Group::Camera => "camera",
            Group::Shot => "shot",
            Group::Image => "image",
            Group::Gps => "gps",
        }
    }
}

/// `Meta` as saved in the cache: the box by its key, the labels as text.
#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    /// `STORED`: what an older version saved is read from the file again.
    #[serde(default)]
    version: u32,
    fields: Vec<(String, String, String)>,
    gps: Option<(f64, f64)>,
}

/// Raised when the panel learns to read something new: 1 = drone positions from the `.SRT`.
const STORED: u32 = 1;

impl Meta {
    pub fn to_json(&self) -> String {
        let fields = self.fields.iter().map(|(g, l, v)| (g.key().to_string(), l.to_string(), v.clone())).collect();
        serde_json::to_string(&Stored { version: STORED, fields, gps: self.gps }).unwrap_or_default()
    }

    /// `None` if it does not read back (saved by another version): it is read from the file again.
    pub fn from_json(json: &str) -> Option<Meta> {
        let stored: Stored = serde_json::from_str(json).ok().filter(|s: &Stored| s.version == STORED)?;
        let mut fields = Vec::new();
        for (g, l, v) in stored.fields {
            let group = Group::ALL.into_iter().find(|x| x.key() == g)?;
            fields.push((group, intern(&l), v));
        }
        Some(Meta { fields, gps: stored.gps })
    }
}

/// The labels are Fluent ids, a few dozen in all: each kept once for the whole session.
fn intern(s: &str) -> &'static str {
    static SEEN: std::sync::Mutex<std::collections::BTreeSet<&'static str>> = std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut seen = SEEN.lock().unwrap();
    match seen.get(s) {
        Some(&kept) => kept,
        None => {
            let kept: &'static str = Box::leak(s.into());
            seen.insert(kept);
            kept
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

/// The EXIF of any file the panel reads, rewritten as a standalone TIFF block for a converted copy:
/// the camera, shot and GPS fields (`exif_fields`). `None` without EXIF.
pub fn exif_chunk(bytes: &[u8]) -> Option<Vec<u8>> {
    let (fields, little) = exif_fields(bytes)?;
    write_exif(&fields, little)
}

/// The EXIF fields of a file worth keeping in a converted copy, and the byte order. Left out: the
/// tags that describe the original's own image data (strips, sensor layout, DNG tags of a RAW) and
/// the maker note, whose offsets point into the original file.
pub fn exif_fields(bytes: &[u8]) -> Option<(Vec<exif::Field>, bool)> {
    let exif = Reader::new().read_from_container(&mut Cursor::new(bytes)).ok()?;
    Some((exif.fields().filter(|f| keep_field(f)).cloned().collect(), exif.little_endian()))
}

/// A field `exif_fields` keeps.
pub fn keep_field(f: &exif::Field) -> bool {
    use exif::Context;
    const TIFF: &[Tag] = &[
        Tag::Make,
        Tag::Model,
        Tag::Software,
        Tag::DateTime,
        Tag::Artist,
        Tag::Copyright,
        Tag::ImageDescription,
        Tag::Orientation,
        Tag::XResolution,
        Tag::YResolution,
        Tag::ResolutionUnit,
        // Rating and RatingPercent (Microsoft): the stars given to the photo.
        Tag(exif::Context::Tiff, 0x4746),
        Tag(exif::Context::Tiff, 0x4749),
    ];
    // A broken entry (type 0 in a Samsung NX3000 file) would stop the whole block from being written.
    !matches!(f.value, exif::Value::Unknown(..))
        && f.ifd_num == In::PRIMARY
        && match f.tag.context() {
            Context::Tiff => TIFF.contains(&f.tag),
            // Only EXIF tags: a Mamiya ZD has SubfileType (0x00FE) in its EXIF directory.
            Context::Exif => f.tag != Tag::MakerNote && f.tag.number() >= 0x8000,
            Context::Gps => true,
            _ => false,
        }
}

/// `fields` as a TIFF block; `None` without fields. A field that cannot be written on its own is
/// left out rather than losing them all.
pub fn write_exif(fields: &[exif::Field], little: bool) -> Option<Vec<u8>> {
    let write = |fields: &[&exif::Field]| {
        let mut writer = exif::experimental::Writer::new();
        for f in fields {
            writer.push_field(f);
        }
        let mut out = Cursor::new(Vec::new());
        (!fields.is_empty() && writer.write(&mut out, little).is_ok()).then(|| out.into_inner())
    };
    let all: Vec<_> = fields.iter().collect();
    write(&all).or_else(|| write(&all.into_iter().filter(|f| write(&[f]).is_some()).collect::<Vec<_>>()))
}

/// The EXIF (TIFF data, as `exif_chunk`) of a file being written, made to fit its pixels: the
/// sizes (ImageWidth/Length, PixelX/YDimension) set to `w` × `h` and the thumbnail taken out,
/// with its bytes zeroed: it showed the original, the part cropped away included. In place, so
/// the offsets of everything else (the MakerNote too) stay right. A broken EXIF is left as it is.
pub fn fit_exif(t: &mut [u8], w: u32, h: u32) {
    let little = match t.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return,
    };
    let u16_at = |t: &[u8], i: usize| t.get(i..i + 2).map(|b| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
    let u32_at = |t: &[u8], i: usize| t.get(i..i + 4).map(|b| if little { u32::from_le_bytes(b.try_into().unwrap()) } else { u32::from_be_bytes(b.try_into().unwrap()) } as usize);
    let put = |t: &mut [u8], i: usize, b: [u8; 4]| {
        if let Some(d) = t.get_mut(i..i + 4) {
            d.copy_from_slice(&b);
        }
    };
    let long = |v: u32| if little { v.to_le_bytes() } else { v.to_be_bytes() };
    // Entries of the directory at `at` (tag, start of the entry), and where its "next" offset is.
    let entries = |t: &[u8], at: usize| {
        let n = u16_at(t, at)? as usize;
        let next = at + 2 + 12 * n;
        t.get(next..next + 4)?;
        Some(((0..n).filter_map(|k| Some((u16_at(t, at + 2 + 12 * k)?, at + 2 + 12 * k))).collect::<Vec<_>>(), next))
    };
    // A size of one value, written as a LONG (SHORT or LONG are both allowed).
    let set = |t: &mut [u8], e: usize, v: u32| {
        if u32_at(t, e + 4) == Some(1) {
            t[e + 2..e + 4].copy_from_slice(if little { &[4, 0] } else { &[0, 4] });
            put(t, e + 8, long(v));
        }
    };
    let Some(ifd0) = u32_at(t, 4) else { return };
    let Some((tags, next)) = entries(t, ifd0) else { return };
    for (tag, e) in tags {
        match tag {
            0x0100 => set(t, e, w),
            0x0101 => set(t, e, h),
            0x8769 => {
                let Some((exif, _)) = u32_at(t, e + 8).and_then(|at| entries(t, at)) else { continue };
                for (tag, e) in exif {
                    match tag {
                        0xA002 => set(t, e, w),
                        0xA003 => set(t, e, h),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    // IFD1, the thumbnail: its picture (JPEG, or one strip) and the directory zeroed, unlinked.
    let Some(ifd1) = u32_at(t, next).filter(|&at| at != 0) else { return };
    if let Some((tags, end)) = entries(t, ifd1) {
        // One value, SHORT or LONG (strips can be either).
        let value = |t: &[u8], tag| {
            let &(_, e) = tags.iter().find(|&&(x, _)| x == tag)?;
            if u16_at(t, e + 2)? == 3 { u16_at(t, e + 8).map(usize::from) } else { u32_at(t, e + 8) }
        };
        for (start, len) in [(0x0201, 0x0202), (0x0111, 0x0117)] {
            if let (Some(s), Some(l)) = (value(t, start), value(t, len))
                && let Some(d) = t.get_mut(s..s.saturating_add(l))
            {
                d.fill(0);
            }
        }
        t[ifd1..end + 4].fill(0);
    }
    put(t, next, [0; 4]);
}

/// The panel for an image: its EXIF if any, and always what the header says (format, size,
/// colour). `None` only when neither can be read.
pub fn read_file(bytes: &[u8], name: &str) -> Option<Meta> {
    let header = crate::decode::header(bytes, name);
    let mut meta = read(bytes);
    if meta.is_none() && header.is_none() {
        return None;
    }
    let meta_ref = meta.get_or_insert_default();
    if let Some((format, [w, h], color)) = header {
        let has = |label| meta_ref.fields.iter().any(|(_, l, _)| *l == label);
        let mut add = vec![(Group::Image, "meta-format", format)];
        if !has("meta-width") {
            add.push((Group::Image, "meta-width", format!("{w} px")));
            add.push((Group::Image, "meta-height", format!("{h} px")));
        }
        let (kind, bits) = color_kind(color);
        add.push((Group::Image, "meta-color", kind.to_string()));
        add.push((Group::Image, "meta-depth", format!("{bits} bit")));
        // Format first in its box, the EXIF fields after.
        meta_ref.fields.splice(0..0, add);
    }
    meta
}

/// Fluent id of the colour model and bits per channel.
fn color_kind(c: image::ColorType) -> (&'static str, u16) {
    let kind = match (c.has_color(), c.has_alpha()) {
        (true, false) => "meta-color-rgb",
        (true, true) => "meta-color-rgba",
        (false, false) => "meta-color-gray",
        (false, true) => "meta-color-graya",
    };
    (kind, c.bits_per_pixel() / u16::from(c.channel_count()))
}

/// `None` if the file has no EXIF (or it cannot be read).
pub fn read(bytes: &[u8]) -> Option<Meta> {
    let exif = match Reader::new().read_from_container(&mut Cursor::new(bytes)) {
        Ok(exif) => exif,
        // RAW files not built on TIFF: their EXIF as Convert reads it.
        Err(_) => Reader::new().read_raw(crate::raw_meta::exif(bytes)?).ok()?,
    };
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

/// The panel's boxes for a video (`mpv::info`); `size` of the file gives the bit rate.
pub fn from_video(info: &crate::mpv::Info, size: u64) -> Meta {
    let tag = |keys: &[&str]| keys.iter().find_map(|k| info.tags.iter().find(|(t, _)| t == k).map(|(_, v)| v.clone()));
    let mut meta = Meta::default();
    let mut add = |g, label, value: Option<String>| {
        if let Some(v) = value {
            meta.fields.push((g, label, v));
        }
    };
    add(Group::Camera, "meta-make", tag(&["com.android.manufacturer", "com.apple.quicktime.make", "make"]));
    add(Group::Camera, "meta-model", tag(&["com.android.model", "com.apple.quicktime.model", "model"]));
    let android = tag(&["com.android.version"]).map(|v| format!("Android {v}"));
    add(Group::Camera, "meta-software", android.or_else(|| tag(&["com.apple.quicktime.software", "encoder"])));
    add(Group::Shot, "meta-taken", tag(&["creation_time", "date_recorded"]).and_then(|t| local_time(&t)));
    add(Group::Shot, "meta-duration", info.duration.map(crate::mpv::clock));
    if let Some((codec, [w, h], rotation, fps)) = &info.video {
        let (w, h) = if rotation % 180 == 90 { (h, w) } else { (w, h) };
        add(Group::Shot, "meta-fps", fps.map(|f| format!("{} fps", trimmed(f))));
        add(Group::Image, "meta-width", Some(format!("{w} px")));
        add(Group::Image, "meta-height", Some(format!("{h} px")));
        add(Group::Image, "meta-video-codec", Some(codec_name(codec)));
    }
    if let Some((codec, rate, channels)) = &info.audio {
        let rate = rate.map(|r| format!(" {} kHz", trimmed(r as f64 / 1000.0))).unwrap_or_default();
        let channels = match channels {
            Some(1) => " mono".into(),
            Some(2) => " stereo".into(),
            Some(n) => format!(" {n} ch"),
            None => String::new(),
        };
        add(Group::Image, "meta-audio", Some(format!("{}{rate}{channels}", codec_name(codec))));
    }
    add(Group::Image, "meta-bitrate", info.duration.filter(|d| *d > 0.0).map(|d| format!("{} Mbit/s", trimmed(size as f64 * 8.0 / d / 1e6))));
    // Written by Converti e ridimensiona from a dashcam's track (`video_read::track_tags`).
    add(Group::Gps, "meta-speed-avg", tag(&["speed_average_kmh"]).map(|v| format!("{v} km/h")));
    add(Group::Gps, "meta-speed-max", tag(&["speed_top_kmh"]).map(|v| format!("{v} km/h")));
    if let Some((lat, lon, alt)) = tag(&["location", "com.apple.quicktime.location.ISO6709"]).as_deref().and_then(iso6709) {
        meta.gps = Some((lat, lon));
        if let Some(alt) = alt {
            meta.fields.push((Group::Gps, "meta-altitude", format!("{alt:.0} m")));
        }
    }
    meta
}

/// When a video starts, in Unix seconds, from its `creation_time` tag.
pub fn video_start(info: &crate::mpv::Info) -> Option<i64> {
    let t = &info.tags.iter().find(|(k, _)| *k == "creation_time" || *k == "date_recorded")?.1;
    Some(first(t).parse::<jiff::Timestamp>().ok()?.as_second())
}

/// A dashcam's track during the video (`gps_track`): where it started, altitude, speeds. Only
/// where the file itself has no location.
pub fn add_track(meta: &mut Meta, points: &[crate::gps_track::Point]) {
    let Some(&(_, lat, lon, alt)) = points.first() else { return };
    if meta.gps.is_some() {
        return;
    }
    meta.gps = Some((lat, lon));
    meta.fields.push((Group::Gps, "meta-altitude", format!("{alt:.0} m")));
    if let Some((avg, top)) = crate::gps_track::speeds(points) {
        meta.fields.push((Group::Gps, "meta-speed-avg", format!("{avg:.0} km/h")));
        meta.fields.push((Group::Gps, "meta-speed-max", format!("{top:.0} km/h")));
    }
}

/// Two decimals at most, none if round: 29.97, 48, 21.1.
fn trimmed(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn codec_name(codec: &str) -> String {
    match codec {
        "h264" => "H.264 (AVC)".into(),
        "hevc" => "H.265 (HEVC)".into(),
        "av1" => "AV1".into(),
        "vp8" | "vp9" | "aac" | "opus" | "flac" | "mp3" => codec.to_uppercase(),
        "mpeg4" => "MPEG-4".into(),
        "mjpeg" => "Motion JPEG".into(),
        "pcm_s16le" | "pcm_s24le" | "pcm_s16be" => "PCM".into(),
        other => other.to_string(),
    }
}

/// `creation_time` (UTC, "2026-01-01T12:00:00.000000Z") in local time, as the panel wants it;
/// none for the zero dates some cameras write.
fn local_time(t: &str) -> Option<String> {
    let ts: jiff::Timestamp = first(t).parse().ok()?;
    let local = ts.to_zoned(jiff::tz::TimeZone::system());
    (local.year() >= 1990).then(|| local.strftime("%Y-%m-%d %H:%M:%S").to_string())
}

/// A tag written twice in an MP4 (the standard atom and the `keys` one, as FFmpeg 9 writes
/// `creation_time` with `use_metadata_tags`) is read back as "a;b": the first value.
pub fn first(t: &str) -> &str {
    t.split(';').next().unwrap_or(t).trim()
}

/// ISO 6709 as phones write it: "+45.4641+009.1919/", with the altitude "+45.4641+009.1919+123.5/".
fn iso6709(s: &str) -> Option<(f64, f64, Option<f64>)> {
    let s = s.trim().trim_end_matches('/');
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices().skip(1) {
        if c == '+' || c == '-' {
            parts.push(&s[start..i]);
            start = i;
        }
    }
    parts.push(&s[start..]);
    let n: Vec<f64> = parts.iter().map(|p| p.parse()).collect::<Result<_, _>>().ok()?;
    match n[..] {
        [lat, lon] => Some((lat, lon, None)),
        [lat, lon, alt] => Some((lat, lon, Some(alt))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use exif::experimental::Writer;
    use exif::{Field, In, Rational, Tag, Value};
    use super::{from_video, iso6709, local_time, read_file, trimmed};

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

    #[test]
    fn raw_not_on_tiff() {
        // An Olympus ORF: a TIFF with "IIRO" in its header, which kamadak-exif does not read.
        let model = field(Tag::Model, Value::Ascii(vec![b"E-M10".to_vec()]));
        let mut writer = Writer::new();
        writer.push_field(&model);
        let mut tiff = std::io::Cursor::new(Vec::new());
        writer.write(&mut tiff, true).unwrap();
        let orf = [&b"IIRO"[..], &tiff.get_ref()[4..]].concat();
        let meta = super::read(&orf).unwrap();
        assert!(meta.fields.iter().any(|(_, l, v)| *l == "meta-model" && v == "E-M10"));
    }

    #[test]
    fn exif_for_a_copy() {
        let fields = [
            field(Tag::Make, Value::Ascii(vec![b"SONY".to_vec()])),
            field(Tag::Compression, Value::Short(vec![7])),
            field(Tag::DateTimeOriginal, Value::Ascii(vec![b"2018:03:13 16:38:58".to_vec()])),
            field(Tag::MakerNote, Value::Undefined(b"offsets into the original".to_vec(), 0)),
            field(Tag::GPSAltitude, Value::Rational(vec![(98, 1).into()])),
        ];
        let mut writer = Writer::new();
        for f in &fields {
            writer.push_field(f);
        }
        let mut tiff = std::io::Cursor::new(Vec::new());
        writer.write(&mut tiff, true).unwrap();

        let chunk = super::exif_chunk(tiff.get_ref()).unwrap();
        let exif = exif::Reader::new().read_raw(chunk).unwrap();
        let has = |tag| exif.get_field(tag, In::PRIMARY).is_some();
        assert!(has(Tag::Make) && has(Tag::DateTimeOriginal) && has(Tag::GPSAltitude));
        assert!(!has(Tag::Compression) && !has(Tag::MakerNote));
        assert_eq!(super::exif_chunk(b"not an image"), None);
        // A broken entry and a TIFF tag in the EXIF directory stay out; the rest is written.
        let broken = field(Tag(exif::Context::Exif, 0), Value::Unknown(0, 0, 0));
        assert!(!super::keep_field(&broken) && !super::keep_field(&field(Tag(exif::Context::Exif, 0xFE), Value::Long(vec![0]))));
        let make = field(Tag::Make, Value::Ascii(vec![b"SAMSUNG".to_vec()]));
        let written = super::write_exif(&[make, broken], true).unwrap();
        assert!(exif::Reader::new().read_raw(written).unwrap().get_field(Tag::Make, In::PRIMARY).is_some());
    }

    #[test]
    fn video_fields() {
        assert_eq!(iso6709("+45.4641+009.1919/"), Some((45.4641, 9.1919, None)));
        assert_eq!(iso6709("-33.8688+151.2093+012.500/"), Some((-33.8688, 151.2093, Some(12.5))));
        assert_eq!(iso6709("nonsense"), None);
        assert_eq!(local_time("1904-01-01T00:00:00.000000Z"), None);
        // Written twice by FFmpeg 9 in a converted MP4.
        assert!(local_time("2026-10-21T09:04:07.000000Z;2026-10-21T09:04:07Z").is_some());
        assert_eq!(trimmed(29.976), "29.98");
        assert_eq!(trimmed(48.0), "48");
        let info = crate::mpv::Info {
            tags: vec![("creation_time", "2026-01-01T12:00:00.000000Z".into()), ("com.android.version", "15".into()), ("location", "+45.4641+009.1919/".into()), ("speed_average_kmh", "32".into()), ("speed_top_kmh", "51".into())],
            duration: Some(180.0),
            video: Some(("hevc".into(), [1920, 1080], 90, Some(30.0))),
            audio: Some(("aac".into(), Some(48000), Some(2))),
        };
        let m = from_video(&info, 45_000_000);
        let get = |l: &str| m.fields.iter().find(|(_, label, _)| *label == l).map(|(_, _, v)| v.as_str());
        assert_eq!(get("meta-software"), Some("Android 15"));
        assert_eq!((get("meta-width"), get("meta-height")), (Some("1080 px"), Some("1920 px"))); // turned
        assert_eq!(get("meta-video-codec"), Some("H.265 (HEVC)"));
        assert_eq!(get("meta-audio"), Some("AAC 48 kHz stereo"));
        assert_eq!(get("meta-bitrate"), Some("2 Mbit/s"));
        assert_eq!(get("meta-duration"), Some("3:00"));
        assert!(get("meta-taken").is_some());
        assert_eq!(m.gps, Some((45.4641, 9.1919)));
        assert_eq!((get("meta-speed-avg"), get("meta-speed-max")), (Some("32 km/h"), Some("51 km/h")));
        // An MKV written by Converti e ridimensiona: the date only as `date_recorded`.
        let mkv = crate::mpv::Info { tags: vec![("date_recorded", "2026-10-19T15:11:29.000000Z".into())], duration: None, video: None, audio: None };
        let m = from_video(&mkv, 1);
        assert!(m.fields.iter().any(|(_, l, _)| *l == "meta-taken"));
        assert_eq!(super::video_start(&mkv), Some(1_792_422_689));
    }

    #[test]
    fn image_without_exif() {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(30, 20));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let m = read_file(&png, "a.png").unwrap();
        let get = |l: &str| m.fields.iter().find(|(_, label, _)| *label == l).map(|(_, _, v)| v.as_str());
        assert_eq!(get("meta-format"), Some("PNG"));
        assert_eq!((get("meta-width"), get("meta-height")), (Some("30 px"), Some("20 px")));
        assert_eq!((get("meta-color"), get("meta-depth")), (Some("meta-color-rgba"), Some("8 bit")));
        assert!(read_file(b"not an image", "a.txt").is_none());
    }

    #[test]
    fn meta_roundtrip() {
        let m = super::Meta { fields: vec![(super::Group::Shot, "meta-taken", "2026-01-01 12:00:00".into()), (super::Group::Gps, "meta-altitude", "12 m".into())], gps: Some((45.4641, 9.1919)) };
        assert_eq!(super::Meta::from_json(&m.to_json()), Some(m));
        assert_eq!(super::Meta::from_json("{}"), None);
        assert_eq!(super::Meta::from_json(r#"{"fields":[],"gps":null}"#), None); // saved before versions
    }

    /// Every way of writing a photo again (Rotate and crop, quarter turns with and without
    /// re-encoding, batch resize, other formats) leaves no thumbnail of the original, the EXIF
    /// sizes those of the pixels written, the orientation upright.
    #[test]
    fn exif_fits_written_file() {
        use crate::batch::{Job, Size};
        use crate::encode::{Format, Options, Original};
        use crate::transform::{Op, Turn};
        use image::{DynamicImage, RgbImage};
        // A thumbnail that is found again if any of it is left: a JPEG of the whole image.
        let img = |w, h| DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7) as u8, (y * 5) as u8, 90])));
        let thumb = Original::with_meta(img(40, 30), &[]).encode(&Options::default()).unwrap();
        let photo = |w, h| {
            let mut writer = Writer::new();
            let fields = [
                field(Tag::Make, Value::Ascii(vec![b"TestCam".to_vec()])),
                field(Tag::Orientation, Value::Short(vec![6])),
                field(Tag::PixelXDimension, Value::Short(vec![w as u16])),
                field(Tag::PixelYDimension, Value::Long(vec![h])),
            ];
            fields.iter().for_each(|f| writer.push_field(f));
            writer.set_jpeg(&thumb, In::THUMBNAIL);
            let mut tiff = std::io::Cursor::new(Vec::new());
            writer.write(&mut tiff, false).unwrap();
            let plain = Original::with_meta(img(w, h), &[]).encode(&Options::default()).unwrap();
            let len = (tiff.get_ref().len() + 8) as u16;
            [&[0xFF, 0xD8, 0xFF, 0xE1][..], &len.to_be_bytes(), b"Exif\0\0", tiff.get_ref(), &plain[2..]].concat()
        };
        let cw = Turn::default().then(Op::Cw);
        let resized = |format| Job { format, size: Size::Percent(50), ..Job::default() };
        for (w, h) in [(64, 32), (65, 33)] {
            let b = photo(w, h);
            let edit = crate::rotate::Edit { turn: Turn::default(), angle: 10.0, crop: crate::rotate::Crop { cx: -8.0, cy: 0.0, w: 20.0, h: 30.0 }, fill: None };
            let mut cropped = crate::decode::decode_full(&b, "x.jpg").unwrap();
            edit.apply(&mut cropped);
            let (turned, lossy) = crate::save_back::turned_file(&b, "x.jpg", cw).unwrap();
            assert_eq!(lossy, w % 16 != 0, "{w}: the size allows a turn without re-encoding");
            let mut out = vec![("cropped.jpg", crate::save_back::encoded_back(&b, "x.jpg", cropped).unwrap().0), ("turned.jpg", turned)];
            for (name, f) in [("x.jpg", None), ("x.png", Some(Format::Png)), ("x.webp", Some(Format::Webp)), ("x.avif", Some(Format::Avif))] {
                out.push((name, crate::batch::convert(&b, "x.jpg", &resized(f)).unwrap().1));
            }
            for (name, data) in out {
                let at = format!("{w}x{h} {name}");
                let exif = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&data)).expect(&at);
                let pixels = crate::decode::decode_full(&data, name).unwrap();
                let get = |tag| exif.get_field(tag, In::PRIMARY).and_then(|f| f.value.get_uint(0));
                assert_eq!((get(Tag::PixelXDimension), get(Tag::PixelYDimension)), (Some(pixels.width()), Some(pixels.height())), "{at}");
                assert_eq!(get(Tag::Orientation), Some(1), "{at}");
                assert!(exif.fields().all(|f| f.ifd_num != In::THUMBNAIL), "{at}");
                assert!(!data.windows(thumb.len()).any(|x| x == thumb), "{at}: the old thumbnail is still in the file");
                assert_eq!(exif.get_field(Tag::Make, In::PRIMARY).map(|f| f.display_value().to_string()), Some("\"TestCam\"".into()), "{at}");
            }
        }
    }
}
