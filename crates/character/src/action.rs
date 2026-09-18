//! The action pose layer: what a body does that is not locomotion.
//!
//! Locomotion is driven by **distance travelled**, because stride is the thing
//! that is specified and a foot that slides is then a bug rather than a
//! setting. An action is the other category: a swing takes the same time
//! whether the attacker is standing still or running, so its progress comes
//! from a caller's tick count and nothing here knows how far anybody walked.
//! ADR-0006 records why those two categories are deliberately separate and why
//! this is a small keyed layer rather than an animation system.
//!
//! There is no graph, no clip, no sampler, no transition table and no additive
//! stack. There are four named actions, each a handful of keys interpolated by
//! progress:
//!
//! | action | what it is for |
//! | --- | --- |
//! | [`ActionKind::Carry`] | holding a weapon at all, which is a pose and not an absence of one |
//! | [`ActionKind::Attack`] | anticipation, the swing, and the follow through |
//! | [`ActionKind::Dodge`] | a committed crouching step, with no roll |
//! | [`ActionKind::Stagger`] | the recoil that makes a hit read as a hit |
//!
//! `Carry` exists because of arithmetic rather than taste. A rigid weapon in a
//! hand driven only by the gait's arm swing hangs straight down with the arm,
//! and a blade long enough to reach ends up below the ground the character is
//! standing on. A weapon holder carries the weapon, so the weapon arm is
//! action driven whenever a weapon is held and only the free arm swings with
//! the gait.
//!
//! Every angle this module produces is passed through the same
//! [`JointAngles::clamped`] table locomotion uses. The clamp is a safety net
//! for hostile input, not part of the animation: a test asserts that no
//! nominal action needs it.

use crate::locomotion::{JointAngles, LEFT, RIGHT};
use crate::skeleton::Side;

/// Which action a body is performing.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ActionKind {
    /// Holding the weapon, ready, with the gait still driving the free arm.
    Carry,
    /// One swing: anticipation, active, follow through.
    Attack,
    /// A committed crouching step out of the way.
    Dodge,
    /// Recoil from taking a hit.
    Stagger,
    /// Beaten: the body sags and the sword goes down.
    ///
    /// Not a death animation and not a ragdoll — rigid parts and ADR-0004 rule
    /// both out. It is the smallest thing that makes a defeat legible, and a
    /// capture is why it exists at all: a defeated body used to hold the carry
    /// pose, so the frame showed it standing there with its sword out, looking
    /// exactly like a body about to swing.
    Defeated,
}

impl ActionKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Carry => "carry",
            Self::Attack => "attack",
            Self::Dodge => "dodge",
            Self::Stagger => "stagger",
            Self::Defeated => "defeated",
        }
    }
}

/// One arm's angles, in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArmAngles {
    pub shoulder_pitch: f32,
    pub shoulder_roll: f32,
    pub elbow_flex: f32,
    pub wrist_pitch: f32,
}

impl ArmAngles {
    const fn new(pitch: f32, roll: f32, elbow: f32, wrist: f32) -> Self {
        Self {
            shoulder_pitch: pitch,
            shoulder_roll: roll,
            elbow_flex: elbow,
            wrist_pitch: wrist,
        }
    }

    fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            shoulder_pitch: mix(self.shoulder_pitch, other.shoulder_pitch),
            shoulder_roll: mix(self.shoulder_roll, other.shoulder_roll),
            elbow_flex: mix(self.elbow_flex, other.elbow_flex),
            wrist_pitch: mix(self.wrist_pitch, other.wrist_pitch),
        }
    }

    /// Total rotation of the hand about its local `X` axis.
    ///
    /// All three arm joints rotate about the same axis, so the direction a
    /// rigid weapon points is decided by this sum plus the grip's own pitch.
    /// That is why the keys below are chosen by what this sum does rather than
    /// by what each joint looks like alone.
    #[must_use]
    pub fn hand_pitch(self) -> f32 {
        self.shoulder_pitch + self.elbow_flex + self.wrist_pitch
    }
}

/// The pose one action produces at one progress.
///
/// The weapon arm is **absolute**: it replaces whatever the gait was doing,
/// because a carried weapon is not swung like an empty hand. The free arm is
/// absolute too but carries its own weight, so `Carry` can leave the gait's
/// counter-swing alone and an attack can hand it back without a pop. Torso,
/// head and pelvis terms are **additive**, so a body can lean into a swing
/// while still walking.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActionPose {
    pub weapon_arm: ArmAngles,
    pub free_arm: ArmAngles,
    /// How much of `free_arm` replaces the gait's own swing, in `[0, 1]`.
    pub free_arm_weight: f32,
    pub spine_yaw: f32,
    pub chest_yaw: f32,
    pub spine_pitch: f32,
    pub chest_pitch: f32,
    pub head_pitch: f32,
    /// Additive pelvis height, in character voxels. Negative is a crouch.
    pub pelvis_rise: f32,
    /// Additive pelvis lateral offset, in character voxels.
    pub pelvis_sway: f32,
    pub pelvis_roll: f32,
}

/// What a caller asks for: an action, how far through it is, and how much of it
/// applies.
///
/// Progress is clamped on construction, so a caller that divides by a bad tick
/// count cannot produce a pose outside the keys. An attack's phase fractions
/// come from the authoritative tick counts, so the keys land exactly on the
/// phase boundaries the rules use and the blade cannot jump when a phase
/// changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionOverlay {
    kind: ActionKind,
    progress: f32,
    weight: f32,
    weapon_side: Side,
    windup_fraction: f32,
    active_fraction: f32,
    direction: f32,
}

impl ActionOverlay {
    fn base(kind: ActionKind, progress: f32, weapon_side: Side) -> Self {
        Self {
            kind,
            progress: clamp01(progress),
            weight: 1.0,
            weapon_side,
            windup_fraction: 0.0,
            active_fraction: 0.0,
            direction: 0.0,
        }
    }

    /// Holding the weapon.
    #[must_use]
    pub fn carry(weapon_side: Side) -> Self {
        Self::base(ActionKind::Carry, 0.0, weapon_side)
    }

    /// One swing. `windup_fraction` and `active_fraction` are that swing's
    /// phase boundaries as fractions of the whole action.
    #[must_use]
    pub fn attack(
        weapon_side: Side,
        progress: f32,
        windup_fraction: f32,
        active_fraction: f32,
    ) -> Self {
        let windup = clamp01(windup_fraction);
        let active = clamp01(active_fraction).min(1.0 - windup);
        Self {
            windup_fraction: windup,
            active_fraction: active,
            ..Self::base(ActionKind::Attack, progress, weapon_side)
        }
    }

    /// A dodge. `direction` is the travel direction relative to the body's own
    /// facing, in radians; zero is straight ahead.
    #[must_use]
    pub fn dodge(weapon_side: Side, progress: f32, direction: f32) -> Self {
        Self {
            direction: finite_or_zero(direction),
            ..Self::base(ActionKind::Dodge, progress, weapon_side)
        }
    }

    /// A collapse. `progress` runs over the defeat hold, so the body sags into
    /// the pose rather than snapping into it, and then stays there.
    #[must_use]
    pub fn defeated(weapon_side: Side, progress: f32) -> Self {
        Self::base(ActionKind::Defeated, progress, weapon_side)
    }

    /// A recoil. `direction` is the direction the hit came from relative to the
    /// body's own facing, in radians.
    #[must_use]
    pub fn stagger(weapon_side: Side, progress: f32, direction: f32) -> Self {
        Self {
            direction: finite_or_zero(direction),
            ..Self::base(ActionKind::Stagger, progress, weapon_side)
        }
    }

    /// Scales how much of the action applies.
    #[must_use]
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = clamp01(weight);
        self
    }

    #[must_use]
    pub const fn kind(&self) -> ActionKind {
        self.kind
    }

    #[must_use]
    pub const fn progress(&self) -> f32 {
        self.progress
    }

    #[must_use]
    pub const fn weight(&self) -> f32 {
        self.weight
    }

    #[must_use]
    pub const fn weapon_side(&self) -> Side {
        self.weapon_side
    }

    /// The pose this overlay describes, before any clamping or blending.
    #[must_use]
    pub fn pose(&self) -> ActionPose {
        match self.kind {
            ActionKind::Carry => carry_pose(),
            ActionKind::Attack => {
                attack_pose(self.progress, self.windup_fraction, self.active_fraction)
            }
            ActionKind::Dodge => dodge_pose(self.progress, self.direction),
            ActionKind::Stagger => stagger_pose(self.progress, self.direction),
            ActionKind::Defeated => defeated_pose(self.progress),
        }
    }
}

fn clamp01(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Smooth ease with zero slope at both ends, so two adjacent keys join without
/// a visible corner.
fn ease(t: f32) -> f32 {
    let t = clamp01(t);
    t * t * (3.0 - 2.0 * t)
}

fn segment(value: f32, low: f32, high: f32) -> f32 {
    if high <= low {
        return 1.0;
    }
    ((value - low) / (high - low)).clamp(0.0, 1.0)
}

/// The carry: sword forward and a little up, elbow softly bent.
///
/// `hand_pitch` here is `0.90` radians. With the grip's own pitch the blade
/// sits above the hand and well forward of the body, which is what keeps a
/// blade long enough to reach clear of the ground at rest.
const CARRY_ARM: ArmAngles = ArmAngles::new(0.20, 0.10, 0.70, 0.00);

/// The windup: shoulder back and the elbow strongly closed, which raises the
/// blade over the weapon shoulder. `hand_pitch` `1.95`.
const WINDUP_ARM: ArmAngles = ArmAngles::new(-0.25, 0.45, 2.20, 0.00);

/// The end of the swing: arm forward, elbow open, wrist leading the tip down.
/// `hand_pitch` `0.20`, so the blade sweeps about `1.75` radians through the
/// space in front of the body and passes horizontal on the way.
const STRIKE_ARM: ArmAngles = ArmAngles::new(0.50, 0.10, 0.00, -0.30);

/// Peak torso twist away from the target during the windup, in radians.
///
/// Positive `chest_yaw` turns the weapon shoulder forward, so the windup is
/// negative and the strike is positive. Both stay inside the style contract's
/// `±18°` spine range with room for the gait's own counter-rotation on top.
const WINDUP_TWIST: f32 = 0.16;
/// Torso twist into the target at the end of the swing.
const STRIKE_TWIST: f32 = 0.16;

fn carry_pose() -> ActionPose {
    ActionPose {
        weapon_arm: CARRY_ARM,
        free_arm: ArmAngles::default(),
        free_arm_weight: 0.0,
        ..ActionPose::default()
    }
}

fn attack_pose(progress: f32, windup_fraction: f32, active_fraction: f32) -> ActionPose {
    let windup_end = windup_fraction;
    let active_end = windup_fraction + active_fraction;
    // Three segments interpolating between shared keys, so the whole action is
    // continuous by construction rather than by tolerance.
    let (arm, twist, pitch) = if progress <= windup_end {
        let t = ease(segment(progress, 0.0, windup_end));
        (CARRY_ARM.lerp(WINDUP_ARM, t), -WINDUP_TWIST * t, -0.06 * t)
    } else if progress <= active_end {
        // The active phase is deliberately not eased: a swing that decelerates
        // into the target reads as a push rather than a cut.
        let t = segment(progress, windup_end, active_end);
        (
            WINDUP_ARM.lerp(STRIKE_ARM, t),
            (STRIKE_TWIST + WINDUP_TWIST).mul_add(t, -WINDUP_TWIST),
            0.20_f32.mul_add(t, -0.06),
        )
    } else {
        let t = ease(segment(progress, active_end, 1.0));
        (
            STRIKE_ARM.lerp(CARRY_ARM, t),
            STRIKE_TWIST * (1.0 - t),
            0.14 * (1.0 - t),
        )
    };
    // The free arm fades in over the windup and back out over the recovery, so
    // neither end of the action pops the gait's own swing.
    let free_weight = if progress <= active_end {
        ease(segment(progress, 0.0, windup_end.max(1.0e-4)))
    } else {
        1.0 - ease(segment(progress, active_end, 1.0))
    };
    let swing = twist / WINDUP_TWIST.max(1.0e-4);
    ActionPose {
        weapon_arm: arm,
        free_arm: ArmAngles::new(
            -0.55 * swing,
            0.16,
            0.30_f32.mul_add(1.0 - swing.abs().min(1.0), 0.35),
            0.0,
        ),
        free_arm_weight: free_weight,
        spine_yaw: twist * 0.6,
        chest_yaw: twist,
        spine_pitch: pitch * 0.4,
        chest_pitch: pitch * 0.6,
        head_pitch: -pitch * 0.3,
        pelvis_rise: 0.0,
        pelvis_sway: 0.0,
        pelvis_roll: twist * 0.25,
    }
}

/// A dodge is a crouch and a lean, held through the middle and released at the
/// end. There is no roll: rolling a rigid voxel body about its own centre is a
/// different decision and is not in this milestone.
fn dodge_pose(progress: f32, direction: f32) -> ActionPose {
    let amount = if progress < 0.2 {
        ease(progress / 0.2)
    } else if progress < 0.66 {
        1.0
    } else {
        1.0 - ease(segment(progress, 0.66, 1.0))
    };
    let forward = direction.cos();
    let lateral = direction.sin();
    ActionPose {
        weapon_arm: ArmAngles::new(0.35, 0.30, 1.10, 0.0).lerp(CARRY_ARM, 1.0 - amount),
        free_arm: ArmAngles::new(0.45, 0.28, 1.20, 0.0),
        free_arm_weight: amount,
        spine_yaw: 0.0,
        chest_yaw: 0.0,
        spine_pitch: amount * 0.10 * forward,
        chest_pitch: amount * 0.14 * forward,
        head_pitch: amount * -0.10,
        pelvis_rise: amount * -1.4,
        pelvis_sway: amount * 0.8 * lateral,
        pelvis_roll: amount * -0.14 * lateral,
    }
}

/// A recoil: everything goes back and down, fast, then returns.
///
/// The torso can only pitch back `12°` by the style contract, so most of the
/// reading comes from the arms, the head and a pelvis drop the joint table does
/// not constrain. That is a deliberate use of the budget available rather than
/// a request to widen the table.
fn stagger_pose(progress: f32, direction: f32) -> ActionPose {
    let amount = if progress < 0.25 {
        ease(progress / 0.25)
    } else {
        1.0 - ease(segment(progress, 0.25, 1.0))
    };
    let forward = direction.cos();
    let lateral = direction.sin();
    // Mostly backward whatever the direction, more so when hit from the front.
    let recoil = 0.6_f32.mul_add(forward.max(0.0), 0.4);
    ActionPose {
        weapon_arm: ArmAngles::new(-0.55, 0.35, 1.30, 0.20).lerp(CARRY_ARM, 1.0 - amount),
        free_arm: ArmAngles::new(-0.65, 0.30, 1.10, 0.0),
        free_arm_weight: amount,
        spine_yaw: amount * 0.08 * lateral,
        chest_yaw: amount * 0.10 * lateral,
        spine_pitch: amount * -0.16 * recoil,
        chest_pitch: amount * -0.18 * recoil,
        head_pitch: amount * -0.22,
        pelvis_rise: -amount,
        pelvis_sway: amount * -0.6 * lateral,
        pelvis_roll: amount * 0.10 * lateral,
    }
}

/// Beaten: knees give, torso folds forward, both arms hang, sword points down.
///
/// The sag is fast and then holds: a quarter of the defeat hold to get there,
/// and the rest of it motionless, because a body that keeps sinking for two and
/// a half seconds reads as a bug rather than as a defeat. `pelvis_rise` at `-1`
/// is the same full drop a dodge uses, which is as far down as the rigid legs go
/// without the feet leaving the ground.
fn defeated_pose(progress: f32) -> ActionPose {
    let amount = ease(segment(progress, 0.0, 0.25));
    ActionPose {
        // Arm down and across, so the blade ends up pointing at the ground
        // instead of at the opponent. The shoulder goes forward rather than back
        // because a dropped arm hangs in front of a folded torso.
        weapon_arm: ArmAngles::new(0.45, 0.12, -0.55, -0.35).lerp(CARRY_ARM, 1.0 - amount),
        free_arm: ArmAngles::new(0.30, 0.10, -0.30, 0.0),
        free_arm_weight: amount,
        spine_yaw: 0.0,
        chest_yaw: 0.0,
        spine_pitch: amount * 0.34,
        chest_pitch: amount * 0.30,
        head_pitch: amount * 0.40,
        pelvis_rise: -amount,
        pelvis_sway: 0.0,
        pelvis_roll: 0.0,
    }
}

/// Composes an action over a set of locomotion angles.
///
/// The weapon arm is replaced, the free arm is replaced by as much as the
/// action asks for, and everything else is added. The result is clamped, which
/// is the same safety net locomotion has and is asserted never to be needed by
/// a nominal action.
#[must_use]
pub fn apply(mut angles: JointAngles, overlay: &ActionOverlay) -> JointAngles {
    let weight = overlay.weight();
    if weight <= 0.0 {
        return angles;
    }
    let pose = overlay.pose();
    let (weapon, free) = match overlay.weapon_side() {
        Side::Left => (LEFT, RIGHT),
        Side::Right | Side::Centre => (RIGHT, LEFT),
    };
    let blend = |from: f32, to: f32, amount: f32| (to - from).mul_add(amount, from);

    angles.shoulder_pitch[weapon] = blend(
        angles.shoulder_pitch[weapon],
        pose.weapon_arm.shoulder_pitch,
        weight,
    );
    angles.shoulder_roll[weapon] = blend(
        angles.shoulder_roll[weapon],
        pose.weapon_arm.shoulder_roll,
        weight,
    );
    angles.elbow_flex[weapon] = blend(
        angles.elbow_flex[weapon],
        pose.weapon_arm.elbow_flex,
        weight,
    );
    angles.wrist_pitch[weapon] = blend(
        angles.wrist_pitch[weapon],
        pose.weapon_arm.wrist_pitch,
        weight,
    );

    let free_weight = weight * pose.free_arm_weight.clamp(0.0, 1.0);
    if free_weight > 0.0 {
        angles.shoulder_pitch[free] = blend(
            angles.shoulder_pitch[free],
            pose.free_arm.shoulder_pitch,
            free_weight,
        );
        angles.shoulder_roll[free] = blend(
            angles.shoulder_roll[free],
            pose.free_arm.shoulder_roll,
            free_weight,
        );
        angles.elbow_flex[free] = blend(
            angles.elbow_flex[free],
            pose.free_arm.elbow_flex,
            free_weight,
        );
        angles.wrist_pitch[free] = blend(
            angles.wrist_pitch[free],
            pose.free_arm.wrist_pitch,
            free_weight,
        );
    }

    angles.spine_yaw += pose.spine_yaw * weight;
    angles.chest_yaw += pose.chest_yaw * weight;
    angles.spine_pitch += pose.spine_pitch * weight;
    angles.chest_pitch += pose.chest_pitch * weight;
    angles.head_pitch += pose.head_pitch * weight;
    angles.pelvis_rise += pose.pelvis_rise * weight;
    angles.pelvis_sway += pose.pelvis_sway * weight;
    angles.pelvis_roll += pose.pelvis_roll * weight;

    angles.clamped()
}

#[cfg(test)]
mod tests {
    use super::{
        ActionKind, ActionOverlay, CARRY_ARM, STRIKE_ARM, WINDUP_ARM, apply, dodge_pose,
        stagger_pose,
    };
    use crate::descriptor::CharacterDescriptor;
    use crate::locomotion::{
        GaitBlend, GaitParameters, JOINT_LIMIT_DEGREES, JointAngles, LEFT, RIGHT, animate,
        within_degrees,
    };
    use crate::skeleton::Side;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn gait() -> GaitParameters {
        let validated = match CharacterDescriptor::golden().validate() {
            Ok(validated) => validated,
            Err(error) => panic!("the golden descriptor must validate: {error}"),
        };
        GaitParameters::derive(validated.body())
    }

    fn nominal_overlays() -> Vec<ActionOverlay> {
        let mut overlays = vec![ActionOverlay::carry(Side::Right)];
        for step in 0..=40 {
            let progress = step as f32 / 40.0;
            overlays.push(ActionOverlay::attack(Side::Right, progress, 0.29, 0.16));
            for direction in [0.0, FRAC_PI_2, PI, -FRAC_PI_2, 0.8] {
                overlays.push(ActionOverlay::dodge(Side::Right, progress, direction));
                overlays.push(ActionOverlay::stagger(Side::Right, progress, direction));
            }
        }
        overlays
    }

    #[test]
    fn no_nominal_action_needs_the_joint_clamp() {
        // The clamp is a safety net for hostile input. An action that depends
        // on it to look valid is a curve that is wrong, so the raw arm angles
        // are checked against the style contract before any clamping.
        let limits = JOINT_LIMIT_DEGREES;
        for overlay in nominal_overlays() {
            let pose = overlay.pose();
            for arm in [pose.weapon_arm, pose.free_arm] {
                assert!(
                    within_degrees(arm.shoulder_pitch, limits.shoulder_pitch),
                    "{} at {} left shoulder_pitch {} outside the contract",
                    overlay.kind().name(),
                    overlay.progress(),
                    arm.shoulder_pitch
                );
                assert!(within_degrees(arm.shoulder_roll, limits.shoulder_roll));
                assert!(within_degrees(arm.elbow_flex, limits.elbow_flex));
                assert!(within_degrees(arm.wrist_pitch, limits.wrist_pitch));
            }
        }
    }

    #[test]
    fn an_action_over_a_gait_stays_inside_every_joint_range() {
        // The torso terms are additive, so the sum with a running gait is what
        // has to fit, not the action alone.
        let gait = gait();
        let overlays = nominal_overlays();
        for run in [0.0_f32, 0.5, 1.0] {
            let blend = GaitBlend { moving: 1.0, run };
            for phase_step in 0..12 {
                let angles = animate(&gait, blend, phase_step as f32 / 12.0, 3.0);
                for overlay in &overlays {
                    let composed = apply(angles, overlay);
                    assert!(composed.is_finite());
                    assert!(
                        composed.within_limits(),
                        "{} at {} left a joint outside its range",
                        overlay.kind().name(),
                        overlay.progress()
                    );
                }
            }
        }
    }

    #[test]
    fn the_attack_is_continuous_across_its_phase_boundaries() {
        // A blade that jumps at a phase change would let the sweep detect a hit
        // in geometry the animation never showed.
        let windup = 0.29;
        let active = 0.16;
        let samples = 2_000;
        let mut previous = ActionOverlay::attack(Side::Right, 0.0, windup, active)
            .pose()
            .weapon_arm;
        let mut worst = 0.0_f32;
        for step in 1..=samples {
            let progress = step as f32 / samples as f32;
            let arm = ActionOverlay::attack(Side::Right, progress, windup, active)
                .pose()
                .weapon_arm;
            worst = worst.max((arm.hand_pitch() - previous.hand_pitch()).abs());
            previous = arm;
        }
        // The whole swing sweeps about 1.75 radians; a discontinuity would be a
        // step far larger than one sample's share of it.
        let share = 1.9 / samples as f32;
        assert!(
            worst < share * 8.0,
            "largest hand-pitch step {worst} exceeds {}",
            share * 8.0
        );
    }

    #[test]
    fn the_attack_starts_and_ends_at_the_carry() {
        let windup = 0.29;
        let active = 0.16;
        let start = ActionOverlay::attack(Side::Right, 0.0, windup, active).pose();
        let end = ActionOverlay::attack(Side::Right, 1.0, windup, active).pose();
        for arm in [start.weapon_arm, end.weapon_arm] {
            // Interpolation to an endpoint is not bit exact, so the claim is
            // that the action returns the body to the carry, not that it
            // reproduces the constant's bit pattern.
            assert!((arm.shoulder_pitch - CARRY_ARM.shoulder_pitch).abs() < 1.0e-6);
            assert!((arm.shoulder_roll - CARRY_ARM.shoulder_roll).abs() < 1.0e-6);
            assert!((arm.elbow_flex - CARRY_ARM.elbow_flex).abs() < 1.0e-6);
            assert!((arm.wrist_pitch - CARRY_ARM.wrist_pitch).abs() < 1.0e-6);
        }
        assert!(start.free_arm_weight.abs() < 1.0e-6);
        assert!(end.free_arm_weight.abs() < 1.0e-6);
    }

    #[test]
    fn the_swing_sweeps_from_raised_to_low_through_horizontal() {
        let windup = 0.29;
        let active = 0.16;
        let raised = ActionOverlay::attack(Side::Right, windup, windup, active)
            .pose()
            .weapon_arm
            .hand_pitch();
        let low = ActionOverlay::attack(Side::Right, windup + active, windup, active)
            .pose()
            .weapon_arm
            .hand_pitch();
        let carried = CARRY_ARM.hand_pitch();
        assert!(
            raised > carried,
            "windup {raised} must raise past carry {carried}"
        );
        assert!(
            low < carried,
            "strike {low} must finish below carry {carried}"
        );
        assert!((raised - WINDUP_ARM.hand_pitch()).abs() < 1.0e-6);
        assert!((low - STRIKE_ARM.hand_pitch()).abs() < 1.0e-6);
        // Every intermediate value between the two extremes is visited, so the
        // blade passes through horizontal rather than jumping over it.
        let mut previous = raised;
        for step in 1..=200 {
            let progress = active.mul_add(step as f32 / 200.0, windup);
            let value = ActionOverlay::attack(Side::Right, progress, windup, active)
                .pose()
                .weapon_arm
                .hand_pitch();
            assert!(
                value <= previous + 1.0e-6,
                "the swing reversed at {progress}"
            );
            previous = value;
        }
    }

    #[test]
    fn carry_replaces_the_weapon_arm_and_leaves_the_free_arm_to_the_gait() {
        let mut angles = JointAngles::default();
        angles.shoulder_pitch[RIGHT] = -0.4;
        angles.shoulder_pitch[LEFT] = 0.4;
        let composed = apply(angles, &ActionOverlay::carry(Side::Right));
        assert!((composed.shoulder_pitch[RIGHT] - CARRY_ARM.shoulder_pitch).abs() < 1.0e-6);
        assert!((composed.shoulder_pitch[LEFT] - 0.4).abs() < 1.0e-6);
    }

    #[test]
    fn a_left_handed_overlay_drives_the_left_arm() {
        let composed = apply(JointAngles::default(), &ActionOverlay::carry(Side::Left));
        assert!((composed.shoulder_pitch[LEFT] - CARRY_ARM.shoulder_pitch).abs() < 1.0e-6);
        assert!(composed.shoulder_pitch[RIGHT].abs() < 1.0e-6);
    }

    #[test]
    fn a_zero_weight_overlay_changes_nothing() {
        let mut angles = JointAngles::default();
        angles.shoulder_pitch[RIGHT] = -0.4;
        let composed = apply(angles, &ActionOverlay::carry(Side::Right).with_weight(0.0));
        assert_eq!(composed, angles);
    }

    #[test]
    fn hostile_progress_and_direction_cannot_leave_the_keys() {
        for progress in [f32::NAN, f32::INFINITY, -1.0, 2.0] {
            let overlay = ActionOverlay::attack(Side::Right, progress, 0.3, 0.2);
            assert!((0.0..=1.0).contains(&overlay.progress()));
            assert!(overlay.pose().weapon_arm.hand_pitch().is_finite());
        }
        for direction in [f32::NAN, f32::INFINITY] {
            assert!(dodge_pose(0.5, direction).pelvis_rise.is_finite());
            assert!(
                ActionOverlay::dodge(Side::Right, 0.5, direction)
                    .pose()
                    .pelvis_sway
                    .is_finite()
            );
            assert!(
                ActionOverlay::stagger(Side::Right, 0.5, direction)
                    .pose()
                    .pelvis_rise
                    .is_finite()
            );
        }
    }

    #[test]
    fn a_dodge_crouches_in_the_middle_and_stands_at_both_ends() {
        assert_eq!(dodge_pose(0.0, 0.0).pelvis_rise, 0.0);
        assert!(dodge_pose(0.4, 0.0).pelvis_rise < -1.0);
        assert!(dodge_pose(1.0, 0.0).pelvis_rise.abs() < 1.0e-6);
    }

    #[test]
    fn a_stagger_peaks_early_and_returns() {
        assert!(stagger_pose(0.25, 0.0).head_pitch < -0.2);
        assert!(stagger_pose(0.0, 0.0).head_pitch.abs() < 1.0e-6);
        assert!(stagger_pose(1.0, 0.0).head_pitch.abs() < 1.0e-6);
    }

    #[test]
    fn an_attacks_phase_fractions_cannot_overflow_the_action() {
        for (windup, active, progress) in [(0.9, 0.9, 0.5), (0.0, 0.0, 1.0), (1.0, 0.5, 0.3)] {
            let overlay = ActionOverlay::attack(Side::Right, progress, windup, active);
            assert!(overlay.pose().weapon_arm.hand_pitch().is_finite());
        }
    }

    #[test]
    fn action_kinds_name_themselves() {
        for kind in [
            ActionKind::Carry,
            ActionKind::Attack,
            ActionKind::Dodge,
            ActionKind::Stagger,
        ] {
            assert!(!kind.name().is_empty());
        }
    }
}
