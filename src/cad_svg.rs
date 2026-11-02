//! A CAD drawing (`cad::Drawing`) written as SVG for resvg: its box, the lines with their
//! weights and dashes, filled areas, texts and attached pictures, on the dark background.

use std::fmt::Write;

use crate::cad::{Anchor, BACKGROUND, Baseline, Drawing, Item, Stroke, THIN};
use crate::cad_geom::P;

/// The corners of a text's box, estimated (0.7 of the height a letter, lines 1.75 heights apart
/// as `svg` writes them), aligned as the text and turned with it.
pub(crate) fn text_box(at: P, h: f64, angle: f64, text: &str, anchor: (Anchor, Baseline)) -> [P; 4] {
    let w = 0.7 * h * text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    let below = 1.75 * h * text.lines().count().saturating_sub(1) as f64;
    let x0 = match anchor.0 {
        Anchor::Start => 0.0,
        Anchor::Middle => -w / 2.0,
        Anchor::End => -w,
    };
    // Rows from the top of the letters to the bottom of the descenders of the last line.
    let (top, bottom) = match anchor.1 {
        Baseline::Alphabetic => (1.4 * h, -0.4 * h - below),
        Baseline::Middle => (0.8 * h, -0.8 * h - below),
        Baseline::Hanging => (0.0, -1.6 * h - below),
    };
    let (s, c) = angle.sin_cos();
    [[x0, bottom], [x0 + w, bottom], [x0 + w, top], [x0, top]].map(|[x, y]| [at[0] + x * c - y * s, at[1] + x * s + y * c])
}

impl Drawing {
    /// Box around what is drawn: lines, and the text positions with an estimate of their size.
    pub(crate) fn bounds(&self) -> Option<[f64; 4]> {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let mut add = |p: P| {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        };
        for item in &self.items {
            match item {
                Item::Line { pts, .. } => pts.iter().filter(|p| !p[0].is_nan()).for_each(|&p| add(p)),
                Item::Text { at, height, angle, text, anchor, .. } => text_box(*at, *height, *angle, text, *anchor).into_iter().for_each(&mut add),
                Item::Image { corners, .. } => corners.iter().for_each(|&p| add(p)),
            }
        }
        (b[0] <= b[2] && b[1] <= b[3]).then_some(b)
    }

    /// The drawing as an SVG whose long side is `side` pixels, lines `stroke` pixels wide (less
    /// than 1 when the SVG will be drawn larger, so that they stay 1 pixel on the screen).
    pub fn svg(&self, side: u32, stroke: f32) -> Result<String, String> {
        let [x0, y0, x1, y1] = self.bounds().ok_or("nothing to draw")?;
        let side = side as f64;
        // A margin of 2% around; a drawing that is one point or one line still gets a size.
        let (w, h) = ((x1 - x0).max(f64::MIN_POSITIVE), (y1 - y0).max(f64::MIN_POSITIVE));
        let span = w.max(h) * 1.04;
        let s = side / span;
        let (pw, ph) = (((w * 1.04).max(span * 0.01) * s).ceil().max(1.0), ((h * 1.04).max(span * 0.01) * s).ceil().max(1.0));
        let (ox, oy) = (x0 - w * 0.02, y1 + h * 0.02);
        let px = |p: P| ((p[0] - ox) * s, (oy - p[1]) * s);
        let rgb = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
        let [br, bg, bb] = BACKGROUND;
        let mut out = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="{pw}" height="{ph}" viewBox="0 0 {pw} {ph}"><rect width="100%" height="100%" fill="#{br:02x}{bg:02x}{bb:02x}"/><g fill="none" stroke-width="{stroke}" stroke-linejoin="round" stroke-linecap="round" font-family="sans-serif">"##);
        for item in &self.items {
            match item {
                Item::Line { pts, color, fill, stroke: line } => {
                    let c = rgb(*color);
                    let _ = write!(out, r#"<path{} d="M"#, stroke_attributes(line, s, stroke));
                    let mut start = false;
                    for (i, p) in pts.iter().enumerate() {
                        if p[0].is_nan() {
                            out.push_str("Z M");
                            start = true;
                            continue;
                        }
                        let (x, y) = px(*p);
                        let _ = write!(out, "{}{x:.2} {y:.2}", if i == 0 || start { "" } else { "L" });
                        start = false;
                    }
                    match fill {
                        Some(o) => {
                            let _ = write!(out, r#"Z" fill="{c}" fill-opacity="{o}" fill-rule="evenodd" stroke="{c}"/>"#);
                        }
                        None => {
                            let _ = write!(out, r#"" stroke="{c}"/>"#);
                        }
                    }
                }
                Item::Image { corners, png } => {
                    let [a, b, c, d, e, f] = crate::cad_image::matrix(corners.map(px));
                    let _ = write!(out, r#"<image width="1" height="1" preserveAspectRatio="none" transform="matrix({a:.3} {b:.3} {c:.3} {d:.3} {e:.3} {f:.3})" href="data:image/png;base64,{png}"/>"#);
                }
                Item::Text { at, height, angle, text, color, anchor } => {
                    let (x, y) = px(*at);
                    let size = height * s;
                    if size < 0.5 * stroke as f64 {
                        continue;
                    }
                    let a = match anchor.0 {
                        Anchor::Start => "start",
                        Anchor::Middle => "middle",
                        Anchor::End => "end",
                    };
                    let b = match anchor.1 {
                        Baseline::Alphabetic => "alphabetic",
                        Baseline::Middle => "central",
                        Baseline::Hanging => "hanging",
                    };
                    // CAD text height is that of the capitals: a font size ~1.4 times it.
                    let _ = write!(out, r#"<text x="{x:.2}" y="{y:.2}" font-size="{:.2}" fill="{}" stroke="none" text-anchor="{a}" dominant-baseline="{b}" transform="rotate({:.3} {x:.2} {y:.2})">"#, size * 1.4, rgb(*color), -angle.to_degrees());
                    for (i, line) in text.lines().enumerate() {
                        let dy = if i == 0 { 0.0 } else { size * 1.4 * 1.25 };
                        let _ = write!(out, r#"<tspan x="{x:.2}" dy="{dy:.2}">{}</tspan>"#, escape(line));
                    }
                    out.push_str("</text>");
                }
            }
        }
        out.push_str("</g></svg>");
        Ok(out)
    }
}

/// Width and dashes of a line in the picture: weights above the thinnest wider in proportion
/// (as AutoCAD shows them, in pixels at any zoom), dashes at the picture's scale `s`, a line
/// type too fine to see drawn continuous.
fn stroke_attributes(line: &Stroke, s: f64, thin: f32) -> String {
    let mut out = String::new();
    let width = thin * (line.weight / THIN).max(1.0);
    if width != thin {
        let _ = write!(out, r#" stroke-width="{width:.2}""#);
    }
    let px: Vec<f64> = line.dash.iter().map(|v| v * s).collect();
    if px.iter().map(|v| v.abs()).sum::<f64>() < 4.0 * f64::from(width) {
        return out;
    }
    // SVG alternates dash and gap from a dash: equal neighbours joined, a zero dash (a dot)
    // before a leading gap, a zero gap after a trailing dash.
    let mut parts: Vec<(bool, f64)> = Vec::new();
    for v in px {
        let dash = v >= 0.0;
        match parts.last_mut() {
            Some((d, len)) if *d == dash => *len += v.abs(),
            _ => parts.push((dash, v.abs())),
        }
    }
    if parts.first().is_some_and(|p| !p.0) {
        parts.insert(0, (true, 0.0));
    }
    if parts.len() % 2 == 1 {
        parts.push((false, 0.0));
    }
    let list: Vec<String> = parts.iter().map(|(_, v)| format!("{:.2}", v.max(0.01))).collect();
    let _ = write!(out, r#" stroke-dasharray="{}""#, list.join(" "));
    out
}

fn escape(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            c => c.to_string(),
        })
        .collect()
}
