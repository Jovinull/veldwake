//! The M5 fixtures: named characters, named poses, and locked signatures.
//!
//! These exist so that an intentional change to a character is a conscious
//! decision rather than a surprise, and they follow the shape M4 established
//! for terrain:
//!
//! - **named descriptors** lock intent: a reference humanoid, a deliberately
//!   different body, and a varied one. Two bodies are what prove a compiler
//!   exists rather than a hard-coded model.
//! - **fingerprints** lock exact output for geometry, the skeleton, and the
//!   collision representation, separately, so a failure says which one moved.
//! - **named poses** lock the states the captures are taken at, so two
//!   screenshots months apart are comparable.
//!
//! No pixel comparison is an oracle anywhere: a driver update would break one
//! without a voxel moving.

use crate::action::{ActionKind, ActionOverlay};
use crate::compiler::{CharacterCompiler, CharacterError, CompiledCharacter, behaviour_signature};
use crate::descriptor::{CharacterDescriptor, CharacterSeed, Proportions};
use crate::hash::{fnv1a64, push_f32, push_u32};
use crate::material::{GarmentScheme, HairTone, PaletteChoice, SkinTone};
use crate::pose::{CharacterState, PosedCharacter, pose, pose_with};
use crate::skeleton::{BONE_COUNT, Side};

/// The reference humanoid every capture and every recorded number refers to.
#[must_use]
pub fn golden_descriptor() -> CharacterDescriptor {
    CharacterDescriptor::golden()
}

/// A deliberately different body: shorter, heavier, broader, differently
/// dressed. Its purpose is to fail if the compiler stops being a compiler.
#[must_use]
pub fn sturdy_descriptor() -> CharacterDescriptor {
    CharacterDescriptor {
        proportions: Proportions::sturdy(),
        seed: CharacterSeed(0x5354_5552_4459_0001),
        palette: PaletteChoice {
            skin: SkinTone::Deep,
            hair: HairTone::Dark,
            garment: GarmentScheme::MossWool,
        },
        ..CharacterDescriptor::golden()
    }
}

/// The golden proportions under bounded seed variation.
#[must_use]
pub fn varied_descriptor() -> CharacterDescriptor {
    CharacterDescriptor {
        seed: CharacterSeed(0x5641_5249_4544_0001),
        build_variation: 0.06,
        palette: PaletteChoice {
            skin: SkinTone::Fair,
            hair: HairTone::Flaxen,
            garment: GarmentScheme::SlateWool,
        },
        ..CharacterDescriptor::golden()
    }
}

/// Compiles the golden humanoid.
pub fn golden_character() -> Result<CompiledCharacter, CharacterError> {
    CharacterCompiler::new().compile_descriptor(&golden_descriptor())
}

/// A pose worth photographing, named so a capture can be reproduced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedPose {
    pub name: &'static str,
    /// World units per second.
    pub speed: f32,
    /// Cycle position in `[0, 1)`.
    pub phase: f32,
    /// Seconds, for the idle breath.
    pub time: f32,
    /// What this pose is meant to show, so a capture is judged against an
    /// intention rather than against taste.
    pub intent: &'static str,
}

/// Every pose the visual evidence is captured at.
pub const NAMED_POSES: &[NamedPose] = &[
    NamedPose {
        name: "rest",
        speed: 0.0,
        phase: 0.0,
        time: 0.0,
        intent: "the measurement pose: symmetric, arms hanging, what the fixtures lock",
    },
    NamedPose {
        name: "idle-a",
        speed: 0.0,
        phase: 0.0,
        time: 0.9,
        intent: "breathing in, weight shifted; proves a standing character is not a statue",
    },
    NamedPose {
        name: "idle-b",
        speed: 0.0,
        phase: 0.0,
        time: 2.7,
        intent: "breathing out; the pair with idle-a shows movement without drift",
    },
    NamedPose {
        name: "walk-contact",
        speed: 1.5,
        phase: 0.0,
        time: 4.0,
        intent: "heel strike: the leg is forward, the opposite arm is forward",
    },
    NamedPose {
        name: "walk-passing",
        speed: 1.5,
        phase: 0.25,
        time: 4.2,
        intent: "mid stance: the swing leg passes the stance leg and the pelvis is high",
    },
    NamedPose {
        name: "walk-push",
        speed: 1.5,
        phase: 0.5,
        time: 4.4,
        intent: "toe off on the opposite side; the mirror of walk-contact",
    },
    NamedPose {
        name: "run-contact",
        speed: 3.8,
        phase: 0.0,
        time: 6.0,
        intent: "the run's contact: deeper knee, longer stride, forward lean",
    },
    NamedPose {
        name: "run-flight",
        speed: 3.8,
        phase: 0.45,
        time: 6.2,
        intent: "the run's swing: the exaggeration the style contract allows",
    },
];

/// Eight equally spaced phases of one walk cycle, for a contact sheet.
#[must_use]
pub fn walk_cycle_phases() -> [f32; 8] {
    let mut phases = [0.0_f32; 8];
    for (index, phase) in phases.iter_mut().enumerate() {
        *phase = index as f32 / 8.0;
    }
    phases
}

/// The state one named pose describes.
#[must_use]
pub fn state_for(pose: &NamedPose) -> CharacterState {
    CharacterState {
        speed: pose.speed,
        phase: pose.phase,
        time: pose.time,
        ..CharacterState::default()
    }
}

/// A compact hash of one posed character.
///
/// Locks what a pose actually produces: every bone transform, every contact,
/// and the blend. It is what makes "the walk changed" a failing test rather
/// than a thing somebody notices in a screenshot.
#[must_use]
pub fn pose_fingerprint(posed: &PosedCharacter) -> u64 {
    let mut bytes = Vec::with_capacity(BONE_COUNT * 32);
    for transform in posed.bone_world() {
        for value in [
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            transform.rotation.x,
            transform.rotation.y,
            transform.rotation.z,
            transform.rotation.w,
        ] {
            push_f32(&mut bytes, value);
        }
    }
    push_f32(&mut bytes, posed.blend().moving);
    push_f32(&mut bytes, posed.blend().run);
    for contact in posed.contacts() {
        push_u32(&mut bytes, u32::from(contact.grounded));
        push_f32(&mut bytes, contact.stance);
    }
    fnv1a64(&bytes)
}

/// Fingerprint of every named pose of the golden humanoid, folded into one
/// number.
pub fn named_pose_signature(character: &CompiledCharacter) -> u64 {
    let mut bytes = Vec::with_capacity(NAMED_POSES.len() * 8);
    for named in NAMED_POSES {
        let state = state_for(named);
        let posed = pose(character, &state, None);
        bytes.extend_from_slice(&pose_fingerprint(&posed).to_le_bytes());
    }
    fnv1a64(&bytes)
}

/// Locked hash of the golden humanoid's compiled surface.
pub const GOLDEN_GEOMETRY_FINGERPRINT: u64 = 0x3ebe_8c82_2f54_9151;
/// Locked hash of the golden humanoid's bone hierarchy and rest pose.
pub const GOLDEN_SKELETON_FINGERPRINT: u64 = 0x2628_aeb4_1d29_79ed;
/// Locked hash of the golden humanoid's collision representation.
pub const GOLDEN_COLLISION_FINGERPRINT: u64 = 0x8181_56de_8637_d934;
/// Locked hash of everything observable about the golden humanoid.
pub const GOLDEN_BEHAVIOUR_SIGNATURE: u64 = 0x6ca7_52c7_0a5b_f919;
/// Locked hash of every named pose of the golden humanoid.
pub const GOLDEN_POSE_SIGNATURE: u64 = 0xfd1e_1f61_ba17_37d2;
/// Locked hash of everything observable about the sturdy humanoid.
pub const STURDY_BEHAVIOUR_SIGNATURE: u64 = 0xb7d6_7add_b984_5142;
/// Locked hash of everything observable about the varied humanoid.
pub const VARIED_BEHAVIOUR_SIGNATURE: u64 = 0xa931_d3c2_ede0_d752;
/// Locked hash of every named action pose of the golden humanoid.
///
/// Added by M6 alongside the action layer. It is deliberately a **new** constant
/// rather than a re-lock of anything above: `pose()` is unchanged, so every M5
/// signature is unchanged, and the art rules the curves answer to live in
/// `docs/audiovisual/COMBAT_STYLE.md` under its own version. Putting them under
/// `CHARACTER_STYLE_VERSION` would have invalidated the identity of three
/// compiled bodies to add a rule that moves no voxel.
pub const GOLDEN_ACTION_POSE_SIGNATURE: u64 = 0xd736_e072_31cb_d790;

/// Every locked fixture value, for the probe to print in one place.
#[must_use]
pub fn locked_values() -> [(&'static str, u64); 8] {
    [
        ("golden geometry", GOLDEN_GEOMETRY_FINGERPRINT),
        ("golden skeleton", GOLDEN_SKELETON_FINGERPRINT),
        ("golden collision", GOLDEN_COLLISION_FINGERPRINT),
        ("golden behaviour", GOLDEN_BEHAVIOUR_SIGNATURE),
        ("golden poses", GOLDEN_POSE_SIGNATURE),
        ("golden action poses", GOLDEN_ACTION_POSE_SIGNATURE),
        ("sturdy behaviour", STURDY_BEHAVIOUR_SIGNATURE),
        ("varied behaviour", VARIED_BEHAVIOUR_SIGNATURE),
    ]
}

/// Computes every signature from scratch; the probe prints these next to the
/// locked ones so a mismatch is one line to read.
pub fn measured_values() -> Result<[(&'static str, u64); 8], CharacterError> {
    let mut compiler = CharacterCompiler::new();
    let golden = compiler.compile_descriptor(&golden_descriptor())?;
    let sturdy = compiler.compile_descriptor(&sturdy_descriptor())?;
    let varied = compiler.compile_descriptor(&varied_descriptor())?;
    Ok([
        ("golden geometry", golden.geometry_fingerprint()),
        ("golden skeleton", golden.skeleton_fingerprint()),
        ("golden collision", golden.collision_fingerprint()),
        ("golden behaviour", behaviour_signature(&golden)),
        ("golden poses", named_pose_signature(&golden)),
        ("golden action poses", action_pose_signature(&golden)),
        ("sturdy behaviour", behaviour_signature(&sturdy)),
        ("varied behaviour", behaviour_signature(&varied)),
    ])
}

#[cfg(test)]
mod tests {
    use super::{
        NAMED_ACTION_POSES, NAMED_POSES, action_pose_signature, action_state_for, locked_values,
        measured_values, overlay_for, pose_fingerprint, state_for, walk_cycle_phases,
    };
    use crate::compiler::CharacterError;
    use crate::pose::{pose, pose_with};
    use std::collections::BTreeSet;

    fn measured(name: &str) -> Result<u64, CharacterError> {
        for (candidate, value) in measured_values()? {
            if candidate == name {
                return Ok(value);
            }
        }
        panic!("no measured value named {name}");
    }

    #[test]
    fn every_locked_signature_still_matches_what_the_compiler_produces()
    -> Result<(), CharacterError> {
        let measured = measured_values()?;
        let locked = locked_values();
        let mut mismatches = Vec::new();
        for (index, (name, value)) in measured.into_iter().enumerate() {
            let (locked_name, locked_value) = locked[index];
            assert_eq!(name, locked_name, "the fixture tables drifted apart");
            if value != locked_value {
                mismatches.push(format!(
                    "{name}: locked {locked_value:#018x} measured {value:#018x}"
                ));
            }
        }
        assert!(
            mismatches.is_empty(),
            "the compiled character changed. Update the locked values deliberately:\n{}",
            mismatches.join("\n")
        );
        Ok(())
    }

    #[test]
    fn named_poses_are_distinct_and_named_once() -> Result<(), CharacterError> {
        let names: BTreeSet<&str> = NAMED_POSES.iter().map(|pose| pose.name).collect();
        assert_eq!(names.len(), NAMED_POSES.len(), "a pose name repeats");
        assert!(NAMED_POSES.len() >= 6, "too few poses for real evidence");

        let character = super::golden_character()?;
        let mut fingerprints = BTreeSet::new();
        for named in NAMED_POSES {
            let posed = pose(&character, &state_for(named), None);
            assert!(posed.is_finite(), "{} is not finite", named.name);
            assert!(
                posed.angles().within_limits(),
                "{} leaves a joint range",
                named.name
            );
            assert!(!named.intent.is_empty());
            fingerprints.insert(pose_fingerprint(&posed));
        }
        assert_eq!(
            fingerprints.len(),
            NAMED_POSES.len(),
            "two named poses are the same pose"
        );
        Ok(())
    }

    #[test]
    fn the_contact_sheet_covers_one_whole_cycle() {
        let phases = walk_cycle_phases();
        assert_eq!(phases.len(), 8);
        assert_eq!(phases[0], 0.0);
        for pair in phases.windows(2) {
            assert!((pair[1] - pair[0] - 0.125).abs() < 1.0e-6);
        }
    }

    #[test]
    fn the_three_fixtures_are_three_different_characters() -> Result<(), CharacterError> {
        // Looked up by name rather than by index: the table grew when M6 added an
        // action signature, and an index would have silently compared the wrong
        // two fixtures.
        let golden = measured("golden behaviour")?;
        let sturdy = measured("sturdy behaviour")?;
        let varied = measured("varied behaviour")?;
        assert_ne!(golden, sturdy);
        assert_ne!(golden, varied);
        assert_ne!(sturdy, varied);
        Ok(())
    }

    #[test]
    fn named_action_poses_are_distinct_finite_and_inside_every_joint_range()
    -> Result<(), CharacterError> {
        let names: BTreeSet<&str> = NAMED_ACTION_POSES.iter().map(|named| named.name).collect();
        assert_eq!(
            names.len(),
            NAMED_ACTION_POSES.len(),
            "an action pose name repeats"
        );
        assert!(
            NAMED_ACTION_POSES.len() >= 8,
            "too few action poses for real evidence"
        );
        let character = super::golden_character()?;
        let mut fingerprints = BTreeSet::new();
        for named in NAMED_ACTION_POSES {
            let overlay = overlay_for(named);
            let posed = pose_with(&character, &action_state_for(named), None, Some(&overlay));
            assert!(posed.is_finite(), "{} is not finite", named.name);
            assert!(
                posed.angles().within_limits(),
                "{} leaves a joint range",
                named.name
            );
            assert!(
                !named.intent.is_empty(),
                "{} has no stated intent",
                named.name
            );
            fingerprints.insert(pose_fingerprint(&posed));
        }
        assert_eq!(
            fingerprints.len(),
            NAMED_ACTION_POSES.len(),
            "two named action poses are the same pose"
        );
        Ok(())
    }

    #[test]
    fn the_action_signature_is_reproducible_and_separate_from_the_locomotion_one()
    -> Result<(), CharacterError> {
        let character = super::golden_character()?;
        let first = action_pose_signature(&character);
        assert_eq!(first, action_pose_signature(&character), "not reproducible");
        assert_ne!(
            first,
            super::named_pose_signature(&character),
            "the two signatures must not be the same number"
        );
        // The action layer must not be able to change what `pose()` produces.
        let plain = pose(&character, &state_for(&NAMED_POSES[0]), None);
        let carried = pose_with(
            &character,
            &state_for(&NAMED_POSES[0]),
            None,
            Some(&overlay_for(&NAMED_ACTION_POSES[0])),
        );
        assert_ne!(
            pose_fingerprint(&plain),
            pose_fingerprint(&carried),
            "the carry pose must actually change the body"
        );
        Ok(())
    }
}

/// One action pose worth locking, named so a capture can be reproduced.
///
/// These are **new** constants rather than an extension of [`NAMED_POSES`]. The
/// M5 poses lock what `pose()` produces and must keep doing exactly that; making
/// them pretend they always covered combat would erase the evidence that the
/// locomotion output is unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedActionPose {
    pub name: &'static str,
    pub kind: ActionKind,
    /// Progress through the action, in `[0, 1]`.
    pub progress: f32,
    /// World units per second the body is travelling at.
    pub speed: f32,
    /// Direction of travel or of the incoming hit, relative to the facing.
    pub direction: f32,
    pub intent: &'static str,
}

/// Fractions of an attack spent in windup and in active, for the locked poses.
///
/// The golden tuning's own numbers, restated here so the character crate does
/// not depend on the combat crate to lock its own curves.
pub const LOCKED_ATTACK_WINDUP_FRACTION: f32 = 22.0 / 75.0;
/// See [`LOCKED_ATTACK_WINDUP_FRACTION`].
pub const LOCKED_ATTACK_ACTIVE_FRACTION: f32 = 12.0 / 75.0;

/// Every action pose the combat evidence is captured at.
pub const NAMED_ACTION_POSES: &[NamedActionPose] = &[
    NamedActionPose {
        name: "carry",
        kind: ActionKind::Carry,
        progress: 0.0,
        speed: 0.0,
        direction: 0.0,
        intent: "standing with the weapon ready: the pose that keeps a blade off the ground",
    },
    NamedActionPose {
        name: "carry-walk",
        kind: ActionKind::Carry,
        progress: 0.0,
        speed: 1.6,
        direction: 0.0,
        intent: "walking with the weapon: the free arm still swings with the gait",
    },
    NamedActionPose {
        name: "attack-anticipation",
        kind: ActionKind::Attack,
        progress: LOCKED_ATTACK_WINDUP_FRACTION * 0.5,
        speed: 0.0,
        direction: 0.0,
        intent: "half way into the windup: the blade is rising and the torso is turning away",
    },
    NamedActionPose {
        name: "attack-wound",
        kind: ActionKind::Attack,
        progress: LOCKED_ATTACK_WINDUP_FRACTION,
        speed: 0.0,
        direction: 0.0,
        intent: "the top of the swing, which is the frame the telegraph has to read from",
    },
    NamedActionPose {
        name: "attack-active",
        kind: ActionKind::Attack,
        progress: LOCKED_ATTACK_WINDUP_FRACTION + LOCKED_ATTACK_ACTIVE_FRACTION * 0.5,
        speed: 0.0,
        direction: 0.0,
        intent: "mid cut, where the blade is horizontal and the hit window is open",
    },
    NamedActionPose {
        name: "attack-follow-through",
        kind: ActionKind::Attack,
        progress: LOCKED_ATTACK_WINDUP_FRACTION + LOCKED_ATTACK_ACTIVE_FRACTION,
        speed: 0.0,
        direction: 0.0,
        intent: "the end of the cut: committed, low, and open",
    },
    NamedActionPose {
        name: "attack-recovered",
        kind: ActionKind::Attack,
        progress: 1.0,
        speed: 0.0,
        direction: 0.0,
        intent: "back to the carry, which is what makes the action loop without a pop",
    },
    NamedActionPose {
        name: "dodge-back",
        kind: ActionKind::Dodge,
        progress: 0.4,
        speed: 7.3,
        direction: core::f32::consts::PI,
        intent: "the middle of a backward dodge: crouched, leaning away, weapon tucked",
    },
    NamedActionPose {
        name: "dodge-side",
        kind: ActionKind::Dodge,
        progress: 0.4,
        speed: 7.3,
        direction: core::f32::consts::FRAC_PI_2,
        intent: "the middle of a sideways dodge, which is the one that clears a swing",
    },
    NamedActionPose {
        name: "stagger-front",
        kind: ActionKind::Stagger,
        progress: 0.25,
        speed: 0.0,
        direction: 0.0,
        intent: "the peak of the recoil from a hit in front: the frame a hit reads from",
    },
    NamedActionPose {
        name: "stagger-side",
        kind: ActionKind::Stagger,
        progress: 0.25,
        speed: 0.0,
        direction: core::f32::consts::FRAC_PI_2,
        intent: "the same recoil from the side, which must not read as the same pose",
    },
];

/// The overlay one named action pose describes.
#[must_use]
pub fn overlay_for(named: &NamedActionPose) -> ActionOverlay {
    match named.kind {
        ActionKind::Carry => ActionOverlay::carry(Side::Right),
        ActionKind::Attack => ActionOverlay::attack(
            Side::Right,
            named.progress,
            LOCKED_ATTACK_WINDUP_FRACTION,
            LOCKED_ATTACK_ACTIVE_FRACTION,
        ),
        ActionKind::Dodge => ActionOverlay::dodge(Side::Right, named.progress, named.direction),
        ActionKind::Stagger => ActionOverlay::stagger(Side::Right, named.progress, named.direction),
    }
}

/// The state one named action pose is evaluated in.
#[must_use]
pub fn action_state_for(named: &NamedActionPose) -> CharacterState {
    CharacterState {
        speed: named.speed,
        phase: 0.3,
        time: 5.0,
        ..CharacterState::default()
    }
}

/// Fingerprint of every named action pose, folded into one number.
///
/// Separate from [`named_pose_signature`] on purpose: a change to an attack curve
/// must move this and leave the locomotion signature alone, and the reverse.
pub fn action_pose_signature(character: &CompiledCharacter) -> u64 {
    let mut bytes = Vec::with_capacity(NAMED_ACTION_POSES.len() * 8);
    for named in NAMED_ACTION_POSES {
        let state = action_state_for(named);
        let overlay = overlay_for(named);
        let posed = pose_with(character, &state, None, Some(&overlay));
        bytes.extend_from_slice(&pose_fingerprint(&posed).to_le_bytes());
    }
    fnv1a64(&bytes)
}
