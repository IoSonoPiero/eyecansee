//! CAD drawings (DXF, DWG) read with `acadrust` and seen from above as an SVG picture, which
//! `decode` draws with resvg: the model space with its blocks, layers (hidden ones left out) and
//! colours, on the dark background of CAD programs.
// ponytail: one picture of the whole drawing; zooming in shows its pixels, not new detail

use std::fmt::Write;
use std::io::Cursor;

use acadrust::entities::{Dimension, EntityType};
use acadrust::types::{Color, Vector3};
use acadrust::{CadDocument, DwgReader, DxfReader};

use crate::cad_geom::{self, Affine, P};

const CAD: &[&str] = &["dxf", "dwg"];

/// AutoCAD's model space background, and its colour 7 (white on a dark background).
pub const BACKGROUND: [u8; 3] = [33, 40, 48];
const WHITE: [u8; 3] = [255, 255, 255];
/// Blocks inside blocks; deeper is a loop or a damaged file.
const MAX_DEPTH: u32 = 16;
/// Points of the whole drawing, so that a damaged file cannot fill the memory.
const MAX_POINTS: usize = 20_000_000;

pub fn is_cad(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| CAD.contains(&e.to_lowercase().as_str()))
}

/// What is drawn, in drawing units, Y up.
#[derive(Debug)]
pub enum Item {
    Line { pts: Vec<P>, color: [u8; 3], fill: Option<f32> },
    Text { at: P, height: f64, angle: f64, text: String, color: [u8; 3], anchor: (Anchor, Baseline) },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Baseline {
    Alphabetic,
    Middle,
    Hanging,
}

#[derive(Debug, Default)]
pub struct Drawing {
    pub items: Vec<Item>,
    points: usize,
}

pub fn read(bytes: &[u8], name: &str) -> Result<Drawing, String> {
    let doc = if name.to_lowercase().ends_with(".dwg") {
        DwgReader::from_stream(Cursor::new(bytes)).read().map_err(|e| e.to_string())?
    } else {
        DxfReader::from_reader(Cursor::new(bytes.to_vec())).and_then(|r| r.read()).map_err(|e| e.to_string())?
    };
    let mut d = Drawing::default();
    let mut walk = Walk { doc: &doc, out: &mut d };
    let model: Vec<&EntityType> = doc.model_space_entities().collect();
    let model = if model.is_empty() { unplaced(&doc) } else { model };
    let top = Context { t: Affine::IDENTITY, layer: None, color: WHITE, depth: 0 };
    for e in model {
        walk.entity(e, &top)?;
    }
    if d.items.is_empty() {
        return Err("nothing to draw in the model space".into());
    }
    Ok(d)
}

/// When the model space's list is empty (seen in an R14 DXF, where the drawing was under a
/// record named `*U`): the entities of every block that nothing inserts, outside the paper space.
fn unplaced(doc: &CadDocument) -> Vec<&EntityType> {
    let used: std::collections::HashSet<&str> = doc
        .entities()
        .filter_map(|e| match e {
            EntityType::Insert(i) => Some(i.block_name.as_str()),
            EntityType::Dimension(d) => Some(d.base().block_name.as_str()),
            EntityType::Table(t) => Some(t.block_name.as_str()),
            _ => None,
        })
        .collect();
    let drawn = |owner| {
        doc.block_records.iter().find(|b| b.handle == owner).is_none_or(|b| !used.contains(b.name.as_str()) && !b.name.to_lowercase().starts_with("*paper_space"))
    };
    doc.entities().filter(|e| drawn(e.common().owner_handle)).collect()
}

/// Where the entities of a block go: transform, the insert's layer (for entities on layer 0)
/// and colour (for those "by block").
struct Context<'a> {
    t: Affine,
    layer: Option<&'a str>,
    color: [u8; 3],
    depth: u32,
}

struct Walk<'a> {
    doc: &'a CadDocument,
    out: &'a mut Drawing,
}

fn xy(v: &Vector3) -> P {
    [v.x, v.y]
}

fn n3(v: &Vector3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

impl<'a> Walk<'a> {
    /// The layer an entity is drawn on, if visible: layer 0 inside a block takes the insert's.
    fn layer<'b>(&self, e: &'b EntityType, ctx: &Context<'b>) -> Option<&'b str>
    where
        'a: 'b,
    {
        let own = e.common().layer.as_str();
        let name = if own == "0" { ctx.layer.unwrap_or(own) } else { own };
        match self.doc.layers.get(name) {
            Some(l) if l.is_frozen() || l.is_off() => None,
            _ => Some(name),
        }
    }

    fn color(&self, c: &Color, layer: &str, ctx: &Context) -> [u8; 3] {
        let rgb = match c {
            Color::ByLayer => self.doc.layers.get(layer).and_then(|l| l.color.rgb()),
            Color::ByBlock => Some(ctx.color.into()),
            c => c.rgb(),
        };
        // 7 is black on paper and white on screen: the table gives white.
        rgb.map_or(WHITE, |(r, g, b)| [r, g, b])
    }

    fn line(&mut self, pts: Vec<P>, t: &Affine, color: [u8; 3], fill: Option<f32>) -> Result<(), String> {
        self.out.points += pts.len();
        if self.out.points > MAX_POINTS {
            return Err(format!("more than {MAX_POINTS} points"));
        }
        let pts: Vec<P> = pts.into_iter().map(|p| t.apply(p)).filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
        if pts.len() >= 2 {
            self.out.items.push(Item::Line { pts, color, fill });
        }
        Ok(())
    }

    fn text(&mut self, t: &Affine, l: Label, color: [u8; 3]) {
        let text = l.text.trim().to_string();
        let at = t.apply(l.at);
        let height = l.height * t.length_scale();
        if text.is_empty() || height.is_nan() || height <= 0.0 || !at[0].is_finite() || !at[1].is_finite() {
            return;
        }
        // The text's direction after the transform; a mirrored text stays readable.
        let angle = t.then_inner(&Affine::rotate(l.angle)).angle();
        self.out.items.push(Item::Text { at, height, angle, text, color, anchor: l.anchor });
    }

    fn entity(&mut self, e: &EntityType, ctx: &Context) -> Result<(), String> {
        let common = e.common();
        if common.invisible {
            return Ok(());
        }
        let Some(layer) = self.layer(e, ctx) else { return Ok(()) };
        let color = self.color(&common.color, layer, ctx);
        let t = &ctx.t;
        if let Some((ocs, label)) = label(e) {
            self.text(&t.then_inner(&ocs), label, color);
            return Ok(());
        }
        match e {
            EntityType::Insert(i) => self.insert(i, layer, color, ctx),
            EntityType::Dimension(d) => self.dimension(d, layer, color, ctx),
            EntityType::Table(tb) if ctx.depth < MAX_DEPTH => {
                // A table is drawn by its block, placed like an insert.
                let dir = tb.horizontal_direction.y.atan2(tb.horizontal_direction.x);
                let local = Affine::translate(tb.insertion_point.x, tb.insertion_point.y).then_inner(&Affine::rotate(dir));
                let inner = Context { t: t.then_inner(&local), layer: Some(layer), color, depth: ctx.depth + 1 };
                self.block(&tb.block_name, &inner)
            }
            EntityType::Hatch(h) => self.hatch(h, t, color),
            e => match shape(e) {
                Some((ocs, pts, fill)) => self.line(pts, &t.then_inner(&ocs), color, fill),
                None => Ok(()),
            },
        }
    }

    fn insert(&mut self, i: &acadrust::entities::Insert, layer: &str, color: [u8; 3], ctx: &Context) -> Result<(), String> {
        if ctx.depth >= MAX_DEPTH {
            return Ok(());
        }
        let base = self.base_point(&i.block_name);
        let ocs = Affine::ocs(n3(&i.normal));
        let (cols, rows) = (i.column_count.max(1), i.row_count.max(1));
        for r in 0..rows {
            for c in 0..cols {
                // Array of inserts: steps along the insert's own axes, before the rotation.
                let step = Affine::rotate(i.rotation).apply([c as f64 * i.column_spacing, r as f64 * i.row_spacing]);
                let local = Affine::translate(i.insert_point.x + step[0], i.insert_point.y + step[1])
                    .then_inner(&Affine::rotate(i.rotation))
                    .then_inner(&Affine::scale(i.x_scale(), i.y_scale()))
                    .then_inner(&Affine::translate(-base[0], -base[1]));
                let inner = Context { t: ctx.t.then_inner(&ocs).then_inner(&local), layer: Some(layer), color, depth: ctx.depth + 1 };
                self.block(&i.block_name, &inner)?;
            }
        }
        // Attributes are already placed in the drawing (only the block's own transform applies).
        for a in i.attributes.iter().filter(|a| !a.flags.invisible && !a.common.invisible) {
            let color = self.color(&a.common.color, layer, &Context { t: ctx.t, layer: Some(layer), color, depth: ctx.depth });
            let o = ctx.t.then_inner(&Affine::ocs(n3(&a.normal)));
            let l = Label { at: xy(&a.insertion_point), height: a.height, angle: a.rotation, text: a.value.clone(), anchor: (Anchor::Start, Baseline::Alphabetic) };
            self.text(&o, l, color);
        }
        Ok(())
    }

    fn block(&mut self, name: &str, ctx: &Context) -> Result<(), String> {
        let doc = self.doc;
        for e in doc.entities_in_block(name) {
            self.entity(e, ctx)?;
        }
        Ok(())
    }

    fn base_point(&self, name: &str) -> P {
        let record = self.doc.block_records.get(name);
        match record.and_then(|r| self.doc.get_entity(r.block_entity_handle)) {
            Some(EntityType::Block(b)) => xy(&b.base_point),
            _ => [0.0, 0.0],
        }
    }

    /// A dimension is drawn by its block (lines, arrows, text), already placed in the drawing.
    fn dimension(&mut self, d: &Dimension, layer: &str, color: [u8; 3], ctx: &Context) -> Result<(), String> {
        let base = d.base();
        if !base.block_name.is_empty() && self.doc.block_records.get(&base.block_name).is_some() && ctx.depth < MAX_DEPTH {
            let inner = Context { t: ctx.t, layer: Some(layer), color, depth: ctx.depth + 1 };
            return self.block(&base.block_name, &inner);
        }
        let text = base.user_text.clone().filter(|t| !t.is_empty() && t != "<>").unwrap_or_else(|| format!("{:.2}", base.actual_measurement));
        let l = Label { at: xy(&base.text_middle_point), height: 2.5, angle: base.text_rotation, text, anchor: (Anchor::Middle, Baseline::Middle) };
        self.text(&ctx.t, l, color);
        Ok(())
    }

    /// Solid hatches filled; the others filled lightly (their pattern lines are not drawn).
    // ponytail: no hatch patterns; a translucent fill shows where they are
    fn hatch(&mut self, h: &acadrust::entities::Hatch, t: &Affine, color: [u8; 3]) -> Result<(), String> {
        use acadrust::entities::BoundaryEdge as E;
        let o = t.then_inner(&Affine::ocs(n3(&h.normal)));
        let fill = Some(if h.is_solid { 1.0 } else { 0.25 });
        for path in &h.paths {
            let mut pts: Vec<P> = Vec::new();
            for edge in &path.edges {
                let part = match edge {
                    E::Line(l) => vec![[l.start.x, l.start.y], [l.end.x, l.end.y]],
                    E::CircularArc(a) => {
                        let c = [a.center.x, a.center.y];
                        if a.counter_clockwise {
                            cad_geom::arc(c, a.radius, a.start_angle, a.end_angle)
                        } else {
                            // Clockwise arcs store their angles measured clockwise.
                            let mut p = cad_geom::arc(c, a.radius, -a.end_angle, -a.start_angle);
                            p.reverse();
                            p
                        }
                    }
                    E::EllipticArc(e) => {
                        let mut p = cad_geom::ellipse([e.center.x, e.center.y], [e.major_axis_endpoint.x, e.major_axis_endpoint.y], e.minor_axis_ratio, e.start_angle, e.end_angle);
                        if !e.counter_clockwise {
                            p.reverse();
                        }
                        p
                    }
                    E::Spline(s) => {
                        let control: Vec<P> = s.control_points.iter().map(xy).collect();
                        cad_geom::spline(s.degree.max(0) as usize, &s.knots, &control, &[]).unwrap_or(control)
                    }
                    E::Polyline(p) => {
                        let v: Vec<(P, f64)> = p.vertices.iter().map(|v| ([v.x, v.y], v.z)).collect();
                        cad_geom::bulged(&v, p.is_closed)
                    }
                };
                pts.extend(part);
            }
            if let Some(&first) = pts.first() {
                pts.push(first);
            }
            self.line(pts, &o, color, fill)?;
        }
        Ok(())
    }
}

/// A text to place: position, height and angle in its own coordinates.
struct Label {
    at: P,
    height: f64,
    angle: f64,
    text: String,
    anchor: (Anchor, Baseline),
}

/// Texts (TEXT, MTEXT) with their coordinate system.
fn label(e: &EntityType) -> Option<(Affine, Label)> {
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
            let l = Label { at: xy(at), height: x.height, angle: x.rotation, text: x.value.clone(), anchor: (h, v) };
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

/// Lines and filled outlines, in their coordinate system: points and the fill's opacity.
fn shape(e: &EntityType) -> Option<(Affine, Vec<P>, Option<f32>)> {
    let ocs = |n: &Vector3| Affine::ocs(n3(n));
    let closed = |mut pts: Vec<P>, close: bool| {
        if close && let Some(&first) = pts.first() {
            pts.push(first);
        }
        pts
    };
    Some(match e {
        EntityType::Line(l) => (Affine::IDENTITY, vec![xy(&l.start), xy(&l.end)], None),
        EntityType::Circle(c) => (ocs(&c.normal), cad_geom::circle(xy(&c.center), c.radius), None),
        EntityType::Arc(a) => (ocs(&a.normal), cad_geom::arc(xy(&a.center), a.radius, a.start_angle, a.end_angle), None),
        EntityType::Ellipse(el) => {
            let pts = cad_geom::ellipse(xy(&el.center), xy(&el.major_axis), el.minor_axis_ratio, el.start_parameter, el.end_parameter);
            // The centre and axis are in world coordinates; a normal pointing down turns the parameter the other way.
            let pts = if el.normal.z < 0.0 { mirror_about(pts, xy(&el.center), xy(&el.major_axis)) } else { pts };
            (Affine::IDENTITY, pts, None)
        }
        EntityType::LwPolyline(p) => {
            let v: Vec<(P, f64)> = p.vertices.iter().map(|v| ([v.location.x, v.location.y], v.bulge)).collect();
            (ocs(&p.normal), cad_geom::bulged(&v, p.is_closed), None)
        }
        EntityType::Polyline2D(p) => {
            let v: Vec<(P, f64)> = p.vertices.iter().map(|v| (xy(&v.location), v.bulge)).collect();
            (ocs(&p.normal), cad_geom::bulged(&v, p.is_closed()), None)
        }
        EntityType::Polyline3D(p) => (Affine::IDENTITY, closed(p.vertices.iter().map(|v| xy(&v.position)).collect(), p.flags.closed), None),
        EntityType::Polyline(p) => (Affine::IDENTITY, closed(p.vertices.iter().map(|v| xy(&v.location)).collect(), p.flags.bits() & 1 != 0), None),
        EntityType::Spline(s) => {
            let control: Vec<P> = s.control_points.iter().map(xy).collect();
            let pts = cad_geom::spline(s.degree.max(0) as usize, &s.knots, &control, &s.weights).unwrap_or_else(|| s.fit_points.iter().map(xy).collect());
            (Affine::IDENTITY, pts, None)
        }
        // Corners in the order 1 2 4 3 make the outline.
        EntityType::Solid(s) => (ocs(&s.normal), vec![xy(&s.first_corner), xy(&s.second_corner), xy(&s.fourth_corner), xy(&s.third_corner), xy(&s.first_corner)], Some(1.0)),
        EntityType::Face3D(f) => (Affine::IDENTITY, vec![xy(&f.first_corner), xy(&f.second_corner), xy(&f.third_corner), xy(&f.fourth_corner), xy(&f.first_corner)], None),
        EntityType::Leader(l) => (Affine::IDENTITY, l.vertices.iter().map(xy).collect(), None),
        // The picture is another file: its frame with a cross.
        EntityType::RasterImage(r) => {
            let c = r.corners().map(|v| xy(&v));
            (Affine::IDENTITY, vec![c[0], c[1], c[2], c[3], c[0], c[2], c[3], c[1]], None)
        }
        _ => return None,
    })
}

/// An ellipse drawn with its normal pointing down: mirrored across its major axis.
fn mirror_about(pts: Vec<P>, c: P, major: P) -> Vec<P> {
    let len = major[0].hypot(major[1]);
    if len == 0.0 {
        return pts;
    }
    let (ux, uy) = (major[0] / len, major[1] / len);
    pts.into_iter()
        .map(|p| {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            let along = dx * ux + dy * uy;
            [c[0] + 2.0 * along * ux - dx, c[1] + 2.0 * along * uy - dy]
        })
        .collect()
}

impl Drawing {
    /// Box around what is drawn: lines, and the text positions with an estimate of their size.
    fn bounds(&self) -> Option<[f64; 4]> {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let mut add = |p: P| {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        };
        for item in &self.items {
            match item {
                Item::Line { pts, .. } => pts.iter().for_each(|&p| add(p)),
                Item::Text { at, height, text, .. } => {
                    add(*at);
                    let w = 0.6 * height * text.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
                    add([at[0] + w, at[1] + height]);
                }
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
                Item::Line { pts, color, fill } => {
                    let c = rgb(*color);
                    let _ = write!(out, r#"<path d="M"#);
                    for (i, p) in pts.iter().enumerate() {
                        let (x, y) = px(*p);
                        let _ = write!(out, "{}{x:.2} {y:.2}", if i == 0 { "" } else { "L" });
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

#[cfg(test)]
mod tests {
    use super::*;

    const DXF: &str = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n62\n1\n10\n0\n20\n0\n30\n0\n11\n100\n21\n50\n31\n0\n0\nCIRCLE\n8\n0\n10\n50\n20\n25\n30\n0\n40\n10\n0\nTEXT\n8\n0\n10\n5\n20\n5\n30\n0\n40\n3\n1\nA & B\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn a_minimal_dxf_becomes_an_svg() {
        let d = read(DXF.as_bytes(), "x.DXF").unwrap();
        assert_eq!(d.items.len(), 3);
        // Colour 1 is red, the circle on layer 0 without a layer table is white.
        assert!(matches!(&d.items[0], Item::Line { color: [255, 0, 0], .. }));
        assert!(matches!(&d.items[1], Item::Line { color: WHITE, .. }));
        let svg = d.svg(1000, 1.0).unwrap();
        assert!(svg.starts_with("<svg") && svg.contains("A &amp; B") && svg.contains(r#"width="1000""#));
        assert!(is_cad("a.dwg") && is_cad("b.DXF") && !is_cad("c.dwf"));
    }

    /// Every drawing under `EYECANSEE_CAD` read and drawn into `EYECANSEE_CAD_OUT` (PNG), with times.
    #[test]
    #[ignore]
    fn cad_live() {
        let (Ok(dir), Ok(out)) = (std::env::var("EYECANSEE_CAD"), std::env::var("EYECANSEE_CAD_OUT")) else { return };
        let mut failed = Vec::new();
        for sub in std::fs::read_dir(&dir).unwrap().flatten().filter(|s| s.path().is_dir()) {
            for f in std::fs::read_dir(sub.path()).unwrap().flatten() {
                let name = f.file_name().to_string_lossy().to_string();
                if !is_cad(&name) {
                    continue;
                }
                let bytes = std::fs::read(f.path()).unwrap();
                let t = std::time::Instant::now();
                match std::panic::catch_unwind(|| read(&bytes, &name)) {
                    Ok(Ok(d)) => {
                        let read = t.elapsed();
                        let t = std::time::Instant::now();
                        let img = crate::decode::cad_picture(&bytes, &name, 1600);
                        println!("{name}: {} items, read {read:.0?}, read and drawn {:.0?}", d.items.len(), t.elapsed());
                        img.unwrap().save(format!("{out}/{name}.png")).unwrap();
                    }
                    Ok(Err(e)) => {
                        println!("{name}: ERROR {e}");
                        failed.push(name);
                    }
                    Err(_) => {
                        println!("{name}: PANIC");
                        failed.push(name);
                    }
                }
            }
        }
        println!("failed: {failed:?}");
    }

    #[test]
    fn a_part_drawn_larger_keeps_thin_lines() {
        let d = read(DXF.as_bytes(), "x.dxf").unwrap();
        let full = crate::decode::cad_render(&d, 400, None).unwrap();
        // The left half, twice as large: lines still about 1 pixel, not 2.
        let (w, h) = full.dimensions();
        let part = crate::decode::cad_render(&d, 400, Some(([0.0, 0.0, w as f32 / 2.0, h as f32], [w, h * 2]))).unwrap();
        assert_eq!(part.dimensions(), (w, h * 2));
        let red = |img: &image::RgbaImage, x: u32| (0..img.height()).filter(|&y| img.get_pixel(x, y)[0] > 150 && img.get_pixel(x, y)[1] < 100).count();
        // The red line crosses each column once: a few pixels, in both pictures.
        assert!((1..=4).contains(&red(&full, w / 4)) && (1..=4).contains(&red(&part, w / 2)));
    }

    #[test]
    fn damaged_files_are_errors() {
        assert!(read(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n", "e.dxf").is_err());
        assert!(read(b"AC1032 not really", "x.dwg").is_err());
        assert!(read(&[0u8; 64], "x.dxf").is_err());
    }
}
