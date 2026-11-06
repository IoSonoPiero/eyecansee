//! The speech bubbles of the editor, without the interface: their shapes (rounded, oval, square,
//! a cloud for thoughts, spikes for a shout), their text broken into lines to fit their width,
//! and the height they need for it. The same code draws the screen and the saved file.

use std::fmt::Write;

use resvg::usvg;
use serde::{Deserialize, Serialize};

use super::doc::{Area, Obj, Pt, Shape, Style};
use super::geom::turned;

/// The shape of a bubble.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Balloon {
    #[default]
    Rounded,
    Oval,
    Square,
    /// A thought: a cloud, round puffs for a tail.
    Cloud,
    /// A shout: spikes all round.
    Shout,
}

impl Balloon {
    pub const ALL: [Balloon; 5] = [Balloon::Rounded, Balloon::Oval, Balloon::Square, Balloon::Cloud, Balloon::Shout];

    /// How much of the box the letters may take, across and down: less in the round shapes.
    fn room(self) -> f32 {
        match self {
            Balloon::Rounded | Balloon::Square => 1.0,
            Balloon::Oval => 0.72,
            Balloon::Cloud => 0.68,
            Balloon::Shout => 0.6,
        }
    }
}

/// Space between the letters and the edge, in heights of the letters.
const PAD: f32 = 0.4;

/// The lines of `text` (its own lines broken where a word would go past `width`), in `font`
/// at `size`, measured with the fonts of `options`.
pub fn lines(text: &str, size: f32, font: Option<&str>, width: f32, options: &usvg::Options) -> Vec<String> {
    let mut out = Vec::new();
    for own in text.lines() {
        let mut line = String::new();
        for word in own.split_whitespace() {
            let longer = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && measure(&longer, size, font, options) > width {
                out.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = longer;
            }
        }
        out.push(line);
    }
    out
}

/// How wide a line is drawn.
fn measure(line: &str, size: f32, font: Option<&str>, options: &usvg::Options) -> f32 {
    let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">"#);
    let _ = super::render::text(&mut svg, (Pt::new(0.0, 0.0), size, 0.0), line, font, "");
    svg.push_str("</svg>");
    usvg::Tree::from_str(&svg, options).map_or(0.0, |t| t.root().abs_bounding_box().width())
}

/// The width the letters of a bubble of `w` may take.
fn text_width(w: f32, style: &Style) -> f32 {
    (w * style.balloon.room() - 2.0 * PAD * style.letters).max(style.letters)
}

/// The bubble made tall enough for its text (as wide as it is: the text goes to a new line):
/// it grows, from its middle; never shrinks under the height given to it.
pub fn fit(o: &mut Obj, options: &usvg::Options) {
    let Shape::Bubble { area, text, font, .. } = &mut o.shape else { return };
    let size = o.style.letters.max(1.0);
    let n = lines(text, size, font.as_deref(), text_width(area.w, &o.style), options).len().max(1) as f32;
    let needed = (1.2 * size * n + 2.0 * PAD * size) / o.style.balloon.room();
    if area.h < needed {
        area.h = needed;
    }
}

/// The bubble: its border drawn twice as wide round its shape and its tail, with the inside of
/// both cut out of it (a mask, only as large as the bubble: one as large as the image cost
/// seconds), so it shows only outside them and not where the tail joins; then the fill, then the
/// letters, broken into lines to fit.
pub fn svg(out: &mut String, (a, tail): (&Area, Pt), (text, font): (&str, Option<&str>), style: &Style, options: &usvg::Options) -> std::fmt::Result {
    let ([r, g, b], [ir, ig, ib]) = (style.color, style.ink);
    // Its own id in this SVG: the address of its box, unique while it is drawn.
    let id = format!("b{:x}", a as *const Area as usize);
    let shape = format!("{}{}", body(a, style.balloon), self::tail(a, tail, style.balloon));
    let reach = a.w.hypot(a.h) / 2.0 + 2.0 * style.width + 1.0;
    let m = 2.0 * style.width + 1.0;
    let (x0, y0) = ((a.centre.x - reach).min(tail.x - m), (a.centre.y - reach).min(tail.y - m));
    let (x1, y1) = ((a.centre.x + reach).max(tail.x + m), (a.centre.y + reach).max(tail.y + m));
    let all = format!(r#"x="{x0}" y="{y0}" width="{}" height="{}""#, x1 - x0, y1 - y0);
    write!(out, r#"<defs><mask id="{id}" maskUnits="userSpaceOnUse" {all}><rect {all} fill="white"/><g fill="black">{shape}</g></mask></defs>"#)?;
    write!(out, r#"<g mask="url(#{id})" fill="none" stroke="rgb({r},{g},{b})" stroke-width="{}" stroke-linejoin="round">{shape}</g>"#, 2.0 * style.width)?;
    if let Some([fr, fg, fb]) = style.fill {
        write!(out, r#"<g fill="rgb({fr},{fg},{fb})">{shape}</g>"#)?;
    }
    if text.trim().is_empty() {
        return Ok(());
    }
    let size = style.letters.max(1.0);
    let broken = lines(text, size, font, text_width(a.w, style), options).join("\n");
    super::render::text(out, (a.centre, size, a.rot), &broken, font, &format!(r#"fill="rgb({ir},{ig},{ib})""#))
}

/// The point at `x`, `y` from the centre of the box, along its sides.
fn at(a: &Area, x: f32, y: f32) -> Pt {
    let p = turned(Pt::new(x, y), a.rot);
    Pt::new(a.centre.x + p.x, a.centre.y + p.y)
}

/// The shape of the bubble in its box, as an SVG element with no paint.
fn body(a: &Area, balloon: Balloon) -> String {
    let (hw, hh) = (a.w / 2.0, a.h / 2.0);
    let turn = format!(r#"transform="rotate({} {} {})""#, a.rot, a.centre.x, a.centre.y);
    let around = |n: usize, radius: &dyn Fn(usize) -> f32| -> Vec<Pt> {
        (0..n).map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            at(a, hw * radius(i) * t.cos(), hh * radius(i) * t.sin())
        }).collect()
    };
    match balloon {
        Balloon::Rounded => format!(r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" {turn}/>"#, a.centre.x - hw, a.centre.y - hh, a.w, a.h, 0.2 * a.w.min(a.h)),
        Balloon::Square => format!(r#"<rect x="{}" y="{}" width="{}" height="{}" {turn}/>"#, a.centre.x - hw, a.centre.y - hh, a.w, a.h),
        Balloon::Oval => format!(r#"<ellipse cx="{}" cy="{}" rx="{hw}" ry="{hh}" {turn}/>"#, a.centre.x, a.centre.y),
        // Puffs: arcs bulging out between points round an ellipse a little inside the box.
        Balloon::Cloud => {
            let p = around(12, &|_| 0.86);
            let mut d = format!("M{} {}", p[0].x, p[0].y);
            for i in 0..p.len() {
                let (s, e) = (p[i], p[(i + 1) % p.len()]);
                let r = 0.6 * (e.x - s.x).hypot(e.y - s.y);
                let _ = write!(d, " A{r} {r} 0 0 1 {} {}", e.x, e.y);
            }
            format!(r#"<path d="{d} Z"/>"#)
        }
        // Spikes out to the box, in between a fifth inside it.
        Balloon::Shout => {
            let p = around(28, &|i| if i % 2 == 0 { 1.0 } else { 0.78 });
            format!(r#"<polygon points="{}"/>"#, p.iter().map(|q| format!("{},{}", q.x, q.y)).collect::<Vec<_>>().join(" "))
        }
    }
}

/// The tail to `tip`: a wedge from across the middle of the bubble, as wide as a third of its
/// short side; for a thought three puffs getting smaller towards it.
fn tail(a: &Area, tip: Pt, balloon: Balloon) -> String {
    let short = a.w.min(a.h);
    let (dx, dy) = (tip.x - a.centre.x, tip.y - a.centre.y);
    if balloon == Balloon::Cloud {
        return [(0.62, 0.11), (0.8, 0.075), (0.95, 0.05)]
            .iter()
            .map(|(t, r)| format!(r#"<circle cx="{}" cy="{}" r="{}"/>"#, a.centre.x + dx * t, a.centre.y + dy * t, r * short))
            .collect();
    }
    let len = dx.hypot(dy).max(1e-3);
    let (nx, ny) = (-dy / len * 0.15 * short, dx / len * 0.15 * short);
    format!(r#"<polygon points="{},{} {},{} {},{}"/>"#, a.centre.x + nx, a.centre.y + ny, tip.x, tip.y, a.centre.x - nx, a.centre.y - ny)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::render::tests::options;

    fn bubble(w: f32, text: &str, balloon: Balloon) -> Obj {
        let style = Style { letters: 20.0, balloon, width: 1.0, fill: Some([255; 3]), ..Style::default() };
        Obj { shape: Shape::Bubble { area: Area { centre: Pt::new(0.0, 0.0), w, h: 30.0, rot: 0.0 }, tail: Pt::new(0.0, 80.0), text: text.into(), font: None }, style }
    }

    /// The words go to a new line where the next would not fit, the lines typed stay; a word
    /// longer than the bubble stays whole.
    #[test]
    fn broken_into_lines() {
        let o = options();
        let one = measure("word", 20.0, None, &o);
        assert!(one > 20.0 && one < 80.0, "{one}");
        assert_eq!(lines("word word word", 20.0, None, one * 2.5, &o), ["word word", "word"]);
        assert_eq!(lines("word\nword", 20.0, None, 1000.0, &o), ["word", "word"]);
        assert_eq!(lines("extraordinarily", 20.0, None, 10.0, &o), ["extraordinarily"]);
    }

    /// A bubble grows taller to hold its text, more when narrower; never shorter than drawn.
    #[test]
    fn grows_with_its_text() {
        let o = options();
        let mut wide = bubble(400.0, "one two three four five six", Balloon::Rounded);
        let mut narrow = bubble(120.0, "one two three four five six", Balloon::Rounded);
        fit(&mut wide, &o);
        fit(&mut narrow, &o);
        let h = |o: &Obj| o.shape.area().unwrap().h;
        assert!(h(&wide) >= 1.2 * 20.0 && h(&narrow) > h(&wide) * 2.0, "{} {}", h(&wide), h(&narrow));
        let mut tall = bubble(400.0, "a", Balloon::Rounded);
        if let Some(a) = tall.shape.area_mut() {
            a.h = 300.0;
        }
        fit(&mut tall, &o);
        assert_eq!(h(&tall), 300.0);
        // An oval needs more room than a rounded box for the same text.
        let mut oval = bubble(400.0, "one two three four five six", Balloon::Oval);
        fit(&mut oval, &o);
        assert!(h(&oval) > h(&wide));
    }

    /// Every shape draws in its box, filled in the middle; the cloud's tail is puffs (white
    /// round the middle of the line to the tip, not along all of it as a wedge).
    #[test]
    fn shapes_drawn() {
        use crate::edit::render::{View, render};
        let o = options();
        for balloon in Balloon::ALL {
            let mut b = bubble(60.0, "", balloon);
            if let Some(a) = b.shape.area_mut() {
                a.h = 40.0;
            }
            let d = crate::edit::doc::Doc { objects: vec![b], ..crate::edit::doc::Doc::new(100, 120, None) };
            let p = render(&d, (100, 120), &View { x: 0.0, y: 0.0, w: 100, h: 120, scale: 1.0 }, &o).unwrap();
            assert_eq!(p.pixel(50, 60).unwrap().alpha(), 255, "{balloon:?} middle");
            assert_eq!(p.pixel(2, 2).unwrap().alpha(), 0, "{balloon:?} corner");
            // On the tail between the first two puffs (y 105..114 and 121..127 with the border).
            let on = p.pixel(50, 117).unwrap().alpha();
            assert!(if balloon == Balloon::Cloud { on == 0 } else { on == 255 }, "{balloon:?} tail {on}");
        }
    }
}
