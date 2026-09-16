//! Headless, bounded chunk residency and detached meshing orchestration.
//!
//! This diagnostic M3B runtime owns CPU chunks. It deliberately has no
//! camera, renderer, persistence, or product world-generation concerns.
//! The client adapter converts camera position into a demand center and
//! consumes [`StreamingRuntime::render_ready_meshes`]; nothing here knows
//! about `wgpu`, `winit`, or GPU residency.

mod demand;
mod runtime;
mod source;
mod types;
mod worker;

pub use demand::{
    BAND_LOD0_RADIUS, BAND_TRANSITION_RADIUS, DemandError, DemandSets, LodSelection,
    StreamingConfig,
};
pub use runtime::{
    MeshStatus, ResidencyStatus, ResidencySummary, RuntimeError, RuntimeMetrics, StreamingRuntime,
};
pub use source::{DiagnosticChunkSource, SourceChunk};
pub use types::{LodLevel, MeshStamp, NeighborPresentation, NeighborStamp, RequestToken};
