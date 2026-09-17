use crate::{CHUNK_EDGE, Chunk, ChunkCoord, VoxelId};

/// Exact number of non-air cells in [`diagnostic_fixture`].
pub const DIAGNOSTIC_FIXTURE_SOLID_COUNT: usize = 31;
/// FNV-1a fingerprint of [`diagnostic_fixture`].
pub const DIAGNOSTIC_FIXTURE_FINGERPRINT: u64 = 0xa465_ff82_7904_04b9;
/// Exact number of solids in [`multichunk_diagnostic_fixture`].
pub const MULTICHUNK_FIXTURE_SOLID_COUNT: usize = 51;
/// FNV-1a fingerprint of [`multichunk_diagnostic_fixture`].
pub const MULTICHUNK_FIXTURE_FINGERPRINT: u64 = 0xe65a_e553_3c4d_b16a;

/// Builds the single asymmetric, deterministic M2 diagnostic fixture.
#[must_use]
pub fn diagnostic_fixture() -> Chunk {
    let mut chunk = Chunk::empty();

    // Adjacent pairs touch each of the six chunk boundaries.
    let boundary_cells = [
        (0, 2, 3, 1),
        (0, 3, 3, 2),
        (31, 5, 6, 2),
        (31, 5, 7, 1),
        (8, 0, 9, 1),
        (9, 0, 9, 2),
        (11, 31, 12, 2),
        (11, 31, 13, 1),
        (14, 15, 0, 1),
        (15, 15, 0, 2),
        (17, 18, 31, 2),
        (17, 19, 31, 1),
    ];
    for (x, y, z, id) in boundary_cells {
        write_fixture_cell(&mut chunk, x, y, z, VoxelId(id));
    }

    // A mixed-ID adjacent pair exercises internal face removal.
    write_fixture_cell(&mut chunk, 15, 15, 15, VoxelId(1));
    write_fixture_cell(&mut chunk, 16, 15, 15, VoxelId(2));

    // Genuinely isolated cells, including a third material.
    write_fixture_cell(&mut chunk, 24, 23, 22, VoxelId(7));
    write_fixture_cell(&mut chunk, 27, 2, 25, VoxelId(1));

    // An asymmetric five-step silhouette (1 + 2 + 3 + 4 + 5 cells).
    for x in 3..=7 {
        let height = x - 2;
        for y in 1..=height {
            let id = if (x + y) % 2 == 0 { 1 } else { 2 };
            write_fixture_cell(&mut chunk, x, y, 20, VoxelId(id));
        }
    }

    debug_assert_eq!(chunk.solid_count(), DIAGNOSTIC_FIXTURE_SOLID_COUNT);
    chunk
}

/// Builds the deterministic, canonically ordered M3A multi-chunk fixture.
///
/// The fixture crosses the `-X` and `+Z` seams of the central M2 fixture.
/// Its order is part of the diagnostic fingerprint but is not a save format.
#[must_use]
pub fn multichunk_diagnostic_fixture() -> Vec<(ChunkCoord, Chunk)> {
    let mut negative_x = Chunk::empty();
    write_fixture_cell(&mut negative_x, 31, 2, 3, VoxelId(7));
    write_fixture_cell(&mut negative_x, 31, 3, 3, VoxelId(1));
    for x in 25..=28 {
        for y in 1..=(x - 24) {
            let id = if (x + y) % 2 == 0 { 2 } else { 7 };
            write_fixture_cell(&mut negative_x, x, y, 12, VoxelId(id));
        }
    }

    let mut positive_z = Chunk::empty();
    write_fixture_cell(&mut positive_z, 17, 18, 0, VoxelId(1));
    write_fixture_cell(&mut positive_z, 17, 19, 0, VoxelId(7));
    for y in 1..=3 {
        write_fixture_cell(&mut positive_z, 8, y, 7, VoxelId(2));
        write_fixture_cell(&mut positive_z, 9, y, 7, VoxelId(7));
    }

    let chunks = vec![
        (ChunkCoord::new(-1, 0, 0), negative_x),
        (ChunkCoord::new(0, 0, 0), diagnostic_fixture()),
        (ChunkCoord::new(0, 0, 1), positive_z),
    ];
    debug_assert_eq!(
        chunks
            .iter()
            .map(|(_, chunk)| chunk.solid_count())
            .sum::<usize>(),
        MULTICHUNK_FIXTURE_SOLID_COUNT
    );
    chunks
}

fn write_fixture_cell(chunk: &mut Chunk, x: usize, y: usize, z: usize, value: VoxelId) {
    if let Err(error) = chunk.write(x, y, z, value) {
        unreachable!("static diagnostic fixture contains an invalid coordinate: {error}");
    }
}

/// Computes stable 64-bit FNV-1a over dimensions and little-endian cell IDs.
///
/// This is a fixture regression fingerprint, not a save or network format.
#[must_use]
pub fn fingerprint(chunk: &Chunk) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in (CHUNK_EDGE as u32).to_le_bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    for voxel in chunk.cells() {
        for byte in voxel.0.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
        }
    }
    hash
}

/// Computes stable 64-bit FNV-1a over ordered chunk coordinates and cells.
///
/// This is a fixture regression fingerprint, not a save or network format.
#[must_use]
pub fn multichunk_fingerprint(chunks: &[(ChunkCoord, Chunk)]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in (CHUNK_EDGE as u32).to_le_bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    for byte in (chunks.len() as u64).to_le_bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    for (coord, chunk) in chunks {
        for component in [coord.x, coord.y, coord.z] {
            for byte in component.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
            }
        }
        for voxel in chunk.cells() {
            for byte in voxel.0.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
            }
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BoundaryPolicy, ChunkNeighborhood, Face, mesh_exposed_faces,
        mesh_exposed_faces_with_neighbors,
    };

    #[test]
    fn diagnostic_fixture_has_locked_content_and_all_boundaries() {
        let fixture = diagnostic_fixture();
        assert_eq!(fixture.solid_count(), DIAGNOSTIC_FIXTURE_SOLID_COUNT);
        assert_eq!(fingerprint(&fixture), DIAGNOSTIC_FIXTURE_FINGERPRINT);

        let boundary_occupancy = [
            (0, 2, 3),
            (31, 5, 6),
            (8, 0, 9),
            (11, 31, 12),
            (14, 15, 0),
            (17, 18, 31),
        ];
        for (x, y, z) in boundary_occupancy {
            assert_ne!(fixture.read(x, y, z), Ok(VoxelId::AIR));
        }
    }

    #[test]
    fn diagnostic_fixture_has_locked_reference_topology() {
        let mesh = mesh_exposed_faces(&diagnostic_fixture());
        assert_eq!(mesh.quad_count(), 132);
        assert_eq!(mesh.vertices().len(), 528);
        assert_eq!(mesh.indices().len(), 792);
    }

    #[test]
    fn coarse_diagnostic_fixture_has_locked_topology() {
        let coarse = diagnostic_fixture().downsample_2x();
        assert_eq!(coarse.solid_count(), 16);
        let mesh = mesh_exposed_faces(&coarse);
        assert_eq!(mesh.quad_count(), 82);
        assert_eq!(mesh.vertices().len(), 328);
        assert_eq!(mesh.indices().len(), 492);
        assert_eq!(coarse, diagnostic_fixture().downsample_2x());
    }

    #[test]
    fn multichunk_fixture_is_canonical_and_crosses_two_seams() {
        let chunks = multichunk_diagnostic_fixture();
        let coords = chunks.iter().map(|(coord, _)| *coord).collect::<Vec<_>>();
        assert_eq!(
            coords,
            vec![
                ChunkCoord::new(-1, 0, 0),
                ChunkCoord::new(0, 0, 0),
                ChunkCoord::new(0, 0, 1),
            ]
        );
        assert_eq!(
            chunks
                .iter()
                .map(|(_, chunk)| chunk.solid_count())
                .sum::<usize>(),
            MULTICHUNK_FIXTURE_SOLID_COUNT
        );
        assert_eq!(
            multichunk_fingerprint(&chunks),
            MULTICHUNK_FIXTURE_FINGERPRINT
        );

        assert_eq!(chunks[0].1.read(31, 2, 3), Ok(VoxelId(7)));
        assert_eq!(chunks[1].1.read(0, 2, 3), Ok(VoxelId(1)));
        assert_eq!(chunks[1].1.read(17, 19, 31), Ok(VoxelId(1)));
        assert_eq!(chunks[2].1.read(17, 19, 0), Ok(VoxelId(7)));
    }

    #[test]
    fn multichunk_fixture_has_locked_neighbor_aware_topology()
    -> Result<(), crate::MeshBoundaryError> {
        let chunks = multichunk_diagnostic_fixture();
        let mut quads = 0;
        let mut vertices = 0;
        let mut indices = 0;

        for (coord, chunk) in &chunks {
            let mut neighborhood = ChunkNeighborhood::new(chunk);
            for face in Face::ALL {
                let Some(neighbor_coord) = coord.neighbor(face) else {
                    continue;
                };
                if let Some((_, neighbor)) = chunks
                    .iter()
                    .find(|(candidate, _)| *candidate == neighbor_coord)
                {
                    neighborhood = neighborhood.with_neighbor(face, neighbor);
                }
            }
            let mesh = mesh_exposed_faces_with_neighbors(neighborhood, BoundaryPolicy::Expose)?;
            quads += mesh.quad_count();
            vertices += mesh.vertices().len();
            indices += mesh.indices().len();
        }

        assert_eq!(quads, 202);
        assert_eq!(vertices, 808);
        assert_eq!(indices, 1_212);
        Ok(())
    }
}
