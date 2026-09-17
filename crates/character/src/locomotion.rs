//! Analytical locomotion: a walk and a run expressed as functions of phase.
//!
//! There is no animation graph, no clip format, and no sampler. A gait is a
//! handful of named amplitudes derived from the body's own proportions, and a
//! pose is those amplitudes evaluated at a phase. That choice is what lets the
//! character style contract state locomotion as numbers a test can check —
//! duty factor, stride, pelvis bob, counter-rotation — instead of as an
//! opinion about a curve.
//!
//! **Phase advances with distance, not with time.** One cycle is two steps, so
//! `phase += distance / (2 * stride)`. Stride is therefore an identity rather
//! than a tuning knob, and a foot that slides is a bug rather than a setting.
//!
//! Angles are signed rotations about a bone's local `X` axis and positive
//! moves the bone's tip forward, toward `-Z`. The knee and the elbow carry
//! flexion as a non-negative magnitude; the pose applies the anatomically
//! correct direction for each.

use crate::descriptor::{BodyMetrics, CHARACTER_VOXEL_SIZE};

/// Index of the left side in the per-side arrays.
pub const LEFT: usize = 0;
/// Index of the right side in the per-side arrays.
pub const RIGHT: usize = 1;

/// The joint ranges of the character style contract, in radians.
///
/// A rejection criterion, not a suggestion: every angle a pose produces is
/// clamped here, and a test asserts that the nominal gaits never need the
/// clamp.
#[derive(Clone, Copy, Debug)]
pub struct JointLimits {
    pub spine_yaw: (f32, f32),
    pub spine_pitch: (f32, f32),
    pub head_yaw: (f32, f32),
    pub head_pitch: (f32, f32),
    pub shoulder_pitch: (f32, f32),
    pub shoulder_roll: (f32, f32),
    pub elbow_flex: (f32, f32),
    pub wrist_pitch: (f32, f32),
    pub hip_pitch: (f32, f32),
    pub hip_roll: (f32, f32),
    pub knee_flex: (f32, f32),
    pub ankle_pitch: (f32, f32),
}

const fn degrees(low: f32, high: f32) -> (f32, f32) {
    (low, high)
}

/// The limits, in degrees, exactly as the style contract lists them.
pub const JOINT_LIMIT_DEGREES: JointLimits = JointLimits {
    spine_yaw: degrees(-18.0, 18.0),
    spine_pitch: degrees(-12.0, 20.0),
    head_yaw: degrees(-45.0, 45.0),
    head_pitch: degrees(-30.0, 30.0),
    shoulder_pitch: degrees(-75.0, 75.0),
    shoulder_roll: degrees(0.0, 35.0),
    elbow_flex: degrees(0.0, 135.0),
    wrist_pitch: degrees(-30.0, 30.0),
    hip_pitch: degrees(-55.0, 75.0),
    hip_roll: degrees(-12.0, 12.0),
    knee_flex: degrees(0.0, 140.0),
    ankle_pitch: degrees(-35.0, 35.0),
};

fn clamp_degrees(value: f32, range: (f32, f32)) -> f32 {
    let low = range.0.to_radians();
    let high = range.1.to_radians();
    value.clamp(low, high)
}

/// Whether a radian value already sits inside a degree range.
#[must_use]
pub fn within_degrees(value: f32, range: (f32, f32)) -> bool {
    let slack = 1.0e-4;
    value >= range.0.to_radians() - slack && value <= range.1.to_radians() + slack
}

/// Everything one gait needs, as multiples of the body's own measurements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaitProfile {
    /// Distance covered by one step, as a fraction of leg length.
    pub stride_per_leg_length: f32,
    /// Fraction of the cycle a given foot is planted.
    pub duty_factor: f32,
    /// Hip pitch amplitude, in radians.
    pub hip_swing: f32,
    /// Extra knee flexion during swing, in radians.
    pub knee_swing: f32,
    /// Knee flexion carried through stance, in radians.
    pub knee_stance: f32,
    /// Shoulder pitch amplitude, in radians.
    pub arm_swing: f32,
    /// Elbow flexion at the forward extreme, in radians.
    pub elbow_flex: f32,
    /// Pelvis vertical travel, peak to peak, as a fraction of leg length.
    pub pelvis_bob: f32,
    /// Pelvis lateral travel, peak to peak, as a fraction of hip width.
    pub pelvis_sway: f32,
    /// Pelvis yaw amplitude, in radians.
    pub pelvis_yaw: f32,
    /// Chest counter-rotation as a multiple of the pelvis yaw.
    pub chest_counter: f32,
    /// Forward lean, in radians.
    pub lean: f32,
}

impl GaitProfile {
    /// The walk. Every value sits inside the character style contract's bands.
    pub const WALK: Self = Self {
        stride_per_leg_length: 0.85,
        duty_factor: 0.62,
        hip_swing: 0.40,
        knee_swing: 0.95,
        knee_stance: 0.16,
        arm_swing: 0.30,
        elbow_flex: 0.38,
        pelvis_bob: 0.035,
        pelvis_sway: 0.040,
        pelvis_yaw: 0.10,
        chest_counter: 0.80,
        lean: 0.035,
    };

    /// The run.
    pub const RUN: Self = Self {
        stride_per_leg_length: 1.25,
        duty_factor: 0.40,
        hip_swing: 0.62,
        knee_swing: 1.55,
        knee_stance: 0.34,
        arm_swing: 0.62,
        elbow_flex: 1.15,
        pelvis_bob: 0.062,
        pelvis_sway: 0.022,
        pelvis_yaw: 0.16,
        chest_counter: 0.90,
        lean: 0.14,
    };

    fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            stride_per_leg_length: mix(self.stride_per_leg_length, other.stride_per_leg_length),
            duty_factor: mix(self.duty_factor, other.duty_factor),
            hip_swing: mix(self.hip_swing, other.hip_swing),
            knee_swing: mix(self.knee_swing, other.knee_swing),
            knee_stance: mix(self.knee_stance, other.knee_stance),
            arm_swing: mix(self.arm_swing, other.arm_swing),
            elbow_flex: mix(self.elbow_flex, other.elbow_flex),
            pelvis_bob: mix(self.pelvis_bob, other.pelvis_bob),
            pelvis_sway: mix(self.pelvis_sway, other.pelvis_sway),
            pelvis_yaw: mix(self.pelvis_yaw, other.pelvis_yaw),
            chest_counter: mix(self.chest_counter, other.chest_counter),
            lean: mix(self.lean, other.lean),
        }
    }
}

/// Speed thresholds, in world units per second.
///
/// Below `IDLE_SPEED` the character is standing; between there and
/// `WALK_SPEED` the walk fades in; above `RUN_SPEED` it is fully running.
pub const IDLE_SPEED: f32 = 0.15;
/// See [`IDLE_SPEED`].
pub const WALK_SPEED: f32 = 0.70;
/// See [`IDLE_SPEED`].
pub const RUN_BLEND_SPEED: f32 = 2.20;
/// See [`IDLE_SPEED`].
pub const RUN_SPEED: f32 = 3.60;

/// Idle breathing period, in seconds.
pub const BREATH_PERIOD: f32 = 3.6;
/// Idle breathing amplitude, in character voxels.
pub const BREATH_AMPLITUDE: f32 = 0.32;
/// Idle weight shift, in character voxels.
pub const IDLE_WEIGHT_SHIFT: f32 = 0.22;

/// How much of the cycle a gait is, blended between idle, walk, and run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaitBlend {
    /// Zero standing still, one fully in a gait.
    pub moving: f32,
    /// Zero walking, one running.
    pub run: f32,
}

/// The gait of one compiled character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaitParameters {
    leg_length_units: f32,
    hip_width_units: f32,
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge1 <= edge0 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl GaitParameters {
    /// Derives the gait from the body it belongs to.
    #[must_use]
    pub fn derive(body: &BodyMetrics) -> Self {
        Self {
            leg_length_units: body.leg_length as f32 * CHARACTER_VOXEL_SIZE,
            hip_width_units: body.hip_width as f32 * CHARACTER_VOXEL_SIZE,
        }
    }

    #[must_use]
    pub const fn leg_length_units(&self) -> f32 {
        self.leg_length_units
    }

    /// How far into a gait a speed puts the character.
    ///
    /// Both components are monotone and continuous in speed, which is what
    /// keeps the idle-to-walk and walk-to-run transitions free of a step.
    #[must_use]
    pub fn blend(&self, speed: f32) -> GaitBlend {
        let speed = if speed.is_finite() {
            speed.max(0.0)
        } else {
            0.0
        };
        GaitBlend {
            moving: smoothstep(IDLE_SPEED, WALK_SPEED, speed),
            run: smoothstep(RUN_BLEND_SPEED, RUN_SPEED, speed),
        }
    }

    /// The gait profile at a blend.
    #[must_use]
    pub fn profile(&self, blend: GaitBlend) -> GaitProfile {
        GaitProfile::WALK.lerp(GaitProfile::RUN, blend.run.clamp(0.0, 1.0))
    }

    /// Distance covered by one step, in world units.
    #[must_use]
    pub fn stride(&self, blend: GaitBlend) -> f32 {
        (self.profile(blend).stride_per_leg_length * self.leg_length_units).max(1.0e-4)
    }

    /// Steps per second at a speed, which is a report rather than an input.
    #[must_use]
    pub fn cadence(&self, speed: f32) -> f32 {
        let blend = self.blend(speed);
        speed / self.stride(blend)
    }

    /// Advances the locomotion phase by a travelled distance.
    ///
    /// One cycle is two steps, so the phase advances by half a cycle per
    /// stride. This is the whole foot-sliding argument: the phase is a
    /// function of distance, so the planted foot stays where the ground is.
    #[must_use]
    pub fn advance_phase(&self, phase: f32, distance: f32, blend: GaitBlend) -> f32 {
        let cycle = 2.0 * self.stride(blend);
        let advanced = phase + distance / cycle;
        if advanced.is_finite() {
            advanced.rem_euclid(1.0)
        } else {
            phase
        }
    }
}

/// A pose expressed as joint angles, before any transform is built.
///
/// Every field is clamped to the style contract's range by
/// [`JointAngles::clamped`], so a pose that leaves a joint range cannot reach
/// the skeleton.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointAngles {
    pub spine_yaw: f32,
    pub spine_pitch: f32,
    pub chest_yaw: f32,
    pub chest_pitch: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub shoulder_pitch: [f32; 2],
    pub shoulder_roll: [f32; 2],
    pub elbow_flex: [f32; 2],
    pub wrist_pitch: [f32; 2],
    pub hip_pitch: [f32; 2],
    pub hip_roll: [f32; 2],
    pub knee_flex: [f32; 2],
    pub ankle_pitch: [f32; 2],
    /// Pelvis vertical offset, in character voxels.
    pub pelvis_rise: f32,
    /// Pelvis lateral offset, in character voxels.
    pub pelvis_sway: f32,
    pub pelvis_yaw: f32,
    pub pelvis_roll: f32,
}

impl JointAngles {
    /// Clamps every joint to its declared range.
    #[must_use]
    pub fn clamped(mut self) -> Self {
        let limits = JOINT_LIMIT_DEGREES;
        self.spine_yaw = clamp_degrees(self.spine_yaw, limits.spine_yaw);
        self.spine_pitch = clamp_degrees(self.spine_pitch, limits.spine_pitch);
        self.chest_yaw = clamp_degrees(self.chest_yaw, limits.spine_yaw);
        self.chest_pitch = clamp_degrees(self.chest_pitch, limits.spine_pitch);
        self.head_yaw = clamp_degrees(self.head_yaw, limits.head_yaw);
        self.head_pitch = clamp_degrees(self.head_pitch, limits.head_pitch);
        for side in [LEFT, RIGHT] {
            self.shoulder_pitch[side] =
                clamp_degrees(self.shoulder_pitch[side], limits.shoulder_pitch);
            self.shoulder_roll[side] =
                clamp_degrees(self.shoulder_roll[side], limits.shoulder_roll);
            self.elbow_flex[side] = clamp_degrees(self.elbow_flex[side], limits.elbow_flex);
            self.wrist_pitch[side] = clamp_degrees(self.wrist_pitch[side], limits.wrist_pitch);
            self.hip_pitch[side] = clamp_degrees(self.hip_pitch[side], limits.hip_pitch);
            self.hip_roll[side] = clamp_degrees(self.hip_roll[side], limits.hip_roll);
            self.knee_flex[side] = clamp_degrees(self.knee_flex[side], limits.knee_flex);
            self.ankle_pitch[side] = clamp_degrees(self.ankle_pitch[side], limits.ankle_pitch);
        }
        self
    }

    /// Whether every joint already sits inside its range.
    #[must_use]
    pub fn within_limits(&self) -> bool {
        let limits = JOINT_LIMIT_DEGREES;
        let mut ok = within_degrees(self.spine_yaw, limits.spine_yaw)
            && within_degrees(self.spine_pitch, limits.spine_pitch)
            && within_degrees(self.chest_yaw, limits.spine_yaw)
            && within_degrees(self.chest_pitch, limits.spine_pitch)
            && within_degrees(self.head_yaw, limits.head_yaw)
            && within_degrees(self.head_pitch, limits.head_pitch);
        for side in [LEFT, RIGHT] {
            ok &= within_degrees(self.shoulder_pitch[side], limits.shoulder_pitch)
                && within_degrees(self.shoulder_roll[side], limits.shoulder_roll)
                && within_degrees(self.elbow_flex[side], limits.elbow_flex)
                && within_degrees(self.wrist_pitch[side], limits.wrist_pitch)
                && within_degrees(self.hip_pitch[side], limits.hip_pitch)
                && within_degrees(self.hip_roll[side], limits.hip_roll)
                && within_degrees(self.knee_flex[side], limits.knee_flex)
                && within_degrees(self.ankle_pitch[side], limits.ankle_pitch);
        }
        ok
    }

    /// Whether every value is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        let scalars = [
            self.spine_yaw,
            self.spine_pitch,
            self.chest_yaw,
            self.chest_pitch,
            self.head_yaw,
            self.head_pitch,
            self.pelvis_rise,
            self.pelvis_sway,
            self.pelvis_yaw,
            self.pelvis_roll,
        ];
        let arrays = [
            self.shoulder_pitch,
            self.shoulder_roll,
            self.elbow_flex,
            self.wrist_pitch,
            self.hip_pitch,
            self.hip_roll,
            self.knee_flex,
            self.ankle_pitch,
        ];
        scalars.into_iter().all(f32::is_finite)
            && arrays
                .into_iter()
                .all(|pair| pair.into_iter().all(f32::is_finite))
    }
}

/// A bell that is zero with zero slope at both ends and one in the middle.
///
/// `sin²` rather than `sin`, because `sin` leaves a slope discontinuity where
/// the stance and swing halves of a cycle meet, and that reads as a hitch.
fn bell(t: f32) -> f32 {
    let s = (std::f32::consts::PI * t.clamp(0.0, 1.0)).sin();
    s * s
}

/// Fraction of the cycle a foot has been planted, zero while it is swinging.
#[must_use]
pub fn stance_weight(phase: f32, duty: f32) -> f32 {
    let t = phase.rem_euclid(1.0);
    if duty <= 0.0 {
        return 0.0;
    }
    if t >= duty {
        return 0.0;
    }
    // Ease in and out of contact so the foot is not nailed down for exactly
    // one frame at each end of stance.
    let edge = (duty * 0.18).max(1.0e-3);
    let rise = (t / edge).clamp(0.0, 1.0);
    let fall = ((duty - t) / edge).clamp(0.0, 1.0);
    rise.min(fall)
}

/// The analytical pose of a moving character at one phase.
///
/// `phase` is the cycle position in `[0, 1)`, `time` drives the idle breath,
/// and `blend` decides how much of the gait applies at all.
#[must_use]
pub fn animate(gait: &GaitParameters, blend: GaitBlend, phase: f32, time: f32) -> JointAngles {
    let profile = gait.profile(blend);
    let moving = blend.moving.clamp(0.0, 1.0);
    let tau = std::f32::consts::TAU;
    let phase = if phase.is_finite() {
        phase.rem_euclid(1.0)
    } else {
        0.0
    };
    let time = if time.is_finite() { time } else { 0.0 };

    let mut angles = JointAngles::default();

    // Idle: a breath and a weight shift, so a standing character is not a
    // statue. Both fade out as the gait fades in.
    let breath = (tau * time / BREATH_PERIOD).sin();
    let idle = 1.0 - moving;
    angles.pelvis_rise = idle * breath * BREATH_AMPLITUDE;
    angles.pelvis_sway = idle * IDLE_WEIGHT_SHIFT;
    angles.pelvis_roll = idle * 0.035;
    angles.chest_pitch = idle * breath * 0.035;
    angles.head_pitch = idle * breath * -0.02;
    angles.shoulder_roll = [idle * 0.05, idle * 0.05];
    angles.elbow_flex = [idle * 0.12, idle * 0.12];

    if moving <= 0.0 {
        return angles.clamped();
    }

    // The two legs are half a cycle apart; the arms oppose the legs on the
    // same side, which is the rule that separates walking from flailing.
    for side in [LEFT, RIGHT] {
        let offset = if side == RIGHT { 0.0 } else { 0.5 };
        let t = (phase + offset).rem_euclid(1.0);
        let swing_u = if t >= profile.duty_factor {
            (t - profile.duty_factor) / (1.0 - profile.duty_factor).max(1.0e-4)
        } else {
            0.0
        };
        let stance_u = if t < profile.duty_factor {
            t / profile.duty_factor.max(1.0e-4)
        } else {
            0.0
        };

        let hip = -profile.hip_swing * (tau * t).cos();
        let knee = profile.knee_stance * bell(stance_u) + profile.knee_swing * bell(swing_u);
        // Keeping the sole parallel to the ground is what an ankle does; the
        // terrain-contact stage refines it, and the limit keeps it plausible.
        let ankle = -(hip - knee) * 0.35;

        angles.hip_pitch[side] = moving * hip;
        angles.knee_flex[side] = moving * knee;
        angles.ankle_pitch[side] = moving * ankle;

        // The arm on this side opposes this side's leg.
        let arm = profile.arm_swing * (tau * t).cos();
        angles.shoulder_pitch[side] = moving * arm;
        angles.shoulder_roll[side] = (1.0 - moving).mul_add(0.05, moving * 0.06);
        let forward = arm.max(0.0) / profile.arm_swing.max(1.0e-4);
        angles.elbow_flex[side] = moving * profile.elbow_flex * (0.35 + 0.65 * forward);
        angles.wrist_pitch[side] = moving * arm * 0.15;
    }

    let bob = profile.pelvis_bob * gait.leg_length_units / CHARACTER_VOXEL_SIZE;
    let sway = profile.pelvis_sway * gait.hip_width_units / CHARACTER_VOXEL_SIZE;
    angles.pelvis_rise += moving * (-0.5 * bob * (2.0 * tau * phase).cos());
    angles.pelvis_sway = angles.pelvis_sway * (1.0 - moving) + moving * sway * (tau * phase).sin();
    angles.pelvis_yaw = moving * profile.pelvis_yaw * (tau * phase).sin();
    angles.spine_yaw = -angles.pelvis_yaw * profile.chest_counter * 0.4;
    angles.chest_yaw = -angles.pelvis_yaw * profile.chest_counter * 0.6;
    angles.spine_pitch += moving * profile.lean * 0.4;
    angles.chest_pitch += moving * profile.lean * 0.6;
    angles.head_pitch += moving * -profile.lean * 0.5;

    angles.clamped()
}

#[cfg(test)]
mod tests {
    use super::{
        BREATH_PERIOD, GaitBlend, GaitParameters, GaitProfile, IDLE_SPEED, JOINT_LIMIT_DEGREES,
        LEFT, RIGHT, RUN_SPEED, WALK_SPEED, animate, stance_weight,
    };
    use crate::descriptor::CharacterDescriptor;

    fn gait() -> GaitParameters {
        let validated = match CharacterDescriptor::golden().validate() {
            Ok(validated) => validated,
            Err(error) => panic!("{error}"),
        };
        GaitParameters::derive(validated.body())
    }

    #[test]
    fn the_profiles_sit_inside_the_style_contract_bands() {
        let walk = GaitProfile::WALK;
        let run = GaitProfile::RUN;
        assert!((0.55..=0.70).contains(&walk.duty_factor));
        assert!((0.30..=0.50).contains(&run.duty_factor));
        for profile in [walk, run] {
            assert!((0.55..=1.30).contains(&profile.stride_per_leg_length));
            assert!((0.02..=0.08).contains(&profile.pelvis_bob));
            assert!((0.00..=0.06).contains(&profile.pelvis_sway));
            assert!((0.4..=1.2).contains(&profile.chest_counter));
        }
        assert!((2.0_f32..=12.0).contains(&run.lean.to_degrees()));
        assert!((2.0_f32..=12.0).contains(&walk.lean.to_degrees()));
    }

    #[test]
    fn the_blend_is_monotone_and_continuous_in_speed() {
        let gait = gait();
        let mut previous = gait.blend(0.0);
        assert_eq!(previous.moving, 0.0);
        let mut speed = 0.0_f32;
        while speed <= 6.0 {
            let blend = gait.blend(speed);
            assert!(blend.moving >= previous.moving - 1.0e-6, "moving went back");
            assert!(blend.run >= previous.run - 1.0e-6, "run went back");
            assert!(
                (blend.moving - previous.moving).abs() < 0.2,
                "moving jumped"
            );
            assert!((blend.run - previous.run).abs() < 0.2, "run jumped");
            previous = blend;
            speed += 0.02;
        }
        assert!(gait.blend(IDLE_SPEED).moving < 1.0e-6);
        assert!((gait.blend(WALK_SPEED).moving - 1.0).abs() < 1.0e-6);
        assert!((gait.blend(RUN_SPEED).run - 1.0).abs() < 1.0e-6);
        assert!(gait.blend(f32::NAN).moving.is_finite());
    }

    #[test]
    fn stride_times_cadence_is_the_speed_it_was_asked_about() {
        let gait = gait();
        for speed in [0.4_f32, 0.9, 1.6, 2.4, 3.2, 4.4] {
            let blend = gait.blend(speed);
            let stride = gait.stride(blend);
            let cadence = gait.cadence(speed);
            assert!(
                (stride * cadence - speed).abs() < 1.0e-4,
                "speed {speed} does not decompose into stride {stride} and cadence {cadence}"
            );
        }
    }

    #[test]
    fn phase_advances_with_distance_and_wraps() {
        let gait = gait();
        let blend = gait.blend(1.4);
        let cycle = 2.0 * gait.stride(blend);
        let start = 0.25;
        let after = gait.advance_phase(start, cycle, blend);
        assert!(
            (after - start).abs() < 1.0e-4,
            "a whole cycle of travel did not return the same phase"
        );
        let half = gait.advance_phase(0.0, cycle * 0.5, blend);
        assert!((half - 0.5).abs() < 1.0e-4);
        assert!(gait.advance_phase(0.9, cycle * 0.5, blend) < 0.5);
        assert!(gait.advance_phase(0.5, f32::NAN, blend).is_finite());
    }

    #[test]
    fn every_pose_is_finite_and_inside_the_joint_limits() {
        let gait = gait();
        for speed in [0.0_f32, 0.3, 0.8, 1.5, 2.6, 3.8, 9.0] {
            let blend = gait.blend(speed);
            let mut phase = 0.0_f32;
            while phase < 1.0 {
                let angles = animate(&gait, blend, phase, phase * 7.3);
                assert!(angles.is_finite(), "speed {speed} phase {phase}");
                assert!(
                    angles.within_limits(),
                    "speed {speed} phase {phase} leaves a joint range: {angles:?}"
                );
                phase += 1.0 / 128.0;
            }
        }
        // A hostile phase or time must not produce a hostile pose.
        let blend = gait.blend(1.5);
        assert!(animate(&gait, blend, f32::NAN, 0.0).is_finite());
        assert!(animate(&gait, blend, 0.3, f32::INFINITY).is_finite());
    }

    #[test]
    fn the_nominal_gaits_never_need_the_clamp() {
        let gait = gait();
        for speed in [0.8_f32, 1.6, 2.6, 3.8] {
            let blend = gait.blend(speed);
            let mut phase = 0.0_f32;
            while phase < 1.0 {
                let angles = animate(&gait, blend, phase, 0.0);
                assert_eq!(
                    angles,
                    angles.clamped(),
                    "speed {speed} phase {phase} was clamped"
                );
                phase += 1.0 / 64.0;
            }
        }
        assert_eq!(
            JOINT_LIMIT_DEGREES.knee_flex.0, 0.0,
            "the knee may not hyperextend"
        );
        assert_eq!(
            JOINT_LIMIT_DEGREES.elbow_flex.0, 0.0,
            "the elbow may not hyperextend"
        );
    }

    #[test]
    fn the_cycle_closes_without_a_hitch() {
        let gait = gait();
        let blend = gait.blend(1.6);
        let end = animate(&gait, blend, 1.0 - 1.0e-4, 0.0);
        let start = animate(&gait, blend, 0.0, 0.0);
        let difference = (end.hip_pitch[RIGHT] - start.hip_pitch[RIGHT]).abs()
            + (end.knee_flex[RIGHT] - start.knee_flex[RIGHT]).abs()
            + (end.shoulder_pitch[RIGHT] - start.shoulder_pitch[RIGHT]).abs()
            + (end.pelvis_rise - start.pelvis_rise).abs();
        assert!(
            difference < 1.0e-3,
            "the wrap leaves a step of {difference}"
        );

        // And the first derivative stays bounded across the wrap, which is
        // what a hitch would show up as.
        let step = 1.0 / 512.0;
        let mut worst = 0.0_f32;
        let mut phase = 0.0_f32;
        while phase < 1.0 {
            let a = animate(&gait, blend, phase, 0.0);
            let b = animate(&gait, blend, phase + step, 0.0);
            worst = worst.max((b.knee_flex[RIGHT] - a.knee_flex[RIGHT]).abs() / step);
            worst = worst.max((b.hip_pitch[RIGHT] - a.hip_pitch[RIGHT]).abs() / step);
            phase += step;
        }
        assert!(worst < 12.0, "angular rate reaches {worst} rad per cycle");
    }

    #[test]
    fn the_two_sides_are_half_a_cycle_apart() {
        let gait = gait();
        let blend = gait.blend(1.6);
        let mut phase = 0.0_f32;
        while phase < 1.0 {
            let here = animate(&gait, blend, phase, 0.0);
            let there = animate(&gait, blend, phase + 0.5, 0.0);
            assert!(
                (here.hip_pitch[RIGHT] - there.hip_pitch[LEFT]).abs() < 1.0e-4,
                "the legs are not opposed at phase {phase}"
            );
            assert!(
                (here.knee_flex[RIGHT] - there.knee_flex[LEFT]).abs() < 1.0e-4,
                "the knees are not opposed at phase {phase}"
            );
            phase += 1.0 / 32.0;
        }
    }

    #[test]
    fn an_arm_always_opposes_the_leg_on_its_own_side() {
        let gait = gait();
        let blend = gait.blend(2.0);
        let mut phase = 0.0_f32;
        let mut checked = 0;
        while phase < 1.0 {
            let angles = animate(&gait, blend, phase, 0.0);
            for side in [LEFT, RIGHT] {
                let hip = angles.hip_pitch[side];
                let shoulder = angles.shoulder_pitch[side];
                if hip.abs() > 0.05 {
                    assert!(
                        hip * shoulder <= 0.0,
                        "side {side} swings its arm and leg together at phase {phase}"
                    );
                    checked += 1;
                }
            }
            phase += 1.0 / 64.0;
        }
        assert!(checked > 32, "the test never reached a meaningful swing");
    }

    #[test]
    fn stance_weight_is_one_in_mid_stance_and_zero_in_swing() {
        let duty = GaitProfile::WALK.duty_factor;
        assert!((stance_weight(duty * 0.5, duty) - 1.0).abs() < 1.0e-4);
        assert_eq!(stance_weight(duty + 0.01, duty), 0.0);
        assert_eq!(stance_weight(0.999, duty), 0.0);
        assert!(stance_weight(0.0, duty) < 0.05, "contact is not eased in");
        assert_eq!(stance_weight(0.3, 0.0), 0.0);
    }

    #[test]
    fn idle_breathes_without_drifting() {
        let gait = gait();
        let blend = GaitBlend {
            moving: 0.0,
            run: 0.0,
        };
        let a = animate(&gait, blend, 0.0, 0.0);
        let b = animate(&gait, blend, 0.0, BREATH_PERIOD * 0.25);
        let c = animate(&gait, blend, 0.0, BREATH_PERIOD);
        assert!(
            (a.pelvis_rise - c.pelvis_rise).abs() < 1.0e-4,
            "the breath does not close"
        );
        assert!(
            (b.pelvis_rise - a.pelvis_rise).abs() > 0.05,
            "the idle pose is a statue"
        );
        assert!(a.pelvis_sway.abs() > 0.05, "idle has no weight shift");
    }
}
