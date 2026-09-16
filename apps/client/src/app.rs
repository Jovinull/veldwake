use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{debug, info};
use veldwake_streaming::StreamingConfig;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    camera::{Camera, CameraController},
    input::{CameraAction, InputState},
    renderer::{RenderOutcome, Renderer},
    streaming::{ChunkPresentation, FrameStreamingReport, StreamingBridge, UploadBudget},
};

const FRAME_REPORT_INTERVAL: Duration = Duration::from_secs(5);

pub fn run() -> Result<(), AppRunError> {
    let event_loop = EventLoop::new().map_err(|error| AppRunError(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .map_err(|error| AppRunError(error.to_string()))?;

    if let Some(error) = app.fatal_error {
        return Err(AppRunError(error));
    }

    Ok(())
}

#[derive(Debug)]
pub struct AppRunError(String);

impl Display for AppRunError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for AppRunError {}

struct App {
    renderer: Option<Renderer>,
    streaming: Option<StreamingBridge>,
    camera: Camera,
    controller: CameraController,
    input: InputState,
    last_frame: Instant,
    frame_stats: FrameStats,
    occluded: bool,
    fatal_error: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            renderer: None,
            streaming: None,
            camera: Camera::default(),
            controller: CameraController::default(),
            input: InputState::default(),
            last_frame: Instant::now(),
            frame_stats: FrameStats::new(),
            occluded: false,
            fatal_error: None,
        }
    }
}

impl App {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppRunError> {
        let attributes = Window::default_attributes()
            .with_title("Veldwake - M3B Streaming Runtime")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_visible(false);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| AppRunError(format!("failed to create window: {error}")))?,
        );
        let initial_size = window.inner_size();
        self.camera
            .set_aspect_from_size(initial_size.width, initial_size.height);

        let config = StreamingConfig::default();
        let budget = UploadBudget::default();
        let streaming = StreamingBridge::new(config, budget, self.camera.position())
            .map_err(|error| AppRunError(error.to_string()))?;
        let renderer = pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(),
            Arc::clone(&window),
            &self.camera,
        ))
        .map_err(|error| AppRunError(error.to_string()))?;

        info!(
            camera_chunk = ?streaming.desired_center(),
            render_radius = config.render_radius,
            dependency_halo = config.dependency_halo,
            retention_radius = config.retention_radius,
            hard_resident_cap = config.hard_resident_cap,
            max_cpu_evictions_per_update = config.max_cpu_evictions_per_update,
            max_uploads_per_frame = budget.max_uploads_per_frame,
            soft_upload_bytes_per_frame = budget.soft_bytes_per_frame,
            max_removals_per_frame = budget.max_removals_per_frame,
            "M3B streaming bridge started"
        );
        self.last_frame = Instant::now();
        self.renderer = Some(renderer);
        self.streaming = Some(streaming);
        window.set_visible(true);
        window.request_redraw();
        info!(
            "M3B diagnostic controls: WASD move, Space/Ctrl vertical, hold right mouse to look, Escape exits"
        );
        Ok(())
    }

    fn fail_and_exit(&mut self, event_loop: &ActiveEventLoop, message: impl Into<String>) {
        let message = message.into();
        tracing::error!(%message, "fatal application error");
        self.fatal_error = Some(message);
        event_loop.exit();
    }

    fn handle_keyboard(&mut self, event_loop: &ActiveEventLoop, event: winit::event::KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };

        if code == KeyCode::Escape && pressed {
            info!("clean shutdown requested by Escape");
            event_loop.exit();
            return;
        }

        if let Some(action) = camera_action(code) {
            self.input.set_action(action, pressed);
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        if self.occluded {
            return;
        }

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.controller
            .update(&mut self.camera, &mut self.input, elapsed);

        let (Some(renderer), Some(streaming)) = (self.renderer.as_mut(), self.streaming.as_mut())
        else {
            return;
        };
        // A rejected anchor keeps the previous demand center; the bridge logs
        // the transition and counts every rejected frame.
        let _anchor = streaming.track_camera(self.camera.position());
        let report = match streaming.update(renderer) {
            Ok(report) => report,
            Err(error) => {
                self.fail_and_exit(event_loop, format!("streaming runtime failed: {error}"));
                return;
            }
        };
        renderer.update_camera(&self.camera);
        let request_next_redraw = match renderer.render() {
            RenderOutcome::Rendered => {
                self.frame_stats.record(now, elapsed, report);
                self.frame_stats.report_if_due(now, streaming, renderer);
                true
            }
            RenderOutcome::Retry => true,
            RenderOutcome::Reconfigured => {
                debug!("frame skipped after surface reconfiguration");
                true
            }
            RenderOutcome::Suspended => false,
            RenderOutcome::Fatal => {
                self.fail_and_exit(event_loop, "fatal GPU or surface error");
                return;
            }
        };

        if request_next_redraw && let Some(renderer) = self.renderer.as_ref() {
            renderer.window().request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }

        if let Err(error) = self.initialize(event_loop) {
            self.fail_and_exit(event_loop, error.to_string());
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(renderer_window_id) = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.window().id())
        else {
            return;
        };
        if window_id != renderer_window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("clean shutdown requested by window");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_keyboard(event_loop, event),
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => self.input.set_look_active(state == ElementState::Pressed),
            WindowEvent::Focused(focused) => {
                self.last_frame = Instant::now();
                if !focused {
                    self.input.reset();
                    debug!("input state reset after focus loss");
                }
            }
            WindowEvent::Resized(size) => {
                self.camera.set_aspect_from_size(size.width, size.height);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                    if size.width > 0 && size.height > 0 {
                        renderer.window().request_redraw();
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(renderer) = self.renderer.as_mut() {
                    let size = renderer.window().inner_size();
                    self.camera.set_aspect_from_size(size.width, size.height);
                    renderer.resize(size);
                    if size.width > 0 && size.height > 0 {
                        renderer.window().request_redraw();
                    }
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.last_frame = Instant::now();
                if !occluded && let Some(renderer) = self.renderer.as_ref() {
                    renderer.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.input.add_look_delta(delta.0, delta.1);
        }
    }
}

fn camera_action(key: KeyCode) -> Option<CameraAction> {
    match key {
        KeyCode::KeyW => Some(CameraAction::Forward),
        KeyCode::KeyS => Some(CameraAction::Backward),
        KeyCode::KeyA => Some(CameraAction::Left),
        KeyCode::KeyD => Some(CameraAction::Right),
        KeyCode::Space => Some(CameraAction::Up),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(CameraAction::Down),
        _ => None,
    }
}

struct FrameStats {
    report_started: Instant,
    frames: u64,
    accumulated_frame_time: Duration,
    uploads: u64,
    upload_bytes: u64,
    deactivations: u64,
    removals: u64,
    deferred_uploads: u64,
    demand_changes: u64,
}

impl FrameStats {
    fn new() -> Self {
        Self {
            report_started: Instant::now(),
            frames: 0,
            accumulated_frame_time: Duration::ZERO,
            uploads: 0,
            upload_bytes: 0,
            deactivations: 0,
            removals: 0,
            deferred_uploads: 0,
            demand_changes: 0,
        }
    }

    fn record(&mut self, _now: Instant, frame_time: Duration, report: FrameStreamingReport) {
        self.frames += 1;
        self.accumulated_frame_time += frame_time;
        self.uploads += report.uploads as u64;
        self.upload_bytes += report.upload_bytes as u64;
        self.deactivations += report.deactivations as u64;
        self.removals += report.removals as u64;
        self.deferred_uploads += report.deferred_uploads as u64;
        self.demand_changes += u64::from(report.demand_changed);
    }

    /// One aggregate line per interval; never one line per chunk per frame.
    fn report_if_due(&mut self, now: Instant, streaming: &StreamingBridge, renderer: &Renderer) {
        let report_span = now.saturating_duration_since(self.report_started);
        if report_span < FRAME_REPORT_INTERVAL || self.frames == 0 {
            return;
        }

        let average_ms = self.accumulated_frame_time.as_secs_f64() * 1000.0 / self.frames as f64;
        let observed_fps = self.frames as f64 / report_span.as_secs_f64();
        let runtime = streaming.runtime();
        let demand = runtime.demand();
        let summary = runtime.summary();
        let metrics = runtime.metrics();
        let totals = streaming.totals();
        let gpu = renderer.residency();
        info!(
            frames = self.frames,
            report_seconds = report_span.as_secs_f64(),
            average_wall_frame_ms = average_ms,
            observed_fps,
            camera_chunk = ?runtime.center(),
            demand_changes = self.demand_changes,
            render = demand.render.len(),
            dependency = demand.dependency.len(),
            retention = demand.retention.len(),
            tracked = summary.tracked,
            cpu_resident = summary.cpu_resident,
            known_absent = summary.known_absent,
            evict_pending = summary.evict_pending,
            load_queued = summary.load_queued,
            loading = summary.loading,
            mesh_waiting = summary.mesh_waiting,
            mesh_dirty = summary.mesh_dirty,
            mesh_meshing = summary.mesh_meshing,
            mesh_ready = summary.mesh_ready,
            queued_loads = summary.queued_loads,
            queued_meshes = summary.queued_meshes,
            jobs_in_flight = summary.jobs_in_flight,
            presented = streaming.presented_count(),
            gpu_resident = gpu.resident,
            gpu_active = gpu.active,
            pending_gpu_removal = streaming.pending_removal_count(),
            "M3B streaming state"
        );
        info!(
            loads_dispatched = metrics.load_jobs_dispatched,
            meshes_dispatched = metrics.mesh_jobs_dispatched,
            stale_loads = metrics.stale_load_results,
            stale_meshes = metrics.stale_mesh_results,
            fairness_loads = metrics.fairness_load_dispatches,
            hard_cap_blocks = metrics.hard_cap_blocks,
            cpu_evictions = metrics.cpu_evictions_finalized,
            eviction_budget_hits = metrics.eviction_budget_hits,
            interval_uploads = self.uploads,
            interval_upload_bytes = self.upload_bytes,
            interval_deactivations = self.deactivations,
            interval_removals = self.removals,
            interval_deferred_uploads = self.deferred_uploads,
            total_uploads = totals.uploads,
            total_upload_bytes = totals.upload_bytes,
            empty_meshes = totals.empty_meshes_presented,
            oversized_uploads = totals.oversized_uploads,
            upload_failures = totals.upload_failures,
            removal_budget_hits = totals.removal_budget_hits,
            anchor_rejections = totals.anchor_rejections,
            snapshot_bytes_dispatched = metrics.snapshot_bytes_dispatched,
            resident_payload_bytes = summary.resident_payload_bytes,
            cpu_mesh_bytes = summary.cpu_mesh_bytes,
            gpu_bytes = gpu.bytes,
            "M3B streaming work and budgets"
        );
        self.report_started = now;
        self.frames = 0;
        self.accumulated_frame_time = Duration::ZERO;
        self.uploads = 0;
        self.upload_bytes = 0;
        self.deactivations = 0;
        self.removals = 0;
        self.deferred_uploads = 0;
        self.demand_changes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::camera_action;
    use crate::input::CameraAction;
    use winit::keyboard::KeyCode;

    #[test]
    fn platform_keys_translate_to_camera_actions() {
        assert_eq!(camera_action(KeyCode::KeyW), Some(CameraAction::Forward));
        assert_eq!(camera_action(KeyCode::KeyD), Some(CameraAction::Right));
        assert_eq!(camera_action(KeyCode::Space), Some(CameraAction::Up));
        assert_eq!(camera_action(KeyCode::KeyQ), None);
    }
}
