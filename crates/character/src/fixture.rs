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

use crate::compiler::{CharacterCompiler, CharacterError, CompiledCharacter, behaviour_signature};
use crate::descriptor::{CharacterDescriptor, CharacterSeed, Proportions};
use crate::hash::{fnv1a64, push_f32, push_u32};
use crate::material::{GarmentScheme, HairTone, PaletteChoice, SkinTone};
use crate::pose::{CharacterState, PosedCharacter, pose};
use crate::skeleton::BONE_COUNT;

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

/// Every locked fixture value, for the probe to print in one place.
#[must_use]
pub fn locked_values() -> [(&'static str, u64); 7] {
    [
        ("golden geometry", GOLDEN_GEOMETRY_FINGERPRINT),
        ("golden skeleton", GOLDEN_SKELETON_FINGERPRINT),
        ("golden collision", GOLDEN_COLLISION_FINGERPRINT),
        ("golden behaviour", GOLDEN_BEHAVIOUR_SIGNATURE),
        ("golden poses", GOLDEN_POSE_SIGNATURE),
        ("sturdy behaviour", STURDY_BEHAVIOUR_SIGNATURE),
        ("varied behaviour", VARIED_BEHAVIOUR_SIGNATURE),
    ]
}

/// Computes every signature from scratch; the probe prints these next to the
/// locked ones so a mismatch is one line to read.
pub fn measured_values() -> Result<[(&'static str, u64); 7], CharacterError> {
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
        ("sturdy behaviour", behaviour_signature(&sturdy)),
        ("varied behaviour", behaviour_signature(&varied)),
    ])
}

#[cfg(test)]
mod tests {
    use super::{
        NAMED_POSES, locked_values, measured_values, pose_fingerprint, state_for, walk_cycle_phases,
    };
    use crate::compiler::CharacterError;
    use crate::pose::pose;
    use std::collections::BTreeSet;

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
        let measured = measured_values()?;
        let golden = measured[3].1;
        let sturdy = measured[5].1;
        let varied = measured[6].1;
        assert_ne!(golden, sturdy);
        assert_ne!(golden, varied);
        assert_ne!(sturdy, varied);
        Ok(())
    }
}
