//! AutoCAD's SHX fonts (shapes 1.0 and 1.1, unifont 1.0, bigfont 1.0 for East Asian two-byte
//! codes), the single-stroke letters of CAD drawings: read from the file and turned into
//! polylines, as the AutoCAD Customization Guide describes shape definitions (vectors, pen up and
//! down, scale, stack, subshapes, octant, fractional and bulge arcs).

use std::collections::HashMap;
use std::f64::consts::PI;

use crate::cad_geom::P;

/// Subshapes inside subshapes; deeper is a loop.
const MAX_DEPTH: u32 = 8;
/// Commands of one character drawn at most (a broken file could loop through subshapes).
const MAX_STEPS: usize = 20_000;

pub struct Shx {
    /// Height of the capitals, in the font's units.
    pub above: f64,
    kind: Kind,
    shapes: HashMap<u16, Vec<u8>>,
    /// Big font: the lead bytes of its two-byte codes.
    pub ranges: Vec<(u8, u8)>,
    /// An old DOS font (`TXT8`, `ROMANS8`): accented letters at their code page 437 numbers.
    pub dos: bool,
}

/// Code page 437 (DOS) from 0x80 to 0xFF.
const CP437: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

fn cp437(c: char) -> Option<u16> {
    CP437.chars().position(|x| x == c).map(|i| 0x80 + i as u16)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Shapes,
    Unifont,
    Bigfont,
}

/// The 16 directions of a vector byte: east, then every 22.5° (roughly) counterclockwise.
const DIRECTIONS: [P; 16] = [
    [1.0, 0.0],
    [1.0, 0.5],
    [1.0, 1.0],
    [0.5, 1.0],
    [0.0, 1.0],
    [-0.5, 1.0],
    [-1.0, 1.0],
    [-1.0, 0.5],
    [-1.0, 0.0],
    [-1.0, -0.5],
    [-1.0, -1.0],
    [-0.5, -1.0],
    [0.0, -1.0],
    [0.5, -1.0],
    [1.0, -1.0],
    [1.0, -0.5],
];

fn u16_le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

/// The definition without its name (up to the first zero).
fn body(def: &[u8]) -> Vec<u8> {
    def.iter().position(|&c| c == 0).map_or_else(Vec::new, |i| def[i + 1..].to_vec())
}

/// A font file, or `None` when it is not one this reader knows.
pub fn parse(bytes: &[u8]) -> Option<Shx> {
    let end = bytes.iter().take(40).position(|&c| c == 0x1A)?;
    let head = std::str::from_utf8(&bytes[..end]).ok()?.trim().to_ascii_lowercase();
    let mut shapes = HashMap::new();
    let mut ranges = Vec::new();
    let mut at = end + 1;
    let kind = if head.starts_with("autocad-86 unifont") {
        Kind::Unifont
    } else if head.starts_with("autocad-86 bigfont") {
        Kind::Bigfont
    } else if head.starts_with("autocad-86 shapes") {
        Kind::Shapes
    } else {
        return None;
    };
    if kind == Kind::Bigfont {
        // Header size, number of shapes, number of lead-byte ranges and the ranges; then an index
        // of code, length and where the definition is in the file.
        let (count, n) = (u16_le(bytes, at + 2)? as usize, u16_le(bytes, at + 4)? as usize);
        at += 6;
        ranges = (0..n).filter_map(|i| Some((u16_le(bytes, at + i * 4)? as u8, u16_le(bytes, at + i * 4 + 2)? as u8))).collect();
        at += n * 4;
        for i in 0..count {
            let e = at + i * 8;
            let (Some(code), Some(len)) = (u16_le(bytes, e), u16_le(bytes, e + 2)) else { break };
            let Some(offset) = bytes.get(e + 4..e + 8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else { break };
            if let (true, Some(def)) = (len > 0, bytes.get(offset..offset + len as usize)) {
                shapes.insert(code, body(def));
            }
        }
    } else if kind == Kind::Unifont {
        let count = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
        let len = u16_le(bytes, at + 4)? as usize;
        shapes.insert(0, body(bytes.get(at + 6..at + 6 + len)?));
        at += 6 + len;
        for _ in 1..count {
            let (Some(code), Some(len)) = (u16_le(bytes, at), u16_le(bytes, at + 2)) else { break };
            let Some(def) = bytes.get(at + 4..at + 4 + len as usize) else { break };
            shapes.insert(code, body(def));
            at += 4 + len as usize;
        }
    } else {
        let count = u16_le(bytes, at + 4)? as usize;
        at += 6;
        let index: Vec<(u16, usize)> = (0..count).filter_map(|i| Some((u16_le(bytes, at + i * 4)?, u16_le(bytes, at + i * 4 + 2)? as usize))).collect();
        at += count * 4;
        for (code, len) in index {
            let Some(def) = bytes.get(at..at + len) else { break };
            shapes.insert(code, body(def));
            at += len;
        }
    }
    // Shape 0: above, below, modes; extended big fonts start with a 0 and give the height next.
    let info = shapes.get(&0)?;
    let above = info.iter().take(2).copied().find(|&a| a > 0)? as f64;
    let extended = info.first() == Some(&0);
    // Accented letters where DOS has them (0x80-0x9F holds no letters in Windows code pages).
    let dos = kind == Kind::Shapes && (0x80..0xA0).filter(|c| shapes.contains_key(c)).count() >= 16;
    let mut font = Shx { above, kind, shapes, ranges, dos };
    // An extended big font (height 0 first) draws its characters at their own scale: their
    // height measured on some of them instead.
    if kind == Kind::Bigfont && extended {
        let mut codes: Vec<u16> = font.shapes.keys().copied().filter(|&c| c >= 0x8000).collect();
        codes.sort_unstable();
        let tall = codes.iter().step_by((codes.len() / 40).max(1)).map(|&c| font.glyph(c).0.iter().flatten().map(|p| p[1]).fold(f64::MIN, f64::max)).fold(0.0, f64::max);
        if tall > 0.0 {
            font.above = tall;
        }
    }
    Some(font)
}

/// A character drawn: its strokes and where the pen ended (the next character's start).
struct Pen {
    at: P,
    down: bool,
    scale: f64,
    /// Width over height of a big font's composite part (1 elsewhere).
    squash: f64,
    stack: Vec<P>,
    strokes: Vec<Vec<P>>,
    steps: usize,
}

impl Pen {
    fn to(&mut self, p: P) {
        if self.down {
            match self.strokes.last_mut() {
                Some(s) if s.last() == Some(&self.at) => s.push(p),
                _ => self.strokes.push(vec![self.at, p]),
            }
        }
        self.at = p;
    }

    fn by(&mut self, dx: f64, dy: f64) {
        let p = [self.at[0] + dx * self.scale * self.squash, self.at[1] + dy * self.scale];
        self.to(p);
    }

    /// An arc of radius `r` (scaled) from where the pen is, at angle `a0` on it, by `sweep`
    /// radians (negative = clockwise).
    fn arc(&mut self, r: f64, a0: f64, sweep: f64) {
        let rx = r * self.squash;
        let c = [self.at[0] - rx * a0.cos(), self.at[1] - r * a0.sin()];
        let n = ((sweep.abs() / (PI / 8.0)).ceil() as usize).max(1);
        for k in 1..=n {
            let a = a0 + sweep * k as f64 / n as f64;
            self.to([c[0] + rx * a.cos(), c[1] + r * a.sin()]);
        }
    }
}

impl Shx {
    /// The shape of a character by its Unicode number (big fonts: by its two-byte code), or for
    /// the degree, plus-minus and diameter signs where AutoCAD's own fonts keep them (127-129).
    /// (DOS fonts: by code page 437, the diameter as its φ.)
    pub fn code(&self, c: char) -> Option<u16> {
        if self.dos {
            let c = if c == '\u{2300}' { 'φ' } else { c };
            let code = if c.is_ascii() { Some(c as u16) } else { cp437(c) };
            return code.filter(|&k| k != 0 && self.shapes.contains_key(&k));
        }
        let alt: &[u16] = match c {
            '°' => &[127],
            '±' => &[128],
            '\u{2300}' => &[129, 0x2205, 0xD8],
            _ => &[],
        };
        u16::try_from(u32::from(c)).ok().into_iter().chain(alt.iter().copied()).find(|k| *k != 0 && self.shapes.contains_key(k))
    }

    pub fn has(&self, code: u16) -> bool {
        code != 0 && self.shapes.contains_key(&code)
    }

    /// The strokes of shape `code` from (0, 0) in font units, and where the next one starts.
    pub fn glyph(&self, code: u16) -> (Vec<Vec<P>>, f64) {
        let mut pen = Pen { at: [0.0; 2], down: true, scale: 1.0, squash: 1.0, stack: Vec::new(), strokes: Vec::new(), steps: 0 };
        match self.shapes.get(&code) {
            Some(def) => self.draw(def, &mut pen, 0),
            // Not in the font: a space as wide as two thirds of the height.
            None => pen.at[0] += self.above * 2.0 / 3.0,
        }
        (pen.strokes, pen.at[0])
    }

    /// The strokes of `text` from (0, 0) along +X in font units, and its length; a character
    /// the font does not have as its question mark.
    #[cfg(test)]
    pub fn strokes(&self, text: &str) -> (Vec<Vec<P>>, f64) {
        let mut out = Vec::new();
        let mut x = 0.0;
        for c in text.chars() {
            let (lines, advance) = self.glyph(self.code(c).unwrap_or(u16::from(b'?')));
            out.extend(lines.into_iter().map(|l| l.into_iter().map(|p| [p[0] + x, p[1]]).collect()));
            x += advance;
        }
        (out, x)
    }

    fn draw(&self, def: &[u8], pen: &mut Pen, depth: u32) {
        let byte = |i: usize| def.get(i).copied().unwrap_or(0);
        let signed = |i: usize| byte(i) as i8 as f64;
        let mut i = 0;
        while i < def.len() && pen.steps < MAX_STEPS {
            pen.steps += 1;
            let b = byte(i);
            if b > 0x0F {
                let d = DIRECTIONS[(b & 0x0F) as usize];
                let len = (b >> 4) as f64;
                pen.by(d[0] * len, d[1] * len);
                i += 1;
                continue;
            }
            match b {
                0 => return,
                1 => pen.down = true,
                2 => pen.down = false,
                3 if byte(i + 1) > 0 => pen.scale /= byte(i + 1) as f64,
                4 => pen.scale *= byte(i + 1) as f64,
                5 => pen.stack.push(pen.at),
                6 => {
                    // Back there without drawing.
                    if let Some(p) = pen.stack.pop() {
                        pen.at = p;
                    }
                }
                // A big font's composite: a shape squeezed into the box at (x, y), `w` wide and
                // `h` high, in the units of the character; the pen then back where it was.
                7 if self.kind == Kind::Bigfont && byte(i + 1) == 0 => {
                    let code = u16::from_be_bytes([byte(i + 2), byte(i + 3)]);
                    let (w, h) = (byte(i + 6) as f64, byte(i + 7) as f64);
                    if let (Some(sub), true, true) = (self.shapes.get(&code), depth < MAX_DEPTH, h > 0.0) {
                        let (down, scale, squash, start) = (pen.down, pen.scale, pen.squash, pen.at);
                        pen.down = false;
                        pen.by(byte(i + 4) as f64, byte(i + 5) as f64);
                        pen.down = true;
                        pen.scale *= h / self.above;
                        pen.squash *= w / h;
                        self.draw(sub, pen, depth + 1);
                        (pen.down, pen.scale, pen.squash, pen.at) = (down, scale, squash, start);
                    }
                }
                7 => {
                    let code = if self.kind == Kind::Unifont { u16::from_be_bytes([byte(i + 1), byte(i + 2)]) } else { u16::from(byte(i + 1)) };
                    // Drawn in place: what it does to the pen stays (big fonts lift it this way).
                    if let (Some(sub), true) = (self.shapes.get(&code), depth < MAX_DEPTH) {
                        self.draw(sub, pen, depth + 1);
                    }
                }
                8 => pen.by(signed(i + 1), signed(i + 2)),
                9 => {
                    let mut k = i + 1;
                    while k + 1 < def.len() && (byte(k), byte(k + 1)) != (0, 0) {
                        pen.by(signed(k), signed(k + 1));
                        k += 2;
                    }
                }
                10 => self.octant_arc(pen, byte(i + 1) as f64, byte(i + 2) as i8),
                11 => {
                    let (start, end) = (byte(i + 1) as f64 * 45.0 / 256.0, byte(i + 2) as f64 * 45.0 / 256.0);
                    let r = u16::from_be_bytes([byte(i + 3), byte(i + 4)]) as f64;
                    self.fractional_arc(pen, r, byte(i + 5) as i8, start, end);
                }
                12 => bulge(pen, signed(i + 1), signed(i + 2), signed(i + 3)),
                13 => {
                    let mut k = i + 1;
                    while k + 1 < def.len() && (byte(k), byte(k + 1)) != (0, 0) {
                        bulge(pen, signed(k), signed(k + 1), signed(k + 2));
                        k += 3;
                    }
                }
                // Only for vertical text: the next command is skipped.
                14 => {
                    i = self.next(def, i + 1);
                    continue;
                }
                _ => {}
            }
            i = self.next(def, i);
        }
    }

    /// Where the command at `i` ends.
    fn next(&self, def: &[u8], i: usize) -> usize {
        let byte = |k: usize| def.get(k).copied().unwrap_or(0);
        let pairs_until_zero = |step: usize| {
            let mut k = i + 1;
            while k + 1 < def.len() && (byte(k), byte(k + 1)) != (0, 0) {
                k += step;
            }
            k + 2
        };
        match byte(i) {
            b if b > 0x0F => i + 1,
            3 | 4 => i + 2,
            7 if self.kind == Kind::Bigfont && byte(i + 1) == 0 => i + 8,
            7 => i + if self.kind == Kind::Unifont { 3 } else { 2 },
            8 => i + 3,
            9 => pairs_until_zero(2),
            10 => i + 3,
            11 => i + 6,
            12 => i + 4,
            13 => pairs_until_zero(3),
            _ => i + 1,
        }
    }

    /// Radius, then the octant byte: sign = clockwise, bits 4-6 the first octant, bits 0-2 how
    /// many (0 = all eight).
    fn octant_arc(&self, pen: &mut Pen, r: f64, s: i8) {
        let r = r * pen.scale;
        let (first, n) = (((s as u8) >> 4 & 7) as f64, match s as u8 & 7 {
            0 => 8.0,
            n => n as f64,
        });
        let dir = if s < 0 { -1.0 } else { 1.0 };
        pen.arc(r, first * PI / 4.0, dir * n * PI / 4.0);
    }

    /// As an octant arc, starting and ending `start` and `end` degrees past octant boundaries.
    fn fractional_arc(&self, pen: &mut Pen, r: f64, s: i8, start: f64, end: f64) {
        let r = r * pen.scale;
        let first = ((s as u8) >> 4 & 7) as f64;
        let n = match s as u8 & 7 {
            0 => 8.0,
            n => n as f64,
        };
        let dir = if s < 0 { -1.0 } else { 1.0 };
        let a0 = (first * 45.0 + dir * start).to_radians();
        let last = first + dir * (n - 1.0);
        let a1 = (last * 45.0 + dir * end).to_radians();
        let mut sweep = a1 - a0;
        if dir > 0.0 && sweep <= 0.0 {
            sweep += 2.0 * PI;
        } else if dir < 0.0 && sweep >= 0.0 {
            sweep -= 2.0 * PI;
        }
        pen.arc(r, a0, sweep);
    }
}

/// To `dx`, `dy` along an arc whose height over the chord is `b`/127 of half the chord (DXF's
/// bulge, positive = counterclockwise); 0 is a straight line.
fn bulge(pen: &mut Pen, dx: f64, dy: f64, b: f64) {
    let (dx, dy) = (dx * pen.scale * pen.squash, dy * pen.scale);
    let q = [pen.at[0] + dx, pen.at[1] + dy];
    if b == 0.0 {
        pen.to(q);
        return;
    }
    let p = pen.at;
    for pt in crate::cad_geom::bulged(&[(p, b / 127.0), (q, 0.0)], false).into_iter().skip(1) {
        pen.to(pt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shapes 1.0 font with "A" as an upside-down V, "-" a bar drawn by a subshape, space.
    fn font() -> Vec<u8> {
        let mut out = b"AutoCAD-86 shapes 1.0\r\n\x1a".to_vec();
        let defs: [(u16, Vec<u8>); 4] = [
            (0, [b"test\0".to_vec(), vec![10, 2, 0, 0]].concat()),
            // Pen down to (5, 10) and (10, 0), then up to the next character's start at (12, 0).
            (u16::from(b'A'), [b"A\0".to_vec(), vec![1, 8, 5, 10, 8, 5, 246, 2, 8, 2, 0, 0]].concat()),
            (u16::from(b'-'), [b"minus\0".to_vec(), vec![2, 8, 0, 5, 1, 7, b'_', 2, 8, 2, 251u8, 0]].concat()),
            (u16::from(b'_'), [b"bar\0".to_vec(), vec![1, 0x80, 0]].concat()),
        ];
        out.extend(0u16.to_le_bytes());
        out.extend(255u16.to_le_bytes());
        out.extend((defs.len() as u16).to_le_bytes());
        for (code, d) in &defs {
            out.extend(code.to_le_bytes());
            out.extend((d.len() as u16).to_le_bytes());
        }
        for (_, d) in &defs {
            out.extend(d);
        }
        out
    }

    #[test]
    fn letters_become_strokes() {
        let mut bytes = font();
        let f = parse(&bytes).unwrap();
        assert_eq!(f.above, 10.0);
        let (strokes, len) = f.strokes("A");
        assert_eq!(strokes, vec![vec![[0.0, 0.0], [5.0, 10.0], [10.0, 0.0]]]);
        assert_eq!(len, 12.0);
        // The subshape draws the bar 8 long, from (0, 5) after a pen-up move.
        let (strokes, _) = f.strokes("-");
        assert_eq!(strokes, vec![vec![[0.0, 5.0], [8.0, 5.0]]]);
        // Unknown characters become nothing visible but take room.
        assert!(f.strokes("é").1 > 0.0);
        assert_eq!((cp437('é'), cp437('°'), cp437('\u{a0}'), cp437('A')), (Some(0x82), Some(0xF8), Some(0xFF), None));
        bytes.truncate(10);
        assert!(parse(&bytes).is_none());
    }

    #[test]
    fn octant_arcs_and_bulges() {
        let f = Shx { above: 1.0, kind: Kind::Shapes, shapes: HashMap::from([(65, vec![10, 4, 0x04, 0]), (66, vec![12, 10, 0, 127, 0])]), ranges: Vec::new(), dos: false };
        // Half a circle of radius 4 from octant 0 counterclockwise: ends at (-8, 0).
        let end = f.strokes("A").0[0].last().copied().unwrap();
        assert!((end[0] + 8.0).abs() < 1e-9 && end[1].abs() < 1e-9);
        // A half circle as a bulge of 127: its top 5 above the chord.
        let top = f.strokes("B").0[0].iter().map(|p| p[1].abs()).fold(0.0, f64::max);
        assert!((top - 5.0).abs() < 0.2, "{top}");
    }
}
