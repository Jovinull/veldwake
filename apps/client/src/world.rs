//! Which world the client streams, and where the camera starts in it.
//!
//! Selection is by environment variable rather than by command-line argument so
//! the client keeps no argument-parsing dependency, exactly as the streaming
//! profile already does. Everything here is parsing and naming: the worlds
//! themselves belong to `veldwake-procedural`, and the diagnostic corridor
//! belongs to `veldwake-streaming`.

use glam::Vec3;
use tracing::warn;
use veldwake_procedural::{
    TerrainGenerator, WorldSeed,
    region::{CameraPose, GOLDEN_POSES},
};
use veldwake_streaming::{ChunkSource, DiagnosticChunkSource, TerrainChunkSource};

use crate::camera::Camera;

/// The continuous horizontal extent of a generated region, in world units.
///
/// **One interpretation of the region's edge, shared by everything that needs
/// it.** A finite region is a whole number of chunks, so its edge is a
/// half-open rectangle: the last column of the last chunk is inside, and the
/// coordinate one past it is not. Writing that arithmetic twice is how two
/// answers to "is this inside the world" start disagreeing at a fractional
/// coordinate nobody tested.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionBounds {
    min_x: f64,
    max_x_exclusive: f64,
    min_z: f64,
    max_z_exclusive: f64,
}

impl RegionBounds {
    /// The horizontal extent of everything a generator will produce.
    #[must_use]
    pub fn of(generator: &TerrainGenerator) -> Self {
        let extent = generator.identity().config.extent;
        let edge = f64::from(u32::try_from(veldwake_voxel::CHUNK_EDGE).unwrap_or(u32::MAX));
        Self {
            min_x: f64::from(extent.min_chunk_x) * edge,
            max_x_exclusive: (f64::from(extent.max_chunk_x) + 1.0) * edge,
            min_z: f64::from(extent.min_chunk_z) * edge,
            max_z_exclusive: (f64::from(extent.max_chunk_z) + 1.0) * edge,
        }
    }

    /// Whether a horizontal position lies inside the region.
    ///
    /// Non-finite input is outside, never a panic and never a silent zero.
    #[must_use]
    pub fn contains(&self, x: f64, z: f64) -> bool {
        x.is_finite()
            && z.is_finite()
            && (self.min_x..self.max_x_exclusive).contains(&x)
            && (self.min_z..self.max_z_exclusive).contains(&z)
    }

    #[must_use]
    pub const fn min_x(&self) -> f64 {
        self.min_x
    }

    #[must_use]
    pub const fn max_x_exclusive(&self) -> f64 {
        self.max_x_exclusive
    }

    #[must_use]
    pub const fn min_z(&self) -> f64 {
        self.min_z
    }

    #[must_use]
    pub const fn max_z_exclusive(&self) -> f64 {
        self.max_z_exclusive
    }
}

/// Environment variable naming the world.
const WORLD_VARIABLE: &str = "VELDWAKE_WORLD";
/// Environment variable naming the starting camera pose.
const POSE_VARIABLE: &str = "VELDWAKE_POSE";

/// The worlds the client can stream.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorldSelection {
    /// The M4 golden slice: the seed every fixture, screenshot, and recorded
    /// measurement refers to.
    #[default]
    Golden,
    /// The golden configuration under another seed. Explicit seed selection for
    /// looking at a different valley, not a configuration system.
    Seeded(u64),
    /// The finite M3 checkerboard corridor, kept so the streaming regressions
    /// can still be looked at.
    Diagnostic,
}

impl WorldSelection {
    /// Reads `VELDWAKE_WORLD`. Unset means the golden slice; an unparsable
    /// value warns and falls back to it rather than failing to start.
    #[must_use]
    pub fn from_environment() -> Self {
        match std::env::var(WORLD_VARIABLE) {
            Ok(value) => match Self::parse(&value) {
                Some(selection) => selection,
                None => {
                    warn!(%value, "unknown VELDWAKE_WORLD; using the golden slice");
                    Self::Golden
                }
            },
            Err(_) => Self::Golden,
        }
    }

    /// `golden`, `diagnostic`, or `seed:<value>` in decimal or `0x` hex.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "" | "golden" | "terrain" | "m4" => Some(Self::Golden),
            "diagnostic" | "fixture" | "m3" => Some(Self::Diagnostic),
            other => {
                let raw = other
                    .strip_prefix("seed:")
                    .or_else(|| other.strip_prefix("seed="))?;
                let parsed = match raw.strip_prefix("0x") {
                    Some(hex) => u64::from_str_radix(hex, 16),
                    None => raw.parse(),
                };
                parsed.ok().map(Self::Seeded)
            }
        }
    }

    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Golden => "golden".to_owned(),
            Self::Seeded(seed) => format!("seed:{seed:#018x}"),
            Self::Diagnostic => "diagnostic".to_owned(),
        }
    }

    /// The streaming source for this world.
    #[must_use]
    pub fn source(self) -> Box<dyn ChunkSource> {
        match self {
            Self::Golden => Box::new(TerrainChunkSource::golden()),
            Self::Seeded(seed) => Box::new(TerrainChunkSource::new(TerrainGenerator::with_seed(
                WorldSeed(seed),
            ))),
            Self::Diagnostic => Box::new(DiagnosticChunkSource),
        }
    }

    /// The terrain generator behind this world, when it has one.
    ///
    /// The diagnostic corridor is not generated terrain and has no
    /// walkable surface to query, so it answers `None` rather than
    /// inventing a floor.
    #[must_use]
    pub fn generator(self) -> Option<TerrainGenerator> {
        match self {
            Self::Golden => Some(TerrainGenerator::golden()),
            Self::Seeded(seed) => Some(TerrainGenerator::with_seed(WorldSeed(seed))),
            Self::Diagnostic => None,
        }
    }

    /// Identity of everything this world generates.
    ///
    /// The disk cache is keyed on it, so switching worlds cannot replay another
    /// world's chunks: every stored entry simply reads as stale.
    #[must_use]
    pub fn fingerprint(self) -> u64 {
        self.source().fingerprint()
    }

    /// Where the camera starts.
    ///
    /// A procedural world starts at a named pose so a capture is reproducible;
    /// the diagnostic corridor keeps the position the M3 smokes used, so its
    /// screenshots stay comparable with the ones already recorded.
    #[must_use]
    pub fn spawn_camera(self, requested_pose: Option<&str>) -> Camera {
        match self {
            Self::Diagnostic => Camera::default(),
            Self::Golden | Self::Seeded(_) => {
                let pose = resolve_pose(requested_pose);
                Camera::at(
                    Vec3::from_array(pose.position),
                    pose.yaw_degrees,
                    pose.pitch_degrees,
                )
            }
        }
    }
}

/// Reads `VELDWAKE_POSE`.
#[must_use]
pub fn requested_pose() -> Option<String> {
    std::env::var(POSE_VARIABLE).ok()
}

/// The named pose, or the first one when the name is unknown or absent.
#[must_use]
pub fn resolve_pose(requested: Option<&str>) -> &'static CameraPose {
    let Some(first) = GOLDEN_POSES.first() else {
        unreachable!("the golden region declares no camera poses");
    };
    let Some(name) = requested else {
        return first;
    };
    let wanted = name.trim().to_ascii_lowercase();
    match GOLDEN_POSES
        .iter()
        .find(|pose| pose.name.eq_ignore_ascii_case(&wanted))
    {
        Some(pose) => pose,
        None => {
            let known: Vec<&str> = GOLDEN_POSES.iter().map(|pose| pose.name).collect();
            warn!(%name, ?known, "unknown VELDWAKE_POSE; using the first pose");
            first
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_procedural_world_has_a_terrain_generator() {
        assert!(WorldSelection::Golden.generator().is_some());
        assert!(WorldSelection::Seeded(7).generator().is_some());
        assert!(WorldSelection::Diagnostic.generator().is_none());
        let Some(golden) = WorldSelection::Golden.generator() else {
            panic!("the golden world has a generator");
        };
        assert_eq!(golden.fingerprint(), WorldSelection::Golden.fingerprint());
    }

    #[test]
    fn world_names_parse_including_an_explicit_seed() {
        assert_eq!(WorldSelection::parse(""), Some(WorldSelection::Golden));
        assert_eq!(
            WorldSelection::parse("golden"),
            Some(WorldSelection::Golden)
        );
        assert_eq!(WorldSelection::parse(" M4 "), Some(WorldSelection::Golden));
        assert_eq!(
            WorldSelection::parse("diagnostic"),
            Some(WorldSelection::Diagnostic)
        );
        assert_eq!(
            WorldSelection::parse("seed:42"),
            Some(WorldSelection::Seeded(42))
        );
        assert_eq!(
            WorldSelection::parse("seed=0xFF"),
            Some(WorldSelection::Seeded(255))
        );
        assert_eq!(WorldSelection::parse("seed:"), None);
        assert_eq!(WorldSelection::parse("seed:zzz"), None);
        assert_eq!(WorldSelection::parse("nonsense"), None);
    }

    #[test]
    fn every_world_has_its_own_identity() {
        let golden = WorldSelection::Golden.fingerprint();
        let seeded = WorldSelection::Seeded(1).fingerprint();
        let diagnostic = WorldSelection::Diagnostic.fingerprint();
        assert_ne!(golden, seeded, "two worlds share a cache key");
        assert_ne!(golden, diagnostic);
        assert_ne!(seeded, diagnostic);
        assert_eq!(golden, WorldSelection::Golden.fingerprint(), "not stable");
        assert!(!WorldSelection::Golden.name().is_empty());
        assert!(WorldSelection::Seeded(9).name().contains("seed"));
    }

    #[test]
    fn a_pose_name_selects_a_pose_and_an_unknown_one_falls_back() {
        for pose in GOLDEN_POSES {
            assert_eq!(resolve_pose(Some(pose.name)).name, pose.name);
            // Case and surrounding space must not matter at a command line.
            let shouted = pose.name.to_ascii_uppercase();
            assert_eq!(resolve_pose(Some(&format!(" {shouted} "))).name, pose.name);
        }
        let Some(first) = GOLDEN_POSES.first() else {
            panic!("the golden region declares no camera poses");
        };
        assert_eq!(resolve_pose(None).name, first.name);
        assert_eq!(resolve_pose(Some("not-a-pose")).name, first.name);
    }

    #[test]
    fn a_procedural_world_starts_at_a_named_pose_and_the_fixture_does_not() {
        let Some(pose) = GOLDEN_POSES.iter().find(|pose| pose.name == "river-bend") else {
            panic!("the river-bend pose disappeared");
        };
        let camera = WorldSelection::Golden.spawn_camera(Some("river-bend"));
        assert_eq!(camera.position(), Vec3::from_array(pose.position));
        // The forward vector must actually point somewhere finite.
        assert!(camera.forward().is_finite());

        let fixture = WorldSelection::Diagnostic.spawn_camera(Some("river-bend"));
        assert_eq!(fixture.position(), Camera::default().position());
    }
}
