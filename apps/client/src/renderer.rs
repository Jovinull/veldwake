use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytemuck::{Pod, Zeroable};
use tracing::{debug, error, info, warn};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, event_loop::OwnedDisplayHandle, window::Window};

use crate::camera::Camera;

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
}

impl CameraUniform {
    fn from_camera(camera: &Camera) -> Self {
        Self {
            view_projection: camera.view_projection().to_cols_array_2d(),
        }
    }
}

const VERTICES: [Vertex; 24] = [
    face_vertex([-1.0, -1.0, 1.0], [0.92, 0.28, 0.26]),
    face_vertex([1.0, -1.0, 1.0], [0.92, 0.28, 0.26]),
    face_vertex([1.0, 1.0, 1.0], [0.92, 0.28, 0.26]),
    face_vertex([-1.0, 1.0, 1.0], [0.92, 0.28, 0.26]),
    face_vertex([-1.0, 1.0, -1.0], [0.22, 0.58, 0.94]),
    face_vertex([1.0, 1.0, -1.0], [0.22, 0.58, 0.94]),
    face_vertex([1.0, -1.0, -1.0], [0.22, 0.58, 0.94]),
    face_vertex([-1.0, -1.0, -1.0], [0.22, 0.58, 0.94]),
    face_vertex([1.0, -1.0, -1.0], [0.94, 0.68, 0.20]),
    face_vertex([1.0, 1.0, -1.0], [0.94, 0.68, 0.20]),
    face_vertex([1.0, 1.0, 1.0], [0.94, 0.68, 0.20]),
    face_vertex([1.0, -1.0, 1.0], [0.94, 0.68, 0.20]),
    face_vertex([-1.0, -1.0, 1.0], [0.34, 0.82, 0.50]),
    face_vertex([-1.0, 1.0, 1.0], [0.34, 0.82, 0.50]),
    face_vertex([-1.0, 1.0, -1.0], [0.34, 0.82, 0.50]),
    face_vertex([-1.0, -1.0, -1.0], [0.34, 0.82, 0.50]),
    face_vertex([1.0, 1.0, -1.0], [0.72, 0.36, 0.90]),
    face_vertex([-1.0, 1.0, -1.0], [0.72, 0.36, 0.90]),
    face_vertex([-1.0, 1.0, 1.0], [0.72, 0.36, 0.90]),
    face_vertex([1.0, 1.0, 1.0], [0.72, 0.36, 0.90]),
    face_vertex([1.0, -1.0, 1.0], [0.18, 0.78, 0.80]),
    face_vertex([-1.0, -1.0, 1.0], [0.18, 0.78, 0.80]),
    face_vertex([-1.0, -1.0, -1.0], [0.18, 0.78, 0.80]),
    face_vertex([1.0, -1.0, -1.0], [0.18, 0.78, 0.80]),
];

const INDICES: [u16; 36] = [
    0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4, 8, 9, 10, 10, 11, 8, 12, 13, 14, 14, 15, 12, 16, 17, 18,
    18, 19, 16, 20, 21, 22, 22, 23, 20,
];

const fn face_vertex(position: [f32; 3], color: [f32; 3]) -> Vertex {
    Vertex { position, color }
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    fatal_gpu_error: Arc<AtomicBool>,
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
                label: Some("Veldwake M1 device"),
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

        let camera_uniform = CameraUniform::from_camera(camera);
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("M1 camera uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("M1 camera bind group layout"),
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
            label: Some("M1 camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("diagnostic.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("M1 diagnostic pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("M1 diagnostic cube pipeline"),
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
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("M1 diagnostic cube vertices"),
            contents: bytemuck::cast_slice(&VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("M1 diagnostic cube indices"),
            contents: bytemuck::cast_slice(&INDICES),
            usage: wgpu::BufferUsages::INDEX,
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
            vertex_buffer,
            index_buffer,
            camera_buffer,
            camera_bind_group,
            depth_view,
            fatal_gpu_error,
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

    pub fn update_camera(&self, camera: &Camera) {
        let uniform = CameraUniform::from_camera(camera);
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
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
                label: Some("M1 surface view"),
                format: Some(self.config.format),
                ..Default::default()
            });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("M1 diagnostic frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("M1 diagnostic cube pass"),
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
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
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
            label: Some("M1 depth texture"),
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
    use super::{select_alpha_mode, select_present_mode, select_surface_format};

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
