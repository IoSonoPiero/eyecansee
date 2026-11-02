//! The 3D model of the viewer drawn by the graphics card (`wgpu`, the one eframe already uses):
//! into a texture of our own with 4× antialiasing and a depth buffer, then shown by egui as an
//! image. Same camera and same light as `render3d` (`render3d::camera`), which stays for the grid
//! and the screenshots, made in the background without the card.

use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::{self, util::DeviceExt};

use crate::model::{DEFAULT, Mesh};
use crate::render3d::{self, View};

const SAMPLES: u32 = 4;
/// Not sRGB: egui reads textures as gamma-encoded values, which is what `render3d` computes too.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Bytes of one vertex: position (3 × f32), colour (4 × u8) and place on the picture (2 × f32).
const STRIDE: u64 = 24;

const SHADER: &str = r"
struct Camera { clip: mat4x4<f32>, eye: mat4x4<f32> };
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var picture: texture_2d<f32>;
@group(1) @binding(1) var linear: sampler;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) eye: vec3<f32>, @location(1) color: vec3<f32>, @location(2) uv: vec2<f32> };

@vertex fn vs(@location(0) p: vec3<f32>, @location(1) c: vec4<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = cam.clip * vec4(p, 1.0);
    o.eye = (cam.eye * vec4(p, 1.0)).xyz;
    o.color = c.rgb;
    o.uv = uv;
    return o;
}

// Flat shading: the triangle's normal from the derivatives; both faces lit, as render3d.
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    let n = normalize(cross(dpdx(i.eye), dpdy(i.eye)));
    let d = abs(dot(n, normalize(vec3(-0.3, 0.45, 0.84))));
    // The colour tinted by the picture (white without one), lit on the sRGB values like render3d.
    let t = textureSample(picture, linear, i.uv).rgb;
    return vec4(i.color * t * (0.28 + 0.72 * d), 1.0);
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

/// The presentation to ask for once the window exists: mailbox where the card offers it.
pub fn present_mode() -> Option<eframe::egui_wgpu::SurfaceConfig> {
    MAILBOX.load(std::sync::atomic::Ordering::Relaxed).then_some(eframe::egui_wgpu::SurfaceConfig { present_mode: wgpu::PresentMode::Mailbox, desired_maximum_frame_latency: Some(1) })
}

/// The picture's textures, made again when the viewer changes size.
struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    out: wgpu::TextureView,
    id: egui::TextureId,
}

/// A vertex buffer, how many vertices it holds and its picture (index in `Model::pictures`).
type Piece = (wgpu::Buffer, u32, usize);

/// The model on the card: its triangles in buffers small enough for any card, grouped by picture,
/// and the pictures (the first is plain white, for the triangles without one).
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
        let picture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout), Some(&picture_layout)], immediate_size: 0 });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Float32x2];
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
            self.model = Some(self.upload(mesh));
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

    /// Three vertices per triangle (each with the triangle's colour), grouped by picture, in
    /// pieces of at most 128 MB or the card's limit; and the pictures.
    fn upload(&self, mesh: &Arc<Mesh>) -> Model {
        let mut pictures = vec![self.picture(&image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])))];
        pictures.extend(mesh.textures.iter().map(|img| self.picture(img)));
        let picture = |i: usize| mesh.tex.get(i).map_or(0, |&k| k as usize).min(pictures.len() - 1);
        let mut order: Vec<usize> = (0..mesh.tris.len()).collect();
        order.sort_by_key(|&i| picture(i));
        let limit = self.rs.device.limits().max_buffer_size.min(128 << 20);
        let per = (limit / (STRIDE * 3)).max(1) as usize;
        let mut pieces = Vec::new();
        for group in order.chunk_by(|&a, &b| picture(a) == picture(b)) {
            for tris in group.chunks(per) {
                let mut bytes = Vec::with_capacity(tris.len() * 3 * STRIDE as usize);
                for &i in tris {
                    let [r, g, b] = mesh.colors.get(i).copied().unwrap_or(DEFAULT);
                    let uv = mesh.uv.get(i).copied().unwrap_or_default();
                    for (k, v) in mesh.tris[i].iter().enumerate() {
                        bytes.extend(mesh.pos[*v as usize].iter().flat_map(|f| f.to_le_bytes()));
                        bytes.extend([r, g, b, 255]);
                        bytes.extend(uv[k].iter().flat_map(|f| f.to_le_bytes()));
                    }
                }
                let buffer = self.rs.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("model"), contents: &bytes, usage: wgpu::BufferUsages::VERTEX });
                pieces.push((buffer, tris.len() as u32 * 3, picture(tris[0])));
            }
        }
        Model { mesh: mesh.clone(), pieces, pictures }
    }

    /// A picture on the card with its smaller copies (mipmaps), so that far away it does not
    /// shimmer.
    // ponytail: the copies are made by the processor when the model is shown, up to 64 × 2048²
    fn picture(&self, img: &image::RgbaImage) -> wgpu::BindGroup {
        let levels = 32 - img.width().max(img.height()).leading_zeros();
        let mut data = img.as_raw().clone();
        let mut level = img.clone();
        for _ in 1..levels {
            level = image::imageops::resize(&level, (level.width() / 2).max(1), (level.height() / 2).max(1), image::imageops::FilterType::Triangle);
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
        let texture = self.rs.device.create_texture_with_data(&self.rs.queue, &desc, wgpu::util::TextureDataOrder::LayerMajor, &data);
        let view = texture.create_view(&Default::default());
        self.rs.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.picture_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
        })
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
