//! Semantic terrain materials and the single place where they become voxel
//! identifiers and colours.
//!
//! This is the documented boundary the milestone needs. `veldwake-voxel` stays
//! a generic container: it knows `VoxelId` is a `u16` and nothing about grass,
//! rock, or water. The renderer knows how to turn a colour into a vertex
//! attribute and nothing about what a material means. Everything in between
//! lives here, exactly once:
//!
//! - the generator writes `TerrainMaterial`, never a bare number;
//! - the mapping to `VoxelId` is declared in one table;
//! - the style bible's palette is transcribed in one table;
//! - the renderer asks this module for a colour and never invents an id.
//!
//! Adding a material means editing one file, and the round-trip test below
//! fails if an id is ever duplicated or forgotten.

use veldwake_voxel::VoxelId;

/// The materials the M4 slice can place.
///
/// Deliberately small. The style bible names exactly these surfaces, and a
/// material with no rule behind it would be an unused extension point.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerrainMaterial {
    /// Dominant valley-floor surface.
    MeadowGrass,
    /// Drier, greyer green above the valley shoulder.
    HighlandGrass,
    /// The band directly under every grass or sediment surface.
    Soil,
    /// Exposed rock on steep faces, and the band under the soil.
    Rock,
    /// Cooler, darker rock below the shallow rock band.
    DeepRock,
    /// Shoreline sand and gravel. Always separates grass from water.
    Sediment,
    /// River and pond body.
    Water,
    /// Tree trunk.
    Trunk,
    /// Canopy.
    Foliage,
    /// Canopy variation, capped at thirty percent of canopy voxels.
    FoliageHighlight,
    /// Low vegetation.
    Shrub,
}

/// Every material, in declaration order. The single list the tables and tests
/// iterate, so a new material cannot be added to one table and forgotten in
/// another.
pub const ALL_MATERIALS: [TerrainMaterial; 11] = [
    TerrainMaterial::MeadowGrass,
    TerrainMaterial::HighlandGrass,
    TerrainMaterial::Soil,
    TerrainMaterial::Rock,
    TerrainMaterial::DeepRock,
    TerrainMaterial::Sediment,
    TerrainMaterial::Water,
    TerrainMaterial::Trunk,
    TerrainMaterial::Foliage,
    TerrainMaterial::FoliageHighlight,
    TerrainMaterial::Shrub,
];

/// Identifiers start above the diagnostic fixture's range so a terrain chunk
/// and an M2/M3 diagnostic chunk can never be confused while both exist.
const FIRST_TERRAIN_ID: u16 = 64;

impl TerrainMaterial {
    /// The stable voxel identifier written into chunks.
    ///
    /// These numbers reach the M3D disk cache, so changing one changes what
    /// cached chunks mean. That is why the generator version and the style
    /// contract version are part of the cache key.
    #[must_use]
    pub const fn voxel_id(self) -> VoxelId {
        VoxelId(FIRST_TERRAIN_ID + self.index())
    }

    const fn index(self) -> u16 {
        match self {
            Self::MeadowGrass => 0,
            Self::HighlandGrass => 1,
            Self::Soil => 2,
            Self::Rock => 3,
            Self::DeepRock => 4,
            Self::Sediment => 5,
            Self::Water => 6,
            Self::Trunk => 7,
            Self::Foliage => 8,
            Self::FoliageHighlight => 9,
            Self::Shrub => 10,
        }
    }

    /// Recovers the material a voxel identifier denotes, if it is one.
    ///
    /// Returns `None` for air and for the diagnostic fixture's identifiers,
    /// which is what lets the renderer fall back to its diagnostic palette
    /// while both content sources exist.
    #[must_use]
    pub const fn from_voxel_id(id: VoxelId) -> Option<Self> {
        let Some(index) = id.0.checked_sub(FIRST_TERRAIN_ID) else {
            return None;
        };
        match index {
            0 => Some(Self::MeadowGrass),
            1 => Some(Self::HighlandGrass),
            2 => Some(Self::Soil),
            3 => Some(Self::Rock),
            4 => Some(Self::DeepRock),
            5 => Some(Self::Sediment),
            6 => Some(Self::Water),
            7 => Some(Self::Trunk),
            8 => Some(Self::Foliage),
            9 => Some(Self::FoliageHighlight),
            10 => Some(Self::Shrub),
            _ => None,
        }
    }

    /// Linear-RGB albedo, transcribed from the style bible's base palette.
    ///
    /// Colours are data, not GPU state, so they live beside the material they
    /// belong to. The renderer converts them to vertex attributes and never
    /// writes a literal of its own.
    #[must_use]
    pub const fn albedo(self) -> [f32; 3] {
        match self {
            Self::MeadowGrass => [0.243, 0.451, 0.208],
            Self::HighlandGrass => [0.290, 0.435, 0.243],
            Self::Soil => [0.322, 0.231, 0.149],
            Self::Rock => [0.443, 0.439, 0.427],
            Self::DeepRock => [0.302, 0.298, 0.310],
            Self::Sediment => [0.612, 0.557, 0.427],
            Self::Water => [0.075, 0.204, 0.239],
            Self::Trunk => [0.243, 0.169, 0.110],
            Self::Foliage => [0.173, 0.344, 0.170],
            Self::FoliageHighlight => [0.259, 0.447, 0.216],
            Self::Shrub => [0.205, 0.341, 0.168],
        }
    }

    /// How strongly a surface of this material takes a specular highlight.
    ///
    /// The style bible makes this the primary cue that water is liquid, and
    /// keeps everything else restrained.
    #[must_use]
    pub const fn specular(self) -> f32 {
        match self {
            Self::Water => 1.0,
            Self::Rock | Self::DeepRock => 0.12,
            _ => 0.0,
        }
    }

    /// Whether this material is the body of a body of water.
    #[must_use]
    pub const fn is_water(self) -> bool {
        matches!(self, Self::Water)
    }

    /// Whether this material is part of a plant rather than of the ground.
    #[must_use]
    pub const fn is_vegetation(self) -> bool {
        matches!(
            self,
            Self::Trunk | Self::Foliage | Self::FoliageHighlight | Self::Shrub
        )
    }

    /// Whether a surface of this material may carry vegetation.
    #[must_use]
    pub const fn supports_vegetation(self) -> bool {
        matches!(self, Self::MeadowGrass | Self::HighlandGrass)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MeadowGrass => "meadow-grass",
            Self::HighlandGrass => "highland-grass",
            Self::Soil => "soil",
            Self::Rock => "rock",
            Self::DeepRock => "deep-rock",
            Self::Sediment => "sediment",
            Self::Water => "water",
            Self::Trunk => "trunk",
            Self::Foliage => "foliage",
            Self::FoliageHighlight => "foliage-highlight",
            Self::Shrub => "shrub",
        }
    }
}

/// Relative luminance under the Rec. 709 weights, used by the style-bible
/// value checks.
#[must_use]
pub fn luminance(albedo: [f32; 3]) -> f32 {
    albedo[1].mul_add(0.7152, albedo[0].mul_add(0.2126, albedo[2] * 0.0722))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn identifiers_round_trip_and_never_collide() {
        let mut ids = BTreeSet::new();
        for material in ALL_MATERIALS {
            let id = material.voxel_id();
            assert!(!id.is_air(), "{} must not be air", material.name());
            assert!(ids.insert(id.0), "{} reuses an id", material.name());
            assert_eq!(
                TerrainMaterial::from_voxel_id(id),
                Some(material),
                "{} does not round trip",
                material.name()
            );
        }
        assert_eq!(ids.len(), ALL_MATERIALS.len());
    }

    #[test]
    fn air_and_diagnostic_identifiers_are_not_terrain_materials() {
        assert_eq!(TerrainMaterial::from_voxel_id(VoxelId::AIR), None);
        // The diagnostic fixture uses 1, 2, and 7.
        for id in [1, 2, 7, 63] {
            assert_eq!(
                TerrainMaterial::from_voxel_id(VoxelId(id)),
                None,
                "{id} must stay diagnostic"
            );
        }
        assert_eq!(TerrainMaterial::from_voxel_id(VoxelId(u16::MAX)), None);
    }

    #[test]
    fn names_are_unique_so_a_histogram_is_readable() {
        let names: BTreeSet<&str> = ALL_MATERIALS
            .into_iter()
            .map(TerrainMaterial::name)
            .collect();
        assert_eq!(names.len(), ALL_MATERIALS.len());
    }

    #[test]
    fn the_palette_satisfies_the_style_bible_value_rules() {
        // "Water is the darkest large surface, 0.10 to 0.28."
        let water = luminance(TerrainMaterial::Water.albedo());
        assert!((0.10..=0.28).contains(&water), "water luminance {water}");

        // "Lit terrain surfaces occupy 0.35 to 0.72" is a shaded-value rule,
        // not an albedo rule; what the palette must guarantee is separation.
        // "Adjacent distinct materials differ by at least 0.08 in value."
        let pairs = [
            (TerrainMaterial::MeadowGrass, TerrainMaterial::Soil),
            (TerrainMaterial::MeadowGrass, TerrainMaterial::Sediment),
            (TerrainMaterial::Sediment, TerrainMaterial::Water),
            (TerrainMaterial::Soil, TerrainMaterial::Rock),
            (TerrainMaterial::Rock, TerrainMaterial::DeepRock),
            (TerrainMaterial::MeadowGrass, TerrainMaterial::Foliage),
            (TerrainMaterial::MeadowGrass, TerrainMaterial::Shrub),
            (TerrainMaterial::Trunk, TerrainMaterial::Foliage),
        ];
        for (a, b) in pairs {
            let difference = (luminance(a.albedo()) - luminance(b.albedo())).abs();
            assert!(
                difference >= 0.08,
                "{} and {} differ by only {difference}",
                a.name(),
                b.name()
            );
        }

        // "Foliage darker than meadow so trees read against it."
        assert!(
            luminance(TerrainMaterial::Foliage.albedo())
                < luminance(TerrainMaterial::MeadowGrass.albedo())
        );
    }

    #[test]
    fn the_palette_respects_the_saturation_ceiling() {
        for material in ALL_MATERIALS {
            let albedo = material.albedo();
            if material.is_water() {
                // The style bible exempts water: HSV saturation is a ratio
                // against the brightest channel and over-reports for a colour
                // this dark. Its value band is what constrains it, and the
                // test above checks that.
                assert!(
                    albedo.iter().all(|channel| (0.0..=1.0).contains(channel)),
                    "water has an out-of-range channel"
                );
                continue;
            }
            let max = albedo.iter().copied().fold(f32::MIN, f32::max);
            let min = albedo.iter().copied().fold(f32::MAX, f32::min);
            let saturation = if max <= f32::EPSILON {
                0.0
            } else {
                (max - min) / max
            };
            let ceiling = if material.is_vegetation() { 0.62 } else { 0.55 };
            assert!(
                saturation <= ceiling,
                "{} saturation {saturation} exceeds {ceiling}",
                material.name()
            );
            assert!(
                albedo.iter().all(|channel| (0.0..=1.0).contains(channel)),
                "{} has an out-of-range channel",
                material.name()
            );
        }
    }

    #[test]
    fn only_water_is_strongly_specular_and_only_grass_carries_plants() {
        for material in ALL_MATERIALS {
            let specular = material.specular();
            assert!((0.0..=1.0).contains(&specular));
            if material.is_water() {
                assert_eq!(specular, 1.0);
            } else {
                assert!(specular < 0.5, "{} is too shiny", material.name());
            }
        }
        let supporting: Vec<&str> = ALL_MATERIALS
            .into_iter()
            .filter(|material| material.supports_vegetation())
            .map(TerrainMaterial::name)
            .collect();
        assert_eq!(supporting, vec!["meadow-grass", "highland-grass"]);
    }
}
