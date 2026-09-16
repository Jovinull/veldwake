use std::{f32::consts::FRAC_PI_2, time::Duration};

use glam::{Mat4, Vec3};

use crate::input::InputState;

const DEFAULT_ASPECT: f32 = 16.0 / 9.0;
const MAX_PITCH_RADIANS: f32 = FRAC_PI_2 - 0.01;
const MAX_PRESENTATION_DELTA: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    aspect: f32,
    vertical_fov_radians: f32,
    near_plane: f32,
    far_plane: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(42.0, 32.0, 58.0),
            yaw: -0.79,
            pitch: -0.40,
            aspect: DEFAULT_ASPECT,
            vertical_fov_radians: 55.0_f32.to_radians(),
            near_plane: 0.1,
            far_plane: 200.0,
        }
    }
}

impl Camera {
    pub fn set_aspect_from_size(&mut self, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }

        self.aspect = width as f32 / height as f32;
        true
    }

    pub fn view_projection(&self) -> Mat4 {
        let projection = glam::camera::rh::proj::directx::perspective(
            self.vertical_fov_radians,
            self.aspect,
            self.near_plane,
            self.far_plane,
        );
        let view = glam::camera::rh::view::look_at_mat4(
            self.position,
            self.position + self.forward(),
            Vec3::Y,
        );
        projection * view
    }

    fn forward(&self) -> Vec3 {
        let pitch_cos = self.pitch.cos();
        Vec3::new(
            self.yaw.sin() * pitch_cos,
            self.pitch.sin(),
            -self.yaw.cos() * pitch_cos,
        )
    }

    fn horizontal_forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraController {
    movement_speed: f32,
    look_sensitivity: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            movement_speed: 12.0,
            look_sensitivity: 0.0025,
        }
    }
}

impl CameraController {
    pub fn update(&self, camera: &mut Camera, input: &mut InputState, elapsed: Duration) {
        let (look_x, look_y) = input.take_look_delta();
        if look_x.is_finite() && look_y.is_finite() {
            camera.yaw -= look_x * self.look_sensitivity;
            camera.pitch = (camera.pitch - look_y * self.look_sensitivity)
                .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);
        }

        let axes = input.movement_axes();
        let right = camera.horizontal_forward().cross(Vec3::Y).normalize();
        let mut direction =
            camera.horizontal_forward() * axes.forward + right * axes.right + Vec3::Y * axes.up;
        if direction.length_squared() > 1.0 {
            direction = direction.normalize();
        }

        let bounded_elapsed = elapsed.min(MAX_PRESENTATION_DELTA).as_secs_f32();
        camera.position += direction * self.movement_speed * bounded_elapsed;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glam::Vec3;

    use super::{Camera, CameraController, MAX_PITCH_RADIANS};
    use crate::input::{CameraAction, InputState};

    const EPSILON: f32 = 0.000_01;

    #[test]
    fn diagonal_movement_is_normalized() {
        let mut camera = Camera {
            position: Vec3::ZERO,
            ..Default::default()
        };
        let mut input = InputState::default();
        input.set_action(CameraAction::Forward, true);
        input.set_action(CameraAction::Right, true);

        CameraController::default().update(&mut camera, &mut input, Duration::from_secs(1));

        assert!((camera.position.length() - 1.2).abs() < EPSILON);
    }

    #[test]
    fn pitch_is_bounded() {
        let mut camera = Camera::default();
        let mut input = InputState::default();
        input.set_look_active(true);
        input.add_look_delta(0.0, -1_000_000.0);

        CameraController::default().update(&mut camera, &mut input, Duration::ZERO);

        assert!((camera.pitch - MAX_PITCH_RADIANS).abs() < EPSILON);
    }

    #[test]
    fn zero_sized_projection_update_is_rejected() {
        let mut camera = Camera::default();
        let original = camera.aspect;

        assert!(!camera.set_aspect_from_size(1920, 0));
        assert_eq!(camera.aspect, original);
        assert!(camera.set_aspect_from_size(800, 600));
        assert!((camera.aspect - (4.0 / 3.0)).abs() < EPSILON);
    }

    #[test]
    fn presentation_delta_is_clamped_after_a_pause() {
        let mut camera = Camera {
            position: Vec3::ZERO,
            ..Default::default()
        };
        let mut input = InputState::default();
        input.set_action(CameraAction::Forward, true);

        CameraController::default().update(&mut camera, &mut input, Duration::from_secs(30));

        assert!((camera.position.length() - 1.2).abs() < EPSILON);
    }

    #[test]
    fn view_projection_is_finite() {
        let matrix = Camera::default().view_projection().to_cols_array();
        assert!(matrix.into_iter().all(f32::is_finite));
    }
}
