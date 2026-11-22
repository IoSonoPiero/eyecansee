//! The Photo Sphere tags (`GPano`, #188) of a picture EyeCanSee writes again (`xmp::rewrite`):
//! they say which part of a sphere the picture covers, in pixels, so they are true only of a
//! picture of the same shape, the right way up. Made smaller or larger with the same proportions,
//! their sizes follow the new size, as the other sizes of the XMP do; cropped, turned on its side,
//! turned upside down or flipped top to bottom, they all go (`Fix::Drop`), the rest of the XMP as
//! it was. Upside down is not seen from the size: who saves says it (`xmp::without_pano`).

use crate::pano::GPANO;

/// The properties of `GPano` measured in pixels, in this order in `PanoFix::Scale`.
const PIXELS: [&str; 10] = [
    "FullPanoWidthPixels",
    "FullPanoHeightPixels",
    "CroppedAreaImageWidthPixels",
    "CroppedAreaImageHeightPixels",
    "CroppedAreaLeftPixels",
    "CroppedAreaTopPixels",
    "LargestValidInteriorRectLeft",
    "LargestValidInteriorRectTop",
    "LargestValidInteriorRectWidth",
    "LargestValidInteriorRectHeight",
];

/// What happens to the `GPano` properties of a packet written again.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) enum PanoFix {
    /// None there, or they fit the picture as they are.
    #[default]
    Keep,
    /// All of them go.
    Drop,
    /// The sizes in pixels made to fit the new size (`PIXELS`, those the packet has).
    Scale([Option<u32>; 10]),
}

impl PanoFix {
    /// The new value of the `GPano` property `local`: dropped, set, or as it is (`None`).
    pub(crate) fn of(self, local: &str) -> Option<crate::xmp::Fix> {
        match self {
            PanoFix::Keep => None,
            PanoFix::Drop => Some(crate::xmp::Fix::Drop),
            PanoFix::Scale(values) => PIXELS.iter().position(|p| *p == local).and_then(|i| values[i]).map(|v| crate::xmp::Fix::Set(v.to_string())),
        }
    }
}

/// What the `GPano` properties of `xmp` need for a picture of `w` × `h` pixels: the shape of the
/// cropped area (of the whole panorama, 2:1 when no size is given) within 2%, or they go; at
/// another size with that shape, their sizes multiplied by the new width over the old one.
pub(crate) fn fix_for(xmp: &[u8], w: u32, h: u32) -> PanoFix {
    let Some(props) = std::str::from_utf8(xmp).ok().and_then(crate::pano::props) else { return PanoFix::Keep };
    let number = |k: &str| props.get(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0);
    let full = number("FullPanoWidthPixels").map(|fw| (fw, number("FullPanoHeightPixels").unwrap_or(fw / 2.0)));
    let size = number("CroppedAreaImageWidthPixels").zip(number("CroppedAreaImageHeightPixels")).or(full);
    let aspect = size.map_or(2.0, |(cw, ch)| cw / ch);
    if h == 0 || (f64::from(w) / f64::from(h) / aspect - 1.0).abs() >= 0.02 {
        return PanoFix::Drop;
    }
    let Some((cw, _)) = size.filter(|(cw, _)| (cw - f64::from(w)).abs() >= 0.5) else { return PanoFix::Keep };
    let k = f64::from(w) / cw;
    let props_pixels = PIXELS.map(|p| props.get(p).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v >= 0.0).map(|v| (v * k).round() as u32));
    PanoFix::Scale(props_pixels)
}

/// Whether `xmp` has `GPano` properties, by its text: a packet without them is never touched.
pub(crate) fn has(xmp: &[u8]) -> bool {
    xmp.windows(GPANO.len()).any(|w| w == GPANO.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(props: &str) -> String {
        format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GPano="{GPANO}" {props}/></rdf:RDF></x:xmpmeta>"#)
    }

    /// The same shape at the same size: kept; smaller: the sizes follow; another shape (a crop, a
    /// quarter turn): all go; no sizes: 2:1 is the shape; no GPano: kept.
    #[test]
    fn what_the_tags_need() {
        let full = packet(r#"GPano:ProjectionType="equirectangular" GPano:FullPanoWidthPixels="12000" GPano:FullPanoHeightPixels="6000" GPano:CroppedAreaImageWidthPixels="12000" GPano:CroppedAreaImageHeightPixels="6000" GPano:CroppedAreaLeftPixels="0" GPano:CroppedAreaTopPixels="0""#);
        assert_eq!(fix_for(full.as_bytes(), 12000, 6000), PanoFix::Keep);
        let PanoFix::Scale(v) = fix_for(full.as_bytes(), 3000, 1500) else { panic!("not scaled") };
        assert_eq!(v[..6], [Some(3000), Some(1500), Some(3000), Some(1500), Some(0), Some(0)]);
        assert_eq!(v[6..], [None; 4]);
        assert_eq!(fix_for(full.as_bytes(), 6000, 12000), PanoFix::Drop, "turned on its side");
        assert_eq!(fix_for(full.as_bytes(), 9000, 6000), PanoFix::Drop, "cropped");
        let bare = packet(r#"GPano:ProjectionType="equirectangular""#);
        assert_eq!(fix_for(bare.as_bytes(), 4000, 2000), PanoFix::Keep);
        assert_eq!(fix_for(bare.as_bytes(), 4000, 3000), PanoFix::Drop);
        assert_eq!(fix_for(b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>", 5, 7), PanoFix::Keep);
        assert!(has(full.as_bytes()) && !has(b"<x:xmpmeta/>"));
        assert_eq!(PanoFix::Scale(v).of("CroppedAreaImageWidthPixels").map(|f| matches!(f, crate::xmp::Fix::Set(s) if s == "3000")), Some(true));
        assert!(PanoFix::Scale(v).of("ProjectionType").is_none() && matches!(PanoFix::Drop.of("ProjectionType"), Some(crate::xmp::Fix::Drop)));
    }

    // The ways a panorama is written again, each with its test (#188, E15): Save from the viewer
    // (turned without loss, or encoded again), Rotate in the grid and Convert (`batch`: the same
    // turn as Save for a JPEG in its own format, else `Original::turned` and `encode`), the editor
    // (`edit::save`: `Original::turned` and `encode`), and a copy made smaller or larger.

    use crate::meta_write::jpeg_xmp_whole;
    use crate::transform::{Op, Turn};

    const OTHER: &str = r#"xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:CreatorTool="Drone &amp; co""#;

    /// A JPEG `w` × `h` with an XMP packet holding `props` and another tag (`xmp:CreatorTool`).
    fn jpeg(w: u32, h: u32, props: &str) -> Vec<u8> {
        let mut jpeg = Vec::new();
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7) as u8, (y * 13) as u8, 90])));
        img.write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        // Without tags, without their namespace either: a file that never had them.
        let packet = if props.is_empty() { packet(OTHER).replace(&format!(r#"xmlns:GPano="{GPANO}" "#), "") } else { packet(&format!("{props} {OTHER}")) };
        let body = [crate::meta_write::JPEG_XMP, packet.as_bytes()].concat();
        let app1 = [&[0xFF, 0xE1][..], &((body.len() + 2) as u16).to_be_bytes(), &body].concat();
        jpeg.splice(2..2, app1);
        jpeg
    }

    fn xmp_of(file: &[u8]) -> String {
        String::from_utf8(jpeg_xmp_whole(file).expect("no XMP")).unwrap()
    }

    /// The Photo Sphere tags are gone, all of them with their namespace; the other tag stays.
    fn without_tags(file: &[u8]) {
        let x = xmp_of(file);
        assert!(!x.contains("GPano") && !x.contains(GPANO), "{x}");
        assert!(x.contains("Drone &amp; co"), "{x}");
    }

    fn turn(op: Op) -> Turn {
        Turn::default().then(op)
    }

    const WHOLE: &str = r#"GPano:ProjectionType="equirectangular" GPano:FullPanoWidthPixels="32" GPano:FullPanoHeightPixels="16" GPano:CroppedAreaImageWidthPixels="32" GPano:CroppedAreaImageHeightPixels="16" GPano:CroppedAreaLeftPixels="0" GPano:CroppedAreaTopPixels="0""#;

    /// Save from the viewer, and Rotate in the grid for a JPEG in its own format: turned without
    /// loss. Half a turn and a flip top to bottom: the tags go; a flip left to right: they stay;
    /// a quarter turn: they go (the shape changed).
    #[test]
    fn save_turned_without_loss() {
        let file = jpeg(32, 16, WHOLE);
        for op in [Op::Half, Op::FlipV, Op::Cw] {
            let (out, lossy) = crate::save_back::turned_file(&file, "pano.jpg", turn(op)).unwrap();
            assert!(!lossy, "{op:?}");
            without_tags(&out);
        }
        let (out, _) = crate::save_back::turned_file(&file, "pano.jpg", turn(Op::FlipH)).unwrap();
        let x = xmp_of(&out);
        assert!(x.contains(r#"GPano:FullPanoWidthPixels="32""#) && x.contains("Drone &amp; co"), "{x}");
    }

    /// Save from the viewer of a JPEG whose size the blocks do not divide: encoded again, and
    /// upside down the tags go (`save_back::encoded_beside`, also Rotate in the grid of a PNG).
    #[test]
    fn save_turned_encoded_again() {
        let file = jpeg(34, 17, r#"GPano:ProjectionType="equirectangular""#);
        let (out, lossy) = crate::save_back::turned_file(&file, "pano.jpg", turn(Op::Half)).unwrap();
        assert!(lossy, "encoded again");
        without_tags(&out);
        let (out, _) = crate::save_back::turned_file(&file, "pano.jpg", turn(Op::FlipH)).unwrap();
        assert!(xmp_of(&out).contains("GPano:ProjectionType"));
    }

    /// Rotate or Convert in the grid to another format, and the editor: the original's metadata
    /// turned with the pixels (`Original::turned`), or the turn of Save as (`Options::turn`).
    #[test]
    fn converted_and_edited() {
        let file = jpeg(32, 16, WHOLE);
        let img = crate::decode::decode_full(&file, "pano.jpg").unwrap();
        let mut upside = img.clone();
        turn(Op::Half).apply(&mut upside);
        let o = crate::encode::Options::default();
        let out = crate::encode::Original::with_meta(upside, &file).alone().turned(turn(Op::Half)).encode(&o).unwrap();
        without_tags(&out);
        let o = crate::encode::Options { turn: turn(Op::FlipV), ..Default::default() };
        let out = crate::encode::Original::with_meta(img.clone(), &file).encode(&o).unwrap();
        without_tags(&out);
        // A crop of the editor, or of Convert: another shape.
        let out = crate::encode::Original::with_meta(img.crop_imm(0, 0, 20, 16), &file).encode(&Default::default()).unwrap();
        without_tags(&out);
        // Edited without a change of shape: kept as they were.
        let out = crate::encode::Original::with_meta(img, &file).encode(&Default::default()).unwrap();
        assert!(xmp_of(&out).contains(r#"GPano:CroppedAreaImageWidthPixels="32""#));
    }

    /// Made smaller with the same proportions (Convert and resize, the editor's Resize): the sizes
    /// of the tags follow, the rest as it was.
    #[test]
    fn resized() {
        let file = jpeg(32, 16, WHOLE);
        let small = crate::decode::decode_full(&file, "pano.jpg").unwrap().resize_exact(8, 4, image::imageops::FilterType::Triangle);
        let x = xmp_of(&crate::encode::Original::with_meta(small, &file).encode(&Default::default()).unwrap());
        for v in [r#"GPano:FullPanoWidthPixels="8""#, r#"GPano:FullPanoHeightPixels="4""#, r#"GPano:CroppedAreaImageWidthPixels="8""#, r#"GPano:CroppedAreaImageHeightPixels="4""#, r#"GPano:ProjectionType="equirectangular""#, "Drone &amp; co"] {
            assert!(x.contains(v), "{v} not in {x}");
        }
    }

    /// Two checks: a file without the tags comes out with the XMP it had before #188 (what
    /// `xmp::fit` alone writes), and `without_pano` leaves a packet without them as it was.
    #[test]
    fn without_the_tags_nothing_changes() {
        let file = jpeg(32, 16, "");
        let (out, _) = crate::save_back::turned_file(&file, "photo.jpg", turn(Op::Half)).unwrap();
        let before = crate::xmp::fit(Some(&jpeg_xmp_whole(&file).unwrap()), 32, 16, None).unwrap();
        assert_eq!(jpeg_xmp_whole(&out).unwrap(), before);
        let sample = crate::xmp::tests::SAMPLE.as_bytes();
        assert_eq!(crate::xmp::without_pano(sample), sample);
        // As elements too, and with other tags around: only the Photo Sphere ones go.
        let elements = format!(r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GPano="{GPANO}" xmlns:dc="http://purl.org/dc/elements/1.1/"><GPano:ProjectionType>equirectangular</GPano:ProjectionType><dc:format>image/jpeg</dc:format></rdf:Description></rdf:RDF></x:xmpmeta>"#);
        let out = String::from_utf8(crate::xmp::without_pano(elements.as_bytes())).unwrap();
        assert!(!out.contains("GPano") && out.contains("<dc:format>image/jpeg</dc:format>"), "{out}");
    }
}
