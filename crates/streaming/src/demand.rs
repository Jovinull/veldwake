use std::{collections::BTreeSet, fmt};

use veldwake_voxel::{ChunkCoord, Face};

/// Tunable limits for the M3B1 diagnostic runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingConfig {
    pub render_radius: u32,
    pub dependency_halo: u32,
    pub retention_radius: u32,
    pub hard_resident_cap: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            render_radius: 1,
            dependency_halo: 1,
            retention_radius: 2,
            hard_resident_cap: 160,
        }
    }
}

impl StreamingConfig {
    pub fn validate(self) -> Result<Self, DemandError> {
        if self.retention_radius < self.render_radius {
            return Err(DemandError::RetentionSmallerThanRender);
        }
        if self.hard_resident_cap == 0 {
            return Err(DemandError::ZeroResidentCap);
        }
        Ok(self)
    }
}

/// Deterministic sets derived from a chunk-space demand center.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DemandSets {
    pub render: BTreeSet<ChunkCoord>,
    pub dependency: BTreeSet<ChunkCoord>,
    pub retention: BTreeSet<ChunkCoord>,
}

impl DemandSets {
    pub fn around(center: ChunkCoord, config: StreamingConfig) -> Result<Self, DemandError> {
        let config = config.validate()?;
        let render = cube(center, config.render_radius)?;
        let mut dependency = render.clone();
        for &coord in &render {
            for face in Face::ALL {
                let mut cursor = coord;
                for _ in 0..config.dependency_halo {
                    cursor = cursor
                        .neighbor(face)
                        .ok_or(DemandError::CoordinateOverflow { center })?;
                    dependency.insert(cursor);
                }
            }
        }
        let retention = cube(center, config.retention_radius)?;
        Ok(Self {
            render,
            dependency,
            retention,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DemandError {
    RetentionSmallerThanRender,
    ZeroResidentCap,
    RadiusTooLarge,
    CoordinateOverflow { center: ChunkCoord },
}

impl fmt::Display for DemandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RetentionSmallerThanRender => {
                write!(formatter, "retention radius is smaller than render radius")
            }
            Self::ZeroResidentCap => write!(formatter, "hard resident cap must be non-zero"),
            Self::RadiusTooLarge => {
                write!(formatter, "demand radius does not fit signed iteration")
            }
            Self::CoordinateOverflow { center } => write!(
                formatter,
                "demand around ({}, {}, {}) exceeds ChunkCoord range",
                center.x, center.y, center.z
            ),
        }
    }
}

impl std::error::Error for DemandError {}

fn cube(center: ChunkCoord, radius: u32) -> Result<BTreeSet<ChunkCoord>, DemandError> {
    let radius = i32::try_from(radius).map_err(|_| DemandError::RadiusTooLarge)?;
    let mut set = BTreeSet::new();
    for dz in -radius..=radius {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                set.insert(ChunkCoord::new(
                    center
                        .x
                        .checked_add(dx)
                        .ok_or(DemandError::CoordinateOverflow { center })?,
                    center
                        .y
                        .checked_add(dy)
                        .ok_or(DemandError::CoordinateOverflow { center })?,
                    center
                        .z
                        .checked_add(dz)
                        .ok_or(DemandError::CoordinateOverflow { center })?,
                ));
            }
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_demand_counts_are_deterministic() -> Result<(), DemandError> {
        let sets = DemandSets::around(ChunkCoord::default(), StreamingConfig::default())?;
        assert_eq!(sets.render.len(), 27);
        assert_eq!(sets.dependency.len(), 81);
        assert_eq!(sets.retention.len(), 125);
        Ok(())
    }

    #[test]
    fn demand_moves_identically_across_positive_and_negative_boundaries() -> Result<(), DemandError>
    {
        let config = StreamingConfig::default();
        let negative = DemandSets::around(ChunkCoord::new(-1, 0, 0), config)?;
        let positive = DemandSets::around(ChunkCoord::new(1, 0, 0), config)?;
        assert_eq!(negative.render.len(), positive.render.len());
        assert!(negative.render.contains(&ChunkCoord::new(-2, 0, 0)));
        assert!(positive.render.contains(&ChunkCoord::new(2, 0, 0)));
        Ok(())
    }

    #[test]
    fn boundary_oscillation_has_symmetric_overlap() -> Result<(), DemandError> {
        let config = StreamingConfig::default();
        let left = DemandSets::around(ChunkCoord::new(-1, 0, 0), config)?;
        let right = DemandSets::around(ChunkCoord::new(0, 0, 0), config)?;
        assert_eq!(left.retention.intersection(&right.retention).count(), 100);
        assert_eq!(left.dependency.intersection(&right.dependency).count(), 60);
        Ok(())
    }
}
