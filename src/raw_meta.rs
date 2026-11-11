//! A RAW file converted to another format: its EXIF and the colour profile of the pixels
//! `decode::raw` gives. `kamadak-exif` reads only the RAW files built on TIFF (CR2, NEF, ARW, DNG);
//! rawler reads the camera and shot data of every maker (CR3, RAF, ORF, RW2 too) and fills what
//! is missing. The maker note stays out, as for every converted file (`meta::exif_fields`).

use exif::{Field, In, Rational, SRational, Tag, Value};
use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;

/// EXIF "ColorSpace" of a camera set to Adobe RGB (DCF "option file"): uncalibrated.
const UNCALIBRATED: u16 = 0xFFFF;

pub struct RawMeta {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

/// `None` if rawler does not take the file for a RAW.
pub fn read(bytes: &[u8]) -> Option<RawMeta> {
    let src = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&src).ok()?;
    let params = RawDecodeParams::default();
    let m = metadata(decoder.as_ref(), &src)?;
    // As `decode::raw`: the camera's own full-size picture when there is one, in the colour space
    // the camera was set to; otherwise developed by rawler, in sRGB. The picture is asked for only
    // when the camera says Adobe RGB (it is decoded once more just to know it is there).
    let adobe = m.exif.color_space == Some(UNCALIBRATED) && matches!(decoder.full_image(&src, &params), Ok(Some(_)));
    let profile: &[u8] = if adobe { include_bytes!("../assets/icc/AdobeCompat-v2.icc") } else { include_bytes!("../assets/icc/sRGB-v2-micro.icc") };
    let (mut fields, little) = merge(exif_fields(bytes), Some((from_rawler(&m), true))).unwrap_or_default();
    // The sizes of the image written, which JPEG requires: `meta::fit_exif` sets them.
    for tag in [Tag::PixelXDimension, Tag::PixelYDimension] {
        if !fields.iter().any(|f| f.tag == tag) {
            fields.push(field(tag, Value::Long(vec![0])));
        }
    }
    // Fields of every EXIF version are written: the version is the latest (2.32), which has them all.
    fields.retain(|f| f.tag != Tag::ExifVersion);
    fields.push(field(Tag::ExifVersion, Value::Undefined(b"0232".to_vec(), 0)));
    if !fields.iter().any(|f| f.tag == Tag::FlashpixVersion) {
        fields.push(field(Tag::FlashpixVersion, Value::Undefined(b"0100".to_vec(), 0)));
    }
    // Text the standard wants with its character code in front (a Pixel DNG has it as plain ASCII).
    for f in &mut fields {
        if [Tag::GPSProcessingMethod, Tag::GPSAreaInformation, Tag::UserComment].contains(&f.tag)
            && let Value::Ascii(text) = &f.value
        {
            f.value = Value::Undefined([&b"ASCII\0\0\0"[..], &text.concat()].concat(), 0);
        }
    }
    if fields.iter().any(|f| f.tag.context() == exif::Context::Gps) && !fields.iter().any(|f| f.tag == Tag::GPSVersionID) {
        fields.push(field(Tag::GPSVersionID, Value::Byte(vec![2, 3, 0, 0])));
    }
    // The colour space of the pixels written, whatever the file said.
    fields.retain(|f| f.tag != Tag::ColorSpace);
    fields.push(field(Tag::ColorSpace, Value::Short(vec![if adobe { UNCALIBRATED } else { 1 }])));
    Some(RawMeta { exif: crate::meta::write_exif(&fields, little), icc: Some(profile.to_vec()), xmp: xmp(bytes) })
}

/// The EXIF of a RAW file as a TIFF block, read as `read` does (the file's own, filled by rawler),
/// without the fields `read` sets for the copy: for the metadata panel, when `kamadak-exif`
/// cannot read the file (CR3, RAF, ORF, RW2, MRW).
pub fn exif(bytes: &[u8]) -> Option<Vec<u8>> {
    let src = RawSource::new_from_slice(bytes);
    let rawler = rawler::get_decoder(&src).ok().and_then(|d| metadata(d.as_ref(), &src));
    let (fields, little) = merge(exif_fields(bytes), rawler.map(|m| (from_rawler(&m), true)))?;
    crate::meta::write_exif(&fields, little)
}

/// rawler's metadata of the file, never for a Sigma X3F: rawler 0.8.0 only reads the directory of
/// an X3F, and its `raw_metadata` there is a `todo!()` that panics (the metadata panel of an X3F
/// showed the panic instead of the fields `kamadak-exif` reads).
pub(crate) fn metadata(decoder: &dyn rawler::decoders::Decoder, src: &RawSource) -> Option<rawler::decoders::RawMetadata> {
    if decoder.format_hint() == rawler::decoders::FormatHint::X3F {
        return None;
    }
    decoder.raw_metadata(src, &RawDecodeParams::default()).ok()
}

/// The XMP packet of a RAW file, wherever the maker put it (a TIFF tag, a CR3 box, the JPEG of a
/// RAF): found by its text, as the XMP specification allows ("packet scanning").
fn xmp(bytes: &[u8]) -> Option<Vec<u8>> {
    let find = |from: usize, what: &[u8]| bytes.get(from..)?.windows(what.len()).position(|w| w == what).map(|i| from + i);
    // The largest packet: a Hasselblad FFF has an empty one before the real one.
    let mut packets = Vec::new();
    let mut from = 0;
    while let Some(start) = find(from, b"<x:xmpmeta") {
        let Some(end) = find(start, b"</x:xmpmeta>").map(|e| e + b"</x:xmpmeta>".len()) else { break };
        packets.push((start, end));
        from = end;
    }
    let (start, end) = packets.into_iter().max_by_key(|(s, e)| e - s)?;
    // In the standard "<?xpacket" wrapper: the maker's own can end far away, after the padding.
    Some([&b"<?xpacket begin=\"\xEF\xBB\xBF\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>"[..], &bytes[start..end], b"<?xpacket end=\"w\"?>"].concat())
}

/// The EXIF the file itself holds, as `meta::exif_fields`. CR2, NEF, ARW and DNG are TIFF files;
/// ORF and RW2 too, with another number in the header; a RAF has it in its JPEG preview; a CR3 in
/// three TIFF blocks of its own (CMT1 the main directory, CMT2 the EXIF one, CMT4 the GPS one).
fn exif_fields(bytes: &[u8]) -> Option<(Vec<Field>, bool)> {
    let u32_be = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize);
    match bytes.get(..4)? {
        b"IIRO" => return crate::meta::exif_fields(&[b"II*\0", &bytes[4..]].concat()),
        // RW2: the whole EXIF is in the JPEG it holds (tag 0x002E); the main directory fills in.
        b"IIU\0" => {
            let tiff = [b"II*\0", &bytes[4..]].concat();
            let jpeg = exif::Reader::new().read_raw(tiff.clone()).ok().and_then(|e| match e.get_field(Tag(exif::Context::Tiff, 0x2e), In::PRIMARY).map(|f| &f.value) {
                Some(Value::Undefined(jpeg, _)) => crate::meta::exif_fields(jpeg),
                _ => None,
            });
            return merge(jpeg, crate::meta::exif_fields(&tiff));
        }
        b"MMOR" => return crate::meta::exif_fields(&[b"MM\0*", &bytes[4..]].concat()),
        _ => {}
    }
    // MRW: blocks (name, length, data) after an 8-byte header; "TTW" holds a TIFF with the EXIF.
    if bytes.starts_with(b"\0MRM") {
        let mut at = 8;
        while let (Some(name), Some(len)) = (bytes.get(at..at + 4), u32_be(at + 4)) {
            if name == b"\0TTW" {
                return crate::meta::exif_fields(bytes.get(at + 8..(at + 8).checked_add(len)?)?);
            }
            at = at.checked_add(8 + len)?;
        }
        return None;
    }
    if bytes.starts_with(b"FUJIFILMCCD-RAW") {
        let (at, len) = (u32_be(84)?, u32_be(88)?);
        return crate::meta::exif_fields(bytes.get(at..at.checked_add(len)?)?);
    }
    if bytes.get(4..12) == Some(b"ftypcrx ") {
        let mut out = (Vec::new(), true);
        for (name, context) in [(b"CMT1", exif::Context::Tiff), (b"CMT2", exif::Context::Exif), (b"CMT4", exif::Context::Gps)] {
            // A box: its size (4 bytes, big-endian, header included), its name, the TIFF data.
            let Some(at) = bytes.windows(4).take(1 << 20).position(|w| w == name) else { continue };
            let Some(tiff) = at.checked_sub(4).and_then(u32_be).and_then(|size| bytes.get(at + 4..at - 4 + size)) else { continue };
            let Ok(exif) = exif::Reader::new().read_raw(tiff.to_vec()) else { continue };
            out.1 = exif.little_endian();
            // Each block is read as a main directory: its tags belong to the directory it stands for.
            let fields = exif.fields().filter(|f| f.ifd_num == In::PRIMARY).map(|f| Field { tag: Tag(context, f.tag.1), ..f.clone() });
            out.0.extend(fields.filter(crate::meta::keep_field));
        }
        return (!out.0.is_empty()).then_some(out);
    }
    crate::meta::exif_fields(bytes)
}

/// Format, size and colour of a RAW file's picture as `decode::raw` develops it (the camera's
/// crop applied, before the EXIF orientation turns it): from the file's structure only, the
/// sensor data is not read. `None` if rawler cannot read it.
pub fn header(bytes: &[u8]) -> Option<crate::decode::Header> {
    use rawler::decoders::FormatHint;
    let src = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&src).ok()?;
    let (format, (size, rgb)) = match decoder.format_hint() {
        // rawler 0.8.0 decompresses a DNG's sensor data even when asked for the "dummy" image
        // (0.4 s for a Leica M10): its tags are read here instead.
        FormatHint::DNG => {
            let ifd = decoder.ifd(rawler::decoders::WellKnownIFD::Raw).ok()??;
            ("DNG".to_string(), dng_shape(&ifd)?)
        }
        hint => {
            let raw = decoder.raw_image(&src, &RawDecodeParams::default(), true).ok()?;
            let name = if hint == FormatHint::Unknown { "RAW".to_string() } else { format!("{hint:?}") };
            (name, developed_shape(&raw))
        }
    };
    // Always at 16 bits per channel (`Intermediate::to_dynamic_image`), also from 12-bit sensors.
    let color = crate::decode::Color { rgb, alpha: false, bits: 16 };
    Some(crate::decode::Header { format, size: size.map(|s| s as u32), color: Some(color) })
}

/// Size and colour (`true`) of the picture `RawDevelop::develop_intermediate` makes with its
/// default steps: the default crop, else the active area, else the whole sensor, which old
/// Fujifilm sensors turn by 45°. A colour filter or several components per pixel give colour.
fn developed_shape(raw: &rawler::RawImage) -> ([usize; 2], bool) {
    let size = match (raw.crop_area.or(raw.active_area), raw.fuji_rotation_width) {
        (Some(crop), _) => [crop.d.w, crop.d.h],
        (None, Some(t)) => {
            let s = if raw.camera.find_hint("fuji_rotate_90cw") { raw.height } else { raw.width } as f64;
            let (a, b) = ((t as f64 * std::f64::consts::SQRT_2) as usize, ((s - t as f64) * std::f64::consts::SQRT_2) as usize);
            [a.max(b), a.min(b)]
        }
        (None, None) => [raw.width, raw.height],
    };
    (size, raw.cpp > 1 || matches!(raw.photometric, rawler::rawimage::RawPhotometricInterpretation::Cfa(_)))
}

/// `developed_shape` from the tags of a DNG's raw image, as rawler's DNG decoder reads them:
/// DefaultCropSize, else ActiveArea (top, left, bottom, right), else the whole image.
fn dng_shape(ifd: &rawler::formats::tiff::IFD) -> Option<([usize; 2], bool)> {
    use rawler::tags::{DngTag, TiffCommonTag};
    let get = |e: Option<&rawler::formats::tiff::Entry>, i| e.map(|e| e.force_usize(i));
    let crop = ifd.get_entry(DngTag::DefaultCropOrigin).and(ifd.get_entry(DngTag::DefaultCropSize));
    let area = ifd.get_entry(DngTag::ActiveArea);
    let size = match (crop, area) {
        (Some(c), _) => [get(Some(c), 0)?, get(Some(c), 1)?],
        (None, Some(a)) => [a.force_usize(3).checked_sub(a.force_usize(1))?, a.force_usize(2).checked_sub(a.force_usize(0))?],
        (None, None) => [get(ifd.get_entry(TiffCommonTag::ImageWidth), 0)?, get(ifd.get_entry(TiffCommonTag::ImageLength), 0)?],
    };
    let cpp = get(ifd.get_entry(TiffCommonTag::SamplesPerPixel), 0).unwrap_or(1);
    let cfa = get(ifd.get_entry(TiffCommonTag::PhotometricInt), 0) == Some(32803);
    Some((size, cpp > 1 || cfa))
}

/// The EXIF orientation of a RAW file as the file holds it, for `decode::raw` when rawler gives
/// none.
pub fn orientation(bytes: &[u8]) -> Option<u16> {
    let exif = exif_fields(bytes).and_then(|(fields, _)| fields.iter().find(|f| f.tag == Tag::Orientation)?.value.get_uint(0).and_then(|o| u16::try_from(o).ok()));
    exif.or_else(|| ciff_orientation(bytes))
}

/// Records read at most in a CIFF file: its sub-heaps could point at each other.
const CIFF_RECORDS: usize = 4096;

/// The rotation of a Canon CRW (CIFF) file as an EXIF orientation (#8): old Canon cameras (EOS
/// 300D, D30, D60, 10D…) write it only in their own records, which rawler does not read.
/// ImageInfo, record 0x1810 (in the ImageProps sub-heap 0x300a), is 32-bit: width, height,
/// pixel aspect ratio, then Rotation as int32s (ExifTool, https://exiftool.org/TagNames/CanonRaw.html
/// and https://exiftool.org/canon_raw.html for the heaps), in degrees: dcraw 1.478 (`parse_ciff`
/// and the `(flip+3600) % 360` switch) makes 90 its flip 6, 180 flip 3, 270 flip 5, which its
/// Orientation table ("50132467") gives for EXIF 6, 3 and 8. A file not trusted: every offset
/// checked, sub-heaps 8 deep and `CIFF_RECORDS` records at most.
pub fn ciff_orientation(b: &[u8]) -> Option<u16> {
    let little = match b.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if b.get(6..14)? != b"HEAPCCDR" {
        return None;
    }
    let root = ciff_u32(b, 2, little)? as usize;
    let mut budget = CIFF_RECORDS;
    let rotation = ciff_rotation(b.get(root..)?, little, 0, &mut budget)?;
    match rotation.rem_euclid(360) {
        90 => Some(6),
        180 => Some(3),
        270 => Some(8),
        _ => None,
    }
}

/// Rotation in the heap `heap` or in one of its sub-heaps: the last 4 bytes give where its table
/// starts, a count of records of 10 bytes (type, size, offset in the heap). Bits 14-15 of the
/// type say where the data is (0: in the heap), bits 11-13 its format (0x2800 and 0x3000: a
/// sub-heap), the rest which record it is.
fn ciff_rotation(heap: &[u8], little: bool, depth: u32, budget: &mut usize) -> Option<i32> {
    let table = ciff_u32(heap, heap.len().checked_sub(4)?, little)? as usize;
    let count = ciff_u16(heap, table, little)? as usize;
    for i in 0..count {
        *budget = budget.checked_sub(1)?;
        let at = table.checked_add(2 + 10 * i)?;
        let (kind, size, offset) = (ciff_u16(heap, at, little)?, ciff_u32(heap, at + 2, little)? as usize, ciff_u32(heap, at + 6, little)? as usize);
        if kind & 0xC000 != 0 {
            continue;
        }
        let Some(data) = offset.checked_add(size).and_then(|end| heap.get(offset..end)) else { continue };
        let found = match kind & 0x3800 {
            0x2800 | 0x3000 if depth < 8 => ciff_rotation(data, little, depth + 1, budget),
            _ if kind == 0x1810 && data.len() >= 16 => ciff_u32(data, 12, little).map(|r| r as i32),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn ciff_u16(b: &[u8], at: usize, little: bool) -> Option<u16> {
    let v: [u8; 2] = b.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if little { u16::from_le_bytes(v) } else { u16::from_be_bytes(v) })
}

fn ciff_u32(b: &[u8], at: usize, little: bool) -> Option<u32> {
    let v: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(if little { u32::from_le_bytes(v) } else { u32::from_be_bytes(v) })
}

/// The fields of `a`, then those of `b` that `a` does not have.
fn merge(a: Option<(Vec<Field>, bool)>, b: Option<(Vec<Field>, bool)>) -> Option<(Vec<Field>, bool)> {
    let Some((mut fields, little)) = a else { return b };
    for f in b.map(|b| b.0).unwrap_or_default() {
        if !fields.iter().any(|g| g.tag == f.tag && g.ifd_num == f.ifd_num) {
            fields.push(f);
        }
    }
    Some((fields, little))
}

fn field(tag: Tag, value: Value) -> Field {
    Field { tag, ifd_num: In::PRIMARY, value }
}

/// The EXIF fields rawler read, as `kamadak-exif` fields. The orientation stays out: the pixels
/// are turned upright.
fn from_rawler(m: &rawler::decoders::RawMetadata) -> Vec<Field> {
    let e = &m.exif;
    let ascii = |s: &str| Value::Ascii(vec![s.as_bytes().to_vec()]);
    let r = |v: &rawler::formats::tiff::Rational| Rational { num: v.n, denom: v.d };
    let sr = |v: &rawler::formats::tiff::SRational| SRational { num: v.n, denom: v.d };
    let mut out = Vec::new();
    let mut put = |tag, value: Option<Value>| {
        if let Some(value) = value {
            out.push(field(tag, value));
        }
    };
    let text = |s: &Option<String>| s.as_deref().filter(|s| !s.is_empty()).map(ascii);
    let short = |v: Option<u16>| v.map(|v| Value::Short(vec![v]));
    let long = |v: Option<u32>| v.map(|v| Value::Long(vec![v]));
    let rat = |v: &Option<rawler::formats::tiff::Rational>| v.as_ref().map(|v| Value::Rational(vec![r(v)]));
    let srat = |v: &Option<rawler::formats::tiff::SRational>| v.as_ref().map(|v| Value::SRational(vec![sr(v)]));
    put(Tag::Make, (!m.make.is_empty()).then(|| ascii(&m.make)));
    put(Tag::Model, (!m.model.is_empty()).then(|| ascii(&m.model)));
    put(Tag::Artist, text(&e.artist));
    put(Tag::Copyright, text(&e.copyright));
    put(Tag::DateTime, text(&e.modify_date));
    put(Tag::ExposureTime, rat(&e.exposure_time));
    put(Tag::FNumber, rat(&e.fnumber));
    put(Tag::ApertureValue, rat(&e.aperture_value));
    put(Tag::BrightnessValue, srat(&e.brightness_value));
    put(Tag::PhotographicSensitivity, short(e.iso_speed_ratings));
    put(Tag::ISOSpeed, long(e.iso_speed));
    put(Tag::RecommendedExposureIndex, long(e.recommended_exposure_index));
    put(Tag::SensitivityType, short(e.sensitivity_type));
    put(Tag::ExposureBiasValue, srat(&e.exposure_bias));
    put(Tag::DateTimeOriginal, text(&e.date_time_original));
    put(Tag::DateTimeDigitized, text(&e.create_date));
    put(Tag::ExposureProgram, short(e.exposure_program));
    put(Tag::OffsetTime, text(&e.offset_time));
    put(Tag::OffsetTimeOriginal, text(&e.offset_time_original));
    put(Tag::OffsetTimeDigitized, text(&e.offset_time_digitized));
    put(Tag::SubSecTime, text(&e.sub_sec_time));
    put(Tag::SubSecTimeOriginal, text(&e.sub_sec_time_original));
    put(Tag::SubSecTimeDigitized, text(&e.sub_sec_time_digitized));
    put(Tag::ShutterSpeedValue, srat(&e.shutter_speed_value));
    put(Tag::MaxApertureValue, rat(&e.max_aperture_value));
    put(Tag::SubjectDistance, rat(&e.subject_distance));
    put(Tag::MeteringMode, short(e.metering_mode));
    put(Tag::LightSource, short(e.light_source));
    put(Tag::Flash, short(e.flash));
    put(Tag::FocalLength, rat(&e.focal_length));
    put(Tag::FlashEnergy, rat(&e.flash_energy));
    put(Tag::ExposureMode, short(e.exposure_mode));
    put(Tag::WhiteBalance, short(e.white_balance));
    put(Tag::SceneCaptureType, short(e.scene_capture_type));
    put(Tag::SubjectDistanceRange, short(e.subject_distance_range));
    put(Tag::CameraOwnerName, text(&e.owner_name));
    put(Tag::BodySerialNumber, text(&e.serial_number));
    put(Tag::LensSerialNumber, text(&e.lens_serial_number));
    put(Tag::LensMake, text(&e.lens_make));
    put(Tag::LensModel, text(&e.lens_model));
    put(Tag::LensSpecification, e.lens_spec.as_ref().map(|s| Value::Rational(s.iter().map(r).collect())));
    put(Tag::UserComment, e.user_comment.as_deref().filter(|s| !s.trim().is_empty()).map(|s| Value::Undefined([b"ASCII\0\0\0", s.as_bytes()].concat(), 0)));
    if let Some(g) = &e.gps {
        out.extend(gps(g));
    }
    out
}

/// The GPS fields rawler read.
fn gps(g: &rawler::exif::ExifGPS) -> Vec<Field> {
    let r = |v: &rawler::formats::tiff::Rational| Rational { num: v.n, denom: v.d };
    let mut out = Vec::new();
    let mut put = |tag, value: Option<Value>| {
        if let Some(value) = value {
            out.push(field(tag, value));
        }
    };
    let text = |s: &Option<String>| s.as_deref().filter(|s| !s.is_empty()).map(|s| Value::Ascii(vec![s.as_bytes().to_vec()]));
    let rat = |v: &Option<rawler::formats::tiff::Rational>| v.as_ref().map(|v| Value::Rational(vec![r(v)]));
    let three = |v: &Option<[rawler::formats::tiff::Rational; 3]>| v.as_ref().map(|v| Value::Rational(v.iter().map(r).collect()));
    put(Tag::GPSVersionID, g.gps_version_id.map(|v| Value::Byte(v.to_vec())));
    put(Tag::GPSLatitudeRef, text(&g.gps_latitude_ref));
    put(Tag::GPSLatitude, three(&g.gps_latitude));
    put(Tag::GPSLongitudeRef, text(&g.gps_longitude_ref));
    put(Tag::GPSLongitude, three(&g.gps_longitude));
    put(Tag::GPSAltitudeRef, g.gps_altitude_ref.map(|v| Value::Byte(vec![v])));
    put(Tag::GPSAltitude, rat(&g.gps_altitude));
    put(Tag::GPSTimeStamp, three(&g.gps_timestamp));
    put(Tag::GPSSatellites, text(&g.gps_satellites));
    put(Tag::GPSStatus, text(&g.gps_status));
    put(Tag::GPSMeasureMode, text(&g.gps_measure_mode));
    put(Tag::GPSDOP, rat(&g.gps_dop));
    put(Tag::GPSSpeedRef, text(&g.gps_speed_ref));
    put(Tag::GPSSpeed, rat(&g.gps_speed));
    put(Tag::GPSTrackRef, text(&g.gps_track_ref));
    put(Tag::GPSTrack, rat(&g.gps_track));
    put(Tag::GPSImgDirectionRef, text(&g.gps_img_direction_ref));
    put(Tag::GPSImgDirection, rat(&g.gps_img_direction));
    put(Tag::GPSMapDatum, text(&g.gps_map_datum));
    put(Tag::GPSDateStamp, text(&g.gps_date_stamp));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TIFF block with `fields`, little-endian.
    fn tiff(fields: &[Field]) -> Vec<u8> {
        crate::meta::write_exif(fields, true).unwrap()
    }

    fn ascii(s: &str) -> Value {
        Value::Ascii(vec![s.as_bytes().to_vec()])
    }

    #[test]
    fn exif_of_orf_and_cr3() {
        let make = field(Tag::Make, ascii("OM"));
        let shot = field(Tag::DateTimeOriginal, ascii("2026:11:04 10:00:00"));
        // ORF: a TIFF with "IIRO" in place of "II*\0".
        let mut orf = tiff(&[make.clone(), shot.clone()]);
        orf[..4].copy_from_slice(b"IIRO");
        let (fields, _) = exif_fields(&orf).unwrap();
        assert!(fields.iter().any(|f| f.tag == Tag::Make) && fields.iter().any(|f| f.tag == Tag::DateTimeOriginal));
        // CR3: the EXIF directory as the main one of the CMT2 box, the GPS one in CMT4.
        let boxed = |name: &[u8; 4], data: Vec<u8>| [&(data.len() as u32 + 8).to_be_bytes()[..], name, &data].concat();
        let as_main = |tag: Tag, value| tiff(&[field(Tag(exif::Context::Tiff, tag.1), value)]);
        let cr3 = [
            &[0, 0, 0, 24][..],
            b"ftypcrx \0\0\0\x01crx isom",
            &boxed(b"CMT1", tiff(&[make])),
            &boxed(b"CMT2", as_main(Tag::DateTimeOriginal, ascii("2026:11:04 10:00:00"))),
            &boxed(b"CMT4", as_main(Tag::GPSLatitudeRef, ascii("N"))),
        ]
        .concat();
        let (fields, _) = exif_fields(&cr3).unwrap();
        for tag in [Tag::Make, Tag::DateTimeOriginal, Tag::GPSLatitudeRef] {
            assert!(fields.iter().any(|f| f.tag == tag), "{tag}");
        }
    }

    #[test]
    fn exif_of_mrw() {
        let ttw = tiff(&[field(Tag::Make, ascii("Minolta")), field(Tag::Orientation, Value::Short(vec![6]))]);
        let block = |name: &[u8; 4], data: &[u8]| [&name[..], &(data.len() as u32).to_be_bytes(), data].concat();
        let blocks = [block(b"\0PRD", &[0; 24]), block(b"\0TTW", &ttw)].concat();
        let mrw = [&b"\0MRM"[..], &(blocks.len() as u32).to_be_bytes(), &blocks].concat();
        let (fields, _) = exif_fields(&mrw).unwrap();
        assert!(fields.iter().any(|f| f.tag == Tag::Make));
        assert_eq!(orientation(&mrw), Some(6));
    }

    /// A CIFF heap: `data` then the table of `records` (type, size, offset), its offset last.
    fn heap(data: &[u8], records: &[(u16, u32, u32)]) -> Vec<u8> {
        let mut h = data.to_vec();
        h.extend_from_slice(&(records.len() as u16).to_le_bytes());
        for &(kind, size, offset) in records {
            h.extend_from_slice(&[kind.to_le_bytes().as_slice(), &size.to_le_bytes(), &offset.to_le_bytes()].concat());
        }
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h
    }

    /// A CRW as the EOS 300D writes it: ImageInfo (0x1810) with `rotation` in the ImageProps
    /// sub-heap (0x300a), after a record whose data is in the record itself.
    fn crw(rotation: i32) -> Vec<u8> {
        let info: Vec<u8> = [3072u32, 2048, 1.0f32.to_bits(), rotation as u32, 8, 24, 257].iter().flat_map(|v| v.to_le_bytes()).collect();
        let props = heap(&info, &[(0x5803, 0x20001, 0x4120_0000), (0x1810, 28, 0)]);
        ciff(&heap(&props, &[(0x300a, props.len() as u32, 0)]))
    }

    /// A CIFF file of 26 bytes of header (order, its length, signature, version, 8 reserved)
    /// and the root heap.
    fn ciff(root: &[u8]) -> Vec<u8> {
        [&b"II"[..], &26u32.to_le_bytes(), b"HEAPCCDR", &[2, 0, 1, 0], &[0; 8], root].concat()
    }

    /// The rotation of a CRW (#8) in degrees, as dcraw reads it: 90 → EXIF 6, 180 → 3, 270 (or
    /// -90) → 8, none for 0.
    #[test]
    fn crw_rotation() {
        for (degrees, exif) in [(0, None), (90, Some(6)), (180, Some(3)), (270, Some(8)), (-90, Some(8))] {
            assert_eq!(orientation(&crw(degrees)), exif, "{degrees}");
        }
    }

    /// A CRW not to be trusted: cut anywhere, no panic and no other rotation (cut right after
    /// ImageProps, that is a whole heap of its own); with a heap holding itself, it stops at once.
    #[test]
    fn crw_damaged() {
        let file = crw(90);
        for end in 0..file.len() {
            assert!(matches!(ciff_orientation(&file[..end]), None | Some(6)), "cut at {end}");
        }
        // 50 records, each a sub-heap that is the heap itself: 50^8 without a limit.
        let looped = heap(&[], &[(0x300a, 2 + 10 * 50 + 4, 0); 50]);
        let started = std::time::Instant::now();
        assert_eq!(ciff_orientation(&ciff(&looped)), None);
        assert!(started.elapsed().as_secs() < 1);
        // Offsets past the end, a table past the end.
        let mut far = crw(90);
        let n = far.len();
        far[n - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(ciff_orientation(&far), None);
    }

    /// The real EOS 300D CRW of the test files (`notes/test-files/online/raw`, Rotation 270 in
    /// ImageInfo, no EXIF) comes out upright, in portrait; the same file with the rotation set to
    /// 90 (a copy, here only) is turned the other way, at 0 it stays in landscape (#8). Written
    /// to `EYECANSEE_SAMPLES_OUT`, if set, to look at.
    #[test]
    #[ignore]
    fn crw_portrait_live() {
        use image::GenericImageView;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("notes/test-files/online/raw/Canon-EOS300D-rot-adobergb.crw");
        let bytes = std::fs::read(&path).expect("test-files/scarica.sh online-raw");
        assert_eq!(orientation(&bytes), Some(8));
        // ImageInfo (3072, 2048, 1.0, 270), its rotation replaced in a copy.
        let info = [0, 12, 0, 0, 0, 8, 0, 0, 0, 0, 0x80, 0x3F, 14, 1, 0, 0];
        let at = bytes.windows(16).position(|w| w == info).expect("ImageInfo") + 12;
        let with = |r: i32| {
            let mut b = bytes.clone();
            b[at..at + 4].copy_from_slice(&r.to_le_bytes());
            b
        };
        let out = std::env::var_os("EYECANSEE_SAMPLES_OUT");
        let mut seen = Vec::new();
        for (r, portrait) in [(270, true), (90, true), (0, false)] {
            let b = with(r);
            let img = crate::decode::decode_full(&b, "a.crw").unwrap();
            assert_eq!(img.width() < img.height(), portrait, "rotation {r}: {:?}", img.dimensions());
            let thumb = crate::decode::thumbnail(&b, "a.crw", 256).unwrap();
            assert_eq!(thumb.width() < thumb.height(), portrait, "thumbnail, rotation {r}");
            if let Some(dir) = &out {
                thumb.save(std::path::Path::new(dir).join(format!("crw-{r}.png"))).unwrap();
            }
            seen.push(img);
        }
        // 270 and 90: the same picture turned by half a turn.
        let mut half = seen[1].clone();
        crate::transform::Turn::default().then(crate::transform::Op::Cw).then(crate::transform::Op::Cw).apply(&mut half);
        assert_eq!(half.to_rgb8().get_pixel(100, 100), seen[0].to_rgb8().get_pixel(100, 100));
    }

    /// The smallest file rawler takes for a Sigma X3F: the "FOVb" mark and an empty directory.
    /// Its metadata was a `todo!()` in rawler 0.8.0 that panicked (#7).
    #[test]
    fn x3f_metadata() {
        let x3f = [&b"FOVb\0\0\0\0SECd"[..], &0x0002_0000u32.to_le_bytes(), &0u32.to_le_bytes(), &8u32.to_le_bytes()].concat();
        assert!(rawler::get_decoder(&RawSource::new_from_slice(&x3f)).is_ok());
        assert!(exif(&x3f).is_none());
        assert!(read(&x3f).is_none());
        assert!(crate::meta::read_file(&x3f, "a.x3f").is_none());
    }

    #[test]
    fn merge_keeps_the_first() {
        let a = Some((vec![field(Tag::Model, ascii("A"))], true));
        let b = Some((vec![field(Tag::Model, ascii("B")), field(Tag::Make, ascii("M"))], false));
        let (fields, little) = merge(a, b).unwrap();
        assert!(little);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].display_value().to_string(), "\"A\"");
        assert!(merge(None, None).is_none());
    }

    #[test]
    fn xmp_found_and_wrapped() {
        let file = b"junk<?xpacket begin=''?><x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf/></x:xmpmeta>   pad <?xpacket end='w'?>junk";
        let xmp = xmp(file).unwrap();
        assert!(xmp.starts_with(b"<?xpacket begin=") && xmp.ends_with(b"<?xpacket end=\"w\"?>"));
        assert!(xmp.windows(10).any(|w| w == b"<x:xmpmeta") && !xmp.windows(4).any(|w| w == b"junk"));
        assert_eq!(super::xmp(b"no packet here"), None);
        // The largest of two packets.
        let two = b"<x:xmpmeta/></x:xmpmeta> <x:xmpmeta><rdf>real</rdf></x:xmpmeta>";
        assert!(super::xmp(two).unwrap().windows(4).any(|w| w == b"real"));
    }
}
