//! The landmark material table and the one place it becomes a voxel
//! identifier and a colour.
//!
//! A separate domain from `TerrainMaterial` on purpose. Both are world voxels
//! and both are written by the same generator, but they answer to different
//! contracts — the style bible constrains terrain, `LANDMARK_STYLE.md`
//! constrains these — and `ARCHITECTURE.md` requires a new content domain to
//! take the next free identifier range rather than extend somebody else's
//! enum.

use veldwake_voxel::VoxelId;

/// First identifier of the landmark range, the next free block after weapons.
pub const LANDMARK_ID_FIRST: u16 = 224;
/// One past the last identifier of the landmark range.
pub const LANDMARK_ID_END: u16 = 256;

/// The three materials the Monolith family is built from.
///
/// Three because `LANDMARK_STYLE.md` allows three: the body, the darker strata
/// and crown that keep a silhouette reading against the sky, and the pale
/// plinth a landmark stands on. A fourth would need a rule behind it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LandmarkMaterial {
    /// The body of every shaft.
    Stone,
    /// Strata courses and the crown: the darkest part of the silhouette.
    Band,
    /// The pale plinth course a landmark stands on.
    Cap,
}

/// Every landmark material, in declaration order.
pub const ALL_LANDMARK_MATERIALS: [LandmarkMaterial; 3] = [
    LandmarkMaterial::Stone,
    LandmarkMaterial::Band,
    LandmarkMaterial::Cap,
];

impl LandmarkMaterial {
    /// The stable voxel identifier written into chunks.
    #[must_use]
    pub const fn voxel_id(self) -> VoxelId {
        VoxelId(LANDMARK_ID_FIRST + self.index())
    }

    const fn index(self) -> u16 {
        match self {
            Self::Stone => 0,
            Self::Band => 1,
            Self::Cap => 2,
        }
    }

    /// Recovers the material a voxel identifier denotes, if it is one.
    #[must_use]
    pub const fn from_voxel_id(id: VoxelId) -> Option<Self> {
        let Some(index) = id.0.checked_sub(LANDMARK_ID_FIRST) else {
            return None;
        };
        match index {
            0 => Some(Self::Stone),
            1 => Some(Self::Band),
            2 => Some(Self::Cap),
            _ => None,
        }
    }

    /// Linear-RGB albedo, transcribed from `LANDMARK_STYLE.md`.
    #[must_use]
    pub const fn albedo(self) -> [f32; 3] {
        match self {
            Self::Stone => [0.310, 0.290, 0.265],
            Self::Band => [0.215, 0.203, 0.190],
            Self::Cap => [0.520, 0.500, 0.455],
        }
    }

    /// How strongly a surface takes a specular highlight.
    ///
    /// Dry, weathered stone: the same restrained value terrain rock uses, so a
    /// landmark does not read as polished.
    #[must_use]
    pub const fn specular(self) -> f32 {
        0.12
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Stone => "landmark-stone",
            Self::Band => "landmark-band",
            Self::Cap => "landmark-cap",
        }
    }

    /// Relative luminance of the albedo, which is what the style contract's
    /// value rules are written in.
    #[must_use]
    pub fn luminance(self) -> f32 {
        let [r, g, b] = self.albedo();
        0.2126_f32.mul_add(r, 0.7152_f32.mul_add(g, 0.0722 * b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{ALL_MATERIALS, TerrainMaterial};

    #[test]
    fn every_landmark_material_round_trips_through_its_identifier() {
        let mut seen = Vec::new();
        for material in ALL_LANDMARK_MATERIALS {
            let id = material.voxel_id();
            assert!(!id.is_air());
            assert!(
                (LANDMARK_ID_FIRST..LANDMARK_ID_END).contains(&id.0),
                "{} escaped the declared range",
                material.name()
            );
            assert_eq!(LandmarkMaterial::from_voxel_id(id), Some(material));
            assert!(!seen.contains(&id), "{} duplicates an id", material.name());
            seen.push(id);
        }
        assert_eq!(seen.len(), ALL_LANDMARK_MATERIALS.len());
    }

    #[test]
    fn the_landmark_range_is_disjoint_from_terrain() {
        for terrain in ALL_MATERIALS {
            assert!(
                LandmarkMaterial::from_voxel_id(terrain.voxel_id()).is_none(),
                "terrain {} is also a landmark identifier",
                terrain.name()
            );
        }
        for raw in LANDMARK_ID_FIRST..LANDMARK_ID_END {
            assert!(TerrainMaterial::from_voxel_id(VoxelId(raw)).is_none());
        }
        // And nothing outside the declared block answers.
        assert!(LandmarkMaterial::from_voxel_id(VoxelId(LANDMARK_ID_FIRST - 1)).is_none());
        assert!(LandmarkMaterial::from_voxel_id(VoxelId(LANDMARK_ID_END)).is_none());
    }

    #[test]
    fn the_palette_obeys_the_landmark_style_value_rules() {
        let stone = LandmarkMaterial::Stone.luminance();
        let band = LandmarkMaterial::Band.luminance();
        let cap = LandmarkMaterial::Cap.luminance();
        assert!(band < stone && stone < cap, "{band} {stone} {cap}");
        for (a, b) in [(stone, band), (cap, stone)] {
            assert!(
                (a - b) >= 0.08,
                "landmark materials {a} and {b} are too close"
            );
        }
        // The crown is dark enough to silhouette against the palest sky the
        // style bible allows, and the body is far enough from rock and meadow
        // grass never to read as terrain.
        assert!(band <= 0.23, "the crown is too light at {band}");
        let rock = luminance(TerrainMaterial::Rock.albedo());
        let grass = luminance(TerrainMaterial::MeadowGrass.albedo());
        for terrain in [rock, grass] {
            assert!(
                (terrain - stone).abs() >= 0.08,
                "landmark stone at {stone} is too close to terrain at {terrain}"
            );
        }
        assert!(cap <= 0.56, "the plinth is brighter than the style allows");
        for material in ALL_LANDMARK_MATERIALS {
            assert!(
                saturation(material.albedo()) <= 0.30,
                "{} is too saturated",
                material.name()
            );
            assert!((material.specular() - 0.12).abs() < 1e-6);
        }
    }

    fn luminance([r, g, b]: [f32; 3]) -> f32 {
        0.2126_f32.mul_add(r, 0.7152_f32.mul_add(g, 0.0722 * b))
    }

    fn saturation([r, g, b]: [f32; 3]) -> f32 {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        if max <= 0.0 { 0.0 } else { (max - min) / max }
    }
}
