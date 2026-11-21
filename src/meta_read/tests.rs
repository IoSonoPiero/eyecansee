//! The panel of a file read in pieces is the panel of the whole file (#177): the same `Meta`,
//! with the bytes not read left at zero and at another value (a reader that looked at them would
//! give a different panel), and with a first read so small that every plan walks in rounds.

use std::panic::AssertUnwindSafe;

use exif::experimental::Writer;
use exif::{Field, In, Rational, Tag, Value};
use image::DynamicImage;

use super::{PANEL, Reading, pieces, read};
use crate::encode::{Format, Options, Original};
use crate::sidecar::Sidecar;
use crate::source::{self, Store};

/// A first read so small, and reads so cheap, that every plan walks in several rounds.
const STRESS: Reading = Reading { fill: 0, head: 1024, cost: 0 };

/// The fills of the bytes not read.
const FILLS: [u8; 2] = [0, 0xA5];

pub(crate) fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

/// The panel's JSON of `bytes`, or that it panicked.
pub(crate) fn panel(bytes: &[u8], name: &str, sidecar: Option<&Sidecar>) -> String {
    std::panic::catch_unwind(AssertUnwindSafe(|| crate::meta::read_file_with(bytes, name, sidecar).map(|m| m.to_json()).unwrap_or_default())).unwrap_or_else(|_| "panic".into())
}

/// How the panel of `path` read in pieces `how` (with each fill) differs from `whole`'s; `None`
/// when it is read whole.
pub(crate) async fn differences(op: &Store, path: &str, whole: &[u8], sidecar: Option<&Sidecar>, how: Reading) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for fill in FILLS {
        let bytes = pieces(op, path, whole.len() as u64, Reading { fill, ..how }).await?;
        for side in [sidecar, None] {
            let (a, b) = (panel(whole, path, side), panel(&bytes, path, side));
            if a != b {
                out.push(format!("fill {fill:#x}, sidecar {}:\n  whole  {a}\n  pieces {b}", side.is_some()));
            }
        }
    }
    Some(out)
}

pub(crate) async fn stored(files: &[(&str, &[u8])]) -> Store {
    let dal = opendal::Operator::new(opendal::services::Memory::default()).unwrap();
    for (path, bytes) in files {
        dal.write(path, bytes.to_vec()).await.unwrap();
    }
    Store::Dal(dal)
}

/// Grey noise, which no encoder makes small.
fn noise(w: u32, h: u32) -> DynamicImage {
    let mut x = 0x2545_F491u32;
    DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |_, _| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        image::Rgb([x as u8, (x >> 8) as u8, (x >> 16) as u8])
    }))
}

fn field(tag: Tag, value: Value) -> Field {
    Field { tag, ifd_num: In::PRIMARY, value }
}

fn exif_block() -> Vec<u8> {
    let ascii = |s: &str| Value::Ascii(vec![s.as_bytes().to_vec()]);
    let fields = [
        field(Tag::Make, ascii("Maker")),
        field(Tag::Model, ascii("Camera 1")),
        field(Tag::DateTimeOriginal, ascii("2026:01:02 03:04:05")),
        field(Tag::ExposureTime, Value::Rational(vec![(1, 250).into()])),
        field(Tag::GPSLatitudeRef, ascii("N")),
        field(Tag::GPSLatitude, Value::Rational(vec![(10, 1).into(), (20, 1).into(), Rational::from((30, 1))])),
        field(Tag::GPSLongitudeRef, ascii("E")),
        field(Tag::GPSLongitude, Value::Rational(vec![(40, 1).into(), (50, 1).into(), Rational::from((0, 1))])),
    ];
    let mut writer = Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut tiff = std::io::Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    tiff.into_inner()
}

const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:description><rdf:Alt><rdf:li xml:lang="x-default">A test picture</rdf:li></rdf:Alt></dc:description><dc:creator><rdf:Seq><rdf:li>Someone</rdf:li></rdf:Seq></dc:creator></rdf:Description></rdf:RDF></x:xmpmeta>"#;

pub(crate) fn segment(marker: u8, parts: &[&[u8]]) -> Vec<u8> {
    let body = parts.concat();
    [&[0xFF, marker][..], &((body.len() + 2) as u16).to_be_bytes(), &body].concat()
}

/// A JPEG of noise with EXIF, XMP and IPTC (`xmp`: with its XMP).
fn jpeg(w: u32, h: u32, xmp: bool) -> Vec<u8> {
    let mut coded = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut coded, 95).encode_image(&noise(w, h)).unwrap();
    let iim = crate::xmp::tests::iim(&[(120, "A caption"), (25, "keyword")]);
    let mut meta = segment(0xE1, &[b"Exif\0\0", &exif_block()]);
    if xmp {
        meta.extend(segment(0xE1, &[crate::meta_write::JPEG_XMP, XMP.as_bytes()]));
    }
    meta.extend(segment(0xED, &[crate::meta_write::JPEG_PHOTOSHOP, &crate::meta_write::irb_of_iim(&iim)]));
    [&coded[..2], &meta, &coded[2..]].concat()
}

/// The same panel from the pieces of `bytes` as from all of them, read in several rounds, and
/// fewer bytes read than the file has.
fn same_from_pieces(name: &str, bytes: &[u8]) {
    rt().block_on(async {
        let op = stored(&[(name, bytes)]).await;
        let found = differences(&op, name, bytes, None, STRESS).await.unwrap_or_else(|| panic!("{name}: read whole"));
        assert!(found.is_empty(), "{name}: {found:#?}");
        source::BYTES_READ.with(|n| n.set(0));
        pieces(&op, name, bytes.len() as u64, STRESS).await.unwrap();
        let read = source::BYTES_READ.with(std::cell::Cell::get);
        assert!(read < bytes.len() as u64 / 2, "{name}: {read} of {} bytes read", bytes.len());
        let json = panel(bytes, name, None);
        assert!(json.contains("meta-format"), "{name}: {json}");
    });
}

#[test]
fn jpeg_from_its_segments() {
    same_from_pieces("a.jpg", &jpeg(700, 500, true));
    same_from_pieces("b.jpg", &jpeg(700, 500, false));
}

#[test]
fn each_format_written_from_its_pieces() {
    let original = jpeg(64, 64, true);
    for (format, w) in [(Format::Png, 500), (Format::Webp, 600), (Format::Tiff, 500), (Format::Avif, 160)] {
        let o = Options { format, avif_speed: 10, png_level: 1, ..Options::default() };
        let bytes = Original::with_meta(noise(w, w), &original).encode(&o).unwrap();
        same_from_pieces(&format!("a.{}", format.ext()), &bytes);
    }
}

/// A JPEG XL in its container, its codestream (a red pixel, from libjxl) made long, with EXIF and
/// XMP boxes after it.
#[test]
fn jxl_boxes_after_the_codestream() {
    const RED: &str = "ff0a0010fc64020013880200c000b59f200000152aa38c1bbc9cebf9f24387c5b48deb0c6db56d610963b3bd30484838838d8b04df9ca591690624a29204";
    let code: Vec<u8> = (0..RED.len()).step_by(2).map(|i| u8::from_str_radix(&RED[i..i + 2], 16).unwrap()).collect();
    let boxed = |kind: &[u8], content: &[u8]| [&((content.len() + 8) as u32).to_be_bytes()[..], kind, content].concat();
    let long = [code, vec![0; 300_000]].concat();
    let bytes = [
        b"\0\0\0\x0CJXL \r\n\x87\n".to_vec(),
        boxed(b"ftyp", b"jxl \0\0\0\0jxl "),
        boxed(b"jxlc", &long),
        boxed(b"Exif", &[&[0u8; 4][..], &exif_block()].concat()),
        boxed(b"xml ", XMP.as_bytes()),
    ]
    .concat();
    same_from_pieces("a.jxl", &bytes);
    assert!(panel(&bytes, "a.jxl", None).contains("A test picture"));
}

#[cfg(feature = "magick")]
#[test]
fn psd_from_its_resources() {
    let bytes = crate::psd::encode(&noise(500, 400), &jpeg(64, 64, true)).unwrap();
    same_from_pieces("a.psd", &bytes);
}

/// A PNG whose pixels are in chunks of 8 KB, as libpng writes them, with an eXIf and an XMP
/// after them.
fn png_small_chunks(w: u32, h: u32) -> Vec<u8> {
    let mut png = Vec::new();
    noise(w, h).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let chunk = |kind: &[u8], data: &[u8]| {
        let crc = super::png::crc(&[kind, data].concat());
        [&(data.len() as u32).to_be_bytes()[..], kind, data, &crc.to_be_bytes()].concat()
    };
    // The chunks of the PNG written: the pixels joined, the others kept.
    let (mut before, mut pixels, mut at) = (png[..8].to_vec(), Vec::new(), 8);
    while at < png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        match &png[at + 4..at + 8] {
            b"IDAT" => pixels.extend_from_slice(&png[at + 8..at + 8 + len]),
            b"IEND" => {}
            _ => before.extend_from_slice(&png[at..at + 12 + len]),
        }
        at += 12 + len;
    }
    let xmp = [&b"XML:com.adobe.xmp\0\0\0\0\0"[..], XMP.as_bytes()].concat();
    let after = [chunk(b"eXIf", &exif_block()), chunk(b"iTXt", &xmp), chunk(b"IEND", b"")].concat();
    let small: Vec<u8> = pixels.chunks(8192).flat_map(|p| chunk(b"IDAT", p)).collect();
    [before, small, after].concat()
}

/// The chunks after the pixels are found from the end of the file.
#[test]
fn png_metadata_after_small_pixel_chunks() {
    let bytes = png_small_chunks(500, 400);
    same_from_pieces("small.png", &bytes);
    let json = panel(&bytes, "small.png", None);
    assert!(json.contains("A test picture") && json.contains("meta-make"), "{json}");
}

/// A JPEG without an XMP of its own and a second picture (MPF) with one, which the panel finds by
/// looking for a packet: the second picture's segments are read too.
#[test]
fn jpeg_second_picture_with_xmp() {
    let first = jpeg(600, 400, false);
    let mut second = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut second, 90).encode_image(&noise(300, 200)).unwrap();
    let second = [&second[..2], &segment(0xE1, &[crate::meta_write::JPEG_XMP, XMP.as_bytes()]), &second[2..]].concat();
    // MPF: version, two pictures, their table; offsets from the MPF header (8 bytes into the segment).
    let mpf = |offset: u32| {
        let mut t = b"MM\0*\0\0\0\x08\0\x03".to_vec();
        t.extend(b"\xB0\x00\0\x07\0\0\0\x040100");
        t.extend(b"\xB0\x01\0\x04\0\0\0\x01\0\0\0\x02");
        t.extend(b"\xB0\x02\0\x07\0\0\0\x20\0\0\0\x32\0\0\0\0");
        t.extend([0u8; 16]);
        t.extend([&[0u8; 8][..], &offset.to_be_bytes(), &[0; 4]].concat());
        segment(0xE2, &[b"MPF\0", &t])
    };
    let len = 2 + mpf(0).len() + first.len() - 2;
    // The segment starts right after SOI: its header is at 2 + 8.
    let bytes = [&first[..2], &mpf((len - 10) as u32), &first[2..], &second].concat();
    same_from_pieces("mpf.jpg", &bytes);
    assert!(panel(&bytes, "mpf.jpg", None).contains("A test picture"));
}

/// A small file is read whole, in one read; a large JPEG only from its start, in one read.
#[test]
fn small_whole_large_from_its_start() {
    rt().block_on(async {
        let (small, large) = (jpeg(300, 200, true), jpeg(1600, 1200, true));
        assert!(large.len() as u64 > super::WHOLE_UNDER, "{}", large.len());
        let op = stored(&[("s.jpg", &small), ("l.jpg", &large)]).await;
        for (path, bytes, most) in [("s.jpg", &small, small.len() as u64), ("l.jpg", &large, super::HEAD)] {
            source::BYTES_READ.with(|n| n.set(0));
            source::READS.with(|n| n.set(0));
            let got = read(&op, path, bytes.len() as u64).await.unwrap();
            assert_eq!(panel(&got, path, None), panel(bytes, path, None));
            assert_eq!(source::READS.with(std::cell::Cell::get), 1, "{path}");
            assert_eq!(source::BYTES_READ.with(std::cell::Cell::get), most, "{path}");
        }
    });
}

/// A size from the listing that is no longer the file's: the same panel (read whole when a
/// read goes past the end).
#[test]
fn file_changed_same_panel() {
    rt().block_on(async {
        let png = png_small_chunks(1100, 1000);
        assert!(png.len() as u64 > super::WHOLE_UNDER, "{}", png.len());
        let jpeg = jpeg(1600, 1200, true);
        let op = stored(&[("a.jpg", &jpeg), ("a.png", &png)]).await;
        for (path, bytes) in [("a.jpg", &jpeg), ("a.png", &png)] {
            for size in [bytes.len() as u64 + 100, bytes.len() as u64 - 100] {
                assert_eq!(panel(&read(&op, path, size).await.unwrap(), path, None), panel(bytes, path, None), "{path} {size}");
            }
        }
        assert!(pieces(&op, "a.bmp", jpeg.len() as u64, PANEL).await.is_none(), "no plan");
    });
}

/// Every picture under `EYECANSEE_SAMPLES` (the private collection, `notes/test-files`) gives the
/// same panel read in pieces as read whole, its sidecar with it and without; the bytes and reads
/// of the panel's reading printed, those of the whole file beside.
/// `EYECANSEE_SAMPLES=… cargo test --no-default-features --features heif,magick same_as_whole_live -- --ignored --nocapture`
#[test]
#[ignore]
fn same_as_whole_live() {
    let root = std::env::var("EYECANSEE_SAMPLES").expect("EYECANSEE_SAMPLES");
    let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root("/")).unwrap());
    let mut files = Vec::new();
    let mut dirs = vec![std::path::PathBuf::from(root)];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if super::planner(&p.to_string_lossy()).is_some() {
                files.push(p);
            }
        }
    }
    files.sort();
    let (mut wrong, mut whole) = (Vec::new(), 0);
    // By extension: files, and those of which the plan with the small first read left bytes unread.
    let mut tried: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    println!("file\tMB\tpanel reads\tpanel MB\tpieces");
    rt().block_on(async {
        for p in files {
            let path = p.to_string_lossy().trim_start_matches('/').to_string();
            let bytes = std::fs::read(&p).unwrap();
            let sidecar = crate::sidecar::read(&op, &path).await;
            let mut shape = Vec::new();
            for how in [PANEL, STRESS] {
                match differences(&op, &path, &bytes, sidecar.as_ref(), how).await {
                    Some(d) => {
                        shape.push("pieces");
                        wrong.extend(d.into_iter().map(|d| format!("{path} (first read {}): {d}", how.head)));
                    }
                    None => shape.push("whole"),
                }
            }
            whole += usize::from(shape[0] == "whole");
            source::BYTES_READ.with(|n| n.set(0));
            let some = pieces(&op, &path, bytes.len() as u64, STRESS).await.is_some() && source::BYTES_READ.with(std::cell::Cell::get) < bytes.len() as u64;
            let ext = path.rsplit('.').next().unwrap_or_default().to_lowercase();
            let t = tried.entry(ext).or_default();
            *t = (t.0 + 1, t.1 + usize::from(some));
            source::BYTES_READ.with(|n| n.set(0));
            source::READS.with(|n| n.set(0));
            read(&op, &path, bytes.len() as u64).await.unwrap();
            let (reads, read_bytes) = (source::READS.with(std::cell::Cell::get), source::BYTES_READ.with(std::cell::Cell::get));
            let mb = |b: u64| format!("{:.2}", b as f64 / 1e6);
            println!("{}\t{}\t{reads}\t{}\t{}", path.rsplit('/').next().unwrap(), mb(bytes.len() as u64), mb(read_bytes), shape.join("/"));
        }
    });
    println!("read whole by the panel's plan: {whole}");
    for (ext, (files, some)) in tried {
        println!("{ext}: {files} files, {some} with bytes not read");
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
