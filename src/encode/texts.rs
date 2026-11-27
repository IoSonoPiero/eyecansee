//! The texts of a file outside EXIF, XMP and IPTC, kept in its copies (#218): the text chunks of
//! a PNG (title, author, description, copyright, comment, the parameters image generators
//! write…) with its colour chunks, the comments of a JPEG, the comment of a GIF. A PNG written
//! from a PNG gets its chunks as they were, a JPEG from a JPEG its comments; the other formats
//! get title, author, description, copyright and comment in their XMP (`xmp::fit_with`).

use std::io::Read;

use super::Format;

/// A text that has a place in XMP: dc:title, dc:creator, dc:description, dc:rights,
/// exif:UserComment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Title,
    Author,
    Description,
    Copyright,
    Comment,
}

/// What a file carries besides EXIF, XMP, IPTC and its colour profile.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Texts {
    /// The text chunks of a PNG (tEXt, zTXt, iTXt) as they were, but its XMP and the raw
    /// profiles of ImageMagick and exiftool (IPTC, EXIF: written from the other blocks, made to
    /// fit the copy): kind and data.
    pub png_text: Vec<([u8; 4], Vec<u8>)>,
    /// The colour chunks of a PNG (gAMA, cHRM, sRGB, cICP): kind and data.
    pub png_color: Vec<([u8; 4], Vec<u8>)>,
    /// The comments (COM segments) of a JPEG, as they were.
    pub jpeg_comments: Vec<Vec<u8>>,
    /// The same texts for the XMP of another format; authors in their order.
    pub fields: Vec<(Field, String)>,
}

/// The most a compressed PNG text is read to (a small file must not ask for much memory).
const INFLATED_MAX: u64 = 1 << 20;

impl Texts {
    pub fn is_empty(&self) -> bool {
        *self == Texts::default()
    }

    /// The fields for the XMP of a `format` file: none when it keeps them in their own place (a
    /// PNG's texts in a PNG, a JPEG's comments in a JPEG).
    pub fn for_xmp(&self, format: Format) -> &[(Field, String)] {
        let own = match format {
            Format::Png => !self.png_text.is_empty(),
            Format::Jpeg => !self.jpeg_comments.is_empty(),
            _ => false,
        };
        if own { &[] } else { &self.fields }
    }
}

/// The texts of a PNG, a JPEG or a GIF; none for the other files.
pub fn read(bytes: &[u8]) -> Texts {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        let jpeg_comments: Vec<Vec<u8>> = crate::ultrahdr::segments(bytes).into_iter().filter(|&(m, ..)| m == 0xFE).map(|(_, s, e)| bytes[s + 4..e].to_vec()).collect();
        let fields = jpeg_comments.iter().map(|c| text(c)).find(|c| !c.trim().is_empty()).map(|c| (Field::Comment, c)).into_iter().collect();
        Texts { jpeg_comments, fields, ..Texts::default() }
    } else if bytes.starts_with(b"GIF8") {
        let fields = gif_comment(bytes).map(|c| (Field::Comment, c)).into_iter().collect();
        Texts { fields, ..Texts::default() }
    } else {
        Texts::default()
    }
}

/// Every chunk of a PNG up to IEND: kind and data.
fn chunks(png: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut v = Vec::new();
    let mut i = 8;
    while let Some(head) = png.get(i..i + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let Some(data) = png.get(i + 8..(i + 8).saturating_add(len)) else { break };
        let kind = [head[4], head[5], head[6], head[7]];
        if &kind == b"IEND" {
            break;
        }
        v.push((kind, data));
        i += 12 + len;
    }
    v
}

fn png(bytes: &[u8]) -> Texts {
    let mut t = Texts::default();
    for (kind, data) in chunks(bytes) {
        match &kind {
            b"gAMA" | b"cHRM" | b"sRGB" | b"cICP" => t.png_color.push((kind, data.to_vec())),
            b"tEXt" | b"zTXt" | b"iTXt" => {
                let Some((key, value)) = png_text(&kind, data) else { continue };
                if key == "XML:com.adobe.xmp" || key.starts_with("Raw profile type") {
                    continue;
                }
                t.png_text.push((kind, data.to_vec()));
                let field = match key.to_ascii_lowercase().as_str() {
                    "title" => Field::Title,
                    "author" => Field::Author,
                    "description" => Field::Description,
                    "copyright" => Field::Copyright,
                    "comment" => Field::Comment,
                    _ => continue,
                };
                if !value.trim().is_empty() && (field == Field::Author || !t.fields.iter().any(|(f, _)| *f == field)) {
                    t.fields.push((field, value));
                }
            }
            _ => {}
        }
    }
    t
}

/// Keyword and text of a PNG text chunk: tEXt and zTXt in Latin-1, iTXt in UTF-8, compressed or
/// not. `None` when it cannot be read.
fn png_text(kind: &[u8; 4], data: &[u8]) -> Option<(String, String)> {
    let at = data.iter().position(|&c| c == 0)?;
    let key: String = data[..at].iter().map(|&c| c as char).collect();
    let rest = &data[at + 1..];
    let value = match kind {
        b"tEXt" => rest.iter().map(|&c| c as char).collect(),
        b"zTXt" => inflate(rest.get(1..)?)?.iter().map(|&c| c as char).collect(),
        _ => {
            // Compression flag and method, language and translated keyword, each ended by a zero.
            let (&flag, rest) = rest.split_first()?;
            let rest = rest.get(1..)?;
            let lang = rest.iter().position(|&c| c == 0)?;
            let rest = &rest[lang + 1..];
            let translated = rest.iter().position(|&c| c == 0)?;
            let raw = &rest[translated + 1..];
            String::from_utf8(if flag == 1 { inflate(raw)? } else { raw.to_vec() }).ok()?
        }
    };
    Some((key, value))
}

fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data).take(INFLATED_MAX).read_to_end(&mut out).ok()?;
    Some(out)
}

/// A comment's bytes as text: UTF-8 when they are, Latin-1 otherwise; without the zeros at its end.
fn text(b: &[u8]) -> String {
    let b = &b[..b.len() - b.iter().rev().take_while(|&&c| c == 0).count()];
    String::from_utf8(b.to_vec()).unwrap_or_else(|_| b.iter().map(|&c| c as char).collect())
}

/// The comments of a GIF (comment extensions), one per line; `None` without any.
fn gif_comment(b: &[u8]) -> Option<String> {
    // The sub-blocks from `i`: their data and where they end.
    let blocks = |mut i: usize| {
        let mut data = Vec::new();
        loop {
            let n = *b.get(i)? as usize;
            if n == 0 {
                return Some((data, i + 1));
            }
            data.extend_from_slice(b.get(i + 1..i + 1 + n)?);
            i += 1 + n;
        }
    };
    let table = |flags: u8| if flags & 0x80 != 0 { 3 << ((flags & 7) + 1) } else { 0 };
    let mut i = 13 + table(*b.get(10)?);
    let mut comments = Vec::new();
    loop {
        match *b.get(i)? {
            0x21 => {
                let (data, end) = blocks(i + 2)?;
                if b[i + 1] == 0xFE && !data.is_empty() {
                    comments.push(text(&data));
                }
                i = end;
            }
            0x2C => i = blocks(i + 10 + table(*b.get(i + 9)?) + 1)?.1,
            _ => break,
        }
    }
    (!comments.is_empty()).then(|| comments.join("\n"))
}

/// `png` with the text chunks `text` and the colour chunks `color` before its image data.
pub fn set_png(png: Vec<u8>, text: &[([u8; 4], Vec<u8>)], color: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    if text.is_empty() && color.is_empty() {
        return png;
    }
    let Some(at) = super::density::png_chunks(&png).into_iter().find(|(_, kind, _)| *kind == b"IDAT").map(|(at, ..)| at) else { return png };
    let mut extra = Vec::new();
    // The colour first: the PNG specification wants it before the palette and the image data.
    for (kind, data) in color.iter().chain(text) {
        crate::meta_write::chunk(&mut extra, kind, data);
    }
    let mut out = png;
    out.splice(at..at, extra);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The XMP and the raw profiles (exiftool's EXIF, ImageMagick's IPTC) are not copied as
    /// texts: they are written from the blocks made to fit the copy.
    #[test]
    fn png_profiles_not_copied() {
        let mut png = crate::encode::tests::png_with_texts();
        let at = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
        let mut extra = Vec::new();
        crate::meta_write::chunk(&mut extra, b"tEXt", b"Raw profile type exif\0\nexif\n4\n00000000\n");
        crate::meta_write::chunk(&mut extra, b"iTXt", b"XML:com.adobe.xmp\0\0\0\0\0<x:xmpmeta/>");
        png.splice(at..at, extra);
        let t = read(&png);
        assert_eq!(t.png_text.len(), 6);
        assert!(t.png_text.iter().all(|(_, d)| !d.starts_with(b"Raw profile") && !d.starts_with(b"XML:")));
        assert_eq!(t.png_color, [(*b"gAMA", 45455u32.to_be_bytes().to_vec())]);
        assert_eq!(t.fields.iter().map(|(f, _)| *f).collect::<Vec<_>>(), [Field::Title, Field::Description, Field::Author, Field::Copyright, Field::Comment]);
        assert_eq!(t.for_xmp(Format::Png), &[]);
        assert_eq!(t.for_xmp(Format::Jpeg).len(), 5);
    }

    /// A broken GIF or PNG gives what can be read, and nothing that is not there.
    #[test]
    fn broken_files() {
        assert!(read(b"GIF89a").is_empty());
        assert!(read(b"\x89PNG\r\n\x1a\n\0\0\0\x09tEXt").is_empty());
        assert_eq!(gif_comment(&[&b"GIF89a"[..], &[1, 0, 1, 0, 0, 0, 0], &[0x21, 0xFE, 2, b'h', b'i', 0, 0x3B]].concat()).as_deref(), Some("hi"));
        assert_eq!(gif_comment(&[&b"GIF89a"[..], &[1, 0, 1, 0, 0, 0, 0], &[0x21, 0xFE, 9, b'h']].concat()), None);
    }

    /// A PNG of a few KB with many compressed authors, each a MB once expanded, and many short
    /// ones (audit E.3, scaled down: 5000 of them asked for 5 GB): what is kept of them stays
    /// small, the chunks are still copied as they are.
    #[test]
    fn many_compressed_texts_bounded() {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut z, &vec![b'a'; INFLATED_MAX as usize]).unwrap();
        let packed = z.finish().unwrap();
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(1, 1)).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut extra = Vec::new();
        for _ in 0..32 {
            crate::meta_write::chunk(&mut extra, b"zTXt", &[&b"Author\0\0"[..], &packed].concat());
        }
        for i in 0..1000 {
            crate::meta_write::chunk(&mut extra, b"tEXt", format!("Author\0someone {i}").as_bytes());
        }
        let at = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
        png.splice(at..at, extra);
        let (t, most) = crate::test_alloc::peak(|| read(&png));
        let kept: usize = t.fields.iter().map(|(_, v)| v.len()).sum();
        assert!(kept <= 5 << 20 && most < 16 << 20, "{} MB of text kept, {} MB held, from a file of {} KB", kept >> 20, most >> 20, png.len() >> 10);
        assert!(t.fields.len() <= 100, "{} authors", t.fields.len());
        assert_eq!(t.png_text.len(), 1032, "every chunk copied");
    }

    /// Without the metadata, the texts and the comments go, the gamma stays (it is needed to
    /// show the picture right, as the colour profile); with a colour profile no colour chunk is
    /// written, the profile says it all.
    #[test]
    fn kept_as_chosen() {
        use crate::encode::{Options, Original};
        use crate::meta_strip::Keep;
        let png = crate::encode::tests::png_with_texts();
        let img = crate::decode::decode_full(&png, "x.png").unwrap();
        let original = Original::with_meta(img.clone(), &png);
        assert!(original.has_meta());
        let kinds = |o: &Options, original: &Original| crate::encode::tests::png_all_chunks(&original.encode(o).unwrap()).into_iter().map(|(k, _)| k).collect::<Vec<_>>();
        let none = kinds(&Options { format: Format::Png, meta: Keep::Nothing, ..Default::default() }, &original);
        assert!(none.contains(b"gAMA") && !none.iter().any(|k| matches!(k, b"tEXt" | b"zTXt" | b"iTXt")), "{none:?}");
        let no_place = kinds(&Options { format: Format::Png, meta: Keep::NoPlace, ..Default::default() }, &original);
        assert_eq!(no_place.iter().filter(|k| matches!(*k, b"tEXt" | b"zTXt" | b"iTXt")).count(), 6);
        // A real RGB profile: one that does not fit the pixels is left out (#214), and the gamma
        // would stay.
        let rgb = include_bytes!("../../assets/icc/sRGB-v2-micro.icc").to_vec();
        let with_icc = Original { icc: Some(rgb), ..Original::with_meta(img, &png) };
        assert!(!kinds(&Options { format: Format::Png, ..Default::default() }, &with_icc).contains(b"gAMA"));
    }
}
