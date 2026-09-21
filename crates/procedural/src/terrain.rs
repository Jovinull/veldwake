//! The terrain field: a composed, inspectable height and water model.
//!
//! This is deliberately not `noise(x, z) * height`. The field is built from
//! named stages that each answer one question, and the generator can explain
//! why any point is a ridge, a slope, a valley floor, or a basin, because
//! [`TerrainSample::landform`] reports the stage that decided it.
//!
//! ```text
//! valley axis (meander)  -> distance from the axis
//!                        -> macroform: floor, shoulder, highland plateau
//!                        -> ridges, masked to the highlands
//!                        -> mid and fine detail, masked away from the floor
//!                        -> river carve, driven by the water surface
//!                        -> height
//! water surface (monotone downstream) + height -> water cells
//! height + slope + water distance -> moisture, biome zone, material
//! ```
//!
//! Every stage is a pure function of world position, the seed, and the
//! configuration. Nothing depends on which chunk is being generated, on the
//! order chunks are generated in, or on any sequential state, so a chunk
//! boundary is not a special case and negative coordinates are ordinary
//! inputs.

use crate::{
    identity::{StreamLabel, TerrainConfig, WorldIdentity},
    material::TerrainMaterial,
    noise::{Fbm, domain_warp, smoothstep, value_2d},
};

/// Which stage of the macroform decided this point. Reported so the generator
/// is inspectable rather than a black box.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Landform {
    /// Inside the river channel or the pond basin.
    Channel,
    /// The flat, walkable meadow between the channel and the shoulder.
    ValleyFloor,
    /// The rising ground between the floor and the highlands.
    Shoulder,
    /// The plateau above the shoulder, where ridges are allowed.
    Highland,
    /// A ridge crest standing above the surrounding highland.
    Ridge,
}

impl Landform {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::ValleyFloor => "valley-floor",
            Self::Shoulder => "shoulder",
            Self::Highland => "highland",
            Self::Ridge => "ridge",
        }
    }
}

/// Ecological micro-zone. One dominant biome, with zones decided by elevation,
/// moisture, slope, and exposure, never by a per-chunk random draw.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BiomeZone {
    /// The sediment margin around water.
    RiverBank,
    /// Moist, gentle meadow near the water course.
    Meadow,
    /// Meadow inside a forest pocket, where trees cluster.
    MeadowWood,
    /// The rising shoulder: sparser vegetation, thinner soil.
    Slope,
    /// Plateau grassland above the shoulder.
    Highland,
    /// Rock too steep for soil or plants.
    RockyRidge,
}

impl BiomeZone {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RiverBank => "river-bank",
            Self::Meadow => "meadow",
            Self::MeadowWood => "meadow-wood",
            Self::Slope => "slope",
            Self::Highland => "highland",
            Self::RockyRidge => "rocky-ridge",
        }
    }

    /// Fraction of placement cells that may carry a tree in this zone.
    #[must_use]
    pub const fn tree_density(self) -> f64 {
        match self {
            Self::MeadowWood => 0.62,
            Self::Meadow => 0.16,
            Self::Slope => 0.11,
            Self::Highland => 0.05,
            Self::RiverBank | Self::RockyRidge => 0.0,
        }
    }

    /// Fraction of placement cells that may carry low vegetation.
    #[must_use]
    pub const fn shrub_density(self) -> f64 {
        match self {
            Self::Meadow => 0.20,
            Self::MeadowWood => 0.14,
            Self::Slope => 0.10,
            Self::Highland => 0.07,
            Self::RiverBank => 0.05,
            Self::RockyRidge => 0.0,
        }
    }
}

/// The part of a column that depends only on its own position.
///
/// Slope is a property of a neighbourhood, not of a point, so a batch
/// generator computes one profile per column over a haloed grid and derives
/// every slope from those heights instead of evaluating the field four more
/// times per column. That is the only reason this type is public: it exists
/// for [`TerrainField::sample_from`], and the chunk generator is its consumer.
#[derive(Clone, Copy, Debug)]
pub struct ColumnProfile {
    /// Surface height in world voxels.
    pub height: f64,
    /// Height of the water surface above this column, whether or not water
    /// actually stands here.
    pub water_surface: f64,
    /// Distance from the valley axis, in voxels.
    pub axis_distance: f64,
    /// How far the macroform has risen towards the plateau, in `[0, 1]`.
    shoulder: f64,
    /// How strongly the channel is carved here, in `[0, 1]`.
    carve_profile: f64,
}

/// Everything the generator knows about one column.
#[derive(Clone, Copy, Debug)]
pub struct TerrainSample {
    /// Surface height in world voxels. The topmost solid voxel is this value
    /// rounded down.
    pub height: f64,
    /// Height of the water surface above this column.
    pub water_surface: f64,
    /// Distance from the valley axis, in voxels.
    pub axis_distance: f64,
    /// Local gradient magnitude, rise over run.
    pub slope: f64,
    /// Dryness to wetness in `[0, 1]`, from height above the water and a slow
    /// field that keeps the transition from reading as a clean ring.
    pub moisture: f64,
    pub landform: Landform,
    pub zone: BiomeZone,
}

impl TerrainSample {
    /// Index of the topmost solid voxel of the column.
    #[must_use]
    pub fn surface_y(&self) -> i64 {
        floor_to_i64(self.height)
    }

    /// Whether the **continuous** water surface stands above the continuous
    /// ground here.
    ///
    /// This is the field relation, and it is not the same question as "is there
    /// a water voxel in this column" — see [`Self::has_water_voxel`], which is
    /// the one anything about the drawn world must ask. The two genuinely
    /// disagree on a band of columns where both values floor to the same voxel:
    /// a column with `height = 15.2` under a water surface of `15.8` is
    /// submerged by this relation and has no water voxel at all.
    #[must_use]
    pub fn is_submerged(&self) -> bool {
        self.water_surface > self.height
    }

    /// Whether the generator writes at least one water voxel in this column.
    ///
    /// **The canonical predicate, and the only one that describes the world a
    /// viewer sees.** The generator fills a cell with water when
    /// `world_y > surface_y && world_y <= water_surface_y`, so such a cell
    /// exists exactly when the water surface floors above the ground surface.
    /// Terrain generation, traversal and any test about visible water all ask
    /// this one function rather than writing the comparison again.
    #[must_use]
    pub fn has_water_voxel(&self) -> bool {
        self.water_surface_y() > self.surface_y()
    }

    /// Index of the topmost water voxel above this column, whether or not
    /// water actually stands here.
    #[must_use]
    pub fn water_surface_y(&self) -> i64 {
        floor_to_i64(self.water_surface)
    }
}

/// Saturating floor, so a non-finite or absurd height cannot panic a generator
/// running on the streaming worker thread.
fn floor_to_i64(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    let floored = value.floor();
    if floored >= i64::MAX as f64 {
        i64::MAX
    } else if floored <= i64::MIN as f64 {
        i64::MIN
    } else {
        floored as i64
    }
}

/// The composed terrain field for one world identity.
#[derive(Clone, Copy, Debug)]
pub struct TerrainField {
    seed: u64,
    config: TerrainConfig,
    meander_stream: u64,
    ridge_stream: u64,
    mid_stream: u64,
    fine_stream: u64,
    warp_stream: u64,
    moisture_stream: u64,
    forest_stream: u64,
}

/// Fractal parameters, named so the intent of each field is readable at the
/// call site instead of being four numbers in a row.
const RIDGE_FIELD: Fbm = Fbm {
    feature_size: 110.0,
    octaves: 4,
    lacunarity: 2.03,
    gain: 0.5,
};
const MID_FIELD: Fbm = Fbm {
    feature_size: 34.0,
    octaves: 3,
    lacunarity: 2.11,
    gain: 0.5,
};
const FINE_FIELD: Fbm = Fbm {
    feature_size: 11.0,
    octaves: 2,
    lacunarity: 2.07,
    gain: 0.5,
};
const WARP_FIELD: Fbm = Fbm {
    feature_size: 260.0,
    octaves: 2,
    lacunarity: 2.0,
    gain: 0.5,
};
const MOISTURE_FIELD: Fbm = Fbm {
    feature_size: 140.0,
    octaves: 3,
    lacunarity: 2.0,
    gain: 0.5,
};
const FOREST_FIELD: Fbm = Fbm {
    feature_size: 96.0,
    octaves: 2,
    lacunarity: 2.0,
    gain: 0.55,
};

/// How far the sampling position is displaced by the warp field.
const WARP_STRENGTH: f64 = 18.0;
/// Horizontal step of the central difference that measures slope.
pub const SLOPE_STEP: f64 = 1.0;
/// Fraction of the ridge amplitude a point must stand above its plateau to
/// count as a crest rather than as ordinary highland.
const RIDGE_CREST_FRACTION: f64 = 0.78;
/// How far above the water surface sediment still replaces grass. This is the
/// rule that makes the shoreline read, and it is why grass never touches the
/// waterline.
pub const SHORE_RISE: f64 = 2.0;
/// Depth of the soil band under a flat surface, in voxels. It thins with slope
/// and disappears on a cliff, so a cliff face shows rock.
const MAX_SOIL_DEPTH: u32 = 3;
/// Vertical period of the rock strata, in voxels.
///
/// Rock alternates between its two tones on a fixed world-height band rather
/// than by depth below the surface. Depth alone cannot satisfy the style
/// bible's cliff rule: a heightmap cliff exposes only as many voxels per column
/// as the terrain drops, which on any walkable gradient is fewer than the depth
/// a second band would start at, so every face would be one flat colour.
/// Banding by world height instead gives every exposed face horizontal colour
/// breaks, and makes the breaks line up across neighbouring columns the way
/// sedimentary layers do.
const STRATA_BAND: i64 = 5;

impl TerrainField {
    #[must_use]
    pub fn new(identity: &WorldIdentity) -> Self {
        Self {
            seed: identity.seed.raw(),
            config: identity.config,
            meander_stream: identity.seed.stream(StreamLabel::ValleyMeander),
            ridge_stream: identity.seed.stream(StreamLabel::HighlandRidges),
            mid_stream: identity.seed.stream(StreamLabel::MidDetail),
            fine_stream: identity.seed.stream(StreamLabel::FineDetail),
            warp_stream: identity.seed.stream(StreamLabel::DomainWarp),
            moisture_stream: identity.seed.stream(StreamLabel::Moisture),
            forest_stream: identity.seed.stream(StreamLabel::ForestPockets),
        }
    }

    #[must_use]
    pub const fn config(&self) -> &TerrainConfig {
        &self.config
    }

    /// Centre of the valley in `z`, as a function of position along the axis.
    ///
    /// A smooth, low-frequency wander. Because it is a pure function of `x`,
    /// the axis is continuous everywhere, chunk boundaries included.
    #[must_use]
    pub fn valley_centre_z(&self, x: f64) -> f64 {
        value_2d(
            self.seed,
            self.meander_stream,
            x / self.config.meander_wavelength,
            0.0,
        ) * self.config.meander_amplitude
    }

    /// Height of the water surface above a point on the axis.
    ///
    /// Strictly decreasing downstream by construction, which is what makes it
    /// impossible for the river to run uphill and impossible for the surface to
    /// step at a chunk boundary: it is one continuous monotone function of
    /// world `x`, not a per-chunk decision that has to be reconciled with its
    /// neighbours.
    #[must_use]
    pub fn water_surface(&self, x: f64) -> f64 {
        let upstream_edge = f64::from(self.config.extent.min_chunk_x) * CHUNK_EDGE_F;
        let travelled = (x - upstream_edge).max(0.0);
        self.config
            .water_gradient
            .mul_add(-travelled, self.config.water_source_height)
    }

    /// How much wider and deeper the channel is here, because of the pond.
    fn pond_influence(&self, x: f64) -> f64 {
        let offset = (x - self.config.pond_centre_x).abs();
        1.0 - smoothstep(
            self.config.pond_half_length * 0.35,
            self.config.pond_half_length,
            offset,
        )
    }

    /// Surface height before the river is carved into it, with the two
    /// intermediate values the sample needs afterwards.
    fn uncarved_height(&self, x: f64, z: f64) -> (f64, f64, f64) {
        let axis_distance = (z - self.valley_centre_z(x)).abs();

        // Macroform: flat floor, then a smooth shoulder, then the plateau.
        let shoulder = smoothstep(
            self.config.valley_floor_half_width,
            self.config.highland_onset,
            axis_distance,
        );
        let macroform = self
            .config
            .highland_rise
            .mul_add(shoulder, self.config.valley_floor_height);

        // Warped coordinates break the axis alignment the detail fields would
        // otherwise inherit from the valley.
        let (wx, wz) = domain_warp(self.seed, self.warp_stream, x, z, WARP_FIELD, WARP_STRENGTH);

        // Ridges belong to the highlands only. The mask is what keeps crests
        // from sprouting on the meadow. It has to open after the floor ends and
        // close by the time the plateau is reached, whatever those two
        // distances are, or a narrow wall would invert it.
        let highland_mask = smoothstep(
            self.config.valley_floor_half_width * 1.15,
            self.config.highland_onset * 1.05,
            axis_distance,
        );
        let ridges = RIDGE_FIELD
            .sample_ridged(self.seed, self.ridge_stream, wx, wz)
            .mul_add(0.5, 0.5)
            * self.config.ridge_amplitude
            * highland_mask;

        // Mid detail is suppressed on the floor so the meadow stays walkable.
        let floor_mask = 1.0
            - smoothstep(
                self.config.valley_floor_half_width * 0.75,
                self.config.valley_floor_half_width * 1.75,
                axis_distance,
            );
        let mid = MID_FIELD.sample(self.seed, self.mid_stream, wx, wz)
            * self.config.mid_detail_amplitude
            * 0.8_f64.mul_add(-floor_mask, 1.0);

        // Fine detail exists only where the style bible allows it: on the
        // shoulder and above, never on the floor or the water margin.
        let fine = FINE_FIELD.sample(self.seed, self.fine_stream, wx, wz)
            * self.config.fine_detail_amplitude
            * highland_mask;

        (macroform + ridges + mid + fine, axis_distance, shoulder)
    }

    /// The per-column work: everything that does not need a neighbourhood.
    ///
    /// The river bed is defined relative to the water surface rather than as a
    /// fixed height, so the bed descends exactly as fast as the water does.
    /// That is what keeps the channel the same depth from one end of the region
    /// to the other without ever cutting above the waterline.
    #[must_use]
    pub fn profile(&self, x: f64, z: f64) -> ColumnProfile {
        let (uncarved, axis_distance, shoulder) = self.uncarved_height(x, z);
        let pond = self.pond_influence(x);

        let half_width = self
            .config
            .pond_extra_half_width
            .mul_add(pond, self.config.river_half_width);
        let bed_depth = self
            .config
            .pond_extra_depth
            .mul_add(pond, self.config.river_bed_depth);
        let water_surface = self.water_surface(x);
        let bed = water_surface - bed_depth;

        // A smooth bell centred on the axis: one at the axis, zero outside the
        // channel, with no discontinuity anywhere in between.
        let carve_profile = 1.0 - smoothstep(half_width * 0.35, half_width, axis_distance);
        let carve = (uncarved - bed).max(0.0) * carve_profile;

        ColumnProfile {
            height: uncarved - carve,
            water_surface,
            axis_distance,
            shoulder,
            carve_profile,
        }
    }

    /// Surface height at a world position.
    #[must_use]
    pub fn height(&self, x: f64, z: f64) -> f64 {
        self.profile(x, z).height
    }

    /// Gradient magnitude by central difference, in rise over run.
    #[must_use]
    pub fn slope(&self, x: f64, z: f64) -> f64 {
        slope_from_neighbours(
            self.height(x - SLOPE_STEP, z),
            self.height(x + SLOPE_STEP, z),
            self.height(x, z - SLOPE_STEP),
            self.height(x, z + SLOPE_STEP),
        )
    }

    /// The full sample: height, water, slope, moisture, landform, and zone.
    #[must_use]
    pub fn sample(&self, x: f64, z: f64) -> TerrainSample {
        self.sample_from(x, z, self.profile(x, z), self.slope(x, z))
    }

    /// The full sample, given a profile and a slope already computed.
    ///
    /// The batch path. Passing a slope that did not come from this field's
    /// heights produces a consistent but meaningless classification, which is
    /// why the only caller is the chunk generator that computed both.
    #[must_use]
    pub fn sample_from(&self, x: f64, z: f64, profile: ColumnProfile, slope: f64) -> TerrainSample {
        let ColumnProfile {
            height,
            water_surface,
            axis_distance,
            shoulder,
            carve_profile,
        } = profile;

        // Classified by distance from the axis against the two configured
        // widths, not by a threshold on the interpolant. The widths are what
        // the art controls actually mean, so the boundaries stay where the
        // configuration says they are however the curve is retuned.
        let landform = if carve_profile > 0.5 && height < water_surface + 0.5 {
            Landform::Channel
        } else if axis_distance <= self.config.valley_floor_half_width {
            Landform::ValleyFloor
        } else if axis_distance < self.config.highland_onset {
            Landform::Shoulder
        } else {
            let plateau = self
                .config
                .highland_rise
                .mul_add(shoulder, self.config.valley_floor_height);
            if height - plateau > self.config.ridge_amplitude * RIDGE_CREST_FRACTION {
                Landform::Ridge
            } else {
                Landform::Highland
            }
        };

        // Moisture: wet near the water surface, drier with height above it,
        // modulated by a slow field so the transition is not a clean ring.
        let proximity = 1.0 - smoothstep(1.0, 26.0, (height - water_surface).max(0.0));
        let field = MOISTURE_FIELD
            .sample(self.seed, self.moisture_stream, x, z)
            .mul_add(0.5, 0.5);
        let moisture = 0.35_f64.mul_add(field, 0.65 * proximity).clamp(0.0, 1.0);

        // Zone order is a priority order: what a place is made of first, what
        // can grow on it second.
        let zone = if slope >= self.config.cliff_slope {
            BiomeZone::RockyRidge
        } else if height <= water_surface + SHORE_RISE {
            BiomeZone::RiverBank
        } else if matches!(landform, Landform::Highland | Landform::Ridge) {
            BiomeZone::Highland
        } else if matches!(landform, Landform::Shoulder) {
            BiomeZone::Slope
        } else if 0.10_f64.mul_add(moisture, self.forest_mask(x, z)) > 0.68 {
            BiomeZone::MeadowWood
        } else {
            BiomeZone::Meadow
        };

        TerrainSample {
            height,
            water_surface,
            axis_distance,
            slope,
            moisture,
            landform,
            zone,
        }
    }

    /// Low-frequency mask that groups trees into pockets instead of scattering
    /// them evenly, which the style bible requires so the landform stays
    /// readable through the vegetation.
    #[must_use]
    pub fn forest_mask(&self, x: f64, z: f64) -> f64 {
        FOREST_FIELD
            .sample(self.seed, self.forest_stream, x, z)
            .mul_add(0.5, 0.5)
    }

    /// The material of the topmost solid voxel of a column.
    #[must_use]
    pub fn surface_material(&self, sample: &TerrainSample) -> TerrainMaterial {
        if sample.slope >= self.config.cliff_slope {
            TerrainMaterial::Rock
        } else if sample.height <= sample.water_surface + SHORE_RISE {
            // Sediment always separates grass from water.
            TerrainMaterial::Sediment
        } else if matches!(sample.landform, Landform::Highland | Landform::Ridge) {
            TerrainMaterial::HighlandGrass
        } else {
            TerrainMaterial::MeadowGrass
        }
    }

    /// The material at one voxel of a column.
    ///
    /// `depth` is zero at the surface voxel and grows downward; `world_y` is
    /// that voxel's height, which is what the strata are banded on.
    #[must_use]
    pub fn subsurface_material(
        &self,
        sample: &TerrainSample,
        depth: u32,
        world_y: i64,
    ) -> TerrainMaterial {
        let surface = self.surface_material(sample);
        if depth == 0 {
            return surface;
        }
        // Soil thins as the ground steepens and vanishes on a cliff, which is
        // why one rule produces a soil band on the meadow and bare strata on a
        // ridge face.
        let soil = if surface == TerrainMaterial::Rock {
            0
        } else if sample.slope > self.config.cliff_slope * 0.6 {
            1
        } else if sample.slope > self.config.cliff_slope * 0.3 {
            2
        } else {
            MAX_SOIL_DEPTH
        };
        if depth <= soil {
            return TerrainMaterial::Soil;
        }
        if world_y.div_euclid(STRATA_BAND).rem_euclid(2) == 0 {
            TerrainMaterial::Rock
        } else {
            TerrainMaterial::DeepRock
        }
    }
}

/// Central difference over four neighbouring heights.
#[must_use]
pub fn slope_from_neighbours(west: f64, east: f64, south: f64, north: f64) -> f64 {
    let run = 2.0 * SLOPE_STEP;
    let dx = (east - west) / run;
    let dz = (north - south) / run;
    dz.mul_add(dz, dx * dx).sqrt()
}

/// Chunk edge as a float, used wherever world and chunk space meet.
pub(crate) const CHUNK_EDGE_F: f64 = veldwake_voxel::CHUNK_EDGE as f64;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{WorldIdentity, WorldSeed};
    use std::collections::BTreeSet;

    fn golden() -> TerrainField {
        TerrainField::new(&WorldIdentity::golden())
    }

    #[test]
    fn the_field_is_a_pure_function_of_position() {
        let field = golden();
        for (x, z) in [(0.0, 0.0), (-311.5, 204.25), (137.0, -88.0)] {
            let first = field.sample(x, z);
            let second = field.sample(x, z);
            assert_eq!(first.height.to_bits(), second.height.to_bits());
            assert_eq!(first.zone, second.zone);
            assert_eq!(first.landform, second.landform);
        }
    }

    #[test]
    fn height_is_continuous_across_chunk_boundaries_and_the_origin() {
        let field = golden();
        // A discontinuity at a chunk boundary would be a visible cliff seam.
        for boundary in [-384.0, -32.0, 0.0, 32.0, 384.0] {
            for other in [-200.0, -32.0, 0.0, 17.0, 240.0] {
                let west = field.height(boundary - 1e-4, other);
                let east = field.height(boundary + 1e-4, other);
                assert!(
                    (west - east).abs() < 1e-2,
                    "seam at x={boundary}, z={other}: {west} against {east}"
                );
                let south = field.height(other, boundary - 1e-4);
                let north = field.height(other, boundary + 1e-4);
                assert!(
                    (south - north).abs() < 1e-2,
                    "seam at z={boundary}, x={other}: {south} against {north}"
                );
            }
        }
    }

    #[test]
    fn negative_coordinates_are_ordinary_inputs() {
        let field = golden();
        let mut heights = Vec::new();
        for x in [-384.0, -200.0, -1.0, 0.0, 1.0, 200.0, 384.0] {
            for z in [-384.0, -1.0, 0.0, 1.0, 384.0] {
                let sample = field.sample(x, z);
                assert!(sample.height.is_finite(), "non-finite height at {x}, {z}");
                assert!(sample.slope.is_finite() && sample.slope >= 0.0);
                assert!((0.0..=1.0).contains(&sample.moisture));
                heights.push(sample.height);
            }
        }
        let min = heights.iter().copied().fold(f64::MAX, f64::min);
        let max = heights.iter().copied().fold(f64::MIN, f64::max);
        assert!(max - min > 20.0, "the region has no relief: {min}..{max}");
    }

    #[test]
    fn the_water_surface_never_runs_uphill() {
        let field = golden();
        let mut previous = f64::MAX;
        let mut x = -400.0;
        while x <= 420.0 {
            let surface = field.water_surface(x);
            assert!(
                surface <= previous + 1e-12,
                "water rose from {previous} to {surface} at x={x}"
            );
            previous = surface;
            x += 0.5;
        }
        assert!(
            field.water_surface(-384.0) - field.water_surface(384.0) > 1.0,
            "the river has no fall"
        );
    }

    #[test]
    fn the_channel_bed_stays_below_the_water_surface_along_the_whole_axis() {
        let field = golden();
        let mut x = -380.0;
        while x <= 400.0 {
            let sample = field.sample(x, field.valley_centre_z(x));
            assert!(
                sample.height < sample.water_surface,
                "dry channel at x={x}: bed {} against water {}",
                sample.height,
                sample.water_surface
            );
            assert_eq!(sample.landform, Landform::Channel, "at x={x}");
            x += 4.0;
        }
    }

    #[test]
    fn the_whole_region_fits_between_its_floor_and_its_ceiling() {
        // Water carved below the bottom chunk would leave the pond without a
        // floor; a ridge above the top chunk would be sliced flat.
        let field = golden();
        let mut deepest = f64::MAX;
        let mut x = -384.0;
        while x <= 416.0 {
            deepest = deepest.min(field.height(x, field.valley_centre_z(x)));
            x += 1.0;
        }
        assert!(deepest > 1.0, "the channel bed reaches y={deepest}");

        let ceiling = f64::from(TerrainConfig::golden().extent.max_chunk_y + 1) * CHUNK_EDGE_F;
        let mut highest = f64::MIN;
        let mut z = -415.0;
        while z <= 415.0 {
            for x in [-384.0, -120.0, 0.0, 200.0, 415.0] {
                highest = highest.max(field.height(x, z));
            }
            z += 3.0;
        }
        assert!(highest < ceiling - 4.0, "a ridge reaches y={highest}");
    }

    #[test]
    fn the_meadow_floor_stays_above_the_waterline() {
        let field = golden();
        let config = TerrainConfig::golden();
        let mut flooded = 0;
        let mut checked = 0;
        let mut x = -360.0;
        while x <= 380.0 {
            let centre = field.valley_centre_z(x);
            for offset in [40.0, 55.0, 70.0] {
                assert!(offset > config.river_half_width + config.pond_extra_half_width);
                for side in [-1.0, 1.0] {
                    checked += 1;
                    if field.sample(x, centre + side * offset).is_submerged() {
                        flooded += 1;
                    }
                }
            }
            x += 8.0;
        }
        assert!(checked > 200);
        assert_eq!(flooded, 0, "{flooded} of {checked} meadow samples flooded");
    }

    #[test]
    fn the_valley_floor_is_walkable_and_the_highlands_are_not_flat() {
        let field = golden();
        let mut floor_slopes = Vec::new();
        let mut highland_relief = Vec::new();
        let mut x = -300.0;
        while x <= 300.0 {
            let centre = field.valley_centre_z(x);
            let floor = field.sample(x, centre + 60.0);
            if matches!(floor.landform, Landform::ValleyFloor) {
                floor_slopes.push(floor.slope);
            }
            let highland = field.sample(x, centre + 300.0);
            if matches!(highland.landform, Landform::Highland | Landform::Ridge) {
                highland_relief.push(highland.height);
            }
            x += 6.0;
        }
        assert!(!floor_slopes.is_empty() && !highland_relief.is_empty());
        // "Gradient below roughly one voxel of rise per eight voxels of run."
        let worst = floor_slopes.iter().copied().fold(f64::MIN, f64::max);
        assert!(worst < 0.125, "valley floor slope {worst}");
        let min = highland_relief.iter().copied().fold(f64::MAX, f64::min);
        let max = highland_relief.iter().copied().fold(f64::MIN, f64::max);
        assert!(max - min > 8.0, "the highlands are flat: {min}..{max}");
    }

    #[test]
    fn every_landform_and_zone_occurs_in_the_golden_region() {
        let field = golden();
        let mut landforms = BTreeSet::new();
        let mut zones = BTreeSet::new();
        let mut x = -380.0;
        while x <= 380.0 {
            let mut z = -380.0;
            while z <= 380.0 {
                let sample = field.sample(x, z);
                landforms.insert(sample.landform.name());
                zones.insert(sample.zone.name());
                z += 7.0;
            }
            x += 7.0;
        }
        for expected in ["channel", "valley-floor", "shoulder", "highland", "ridge"] {
            assert!(landforms.contains(expected), "no {expected} in the region");
        }
        for expected in [
            "river-bank",
            "meadow",
            "meadow-wood",
            "slope",
            "highland",
            "rocky-ridge",
        ] {
            assert!(zones.contains(expected), "no {expected} in the region");
        }
    }

    #[test]
    fn sediment_always_separates_grass_from_water() {
        let field = golden();
        let mut x = -300.0;
        while x <= 300.0 {
            let centre = field.valley_centre_z(x);
            // Walk out from the axis to the first grass surface. It must stand
            // clear of the waterline, with the sediment margin behind it.
            let mut offset = 0.0;
            while offset < 60.0 {
                let sample = field.sample(x, centre + offset);
                if field.surface_material(&sample).supports_vegetation() {
                    assert!(
                        sample.height > sample.water_surface + SHORE_RISE,
                        "grass at the waterline at x={x}, offset={offset}"
                    );
                    break;
                }
                offset += 0.5;
            }
            x += 9.0;
        }
    }

    #[test]
    fn soil_sits_under_grass_and_thins_to_nothing_on_a_cliff() {
        let field = golden();
        let sample = field.sample(0.0, field.valley_centre_z(0.0) + 60.0);
        let top = sample.surface_y();
        assert_eq!(
            field.subsurface_material(&sample, 0, top),
            field.surface_material(&sample)
        );
        for depth in 1..=MAX_SOIL_DEPTH {
            assert_eq!(
                field.subsurface_material(&sample, depth, top - i64::from(depth)),
                TerrainMaterial::Soil,
                "the meadow has no soil at depth {depth}"
            );
        }
        assert_ne!(
            field.subsurface_material(&sample, MAX_SOIL_DEPTH + 1, top - 4),
            TerrainMaterial::Soil,
            "the soil band never ends"
        );

        // A steep column carries no soil at all, so the face shows rock.
        let steep = TerrainSample {
            slope: 1.4,
            ..sample
        };
        assert_eq!(field.surface_material(&steep), TerrainMaterial::Rock);
        for depth in 1..12 {
            let material = field.subsurface_material(&steep, depth, top - i64::from(depth));
            assert!(
                matches!(material, TerrainMaterial::Rock | TerrainMaterial::DeepRock),
                "a cliff face shows {} at depth {depth}",
                material.name()
            );
        }
    }

    #[test]
    fn rock_strata_band_horizontally_so_a_cliff_face_has_colour_breaks() {
        let field = golden();
        // A column on the valley wall, where the exposed face is what a player
        // sees. The strata must change at least twice over the height of the
        // wall, and must agree between neighbouring columns at the same height.
        let steep = TerrainSample {
            slope: 1.4,
            ..field.sample(0.0, field.valley_centre_z(0.0) + 110.0)
        };
        let mut breaks = 0;
        let mut previous = None;
        for world_y in 20..=64 {
            let material = field.subsurface_material(&steep, 1, world_y);
            if previous.is_some_and(|earlier| earlier != material) {
                breaks += 1;
            }
            previous = Some(material);
        }
        assert!(breaks >= 2, "a whole wall shows {breaks} colour breaks");

        // Strata are a function of height alone, so they line up across columns
        // instead of stepping with each column's own surface.
        let other = TerrainSample {
            slope: 1.4,
            ..field.sample(60.0, field.valley_centre_z(60.0) + 112.0)
        };
        for world_y in 20..=64 {
            assert_eq!(
                field.subsurface_material(&steep, 1, world_y),
                field.subsurface_material(&other, 1, world_y),
                "the strata step between neighbouring columns at y={world_y}"
            );
        }
    }

    #[test]
    fn the_batch_path_agrees_with_the_direct_path() {
        let field = golden();
        for (x, z) in [(12.5, -40.5), (-201.5, 88.5), (330.5, 300.5)] {
            let direct = field.sample(x, z);
            let batched = field.sample_from(
                x,
                z,
                field.profile(x, z),
                slope_from_neighbours(
                    field.height(x - SLOPE_STEP, z),
                    field.height(x + SLOPE_STEP, z),
                    field.height(x, z - SLOPE_STEP),
                    field.height(x, z + SLOPE_STEP),
                ),
            );
            assert_eq!(direct.height.to_bits(), batched.height.to_bits());
            assert_eq!(direct.slope.to_bits(), batched.slope.to_bits());
            assert_eq!(direct.zone, batched.zone);
            assert_eq!(direct.landform, batched.landform);
        }
    }

    #[test]
    fn a_different_seed_changes_the_region_but_keeps_the_contract() {
        let golden = golden();
        let other = TerrainField::new(&WorldIdentity::new(WorldSeed(99), TerrainConfig::golden()));
        let mut differing = 0;
        let mut x = -200.0;
        while x <= 200.0 {
            if (golden.height(x, 40.0) - other.height(x, 40.0)).abs() > 0.5 {
                differing += 1;
            }
            // The water contract is geometry, not noise: it holds for every seed.
            assert!(other.water_surface(x) >= other.water_surface(x + 1.0));
            let sample = other.sample(x, other.valley_centre_z(x));
            assert!(sample.height < sample.water_surface, "dry channel at x={x}");
            x += 5.0;
        }
        assert!(differing > 20, "a new seed produced the same valley");
    }
}
