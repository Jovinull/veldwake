//! World identity: seed, generator version, configuration, and the fingerprint
//! that ties a generated chunk to the rules that produced it.
//!
//! `WORLD-001` requires that the same seed, generator version, and compatible
//! configuration yield equivalent results. The fingerprint here is how that
//! promise is made checkable downstream: the M3D disk cache stores it, and any
//! deliberate change to generation invalidates every cached chunk instead of
//! silently replaying content the generator no longer produces.
//!
//! The runtime fingerprint is deliberately cheap: it hashes a small declared
//! descriptor, not the generated world. Verifying that the descriptor actually
//! tracks behaviour is expensive and therefore lives in tests, where a locked
//! regional signature forces a deliberate decision whenever output changes.

use crate::hash::fnv1a64;

/// Why an art-control descriptor cannot safely be compiled into terrain.
///
/// Controls are public because probes and future descriptor loading need to
/// inspect them, but arbitrary bit patterns are not valid generator input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TerrainConfigError {
    NonFinite {
        field: &'static str,
    },
    NonPositive {
        field: &'static str,
    },
    Negative {
        field: &'static str,
    },
    InvertedExtent {
        axis: &'static str,
    },
    InvalidValleyWidths,
    TreeSpacingTooSmall {
        found: i64,
        minimum: i64,
    },
    VerticalBounds {
        lowest_content: f64,
        highest_content: f64,
        min_y: i64,
        max_y_exclusive: i64,
    },
}

impl std::fmt::Display for TerrainConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite { field } => {
                write!(formatter, "terrain control {field} must be finite")
            }
            Self::NonPositive { field } => {
                write!(formatter, "terrain control {field} must be positive")
            }
            Self::Negative { field } => {
                write!(formatter, "terrain control {field} cannot be negative")
            }
            Self::InvertedExtent { axis } => {
                write!(formatter, "terrain extent is inverted on {axis}")
            }
            Self::InvalidValleyWidths => write!(
                formatter,
                "valley_floor_half_width must be smaller than highland_onset"
            ),
            Self::TreeSpacingTooSmall { found, minimum } => write!(
                formatter,
                "tree spacing {found} is below the {minimum}-voxel minimum"
            ),
            Self::VerticalBounds {
                lowest_content,
                highest_content,
                min_y,
                max_y_exclusive,
            } => write!(
                formatter,
                "terrain content [{lowest_content}, {highest_content}] exceeds vertical region [{min_y}, {max_y_exclusive})"
            ),
        }
    }
}

impl std::error::Error for TerrainConfigError {}

/// Bumped by hand for an intentional generator-contract revision that the
/// descriptor below does not already capture.
pub const TERRAIN_GENERATOR_VERSION: u32 = 1;

/// Version of `docs/audiovisual/STYLE_BIBLE.md`.
///
/// The style bible constrains geometry as well as shading, so it participates
/// in the fingerprint: changing a rule that moves a voxel must invalidate
/// cached chunks. A shading-only change does not move geometry, and the
/// document says so explicitly, but the version is bumped together either way
/// because one number is easier to reason about than two.
pub const STYLE_CONTRACT_VERSION: u32 = 1;

/// Locked by the exhaustive canonical-golden-region test in `region`. This is
/// folded into the cheap descriptor fingerprint rather than recomputed at
/// runtime: a golden-world output change must update this value deliberately,
/// which invalidates cache entries without making every cache lookup generate
/// terrain. It is not a universal multi-seed proof; general algorithm changes
/// still require the explicit [`TERRAIN_GENERATOR_VERSION`] contract bump.
pub const TERRAIN_BEHAVIOR_SIGNATURE: u64 = 0x2d88_497f_a6d6_d4b5;

/// The seed a world is generated from.
///
/// A newtype rather than a bare `u64` so a seed cannot be confused with a
/// fingerprint, a hash, or an index at a call site.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldSeed(pub u64);

impl WorldSeed {
    /// The named seed of the M4 golden slice. Every fixture, screenshot, and
    /// recorded measurement in that milestone refers to this value.
    pub const GOLDEN: Self = Self(0x5645_4c44_5741_4b45);

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Derives a stable child stream from a domain label.
    ///
    /// `DETERMINISM.md` requires named streams so that adding a draw to one
    /// system cannot perturb another. Every generator stage takes its stream
    /// from here rather than reusing the raw seed.
    #[must_use]
    pub fn stream(self, label: StreamLabel) -> u64 {
        fnv1a64(
            &[
                self.0.to_le_bytes(),
                (label as u64).to_le_bytes(),
                u64::from(TERRAIN_GENERATOR_VERSION).to_le_bytes(),
            ]
            .concat(),
        )
    }
}

/// Named random streams. Each generator stage owns exactly one, so the streams
/// stay independent and an added draw is a local change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum StreamLabel {
    ValleyMeander = 1,
    HighlandRidges = 2,
    MidDetail = 3,
    FineDetail = 4,
    DomainWarp = 5,
    Moisture = 6,
    ForestPockets = 7,
    TreePlacement = 8,
    TreeShape = 9,
    ShrubPlacement = 10,
    SurfaceVariation = 11,
}

/// The finite extent of a generated region, in chunks, inclusive.
///
/// M4 generates a vertical slice, not a world: a region large enough to walk
/// through and photograph, with authoritative absence outside it. Absence is
/// what lets the streaming runtime distinguish "nothing here, ever" from "not
/// loaded yet", exactly as the diagnostic source did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionExtent {
    pub min_chunk_x: i32,
    pub max_chunk_x: i32,
    pub min_chunk_y: i32,
    pub max_chunk_y: i32,
    pub min_chunk_z: i32,
    pub max_chunk_z: i32,
}

impl RegionExtent {
    /// The M4 golden region: a valley 800 voxels along its axis and 800 across,
    /// 96 voxels tall. Large enough for a long traversal and for background
    /// silhouettes at the style bible's 320-voxel atmosphere distance.
    pub const GOLDEN: Self = Self {
        min_chunk_x: -12,
        max_chunk_x: 12,
        min_chunk_y: 0,
        max_chunk_y: 2,
        min_chunk_z: -12,
        max_chunk_z: 12,
    };

    #[must_use]
    pub const fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        x >= self.min_chunk_x
            && x <= self.max_chunk_x
            && y >= self.min_chunk_y
            && y <= self.max_chunk_y
            && z >= self.min_chunk_z
            && z <= self.max_chunk_z
    }

    fn descriptor(&self) -> [i32; 6] {
        [
            self.min_chunk_x,
            self.max_chunk_x,
            self.min_chunk_y,
            self.max_chunk_y,
            self.min_chunk_z,
            self.max_chunk_z,
        ]
    }
}

/// Everything that decides what a world looks like, other than the seed.
///
/// The fields are art controls with semantic names. Terrain code reads them
/// instead of scattering magic constants, and the fingerprint hashes them so a
/// changed control invalidates cached chunks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainConfig {
    pub extent: RegionExtent,
    /// Height of the meadow floor, in voxels above the region base.
    pub valley_floor_height: f64,
    /// Vertical rise from the meadow floor to the highland plateau.
    pub highland_rise: f64,
    /// Half-width of the flat valley floor, measured from the valley axis.
    pub valley_floor_half_width: f64,
    /// Distance from the axis at which the highland plateau is fully reached.
    ///
    /// The gap between this and `valley_floor_half_width` is the whole width of
    /// the valley wall, so it is what decides whether the wall is a slope or a
    /// cliff: the steepest gradient a smoothstep reaches is `1.875 * rise / gap`.
    pub highland_onset: f64,
    /// Peak-to-trough amplitude of the ridge field on the highlands.
    pub ridge_amplitude: f64,
    /// Amplitude of mid-scale relief, suppressed on the valley floor.
    pub mid_detail_amplitude: f64,
    /// Amplitude of fine relief, suppressed on the floor and near water.
    pub fine_detail_amplitude: f64,
    /// How far the valley axis wanders from a straight line.
    pub meander_amplitude: f64,
    /// Along-axis wavelength of the meander.
    pub meander_wavelength: f64,
    /// Water height at the upstream edge of the region.
    pub water_source_height: f64,
    /// Fall of the water surface per voxel travelled downstream.
    pub water_gradient: f64,
    /// Depth of the river bed below the water surface.
    pub river_bed_depth: f64,
    /// Half-width of the river channel.
    pub river_half_width: f64,
    /// Centre of the pond along the valley axis, in world voxels.
    pub pond_centre_x: f64,
    /// Along-axis half-length of the pond.
    pub pond_half_length: f64,
    /// Extra channel half-width at the pond centre.
    pub pond_extra_half_width: f64,
    /// Extra bed depth at the pond centre.
    pub pond_extra_depth: f64,
    /// Rise over run at or above which a surface becomes exposed rock.
    pub cliff_slope: f64,
    /// Rise over run above which trees are rejected.
    pub tree_max_slope: f64,
    /// Spacing of the tree placement lattice, in voxels.
    pub tree_spacing: i64,
    /// Spacing of the low-vegetation placement lattice, in voxels.
    pub shrub_spacing: i64,
}

impl TerrainConfig {
    /// The configuration of the M4 golden slice. Values are the style bible's
    /// amplitude budget and proportion rules expressed as numbers.
    #[must_use]
    pub const fn golden() -> Self {
        Self {
            extent: RegionExtent::GOLDEN,
            valley_floor_height: 18.0,
            highland_rise: 48.0,
            valley_floor_half_width: 80.0,
            highland_onset: 132.0,
            ridge_amplitude: 14.0,
            mid_detail_amplitude: 4.5,
            fine_detail_amplitude: 1.6,
            meander_amplitude: 44.0,
            meander_wavelength: 330.0,
            water_source_height: 15.6,
            water_gradient: 0.0042,
            river_bed_depth: 4.2,
            river_half_width: 13.0,
            pond_centre_x: 96.0,
            pond_half_length: 86.0,
            pond_extra_half_width: 26.0,
            pond_extra_depth: 2.6,
            cliff_slope: 0.55,
            tree_max_slope: 0.35,
            tree_spacing: 8,
            shrub_spacing: 3,
        }
    }

    /// Rejects controls that would divide by zero, propagate non-finite
    /// values, invalidate the vegetation spacing proof, or clip generated
    /// terrain/vegetation at the declared vertical region boundary.
    pub fn validate(&self) -> Result<(), TerrainConfigError> {
        for (axis, low, high) in [
            ("x", self.extent.min_chunk_x, self.extent.max_chunk_x),
            ("y", self.extent.min_chunk_y, self.extent.max_chunk_y),
            ("z", self.extent.min_chunk_z, self.extent.max_chunk_z),
        ] {
            if low > high {
                return Err(TerrainConfigError::InvertedExtent { axis });
            }
        }
        for (field, value) in [
            ("valley_floor_height", self.valley_floor_height),
            ("highland_rise", self.highland_rise),
            ("valley_floor_half_width", self.valley_floor_half_width),
            ("highland_onset", self.highland_onset),
            ("ridge_amplitude", self.ridge_amplitude),
            ("mid_detail_amplitude", self.mid_detail_amplitude),
            ("fine_detail_amplitude", self.fine_detail_amplitude),
            ("meander_amplitude", self.meander_amplitude),
            ("meander_wavelength", self.meander_wavelength),
            ("water_source_height", self.water_source_height),
            ("water_gradient", self.water_gradient),
            ("river_bed_depth", self.river_bed_depth),
            ("river_half_width", self.river_half_width),
            ("pond_centre_x", self.pond_centre_x),
            ("pond_half_length", self.pond_half_length),
            ("pond_extra_half_width", self.pond_extra_half_width),
            ("pond_extra_depth", self.pond_extra_depth),
            ("cliff_slope", self.cliff_slope),
            ("tree_max_slope", self.tree_max_slope),
        ] {
            if !value.is_finite() {
                return Err(TerrainConfigError::NonFinite { field });
            }
        }
        for (field, value) in [
            ("highland_rise", self.highland_rise),
            ("ridge_amplitude", self.ridge_amplitude),
            ("mid_detail_amplitude", self.mid_detail_amplitude),
            ("fine_detail_amplitude", self.fine_detail_amplitude),
            ("meander_amplitude", self.meander_amplitude),
            ("river_bed_depth", self.river_bed_depth),
            ("pond_extra_half_width", self.pond_extra_half_width),
            ("pond_extra_depth", self.pond_extra_depth),
        ] {
            if value < 0.0 {
                return Err(TerrainConfigError::Negative { field });
            }
        }
        for (field, value) in [
            ("valley_floor_half_width", self.valley_floor_half_width),
            ("highland_onset", self.highland_onset),
            ("meander_wavelength", self.meander_wavelength),
            ("water_gradient", self.water_gradient),
            ("river_bed_depth", self.river_bed_depth),
            ("river_half_width", self.river_half_width),
            ("pond_half_length", self.pond_half_length),
            ("cliff_slope", self.cliff_slope),
            ("tree_max_slope", self.tree_max_slope),
        ] {
            if value <= 0.0 {
                return Err(TerrainConfigError::NonPositive { field });
            }
        }
        if self.valley_floor_half_width >= self.highland_onset {
            return Err(TerrainConfigError::InvalidValleyWidths);
        }
        if self.tree_spacing < 6 {
            return Err(TerrainConfigError::TreeSpacingTooSmall {
                found: self.tree_spacing,
                minimum: 6,
            });
        }
        if self.shrub_spacing <= 0 {
            return Err(TerrainConfigError::NonPositive {
                field: "shrub_spacing",
            });
        }

        let min_y = i64::from(self.extent.min_chunk_y) * 32;
        let max_y_exclusive = (i64::from(self.extent.max_chunk_y) + 1) * 32;
        // Widen before deriving the inclusive span: public descriptors may
        // legally order the complete i32 range, for which i32 subtraction
        // would overflow before validation could return its typed error.
        let region_width = (i64::from(self.extent.max_chunk_x) - i64::from(self.extent.min_chunk_x)
            + 1) as f64
            * 32.0;
        let lowest_content = self.water_source_height
            - self.water_gradient * region_width
            - self.river_bed_depth
            - self.pond_extra_depth;
        // Ridge noise is non-negative, while the signed detail fields are
        // bounded by their amplitudes. The golden region's complete generated
        // voxel envelope (including vegetation) is separately locked in
        // `generator` tests; this descriptor-only bound rejects terrain and
        // water controls that would already escape the vertical slice.
        let highest_content = self.valley_floor_height
            + self.highland_rise
            + self.ridge_amplitude
            + self.mid_detail_amplitude
            + self.fine_detail_amplitude;
        if lowest_content < min_y as f64 || highest_content >= max_y_exclusive as f64 {
            return Err(TerrainConfigError::VerticalBounds {
                lowest_content,
                highest_content,
                min_y,
                max_y_exclusive,
            });
        }
        Ok(())
    }

    /// Serialises the configuration for hashing. Floating-point controls are
    /// hashed by their bit pattern, which is exact and avoids a formatting
    /// round trip; the order is fixed here and must never be reshuffled
    /// casually, because it defines the fingerprint.
    fn descriptor(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(256);
        for value in self.extent.descriptor() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            self.valley_floor_height,
            self.highland_rise,
            self.valley_floor_half_width,
            self.highland_onset,
            self.ridge_amplitude,
            self.mid_detail_amplitude,
            self.fine_detail_amplitude,
            self.meander_amplitude,
            self.meander_wavelength,
            self.water_source_height,
            self.water_gradient,
            self.river_bed_depth,
            self.river_half_width,
            self.pond_centre_x,
            self.pond_half_length,
            self.pond_extra_half_width,
            self.pond_extra_depth,
            self.cliff_slope,
            self.tree_max_slope,
        ] {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        for value in [self.tree_spacing, self.shrub_spacing] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

/// Identity of one generated world: what to generate, and under which rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldIdentity {
    pub seed: WorldSeed,
    pub config: TerrainConfig,
}

impl WorldIdentity {
    #[must_use]
    pub const fn new(seed: WorldSeed, config: TerrainConfig) -> Self {
        Self { seed, config }
    }

    /// Validates the descriptor before it is used to generate any chunk.
    pub fn validate(&self) -> Result<(), TerrainConfigError> {
        self.config.validate()
    }

    /// The M4 golden slice: golden seed, golden configuration.
    #[must_use]
    pub const fn golden() -> Self {
        Self::new(WorldSeed::GOLDEN, TerrainConfig::golden())
    }

    /// The cheap runtime fingerprint.
    ///
    /// Hashes the seed, the generator version, the style contract version, and
    /// the full configuration descriptor. Anything that changes what the
    /// generator produces must change one of those, which is what the locked
    /// behavioural test in `region` enforces.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::with_capacity(320);
        bytes.extend_from_slice(b"veldwake.terrain");
        bytes.extend_from_slice(&self.seed.0.to_le_bytes());
        bytes.extend_from_slice(&TERRAIN_GENERATOR_VERSION.to_le_bytes());
        bytes.extend_from_slice(&STYLE_CONTRACT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&TERRAIN_BEHAVIOR_SIGNATURE.to_le_bytes());
        bytes.extend_from_slice(&self.config.descriptor());
        fnv1a64(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_is_deterministic_and_reacts_to_every_input() {
        let golden = WorldIdentity::golden();
        assert_eq!(golden.fingerprint(), golden.fingerprint());

        let other_seed = WorldIdentity::new(WorldSeed(1), TerrainConfig::golden());
        assert_ne!(golden.fingerprint(), other_seed.fingerprint());

        let mut config = TerrainConfig::golden();
        config.ridge_amplitude += 0.5;
        assert_ne!(
            golden.fingerprint(),
            WorldIdentity::new(WorldSeed::GOLDEN, config).fingerprint(),
            "an art control must invalidate cached chunks"
        );

        let mut extent = TerrainConfig::golden();
        extent.extent.max_chunk_x += 1;
        assert_ne!(
            golden.fingerprint(),
            WorldIdentity::new(WorldSeed::GOLDEN, extent).fingerprint(),
            "a region change must invalidate cached chunks"
        );

        let mut spacing = TerrainConfig::golden();
        spacing.tree_spacing += 1;
        assert_ne!(
            golden.fingerprint(),
            WorldIdentity::new(WorldSeed::GOLDEN, spacing).fingerprint()
        );
    }

    #[test]
    fn named_streams_are_independent_and_stable() {
        let seed = WorldSeed::GOLDEN;
        let labels = [
            StreamLabel::ValleyMeander,
            StreamLabel::HighlandRidges,
            StreamLabel::MidDetail,
            StreamLabel::FineDetail,
            StreamLabel::DomainWarp,
            StreamLabel::Moisture,
            StreamLabel::ForestPockets,
            StreamLabel::TreePlacement,
            StreamLabel::TreeShape,
            StreamLabel::ShrubPlacement,
            StreamLabel::SurfaceVariation,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for label in labels {
            assert_eq!(seed.stream(label), seed.stream(label), "stable");
            assert!(seen.insert(seed.stream(label)), "streams must be distinct");
        }
        assert_ne!(
            seed.stream(StreamLabel::MidDetail),
            WorldSeed(1).stream(StreamLabel::MidDetail)
        );
    }

    #[test]
    fn the_golden_region_is_finite_and_rejects_the_outside() {
        let extent = RegionExtent::GOLDEN;
        assert!(extent.contains(0, 0, 0));
        assert!(extent.contains(-12, 0, -12));
        assert!(extent.contains(12, 2, 12));
        assert!(!extent.contains(13, 0, 0));
        assert!(!extent.contains(-13, 0, 0));
        assert!(!extent.contains(0, 3, 0));
        assert!(!extent.contains(0, -1, 0));
        assert!(!extent.contains(0, 0, 13));
    }

    #[test]
    fn the_golden_configuration_satisfies_the_style_bible_proportions() {
        let config = TerrainConfig::golden();
        assert_eq!(config.validate(), Ok(()));
        // "The valley is wider than it is deep: floor width at least three
        // times the highland rise."
        let floor_width = config.valley_floor_half_width * 2.0;
        assert!(
            floor_width >= config.highland_rise * 3.0,
            "floor {floor_width} against rise {}",
            config.highland_rise
        );
        // "The river corridor occupies between one twelfth and one sixth of
        // the valley floor width."
        let corridor = config.river_half_width * 2.0;
        assert!(corridor >= floor_width / 12.0, "river too narrow");
        assert!(corridor <= floor_width / 6.0, "river too wide");
        // Amplitude budget from the style bible's detail table.
        assert!((40.0..=56.0).contains(&config.highland_rise));
        assert!((10.0..=18.0).contains(&config.ridge_amplitude));
        assert!((3.0..=6.0).contains(&config.mid_detail_amplitude));
        assert!((1.0..=2.0).contains(&config.fine_detail_amplitude));
        // Minimum tree spacing of six voxels.
        assert!(config.tree_spacing >= 6);
    }

    #[test]
    fn invalid_art_controls_are_rejected_before_generation() {
        let assert_rejected = |name: &str, config: TerrainConfig| {
            assert!(config.validate().is_err(), "{name} was accepted");
            assert!(
                crate::TerrainGenerator::new(WorldIdentity::new(WorldSeed::GOLDEN, config))
                    .is_err(),
                "{name} reached the generator"
            );
        };
        let mut config = TerrainConfig::golden();
        config.tree_spacing = 0;
        assert_rejected("zero tree spacing", config);
        config.tree_spacing = -1;
        assert_rejected("negative tree spacing", config);
        config = TerrainConfig::golden();
        config.shrub_spacing = 0;
        assert_rejected("zero shrub spacing", config);
        config.shrub_spacing = -1;
        assert_rejected("negative shrub spacing", config);
        config = TerrainConfig::golden();
        config.ridge_amplitude = f64::NAN;
        assert_rejected("nan", config);
        config.meander_wavelength = f64::INFINITY;
        assert_rejected("infinity", config);
        config = TerrainConfig::golden();
        config.extent.min_chunk_x = config.extent.max_chunk_x + 1;
        assert_rejected("inverted extent", config);
        config = TerrainConfig::golden();
        config.valley_floor_half_width = config.highland_onset;
        assert_rejected("inverted valley", config);
        config = TerrainConfig::golden();
        config.fine_detail_amplitude = -1.0;
        assert_rejected("negative amplitude", config);
        config = TerrainConfig::golden();
        config.river_bed_depth = 100.0;
        assert_rejected("outside water bed", config);
    }

    #[test]
    fn extreme_ordered_extents_return_typed_validation_results_without_overflow() {
        let mut full_range = TerrainConfig::golden();
        full_range.extent.min_chunk_x = i32::MIN;
        full_range.extent.max_chunk_x = i32::MAX;
        assert!(matches!(
            full_range.validate(),
            Err(TerrainConfigError::VerticalBounds { .. })
        ));

        let mut one_chunk_at_the_edge = TerrainConfig::golden();
        one_chunk_at_the_edge.extent.min_chunk_x = i32::MAX;
        one_chunk_at_the_edge.extent.max_chunk_x = i32::MAX;
        assert_eq!(one_chunk_at_the_edge.validate(), Ok(()));

        let mut inverted_extremes = TerrainConfig::golden();
        inverted_extremes.extent.min_chunk_x = i32::MAX;
        inverted_extremes.extent.max_chunk_x = i32::MIN;
        assert_eq!(
            inverted_extremes.validate(),
            Err(TerrainConfigError::InvertedExtent { axis: "x" })
        );
    }
}
