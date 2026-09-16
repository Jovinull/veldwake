use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
    time::{Duration, Instant},
};

use tracing::{debug, info};
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
            .with_title("Veldwake — M1 Diagnostic Renderer")
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
        let renderer = pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(),
            Arc::clone(&window),
            &self.camera,
        ))
        .map_err(|error| AppRunError(error.to_string()))?;

        self.last_frame = Instant::now();
        self.renderer = Some(renderer);
        window.set_visible(true);
        window.request_redraw();
        info!(
            "M1 diagnostic controls: WASD move, Space/Ctrl vertical, hold right mouse to look, Escape exits"
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

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        renderer.update_camera(&self.camera);
        let request_next_redraw = match renderer.render() {
            RenderOutcome::Rendered => {
                self.frame_stats.record(now, elapsed);
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
}

impl FrameStats {
    fn new() -> Self {
        Self {
            report_started: Instant::now(),
            frames: 0,
            accumulated_frame_time: Duration::ZERO,
        }
    }

    fn record(&mut self, now: Instant, frame_time: Duration) {
        self.frames += 1;
        self.accumulated_frame_time += frame_time;
        let report_span = now.saturating_duration_since(self.report_started);
        if report_span < FRAME_REPORT_INTERVAL || self.frames == 0 {
            return;
        }

        let average_ms = self.accumulated_frame_time.as_secs_f64() * 1000.0 / self.frames as f64;
        let observed_fps = self.frames as f64 / report_span.as_secs_f64();
        info!(
            frames = self.frames,
            report_seconds = report_span.as_secs_f64(),
            average_wall_frame_ms = average_ms,
            observed_fps,
            "presentation timing sample"
        );
        self.report_started = now;
        self.frames = 0;
        self.accumulated_frame_time = Duration::ZERO;
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
