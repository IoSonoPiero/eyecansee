//! The texts of CAD drawings: where and how big (`Label`), from texts, multiline texts and
//! attributes, with AutoCAD's %% codes and the signs of its gdt font made Unicode.

use acadrust::entities::EntityType;

use crate::cad::{Anchor, Baseline, n3, xy};
use crate::cad_geom::{Affine, P};

/// A text to place: position, height and angle in its own coordinates.
pub(crate) struct Label {
    pub(crate) at: P,
    pub(crate) height: f64,
    pub(crate) angle: f64,
    pub(crate) text: String,
    pub(crate) anchor: (Anchor, Baseline),
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
            let l = Label { at: xy(at), height: x.height, angle: x.rotation, text: percent_codes(&x.value), anchor: (h, v) };
            Some((Affine::ocs(n3(&x.normal)), l))
        }
        EntityType::MText(m) => {
            let text = acadrust::entities::mtext_format::parse_mtext(&m.value, true).to_plain_text();
            let angle = m.dwg_x_direction.as_ref().map_or(m.rotation, |d| d.y.atan2(d.x));
            // 1-9: top, middle, bottom rows of left, centre, right.
            let a = m.attachment_point as i32 - 1;
            let h = [Anchor::Start, Anchor::Middle, Anchor::End][(a % 3).clamp(0, 2) as usize];
            let v = [Baseline::Hanging, Baseline::Middle, Baseline::Alphabetic][(a / 3).clamp(0, 2) as usize];
            Some((Affine::IDENTITY, Label { at: xy(&m.insertion_point), height: m.height, angle, text, anchor: (h, v) }))
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
