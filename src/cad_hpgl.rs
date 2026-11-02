//! HP-GL and HP-GL/2 plot files (`.plt`, `.hpgl`): pen moves, arcs, circles, rectangles, labels
//! and encoded polylines, also inside a PCL job, as a `cad::Drawing` in plotter units (40 a
//! millimetre), with user units (IP, SC), line types (LT), pen widths (PW) and the pens in the
//! colours of plotters on a dark background.
// ponytail: P1 and P2 of an unknown plotter taken as an A4 sheet; adaptive line types as fixed ones

use crate::cad::{Anchor, Baseline, Drawing, Item, MAX_POINTS, Stroke};
use crate::cad_geom::{self, P};

/// Pens 1-8 of a plotter; pen 1 (black on paper) white, as in CAD programs.
const PENS: [[u8; 3]; 8] = [[255, 255, 255], [255, 0, 0], [0, 255, 0], [255, 255, 0], [0, 0, 255], [255, 0, 255], [0, 255, 255], [255, 128, 0]];
/// Default label size: 0.375 cm (in plotter units).
const LABEL: f64 = 150.0;
/// End of a label (ETX).
const ETX: u8 = 3;
/// P1 and P2 until IP sets them: an A4 sheet of a typical plotter, in plotter units.
const SHEET: [P; 2] = [[0.0, 0.0], [10_900.0, 7_650.0]];
/// The line types 1-8 of HP-GL/2, in percent of the pattern (dash positive, gap negative, dot 0).
const LINE_TYPES: [&[f64]; 8] = [
    &[0.0, -100.0],
    &[50.0, -50.0],
    &[70.0, -30.0],
    &[80.0, -10.0, 0.0, -10.0],
    &[70.0, -10.0, 10.0, -10.0],
    &[50.0, -10.0, 10.0, -10.0, 10.0, -10.0],
    &[70.0, -10.0, 0.0, -10.0, 0.0, -10.0],
    &[50.0, -10.0, 0.0, -10.0, 10.0, -10.0, 0.0, -10.0],
];

struct Plotter {
    out: Drawing,
    at: P,
    down: bool,
    relative: bool,
    pen: usize,
    colors: Vec<[u8; 3]>,
    path: Vec<P>,
    label_height: f64,
    label_angle: f64,
    /// P1 and P2, and user units to plotter units (`x' = k·x + o`) after SC.
    corners: [P; 2],
    k: P,
    o: P,
    dash: Vec<f64>,
    /// Width of each pen in millimetres; 0 = the thinnest.
    widths: Vec<f32>,
}

pub fn read(bytes: &[u8]) -> Result<Drawing, String> {
    let mut p = Plotter {
        out: Drawing::default(),
        at: [0.0; 2],
        down: false,
        relative: false,
        pen: 1,
        colors: PENS.to_vec(),
        path: Vec::new(),
        label_height: LABEL,
        label_angle: 0.0,
        corners: SHEET,
        k: [1.0; 2],
        o: [0.0; 2],
        dash: Vec::new(),
        widths: Vec::new(),
    };
    let mut i = 0;
    let mut commands = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == 0x1b {
            // A PCL or device command: up to its final capital letter.
            i += bytes[i + 1..].iter().position(|b| b.is_ascii_uppercase()).map_or(bytes.len(), |k| k + 2);
            continue;
        }
        if !c.is_ascii_alphabetic() || !bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        let name = [c.to_ascii_uppercase(), bytes[i + 1].to_ascii_uppercase()];
        i += 2;
        if &name == b"LB" {
            let end = bytes[i..].iter().position(|&b| b == ETX).map_or(bytes.len(), |k| i + k);
            p.label(&String::from_utf8_lossy(&bytes[i..end]));
            i = end + 1;
            continue;
        }
        let end = bytes[i..].iter().position(|&b| b == b';' || b.is_ascii_alphabetic() && &name != b"PE").map_or(bytes.len(), |k| i + k);
        if &name == b"PE" {
            p.encoded(&bytes[i..end])?;
        } else {
            let args: Vec<f64> = String::from_utf8_lossy(&bytes[i..end]).split(|c: char| c == ',' || c.is_whitespace()).filter_map(|s| s.parse().ok()).collect();
            p.command(name, &args)?;
        }
        commands += 1;
        i = end;
    }
    p.flush()?;
    if commands == 0 || p.out.items.is_empty() {
        return Err("not an HP-GL plot file, or nothing drawn".into());
    }
    Ok(p.out)
}

impl Plotter {
    fn color(&self) -> [u8; 3] {
        self.colors.get(self.pen.wrapping_sub(1)).copied().unwrap_or(PENS[0])
    }

    fn flush(&mut self) -> Result<(), String> {
        let pts = std::mem::take(&mut self.path);
        self.add(pts, None)
    }

    fn add(&mut self, pts: Vec<P>, fill: Option<f32>) -> Result<(), String> {
        self.out.points += pts.len();
        if self.out.points > MAX_POINTS {
            return Err(format!("more than {MAX_POINTS} points"));
        }
        if pts.len() >= 2 && self.pen > 0 {
            let weight = self.widths.get(self.pen.wrapping_sub(1)).copied().unwrap_or(0.0);
            self.out.items.push(Item::Line { pts, color: self.color(), fill, stroke: Stroke { dash: self.dash.clone(), weight } });
        }
        Ok(())
    }

    /// A move to `to`, drawing if the pen is down.
    fn go(&mut self, to: P) -> Result<(), String> {
        if self.down {
            if self.path.is_empty() {
                self.path.push(self.at);
            }
            self.path.push(to);
        } else {
            self.flush()?;
        }
        self.at = to;
        Ok(())
    }

    /// A point in user units to plotter units, and a distance (relative moves, radii).
    fn user(&self, p: &[f64]) -> P {
        [self.k[0] * p[0] + self.o[0], self.k[1] * p[1] + self.o[1]]
    }

    fn by(&self, d: &[f64]) -> P {
        [self.at[0] + self.k[0] * d[0], self.at[1] + self.k[1] * d[1]]
    }

    /// SC: user units between xmin, xmax and ymin, ymax fill P1 to P2 (type 0), keeping their
    /// proportions (type 1), or have a size of their own (type 2: xmin, factor, ymin, factor).
    fn scaling(&mut self, a: &[f64]) {
        let [p1, p2] = self.corners;
        let (k, min) = match a {
            [x0, fx, y0, fy, t, ..] if *t == 2.0 => ([*fx, *fy], [*x0, *y0]),
            [x0, x1, y0, y1, rest @ ..] if x1 != x0 && y1 != y0 => {
                let k = [(p2[0] - p1[0]) / (x1 - x0), (p2[1] - p1[1]) / (y1 - y0)];
                let k = if rest.first() == Some(&1.0) {
                    let m = k[0].abs().min(k[1].abs());
                    [m.copysign(k[0]), m.copysign(k[1])]
                } else {
                    k
                };
                (k, [*x0, *y0])
            }
            _ => ([1.0; 2], [0.0; 2]),
        };
        let o = if a.len() >= 4 { [p1[0] - min[0] * k[0], p1[1] - min[1] * k[1]] } else { [0.0; 2] };
        (self.k, self.o) = (k, o);
    }

    /// LT type, length in percent of the P1-P2 diagonal (4% if not given); without a type continuous.
    fn line_type(&mut self, a: &[f64]) {
        let Some(&t) = a.first() else {
            self.dash.clear();
            return;
        };
        let [p1, p2] = self.corners;
        let length = a.get(1).copied().unwrap_or(4.0) / 100.0 * (p2[0] - p1[0]).hypot(p2[1] - p1[1]);
        self.dash = match LINE_TYPES.get((t.abs() as usize).wrapping_sub(1)) {
            Some(pattern) => pattern.iter().map(|v| v / 100.0 * length).collect(),
            None => Vec::new(),
        };
    }

    /// PW width in millimetres, for one pen or for all.
    fn pen_width(&mut self, a: &[f64]) {
        let w = a[0].clamp(0.0, 10.0) as f32;
        match a.get(1) {
            Some(&pen) if (1.0..=256.0).contains(&pen) => {
                self.widths.resize(self.widths.len().max(pen as usize), 0.0);
                self.widths[pen as usize - 1] = w;
            }
            _ => self.widths = vec![w; self.colors.len().max(8)],
        }
    }

    fn command(&mut self, name: [u8; 2], a: &[f64]) -> Result<(), String> {
        let arg = |k: usize, d: f64| a.get(k).copied().unwrap_or(d);
        match &name {
            b"IN" | b"DF" => {
                self.flush()?;
                (self.down, self.relative, self.label_height, self.label_angle) = (false, false, LABEL, 0.0);
                (self.k, self.o, self.corners) = ([1.0; 2], [0.0; 2], SHEET);
                self.dash.clear();
                if &name == b"IN" {
                    self.widths.clear();
                }
            }
            b"IP" => self.corners = if a.len() >= 4 { [[a[0], a[1]], [a[2], a[3]]] } else { SHEET },
            b"SC" => self.scaling(a),
            b"LT" => {
                self.flush()?;
                self.line_type(a);
            }
            b"PW" if !a.is_empty() => {
                self.flush()?;
                self.pen_width(a);
            }
            b"SP" => {
                self.flush()?;
                self.pen = arg(0, 0.0) as usize;
            }
            b"PC" if a.len() >= 4 => {
                let pen = a[0] as usize;
                if (1..=256).contains(&pen) {
                    self.colors.resize(self.colors.len().max(pen), PENS[0]);
                    self.colors[pen - 1] = [a[1], a[2], a[3]].map(|v| v.clamp(0.0, 255.0) as u8);
                }
            }
            b"PU" | b"PD" | b"PA" | b"PR" => {
                match &name {
                    b"PU" => self.down = false,
                    b"PD" => self.down = true,
                    b"PA" => self.relative = false,
                    _ => self.relative = true,
                }
                if &name == b"PU" {
                    self.flush()?;
                }
                for xy in a.as_chunks::<2>().0 {
                    let to = if self.relative { self.by(xy) } else { self.user(xy) };
                    self.go(to)?;
                }
            }
            b"CI" => {
                let pts = cad_geom::circle(self.at, arg(0, 0.0) * self.k[0].abs());
                self.flush()?;
                self.add(pts, None)?;
            }
            b"AA" | b"AR" if a.len() >= 3 => {
                let c = if &name == b"AA" { self.user(a) } else { self.by(a) };
                let (r, start) = ((self.at[0] - c[0]).hypot(self.at[1] - c[1]), (self.at[1] - c[1]).atan2(self.at[0] - c[0]));
                let sweep = a[2].to_radians();
                let pts = cad_geom::arc(c, r, start.min(start + sweep), start.max(start + sweep));
                let end = [c[0] + r * (start + sweep).cos(), c[1] + r * (start + sweep).sin()];
                if self.down {
                    self.flush()?;
                    let pts = if sweep < 0.0 { pts.into_iter().rev().collect() } else { pts };
                    self.add(pts, None)?;
                }
                self.at = end;
            }
            b"EA" | b"ER" | b"RA" | b"RR" if a.len() >= 2 => {
                let o = if name[1] == b'A' { self.user(a) } else { self.by(a) };
                let (s, fill) = (self.at, (name[0] == b'R').then_some(1.0));
                self.flush()?;
                self.add(vec![s, [o[0], s[1]], o, [s[0], o[1]], s], fill)?;
            }
            // Character size in centimetres (absolute) and direction as run and rise.
            b"SI" if a.len() >= 2 => self.label_height = a[1] * 400.0,
            b"DI" if a.len() >= 2 => self.label_angle = a[1].atan2(a[0]),
            b"DI" => self.label_angle = 0.0,
            _ => {}
        }
        Ok(())
    }

    fn label(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if !text.trim().is_empty() && self.pen > 0 {
            self.out.items.push(Item::Text { at: self.at, height: self.label_height, angle: self.label_angle, text, color: self.color(), anchor: (Anchor::Start, Baseline::Alphabetic) });
        }
    }

    /// PE: coordinates written as base-64 (or, after `7`, base-32) numbers, relative unless
    /// after `=`, with pen up for the next after `<`, a pen after `:` and fraction bits after `>`.
    fn encoded(&mut self, data: &[u8]) -> Result<(), String> {
        let (mut base32, mut frac, mut absolute, mut up) = (false, 0u32, false, false);
        let mut nums = Numbers { data, at: 0 };
        while let Some(&c) = data.get(nums.at) {
            match c {
                b'7' => base32 = true,
                b':' => {
                    nums.at += 1;
                    self.flush()?;
                    self.pen = nums.next(base32).unwrap_or(0).max(0) as usize;
                    continue;
                }
                b'>' => {
                    nums.at += 1;
                    frac = nums.next(base32).unwrap_or(0).clamp(0, 26) as u32;
                    continue;
                }
                b'=' => absolute = true,
                b'<' => up = true,
                63..=126 | 191..=254 => {
                    let (Some(x), Some(y)) = (nums.next(base32), nums.next(base32)) else { break };
                    let scale = f64::from(1u32 << frac);
                    let (x, y) = (x as f64 / scale, y as f64 / scale);
                    let to = if absolute { [x, y] } else { [self.at[0] + x, self.at[1] + y] };
                    self.down = !up;
                    self.go(to)?;
                    (absolute, up) = (false, false);
                    continue;
                }
                _ => {}
            }
            nums.at += 1;
        }
        Ok(())
    }
}

struct Numbers<'a> {
    data: &'a [u8],
    at: usize,
}

impl Numbers<'_> {
    /// One signed number: digits from the lowest, the last one marked; the lowest bit is the sign.
    fn next(&mut self, base32: bool) -> Option<i64> {
        let (bits, term) = if base32 { (5, 95..=126) } else { (6, 191..=254) };
        let mut v: i64 = 0;
        for shift in (0..60).step_by(bits) {
            let c = *self.data.get(self.at)?;
            self.at += 1;
            let digit = if term.contains(&c) { c - term.start() } else if (63..*term.start()).contains(&c) { c - 63 } else { return None };
            v |= i64::from(digit) << shift;
            if term.contains(&c) {
                return Some(if v & 1 == 1 { -(v >> 1) } else { v >> 1 });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_pens_shapes_labels_and_encoded_lines() {
        let plt = b"\x1b%-12345X\x1bE\x1b%0BIN;SP2;PU0,0;PD1000,0,1000,1000;PU;SP1;PA2000,0;CI500;LBHello\x03PR100,0;EA3000,1000;PE<=\x40\x40\xbf\xbf;\x1b%0A";
        let d = read(plt).unwrap();
        let lines: Vec<_> = d.items.iter().filter_map(|i| match i { Item::Line { pts, color, .. } => Some((pts.len(), *color)), _ => None }).collect();
        assert_eq!(lines[0], (3, [255, 0, 0]));
        assert!(lines[1].0 > 30);
        assert!(d.items.iter().any(|i| matches!(i, Item::Text { text, at: [2000.0, 0.0], .. } if text == "Hello")));
        assert!(read(b"set terminal png\nplot sin(x)\n").is_err());
        // User units 0-100 on P1-P2 = 0-4000, a dashed line type and a pen 0.5 mm wide.
        let d = read(b"IN;IP0,0,4000,4000;SC0,100,0,100;SP1;PW0.5;LT2,10;PU0,0;PD100,50;").unwrap();
        let Item::Line { pts, stroke, .. } = &d.items[0] else { panic!() };
        assert_eq!(pts[1], [4000.0, 2000.0]);
        assert_eq!(stroke.weight, 0.5);
        assert!((stroke.dash[0] - 0.5 * 0.1 * 4000.0 * 2f64.sqrt()).abs() < 1e-6 && stroke.dash[1] < 0.0);
        // PE: 1 is written as 2 (sign in the lowest bit): 'A' (63 + 2) then the end mark 191 + 0.
        let mut n = Numbers { data: &[65, 191], at: 0 };
        assert_eq!(n.next(false), Some(1));
    }
}
