//! Headless, bounded chunk residency and detached meshing orchestration.
//!
//! This diagnostic M3B1 runtime owns CPU chunks. It deliberately has no
//! camera, renderer, persistence, or product world-generation concerns.

mod demand;
mod runtime;
mod source;
mod types;
mod worker;

pub use demand::{DemandError, DemandSets, StreamingConfig};
pub use runtime::{MeshStatus, ResidencyStatus, RuntimeError, RuntimeMetrics, StreamingRuntime};
pub use source::{DiagnosticChunkSource, SourceChunk};
pub use types::{MeshStamp, NeighborStamp, RequestToken};
