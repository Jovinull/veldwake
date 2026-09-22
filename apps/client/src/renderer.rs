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
use veldwake_character::{BONE_COUNT, CompiledCharacter, PosedCharacter};
use veldwake_combat::CompiledWeapon;
use veldwake_procedural::{LandmarkMaterial, TerrainMaterial};
use veldwake_streaming::LodLevel;
use veldwake_voxel::{CHUNK_EDGE, ChunkCoord, Mesh, VoxelId};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, event_loop::OwnedDisplayHandle, window::Window};

use crate::{
    camera::Camera,
    debug::{DebugPrimitive, DebugShape},
    lighting::{Lighting, SHADOW_MAP_EDGE, Weather, shadow_centre, shadow_view_projection},
    streaming::{ChunkPresentation, ChunkUploadError, GpuResidency, PresentationCommitError},
    vfx::{MAX_VFX_INSTANCES, VfxInstance},
};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// What one frame's effects cost: draw calls, instances, and bytes uploaded.
///
/// All three are zero with nothing in flight, which is the contract rather than
/// a coincidence, and the reason it is three numbers and not a boolean.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VfxFrameWork {
    pub draws: usize,
    pub instances: u32,
    pub bytes: usize,
}

/// What the GPU needs about one meshed voxel face corner.
///
/// The normal comes from the mesher and the colour from the material table, so
/// the renderer neither derives geometry nor invents a palette. `specular` is
/// the only material property the shading model needs beyond colour, which is
/// why there is no material index and no lookup table on the GPU.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
    specular: f32,
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Everything every shader needs about the frame: where the camera is, where
/// the sun is, and what the weather is doing.
///
/// The layout is mirrored in `scene.wgsl`, which every shader in the client
/// concatenates. Scalars ride in the `w` lane of a vector because a uniform
/// buffer aligns each member to sixteen bytes anyway.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct SceneUniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    light_view_projection: [[f32; 4]; 4],
    camera_position: [f32; 4],
    /// `xyz` toward the sun, `w` sun intensity.
    sun: [f32; 4],
    /// `rgb` sun colour, `a` the fraction of the lit value a shadow keeps.
    sun_color: [f32; 4],
    /// `rgb` ambient from above, `a` ambient intensity.
    sky_ambient: [f32; 4],
    /// `rgb` ambient from below, `a` how much specular water keeps.
    ground_bounce: [f32; 4],
    sky_zenith: [f32; 4],
    sky_mid: [f32; 4],
    sky_horizon: [f32; 4],
    /// `rgb` fog colour at the horizon, `a` fog density per voxel.
    fog: [f32; 4],
    /// `x` fog height falloff, `y` fog reference height, `z` shadow texel size,
    /// `w` `Lod1` debug tint strength.
    params: [f32; 4],
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

/// One corner of the solid unit cube the effect chips are drawn from.
///
/// Position and normal, centred on the origin so an instance is a placement
/// rather than a translation: the shader multiplies by the edge length and adds
/// the centre, which is the same arithmetic the debug boxes use.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct VfxVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

impl VfxVertex {
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

impl VfxInstance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// The six faces of a unit cube centred on the origin, as two triangles each.
///
/// Thirty-six vertices rather than an index buffer, because thirty-six is
/// nothing and a second buffer to bind is not.
fn vfx_cube() -> [VfxVertex; 36] {
    const H: f32 = 0.5;
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        // normal, in-plane u, in-plane v
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let mut out = [VfxVertex {
        position: [0.0; 3],
        normal: [0.0; 3],
    }; 36];
    let mut index = 0;
    for (normal, u, v) in faces {
        let corner = |su: f32, sv: f32| {
            [
                normal[0].mul_add(H, u[0].mul_add(su * H, v[0] * sv * H)),
                normal[1].mul_add(H, u[1].mul_add(su * H, v[1] * sv * H)),
                normal[2].mul_add(H, u[2].mul_add(su * H, v[2] * sv * H)),
            ]
        };
        // Counter-clockwise seen from outside, matching every other pipeline.
        for (su, sv) in [
            (-1.0, -1.0),
            (1.0, -1.0),
            (1.0, 1.0),
            (-1.0, -1.0),
            (1.0, 1.0),
            (-1.0, 1.0),
        ] {
            out[index] = VfxVertex {
                position: corner(su, sv),
                normal,
            };
            index += 1;
        }
    }
    out
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

/// One body part's world matrix plus its shadow parameters.
///
/// Eighty bytes, separate from [`ModelUniform`] on purpose. A body part needs a
/// full transform because it rotates about its joint, while a chunk needs a
/// translation and a uniform scale; widening the chunk uniform to share one
/// path would move every recorded chunk byte figure for no benefit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct PartUniform {
    model: [[f32; 4]; 4],
    /// `x` the shadow normal offset in world units; `yzw` padding.
    params: [f32; 4],
}

/// How far along its own normal a character surface is pushed before the shadow
/// lookup, in world units.
///
/// Terrain uses `0.35`, which is a third of a terrain voxel and about three
/// shadow texels. At character scale that same offset is five and a half
/// character voxels, wider than a limb, so it would move the sample clean off
/// the body. This is roughly one and a quarter shadow texels instead: enough to
/// keep a character surface out of its own acne, small enough to stay on the
/// body it belongs to.
const CHARACTER_SHADOW_NORMAL_OFFSET: f32 = 0.13;

impl PartUniform {
    fn from_matrix(model: glam::Mat4) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            params: [CHARACTER_SHADOW_NORMAL_OFFSET, 0.0, 0.0, 0.0],
        }
    }
}

/// One uploaded rigid part and what it cost.
struct UploadedPart {
    part: GpuCharacterPart,
    vertex_bytes: usize,
    index_bytes: usize,
    quads: usize,
}

/// Turns a borrowed label into the `'static` one the error type carries.
///
/// The upload errors name a bone, and every bone name in the character crate is
/// already `'static`; a weapon has one name, so this maps the two cases it can
/// actually see and refuses to invent a leak for anything else.
fn leaked(label: &str) -> &'static str {
    match label {
        "weapon" => "weapon",
        other => veldwake_character::skeleton::ALL_BONES
            .into_iter()
            .map(|bone| bone.name())
            .find(|name| *name == other)
            .unwrap_or("unknown part"),
    }
}

/// One compiled body part on the GPU, uploaded once and never re-uploaded.
struct GpuCharacterPart {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// One drawn body on the GPU, and the weapon in its hand.
///
/// Rigid parts make the geometry static: the buffers are immutable after upload
/// and a frame writes only the transforms. **A weapon is a rigid part like any
/// other** — the same vertex layout, the same eighty-byte uniform, the same
/// pipeline and the same shared lighting function — so drawing one costs no new
/// shader and no second shading policy. All it needs is its own palette.
struct GpuActor {
    parts: Vec<GpuCharacterPart>,
    weapon: Option<GpuCharacterPart>,
    vertex_bytes: usize,
    index_bytes: usize,
    uniform_bytes: usize,
    quads: usize,
}

impl GpuActor {
    /// Every drawable part of this actor, the weapon last.
    fn drawables(&self) -> impl Iterator<Item = &GpuCharacterPart> {
        self.parts.iter().chain(self.weapon.iter())
    }

    fn draw_count(&self) -> usize {
        self.parts.len() + usize::from(self.weapon.is_some())
    }
}

/// What the drawn bodies cost the renderer, for the milestone measurements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CharacterGpuStats {
    /// How many bodies are resident.
    pub actors: usize,
    /// How many weapons are resident.
    pub weapons: usize,
    /// Body parts across every actor, weapons excluded.
    pub parts: usize,
    pub quads: usize,
    pub vertex_bytes: usize,
    pub index_bytes: usize,
    pub uniform_bytes: usize,
    /// Bytes written every frame a pose is applied.
    pub dynamic_upload_bytes: usize,
    pub world_draws: usize,
    pub shadow_draws: usize,
}

const fn level_scale(lod: LodLevel) -> f32 {
    match lod {
        LodLevel::Lod0 => 1.0,
        LodLevel::Lod1 => 2.0,
    }
}

impl SceneUniform {
    fn build(camera: &Camera, weather: Weather, lod_tint: f32) -> Self {
        let lighting = Lighting::for_weather(weather);
        let view_projection = camera.view_projection();
        let position = camera.position();
        let light = shadow_view_projection(
            shadow_centre(position, camera.forward()),
            lighting.sun_direction,
        );
        Self {
            view_projection: view_projection.to_cols_array_2d(),
            inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
            light_view_projection: light.to_cols_array_2d(),
            camera_position: [position.x, position.y, position.z, 0.0],
            sun: [
                lighting.sun_direction.x,
                lighting.sun_direction.y,
                lighting.sun_direction.z,
                lighting.sun_intensity,
            ],
            sun_color: rgba(lighting.sun_color, lighting.shadow_floor),
            sky_ambient: rgba(lighting.sky_ambient, lighting.ambient_intensity),
            ground_bounce: rgba(lighting.ground_bounce, lighting.water_specular),
            sky_zenith: rgba(lighting.sky_zenith, 0.0),
            sky_mid: rgba(lighting.sky_mid, 0.0),
            sky_horizon: rgba(lighting.sky_horizon, 0.0),
            fog: rgba(lighting.fog_color, lighting.fog_density),
            params: [
                lighting.fog_height_falloff,
                lighting.fog_reference_height,
                1.0 / SHADOW_MAP_EDGE as f32,
                lod_tint,
            ],
        }
    }
}

const fn rgba(color: [f32; 3], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], alpha]
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

/// Why a compiled character cannot be represented by the fixed M5 GPU path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharacterUploadError {
    PartCount { found: usize, expected: usize },
    EmptyMesh { bone: &'static str },
    TooManyIndices { bone: &'static str, count: usize },
    NonCharacterMaterial { bone: &'static str, voxel: VoxelId },
}

impl Display for CharacterUploadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PartCount { found, expected } => {
                write!(
                    formatter,
                    "character has {found} parts; M5 requires {expected}"
                )
            }
            Self::EmptyMesh { bone } => {
                write!(formatter, "character part {bone} has an empty mesh")
            }
            Self::TooManyIndices { bone, count } => write!(
                formatter,
                "character part {bone} has {count} indices, beyond the u32 index format"
            ),
            Self::NonCharacterMaterial { bone, voxel } => write!(
                formatter,
                "character part {bone} carries non-character voxel identifier {}",
                voxel.0
            ),
        }
    }
}

impl Error for CharacterUploadError {}

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
    /// Depth-only pass that fills the shadow map from the sun.
    shadow_pipeline: wgpu::RenderPipeline,
    /// Full-screen procedural sky, drawn before the world and writing no depth.
    sky_pipeline: wgpu::RenderPipeline,
    model_layout: wgpu::BindGroupLayout,
    chunks: BTreeMap<ChunkCoord, GpuChunkSlot>,
    scene_buffer: wgpu::Buffer,
    /// Scene state plus the shadow map, for everything that reads shadows.
    scene_bind_group: wgpu::BindGroup,
    /// Scene state alone, for the shadow and debug passes. The shadow pass
    /// cannot bind the map it is writing, and a line has nothing to shade.
    scene_only_bind_group: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    fatal_gpu_error: Arc<AtomicBool>,
    /// Separate `LineList` pipeline for the debug views. Nothing below is
    /// touched while the views are off.
    debug_pipeline: wgpu::RenderPipeline,
    /// One instanced pass for the two effects. The buffer is allocated once at
    /// its maximum and never grows, so a frame with chips in it writes and a
    /// frame without one does not.
    vfx_pipeline: wgpu::RenderPipeline,
    vfx_vertices: wgpu::Buffer,
    vfx_instances: wgpu::Buffer,
    /// Live instances written this frame. Zero means no write and no draw.
    vfx_live: u32,
    /// Bytes of instance data written this frame, for the accounting.
    vfx_instance_bytes: usize,
    debug_layout: wgpu::BindGroupLayout,
    debug_vertices: wgpu::Buffer,
    debug_slots: Vec<DebugSlot>,
    debug_draws: Vec<(usize, std::ops::Range<u32>)>,
    debug_frame_work: DebugFrameWork,
    /// Character body parts: their own pipelines and their own uniform,
    /// sharing the vertex layout, the scene bind group, and every
    /// lighting constant with the world pass.
    character_pipeline: wgpu::RenderPipeline,
    character_shadow_pipeline: wgpu::RenderPipeline,
    part_layout: wgpu::BindGroupLayout,
    /// The drawn bodies. One for the M5 preview, two for a fight; the renderer
    /// holds no scene graph and nothing here is an entity.
    actors: Vec<GpuActor>,
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

        let scene_uniform = SceneUniform::build(camera, Weather::default(), 0.0);
        let scene_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("M4 scene uniform"),
            contents: bytemuck::bytes_of(&scene_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let scene_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<SceneUniform>() as u64),
            },
            count: None,
        };
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("M4 scene bind group layout"),
            entries: &[
                scene_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let scene_only_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("M4 scene-only bind group layout"),
            entries: &[scene_entry],
        });
        let shadow_view = create_shadow_view(&device);
        // Comparison sampling with linear filtering is what turns each of the
        // nine taps into a bilinear comparison, so a three-by-three kernel
        // gives a soft edge instead of nine hard steps.
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("M4 shadow comparison sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("M4 scene bind group"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: scene_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
        let scene_only_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("M4 scene-only bind group"),
            layout: &scene_only_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene_buffer.as_entire_binding(),
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
        let shader = lit_shader_module(&device, "M4 world shader", include_str!("world.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("M4 world pipeline layout"),
            bind_group_layouts: &[Some(&scene_layout), Some(&model_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M4 world pipeline"),
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
        let shadow_shader = shader_module(&device, "M4 shadow shader", include_str!("shadow.wgsl"));
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("M4 shadow pipeline layout"),
                bind_group_layouts: &[Some(&scene_only_layout), Some(&model_layout)],
                immediate_size: 0,
            });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M4 shadow pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                // Casting from back faces moves the acne to surfaces the camera
                // cannot see, which is cheaper and steadier than fighting it
                // with bias alone.
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            // No fragment stage: the pass exists only to write depth.
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        let part_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("M5 character part bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<PartUniform>() as u64),
                },
                count: None,
            }],
        });
        let character_shader = lit_shader_module(
            &device,
            "M5 character shader",
            include_str!("character.wgsl"),
        );
        let character_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("M5 character pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout), Some(&part_layout)],
                immediate_size: 0,
            });
        let character_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M5 character pipeline"),
            layout: Some(&character_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &character_shader,
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
                module: &character_shader,
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
        let character_shadow_shader = shader_module(
            &device,
            "M5 character shadow shader",
            include_str!("character_shadow.wgsl"),
        );
        let character_shadow_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("M5 character shadow pipeline layout"),
                bind_group_layouts: &[Some(&scene_only_layout), Some(&part_layout)],
                immediate_size: 0,
            });
        let character_shadow_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("M5 character shadow pipeline"),
                layout: Some(&character_shadow_layout),
                vertex: wgpu::VertexState {
                    module: &character_shadow_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(Vertex::layout())],
                },
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Ccw,
                    // Cast from back faces, exactly as chunks do. Casting from
                    // the lit faces was tried first and produced visible acne
                    // on the chest, because one shadow texel is `0.109` world
                    // units and a character voxel is `0.0833`: the map cannot
                    // resolve one part shadowing another, so the comparison
                    // dithers along the boundary.
                    //
                    // The cost is real and is a limitation rather than a fix:
                    // the recorded depth is the far side of a part that is only
                    // a few voxels thick, so a character does not shadow itself
                    // at all. It still casts a correct shadow on the world,
                    // which is the reading that matters at this map resolution.
                    cull_mode: Some(wgpu::Face::Front),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                fragment: None,
                multiview_mask: None,
                cache: None,
            });

        let sky_shader = shader_module(&device, "M4 sky shader", include_str!("sky.wgsl"));
        let sky_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("M4 sky pipeline layout"),
            bind_group_layouts: &[Some(&scene_layout)],
            immediate_size: 0,
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M4 sky pipeline"),
            layout: Some(&sky_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            // The sky is behind everything: it never writes depth and never
            // rejects a pixel, so the world simply draws over it.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
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
        let debug_shader = shader_module(
            &device,
            "M3C debug line shader",
            include_str!("debug_line.wgsl"),
        );
        let debug_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("M3C debug line pipeline layout"),
                bind_group_layouts: &[Some(&scene_only_layout), Some(&debug_layout)],
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

        // The effect chips. One pipeline, one static cube, one instance buffer
        // allocated at its maximum once: a burst writes into it and an empty
        // frame leaves it alone.
        let vfx_shader = shader_module(&device, "M6 effect chip shader", include_str!("vfx.wgsl"));
        let vfx_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("M6 effect chip pipeline layout"),
            bind_group_layouts: &[Some(&scene_only_layout)],
            immediate_size: 0,
        });
        let vfx_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M6 effect chip pipeline"),
            layout: Some(&vfx_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vfx_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(VfxVertex::layout()), Some(VfxInstance::layout())],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            // Solid geometry, so it writes depth like a body does: a chip
            // behind a torso is behind it.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &vfx_shader,
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
        let vfx_vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("M6 effect chip unit cube"),
            contents: bytemuck::cast_slice(&vfx_cube()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let vfx_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("M6 effect chip instances"),
            size: (size_of::<VfxInstance>() * MAX_VFX_INSTANCES) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
            shadow_pipeline,
            sky_pipeline,
            model_layout,
            chunks: BTreeMap::new(),
            scene_buffer,
            scene_bind_group,
            scene_only_bind_group,
            shadow_view,
            depth_view,
            fatal_gpu_error,
            debug_pipeline,
            debug_layout,
            debug_vertices,
            vfx_pipeline,
            vfx_vertices,
            vfx_instances,
            vfx_live: 0,
            vfx_instance_bytes: 0,
            debug_slots: Vec::new(),
            debug_draws: Vec::new(),
            debug_frame_work: DebugFrameWork::default(),
            character_pipeline,
            character_shadow_pipeline,
            part_layout,
            actors: Vec::new(),
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

    /// Uploads the camera, the weather state, and the `Lod1` debug tint
    /// strength (`0.0` when the `Lod` view is not active).
    ///
    /// One buffer write per frame. Everything the shaders need about the frame
    /// travels together, so a weather change costs exactly what a camera move
    /// costs and there is no second path to keep in step.
    pub fn update_scene(&self, camera: &Camera, weather: Weather, lod_tint: f32) {
        let uniform = SceneUniform::build(camera, weather, lod_tint);
        self.queue
            .write_buffer(&self.scene_buffer, 0, bytemuck::bytes_of(&uniform));
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

    /// Replaces the effect chips drawn this frame.
    ///
    /// An empty slice writes no bytes and issues no draw, which is the same
    /// contract the debug views hold: with nothing in flight the effects cost
    /// exactly the pipeline and the buffers that were created at startup.
    /// Anything past [`MAX_VFX_INSTANCES`] is refused here as well as in the pool,
    /// because a renderer that trusts a caller's length is one bad caller from
    /// writing past a buffer.
    pub fn set_vfx_instances(&mut self, instances: &[VfxInstance]) {
        self.vfx_live = 0;
        self.vfx_instance_bytes = 0;
        if instances.is_empty() {
            return;
        }
        let count = instances.len().min(MAX_VFX_INSTANCES);
        let bytes = bytemuck::cast_slice(&instances[..count]);
        self.queue.write_buffer(&self.vfx_instances, 0, bytes);
        self.vfx_live = u32::try_from(count).unwrap_or(0);
        self.vfx_instance_bytes = bytes.len();
    }

    /// What the effects cost the GPU in the last frame.
    #[must_use]
    pub const fn vfx_frame_work(&self) -> VfxFrameWork {
        VfxFrameWork {
            draws: if self.vfx_live == 0 { 0 } else { 1 },
            instances: self.vfx_live,
            bytes: self.vfx_instance_bytes,
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
            // Shadow pass first: the world pass samples what it writes, so the
            // two cannot share an encoder scope.
            let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("M4 shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
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
            shadow_pass.set_pipeline(&self.shadow_pipeline);
            shadow_pass.set_bind_group(0, &self.scene_only_bind_group, &[]);
            for chunk in self
                .chunks
                .values()
                .filter(|slot| slot.drawable)
                .filter_map(|slot| slot.active.as_ref())
            {
                shadow_pass.set_bind_group(1, &chunk.model_bind_group, &[]);
                shadow_pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                shadow_pass
                    .set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                shadow_pass.draw_indexed(0..chunk.index_count, 0, 0..1);
            }
            if !self.actors.is_empty() {
                shadow_pass.set_pipeline(&self.character_shadow_pipeline);
                shadow_pass.set_bind_group(0, &self.scene_only_bind_group, &[]);
                for part in self.actors.iter().flat_map(GpuActor::drawables) {
                    shadow_pass.set_bind_group(1, &part.bind_group, &[]);
                    shadow_pass.set_vertex_buffer(0, part.vertex_buffer.slice(..));
                    shadow_pass
                        .set_index_buffer(part.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    shadow_pass.draw_indexed(0..part.index_count, 0, 0..1);
                }
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("M4 world pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // The sky pass covers every pixel, so this clear is a
                        // safety net rather than a visible colour.
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
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.draw(0..3, 0..1);

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
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
            if !self.actors.is_empty() {
                pass.set_pipeline(&self.character_pipeline);
                pass.set_bind_group(0, &self.scene_bind_group, &[]);
                for part in self.actors.iter().flat_map(GpuActor::drawables) {
                    pass.set_bind_group(1, &part.bind_group, &[]);
                    pass.set_vertex_buffer(0, part.vertex_buffer.slice(..));
                    pass.set_index_buffer(part.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..part.index_count, 0, 0..1);
                }
            }
            if self.vfx_live > 0 {
                pass.set_pipeline(&self.vfx_pipeline);
                pass.set_bind_group(0, &self.scene_only_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vfx_vertices.slice(..));
                pass.set_vertex_buffer(1, self.vfx_instances.slice(..));
                pass.draw(0..36, 0..self.vfx_live);
            }
            if !self.debug_draws.is_empty() {
                pass.set_pipeline(&self.debug_pipeline);
                pass.set_bind_group(0, &self.scene_only_bind_group, &[]);
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
    /// Uploads a compiled character once, as the only actor, with no weapon.
    ///
    /// The M5 path, kept exactly as it was: one body, no weapon, actor zero.
    pub fn upload_character(
        &mut self,
        character: &CompiledCharacter,
    ) -> Result<(), CharacterUploadError> {
        self.actors.clear();
        self.upload_actor(character, None).map(|_| ())
    }

    /// Uploads one body and, optionally, the weapon in its hand.
    ///
    /// Returns the actor's index, which is what a later pose refers to. Rigid
    /// parts make the geometry static, so this happens at startup and never
    /// again; a frame writes only the transforms.
    pub fn upload_actor(
        &mut self,
        character: &CompiledCharacter,
        weapon: Option<&CompiledWeapon>,
    ) -> Result<usize, CharacterUploadError> {
        if character.parts().len() != BONE_COUNT {
            return Err(CharacterUploadError::PartCount {
                found: character.parts().len(),
                expected: BONE_COUNT,
            });
        }
        let palette = character.palette();
        let mut parts = Vec::with_capacity(character.parts().len());
        let mut vertex_bytes = 0;
        let mut index_bytes = 0;
        let mut quads = 0;
        for part in character.parts() {
            let uploaded = self.upload_part(part.mesh(), part.bone().name(), &|voxel| {
                palette.appearance(voxel)
            })?;
            vertex_bytes += uploaded.vertex_bytes;
            index_bytes += uploaded.index_bytes;
            quads += uploaded.quads;
            parts.push(uploaded.part);
        }

        // A weapon is a rigid part with its own palette and nothing else new.
        let weapon_part = match weapon {
            Some(weapon) => {
                let weapon_palette = weapon.palette();
                let uploaded = self.upload_part(weapon.mesh(), "weapon", &|voxel| {
                    weapon_palette.appearance(voxel)
                })?;
                vertex_bytes += uploaded.vertex_bytes;
                index_bytes += uploaded.index_bytes;
                quads += uploaded.quads;
                Some(uploaded.part)
            }
            None => None,
        };

        let draws = parts.len() + usize::from(weapon_part.is_some());
        let uniform_bytes = draws * size_of::<PartUniform>();
        info!(
            actor = self.actors.len(),
            parts = parts.len(),
            weapon = weapon_part.is_some(),
            quads,
            vertex_bytes,
            index_bytes,
            uniform_bytes,
            fingerprint = format_args!("{:#018x}", character.fingerprint()),
            weapon_fingerprint =
                format_args!("{:#018x}", weapon.map_or(0, CompiledWeapon::fingerprint)),
            "actor uploaded"
        );
        self.actors.push(GpuActor {
            parts,
            weapon: weapon_part,
            vertex_bytes,
            index_bytes,
            uniform_bytes,
            quads,
        });
        Ok(self.actors.len() - 1)
    }

    /// Uploads one rigid mesh and its per-frame uniform.
    ///
    /// The one place a mesh becomes GPU buffers, so a body part and a weapon
    /// cannot drift apart in layout. The appearance lookup is the caller's,
    /// because the two carry different palettes and neither may read the other's.
    fn upload_part(
        &self,
        mesh: &Mesh,
        label: &str,
        appearance: &dyn Fn(VoxelId) -> Option<([f32; 3], f32)>,
    ) -> Result<UploadedPart, CharacterUploadError> {
        if mesh.vertices().is_empty() || mesh.indices().is_empty() {
            return Err(CharacterUploadError::EmptyMesh {
                bone: leaked(label),
            });
        }
        let index_count = u32::try_from(mesh.indices().len()).map_err(|_| {
            CharacterUploadError::TooManyIndices {
                bone: leaked(label),
                count: mesh.indices().len(),
            }
        })?;
        let vertices = mesh
            .vertices()
            .iter()
            .map(|vertex| -> Result<Vertex, CharacterUploadError> {
                // A material answers for itself, from the one table its own crate
                // owns. A vertex that is not one of its identifiers is a compiler
                // bug, and drawing it in another domain's palette would hide that.
                let (color, specular) =
                    appearance(vertex.voxel).ok_or(CharacterUploadError::NonCharacterMaterial {
                        bone: leaked(label),
                        voxel: vertex.voxel,
                    })?;
                Ok(Vertex {
                    position: vertex.position,
                    normal: vertex.normal,
                    color,
                    specular,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("M5 rigid part vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("M5 rigid part indices"),
                contents: bytemuck::cast_slice(mesh.indices()),
                usage: wgpu::BufferUsages::INDEX,
            });
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("M5 rigid part uniform"),
                contents: bytemuck::bytes_of(&PartUniform::from_matrix(glam::Mat4::IDENTITY)),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("M5 rigid part bind group"),
            layout: &self.part_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        Ok(UploadedPart {
            vertex_bytes: vertices.len() * size_of::<Vertex>(),
            index_bytes: size_of_val(mesh.indices()),
            quads: mesh.quad_count(),
            part: GpuCharacterPart {
                vertex_buffer,
                index_buffer,
                index_count,
                uniform_buffer,
                bind_group,
            },
        })
    }

    /// Writes one frame of part transforms.
    ///
    /// The only per-frame character work there is. The matrices are composed by
    /// `veldwake-character`, so the renderer decides nothing about a pose.
    pub fn set_character_pose(&self, posed: &PosedCharacter) {
        self.set_actor_pose(0, posed, None);
    }

    /// Writes one actor's transforms, and its weapon's if it has one.
    ///
    /// A weapon matrix the caller does not supply leaves the weapon where it was,
    /// which is what a body with no weapon wants.
    pub fn set_actor_pose(&self, actor: usize, posed: &PosedCharacter, weapon: Option<glam::Mat4>) {
        let Some(actor) = self.actors.get(actor) else {
            return;
        };
        // Upload only follows a successful `upload_actor`, which requires the
        // compiler's fixed sixteen-part contract. `part_matrices` is the same
        // fixed-size array, so indexing makes an accidental mismatch impossible
        // to hide by truncating a `zip`.
        for index in 0..BONE_COUNT {
            let Some(part) = actor.parts.get(index) else {
                return;
            };
            let matrix = posed.part_matrices()[index];
            self.queue.write_buffer(
                &part.uniform_buffer,
                0,
                bytemuck::bytes_of(&PartUniform::from_matrix(matrix)),
            );
        }
        if let (Some(part), Some(matrix)) = (actor.weapon.as_ref(), weapon) {
            self.queue.write_buffer(
                &part.uniform_buffer,
                0,
                bytemuck::bytes_of(&PartUniform::from_matrix(matrix)),
            );
        }
    }

    /// What the resident bodies cost, or zeroes when there are none.
    pub fn character_stats(&self) -> CharacterGpuStats {
        let mut stats = CharacterGpuStats::default();
        for actor in &self.actors {
            stats.actors += 1;
            stats.weapons += usize::from(actor.weapon.is_some());
            stats.parts += actor.parts.len();
            stats.quads += actor.quads;
            stats.vertex_bytes += actor.vertex_bytes;
            stats.index_bytes += actor.index_bytes;
            stats.uniform_bytes += actor.uniform_bytes;
            stats.world_draws += actor.draw_count();
            stats.shadow_draws += actor.draw_count();
        }
        stats.dynamic_upload_bytes = stats.uniform_bytes;
        stats
    }

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
            // Releasing first here too: a staged empty mesh holds no buffers,
            // and the commit it schedules removes the chunk.
            let slot = self.chunks.entry(coord).or_default();
            slot.staged = None;
            slot.staged_empty = true;
            return Ok(0);
        }
        // Reject before touching the slot. An early rejection must never
        // destroy staging that is still valid.
        let index_count =
            u32::try_from(mesh.indices().len()).map_err(|_| ChunkUploadError::TooManyIndices {
                coord,
                indices: mesh.indices().len(),
            })?;
        // Release the obsolete replacement before allocating its successor.
        //
        // The bridge restages a coordinate only when the staged stamp is no
        // longer that chunk's target, and a staged mesh whose stamp differs
        // from the target can never be committed, so what is dropped here is
        // provably dead. Taking it out of the map before the new buffers exist
        // is what keeps presentation-owned chunk-mesh bytes from ever holding
        // two replacements of one chunk at the same instant, which the bridge
        // could not observe from outside the call. The committed mesh is not
        // touched and keeps drawing.
        let obsolete = self.chunks.get_mut(&coord).and_then(|slot| {
            slot.staged_empty = false;
            slot.staged.take()
        });
        drop(obsolete);
        let vertices = mesh
            .vertices()
            .iter()
            .map(|vertex| {
                let (color, specular) = voxel_appearance(vertex.voxel);
                Vertex {
                    position: vertex.position,
                    normal: vertex.normal,
                    color,
                    specular,
                }
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
        // The slot's previous replacement was already released above, so this
        // installs into an empty staging position.
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

/// The colour and specular response of one voxel identifier.
///
/// A terrain or landmark material answers for itself, from the one table in
/// `veldwake-procedural`; the renderer does not keep a second palette and
/// cannot drift from the style bible or from `LANDMARK_STYLE.md`. Identifiers
/// neither table claims are the M3 diagnostic fixture, which keeps its old
/// colours so the regression view still looks like itself.
fn voxel_appearance(voxel: VoxelId) -> ([f32; 3], f32) {
    if let Some(material) = TerrainMaterial::from_voxel_id(voxel) {
        return (material.albedo(), material.specular());
    }
    if let Some(material) = LandmarkMaterial::from_voxel_id(voxel) {
        return (material.albedo(), material.specular());
    }
    (diagnostic_color(voxel), 0.0)
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

/// Builds a shader module from the shared scene declaration plus one stage.
///
/// WGSL has no include directive, so the uniform layout every shader agrees on
/// is concatenated in front of each one. One definition, checked by the
/// compiler in every module that uses it.
fn shader_module(device: &wgpu::Device, label: &str, body: &str) -> wgpu::ShaderModule {
    let source = format!("{}\n{body}", include_str!("scene.wgsl"));
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

/// The shadow map: one square depth texture, sampled for comparison.
/// A shader that shades a surface: the shared scene layout plus the one
/// stylized shading model, prepended to the body.
///
/// WGSL has no include directive, so terrain and characters share their
/// lighting by sharing this source rather than by copying it.
fn lit_shader_module(device: &wgpu::Device, label: &str, body: &str) -> wgpu::ShaderModule {
    let source = format!(
        "{}\n{}\n{body}",
        include_str!("scene.wgsl"),
        include_str!("shading.wgsl")
    );
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

fn create_shadow_view(device: &wgpu::Device) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("M4 shadow map"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_EDGE,
                height: SHADOW_MAP_EDGE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
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
        ModelUniform, SceneUniform, Vertex, debug_line_vertices, debug_vertex_range,
        diagnostic_color, gpu_payload_bytes, select_alpha_mode, select_present_mode,
        select_surface_format, voxel_appearance,
    };
    use crate::debug::{DebugKind, DebugPrimitive, DebugShape};
    use crate::lighting::Weather;
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
            world: None,
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
        // 528 vertices of 40 bytes (position, normal, colour, specular), 792
        // u32 indices, one 32-byte model uniform. The vertex grew by sixteen
        // bytes in M4; the accounting has to grow with it or the reported
        // presentation-owned bytes would understate what the GPU holds.
        assert_eq!(std::mem::size_of::<Vertex>(), 40);
        assert_eq!(mesh.vertices().len(), 528);
        assert_eq!(mesh.indices().len(), 792);
        assert_eq!(gpu_payload_bytes(&mesh), 528 * 40 + 792 * 4 + 32);
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
    fn terrain_materials_carry_their_own_colour_and_the_fixture_keeps_its_own() {
        use veldwake_procedural::TerrainMaterial;

        for material in veldwake_procedural::material::ALL_MATERIALS {
            let (color, specular) = voxel_appearance(material.voxel_id());
            assert_eq!(
                color,
                material.albedo(),
                "{} lost its colour",
                material.name()
            );
            assert_eq!(specular, material.specular());
        }
        // Only water is strongly specular, which is the cue that it is liquid.
        assert_eq!(voxel_appearance(TerrainMaterial::Water.voxel_id()).1, 1.0);
        assert_eq!(
            voxel_appearance(TerrainMaterial::MeadowGrass.voxel_id()).1,
            0.0
        );

        // The M3 fixture identifiers still resolve to the diagnostic palette,
        // with no specular, so the regression view is unchanged.
        for id in [1, 2, 7] {
            let (color, specular) = voxel_appearance(VoxelId(id));
            assert_eq!(color, diagnostic_color(VoxelId(id)));
            assert_eq!(specular, 0.0);
        }
    }

    #[test]
    fn the_scene_uniform_is_finite_and_carries_the_weather() {
        use crate::camera::Camera;

        let camera = Camera::default();
        let clear = SceneUniform::build(&camera, Weather::Clear, 0.0);
        let overcast = SceneUniform::build(&camera, Weather::Overcast, 0.0);

        for matrix in [
            clear.view_projection,
            clear.inverse_view_projection,
            clear.light_view_projection,
        ] {
            assert!(
                matrix.iter().flatten().all(|value| value.is_finite()),
                "a scene matrix is not finite"
            );
        }
        // The inverse must actually invert: the sky pass unprojects with it.
        let forward = glam::Mat4::from_cols_array_2d(&clear.view_projection);
        let inverse = glam::Mat4::from_cols_array_2d(&clear.inverse_view_projection);
        let identity = forward * inverse;
        for (index, value) in identity.to_cols_array().into_iter().enumerate() {
            let expected = if index % 5 == 0 { 1.0 } else { 0.0 };
            assert!(
                (value - expected).abs() < 1e-3,
                "not an inverse: {identity:?}"
            );
        }

        assert!(
            overcast.sun[3] < clear.sun[3],
            "overcast did not dim the sun"
        );
        assert!(
            overcast.sky_ambient[3] > clear.sky_ambient[3],
            "overcast did not raise the fill"
        );
        assert!(
            overcast.fog[3] > clear.fog[3],
            "overcast did not thicken the fog"
        );
        // The debug tint rides in the same buffer as everything else.
        assert_eq!(
            SceneUniform::build(&camera, Weather::Clear, 1.0).params[3],
            1.0
        );
        assert_eq!(clear.params[3], 0.0);
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
