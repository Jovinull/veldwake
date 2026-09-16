use crate::{CHUNK_EDGE, Chunk, VoxelId};

/// Exact number of non-air cells in [`diagnostic_fixture`].
pub const DIAGNOSTIC_FIXTURE_SOLID_COUNT: usize = 31;
/// FNV-1a fingerprint of [`diagnostic_fixture`].
pub const DIAGNOSTIC_FIXTURE_FINGERPRINT: u64 = 0xa465_ff82_7904_04b9;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_exposed_faces;

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
}
