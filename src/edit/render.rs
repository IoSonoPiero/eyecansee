//! Drawing the objects of the editor: each becomes SVG and resvg draws it into a pixmap, for a
//! part of the result and at a given scale. The same code makes the preview (at the scale of the
//! screen) and the saved file (at 1), so what is shown is what is saved. The pixmap holds only the
//! objects, over transparency: the photo is under it on the screen and when saving. The cover box
//! is drawn here for the screen, in its place among the others; when saving it is written into the
//! pixels instead (`edit::save::cover`), solid and on whole pixels.

use std::fmt::Write;

use resvg::{tiny_skia, usvg};

use super::doc::{Area, Dash, Doc, Head, Obj, Pt, Shape, Style};

/// The part of the result drawn: `x`, `y` its top left corner in pixels of the result (after the
/// angle and the crop: 0, 0 is the top left corner of the crop), `w` × `h` the pixmap, `scale`
/// pixmap pixels per pixel of the result.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub x: f32,
    pub y: f32,
    pub w: u32,
    pub h: u32,
    pub scale: f32,
}

/// The objects of `doc` on an image of `size` (upright, before the turns), as `view` shows them.
/// `options` gives the fonts (`decode::usvg_options`: those of the system).
pub fn render(doc: &Doc, size: (u32, u32), view: &View, options: &usvg::Options) -> Option<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(&svg(doc, size, view), options).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(view.w, view.h)?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Some(pixmap)
}

/// Where the objects of `doc` are in the result, lines and letters included: left, top, right,
/// bottom in its pixels; `None` when nothing is drawn.
pub fn bounds(doc: &Doc, size: (u32, u32), options: &usvg::Options) -> Option<[f32; 4]> {
    let (w, h) = doc.crop.pixels();
    let tree = usvg::Tree::from_str(&svg(doc, size, &View { x: 0.0, y: 0.0, w, h, scale: 1.0 }), options).ok()?;
    let r = tree.root().abs_stroke_bounding_box();
    (r.width() > 0.0 && r.height() > 0.0).then(|| [r.left(), r.top(), r.right(), r.bottom()])
}

/// The SVG of the objects. A point of an object (from the centre of the turned image) is turned
/// by the angle, then put in the result as `rotate::rotate_crop` puts the pixels, then in the view.
fn svg(doc: &Doc, (w, h): (u32, u32), view: &View) -> String {
    let (iw, ih) = if doc.turn.sideways() { (h, w) } else { (w, h) };
    let (left, top) = crate::rotate::corner(iw as f32, ih as f32, doc.crop);
    let (tx, ty) = (-(left + view.x), -(top + view.y));
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{}" height="{}"><g transform="scale({}) translate({tx} {ty}) rotate({})">"#, view.w, view.h, view.scale, doc.angle);
    // The filters first: they change the photo, the objects drawn stay over them.
    let filter = |o: &&Obj| matches!(o.shape, Shape::Filter { .. });
    for o in doc.objects.iter().filter(filter).chain(doc.objects.iter().filter(|o| !filter(o))) {
        object(&mut out, o);
    }
    out.push_str("</g></svg>");
    out
}

fn object(out: &mut String, o: &Obj) {
    let Style { color: [r, g, b], width, opacity, dash, head, fill: inside } = o.style;
    let stroke = format!(r#"stroke="rgb({r},{g},{b})" stroke-width="{width}" fill="none"{}"#, dashes(dash, width));
    let fill = format!(r#"fill="rgb({r},{g},{b})""#);
    // The opacity on a group: where the line of an arrow meets its head it is not doubled.
    let _ = write!(out, r#"<g opacity="{opacity}">"#);
    let _ = match &o.shape {
        Shape::Line(a, b, c) => line(out, *a, *b, *c, &stroke),
        Shape::Arrow(a, b, c) => arrow(out, (*a, *b, *c), head, width, &stroke, &fill),
        // The inside first, the border over it.
        Shape::Rect(area) | Shape::Ellipse(area) => {
            let (tag, at) = if matches!(o.shape, Shape::Rect(_)) { ("rect", rect(area)) } else { ("ellipse", ellipse(area)) };
            if let Some([fr, fg, fb]) = inside {
                let _ = write!(out, r#"<{tag} {at} fill="rgb({fr},{fg},{fb})"/>"#);
            }
            write!(out, r#"<{tag} {at} {stroke}/>"#)
        }
        // A plain fill at the opacity of the style (`Style::highlighter`): over a layer of its own
        // a multiply would have no photo to blend with (tried: plain yellow).
        Shape::Highlight(area) => write!(out, r#"<rect {} {fill}/>"#, rect(area)),
        Shape::Text { text, size, font, path: Some(path), .. } => text_on(out, *path, (text, *size), font.as_deref(), &fill),
        Shape::Text { at, text, size, rot, font, .. } => self::text(out, (*at, *size, *rot), text, font.as_deref(), &fill),
        // Solid, no soft edge; when saving written into the pixels instead.
        Shape::Cover(area) => write!(out, r#"<rect {} {fill} shape-rendering="crispEdges"/>"#, rect(area)),
        // The picture stretched over its box, turned with it.
        Shape::Patch { area, pixels } => write!(out, r#"<image {} preserveAspectRatio="none" xlink:href="data:image/png;base64,{}"/>"#, rect(area), pixels.png_base64()),
        // Its preview (made by the page from the photo), over the box as it is now.
        Shape::Filter { area, preview, .. } => match &preview.0 {
            Some((_, _, pixels)) => write!(out, r#"<image {} preserveAspectRatio="none" xlink:href="data:image/png;base64,{}"/>"#, rect(area), pixels.png_base64()),
            None => Ok(()),
        },
    };
    out.push_str("</g>");
}

/// Ends of the lines and dashes: round when solid; square ends on dashes, round ones would
/// close the gaps; round dots of the width of the line, a width apart.
fn dashes(dash: Dash, width: f32) -> String {
    match dash {
        Dash::Solid => r#" stroke-linecap="round""#.into(),
        Dash::Dashed => format!(r#" stroke-linecap="butt" stroke-dasharray="{} {}""#, 3.0 * width, 2.0 * width),
        Dash::Dotted => format!(r#" stroke-linecap="round" stroke-dasharray="0 {}""#, 2.0 * width),
    }
}

/// Position, size and own angle of a rectangle.
fn rect(a: &Area) -> String {
    let Area { centre: c, w, h, rot } = *a;
    format!(r#"x="{}" y="{}" width="{w}" height="{h}" transform="rotate({rot} {} {})""#, c.x - w / 2.0, c.y - h / 2.0, c.x, c.y)
}

/// Centre, radii and own angle of the ellipse in a box.
fn ellipse(a: &Area) -> String {
    let Area { centre: c, w, h, rot } = *a;
    format!(r#"cx="{}" cy="{}" rx="{}" ry="{}" transform="rotate({rot} {} {})""#, c.x, c.y, w / 2.0, h / 2.0, c.x, c.y)
}

/// A line from `a` to `b`, straight or bent by `c`.
fn line(out: &mut String, a: Pt, b: Pt, c: Option<Pt>, stroke: &str) -> std::fmt::Result {
    match c {
        None => write!(out, r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {stroke}/>"#, a.x, a.y, b.x, b.y),
        Some(c) => write!(out, r#"<path d="M{} {} Q{} {} {} {}" {stroke}/>"#, a.x, a.y, c.x, c.y, b.x, b.y),
    }
}

/// The way an arrow leaves `at`, away from `towards` (the control point of a bent one): a unit
/// vector, `None` when the two are the same point.
fn direction(at: Pt, towards: Pt) -> Option<(f32, f32)> {
    let (dx, dy) = (at.x - towards.x, at.y - towards.y);
    let len = dx.hypot(dy);
    (len > 0.0).then(|| (dx / len, dy / len))
}

/// An arrow from `a` to `b` (bent by `c`) with its head: filled, open, at both ends, or a dot.
/// The line stops a little inside a filled head, so no gap shows; only the head when the line is
/// shorter than it.
fn arrow(out: &mut String, (a, b, c): (Pt, Pt, Option<Pt>), head: Head, width: f32, stroke: &str, fill: &str) -> std::fmt::Result {
    let len = (b.x - a.x).hypot(b.y - a.y);
    let (Some(end), Some(start)) = (direction(b, c.unwrap_or(a)), direction(a, c.unwrap_or(b))) else { return Ok(()) };
    let size = 4.0 * width + 4.0;
    // Where the line stops at either end.
    let short = |p: Pt, (ux, uy): (f32, f32)| Pt::new(p.x - ux * size * 0.8, p.y - uy * size * 0.8);
    let (from, to) = match head {
        Head::Filled => (a, short(b, end)),
        Head::Double => (short(a, start), short(b, end)),
        Head::Open | Head::Dot => (a, b),
    };
    if len > size * if head == Head::Double { 2.0 } else { 1.0 } || head == Head::Open || head == Head::Dot {
        // The control point moves with the ends, so the curve keeps its bend.
        let c = c.map(|c| Pt::new(c.x + (from.x - a.x + to.x - b.x) / 2.0, c.y + (from.y - a.y + to.y - b.y) / 2.0));
        line(out, from, to, c, stroke)?;
    }
    let filled = |out: &mut String, tip: Pt, (ux, uy): (f32, f32)| {
        let (bx, by) = (tip.x - ux * size, tip.y - uy * size);
        let (px, py) = (-uy * size / 2.0, ux * size / 2.0);
        write!(out, r#"<polygon points="{},{} {},{} {},{}" {fill}/>"#, tip.x, tip.y, bx + px, by + py, bx - px, by - py)
    };
    match head {
        Head::Filled => filled(out, b, end),
        Head::Double => {
            filled(out, b, end)?;
            filled(out, a, start)
        }
        Head::Open => {
            let (ux, uy) = end;
            let (bx, by) = (b.x - ux * size, b.y - uy * size);
            let (px, py) = (-uy * size / 2.0, ux * size / 2.0);
            // Solid even on a dashed arrow: a dashed head would not read as one.
            let solid = stroke.split(" stroke-dasharray").next().unwrap_or(stroke).replace("stroke-linecap=\"butt\"", "stroke-linecap=\"round\"");
            write!(out, r#"<polyline points="{},{} {},{} {},{}" {solid} stroke-linejoin="round"/>"#, bx + px, by + py, b.x, b.y, bx - px, by - py)
        }
        Head::Dot => write!(out, r#"<circle cx="{}" cy="{}" r="{}" {fill}/>"#, b.x, b.y, size * 0.4),
    }
}

/// Centred on `at`, one line under the other, in `font` (falling back on the system's
/// sans-serif one, also for a font no longer installed).
fn text(out: &mut String, (at, size, rot): (Pt, f32, f32), text: &str, font: Option<&str>, fill: &str) -> std::fmt::Result {
    let lines: Vec<&str> = text.lines().collect();
    let first = at.y - (lines.len().max(1) - 1) as f32 * 0.6 * size;
    let family = family(font);
    write!(out, r#"<text font-family="{family}sans-serif" font-size="{size}" text-anchor="middle" dominant-baseline="central" transform="rotate({rot} {} {})" {fill}>"#, at.x, at.y)?;
    for (i, line) in lines.iter().enumerate() {
        write!(out, r#"<tspan x="{}" y="{}">{}</tspan>"#, at.x, first + i as f32 * 1.2 * size, escaped(line))?;
    }
    out.push_str("</text>");
    Ok(())
}

/// The family of a font for `font-family`, before the sans-serif fallback; quotes and markup out
/// of the name: it goes inside an attribute.
fn family(font: Option<&str>) -> String {
    font.map(|f| format!("'{}', ", f.replace(['\'', '"', '<', '>', '&'], ""))).unwrap_or_default()
}

/// A text along its line (straight or bent), centred on it, its lines one after the other.
fn text_on(out: &mut String, (a, b, c): (Pt, Pt, Option<Pt>), (text, size): (&str, f32), font: Option<&str>, fill: &str) -> std::fmt::Result {
    // An id of its own in this SVG: the address of the text, unique while it is drawn.
    let id = format!("t{:x}", text.as_ptr() as usize);
    let d = match c {
        Some(c) => format!("M{} {} Q{} {} {} {}", a.x, a.y, c.x, c.y, b.x, b.y),
        None => format!("M{} {} L{} {}", a.x, a.y, b.x, b.y),
    };
    let family = family(font);
    let words = escaped(&text.lines().collect::<Vec<_>>().join(" "));
    write!(out, r##"<defs><path id="{id}" d="{d}"/></defs><text font-family="{family}sans-serif" font-size="{size}" dominant-baseline="central" {fill}><textPath xlink:href="#{id}" startOffset="50%" text-anchor="middle">{words}</textPath></text>"##)
}

fn escaped(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use image::{DynamicImage, RgbImage};

    use super::*;
    use crate::edit::doc::Area;
    use crate::rotate::Crop;
    use crate::transform::Op;

    /// The fonts of the tests: only the one egui carries in the program (Ubuntu Light), the same
    /// on every system and in the CI, as the sans-serif font.
    pub(crate) fn options() -> usvg::Options<'static> {
        let mut db = usvg::fontdb::Database::new();
        let font = &eframe::egui::FontDefinitions::default().font_data["Ubuntu-Light"];
        db.load_font_data((*font.font).as_ref().to_vec());
        let family = db.faces().next().expect("the font of egui").families[0].0.clone();
        db.set_sans_serif_family(family);
        usvg::Options { fontdb: Arc::new(db), ..Default::default() }
    }

    const W: u32 = 100;
    const H: u32 = 80;

    fn doc(objects: Vec<Obj>) -> Doc {
        Doc { objects, ..Doc::new(W, H, None) }
    }

    /// All of the result at 1 pixel per pixel.
    fn whole(d: &Doc) -> View {
        let (w, h) = d.crop.pixels();
        View { x: 0.0, y: 0.0, w, h, scale: 1.0 }
    }

    fn draw(d: &Doc, view: &View) -> tiny_skia::Pixmap {
        render(d, (W, H), view, &options()).expect("drawn")
    }

    fn alpha(p: &tiny_skia::Pixmap, x: u32, y: u32) -> u8 {
        p.pixel(x, y).unwrap().alpha()
    }

    /// Centre of the drawn pixels, weighted by their alpha.
    fn centroid(p: &tiny_skia::Pixmap) -> (f32, f32) {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for (i, px) in p.pixels().iter().enumerate() {
            let a = px.alpha() as f32;
            sx += a * ((i as u32 % p.width()) as f32 + 0.5);
            sy += a * ((i as u32 / p.width()) as f32 + 0.5);
            n += a;
        }
        (sx / n, sy / n)
    }

    /// Top row with ink, and the columns from the first to the last with ink.
    fn ink(p: &tiny_skia::Pixmap) -> (u32, u32, u32) {
        let (mut top, mut x0, mut x1) = (u32::MAX, u32::MAX, 0);
        for (i, px) in p.pixels().iter().enumerate() {
            if px.alpha() > 64 {
                let (x, y) = (i as u32 % p.width(), i as u32 / p.width());
                (top, x0, x1) = (top.min(y), x0.min(x), x1.max(x));
            }
        }
        (top, x0, x1)
    }

    fn style() -> Style {
        Style { color: [255, 0, 0], width: 4.0, opacity: 1.0, dash: Dash::Solid, head: Head::Filled, fill: None }
    }

    fn area(x: f32, y: f32, w: f32, h: f32) -> Area {
        Area { centre: Pt::new(x, y), w, h, rot: 0.0 }
    }

    /// Each kind of object where it was put, in pixels of the result (the centre of the image is
    /// at 50, 40), and nothing far from it.
    #[test]
    fn each_object_in_its_place() {
        let line = draw(&doc(vec![Obj { shape: Shape::Line(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0), None), style: style() }]), &whole(&doc(vec![])));
        assert_eq!((alpha(&line, 50, 40), alpha(&line, 25, 40), alpha(&line, 50, 30), alpha(&line, 10, 40)), (255, 255, 0, 0));

        let arrow = draw(&doc(vec![Obj { shape: Shape::Arrow(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0), None), style: style() }]), &whole(&doc(vec![])));
        // The head (20 px long, 20 wide at its base at x 60, the tip at 80), the line 4 wide.
        assert_eq!((alpha(&arrow, 64, 34), alpha(&arrow, 64, 46), alpha(&arrow, 25, 40)), (255, 255, 255));
        assert_eq!((alpha(&arrow, 25, 34), alpha(&arrow, 82, 40)), (0, 0));

        let rect = draw(&doc(vec![Obj { shape: Shape::Rect(area(0.0, 0.0, 40.0, 20.0)), style: style() }]), &whole(&doc(vec![])));
        // Outline only: the sides at x 30 and 70, y 30 and 50.
        assert_eq!((alpha(&rect, 30, 40), alpha(&rect, 69, 40), alpha(&rect, 50, 30), alpha(&rect, 50, 49)), (255, 255, 255, 255));
        assert_eq!((alpha(&rect, 50, 40), alpha(&rect, 20, 40)), (0, 0));

        let text = draw(&doc(vec![Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "Ab".into(), size: 30.0, rot: 0.0, font: None, path: None }, style: style() }]), &whole(&doc(vec![])));
        let (cx, cy) = centroid(&text);
        assert!((cx - 50.0).abs() < 4.0 && (cy - 40.0).abs() < 6.0, "text centred: {cx}, {cy}");
        assert_eq!((alpha(&text, 5, 5), alpha(&text, 95, 75)), (0, 0));

        // The cover box solid, for the screen (when saving it goes into the pixels).
        let cover = draw(&doc(vec![Obj { shape: Shape::Cover(area(0.0, 0.0, 40.0, 20.0)), style: style() }]), &whole(&doc(vec![])));
        assert_eq!((alpha(&cover, 50, 40), alpha(&cover, 31, 31), alpha(&cover, 20, 40)), (255, 255, 0));
    }

    /// With an angle and after flips and turns of the document the object stays over the same
    /// part of the photo: the centre of a white square in the edited photo and of a red square
    /// drawn over it are the same point of the result.
    #[test]
    fn objects_follow_angle_and_turns() {
        let photo = DynamicImage::ImageRgb8(RgbImage::from_fn(W, H, |x, y| if (68..73).contains(&x) && (18..23).contains(&y) { image::Rgb([255; 3]) } else { image::Rgb([0; 3]) }));
        // The square of the photo, from the centre of the image.
        let start = doc(vec![Obj { shape: Shape::Rect(area(20.5, -19.5, 2.5, 2.5)), style: Style { width: 2.5, ..style() } }]);
        let cases: [&[Op]; 6] = [&[], &[Op::Cw], &[Op::Ccw], &[Op::Half], &[Op::FlipH], &[Op::Cw, Op::FlipV]];
        for ops in cases {
            for angle in [0.0, 30.0, -12.5] {
                let mut d = start.clone();
                ops.iter().for_each(|&op| d.turned(op));
                d.angle = angle;
                let (w, h) = if d.turn.sideways() { (H, W) } else { (W, H) };
                d.crop = Crop { cx: 3.0, cy: -4.0, w: w as f32 * 0.9, h: h as f32 * 0.9 };
                let mut edited = photo.clone();
                d.edit().apply(&mut edited);
                let shown = tiny_skia::Pixmap::from_vec(edited.to_rgba8().into_raw(), tiny_skia::IntSize::from_wh(edited.width(), edited.height()).unwrap()).unwrap();
                // White is where the photo is not black: as alpha, the red channel of the photo.
                let mut white = shown.clone();
                white.pixels_mut().iter_mut().for_each(|p| *p = tiny_skia::PremultipliedColorU8::from_rgba(p.red(), p.red(), p.red(), p.red()).unwrap());
                let (ex, ey) = centroid(&white);
                let (gx, gy) = centroid(&draw(&d, &whole(&d)));
                assert!((gx - ex).abs() < 0.5 && (gy - ey).abs() < 0.5, "{ops:?} {angle}: object at {gx},{gy}, photo at {ex},{ey}");
            }
        }
    }

    /// Drawn at a quarter of the size, or at full size and then reduced (each 4 × 4 block
    /// averaged): the same picture but for the antialiasing.
    #[test]
    fn same_at_any_scale() {
        let d = doc(vec![
            Obj { shape: Shape::Line(Pt::new(-40.0, -30.0), Pt::new(35.0, 25.0), None), style: style() },
            Obj { shape: Shape::Arrow(Pt::new(-40.0, 30.0), Pt::new(30.0, -10.0), None), style: Style { dash: Dash::Dashed, width: 3.0, ..style() } },
            Obj { shape: Shape::Rect(Area { rot: 20.0, ..area(10.0, 5.0, 50.0, 30.0) }), style: Style { opacity: 0.5, ..style() } },
            Obj { shape: Shape::Text { at: Pt::new(-10.0, 0.0), text: "àèìòù".into(), size: 24.0, rot: -10.0, font: None, path: None }, style: style() },
        ]);
        let full = draw(&d, &View { x: 0.0, y: 0.0, w: W * 4, h: H * 4, scale: 4.0 });
        let small = draw(&d, &View { x: 0.0, y: 0.0, w: W, h: H, scale: 1.0 });
        let (mut worst, mut total) = (0u32, 0u64);
        for (y, x) in (0..H).flat_map(|y| (0..W).map(move |x| (y, x))) {
            let mut sum = [0u32; 4];
            for (dx, dy) in (0..4).flat_map(|dx| (0..4).map(move |dy| (dx, dy))) {
                let p = full.pixel(x * 4 + dx, y * 4 + dy).unwrap();
                for (s, v) in sum.iter_mut().zip([p.red(), p.green(), p.blue(), p.alpha()]) {
                    *s += v as u32;
                }
            }
            let p = small.pixel(x, y).unwrap();
            for (s, v) in sum.iter().zip([p.red(), p.green(), p.blue(), p.alpha()]) {
                let diff = (s / 16).abs_diff(v as u32);
                worst = worst.max(diff);
                total += diff as u64;
            }
        }
        let mean = total as f64 / (W * H * 4) as f64;
        assert!(worst <= 48 && mean <= 1.0, "worst {worst}, mean {mean:.3}");
    }

    /// Opacity in the alpha of the pixels, dashes as gaps along the line.
    #[test]
    fn opacity_and_dashes() {
        let line = |style| draw(&doc(vec![Obj { shape: Shape::Line(Pt::new(-45.0, 0.0), Pt::new(45.0, 0.0), None), style }]), &whole(&doc(vec![])));
        assert_eq!(alpha(&line(Style { opacity: 0.5, ..style() }), 50, 40), 128);
        let gaps = |p: &tiny_skia::Pixmap| (10..90).filter(|&x| alpha(p, x, 40) == 0).count();
        let switches = |p: &tiny_skia::Pixmap| (10..90).filter(|&x| (alpha(p, x, 40) > 128) != (alpha(p, x + 1, 40) > 128)).count();
        assert_eq!(gaps(&line(style())), 0);
        // Dashes of 12 and gaps of 8, round dots of 4 every 8.
        let dashed = line(Style { dash: Dash::Dashed, ..style() });
        assert!(gaps(&dashed) > 20 && switches(&dashed) >= 7, "{} {}", gaps(&dashed), switches(&dashed));
        let dotted = line(Style { dash: Dash::Dotted, ..style() });
        assert!(gaps(&dotted) > 30 && switches(&dotted) >= 18, "{} {}", gaps(&dotted), switches(&dotted));
    }

    /// Accented letters keep their accents, and none is lost: each accented vowel has the ink of
    /// the plain one and more where it has none (the accent), and the words are as wide.
    #[test]
    fn accents_kept() {
        let text = |s: &str| draw(&doc(vec![Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: s.into(), size: 30.0, rot: 0.0, font: None, path: None }, style: style() }]), &whole(&doc(vec![])));
        let inked = |p: &tiny_skia::Pixmap| p.pixels().iter().map(|px| px.alpha() > 128).collect::<Vec<_>>();
        for (accented, plain) in "àèìòù".chars().zip("aeiou".chars()) {
            let (a, p) = (inked(&text(&accented.to_string())), inked(&text(&plain.to_string())));
            let base = p.iter().filter(|&&x| x).count();
            let kept = a.iter().zip(&p).filter(|(a, p)| **a && **p).count();
            let accent = a.iter().zip(&p).filter(|(a, p)| **a && !**p).count();
            assert!(kept * 10 >= base * 9 && accent >= 8, "{accented}: {kept} of {base} kept, {accent} more");
        }
        let (a, p) = (ink(&text("àèìòù")), ink(&text("aeiou")));
        let (wa, wp) = (a.2 - a.1, p.2 - p.1);
        assert!(wp > 40 && wa.abs_diff(wp) <= 3, "widths {wa} and {wp}");
        // Two lines, one under the other.
        let (two, one) = (ink(&text("à\nà")), ink(&text("à")));
        assert!(two.0 + 10 < one.0);
    }

    /// The highlighter: the whole rectangle filled, yellow at 40%, turned by its own angle; the
    /// photo shows through it.
    #[test]
    fn highlighter() {
        let d = doc(vec![Obj { shape: Shape::Highlight(Area { rot: 90.0, ..area(0.0, 0.0, 40.0, 10.0) }), style: Style::highlighter() }]);
        let p = draw(&d, &whole(&d));
        // Turned upright: 10 wide (x 45 to 55), 40 high (y 20 to 60).
        let c = p.pixel(50, 40).unwrap().demultiply();
        assert_eq!((c.red(), c.green(), c.blue(), c.alpha()), (255, 230, 0, 102));
        assert_eq!((alpha(&p, 46, 21), alpha(&p, 53, 58), alpha(&p, 50, 18), alpha(&p, 60, 40), alpha(&p, 30, 40)), (102, 102, 0, 0, 0));
    }

    /// The box of an object holds all of it, line width and arrow head included; nothing drawn,
    /// no box.
    #[test]
    fn bounds_hold_the_object() {
        let d = doc(vec![Obj { shape: Shape::Arrow(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0), None), style: style() }]);
        let [l, t, r, b] = bounds(&d, (W, H), &options()).unwrap();
        // From the round end of the line (x 20, less half its width) to the tip (80), the head
        // 20 wide around y 40.
        assert!((l - 18.0).abs() < 0.01 && (r - 80.0).abs() < 0.01 && (t - 30.0).abs() < 0.01 && (b - 50.0).abs() < 0.01, "{l} {t} {r} {b}");
        let p = draw(&d, &whole(&d));
        for (i, px) in p.pixels().iter().enumerate() {
            let (x, y) = ((i as u32 % W) as f32 + 0.5, (i as u32 / W) as f32 + 0.5);
            assert!(px.alpha() == 0 || (x > l - 1.0 && x < r + 1.0 && y > t - 1.0 && y < b + 1.0), "{x},{y} outside");
        }
        assert_eq!(bounds(&doc(vec![]), (W, H), &options()), None);
    }

    /// The heads: open (no fill between its two strokes), at both ends, a dot at the tip; a bent
    /// line passes through the middle of its curve, not the straight middle.
    #[test]
    fn heads_and_curves() {
        let arrow = |head| draw(&doc(vec![Obj { shape: Shape::Arrow(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0), None), style: Style { head, ..style() } }]), &whole(&doc(vec![])));
        // Tip at 80, 40; heads 20 long and 20 wide.
        let on = |p: &tiny_skia::Pixmap, x, y| alpha(p, x, y) > 128;
        let open = arrow(Head::Open);
        assert!(on(&open, 79, 40) && on(&open, 63, 33) && !on(&open, 66, 37), "open head");
        let double = arrow(Head::Double);
        assert!(on(&double, 36, 34) && on(&double, 76, 40) && on(&double, 50, 40), "double head");
        let dot = arrow(Head::Dot);
        assert!(on(&dot, 80, 46) && !on(&dot, 64, 34), "dot");
        let bent = draw(&doc(vec![Obj { shape: Shape::Line(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0), Some(Pt::new(0.0, 40.0))), style: style() }]), &whole(&doc(vec![])));
        // The middle of the curve 20 below the straight one: at 50, 60.
        assert!(on(&bent, 50, 60) && !on(&bent, 50, 40), "bent");
    }

    /// A text in a font that is not installed, or whose name has quotes and markup in it, is
    /// still drawn, in the sans-serif one.
    #[test]
    fn missing_font_falls_back() {
        for font in [None, Some("No Such Font"), Some("Bad\" <font>&'name")] {
            let t = draw(&doc(vec![Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "Ab".into(), size: 30.0, rot: 0.0, font: font.map(String::from), path: None }, style: style() }]), &whole(&doc(vec![])));
            assert!(t.pixels().iter().filter(|p| p.alpha() > 128).count() > 100, "{font:?}");
        }
    }

    /// A rectangle filled: the inside in its colour, the border in the other.
    #[test]
    fn filled_rectangle() {
        let d = doc(vec![Obj { shape: Shape::Rect(area(0.0, 0.0, 40.0, 20.0)), style: Style { fill: Some([0, 0, 255]), ..style() } }]);
        let p = draw(&d, &whole(&d));
        let c = |x, y| { let c = p.pixel(x, y).unwrap().demultiply(); (c.red(), c.green(), c.blue(), c.alpha()) };
        assert_eq!((c(50, 40), c(30, 40)), ((0, 0, 255, 255), (255, 0, 0, 255)));
    }

    /// An ellipse filled: the inside in its colour, the border on the ellipse, the corners of its
    /// box empty; turned by its own angle.
    #[test]
    fn filled_ellipse() {
        let d = doc(vec![Obj { shape: Shape::Ellipse(area(0.0, 0.0, 40.0, 20.0)), style: Style { fill: Some([0, 0, 255]), ..style() } }]);
        let p = draw(&d, &whole(&d));
        let c = |p: &tiny_skia::Pixmap, x, y| { let c = p.pixel(x, y).unwrap().demultiply(); (c.red(), c.green(), c.blue(), c.alpha()) };
        // Centre 50, 40; radii 20 and 10: the border at x 30 and y 30.
        assert_eq!((c(&p, 50, 40), c(&p, 30, 40), c(&p, 50, 30)), ((0, 0, 255, 255), (255, 0, 0, 255), (255, 0, 0, 255)));
        assert_eq!((c(&p, 31, 31).3, c(&p, 69, 49).3), (0, 0));
        // Turned upright: 20 wide, 40 high.
        let d = doc(vec![Obj { shape: Shape::Ellipse(Area { rot: 90.0, ..area(0.0, 0.0, 40.0, 20.0) }), style: Style { fill: Some([0, 0, 255]), ..style() } }]);
        let p = draw(&d, &whole(&d));
        assert_eq!((c(&p, 50, 25).2, c(&p, 35, 40).3), (255, 0));
    }

    /// A patch: its pixels where its box is, stretched to it.
    #[test]
    fn patch_drawn_in_its_box() {
        let pixels = crate::edit::doc::Pixels::new(image::RgbaImage::from_fn(4, 2, |x, _| if x < 2 { image::Rgba([0, 0, 255, 255]) } else { image::Rgba([0, 255, 0, 255]) }));
        let d = doc(vec![Obj { shape: Shape::Patch { area: area(0.0, 0.0, 40.0, 20.0), pixels }, style: style() }]);
        let p = draw(&d, &whole(&d));
        let c = |x, y| { let c = p.pixel(x, y).unwrap().demultiply(); (c.red(), c.green(), c.blue(), c.alpha()) };
        // The box from 30 to 70: its left half blue, its right half green, nothing outside.
        assert_eq!((c(35, 40), c(65, 40), c(20, 40).3), ((0, 0, 255, 255), (0, 255, 0, 255), 0));
    }

    /// A text on a bent line: its letters along the curve, well below the straight line.
    #[test]
    fn text_on_a_curve() {
        let path = Some((Pt::new(-40.0, 0.0), Pt::new(40.0, 0.0), Some(Pt::new(0.0, 60.0))));
        let d = doc(vec![Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "Abcdefg".into(), size: 10.0, rot: 0.0, font: None, path }, style: style() }]);
        let p = draw(&d, &whole(&d));
        let (_, cy) = centroid(&p);
        // The middle of the curve 30 below the centre (40 in the result): the letters around 70.
        assert!(cy > 60.0, "letters at {cy}");
    }
}

