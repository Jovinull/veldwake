//! Runtime state and the pose it produces.
//!
//! This module is the line between **identity** and **state**. A
//! [`CompiledCharacter`] never changes; a [`CharacterState`] is where it is,
//! which way it faces, how fast it is going, and where in the gait cycle it is.
//! [`pose`] is a pure function of the two plus a ground sampler, so the same
//! inputs give the same pose in a test, in a probe, and in the client.
//!
//! Terrain contact is the reason the leg IK exists. The ground a viewer sees is
//! a staircase of voxel tops, so:
//!
//! 1. the **feet** are placed on the block they stand on, exactly;
//! 2. the **pelvis** follows a smoothed height, advanced in
//!    [`CharacterState::advance`] rather than filtered inside `pose`, so the
//!    pose stays a pure function of state;
//! 3. **two-bone leg IK** absorbs the difference between the two.
//!
//! That is the whole contact subset. There is no gravity, no collision
//! resolution, no character controller, and no physics dependency.

use glam::{Mat4, Quat, Vec3};

use crate::compiler::CompiledCharacter;
use crate::descriptor::CHARACTER_VOXEL_SIZE;
use crate::ground::GroundSampler;
use crate::ik::{clamp_rotation_angle, solve_two_bone};
use crate::locomotion::{
    GaitBlend, JOINT_LIMIT_DEGREES, JointAngles, LEFT, RIGHT, animate, foot_offset, stance_weight,
};
use crate::skeleton::{ALL_BONES, BONE_COUNT, BoneId, Side, Transform};

/// How quickly the pelvis follows a **rise** in ground height, in seconds.
///
/// The feet snap to the block they stand on; without this the pelvis would snap
/// with them and a terraced slope would read as the character hopping up a
/// staircase. Small enough that the body never lags visibly behind the feet.
pub const PELVIS_RISE_TAU: f32 = 0.12;

/// How quickly the pelvis follows a **fall** in ground height, in seconds.
///
/// Much faster, and the asymmetry is geometry rather than taste. At rest the
/// legs are straight, so the hip sits exactly one leg length above the sole and
/// there is no downward headroom at all: every centimetre the pelvis lags above
/// the ground is a centimetre a planted foot cannot reach. Lagging *below* the
/// ground costs nothing, because a leg folds.
pub const PELVIS_FALL_TAU: f32 = 0.035;

/// Shortest hip-to-ankle span the solver will ask for, as a fraction of the
/// leg's own length.
///
/// A ground query can name a target the leg cannot plausibly reach — a block
/// a whole world unit above a pelvis whose leg is two thirds of that. Without
/// a floor the chain folds toward its dead zone and the knee ends up over the
/// hip, which is a pose no amount of exaggeration excuses. Clamping the span
/// keeps the knee inside its declared range and reports the foot as not
/// reached, which is the honest answer: a character this size cannot step onto
/// a terrain terrace.
pub const MIN_LEG_SPAN_FRACTION: f32 = 0.45;

/// How far ahead and behind a foot the slope is measured, in world units.
///
/// A stall must not teleport a character, exactly as the camera controller
/// already clamps its own presentation delta.
pub const MAX_STEP_SECONDS: f32 = 0.10;

/// Everything about a character that changes over time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterState {
    /// World position on the ground plane.
    pub x: f32,
    pub z: f32,
    /// Yaw in radians; zero faces `-Z`, the same convention the camera uses.
    pub facing: f32,
    /// World units per second.
    pub speed: f32,
    /// Locomotion cycle position in `[0, 1)`.
    pub phase: f32,
    /// Seconds since the character started existing; drives the idle breath.
    pub time: f32,
    /// Smoothed world height the character stands on.
    pub base_height: f32,
    /// Whether the last ground query had an answer.
    pub grounded: bool,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            x: 0.0,
            z: 0.0,
            facing: 0.0,
            speed: 0.0,
            phase: 0.0,
            time: 0.0,
            base_height: 0.0,
            grounded: false,
        }
    }
}

impl CharacterState {
    /// A character standing still at a place, already settled on the ground.
    #[must_use]
    pub fn standing(x: f32, z: f32, facing: f32, ground: Option<&dyn GroundSampler>) -> Self {
        let mut state = Self {
            x,
            z,
            facing,
            ..Self::default()
        };
        if let Some(ground) = ground
            && let Some(height) = ground.surface(f64::from(x), f64::from(z))
        {
            state.base_height = height as f32;
            state.grounded = true;
        }
        state
    }

    /// The world point the character stands on.
    #[must_use]
    pub fn stand_point(&self) -> Vec3 {
        Vec3::new(self.x, self.base_height, self.z)
    }

    /// Advances phase, time, and the smoothed ground height.
    ///
    /// The pelvis filter lives here rather than in [`pose`] so that a pose is
    /// reproducible from a state alone.
    pub fn advance(
        &mut self,
        seconds: f32,
        character: &CompiledCharacter,
        ground: Option<&dyn GroundSampler>,
    ) {
        let step = if seconds.is_finite() {
            seconds.clamp(0.0, MAX_STEP_SECONDS)
        } else {
            0.0
        };
        let gait = character.gait();
        let blend = gait.blend(self.speed);
        let distance = self.speed.max(0.0) * step;
        self.phase = gait.advance_phase(self.phase, distance, blend);
        self.time += step;

        let Some(ground) = ground else {
            self.grounded = false;
            return;
        };
        match ground.surface(f64::from(self.x), f64::from(self.z)) {
            Some(height) => {
                let target = height as f32;
                if self.grounded {
                    let tau = if target < self.base_height {
                        PELVIS_FALL_TAU
                    } else {
                        PELVIS_RISE_TAU
                    };
                    let alpha = 1.0 - (-step / tau).exp();
                    self.base_height += (target - self.base_height) * alpha;
                } else {
                    self.base_height = target;
                }
                self.grounded = true;
            }
            None => self.grounded = false,
        }
    }

    /// Moves the character along its facing and advances everything else.
    pub fn walk_forward(
        &mut self,
        seconds: f32,
        character: &CompiledCharacter,
        ground: Option<&dyn GroundSampler>,
    ) {
        let step = if seconds.is_finite() {
            seconds.clamp(0.0, MAX_STEP_SECONDS)
        } else {
            0.0
        };
        let distance = self.speed.max(0.0) * step;
        let forward = facing_direction(self.facing);
        self.x += forward.x * distance;
        self.z += forward.z * distance;
        self.advance(seconds, character, ground);
    }
}

/// What one foot is doing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FootContact {
    /// Whether the ground answered under this foot.
    pub grounded: bool,
    /// How planted the foot is, from zero in swing to one in mid stance.
    pub stance: f32,
    /// Ground height under the foot, in world units.
    pub ground_height: f32,
    /// Lowest world height of the posed foot's box, in world units.
    pub sole_height: f32,
    /// Whether the IK reached its target without clamping.
    pub reached: bool,
}

impl FootContact {
    /// Signed distance from the sole to the ground, positive above it.
    #[must_use]
    pub fn clearance(&self) -> f32 {
        self.sole_height - self.ground_height
    }
}

/// A character placed in the world at one instant.
#[derive(Clone, Debug, PartialEq)]
pub struct PosedCharacter {
    world: Mat4,
    bone_world: [Transform; BONE_COUNT],
    part_matrices: [Mat4; BONE_COUNT],
    angles: JointAngles,
    blend: GaitBlend,
    contacts: [FootContact; 2],
}

impl PosedCharacter {
    /// The character's placement: world units, yaw, and the voxel scale.
    #[must_use]
    pub const fn world_matrix(&self) -> Mat4 {
        self.world
    }

    /// Bone transforms in character-local voxel space.
    #[must_use]
    pub const fn bone_world(&self) -> &[Transform; BONE_COUNT] {
        &self.bone_world
    }

    /// One world matrix per body part, in bone order.
    ///
    /// This is exactly what a renderer uploads; nothing further has to be
    /// composed on the presentation side.
    #[must_use]
    pub const fn part_matrices(&self) -> &[Mat4; BONE_COUNT] {
        &self.part_matrices
    }

    #[must_use]
    pub const fn angles(&self) -> &JointAngles {
        &self.angles
    }

    #[must_use]
    pub const fn blend(&self) -> GaitBlend {
        self.blend
    }

    /// Foot contact, indexed by [`LEFT`] and [`RIGHT`].
    #[must_use]
    pub const fn contacts(&self) -> &[FootContact; 2] {
        &self.contacts
    }

    /// Whether every transform is finite and rigid.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.bone_world.iter().all(|t| t.is_finite())
            && self
                .part_matrices
                .iter()
                .all(|m| m.to_cols_array().into_iter().all(f32::is_finite))
    }
}

/// The world rotation of a character facing `yaw`.
///
/// **Negated on purpose.** The client's camera reads a yaw as the direction
/// `(sin yaw, 0, -cos yaw)`, and a character has to face the same way at the
/// same number or nothing in the client agrees with anything else. A right
/// handed rotation about `+Y` turns `-Z` toward `-X`, which is the opposite
/// sense, so the sign is flipped here once rather than at every call site.
///
/// This was not a theory: the first captures showed the character walking
/// backwards along its own course.
#[must_use]
pub fn facing_rotation(yaw: f32) -> Quat {
    Quat::from_rotation_y(-yaw)
}

/// The direction a character faces, in world space.
#[must_use]
pub fn facing_direction(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin(), 0.0, -yaw.cos())
}

fn leg_bones(side: usize) -> (BoneId, BoneId, BoneId) {
    if side == LEFT {
        (BoneId::ThighL, BoneId::ShinL, BoneId::FootL)
    } else {
        (BoneId::ThighR, BoneId::ShinR, BoneId::FootR)
    }
}

/// Builds every bone's local transform from the rest pose and a set of angles.
fn locals_from_angles(
    character: &CompiledCharacter,
    angles: &JointAngles,
) -> [Transform; BONE_COUNT] {
    let skeleton = character.skeleton();
    let mut locals = skeleton.rest_locals();
    for bone in ALL_BONES {
        let rest = locals[bone.index()];
        let side = match bone.side() {
            Side::Left => LEFT,
            Side::Right => RIGHT,
            Side::Centre => LEFT,
        };
        let sign = match bone.side() {
            Side::Left => -1.0,
            Side::Right => 1.0,
            Side::Centre => 0.0,
        };
        let updated = match bone {
            BoneId::Root => Transform::new(
                Quat::from_rotation_y(angles.pelvis_yaw)
                    * Quat::from_rotation_z(angles.pelvis_roll),
                rest.translation + Vec3::new(angles.pelvis_sway, angles.pelvis_rise, 0.0),
            ),
            BoneId::Spine => Transform::new(
                Quat::from_rotation_y(angles.spine_yaw) * Quat::from_rotation_x(angles.spine_pitch),
                rest.translation,
            ),
            BoneId::Chest => Transform::new(
                Quat::from_rotation_y(angles.chest_yaw) * Quat::from_rotation_x(angles.chest_pitch),
                rest.translation,
            ),
            BoneId::Head => Transform::new(
                Quat::from_rotation_y(angles.head_yaw) * Quat::from_rotation_x(angles.head_pitch),
                rest.translation,
            ),
            BoneId::UpperArmL | BoneId::UpperArmR => Transform::new(
                Quat::from_rotation_z(sign * angles.shoulder_roll[side])
                    * Quat::from_rotation_x(angles.shoulder_pitch[side]),
                rest.translation,
            ),
            BoneId::ForearmL | BoneId::ForearmR => Transform::new(
                Quat::from_rotation_x(angles.elbow_flex[side]),
                rest.translation,
            ),
            BoneId::HandL | BoneId::HandR => Transform::new(
                Quat::from_rotation_x(angles.wrist_pitch[side]),
                rest.translation,
            ),
            BoneId::ThighL | BoneId::ThighR => Transform::new(
                Quat::from_rotation_z(sign * angles.hip_roll[side])
                    * Quat::from_rotation_x(angles.hip_pitch[side]),
                rest.translation,
            ),
            BoneId::ShinL | BoneId::ShinR => Transform::new(
                Quat::from_rotation_x(-angles.knee_flex[side]),
                rest.translation,
            ),
            BoneId::FootL | BoneId::FootR => Transform::new(
                Quat::from_rotation_x(angles.ankle_pitch[side]),
                rest.translation,
            ),
        };
        locals[bone.index()] = updated;
    }
    locals
}

fn part_matrices(
    character: &CompiledCharacter,
    world: Mat4,
    bone_world: &[Transform; BONE_COUNT],
) -> [Mat4; BONE_COUNT] {
    let mut matrices = [Mat4::IDENTITY; BONE_COUNT];
    for (index, part) in character.parts().iter().enumerate() {
        let origin = Vec3::from_array(part.volume().origin);
        matrices[index] = world * bone_world[index].to_mat4() * Mat4::from_translation(origin);
    }
    matrices
}

fn sole_height(character: &CompiledCharacter, matrix: Mat4, bone: BoneId) -> f32 {
    let part = character.collision().part_box(bone);
    part.corners()
        .into_iter()
        .map(|corner| matrix.transform_point3(corner).y)
        .fold(f32::INFINITY, f32::min)
}

/// Poses a character.
///
/// With `ground` set to `None` the result is the animation alone, which is what
/// the neutral preview scene and the pure-animation tests want. With a sampler
/// the legs are solved against the surface.
#[must_use]
pub fn pose(
    character: &CompiledCharacter,
    state: &CharacterState,
    ground: Option<&dyn GroundSampler>,
) -> PosedCharacter {
    let gait = character.gait();
    let blend = gait.blend(state.speed);
    let angles = animate(gait, blend, state.phase, state.time);
    let mut locals = locals_from_angles(character, &angles);

    let skeleton = character.skeleton();
    let mut bone_world = [Transform::IDENTITY; BONE_COUNT];
    skeleton.compose_into(Transform::IDENTITY, &locals, &mut bone_world);

    let world = Mat4::from_scale_rotation_translation(
        Vec3::splat(CHARACTER_VOXEL_SIZE),
        facing_rotation(state.facing),
        state.stand_point(),
    );

    let mut contacts = [FootContact::default(); 2];
    if let Some(ground) = ground {
        let body = character.body();
        let profile = gait.profile(blend);
        let ankle_above_sole = body.ankle_y as f32 * CHARACTER_VOXEL_SIZE;
        let thigh_length = body.thigh_length as f32;
        let shin_length = (body.knee_y - body.ankle_y) as f32;
        let ankle_limit = JOINT_LIMIT_DEGREES.ankle_pitch.1.to_radians();
        let forward = facing_direction(state.facing);
        let toe_reach = f64::from((body.foot_length - 1) as f32 * CHARACTER_VOXEL_SIZE);
        let heel_reach = f64::from(CHARACTER_VOXEL_SIZE);

        for side in [LEFT, RIGHT] {
            let (thigh, shin, foot) = leg_bones(side);
            let hip_local = bone_world[thigh.index()].translation;
            let ankle_local = bone_world[foot.index()].translation;
            let ankle_world = world.transform_point3(ankle_local);

            let side_offset = if side == RIGHT { 0.0 } else { 0.5 };
            let cycle = (state.phase + side_offset).rem_euclid(1.0);
            let animated_stance = stance_weight(cycle, profile.duty_factor);
            // Standing still means both feet are planted.
            let stance = animated_stance.max(1.0 - blend.moving).clamp(0.0, 1.0);

            let sample = |dx: f64, dz: f64| {
                ground.surface(f64::from(ankle_world.x) + dx, f64::from(ankle_world.z) + dz)
            };
            let Some(under_ankle) = sample(0.0, 0.0) else {
                contacts[side] = FootContact {
                    grounded: false,
                    stance,
                    ground_height: f32::NAN,
                    sole_height: ankle_world.y - ankle_above_sole,
                    reached: false,
                };
                continue;
            };
            let toe = sample(
                f64::from(forward.x) * toe_reach,
                f64::from(forward.z) * toe_reach,
            );
            let heel = sample(
                -f64::from(forward.x) * heel_reach,
                -f64::from(forward.z) * heel_reach,
            );
            // Two questions, and conflating them was a bug worth a comment.
            //
            // **What does this foot stand on?** The block under its own ankle.
            // A rigid foot overhanging a higher terrace clips the riser with
            // its toe, which is bounded by the foot's length and is what a
            // blocky world does to a blocky foot. Planting on the higher block
            // instead asks the leg to lift a sole a whole terrain voxel above
            // the pelvis; no leg can, so the solver clamped and the contact
            // reported a foot most of a terrace away from where it really was.
            let support = under_ankle as f32;
            // **What must this foot clear while swinging?** The highest block
            // under any part of it, so a swinging toe does not pass through
            // the terrace the character is about to step onto.
            let clearance_support = support
                .max(toe.map_or(f32::NEG_INFINITY, |height| height as f32))
                .max(heel.map_or(f32::NEG_INFINITY, |height| height as f32));
            let slope_pitch = match (toe, heel) {
                (Some(front), Some(back)) => {
                    (((front - back) / (toe_reach + heel_reach)) as f32).atan()
                }
                _ => 0.0,
            }
            .clamp(-ankle_limit, ankle_limit);

            let planted = support + ankle_above_sole;
            let free = ankle_world.y.max(clearance_support + ankle_above_sole);
            // The ankle can never be asked for more than the leg can span.
            let hip_world_y = world.transform_point3(hip_local).y;
            let ankle_ceiling = hip_world_y
                - (thigh_length + shin_length) * MIN_LEG_SPAN_FRACTION * CHARACTER_VOXEL_SIZE;
            let wanted = free + (planted - free) * stance;
            let mut target_world_y = wanted.min(ankle_ceiling);
            let mut limited = target_world_y < wanted;
            // Leg lengths to voxels, faded in with the gait so a standing
            // character keeps its feet under its hips.
            let travel_offset = foot_offset(&profile, cycle)
                * blend.moving.clamp(0.0, 1.0)
                * body.leg_length as f32;
            let animated_foot = locals[foot.index()].rotation;
            let mut reached = false;
            let mut sole = ankle_world.y - ankle_above_sole;

            // Three passes at most. The first places the ankle; the rest
            // correct for the fact that a pitched rigid foot puts its lowest
            // corner somewhere no ankle-space target can express. A sole under
            // the block is always corrected in full; a sole hovering over it is
            // corrected in proportion to how planted the foot is, so a swinging
            // foot is never dragged down onto the ground.
            for _ in 0..3 {
                let root_rotation = bone_world[BoneId::Root.index()].rotation;
                let target_local_y = (target_world_y - state.base_height) / CHARACTER_VOXEL_SIZE;
                // Forward placement comes from the gait's own offset rather
                // than from wherever the animated knee happened to leave the
                // ankle. Seeding the hip angle alone is not enough: the knee
                // bends the shin, so the realised offset drifts from the one
                // the stride asked for and the foot slides again. Solving the
                // leg for both axes is what makes the offset the offset.
                let target_local =
                    Vec3::new(ankle_local.x, target_local_y, hip_local.z - travel_offset);
                let solution = solve_two_bone(
                    hip_local,
                    target_local,
                    thigh_length,
                    shin_length,
                    root_rotation * Vec3::NEG_Z,
                );
                reached = solution.reached && !limited;

                let thigh_world_rotation =
                    Quat::from_rotation_arc(Vec3::NEG_Y, solution.upper_direction);
                locals[thigh.index()] = Transform::new(
                    root_rotation.inverse() * thigh_world_rotation,
                    locals[thigh.index()].translation,
                );
                let knee_local = hip_local + solution.upper_direction * thigh_length;
                let lower = target_local - knee_local;
                let lower_direction = if lower.length_squared() > 1.0e-8 {
                    lower.normalize()
                } else {
                    solution.upper_direction
                };
                let shin_world_rotation = Quat::from_rotation_arc(Vec3::NEG_Y, lower_direction);
                locals[shin.index()] = Transform::new(
                    thigh_world_rotation.inverse() * shin_world_rotation,
                    locals[shin.index()].translation,
                );
                skeleton.compose_into(Transform::IDENTITY, &locals, &mut bone_world);

                // Pitch the sole to the local gradient, blended in with stance.
                let contact_foot = clamp_rotation_angle(
                    bone_world[shin.index()].rotation.inverse()
                        * Quat::from_rotation_x(slope_pitch),
                    ankle_limit,
                );
                locals[foot.index()] = Transform::new(
                    animated_foot.slerp(contact_foot, stance),
                    locals[foot.index()].translation,
                );
                skeleton.compose_into(Transform::IDENTITY, &locals, &mut bone_world);

                let matrices = part_matrices(character, world, &bone_world);
                sole = sole_height(character, matrices[foot.index()], foot);
                let error = support - sole;
                let correction = if error > 0.0 { error } else { error * stance };
                if correction.abs() <= 1.0e-5 {
                    break;
                }
                let corrected = target_world_y + correction;
                let next = corrected.min(ankle_ceiling);
                limited |= next < corrected;
                if (next - target_world_y).abs() <= 1.0e-5 {
                    break;
                }
                target_world_y = next;
            }

            contacts[side] = FootContact {
                grounded: true,
                stance,
                ground_height: support,
                sole_height: sole,
                reached,
            };
        }
    }

    let part_matrices = part_matrices(character, world, &bone_world);
    PosedCharacter {
        world,
        bone_world,
        part_matrices,
        angles,
        blend,
        contacts,
    }
}

/// Convenience: the rest pose with no gait and no contact.
#[must_use]
pub fn rest_pose(character: &CompiledCharacter) -> PosedCharacter {
    let state = CharacterState::default();
    let angles = JointAngles::default();
    let locals = locals_from_angles(character, &angles);
    let mut bone_world = [Transform::IDENTITY; BONE_COUNT];
    character
        .skeleton()
        .compose_into(Transform::IDENTITY, &locals, &mut bone_world);
    let world = Mat4::from_scale_rotation_translation(
        Vec3::splat(CHARACTER_VOXEL_SIZE),
        facing_rotation(0.0),
        state.stand_point(),
    );
    let part_matrices = part_matrices(character, world, &bone_world);
    PosedCharacter {
        world,
        bone_world,
        part_matrices,
        angles,
        blend: GaitBlend {
            moving: 0.0,
            run: 0.0,
        },
        contacts: [FootContact::default(); 2],
    }
}

#[cfg(test)]
mod tests {
    use super::{CharacterState, LEFT, RIGHT, pose, rest_pose};
    use crate::compiler::{CharacterCompiler, CompiledCharacter};
    use crate::descriptor::{CHARACTER_VOXEL_SIZE, CharacterDescriptor};
    use crate::ground::{BoundedGround, FlatGround, RampGround, StepGround, SteppedRamp};
    use crate::skeleton::{ALL_BONES, BoneId};

    fn character() -> CompiledCharacter {
        match CharacterCompiler::new().compile_descriptor(&CharacterDescriptor::golden()) {
            Ok(character) => character,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn a_pose_is_finite_and_never_stretches_a_bone() {
        let character = character();
        let ground = FlatGround::at(20.0);
        let mut state = CharacterState::standing(3.0, -7.0, 0.7, Some(&ground));
        let rest = character.skeleton().rest_locals();
        for step in 0..240 {
            state.speed = match step % 3 {
                0 => 0.0,
                1 => 2.0,
                _ => 5.2,
            };
            state.walk_forward(1.0 / 60.0, &character, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            assert!(posed.is_finite(), "step {step}");
            for bone in ALL_BONES {
                let Some(parent) = bone.parent() else {
                    continue;
                };
                let expected = rest[bone.index()].translation.length();
                let found = (posed.bone_world()[bone.index()].translation
                    - posed.bone_world()[parent.index()].translation)
                    .length();
                assert!(
                    (found - expected).abs() < 1.0e-3,
                    "{} stretched from {expected} to {found} at step {step}",
                    bone.name()
                );
            }
        }
    }

    #[test]
    fn standing_on_a_flat_floor_puts_both_soles_on_it() {
        let character = character();
        let ground = FlatGround::at(12.0);
        let state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
        let posed = pose(&character, &state, Some(&ground));
        for side in [LEFT, RIGHT] {
            let contact = posed.contacts()[side];
            assert!(contact.grounded);
            assert!(
                contact.clearance().abs() <= CHARACTER_VOXEL_SIZE,
                "side {side} sole clearance {}",
                contact.clearance()
            );
        }
    }

    #[test]
    fn a_planted_foot_stays_on_the_surface_through_a_whole_walk() {
        let character = character();
        let ground = FlatGround::at(9.0);
        let mut state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
        state.speed = 2.0;
        let mut worst = 0.0_f32;
        let mut checked = 0;
        for _ in 0..600 {
            state.walk_forward(1.0 / 120.0, &character, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            for side in [LEFT, RIGHT] {
                let contact = posed.contacts()[side];
                assert!(
                    contact.clearance() > -CHARACTER_VOXEL_SIZE * 1.5,
                    "side {side} sole sank {} below the floor",
                    contact.clearance()
                );
                if contact.stance > 0.9 {
                    worst = worst.max(contact.clearance().abs());
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "the walk never reached mid stance");
        assert!(
            worst <= CHARACTER_VOXEL_SIZE,
            "a planted sole drifted {worst} from the floor"
        );
    }

    #[test]
    fn a_slope_lowers_the_downhill_foot_and_drops_the_pelvis() {
        let character = character();
        let ground = RampGround {
            slope: 0.35,
            height_at_origin: 10.0,
        };
        // Facing -Z puts the two feet on different x, which is the axis the
        // ramp rises along; facing along the ramp would put both at one height.
        let mut state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
        // Settle the smoothed pelvis height.
        for _ in 0..120 {
            state.advance(1.0 / 60.0, &character, Some(&ground));
        }
        let posed = pose(&character, &state, Some(&ground));
        let left = posed.contacts()[LEFT];
        let right = posed.contacts()[RIGHT];
        assert!(left.grounded && right.grounded);
        assert!(
            (left.ground_height - right.ground_height).abs() > 1.0e-3,
            "the two feet found the same height on a slope"
        );
        for side in [LEFT, RIGHT] {
            assert!(
                posed.contacts()[side].clearance().abs() <= CHARACTER_VOXEL_SIZE * 1.5,
                "side {side} clearance {} on a slope",
                posed.contacts()[side].clearance()
            );
        }
    }

    /// A terrace the leg can actually reach: the sole contract holds exactly.
    #[test]
    fn a_terraced_slope_keeps_the_feet_on_the_blocks() {
        let character = character();
        let ground = SteppedRamp {
            slope: 0.18,
            height_at_origin: 14.0,
            voxel: 0.25,
        };
        let mut state =
            CharacterState::standing(-30.0, 0.0, std::f32::consts::FRAC_PI_2, Some(&ground));
        state.speed = 2.0;
        let mut worst_stance = 0.0_f32;
        for _ in 0..1200 {
            state.walk_forward(1.0 / 120.0, &character, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            for side in [LEFT, RIGHT] {
                let contact = posed.contacts()[side];
                assert!(contact.grounded);
                if contact.stance > 0.9 {
                    worst_stance = worst_stance.max(contact.clearance().abs());
                }
                assert!(
                    contact.clearance() > -CHARACTER_VOXEL_SIZE * 1.2,
                    "a sole cut {} into a block",
                    contact.clearance()
                );
            }
        }
        // Two and a half character voxels. A planted foot is not pinned to the
        // world: its horizontal position comes from the gait, so during stance
        // it drifts across the ground and can cross a terrace edge, and the
        // sole then follows the new block over the next few frames. Pinning the
        // foot for the length of a stance is the standard cure and is not in
        // this milestone.
        assert!(
            worst_stance <= CHARACTER_VOXEL_SIZE * 2.5,
            "a planted sole drifted {worst_stance} from the block top"
        );
    }

    /// A terrain terrace is a whole world unit, and the golden humanoid's leg
    /// is `1.17`, so the character is now big enough for its own world. What it
    /// still cannot do is plant a foot on a terrace the instant its toe reaches
    /// one: the pelvis has to rise first, and until it has, the leg cannot put
    /// a sole a terrace above the hip.
    ///
    /// The earlier scale made this far worse. At `1.75` world units the leg was
    /// `0.69`, shorter than a single terrace, and no amount of pelvis rise
    /// helped. That capture is what moved the scale; this test is what keeps
    /// both the decision and its remaining cost honest.
    ///
    /// What is guaranteed here: nothing becomes infinite, no joint leaves its
    /// range, the solver says plainly when it did not reach, and the deviation
    /// is bounded by one terrace rather than unbounded.
    #[test]
    fn a_terrain_scale_terrace_is_bounded_and_reported() {
        let character = character();
        let leg = character.body().leg_length as f32 * CHARACTER_VOXEL_SIZE;
        assert!(
            leg > 1.0,
            "the leg is {leg} world units, shorter than one terrain voxel; a \
             character this size cannot walk its own world"
        );

        let ground = SteppedRamp::terrain(0.18, 14.0);
        let mut state =
            CharacterState::standing(-30.0, 0.0, std::f32::consts::FRAC_PI_2, Some(&ground));
        state.speed = 2.0;
        let mut worst_clip = 0.0_f32;
        let mut worst_stance = 0.0_f32;
        let mut unreached = 0_u32;
        for _ in 0..1200 {
            state.walk_forward(1.0 / 120.0, &character, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            assert!(posed.is_finite());
            assert!(posed.angles().within_limits());
            for side in [LEFT, RIGHT] {
                let contact = posed.contacts()[side];
                assert!(contact.grounded);
                if !contact.reached {
                    unreached += 1;
                }
                if contact.stance > 0.95 {
                    worst_stance = worst_stance.max(contact.clearance().abs());
                }
                worst_clip = worst_clip.min(contact.clearance()).min(0.0);
            }
        }
        assert!(
            unreached > 0,
            "the solver silently claimed to reach a terrace it could not"
        );
        assert!(
            worst_stance < 1.0,
            "a planted sole drifted {worst_stance}, a whole terrain voxel or more"
        );
        assert!(worst_clip > -1.0, "a foot cut {worst_clip} into a riser");
    }

    /// Foot sliding, measured rather than eyeballed.
    ///
    /// This is the defect a distance-driven gait exists to remove, and it is
    /// also the one a capture cannot settle: a screen capture stalls the
    /// client, so consecutive frames of a burst land about a stride apart and
    /// a planted foot has legitimately moved on between them. The measurement
    /// belongs here, and it found a real bug — the hip swing was inverted
    /// relative to the stance window, so the planted foot swept *forward*
    /// through stance and the character walked like a treadmill. That version
    /// measured `1.39` world units of wander per stance against a stride of
    /// `0.99`; this one measures hundredths.
    ///
    /// What is measured is how far the **ankle joint** wanders across a single
    /// stance. The ankle is the joint the contact solver plants; the foot box
    /// around it pitches to the ground, so the part itself moves a little more
    /// than the ankle does, and that is a foot rolling rather than a foot
    /// sliding. Both numbers are printed.
    #[test]
    fn a_planted_foot_does_not_slide_on_level_ground() {
        let character = character();
        let ground = FlatGround::at(0.0);
        let foot_bone = [BoneId::FootL.index(), BoneId::FootR.index()];

        for legs_per_second in [1.2_f32, 3.4] {
            let mut state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
            state.speed = character.gait().speed_for(legs_per_second);
            // Per side, the bounding box of this stance so far: ankle then part.
            let mut ankle_span: [Option<(f32, f32, f32, f32)>; 2] = [None, None];
            let mut part_span: [Option<(f32, f32, f32, f32)>; 2] = [None, None];
            let mut worst_ankle = 0.0_f32;
            let mut worst_part = 0.0_f32;
            let mut stances = 0_u32;

            for _ in 0..4800 {
                state.walk_forward(1.0 / 240.0, &character, Some(&ground));
                let posed = pose(&character, &state, Some(&ground));
                let world = posed.world_matrix();
                for side in [LEFT, RIGHT] {
                    let bone = foot_bone[side];
                    let ankle = world.transform_point3(posed.bone_world()[bone].translation);
                    let part = posed.part_matrices()[bone].w_axis;
                    let planted = posed.contacts()[side].stance > 0.9;
                    for (span, x, z) in [
                        (&mut ankle_span[side], ankle.x, ankle.z),
                        (&mut part_span[side], part.x, part.z),
                    ] {
                        if planted {
                            *span = Some(match *span {
                                Some((lo_x, hi_x, lo_z, hi_z)) => {
                                    (lo_x.min(x), hi_x.max(x), lo_z.min(z), hi_z.max(z))
                                }
                                None => (x, x, z, z),
                            });
                        }
                    }
                    if planted {
                        continue;
                    }
                    if let Some((lo_x, hi_x, lo_z, hi_z)) = ankle_span[side].take() {
                        worst_ankle =
                            worst_ankle.max(((hi_x - lo_x).powi(2) + (hi_z - lo_z).powi(2)).sqrt());
                        stances += 1;
                    }
                    if let Some((lo_x, hi_x, lo_z, hi_z)) = part_span[side].take() {
                        worst_part =
                            worst_part.max(((hi_x - lo_x).powi(2) + (hi_z - lo_z).powi(2)).sqrt());
                    }
                }
            }

            assert!(
                stances >= 20,
                "only {stances} completed stances at {legs_per_second} leg lengths per second"
            );
            println!(
                "{legs_per_second} leg lengths per second, {stances} stances: ankle wanders \
                 {worst_ankle} world units ({:.2} character voxels), foot part {worst_part} \
                 ({:.2} voxels)",
                worst_ankle / CHARACTER_VOXEL_SIZE,
                worst_part / CHARACTER_VOXEL_SIZE
            );
            // A foot riding along with the body would wander a whole stride.
            assert!(
                worst_ankle < CHARACTER_VOXEL_SIZE,
                "a planted ankle wandered {worst_ankle} world units, more than one \
                 character voxel, at {legs_per_second} leg lengths per second"
            );
            // The foot box rolls about the planted ankle, which is what a foot
            // does; it may not roll more than half its own length.
            let foot_length = character.body().foot_length as f32 * CHARACTER_VOXEL_SIZE;
            assert!(
                worst_part < foot_length * 0.5,
                "the foot part wandered {worst_part} of a {foot_length}-unit foot"
            );
        }
    }

    #[test]
    fn a_step_does_not_produce_a_nan_or_an_impossible_leg() {
        let character = character();
        let ground = StepGround {
            edge_x: 0.0,
            low: 10.0,
            high: 11.0,
        };
        let mut state =
            CharacterState::standing(-2.0, 0.0, std::f32::consts::FRAC_PI_2, Some(&ground));
        state.speed = 2.0;
        for _ in 0..500 {
            state.walk_forward(1.0 / 120.0, &character, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            assert!(posed.is_finite());
            assert!(posed.angles().is_finite());
            assert!(posed.angles().within_limits());
        }
    }

    #[test]
    fn contact_works_the_same_at_negative_coordinates_and_across_the_origin() {
        let character = character();
        let ground = FlatGround::at(-5.0);
        for (x, z) in [
            (-300.0_f32, -420.0_f32),
            (-0.5, 0.5),
            (0.0, 0.0),
            (512.0, -33.0),
        ] {
            let state = CharacterState::standing(x, z, 2.1, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            for side in [LEFT, RIGHT] {
                let contact = posed.contacts()[side];
                assert!(contact.grounded, "no ground at ({x}, {z})");
                assert!(
                    contact.clearance().abs() <= CHARACTER_VOXEL_SIZE,
                    "clearance {} at ({x}, {z})",
                    contact.clearance()
                );
            }
        }
    }

    #[test]
    fn leaving_the_region_reports_absence_rather_than_a_floor_at_zero() {
        let character = character();
        let ground = BoundedGround {
            inner: FlatGround::at(6.0),
            min_x: -4.0,
            max_x: 4.0,
            min_z: -4.0,
            max_z: 4.0,
        };
        let mut state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
        assert!(state.grounded);
        state.x = 40.0;
        state.advance(1.0 / 60.0, &character, Some(&ground));
        assert!(!state.grounded, "absence was turned into a height");
        let posed = pose(&character, &state, Some(&ground));
        assert!(posed.is_finite());
        for side in [LEFT, RIGHT] {
            assert!(!posed.contacts()[side].grounded);
        }
    }

    #[test]
    fn the_pelvis_falls_faster_than_it_rises() {
        let character = character();
        let high = FlatGround::at(11.0);
        let low = FlatGround::at(10.0);

        let mut rising = CharacterState::standing(0.0, 0.0, 0.0, Some(&low));
        let mut falling = CharacterState::standing(0.0, 0.0, 0.0, Some(&high));
        for _ in 0..4 {
            rising.advance(1.0 / 60.0, &character, Some(&high));
            falling.advance(1.0 / 60.0, &character, Some(&low));
        }
        let risen = rising.base_height - 10.0;
        let fallen = 11.0 - falling.base_height;
        assert!(
            fallen > risen * 1.5,
            "the pelvis fell {fallen} and rose {risen}; falling must be faster"
        );
        // Falling has to be fast because a straight leg has no downward
        // headroom: whatever the pelvis lags above the ground is unreachable.
        assert!(fallen > 0.6, "the pelvis barely fell: {fallen}");
    }

    #[test]
    fn the_pelvis_follows_a_step_without_teleporting() {
        let character = character();
        let low = FlatGround::at(10.0);
        let high = FlatGround::at(11.0);
        let mut state = CharacterState::standing(0.0, 0.0, 0.0, Some(&low));
        assert!((state.base_height - 10.0).abs() < 1.0e-6);
        let mut previous = state.base_height;
        let mut jumped = false;
        for _ in 0..90 {
            state.advance(1.0 / 60.0, &character, Some(&high));
            if (state.base_height - previous).abs() > 0.35 {
                jumped = true;
            }
            previous = state.base_height;
        }
        assert!(!jumped, "the pelvis teleported up the step");
        assert!(
            (state.base_height - 11.0).abs() < 0.02,
            "the pelvis never arrived: {}",
            state.base_height
        );
    }

    #[test]
    fn a_pose_without_ground_is_the_animation_alone() {
        let character = character();
        let state = CharacterState {
            speed: 1.6,
            phase: 0.3,
            ..CharacterState::default()
        };
        let posed = pose(&character, &state, None);
        assert!(posed.is_finite());
        for side in [LEFT, RIGHT] {
            assert!(!posed.contacts()[side].grounded);
        }
    }

    #[test]
    fn the_rest_pose_stands_on_its_own_origin() {
        let character = character();
        let posed = rest_pose(&character);
        assert!(posed.is_finite());
        let feet = [BoneId::FootL, BoneId::FootR];
        for foot in feet {
            let matrix = posed.part_matrices()[foot.index()];
            let lowest = character
                .collision()
                .part_box(foot)
                .corners()
                .into_iter()
                .map(|corner| matrix.transform_point3(corner).y)
                .fold(f32::INFINITY, f32::min);
            assert!(
                lowest.abs() < 1.0e-3,
                "{} sole sits at {lowest} instead of on the origin",
                foot.name()
            );
        }
    }

    #[test]
    fn the_world_matrix_places_and_scales_the_character() {
        let character = character();
        let ground = FlatGround::at(4.0);
        let state = CharacterState::standing(12.0, -8.0, 0.0, Some(&ground));
        let posed = pose(&character, &state, Some(&ground));
        let origin = posed.world_matrix().transform_point3(glam::Vec3::ZERO);
        assert!((origin.x - 12.0).abs() < 1.0e-4);
        assert!((origin.z + 8.0).abs() < 1.0e-4);
        assert!((origin.y - 4.0).abs() < 1.0e-4);
        let head = posed.bone_world()[BoneId::Head.index()].translation;
        let head_world = posed.world_matrix().transform_point3(head);
        let expected = 4.0 + character.body().neck_y as f32 * CHARACTER_VOXEL_SIZE;
        assert!(
            (head_world.y - expected).abs() < 0.05,
            "the neck sits at {} rather than {expected}",
            head_world.y
        );
    }

    #[test]
    fn facing_turns_the_character_the_way_the_camera_turns() {
        let character = character();
        let ground = FlatGround::at(0.0);
        // The toes have to point the same way the client's camera would look
        // at the same yaw. Zero alone does not prove it: the wrong sign is
        // still identity there, and the first captures showed a character
        // walking backwards because only zero had been checked.
        for yaw in [
            0.0_f32,
            std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
            2.4,
        ] {
            let state = CharacterState::standing(0.0, 0.0, yaw, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            let foot = posed.part_matrices()[BoneId::FootR.index()];
            let toe = foot.transform_point3(glam::Vec3::new(1.5, 1.5, 0.0));
            let heel = foot.transform_point3(glam::Vec3::new(1.5, 1.5, 5.0));
            let toes = (toe - heel).normalize();
            let expected = super::facing_direction(yaw);
            assert!(
                toes.dot(expected) > 0.95,
                "at yaw {yaw} the toes point {toes} but the camera would look {expected}"
            );
        }
    }

    #[test]
    fn a_walking_character_moves_the_way_it_faces() {
        let character = character();
        let ground = FlatGround::at(3.0);
        for yaw in [0.0_f32, std::f32::consts::FRAC_PI_2, 2.4, -1.1] {
            let mut state = CharacterState::standing(0.0, 0.0, yaw, Some(&ground));
            state.speed = 2.0;
            for _ in 0..60 {
                state.walk_forward(1.0 / 60.0, &character, Some(&ground));
            }
            let travelled = glam::Vec3::new(state.x, 0.0, state.z).normalize();
            let expected = super::facing_direction(yaw);
            assert!(
                travelled.dot(expected) > 0.999,
                "at yaw {yaw} the character walked {travelled} while facing {expected}"
            );
        }
    }
}
