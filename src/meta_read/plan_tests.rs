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

/// A directory of fifty entries that each point at the same hundred thousand directories (audit
/// E.1, scaled down: a file of 2 MB made so asked for gigabytes): the plan does not hold a value
/// for each of them.
#[test]
fn tiff_long_pointer_lists_not_held() {
    const SHORT: u16 = 3;
    let n = 100_000u32;
    let entries: Vec<_> = (0..50).map(|_| (0x014A, SHORT, n, 2000)).collect();
    let bytes = file(2000 + 2 * n as usize + 100, &[(0, b"II*\0\x08\0\0\0"), (8, &ifd(&entries, 0))]);
    let mut s = Sparse::new(bytes.len(), 0);
    s.fill(0, &bytes);
    let (_, most) = crate::test_alloc::peak(|| planner("a.tif").unwrap()(&s));
    assert!(most < 16 << 20, "{} MB held for a file of {} KB", most >> 20, bytes.len() >> 10);
}

/// A HEIC whose `iloc` gives offsets and lengths in no bytes at all, each item with 65 535 extents
/// that take nothing to read (audit E.4, scaled down: 65 535 items kept the panel busy for
/// minutes): read whole, at once.
#[test]
fn heif_extents_of_no_bytes_whole() {
    let mut iloc = vec![0u8; 4];
    iloc.extend([0, 0]);
    iloc.extend(100u16.to_be_bytes());
    for id in 0..100u16 {
        iloc.extend(id.to_be_bytes());
        iloc.extend(0u16.to_be_bytes());
        iloc.extend(0xFFFFu16.to_be_bytes());
    }
    let meta = boxed(b"meta", &[&[0u8; 4][..], &boxed(b"iinf", &[0; 6]), &boxed(b"iloc", &iloc)].concat());
    let bytes = [boxed(b"ftyp", b"heic\0\0\0\0mif1heic"), meta].concat();
    let mut s = Sparse::new(bytes.len(), 0);
    s.fill(0, &bytes);
    let start = std::time::Instant::now();
    let plan = planner("a.heic").unwrap()(&s);
    assert_eq!(plan, Plan::Whole, "after {:?}", start.elapsed());
}

/// A TIFF whose two hundred main directories all point at the same table of twenty thousand
/// strips: the strips noted stop at a number no real file has, and it is read whole (audit E.4).
#[test]
fn tiff_strips_of_many_directories_bounded() {
    let n = 20_000u32;
    let (offsets, counts) = (10_000u32, 10_000 + 4 * n);
    let table_end = (counts + 4 * n) as usize;
    let strips: Vec<u8> = (0..n).flat_map(|i| (table_end as u32 + 64 * i).to_le_bytes()).collect();
    let ones: Vec<u8> = (0..n).flat_map(|_| 1u32.to_le_bytes()).collect();
    let dir = |next: u32| ifd(&[(0x0111, LONG, n, offsets), (0x0117, LONG, n, counts)], next);
    let size = ifd(&[], 0).len() + 24;
    let mut parts: Vec<(usize, Vec<u8>)> = (0..200).map(|k| (8 + k * size, dir(if k < 199 { (8 + (k + 1) * size) as u32 } else { 0 }))).collect();
    parts.extend([(offsets as usize, strips), (counts as usize, ones)]);
    let mut bytes = file(table_end + 64 * n as usize, &parts.iter().map(|(at, p)| (*at, p.as_slice())).collect::<Vec<_>>());
    bytes[..8].copy_from_slice(b"II*\0\x08\0\0\0");
    let mut s = Sparse::new(bytes.len(), 0);
    s.fill(0, &bytes);
    let (plan, most) = crate::test_alloc::peak(|| planner("a.tif").unwrap()(&s));
    assert!(most < 32 << 20, "{} MB held, {:?}", most >> 20, std::mem::discriminant(&plan));
}

/// Thousands of pixel parts without thousands of structure parts: at once, not one against each
/// (audit E.4: 256 directories of 1000 entries and a million strips took hours).
#[test]
fn subtract_many_from_many_at_once() {
    let from: Vec<Range<usize>> = (0..100_000).map(|i| i * 100..i * 100 + 50).collect();
    let out: Vec<Range<usize>> = (0..10_000).map(|i| i * 1000 + 40..i * 1000 + 60).collect();
    let start = std::time::Instant::now();
    let left = super::tiff::subtract(&from, &out);
    let took = start.elapsed();
    assert_eq!(left.len(), 100_000);
    assert_eq!((left[0].clone(), left[10].clone()), (0..40, 1000..1040));
    assert!(took < std::time::Duration::from_secs(1), "{took:?}");
}

/// A PNG whose end holds thousands of pixel chunk headers that all lead to the same large chunk,
/// whose checksum is wrong: each would check it again (audit E.4). At once.
#[test]
fn png_many_false_last_pixels_at_once() {
    let (headers, big) = (4000usize, 100_000usize);
    let head = [&b"\x89PNG\r\n\x1a\n"[..], &13u32.to_be_bytes(), b"IHDR", &[0; 17]].concat();
    // A first pixel chunk, then bytes not read; then the end of the file.
    let gap = 1 << 20;
    let first = [&(gap as u32).to_be_bytes()[..], b"IDAT"].concat();
    let tail_at = head.len() + 12 + gap + 4096;
    let big_at = tail_at + 8 * headers + 16;
    let mut tail = Vec::new();
    for k in 0..headers {
        tail.extend(((big_at - (tail_at + 8 * k) - 12) as u32).to_be_bytes());
        tail.extend(b"IDAT");
    }
    tail.extend([0; 16]);
    tail.extend((big as u32).to_be_bytes());
    tail.extend(b"tEXt");
    tail.extend(vec![b'x'; big + 4]);
    let iend = [&[0u8; 4][..], b"IEND", &0xAE42_6082u32.to_be_bytes()].concat();
    let start = [head, first].concat();
    let len = tail_at + tail.len() + iend.len();
    let mut s = Sparse::new(len, 0);
    s.fill(0, &start);
    s.fill(tail_at, &[tail, iend].concat());
    let begin = std::time::Instant::now();
    let plan = planner("a.png").unwrap()(&s);
    let took = begin.elapsed();
    assert!(took < std::time::Duration::from_secs(1), "{took:?}: {:?}", std::mem::discriminant(&plan));
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
