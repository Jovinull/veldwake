use std::fmt;
use std::mem::size_of;

/// Edge length of the experimental M2 chunk, in voxels.
pub const CHUNK_EDGE: usize = 32;
/// Number of cells in an experimental M2 chunk.
pub const CHUNK_VOLUME: usize = CHUNK_EDGE * CHUNK_EDGE * CHUNK_EDGE;
/// Logical bytes occupied by the dense voxel cell payload.
pub const CHUNK_BYTES: usize = CHUNK_VOLUME * size_of::<VoxelId>();
/// Edge length of the M3C coarse grid: one cell per 2×2×2 chunk voxels.
pub const COARSE_EDGE: usize = CHUNK_EDGE / 2;

/// A compact voxel/material identifier. Zero is always air in M2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct VoxelId(pub u16);

impl VoxelId {
    pub const AIR: Self = Self(0);

    #[must_use]
    pub const fn is_air(self) -> bool {
        self.0 == Self::AIR.0
    }
}

/// A validated coordinate local to one dense grid of edge `EDGE`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GridCoord<const EDGE: usize> {
    x: u8,
    y: u8,
    z: u8,
}

/// A validated coordinate local to one chunk.
pub type LocalCoord = GridCoord<CHUNK_EDGE>;

impl<const EDGE: usize> GridCoord<EDGE> {
    /// Validates a local coordinate. Out-of-bounds is an error, never air.
    pub fn new(x: usize, y: usize, z: usize) -> Result<Self, ChunkBoundsError> {
        if x >= EDGE || y >= EDGE || z >= EDGE {
            return Err(ChunkBoundsError {
                x,
                y,
                z,
                edge: EDGE,
            });
        }

        Ok(Self {
            x: x as u8,
            y: y as u8,
            z: z as u8,
        })
    }

    #[must_use]
    pub const fn x(self) -> usize {
        self.x as usize
    }

    #[must_use]
    pub const fn y(self) -> usize {
        self.y as usize
    }

    #[must_use]
    pub const fn z(self) -> usize {
        self.z as usize
    }
}

/// An invalid coordinate presented to the public grid API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkBoundsError {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub edge: usize,
}

impl fmt::Display for ChunkBoundsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "local voxel coordinate ({}, {}, {}) is outside 0..{}",
            self.x, self.y, self.z, self.edge
        )
    }
}

impl std::error::Error for ChunkBoundsError {}

/// One dense, fixed-edge voxel grid in a right-handed, Y-up local space.
///
/// Storage is a heap slice of `EDGE³` cells so the type needs no generic
/// const expression; `EDGE` only parameterizes coordinates and strides.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DenseGrid<const EDGE: usize> {
    cells: Box<[VoxelId]>,
    solid_count: usize,
}

/// The experimental M2 chunk: a `32³` dense grid.
pub type Chunk = DenseGrid<CHUNK_EDGE>;
/// The M3C coarse grid derived from one chunk by `downsample_2x`.
pub type CoarseGrid = DenseGrid<COARSE_EDGE>;

impl<const EDGE: usize> DenseGrid<EDGE> {
    /// Number of cells in this grid.
    pub const VOLUME: usize = EDGE * EDGE * EDGE;
    /// Logical bytes occupied by the dense cell payload.
    pub const BYTES: usize = Self::VOLUME * size_of::<VoxelId>();

    #[must_use]
    pub fn empty() -> Self {
        Self {
            cells: vec![VoxelId::AIR; Self::VOLUME].into_boxed_slice(),
            solid_count: 0,
        }
    }

    /// Reads a cell, returning a typed error for coordinates outside the grid.
    pub fn read(&self, x: usize, y: usize, z: usize) -> Result<VoxelId, ChunkBoundsError> {
        let coord = GridCoord::<EDGE>::new(x, y, z)?;
        Ok(self.read_local(coord))
    }

    /// Writes a cell and returns its previous value.
    pub fn write(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        value: VoxelId,
    ) -> Result<VoxelId, ChunkBoundsError> {
        let coord = GridCoord::<EDGE>::new(x, y, z)?;
        Ok(self.write_local(coord, value))
    }

    #[must_use]
    pub fn read_local(&self, coord: GridCoord<EDGE>) -> VoxelId {
        self.cells[linear_index(coord)]
    }

    #[must_use]
    pub fn write_local(&mut self, coord: GridCoord<EDGE>, value: VoxelId) -> VoxelId {
        let cell = &mut self.cells[linear_index(coord)];
        let previous = *cell;

        match (previous.is_air(), value.is_air()) {
            (true, false) => self.solid_count += 1,
            (false, true) => self.solid_count -= 1,
            _ => {}
        }

        *cell = value;
        previous
    }

    #[must_use]
    pub const fn solid_count(&self) -> usize {
        self.solid_count
    }

    pub(crate) fn cells(&self) -> &[VoxelId] {
        &self.cells
    }
}

impl Chunk {
    /// Derives the coarse grid: each coarse cell covers `2×2×2` chunk voxels.
    ///
    /// Occupancy is conservative (solid if any covered voxel is solid) so
    /// one-voxel features survive; the material is the most frequent solid ID
    /// among the covered voxels, ties resolved by the lowest ID. The result
    /// depends only on local content, never on the chunk's world coordinate.
    #[must_use]
    pub fn downsample_2x(&self) -> CoarseGrid {
        let mut coarse = CoarseGrid::empty();
        for z in 0..COARSE_EDGE {
            for y in 0..COARSE_EDGE {
                for x in 0..COARSE_EDGE {
                    let mut solids: [(VoxelId, u8); 8] = [(VoxelId::AIR, 0); 8];
                    let mut distinct = 0;
                    for dz in 0..2 {
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let fine = self.read_local(local_coord_from_loop(
                                    2 * x + dx,
                                    2 * y + dy,
                                    2 * z + dz,
                                ));
                                if fine.is_air() {
                                    continue;
                                }
                                match solids[..distinct].iter_mut().find(|(id, _)| *id == fine) {
                                    Some((_, count)) => *count += 1,
                                    None => {
                                        solids[distinct] = (fine, 1);
                                        distinct += 1;
                                    }
                                }
                            }
                        }
                    }
                    let winner = solids[..distinct]
                        .iter()
                        .copied()
                        .min_by_key(|&(id, count)| (std::cmp::Reverse(count), id.0));
                    if let Some((material, _)) = winner {
                        let previous =
                            coarse.write_local(coarse_coord_from_loop(x, y, z), material);
                        debug_assert!(previous.is_air());
                    }
                }
            }
        }
        coarse
    }
}

impl<const EDGE: usize> Default for DenseGrid<EDGE> {
    fn default() -> Self {
        Self::empty()
    }
}

#[must_use]
pub(crate) const fn linear_index<const EDGE: usize>(coord: GridCoord<EDGE>) -> usize {
    coord.x() + EDGE * (coord.y() + EDGE * coord.z())
}

fn local_coord_from_loop(x: usize, y: usize, z: usize) -> LocalCoord {
    match LocalCoord::new(x, y, z) {
        Ok(coord) => coord,
        Err(error) => unreachable!("downsample loop generated invalid chunk coordinate: {error}"),
    }
}

fn coarse_coord_from_loop(x: usize, y: usize, z: usize) -> GridCoord<COARSE_EDGE> {
    match GridCoord::new(x, y, z) {
        Ok(coord) => coord,
        Err(error) => unreachable!("downsample loop generated invalid coarse coordinate: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_has_expected_corners_and_strides() -> Result<(), ChunkBoundsError> {
        assert_eq!(linear_index(LocalCoord::new(0, 0, 0)?), 0);
        assert_eq!(linear_index(LocalCoord::new(1, 0, 0)?), 1);
        assert_eq!(linear_index(LocalCoord::new(0, 1, 0)?), CHUNK_EDGE);
        assert_eq!(
            linear_index(LocalCoord::new(0, 0, 1)?),
            CHUNK_EDGE * CHUNK_EDGE
        );
        assert_eq!(linear_index(LocalCoord::new(31, 31, 31)?), CHUNK_VOLUME - 1);
        Ok(())
    }

    #[test]
    fn every_coordinate_has_a_unique_index() -> Result<(), ChunkBoundsError> {
        let mut seen = vec![false; CHUNK_VOLUME];
        for z in 0..CHUNK_EDGE {
            for y in 0..CHUNK_EDGE {
                for x in 0..CHUNK_EDGE {
                    let index = linear_index(LocalCoord::new(x, y, z)?);
                    assert!(!seen[index], "duplicate index {index}");
                    seen[index] = true;
                }
            }
        }
        assert!(seen.into_iter().all(|value| value));
        Ok(())
    }

    #[test]
    fn bounds_errors_are_distinct_from_air() {
        let mut chunk = Chunk::empty();
        assert_eq!(chunk.read(31, 31, 31), Ok(VoxelId::AIR));
        assert_eq!(
            chunk.read(32, 0, 0),
            Err(ChunkBoundsError {
                x: 32,
                y: 0,
                z: 0,
                edge: 32
            })
        );
        assert_eq!(
            LocalCoord::new(0, 32, 0),
            Err(ChunkBoundsError {
                x: 0,
                y: 32,
                z: 0,
                edge: 32
            })
        );
        assert_eq!(
            chunk.write(0, 0, 32, VoxelId(1)),
            Err(ChunkBoundsError {
                x: 0,
                y: 0,
                z: 32,
                edge: 32
            })
        );
        assert_eq!(chunk.solid_count(), 0);
    }

    #[test]
    fn read_write_return_previous_value_and_track_solids() -> Result<(), ChunkBoundsError> {
        let mut chunk = Chunk::empty();
        assert_eq!(chunk.solid_count(), 0);
        assert_eq!(chunk.write(2, 3, 4, VoxelId(7))?, VoxelId::AIR);
        assert_eq!(chunk.read(2, 3, 4)?, VoxelId(7));
        assert_eq!(chunk.solid_count(), 1);
        assert_eq!(chunk.write(2, 3, 4, VoxelId(9))?, VoxelId(7));
        assert_eq!(chunk.solid_count(), 1);
        assert_eq!(chunk.write(2, 3, 4, VoxelId::AIR)?, VoxelId(9));
        assert_eq!(chunk.solid_count(), 0);
        Ok(())
    }

    #[test]
    fn chunk_constants_match_the_generic_grid() {
        assert_eq!(Chunk::VOLUME, CHUNK_VOLUME);
        assert_eq!(Chunk::BYTES, CHUNK_BYTES);
        assert_eq!(CoarseGrid::VOLUME, 4_096);
        assert_eq!(CoarseGrid::BYTES, 8_192);
        assert_eq!(Chunk::empty().cells().len(), CHUNK_VOLUME);
        assert_eq!(CoarseGrid::empty().cells().len(), 4_096);
    }

    #[test]
    fn coarse_grid_bounds_read_write_and_strides() -> Result<(), ChunkBoundsError> {
        let mut grid = CoarseGrid::empty();
        assert_eq!(grid.read(15, 15, 15), Ok(VoxelId::AIR));
        assert_eq!(
            grid.read(16, 0, 0),
            Err(ChunkBoundsError {
                x: 16,
                y: 0,
                z: 0,
                edge: 16
            })
        );
        assert_eq!(grid.write(15, 15, 15, VoxelId(5))?, VoxelId::AIR);
        assert_eq!(grid.read(15, 15, 15)?, VoxelId(5));
        assert_eq!(grid.solid_count(), 1);
        assert_eq!(
            linear_index(GridCoord::<COARSE_EDGE>::new(15, 15, 15)?),
            CoarseGrid::VOLUME - 1
        );
        assert_eq!(linear_index(GridCoord::<COARSE_EDGE>::new(0, 1, 0)?), 16);
        assert_eq!(linear_index(GridCoord::<COARSE_EDGE>::new(0, 0, 1)?), 256);
        Ok(())
    }

    #[test]
    fn downsample_of_empty_and_full_chunks() -> Result<(), ChunkBoundsError> {
        assert_eq!(Chunk::empty().downsample_2x().solid_count(), 0);
        let mut full = Chunk::empty();
        for z in 0..CHUNK_EDGE {
            for y in 0..CHUNK_EDGE {
                for x in 0..CHUNK_EDGE {
                    full.write(x, y, z, VoxelId(3))?;
                }
            }
        }
        let coarse = full.downsample_2x();
        assert_eq!(coarse.solid_count(), CoarseGrid::VOLUME);
        assert!(coarse.cells().iter().all(|cell| *cell == VoxelId(3)));
        Ok(())
    }

    #[test]
    fn downsample_keeps_one_voxel_features_occupied() -> Result<(), ChunkBoundsError> {
        // A one-voxel-thick floor and one isolated voxel both survive.
        let mut chunk = Chunk::empty();
        for z in 0..CHUNK_EDGE {
            for x in 0..CHUNK_EDGE {
                chunk.write(x, 0, z, VoxelId(1))?;
            }
        }
        chunk.write(7, 9, 11, VoxelId(4))?;
        let coarse = chunk.downsample_2x();
        assert_eq!(coarse.solid_count(), COARSE_EDGE * COARSE_EDGE + 1);
        for z in 0..COARSE_EDGE {
            for x in 0..COARSE_EDGE {
                assert_eq!(coarse.read(x, 0, z)?, VoxelId(1));
                assert_eq!(coarse.read(x, 1, z)?, VoxelId::AIR);
            }
        }
        assert_eq!(coarse.read(3, 4, 5)?, VoxelId(4));
        Ok(())
    }

    #[test]
    fn downsample_material_is_the_majority_with_lowest_id_tie_break() -> Result<(), ChunkBoundsError>
    {
        let mut chunk = Chunk::empty();
        // Block (0,0,0): three of ID 9, two of ID 2, one of ID 5 -> 9 wins.
        for (index, id) in [9, 9, 9, 2, 2, 5].into_iter().enumerate() {
            chunk.write(index % 2, (index / 2) % 2, index / 4, VoxelId(id))?;
        }
        // Block (1,0,0): two of ID 8 and two of ID 3 -> tie, lowest ID 3 wins.
        for (index, id) in [8, 3, 8, 3].into_iter().enumerate() {
            chunk.write(2 + index % 2, (index / 2) % 2, 0, VoxelId(id))?;
        }
        let coarse = chunk.downsample_2x();
        assert_eq!(coarse.read(0, 0, 0)?, VoxelId(9));
        assert_eq!(coarse.read(1, 0, 0)?, VoxelId(3));
        assert_eq!(coarse.solid_count(), 2);
        Ok(())
    }

    #[test]
    fn downsample_is_deterministic_and_coordinate_independent() {
        // The negative-coordinate fixture chunk is content; its world address
        // never enters the downsample, so repeated derivation is identical.
        let fixture = crate::multichunk_diagnostic_fixture();
        let (coord, negative) = &fixture[0];
        assert_eq!(coord.x, -1);
        let first = negative.downsample_2x();
        let second = negative.clone().downsample_2x();
        assert_eq!(first, second);
        for z in 0..COARSE_EDGE {
            for y in 0..COARSE_EDGE {
                for x in 0..COARSE_EDGE {
                    let any_solid = (0..8).any(|bit| {
                        negative
                            .read(
                                2 * x + (bit & 1),
                                2 * y + ((bit >> 1) & 1),
                                2 * z + (bit >> 2),
                            )
                            .is_ok_and(|voxel| !voxel.is_air())
                    });
                    assert_eq!(
                        !first.read(x, y, z).is_ok_and(|voxel| voxel.is_air()),
                        any_solid,
                        "occupancy mismatch at ({x}, {y}, {z})"
                    );
                }
            }
        }
    }
}
