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
    let m = decoder.raw_metadata(&src, &params).ok()?;
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
    let rawler = rawler::get_decoder(&src).ok().and_then(|d| d.raw_metadata(&src, &RawDecodeParams::default()).ok());
    let (fields, little) = merge(exif_fields(bytes), rawler.map(|m| (from_rawler(&m), true)))?;
    crate::meta::write_exif(&fields, little)
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
    let (fields, _) = exif_fields(bytes)?;
    fields.iter().find(|f| f.tag == Tag::Orientation)?.value.get_uint(0).and_then(|o| u16::try_from(o).ok())
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
