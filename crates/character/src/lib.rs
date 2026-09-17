//! Deterministic CPU character compilation, skeleton, and procedural
//! locomotion for Veldwake.
//!
//! This crate answers one question: what is this humanoid, and what is it
//! doing right now? It knows nothing about GPUs, windows, cameras, frames,
//! files, scheduling, or world generation. Compiling a character is a pure
//! function of a descriptor; posing one is a pure function of a compiled
//! character, a runtime state, and a ground query.
//!
//! The dependency direction is `voxel <- character <- client`:
//!
//! - `veldwake-voxel` supplies the dense grid and the exposed-face mesher, and
//!   stays a generic container with no content semantics;
//! - this crate owns character meaning, including the one table that maps a
//!   [`CharacterMaterial`](material::CharacterMaterial) to a `VoxelId` and to a
//!   colour, and the declared identifier range that keeps it disjoint from
//!   terrain;
//! - the client renders what it is given, adapts its terrain field to
//!   [`GroundSampler`](ground::GroundSampler), and never invents an identifier.
//!
//! It deliberately does **not** depend on `veldwake-procedural`. A headless
//! character crate has no business compiling a world generator, and the one
//! thing it needs from terrain — the height of the ground under a foot — is a
//! two-line trait the client implements.
//!
//! The modules read in the order a character is built:
//!
//! | module | question it answers |
//! | --- | --- |
//! | [`descriptor`] | what is this character, and is that even possible? |
//! | [`material`] | what does a character material mean to a voxel and to a renderer? |
//! | [`skeleton`] | which bones, where, and in what order? |
//! | [`geometry`] | which voxels, and what do they mesh to? |
//! | [`collision`] | what volume would this humanoid occupy? |
//! | [`compiler`] | all of the above, once, immutably, with a fingerprint |
//! | [`locomotion`] | what angle is every joint at, at this phase? |
//! | [`ground`] | what is under the foot? |
//! | [`ik`] | how does the leg reach it? |
//! | [`pose`] | where is every part, in the world, right now? |
//! | [`course`] | a reproducible path for evidence |
//! | [`fixture`] | which characters, poses, and values are locked |

mod hash;

pub mod collision;
pub mod compiler;
pub mod course;
pub mod descriptor;
pub mod fixture;
pub mod geometry;
pub mod ground;
pub mod ik;
pub mod locomotion;
pub mod material;
pub mod pose;
pub mod skeleton;

pub use compiler::{CharacterCompiler, CharacterError, CompiledCharacter, CompiledPart};
pub use course::{CharacterCourse, CourseLeg, CourseSample};
pub use descriptor::{
    Archetype, CHARACTER_COMPILER_VERSION, CHARACTER_SCHEMA_VERSION, CHARACTER_STYLE_VERSION,
    CHARACTER_VOXEL_SIZE, CHARACTER_VOXELS_PER_WORLD_UNIT, CharacterDescriptor,
    CharacterDescriptorError, CharacterIdentity, CharacterSeed, Proportions, ValidatedDescriptor,
};
pub use ground::GroundSampler;
pub use material::{CHARACTER_ID_END, CHARACTER_ID_FIRST, CharacterMaterial, CompiledPalette};
pub use pose::{CharacterState, FootContact, PosedCharacter, pose, rest_pose};
pub use skeleton::{BONE_COUNT, BoneId, Skeleton, Transform};
