//! A picture shown smaller than its pixels, filtered on the graphics card: each point of the
//! screen is the weighted average of the pixels around it (a Lanczos-3 filter, in two passes,
//! rows then columns), at every zoom and while zooming. The halved copies (`mipmap`) alone left
//! the zooms between 50% and 100% to the card's linear filter, which mixes only the 2 × 2 nearest
//! pixels: lines one pixel thin shown at 62% came out as stripes (moiré). The filter starts from
//! the halved copy just larger than the screen, so it never takes more than 17 pixels a pass.
//!
//! The cut-off is 1.35 times lower than a plain Lanczos-3: measured with ImageMagick on lines one
//! pixel thin, a plain one still leaves stripes 156 grey levels apart at 90% (it lets through a
//! part of what the screen cannot show), 1.35 lower 5 at 90% and 1 at 62%, and the photo stays
//! sharper than with a Gaussian that removes the stripes as well (PSNR against a plain Lanczos:
//! 34-38 dB against 25-29).

use std::sync::Mutex;

use eframe::egui::{self, TextureId};
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::{self, util::DeviceExt};

use crate::mipmap::Picture;

/// How much lower than a plain Lanczos-3 the cut-off is (see the module's comment).
pub(crate) const BLUR: f32 = 1.35;
/// Lobes of the Lanczos filter.
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
    pad: i32,
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
    let reach = 3.0 * p.width;
    let first = i32(floor(centre - reach));
    let end = i32(ceil(centre + reach));
    var sum = vec4(0.0);
    var total = 0.0;
    for (var j = first; j <= end; j = j + 1) {
        let w = lanczos((f32(j) + 0.5 - centre) / p.width);
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

/// The Lanczos-3 filter at `t` filter widths from the centre: the shader's `lanczos`, for the tests.
#[cfg(test)]
fn lanczos(t: f32) -> f32 {
    let sinc = |x: f32| if x.abs() < 1e-5 { 1.0 } else { (std::f32::consts::PI * x).sin() / (std::f32::consts::PI * x) };
    if t.abs() >= LOBES { 0.0 } else { sinc(t) * sinc(t / LOBES) }
}

/// The filter's width, in input pixels, for `step` input pixels per output pixel.
pub(crate) fn width(step: f32) -> f32 {
    step.max(1.0) * BLUR
}

/// One output pixel along an axis, as the shader makes it: `get(j)` the input pixel `j` (the
/// edges repeated), the footprint starting at `origin` and `step` wide per pixel.
#[cfg(test)]
fn filtered(get: impl Fn(i64) -> f32, last: i64, origin: f32, step: f32, at: usize) -> f32 {
    let (centre, w) = (origin + (at as f32 + 0.5) * step, width(step));
    let reach = LOBES * w;
    let (mut sum, mut total) = (0.0, 0.0);
    for j in (centre - reach).floor() as i64..=(centre + reach).ceil() as i64 {
        let k = lanczos((j as f32 + 0.5 - centre) / w);
        sum += get(j.clamp(0, last)) * k;
        total += k;
    }
    sum / total
}

/// A grey picture (`size`, row by row) made `out` pixels, from all of it: rows, then columns, as on
/// the graphics card (the result clamped to 0-255).
#[cfg(test)]
fn resample(grey: &[f32], size: [usize; 2], out: [usize; 2]) -> Vec<f32> {
    let [w, h] = size;
    let (sx, sy) = (w as f32 / out[0] as f32, h as f32 / out[1] as f32);
    let mut mid = vec![0.0; out[0] * h];
    for y in 0..h {
        for x in 0..out[0] {
            mid[y * out[0] + x] = filtered(|j| grey[y * w + j as usize], w as i64 - 1, 0.0, sx, x);
        }
    }
    let mut v = vec![0.0; out[0] * out[1]];
    for y in 0..out[1] {
        for x in 0..out[0] {
            v[y * out[0] + x] = filtered(|j| mid[j as usize * out[0] + x], h as i64 - 1, 0.0, sy, y).clamp(0.0, 255.0);
        }
    }
    v
}

/// The two passes' pictures of one picture drawn this frame, kept from frame to frame and grown
/// when needed.
struct Slot {
    mid: (wgpu::Texture, wgpu::TextureView),
    out: (wgpu::Texture, wgpu::TextureView),
    id: TextureId,
    uniforms: [wgpu::Buffer; 2],
    used: bool,
}

struct Smoother {
    rs: RenderState,
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
pub(crate) fn init(rs: &RenderState) {
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
    *SMOOTHER.lock().unwrap() = Some(Smoother { rs: rs.clone(), layout, rows, columns, slots: Vec::new(), next: 0 });
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
    let source = pic.texture_at(scale);
    let texture = s.rs.renderer.read().texture(&source.id())?.texture.clone()?;
    let (w, h) = (texture.width() as f32, texture.height() as f32);
    let (x0, y0) = (uv.min.x * w, uv.min.y * h);
    let (sx, sy) = (uv.width() * w / out[0] as f32, uv.height() * h / out[1] as f32);
    // The rows the columns pass reads: the footprint and the filter's reach.
    let reach = LOBES * width(sy);
    let first = (y0 - reach).floor() as i32;
    let rows = ((y0 + uv.height() * h + reach).ceil() as i32 - first).max(1) as u32;
    if rows > max {
        return None;
    }
    let slot = s.slot([out[0], rows], out);
    let pass = |origin: f32, step: f32, axis: u32, offset: i32, last: [i32; 2]| {
        let mut b = Vec::with_capacity(32);
        for v in [origin, step, width(step)] {
            b.extend(v.to_le_bytes());
        }
        for v in [axis as i32, offset, 0, last[0], last[1]] {
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
    let o = &slot.out.0;
    Some((slot.id, egui::vec2(out[0] as f32 / o.width() as f32, out[1] as f32 / o.height() as f32)))
}

impl Smoother {
    /// The next slot of this frame, large enough for the two passes.
    fn slot(&mut self, mid: [u32; 2], out: [u32; 2]) -> usize {
        let i = self.next;
        self.next += 1;
        let fits = |t: &wgpu::Texture, size: [u32; 2]| t.width() >= size[0] && t.height() >= size[1];
        match self.slots.get(i) {
            Some(slot) if fits(&slot.mid.0, mid) && fits(&slot.out.0, out) => {}
            _ => {
                let grown = |t: Option<&wgpu::Texture>, size: [u32; 2]| {
                    let max = self.rs.device.limits().max_texture_dimension_2d;
                    let old = t.map_or([0, 0], |t| [t.width(), t.height()]);
                    std::array::from_fn::<u32, 2, _>(|k| size[k].max(old[k]).div_ceil(GRAIN).saturating_mul(GRAIN).min(max))
                };
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
                let old = self.slots.get(i);
                let mid = make(grown(old.map(|s| &s.mid.0), mid), MID);
                let out = make(grown(old.map(|s| &s.out.0), out), OUT);
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
                let slot = Slot { mid, out, id, uniforms: [uniform(), uniform()], used: true };
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

    /// A flat grey stays the same grey (the weights add up to one).
    #[test]
    fn flat_grey_stays() {
        let v = resample(&[100.0; 40 * 30], [40, 30], [25, 19]);
        assert!(v.iter().all(|g| (g - 100.0).abs() < 1e-3));
    }
}
