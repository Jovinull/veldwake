use veldwake_voxel::{CHUNK_EDGE, Chunk, ChunkCoord, LocalCoord, VoxelId};

use crate::hash::fnv1a64;

/// Bumped by hand whenever the generated content changes in a way the
/// descriptor in [`DiagnosticChunkSource::fingerprint`] does not already
/// capture. Forgetting to bump it would let a disk cache replay content the
/// source no longer produces, so the locked fingerprint test exists to make
/// that impossible to do silently.
const SOURCE_SCHEMA_REVISION: u32 = 1;

/// Inclusive half-extents of the finite corridor, in chunks.
const CORRIDOR_X: i32 = 4;
const CORRIDOR_Y: i32 = 1;
const CORRIDOR_Z: i32 = 4;
/// Edge of one checkerboard square, in voxels.
const CHECKER_PERIOD: i64 = 8;
/// Materials of the two checkerboard squares and of the landmark column.
const CHECKER_MATERIAL_A: u16 = 1;
const CHECKER_MATERIAL_B: u16 = 2;
const LANDMARK_MATERIAL: u16 = 7;
/// Height range and local `z` of the asymmetric landmark column.
const LANDMARK_TOP: usize = 6;
const LANDMARK_Z: usize = 3;

/// Result of the finite diagnostic source. Absence is authoritative knowledge,
/// unlike a chunk that has not completed loading.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceChunk {
    Present(Chunk),
    KnownAbsent,
}

/// Finite, code-defined streaming input used only to exercise M3B lifecycles.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiagnosticChunkSource;

impl DiagnosticChunkSource {
    /// Deterministic identity of everything this source generates.
    ///
    /// A disk cache entry is only valid while the source that produced it is
    /// unchanged, so the cache stores this value and refuses any entry that
    /// disagrees. The identity is derived from an explicit descriptor of the
    /// generation rules plus a hand-maintained revision, never from the
    /// compiled code, so changing a rule without bumping the revision would go
    /// unnoticed if the locked test did not exist.
    ///
    /// This is a cache-invalidation key. It is not a save-format version and
    /// promises nothing about compatibility across builds.
    #[must_use]
    pub fn fingerprint(self) -> u64 {
        let mut descriptor = Vec::new();
        descriptor.extend_from_slice(b"veldwake.diagnostic-chunk-source");
        descriptor.extend_from_slice(&SOURCE_SCHEMA_REVISION.to_le_bytes());
        for bound in [CORRIDOR_X, CORRIDOR_Y, CORRIDOR_Z] {
            descriptor.extend_from_slice(&bound.to_le_bytes());
        }
        descriptor.extend_from_slice(&CHECKER_PERIOD.to_le_bytes());
        for material in [CHECKER_MATERIAL_A, CHECKER_MATERIAL_B, LANDMARK_MATERIAL] {
            descriptor.extend_from_slice(&material.to_le_bytes());
        }
        descriptor.extend_from_slice(&(LANDMARK_TOP as u64).to_le_bytes());
        descriptor.extend_from_slice(&(LANDMARK_Z as u64).to_le_bytes());
        descriptor.extend_from_slice(&(CHUNK_EDGE as u64).to_le_bytes());
        fnv1a64(&descriptor)
    }

    #[must_use]
    pub fn load(self, coord: ChunkCoord) -> SourceChunk {
        if !(-CORRIDOR_X..=CORRIDOR_X).contains(&coord.x)
            || !(-CORRIDOR_Y..=CORRIDOR_Y).contains(&coord.y)
            || !(-CORRIDOR_Z..=CORRIDOR_Z).contains(&coord.z)
        {
            return SourceChunk::KnownAbsent;
        }

        let mut chunk = Chunk::empty();
        if coord.y == 0 {
            for z in 0..CHUNK_EDGE {
                for x in 0..CHUNK_EDGE {
                    let world_x = i64::from(coord.x) * CHUNK_EDGE as i64 + x as i64;
                    let world_z = i64::from(coord.z) * CHUNK_EDGE as i64 + z as i64;
                    let voxel = if (world_x.div_euclid(CHECKER_PERIOD)
                        + world_z.div_euclid(CHECKER_PERIOD))
                    .rem_euclid(2)
                        == 0
                    {
                        VoxelId(CHECKER_MATERIAL_A)
                    } else {
                        VoxelId(CHECKER_MATERIAL_B)
                    };
                    let previous = chunk.write_local(local(x, 0, z), voxel);
                    debug_assert!(previous.is_air());
                }
            }

            // An asymmetric landmark spans both sides of x=0 without noise or seeds.
            if (-1..=0).contains(&coord.x) && coord.z == 0 {
                let x = if coord.x == -1 { CHUNK_EDGE - 1 } else { 0 };
                for y in 1..=LANDMARK_TOP {
                    let previous =
                        chunk.write_local(local(x, y, LANDMARK_Z), VoxelId(LANDMARK_MATERIAL));
                    debug_assert!(previous.is_air());
                }
            }
        }
        SourceChunk::Present(chunk)
    }
}

fn local(x: usize, y: usize, z: usize) -> LocalCoord {
    match LocalCoord::new(x, y, z) {
        Ok(coord) => coord,
        Err(error) => unreachable!("diagnostic source emitted invalid local coordinate: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fingerprint is locked so a generation change cannot ship without
    /// someone deciding what it means for cached content. Changing the source
    /// rules must bump `SOURCE_SCHEMA_REVISION` and update this value in the
    /// same change.
    #[test]
    fn the_source_fingerprint_is_locked_and_moves_with_the_revision() {
        assert_eq!(
            DiagnosticChunkSource.fingerprint(),
            0xe4a6_d759_3d06_0a38,
            "the diagnostic source identity changed; bump SOURCE_SCHEMA_REVISION deliberately"
        );
        assert_eq!(
            DiagnosticChunkSource.fingerprint(),
            DiagnosticChunkSource.fingerprint(),
            "the identity must be deterministic"
        );
    }

    #[test]
    fn source_is_finite_and_distinguishes_absence_from_empty_chunks() {
        assert!(matches!(
            DiagnosticChunkSource.load(ChunkCoord::new(5, 0, 0)),
            SourceChunk::KnownAbsent
        ));
        let SourceChunk::Present(empty) = DiagnosticChunkSource.load(ChunkCoord::new(0, 1, 0))
        else {
            panic!("coordinate inside source bounds must be present");
        };
        assert_eq!(empty.solid_count(), 0);
    }

    #[test]
    fn negative_and_positive_chunks_share_a_continuous_seam() {
        let SourceChunk::Present(negative) = DiagnosticChunkSource.load(ChunkCoord::new(-1, 0, 0))
        else {
            panic!("negative diagnostic chunk must exist");
        };
        let SourceChunk::Present(origin) = DiagnosticChunkSource.load(ChunkCoord::new(0, 0, 0))
        else {
            panic!("origin diagnostic chunk must exist");
        };
        for z in 0..CHUNK_EDGE {
            assert!(!negative.read_local(local(CHUNK_EDGE - 1, 0, z)).is_air());
            assert!(!origin.read_local(local(0, 0, z)).is_air());
        }
    }
}
