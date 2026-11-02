//! CAD drawings (DXF, DWG) read with `acadrust` and seen from above as an SVG picture, which
//! `decode` draws with resvg: the model space with its blocks, layers (hidden ones left out) and
//! colours, line types and weights, hatch patterns, on the dark background of CAD programs.
// ponytail: one picture of the whole drawing; zooming in shows its pixels, not new detail

use std::io::Cursor;

use acadrust::entities::{Dimension, EntityType};
use acadrust::types::{Color, Vector3};
use acadrust::{CadDocument, DwgReader, DxfReader};

use crate::cad_geom::{self, Affine, P};
use crate::cad_text::{Label, gdt, label};

pub(crate) const CAD: &[&str] = &["dxf", "dwg", "plt", "hpgl", "hpg", "hgl"];

/// AutoCAD's model space background, and its colour 7 (white on a dark background).
pub const BACKGROUND: [u8; 3] = [33, 40, 48];
pub(crate) const WHITE: [u8; 3] = [255, 255, 255];
/// Blocks inside blocks; deeper is a loop or a damaged file.
const MAX_DEPTH: u32 = 16;
/// Points of the whole drawing, so that a damaged file cannot fill the memory.
pub(crate) const MAX_POINTS: usize = 20_000_000;
/// Pieces of pattern lines in one hatch: beyond, it is too dense to see and is filled lightly.
const MAX_PATTERN: usize = 200_000;

pub fn is_cad(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, e)| CAD.contains(&e.to_lowercase().as_str()))
}

/// What is drawn, in drawing units, Y up.
#[derive(Debug)]
pub enum Item {
    Line { pts: Vec<P>, color: [u8; 3], fill: Option<f32>, stroke: Stroke },
    Text { at: P, height: f64, angle: f64, text: String, color: [u8; 3], anchor: (Anchor, Baseline) },
    /// A picture attached to the drawing: its corners (bottom-left, bottom-right, top-right,
    /// top-left) and the PNG in base64.
    Image { corners: [P; 4], png: String },
}

/// How a line is drawn: its dashes in drawing units (positive dash, negative gap, zero dot;
/// empty = continuous) and its weight in millimetres.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stroke {
    pub dash: Vec<f64>,
    pub weight: f32,
}

/// Between two outlines of one filled area (`Walk::area`): the SVG starts a new outline there.
const GAP: P = [f64::NAN, f64::NAN];

/// A colour too dark to see on the drawing's background (black given as RGB, black pens) is
/// drawn white, as colour 7.
pub(crate) fn visible(c: [u8; 3]) -> [u8; 3] {
    let luma = 0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32;
    if luma < 40.0 { WHITE } else { c }
}

/// AutoCAD's default line weight (LWDEFAULT), drawn one pixel wide.
pub(crate) const THIN: f32 = 0.25;

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
    /// The layouts (paper space) with something on them, by name, in their tab order.
    pub sheets: Vec<(String, std::sync::Arc<Drawing>)>,
    pub(crate) points: usize,
}

pub fn read(bytes: &[u8], name: &str) -> Result<Drawing, String> {
    read_with(bytes, name, &crate::cad_image::none)
}

/// Like `read`, with the pictures attached to the drawing read through `sibling`.
pub fn read_with(bytes: &[u8], name: &str, sibling: crate::model::Sibling) -> Result<Drawing, String> {
    let lower = name.to_lowercase();
    if [".plt", ".hpgl", ".hpg", ".hgl"].iter().any(|e| lower.ends_with(e)) {
        return crate::cad_hpgl::read(bytes);
    }
    let doc = if lower.ends_with(".dwg") {
        DwgReader::from_stream(Cursor::new(bytes)).read().map_err(|e| e.to_string())?
    } else {
        DxfReader::from_reader(Cursor::new(bytes.to_vec())).and_then(|r| r.read()).map_err(|e| e.to_string())?
    };
    let mut d = Drawing::default();
    let fonts = std::rc::Rc::new(crate::cad_text::fonts(&doc, sibling));
    let mut walk = Walk::new(&doc, &mut d);
    walk.sibling = sibling;
    walk.fonts = fonts.clone();
    let model: Vec<&EntityType> = doc.model_space_entities().collect();
    let model = if model.is_empty() { unplaced(&doc) } else { model };
    let top = Context { t: Affine::IDENTITY, layer: None, color: WHITE, style: CONTINUOUS, depth: 0 };
    for e in model {
        walk.entity(e, &top)?;
    }
    walk.finish()?;
    if d.items.is_empty() {
        return Err("nothing to draw in the model space".into());
    }
    d.sheets = crate::cad_layout::sheets(&doc, sibling, &fonts);
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

/// Where the entities of a block go: transform, the insert's layer (for entities on layer 0),
/// colour, line type and weight (for those "by block").
pub(crate) struct Context<'a> {
    pub(crate) t: Affine,
    pub(crate) layer: Option<&'a str>,
    pub(crate) color: [u8; 3],
    pub(crate) style: Style<'a>,
    pub(crate) depth: u32,
}

/// A line type by name, its scale, and a weight in millimetres.
#[derive(Clone, Copy)]
pub(crate) struct Style<'a> {
    linetype: &'a str,
    scale: f64,
    weight: f32,
}

pub(crate) const CONTINUOUS: Style = Style { linetype: "Continuous", scale: 1.0, weight: THIN };

pub(crate) struct Walk<'a> {
    doc: &'a CadDocument,
    out: &'a mut Drawing,
    /// Inside a layout's viewport: its box on the sheet, and the layers frozen in it (lower case).
    pub(crate) clip: Option<[f64; 4]>,
    pub(crate) frozen: Vec<String>,
    /// The files next to the drawing, for its pictures.
    pub(crate) sibling: crate::model::Sibling<'a>,
    /// Lines without end (XLINE) or with one (RAY), drawn across the drawing once its size is known.
    infinite: Vec<Infinite>,
    /// The SHX fonts of the text styles (`cad_text::fonts`).
    pub(crate) fonts: std::rc::Rc<std::collections::HashMap<String, crate::cad_text::Font>>,
}

/// Where an XLINE or RAY passes, in drawing units, and how it is drawn.
struct Infinite {
    base: P,
    dir: P,
    ray: bool,
    color: [u8; 3],
    stroke: Stroke,
}

pub(crate) fn xy(v: &Vector3) -> P {
    [v.x, v.y]
}

pub(crate) fn n3(v: &Vector3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

impl<'a> Walk<'a> {
    pub(crate) fn new(doc: &'a CadDocument, out: &'a mut Drawing) -> Self {
        Walk { doc, out, clip: None, frozen: Vec::new(), sibling: &crate::cad_image::none, infinite: Vec::new(), fonts: Default::default() }
    }

    /// The layer an entity is drawn on, if visible: layer 0 inside a block takes the insert's.
    fn layer<'b>(&self, e: &'b EntityType, ctx: &Context<'b>) -> Option<&'b str>
    where
        'a: 'b,
    {
        let own = e.common().layer.as_str();
        let name = if own == "0" { ctx.layer.unwrap_or(own) } else { own };
        match self.doc.layers.get(name) {
            Some(l) if l.is_frozen() || l.is_off() => None,
            _ if self.frozen.iter().any(|f| f.eq_ignore_ascii_case(name)) => None,
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
        visible(rgb.map_or(WHITE, |(r, g, b)| [r, g, b]))
    }

    /// An entity's line type and weight, "by layer" and "by block" resolved.
    fn style<'b>(&self, c: &'b acadrust::entities::EntityCommon, layer: &str, ctx: &Context<'b>) -> Style<'b>
    where
        'a: 'b,
    {
        use acadrust::types::LineWeight as W;
        let l = self.doc.layers.get(layer);
        let linetype = match c.linetype.as_str() {
            "" => l.map_or("Continuous", |l| l.line_type.as_str()),
            n if n.eq_ignore_ascii_case("ByLayer") => l.map_or("Continuous", |l| l.line_type.as_str()),
            n if n.eq_ignore_ascii_case("ByBlock") => ctx.style.linetype,
            n => n,
        };
        let mm = |w: &W| match w {
            W::Value(v) if *v >= 0 => Some(f32::from(*v) / 100.0),
            W::ByBlock => Some(ctx.style.weight),
            _ => None,
        };
        let weight = match c.line_weight {
            W::ByLayer => l.and_then(|l| mm(&l.line_weight)),
            ref w => mm(w),
        };
        let scale = if c.linetype_scale.is_finite() && c.linetype_scale > 0.0 { c.linetype_scale } else { 1.0 };
        Style { linetype, scale: if linetype.eq_ignore_ascii_case("ByBlock") { ctx.style.scale } else { scale }, weight: weight.unwrap_or(THIN) }
    }

    /// The dashes of a line type at the drawing's scale (LTSCALE) and the block's.
    fn stroke(&self, style: Style, t: &Affine) -> Stroke {
        let global = self.doc.header.linetype_scale;
        let global = if global.is_finite() && global > 0.0 { global } else { 1.0 };
        let dash: Vec<f64> = self.doc.line_types.get(style.linetype).map(|lt| lt.elements.iter().map(|e| e.length * global * style.scale * t.length_scale()).collect()).unwrap_or_default();
        // Without a gap it is a continuous line.
        let dash = if dash.iter().any(|&v| v < 0.0) && dash.iter().all(|v| v.is_finite()) { dash } else { Vec::new() };
        Stroke { dash, weight: style.weight }
    }

    pub(crate) fn line(&mut self, pts: Vec<P>, t: &Affine, color: [u8; 3], fill: Option<f32>, stroke: Stroke) -> Result<(), String> {
        self.out.points += pts.len();
        if self.out.points > MAX_POINTS {
            return Err(format!("more than {MAX_POINTS} points"));
        }
        let pts: Vec<P> = pts.into_iter().map(|p| t.apply(p)).filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
        let pieces = match (self.clip, fill) {
            (None, _) => vec![pts],
            (Some(b), Some(_)) => vec![cad_geom::clip_polygon(&pts, b)],
            (Some(b), None) => cad_geom::clip_polyline(&pts, b),
        };
        for pts in pieces.into_iter().filter(|p| p.len() >= 2) {
            self.out.items.push(Item::Line { pts, color, fill, stroke: stroke.clone() });
        }
        Ok(())
    }

    fn text(&mut self, t: &Affine, l: Label, color: [u8; 3]) {
        let text = l.text.trim().to_string();
        let at = t.apply(l.at);
        let height = l.height * t.length_scale();
        let outside = self.clip.is_some_and(|b| at[0] < b[0] || at[0] > b[2] || at[1] < b[1] || at[1] > b[3]);
        if text.is_empty() || height.is_nan() || height <= 0.0 || !at[0].is_finite() || !at[1].is_finite() || outside {
            return;
        }
        // The text's direction after the transform; a mirrored text stays readable.
        let angle = t.then_inner(&Affine::rotate(l.angle)).angle();
        let style = if l.style.is_empty() { "standard".to_string() } else { l.style.to_ascii_lowercase() };
        if let Some(font) = self.fonts.get(&style) {
            let stroke = Stroke { dash: Vec::new(), weight: THIN };
            for pts in crate::cad_text::strokes(font, at, height, angle, &text, l.anchor) {
                let _ = self.line(pts, &Affine::IDENTITY, color, None, stroke.clone());
            }
            return;
        }
        self.out.items.push(Item::Text { at, height, angle, text, color, anchor: l.anchor });
    }

    pub(crate) fn entity(&mut self, e: &EntityType, ctx: &Context) -> Result<(), String> {
        let common = e.common();
        if common.invisible {
            return Ok(());
        }
        let Some(layer) = self.layer(e, ctx) else { return Ok(()) };
        let color = self.color(&common.color, layer, ctx);
        let style = self.style(common, layer, ctx);
        let t = &ctx.t;
        if let Some((ocs, label)) = label(e) {
            self.text(&t.then_inner(&ocs), label, color);
            return Ok(());
        }
        match e {
            EntityType::Insert(i) => self.insert(i, layer, color, style, ctx),
            EntityType::Dimension(d) => self.dimension(d, layer, color, ctx),
            EntityType::Table(tb) if ctx.depth < MAX_DEPTH => {
                // A table is drawn by its block, placed like an insert.
                let dir = tb.horizontal_direction.y.atan2(tb.horizontal_direction.x);
                let local = Affine::translate(tb.insertion_point.x, tb.insertion_point.y).then_inner(&Affine::rotate(dir));
                if self.doc.block_records.get(&tb.block_name).is_none() {
                    return self.table_grid(tb, &t.then_inner(&local), color);
                }
                let inner = Context { t: t.then_inner(&local), layer: Some(layer), color, style, depth: ctx.depth + 1 };
                self.block(&tb.block_name, &inner)
            }
            EntityType::Hatch(h) => self.hatch(h, t, color, style.weight),
            EntityType::Tolerance(x) => {
                self.tolerance(x, t, color);
                Ok(())
            }
            EntityType::MLine(m) => {
                let stroke = self.stroke(style, t);
                self.mline(m, t, color, stroke)
            }
            EntityType::XLine(_) | EntityType::Ray(_) => {
                let (base, dir, ray) = match e {
                    EntityType::XLine(x) => (xy(&x.base_point), xy(&x.direction), false),
                    EntityType::Ray(r) => (xy(&r.base_point), xy(&r.direction), true),
                    _ => unreachable!(),
                };
                let stroke = self.stroke(style, t);
                let (b, d) = (t.apply(base), t.apply([base[0] + dir[0], base[1] + dir[1]]));
                let inf = Infinite { base: b, dir: [d[0] - b[0], d[1] - b[1]], ray, color, stroke };
                // In a viewport its frame is the limit; elsewhere the drawing's size, at the end.
                match self.clip {
                    Some(c) => self.draw_infinite(&inf, c),
                    None => {
                        self.infinite.push(inf);
                        Ok(())
                    }
                }
            }
            // Out of a layout's viewports, cutting a picture is not done: then its frame.
            EntityType::RasterImage(r) if self.clip.is_none() && let Some(png) = crate::cad_image::picture(self.doc, r, self.sibling) => {
                let corners = crate::cad_image::corners(r).map(|p| t.apply(p));
                self.out.points += 4;
                self.out.items.push(Item::Image { corners, png });
                Ok(())
            }
            e => match shape(e) {
                Some((ocs, pts, fill)) => {
                    let stroke = self.stroke(style, t);
                    self.line(pts, &t.then_inner(&ocs), color, fill, stroke)
                }
                None => Ok(()),
            },
        }
    }

    fn insert(&mut self, i: &acadrust::entities::Insert, layer: &str, color: [u8; 3], style: Style, ctx: &Context) -> Result<(), String> {
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
                let inner = Context { t: ctx.t.then_inner(&ocs).then_inner(&local), layer: Some(layer), color, style, depth: ctx.depth + 1 };
                self.block(&i.block_name, &inner)?;
            }
        }
        // Attributes are already placed in the drawing (only the block's own transform applies).
        for a in i.attributes.iter().filter(|a| !a.flags.invisible && !a.common.invisible) {
            let color = self.color(&a.common.color, layer, &Context { t: ctx.t, layer: Some(layer), color, style, depth: ctx.depth });
            let o = ctx.t.then_inner(&Affine::ocs(n3(&a.normal)));
            let l = Label { at: xy(&a.insertion_point), height: a.height, angle: a.rotation, text: a.value.clone(), anchor: (Anchor::Start, Baseline::Alphabetic), style: a.text_style.clone() };
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
            let inner = Context { t: ctx.t, layer: Some(layer), color, style: ctx.style, depth: ctx.depth + 1 };
            return self.block(&base.block_name, &inner);
        }
        let text = base.user_text.clone().filter(|t| !t.is_empty() && t != "<>").unwrap_or_else(|| format!("{:.2}", base.actual_measurement));
        let l = Label { at: xy(&base.text_middle_point), height: 2.5, angle: base.text_rotation, text, anchor: (Anchor::Middle, Baseline::Middle), style: String::new() };
        self.text(&ctx.t, l, color);
        Ok(())
    }

    /// Solid hatches filled; the others drawn with their pattern lines inside the boundaries,
    /// or filled lightly when the pattern is missing or too dense to draw.
    fn hatch(&mut self, h: &acadrust::entities::Hatch, t: &Affine, color: [u8; 3], weight: f32) -> Result<(), String> {
        let o = t.then_inner(&Affine::ocs(n3(&h.normal)));
        let bounds: Vec<Vec<P>> = h.paths.iter().map(boundary).filter(|b| b.len() >= 3).collect();
        let thin = Stroke { dash: Vec::new(), weight };
        if !h.is_solid && !h.pattern.lines.is_empty() {
            let budget = MAX_PATTERN.min(MAX_POINTS.saturating_sub(self.out.points) / 2);
            let lines: Option<Vec<[P; 2]>> = h.pattern.lines.iter().try_fold(Vec::new(), |mut all, l| {
                let family = cad_geom::PatternLine { angle: l.angle, base: [l.base_point.x, l.base_point.y], offset: [l.offset.x, l.offset.y], dashes: &l.dash_lengths };
                all.extend(cad_geom::hatch_lines(&bounds, &family, budget.saturating_sub(all.len()))?);
                Some(all)
            });
            if let Some(lines) = lines {
                for [a, b] in lines {
                    self.line(vec![a, b], &o, color, None, thin.clone())?;
                }
                return Ok(());
            }
        }
        // One area with every boundary, filled even-odd: the islands stay empty.
        let fill = Some(if h.is_solid { 1.0 } else { 0.25 });
        self.area(bounds, &o, color, fill, thin)
    }

    /// A table without its block: the grid of its rows (going down) and columns, and the text of
    /// each cell in its middle.
    // ponytail: merged cells keep their inner lines
    fn table_grid(&mut self, tb: &acadrust::entities::Table, t: &Affine, color: [u8; 3]) -> Result<(), String> {
        let xs: Vec<f64> = std::iter::once(0.0).chain(tb.columns.iter().scan(0.0, |x, c| { *x += c.width; Some(*x) })).collect();
        let ys: Vec<f64> = std::iter::once(0.0).chain(tb.rows.iter().scan(0.0, |y, r| { *y -= r.height; Some(*y) })).collect();
        let (w, h) = (*xs.last().unwrap_or(&0.0), *ys.last().unwrap_or(&0.0));
        for &y in &ys {
            self.line(vec![[0.0, y], [w, y]], t, color, None, Stroke::default())?;
        }
        for &x in &xs {
            self.line(vec![[x, 0.0], [x, h]], t, color, None, Stroke::default())?;
        }
        for (r, row) in tb.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate().take(tb.columns.len()) {
                let text = acadrust::entities::mtext_format::parse_mtext(cell.text_value(), true).to_plain_text();
                let size = cell.contents.first().map(|c| c.text_height).filter(|h| *h > 0.0).unwrap_or(row.height * 0.4);
                let at = [(xs[c] + xs[c + 1]) / 2.0, (ys[r] + ys[r + 1]) / 2.0];
                self.text(t, Label { at, height: size, angle: 0.0, text, anchor: (Anchor::Middle, Baseline::Middle), style: String::new() }, color);
            }
        }
        Ok(())
    }

    /// A feature control frame: its rows (^J) with their cells (%%v) in boxes, the GD&T symbols
    /// of the gdt font as their Unicode signs.
    fn tolerance(&mut self, x: &acadrust::entities::Tolerance, t: &Affine, color: [u8; 3]) {
        let h = if x.text_height > 0.0 { x.text_height } else { 2.5 };
        let angle = x.direction.y.atan2(x.direction.x);
        let o = t.then_inner(&Affine::ocs(n3(&x.normal)));
        for (row, line) in x.text.split("^J").enumerate() {
            let text = gdt(line);
            let at = [x.insertion_point.x, x.insertion_point.y - row as f64 * 2.0 * h];
            let frame = crate::cad_svg::text_box(at, h, angle, &text, (Anchor::Start, Baseline::Middle));
            let _ = self.line(vec![frame[0], frame[1], frame[2], frame[3], frame[0]], &o, color, None, Stroke::default());
            self.text(&o, Label { at, height: h, angle, text, anchor: (Anchor::Start, Baseline::Middle), style: String::new() }, color);
        }
    }

    /// A multiline: each of its lines through the vertices, at its distance along the miters.
    fn mline(&mut self, m: &acadrust::entities::MLine, t: &Affine, color: [u8; 3], stroke: Stroke) -> Result<(), String> {
        let lines = m.vertices.iter().map(|v| v.segments.len()).max().unwrap_or(0);
        for k in 0..lines {
            let mut pts: Vec<P> = m
                .vertices
                .iter()
                .filter_map(|v| {
                    let d = *v.segments.get(k)?.parameters.first()?;
                    Some([v.position.x + v.miter.x * d, v.position.y + v.miter.y * d])
                })
                .collect();
            if m.flags.contains(acadrust::entities::MLineFlags::CLOSED)
                && let Some(&first) = pts.first()
            {
                pts.push(first);
            }
            self.line(pts, t, color, None, stroke.clone())?;
        }
        Ok(())
    }

    /// An XLINE or RAY across the box `b` (x0, y0, x1, y1), as far as it reaches.
    fn draw_infinite(&mut self, l: &Infinite, b: [f64; 4]) -> Result<(), String> {
        let len = l.dir[0].hypot(l.dir[1]);
        if len == 0.0 || !len.is_finite() {
            return Ok(());
        }
        let d = [l.dir[0] / len, l.dir[1] / len];
        // Far enough to leave the box from wherever the line starts.
        let reach = (b[2] - b[0]).hypot(b[3] - b[1]) + (l.base[0] - (b[0] + b[2]) / 2.0).hypot(l.base[1] - (b[1] + b[3]) / 2.0);
        let from = if l.ray { 0.0 } else { -reach };
        let pts = vec![[l.base[0] + d[0] * from, l.base[1] + d[1] * from], [l.base[0] + d[0] * reach, l.base[1] + d[1] * reach]];
        for piece in cad_geom::clip_polyline(&pts, b).into_iter().filter(|p| p.len() >= 2) {
            self.out.items.push(Item::Line { pts: piece, color: l.color, fill: None, stroke: l.stroke.clone() });
        }
        Ok(())
    }

    /// The XLINEs and RAYs met outside viewports, across the drawing's box (a little larger);
    /// a drawing made only of them gets a box around their points.
    pub(crate) fn finish(mut self) -> Result<(), String> {
        let lines = std::mem::take(&mut self.infinite);
        if lines.is_empty() {
            return Ok(());
        }
        let b = self.out.bounds().unwrap_or_else(|| {
            let (xs, ys): (Vec<f64>, Vec<f64>) = lines.iter().map(|l| (l.base[0], l.base[1])).unzip();
            let lo = [xs.iter().copied().fold(f64::MAX, f64::min), ys.iter().copied().fold(f64::MAX, f64::min)];
            let hi = [xs.iter().copied().fold(f64::MIN, f64::max), ys.iter().copied().fold(f64::MIN, f64::max)];
            [lo[0], lo[1], hi[0], hi[1]]
        });
        let m = ((b[2] - b[0]).max(b[3] - b[1]) * 0.05).max(1.0);
        let b = [b[0] - m, b[1] - m, b[2] + m, b[3] + m];
        lines.iter().try_for_each(|l| self.draw_infinite(l, b))
    }

    /// Several closed outlines filled as one (a hatch with islands), kept apart in the points by
    /// a `GAP` point, each cut to the viewport inside a layout.
    fn area(&mut self, rings: Vec<Vec<P>>, t: &Affine, color: [u8; 3], fill: Option<f32>, stroke: Stroke) -> Result<(), String> {
        let mut pts: Vec<P> = Vec::new();
        for ring in rings {
            self.out.points += ring.len() + 1;
            let ring: Vec<P> = ring.into_iter().map(|p| t.apply(p)).filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
            let ring = match self.clip {
                Some(b) => cad_geom::clip_polygon(&ring, b),
                None => ring,
            };
            if ring.len() >= 3 {
                if !pts.is_empty() {
                    pts.push(GAP);
                }
                pts.extend(ring);
            }
        }
        if self.out.points > MAX_POINTS {
            return Err(format!("more than {MAX_POINTS} points"));
        }
        if !pts.is_empty() {
            self.out.items.push(Item::Line { pts, color, fill, stroke });
        }
        Ok(())
    }
}

/// A hatch boundary as a closed list of points.
fn boundary(path: &acadrust::entities::BoundaryPath) -> Vec<P> {
    use acadrust::entities::BoundaryEdge as E;
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
    pts
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
                let path = f.path().to_string_lossy().to_string();
                let next = |rel: &str| crate::model::sibling_path(&path, rel).and_then(|p| std::fs::read(p).ok());
                match std::panic::catch_unwind(|| read_with(&bytes, &name, &next)) {
                    Ok(Ok(d)) => {
                        let read = t.elapsed();
                        let t = std::time::Instant::now();
                        let img = crate::decode::cad_render(&d, 1600, None);
                        println!("{name}: {} items, read {read:.0?}, drawn {:.0?}", d.items.len(), t.elapsed());
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
