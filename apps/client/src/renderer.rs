use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    mem::{size_of, size_of_val},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytemuck::{Pod, Zeroable};
use tracing::{debug, error, info, warn};
use veldwake_streaming::LodLevel;
use veldwake_voxel::{CHUNK_EDGE, ChunkCoord, Mesh, VoxelId};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, event_loop::OwnedDisplayHandle, window::Window};

use crate::{
    camera::Camera,
    debug::{DebugPrimitive, DebugShape},
    streaming::{ChunkPresentation, ChunkUploadError, GpuResidency, PresentationCommitError},
};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
    /// `x` is how far a `Lod1` mesh is blended toward the debug tint; the rest
    /// is WGSL alignment padding.
    debug: [f32; 4],
}

/// One debug wireframe primitive: where to place the unit geometry and what
/// color to draw its lines.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DebugPrimitiveUniform {
    /// `xyz` world origin, `w` edge length in world units.
    placement: [f32; 4],
    color: [f32; 4],
}

impl DebugPrimitiveUniform {
    fn from_primitive(primitive: &DebugPrimitive) -> Self {
        let (origin, edge) = primitive.placement();
        Self {
            placement: [origin[0], origin[1], origin[2], edge],
            color: [
                primitive.color[0],
                primitive.color[1],
                primitive.color[2],
                1.0,
            ],
        }
    }
}

/// Position-only vertex of the shared unit-cube line geometry.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct DebugVertex {
    position: [f32; 3],
}

impl DebugVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Vertices of the 12 unit-cube edges, drawn as a `LineList`.
const CUBE_EDGE_VERTICES: u32 = 24;
/// Vertices of one face outline: four lines.
const FACE_OUTLINE_VERTICES: u32 = 8;

/// The one shared line buffer: the cube edges first, then the six face
/// outlines in `Face::ALL` order. Every primitive is this geometry placed and
/// scaled by its uniform, so no debug geometry is ever uploaded per frame.
fn debug_line_vertices() -> Vec<DebugVertex> {
    const CORNERS: [[f32; 3]; 8] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    // Face::ALL order: -x, +x, -y, +y, -z, +z.
    const FACES: [[usize; 4]; 6] = [
        [0, 3, 7, 4],
        [1, 2, 6, 5],
        [0, 1, 5, 4],
        [3, 2, 6, 7],
        [0, 1, 2, 3],
        [4, 5, 6, 7],
    ];

    let mut vertices =
        Vec::with_capacity(CUBE_EDGE_VERTICES as usize + 6 * FACE_OUTLINE_VERTICES as usize);
    for (from, to) in EDGES {
        vertices.push(DebugVertex {
            position: CORNERS[from],
        });
        vertices.push(DebugVertex {
            position: CORNERS[to],
        });
    }
    for face in FACES {
        for corner in 0..4 {
            vertices.push(DebugVertex {
                position: CORNERS[face[corner]],
            });
            vertices.push(DebugVertex {
                position: CORNERS[face[(corner + 1) % 4]],
            });
        }
    }
    vertices
}

/// Where a shape lives in the shared line buffer.
fn debug_vertex_range(shape: DebugShape) -> std::ops::Range<u32> {
    match shape {
        DebugShape::Box => 0..CUBE_EDGE_VERTICES,
        DebugShape::Face(face) => {
            let base = CUBE_EDGE_VERTICES + face as u32 * FACE_OUTLINE_VERTICES;
            base..base + FACE_OUTLINE_VERTICES
        }
    }
}

/// One reusable uniform buffer and bind group for a debug primitive. The pool
/// grows to the largest primitive count seen and is then reused, so an active
/// debug view allocates nothing per frame.
struct DebugSlot {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Debug-only work requested for the current frame. Fixed startup resources
/// and slots retained from earlier debug use are intentionally not included.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DebugFrameWork {
    pub primitive_allocations: usize,
    pub uniform_writes: usize,
    pub draw_calls: usize,
}

impl DebugFrameWork {
    const fn planned(existing_slots: usize, primitives: usize) -> Self {
        Self {
            primitive_allocations: primitives.saturating_sub(existing_slots),
            uniform_writes: primitives,
            draw_calls: primitives,
        }
    }
}

fn create_debug_slot(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> DebugSlot {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("M3C debug primitive uniform"),
        size: size_of::<DebugPrimitiveUniform>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("M3C debug primitive bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    DebugSlot { buffer, bind_group }
}

/// Two `vec4` for WGSL uniform alignment: the chunk origin (never scaled)
/// and the local cell size of the level in `x`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ModelUniform {
    translation: [f32; 4],
    scale: [f32; 4],
}

impl ModelUniform {
    fn from_chunk(coord: ChunkCoord, lod: LodLevel) -> Self {
        let edge = CHUNK_EDGE as f32;
        Self {
            translation: [
                coord.x as f32 * edge,
                coord.y as f32 * edge,
                coord.z as f32 * edge,
                0.0,
            ],
            scale: [level_scale(lod), 0.0, 0.0, 0.0],
        }
    }

    #[cfg(test)]
    /// World-space bounds of the chunk volume this uniform places: the
    /// `Lod1` grid has half the cells at twice the size, so both levels span
    /// exactly `CHUNK_EDGE` world units from the same origin.
    fn world_bounds(&self, lod: LodLevel) -> ([f32; 3], [f32; 3]) {
        let cells = match lod {
            LodLevel::Lod0 => CHUNK_EDGE,
            LodLevel::Lod1 => veldwake_voxel::COARSE_EDGE,
        } as f32;
        let extent = cells * self.scale[0];
        let min = [
            self.translation[0],
            self.translation[1],
            self.translation[2],
        ];
        (min, [min[0] + extent, min[1] + extent, min[2] + extent])
    }
}

const fn level_scale(lod: LodLevel) -> f32 {
    match lod {
        LodLevel::Lod0 => 1.0,
        LodLevel::Lod1 => 2.0,
    }
}

impl CameraUniform {
    fn from_camera(camera: &Camera, lod_tint: f32) -> Self {
        Self {
            view_projection: camera.view_projection().to_cols_array_2d(),
            debug: [lod_tint, 0.0, 0.0, 0.0],
        }
    }
}

/// Disposable GPU buffers of one chunk mesh at one level. Holds no
/// streaming stamps.
struct GpuMeshBuffers {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    model_bind_group: wgpu::BindGroup,
    _model_buffer: wgpu::Buffer,
    bytes: usize,
    quads: usize,
    /// Presentation level this mesh was uploaded at.
    lod: LodLevel,
}

/// The drawn mesh of a chunk plus an optional staged replacement that is
/// uploaded but not drawn until the bridge commits its transition group.
/// `None` in `staged` with `staged_empty` set means "replace with nothing".
#[derive(Default)]
struct GpuChunkSlot {
    active: Option<GpuMeshBuffers>,
    /// Drawable this frame. Cleared immediately when the bridge revokes it.
    drawable: bool,
    staged: Option<GpuMeshBuffers>,
    staged_empty: bool,
}

/// Exact GPU bytes a mesh occupies: converted vertices, `u32` indices, model uniform.
fn gpu_payload_bytes(mesh: &Mesh) -> usize {
    mesh.vertices().len() * size_of::<Vertex>()
        + size_of_val(mesh.indices())
        + size_of::<ModelUniform>()
}

#[derive(Debug)]
pub enum RendererInitError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    MissingSurfaceFormat,
    MissingPresentMode,
    MissingAlphaMode,
}

impl Display for RendererInitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Surface(error) => write!(formatter, "failed to create GPU surface: {error}"),
            Self::Adapter(error) => write!(formatter, "failed to select GPU adapter: {error}"),
            Self::Device(error) => write!(formatter, "failed to create GPU device: {error}"),
            Self::MissingSurfaceFormat => write!(formatter, "surface exposes no texture format"),
            Self::MissingPresentMode => write!(formatter, "surface exposes no present mode"),
            Self::MissingAlphaMode => write!(formatter, "surface exposes no alpha mode"),
        }
    }
}

impl Error for RendererInitError {}

#[derive(Debug, Eq, PartialEq)]
pub enum RenderOutcome {
    Rendered,
    Retry,
    Reconfigured,
    Suspended,
    Fatal,
}

pub struct Renderer {
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    configured: bool,
    pipeline: wgpu::RenderPipeline,
    model_layout: wgpu::BindGroupLayout,
    chunks: BTreeMap<ChunkCoord, GpuChunkSlot>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    fatal_gpu_error: Arc<AtomicBool>,
    /// Separate `LineList` pipeline for the debug views. Nothing below is
    /// touched while the views are off.
    debug_pipeline: wgpu::RenderPipeline,
    debug_layout: wgpu::BindGroupLayout,
    debug_vertices: wgpu::Buffer,
    debug_slots: Vec<DebugSlot>,
    debug_draws: Vec<(usize, std::ops::Range<u32>)>,
    debug_frame_work: DebugFrameWork,
}

impl Renderer {
    pub async fn new(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
        camera: &Camera,
    ) -> Result<Self, RendererInitError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance
            .create_surface(window.clone())
            .map_err(RendererInitError::Surface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                apply_limit_buckets: false,
            })
            .await
            .map_err(RendererInitError::Adapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Veldwake M3B device"),
                required_features: wgpu::Features::empty(),
                ..Default::default()
            })
            .await
            .map_err(RendererInitError::Device)?;

        let fatal_gpu_error = Arc::new(AtomicBool::new(false));
        install_error_handlers(&device, &fatal_gpu_error);

        let adapter_info = adapter.get_info();
        let capabilities = surface.get_capabilities(&adapter);
        let format = select_surface_format(&capabilities.formats)
            .ok_or(RendererInitError::MissingSurfaceFormat)?;
        let present_mode = select_present_mode(&capabilities.present_modes)
            .ok_or(RendererInitError::MissingPresentMode)?;
        let alpha_mode = select_alpha_mode(&capabilities.alpha_modes)
            .ok_or(RendererInitError::MissingAlphaMode)?;
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![format],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };

        let camera_uniform = CameraUniform::from_camera(camera, 0.0);
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("diagnostic camera uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("diagnostic camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<CameraUniform>() as u64),
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("diagnostic camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("diagnostic chunk model bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<ModelUniform>() as u64),
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("diagnostic.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("M3B diagnostic pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&model_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M3B diagnostic voxel pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let debug_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("M3C debug primitive bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        size_of::<DebugPrimitiveUniform>() as u64
                    ),
                },
                count: None,
            }],
        });
        let debug_shader = device.create_shader_module(wgpu::include_wgsl!("debug_line.wgsl"));
        let debug_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("M3C debug line pipeline layout"),
                bind_group_layouts: &[Some(&camera_layout), Some(&debug_layout)],
                immediate_size: 0,
            });
        let debug_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M3C debug line pipeline"),
            layout: Some(&debug_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &debug_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(DebugVertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            // Lines are an overlay on the world: occluded by geometry in front
            // of them, but never writing depth over the meshes.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &debug_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let debug_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("M3C debug unit line geometry"),
            contents: bytemuck::cast_slice(&debug_line_vertices()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let depth_view = create_depth_view(&device, config.width, config.height);

        let mut renderer = Self {
            instance,
            window,
            surface,
            adapter,
            device,
            queue,
            config,
            size,
            configured: false,
            pipeline,
            model_layout,
            chunks: BTreeMap::new(),
            camera_buffer,
            camera_bind_group,
            depth_view,
            fatal_gpu_error,
            debug_pipeline,
            debug_layout,
            debug_vertices,
            debug_slots: Vec::new(),
            debug_draws: Vec::new(),
            debug_frame_work: DebugFrameWork::default(),
        };
        renderer.configure_if_visible();
        renderer.log_configuration(&adapter_info, &capabilities);
        Ok(renderer)
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
        if size.width == 0 || size.height == 0 {
            self.configured = false;
            debug!(
                width = size.width,
                height = size.height,
                "surface suspended at zero size"
            );
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, size.width, size.height);
        self.configured = true;
        debug!(
            width = size.width,
            height = size.height,
            "surface reconfigured"
        );
    }

    /// Uploads the camera and the `Lod1` debug tint strength (`0.0` when the
    /// `Lod` view is not active).
    pub fn update_camera(&self, camera: &Camera, lod_tint: f32) {
        let uniform = CameraUniform::from_camera(camera, lod_tint);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    /// Replaces the debug primitives drawn this frame.
    ///
    /// An empty slice writes nothing and draws nothing, so a disabled debug
    /// view costs no uploads and no draw calls. The uniform pool is reused
    /// across frames and never shrinks below the largest set seen.
    pub fn set_debug_primitives(&mut self, primitives: &[DebugPrimitive]) {
        self.debug_draws.clear();
        self.debug_frame_work = DebugFrameWork::planned(self.debug_slots.len(), primitives.len());
        if primitives.is_empty() {
            return;
        }
        while self.debug_slots.len() < primitives.len() {
            self.debug_slots
                .push(create_debug_slot(&self.device, &self.debug_layout));
        }
        for (index, primitive) in primitives.iter().enumerate() {
            let uniform = DebugPrimitiveUniform::from_primitive(primitive);
            self.queue.write_buffer(
                &self.debug_slots[index].buffer,
                0,
                bytemuck::bytes_of(&uniform),
            );
            self.debug_draws
                .push((index, debug_vertex_range(primitive.shape)));
        }
    }

    /// Debug line draw calls issued in the last frame: one per primitive.
    #[must_use]
    pub fn debug_draw_count(&self) -> usize {
        self.debug_draws.len()
    }

    /// Uniform buffers and bind groups allocated for debug primitives. This is
    /// the cost of the one-bind-group-per-box diagnostic pattern.
    #[must_use]
    pub fn debug_slot_count(&self) -> usize {
        self.debug_slots.len()
    }

    /// Debug-only allocations, uniform writes, and draw calls for this frame.
    /// In `Off`, all three are zero even if slots from an earlier debug mode
    /// remain retained in the reusable pool.
    #[must_use]
    pub const fn debug_frame_work(&self) -> DebugFrameWork {
        self.debug_frame_work
    }

    pub fn render(&mut self) -> RenderOutcome {
        if self.fatal_gpu_error.load(Ordering::Acquire) {
            error!("GPU device reported a fatal error; stopping render loop");
            return RenderOutcome::Fatal;
        }
        if !self.configured {
            return RenderOutcome::Suspended;
        }

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                self.reconfigure();
                return RenderOutcome::Reconfigured;
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                warn!("surface acquisition timed out; frame skipped");
                return RenderOutcome::Retry;
            }
            wgpu::CurrentSurfaceTexture::Occluded => return RenderOutcome::Suspended,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.reconfigure();
                return RenderOutcome::Reconfigured;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                return self.recreate_lost_surface();
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                error!("surface acquisition reported a validation error");
                return RenderOutcome::Fatal;
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                label: Some("diagnostic surface view"),
                format: Some(self.config.format),
                ..Default::default()
            });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("M3B diagnostic frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("M3B diagnostic voxel pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.035,
                            g: 0.055,
                            b: 0.085,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for chunk in self
                .chunks
                .values()
                .filter(|slot| slot.drawable)
                .filter_map(|slot| slot.active.as_ref())
            {
                pass.set_bind_group(1, &chunk.model_bind_group, &[]);
                pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }
            if !self.debug_draws.is_empty() {
                pass.set_pipeline(&self.debug_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.debug_vertices.slice(..));
                for (slot, vertices) in &self.debug_draws {
                    pass.set_bind_group(1, &self.debug_slots[*slot].bind_group, &[]);
                    pass.draw(vertices.clone(), 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(surface_texture);
        RenderOutcome::Rendered
    }

    fn configure_if_visible(&mut self) {
        if self.size.width > 0 && self.size.height > 0 {
            self.surface.configure(&self.device, &self.config);
            self.configured = true;
        }
    }

    fn reconfigure(&mut self) {
        if self.configured {
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn recreate_lost_surface(&mut self) -> RenderOutcome {
        match self.instance.create_surface(self.window.clone()) {
            Ok(surface) => {
                self.surface = surface;
                self.reconfigure();
                warn!("lost surface recreated");
                RenderOutcome::Reconfigured
            }
            Err(error) => {
                error!(%error, "failed to recreate lost surface");
                RenderOutcome::Fatal
            }
        }
    }

    fn log_configuration(
        &self,
        adapter_info: &wgpu::AdapterInfo,
        capabilities: &wgpu::SurfaceCapabilities,
    ) {
        let limits = self.adapter.limits();
        info!(
            adapter = %adapter_info.name,
            backend = ?adapter_info.backend,
            device_type = ?adapter_info.device_type,
            driver = %adapter_info.driver,
            driver_info = %adapter_info.driver_info,
            vendor = adapter_info.vendor,
            device = adapter_info.device,
            surface_format = ?self.config.format,
            present_mode = ?self.config.present_mode,
            alpha_mode = ?self.config.alpha_mode,
            width = self.size.width,
            height = self.size.height,
            max_texture_dimension_2d = limits.max_texture_dimension_2d,
            max_bind_groups = limits.max_bind_groups,
            "GPU renderer initialized"
        );
        debug!(
            formats = ?capabilities.formats,
            present_modes = ?capabilities.present_modes,
            alpha_modes = ?capabilities.alpha_modes,
            features = ?self.adapter.features(),
            "adapter and surface capabilities"
        );
    }

    /// Whether `coord` holds a staged replacement, including a staged empty
    /// mesh (a commit that removes the chunk).
    fn has_staged(&self, coord: ChunkCoord) -> bool {
        self.chunks
            .get(&coord)
            .is_some_and(|slot| slot.staged.is_some() || slot.staged_empty)
    }
}

impl ChunkPresentation for Renderer {
    fn gpu_payload_bytes(&self, mesh: &Mesh) -> usize {
        gpu_payload_bytes(mesh)
    }

    fn stage_chunk(
        &mut self,
        coord: ChunkCoord,
        lod: LodLevel,
        mesh: &Mesh,
    ) -> Result<usize, ChunkUploadError> {
        if mesh.indices().is_empty() {
            let slot = self.chunks.entry(coord).or_default();
            slot.staged = None;
            slot.staged_empty = true;
            return Ok(0);
        }
        let index_count =
            u32::try_from(mesh.indices().len()).map_err(|_| ChunkUploadError::TooManyIndices {
                coord,
                indices: mesh.indices().len(),
            })?;
        let vertices = mesh
            .vertices()
            .iter()
            .map(|vertex| Vertex {
                position: vertex.position,
                color: diagnostic_color(vertex.voxel),
            })
            .collect::<Vec<_>>();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("M3B streamed chunk vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("M3B streamed chunk indices"),
                contents: bytemuck::cast_slice(mesh.indices()),
                usage: wgpu::BufferUsages::INDEX,
            });
        let model_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("M3B streamed chunk model uniform"),
                contents: bytemuck::bytes_of(&ModelUniform::from_chunk(coord, lod)),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let model_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("M3B streamed chunk model bind group"),
            layout: &self.model_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: model_buffer.as_entire_binding(),
            }],
        });
        let bytes = gpu_payload_bytes(mesh);
        // Replacing a staged entry drops the previous staged buffers; wgpu
        // keeps them alive for any already-submitted work.
        let slot = self.chunks.entry(coord).or_default();
        slot.staged = Some(GpuMeshBuffers {
            vertex_buffer,
            index_buffer,
            index_count,
            model_bind_group,
            _model_buffer: model_buffer,
            bytes,
            quads: mesh.quad_count(),
            lod,
        });
        slot.staged_empty = false;
        Ok(bytes)
    }

    fn commit_staged_group(&mut self, group: &[ChunkCoord]) -> Result<(), PresentationCommitError> {
        // Verify the whole group before touching any slot: a partial swap
        // could draw a pair that mixes an old and a new seam.
        if let Some(missing) = group.iter().find(|coord| !self.has_staged(**coord)) {
            return Err(PresentationCommitError {
                coord: *missing,
                group_size: group.len(),
            });
        }
        for coord in group {
            // Verified above and nothing mutates `chunks` in between, so every
            // member is present; a missing one would simply hold no GPU state.
            if let Some(slot) = self.chunks.get_mut(coord) {
                slot.active = slot.staged.take();
                slot.staged_empty = false;
                slot.drawable = slot.active.is_some();
                if slot.active.is_none() {
                    self.chunks.remove(coord);
                }
            }
        }
        Ok(())
    }

    fn discard_staged(&mut self, coord: ChunkCoord) -> bool {
        let Some(slot) = self.chunks.get_mut(&coord) else {
            return false;
        };
        let had = slot.staged.take().is_some() || slot.staged_empty;
        slot.staged_empty = false;
        if slot.active.is_none() {
            self.chunks.remove(&coord);
        }
        had
    }

    fn deactivate_chunk(&mut self, coord: ChunkCoord) -> bool {
        self.chunks
            .get_mut(&coord)
            .filter(|slot| slot.active.is_some())
            .map(|slot| slot.drawable = false)
            .is_some()
    }

    fn remove_chunk(&mut self, coord: ChunkCoord) -> bool {
        self.chunks
            .remove(&coord)
            .is_some_and(|slot| slot.active.is_some())
    }

    fn residency(&self) -> GpuResidency {
        let mut residency = GpuResidency::default();
        for slot in self.chunks.values() {
            if let Some(chunk) = &slot.active {
                let level = match chunk.lod {
                    LodLevel::Lod0 => &mut residency.lod0,
                    LodLevel::Lod1 => &mut residency.lod1,
                };
                level.resident += 1;
                level.bytes += chunk.bytes;
                level.quads += chunk.quads;
                if slot.drawable {
                    level.active += 1;
                }
            }
            if let Some(chunk) = &slot.staged {
                let level = match chunk.lod {
                    LodLevel::Lod0 => &mut residency.lod0,
                    LodLevel::Lod1 => &mut residency.lod1,
                };
                level.staged += 1;
                level.staged_bytes += chunk.bytes;
            }
        }
        residency
    }
}

fn diagnostic_color(voxel: VoxelId) -> [f32; 3] {
    const PALETTE: [[f32; 3]; 8] = [
        [0.30, 0.76, 0.42],
        [0.95, 0.55, 0.20],
        [0.28, 0.60, 0.95],
        [0.82, 0.36, 0.86],
        [0.18, 0.78, 0.80],
        [0.92, 0.30, 0.32],
        [0.88, 0.82, 0.24],
        [0.62, 0.48, 0.92],
    ];

    if voxel.is_air() {
        return [0.0; 3];
    }

    PALETTE[usize::from(voxel.0 - 1) % PALETTE.len()]
}

fn install_error_handlers(device: &wgpu::Device, fatal: &Arc<AtomicBool>) {
    let device_lost = Arc::clone(fatal);
    device.set_device_lost_callback(move |reason, message| {
        error!(?reason, %message, "GPU device lost");
        device_lost.store(true, Ordering::Release);
    });

    let uncaptured_fatal = Arc::clone(fatal);
    device.on_uncaptured_error(Arc::new(move |gpu_error| match gpu_error {
        wgpu::Error::OutOfMemory { source } => {
            error!(%source, "GPU out of memory");
            uncaptured_fatal.store(true, Ordering::Release);
        }
        wgpu::Error::Validation {
            source,
            description,
        } => {
            error!(%source, %description, "uncaptured GPU validation error");
            uncaptured_fatal.store(true, Ordering::Release);
        }
        wgpu::Error::Internal {
            source,
            description,
        } => {
            error!(%source, %description, "internal GPU error");
            uncaptured_fatal.store(true, Ordering::Release);
        }
    }));
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("diagnostic depth texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn select_surface_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| formats.first().copied())
}

fn select_present_mode(modes: &[wgpu::PresentMode]) -> Option<wgpu::PresentMode> {
    [wgpu::PresentMode::Fifo, wgpu::PresentMode::AutoVsync]
        .into_iter()
        .find(|candidate| modes.contains(candidate))
        .or_else(|| modes.first().copied())
}

fn select_alpha_mode(modes: &[wgpu::CompositeAlphaMode]) -> Option<wgpu::CompositeAlphaMode> {
    modes
        .iter()
        .copied()
        .find(|mode| *mode == wgpu::CompositeAlphaMode::Auto)
        .or_else(|| modes.first().copied())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        CUBE_EDGE_VERTICES, DebugFrameWork, DebugPrimitiveUniform, FACE_OUTLINE_VERTICES,
        ModelUniform, debug_line_vertices, debug_vertex_range, diagnostic_color, gpu_payload_bytes,
        select_alpha_mode, select_present_mode, select_surface_format,
    };
    use crate::debug::{DebugKind, DebugPrimitive, DebugShape};
    use veldwake_streaming::LodLevel;
    use veldwake_voxel::{ChunkCoord, Face, Mesh, VoxelId, diagnostic_fixture, mesh_exposed_faces};

    #[test]
    fn debug_off_plans_no_per_frame_debug_work_even_with_retained_slots() {
        assert_eq!(DebugFrameWork::planned(0, 0), DebugFrameWork::default());
        assert_eq!(DebugFrameWork::planned(637, 0), DebugFrameWork::default());
        assert_eq!(
            DebugFrameWork::planned(100, 180),
            DebugFrameWork {
                primitive_allocations: 80,
                uniform_writes: 180,
                draw_calls: 180,
            }
        );
        assert_eq!(
            DebugFrameWork::planned(637, 180),
            DebugFrameWork {
                primitive_allocations: 0,
                uniform_writes: 180,
                draw_calls: 180,
            }
        );
    }

    #[test]
    fn debug_line_geometry_holds_the_cube_edges_then_every_face_outline() {
        let vertices = debug_line_vertices();
        assert_eq!(
            vertices.len(),
            CUBE_EDGE_VERTICES as usize + 6 * FACE_OUTLINE_VERTICES as usize
        );

        // Twelve distinct cube edges, each along exactly one axis.
        let mut edges = BTreeSet::new();
        let (edge_pairs, _) = vertices[..CUBE_EDGE_VERTICES as usize].as_chunks::<2>();
        for pair in edge_pairs {
            let from = pair[0].position.map(f32::to_bits);
            let to = pair[1].position.map(f32::to_bits);
            let differing = (0..3).filter(|axis| from[*axis] != to[*axis]).count();
            assert_eq!(differing, 1, "a cube edge moves along one axis");
            let mut key = [from, to];
            key.sort();
            assert!(edges.insert(key), "no cube edge is drawn twice");
        }
        assert_eq!(edges.len(), 12);

        // Each face outline lies on its own plane of the unit cube.
        for (index, face) in Face::ALL.into_iter().enumerate() {
            let range = debug_vertex_range(DebugShape::Face(face));
            assert_eq!(
                range.start,
                CUBE_EDGE_VERTICES + index as u32 * FACE_OUTLINE_VERTICES
            );
            let (axis, plane) = match face {
                Face::NegativeX => (0, 0.0),
                Face::PositiveX => (0, 1.0),
                Face::NegativeY => (1, 0.0),
                Face::PositiveY => (1, 1.0),
                Face::NegativeZ => (2, 0.0),
                Face::PositiveZ => (2, 1.0),
            };
            let outline = &vertices[range.start as usize..range.end as usize];
            assert_eq!(outline.len(), FACE_OUTLINE_VERTICES as usize);
            for vertex in outline {
                assert_eq!(vertex.position[axis], plane, "{face:?} outline off plane");
            }
        }
        assert_eq!(debug_vertex_range(DebugShape::Box), 0..CUBE_EDGE_VERTICES);
    }

    #[test]
    fn a_debug_uniform_places_and_colors_the_shared_unit_geometry() {
        let primitive = DebugPrimitive {
            coord: ChunkCoord::new(2, -1, 0),
            kind: DebugKind::Presented(LodLevel::Lod1),
            shape: DebugShape::Box,
            inset: 4.0,
            color: [0.25, 0.5, 0.75],
        };
        let uniform = DebugPrimitiveUniform::from_primitive(&primitive);
        assert_eq!(uniform.placement, [68.0, -28.0, 4.0, 24.0]);
        assert_eq!(uniform.color, [0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn gpu_payload_bytes_are_exact_for_the_uploaded_layout() {
        assert_eq!(std::mem::size_of::<ModelUniform>(), 32);
        assert_eq!(gpu_payload_bytes(&Mesh::default()), 32);
        let mesh = mesh_exposed_faces(&diagnostic_fixture());
        // 528 vertices of 24 bytes, 792 u32 indices, one 32-byte model uniform.
        assert_eq!(mesh.vertices().len(), 528);
        assert_eq!(mesh.indices().len(), 792);
        assert_eq!(gpu_payload_bytes(&mesh), 528 * 24 + 792 * 4 + 32);
    }

    #[test]
    fn both_levels_span_the_same_world_volume_at_positive_and_negative_chunks() {
        for coord in [ChunkCoord::new(3, 1, -2), ChunkCoord::new(-1, -1, -1)] {
            let fine = ModelUniform::from_chunk(coord, LodLevel::Lod0);
            let coarse = ModelUniform::from_chunk(coord, LodLevel::Lod1);
            assert_eq!(
                fine.translation, coarse.translation,
                "origin is never scaled"
            );
            assert_eq!(fine.scale, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(coarse.scale, [2.0, 0.0, 0.0, 0.0]);
            let expected_min = [
                coord.x as f32 * 32.0,
                coord.y as f32 * 32.0,
                coord.z as f32 * 32.0,
            ];
            let expected_max = [
                expected_min[0] + 32.0,
                expected_min[1] + 32.0,
                expected_min[2] + 32.0,
            ];
            assert_eq!(
                fine.world_bounds(LodLevel::Lod0),
                (expected_min, expected_max)
            );
            assert_eq!(
                coarse.world_bounds(LodLevel::Lod1),
                (expected_min, expected_max)
            );
        }
    }

    #[test]
    fn chunk_model_translation_preserves_signed_chunk_offsets() {
        assert_eq!(
            ModelUniform::from_chunk(ChunkCoord::new(-1, 2, 3), LodLevel::Lod0).translation,
            [-32.0, 64.0, 96.0, 0.0]
        );
        assert_eq!(
            ModelUniform::from_chunk(ChunkCoord::new(-1, 2, 3), LodLevel::Lod1).translation,
            [-32.0, 64.0, 96.0, 0.0]
        );
    }

    #[test]
    fn diagnostic_voxel_colors_are_stable_and_distinct() {
        assert_eq!(diagnostic_color(VoxelId::AIR), [0.0; 3]);
        assert_eq!(diagnostic_color(VoxelId(1)), [0.30, 0.76, 0.42]);
        assert_eq!(diagnostic_color(VoxelId(2)), [0.95, 0.55, 0.20]);
        assert_eq!(diagnostic_color(VoxelId(7)), [0.88, 0.82, 0.24]);
    }

    #[test]
    fn surface_format_prefers_srgb_and_has_a_fallback() {
        let formats = [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ];
        assert_eq!(
            select_surface_format(&formats),
            Some(wgpu::TextureFormat::Bgra8UnormSrgb)
        );
        assert_eq!(
            select_surface_format(&[wgpu::TextureFormat::Rgba16Float]),
            Some(wgpu::TextureFormat::Rgba16Float)
        );
        assert_eq!(select_surface_format(&[]), None);
    }

    #[test]
    fn present_mode_prefers_portable_vsync() {
        let modes = [wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo];
        assert_eq!(select_present_mode(&modes), Some(wgpu::PresentMode::Fifo));
        assert_eq!(select_present_mode(&[]), None);
    }

    #[test]
    fn alpha_mode_prefers_auto_and_has_a_fallback() {
        let modes = [
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::Auto,
        ];
        assert_eq!(
            select_alpha_mode(&modes),
            Some(wgpu::CompositeAlphaMode::Auto)
        );
        assert_eq!(select_alpha_mode(&[]), None);
    }
}
