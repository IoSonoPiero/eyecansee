//! The texts of CAD drawings: where and how big (`Label`), from texts, multiline texts and
//! attributes, with AutoCAD's %% codes and the signs of its gdt font made Unicode; drawn with
//! their style's SHX font (`cad_shx`) when it is next to the drawing or in AutoCAD's folder.

use std::collections::HashMap;
use std::sync::Arc;

use acadrust::CadDocument;
use acadrust::entities::EntityType;

use crate::cad::{Anchor, Baseline, n3, xy};
use crate::cad_geom::{Affine, P};
use crate::cad_shx::Shx;

/// A text to place: position, height and angle in its own coordinates.
pub(crate) struct Label {
    pub(crate) at: P,
    pub(crate) height: f64,
    pub(crate) angle: f64,
    pub(crate) text: String,
    pub(crate) anchor: (Anchor, Baseline),
    /// The text style's name ("" for the standard one).
    pub(crate) style: String,
}

/// A text style's SHX font, its big font for East Asian characters with the drawing's code page
/// (how their characters are numbered), and the style's width factor and slant (radians).
pub(crate) struct Font {
    shx: Arc<Shx>,
    big: Option<Arc<Shx>>,
    encoding: &'static encoding_rs::Encoding,
    width: f64,
    oblique: f64,
}

/// The encoding of a drawing's code page (`ANSI_936`...); for a big font in a drawing that does
/// not say, guessed from the font's lead bytes (Shift-JIS starts below 0xA0 and again at 0xE0,
/// Big5 takes 0x80-0xFF in one range, otherwise GBK).
fn encoding(code_page: &str, big: Option<&Shx>) -> &'static encoding_rs::Encoding {
    let n: u32 = code_page.trim().to_ascii_uppercase().trim_start_matches("ANSI_").parse().unwrap_or(0);
    match n {
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        949 => encoding_rs::EUC_KR,
        950 => encoding_rs::BIG5,
        874 | 1250..=1258 => encoding_rs::Encoding::for_label(format!("windows-{n}").as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252),
        _ => match big.map(|b| b.ranges.as_slice()) {
            Some(r) if r.iter().any(|&(lo, hi)| lo < 0xA0 && hi < 0xA0) => encoding_rs::SHIFT_JIS,
            Some([(lo, hi)]) if *lo <= 0x81 && *hi >= 0xFE => encoding_rs::BIG5,
            Some(_) => encoding_rs::GBK,
            None => encoding_rs::WINDOWS_1252,
        },
    }
}

/// A character's number in a code page: one byte, or a lead and a trail byte.
fn native(c: char, encoding: &'static encoding_rs::Encoding) -> Option<u16> {
    let mut buf = [0u8; 4];
    let (bytes, _, errors) = encoding.encode(c.encode_utf8(&mut buf));
    match (errors, bytes.as_ref()) {
        (false, [b]) => Some(u16::from(*b)),
        (false, [lead, trail]) => Some(u16::from_be_bytes([*lead, *trail])),
        _ => None,
    }
}

/// The SHX fonts of the drawing's text styles, by style name in lower case; a font file read
/// once even when several styles use it.
pub(crate) fn fonts(doc: &CadDocument, sibling: crate::model::Sibling) -> HashMap<String, Font> {
    let mut files: HashMap<String, Option<Arc<Shx>>> = HashMap::new();
    let mut load = |file: &str| {
        let file = file.trim();
        if file.is_empty() {
            return None;
        }
        files.entry(file.to_ascii_lowercase()).or_insert_with(|| font_file(file, sibling).and_then(|b| crate::cad_shx::parse(&b)).map(Arc::new)).clone()
    };
    let mut out = HashMap::new();
    for st in doc.text_styles.iter() {
        if !st.true_type_font.trim().is_empty() {
            continue;
        }
        let Some(shx) = load(&st.font_file) else { continue };
        let big = load(&st.big_font_file);
        let width = if st.width_factor > 0.0 && st.width_factor.is_finite() { st.width_factor } else { 1.0 };
        let oblique = if st.oblique_angle.is_finite() { st.oblique_angle } else { 0.0 };
        let encoding = encoding(&doc.header.code_page, big.as_deref());
        out.insert(st.name.to_ascii_lowercase(), Font { shx, big, encoding, width, oblique });
    }
    out
}

/// An SHX file by the name a style gives (`.shx` added when missing): next to the drawing, in a
/// `fonts` folder there, or (on Windows) in an AutoCAD installation's `Fonts` folder.
fn font_file(file: &str, sibling: crate::model::Sibling) -> Option<Vec<u8>> {
    let name = if file.contains('.') { file.to_string() } else { format!("{file}.shx") };
    let base = name.rsplit(['/', '\\']).next()?.to_string();
    if !base.to_ascii_lowercase().ends_with(".shx") {
        return None;
    }
    let lower = base.to_ascii_lowercase();
    let near = [name.clone(), base.clone(), lower.clone(), format!("fonts/{base}"), format!("Fonts/{base}"), format!("fonts/{lower}")];
    near.iter().find_map(|n| sibling(n)).or_else(|| autocad_fonts().into_iter().find_map(|d| std::fs::read(d.join(&base)).ok()))
}

/// The `Fonts` folders of AutoCAD and its relatives under Program Files.
fn autocad_fonts() -> Vec<std::path::PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let roots = ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(|r| std::path::PathBuf::from(r).join("Autodesk"));
    roots.filter_map(|r| std::fs::read_dir(r).ok()).flatten().flatten().map(|e| e.path().join("Fonts")).filter(|p| p.is_dir()).collect()
}

/// The strokes of one line in the font's units, and its length: each character from the font
/// (by Unicode, else by its code-page byte), else from the big font at the same height, else a
/// question mark.
fn line_strokes(font: &Font, line: &str) -> (Vec<Vec<P>>, f64) {
    let mut out = Vec::new();
    let mut x = 0.0;
    for c in line.chars() {
        // A DOS font numbers its letters by code page 437 (`Shx::code`), not by the drawing's.
        let byte = native(c, font.encoding).filter(|_| !font.shx.dos);
        let (shx, code) = match (font.shx.code(c), byte.filter(|&b| font.shx.has(b)), &font.big) {
            (Some(code), _, _) | (None, Some(code), _) => (&font.shx, code),
            (None, None, Some(big)) if byte.is_some_and(|b| big.has(b)) => (big, byte.unwrap_or(0)),
            _ => (&font.shx, font.shx.code('?').unwrap_or(0)),
        };
        let k = font.shx.above / shx.above;
        let (lines, advance) = shx.glyph(code);
        out.extend(lines.into_iter().map(|l| l.into_iter().map(|p| [x + p[0] * k, p[1] * k]).collect::<Vec<P>>()));
        x += advance * k;
    }
    (out, x)
}

/// The strokes of a text in `font`: at `at` (drawing units), capitals `height` high, turned by
/// `angle`, lines 5/3 of the height apart as AutoCAD sets them.
pub(crate) fn strokes(font: &Font, at: P, height: f64, angle: f64, text: &str, anchor: (Anchor, Baseline)) -> Vec<Vec<P>> {
    let k = height / font.shx.above;
    let slant = font.oblique.tan();
    let (s, c) = angle.sin_cos();
    let y0 = match anchor.1 {
        Baseline::Alphabetic => 0.0,
        Baseline::Middle => -height / 2.0,
        Baseline::Hanging => -height,
    };
    let mut out = Vec::new();
    for (row, line) in text.lines().enumerate() {
        let (lines, len) = line_strokes(font, line);
        let len = len * k * font.width;
        let x0 = match anchor.0 {
            Anchor::Start => 0.0,
            Anchor::Middle => -len / 2.0,
            Anchor::End => -len,
        };
        let y = y0 - row as f64 * height * 5.0 / 3.0;
        for l in lines {
            out.push(l.iter().map(|p| {
                let (x, yy) = (x0 + (p[0] * font.width + p[1] * slant) * k, y + p[1] * k);
                [at[0] + x * c - yy * s, at[1] + x * s + yy * c]
            }).collect());
        }
    }
    out
}


/// AutoCAD's codes in texts: %%c diameter, %%d degree, %%p plus-minus, %%nnn a character by its
/// number (127-129 as the signs of AutoCAD's fonts), %%% a percent sign; %%o and %%u (over- and underline on and off) dropped.
pub(crate) fn percent_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("%%") {
        out.push_str(&rest[..i]);
        rest = &rest[i + 2..];
        let digits = rest.bytes().take(3).take_while(u8::is_ascii_digit).count();
        if digits == 3 {
            // In AutoCAD's SHX fonts 127, 128 and 129 are the degree, plus-minus and diameter signs.
            out.extend(rest[..3].parse::<u32>().ok().and_then(|n| match n {
                127 => Some('°'),
                128 => Some('±'),
                129 => Some('\u{2300}'),
                n => char::from_u32(n),
            }));
            rest = &rest[3..];
            continue;
        }
        let Some(c) = rest.chars().next() else { break };
        match c.to_ascii_lowercase() {
            'c' => out.push('\u{2300}'),
            'd' => out.push('°'),
            'p' => out.push('±'),
            '%' => out.push('%'),
            'o' | 'u' => {}
            _ => {
                out.push_str("%%");
                out.push(c);
            }
        }
        rest = &rest[c.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// A row of a feature control frame as text: cells apart, the letters of the gdt font as the
/// symbols they draw.
pub(crate) fn gdt(s: &str) -> String {
    let symbol = |c: char| match c {
        'a' => '∠', 'b' => '⊥', 'c' => '⏥', 'd' => '⌓', 'e' => '○', 'f' => '∥', 'g' => '⌭', 'h' => '↗', 'i' => '⌯', 'j' => '⌖',
        'k' => '⌒', 'l' => 'Ⓛ', 'm' => 'Ⓜ', 'n' => '⌀', 'p' => 'Ⓟ', 'r' => '◎', 's' => 'Ⓢ', 't' => '⌰', 'u' => '⏤', c => c,
    };
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("{\\Fgdt;") {
        out.push_str(&rest[..i]);
        rest = &rest[i + 7..];
        let end = rest.find('}').unwrap_or(rest.len());
        out.extend(rest[..end].chars().map(symbol));
        rest = rest.get(end + 1..).unwrap_or("");
    }
    out.push_str(rest);
    percent_codes(&out.replace("%%v", " | ").replace("%%V", " | ")).trim_matches([' ', '|']).to_string()
}

/// Texts (TEXT, MTEXT) with their coordinate system.
pub(crate) fn label(e: &EntityType) -> Option<(Affine, Label)> {
    match e {
        EntityType::Text(x) => {
            use acadrust::entities::{TextHorizontalAlignment as H, TextVerticalAlignment as V};
            let h = match x.horizontal_alignment {
                H::Left | H::Aligned | H::Fit => Anchor::Start,
                H::Center | H::Middle => Anchor::Middle,
                H::Right => Anchor::End,
            };
            let v = match x.vertical_alignment {
                _ if x.horizontal_alignment == H::Middle => Baseline::Middle,
                V::Baseline | V::Bottom => Baseline::Alphabetic,
                V::Middle => Baseline::Middle,
                V::Top => Baseline::Hanging,
            };
            // Aligned text sits on its alignment point, left text on its insertion point.
            let at = if h == Anchor::Start && v == Baseline::Alphabetic { &x.insertion_point } else { x.alignment_point.as_ref().unwrap_or(&x.insertion_point) };
            let l = Label { at: xy(at), height: x.height, angle: x.rotation, text: percent_codes(&x.value), anchor: (h, v), style: x.style.clone() };
            Some((Affine::ocs(n3(&x.normal)), l))
        }
        EntityType::MText(m) => {
            let text = acadrust::entities::mtext_format::parse_mtext(&m.value, true).to_plain_text();
            let angle = m.dwg_x_direction.as_ref().map_or(m.rotation, |d| d.y.atan2(d.x));
            // 1-9: top, middle, bottom rows of left, centre, right.
            let a = m.attachment_point as i32 - 1;
            let h = [Anchor::Start, Anchor::Middle, Anchor::End][(a % 3).clamp(0, 2) as usize];
            let v = [Baseline::Hanging, Baseline::Middle, Baseline::Alphabetic][(a / 3).clamp(0, 2) as usize];
            Some((Affine::IDENTITY, Label { at: xy(&m.insertion_point), height: m.height, angle, text, anchor: (h, v), style: m.style.clone() }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cad_svg::text_box;

    #[test]
    fn feature_control_frames() {
        assert_eq!(gdt("{\\Fgdt;j}%%v{\\Fgdt;n}0.1{\\Fgdt;m}%%vA%%vB"), "⌖ | ⌀0.1Ⓜ | A | B");
    }

    #[test]
    fn percent_codes_and_text_boxes() {
        assert_eq!(percent_codes("%%c20 90%%d %%p0.1 %%1292 90%%127 %%065 50%%% %%uA%%u"), "\u{2300}20 90° ±0.1 \u{2300}2 90° A 50% A");
        // Centred, turned by 90°: the box goes up and down from the point, to the left of it.
        let b = text_box([0.0, 0.0], 1.0, std::f64::consts::FRAC_PI_2, "ABCD", (Anchor::Middle, Baseline::Alphabetic));
        assert!(b.iter().any(|p| p[1] < -1.0) && b.iter().any(|p| p[1] > 1.0) && b.iter().any(|p| p[0] < -1.0));
    }
}
#[cfg(test)]
mod shx_live {
    use super::*;

    /// Texts drawn with the SHX fonts in `EYECANSEE_SHX` (folder) into `EYECANSEE_SHX_OUT` (PNG).
    #[test]
    #[ignore]
    fn shx_live() {
        let (Ok(dir), Ok(out)) = (std::env::var("EYECANSEE_SHX"), std::env::var("EYECANSEE_SHX_OUT")) else { return };
        let load = |n: &str| crate::cad_shx::parse(&std::fs::read(format!("{dir}/{n}")).ok()?).map(Arc::new);
        let rows = [("txt.shx", None, "ABC abc 0123 °±⌀ àèé €", encoding_rs::WINDOWS_1252), ("TXT8.SHX", None, "ABC abc 0123 àèéüß ° ± ⌀ ½", encoding_rs::WINDOWS_1252), ("ROMANS8.SHX", None, "Perché così? Größe 20°±0,1 ⌀5", encoding_rs::WINDOWS_1252), ("tssdeng.shx", Some("hztxt.shx"), "长度 66.04 中文测试 ABC", encoding_rs::GBK), ("simplex.shx", Some("hztxt.shx"), "长度 中文测试 ABC", encoding_rs::GBK), ("simplex.shx", Some("gbcbig.shx"), "长度 中文测试 ABC", encoding_rs::GBK), ("tssdeng.shx", None, "长度 中文测试 ABC", encoding_rs::GBK), ("simplex.shx", Some("chineset.shx"), "長度 中文測試 ABC", encoding_rs::BIG5), ("simplex.shx", Some("extfont.shx"), "日本語テスト ABC", encoding_rs::SHIFT_JIS), ("romans.shx", Some("GHS.SHX"), "한국어 시험 ABC", encoding_rs::EUC_KR)];
        let mut d = crate::cad::Drawing::default();
        for (i, (main, big, text, encoding)) in rows.iter().enumerate() {
            let Some(shx) = load(main) else { continue };
            let font = Font { shx, big: big.and_then(load), encoding, width: 1.0, oblique: 0.0 };
            for pts in strokes(&font, [0.0, -(i as f64) * 20.0], 10.0, 0.0, text, (Anchor::Start, Baseline::Alphabetic)) {
                d.items.push(crate::cad::Item::Line { pts, color: [255; 3], fill: None, stroke: crate::cad::Stroke { dash: Vec::new(), weight: crate::cad::THIN } });
            }
        }
        crate::decode::cad_render(&d, 1600, None).unwrap().save(format!("{out}/shx.png")).unwrap();
    }
}
