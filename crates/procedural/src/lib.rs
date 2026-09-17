//! Deterministic CPU world generation for Veldwake.
//!
//! This crate answers one question: what is in the world at a given place? It
//! knows nothing about GPUs, windows, cameras, frames, files, or scheduling.
//! Generation is a pure function of a chunk coordinate and a world identity, so
//! it runs on the streaming worker, in a headless test, or in a command-line
//! probe with identical results.
//!
//! The dependency direction is `voxel <- procedural <- streaming <- client`:
//!
//! - `veldwake-voxel` stays a generic container with no terrain semantics;
//! - this crate owns terrain meaning, including the one table that maps a
//!   material to a [`VoxelId`](veldwake_voxel::VoxelId) and to a colour;
//! - `veldwake-streaming` keeps scheduling, residency, and caching, and adapts
//!   this generator to its own source interface;
//! - the client renders what it is given and never invents an identifier.
//!
//! The modules read in the order the world is built:
//!
//! | module | question it answers |
//! | --- | --- |
//! | [`noise`] | what is the value of a field at a position? |
//! | [`identity`] | which world is this, and under which rules? |
//! | [`terrain`] | how high is the ground, what is it made of, what grows here? |
//! | [`vegetation`] | which plants stand where, and what shape are they? |
//! | [`material`] | what does a material mean to a chunk and to a renderer? |
//! | [`generator`] | what voxels does one chunk contain? |
//! | [`region`] | which places and values are locked as fixtures? |

mod hash;

pub mod generator;
pub mod identity;
pub mod material;
pub mod noise;
pub mod region;
pub mod terrain;
pub mod vegetation;

pub use generator::TerrainGenerator;
pub use identity::{
    RegionExtent, STYLE_CONTRACT_VERSION, StreamLabel, TERRAIN_GENERATOR_VERSION, TerrainConfig,
    WorldIdentity, WorldSeed,
};
pub use material::TerrainMaterial;
pub use terrain::{BiomeZone, Landform, TerrainField, TerrainSample};
pub use vegetation::VegetationSystem;
