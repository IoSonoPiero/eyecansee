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
/// Bytes of one vertex: position (3 × f32) and colour (4 × u8).
const STRIDE: u64 = 16;

const SHADER: &str = r"
struct Camera { clip: mat4x4<f32>, eye: mat4x4<f32> };
@group(0) @binding(0) var<uniform> cam: Camera;

struct Out { @builtin(position) pos: vec4<f32>, @location(0) eye: vec3<f32>, @location(1) color: vec3<f32> };

@vertex fn vs(@location(0) p: vec3<f32>, @location(1) c: vec4<f32>) -> Out {
    var o: Out;
    o.pos = cam.clip * vec4(p, 1.0);
    o.eye = (cam.eye * vec4(p, 1.0)).xyz;
    o.color = c.rgb;
    return o;
}

// Flat shading: the triangle's normal from the derivatives; both faces lit, as render3d.
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    let n = normalize(cross(dpdx(i.eye), dpdy(i.eye)));
    let d = abs(dot(n, normalize(vec3(-0.3, 0.45, 0.84))));
    // Lit on the sRGB values, like render3d.
    return vec4(i.color * (0.28 + 0.72 * d), 1.0);
}
";

/// The picture's textures, made again when the viewer changes size.
struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    out: wgpu::TextureView,
    id: egui::TextureId,
}

/// A vertex buffer and how many vertices it holds.
type Piece = (wgpu::Buffer, u32);

pub struct Gpu {
    rs: RenderState,
    pipeline: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// The model on the card, in buffers small enough for any card, with their vertex counts.
    mesh: Option<(Arc<Mesh>, Vec<Piece>)>,
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4];
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
        Gpu { rs, pipeline, camera, bind, mesh: None, target: None }
    }

    /// `mesh` from `view` at `size` pixels: the texture to paint.
    pub fn draw(&mut self, mesh: &Arc<Mesh>, view: &View, size: [u32; 2]) -> egui::TextureId {
        let size = size.map(|v| v.clamp(1, self.rs.device.limits().max_texture_dimension_2d));
        if !self.mesh.as_ref().is_some_and(|(m, _)| Arc::ptr_eq(m, mesh)) {
            self.mesh = Some((mesh.clone(), self.upload(mesh)));
        }
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            self.target = Some(self.target(size));
        }
        let (Some((_, buffers)), Some(t)) = (&self.mesh, &self.target) else { unreachable!() };
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
            for (buffer, vertices) in buffers {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..*vertices, 0..1);
            }
        }
        self.rs.queue.submit([encoder.finish()]);
        t.id
    }

    /// Three vertices per triangle (each with the triangle's colour), in pieces of at most
    /// 128 MB or the card's limit.
    fn upload(&self, mesh: &Mesh) -> Vec<Piece> {
        let limit = self.rs.device.limits().max_buffer_size.min(128 << 20);
        let per = (limit / (STRIDE * 3)).max(1) as usize;
        mesh.tris
            .chunks(per)
            .enumerate()
            .map(|(c, tris)| {
                let mut bytes = Vec::with_capacity(tris.len() * 3 * STRIDE as usize);
                for (i, t) in tris.iter().enumerate() {
                    let [r, g, b] = mesh.colors.get(c * per + i).copied().unwrap_or(DEFAULT);
                    for v in t {
                        bytes.extend(mesh.pos[*v as usize].iter().flat_map(|f| f.to_le_bytes()));
                        bytes.extend([r, g, b, 255]);
                    }
                }
                let buffer = self.rs.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("model"), contents: &bytes, usage: wgpu::BufferUsages::VERTEX });
                (buffer, tris.len() as u32 * 3)
            })
            .collect()
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
