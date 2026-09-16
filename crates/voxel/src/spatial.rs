use std::fmt;

use crate::{CHUNK_EDGE, Face, LocalCoord};

/// Signed address of one chunk in the right-handed, Y-up voxel grid.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Returns an axial neighbor, or `None` rather than wrapping at `i32` limits.
    #[must_use]
    pub fn neighbor(self, face: Face) -> Option<Self> {
        match face {
            Face::NegativeX => self.x.checked_sub(1).map(|x| Self { x, ..self }),
            Face::PositiveX => self.x.checked_add(1).map(|x| Self { x, ..self }),
            Face::NegativeY => self.y.checked_sub(1).map(|y| Self { y, ..self }),
            Face::PositiveY => self.y.checked_add(1).map(|y| Self { y, ..self }),
            Face::NegativeZ => self.z.checked_sub(1).map(|z| Self { z, ..self }),
            Face::PositiveZ => self.z.checked_add(1).map(|z| Self { z, ..self }),
        }
    }

    /// Converts a validated chunk-local coordinate into a global voxel coordinate.
    #[must_use]
    pub fn to_world_voxel(self, local: LocalCoord) -> WorldVoxelCoord {
        let edge = CHUNK_EDGE as i64;
        WorldVoxelCoord {
            x: i64::from(self.x) * edge + local.x() as i64,
            y: i64::from(self.y) * edge + local.y() as i64,
            z: i64::from(self.z) * edge + local.z() as i64,
        }
    }
}

/// Signed global address of one voxel cell.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldVoxelCoord {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl WorldVoxelCoord {
    #[must_use]
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    /// Splits a world voxel into its canonical chunk and non-negative local coordinate.
    pub fn split(self) -> Result<(ChunkCoord, LocalCoord), WorldCoordinateRangeError> {
        let edge = CHUNK_EDGE as i64;
        let quotient = [
            self.x.div_euclid(edge),
            self.y.div_euclid(edge),
            self.z.div_euclid(edge),
        ];
        let [x, y, z] = quotient.map(i32::try_from);
        let chunk = ChunkCoord {
            x: x.map_err(|_| WorldCoordinateRangeError { world: self })?,
            y: y.map_err(|_| WorldCoordinateRangeError { world: self })?,
            z: z.map_err(|_| WorldCoordinateRangeError { world: self })?,
        };
        let local = checked_local_from_remainders(
            self.x.rem_euclid(edge),
            self.y.rem_euclid(edge),
            self.z.rem_euclid(edge),
        );
        Ok((chunk, local))
    }
}

/// A world coordinate whose canonical chunk quotient does not fit `ChunkCoord`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldCoordinateRangeError {
    pub world: WorldVoxelCoord,
}

impl fmt::Display for WorldCoordinateRangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "world voxel ({}, {}, {}) is outside the i32 chunk-coordinate range",
            self.world.x, self.world.y, self.world.z
        )
    }
}

impl std::error::Error for WorldCoordinateRangeError {}

fn checked_local_from_remainders(x: i64, y: i64, z: i64) -> LocalCoord {
    match LocalCoord::new(x as usize, y as usize, z as usize) {
        Ok(local) => local,
        Err(error) => unreachable!("Euclidean remainder violated local-coordinate bounds: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_boundaries_are_canonical_on_every_axis() -> Result<(), Box<dyn std::error::Error>>
    {
        let cases = [
            (32, 1, 0),
            (31, 0, 31),
            (0, 0, 0),
            (-1, -1, 31),
            (-32, -1, 0),
            (-33, -2, 31),
        ];

        for (world_value, expected_chunk, expected_local) in cases {
            for axis in 0..3 {
                let mut world = [0_i64; 3];
                world[axis] = world_value;
                let (chunk, local) = WorldVoxelCoord::new(world[0], world[1], world[2]).split()?;
                assert_eq!([chunk.x, chunk.y, chunk.z][axis], expected_chunk);
                assert_eq!([local.x(), local.y(), local.z()][axis], expected_local);
            }
        }
        Ok(())
    }

    #[test]
    fn chunk_local_world_round_trip_covers_all_axes_and_extremes()
    -> Result<(), Box<dyn std::error::Error>> {
        let chunk_coords = [
            ChunkCoord::new(i32::MIN, 0, i32::MAX),
            ChunkCoord::new(-2, -1, 0),
            ChunkCoord::new(1, 2, 3),
        ];
        let locals = [LocalCoord::new(0, 31, 1)?, LocalCoord::new(31, 0, 30)?];

        for chunk in chunk_coords {
            for local in locals {
                let world = chunk.to_world_voxel(local);
                assert_eq!(world.split()?, (chunk, local));
            }
        }
        Ok(())
    }

    #[test]
    fn world_to_chunk_rejects_quotients_outside_i32() {
        let edge = CHUNK_EDGE as i64;
        let above = WorldVoxelCoord::new((i64::from(i32::MAX) + 1) * edge, 0, 0);
        let below = WorldVoxelCoord::new(i64::from(i32::MIN) * edge - 1, 0, 0);

        assert_eq!(
            above.split(),
            Err(WorldCoordinateRangeError { world: above })
        );
        assert_eq!(
            below.split(),
            Err(WorldCoordinateRangeError { world: below })
        );
    }

    #[test]
    fn axial_neighbors_use_checked_arithmetic() {
        let origin = ChunkCoord::default();
        for face in Face::ALL {
            let neighbor = origin.neighbor(face);
            assert!(neighbor.is_some(), "origin must have neighbor at {face:?}");
        }

        assert_eq!(
            ChunkCoord::new(i32::MIN, 4, 5).neighbor(Face::NegativeX),
            None
        );
        assert_eq!(
            ChunkCoord::new(i32::MAX, 4, 5).neighbor(Face::PositiveX),
            None
        );
        assert_eq!(
            ChunkCoord::new(4, i32::MIN, 5).neighbor(Face::NegativeY),
            None
        );
        assert_eq!(
            ChunkCoord::new(4, i32::MAX, 5).neighbor(Face::PositiveY),
            None
        );
        assert_eq!(
            ChunkCoord::new(4, 5, i32::MIN).neighbor(Face::NegativeZ),
            None
        );
        assert_eq!(
            ChunkCoord::new(4, 5, i32::MAX).neighbor(Face::PositiveZ),
            None
        );
    }
}
