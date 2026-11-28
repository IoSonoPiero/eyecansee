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

use crate::model::Mesh;
use crate::render3d::{self, View};

mod prepare;
use crate::settings::Graphics;

const SAMPLES: u32 = 4;
/// Not sRGB: egui reads textures as gamma-encoded values, which is what `render3d` computes too.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// With a stencil for the section's fill (`Pass`).
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;
/// Bytes of one vertex: position (3 × f32), colour (4 × u8), place on the pictures (2 × f32),
/// normal (3 × f32, zero for the face's own), metal, roughness, occlusion and what applies (4 ×
/// u8: 1 = the surface, 2 = bumps), glow (4 × u8).
const STRIDE: u64 = 44;
/// Bytes of the camera: two 4×4 matrices, the plane of a section and the colour of its cut.
const CAMERA: u64 = 160;

const SHADER: &str = r"
// `plane` and `cap` for a section (`render3d::Cut`): cap.w 1 when there is one.
struct Camera { clip: mat4x4<f32>, eye: mat4x4<f32>, plane: vec4<f32>, cap: vec4<f32> };
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
    @location(6) world: vec3<f32>,
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
    o.world = p;
    return o;
}

fn unit(v: vec3<f32>) -> vec3<f32> {
    let l = dot(v, v);
    return select(vec3(0.0), v * inverseSqrt(l), l > 0.0);
}

// Flat shading (the triangle's normal from the derivatives, both faces lit) as render3d; with a
// surface the same light as render_shine: change both together.
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    // A section as render3d draws it: nothing beyond the plane, but the cut's own face (4).
    let what = u32(round(i.pbr.w * 255.0));
    if (cam.cap.w > 0.5 && (what & 4u) == 0u && dot(cam.plane.xyz, i.world) > cam.plane.w) {
        discard;
    }
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
    // The face of a section (4): lighter than the model's faces, so it stands out.
    if ((what & 4u) != 0u) {
        return vec4(color * (0.65 + 0.35 * abs(dot(face, l))), 1.0);
    }
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

/// The window is drawn with DirectX 12 (`adapter_selector`).
static STRETCHES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// While the window changes size, the last picture drawn is shown stretched over the new size
/// (DirectX 12, `DXGI_SCALING_STRETCH` in wgpu), and not left in a corner or as the card likes.
pub fn stretches() -> bool {
    STRETCHES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Picks the graphics card as eframe would (the fast one, or the saving one with
/// `WGPU_POWER_PREF=low`), and notes whether it can present in "mailbox" mode for this window:
/// the app then never waits for the desktop to show a frame. Measured on a hybrid laptop with the
/// monitor on the NVIDIA card and the desktop composed on the Intel one: windows stalled up to
/// 0.9 s with the default (FIFO), mailbox was clearly smoother (2026-11-01, `present_mode`).
pub fn adapter_selector(graphics: Graphics) -> eframe::egui_wgpu::NativeAdapterSelectorMethod {
    Arc::new(move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| {
        let saving = std::env::var("WGPU_POWER_PREF").is_ok_and(|v| v.eq_ignore_ascii_case("low"));
        let wanted = if saving { wgpu::DeviceType::IntegratedGpu } else { wgpu::DeviceType::DiscreteGpu };
        let usable: Vec<&wgpu::Adapter> = adapters.iter().filter(|a| surface.is_none_or(|s| a.is_surface_supported(s))).collect();
        // On Windows the one chosen in the Options first among cards of the same kind, DirectX 12
        // unless changed: while the window changes size it stretches the last picture drawn over
        // the new size, on every card, which entering full screen counts on
        // (`viewer::enter_full`); with Vulkan an NVIDIA card left it in the top left corner (#171).
        let first = cfg!(windows).then_some(match graphics {
            Graphics::Directx12 => wgpu::Backend::Dx12,
            Graphics::Vulkan => wgpu::Backend::Vulkan,
        });
        let cards: Vec<_> = usable.iter().map(|a| a.get_info()).map(|i| (i.device_type, i.backend)).collect();
        // Every card found, for whoever reads the log of a computer that draws badly (#319).
        for a in adapters {
            let i = a.get_info();
            let can = usable.iter().any(|u| std::ptr::eq(*u, a));
            // An OpenGL card is dropped when its EGL has no native-renderable config, which wgpu
            // asks for on Linux: the NVIDIA proprietary driver has none (#321, gfx-rs/wgpu#10708).
            let why = match (can, i.backend) {
                (true, _) => "",
                (false, wgpu::Backend::Gl) => ", cannot draw this window (its EGL has no native-renderable config, which wgpu's OpenGL asks for: gfx-rs/wgpu#10708)",
                (false, _) => ", cannot draw this window",
            };
            log!("graphics card found: {} ({:?}, {:?}, driver {}){why}", i.name, i.device_type, i.backend, i.driver);
        }
        let chosen = pick(&cards, wanted, first).map(|i| usable[i]).ok_or("no graphics card can draw this window")?;
        let mailbox = surface.is_some_and(|s| s.get_capabilities(chosen).present_modes.contains(&wgpu::PresentMode::Mailbox));
        MAILBOX.store(mailbox, std::sync::atomic::Ordering::Relaxed);
        STRETCHES.store(chosen.get_info().backend == wgpu::Backend::Dx12, std::sync::atomic::Ordering::Relaxed);
        Ok((*chosen).clone())
    })
}

/// The card the window is drawn with, for Help > About (#319).
static CARD: std::sync::OnceLock<wgpu::AdapterInfo> = std::sync::OnceLock::new();

/// Keeps the card the window is drawn with (`App::with_gpu`).
pub(crate) fn keep_card(info: wgpu::AdapterInfo) {
    let _ = CARD.set(info);
}

/// The card the window is drawn with, in words: its name, then its kind, the kind of drawing and
/// the driver ("Intel(R) Iris(R) Xe Graphics · integrated card · Vulkan · Mesa 26.2.6"). `None`
/// before the window is made.
pub(crate) fn card_text(i18n: &crate::i18n::I18n) -> Option<String> {
    let i = CARD.get()?;
    let kind = match i.device_type {
        wgpu::DeviceType::DiscreteGpu => "gpu-discrete",
        wgpu::DeviceType::IntegratedGpu => "gpu-integrated",
        wgpu::DeviceType::VirtualGpu => "gpu-virtual",
        wgpu::DeviceType::Cpu => "gpu-cpu",
        wgpu::DeviceType::Other => "gpu-other",
    };
    let backend = match i.backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Gl => "OpenGL",
        wgpu::Backend::Dx12 => "DirectX 12",
        wgpu::Backend::Metal => "Metal",
        _ => "",
    };
    let driver = format!("{} {}", i.driver, i.driver_info);
    let parts = [i.name.as_str(), &i18n.t(kind), backend, driver.trim()];
    Some(parts.iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · "))
}

/// The card to draw with, by kind: the one `wanted`, then a dedicated card, an integrated one, a
/// virtual or other one, the processor last (Mesa's llvmpipe, Windows' WARP). Between cards of
/// the same kind `first` (the kind of drawing chosen in the Options, on Windows) and then wgpu's
/// own order, which puts Vulkan first: without the kind, a Vulkan that exists only as llvmpipe
/// came before an integrated card that draws with OpenGL (#318).
fn pick(cards: &[(wgpu::DeviceType, wgpu::Backend)], wanted: wgpu::DeviceType, first: Option<wgpu::Backend>) -> Option<usize> {
    let kind = |t: wgpu::DeviceType| match t {
        _ if t == wanted => 0,
        wgpu::DeviceType::DiscreteGpu => 1,
        wgpu::DeviceType::IntegratedGpu => 2,
        wgpu::DeviceType::VirtualGpu => 3,
        wgpu::DeviceType::Other => 4,
        wgpu::DeviceType::Cpu => 5,
    };
    (0..cards.len()).min_by_key(|&i| (kind(cards[i].0), first.is_some_and(|b| cards[i].1 != b), i))
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

/// What a frame does for the model it draws (`Gpu::draw`, audit 4.12).
#[derive(Debug, PartialEq)]
enum Step {
    /// It is on the card: drawn.
    Draw,
    /// Its bytes asked of a thread of their own; nothing drawn yet.
    Start,
    /// What the thread made put on the card; drawn once it is all there.
    Take,
}

/// `on_card`: the model is on the card; `coming`: its bytes are being made (`Step`). They were
/// made inside the frame: with millions of triangles the window stopped for that time.
fn step(on_card: bool, coming: bool) -> Step {
    match (on_card, coming) {
        (true, _) => Step::Draw,
        (false, false) => Step::Start,
        (false, true) => Step::Take,
    }
}

/// A model on its way to the card: the parts its thread makes (`prepare::model`), put on the
/// card as they come, a piece at a time.
struct Coming {
    mesh: Arc<Mesh>,
    parts: std::sync::mpsc::Receiver<prepare::Part>,
    views: Vec<wgpu::TextureView>,
    pieces: Vec<Piece>,
    pictures: Vec<wgpu::BindGroup>,
    groups: HashMap<[u16; 5], usize>,
    started: std::time::Instant,
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
    /// White pictures, for the face of a section.
    plain: wgpu::BindGroup,
}

pub struct Gpu {
    rs: RenderState,
    pipeline: wgpu::RenderPipeline,
    /// For a section (#145): the inside counted in the stencil, then the cut filled where it is.
    stencil: wgpu::RenderPipeline,
    cap: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    picture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    model: Option<Model>,
    /// The model being put on the card.
    coming: Option<Coming>,
    target: Option<Target>,
    /// To be drawn again when a part of a model is made.
    ctx: egui::Context,
}

impl Gpu {
    pub fn new(rs: RenderState, ctx: egui::Context) -> Self {
        let device = &rs.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("model"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let camera = device.create_buffer(&wgpu::BufferDescriptor { label: Some("camera"), size: CAMERA, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
        let pipeline = make_pipeline(device, &pipeline_layout, &module, Pass::Model);
        let stencil = make_pipeline(device, &pipeline_layout, &module, Pass::Inside);
        let cap = make_pipeline(device, &pipeline_layout, &module, Pass::Cap);
        Gpu { rs, pipeline, stencil, cap, camera, bind, picture_layout, sampler, model: None, coming: None, target: None, ctx }
    }

    /// Lets go of the model and of the picture it is drawn into (`App::card_lets_go`); the next
    /// model drawn makes them again. At the start of a frame: the last one, which may have shown
    /// the picture, is already drawn.
    pub fn forget(&mut self) {
        self.model = None;
        self.coming = None;
        if let Some(t) = self.target.take() {
            self.rs.renderer.write().free_texture(&t.id);
        }
    }

    /// `mesh` from `view` at `size` pixels: the texture to paint; `None` while the model is on
    /// its way to the card (`Step`).
    pub fn draw(&mut self, mesh: &Arc<Mesh>, view: &View, size: [u32; 2]) -> Option<egui::TextureId> {
        let size = size.map(|v| v.clamp(1, self.rs.device.limits().max_texture_dimension_2d));
        let on_card = self.model.as_ref().is_some_and(|m| Arc::ptr_eq(&m.mesh, mesh));
        let coming = self.coming.as_ref().is_some_and(|c| Arc::ptr_eq(&c.mesh, mesh));
        match step(on_card, coming) {
            Step::Draw => {}
            Step::Start => {
                self.start(mesh);
                return None;
            }
            Step::Take => {
                if !self.take() {
                    return None;
                }
            }
        }
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            self.target = Some(self.target(size));
        }
        let (Some(model), Some(t)) = (&self.model, &self.target) else { unreachable!() };
        let (clip, eye) = render3d::camera(mesh, view, size[0], size[1]);
        let mut bytes = Vec::with_capacity(CAMERA as usize);
        for m in [clip, eye] {
            bytes.extend(m.iter().flatten().flat_map(|v| v.to_le_bytes()));
        }
        let (plane, on) = view.cut.map_or(([0.0; 4], 0.0), |c| (c.plane(mesh), 1.0));
        let cap = render3d::CAP.map(|c| f32::from(c) / 255.0);
        bytes.extend(plane.into_iter().chain([cap[0], cap[1], cap[2], on]).flat_map(|v: f32| v.to_le_bytes()));
        let cap = view.cut.filter(|c| c.fill).map(|c| {
            let mut bytes = Vec::with_capacity(6 * STRIDE as usize);
            prepare::cap_vertices(mesh, &c, &mut bytes);
            self.rs.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("cut"), contents: &bytes, usage: wgpu::BufferUsages::VERTEX })
        });
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
                    stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            let pieces = |pass: &mut wgpu::RenderPass| {
                for (buffer, vertices, picture) in &model.pieces {
                    pass.set_bind_group(1, &model.pictures[*picture], &[]);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..*vertices, 0..1);
                }
            };
            // A section: where the cut crosses the inside, counted in the stencil, its face.
            if let Some(cap) = &cap {
                pass.set_pipeline(&self.stencil);
                pieces(&mut pass);
                pass.set_pipeline(&self.cap);
                pass.set_stencil_reference(0);
                pass.set_bind_group(1, &model.plain, &[]);
                pass.set_vertex_buffer(0, cap.slice(..));
                pass.draw(0..6, 0..1);
            }
            pass.set_pipeline(&self.pipeline);
            pieces(&mut pass);
        }
        self.rs.queue.submit([encoder.finish()]);
        Some(t.id)
    }

    /// The bytes of `mesh` asked of a thread of their own (`prepare::model`): three vertices per
    /// triangle, grouped by pictures, in pieces of at most 128 MB or the card's limit, and the
    /// pictures with their copies. At most one piece waits for the window: the thread waits for
    /// it to be taken.
    fn start(&mut self, mesh: &Arc<Mesh>) {
        self.model = None;
        let per = prepare::per(self.rs.device.limits().max_buffer_size.min(128 << 20));
        let (tx, parts) = std::sync::mpsc::sync_channel(1);
        let (m, ctx) = (mesh.clone(), self.ctx.clone());
        let job = move || {
            prepare::model(&m, per, |part| {
                let sent = tx.send(part).is_ok();
                ctx.request_repaint();
                sent
            });
        };
        if let Err(e) = std::thread::Builder::new().name("model on the card".into()).spawn(job) {
            log!("model on the graphics card: no thread ({e})");
        }
        let started = std::time::Instant::now();
        self.coming = Some(Coming { mesh: mesh.clone(), parts, views: Vec::new(), pieces: Vec::new(), pictures: Vec::new(), groups: HashMap::new(), started });
    }

    /// The parts of the model on its way made meanwhile, put on the card: buffers and textures
    /// only. `true` once it is all there (`model`).
    fn take(&mut self) -> bool {
        let Some(mut c) = self.coming.take() else { return false };
        loop {
            match c.parts.try_recv() {
                Ok(prepare::Part::Pictures(p)) => c.views = p.iter().map(|p| self.picture(p)).collect(),
                Ok(prepare::Part::Piece(k, vertices, bytes)) => {
                    let at = *c.groups.entry(k).or_insert_with(|| {
                        c.pictures.push(self.bind(&c.views, k));
                        c.pictures.len() - 1
                    });
                    let buffer = self.rs.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("model"), contents: &bytes, usage: wgpu::BufferUsages::VERTEX });
                    c.pieces.push((buffer, vertices, at));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.coming = Some(c);
                    return false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if c.views.is_empty() {
            log!("model on the graphics card: its pictures never came");
            return false;
        }
        log!("model on the graphics card: {} triangles, {} pictures, {:.0?} (made outside the window's frames)", c.mesh.tris.len(), c.mesh.textures.len(), c.started.elapsed());
        let plain = self.bind(&c.views, [0; 5]);
        self.model = Some(Model { mesh: c.mesh, pieces: c.pieces, pictures: c.pictures, plain });
        true
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
    fn picture(&self, p: &prepare::Picture) -> wgpu::TextureView {
        let desc = wgpu::TextureDescriptor {
            label: Some("picture"),
            size: wgpu::Extent3d { width: p.size[0], height: p.size[1], depth_or_array_layers: 1 },
            mip_level_count: p.levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        self.rs.device.create_texture_with_data(&self.rs.queue, &desc, wgpu::util::TextureDataOrder::LayerMajor, &p.data).create_view(&Default::default())
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

/// The three passes of the model: drawn; counted in the stencil (every face, front or back,
/// flips it: odd where the ray crosses the inside of a closed model, whichever way its triangles
/// turn); the cut's face, drawn only there.
#[derive(Clone, Copy, PartialEq)]
enum Pass {
    Model,
    Inside,
    Cap,
}

fn make_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, pass: Pass) -> wgpu::RenderPipeline {
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Float32x2, 3 => Float32x3, 4 => Unorm8x4, 5 => Unorm8x4];
    let face = |compare, pass_op| wgpu::StencilFaceState { compare, fail_op: wgpu::StencilOperation::Keep, depth_fail_op: wgpu::StencilOperation::Keep, pass_op };
    let stencil = match pass {
        Pass::Model => wgpu::StencilState::default(),
        Pass::Inside => {
            let f = face(wgpu::CompareFunction::Always, wgpu::StencilOperation::Invert);
            wgpu::StencilState { front: f, back: f, read_mask: 1, write_mask: 1 }
        }
        Pass::Cap => {
            let f = face(wgpu::CompareFunction::NotEqual, wgpu::StencilOperation::Keep);
            wgpu::StencilState { front: f, back: f, read_mask: 1, write_mask: 0 }
        }
    };
    let counting = pass == Pass::Inside;
    let depth = wgpu::DepthStencilState {
        format: DEPTH,
        depth_write_enabled: Some(!counting),
        depth_compare: Some(if counting { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::Less }),
        stencil,
        bias: Default::default(),
    };
    let writes = if counting { wgpu::ColorWrites::empty() } else { wgpu::ColorWrites::ALL };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("model"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout { array_stride: STRIDE, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth),
        multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
        fragment: Some(wgpu::FragmentState { module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: writes })] }),
        multiview_mask: None,
        cache: None,
    })
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

    /// Audit 4.12: a model not on the card yet is not made ready inside the frame (its triangles
    /// sorted, 132 bytes written for each, its pictures halved down to 1 × 1: seconds with millions
    /// of triangles, the window stopped): a thread of its own makes its bytes, the frames put them
    /// on the card as they come and draw it once it is all there.
    #[test]
    fn a_model_is_not_made_ready_in_the_frame_4_12() {
        assert_eq!([step(false, false), step(false, true), step(true, false), step(true, true)], [Step::Start, Step::Take, Step::Draw, Step::Draw]);
    }

    use wgpu::{Backend as B, DeviceType as T};

    /// Vulkan only as llvmpipe, OpenGL on the integrated card (Nobara in a virtual machine with
    /// virgl, #318): the card, not the processor.
    #[test]
    fn pick_prefers_a_card_to_the_processor() {
        let cards = [(T::Cpu, B::Vulkan), (T::IntegratedGpu, B::Gl)];
        assert_eq!(pick(&cards, T::DiscreteGpu, None), Some(1));
        assert_eq!(pick(&cards, T::IntegratedGpu, None), Some(1));
    }

    /// The card asked for first, then dedicated before integrated before virtual; the processor
    /// only when nothing else draws; none with no card at all.
    #[test]
    fn pick_by_kind() {
        let cards = [(T::Cpu, B::Vulkan), (T::VirtualGpu, B::Vulkan), (T::IntegratedGpu, B::Vulkan), (T::DiscreteGpu, B::Vulkan)];
        assert_eq!(pick(&cards, T::DiscreteGpu, None), Some(3));
        assert_eq!(pick(&cards, T::IntegratedGpu, None), Some(2));
        assert_eq!(pick(&cards[..2], T::DiscreteGpu, None), Some(1));
        assert_eq!(pick(&cards[..1], T::DiscreteGpu, None), Some(0));
        assert_eq!(pick(&[], T::DiscreteGpu, None), None);
    }

    /// Same kind: the kind of drawing chosen in the Options (Windows), otherwise wgpu's order;
    /// a dedicated card still wins over the one chosen on an integrated card.
    #[test]
    fn pick_same_kind_by_backend() {
        let cards = [(T::IntegratedGpu, B::Vulkan), (T::IntegratedGpu, B::Dx12), (T::DiscreteGpu, B::Vulkan)];
        assert_eq!(pick(&cards[..2], T::DiscreteGpu, Some(B::Dx12)), Some(1));
        assert_eq!(pick(&cards[..2], T::DiscreteGpu, Some(B::Vulkan)), Some(0));
        assert_eq!(pick(&cards[..2], T::DiscreteGpu, None), Some(0));
        assert_eq!(pick(&cards, T::DiscreteGpu, Some(B::Dx12)), Some(2));
    }
}
