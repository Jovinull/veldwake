use std::{collections::BTreeSet, fmt};

use veldwake_voxel::{ChunkCoord, Face};

use crate::types::LodLevel;

/// How render-demand chunks are assigned a level of detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LodSelection {
    /// Every render-demand chunk is `Lod0`; the M3B behavior.
    Lod0Only,
    /// Spatial band by Chebyshev chunk distance `d` from the camera chunk:
    /// `d <= 1` is `Lod0`, `d == 2` keeps the previous level, `d >= 3` is
    /// `Lod1`. A coordinate with no history starts at `Lod1` unless `d <= 1`.
    Banded,
}

/// Chebyshev distance inside which `Banded` forces `Lod0`.
pub const BAND_LOD0_RADIUS: u32 = 1;
/// Chebyshev distance at which `Banded` keeps the previous level.
pub const BAND_TRANSITION_RADIUS: u32 = 2;

impl LodSelection {
    /// Deterministic level for a render-demand chunk at Chebyshev distance
    /// `distance`, given the level it had while retained (if any).
    #[must_use]
    pub const fn select(self, distance: u32, previous: Option<LodLevel>) -> LodLevel {
        match self {
            Self::Lod0Only => LodLevel::Lod0,
            Self::Banded => {
                if distance <= BAND_LOD0_RADIUS {
                    LodLevel::Lod0
                } else if distance == BAND_TRANSITION_RADIUS {
                    match previous {
                        Some(level) => level,
                        None => LodLevel::Lod1,
                    }
                } else {
                    LodLevel::Lod1
                }
            }
        }
    }
}

/// Tunable limits for the M3B/M3C diagnostic runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamingConfig {
    pub render_radius: u32,
    pub dependency_halo: u32,
    pub retention_radius: u32,
    pub hard_resident_cap: usize,
    /// CPU records whose eviction may be finalized in one update.
    pub max_cpu_evictions_per_update: usize,
    pub lod_selection: LodSelection,
}

impl Default for StreamingConfig {
    /// The M3B diagnostic profile: radius 1, every render chunk at `Lod0`.
    fn default() -> Self {
        Self::default_profile()
    }
}

impl StreamingConfig {
    /// `Default::default()` as a `const fn`, for const profile tables.
    #[must_use]
    pub const fn default_profile() -> Self {
        Self {
            render_radius: 1,
            dependency_halo: 1,
            retention_radius: 2,
            hard_resident_cap: 160,
            max_cpu_evictions_per_update: 8,
            lod_selection: LodSelection::Lod0Only,
        }
    }
}

impl StreamingConfig {
    /// The M3C baseline: the same visible distance, halo, retention, and cap
    /// as `m3c_diagnostic`, but every render chunk at `Lod0`. It is the
    /// no-LOD reference the LOD profile is measured against.
    #[must_use]
    pub const fn m3c_baseline() -> Self {
        Self {
            lod_selection: LodSelection::Lod0Only,
            ..Self::m3c_diagnostic()
        }
    }

    /// The M3C diagnostic profile: visible radius 3 with the banded selector.
    ///
    /// Set sizes at these radii are 343 render, 637 dependency, and 729
    /// retention chunks; the union of two retention cubes one chunk apart is
    /// 810, which the cap covers as transient headroom for this profile.
    #[must_use]
    pub const fn m3c_diagnostic() -> Self {
        Self {
            render_radius: 3,
            dependency_halo: 1,
            retention_radius: 4,
            hard_resident_cap: 810,
            max_cpu_evictions_per_update: 8,
            lod_selection: LodSelection::Banded,
        }
    }
}

impl StreamingConfig {
    /// The M4 golden profile: visible radius 6, every render chunk at `Lod0`.
    ///
    /// Wider than the M3 profiles because the M4 region has to read as a place
    /// rather than as a patch: the style bible separates foreground, midground,
    /// and background by atmosphere, and a radius that stops inside the
    /// midground leaves nothing for the background to be.
    ///
    /// `Lod0` only, deliberately. The M3C decision stands that the LOD band is
    /// opt-in, and a golden image measured against a band whose coarse seams
    /// are still under investigation would be measuring two things at once.
    /// `m4_golden_banded` exists so the band is still exercised against real
    /// content, as a separate run.
    ///
    /// The cap is the retention set plus the union headroom one chunk of
    /// movement needs, the same rule `m3c_diagnostic` uses. Most of those
    /// coordinates are authoritative absence: the region is three chunks tall,
    /// so a cubic demand set spends most of its volume above and below it and
    /// holds no payload there.
    #[must_use]
    pub const fn m4_golden() -> Self {
        Self {
            render_radius: 6,
            dependency_halo: 1,
            retention_radius: 7,
            hard_resident_cap: 4_100,
            max_cpu_evictions_per_update: 8,
            lod_selection: LodSelection::Lod0Only,
        }
    }

    /// The M4 golden profile with the M3C band enabled, for the compatibility
    /// run. Never the default: enabling LOD silently would change what every
    /// golden capture is a picture of.
    #[must_use]
    pub const fn m4_golden_banded() -> Self {
        Self {
            lod_selection: LodSelection::Banded,
            ..Self::m4_golden()
        }
    }
}

impl StreamingConfig {
    /// Rejects every configuration whose sets would contradict each other.
    ///
    /// Retention must cover the whole dependency set. The dependency set is the
    /// render cube grown by `dependency_halo` along each axis, so its extent is
    /// `render_radius + dependency_halo`. A smaller retention radius would mark
    /// dependency coordinates for eviction in the same update that created them.
    pub fn validate(self) -> Result<Self, DemandError> {
        let required_retention = self.render_radius.checked_add(self.dependency_halo).ok_or(
            DemandError::RadiusOverflow {
                render_radius: self.render_radius,
                dependency_halo: self.dependency_halo,
            },
        )?;
        if self.retention_radius < required_retention {
            return Err(DemandError::RetentionTooSmall {
                render_radius: self.render_radius,
                dependency_halo: self.dependency_halo,
                retention_radius: self.retention_radius,
                required_retention,
            });
        }
        if self.hard_resident_cap == 0 {
            return Err(DemandError::ZeroResidentCap);
        }
        if self.max_cpu_evictions_per_update == 0 {
            return Err(DemandError::ZeroEvictionBudget);
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
    RetentionTooSmall {
        render_radius: u32,
        dependency_halo: u32,
        retention_radius: u32,
        required_retention: u32,
    },
    RadiusOverflow {
        render_radius: u32,
        dependency_halo: u32,
    },
    ZeroResidentCap,
    ZeroEvictionBudget,
    RadiusTooLarge,
    CoordinateOverflow {
        center: ChunkCoord,
    },
}

impl fmt::Display for DemandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RetentionTooSmall {
                render_radius,
                dependency_halo,
                retention_radius,
                required_retention,
            } => write!(
                formatter,
                "retention radius {retention_radius} does not cover the dependency set; \
                 render radius {render_radius} plus halo {dependency_halo} requires at least \
                 {required_retention}"
            ),
            Self::RadiusOverflow {
                render_radius,
                dependency_halo,
            } => write!(
                formatter,
                "render radius {render_radius} plus dependency halo {dependency_halo} overflows u32"
            ),
            Self::ZeroResidentCap => write!(formatter, "hard resident cap must be non-zero"),
            Self::ZeroEvictionBudget => {
                write!(formatter, "CPU eviction budget per update must be non-zero")
            }
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
    fn the_m4_profile_is_wide_lod0_only_and_internally_consistent() -> Result<(), DemandError> {
        let config = StreamingConfig::m4_golden();
        assert_eq!(config.lod_selection, LodSelection::Lod0Only);
        assert!(
            config.render_radius > StreamingConfig::m3c_diagnostic().render_radius,
            "the golden slice must see further than the M3C diagnostic"
        );
        let sets = DemandSets::around(ChunkCoord::default(), config.validate()?)?;
        assert_eq!(sets.render.len(), 2_197);
        assert_eq!(sets.dependency.len(), 3_211);
        assert_eq!(sets.retention.len(), 3_375);
        // The visible radius has to reach across the valley the region builds,
        // or the far wall never appears and the frame has no background.
        assert!(
            veldwake_voxel::CHUNK_EDGE as u32 * config.render_radius >= 190,
            "the visible radius does not reach the far valley wall"
        );
        assert!(
            config.hard_resident_cap > sets.retention.len(),
            "the cap leaves no headroom for one chunk of movement"
        );

        let banded = StreamingConfig::m4_golden_banded().validate()?;
        assert_eq!(banded.lod_selection, LodSelection::Banded);
        assert_eq!(banded.render_radius, config.render_radius);
        Ok(())
    }

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

    #[test]
    fn m3c_profile_sets_have_the_documented_sizes() -> Result<(), DemandError> {
        let config = StreamingConfig::m3c_diagnostic().validate()?;
        let sets = DemandSets::around(ChunkCoord::default(), config)?;
        assert_eq!(sets.render.len(), 343);
        assert_eq!(sets.dependency.len(), 637);
        assert_eq!(sets.retention.len(), 729);
        let shifted = DemandSets::around(ChunkCoord::new(1, 0, 0), config)?;
        assert_eq!(sets.retention.union(&shifted.retention).count(), 810);
        assert!(config.hard_resident_cap >= 810);
        Ok(())
    }

    #[test]
    fn baseline_profile_differs_from_banded_only_in_selection() -> Result<(), DemandError> {
        let baseline = StreamingConfig::m3c_baseline().validate()?;
        let banded = StreamingConfig::m3c_diagnostic().validate()?;
        assert_eq!(baseline.lod_selection, LodSelection::Lod0Only);
        assert_eq!(
            StreamingConfig {
                lod_selection: LodSelection::Banded,
                ..baseline
            },
            banded
        );
        assert_ne!(baseline, StreamingConfig::default());
        Ok(())
    }

    #[test]
    fn banded_selection_follows_the_transition_band() {
        let banded = LodSelection::Banded;
        assert_eq!(banded.select(0, None), LodLevel::Lod0);
        assert_eq!(banded.select(1, Some(LodLevel::Lod1)), LodLevel::Lod0);
        assert_eq!(banded.select(2, None), LodLevel::Lod1);
        assert_eq!(banded.select(2, Some(LodLevel::Lod0)), LodLevel::Lod0);
        assert_eq!(banded.select(2, Some(LodLevel::Lod1)), LodLevel::Lod1);
        assert_eq!(banded.select(3, Some(LodLevel::Lod0)), LodLevel::Lod1);
        assert_eq!(banded.select(7, None), LodLevel::Lod1);
        for distance in 0..5 {
            assert_eq!(
                LodSelection::Lod0Only.select(distance, Some(LodLevel::Lod1)),
                LodLevel::Lod0
            );
        }
    }

    #[test]
    fn valid_configurations_are_accepted() -> Result<(), DemandError> {
        StreamingConfig::default().validate()?;
        StreamingConfig {
            render_radius: 0,
            dependency_halo: 1,
            retention_radius: 1,
            hard_resident_cap: 16,
            max_cpu_evictions_per_update: 1,
            ..StreamingConfig::default()
        }
        .validate()?;
        StreamingConfig {
            render_radius: 2,
            dependency_halo: 3,
            retention_radius: 9,
            hard_resident_cap: 1,
            max_cpu_evictions_per_update: 64,
            ..StreamingConfig::default()
        }
        .validate()?;
        Ok(())
    }

    #[test]
    fn retention_must_cover_the_dependency_halo() {
        let config = StreamingConfig {
            render_radius: 2,
            dependency_halo: 1,
            retention_radius: 2,
            ..StreamingConfig::default()
        };
        assert_eq!(
            config.validate(),
            Err(DemandError::RetentionTooSmall {
                render_radius: 2,
                dependency_halo: 1,
                retention_radius: 2,
                required_retention: 3,
            })
        );
    }

    #[test]
    fn radius_sum_uses_checked_addition() {
        let config = StreamingConfig {
            render_radius: u32::MAX,
            dependency_halo: 1,
            retention_radius: u32::MAX,
            ..StreamingConfig::default()
        };
        assert_eq!(
            config.validate(),
            Err(DemandError::RadiusOverflow {
                render_radius: u32::MAX,
                dependency_halo: 1,
            })
        );
    }

    #[test]
    fn zero_budgets_are_rejected() {
        assert_eq!(
            StreamingConfig {
                hard_resident_cap: 0,
                ..StreamingConfig::default()
            }
            .validate(),
            Err(DemandError::ZeroResidentCap)
        );
        assert_eq!(
            StreamingConfig {
                max_cpu_evictions_per_update: 0,
                ..StreamingConfig::default()
            }
            .validate(),
            Err(DemandError::ZeroEvictionBudget)
        );
    }

    #[test]
    fn oversized_radius_is_rejected_before_iteration() {
        let config = StreamingConfig {
            render_radius: u32::MAX,
            dependency_halo: 0,
            retention_radius: u32::MAX,
            ..StreamingConfig::default()
        };
        assert_eq!(
            DemandSets::around(ChunkCoord::default(), config),
            Err(DemandError::RadiusTooLarge)
        );
    }

    #[test]
    fn a_validated_dependency_set_is_always_inside_retention() -> Result<(), DemandError> {
        for (render_radius, dependency_halo, retention_radius) in
            [(0, 1, 1), (1, 1, 2), (1, 2, 3), (2, 1, 4)]
        {
            let config = StreamingConfig {
                render_radius,
                dependency_halo,
                retention_radius,
                ..StreamingConfig::default()
            };
            let sets = DemandSets::around(ChunkCoord::new(-3, 1, 2), config)?;
            assert!(
                sets.dependency.is_subset(&sets.retention),
                "dependency escaped retention for {config:?}"
            );
            assert!(sets.render.is_subset(&sets.dependency));
        }
        Ok(())
    }
}
