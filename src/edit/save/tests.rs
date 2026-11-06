use std::io::Cursor;

use exif::experimental::Writer;
use exif::{Field, In, Tag, Value};
use image::{DynamicImage, GenericImageView, Rgb, RgbImage};

use super::*;
use crate::edit::doc::{Obj, Pt, Style};
use crate::edit::render::tests::options;
use crate::rotate::Crop;
use crate::transform::Op;

const BLUE: [u8; 3] = [0, 0, 255];

/// A grey gradient with a pattern no box could pass for: black and white squares of 2 pixels and
/// coloured stripes, in 16..48 × 12..36 (the middle of 64 × 48).
fn motif() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(64, 48, |x, y| {
        if (16..48).contains(&x) && (12..36).contains(&y) {
            if y < 24 { if (x / 2 + y / 2) % 2 == 0 { Rgb([0; 3]) } else { Rgb([255; 3]) } } else { Rgb([(x * 8) as u8, 255 - (y * 6) as u8, ((x * y) % 256) as u8]) }
        } else {
            Rgb([(x * 3) as u8, (y * 4) as u8, 128])
        }
    }))
}

fn obj(shape: Shape, style: Style) -> Obj {
    Obj { shape, style }
}

fn cover_at(x: f32, y: f32, w: f32, h: f32) -> Obj {
    obj(Shape::Cover(Area { centre: Pt::new(x, y), w, h, rot: 0.0 }), Style { color: BLUE, ..Style::default() })
}

fn file(img: &DynamicImage, format: Format, webp_lossless: bool) -> Vec<u8> {
    Original::with_meta(img.clone(), &[]).encode(&Options { format, webp_lossless, ..Options::default() }).unwrap()
}

fn saved(bytes: &[u8], name: &str, doc: &Doc) -> Saved {
    let original = Original::with_meta_of(crate::decode::decode_full(bytes, name).unwrap(), bytes, name);
    save(doc, &original, bytes, name, "edited", &[name.to_string()], &options()).unwrap()
}

/// Luminance of a pixel.
fn luma(p: image::Rgba<u8>) -> f32 {
    0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
}

/// Correlation between the luminance of two images in 16..48 × 12..36: 0 when one is flat.
fn correlation(a: &DynamicImage, b: &DynamicImage) -> f32 {
    let pairs: Vec<(f32, f32)> = (12..36).flat_map(|y| (16..48).map(move |x| (x, y))).map(|(x, y)| (luma(a.get_pixel(x, y)), luma(b.get_pixel(x, y)))).collect();
    let n = pairs.len() as f32;
    let (ma, mb) = (pairs.iter().map(|p| p.0).sum::<f32>() / n, pairs.iter().map(|p| p.1).sum::<f32>() / n);
    let cov: f32 = pairs.iter().map(|p| (p.0 - ma) * (p.1 - mb)).sum();
    let (va, vb): (f32, f32) = (pairs.iter().map(|p| (p.0 - ma).powi(2)).sum(), pairs.iter().map(|p| (p.1 - mb).powi(2)).sum());
    if va == 0.0 || vb == 0.0 { 0.0 } else { cov / (va * vb).sqrt() }
}

/// Under a cover box nothing of the pattern is left in the saved file, decoded again: in the
/// lossless formats every pixel is the colour of the box, in the lossy ones nearly, and its
/// luminance has nothing to do with the pattern's.
#[test]
fn nothing_left_under_a_cover() {
    let img = motif();
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(cover_at(0.0, 0.0, 32.0, 24.0));
    for (format, name, webp_lossless, exact) in [
        (Format::Jpeg, "a.jpg", false, false),
        (Format::Png, "a.png", false, true),
        (Format::Webp, "a.webp", true, true),
        (Format::Webp, "a.webp", false, false),
        (Format::Avif, "a.avif", false, false),
        (Format::Tiff, "a.tif", false, true),
    ] {
        let bytes = file(&img, format, webp_lossless);
        let out = saved(&bytes, name, &doc);
        assert_eq!(out.name, format!("a (edited).{}", name.rsplit('.').next().unwrap()));
        let back = crate::decode::decode_full(&out.data, &out.name).unwrap();
        assert!(correlation(&img, &img) > 0.99, "the test pattern");
        let r = correlation(&img, &back);
        assert!(r.abs() < 0.1, "{name} {webp_lossless}: correlation {r}");
        let margin = if exact { 0 } else { 2 };
        for (x, y) in (12 + margin..36 - margin).flat_map(|y| (16 + margin..48 - margin).map(move |x| (x, y))) {
            let p = back.get_pixel(x, y);
            let off = p.0[..3].iter().zip(BLUE).map(|(a, b)| a.abs_diff(b)).max().unwrap();
            assert!(off <= if exact { 0 } else { 24 }, "{name} {webp_lossless} at {x},{y}: {:?}", p.0);
        }
    }
}

fn field(tag: Tag, value: Value) -> Field {
    Field { tag, ifd_num: In::PRIMARY, value }
}

/// A phone photo with everything that must not survive an edit: an EXIF thumbnail, an XMP
/// thumbnail and Motion Photo description, a Photoshop thumbnail, an MPF segment and a video after
/// the end of the picture. Also an author, which stays.
fn motion_photo(img: &DynamicImage, thumb: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    let fields = [
        field(Tag::Make, Value::Ascii(vec![b"TestPhone".to_vec()])),
        field(Tag::PixelXDimension, Value::Long(vec![img.width()])),
        field(Tag::PixelYDimension, Value::Long(vec![img.height()])),
    ];
    fields.iter().for_each(|f| w.push_field(f));
    w.set_jpeg(thumb, In::THUMBNAIL);
    let mut tiff = Cursor::new(Vec::new());
    w.write(&mut tiff, false).unwrap();
    let exif = [&b"Exif\0\0"[..], tiff.get_ref()].concat();
    let xmp = concat!(
        "http://ns.adobe.com/xap/1.0/\0<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">",
        "<rdf:Description xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:xmpGImg=\"http://ns.adobe.com/xap/1.0/g/img/\" GCamera:MotionPhoto=\"1\" GCamera:MotionPhotoVersion=\"1\" GCamera:MicroVideoOffset=\"40\">",
        "<dc:creator><rdf:Seq><rdf:li>Ann</rdf:li></rdf:Seq></dc:creator>",
        "<xmp:Thumbnails><rdf:Alt><rdf:li rdf:parseType=\"Resource\"><xmpGImg:image>/9j/4AAQ</xmpGImg:image></rdf:li></rdf:Alt></xmp:Thumbnails>",
        "<Container:Directory><rdf:Seq><rdf:li rdf:parseType=\"Resource\"><Container:Item Item:Semantic=\"Primary\" Item:Mime=\"image/jpeg\"/></rdf:li><rdf:li rdf:parseType=\"Resource\"><Container:Item Item:Semantic=\"MotionPhoto\" Item:Mime=\"video/mp4\" Item:Length=\"40\"/></rdf:li></rdf:Seq></Container:Directory>",
        "</rdf:Description></rdf:RDF></x:xmpmeta>"
    );
    let mpf = [&b"MPF\0II*\0"[..], &8u32.to_le_bytes(), &0u16.to_le_bytes(), &0u32.to_le_bytes()].concat();
    let irb = [crate::meta_write::JPEG_PHOTOSHOP, &crate::meta_write::resource(1036, &[0x5A; 24])].concat();
    let segment = |marker: u8, body: &[u8]| [&[0xFF, marker][..], &((body.len() + 2) as u16).to_be_bytes(), body].concat();
    let plain = file(img, Format::Jpeg, false);
    let video = [&[0, 0, 0, 0x18][..], b"ftypmp42", &[0x11; 28]].concat();
    [&plain[..2], &segment(0xE1, &exif), &segment(0xE1, xmp.as_bytes()), &segment(0xE2, &mpf), &segment(0xED, &irb), &plain[2..], &video].concat()
}

/// A Motion Photo with MPF, thumbnails everywhere, cropped and with a cover box: the copy is the
/// picture alone, ending at its end, no MPF, no thumbnail, no Motion Photo description; the EXIF
/// with the new size, the author kept.
#[test]
fn copy_is_the_picture_alone() {
    let img = motif();
    let thumb = file(&DynamicImage::ImageRgb8(RgbImage::from_fn(16, 12, |x, y| Rgb([(x * 15) as u8, (y * 20) as u8, 7]))), Format::Jpeg, false);
    let bytes = motion_photo(&img, &thumb);
    assert!(!crate::save_back::can_turn_in_place(&bytes, "a.jpg"), "a Motion Photo with MPF");
    let mut doc = Doc::new(64, 48, None);
    doc.crop = Crop::centred(40.0, 30.0);
    doc.objects.push(cover_at(0.0, 0.0, 10.0, 10.0));
    let out = saved(&bytes, "a.jpg", &doc);
    let d = &out.data;
    assert!(d.ends_with(&[0xFF, 0xD9]) && !d.windows(8).any(|w| w == b"ftypmp42"), "nothing after the picture");
    assert!(crate::ultrahdr::segments(d).iter().all(|&(m, s, e)| !(m == 0xE2 && d[s + 4..e].starts_with(b"MPF\0"))), "no MPF");
    let text = String::from_utf8_lossy(d);
    for gone in ["MotionPhoto", "MicroVideo", "Container:Directory", "Container:Item", "Thumbnails", "/9j/4AAQ"] {
        assert!(!text.contains(gone), "{gone} still there");
    }
    assert!(text.contains("<rdf:li>Ann</rdf:li>"), "author kept");
    assert!(!d.windows(thumb.len()).any(|w| w == thumb) && !d.windows(24).any(|w| w == [0x5A; 24]), "a thumbnail is still there");
    let exif = exif::Reader::new().read_from_container(&mut Cursor::new(d)).unwrap();
    let get = |tag| exif.get_field(tag, In::PRIMARY).and_then(|f| f.value.get_uint(0));
    assert_eq!((get(Tag::PixelXDimension), get(Tag::PixelYDimension)), (Some(40), Some(30)));
    assert!(exif.fields().all(|f| f.ifd_num != In::THUMBNAIL));
    assert!(exif.get_field(Tag::Make, In::PRIMARY).is_some());
    assert_eq!(crate::decode::decode_full(d, "x.jpg").unwrap().dimensions(), (40, 30));
}

/// Ultra HDR: the copy stays HDR; the gain map has gain 0 on every pixel the cover box touches
/// even in part and on the box of an opaque object, and is as it was under the highlighter.
#[test]
fn ultra_hdr_gain_under_objects() {
    let bytes = crate::ultrahdr::tests::sample(64, 32);
    let mut doc = Doc::new(64, 32, None);
    // In the photo 18.75..29.25 × 12..20: in the gain map (half the size) 9..15 × 6..10.
    doc.objects.push(cover_at(-8.0, 0.0, 10.5, 8.0));
    // A line 2 wide from 46 to 56 at y 6, round ends: 45..57 × 5..7, in the gain map 22..29 × 2..4.
    doc.objects.push(obj(Shape::Line(Pt::new(14.0, -10.0), Pt::new(24.0, -10.0), None), Style { width: 2.0, ..Style::default() }));
    // The highlighter at 4..12 × 21..27, in the gain map 2..6 × 10..14.
    doc.objects.push(obj(Shape::Highlight { area: Area { centre: Pt::new(-24.0, 8.0), w: 8.0, h: 6.0, rot: 0.0 }, preview: Default::default() }, Style::highlighter()));
    let out = saved(&bytes, "a.jpg", &doc);
    assert!(crate::ultrahdr::is(&out.data), "still Ultra HDR");
    let before = image::load_from_memory(crate::ultrahdr::split(&bytes).unwrap().1).unwrap();
    let after = image::load_from_memory(crate::ultrahdr::split(&out.data).unwrap().1).unwrap();
    let low = |x0: u32, x1: u32, y0: u32, y1: u32| (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (x, y)));
    // Gain 0 exactly before encoding (`ultrahdr::tests::lowest_gain_where_touched`); in the file
    // the JPEG of the gain map halves its colours (4:2:0) and blurs them into a strip 2 pixels
    // high (measured up to 39 in one channel, next to the old gain of the pixel beside it): the
    // luminance stays at most 12, or a third of what it was on the edge.
    for (x, y) in low(9, 15, 6, 10).chain(low(22, 29, 2, 4)) {
        let (p, was) = (after.get_pixel(x, y), before.get_pixel(x, y));
        assert!(luma(p) <= 12.0f32.max(luma(was) / 3.0), "gain at {x},{y}: {p:?}, was {was:?}");
    }
    for (x, y) in low(2, 6, 10, 14) {
        let (a, b) = (after.get_pixel(x, y), before.get_pixel(x, y));
        assert!(a.0[..3].iter().zip(&b.0[..3]).all(|(a, b)| a.abs_diff(*b) <= 8), "under the highlighter at {x},{y}: {a:?} against {b:?}");
    }
    // The photo itself: blue under the cover.
    let photo = image::load_from_memory(crate::ultrahdr::split(&out.data).unwrap().0).unwrap();
    let p = photo.get_pixel(24, 16);
    assert!(p[0] < 30 && p[1] < 30 && p[2] > 220, "{p:?}");
}

/// The format of the copy for each kind of original, and its name.
#[test]
fn format_and_name() {
    for (name, float, transparent, format) in [
        ("a.jpg", false, false, Format::Jpeg),
        ("a.JPEG", false, true, Format::Jpeg),
        ("a.png", false, false, Format::Png),
        ("a.webp", false, false, Format::Webp),
        ("a.avif", false, false, Format::Avif),
        ("a.tiff", false, false, Format::Tiff),
        ("a.bmp", false, false, Format::Bmp),
        ("a.gif", false, false, Format::Gif),
        ("a.qoi", false, false, Format::Qoi),
        ("a.heic", false, false, Format::Jpeg),
        ("a.heic", false, true, Format::Png),
        ("a.jxl", false, false, Format::Jpeg),
        ("a.jxl", false, true, Format::Png),
        ("a.cr3", false, false, Format::Jpeg),
        ("a.dng", false, false, Format::Jpeg),
        ("a.psd", false, false, Format::Jpeg),
        ("a.psd", false, true, Format::Png),
        ("a.exr", true, false, Format::Png),
        ("a.hdr", true, false, Format::Png),
        ("a.tga", false, true, Format::Png),
    ] {
        assert_eq!(target(name, float, transparent), format, "{name} {float} {transparent}");
    }
    let taken = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    assert_eq!(copy_name("a.jpg", None, "edited", &taken(&["a.jpg"])), "a (edited).jpg");
    assert_eq!(copy_name("foto.jpg", None, "modificata", &taken(&["foto.jpg"])), "foto (modificata).jpg");
    assert_eq!(copy_name("a.jpg", None, "edited", &taken(&["a.jpg", "A (Edited).JPG"])), "a (edited 2).jpg");
    assert_eq!(copy_name("IMG_1.JPG", None, "edited", &[]), "IMG_1 (edited).JPG");
    assert_eq!(copy_name("x.heic", Some("jpg"), "modificata", &[]), "x (modificata).jpg");
    assert_eq!(copy_name("a (edited).jpg", None, "edited", &taken(&["a.jpg", "a (edited).jpg"])), "a (edited 2).jpg");
    assert_eq!(copy_name("a (edited 2).jpg", None, "edited", &taken(&["a (edited).jpg", "a (edited 2).jpg"])), "a (edited 3).jpg");
    assert_eq!(copy_name("a (edited by Ann).jpg", None, "edited", &[]), "a (edited by Ann) (edited).jpg");
    assert_eq!(copy_name("README", Some("png"), "edited", &[]), "README (edited).png");
    assert_eq!(copy_name(".hidden.png", None, "edited", &[]), ".hidden (edited).png");
    assert_eq!((depth(false, true, Format::Jpeg), depth(false, true, Format::Png), depth(false, true, Format::Tiff), depth(true, false, Format::Png), depth(false, false, Format::Jpeg)), (Depth::To8Bits, Depth::Kept, Depth::Kept, Depth::HdrCut, Depth::Kept));
}

/// PNG and TIFF at 16 bits stay at 16 bits: outside the objects every value as it was (not
/// multiples of 257, so a pass through 8 bits would show).
#[test]
fn sixteen_bits_kept() {
    let img = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(40, 30, |x, y| Rgb([1000 + x as u16 * 37, 2000 + y as u16 * 101, 30001])));
    let mut doc = Doc::new(40, 30, None);
    doc.objects.push(cover_at(-10.0, -8.0, 6.0, 4.0));
    doc.objects.push(obj(Shape::Line(Pt::new(5.0, 5.0), Pt::new(15.0, 5.0), None), Style::default()));
    for (format, name) in [(Format::Png, "a.png"), (Format::Tiff, "a.tif")] {
        let out = saved(&file(&img, format, false), name, &doc);
        assert_eq!((out.depth, out.lossy), (Depth::Kept, false));
        let back = crate::decode::decode_full(&out.data, name).unwrap();
        let DynamicImage::ImageRgb16(b) = &back else { panic!("{name}: {:?}", back.color()) };
        let DynamicImage::ImageRgb16(a) = &img else { unreachable!() };
        assert_eq!(b.get_pixel(1, 1), a.get_pixel(1, 1), "{name}");
        assert_eq!(b.get_pixel(30, 25), a.get_pixel(30, 25), "{name}");
        assert_eq!(b.get_pixel(10, 7).0, [0, 0, 65535], "{name}: under the cover");
        assert!(b.get_pixel(30, 20).0[0] > 60000, "{name}: the red line");
    }
}

/// OpenEXR becomes PNG at 16 bits, and the Save window will say that above white is cut.
#[test]
fn exr_to_png_16() {
    let img = DynamicImage::ImageRgb32F(image::ImageBuffer::from_pixel(20, 10, Rgb([0.5f32, 0.25, 2.0])));
    let mut doc = Doc::new(20, 10, None);
    doc.objects.push(obj(Shape::Line(Pt::new(-5.0, 0.0), Pt::new(5.0, 0.0), None), Style::default()));
    let out = saved(&file(&img, Format::Exr, false), "a.exr", &doc);
    assert_eq!((out.name.as_str(), out.depth), ("a (edited).png", Depth::HdrCut));
    let back = crate::decode::decode_full(&out.data, &out.name).unwrap();
    let DynamicImage::ImageRgb16(b) = &back else { panic!("{:?}", back.color()) };
    assert_eq!(b.get_pixel(1, 1).0, [32768, 16384, 65535]);
}

/// Only quarter turns: the viewer's Save, a JPEG without re-encoding. Animations are refused.
#[test]
fn turns_alone_and_animations() {
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(64, 32, |x, y| Rgb([(x * 4) as u8, (y * 8) as u8, 9])));
    let bytes = file(&img, Format::Jpeg, false);
    let mut doc = Doc::new(64, 32, None);
    doc.turned(Op::Cw);
    let out = saved(&bytes, "a.jpg", &doc);
    assert_eq!((out.data.clone(), out.lossy), crate::save_back::turned_file(&bytes, "a.jpg", doc.turn).unwrap());
    assert!(!out.lossy);
    let apng = crate::encode::tests::apng(&file(&img, Format::Png, false));
    let original = Original::with_meta(img.clone(), &apng);
    assert!(save(&Doc::new(64, 32, None), &original, &apng, "a.png", "edited", &[], &options()).is_err());
}

/// A grey photo gets colours for a red line; the line at half opacity is half red over the grey.
#[test]
fn grey_photo_and_opacity() {
    let img = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(40, 20, image::Luma([100])));
    let mut doc = Doc::new(40, 20, None);
    doc.objects.push(obj(Shape::Line(Pt::new(-15.0, 0.0), Pt::new(15.0, 0.0), None), Style { opacity: 0.5, width: 6.0, ..Style::default() }));
    let out = saved(&file(&img, Format::Png, false), "a.png", &doc);
    let back = crate::decode::decode_full(&out.data, "a.png").unwrap();
    assert!(back.color().has_color());
    let p = back.get_pixel(20, 10);
    assert!(p[0].abs_diff(178) <= 1 && p[1].abs_diff(50) <= 1 && p[2].abs_diff(50) <= 1, "{p:?}");
    assert_eq!(back.get_pixel(2, 2).0, [100, 100, 100, 255]);
}

/// Real photos: `EYECANSEE_SAMPLES=<folder>` (never changed), the copies in
/// `EYECANSEE_SAMPLES_OUT` to look at. Each JPEG with a cover box in the middle: the copy ends at
/// the end of its picture unless it stays Ultra HDR, has no MPF but an Ultra HDR one's, no EXIF
/// thumbnail and no Motion Photo description.
#[test]
#[ignore]
fn save_live() {
    let dir = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES");
    let out_dir = std::env::var("EYECANSEE_SAMPLES_OUT").expect("EYECANSEE_SAMPLES_OUT");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if Format::of_file(&name) != Some(Format::Jpeg) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let original = Original::with_meta_of(crate::decode::decode_full(&bytes, &name).unwrap(), &bytes, &name);
        let (w, h) = original.image.dimensions();
        let mut doc = Doc::new(w, h, None);
        doc.objects.push(cover_at(0.0, 0.0, w as f32 / 4.0, h as f32 / 4.0));
        let out = save(&doc, &original, &bytes, &name, "edited", std::slice::from_ref(&name), &crate::decode::usvg_options()).unwrap();
        let d = &out.data;
        let hdr = crate::ultrahdr::is(d);
        assert_eq!(hdr, crate::ultrahdr::is(&bytes), "{name}: Ultra HDR kept");
        if !hdr {
            assert!(d.ends_with(&[0xFF, 0xD9]), "{name}: something after the picture");
            assert!(crate::ultrahdr::segments(d).iter().all(|&(m, s, e)| !(m == 0xE2 && d[s + 4..e].starts_with(b"MPF\0"))), "{name}: MPF");
            let text = String::from_utf8_lossy(d);
            assert!(!text.contains("MotionPhoto") && !text.contains("MicroVideo"), "{name}: Motion Photo");
        }
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(d)) {
            assert!(exif.fields().all(|f| f.ifd_num != In::THUMBNAIL), "{name}: EXIF thumbnail");
        }
        std::fs::write(std::path::Path::new(&out_dir).join(&out.name), d).unwrap();
        println!("{name} → {} ({} bytes, Ultra HDR {hdr})", out.name, d.len());
    }
}

/// The phone photos of the test files (`notes/test-files/online/telefono`, from the private
/// release with `test-files/scarica.sh online-telefono`, or `EYECANSEE_PHONE_SAMPLES`), each with
/// a cover box: the Motion Photo's copy is the picture alone, without the video and what
/// described it; the Apple photo keeps its gain map (MPF of two pictures), gain 0 under the box.
#[test]
#[ignore]
fn phone_samples() {
    let dir = std::env::var_os("EYECANSEE_PHONE_SAMPLES").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("notes/test-files/online/telefono"), std::path::PathBuf::from);
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name} in {}: {e} (test-files/scarica.sh online-telefono)", dir.display()));
    let blue = |img: &DynamicImage, x, y| {
        let p = img.get_pixel(x, y);
        assert!(p[0] < 30 && p[1] < 30 && p[2] > 220, "not covered at {x},{y}: {p:?}");
    };

    let bytes = read("motion_photo.jpeg");
    assert!(bytes.windows(8).any(|w| w == b"ftypisom") && String::from_utf8_lossy(&bytes).contains("MotionPhoto"), "a Motion Photo with its video");
    let mut doc = Doc::new(1024, 768, None);
    doc.objects.push(cover_at(0.0, 0.0, 200.0, 100.0));
    let out = saved(&bytes, "motion_photo.jpeg", &doc);
    assert_eq!(out.name, "motion_photo (edited).jpeg");
    let d = &out.data;
    assert!(d.ends_with(&[0xFF, 0xD9]) && !d.windows(4).any(|w| w == b"ftyp"), "nothing after the picture");
    let text = String::from_utf8_lossy(d);
    for gone in ["MotionPhoto", "Container:Directory", "Container:Item"] {
        assert!(!text.contains(gone), "{gone} still there");
    }
    let back = crate::decode::decode_full(d, "x.jpg").unwrap();
    assert_eq!(back.dimensions(), (1024, 768));
    blue(&back, 512, 384);

    let bytes = read("apple_gainmap_new.jpg");
    assert!(crate::ultrahdr::is(&bytes), "two pictures in MPF, the gain map up to the end");
    let mut doc = Doc::new(384, 512, None);
    // In the photo 142..242 × 206..306.
    doc.objects.push(cover_at(0.0, 0.0, 100.0, 100.0));
    let out = saved(&bytes, "apple_gainmap_new.jpg", &doc);
    assert!(crate::ultrahdr::is(&out.data), "still with its gain map");
    let (photo, gain) = crate::ultrahdr::split(&out.data).unwrap();
    blue(&image::load_from_memory(photo).unwrap(), 192, 256);
    let gain = image::load_from_memory(gain).unwrap();
    let k = gain.width() as f32 / 384.0;
    // Inside the box by a pixel of the gain map: there the JPEG of the gain map does not reach
    // the pixels beside it.
    let (x0, x1, y0, y1) = ((142.0 * k).ceil() as u32 + 1, (242.0 * k) as u32 - 1, (206.0 * k).ceil() as u32 + 1, (306.0 * k) as u32 - 1);
    for (x, y) in (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))) {
        assert!(luma(gain.get_pixel(x, y)) <= 12.0, "gain at {x},{y}: {:?}", gain.get_pixel(x, y));
    }
    println!("gain map {}×{}, checked {}..{} × {}..{}", gain.width(), gain.height(), x0, x1, y0, y1);
}

/// libmphoto's Motion Photo turned without loss: no save keeps the video after the picture, so
/// none keeps what describes it. The viewer's Save leaves the file alone (the original keeps
/// both); the lossless turn itself writes the picture alone (libjpeg-turbo stops at its end);
/// the editor's copy of a turn is the picture alone too. The lossless turn keeps
/// `Container:Directory` (an Ultra HDR photo needs it): only the GCamera tags go there. If a
/// save ever keeps the video, the description must stay (`xmp::fix`).
#[test]
#[ignore]
fn motion_photo_turned() {
    let dir = std::env::var_os("EYECANSEE_PHONE_SAMPLES").map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("notes/test-files/online/telefono"), std::path::PathBuf::from);
    let bytes = std::fs::read(dir.join("motion_photo.jpeg")).unwrap_or_else(|e| panic!("motion_photo.jpeg in {}: {e} (test-files/scarica.sh online-telefono)", dir.display()));
    let video = |d: &[u8]| d.windows(4).any(|w| w == b"ftyp");
    // The GCamera namespace, whatever its prefix (here "Camera:").
    let tagged = |d: &[u8]| [":MotionPhoto", ":MicroVideo"].iter().any(|t| String::from_utf8_lossy(d).contains(t));
    let described = |d: &[u8]| String::from_utf8_lossy(d).contains("MotionPhoto");
    assert!(video(&bytes) && tagged(&bytes));
    let cw = crate::transform::Turn::default().then(Op::Cw);
    // The viewer's Save (`replace::rewritten`) and Batch's lossless path skip it.
    assert!(!crate::save_back::can_turn_in_place(&bytes, "motion_photo.jpeg") && !crate::ultrahdr::is(&bytes));
    assert!(crate::save_back::turned_file(&bytes, "motion_photo.jpeg", cw).is_err());
    let turned = crate::transform::jpeg_lossless(&bytes, cw).unwrap();
    assert!(turned.ends_with(&[0xFF, 0xD9]) && !video(&turned) && !tagged(&turned), "lossless turn: the picture alone");
    let mut doc = Doc::new(1024, 768, None);
    doc.turned(Op::Cw);
    let out = saved(&bytes, "motion_photo.jpeg", &doc);
    assert!(!video(&out.data) && !described(&out.data), "editor's copy: the picture alone");
    assert_eq!(crate::decode::decode_full(&out.data, "x.jpg").unwrap().dimensions(), (768, 1024));
}

/// Layers: a cover box hides the line drawn before it, and a line drawn after it stays on top.
#[test]
fn cover_hides_what_is_under_it() {
    let line = |y| obj(Shape::Line(Pt::new(-20.0, y), Pt::new(20.0, y), None), Style { color: [255, 0, 0], width: 2.0, ..Style::default() });
    let mut doc = Doc::new(64, 48, None);
    doc.objects = vec![line(-2.0), cover_at(0.0, 0.0, 30.0, 12.0), line(2.0)];
    let img = result(&doc, &Original::with_meta(motif(), &[]), &options());
    let at = |x, y| img.get_pixel(x, y);
    // Centre at 32, 24: the first line at y 22 is under the box, the second at 26 over it.
    assert_eq!(at(32, 22).0[..3], BLUE);
    assert_eq!(at(32, 26).0[..3], [255, 0, 0]);
}

/// A filter is applied to the photo at full size in the saved image, under the objects drawn
/// (also those drawn before it): black and white on the whole image, the red line still red.
#[test]
fn filter_under_the_objects() {
    use crate::edit::filter::Filter;
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(obj(Shape::Line(Pt::new(-20.0, 0.0), Pt::new(20.0, 0.0), None), Style { color: [255, 0, 0], width: 2.0, ..Style::default() }));
    doc.objects.push(obj(Shape::Filter { area: Area { centre: Pt::new(0.0, 0.0), w: 64.0, h: 48.0, rot: 0.0 }, filter: Filter::Gray, amount: 0.0, preview: Default::default() }, Style::default()));
    let img = result(&doc, &Original::with_meta(motif(), &[]), &options());
    let p = img.get_pixel(32, 24).0;
    assert_eq!(p[..3], [255, 0, 0], "the line over the filter");
    for (x, y) in [(5, 5), (40, 30), (20, 15)] {
        let [r, g, b, _] = img.get_pixel(x, y).0;
        assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1, "grey at {x},{y}: {r} {g} {b}");
    }
}


/// An ellipse in the saved image: its fill inside, the photo in the corners of its box.
#[test]
fn ellipse_saved() {
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(obj(Shape::Ellipse(Area { centre: Pt::new(0.0, 0.0), w: 40.0, h: 20.0, rot: 0.0 }), Style { width: 2.0, fill: Some(BLUE), ..Style::default() }));
    let photo = motif();
    let img = result(&doc, &Original::with_meta(photo.clone(), &[]), &options());
    assert_eq!(img.get_pixel(32, 24).0[..3], BLUE);
    assert_eq!(img.get_pixel(13, 15), photo.get_pixel(13, 15), "corner of the box");
}

/// A stroke by hand in the saved image, where it was drawn.
#[test]
fn by_hand_saved() {
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(obj(Shape::Pen(vec![Pt::new(-20.0, -10.0), Pt::new(0.0, 0.0), Pt::new(20.0, -10.0)]), Style { color: BLUE, width: 3.0, ..Style::default() }));
    let img = result(&doc, &Original::with_meta(motif(), &[]), &options());
    assert_eq!(img.get_pixel(12, 14).0[..3], BLUE);
    assert_eq!(img.get_pixel(52, 14).0[..3], BLUE);
}

/// A step in the saved image: its disc in its colour around the white number.
#[test]
fn step_saved() {
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(crate::edit::geom::step(Pt::new(0.0, 0.0), 2, 0.0, Style { color: BLUE, width: 30.0, ..Style::default() }));
    let img = result(&doc, &Original::with_meta(motif(), &[]), &options());
    assert_eq!(img.get_pixel(32, 11).0[..3], BLUE);
    let white = (24..40).flat_map(|x| (16..32).map(move |y| (x, y))).filter(|&(x, y)| img.get_pixel(x, y).0[..3].iter().all(|&v| v > 180)).count();
    assert!(white > 15, "{white}");
}

/// A bubble in the saved image: its border, its fill, its tail.
#[test]
fn bubble_saved() {
    let mut doc = Doc::new(64, 48, None);
    let style = Style { color: BLUE, width: 2.0, fill: Some([255, 255, 255]), letters: 8.0, ..Style::default() };
    doc.objects.push(obj(Shape::Bubble { area: Area { centre: Pt::new(0.0, -8.0), w: 40.0, h: 20.0, rot: 0.0 }, tail: Pt::new(-10.0, 20.0), text: String::new(), font: None }, style));
    let img = result(&doc, &Original::with_meta(motif(), &[]), &options());
    // The box 12..52 × 6..26: the border above it, white inside, the tail under it.
    assert_eq!(img.get_pixel(32, 5).0[..3], BLUE);
    assert_eq!(img.get_pixel(40, 12).0[..3], [255; 3]);
    assert_eq!(img.get_pixel(25, 34).0[..3], [255; 3]);
}

/// A lens in the saved image shows the photo magnified at full size: on a gradient, 8 pixels
/// right of its centre the photo of 4 pixels right of it; outside it the photo as it was. Its box
/// is one where an Ultra HDR photo loses its gain (what it shows is from elsewhere).
#[test]
fn lens_saved() {
    let photo = DynamicImage::ImageRgb8(RgbImage::from_fn(64, 48, |x, _| Rgb([(x * 4) as u8; 3])));
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(obj(Shape::Lens { area: Area { centre: Pt::new(0.0, 0.0), w: 24.0, h: 24.0, rot: 0.0 }, preview: Default::default() }, Style { width: 0.0, zoom: 2, ..Style::default() }));
    let img = result(&doc, &Original::with_meta(photo.clone(), &[]), &options());
    let v = img.get_pixel(40, 24).0[0];
    assert!(v.abs_diff(144) <= 4, "magnified: {v}");
    assert_eq!(img.get_pixel(50, 24), photo.get_pixel(50, 24));
    let mut copy = photo.clone();
    doc.edit().apply(&mut copy);
    assert_eq!(paint(&doc, &photo, &mut copy, &options()).len(), 1);
}

/// Spotlights in the saved image: inside them the photo as it was, outside it darkened by the
/// darker one, a line drawn over the darkening; a transparent pixel stays transparent.
#[test]
fn spotlight_saved() {
    let photo = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(64, 48, |x, _| if x < 4 { image::Rgba([0; 4]) } else { image::Rgba([200, 100, 50, 255]) }));
    let mut doc = Doc::new(64, 48, None);
    let spot = |x, opacity| obj(Shape::Spotlight(Area { centre: Pt::new(x, 0.0), w: 12.0, h: 12.0, rot: 0.0 }), Style { opacity, ..Style::default() });
    doc.objects = vec![obj(Shape::Line(Pt::new(-20.0, 16.0), Pt::new(20.0, 16.0), None), Style { color: BLUE, width: 2.0, ..Style::default() }), spot(-14.0, 0.25), spot(14.0, 0.5)];
    let img = result(&doc, &Original::with_meta(photo, &[]), &options());
    assert_eq!((img.get_pixel(18, 24).0, img.get_pixel(46, 24).0), ([200, 100, 50, 255], [200, 100, 50, 255]));
    assert_eq!(img.get_pixel(32, 24).0, [100, 50, 25, 255]);
    assert_eq!(img.get_pixel(32, 40).0[..3], BLUE);
    assert_eq!(img.get_pixel(1, 24).0[3], 0);
}

/// A stamp in the saved image, in its colour where its icon has lines; the photo between them.
#[test]
fn stamp_saved() {
    let mut doc = Doc::new(64, 48, None);
    doc.objects.push(crate::edit::geom::stamp(Pt::new(0.0, 0.0), 0.0, Style { color: BLUE, width: 40.0, ..Style::default() }));
    let photo = motif();
    let img = result(&doc, &Original::with_meta(photo.clone(), &[]), &options());
    let blue = (12..52).flat_map(|x| (4..44).map(move |y| (x, y))).filter(|&(x, y)| img.get_pixel(x, y).0[..3] == BLUE).count();
    assert!(blue > 50, "{blue}");
    assert_eq!(img.get_pixel(14, 6), photo.get_pixel(14, 6));
}

/// The highlighter in the saved image multiplies the photo: black stays black, white becomes its
/// colour, at its opacity half way; a line drawn before it stays red over it (it is under the
/// objects); 16 bits stay 16 bits.
#[test]
fn highlighter_multiplies() {
    let photo = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(64, 48, |x, _| image::Rgb([if x < 32 { 0 } else { 65535 }; 3])));
    let mut doc = Doc::new(64, 48, None);
    let mark = |y, opacity| obj(Shape::Highlight { area: Area { centre: Pt::new(0.0, y), w: 40.0, h: 8.0, rot: 0.0 }, preview: Default::default() }, Style { opacity, ..Style::highlighter() });
    doc.objects = vec![obj(Shape::Line(Pt::new(4.0, -20.0), Pt::new(4.0, -12.0), None), Style { width: 2.0, ..Style::default() }), mark(-16.0, 1.0), mark(10.0, 0.5)];
    let img = result(&doc, &Original::with_meta(photo, &[]), &options());
    let DynamicImage::ImageRgb16(b) = &img else { panic!("{:?}", img.color()) };
    assert_eq!((b.get_pixel(20, 8).0, b.get_pixel(44, 8).0), ([0, 0, 0], [65535, 59110, 0]));
    assert_eq!(b.get_pixel(44, 34).0, [65535, 62323, 32768]);
    assert_eq!(b.get_pixel(36, 8).0, [65535, 0, 0], "the line over it");
    assert_eq!(b.get_pixel(44, 24).0, [65535; 3], "outside");
}

/// A text with an outline and a box in the saved image: the box in its colour round the letters,
/// the photo past it.
#[test]
fn text_box_saved() {
    use crate::edit::doc::Back;
    let mut doc = Doc::new(64, 48, None);
    let style = Style { color: [255, 0, 0], outline: Some(([255, 255, 255], 0.1)), back: Some(Back { color: BLUE, opacity: 1.0, margin: 0.3, radius: 0.0 }), ..Style::default() };
    doc.objects.push(obj(Shape::Text { at: Pt::new(0.0, 0.0), text: "Hi".into(), size: 16.0, rot: 0.0, font: None, path: None }, style));
    let photo = motif();
    let img = result(&doc, &Original::with_meta(photo.clone(), &[]), &options());
    // Letters 16 high round 32, 24, a line 19.2 high, the box 4.8 past it: from y 9.6.
    assert_eq!(img.get_pixel(32, 11).0[..3], BLUE);
    assert_eq!(img.get_pixel(32, 8), photo.get_pixel(32, 8));
}

/// A lens over a cover box never shows what the box hides: in the saved PNG, wherever the lens
/// is drawn before or after the box, every pixel inside the lens is the box's colour (the lens
/// shows a part 15 wide, inside the box) and none of the pattern is left; over Pixelate it shows
/// the blocks, not the 2-pixel squares of the pattern.
#[test]
fn lens_over_a_cover() {
    use crate::edit::filter::Filter;
    let bytes = file(&motif(), Format::Png, false);
    let lens = obj(Shape::Lens { area: Area { centre: Pt::new(0.0, -6.0), w: 30.0, h: 30.0, rot: 0.0 }, preview: Default::default() }, Style { width: 0.0, zoom: 2, ..Style::default() });
    for before in [true, false] {
        let mut doc = Doc::new(64, 48, None);
        let cover = cover_at(0.0, -6.0, 20.0, 18.0);
        doc.objects = if before { vec![lens.clone(), cover] } else { vec![cover, lens.clone()] };
        let img = image::load_from_memory(&saved(&bytes, "a.png", &doc).data).unwrap();
        for (x, y) in (18..46).flat_map(|x| (5..31).map(move |y| (x, y))) {
            assert_eq!(img.get_pixel(x, y).0[..3], BLUE, "lens {} the box, at {x},{y}", if before { "before" } else { "after" });
        }
        assert!(correlation(&img, &motif()) < 0.5);
    }
    let mut doc = Doc::new(64, 48, None);
    doc.objects = vec![obj(Shape::Filter { area: Area { centre: Pt::new(0.0, -6.0), w: 32.0, h: 24.0, rot: 0.0 }, filter: Filter::Pixelate, amount: 8.0, preview: Default::default() }, Style::default()), lens];
    let img = image::load_from_memory(&saved(&bytes, "a.png", &doc).data).unwrap();
    // The squares of the pattern would make neighbours differ by 255 every 4 pixels in the lens.
    let jumps = (18..45).flat_map(|x| (6..30).map(move |y| (x, y))).filter(|&(x, y)| (luma(img.get_pixel(x, y)) - luma(img.get_pixel(x + 1, y))).abs() > 100.0).count();
    assert!(jumps < 40, "{jumps} jumps");
}

/// A magnifier with the objects shows those drawn before it, magnified around its centre (a
/// line through it stays level, twice as wide), also when the lens is turned by a free angle;
/// not those drawn after it (they stay over it at their size); without the option, the photo
/// alone.
#[test]
fn lens_with_objects() {
    let photo = DynamicImage::ImageRgb8(RgbImage::from_pixel(64, 48, Rgb([255; 3])));
    // A blue line across the middle (y 24), 2 wide: in a lens 2 times, 4 wide (22..26).
    let line = obj(Shape::Line(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0), None), Style { color: BLUE, width: 2.0, ..Style::default() });
    let lens = |rot, with_objects| obj(Shape::Lens { area: Area { centre: Pt::new(0.0, 0.0), w: 24.0, h: 24.0, rot }, preview: Default::default() }, Style { width: 0.0, zoom: 2, with_objects, ..Style::default() });
    let shown = |objects: Vec<Obj>| {
        let doc = Doc { objects, ..Doc::new(64, 48, None) };
        result(&doc, &Original::with_meta(photo.clone(), &[]), &options())
    };
    for rot in [0.0, 30.0] {
        let img = shown(vec![line.clone(), lens(rot, true)]);
        // Turned, the lens's picture is drawn turned: its edges soften by a pixel at most.
        let blue = |y| img.get_pixel(40, y).0[..3].iter().zip(BLUE).all(|(a, b)| a.abs_diff(b) <= 60);
        assert!(blue(22) && blue(23) && blue(24) && blue(25) && img.get_pixel(40, 27).0[0] == 255, "{rot}°");
        // Without the option the lens shows the photo: white over the line drawn before it.
        let plain = shown(vec![line.clone(), lens(rot, false)]);
        assert_eq!(plain.get_pixel(40, 24).0[..3], [255; 3], "{rot}°");
    }
    // Drawn after the lens: over it at its own width (23..25), not magnified.
    let img = shown(vec![lens(0.0, true), line]);
    assert_eq!((img.get_pixel(40, 24).0[..3].to_vec(), img.get_pixel(40, 26).0[0]), (BLUE.to_vec(), 255));
}
