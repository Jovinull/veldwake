//! The humanoid skeleton: sixteen bones, their rest pose, and the rules that
//! make a pose composable in one forward pass.
//!
//! Sixteen is the smallest hierarchy that can carry what M5 has to prove.
//! Every bone is here for a reason that a capture would expose if it were
//! missing:
//!
//! - `Root`, `Spine`, and `Chest` are three separate bones so the chest can
//!   counter-rotate against the pelvis. With one torso bone a walk reads as
//!   sliding while swinging the arms, which is the failure this milestone is
//!   most likely to ship by accident.
//! - `Foot` is separate because it is the bone that proves slope adaptation.
//! - `Hand` is separate because an arm whose hand does not counter-rotate
//!   reads as a stick. It is **not** here as a weapon socket; combat is M6.
//!
//! There is no clavicle, no neck, no finger, and no twist bone. Nothing in
//! this milestone consumes one.
//!
//! Bones are stored in topological order, so `parent index < child index` and
//! composing world transforms is a single forward pass with no recursion and
//! no visited set. That ordering is a tested invariant, not a convention.

use glam::{Mat4, Quat, Vec3};

use crate::descriptor::BodyMetrics;

/// Number of bones in the humanoid skeleton.
pub const BONE_COUNT: usize = 16;

/// Outward roll of the arms in the rest pose, in degrees.
///
/// The character style contract's posture rule: arms that hang exactly
/// vertical read as fused to the torso.
pub const REST_ARM_ROLL_DEGREES: f32 = 6.0;

/// One bone of the humanoid skeleton.
///
/// A plain enum rather than an index newtype: the set is fixed, the compiler
/// checks exhaustiveness, and a caller cannot invent a bone that does not
/// exist.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum BoneId {
    Root = 0,
    Spine = 1,
    Chest = 2,
    Head = 3,
    UpperArmL = 4,
    ForearmL = 5,
    HandL = 6,
    UpperArmR = 7,
    ForearmR = 8,
    HandR = 9,
    ThighL = 10,
    ShinL = 11,
    FootL = 12,
    ThighR = 13,
    ShinR = 14,
    FootR = 15,
}

/// Every bone, in topological order.
pub const ALL_BONES: [BoneId; BONE_COUNT] = [
    BoneId::Root,
    BoneId::Spine,
    BoneId::Chest,
    BoneId::Head,
    BoneId::UpperArmL,
    BoneId::ForearmL,
    BoneId::HandL,
    BoneId::UpperArmR,
    BoneId::ForearmR,
    BoneId::HandR,
    BoneId::ThighL,
    BoneId::ShinL,
    BoneId::FootL,
    BoneId::ThighR,
    BoneId::ShinR,
    BoneId::FootR,
];

/// Which side of the body a bone belongs to.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Side {
    Left,
    Right,
    Centre,
}

impl Side {
    /// `-1` for the left side, `+1` for the right, `0` on the midline.
    #[must_use]
    pub const fn sign(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
            Self::Centre => 0.0,
        }
    }

    /// The opposite side; the midline is its own opposite.
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Centre => Self::Centre,
        }
    }
}

impl BoneId {
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn parent(self) -> Option<Self> {
        match self {
            Self::Root => None,
            Self::Spine => Some(Self::Root),
            Self::Chest => Some(Self::Spine),
            Self::Head | Self::UpperArmL | Self::UpperArmR => Some(Self::Chest),
            Self::ForearmL => Some(Self::UpperArmL),
            Self::HandL => Some(Self::ForearmL),
            Self::ForearmR => Some(Self::UpperArmR),
            Self::HandR => Some(Self::ForearmR),
            Self::ThighL | Self::ThighR => Some(Self::Root),
            Self::ShinL => Some(Self::ThighL),
            Self::FootL => Some(Self::ShinL),
            Self::ShinR => Some(Self::ThighR),
            Self::FootR => Some(Self::ShinR),
        }
    }

    #[must_use]
    pub const fn side(self) -> Side {
        match self {
            Self::Root | Self::Spine | Self::Chest | Self::Head => Side::Centre,
            Self::UpperArmL
            | Self::ForearmL
            | Self::HandL
            | Self::ThighL
            | Self::ShinL
            | Self::FootL => Side::Left,
            Self::UpperArmR
            | Self::ForearmR
            | Self::HandR
            | Self::ThighR
            | Self::ShinR
            | Self::FootR => Side::Right,
        }
    }

    /// The same bone on the other side of the body.
    #[must_use]
    pub const fn mirrored(self) -> Self {
        match self {
            Self::UpperArmL => Self::UpperArmR,
            Self::UpperArmR => Self::UpperArmL,
            Self::ForearmL => Self::ForearmR,
            Self::ForearmR => Self::ForearmL,
            Self::HandL => Self::HandR,
            Self::HandR => Self::HandL,
            Self::ThighL => Self::ThighR,
            Self::ThighR => Self::ThighL,
            Self::ShinL => Self::ShinR,
            Self::ShinR => Self::ShinL,
            Self::FootL => Self::FootR,
            Self::FootR => Self::FootL,
            other => other,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Spine => "spine",
            Self::Chest => "chest",
            Self::Head => "head",
            Self::UpperArmL => "upper-arm-l",
            Self::ForearmL => "forearm-l",
            Self::HandL => "hand-l",
            Self::UpperArmR => "upper-arm-r",
            Self::ForearmR => "forearm-r",
            Self::HandR => "hand-r",
            Self::ThighL => "thigh-l",
            Self::ShinL => "shin-l",
            Self::FootL => "foot-l",
            Self::ThighR => "thigh-r",
            Self::ShinR => "shin-r",
            Self::FootR => "foot-r",
        }
    }
}

/// A rigid transform: rotation then translation, with no scale.
///
/// Rigid on purpose. A character built from rigid voxel parts must never
/// stretch a bone, and a transform type that cannot express a scale is how
/// that is guaranteed rather than asserted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub rotation: Quat,
    pub translation: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        rotation: Quat::IDENTITY,
        translation: Vec3::ZERO,
    };

    #[must_use]
    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            rotation: Quat::IDENTITY,
            translation,
        }
    }

    #[must_use]
    pub const fn new(rotation: Quat, translation: Vec3) -> Self {
        Self {
            rotation,
            translation,
        }
    }

    /// `self` applied after `child`: the usual parent-times-child order.
    #[must_use]
    pub fn compose(self, child: Self) -> Self {
        Self {
            rotation: self.rotation * child.rotation,
            translation: self.translation + self.rotation * child.translation,
        }
    }

    #[must_use]
    pub fn transform_point(self, point: Vec3) -> Vec3 {
        self.translation + self.rotation * point
    }

    /// The rotation alone, applied to a direction.
    #[must_use]
    pub fn transform_direction(self, direction: Vec3) -> Vec3 {
        self.rotation * direction
    }

    #[must_use]
    pub fn to_mat4(self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation, self.translation)
    }

    /// Whether every component is finite, which every pose stage asserts.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.translation.is_finite()
            && self.rotation.x.is_finite()
            && self.rotation.y.is_finite()
            && self.rotation.z.is_finite()
            && self.rotation.w.is_finite()
            && (self.rotation.length_squared() - 1.0).abs() < 1.0e-3
    }
}

/// One bone's fixed description.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bone {
    pub id: BoneId,
    pub parent: Option<BoneId>,
    /// Rest transform relative to the parent, in character voxels.
    pub rest_local: Transform,
}

/// The compiled skeleton: sixteen bones and their rest pose.
///
/// Everything here is in **character voxel units** with the ground at `y = 0`
/// and the body facing `-Z`, the same direction the client's camera faces at
/// zero yaw. One scale at the root is what turns voxels into world units, so
/// nothing inside the skeleton has to know about world scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skeleton {
    bones: [Bone; BONE_COUNT],
}

impl Skeleton {
    /// Derives the skeleton from a validated body.
    ///
    /// Every offset comes from [`BodyMetrics`]; there is no bone position in
    /// this function that is not a measurement of the body it belongs to.
    #[must_use]
    pub fn derive(body: &BodyMetrics) -> Self {
        let roll = REST_ARM_ROLL_DEGREES.to_radians();
        let half_limb = body.limb_thickness as f32 * 0.5;
        let shoulder_x = body.arm_inner as f32 + half_limb;
        let hip_x = body.leg_inner as f32 + half_limb;
        let up = |value: i32| Vec3::new(0.0, value as f32, 0.0);
        let down = |value: i32| Vec3::new(0.0, -(value as f32), 0.0);

        let arm_rest = |side: Side| {
            Transform::new(
                Quat::from_rotation_z(roll * side.sign()),
                Vec3::new(
                    shoulder_x * side.sign(),
                    (body.shoulder_y - body.chest_y) as f32,
                    0.0,
                ),
            )
        };
        let thigh_rest =
            |side: Side| Transform::from_translation(Vec3::new(hip_x * side.sign(), 0.0, 0.0));

        let mut bones = [Bone {
            id: BoneId::Root,
            parent: None,
            rest_local: Transform::IDENTITY,
        }; BONE_COUNT];
        for id in ALL_BONES {
            let rest_local = match id {
                BoneId::Root => Transform::from_translation(up(body.hip_y)),
                BoneId::Spine => Transform::from_translation(up(body.spine_y - body.hip_y)),
                BoneId::Chest => Transform::from_translation(up(body.chest_y - body.spine_y)),
                BoneId::Head => Transform::from_translation(up(body.neck_y - body.chest_y)),
                BoneId::UpperArmL => arm_rest(Side::Left),
                BoneId::UpperArmR => arm_rest(Side::Right),
                BoneId::ForearmL | BoneId::ForearmR => {
                    Transform::from_translation(down(body.upper_arm_length))
                }
                BoneId::HandL | BoneId::HandR => {
                    Transform::from_translation(down(body.forearm_length))
                }
                BoneId::ThighL => thigh_rest(Side::Left),
                BoneId::ThighR => thigh_rest(Side::Right),
                BoneId::ShinL | BoneId::ShinR => {
                    Transform::from_translation(down(body.thigh_length))
                }
                BoneId::FootL | BoneId::FootR => {
                    Transform::from_translation(down(body.knee_y - body.ankle_y))
                }
            };
            bones[id.index()] = Bone {
                id,
                parent: id.parent(),
                rest_local,
            };
        }
        Self { bones }
    }

    #[must_use]
    pub const fn bones(&self) -> &[Bone; BONE_COUNT] {
        &self.bones
    }

    #[must_use]
    pub const fn bone(&self, id: BoneId) -> &Bone {
        &self.bones[id.index()]
    }

    /// Composes world transforms from per-bone local transforms.
    ///
    /// One forward pass, which is correct only because the bones are stored in
    /// topological order. `root` is the transform of the whole character.
    pub fn compose_into(
        &self,
        root: Transform,
        locals: &[Transform; BONE_COUNT],
        out: &mut [Transform; BONE_COUNT],
    ) {
        for bone in &self.bones {
            let parent = match bone.parent {
                Some(parent) => out[parent.index()],
                None => root,
            };
            out[bone.id.index()] = parent.compose(locals[bone.id.index()]);
        }
    }

    /// The rest pose's local transforms.
    #[must_use]
    pub fn rest_locals(&self) -> [Transform; BONE_COUNT] {
        let mut locals = [Transform::IDENTITY; BONE_COUNT];
        for bone in &self.bones {
            locals[bone.id.index()] = bone.rest_local;
        }
        locals
    }

    /// World transforms of the rest pose, with the character at the origin.
    #[must_use]
    pub fn rest_world(&self) -> [Transform; BONE_COUNT] {
        let mut out = [Transform::IDENTITY; BONE_COUNT];
        self.compose_into(Transform::IDENTITY, &self.rest_locals(), &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{ALL_BONES, BONE_COUNT, BoneId, Side, Skeleton, Transform};
    use crate::descriptor::CharacterDescriptor;
    use glam::Vec3;
    use std::collections::BTreeSet;

    fn skeleton() -> Skeleton {
        let validated = match CharacterDescriptor::golden().validate() {
            Ok(validated) => validated,
            Err(error) => panic!("the golden descriptor must validate: {error}"),
        };
        Skeleton::derive(validated.body())
    }

    #[test]
    fn the_hierarchy_has_one_root_a_unique_parent_and_no_cycle() {
        let skeleton = skeleton();
        let roots: Vec<BoneId> = ALL_BONES
            .into_iter()
            .filter(|id| id.parent().is_none())
            .collect();
        assert_eq!(roots, vec![BoneId::Root], "there must be exactly one root");

        for id in ALL_BONES {
            // Walking to the root must terminate, which is the acyclicity
            // proof; a cycle would exhaust the bounded step count.
            let mut cursor = Some(id);
            let mut steps = 0;
            while let Some(current) = cursor {
                cursor = current.parent();
                steps += 1;
                assert!(steps <= BONE_COUNT, "{} sits on a cycle", id.name());
            }
        }
        assert_eq!(skeleton.bones().len(), BONE_COUNT);
    }

    #[test]
    fn bones_are_stored_in_topological_order() {
        for id in ALL_BONES {
            if let Some(parent) = id.parent() {
                assert!(
                    parent.index() < id.index(),
                    "{} precedes its parent {}",
                    id.name(),
                    parent.name()
                );
            }
        }
        for (index, id) in ALL_BONES.into_iter().enumerate() {
            assert_eq!(index, id.index(), "{} is stored out of place", id.name());
        }
    }

    #[test]
    fn bone_names_are_unique_and_the_count_is_bounded() {
        let names: BTreeSet<&str> = ALL_BONES.iter().map(|id| id.name()).collect();
        assert_eq!(names.len(), BONE_COUNT);
        assert_eq!(BONE_COUNT, 16, "the bone budget is a milestone decision");
    }

    #[test]
    fn every_rest_transform_is_finite_and_rigid() {
        let skeleton = skeleton();
        for bone in skeleton.bones() {
            assert!(
                bone.rest_local.is_finite(),
                "{} has a non-rigid rest transform",
                bone.id.name()
            );
        }
        for transform in skeleton.rest_world() {
            assert!(transform.is_finite());
        }
    }

    #[test]
    fn the_rest_pose_places_the_joints_where_the_body_says() {
        let validated = match CharacterDescriptor::golden().validate() {
            Ok(validated) => validated,
            Err(error) => panic!("{error}"),
        };
        let body = validated.body();
        let world = Skeleton::derive(body).rest_world();
        let height_of = |id: BoneId| world[id.index()].translation.y;
        assert!((height_of(BoneId::Root) - body.hip_y as f32).abs() < 1.0e-4);
        assert!((height_of(BoneId::Spine) - body.spine_y as f32).abs() < 1.0e-4);
        assert!((height_of(BoneId::Chest) - body.chest_y as f32).abs() < 1.0e-4);
        assert!((height_of(BoneId::Head) - body.neck_y as f32).abs() < 1.0e-4);
        assert!((height_of(BoneId::ThighR) - body.hip_y as f32).abs() < 1.0e-4);
        assert!((height_of(BoneId::ShinR) - body.knee_y as f32).abs() < 1.0e-4);
        assert!((height_of(BoneId::FootR) - body.ankle_y as f32).abs() < 1.0e-4);
        // The arms carry a rest roll, so their joints are not exactly on the
        // nominal heights; the shoulder itself still is.
        assert!((height_of(BoneId::UpperArmR) - body.shoulder_y as f32).abs() < 1.0e-4);
    }

    #[test]
    fn the_rest_pose_is_mirror_symmetric() {
        let world = skeleton().rest_world();
        for id in ALL_BONES {
            if id.side() == Side::Centre {
                assert!(
                    world[id.index()].translation.x.abs() < 1.0e-4,
                    "{} sits off the midline",
                    id.name()
                );
                continue;
            }
            let mine = world[id.index()];
            let theirs = world[id.mirrored().index()];
            assert!(
                (mine.translation.x + theirs.translation.x).abs() < 1.0e-4,
                "{} does not mirror in x",
                id.name()
            );
            assert!(
                (mine.translation.y - theirs.translation.y).abs() < 1.0e-4,
                "{} does not mirror in y",
                id.name()
            );
            assert!(
                (mine.translation.z - theirs.translation.z).abs() < 1.0e-4,
                "{} does not mirror in z",
                id.name()
            );
        }
    }

    #[test]
    fn the_arms_hang_outward_rather_than_fused_to_the_torso() {
        let world = skeleton().rest_world();
        let shoulder = world[BoneId::UpperArmR.index()].translation;
        let wrist = world[BoneId::HandR.index()].translation;
        assert!(
            wrist.x > shoulder.x,
            "the right arm does not splay outward: {shoulder} -> {wrist}"
        );
        assert!(wrist.y < shoulder.y, "the right arm does not hang down");
    }

    #[test]
    fn composition_is_a_single_forward_pass_that_matches_manual_chaining() {
        let skeleton = skeleton();
        let locals = skeleton.rest_locals();
        let mut out = [Transform::IDENTITY; BONE_COUNT];
        let root = Transform::from_translation(Vec3::new(3.0, -2.0, 7.0));
        skeleton.compose_into(root, &locals, &mut out);

        for id in ALL_BONES {
            let mut manual = locals[id.index()];
            let mut cursor = id.parent();
            while let Some(parent) = cursor {
                manual = locals[parent.index()].compose(manual);
                cursor = parent.parent();
            }
            manual = root.compose(manual);
            let found = out[id.index()];
            assert!(
                (found.translation - manual.translation).length() < 1.0e-4,
                "{} disagrees with manual chaining",
                id.name()
            );
        }
    }

    #[test]
    fn bone_sides_mirror_and_the_midline_is_its_own_opposite() {
        for id in ALL_BONES {
            assert_eq!(id.mirrored().mirrored(), id);
            assert_eq!(id.side().opposite(), id.mirrored().side());
        }
        assert_eq!(Side::Centre.opposite(), Side::Centre);
        assert_eq!(Side::Left.sign(), -1.0);
        assert_eq!(Side::Right.sign(), 1.0);
        assert_eq!(Side::Centre.sign(), 0.0);
    }
}
