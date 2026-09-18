use std::{f32::consts::FRAC_PI_2, time::Duration};

use glam::{Mat4, Vec2, Vec3};

use veldwake_character::GroundSampler;

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
            // Far enough to reach the M4 golden profile's visible radius plus
            // the diagonal, so the far plane never clips terrain the streaming
            // runtime has already paid to load and mesh.
            far_plane: 460.0,
        }
    }
}

impl Camera {
    /// A camera at a named pose, in the convention `region::CameraPose` uses:
    /// yaw in degrees clockwise from `-Z`, pitch in degrees above the horizon.
    #[must_use]
    pub fn at(position: Vec3, yaw_degrees: f32, pitch_degrees: f32) -> Self {
        Self {
            position,
            yaw: yaw_degrees.to_radians(),
            pitch: pitch_degrees
                .to_radians()
                .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS),
            ..Default::default()
        }
    }

    /// Finite world-unit position; one world unit is one voxel edge.
    pub const fn position(&self) -> Vec3 {
        self.position
    }

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

    pub fn forward(&self) -> Vec3 {
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

    /// Places the camera without disturbing its projection.
    ///
    /// [`Camera::at`] rebuilds from the default, which would silently drop the
    /// aspect ratio the window set; a follow camera has to move every frame, so it
    /// needs a mutator rather than a constructor.
    pub fn place(&mut self, position: Vec3, yaw: f32, pitch: f32) {
        if !position.is_finite() || !yaw.is_finite() || !pitch.is_finite() {
            return;
        }
        self.position = position;
        self.yaw = yaw;
        self.pitch = pitch.clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);
    }

    /// The planar direction the camera looks along, which is the frame a player's
    /// movement intent is expressed in.
    pub fn planar_forward(&self) -> Vec2 {
        let forward = self.horizontal_forward();
        Vec2::new(forward.x, forward.z)
    }

    /// The planar direction to the camera's right.
    pub fn planar_right(&self) -> Vec2 {
        let forward = self.planar_forward();
        Vec2::new(-forward.y, forward.x)
    }
}

/// How far behind the anchor a follow camera sits, in world units.
const FOLLOW_DISTANCE: f32 = 6.2;
/// How far above the stand point the camera aims, in world units.
///
/// Above the player's own head, which the first real run showed is necessary: at
/// chest height and directly behind, the player's body stood exactly in front of
/// the adversary six units beyond it and hid the thing the player is fighting.
const FOLLOW_EYE_HEIGHT: f32 = 2.10;
/// How far to the camera's right the view is offset, in world units.
///
/// The other half of that fix, and the reason every third-person action camera
/// does it: a view directly down the player's spine has the player between the
/// viewer and everything the player cares about. Just over a body's width is
/// enough to see past it without the shot becoming a side view.
const FOLLOW_LATERAL: f32 = 1.15;
/// Pitch the follow camera starts at, in radians below the horizon.
const FOLLOW_PITCH: f32 = -0.24;
/// How quickly the anchor catches up with the body, in seconds.
///
/// Small, but not zero. **The anchor is the stand point, never the pelvis**: the
/// gait drops and bobs the pelvis by a fraction of a leg length every step, and a
/// camera that followed it would pulse the whole screen at the step frequency.
const FOLLOW_ANCHOR_TAU: f32 = 0.08;
/// Lowest the camera may sit above the ground under it, in world units.
const FOLLOW_GROUND_CLEARANCE: f32 = 0.55;

/// How long one confirmed hit moves the camera, in combat ticks.
///
/// Sixteen ticks is `0.133` seconds at `120` Hz: long enough to read as an
/// impact and short enough that two hits in quick succession do not merge into
/// one continuous wobble. It is a tick count rather than a duration because a
/// capture has to be able to reproduce it exactly, and because a frozen named
/// moment runs no ticks and must therefore hold whatever the camera was doing.
pub const SHAKE_TICKS: u32 = 16;

/// The largest the camera is ever displaced by a hit, in world units.
///
/// A hard cap, not a starting point. `0.055` is a twelfth of the distance from
/// the camera to the body it follows and about two thirds of a character voxel:
/// visible in a motion strip, and small enough that it cannot put a body out of
/// frame or a camera through a hillside. The amplitude decays from here to zero
/// and nothing may scale it up — [`HitShake::strike`] restarts the decay rather
/// than adding to it, so a second hit during the first cannot compound.
pub const SHAKE_MAX_OFFSET: f32 = 0.055;

/// The camera's response to a landed hit.
///
/// Confirmed hits only. It has no idea what a miss, a dodge or the start of a
/// swing is, which is the point: the only way to move the camera through this
/// type is to call [`Self::strike`], and the only caller does so from a
/// `CombatEvent::Hit`. Presentation cannot invent a hit the rules did not
/// produce.
///
/// Deterministic by construction: the displacement is a function of an integer
/// tick counter and nothing else — no wall clock, no random source, no frame
/// rate. The same hit at the same tick displaces the camera by the same amount
/// on every run and on every host.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitShake {
    /// Ticks since the most recent hit. At or past [`SHAKE_TICKS`] the camera is
    /// still, and `None` before any hit at all.
    elapsed: Option<u32>,
    /// How many hits have moved the camera, for the observability line.
    strikes: u64,
}

impl HitShake {
    /// Registers one confirmed hit.
    ///
    /// Restarts the decay rather than adding to it, so the cap holds however
    /// many hits land at once.
    pub fn strike(&mut self) {
        self.elapsed = Some(0);
        self.strikes = self.strikes.saturating_add(1);
    }

    /// Advances by however many combat ticks the frame ran, which may be none.
    pub fn advance(&mut self, ticks: u32) {
        if let Some(elapsed) = self.elapsed {
            self.elapsed = Some(elapsed.saturating_add(ticks).min(SHAKE_TICKS));
        }
    }

    /// Whether the camera is still moving.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.elapsed.is_some_and(|elapsed| elapsed < SHAKE_TICKS)
    }

    #[must_use]
    pub const fn strikes(&self) -> u64 {
        self.strikes
    }

    /// How far the camera is displaced now, given its own right vector.
    ///
    /// Sideways and vertical only. Along the view direction a displacement of
    /// this size is invisible, and across it, it reads.
    #[must_use]
    pub fn offset(&self, right: Vec3) -> Vec3 {
        let Some(elapsed) = self.elapsed else {
            return Vec3::ZERO;
        };
        if elapsed >= SHAKE_TICKS {
            return Vec3::ZERO;
        }
        // Two periods that do not divide each other, so the decay reads as a
        // knock rather than as one clean oscillation.
        let t = f32::from(u16::try_from(elapsed).unwrap_or(u16::MAX));
        let decay = 1.0 - t / f32::from(u16::try_from(SHAKE_TICKS).unwrap_or(u16::MAX));
        let amplitude = SHAKE_MAX_OFFSET * decay;
        let lateral = (t * 1.9).sin();
        let vertical = (t * 2.7 + 1.1).sin();
        let offset = right * (amplitude * lateral) + Vec3::Y * (amplitude * vertical);
        // The cap is enforced here as well as derived, because a cap that is
        // only implied by the arithmetic is one refactor from being untrue.
        offset.clamp_length_max(SHAKE_MAX_OFFSET)
    }
}

/// A third-person camera that orbits a body.
///
/// Deliberately not a camera system: there is no occlusion solving, no collision
/// volume, no lock-on and no field-of-view response. What it does is follow a
/// stable anchor, take its yaw and pitch from the mouse, and refuse to sit inside
/// the ground.
#[derive(Clone, Copy, Debug)]
pub struct FollowController {
    yaw: f32,
    pitch: f32,
    distance: f32,
    anchor: Vec3,
    settled: bool,
    look_sensitivity: f32,
}

impl Default for FollowController {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: FOLLOW_PITCH,
            distance: FOLLOW_DISTANCE,
            anchor: Vec3::ZERO,
            settled: false,
            look_sensitivity: 0.0025,
        }
    }
}

impl FollowController {
    /// Starts the camera behind a body looking the way that body faces.
    #[must_use]
    pub fn behind(anchor: Vec3, facing: f32) -> Self {
        Self {
            yaw: facing,
            anchor,
            settled: true,
            ..Self::default()
        }
    }

    /// Moves the camera for one frame.
    ///
    /// `target` is the point the followed body stands on. `offset` is an extra
    /// world-space displacement, which is how a hit's camera response reaches the
    /// camera without this controller knowing what a hit is.
    pub fn update(
        &mut self,
        camera: &mut Camera,
        input: &mut InputState,
        target: Vec3,
        offset: Vec3,
        elapsed: Duration,
        ground: Option<&dyn GroundSampler>,
    ) {
        let (look_x, look_y) = input.take_look_delta();
        if look_x.is_finite() && look_y.is_finite() {
            self.yaw -= look_x * self.look_sensitivity;
            self.pitch = (self.pitch - look_y * self.look_sensitivity)
                .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);
        }
        if !target.is_finite() {
            return;
        }
        if self.settled {
            let step = elapsed.min(MAX_PRESENTATION_DELTA).as_secs_f32();
            let alpha = 1.0 - (-step / FOLLOW_ANCHOR_TAU).exp();
            self.anchor += (target - self.anchor) * alpha;
        } else {
            self.anchor = target;
            self.settled = true;
        }

        let eye = self.anchor + Vec3::Y * FOLLOW_EYE_HEIGHT;
        let pitch_cos = self.pitch.cos();
        let forward = Vec3::new(
            self.yaw.sin() * pitch_cos,
            self.pitch.sin(),
            -self.yaw.cos() * pitch_cos,
        );
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let mut position = eye - forward * self.distance + right * FOLLOW_LATERAL + offset;
        // The one concession to the world: a camera inside a hillside photographs
        // the inside of a hillside. There is no occlusion solving beyond this.
        if let Some(ground) = ground
            && let Some(height) = ground.surface(f64::from(position.x), f64::from(position.z))
        {
            let floor = height as f32 + FOLLOW_GROUND_CLEARANCE;
            if position.y < floor {
                position.y = floor;
            }
        }
        // The camera is offset to one side, so it has to yaw back toward the
        // anchor or the player drifts out of frame.
        let toward = eye - position;
        let yaw = toward.x.atan2(-toward.z);
        camera.place(position, yaw, self.pitch);
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

    use super::{HitShake, SHAKE_MAX_OFFSET, SHAKE_TICKS};

    #[test]
    fn a_camera_that_was_never_hit_does_not_move() {
        let shake = HitShake::default();
        assert!(!shake.is_active());
        assert_eq!(shake.strikes(), 0);
        assert_eq!(shake.offset(Vec3::X), Vec3::ZERO);
        // Advancing without a hit is still nothing: a fight full of misses,
        // dodges and swings that never land leaves the camera alone.
        let mut shake = shake;
        for _ in 0..1_000 {
            shake.advance(4);
            assert_eq!(shake.offset(Vec3::X), Vec3::ZERO);
        }
        assert!(!shake.is_active());
    }

    #[test]
    fn one_hit_moves_the_camera_and_stops() {
        let mut shake = HitShake::default();
        shake.strike();
        assert!(shake.is_active());
        assert_eq!(shake.strikes(), 1);
        let mut moved = 0_u32;
        for tick in 0..SHAKE_TICKS {
            let offset = shake.offset(Vec3::X);
            assert!(offset.is_finite(), "tick {tick} gave {offset}");
            if offset.length() > 0.0 {
                moved += 1;
            }
            shake.advance(1);
        }
        assert!(
            moved > SHAKE_TICKS / 2,
            "only {moved} ticks displaced anything"
        );
        // Exactly at the end, and forever after, the camera is still again.
        assert!(!shake.is_active());
        assert_eq!(shake.offset(Vec3::X), Vec3::ZERO);
        shake.advance(10_000);
        assert_eq!(shake.offset(Vec3::X), Vec3::ZERO);
    }

    #[test]
    fn no_number_of_hits_exceeds_the_amplitude_cap() {
        // The cap is the whole safety argument: a camera that can be driven
        // arbitrarily far by stacking hits can be driven out of the world.
        let mut shake = HitShake::default();
        let mut worst = 0.0_f32;
        for round in 0..200 {
            shake.strike();
            // A second hit inside the first, which is what a fast exchange does.
            if round % 3 == 0 {
                shake.strike();
            }
            for _ in 0..SHAKE_TICKS {
                worst = worst.max(shake.offset(Vec3::X).length());
                shake.advance(1);
            }
        }
        assert!(
            worst <= SHAKE_MAX_OFFSET + 1.0e-6,
            "the camera moved {worst} against a cap of {SHAKE_MAX_OFFSET}"
        );
        assert!(worst > 0.0, "the cap held by never moving at all");
    }

    #[test]
    fn the_same_ticks_give_the_same_camera() {
        // What makes a capture fixture reproducible. No wall clock and no random
        // source reach this, so two runs of the same tick sequence have to agree
        // exactly, not approximately.
        let run = || {
            let mut shake = HitShake::default();
            let mut trace = Vec::new();
            for tick in 0..64_u32 {
                if tick % 21 == 0 {
                    shake.strike();
                }
                trace.push(shake.offset(Vec3::X).to_array());
                shake.advance(if tick % 5 == 0 { 4 } else { 1 });
            }
            trace
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_frozen_frame_holds_the_camera_where_it_was() {
        // A named moment runs no ticks at all. The camera has to hold whatever
        // displacement it had, because a capture of a frozen frame that keeps
        // drifting is a capture of nothing in particular.
        let mut shake = HitShake::default();
        shake.strike();
        shake.advance(3);
        let held = shake.offset(Vec3::X);
        for _ in 0..500 {
            shake.advance(0);
            assert_eq!(shake.offset(Vec3::X), held);
        }
        assert!(held.length() > 0.0, "the held offset was nothing");
    }

    #[test]
    fn the_camera_is_displaced_across_the_view_and_never_along_it() {
        // Sideways and vertical read as an impact; forward and back read as a
        // zoom, and at this amplitude they read as nothing at all.
        let mut shake = HitShake::default();
        shake.strike();
        let right = Vec3::X;
        for _ in 0..SHAKE_TICKS {
            let offset = shake.offset(right);
            assert!(
                offset.z.abs() <= f32::EPSILON,
                "the camera moved {} along its own view",
                offset.z
            );
            shake.advance(1);
        }
    }

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
