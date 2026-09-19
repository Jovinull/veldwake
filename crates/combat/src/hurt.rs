//! The volume a blade has to touch to hurt a body.
//!
//! M5 already derives a [`BodyCapsule`] that contains **every voxel of the rest
//! pose**, arms and hands included. That is the right volume for keeping two
//! bodies from standing inside each other, and it is the wrong volume for
//! deciding a hit. A capture proved it rather than an argument: with arms
//! counted, the adversary's capsule came out `0.8274` wide on a body `2.75`
//! tall and reached from `0.52` below the ground to `0.03` above the top of its
//! head, so the blade was inside it for the whole windup and the hit registered
//! with the sword still raised over the shoulder, its tip in the air above the
//! head. The frame showed no contact and the rules said a hit landed.
//!
//! So the hurt volume is derived here, from the **torso column** only: pelvis,
//! spine, chest and head. Everything that swings — both arms, both legs below
//! the hip, the feet and the weapon — is outside it, and a cut that only reaches
//! an outstretched arm or a trailing shin therefore does not register.
//!
//! Three rounds of measurement chose that set, and each one removed something.
//! The arms went first: they are what made the volume as wide as the hands
//! reach. Then the shins, which a striding body throws `0.30` world units clear
//! of any capsule fitted to it standing still. Then the thighs, for `0.35`. A
//! single upright capsule that holds a leg at the top of its stride is `0.83`
//! wide on a body `0.50` through the chest, which is M5's capsule again by
//! another route: fitting it to the current pose rather than the rest pose does
//! not help, because the width is the stride's, not the fit's. The torso column
//! is the widest core that stays the width of the body, and it is what the blade
//! meets in any case — the active window sweeps a height band of `1.14` to
//! `2.84` above the ground, which is chest and head.
//!
//! The volume is still fitted to the pose the body is **currently** in, once per
//! tick, because a torso leans and twists through a swing and a volume measured
//! from the still pose would be a tick or more behind the body it describes.
//! Four bones' worth of corners per body per tick, thirty-two points.
//!
//! `the_hurt_capsule_contains_the_core_of_the_body_at_every_moment_of_a_fight`
//! in [`crate::encounter`] holds the other side of the bargain: what is in the
//! core may not leave the volume.

use glam::Vec3;

use glam::Mat4;

use veldwake_character::collision::{BodyCapsule, CollisionRepresentation};
use veldwake_character::descriptor::CHARACTER_VOXEL_SIZE;
use veldwake_character::skeleton::{BONE_COUNT, BoneId};
use veldwake_character::{CharacterState, CompiledCharacter, pose};

use crate::hit::{Capsule, Segment};

/// The bones a hit is decided against.
///
/// Four of sixteen: pelvis, spine, chest, head. Every limb is out, and every
/// one of them was measured out rather than argued out — see the module
/// documentation for the three rounds and the numbers. What is left is the part
/// of a humanoid that does not swing, which is the only part a single upright
/// capsule can wrap without becoming a barrel.
pub const HURT_CORE: [BoneId; 4] = [BoneId::Root, BoneId::Spine, BoneId::Chest, BoneId::Head];

/// How much slack the hurt capsule keeps around the core it was measured from,
/// in world units.
///
/// One character voxel, and an epsilon rather than an allowance. The volume is
/// refitted every tick, so it has no lean or stride to absorb; what it does have
/// is corners sitting exactly on its own surface, where a containment test is a
/// floating-point coin toss. The margin decides them, and it is a whole voxel
/// rather than something smaller only because a voxel is the unit this body is
/// built from and is still invisible at this scale. The residual overshoot is
/// printed by `combat-probe bodies` and asserted by
/// `the_hurt_capsule_contains_the_core_of_the_body_at_every_moment_of_a_fight`.
pub const HURT_MARGIN: f32 = CHARACTER_VOXEL_SIZE;

/// An upright capsule around the core of one body, relative to the point it
/// stands on.
///
/// The same three numbers as a [`BodyCapsule`], and deliberately the same shape,
/// so the two volumes can be compared in one table. It is a distinct type
/// because confusing the two is exactly the mistake this module exists to undo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HurtVolume {
    radius: f32,
    base_height: f32,
    segment_height: f32,
}

impl HurtVolume {
    /// The volume of one compiled body standing still.
    ///
    /// The reference figure: what a body's core measures with no action and no
    /// gait, which is the number a document or a table can quote. The volume a
    /// hit is decided against is [`Self::from_pose`], refitted every tick. The
    /// body is posed at the origin with no ground, so the matrices come out
    /// already relative to its stand point, and the pose path measured is the
    /// one the renderer draws with — a capsule derived from the skeleton's rest
    /// transforms instead would describe a body nobody ever sees.
    ///
    /// The radius is the widest the core ever gets from the vertical axis, plus
    /// [`HURT_MARGIN`]. The cap centres are then **inset** by that radius from
    /// the core's lowest and highest points, so the capsule's outer band is
    /// exactly the core's vertical extent and not a finger's width more.
    ///
    /// That is the deliberate half of the trade, and M5's whole-body capsule
    /// makes the other choice: it solves the cap centres so that every corner
    /// satisfies the capsule's own containment test, which is exact but pushes
    /// the caps out past the body — a point at horizontal distance `h` from the
    /// axis needs its cap centre within `sqrt(r^2 - h^2)` of it vertically, so
    /// the top of the capsule ends up as much as `r` above the top of the head.
    /// A volume that reaches above the head takes hits in the air above it,
    /// which is the exact defect this module exists to undo, so the band wins
    /// and containment becomes approximate: the head and the ankles are the
    /// narrow ends of a humanoid, so their rim corners fall outside by well
    /// under one voxel. `combat-probe bodies` prints how far, every tick of the
    /// reference fight, and
    /// `the_hurt_capsule_contains_the_core_of_the_body_at_every_moment_of_a_fight`
    /// fails if it ever exceeds one.
    #[must_use]
    pub fn derive(character: &CompiledCharacter) -> Self {
        let state = CharacterState::default();
        let posed = pose(character, &state, None);
        Self::from_pose(
            character.collision(),
            posed.part_matrices(),
            state.stand_point(),
        )
    }

    /// Fits the volume to the pose a body is in now.
    ///
    /// `matrices` are the same part matrices the renderer draws with and `stand`
    /// is the point the body stands on, so the result is in that body's own
    /// stand-relative frame and [`Self::placed`] puts it back.
    #[must_use]
    pub fn from_pose(
        collision: &CollisionRepresentation,
        matrices: &[Mat4; BONE_COUNT],
        stand: Vec3,
    ) -> Self {
        let mut radius_squared = 0.0_f32;
        let mut core_low = f32::INFINITY;
        let mut core_high = f32::NEG_INFINITY;
        for bone in HURT_CORE {
            let matrix = matrices[bone.index()];
            for corner in collision.part_box(bone).corners() {
                let point = matrix.transform_point3(corner) - stand;
                radius_squared = radius_squared.max(point.x.mul_add(point.x, point.z * point.z));
                core_low = core_low.min(point.y);
                core_high = core_high.max(point.y);
            }
        }
        let radius = radius_squared.sqrt().max(1.0e-4) + HURT_MARGIN;

        // Second pass, and the one that makes containment exact rather than
        // approximate: a corner at horizontal distance `h` from the axis needs
        // its cap centre within `sqrt(r^2 - h^2)` of it vertically, so the caps
        // go wherever every corner's constraint is satisfied at once. Clamping
        // the caps to the core's own extent instead would keep the volume
        // strictly inside the body, and measurement said no: it left a thigh
        // corner `0.12` world units outside, half again the margin, while the
        // containment solve costs only that the top of the volume sits about
        // `0.10` above the crown of a cubic head. A volume the body can leave is
        // worse than one that clears the head by an eighth of a voxel-and-a-bit,
        // because the first drops hits the player watched land.
        let mut cap_low = f32::INFINITY;
        let mut cap_high = f32::NEG_INFINITY;
        for bone in HURT_CORE {
            let matrix = matrices[bone.index()];
            for corner in collision.part_box(bone).corners() {
                let point = matrix.transform_point3(corner) - stand;
                let horizontal_squared = point.x.mul_add(point.x, point.z * point.z);
                let reach = radius.mul_add(radius, -horizontal_squared).max(0.0).sqrt();
                cap_low = cap_low.min(point.y + reach);
                cap_high = cap_high.max(point.y - reach);
            }
        }
        let base_height = cap_low;
        let cap_high = cap_high.max(base_height);
        Self {
            radius,
            base_height,
            segment_height: cap_high - base_height,
        }
    }

    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    /// Height of the lower cap's centre above the ground. It can be negative:
    /// the core reaches the ankle, which is less than one radius up.
    #[must_use]
    pub const fn base_height(&self) -> f32 {
        self.base_height
    }

    #[must_use]
    pub const fn segment_height(&self) -> f32 {
        self.segment_height
    }

    /// The lowest and highest points of the volume above the ground.
    #[must_use]
    pub fn band(&self) -> (f32, f32) {
        (
            self.base_height - self.radius,
            self.base_height + self.segment_height + self.radius,
        )
    }

    /// The volume placed on a body standing at `stand`.
    #[must_use]
    pub fn placed(&self, stand: Vec3) -> Capsule {
        let low = stand + Vec3::Y * self.base_height;
        let high = low + Vec3::Y * self.segment_height;
        Capsule::new(Segment::new(low, high), self.radius)
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.radius.is_finite()
            && self.radius > 0.0
            && self.base_height.is_finite()
            && self.segment_height.is_finite()
            && self.segment_height >= 0.0
    }
}

/// How much narrower the hurt volume is than the whole-body capsule beside it.
///
/// A ratio rather than a pass: it is the number that says whether the two
/// volumes are still doing two different jobs.
#[must_use]
pub fn narrowing(hurt: &HurtVolume, body: BodyCapsule) -> f32 {
    hurt.radius / body.radius
}

#[cfg(test)]
mod tests {
    use super::{HURT_CORE, HURT_MARGIN, HurtVolume, narrowing};
    use crate::fixture;
    use veldwake_character::descriptor::CHARACTER_VOXEL_SIZE;
    use veldwake_character::skeleton::BoneId;
    use veldwake_character::{CharacterCompiler, CharacterState, CompiledCharacter, pose};

    fn bodies() -> [CompiledCharacter; 2] {
        let setup = fixture::golden_setup();
        let mut compiler = CharacterCompiler::new();
        let player = match compiler.compile_descriptor(&setup.player) {
            Ok(character) => character,
            Err(error) => panic!("{error}"),
        };
        let adversary = match compiler.compile_descriptor(&setup.adversary) {
            Ok(character) => character,
            Err(error) => panic!("{error}"),
        };
        [player, adversary]
    }

    #[test]
    fn the_hurt_volume_is_finite_and_upright() {
        for body in bodies() {
            let hurt = HurtVolume::derive(&body);
            assert!(hurt.is_finite(), "{hurt:?}");
            let (low, high) = hurt.band();
            assert!(low < high, "{low} is not below {high}");
        }
    }

    #[test]
    fn the_hurt_volume_is_no_wider_than_the_silhouette_it_sits_in() {
        // The oracle is the descriptor, not the thing being measured: a body is
        // never wider than its own shoulder span, arms included, so a volume
        // inside the silhouette cannot be wider than half of it. This is the
        // assertion that would have caught the original defect, where the
        // volume was as wide as the hands reach.
        for body in bodies() {
            let hurt = HurtVolume::derive(&body);
            let metrics = body.body();
            let half_span =
                f64::from(metrics.shoulder_span) / 2.0 * f64::from(CHARACTER_VOXEL_SIZE);
            let half_waist = f64::from(metrics.waist_width) / 2.0 * f64::from(CHARACTER_VOXEL_SIZE);
            let radius = f64::from(hurt.radius());
            assert!(
                radius <= half_span + f64::from(HURT_MARGIN),
                "the hurt volume is {radius:.4} wide inside a {half_span:.4} silhouette"
            );
            assert!(
                radius >= half_waist,
                "the hurt volume is {radius:.4} wide around a {half_waist:.4} waist"
            );
        }
    }

    #[test]
    fn the_hurt_volume_is_narrower_than_the_whole_body_capsule() {
        // Two volumes doing two jobs. If these ever came back the same width,
        // the derivation would have gone back to measuring arms.
        for body in bodies() {
            let hurt = HurtVolume::derive(&body);
            let capsule = body.collision().capsule();
            let ratio = narrowing(&hurt, capsule);
            assert!(
                ratio < 1.0,
                "the hurt volume is {ratio:.4} of the body capsule's width, \
                 hurt radius {:.4}, body radius {:.4}",
                hurt.radius(),
                capsule.radius
            );
        }
    }

    #[test]
    fn the_hurt_volume_stands_clear_of_the_ground_and_the_crown() {
        // The whole-body capsule's lower cap bulges half a world unit below the
        // ground, because it is wide enough to hold the arms and a wide capsule
        // has deep caps: a blade dragged through the turf in front of a body
        // would find it. The torso volume starts at the hip instead, and its top
        // clears the crown by less than its own radius, which is the
        // unavoidable cost of wrapping a cubic head in a capsule.
        for body in bodies() {
            let hurt = HurtVolume::derive(&body);
            let metrics = body.body();
            let height = metrics.height_units();
            let hip = f32::from(i16::try_from(metrics.hip_y).unwrap_or(0)) * CHARACTER_VOXEL_SIZE;
            let (low, high) = hurt.band();
            assert!(
                low > 0.0,
                "the hurt volume reaches {low:.4} below the ground"
            );
            assert!(
                low <= hip,
                "the hurt volume starts at {low:.4}, above the {hip:.4} hip it is supposed to cover"
            );
            assert!(
                high <= height + hurt.radius(),
                "the hurt volume reaches {high:.4} on a body {height:.4} tall"
            );
        }
    }

    #[test]
    fn a_refit_volume_follows_the_body_that_moved() {
        // The reason the volume is refitted every tick rather than measured
        // once. A body standing at one place and the same body standing four
        // units away must produce the same stand-relative volume, and a body
        // mid-stride must not produce the still one.
        for body in bodies() {
            let here = CharacterState::default();
            let there = CharacterState {
                x: 4.0,
                z: -7.0,
                ..CharacterState::default()
            };
            let striding = CharacterState {
                speed: 2.0,
                phase: 0.25,
                ..CharacterState::default()
            };
            let fit = |state: &CharacterState| {
                let posed = pose(&body, state, None);
                HurtVolume::from_pose(body.collision(), posed.part_matrices(), state.stand_point())
            };
            let (here, there, striding) = (fit(&here), fit(&there), fit(&striding));
            // Close rather than equal: the part matrices carry the world
            // position, so subtracting the stand point back off a body seven
            // units away costs the last digit or two of an `f32`. A tenth of a
            // millimetre of drift is the honest claim; bit equality is not.
            let tolerance = 1.0e-4;
            for (mine, theirs, name) in [
                (here.radius(), there.radius(), "radius"),
                (here.base_height(), there.base_height(), "base height"),
                (
                    here.segment_height(),
                    there.segment_height(),
                    "segment height",
                ),
            ] {
                assert!(
                    (mine - theirs).abs() <= tolerance,
                    "the volume's {name} moved with the world: {mine} against {theirs}"
                );
            }
            assert!(
                (here.segment_height() - striding.segment_height()).abs() > tolerance
                    || (here.base_height() - striding.base_height()).abs() > tolerance
                    || (here.radius() - striding.radius()).abs() > tolerance,
                "the volume ignored a stride, so it was not refitted"
            );
        }
    }

    #[test]
    fn the_core_is_the_body_without_its_arms_or_feet() {
        // The accepted limitation, written down as a test so that quietly
        // adding a hand to the core has to be a deliberate edit.
        for bone in [
            BoneId::UpperArmL,
            BoneId::ForearmL,
            BoneId::HandL,
            BoneId::UpperArmR,
            BoneId::ForearmR,
            BoneId::HandR,
            BoneId::ThighL,
            BoneId::ThighR,
            BoneId::ShinL,
            BoneId::ShinR,
            BoneId::FootL,
            BoneId::FootR,
        ] {
            assert!(!HURT_CORE.contains(&bone), "{} is in the core", bone.name());
        }
        assert_eq!(HURT_CORE.len(), 4);
    }
}
