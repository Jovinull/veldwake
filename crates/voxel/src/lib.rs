//! CPU-only voxel data and reference meshing for the M2 prototype.
//!
//! These types are experimental runtime structures, not persisted formats.

mod chunk;
mod fixture;
mod mesh;

pub use chunk::{
    CHUNK_BYTES, CHUNK_EDGE, CHUNK_VOLUME, Chunk, ChunkBoundsError, LocalCoord, VoxelId,
};
pub use fixture::{
    DIAGNOSTIC_FIXTURE_FINGERPRINT, DIAGNOSTIC_FIXTURE_SOLID_COUNT, diagnostic_fixture, fingerprint,
};
pub use mesh::{Face, Mesh, Vertex, mesh_exposed_faces};
