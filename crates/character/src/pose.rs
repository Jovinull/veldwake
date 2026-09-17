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
    GaitBlend, JOINT_LIMIT_DEGREES, JointAngles, LEFT, RIGHT, animate, stance_weight,
};
use crate::skeleton::{ALL_BONES, BONE_COUNT, BoneId, Side, Transform};

/// How quickly the pelvis follows a change in ground height, in seconds.
///
/// The feet snap to the block they stand on; without this the pelvis would snap
/// with them and a gentle terraced slope would read as the character hopping up
/// a staircase. Small enough that the body never lags visibly behind the feet.
pub const PELVIS_FOLLOW_TAU: f32 = 0.10;

/// Largest frame step the state will integrate, in seconds.
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
                    let alpha = 1.0 - (-step / PELVIS_FOLLOW_TAU).exp();
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
        self.x += self.facing.sin() * distance;
        self.z += -self.facing.cos() * distance;
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
        Quat::from_rotation_y(state.facing),
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
        let forward = Vec3::new(state.facing.sin(), 0.0, -state.facing.cos());
        let toe_reach = f64::from((body.foot_length - 1) as f32 * CHARACTER_VOXEL_SIZE);
        let heel_reach = f64::from(CHARACTER_VOXEL_SIZE);

        for side in [LEFT, RIGHT] {
            let (thigh, shin, foot) = leg_bones(side);
            let hip_local = bone_world[thigh.index()].translation;
            let ankle_local = bone_world[foot.index()].translation;
            let ankle_world = world.transform_point3(ankle_local);

            let offset = if side == RIGHT { 0.0 } else { 0.5 };
            let animated_stance = stance_weight(state.phase + offset, profile.duty_factor);
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
            // A foot is a rigid box reaching forward of its ankle, so what it
            // stands on is the highest block under any of it, not the block
            // under the ankle alone. Sampling only the ankle lets a toe cut
            // into the terrace the character is stepping onto.
            let toe = sample(
                f64::from(forward.x) * toe_reach,
                f64::from(forward.z) * toe_reach,
            );
            let heel = sample(
                -f64::from(forward.x) * heel_reach,
                -f64::from(forward.z) * heel_reach,
            );
            let support = (under_ankle as f32)
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
            let free = ankle_world.y.max(planted);
            let mut target_world_y = free + (planted - free) * stance;
            let animated_foot = locals[foot.index()].rotation;
            let mut reached = false;
            let mut sole = ankle_world.y - ankle_above_sole;

            // Two passes at most. The first places the ankle; the second
            // corrects for the fact that a pitched rigid foot can still put a
            // corner under the block, which no ankle-space target can express.
            for _ in 0..2 {
                let root_rotation = bone_world[BoneId::Root.index()].rotation;
                let target_local_y = (target_world_y - state.base_height) / CHARACTER_VOXEL_SIZE;
                let target_local = Vec3::new(ankle_local.x, target_local_y, ankle_local.z);
                let solution = solve_two_bone(
                    hip_local,
                    target_local,
                    thigh_length,
                    shin_length,
                    root_rotation * Vec3::NEG_Z,
                );
                reached = solution.reached;

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
                let deficit = support - sole;
                if deficit <= 1.0e-5 {
                    break;
                }
                target_world_y += deficit;
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
        Quat::IDENTITY,
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
                1 => 1.4,
                _ => 3.9,
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
        state.speed = 1.5;
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
        state.speed = 1.4;
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
        assert!(
            worst_stance <= CHARACTER_VOXEL_SIZE * 1.5,
            "a planted sole drifted {worst_stance} from the block top"
        );
    }

    /// One **terrain** voxel is a whole world unit, and the golden humanoid's
    /// leg is `0.6875` world units. A terrain terrace is therefore taller than
    /// the character's leg, and no amount of IK can lift a foot onto it: the
    /// chain clamps and the swinging foot passes through the riser.
    ///
    /// This is a scale finding, not a solver bug, and it is a test so that it
    /// stays a known quantity rather than a surprise in a capture. What the
    /// solver must still guarantee is that nothing becomes infinite, no joint
    /// leaves its range, and a planted foot still rests on what is under it.
    #[test]
    fn a_terrain_scale_terrace_is_taller_than_the_leg_and_is_bounded_not_hidden() {
        let character = character();
        let leg = character.body().leg_length as f32 * CHARACTER_VOXEL_SIZE;
        assert!(
            leg < 1.0,
            "the leg is {leg} world units; this test documents the case where a              one-unit terrain terrace is taller than that"
        );

        let ground = SteppedRamp::terrain(0.18, 14.0);
        let mut state =
            CharacterState::standing(-30.0, 0.0, std::f32::consts::FRAC_PI_2, Some(&ground));
        state.speed = 1.4;
        let mut worst_clip = 0.0_f32;
        let mut worst_stance = 0.0_f32;
        for _ in 0..1200 {
            state.walk_forward(1.0 / 120.0, &character, Some(&ground));
            let posed = pose(&character, &state, Some(&ground));
            assert!(posed.is_finite());
            assert!(posed.angles().within_limits());
            for side in [LEFT, RIGHT] {
                let contact = posed.contacts()[side];
                assert!(contact.grounded);
                if contact.stance > 0.95 {
                    worst_stance = worst_stance.max(contact.clearance().abs());
                }
                worst_clip = worst_clip.min(contact.clearance()).min(0.0);
            }
        }
        assert!(
            worst_stance <= CHARACTER_VOXEL_SIZE * 3.0,
            "a planted sole drifted {worst_stance} from the block it rests on"
        );
        assert!(
            worst_clip > -1.0,
            "a foot passed further than one whole terrain voxel through a riser: {worst_clip}"
        );
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
        state.speed = 1.2;
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
        let state = CharacterState::standing(0.0, 0.0, 0.0, Some(&ground));
        let posed = pose(&character, &state, Some(&ground));
        // At zero yaw the toes point toward -Z, like the camera's forward.
        let foot = posed.part_matrices()[BoneId::FootR.index()];
        let toe = foot.transform_point3(glam::Vec3::new(1.5, 1.5, 0.0));
        let heel = foot.transform_point3(glam::Vec3::new(1.5, 1.5, 5.0));
        assert!(toe.z < heel.z, "the toes do not point forward");
    }
}
