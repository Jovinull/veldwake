//! Headless, bounded chunk residency and detached meshing orchestration.
//!
//! This diagnostic M3B runtime owns CPU chunks. It deliberately has no
//! camera, renderer, or product world-generation concerns. Since M3D it also
//! owns an optional, experimental disk cache of source results: a discardable
//! accelerator, never authoritative world state and never a save format. Since
//! M4 the content itself comes from a [`ChunkSource`]: either the diagnostic
//! corridor that the M3 regression suite is written against, or the procedural
//! world generator adapted by [`TerrainChunkSource`].
//! The client adapter converts camera position into a demand center and
//! consumes [`StreamingRuntime::render_ready_meshes`]; nothing here knows
//! about `wgpu`, `winit`, or GPU residency.

mod cache;
mod demand;
mod hash;
mod runtime;
mod source;
mod types;
mod worker;

pub use cache::{CacheConfig, CacheFootprint, CacheOpenReport, ChunkCache, PayloadEncoding};
pub use demand::{
    BAND_LOD0_RADIUS, BAND_TRANSITION_RADIUS, DemandError, DemandSets, LodSelection,
    StreamingConfig,
};
pub use runtime::{
    CacheMetrics, InvalidationCause, MeshStatus, ResidencyStatus, ResidencySummary, RuntimeError,
    RuntimeMetrics, StreamingRuntime, TimingStat, TrackedState,
};
pub use source::{ChunkSource, DiagnosticChunkSource, SourceChunk, TerrainChunkSource};
pub use types::{
    LodLevel, MeshStamp, NeighborPresentation, NeighborStamp, RequestToken, SeamContract,
};
