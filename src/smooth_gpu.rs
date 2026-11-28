//! A picture shown smaller than its pixels, filtered on the graphics card: each point of the
//! screen is the weighted average of the pixels around it (a Lanczos-3 filter, in two passes,
//! rows then columns), at every zoom and while zooming. The halved copies (`mipmap`) alone left
//! the zooms between 50% and 100% to the card's linear filter, which mixes only the 2 × 2 nearest
//! pixels: lines one pixel thin shown at 62% came out as stripes (moiré). The filter starts from
//! the halved copy just larger than the screen, so it never takes more than 17 pixels a pass.
//!
//! Under two thirds of the picture's size the filter is a plain Lanczos-3, as ImageMagick's: a
//! photo fitted to the window keeps all the detail the screen can show. From there to 100% its
//! cut-off goes lower, up to 1.35 times (`width`): the filter is never narrower than 1.5 pixels
//! of the picture, so lines one pixel thin (a screenshot, a drawing) come out an even grey, where
//! a plain one leaves stripes (ImageMagick: a spread of 56 grey levels at 90%, 2 with this). A
//! cut-off 1.35 times lower at every zoom removed the stripes of coarser lines too, but a photo
//! at 24% kept only 75% of its fine detail and looked blurred (17/11). More lobes (5, 8) keep
//! the detail and the even grey, but leave echoes beside the edges of text and cost the card up
//! to twice the time. What is left: lines two pixels thin between 40% and 48% (and coarser ones
//! lower) show stripes, as with any plain Lanczos-3.
//!
//! A picture that has not changed, drawn again at the same place and size, is not filtered
//! again: the result of the last frame is still there (`Memo`).

use std::sync::Mutex;

use eframe::egui::{self, TextureId};
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::{self, util::DeviceExt};

use crate::mipmap::Picture;

/// How much lower than a plain Lanczos-3 the cut-off is at most, close to 100% (see the module's
/// comment).
pub(crate) const BLUR: f32 = 1.35;
/// The filter is never narrower than this, in pixels of the picture.
const NARROWEST: f32 = 1.5;
/// Lobes of the Lanczos filter (the shader has the same number).
const LOBES: f32 = 3.0;

const SHADER: &str = r#"
struct Pass {
    // In the input's pixels: where the first output pixel's footprint starts, and how many input
    // pixels an output pixel covers, along the axis filtered; the filter's width.
    origin: f32,
    step: f32,
    width: f32,
    // 0: along x (rows), 1: along y (columns).
    axis: u32,
    // Added to the other coordinate (the first row read by the rows pass).
    offset: i32,
    // 0: Lanczos, 1: Mitchell (B = C = 1/3).
    kernel: i32,
    // The input's last pixel.
    last: vec2<i32>,
};

@group(0) @binding(0) var input: texture_2d<f32>;
@group(0) @binding(1) var<uniform> p: Pass;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn sinc(x: f32) -> f32 {
    if abs(x) < 1e-5 {
        return 1.0;
    }
    let a = 3.14159265 * x;
    return sin(a) / a;
}

fn lanczos(t: f32) -> f32 {
    if abs(t) >= 3.0 {
        return 0.0;
    }
    return sinc(t) * sinc(t / 3.0);
}

fn mitchell(t: f32) -> f32 {
    let x = abs(t);
    if x < 1.0 {
        return (7.0 * x * x * x - 12.0 * x * x + 16.0 / 3.0) / 6.0;
    }
    if x < 2.0 {
        return (-7.0 / 3.0 * x * x * x + 12.0 * x * x - 20.0 * x + 32.0 / 3.0) / 6.0;
    }
    return 0.0;
}

fn kernel(t: f32) -> f32 {
    if p.kernel == 1 {
        return mitchell(t);
    }
    return lanczos(t);
}

fn filtered(at: vec4<f32>) -> vec4<f32> {
    let px = vec2<i32>(at.xy);
    var along = px.x;
    var across = px.y;
    if p.axis == 1u {
        along = px.y;
        across = px.x;
    }
    across = across + p.offset;
    let centre = p.origin + (f32(along) + 0.5) * p.step;
    // As far as the filter is not zero: Lanczos' three lobes, Mitchell's two.
    let reach = select(3.0, 2.0, p.kernel == 1) * p.width;
    let first = i32(floor(centre - reach));
    let end = i32(ceil(centre + reach));
    var sum = vec4(0.0);
    var total = 0.0;
    for (var j = first; j <= end; j = j + 1) {
        let w = kernel((f32(j) + 0.5 - centre) / p.width);
        var c = vec2(j, across);
        if p.axis == 1u {
            c = vec2(across, j);
        }
        sum = sum + textureLoad(input, clamp(c, vec2(0), p.last), 0) * w;
        total = total + w;
    }
    return sum / total;
}

@fragment fn rows(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    return filtered(at);
}

// Premultiplied: no colour beyond its alpha, nothing beyond 0 and 1 (the filter's lobes).
@fragment fn columns(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let c = clamp(filtered(at), vec4(0.0), vec4(1.0));
    return vec4(min(c.rgb, vec3(c.a)), c.a);
}
"#;

/// The Lanczos filter at `t` filter widths from the centre: the shader's `lanczos`, for the tests.
#[cfg(test)]
fn lanczos(t: f32) -> f32 {
    let sinc = |x: f32| if x.abs() < 1e-5 { 1.0 } else { (std::f32::consts::PI * x).sin() / (std::f32::consts::PI * x) };
    if t.abs() >= LOBES { 0.0 } else { sinc(t) * sinc(t / LOBES) }
}

/// The Mitchell filter (B = C = 1/3): the shader's `mitchell`, for the tests.
#[cfg(test)]
fn mitchell_at(t: f32) -> f32 {
    let x = t.abs();
    if x < 1.0 {
        (7.0 * x * x * x - 12.0 * x * x + 16.0 / 3.0) / 6.0
    } else if x < 2.0 {
        (-7.0 / 3.0 * x * x * x + 12.0 * x * x - 20.0 * x + 32.0 / 3.0) / 6.0
    } else {
        0.0
    }
}

/// The filter's width, in input pixels, for `step` input pixels per output pixel: the step, but
/// never under `NARROWEST` (nor over `BLUR` steps).
pub(crate) fn width(step: f32) -> f32 {
    let step = step.max(1.0);
    step.max(NARROWEST.min(step * BLUR))
}

/// One output pixel along an axis, as the shader makes it: `get(j)` the input pixel `j` (the
/// edges repeated), the footprint starting at `origin` and `step` wide per pixel.
#[cfg(test)]
fn filtered(get: impl Fn(i64) -> f32, last: i64, origin: f32, step: f32, at: usize, mitchell: bool) -> f32 {
    let w = if mitchell { step.max(1.0) } else { width(step) };
    let centre = origin + (at as f32 + 0.5) * step;
    let reach = LOBES * w;
    let (mut sum, mut total) = (0.0, 0.0);
    for j in (centre - reach).floor() as i64..=(centre + reach).ceil() as i64 {
        let t = (j as f32 + 0.5 - centre) / w;
        let k = if mitchell { mitchell_at(t) } else { lanczos(t) };
        sum += get(j.clamp(0, last)) * k;
        total += k;
    }
    sum / total
}

/// A grey picture (`size`, row by row) made `out` pixels, from all of it: rows, then columns, as on
/// the graphics card (the result clamped to 0-255).
#[cfg(test)]
fn resample(grey: &[f32], size: [usize; 2], out: [usize; 2]) -> Vec<f32> {
    resample_with(grey, size, out, false)
}

/// `resample` with Lanczos or Mitchell.
#[cfg(test)]
fn resample_with(grey: &[f32], size: [usize; 2], out: [usize; 2], mitchell: bool) -> Vec<f32> {
    let [w, h] = size;
    let (sx, sy) = (w as f32 / out[0] as f32, h as f32 / out[1] as f32);
    let mut mid = vec![0.0; out[0] * h];
    for y in 0..h {
        for x in 0..out[0] {
            mid[y * out[0] + x] = filtered(|j| grey[y * w + j as usize], w as i64 - 1, 0.0, sx, x, mitchell);
        }
    }
    let mut v = vec![0.0; out[0] * out[1]];
    for y in 0..out[1] {
        for x in 0..out[0] {
            v[y * out[0] + x] = filtered(|j| mid[j as usize * out[0] + x], h as i64 - 1, 0.0, sy, y, mitchell).clamp(0.0, 255.0);
        }
    }
    v
}

/// The rows of a picture `height` high that the columns pass reads, for a part that starts at
/// `y0` and is `span` high, with the filter reaching `reach` rows beyond: the first and how
/// many. Only rows of the picture: beyond its edges the columns pass repeats the first and the
/// last, as the rows pass would have (counted too, a picture as high as the card allows had
/// more rows than a texture can be, and was left to the halved copies).
fn rows_read(y0: f32, span: f32, reach: f32, height: u32) -> (i32, u32) {
    let first = ((y0 - reach).floor() as i32).max(0);
    let end = ((y0 + span + reach).ceil() as i32).min(height as i32);
    (first, (end - first).max(1) as u32)
}

/// The two passes' pictures of one picture drawn this frame, kept from frame to frame and grown
/// when needed.
struct Slot {
    mid: (wgpu::Texture, wgpu::TextureView),
    out: (wgpu::Texture, wgpu::TextureView),
    id: TextureId,
    uniforms: [wgpu::Buffer; 2],
    used: bool,
    memo: Memo,
}

/// What a result depends on: the texture read and how many times the picture changed, the part
/// (its bits), the size, the filter.
type Key = (TextureId, u64, [u32; 4], [u32; 2], bool);

/// What a slot's result holds, to tell whether it must be made again.
#[derive(Default)]
struct Memo {
    /// What the result was made from, when it can be kept.
    key: Option<Key>,
    /// The texture and the change of the picture filtered last, and the frame it was first
    /// filtered at.
    seen: Option<(TextureId, u64, u64)>,
}

/// What to do with a slot's result.
#[derive(Debug, PartialEq)]
enum Ask {
    /// Still good: nothing to filter.
    Kept,
    /// To make, and to keep.
    Make,
    /// To make now and again at the next frame: the picture has just changed.
    Again,
}

impl Memo {
    /// For a result that depends on `key`. A picture put in the place of another (`Picture::set`:
    /// the next frame of an animation, another page) reaches the card only at the end of the
    /// frame, after this is asked: what is filtered in that frame is still the picture before, so
    /// it is not kept and the frame is drawn again. `frame`: the number of the frame being drawn;
    /// egui may draw one in two passes, and the second is still before the picture reaches the card.
    fn ask(&mut self, key: Key, frame: u64) -> Ask {
        if self.key == Some(key) {
            return Ask::Kept;
        }
        let first = match self.seen {
            Some((id, version, first)) if (id, version) == (key.0, key.1) => first,
            _ => frame,
        };
        let settled = frame > first;
        self.seen = Some((key.0, key.1, first));
        self.key = settled.then_some(key);
        if settled { Ask::Make } else { Ask::Again }
    }
}

struct Smoother {
    rs: RenderState,
    /// To ask for another frame.
    ctx: egui::Context,
    layout: wgpu::BindGroupLayout,
    rows: wgpu::RenderPipeline,
    columns: wgpu::RenderPipeline,
    slots: Vec<Slot>,
    next: usize,
}

static SMOOTHER: Mutex<Option<Smoother>> = Mutex::new(None);

const MID: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const OUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Sizes of the passes' pictures rounded up to this, so that zooming does not make new ones at
/// every frame.
const GRAIN: u32 = 256;

/// With eframe's graphics card, at the start.
pub(crate) fn init(rs: &RenderState, ctx: &egui::Context) {
    let device = &rs.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("smooth"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("smooth"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("smooth"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let pipeline = |entry: &str, format| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("smooth"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(entry), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview_mask: None,
            cache: None,
        })
    };
    let (rows, columns) = (pipeline("rows", MID), pipeline("columns", OUT));
    *SMOOTHER.lock().unwrap() = Some(Smoother { rs: rs.clone(), ctx: ctx.clone(), layout, rows, columns, slots: Vec::new(), next: 0 });
}

/// At the start of each frame: the pictures of the frame before not drawn again are let go.
pub(crate) fn new_frame() {
    let mut guard = SMOOTHER.lock().unwrap();
    let Some(s) = guard.as_mut() else { return };
    let used = s.slots.iter().take_while(|slot| slot.used).count();
    for slot in s.slots.drain(used..) {
        s.rs.renderer.write().free_texture(&slot.id);
    }
    s.slots.iter_mut().for_each(|slot| slot.used = false);
    s.next = 0;
}

/// The part `uv` (0 to 1, the picture's own orientation) of `pic` made `out` pixels by the filter,
/// for a picture shown at `scale` screen pixels per pixel (below 1). The texture to draw and the
/// part of it that holds the result (0 to 1). `None` without the graphics card, or while the
/// picture is not on it yet (its first frame).
pub(crate) fn resampled(pic: &Picture, scale: f32, uv: egui::Rect, out: [u32; 2]) -> Option<(TextureId, egui::Vec2)> {
    let mut guard = SMOOTHER.lock().unwrap();
    let s = guard.as_mut()?;
    let max = s.rs.device.limits().max_texture_dimension_2d;
    if out[0] == 0 || out[1] == 0 || out[0] > max || out[1] > max {
        return None;
    }
    let filter = crate::mipmap::filter();
    let mitchell = filter == crate::settings::SmoothFilter::Mitchell;
    let source: &egui::TextureHandle = if filter == crate::settings::SmoothFilter::Halves { pic.texture_at(scale) } else { pic };
    // Mitchell at its own width: the wider one is Lanczos' (see the module's comment).
    let width = |step: f32| if mitchell { step.max(1.0) } else { width(step) };
    let texture = s.rs.renderer.read().texture(&source.id())?.texture.clone()?;
    let (w, h) = (texture.width() as f32, texture.height() as f32);
    let (x0, y0) = (uv.min.x * w, uv.min.y * h);
    let (sx, sy) = (uv.width() * w / out[0] as f32, uv.height() * h / out[1] as f32);
    // The rows the columns pass reads: the footprint and the filter's reach.
    let reach = LOBES * width(sy);
    let (first, rows) = rows_read(y0, uv.height() * h, reach, texture.height());
    if rows > max {
        return None;
    }
    let slot = s.slot([out[0], rows], out);
    let key: Key = (source.id(), pic.version(), [uv.min.x, uv.min.y, uv.max.x, uv.max.y].map(f32::to_bits), out, mitchell);
    let extent = |o: &wgpu::Texture| egui::vec2(out[0] as f32 / o.width() as f32, out[1] as f32 / o.height() as f32);
    let frame = s.ctx.cumulative_frame_nr();
    match s.slots[slot].memo.ask(key, frame) {
        Ask::Kept => return Some((s.slots[slot].id, extent(&s.slots[slot].out.0))),
        Ask::Again => s.ctx.request_repaint(),
        Ask::Make => {}
    }
    let pass = |origin: f32, step: f32, axis: u32, offset: i32, last: [i32; 2]| {
        let mut b = Vec::with_capacity(32);
        for v in [origin, step, width(step)] {
            b.extend(v.to_le_bytes());
        }
        for v in [axis as i32, offset, mitchell as i32, last[0], last[1]] {
            b.extend(v.to_le_bytes());
        }
        b
    };
    let s = guard.as_mut()?;
    let slot = &s.slots[slot];
    let queue = &s.rs.queue;
    queue.write_buffer(&slot.uniforms[0], 0, &pass(x0, sx, 0, first, [w as i32 - 1, h as i32 - 1]));
    queue.write_buffer(&slot.uniforms[1], 0, &pass(y0 - first as f32, sy, 1, 0, [out[0] as i32 - 1, rows as i32 - 1]));
    let source_view = texture.create_view(&Default::default());
    let bind = |view: &wgpu::TextureView, uniforms: &wgpu::Buffer| {
        s.rs.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smooth"),
            layout: &s.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }, wgpu::BindGroupEntry { binding: 1, resource: uniforms.as_entire_binding() }],
        })
    };
    let (first_bind, second_bind) = (bind(&source_view, &slot.uniforms[0]), bind(&slot.mid.1, &slot.uniforms[1]));
    let mut encoder = s.rs.device.create_command_encoder(&Default::default());
    for (view, pipeline, group, size) in [(&slot.mid.1, &s.rows, &first_bind, [out[0], rows]), (&slot.out.1, &s.columns, &second_bind, out)] {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("smooth"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, size[0] as f32, size[1] as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, size[0], size[1]);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
    Some((slot.id, extent(&slot.out.0)))
}

/// The new size of one of a slot's pictures, `old` its size (`None`: none yet) and `size` the size
/// asked: `None` while it holds it; otherwise grown to hold it, in steps of `GRAIN`, never beyond
/// `max`.
fn slot_size(old: Option<[u32; 2]>, size: [u32; 2], max: u32) -> Option<[u32; 2]> {
    let had = old.unwrap_or([0, 0]);
    if old.is_some() && had[0] >= size[0] && had[1] >= size[1] {
        return None;
    }
    Some(std::array::from_fn(|k| size[k].max(had[k]).div_ceil(GRAIN).saturating_mul(GRAIN).min(max)))
}

impl Smoother {
    /// The next slot of this frame, large enough for the two passes.
    fn slot(&mut self, mid: [u32; 2], out: [u32; 2]) -> usize {
        let i = self.next;
        self.next += 1;
        let max = self.rs.device.limits().max_texture_dimension_2d;
        let had = |t: &wgpu::Texture| [t.width(), t.height()];
        let old = self.slots.get(i).map(|s| (had(&s.mid.0), had(&s.out.0)));
        let (new_mid, new_out) = (slot_size(old.map(|o| o.0), mid, max), slot_size(old.map(|o| o.1), out, max));
        match (old, new_mid, new_out) {
            (Some(_), None, None) => {}
            _ => {
                let make = |size: [u32; 2], format| {
                    let t = self.rs.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("smooth"),
                        size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let v = t.create_view(&Default::default());
                    (t, v)
                };
                let mid = make(new_mid.or(old.map(|o| o.0)).unwrap_or(mid), MID);
                let out = make(new_out.or(old.map(|o| o.1)).unwrap_or(out), OUT);
                let old = self.slots.get(i);
                let mut renderer = self.rs.renderer.write();
                let id = match old {
                    Some(s) => {
                        renderer.update_egui_texture_from_wgpu_texture(&self.rs.device, &out.1, wgpu::FilterMode::Linear, s.id);
                        s.id
                    }
                    None => renderer.register_native_texture(&self.rs.device, &out.1, wgpu::FilterMode::Linear),
                };
                drop(renderer);
                let uniform = || self.rs.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("smooth"), contents: &[0; 32], usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
                let slot = Slot { mid, out, id, uniforms: [uniform(), uniform()], used: true, memo: Memo::default() };
                if i < self.slots.len() {
                    self.slots[i] = slot;
                } else {
                    self.slots.push(slot);
                }
            }
        }
        self.slots[i].used = true;
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kernel: 1 at the centre, 0 at the whole numbers and from 3 on.
    #[test]
    fn lanczos_is_one_at_the_centre_and_zero_beyond() {
        assert_eq!(lanczos(0.0), 1.0);
        assert!(lanczos(1.0).abs() < 1e-6 && lanczos(2.0).abs() < 1e-6);
        assert_eq!(lanczos(3.0), 0.0);
        assert!(lanczos(1.5) < 0.0);
    }

    /// `size` pixels, white where `white(x, y)`, black elsewhere, as grey levels.
    fn pattern(size: [usize; 2], white: impl Fn(usize, usize) -> bool) -> Vec<f32> {
        (0..size[0] * size[1]).map(|i| if white(i % size[0], i / size[0]) { 255.0 } else { 0.0 }).collect()
    }

    /// As the viewer draws a picture of `size` on `out` pixels: from the halved copy `level`
    /// chooses, filtered.
    fn shown(grey: &[f32], size: [usize; 2], out: [usize; 2]) -> Vec<f32> {
        let pixels = grey.iter().map(|g| eframe::egui::Color32::from_gray(*g as u8)).collect();
        let img = eframe::egui::ColorImage::new(size, pixels);
        let copies = crate::mipmap::halves(&img, 1);
        match crate::mipmap::level(out[0] as f32 / size[0] as f32, copies.len()) {
            0 => resample(grey, size, out),
            l => {
                let c = &copies[l - 1];
                resample(&c.pixels.iter().map(|p| p.r() as f32).collect::<Vec<_>>(), c.size, out)
            }
        }
    }

    /// The highest and lowest grey inside, away from the edges.
    fn spread(v: &[f32], out: [usize; 2]) -> f32 {
        let inside: Vec<f32> = (0..v.len()).filter(|i| (8..out[0] - 8).contains(&(i % out[0])) && (8..out[1] - 8).contains(&(i / out[0]))).map(|i| v[i]).collect();
        inside.iter().copied().fold(f32::MIN, f32::max) - inside.iter().copied().fold(f32::MAX, f32::min)
    }

    /// Lines one pixel thin (as `magick -size 3000x2000 xc: -fx 'i%2'`) and a chessboard shown at
    /// 62%, 75% and 90%, and at 40% and 20% from the halved copies: an even grey, within 6 levels
    /// of 255. The halved copies alone (drawn by the card's linear filter) left stripes there.
    #[test]
    fn thin_lines_and_chessboard_between_50_and_100_are_grey() {
        let size = [600, 400];
        let lines = pattern(size, |x, _| x % 2 == 1);
        let chess = pattern(size, |x, y| (x + y) % 2 == 1);
        for (name, img) in [("lines", &lines), ("chessboard", &chess)] {
            for percent in [62, 75, 90, 40, 20] {
                let out = [size[0] * percent / 100, size[1] * percent / 100];
                let s = spread(&shown(img, size, out), out);
                assert!(s <= 6.0, "{name} at {percent}%: grey from one point to the next differs by {s}");
            }
        }
    }

    /// Rings closer and closer from the centre (a zone plate, as `anelli.png`) shown at 25%, with the
    /// three filters of Options > Look. Where the rings are finer than the screen's pixels the right
    /// result is an even grey: what is left there are false rings. From the whole picture they are
    /// fewer than from the halved copies; the real rings near the centre stay with all three.
    #[test]
    fn zone_plate_at_a_quarter_with_the_three_filters() {
        let (size, out) = ([1200, 800], [300, 200]);
        // Local frequency k·r/π cycles a pixel: 0.45 at the corners, under the picture's own limit.
        let rmax = ((600.0f32).powi(2) + 400.0f32.powi(2)).sqrt();
        let k = 0.45 * std::f32::consts::PI / rmax;
        let grey: Vec<f32> = (0..size[0] * size[1])
            .map(|i| {
                let (x, y) = ((i % size[0]) as f32 + 0.5 - 600.0, (i / size[0]) as f32 + 0.5 - 400.0);
                127.5 + 127.5 * (k * (x * x + y * y)).cos()
            })
            .collect();
        // Beyond r = 300 the rings are finer than 0.19 cycles a pixel, over the 0.125 a quarter shows.
        let rms = |v: &[f32], far: bool| {
            let pick: Vec<f32> = (0..v.len())
                .filter(|i| {
                    let (x, y) = ((i % out[0]) as f32 * 4.0 + 2.0 - 600.0, (i / out[0]) as f32 * 4.0 + 2.0 - 400.0);
                    let r = (x * x + y * y).sqrt();
                    if far { r > 300.0 && (8..out[0] - 8).contains(&(i % out[0])) && (8..out[1] - 8).contains(&(i / out[0])) } else { r < 60.0 }
                })
                .map(|i| v[i] - 127.5)
                .collect();
            (pick.iter().map(|d| d * d).sum::<f32>() / pick.len() as f32).sqrt()
        };
        let halves = shown(&grey, size, out);
        let whole = resample_with(&grey, size, out, false);
        let mitchell = resample_with(&grey, size, out, true);
        let (fh, fw, fm) = (rms(&halves, true), rms(&whole, true), rms(&mitchell, true));
        let (ch, cw, cm) = (rms(&halves, false), rms(&whole, false), rms(&mitchell, false));
        eprintln!("false rings (rms grey levels): halves {fh:.2}, whole {fw:.2}, Mitchell {fm:.2}; real rings at the centre: {ch:.1}, {cw:.1}, {cm:.1}");
        assert!(fw < fh && fm < fh, "from the whole picture fewer false rings: {fh} {fw} {fm}");
        assert!(ch > 40.0 && cw > 40.0 && cm > 40.0, "the real rings stay: {ch} {cw} {cm}");
    }

    /// The width: a plain Lanczos-3 under two thirds, 1.35 times wider close to 100%, never under
    /// 1.5 pixels of the picture in between.
    #[test]
    fn plain_under_two_thirds_wider_towards_100() {
        for step in [1.5, 2.0, 4.2, 12.0] {
            assert_eq!(width(step), step);
        }
        assert_eq!(width(1.25), 1.5);
        assert!((width(1.0 / 0.9) - 1.5).abs() < 1e-6 && (width(1.01) - 1.01 * BLUR).abs() < 1e-6);
    }

    /// A photo's fine detail stays: waves 14 pixels long shown at 25% (0.57 of the finest the
    /// screen can show) keep nearly all their height; a cut-off 1.35 times lower kept 82%.
    #[test]
    fn fine_detail_that_the_screen_can_show_stays() {
        let (size, out) = ([1600, 160], [400, 40]);
        let grey: Vec<f32> = (0..size[0] * size[1]).map(|i| 127.5 + 100.0 * (std::f32::consts::TAU * (i % size[0]) as f32 / 14.0).sin()).collect();
        let kept = spread(&resample(&grey, size, out), out) / 200.0;
        eprintln!("waves 14 pixels long at 25%: {:.0}% of their height", kept * 100.0);
        assert!(kept > 0.9, "{kept}");
    }

    /// A result is kept while what it was made from is the same; a picture that has just changed
    /// is filtered at that frame and at the next, and only then kept.
    #[test]
    fn a_result_is_kept_but_not_of_a_picture_just_changed() {
        let key = |version: u64, zoom: u32| -> Key { (TextureId::Managed(7), version, [0, 0, zoom, zoom], [100, 80], false) };
        let mut memo = Memo::default();
        // A picture never seen: made twice (the first may be of what the texture held before).
        assert_eq!([memo.ask(key(0, 1), 10), memo.ask(key(0, 1), 11), memo.ask(key(0, 1), 12)], [Ask::Again, Ask::Make, Ask::Kept]);
        // Another zoom of the same picture: made once.
        assert_eq!([memo.ask(key(0, 2), 13), memo.ask(key(0, 2), 14)], [Ask::Make, Ask::Kept]);
        // The next frame of an animation: not kept at once.
        assert_eq!([memo.ask(key(1, 2), 15), memo.ask(key(1, 2), 16), memo.ask(key(1, 2), 17)], [Ask::Again, Ask::Make, Ask::Kept]);
        // A frame drawn in two passes: the second is not "the next frame", the picture is not on the
        // card yet.
        assert_eq!([memo.ask(key(2, 2), 20), memo.ask(key(2, 2), 20), memo.ask(key(2, 2), 21), memo.ask(key(2, 2), 22)], [Ask::Again, Ask::Again, Ask::Make, Ask::Kept]);
    }

    /// The rows read are rows of the picture: one as high as a texture can be, shown whole, has
    /// that many and not more; a part in the middle has its own and the filter's reach.
    #[test]
    fn the_rows_read_are_inside_the_picture() {
        assert_eq!(rows_read(0.0, 8192.0, 12.6, 8192), (0, 8192));
        assert_eq!(rows_read(100.0, 50.0, 4.5, 8192), (95, 60));
        assert_eq!(rows_read(8100.0, 92.0, 30.0, 8192), (8070, 122));
    }

    /// Audit 4.10: a slot made for a large picture (a photo of 8192 × 5462 fitted to a window of
    /// 1920 × 1080: the rows pass 1792 × 5632, 8 bytes a pixel, 80 MB) is made again smaller when a
    /// picture asks for less than half of it on a side; zooming a little keeps it. It only grew:
    /// the largest of the session stayed.
    #[test]
    fn the_slot_comes_down_4_10() {
        let max = 16384;
        let big = [1792, 5632];
        let bytes = |s: Option<[u32; 2]>| s.map(|[w, h]| w as u64 * h as u64 * 8);
        // A landscape photo fitted after it.
        let after = slot_size(Some(big), [1700, 1080], max);
        eprintln!("rows pass after a smaller picture: {after:?}, {:?} bytes (before {:?})", bytes(after), bytes(Some(big)));
        assert_eq!(after, Some([1792, 1280]));
        // Zooming a little: kept.
        assert_eq!(slot_size(Some(big), [1600, 4000], max), None);
        // Larger: grown, as before; and the first one.
        assert_eq!(slot_size(Some([1024, 1024]), [1100, 900], max), Some([1280, 1024]));
        assert_eq!(slot_size(None, [10, 10], max), Some([256, 256]));
    }

    /// A flat grey stays the same grey (the weights add up to one).
    #[test]
    fn flat_grey_stays() {
        let v = resample(&[100.0; 40 * 30], [40, 30], [25, 19]);
        assert!(v.iter().all(|g| (g - 100.0).abs() < 1e-3));
    }
}
