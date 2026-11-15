//! The 3D model of the viewer drawn by the graphics card (`wgpu`, the one eframe already uses):
//! into a texture of our own with 4× antialiasing and a depth buffer, then shown by egui as an
//! image. Same camera and same light as `render3d` (`render3d::camera`, and `render_shine` for
//! surfaces with a material), which stays for the grid and the screenshots, made in the
//! background without the card.

use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::{self, util::DeviceExt};

use std::collections::HashMap;

use crate::model::{DEFAULT, Mesh, Surface};
use crate::render3d::{self, View};

const SAMPLES: u32 = 4;
/// Not sRGB: egui reads textures as gamma-encoded values, which is what `render3d` computes too.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Bytes of one vertex: position (3 × f32), colour (4 × u8), place on the pictures (2 × f32),
/// normal (3 × f32, zero for the face's own), metal, roughness, occlusion and what applies (4 ×
/// u8: 1 = the surface, 2 = bumps), glow (4 × u8).
const STRIDE: u64 = 44;

const SHADER: &str = r"
struct Camera { clip: mat4x4<f32>, eye: mat4x4<f32> };
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var picture: texture_2d<f32>;
@group(1) @binding(1) var linear: sampler;
@group(1) @binding(2) var bumps: texture_2d<f32>;
@group(1) @binding(3) var rough_metal: texture_2d<f32>;
@group(1) @binding(4) var occlusion: texture_2d<f32>;
@group(1) @binding(5) var glow_map: texture_2d<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) eye: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) pbr: vec4<f32>,
    @location(5) glow: vec3<f32>,
};

@vertex fn vs(@location(0) p: vec3<f32>, @location(1) c: vec4<f32>, @location(2) uv: vec2<f32>, @location(3) n: vec3<f32>, @location(4) pbr: vec4<f32>, @location(5) glow: vec4<f32>) -> Out {
    var o: Out;
    o.pos = cam.clip * vec4(p, 1.0);
    o.eye = (cam.eye * vec4(p, 1.0)).xyz;
    o.color = c.rgb;
    o.uv = uv;
    o.normal = (cam.eye * vec4(n, 0.0)).xyz;
    o.pbr = pbr;
    o.glow = glow.rgb;
    return o;
}

fn unit(v: vec3<f32>) -> vec3<f32> {
    let l = dot(v, v);
    return select(vec3(0.0), v * inverseSqrt(l), l > 0.0);
}

// Flat shading (the triangle's normal from the derivatives, both faces lit) as render3d; with a
// surface the same light as render_shine: change both together.
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    let dp1 = dpdx(i.eye);
    let dp2 = dpdy(i.eye);
    let duv1 = dpdx(i.uv);
    let duv2 = dpdy(i.uv);
    // The colour tinted by the picture (white without one), lit on the sRGB values like render3d.
    let color = i.color * textureSample(picture, linear, i.uv).rgb;
    let bump = textureSample(bumps, linear, i.uv).xyz * 2.0 - 1.0;
    let rm = textureSample(rough_metal, linear, i.uv).rgb;
    let occ = textureSample(occlusion, linear, i.uv).r;
    let glow = i.glow * textureSample(glow_map, linear, i.uv).rgb;
    let l = unit(vec3(-0.3, 0.45, 0.84));
    var face = unit(cross(dp1, dp2));
    let what = u32(round(i.pbr.w * 255.0));
    if ((what & 1u) == 0u) {
        return vec4(color * (0.28 + 0.72 * abs(dot(face, l))), 1.0);
    }
    // Towards the camera, at the origin.
    face = select(face, -face, dot(face, i.eye) > 0.0);
    var n = select(face, unit(i.normal), dot(i.normal, i.normal) > 0.0);
    n = select(n, -n, dot(n, face) < 0.0);
    if ((what & 2u) != 0u) {
        // The picture's u and v on the face (cotangent frame); green is up, where v gets smaller.
        let t = unit(cross(dp2, n) * duv1.x + cross(n, dp1) * duv2.x);
        let b = unit(cross(dp2, n) * duv1.y + cross(n, dp1) * duv2.y);
        let t2 = unit(t - n * dot(n, t));
        let b2 = unit(b - n * dot(n, b));
        n = unit(t2 * bump.x - b2 * bump.y + n * bump.z);
        n = select(face, n, dot(n, n) > 0.0);
    }
    let d = abs(dot(n, l));
    let rough = clamp(i.pbr.y * rm.g, 0.05, 1.0);
    let metal = clamp(i.pbr.x * rm.b, 0.0, 1.0);
    let ambient = 0.28 * (1.0 + i.pbr.z * (occ - 1.0));
    let nh = max(dot(n, unit(l + vec3(0.0, 0.0, 1.0))), 0.0);
    let a2 = pow(rough, 4.0);
    let den = nh * nh * (a2 - 1.0) + 1.0;
    let spread = min(a2 / (3.14159265 * den * den), 32.0);
    let f0 = vec3(0.04) + (color - vec3(0.04)) * metal;
    return vec4(color * (ambient + 0.72 * d) + f0 * spread * 0.25 * d + glow, 1.0);
}
";

/// The graphics card chosen offers "mailbox" presentation for the window (`adapter_selector`).
static MAILBOX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Picks the graphics card as eframe would (the fast one, or the saving one with
/// `WGPU_POWER_PREF=low`), and notes whether it can present in "mailbox" mode for this window:
/// the app then never waits for the desktop to show a frame. Measured on a hybrid laptop with the
/// monitor on the NVIDIA card and the desktop composed on the Intel one: windows stalled up to
/// 0.9 s with the default (FIFO), mailbox was clearly smoother (2026-11-01, `present_mode`).
pub fn adapter_selector() -> eframe::egui_wgpu::NativeAdapterSelectorMethod {
    Arc::new(|adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
        let saving = std::env::var("WGPU_POWER_PREF").is_ok_and(|v| v.eq_ignore_ascii_case("low"));
        let wanted = if saving { wgpu::DeviceType::IntegratedGpu } else { wgpu::DeviceType::DiscreteGpu };
        let usable: Vec<&wgpu::Adapter> = adapters.iter().filter(|a| surface.is_none_or(|s| a.is_surface_supported(s))).collect();
        let chosen = usable.iter().find(|a| a.get_info().device_type == wanted).or_else(|| usable.first()).ok_or("no graphics card can draw this window")?;
        let mailbox = surface.is_some_and(|s| s.get_capabilities(chosen).present_modes.contains(&wgpu::PresentMode::Mailbox));
        MAILBOX.store(mailbox, std::sync::atomic::Ordering::Relaxed);
        Ok((*chosen).clone())
    })
}

/// eframe's device, asked for with small blocks of graphics memory: a texture larger than a block
/// gets memory of its own, given back to the card (or, on cards that share the computer's memory,
/// to the computer) as soon as it is freed. With wgpu's default ("performance") the blocks are of
/// 128-256 MB and the last empty one of each kind is kept: after the editor closed on a 50 MP
/// photo (its 8192 × 5462 texture, 179 MB) the process kept 256 MB more of graphics memory until
/// it quit (Linux, NVIDIA, Vulkan; on Windows with an Intel card the same 250 MB showed as the
/// process's own memory, 2026-11-14). Pictures are few and large: the smaller blocks cost nothing
/// measurable [Inference].
pub fn frugal(base: Arc<dyn Fn(&wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> + Send + Sync>) -> Arc<dyn Fn(&wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> + Send + Sync> {
    Arc::new(move |adapter| frugal_descriptor(base(adapter)))
}

fn frugal_descriptor(base: wgpu::DeviceDescriptor<'static>) -> wgpu::DeviceDescriptor<'static> {
    wgpu::DeviceDescriptor { memory_hints: wgpu::MemoryHints::MemoryUsage, ..base }
}

/// The presentation to ask for once the window exists: mailbox where the card offers it, with
/// two frames in flight: with one, full screen on a 240 Hz laptop panel (GNOME showing the window
/// directly) went at 120, every other refresh (2026-11-02).
pub fn present_mode() -> Option<eframe::egui_wgpu::SurfaceConfig> {
    MAILBOX.load(std::sync::atomic::Ordering::Relaxed).then_some(eframe::egui_wgpu::SurfaceConfig { present_mode: wgpu::PresentMode::Mailbox, desired_maximum_frame_latency: Some(2) })
}

/// The picture's textures, made again when the viewer changes size.
struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    out: wgpu::TextureView,
    id: egui::TextureId,
}

/// A vertex buffer, how many vertices it holds and its pictures (index in `Model::pictures`).
type Piece = (wgpu::Buffer, u32, usize);

/// The model on the card: its triangles in buffers small enough for any card, grouped by their
/// pictures (colour, bumps, roughness and metal, occlusion, glow), and those groups' pictures.
struct Model {
    mesh: Arc<Mesh>,
    pieces: Vec<Piece>,
    pictures: Vec<wgpu::BindGroup>,
}

pub struct Gpu {
    rs: RenderState,
    pipeline: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    picture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    model: Option<Model>,
    target: Option<Target>,
}

impl Gpu {
    pub fn new(rs: RenderState) -> Self {
        let device = &rs.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("model"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let camera = device.create_buffer(&wgpu::BufferDescriptor { label: Some("camera"), size: 128, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera.as_entire_binding() }] });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let sampler_entry = wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None };
        let picture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[texture(0), sampler_entry, texture(2), texture(3), texture(4), texture(5)] });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout), Some(&picture_layout)], immediate_size: 0 });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Float32x2, 3 => Float32x3, 4 => Unorm8x4, 5 => Unorm8x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: STRIDE, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Gpu { rs, pipeline, camera, bind, picture_layout, sampler, model: None, target: None }
    }

    /// `mesh` from `view` at `size` pixels: the texture to paint.
    pub fn draw(&mut self, mesh: &Arc<Mesh>, view: &View, size: [u32; 2]) -> egui::TextureId {
        let size = size.map(|v| v.clamp(1, self.rs.device.limits().max_texture_dimension_2d));
        if !self.model.as_ref().is_some_and(|m| Arc::ptr_eq(&m.mesh, mesh)) {
            let t = std::time::Instant::now();
            self.model = Some(self.upload(mesh));
            log!("model on the graphics card: {} triangles, {} pictures, {:.0?}", mesh.tris.len(), mesh.textures.len(), t.elapsed());
        }
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            self.target = Some(self.target(size));
        }
        let (Some(model), Some(t)) = (&self.model, &self.target) else { unreachable!() };
        let (clip, eye) = render3d::camera(mesh, view, size[0], size[1]);
        let mut bytes = Vec::with_capacity(128);
        for m in [clip, eye] {
            bytes.extend(m.iter().flatten().flat_map(|v| v.to_le_bytes()));
        }
        self.rs.queue.write_buffer(&self.camera, 0, &bytes);
        let mut encoder = self.rs.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("model"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    depth_slice: None,
                    resolve_target: Some(&t.out),
                    // Transparent around the model: the viewer paints the background under it.
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            for (buffer, vertices, picture) in &model.pieces {
                pass.set_bind_group(1, &model.pictures[*picture], &[]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*vertices, 0..1);
            }
        }
        self.rs.queue.submit([encoder.finish()]);
        t.id
    }

    /// Three vertices per triangle, grouped by pictures, in pieces of at most 128 MB or the
    /// card's limit; and the pictures.
    fn upload(&self, mesh: &Arc<Mesh>) -> Model {
        let mut views = vec![self.picture(&image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])))];
        views.extend(mesh.textures.iter().map(|img| self.picture(img)));
        let keys: Vec<[u16; 5]> = (0..mesh.tris.len()).map(|i| key(mesh, i).map(|k| if (k as usize) < views.len() { k } else { 0 })).collect();
        let mut order: Vec<usize> = (0..mesh.tris.len()).collect();
        order.sort_by_key(|&i| keys[i]);
        let limit = self.rs.device.limits().max_buffer_size.min(128 << 20);
        let per = (limit / (STRIDE * 3)).max(1) as usize;
        let (mut pieces, mut pictures, mut groups) = (Vec::new(), Vec::new(), HashMap::new());
        for group in order.chunk_by(|&a, &b| keys[a] == keys[b]) {
            let k = keys[group[0]];
            let at = *groups.entry(k).or_insert_with(|| {
                pictures.push(self.bind(&views, k));
                pictures.len() - 1
            });
            for tris in group.chunks(per) {
                let mut bytes = Vec::with_capacity(tris.len() * 3 * STRIDE as usize);
                for &i in tris {
                    vertices(mesh, i, &mut bytes);
                }
                let buffer = self.rs.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("model"), contents: &bytes, usage: wgpu::BufferUsages::VERTEX });
                pieces.push((buffer, tris.len() as u32 * 3, at));
            }
        }
        Model { mesh: mesh.clone(), pieces, pictures }
    }

    /// The pictures of one group (`key`), white where there is none.
    fn bind(&self, views: &[wgpu::TextureView], key: [u16; 5]) -> wgpu::BindGroup {
        let view = |slot: usize| wgpu::BindingResource::TextureView(&views[key[slot] as usize]);
        let entry = |binding, resource| wgpu::BindGroupEntry { binding, resource };
        self.rs.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.picture_layout,
            entries: &[entry(0, view(0)), entry(1, wgpu::BindingResource::Sampler(&self.sampler)), entry(2, view(1)), entry(3, view(2)), entry(4, view(3)), entry(5, view(4))],
        })
    }

    /// A picture on the card with its smaller copies (mipmaps), so that far away it does not
    /// shimmer.
    fn picture(&self, img: &image::RgbaImage) -> wgpu::TextureView {
        let levels = 32 - img.width().max(img.height()).leading_zeros();
        let mut data = img.as_raw().clone();
        let mut level = img.clone();
        for _ in 1..levels {
            level = half(&level);
            data.extend_from_slice(level.as_raw());
        }
        let desc = wgpu::TextureDescriptor {
            label: Some("picture"),
            size: wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        self.rs.device.create_texture_with_data(&self.rs.queue, &desc, wgpu::util::TextureDataOrder::LayerMajor, &data).create_view(&Default::default())
    }

    fn target(&self, size: [u32; 2]) -> Target {
        let texture = |format, samples, usage| {
            let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
            let d = wgpu::TextureDescriptor { label: Some("model"), size: extent, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] };
            self.rs.device.create_texture(&d).create_view(&Default::default())
        };
        let msaa = texture(FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = texture(DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let out = texture(FORMAT, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
        let mut renderer = self.rs.renderer.write();
        let id = match &self.target {
            Some(old) => {
                renderer.update_egui_texture_from_wgpu_texture(&self.rs.device, &out, wgpu::FilterMode::Linear, old.id);
                old.id
            }
            None => renderer.register_native_texture(&self.rs.device, &out, wgpu::FilterMode::Linear),
        };
        Target { size, msaa, depth, out, id }
    }
}

/// Half the size, each pixel the average of the 2×2 under it (the last row or column repeated
/// when odd): ~5 ms for 2048² where a filtered resize took ~60.
fn half(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    image::RgbaImage::from_fn((w / 2).max(1), (h / 2).max(1), |x, y| {
        let (x0, y0) = ((2 * x).min(w - 1), (2 * y).min(h - 1));
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let p = [img.get_pixel(x0, y0), img.get_pixel(x1, y0), img.get_pixel(x0, y1), img.get_pixel(x1, y1)];
        image::Rgba(std::array::from_fn(|c| ((p.iter().map(|q| q[c] as u32).sum::<u32>() + 2) / 4) as u8))
    })
}

/// The surface of triangle `i`, if it has one.
fn surface(mesh: &Mesh, i: usize) -> Option<&Surface> {
    mesh.surfaces.get((*mesh.surface.get(i)? as usize).checked_sub(1)?)
}

/// Triangle `i`'s pictures: colour, bumps, roughness and metal, occlusion, glow (0 = none).
fn key(mesh: &Mesh, i: usize) -> [u16; 5] {
    let tex = mesh.tex.get(i).copied().unwrap_or(0);
    surface(mesh, i).map_or([tex, 0, 0, 0, 0], |s| [tex, s.bumps, s.rough_metal, s.occlusion_map, s.glow_map])
}

/// The three vertices of triangle `i` (`STRIDE` bytes each).
fn vertices(mesh: &Mesh, i: usize, out: &mut Vec<u8>) {
    let [r, g, b] = mesh.colors.get(i).copied().unwrap_or(DEFAULT);
    let uv = mesh.uv.get(i).copied().unwrap_or_default();
    let normals = mesh.normals.get(i).copied().unwrap_or_default();
    let unorm = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (pbr, glow) = match surface(mesh, i) {
        Some(s) => ([unorm(s.metallic), unorm(s.roughness), unorm(s.occlusion), 1 + if s.bumps > 0 { 2 } else { 0 }], s.glow.map(unorm)),
        None => ([0, 255, 0, 0], [0; 3]),
    };
    for (k, v) in mesh.tris[i].iter().enumerate() {
        out.extend(mesh.pos[*v as usize].iter().flat_map(|f| f.to_le_bytes()));
        out.extend([r, g, b, 255]);
        out.extend(uv[k].iter().chain(&normals[k]).flat_map(|f| f.to_le_bytes()));
        out.extend(pbr);
        out.extend([glow[0], glow[1], glow[2], 255]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// eframe's device as it asks for it (limits, label), only with the small memory blocks.
    #[test]
    fn frugal_keeps_the_rest() {
        let base = wgpu::DeviceDescriptor { label: Some("egui wgpu device"), required_limits: wgpu::Limits { max_texture_dimension_2d: 8192, ..Default::default() }, ..Default::default() };
        let d = frugal_descriptor(base);
        assert!(matches!(d.memory_hints, wgpu::MemoryHints::MemoryUsage));
        assert_eq!((d.label, d.required_limits.max_texture_dimension_2d), (Some("egui wgpu device"), 8192));
    }
}
