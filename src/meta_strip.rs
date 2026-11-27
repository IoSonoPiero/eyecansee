//! What a saved file keeps of the original's metadata, when the user chooses it while saving
//! (#55): everything (the rule of every save), everything but the place, or nothing but what the
//! picture needs to look right (its colour profile; the pixels are already upright). Without the
//! place: the GPS directory of the EXIF, the GPS and place properties of the XMP and the place
//! fields of the IPTC, as Lightroom's "Remove Location Info" with its saved locations.


/// What a new file keeps of the original's metadata.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Keep {
    #[default]
    All,
    /// Everything but where the photo was taken.
    NoPlace,
    /// Only the colour profile.
    Nothing,
}

impl Keep {
    pub const ALL: [Keep; 3] = [Keep::All, Keep::NoPlace, Keep::Nothing];

    /// The Fluent id of its name in the menus.
    pub fn label(self) -> &'static str {
        match self {
            Keep::All => "meta-keep-all",
            Keep::NoPlace => "meta-keep-no-place",
            Keep::Nothing => "meta-keep-nothing",
        }
    }
}

/// The yellow line when saving an image with a cover box (#359): the box hides pixels, and with
/// everything kept the file still records where the photo was taken. `place` (whether the photo
/// records it) is asked last: it reads the metadata again.
pub fn warn_place(cover: bool, keep: Keep, place: impl FnOnce() -> bool) -> bool {
    cover && keep == Keep::All && place()
}

/// The EXIF (a TIFF structure) holds a position: a latitude in its GPS directory. Some cameras
/// write the directory with its version only, which says nowhere.
pub fn exif_has_position(t: &[u8]) -> bool {
    exif::Reader::new().read_raw(t.to_vec()).is_ok_and(|e| e.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY).is_some())
}

/// IPTC-IIM datasets of the place: content location code and name (2:26, 2:27), city (2:90),
/// sublocation (2:92), province or state (2:95), country code and name (2:100, 2:101).
const IIM_PLACE: [u8; 7] = [26, 27, 90, 92, 95, 100, 101];

/// The EXIF (a TIFF structure) without its GPS directory: the pointer to it taken out of the first
/// directory, the directory and the values it points to zeroed. Nothing moves, so every other
/// offset stays right. Unchanged when it is not a TIFF structure or has no GPS.
pub fn exif_without_gps(t: &mut [u8]) {
    let little = match t.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        _ => return,
    };
    let u16_at = |t: &[u8], i: usize| t.get(i..i + 2).map(|b| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
    let u32_at = |t: &[u8], i: usize| t.get(i..i + 4).map(|b| if little { u32::from_le_bytes(b.try_into().unwrap()) } else { u32::from_be_bytes(b.try_into().unwrap()) } as usize);
    let Some(ifd0) = u32_at(t, 4) else { return };
    let Some(n) = u16_at(t, ifd0).map(usize::from) else { return };
    let entry = |k: usize| ifd0 + 2 + 12 * k;
    if t.len() < entry(n) + 4 {
        return;
    }
    let Some(k) = (0..n).find(|&k| u16_at(t, entry(k)) == Some(0x8825)) else { return };
    // The GPS directory: each value longer than 4 bytes, then the directory itself.
    if let Some(gps) = u32_at(t, entry(k) + 8)
        && let Some(m) = u16_at(t, gps).map(usize::from)
        && t.len() >= gps + 2 + 12 * m + 4
    {
        for j in 0..m {
            let e = gps + 2 + 12 * j;
            let size = match u16_at(t, e + 2) {
                Some(1 | 2 | 6 | 7) => 1,
                Some(3 | 8) => 2,
                Some(4 | 9 | 11 | 13) => 4,
                Some(5 | 10 | 12) => 8,
                _ => 0,
            };
            let len = size * u32_at(t, e + 4).unwrap_or(0);
            if len > 4
                && let Some(at) = u32_at(t, e + 8)
                && let Some(d) = t.get_mut(at..at.saturating_add(len))
            {
                d.fill(0);
            }
        }
        t[gps..gps + 2 + 12 * m + 4].fill(0);
    }
    // The pointer out: the entries after it move up by one, the count and the "next" offset follow.
    let end = entry(n) + 4;
    t.copy_within(entry(k + 1)..end, entry(k));
    t[end - 12..end].fill(0);
    let count = if little { ((n - 1) as u16).to_le_bytes() } else { ((n - 1) as u16).to_be_bytes() };
    t[ifd0..ifd0 + 2].copy_from_slice(&count);
}

/// IPTC-IIM without the datasets of the place; the others as they were. What cannot be read
/// (a dataset cut short) is left out.
pub fn iim_without_place(iim: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(iim.len());
    let mut i = 0;
    while i + 5 <= iim.len() && iim[i] == 0x1C {
        let (record, dataset) = (iim[i + 1], iim[i + 2]);
        let len = u16::from_be_bytes([iim[i + 3], iim[i + 4]]);
        // Extended lengths (top bit set) are for very large data, never a place: kept whole.
        let (head, len) = if len & 0x8000 == 0 {
            (5, len as usize)
        } else {
            let k = usize::from(len & 0x7FFF);
            let Some(b) = iim.get(i + 5..i + 5 + k) else { break };
            (5 + k, b.iter().fold(0usize, |v, &x| v << 8 | usize::from(x)))
        };
        // An extended length is up to eight bytes, up to the largest number.
        let Some(next) = (i + head).checked_add(len) else { break };
        let Some(whole) = iim.get(i..next) else { break };
        if !(record == 2 && IIM_PLACE.contains(&dataset)) {
            out.extend_from_slice(whole);
        }
        i = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use exif::experimental::Writer;
    use exif::{Field, In, Rational, Tag, Value};

    /// #359: the line only with a cover box and everything kept, and only if the photo records
    /// its place; the metadata are not read again when the rest says no.
    #[test]
    fn place_warned_only_with_a_cover_and_all() {
        assert!(warn_place(true, Keep::All, || true));
        assert!(!warn_place(true, Keep::All, || false), "no place in the photo");
        for (cover, keep) in [(false, Keep::All), (true, Keep::NoPlace), (true, Keep::Nothing)] {
            assert!(!warn_place(cover, keep, || panic!("{cover} {keep:?}: the metadata read for nothing")), "{cover} {keep:?}");
        }
    }

    /// #359: an EXIF records the place with a position in its GPS directory (a phone's photo,
    /// without XMP); a GPS directory with only its version, or no GPS, does not.
        /// #359: the position of a photo whose EXIF has a part kamadak-exif rejects as a whole (a
    /// "next" link where none is expected, issue #68, as `Quebec City.jpg` of the test files): the
    /// yellow line of the editor must still know the photo records its place.
    #[test]
    fn place_in_an_exif_with_an_invalid_part() {
        let mut tiff = b"MM\0\x2a\0\0\0\x08".to_vec();
        // IFD0: GPSInfo at 26, no next directory.
        tiff.extend(b"\0\x01\x88\x25\0\x04\0\0\0\x01\0\0\0\x1a\0\0\0\0");
        // GPS directory: GPSLatitude, three rationals at 44, then a next directory where none is expected.
        tiff.extend(b"\0\x01\0\x02\0\x05\0\0\0\x03\0\0\0\x2c\0\0\0\x01");
        for (n, d) in [(46u32, 1u32), (48, 1), (4311, 100)] {
            tiff.extend(n.to_be_bytes());
            tiff.extend(d.to_be_bytes());
        }
        assert!(exif::Reader::new().read_raw(tiff.clone()).is_err(), "rejected as a whole without continue_on_error");
        assert!(exif_has_position(&tiff));
    }

#[test]
    fn place_in_the_exif() {
        let img = image::DynamicImage::new_rgb8(8, 8);
        let place = |exif: Vec<u8>| crate::encode::Original::with_exif(img.clone(), Some(exif)).has_place(crate::encode::Format::Jpeg);
        for big in [false, true] {
            assert!(exif_has_position(&exif_with_gps(big)) && place(exif_with_gps(big)), "big endian {big}");
        }
        let f = |tag, value| Field { tag, ifd_num: In::PRIMARY, value };
        for fields in [vec![f(Tag::Make, Value::Ascii(vec![b"Xiaomi".to_vec()])), f(Tag::GPSVersionID, Value::Byte(vec![2, 2, 0, 0]))], vec![f(Tag::Make, Value::Ascii(vec![b"Xiaomi".to_vec()]))]] {
            let mut w = Writer::new();
            fields.iter().for_each(|x| w.push_field(x));
            let mut out = std::io::Cursor::new(Vec::new());
            w.write(&mut out, true).unwrap();
            let exif = out.into_inner();
            assert!(!exif_has_position(&exif) && !place(exif), "{:?}", fields.iter().map(|f| f.tag).collect::<Vec<_>>());
        }
    }

    fn exif_with_gps(big_endian: bool) -> Vec<u8> {
        let f = |tag, value| Field { tag, ifd_num: In::PRIMARY, value };
        let fields = [
            f(Tag::Make, Value::Ascii(vec![b"Xiaomi".to_vec()])),
            f(Tag::Orientation, Value::Short(vec![1])),
            f(Tag::DateTimeOriginal, Value::Ascii(vec![b"2026:10:09 17:48:35".to_vec()])),
            f(Tag::GPSLatitudeRef, Value::Ascii(vec![b"N".to_vec()])),
            f(Tag::GPSLatitude, Value::Rational(vec![(45, 1).into(), (30, 1).into(), Rational::from((7, 1))])),
            f(Tag::GPSLongitude, Value::Rational(vec![(9, 1).into(), (15, 1).into(), Rational::from((36, 1))])),
        ];
        let mut w = Writer::new();
        fields.iter().for_each(|x| w.push_field(x));
        let mut out = std::io::Cursor::new(Vec::new());
        w.write(&mut out, !big_endian).unwrap();
        out.into_inner()
    }

    /// The GPS gone, its values zeroed in the bytes too; the rest read as before.
    #[test]
    fn gps_out_of_the_exif() {
        for big in [false, true] {
            let mut t = exif_with_gps(big);
            let len = t.len();
            let before = exif::Reader::new().read_raw(t.clone()).unwrap();
            assert!(before.get_field(Tag::GPSLatitude, In::PRIMARY).is_some());
            exif_without_gps(&mut t);
            assert_eq!(t.len(), len);
            let after = exif::Reader::new().read_raw(t.clone()).unwrap();
            assert!(after.fields().all(|f| f.ifd_num != In::PRIMARY || !matches!(f.tag.context(), exif::Context::Gps)), "{:?}", after.fields().map(|f| f.tag).collect::<Vec<_>>());
            assert_eq!(after.get_field(Tag::Make, In::PRIMARY).map(|f| f.display_value().to_string()), Some("\"Xiaomi\"".into()));
            assert!(after.get_field(Tag::DateTimeOriginal, In::PRIMARY).is_some());
            assert!(after.get_field(Tag::Orientation, In::PRIMARY).is_some());
            // 45/1, 30/1, 7/1 written as rationals: not in the bytes any more.
            let rational = |v: u32| if big { v.to_be_bytes() } else { v.to_le_bytes() };
            let lat: Vec<u8> = [45, 1, 30, 1, 7, 1].iter().flat_map(|&v| rational(v)).collect();
            assert!(!t.windows(lat.len()).any(|w| w == lat));
            assert!(!t.windows(2).any(|w| w == b"N\0"));
        }
        // Not a TIFF, or without GPS: as it was.
        let mut junk = b"nothing here".to_vec();
        exif_without_gps(&mut junk);
        assert_eq!(junk, b"nothing here");
    }

    #[test]
    fn place_out_of_the_iptc() {
        let ds = |n: u8, v: &str| [&[0x1C, 2, n][..], &(v.len() as u16).to_be_bytes(), v.as_bytes()].concat();
        let iim = [ds(0, "\0\x04"), ds(80, "Ann"), ds(90, "Lecce"), ds(101, "Italy"), ds(25, "sea"), ds(92, "Porto")].concat();
        assert_eq!(iim_without_place(&iim), [ds(0, "\0\x04"), ds(80, "Ann"), ds(25, "sea")].concat());
        assert_eq!(iim_without_place(&iim[..iim.len() - 2]), [ds(0, "\0\x04"), ds(80, "Ann"), ds(25, "sea")].concat(), "cut short");
    }

    /// #275: an extended length of eight bytes `FF` (the largest number) added to where the field
    /// starts went past the largest number: a panic in a debug build. Read as a field cut short.
    #[test]
    fn iptc_extended_length_too_large() {
        let ds = |n: u8, v: &str| [&[0x1C, 2, n][..], &(v.len() as u16).to_be_bytes(), v.as_bytes()].concat();
        let huge = [&[0x1C, 2, 5, 0x80, 8][..], &[0xFF; 8], b"caption"].concat();
        let iim = [ds(80, "Ann"), huge].concat();
        assert_eq!(iim_without_place(&iim), ds(80, "Ann"));
    }
}
