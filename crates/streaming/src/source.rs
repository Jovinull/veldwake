use veldwake_voxel::{CHUNK_EDGE, Chunk, ChunkCoord, LocalCoord, VoxelId};

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
    #[must_use]
    pub fn load(self, coord: ChunkCoord) -> SourceChunk {
        if !(-4..=4).contains(&coord.x)
            || !(-1..=1).contains(&coord.y)
            || !(-4..=4).contains(&coord.z)
        {
            return SourceChunk::KnownAbsent;
        }

        let mut chunk = Chunk::empty();
        if coord.y == 0 {
            for z in 0..CHUNK_EDGE {
                for x in 0..CHUNK_EDGE {
                    let world_x = i64::from(coord.x) * CHUNK_EDGE as i64 + x as i64;
                    let world_z = i64::from(coord.z) * CHUNK_EDGE as i64 + z as i64;
                    let voxel =
                        if (world_x.div_euclid(8) + world_z.div_euclid(8)).rem_euclid(2) == 0 {
                            VoxelId(1)
                        } else {
                            VoxelId(2)
                        };
                    let previous = chunk.write_local(local(x, 0, z), voxel);
                    debug_assert!(previous.is_air());
                }
            }

            // An asymmetric landmark spans both sides of x=0 without noise or seeds.
            if (-1..=0).contains(&coord.x) && coord.z == 0 {
                let x = if coord.x == -1 { CHUNK_EDGE - 1 } else { 0 };
                for y in 1..=6 {
                    let previous = chunk.write_local(local(x, y, 3), VoxelId(7));
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
