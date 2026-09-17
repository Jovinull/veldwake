use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{debug, info, warn};
use veldwake_streaming::{CacheConfig, ChunkCache, DiagnosticChunkSource, StreamingConfig};
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
    debug::{DebugMode, debug_primitives},
    input::{CameraAction, InputState},
    renderer::{RenderOutcome, Renderer},
    streaming::{ChunkPresentation, FrameStreamingReport, StreamingBridge, UploadBudget},
};

const FRAME_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// Opt-in directory for the experimental M3D chunk cache. Unset means no
/// cache and no filesystem access from the streaming worker.
const CACHE_DIR_VARIABLE: &str = "VELDWAKE_CACHE_DIR";

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
    /// Active debug view, cycled by `F1`. `Off` is the shipped rendering.
    debug_mode: DebugMode,
    /// Whether the box-drawing views draw their boxes, toggled by `F2`.
    debug_boxes: bool,
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
            debug_mode: DebugMode::Off,
            debug_boxes: true,
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

        let profile = StreamingProfile::from_environment();
        let config = profile.config();
        let budget = UploadBudget::default();
        // The experimental disk cache is opt-in and off by default: without
        // the variable the client behaves exactly as before and touches no
        // filesystem. The client picks a directory and nothing else; the entry
        // format lives entirely inside the streaming crate.
        let cache = match std::env::var_os(CACHE_DIR_VARIABLE) {
            Some(dir) => {
                let cache_config = CacheConfig::new(PathBuf::from(dir));
                match ChunkCache::open(&cache_config, DiagnosticChunkSource.fingerprint()) {
                    Ok((cache, report)) => {
                        info!(
                            entries_dir = %report.entries_dir.display(),
                            entries = report.footprint.entries,
                            disk_bytes = report.footprint.bytes,
                            swept_temporaries = report.temporaries_removed,
                            "experimental chunk cache opened"
                        );
                        Some(cache)
                    }
                    Err(error) => {
                        // A cache is an accelerator: failing to open one must
                        // never stop the client from running without it.
                        warn!(%error, "chunk cache could not be opened; continuing without it");
                        None
                    }
                }
            }
            None => None,
        };
        let streaming = match cache {
            Some(cache) => {
                StreamingBridge::with_cache(config, budget, self.camera.position(), cache)
            }
            None => StreamingBridge::new(config, budget, self.camera.position()),
        }
        .map_err(|error| AppRunError(error.to_string()))?;
        let renderer = pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(),
            Arc::clone(&window),
            &self.camera,
        ))
        .map_err(|error| AppRunError(error.to_string()))?;

        info!(
            profile = profile.name(),
            lod_selection = ?config.lod_selection,
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
            "M3B diagnostic controls: WASD move, Space/Ctrl vertical, hold right mouse to look, \
             F1 cycles debug views (off/lod/residency/boundaries), F2 toggles debug boxes, \
             Escape exits"
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

        if pressed && let Some(action) = debug_action(code) {
            match action {
                DebugAction::CycleMode => self.debug_mode = self.debug_mode.next(),
                DebugAction::ToggleBoxes => self.debug_boxes = !self.debug_boxes,
            }
            info!(
                mode = self.debug_mode.name(),
                boxes = self.debug_boxes,
                boxes_apply = self.debug_mode.uses_boxes(),
                "debug view changed"
            );
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
        // the transition and counts every rejected frame, so no outcome needs
        // handling here.
        streaming.track_camera(self.camera.position());
        let report = match streaming.update(renderer) {
            Ok(report) => report,
            Err(error) => {
                self.fail_and_exit(event_loop, format!("streaming runtime failed: {error}"));
                return;
            }
        };
        // Off produces no primitives, debug-slot allocations, debug uniform
        // writes, or debug draws. Fixed startup resources and any reusable
        // slots retained after prior debug use still exist.
        let primitives = debug_primitives(streaming, self.debug_mode, self.debug_boxes);
        renderer.set_debug_primitives(&primitives);
        renderer.update_camera(&self.camera, self.debug_mode.lod_tint());
        let submit_started = Instant::now();
        let outcome = renderer.render();
        let submit_time = submit_started.elapsed();
        let request_next_redraw = match outcome {
            RenderOutcome::Rendered => {
                self.frame_stats
                    .record(now, elapsed, submit_time, report, streaming);
                self.frame_stats.report_if_due(
                    now,
                    streaming,
                    renderer,
                    self.debug_mode,
                    self.debug_boxes,
                );
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

/// Keyboard control of the debug views. Separate from camera actions so the
/// mapping is testable without a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DebugAction {
    CycleMode,
    ToggleBoxes,
}

const fn debug_action(key: KeyCode) -> Option<DebugAction> {
    match key {
        KeyCode::F1 => Some(DebugAction::CycleMode),
        KeyCode::F2 => Some(DebugAction::ToggleBoxes),
        _ => None,
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

/// Which streaming configuration the diagnostic client runs. Selected by the
/// `VELDWAKE_PROFILE` environment variable so no CLI dependency is needed:
/// `default` (M3B), `m3c-baseline` (radius 3, `Lod0` only), `m3c-banded`
/// (radius 3, `Lod0`/`Lod1` band).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamingProfile {
    Default,
    M3cBaseline,
    M3cBanded,
}

impl StreamingProfile {
    fn from_environment() -> Self {
        match std::env::var("VELDWAKE_PROFILE") {
            Ok(value) => match Self::parse(&value) {
                Some(profile) => profile,
                None => {
                    warn!(%value, "unknown VELDWAKE_PROFILE; using default");
                    Self::Default
                }
            },
            Err(_) => Self::Default,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "default" | "m3b" => Some(Self::Default),
            "m3c-baseline" | "baseline" => Some(Self::M3cBaseline),
            "m3c-banded" | "banded" | "lod" => Some(Self::M3cBanded),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::M3cBaseline => "m3c-baseline",
            Self::M3cBanded => "m3c-banded",
        }
    }

    const fn config(self) -> StreamingConfig {
        match self {
            Self::Default => StreamingConfig::default_profile(),
            Self::M3cBaseline => StreamingConfig::m3c_baseline(),
            Self::M3cBanded => StreamingConfig::m3c_diagnostic(),
        }
    }
}

struct FrameStats {
    report_started: Instant,
    started: Instant,
    frames: u64,
    accumulated_frame_time: Duration,
    submit_total: Duration,
    submit_max: Duration,
    /// First time the runtime reported idle with every render chunk decided.
    idle_reached: Option<Duration>,
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
            started: Instant::now(),
            frames: 0,
            accumulated_frame_time: Duration::ZERO,
            submit_total: Duration::ZERO,
            submit_max: Duration::ZERO,
            idle_reached: None,
            uploads: 0,
            upload_bytes: 0,
            deactivations: 0,
            removals: 0,
            deferred_uploads: 0,
            demand_changes: 0,
        }
    }

    fn record(
        &mut self,
        now: Instant,
        frame_time: Duration,
        submit_time: Duration,
        report: FrameStreamingReport,
        streaming: &StreamingBridge,
    ) {
        self.frames += 1;
        self.accumulated_frame_time += frame_time;
        self.submit_total += submit_time;
        self.submit_max = self.submit_max.max(submit_time);
        if self.idle_reached.is_none()
            && streaming.runtime().is_idle()
            && streaming.pending_removal_count() == 0
            && streaming.staged_count() == 0
            && report.uploads == 0
        {
            let elapsed = now.saturating_duration_since(self.started);
            self.idle_reached = Some(elapsed);
            let gpu_active = streaming.presented_count();
            info!(
                time_to_idle_ms = elapsed.as_secs_f64() * 1000.0,
                presented = gpu_active,
                "M3C streaming reached idle coverage"
            );
        }
        self.uploads += report.uploads as u64;
        self.upload_bytes += report.upload_bytes as u64;
        self.deactivations += report.deactivations as u64;
        self.removals += report.removals as u64;
        self.deferred_uploads += report.deferred_uploads as u64;
        self.demand_changes += u64::from(report.demand_changed);
    }

    /// One aggregate line per interval; never one line per chunk per frame.
    fn report_if_due(
        &mut self,
        now: Instant,
        streaming: &StreamingBridge,
        renderer: &Renderer,
        debug_mode: DebugMode,
        debug_boxes: bool,
    ) {
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
        let gaps = streaming.gaps();
        let gpu = renderer.residency();
        let chunk_mesh_gpu_bytes = streaming.chunk_mesh_gpu_bytes();
        let debug_work = renderer.debug_frame_work();
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
            gpu_resident = gpu.resident(),
            gpu_active = gpu.active(),
            gpu_quads = gpu.quads(),
            lod0_desired = summary.lod0_desired,
            lod1_desired = summary.lod1_desired,
            lod0_ready = summary.lod0_ready,
            lod1_ready = summary.lod1_ready,
            lod0_gpu_resident = gpu.lod0.resident,
            lod0_gpu_active = gpu.lod0.active,
            lod0_gpu_quads = gpu.lod0.quads,
            lod0_gpu_bytes = gpu.lod0.bytes,
            lod1_gpu_resident = gpu.lod1.resident,
            lod1_gpu_active = gpu.lod1.active,
            lod1_gpu_quads = gpu.lod1.quads,
            lod1_gpu_bytes = gpu.lod1.bytes,
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
            chunk_mesh_active_bytes = gpu.bytes(),
            total_uploads_lod0 = totals.uploads_lod0,
            total_uploads_lod1 = totals.uploads_lod1,
            total_upload_bytes_lod0 = totals.upload_bytes_lod0,
            total_upload_bytes_lod1 = totals.upload_bytes_lod1,
            lod_swaps = metrics.lod_swaps,
            stale_lod = metrics.stale_lod_results,
            gaps_closed = gaps.closed_gaps(),
            gaps_lod = gaps.lod_gaps,
            gaps_neighbor = gaps.neighbor_presentation_gaps,
            gaps_membership = gaps.membership_gaps,
            gaps_data = gaps.data_gaps,
            gaps_unattributed = gaps.unattributed_gaps,
            gap_frames_total = gaps.gap_frames_total,
            gap_frames_max = gaps.max_gap_frames,
            gap_max_simultaneous = gaps.max_simultaneous_missing,
            gap_current_missing = gaps.current_missing,
            gap_frames_with_missing = gaps.frames_with_missing,
            ready_undrawn_now = gaps.ready_undrawn_now,
            ready_undrawn_max = gaps.ready_undrawn_max,
            ready_undrawn_frames = gaps.ready_undrawn_frames,
            ready_undrawn_chunk_frames = gaps.ready_undrawn_chunk_frames,
            frontier_pipeline_pending_now = gaps.frontier_pipeline_pending_now,
            frontier_pipeline_pending_max = gaps.frontier_pipeline_pending_max,
            ready_awaiting_upload_now = gaps.ready_awaiting_upload_now,
            ready_awaiting_upload_max = gaps.ready_awaiting_upload_max,
            ready_blocked_transition_now = gaps.ready_blocked_transition_now,
            ready_blocked_transition_max = gaps.ready_blocked_transition_max,
            committed_missing_now = gaps.committed_missing_now,
            committed_missing_max = gaps.committed_missing_max,
            blocked_groups_now = gaps.blocked_groups_now,
            blocked_group_max = gaps.blocked_group_max,
            constrained_undrawn_now = gaps.constrained_undrawn_now,
            constrained_undrawn_max = gaps.constrained_undrawn_max,
            transition_commits = totals.transition_commits,
            transition_chunks = totals.transition_chunks,
            restaged = totals.restaged,
            staged_discarded = totals.staged_discarded,
            staged_now = streaming.staged_count(),
            staged_bytes = streaming.staged_bytes(),
            peak_staged_bytes = totals.peak_staged_bytes,
            committed_retained = metrics.committed_retained,
            committed_dropped = metrics.committed_dropped,
            transition_pending = summary.transition_pending,
            gpu_staged = gpu.staged(),
            gpu_staged_bytes = gpu.staged_bytes(),
            chunk_mesh_committed_bytes = chunk_mesh_gpu_bytes.committed,
            chunk_mesh_total_bytes = chunk_mesh_gpu_bytes.total(),
            peak_chunk_mesh_committed_bytes = totals.peak_chunk_mesh_committed_bytes,
            peak_chunk_mesh_total_bytes = totals.peak_chunk_mesh_total_bytes,
            presentation_commit_failures = totals.presentation_commit_failures,
            commit_invariant_failures = totals.commit_invariant_failures,
            debug_mode = debug_mode.name(),
            debug_boxes,
            debug_draws = renderer.debug_draw_count(),
            debug_slots = renderer.debug_slot_count(),
            debug_primitive_allocations = debug_work.primitive_allocations,
            debug_uniform_writes = debug_work.uniform_writes,
            snapshot_build_total_us = metrics.snapshot_build.total_us,
            snapshot_build_max_us = metrics.snapshot_build.max_us,
            lod1_derivation_total_us = metrics.lod1_derivation.total_us,
            lod1_derivation_max_us = metrics.lod1_derivation.max_us,
            worker_mesh_lod0_total_us = metrics.worker_mesh_lod0.total_us,
            worker_mesh_lod0_max_us = metrics.worker_mesh_lod0.max_us,
            worker_mesh_lod1_total_us = metrics.worker_mesh_lod1.total_us,
            worker_mesh_lod1_max_us = metrics.worker_mesh_lod1.max_us,
            interval_submit_mean_us = self.submit_total.as_micros() / u128::from(self.frames),
            interval_submit_max_us = self.submit_max.as_micros(),
            time_to_idle_ms = self.idle_reached.map(|d| d.as_secs_f64() * 1000.0),
            "M3B streaming work and budgets"
        );
        self.report_started = now;
        self.frames = 0;
        self.accumulated_frame_time = Duration::ZERO;
        self.submit_total = Duration::ZERO;
        self.submit_max = Duration::ZERO;
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
    use super::{DebugAction, camera_action, debug_action};
    use crate::debug::DebugMode;
    use crate::input::CameraAction;
    use winit::keyboard::KeyCode;

    #[test]
    fn function_keys_drive_the_debug_views_and_nothing_else_does() {
        assert_eq!(debug_action(KeyCode::F1), Some(DebugAction::CycleMode));
        assert_eq!(debug_action(KeyCode::F2), Some(DebugAction::ToggleBoxes));
        assert_eq!(debug_action(KeyCode::F3), None);
        assert_eq!(debug_action(KeyCode::KeyW), None);
        assert_eq!(camera_action(KeyCode::F1), None);
        assert_eq!(camera_action(KeyCode::F2), None);

        // Four presses of F1 return to the shipped rendering.
        let mut mode = DebugMode::Off;
        for _ in 0..4 {
            mode = mode.next();
        }
        assert_eq!(mode, DebugMode::Off);
    }

    #[test]
    fn streaming_profile_parses_documented_names_only() {
        use super::StreamingProfile;
        use veldwake_streaming::StreamingConfig;
        assert_eq!(
            StreamingProfile::parse("default"),
            Some(StreamingProfile::Default)
        );
        assert_eq!(StreamingProfile::parse(""), Some(StreamingProfile::Default));
        assert_eq!(
            StreamingProfile::parse(" M3C-Baseline "),
            Some(StreamingProfile::M3cBaseline)
        );
        assert_eq!(
            StreamingProfile::parse("banded"),
            Some(StreamingProfile::M3cBanded)
        );
        assert_eq!(StreamingProfile::parse("m3d"), None);
        assert_eq!(
            StreamingProfile::Default.config(),
            StreamingConfig::default()
        );
        assert_eq!(
            StreamingProfile::M3cBaseline.config(),
            StreamingConfig::m3c_baseline()
        );
        assert_eq!(
            StreamingProfile::M3cBanded.config(),
            StreamingConfig::m3c_diagnostic()
        );
    }

    #[test]
    fn platform_keys_translate_to_camera_actions() {
        assert_eq!(camera_action(KeyCode::KeyW), Some(CameraAction::Forward));
        assert_eq!(camera_action(KeyCode::KeyD), Some(CameraAction::Right));
        assert_eq!(camera_action(KeyCode::Space), Some(CameraAction::Up));
        assert_eq!(camera_action(KeyCode::KeyQ), None);
    }
}
