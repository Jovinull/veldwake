//! The golden region's fixtures: named places, locked signatures, and the
//! camera poses the visual evidence is captured from.
//!
//! These exist so that an intentional change to the world is a conscious
//! decision rather than a surprise. A locked fingerprint is not a claim that
//! the current terrain is correct; it is a tripwire that makes a change visible
//! in a diff and forces whoever made it to say why.
//!
//! What is locked, and why that kind of oracle:
//!
//! - **named probes** lock meaning, not bytes: this column is a ridge, that one
//!   is a river bank. They survive a retuned amplitude and fail on a broken
//!   classification, which is the failure that matters.
//! - **chunk fingerprints** lock exact content for a small, spread-out set of
//!   chunks, including negative coordinates and the region's corners.
//! - **the regional signature** folds those fingerprints into one number, so a
//!   change anywhere in the sampled set is one failing assertion with a value
//!   to paste back.
//! - **camera poses** lock where the screenshots are taken from, so two
//!   captures months apart are comparable.
//!
//! No pixel-perfect screenshot is used as an oracle anywhere: a driver update
//! would break it without a single voxel changing.

use veldwake_voxel::{ChunkCoord, fingerprint};

use crate::{
    generator::TerrainGenerator,
    hash::fnv1a64,
    material::TerrainMaterial,
    terrain::{BiomeZone, Landform},
};

/// A named place in the golden region, with the classification it must keep.
#[derive(Clone, Copy, Debug)]
pub struct GoldenProbe {
    /// What this place is, in the language of the milestone document.
    pub name: &'static str,
    pub x: i64,
    pub z: i64,
    pub landform: Landform,
    pub zone: BiomeZone,
    pub surface: TerrainMaterial,
}

/// Named places that together cover the valley, the water, the shoulder, the
/// highlands, a cliff, a forest pocket, and negative coordinates on both axes.
pub const GOLDEN_PROBES: &[GoldenProbe] = &[
    GoldenProbe {
        name: "river-centre-origin",
        x: 0,
        z: 18,
        landform: Landform::Channel,
        zone: BiomeZone::RiverBank,
        surface: TerrainMaterial::Sediment,
    },
    GoldenProbe {
        name: "pond-centre",
        x: 96,
        z: 6,
        landform: Landform::Channel,
        zone: BiomeZone::RiverBank,
        surface: TerrainMaterial::Sediment,
    },
    GoldenProbe {
        name: "meadow-open-east",
        x: 200,
        z: 90,
        landform: Landform::ValleyFloor,
        zone: BiomeZone::Meadow,
        surface: TerrainMaterial::MeadowGrass,
    },
    GoldenProbe {
        name: "forest-pocket-north",
        x: 0,
        z: 80,
        landform: Landform::ValleyFloor,
        zone: BiomeZone::MeadowWood,
        surface: TerrainMaterial::MeadowGrass,
    },
    GoldenProbe {
        name: "forest-pocket-negative",
        x: -160,
        z: -40,
        landform: Landform::ValleyFloor,
        zone: BiomeZone::MeadowWood,
        surface: TerrainMaterial::MeadowGrass,
    },
    GoldenProbe {
        name: "grassy-shoulder-west",
        x: -200,
        z: 96,
        landform: Landform::Shoulder,
        zone: BiomeZone::Slope,
        surface: TerrainMaterial::MeadowGrass,
    },
    GoldenProbe {
        name: "cliff-face-north",
        x: 0,
        z: 126,
        landform: Landform::Shoulder,
        zone: BiomeZone::RockyRidge,
        surface: TerrainMaterial::Rock,
    },
    GoldenProbe {
        name: "cliff-face-south",
        x: -200,
        z: -96,
        landform: Landform::Shoulder,
        zone: BiomeZone::RockyRidge,
        surface: TerrainMaterial::Rock,
    },
    GoldenProbe {
        name: "highland-north",
        x: 0,
        z: 330,
        landform: Landform::Highland,
        zone: BiomeZone::Highland,
        surface: TerrainMaterial::HighlandGrass,
    },
    GoldenProbe {
        name: "ridge-crest-negative",
        x: -340,
        z: -350,
        landform: Landform::Ridge,
        zone: BiomeZone::Highland,
        surface: TerrainMaterial::HighlandGrass,
    },
    GoldenProbe {
        name: "highland-negative",
        x: -300,
        z: -330,
        landform: Landform::Highland,
        zone: BiomeZone::Highland,
        surface: TerrainMaterial::HighlandGrass,
    },
];

/// Chunks whose exact content is locked by the regional signature.
///
/// Chosen to span the region rather than to cluster: both horizontal signs, the
/// ground layer and the layer above it, the corners, the river, and the pond.
pub const SIGNATURE_CHUNKS: &[ChunkCoord] = &[
    ChunkCoord::new(0, 0, 0),
    ChunkCoord::new(0, 1, 0),
    ChunkCoord::new(0, 2, 0),
    ChunkCoord::new(3, 0, 0),
    ChunkCoord::new(-3, 0, 0),
    ChunkCoord::new(0, 0, 3),
    ChunkCoord::new(0, 0, -3),
    ChunkCoord::new(-1, 0, -1),
    ChunkCoord::new(-1, 1, -1),
    ChunkCoord::new(5, 0, -4),
    ChunkCoord::new(-8, 0, 6),
    ChunkCoord::new(-12, 0, -12),
    ChunkCoord::new(12, 0, 12),
    ChunkCoord::new(12, 2, -12),
    ChunkCoord::new(-12, 2, 12),
    // Just outside the region on each axis: absence is part of the contract.
    ChunkCoord::new(13, 0, 0),
    ChunkCoord::new(-13, 0, 0),
    ChunkCoord::new(0, 3, 0),
    ChunkCoord::new(0, -1, 0),
];

/// Locked signature of [`SIGNATURE_CHUNKS`] under the golden identity.
///
/// Update this deliberately, in the same change that alters generation, and say
/// in the milestone document what moved and why.
///
/// **Old** `0x1285_7799_1516_4f6a`, **new** `0x0cd9_5da6_1656_b11b`, **why**:
/// M8 writes three landmarks into the world, and two of the fixture chunks
/// hold part of one. Terrain, water and vegetation are unchanged outside the
/// landmark reservations; what moved is the content of those chunks.
pub const GOLDEN_REGION_SIGNATURE: u64 = 0x0cd9_5da6_1656_b11b;

/// Exhaustive signature of every present chunk in the finite canonical golden
/// region. Unlike [`GOLDEN_REGION_SIGNATURE`], which is a compact spread-out
/// fixture, this is the golden-world cache-invalidation tripwire: it is
/// deliberately test-only work and is never part of runtime source
/// fingerprinting cost. It does not prove output equivalence for arbitrary
/// seeds; an intentional general algorithm change still bumps
/// `TERRAIN_GENERATOR_VERSION`.
///
/// **Old** `0x2d88_497f_a6d6_d4b5`, **new** `0x2d59_3291_f052_9f23`, **why**:
/// M8 adds landmark voxels and removes the plants their reservations
/// suppress. The terrain field itself did not move — `TERRAIN_GENERATOR_VERSION`
/// is deliberately unchanged, because bumping it would reseed every stream and
/// regenerate the whole valley.
pub const GOLDEN_WORLD_BEHAVIOR_SIGNATURE: u64 = 0x2d59_3291_f052_9f23;

/// Folds the locked chunks into one value.
///
/// Absence and emptiness contribute differently, so a region that silently
/// stopped generating would not hash the same as one that shrank.
#[must_use]
pub fn region_signature(generator: &TerrainGenerator) -> u64 {
    let mut bytes = Vec::with_capacity(SIGNATURE_CHUNKS.len() * 24);
    for coord in SIGNATURE_CHUNKS {
        bytes.extend_from_slice(&coord.x.to_le_bytes());
        bytes.extend_from_slice(&coord.y.to_le_bytes());
        bytes.extend_from_slice(&coord.z.to_le_bytes());
        match generator.generate(*coord) {
            None => bytes.push(0),
            Some(chunk) => {
                bytes.push(1);
                bytes.extend_from_slice(&fingerprint(&chunk).to_le_bytes());
                bytes.extend_from_slice(&(chunk.solid_count() as u64).to_le_bytes());
            }
        }
    }
    fnv1a64(&bytes)
}

#[cfg(test)]
fn full_region_signature(generator: &TerrainGenerator) -> u64 {
    let extent = generator.identity().config.extent;
    let mut bytes = Vec::with_capacity(1_875 * 32);
    for z in extent.min_chunk_z..=extent.max_chunk_z {
        for y in extent.min_chunk_y..=extent.max_chunk_y {
            for x in extent.min_chunk_x..=extent.max_chunk_x {
                let coord = ChunkCoord::new(x, y, z);
                let Some(chunk) = generator.generate(coord) else {
                    panic!("declared region omitted {coord:?}");
                };
                bytes.extend_from_slice(&x.to_le_bytes());
                bytes.extend_from_slice(&y.to_le_bytes());
                bytes.extend_from_slice(&z.to_le_bytes());
                bytes.extend_from_slice(&fingerprint(&chunk).to_le_bytes());
                bytes.extend_from_slice(&(chunk.solid_count() as u64).to_le_bytes());
            }
        }
    }
    fnv1a64(&bytes)
}

/// A named camera position for visual evidence.
///
/// Yaw is degrees clockwise from `-Z`, pitch is degrees above the horizon: the
/// same convention the client's free camera uses, so a pose can be typed into
/// the client and compared with an earlier capture of the same name.
#[derive(Clone, Copy, Debug)]
pub struct CameraPose {
    pub name: &'static str,
    pub position: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    /// What this pose is meant to show, so a capture can be judged against an
    /// intention rather than against taste.
    pub intent: &'static str,
}

/// The poses every visual capture of the golden slice uses.
pub const GOLDEN_POSES: &[CameraPose] = &[
    CameraPose {
        name: "valley-wide",
        position: [-300.0, 35.0, -55.0],
        yaw_degrees: 135.0,
        pitch_degrees: -13.0,
        intent: "the valley across its width: meadow underfoot, river and forest beyond, far wall at the horizon",
    },
    CameraPose {
        name: "river-bend",
        position: [-70.0, 32.0, -14.0],
        yaw_degrees: 112.0,
        pitch_degrees: -15.0,
        intent: "water against sediment and grass, close enough to read the shoreline",
    },
    CameraPose {
        name: "pond-shore",
        // Facing back along the sun's azimuth, because a specular highlight on
        // a horizontal surface only exists where the sun's mirror image is.
        // A shot of water with the sun behind the camera proves nothing about
        // whether water carries a highlight at all.
        position: [150.0, 32.0, 70.0],
        yaw_degrees: -38.0,
        pitch_degrees: -14.0,
        intent: "the pond as a body of water, looking into the sun's reflection",
    },
    CameraPose {
        name: "cliff-face",
        position: [0.0, 31.0, 36.0],
        yaw_degrees: 180.0,
        pitch_degrees: 7.0,
        intent: "the northern valley wall: grass over soil, then banded rock",
    },
    CameraPose {
        name: "forest-pocket",
        position: [-150.0, 33.0, 70.0],
        yaw_degrees: 45.0,
        pitch_degrees: -12.0,
        intent: "tree clustering, canopy silhouettes, and low vegetation at eye level",
    },
    CameraPose {
        name: "depth-stack",
        position: [-300.0, 90.0, -112.0],
        yaw_degrees: 168.0,
        pitch_degrees: -31.0,
        intent: "foreground wall, midground valley, background ridge, separated by air alone",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{TerrainConfig, WorldSeed};
    use std::collections::BTreeSet;

    #[test]
    fn every_named_probe_still_describes_the_place_it_names() {
        let generator = TerrainGenerator::golden();
        let field = generator.field();
        for probe in GOLDEN_PROBES {
            let sample = field.sample(probe.x as f64 + 0.5, probe.z as f64 + 0.5);
            assert_eq!(
                sample.landform,
                probe.landform,
                "{} at ({}, {}) is now {}",
                probe.name,
                probe.x,
                probe.z,
                sample.landform.name()
            );
            assert_eq!(
                sample.zone,
                probe.zone,
                "{} is now zoned {}",
                probe.name,
                sample.zone.name()
            );
            assert_eq!(
                field.surface_material(&sample),
                probe.surface,
                "{} now has a {} surface",
                probe.name,
                field.surface_material(&sample).name()
            );
        }
    }

    #[test]
    fn the_probes_cover_the_region_rather_than_one_corner_of_it() {
        let landforms: BTreeSet<&str> = GOLDEN_PROBES
            .iter()
            .map(|probe| probe.landform.name())
            .collect();
        assert_eq!(landforms.len(), 5, "the probes only see {landforms:?}");
        let zones: BTreeSet<&str> = GOLDEN_PROBES
            .iter()
            .map(|probe| probe.zone.name())
            .collect();
        assert!(zones.len() >= 5, "the probes only see the zones {zones:?}");
        assert!(
            GOLDEN_PROBES.iter().any(|probe| probe.x < 0),
            "no probe at a negative x"
        );
        assert!(
            GOLDEN_PROBES.iter().any(|probe| probe.z < 0),
            "no probe at a negative z"
        );
        assert!(
            GOLDEN_PROBES
                .iter()
                .any(|probe| probe.surface == TerrainMaterial::Sediment),
            "no probe on the water margin"
        );
    }

    #[test]
    fn the_regional_signature_is_locked_and_moves_with_the_world() {
        let generator = TerrainGenerator::golden();
        let signature = region_signature(&generator);
        assert_eq!(
            signature, GOLDEN_REGION_SIGNATURE,
            "the golden region changed; update the signature deliberately and say why"
        );
        assert_eq!(signature, region_signature(&generator), "not deterministic");
        assert_ne!(
            signature,
            region_signature(&TerrainGenerator::with_seed(WorldSeed(1))),
            "the signature ignores the seed"
        );
    }

    #[test]
    fn exhaustive_world_behavior_is_locked_into_the_cache_identity() {
        use crate::identity::TERRAIN_BEHAVIOR_SIGNATURE;
        let signature = full_region_signature(&TerrainGenerator::golden());
        assert_eq!(signature, GOLDEN_WORLD_BEHAVIOR_SIGNATURE);
        assert_eq!(
            TERRAIN_BEHAVIOR_SIGNATURE, GOLDEN_WORLD_BEHAVIOR_SIGNATURE,
            "the cache identity must carry the same exhaustive behavior tripwire"
        );
    }

    #[test]
    fn the_signature_set_reaches_both_signs_both_layers_and_the_outside() {
        let extent = TerrainConfig::golden().extent;
        assert!(SIGNATURE_CHUNKS.iter().any(|coord| coord.x < 0));
        assert!(SIGNATURE_CHUNKS.iter().any(|coord| coord.z < 0));
        assert!(SIGNATURE_CHUNKS.iter().any(|coord| coord.y > 0));
        assert!(
            SIGNATURE_CHUNKS
                .iter()
                .any(|coord| !extent.contains(coord.x, coord.y, coord.z)),
            "the signature never checks that absence is still absent"
        );
        let unique: BTreeSet<ChunkCoord> = SIGNATURE_CHUNKS.iter().copied().collect();
        assert_eq!(unique.len(), SIGNATURE_CHUNKS.len(), "a duplicated chunk");
    }

    #[test]
    fn the_camera_poses_are_named_distinct_and_inside_the_region() {
        let extent = TerrainConfig::golden().extent;
        let edge = veldwake_voxel::CHUNK_EDGE as f32;
        let min_x = extent.min_chunk_x as f32 * edge;
        let max_x = (extent.max_chunk_x + 1) as f32 * edge;
        let min_z = extent.min_chunk_z as f32 * edge;
        let max_z = (extent.max_chunk_z + 1) as f32 * edge;
        let ceiling = (extent.max_chunk_y + 1) as f32 * edge;

        let mut names = BTreeSet::new();
        for pose in GOLDEN_POSES {
            assert!(names.insert(pose.name), "{} is not unique", pose.name);
            assert!(
                !pose.intent.is_empty(),
                "{} has no stated intent",
                pose.name
            );
            assert!(
                (min_x..max_x).contains(&pose.position[0])
                    && (min_z..max_z).contains(&pose.position[2]),
                "{} stands outside the region",
                pose.name
            );
            assert!(
                pose.position[1] > 0.0 && pose.position[1] < ceiling,
                "{} stands outside the vertical extent",
                pose.name
            );
            assert!(
                (-90.0..=90.0).contains(&pose.pitch_degrees),
                "{} has an impossible pitch",
                pose.name
            );
        }
        assert!(GOLDEN_POSES.len() >= 5, "too few poses for real evidence");
    }

    #[test]
    fn every_pose_stands_in_open_air_above_the_ground() {
        // The first set of poses was written before the terrain existed and put
        // the camera inside a hillside, where the whole frame is backfaces and
        // sky. A pose has to clear the column it stands in.
        let generator = TerrainGenerator::golden();
        let field = generator.field();
        for pose in GOLDEN_POSES {
            let sample = field.sample(
                f64::from(pose.position[0]) + 0.5,
                f64::from(pose.position[2]) + 0.5,
            );
            let clearance = f64::from(pose.position[1]) - sample.height;
            assert!(
                clearance >= 4.0,
                "{} stands {clearance:.1} voxels above ground {:.1}",
                pose.name,
                sample.height
            );
            assert!(
                clearance <= 40.0,
                "{} floats {clearance:.1} voxels above the ground",
                pose.name
            );
            assert!(!sample.is_submerged(), "{} stands in the water", pose.name);

            // A camera inside a canopy sees one flat green wall. Clearing the
            // ground is not enough; it has to clear what grows on it.
            let x = pose.position[0] as i64;
            let y = pose.position[1] as i64;
            let z = pose.position[2] as i64;
            for offset in -1..=1 {
                assert!(
                    !generator.vegetation().occupied(x, y + offset, z),
                    "{} stands inside a plant",
                    pose.name
                );
            }
        }
    }
}
