//! CPU-only voxel data, coordinates, neighborhoods, and reference meshing.
//!
//! These types are experimental runtime structures, not persisted formats.

mod chunk;
mod fixture;
mod mesh;
mod spatial;

pub use chunk::{
    CHUNK_BYTES, CHUNK_EDGE, CHUNK_VOLUME, Chunk, ChunkBoundsError, LocalCoord, VoxelId,
};
pub use fixture::{
    DIAGNOSTIC_FIXTURE_FINGERPRINT, DIAGNOSTIC_FIXTURE_SOLID_COUNT, MULTICHUNK_FIXTURE_FINGERPRINT,
    MULTICHUNK_FIXTURE_SOLID_COUNT, diagnostic_fixture, fingerprint, multichunk_diagnostic_fixture,
    multichunk_fingerprint,
};
pub use mesh::{
    BoundaryPolicy, ChunkNeighborhood, Face, Mesh, MeshBoundaryError, NeighborSample, Vertex,
    mesh_exposed_faces, mesh_exposed_faces_with_neighbors,
};
pub use spatial::{ChunkCoord, WorldCoordinateRangeError, WorldVoxelCoord};
