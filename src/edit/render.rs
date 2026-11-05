//! Drawing the objects of the editor: each becomes SVG and resvg draws it into a pixmap, for a
//! part of the result and at a given scale. The same code makes the preview (at the scale of the
//! screen) and the saved file (at 1), so what is shown is what is saved. The pixmap holds only the
//! objects, over transparency: the photo is under it on the screen and when saving. The cover box
//! is not drawn here: it is written into the pixels when saving, solid and on whole pixels.

use std::fmt::Write;

use resvg::{tiny_skia, usvg};

use super::doc::{Area, Dash, Doc, Obj, Pt, Shape, Style};

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
    let mut out = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}"><g transform="scale({}) translate({tx} {ty}) rotate({})">"#, view.w, view.h, view.scale, doc.angle);
    for o in &doc.objects {
        object(&mut out, o);
    }
    out.push_str("</g></svg>");
    out
}

fn object(out: &mut String, o: &Obj) {
    let Style { color: [r, g, b], width, opacity, dash } = o.style;
    let stroke = format!(r#"stroke="rgb({r},{g},{b})" stroke-width="{width}" fill="none"{}"#, dashes(dash, width));
    let fill = format!(r#"fill="rgb({r},{g},{b})""#);
    // The opacity on a group: where the line of an arrow meets its head it is not doubled.
    let _ = write!(out, r#"<g opacity="{opacity}">"#);
    let _ = match &o.shape {
        Shape::Line(a, b) => write!(out, r#"<line x1="{}" y1="{}" x2="{}" y2="{}" {stroke}/>"#, a.x, a.y, b.x, b.y),
        Shape::Arrow(a, b) => arrow(out, *a, *b, width, &stroke, &fill),
        Shape::Rect(area) => write!(out, r#"<rect {} {stroke}/>"#, rect(area)),
        // A plain fill at the opacity of the style (`Style::highlighter`): over a layer of its own
        // a multiply would have no photo to blend with (tried: plain yellow).
        Shape::Highlight(area) => write!(out, r#"<rect {} {fill}/>"#, rect(area)),
        Shape::Text { at, text, size, rot } => self::text(out, *at, text, *size, *rot, &fill),
        // Written into the pixels when saving.
        Shape::Cover(_) => Ok(()),
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

/// A line from `a` to the base of a filled head at `b`; only the head when the line is shorter.
fn arrow(out: &mut String, a: Pt, b: Pt, width: f32, stroke: &str, fill: &str) -> std::fmt::Result {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return Ok(());
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = 4.0 * width + 4.0;
    let (bx, by) = (b.x - ux * head, b.y - uy * head);
    if len > head {
        // A little into the head, so no gap shows between them.
        let (ex, ey) = (b.x - ux * head * 0.8, b.y - uy * head * 0.8);
        write!(out, r#"<line x1="{}" y1="{}" x2="{ex}" y2="{ey}" {stroke}/>"#, a.x, a.y)?;
    }
    let (px, py) = (-uy * head / 2.0, ux * head / 2.0);
    write!(out, r#"<polygon points="{},{} {},{} {},{}" {fill}/>"#, b.x, b.y, bx + px, by + py, bx - px, by - py)
}

/// Centred on `at`, one line under the other, in the system's sans-serif font.
fn text(out: &mut String, at: Pt, text: &str, size: f32, rot: f32, fill: &str) -> std::fmt::Result {
    let lines: Vec<&str> = text.lines().collect();
    let first = at.y - (lines.len().max(1) - 1) as f32 * 0.6 * size;
    write!(out, r#"<text font-family="sans-serif" font-size="{size}" text-anchor="middle" dominant-baseline="central" transform="rotate({rot} {} {})" {fill}>"#, at.x, at.y)?;
    for (i, line) in lines.iter().enumerate() {
        write!(out, r#"<tspan x="{}" y="{}">{}</tspan>"#, at.x, first + i as f32 * 1.2 * size, escaped(line))?;
    }
    out.push_str("</text>");
    Ok(())
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
        Style { color: [255, 0, 0], width: 4.0, opacity: 1.0, dash: Dash::Solid }
    }

    fn area(x: f32, y: f32, w: f32, h: f32) -> Area {
        Area { centre: Pt::new(x, y), w, h, rot: 0.0 }
    }

    /// Each kind of object where it was put, in pixels of the result (the centre of the image is
    /// at 50, 40), and nothing far from it.
    #[test]
    fn each_object_in_its_place() {
        let line = draw(&doc(vec![Obj { shape: Shape::Line(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0)), style: style() }]), &whole(&doc(vec![])));
        assert_eq!((alpha(&line, 50, 40), alpha(&line, 25, 40), alpha(&line, 50, 30), alpha(&line, 10, 40)), (255, 255, 0, 0));

        let arrow = draw(&doc(vec![Obj { shape: Shape::Arrow(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0)), style: style() }]), &whole(&doc(vec![])));
        // The head (20 px long, 20 wide at its base at x 60, the tip at 80), the line 4 wide.
        assert_eq!((alpha(&arrow, 64, 34), alpha(&arrow, 64, 46), alpha(&arrow, 25, 40)), (255, 255, 255));
        assert_eq!((alpha(&arrow, 25, 34), alpha(&arrow, 82, 40)), (0, 0));

        let rect = draw(&doc(vec![Obj { shape: Shape::Rect(area(0.0, 0.0, 40.0, 20.0)), style: style() }]), &whole(&doc(vec![])));
        // Outline only: the sides at x 30 and 70, y 30 and 50.
        assert_eq!((alpha(&rect, 30, 40), alpha(&rect, 69, 40), alpha(&rect, 50, 30), alpha(&rect, 50, 49)), (255, 255, 255, 255));
        assert_eq!((alpha(&rect, 50, 40), alpha(&rect, 20, 40)), (0, 0));

        let text = draw(&doc(vec![Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: "Ab".into(), size: 30.0, rot: 0.0 }, style: style() }]), &whole(&doc(vec![])));
        let (cx, cy) = centroid(&text);
        assert!((cx - 50.0).abs() < 4.0 && (cy - 40.0).abs() < 6.0, "text centred: {cx}, {cy}");
        assert_eq!((alpha(&text, 5, 5), alpha(&text, 95, 75)), (0, 0));

        // Not drawn here: the cover box (written into the pixels when saving).
        assert!(draw(&doc(vec![Obj { shape: Shape::Cover(area(0.0, 0.0, 40.0, 20.0)), style: style() }]), &whole(&doc(vec![]))).pixels().iter().all(|p| p.alpha() == 0));
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
            Obj { shape: Shape::Line(Pt::new(-40.0, -30.0), Pt::new(35.0, 25.0)), style: style() },
            Obj { shape: Shape::Arrow(Pt::new(-40.0, 30.0), Pt::new(30.0, -10.0)), style: Style { dash: Dash::Dashed, width: 3.0, ..style() } },
            Obj { shape: Shape::Rect(Area { rot: 20.0, ..area(10.0, 5.0, 50.0, 30.0) }), style: Style { opacity: 0.5, ..style() } },
            Obj { shape: Shape::Text { at: Pt::new(-10.0, 0.0), text: "àèìòù".into(), size: 24.0, rot: -10.0 }, style: style() },
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
        let line = |style| draw(&doc(vec![Obj { shape: Shape::Line(Pt::new(-45.0, 0.0), Pt::new(45.0, 0.0)), style }]), &whole(&doc(vec![])));
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
        let text = |s: &str| draw(&doc(vec![Obj { shape: Shape::Text { at: Pt::new(0.0, 0.0), text: s.into(), size: 30.0, rot: 0.0 }, style: style() }]), &whole(&doc(vec![])));
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
        let d = doc(vec![Obj { shape: Shape::Arrow(Pt::new(-30.0, 0.0), Pt::new(30.0, 0.0)), style: style() }]);
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
}
