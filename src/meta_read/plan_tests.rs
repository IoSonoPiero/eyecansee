//! The plans of the formats no encoder here writes (RAW files, a JPEG XL with its boxes), on
//! files made by hand: what they skip. The decoders are tried on real files by
//! `tests::same_as_whole_live`.

use std::ops::Range;

use super::{FIRST, Plan, Sparse, merged, planner};

/// The plan of `bytes` named `name`, all of them read: what it skips, joined.
fn skipped(name: &str, bytes: &[u8]) -> Vec<Range<usize>> {
    let mut s = Sparse::new(bytes.len(), 0);
    s.fill(0, bytes);
    match planner(name).unwrap()(&s) {
        Plan::Skip(skip) => merged(skip.into_iter().filter(|r| r.start < r.end).collect(), 0),
        other => panic!("{name}: {other:?}"),
    }
}

/// A little-endian TIFF directory: its entries (tag, type, count, value or offset), the next one.
fn ifd(entries: &[(u16, u16, u32, u32)], next: u32) -> Vec<u8> {
    let mut out = (entries.len() as u16).to_le_bytes().to_vec();
    for &(tag, kind, count, value) in entries {
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(value.to_le_bytes());
    }
    out.extend(next.to_le_bytes());
    out
}

/// `len` bytes with `parts` put at their places.
fn file(len: usize, parts: &[(usize, &[u8])]) -> Vec<u8> {
    let mut out = vec![0x55; len];
    for (at, p) in parts {
        out[*at..at + p.len()].copy_from_slice(p);
    }
    out
}

/// A JPEG's start: SOI, an APP1 of `app` bytes, the header of a scan; then coded data.
fn jpeg_start(app: usize) -> Vec<u8> {
    let app = super::tests::segment(0xE1, &[&vec![b'x'; app]]);
    [&[0xFF, 0xD8][..], &app, &super::tests::segment(0xDA, &[&[0; 10]])].concat()
}

const LONG: u16 = 4;
const UNDEFINED: u16 = 7;

/// A Panasonic RW2: its sensor data from 0x0118 to the end, its preview JPEG in tag 0x002E.
#[test]
fn rw2_sensor_to_the_end_and_its_preview() {
    let jpeg = jpeg_start(100);
    let dir = ifd(&[(0x002E, UNDEFINED, 5000, 200), (0x0118, LONG, 1, 10_000)], 0);
    let bytes = file(40_000, &[(0, b"IIU\0\x08\0\0\0"), (8, &dir), (200, &jpeg)]);
    assert_eq!(skipped("a.rw2", &bytes), [200 + jpeg.len()..5200, 10_000 + FIRST..40_000]);
}

/// Tiles one byte apart (padding to an even place) are one part; a directory of thousands of
/// entries is not one; the "next" link of the EXIF directory is not followed.
#[test]
fn tiff_padding_big_directories_and_next_links() {
    let offsets: Vec<u8> = [1000u32, 3001].iter().flat_map(|o| o.to_le_bytes()).collect();
    let counts: Vec<u8> = [2000u32, 2000].iter().flat_map(|o| o.to_le_bytes()).collect();
    let main = ifd(&[(0x0144, LONG, 2, 200), (0x0145, LONG, 2, 208), (0x8769, LONG, 1, 300), (0xA005, LONG, 1, 400)], 0);
    // The EXIF directory points at another with strips, which is not a picture of the file.
    let exif = ifd(&[(0x9000, UNDEFINED, 4, 0)], 500);
    let other = ifd(&[(0x0111, LONG, 1, 6000), (0x0117, LONG, 1, 1000)], 0);
    let bytes = file(8000, &[(0, b"II*\0\x08\0\0\0"), (8, &main), (200, &offsets), (208, &counts), (300, &exif), (400, &[0x88, 0x13]), (500, &other)]);
    assert_eq!(skipped("a.tif", &bytes), [1000..5001]);
}

/// A RAF: the JPEG preview after its segments; in Fujifilm's TIFF, the sensor data but its start.
#[test]
fn raf_preview_and_sensor_data() {
    let jpeg = jpeg_start(300);
    let mut head = b"FUJIFILMCCD-RAW 0201FF383501".to_vec();
    head.resize(84, 0);
    for v in [160u32, 2000, 2200, 100, 4000, 0] {
        head.extend(v.to_be_bytes());
    }
    let fuji = ifd(&[(0xF007, LONG, 1, 1000), (0xF008, LONG, 1, 20_000)], 0);
    let main = ifd(&[(0xF000, LONG, 1, 100)], 0);
    let bytes = file(30_000, &[(0, &head), (160, &jpeg), (4000, b"II*\0\x08\0\0\0"), (4008, &main), (4100, &fuji)]);
    assert_eq!(skipped("a.raf", &bytes), [160 + jpeg.len()..2160, 5000 + FIRST..25_000]);
}

/// An MRW: its blocks, then the pixels but their start.
#[test]
fn mrw_blocks_then_pixels() {
    let bytes = file(50_000, &[(0, b"\0MRM\0\0\x03\xE8")]);
    assert_eq!(skipped("a.mrw", &bytes), [1008 + FIRST..50_000]);
}

/// An ISO box of `kind` around `content`.
fn boxed(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
    [&((content.len() + 8) as u32).to_be_bytes()[..], kind, content].concat()
}

/// A CR3 track whose sample entry is `entry`, one sample of `size` bytes at `at`.
fn track(entry: &[u8; 4], at: u64, size: u32) -> Vec<u8> {
    let stsd = boxed(b"stsd", &[&[0u8; 8][..], &boxed(entry, &[0; 8])].concat());
    let stsz = boxed(b"stsz", &[&[0u8; 4][..], &size.to_be_bytes(), &1u32.to_be_bytes()].concat());
    let co64 = boxed(b"co64", &[&[0u8; 4][..], &1u32.to_be_bytes(), &at.to_be_bytes()].concat());
    let stbl = boxed(b"stbl", &[stsd, stsz, co64].concat());
    boxed(b"trak", &boxed(b"mdia", &boxed(b"minf", &stbl)))
}

/// A CR3: the samples of its picture tracks but their start (a JPEG's segments); the timed
/// metadata track read.
#[test]
fn cr3_picture_samples() {
    let jpeg = jpeg_start(200);
    let ftyp = boxed(b"ftyp", b"crx \0\0\0\x01crx isom");
    let moov = boxed(b"moov", &[track(b"CRAW", 1000, 9000), track(b"CRAW", 10_000, 30_000), track(b"CTMD", 40_000, 500)].concat());
    let head = [ftyp, moov].concat();
    assert!(head.len() + 8 <= 1000, "{}", head.len());
    let mdat = [&(60_000 - head.len() as u32).to_be_bytes()[..], b"mdat"].concat();
    let bytes = file(60_000, &[(0, &head), (head.len(), &mdat), (1000, &jpeg), (10_000, &[1, 2])]);
    assert_eq!(skipped("a.cr3", &bytes), [1000 + jpeg.len()..10_000, 10_000 + FIRST..40_000]);
}

/// A JPEG XL whose codestream is in two boxes around its metadata, as libjxl writes it: the
/// start of the codestream across them, every other box.
#[test]
fn jxl_codestream_start_across_boxes() {
    let start = super::bmff::CODESTREAM_START;
    let first = boxed(b"jxlp", &[0u8; 4 + 100]);
    let exif = boxed(b"Exif", &[0u8; 50]);
    let second = boxed(b"jxlp", &vec![0u8; 4 + 200_000]);
    let head = [b"\0\0\0\x0CJXL \r\n\x87\n".to_vec(), boxed(b"ftyp", b"jxl \0\0\0\0jxl "), first, exif].concat();
    let bytes = [head.clone(), second].concat();
    // The second part's data starts after its header and index.
    let data = head.len() + 12;
    assert_eq!(skipped("a.jxl", &bytes), [data + start - 100..bytes.len()]);
}

/// A PNG with its pixels not ending where the file's end says, or chunks after IEND: read whole.
#[test]
fn png_with_bytes_after_its_end_whole() {
    let mut s = Sparse::new(40, 0);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend([0, 0, 0, 0]);
    png.extend(b"IEND\xAE\x42\x60\x82");
    png.resize(40, 7);
    s.fill(0, &png);
    assert_eq!(planner("a.png").unwrap()(&s), Plan::Whole);
}

/// A JPEG with bytes between its segments, which the EXIF reader passes over: read whole.
#[test]
fn jpeg_with_bytes_between_segments_whole() {
    let mut bytes = jpeg_start(10);
    bytes.insert(2 + 14, 0);
    let mut s = Sparse::new(bytes.len() + 100, 0);
    s.fill(0, &bytes);
    assert_eq!(planner("a.jpg").unwrap()(&s), Plan::Whole);
}
