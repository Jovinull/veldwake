use std::fmt;
use std::mem::size_of;

/// Edge length of the experimental M2 chunk, in voxels.
pub const CHUNK_EDGE: usize = 32;
/// Number of cells in an experimental M2 chunk.
pub const CHUNK_VOLUME: usize = CHUNK_EDGE * CHUNK_EDGE * CHUNK_EDGE;
/// Logical bytes occupied by the dense voxel cell payload.
pub const CHUNK_BYTES: usize = CHUNK_VOLUME * size_of::<VoxelId>();

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

/// A validated coordinate local to one chunk.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LocalCoord {
    x: u8,
    y: u8,
    z: u8,
}

impl LocalCoord {
    /// Validates a local coordinate. Out-of-bounds is an error, never air.
    pub fn new(x: usize, y: usize, z: usize) -> Result<Self, ChunkBoundsError> {
        if x >= CHUNK_EDGE || y >= CHUNK_EDGE || z >= CHUNK_EDGE {
            return Err(ChunkBoundsError { x, y, z });
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

/// An invalid coordinate presented to the public chunk API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkBoundsError {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

impl fmt::Display for ChunkBoundsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "local voxel coordinate ({}, {}, {}) is outside 0..{}",
            self.x, self.y, self.z, CHUNK_EDGE
        )
    }
}

impl std::error::Error for ChunkBoundsError {}

/// One dense, fixed-size M2 chunk in a right-handed, Y-up local space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    cells: Box<[VoxelId]>,
    solid_count: usize,
}

impl Chunk {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cells: vec![VoxelId::AIR; CHUNK_VOLUME].into_boxed_slice(),
            solid_count: 0,
        }
    }

    /// Reads a cell, returning a typed error for coordinates outside the chunk.
    pub fn read(&self, x: usize, y: usize, z: usize) -> Result<VoxelId, ChunkBoundsError> {
        let coord = LocalCoord::new(x, y, z)?;
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
        let coord = LocalCoord::new(x, y, z)?;
        Ok(self.write_local(coord, value))
    }

    #[must_use]
    pub fn read_local(&self, coord: LocalCoord) -> VoxelId {
        self.cells[linear_index(coord)]
    }

    #[must_use]
    pub fn write_local(&mut self, coord: LocalCoord, value: VoxelId) -> VoxelId {
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

impl Default for Chunk {
    fn default() -> Self {
        Self::empty()
    }
}

#[must_use]
pub(crate) const fn linear_index(coord: LocalCoord) -> usize {
    coord.x() + CHUNK_EDGE * (coord.y() + CHUNK_EDGE * coord.z())
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
            Err(ChunkBoundsError { x: 32, y: 0, z: 0 })
        );
        assert_eq!(
            LocalCoord::new(0, 32, 0),
            Err(ChunkBoundsError { x: 0, y: 32, z: 0 })
        );
        assert_eq!(
            chunk.write(0, 0, 32, VoxelId(1)),
            Err(ChunkBoundsError { x: 0, y: 0, z: 32 })
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
}
