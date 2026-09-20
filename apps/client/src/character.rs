//! The client's character adapter: which humanoid, where it stands, and what
//! the ground under it is.
//!
//! `veldwake-character` is headless and knows nothing about world generation.
//! This module is the whole boundary between it and the M4 terrain:
//! [`TerrainGround`] answers `GroundSampler` from a `TerrainField`, and
//! [`CharacterScene`] owns the compiled character, its runtime state, and the
//! diagnostic course that makes a capture reproducible.
//!
//! **What the ground contract means here.** `GroundSampler::surface` returns
//! the top face of the topmost solid voxel of a column — the surface a viewer
//! can see, which is a step function because the terrain is made of blocks. It
//! is deliberately not the terrain field's continuous height: a foot placed on
//! a smooth surface either floats above the block it is standing on or sinks
//! into it, and the image is what M5 is judged on.
//!
//! Water is not ground. A column whose surface voxel is water reports the solid
//! bed under it, so a character never stands on a river.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::Vec3;
use tracing::warn;
use veldwake_character::{
    CharacterCompiler, CharacterCourse, CharacterDescriptor, CharacterError, CharacterState,
    CompiledCharacter, CourseLeg, GroundSampler, PosedCharacter,
    fixture::{
        NAMED_POSES, NamedPose, golden_descriptor, state_for, sturdy_descriptor, walk_cycle_phases,
    },
    pose::pose,
};
use veldwake_procedural::{TerrainField, TerrainGenerator, terrain::TerrainSample};

use crate::camera::Camera;
use crate::world::RegionBounds;

/// Environment variable selecting what the character does.
const CHARACTER_VARIABLE: &str = "VELDWAKE_CHARACTER";

/// The walkable surface of one terrain field.
///
/// Holds a borrow rather than a generator so the adapter costs nothing and
/// cannot outlive the world it describes.
#[derive(Clone, Copy, Debug)]
pub struct TerrainGround<'a> {
    field: &'a TerrainField,
    /// Half-open continuous world bounds of the region, so absence stays
    /// absence without discarding the fractional half of an edge voxel.
    bounds: RegionBounds,
}

impl<'a> TerrainGround<'a> {
    /// Borrows the walkable surface of a generator's region.
    #[must_use]
    pub fn new(generator: &'a TerrainGenerator) -> Self {
        Self {
            field: generator.field(),
            bounds: RegionBounds::of(generator),
        }
    }

    /// The region this adapter answers inside.
    #[must_use]
    pub const fn bounds(&self) -> RegionBounds {
        self.bounds
    }

    /// The terrain sample under a position, for tests and diagnostics.
    #[must_use]
    pub fn sample(&self, x: f64, z: f64) -> TerrainSample {
        self.field.sample(x, z)
    }

    #[must_use]
    fn inside(&self, x: f64, z: f64) -> bool {
        self.bounds.contains(x, z)
    }
}

impl GroundSampler for TerrainGround<'_> {
    fn surface(&self, x: f64, z: f64) -> Option<f64> {
        if !x.is_finite() || !z.is_finite() || !self.inside(x, z) {
            return None;
        }
        let sample = self.field.sample(x, z);
        // `surface_y` is the index of the topmost solid voxel, so the face a
        // sole rests on is one above it. Water standing over that column does
        // not raise the ground: a character walks on the bed, not on the river.
        Some((sample.surface_y() + 1) as f64)
    }
}

/// What the client does with the character.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CharacterSelection {
    /// No character at all. The M3 and M4 regression smokes use this.
    Off,
    /// The golden humanoid standing at the course's start, breathing.
    #[default]
    Idle,
    /// Held at one named pose, for the proportion and articulation captures.
    Pose(&'static NamedPose),
    /// Held at one phase of a gait, for the walk-cycle contact sheet.
    ///
    /// Eight separate runs at eight phases is better evidence than a timed
    /// burst: every tile is exactly reproducible and none of them depends on
    /// when a screenshot happened to land.
    Frozen { speed: f32, phase: f32 },
    /// Driven along the diagnostic course.
    Course,
    /// Driven up and down the terraced strip, for the contact captures.
    Slope,
    /// Standing on one terrace of that strip, a step in front of the toes.
    SlopeStand,
    /// Walking out and back across the portrait clearing, close to the camera.
    Clearing,
    /// The sturdy fixture, standing, so two bodies can be compared in place.
    Sturdy,
}

impl CharacterSelection {
    /// Where this selection puts the character: `(x, z, facing)`.
    ///
    /// The course starts on its corridor; everything else stands in the
    /// portrait clearing, facing the front camera.
    #[must_use]
    pub fn stand_point(self) -> (f32, f32, f32) {
        match self {
            Self::Course => {
                let start = GOLDEN_COURSE.sample(0.0);
                (start.x, start.z, start.facing)
            }
            Self::Slope => {
                let start = SLOPE_COURSE.sample(0.0);
                (start.x, start.z, start.facing)
            }
            Self::SlopeStand => (SLOPE_STAND_X, SLOPE_STAND_Z, -FRAC_PI_2),
            Self::Clearing => {
                let start = CLEARING_COURSE.sample(0.0);
                (start.x, start.z, start.facing)
            }
            // Facing `-Z`, and the sun is why. M4's key light comes from an
            // azimuth of -38 degrees, so it arrives from `-X` and `-Z`: a
            // character facing `+X` turns its whole front away from it and
            // every portrait came back in its own shade. Facing `-Z` puts the
            // key on the front and the fill on the character's left, which is
            // an ordinary three-quarter key and the reason the portrait poses
            // below stand where they do.
            _ => (PORTRAIT_X, PORTRAIT_Z, 0.0),
        }
    }

    /// Reads `VELDWAKE_CHARACTER`. Unset means idle; an unparsable value warns
    /// and falls back rather than failing to start.
    #[must_use]
    pub fn from_environment() -> Self {
        match std::env::var(CHARACTER_VARIABLE) {
            Ok(value) => match Self::parse(&value) {
                Some(selection) => selection,
                None => {
                    warn!(%value, "unknown VELDWAKE_CHARACTER; using idle");
                    Self::Idle
                }
            },
            Err(_) => Self::Idle,
        }
    }

    /// `off`, `idle`, `course`, `sturdy`, or `pose:<name>`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "" | "idle" => Some(Self::Idle),
            "off" | "none" => Some(Self::Off),
            "course" | "walk" => Some(Self::Course),
            "slope" => Some(Self::Slope),
            "slope-stand" => Some(Self::SlopeStand),
            "clearing" => Some(Self::Clearing),
            "sturdy" => Some(Self::Sturdy),
            other => {
                // The same speeds the course walks and runs at, so a frozen
                // phase is a frozen frame of the gait the captures show in
                // motion rather than a second gait nobody ever sees move.
                for (prefix, speed) in [("walk:", WALK), ("run:", RUN)] {
                    if let Some(index) = other.strip_prefix(prefix) {
                        let index: u32 = index.trim().parse().ok()?;
                        let phases = walk_cycle_phases();
                        let phase = *phases.get(index as usize)?;
                        return Some(Self::Frozen { speed, phase });
                    }
                }
                let name = other
                    .strip_prefix("pose:")
                    .or_else(|| other.strip_prefix("pose="))?;
                NAMED_POSES
                    .iter()
                    .find(|pose| pose.name.eq_ignore_ascii_case(name.trim()))
                    .map(Self::Pose)
            }
        }
    }

    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Off => "off".to_owned(),
            Self::Idle => "idle".to_owned(),
            Self::Pose(pose) => format!("pose:{}", pose.name),
            Self::Course => "course".to_owned(),
            Self::Slope => "slope".to_owned(),
            Self::SlopeStand => "slope-stand".to_owned(),
            Self::Clearing => "clearing".to_owned(),
            Self::Sturdy => "sturdy".to_owned(),
            Self::Frozen { speed, phase } => format!("frozen:{speed}:{phase}"),
        }
    }

    #[must_use]
    pub const fn is_off(self) -> bool {
        matches!(self, Self::Off)
    }

    fn descriptor(self) -> CharacterDescriptor {
        match self {
            Self::Sturdy => sturdy_descriptor(),
            _ => golden_descriptor(),
        }
    }
}

/// Where the diagnostic course runs, in world voxels.
///
/// Open meadow, flat over the whole path, dry, and clear of trees. Flat is the
/// point: a walk cycle has to be judged without a terrace confusing it, so the
/// one course that shows the gait shows nothing else. Terraces get their own
/// course, at [`SLOPE_X`]. Water is not ground, and a tree overhead puts a
/// capture in deep shade. The corridor was found by scanning the golden region
/// and is proved by `the_diagnostic_course_stays_in_the_region_dry_and_walkable`.
///
/// The path runs at a negative `z` throughout. It does not cross `x = 0`
/// because the golden region has no flat, dry, tree-free corridor of this
/// length that does; negative-coordinate contact is proved headlessly in
/// `veldwake-character` instead, at coordinates far more hostile than a
/// capture could reach.
pub const COURSE_START_X: f32 = 46.0;
/// See [`COURSE_START_X`].
pub const COURSE_START_Z: f32 = -36.0;

/// Where the character stands for every held pose and portrait capture.
///
/// A clearing, and it had to be one. The first portrait captures were taken at
/// the course's start, and they came back with the lower body behind a wall of
/// green: an M4 shrub stands within four world units of that spot, which is
/// exactly where the front camera is. A body cannot be judged through a bush.
///
/// This column was found by scanning the golden region for one that is dry,
/// level for five world units in every direction, free of vegetation for
/// seven, and — the condition the first clearing failed — open for sixteen,
/// with nothing around it standing more than two world units above it. The
/// first clearing sat at the foot of a rise, and a camera four units in front
/// of the character was inside the hillside behind it.
///
/// The course keeps its own start, because it needs a long level corridor
/// rather than a level disc and no column in the region offers both.
/// `the_portrait_stand_is_a_level_clearing` keeps this one honest.
pub const PORTRAIT_X: f32 = -69.0;
/// See [`PORTRAIT_X`].
pub const PORTRAIT_Z: f32 = 49.0;

/// Where the character walks to prove terrain contact on a step.
///
/// The golden course is deliberately level, because a walk cycle has to be
/// judged without a terrace confusing it. The contact claim is the opposite:
/// it is only interesting where the ground is not level. This column starts a
/// strip that climbs **seven single-voxel terraces in eighteen world units**
/// east, dry and clear of vegetation for three units either side, and it was
/// found by scanning the golden region for exactly that.
///
/// `the_slope_course_climbs_real_terraces` asserts the terraces are still
/// there, still one voxel each, and still dry.
pub const SLOPE_X: f32 = 41.0;
/// See [`SLOPE_X`].
pub const SLOPE_Z: f32 = -116.0;

/// Where the character stands for the terrace-contact capture.
///
/// One column of the slope strip, chosen so the evidence is in one frame: the
/// block top under the soles is `77`, the terrace in front of the toes drops
/// to `76` about a third of a world unit ahead, and the character faces down
/// the slope so the sun is on its front. A moving character cannot serve here
/// — a camera anchored to the foot of a climb loses a climbing subject — so
/// the standing capture and the walking one are separate.
pub const SLOPE_STAND_X: f32 = 49.1;
/// See [`SLOPE_STAND_X`].
pub const SLOPE_STAND_Z: f32 = -116.0;

/// Walking speed, in world units per second.
///
/// A little under two leg lengths per second for the golden humanoid, which is
/// an ordinary walk. The numbers are absolute because a course is a fixture
/// tied to one character; `GaitParameters::speed_for` is what converts a
/// leg-relative speed into one of these if another body ever needs a course.
const WALK: f32 = 2.0;
/// Running speed, in world units per second: a little over four leg lengths.
const RUN: f32 = 5.0;
/// The clearing walk's speed: `0.86` leg lengths per second, which is a walk.
///
/// Deliberately slower than [`WALK`], and the reason is a nice demonstration of
/// why phase advances with distance. A slower walk puts more frames inside the
/// same stride, which is what a capture looking for foot sliding needs, and it
/// widens the window a settle has to land in. The *pose sequence* is identical:
/// stride length is what is specified, so halving the speed halves the cadence
/// and changes nothing else.
const CLEARING_WALK: f32 = 1.0;

/// The course is a closed loop, and the arithmetic below is what closes it:
/// `12 s` of walking and `8 s` of running east cover `64` world units, and the
/// same two legs westward cover the same `64` back. It ends where it started,
/// facing the way it started, so sampling it modulo its own duration repeats
/// without a jump.
///
/// That matters for evidence rather than for the character. A capture waits
/// for the world to stream in before it takes a frame, and a course that ran
/// once and stopped had always finished by then: every capture of a walk came
/// back as a capture of a character standing still.
const COURSE_LEGS: &[CourseLeg] = &[
    CourseLeg {
        name: "stand",
        seconds: 4.0,
        speed: 0.0,
        facing: FRAC_PI_2,
    },
    CourseLeg {
        name: "walk-east",
        seconds: 12.0,
        speed: WALK,
        facing: FRAC_PI_2,
    },
    CourseLeg {
        name: "run-east",
        seconds: 8.0,
        speed: RUN,
        facing: FRAC_PI_2,
    },
    CourseLeg {
        name: "turn-west",
        seconds: 3.0,
        speed: 0.0,
        facing: -FRAC_PI_2,
    },
    CourseLeg {
        name: "run-west",
        seconds: 8.0,
        speed: RUN,
        facing: -FRAC_PI_2,
    },
    CourseLeg {
        name: "walk-west",
        seconds: 12.0,
        speed: WALK,
        facing: -FRAC_PI_2,
    },
    CourseLeg {
        name: "turn-east",
        seconds: 3.0,
        speed: 0.0,
        facing: FRAC_PI_2,
    },
    CourseLeg {
        name: "rest",
        seconds: 4.0,
        speed: 0.0,
        facing: FRAC_PI_2,
    },
];

/// The slope course: up the terraces, back down them, and round again.
///
/// Nine seconds at `WALK` is exactly the eighteen world units the strip
/// climbs, so the loop closes on its own start the way the golden course does.
const SLOPE_LEGS: &[CourseLeg] = &[
    CourseLeg {
        name: "stand-below",
        seconds: 3.0,
        speed: 0.0,
        facing: FRAC_PI_2,
    },
    CourseLeg {
        name: "climb",
        seconds: 9.0,
        speed: WALK,
        facing: FRAC_PI_2,
    },
    CourseLeg {
        name: "turn-down",
        seconds: 2.0,
        speed: 0.0,
        facing: -FRAC_PI_2,
    },
    CourseLeg {
        name: "descend",
        seconds: 9.0,
        speed: WALK,
        facing: -FRAC_PI_2,
    },
    CourseLeg {
        name: "turn-up",
        seconds: 2.0,
        speed: 0.0,
        facing: FRAC_PI_2,
    },
];

/// The slope course, for the terrain-contact captures.
pub const SLOPE_COURSE: CharacterCourse = CharacterCourse {
    name: "terrace-climb",
    start_x: SLOPE_X,
    start_z: SLOPE_Z,
    legs: SLOPE_LEGS,
};

/// A short walk across the portrait clearing, and the reason it exists is a
/// capture that could not be taken any other way.
///
/// Foot sliding is the first thing to look for in a walk, and seeing it needs
/// consecutive frames of a translating character from close up. The level
/// corridor cannot give that: it is tree-free but not shrub-free — seventeen
/// vegetation voxels stand on the walked line itself — and a scan of sixty
/// world units of it found no point at all with a clear sight line to a camera
/// twelve units to the side. Every attempt came back with the character's
/// shadow on the grass and the character behind a bush.
///
/// The portrait clearing has no vegetation within seven world units and is
/// level within five, so a walk of four units out and four back stays inside
/// both. It travels along `z` so the portrait cameras, which are placed for a
/// character facing `-Z`, see it from the side as it crosses.
const CLEARING_LEGS: &[CourseLeg] = &[
    CourseLeg {
        name: "stand",
        seconds: 1.0,
        speed: 0.0,
        facing: 0.0,
    },
    CourseLeg {
        name: "walk-out",
        seconds: 4.0,
        speed: CLEARING_WALK,
        facing: 0.0,
    },
    CourseLeg {
        name: "turn-back",
        seconds: 1.0,
        speed: 0.0,
        facing: PI,
    },
    CourseLeg {
        name: "walk-back",
        seconds: 4.0,
        speed: CLEARING_WALK,
        facing: PI,
    },
    CourseLeg {
        name: "turn-out",
        seconds: 1.0,
        speed: 0.0,
        facing: 0.0,
    },
];

/// The clearing walk, for the close motion strip.
pub const CLEARING_COURSE: CharacterCourse = CharacterCourse {
    name: "clearing-walk",
    start_x: PORTRAIT_X,
    start_z: PORTRAIT_Z,
    legs: CLEARING_LEGS,
};

/// The diagnostic path every locomotion capture uses.
pub const GOLDEN_COURSE: CharacterCourse = CharacterCourse {
    name: "meadow-crossing",
    start_x: COURSE_START_X,
    start_z: COURSE_START_Z,
    legs: COURSE_LEGS,
};

/// A camera pose framing the character rather than the landscape.
///
/// The character's counterpart to `procedural::region::GOLDEN_POSES`, and it
/// lives here rather than there for the same reason the ground adapter does:
/// the world crate has no business knowing where a character stands.
#[derive(Clone, Copy, Debug)]
pub struct CharacterCameraPose {
    pub name: &'static str,
    /// World position, relative to the ground the character stands on.
    pub offset: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    /// What this pose is meant to show, so a capture is judged against an
    /// intention rather than against taste.
    pub intent: &'static str,
}

/// Every camera pose the character captures use.
///
/// Offsets are from the point the character stands on, not from a world
/// coordinate, so the portrait clearing and the course corridor share one set
/// of poses and moving either moves its cameras with it.
///
/// The portrait poses stand around a character facing `-Z`, with the sun over
/// the viewer's left shoulder; `character-walk-by` is the exception, because
/// it watches the course, and the course walks `+X`.
pub const CHARACTER_CAMERA_POSES: &[CharacterCameraPose] = &[
    CharacterCameraPose {
        name: "character-front",
        offset: [0.0, 1.25, -4.10],
        yaw_degrees: 180.0,
        pitch_degrees: -3.0,
        intent: "proportions head on: head, shoulders, waist, two legs, two arms",
    },
    CharacterCameraPose {
        name: "character-three-quarter",
        // Front and left, which is the lit pair of faces. Front and right sees
        // the same body with one face in shade, and a capture meant to prove
        // limb separation should not spend half its contrast on the sun.
        offset: [-2.90, 1.30, -2.90],
        yaw_degrees: 135.0,
        pitch_degrees: -4.0,
        intent: "the reading pose: depth, limb separation, and the garment bands",
    },
    CharacterCameraPose {
        name: "character-side",
        // The character's left, which is the lit side, and fifteen degrees
        // round toward the front. A true profile puts the near arm exactly
        // over the torso and the capture loses the one thing it is for; a
        // little rotation keeps the arm hang and the foot length and gives
        // the torso depth back.
        offset: [-4.00, 1.25, -1.07],
        yaw_degrees: 105.0,
        pitch_degrees: -3.0,
        intent: "profile: foot length, torso depth, and how far the arms hang",
    },
    CharacterCameraPose {
        name: "character-silhouette",
        offset: [0.0, 0.45, -3.40],
        yaw_degrees: 180.0,
        pitch_degrees: 16.0,
        intent: "the body against sky alone, where only the outline carries it",
    },
    CharacterCameraPose {
        name: "character-detail",
        offset: [-0.45, 1.90, -2.05],
        yaw_degrees: 167.5,
        pitch_degrees: -2.0,
        intent: "head and chest close up: face voxels, hair, collar, accent stripe",
    },
    CharacterCameraPose {
        name: "character-contact",
        // Far enough back for both boots and the ground they stand on. The
        // first framing was so close that the capture was two boots against
        // featureless grass, which shows a sole but not what it rests on.
        offset: [-1.60, 0.95, -2.40],
        yaw_degrees: 146.0,
        pitch_degrees: -14.0,
        intent: "the soles on the blocks: the whole terrain-contact claim in one frame",
    },
    CharacterCameraPose {
        name: "character-scale",
        offset: [-8.0, 4.10, -12.0],
        yaw_degrees: 146.0,
        pitch_degrees: -16.0,
        intent: "the character against a terrain voxel and a tree: is the scale right?",
    },
    CharacterCameraPose {
        name: "character-in-scene",
        // Twenty-eight units out put a tree between the camera and the
        // character and the capture came back as a photograph of a forest.
        offset: [-11.0, 5.0, -15.0],
        yaw_degrees: 143.0,
        pitch_degrees: -12.0,
        intent: "does the character belong to the same game as the M4 valley?",
    },
    CharacterCameraPose {
        name: "character-slope",
        // Downhill of the character and to its lit side, close enough that a
        // sole and the terrace edge in front of it are in the same frame. Two
        // framings were thrown away before this one, both anchored to the foot
        // of the climb: on a slope a camera anchored to where a walk starts is
        // not anchored to the character at all, and both captures came back as
        // photographs of a hillside.
        offset: [-2.80, 2.00, -3.00],
        yaw_degrees: 131.0,
        pitch_degrees: -17.0,
        intent: "soles on terrace tops: the terrain-contact claim where the ground steps",
    },
    CharacterCameraPose {
        name: "character-clearing",
        // Aimed at the middle of the clearing walk rather than at its start,
        // and far enough back that four world units of travel stay in frame.
        // Framed at the start instead, the character left the left edge of the
        // view inside one stride.
        offset: [-6.50, 1.60, -2.00],
        yaw_degrees: 107.0,
        pitch_degrees: -3.0,
        intent: "a translating character from the side: foot sliding, if there is any",
    },
    CharacterCameraPose {
        name: "character-slope-walk",
        // Beside the middle of the terraced strip, not at the bottom of it.
        // `character-slope` frames the standing terrace capture and is
        // anchored to where that character stands; a climbing character needs
        // its own pose, because the two stand points are eight world units
        // apart and a frame at this distance shows nine.
        offset: [6.00, 5.00, -7.00],
        yaw_degrees: 157.0,
        pitch_degrees: -11.0,
        intent: "walking up terraces: whether a climbing foot finds each step",
    },
    CharacterCameraPose {
        name: "character-walk-by",
        // South of the start of the walk and well above it, looking north
        // across the corridor. Two things set this pose. The course covers
        // sixty-four world units and a frame at this distance shows about
        // fourteen, so it watches the first seconds of the walk rather than
        // trying to follow the whole path — a fixed camera cannot, and the
        // first framing photographed empty meadow while the character walked
        // past off to the east. Then, at eye height, an M4 shrub stood in the
        // line of sight and the capture came back with the character's shadow
        // visible and the character behind a bush. The corridor is tree-free,
        // not shrub-free, so this pose looks over the undergrowth instead.
        offset: [4.0, 8.0, -14.0],
        yaw_degrees: 180.0,
        pitch_degrees: -26.0,
        intent: "side on, where the course's walk passes: the cycle in motion",
    },
];

/// The named character camera pose, if it is one.
#[must_use]
pub fn character_camera_pose(name: &str) -> Option<&'static CharacterCameraPose> {
    let wanted = name.trim().to_ascii_lowercase();
    CHARACTER_CAMERA_POSES
        .iter()
        .find(|pose| pose.name.eq_ignore_ascii_case(&wanted))
}

/// The world position of a character camera pose, given the point on the
/// ground the character stands on.
#[must_use]
pub fn character_camera_position(pose: &CharacterCameraPose, stand: [f32; 3]) -> [f32; 3] {
    [
        stand[0] + pose.offset[0],
        stand[1] + pose.offset[1],
        stand[2] + pose.offset[2],
    ]
}

/// A camera at a named character pose, if that name is one.
///
/// The pose is placed relative to the ground the character stands on, so a
/// capture frames the body rather than a fixed world height.
#[must_use]
pub fn spawn_character_camera(name: &str, generator: &TerrainGenerator) -> Option<Camera> {
    let pose = character_camera_pose(name)?;
    let ground = TerrainGround::new(generator);
    // The camera frames where the character actually is, which is what the
    // selection decides. Anchoring on a fixed world coordinate instead is how
    // a capture ends up pointing at empty meadow.
    let (x, z, _) = CharacterSelection::from_environment().stand_point();
    let height = ground.surface(f64::from(x), f64::from(z))?;
    let position = character_camera_position(pose, [x, height as f32, z]);
    // The pose's intent belongs in the log so a capture can be judged against
    // what it was meant to show rather than against taste.
    tracing::info!(
        pose = pose.name,
        intent = pose.intent,
        ?position,
        "character camera pose"
    );
    Some(Camera::at(
        Vec3::from_array(position),
        pose.yaw_degrees,
        pose.pitch_degrees,
    ))
}

/// The client's character, its state, and how it is driven.
pub struct CharacterScene {
    selection: CharacterSelection,
    character: CompiledCharacter,
    state: CharacterState,
    elapsed: f32,
}

impl CharacterScene {
    /// Compiles the selected character and settles it on the ground.
    pub fn new(
        selection: CharacterSelection,
        ground: Option<&dyn GroundSampler>,
    ) -> Result<Self, CharacterError> {
        let character = CharacterCompiler::new().compile_descriptor(&selection.descriptor())?;
        let (x, z, facing) = selection.stand_point();
        let mut state = CharacterState::standing(x, z, facing, ground);
        match selection {
            CharacterSelection::Pose(named) => {
                let held = state_for(named);
                state.speed = held.speed;
                state.phase = held.phase;
                state.time = held.time;
            }
            CharacterSelection::Frozen { speed, phase } => {
                state.speed = speed;
                state.phase = phase;
            }
            _ => {}
        }
        Ok(Self {
            selection,
            character,
            state,
            elapsed: 0.0,
        })
    }

    #[must_use]
    pub const fn character(&self) -> &CompiledCharacter {
        &self.character
    }

    #[must_use]
    pub const fn state(&self) -> &CharacterState {
        &self.state
    }

    #[must_use]
    pub const fn selection(&self) -> CharacterSelection {
        self.selection
    }

    /// Seconds since the scene started.
    #[must_use]
    pub const fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Advances the character and returns the pose to draw.
    ///
    /// A held pose does not advance: it is a measurement, and a capture of it
    /// taken a minute later must be the same picture.
    pub fn update(&mut self, seconds: f32, ground: Option<&dyn GroundSampler>) -> PosedCharacter {
        let step = if seconds.is_finite() {
            seconds.clamp(0.0, 0.1)
        } else {
            0.0
        };
        match self.selection {
            CharacterSelection::Pose(_) | CharacterSelection::Frozen { .. } => {}
            CharacterSelection::Course
            | CharacterSelection::Slope
            | CharacterSelection::Clearing => {
                let course = match self.selection {
                    CharacterSelection::Slope => &SLOPE_COURSE,
                    CharacterSelection::Clearing => &CLEARING_COURSE,
                    _ => &GOLDEN_COURSE,
                };
                self.elapsed += step;
                // Modulo, because the course is a closed loop: it ends where
                // and how it started, so wrapping is continuous in position,
                // facing and speed, and a capture taken after any settle finds
                // the character somewhere in the loop rather than finished.
                let duration = course.duration();
                if duration > 0.0 && self.elapsed >= duration {
                    self.elapsed %= duration;
                }
                let sample = course.sample(self.elapsed);
                self.state.x = sample.x;
                self.state.z = sample.z;
                self.state.facing = sample.facing;
                self.state.speed = sample.speed;
                self.state.advance(step, &self.character, ground);
            }
            _ => {
                self.elapsed += step;
                self.state.speed = 0.0;
                self.state.advance(step, &self.character, ground);
            }
        }
        pose(&self.character, &self.state, ground)
    }
}

#[cfg(test)]
mod tests {
    use super::{CharacterScene, CharacterSelection, GOLDEN_COURSE, TerrainGround};
    use veldwake_character::{
        CHARACTER_ID_END, CHARACTER_ID_FIRST, CharacterMaterial, GroundSampler,
        descriptor::CHARACTER_VOXEL_SIZE,
        fixture::NAMED_POSES,
        locomotion::{LEFT, RIGHT},
    };
    use veldwake_procedural::{TerrainGenerator, TerrainMaterial, material::ALL_MATERIALS};
    use veldwake_voxel::VoxelId;

    fn generator() -> TerrainGenerator {
        TerrainGenerator::golden()
    }

    #[test]
    fn character_and_terrain_identifiers_never_collide() {
        // The client is the lowest place both tables are visible at once, so it
        // is where the renderer's ordered lookup is proved unambiguous.
        for terrain in ALL_MATERIALS {
            let id = terrain.voxel_id();
            assert!(
                CharacterMaterial::from_voxel_id(id).is_none(),
                "terrain {} is also a character identifier",
                terrain.name()
            );
        }
        for character in veldwake_character::material::ALL_MATERIALS {
            let id = character.voxel_id();
            assert!(
                TerrainMaterial::from_voxel_id(id).is_none(),
                "character {} is also a terrain identifier",
                character.name()
            );
            assert!(!id.is_air());
        }
        // And the whole declared reservation is disjoint, not only what is used.
        for raw in CHARACTER_ID_FIRST..CHARACTER_ID_END {
            assert!(TerrainMaterial::from_voxel_id(VoxelId(raw)).is_none());
        }
        for raw in [1_u16, 2, 7] {
            assert!(CharacterMaterial::from_voxel_id(VoxelId(raw)).is_none());
            assert!(TerrainMaterial::from_voxel_id(VoxelId(raw)).is_none());
        }
    }

    /// The portrait stand is what the proportion captures are judged on, so
    /// the two things that ruined the first attempt are asserted rather than
    /// remembered: a terrace crossing the frame, and foliage standing between
    /// the camera and the body.
    #[test]
    fn the_portrait_stand_is_a_level_clearing() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let field = generator.field();
        let vegetation = generator.vegetation();
        let x = super::PORTRAIT_X as i64;
        let z = super::PORTRAIT_Z as i64;
        let Some(height) = ground.surface(f64::from(x as f32), f64::from(z as f32)) else {
            panic!("the portrait stand is outside the region");
        };
        assert!(!ground.sample(x as f64, z as f64).is_submerged());

        // Level out to five world units: every portrait camera is closer than
        // that, so nothing in frame can be a terrace edge.
        for dz in -5..=5_i64 {
            for dx in -5..=5_i64 {
                if dx * dx + dz * dz > 25 {
                    continue;
                }
                let sx = (x + dx) as f64;
                let sz = (z + dz) as f64;
                assert_eq!(
                    ground.surface(sx, sz),
                    Some(height),
                    "the portrait clearing steps at ({sx}, {sz})"
                );
                assert!(!ground.sample(sx, sz).is_submerged());
            }
        }

        // Open out to sixteen: nothing near enough to stand behind the
        // character may rise more than two world units above its feet, or a
        // camera placed in front of it ends up inside a hillside.
        for dz in -16..=16_i64 {
            for dx in -16..=16_i64 {
                if dx * dx + dz * dz > 256 {
                    continue;
                }
                let sx = (x + dx) as f64;
                let sz = (z + dz) as f64;
                let Some(there) = ground.surface(sx, sz) else {
                    panic!("the portrait clearing reaches the region edge at ({sx}, {sz})");
                };
                assert!(
                    there - height <= 2.0,
                    "the ground at ({sx}, {sz}) stands {} above the portrait stand",
                    there - height
                );
            }
        }

        // Free of vegetation out to seven, which is further than every
        // portrait camera stands and taller than the character plus a shrub.
        let base = height as i64;
        for dz in -7..=7_i64 {
            for dx in -7..=7_i64 {
                if dx * dx + dz * dz > 49 {
                    continue;
                }
                for y in base..base + 14 {
                    assert!(
                        !vegetation.occupied(field, x + dx, y, z + dz),
                        "vegetation stands at ({}, {y}, {}) in the portrait clearing",
                        x + dx,
                        z + dz
                    );
                }
            }
        }
    }

    #[test]
    fn the_ground_is_the_block_top_and_absence_stays_absence() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        for (x, z) in [(0.0_f64, 40.0_f64), (-96.0, -54.0), (-300.0, -55.0)] {
            let Some(height) = ground.surface(x, z) else {
                panic!("the golden region has ground at ({x}, {z})");
            };
            let sample = ground.sample(x, z);
            assert_eq!(
                height,
                (sample.surface_y() + 1) as f64,
                "the ground at ({x}, {z}) is not the block top"
            );
            // A block top is an integer: the surface a viewer sees is a face.
            assert!((height - height.round()).abs() < 1.0e-9);
        }
        // Outside the finite region there is no answer, never a floor at zero.
        assert_eq!(ground.surface(10_000.0, 0.0), None);
        assert_eq!(ground.surface(0.0, -10_000.0), None);
        assert_eq!(ground.surface(f64::NAN, 0.0), None);
    }

    #[test]
    fn terrain_ground_keeps_the_fractional_edge_of_each_last_voxel_column() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        // `RegionExtent::GOLDEN` is -384 through the half-open edge at 416
        // in both axes.  Terrain cells are discrete, but a foot query is
        // continuous: every point in the final cell [415, 416) remains on
        // its physical column, including its centre.
        for (x, z, present) in [
            (-384.0, 0.0, true),
            (-384.000_001, 0.0, false),
            (-383.5, 0.0, true),
            (415.0, 0.0, true),
            (415.5, 0.0, true),
            (415.999_999, 0.0, true),
            (416.0, 0.0, false),
            (0.0, -384.0, true),
            (0.0, -384.000_001, false),
            (0.0, -383.5, true),
            (0.0, 415.0, true),
            (0.0, 415.5, true),
            (0.0, 415.999_999, true),
            (0.0, 416.0, false),
        ] {
            assert_eq!(
                ground.surface(x, z).is_some(),
                present,
                "unexpected finite-region answer at ({x}, {z})"
            );
        }
    }

    /// The capture harness waits for the world to stream before it takes a
    /// frame, so the course has to still be running by then, and it is because
    /// it repeats. Repeating is only honest if the wrap is not a teleport.
    /// The contact claim is only interesting where the ground is not level,
    /// so the strip this course walks has to actually step. Seven terraces in
    /// eighteen world units, every one of them a single terrain voxel, and
    /// none of them under water.
    #[test]
    fn the_slope_course_climbs_real_terraces() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let start = super::SLOPE_COURSE.sample(0.0);
        let Some(base) = ground.surface(f64::from(start.x), f64::from(start.z)) else {
            panic!("the slope course starts outside the region");
        };

        let mut previous = base;
        let mut steps = 0_u32;
        let mut highest = base;
        for (_, sample) in super::SLOPE_COURSE.walk(1.0 / 20.0) {
            let x = f64::from(sample.x);
            let z = f64::from(sample.z);
            let Some(height) = ground.surface(x, z) else {
                panic!("the slope course leaves the region at ({x}, {z})");
            };
            assert!(
                !ground.sample(x, z).is_submerged(),
                "the slope course walks into water at ({x}, {z})"
            );
            let rise = height - previous;
            assert!(
                rise.abs() <= 1.0,
                "the slope course crosses a {rise}-voxel step at ({x}, {z})"
            );
            if rise.abs() > 0.5 {
                steps += 1;
            }
            highest = highest.max(height);
            previous = height;
        }
        assert!(steps >= 6, "the slope course only crosses {steps} terraces");
        assert!(
            highest - base >= 3.0,
            "the slope course only climbs {} world units",
            highest - base
        );

        // And it is a loop, like the golden one.
        let end = super::SLOPE_COURSE.sample(super::SLOPE_COURSE.duration());
        assert!((end.x - start.x).abs() < 1.0e-3 && (end.z - start.z).abs() < 1.0e-3);
        assert!((end.facing - start.facing).abs() < 1.0e-6);
    }

    /// The terrace-contact capture is only evidence if there is a terrace in
    /// it, a third of a world unit in front of the toes and no further.
    #[test]
    fn the_terrace_stand_has_a_step_in_front_of_the_toes() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let x = f64::from(super::SLOPE_STAND_X);
        let z = f64::from(super::SLOPE_STAND_Z);
        let Some(under) = ground.surface(x, z) else {
            panic!("the terrace stand is outside the region");
        };
        assert!(!ground.sample(x, z).is_submerged());

        // The character faces `-X`, so the toes reach toward a smaller `x`.
        let toe = x - 0.42;
        assert_eq!(
            ground.surface(toe, z),
            Some(under),
            "the toes hang over the edge rather than standing on the block"
        );
        // The terrain field is continuous in `x`, so a terrace edge falls
        // wherever the height crosses an integer and not at an integer column.
        // Walk forward until the block top drops and report where it did.
        let mut edge = None;
        let mut step = 0.05_f64;
        while step <= 2.0 {
            let Some(ahead) = ground.surface(x - step, z) else {
                panic!("the slope ends at the stand");
            };
            if (ahead - under).abs() > 0.5 {
                assert!(
                    (under - ahead - 1.0).abs() < 1.0e-9,
                    "the step in front is {} voxels, not one",
                    under - ahead
                );
                edge = Some(step);
                break;
            }
            step += 0.05;
        }
        let Some(edge) = edge else {
            panic!("no terrace edge within two world units in front of the toes");
        };
        assert!(
            (0.42..=1.20).contains(&edge),
            "the terrace edge is {edge} world units ahead: either under the              foot or too far away to be in the same frame"
        );
        // And the sole really does settle on that block top, not near it.
        let sampler: &dyn GroundSampler = &ground;
        let Ok(mut scene) = CharacterScene::new(CharacterSelection::SlopeStand, Some(sampler))
        else {
            panic!("the golden humanoid must compile");
        };
        for _ in 0..180 {
            scene.update(1.0 / 60.0, Some(sampler));
        }
        let posed = scene.update(1.0 / 60.0, Some(sampler));
        for side in [LEFT, RIGHT] {
            let contact = posed.contacts()[side];
            assert!(contact.grounded);
            assert!(
                contact.clearance().abs() <= CHARACTER_VOXEL_SIZE,
                "a sole sits {} from the terrace top",
                contact.clearance()
            );
        }
    }

    /// The clearing walk has to stay inside the two radii that make the
    /// clearing usable at all, or the capture it exists for is a capture of
    /// the character behind a bush again.
    #[test]
    fn the_clearing_walk_stays_inside_the_clearing() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let field = generator.field();
        let vegetation = generator.vegetation();
        let start = super::CLEARING_COURSE.sample(0.0);
        let Some(base) = ground.surface(f64::from(start.x), f64::from(start.z)) else {
            panic!("the clearing walk starts outside the region");
        };

        let mut furthest = 0.0_f32;
        let mut walked = false;
        for (_, sample) in super::CLEARING_COURSE.walk(1.0 / 30.0) {
            let x = f64::from(sample.x);
            let z = f64::from(sample.z);
            assert_eq!(
                ground.surface(x, z),
                Some(base),
                "the clearing walk steps at ({x}, {z})"
            );
            assert!(!ground.sample(x, z).is_submerged());
            let reach = ((sample.x - start.x).powi(2) + (sample.z - start.z).powi(2)).sqrt();
            furthest = furthest.max(reach);
            walked |= sample.speed > 0.0;
            // Nothing planted within two world units of anywhere it walks,
            // which is what keeps a four-unit portrait camera looking at a
            // character rather than at undergrowth.
            let cx = sample.x.round() as i64;
            let cz = sample.z.round() as i64;
            for dz in -2..=2_i64 {
                for dx in -2..=2_i64 {
                    for y in base as i64..base as i64 + 14 {
                        assert!(
                            !vegetation.occupied(field, cx + dx, y, cz + dz),
                            "vegetation stands at ({}, {y}, {}) on the clearing walk",
                            cx + dx,
                            cz + dz
                        );
                    }
                }
            }
        }
        assert!(walked, "the clearing walk never walks");
        assert!(
            (3.0..=5.0).contains(&furthest),
            "the clearing walk reaches {furthest} world units out"
        );
        // Most of the loop is walking, because a capture has to land in it.
        let walking: f32 = super::CLEARING_LEGS
            .iter()
            .filter(|leg| leg.speed > 0.0)
            .map(|leg| leg.seconds)
            .sum();
        assert!(
            walking / super::CLEARING_COURSE.duration() > 0.6,
            "only {walking} s of the clearing loop is spent walking"
        );

        let end = super::CLEARING_COURSE.sample(super::CLEARING_COURSE.duration());
        assert!((end.x - start.x).abs() < 1.0e-3 && (end.z - start.z).abs() < 1.0e-3);
        assert!((end.facing - start.facing).abs() < 1.0e-6);
    }

    #[test]
    fn the_diagnostic_course_closes_into_a_loop() {
        let duration = GOLDEN_COURSE.duration();
        assert!(duration > 30.0, "the course is only {duration} s long");
        let start = GOLDEN_COURSE.sample(0.0);
        let end = GOLDEN_COURSE.sample(duration);
        assert!(
            (end.x - start.x).abs() < 1.0e-3 && (end.z - start.z).abs() < 1.0e-3,
            "the course ends at ({}, {}) but starts at ({}, {})",
            end.x,
            end.z,
            start.x,
            start.z
        );
        assert!((end.facing - start.facing).abs() < 1.0e-6);
        assert!(end.speed.abs() < 1.0e-6 && start.speed.abs() < 1.0e-6);

        // A loop is only useful if it actually walks and actually runs.
        let mut walked = false;
        let mut ran = false;
        let mut furthest = 0.0_f32;
        for (_, sample) in GOLDEN_COURSE.walk(1.0 / 20.0) {
            walked |= (sample.speed - super::WALK).abs() < 1.0e-6;
            ran |= (sample.speed - super::RUN).abs() < 1.0e-6;
            furthest = furthest.max((sample.x - start.x).abs());
        }
        assert!(walked && ran, "the course never walks or never runs");
        assert!(
            furthest > 60.0,
            "the course only reaches {furthest} units out"
        );
    }

    #[test]
    fn the_diagnostic_course_stays_in_the_region_dry_and_walkable() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let mut checked = 0;
        let mut lowest = f64::INFINITY;
        let mut highest = f64::NEG_INFINITY;
        let mut negative_coordinates = false;
        for (_, sample) in GOLDEN_COURSE.walk(1.0 / 20.0) {
            negative_coordinates |= sample.x < 0.0 || sample.z < 0.0;
            let x = f64::from(sample.x);
            let z = f64::from(sample.z);
            let Some(height) = ground.surface(x, z) else {
                panic!(
                    "the course leaves the region at ({x}, {z}) on leg {}",
                    sample.leg
                );
            };
            let terrain = ground.sample(x, z);
            assert!(
                !terrain.is_submerged(),
                "the course walks into water at ({x}, {z}) on leg {}",
                sample.leg
            );
            assert!(
                terrain.slope < 0.35,
                "the course crosses a slope of {} at ({x}, {z}) on leg {}",
                terrain.slope,
                sample.leg
            );
            lowest = lowest.min(height);
            highest = highest.max(height);
            checked += 1;
        }
        assert!(checked > 900, "only {checked} course samples");
        assert!(
            negative_coordinates,
            "the course never visits a negative coordinate"
        );
        // One block level for the whole path, and that is a requirement
        // rather than a nicety: one terrain voxel is a whole world unit and
        // the golden humanoid's leg is `0.6875`, so a terrace on the course
        // would be a step the character physically cannot take.
        assert!(
            (highest - lowest).abs() < 1.0e-9,
            "the course crosses a terrace: block tops range over {} world units",
            highest - lowest
        );
    }

    #[test]
    fn the_character_settles_on_the_terrain_with_its_soles_on_the_blocks() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let sampler: &dyn GroundSampler = &ground;
        let Ok(mut scene) = CharacterScene::new(CharacterSelection::Idle, Some(sampler)) else {
            panic!("the golden humanoid must compile");
        };
        for _ in 0..120 {
            scene.update(1.0 / 60.0, Some(sampler));
        }
        let posed = scene.update(1.0 / 60.0, Some(sampler));
        assert!(posed.is_finite());
        for side in [LEFT, RIGHT] {
            let contact = posed.contacts()[side];
            assert!(contact.grounded, "side {side} found no ground");
            assert!(
                contact.clearance().abs() <= CHARACTER_VOXEL_SIZE * 1.5,
                "side {side} sole clearance {}",
                contact.clearance()
            );
        }
    }

    #[test]
    fn walking_the_whole_course_keeps_every_joint_legal_and_every_sole_on_the_ground() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let sampler: &dyn GroundSampler = &ground;
        let Ok(mut scene) = CharacterScene::new(CharacterSelection::Course, Some(sampler)) else {
            panic!("the golden humanoid must compile");
        };
        let mut worst_planted = 0.0_f32;
        let mut worst_clip = 0.0_f32;
        let steps = (GOLDEN_COURSE.duration() * 60.0) as usize;
        for _ in 0..steps {
            let posed = scene.update(1.0 / 60.0, Some(sampler));
            assert!(posed.is_finite());
            assert!(posed.angles().within_limits());
            for side in [LEFT, RIGHT] {
                let contact = posed.contacts()[side];
                assert!(contact.grounded);
                if contact.stance > 0.9 {
                    worst_planted = worst_planted.max(contact.clearance().abs());
                }
                worst_clip = worst_clip.min(contact.clearance());
            }
        }
        assert!(
            worst_planted <= CHARACTER_VOXEL_SIZE * 1.5,
            "a planted sole drifted {worst_planted} from the ground"
        );
        assert!(
            worst_clip > -CHARACTER_VOXEL_SIZE * 2.0,
            "a sole cut {worst_clip} into the terrain"
        );
    }

    #[test]
    fn the_selection_parses_documented_names_only() {
        assert_eq!(
            CharacterSelection::parse("off"),
            Some(CharacterSelection::Off)
        );
        assert_eq!(
            CharacterSelection::parse(""),
            Some(CharacterSelection::Idle)
        );
        assert_eq!(
            CharacterSelection::parse(" COURSE "),
            Some(CharacterSelection::Course)
        );
        assert_eq!(
            CharacterSelection::parse("sturdy"),
            Some(CharacterSelection::Sturdy)
        );
        for named in NAMED_POSES {
            let Some(CharacterSelection::Pose(found)) =
                CharacterSelection::parse(&format!("pose:{}", named.name))
            else {
                panic!("pose {} does not parse", named.name);
            };
            assert_eq!(found.name, named.name);
        }
        assert_eq!(CharacterSelection::parse("pose:nonsense"), None);
        assert_eq!(CharacterSelection::parse("nonsense"), None);
        assert!(CharacterSelection::Off.is_off());
        assert!(!CharacterSelection::Idle.is_off());
        assert!(CharacterSelection::Idle.name().contains("idle"));
    }

    #[test]
    fn every_character_camera_pose_is_named_once_and_frames_the_character() {
        use super::{CHARACTER_CAMERA_POSES, character_camera_pose, character_camera_position};
        use std::collections::BTreeSet;
        let names: BTreeSet<&str> = CHARACTER_CAMERA_POSES.iter().map(|p| p.name).collect();
        assert_eq!(names.len(), CHARACTER_CAMERA_POSES.len());
        assert!(
            CHARACTER_CAMERA_POSES.len() >= 6,
            "too few poses for evidence"
        );

        let generator = generator();
        let ground = TerrainGround::new(&generator);
        // Both stand points are photographed, so both must frame something.
        for selection in [CharacterSelection::Idle, CharacterSelection::Course] {
            let (x, z, _) = selection.stand_point();
            let Some(stand) = ground.surface(f64::from(x), f64::from(z)) else {
                panic!("{} stands on ground", selection.name());
            };
            for pose in CHARACTER_CAMERA_POSES {
                assert!(!pose.intent.is_empty());
                assert_eq!(
                    character_camera_pose(pose.name).map(|p| p.name),
                    Some(pose.name)
                );
                let position = character_camera_position(pose, [x, stand as f32, z]);
                assert!(position.iter().all(|value| value.is_finite()));
                // A camera inside the ground photographs the inside of a hill,
                // which is the M4 lesson about poses being fixtures.
                let Some(under) = ground.surface(f64::from(position[0]), f64::from(position[2]))
                else {
                    panic!("camera pose {} stands outside the region", pose.name);
                };
                assert!(
                    f64::from(position[1]) > under + 0.2,
                    "camera pose {} sits inside the ground at {}",
                    pose.name,
                    selection.name()
                );
            }
        }
        assert!(character_camera_pose("not-a-pose").is_none());
    }

    #[test]
    fn a_frozen_gait_phase_parses_and_holds() {
        let Some(CharacterSelection::Frozen { speed, phase }) = CharacterSelection::parse("walk:3")
        else {
            panic!("walk:3 must parse");
        };
        assert!((speed - super::WALK).abs() < 1.0e-6);
        assert!((phase - 0.375).abs() < 1.0e-6);
        let Some(CharacterSelection::Frozen { speed, .. }) = CharacterSelection::parse("run:0")
        else {
            panic!("run:0 must parse");
        };
        assert!((speed - super::RUN).abs() < 1.0e-6);
        assert_eq!(CharacterSelection::parse("walk:9"), None);
        assert_eq!(CharacterSelection::parse("walk:x"), None);

        let Some(selection) = CharacterSelection::parse("walk:5") else {
            panic!("walk:5 must parse");
        };
        let Ok(mut scene) = CharacterScene::new(selection, None) else {
            panic!("the golden humanoid must compile");
        };
        let first = scene.update(1.0 / 60.0, None);
        for _ in 0..300 {
            scene.update(1.0 / 60.0, None);
        }
        assert_eq!(
            first.bone_world(),
            scene.update(1.0 / 60.0, None).bone_world()
        );
        assert!(first.blend().moving > 0.9, "a frozen walk is not walking");
    }

    #[test]
    fn a_held_pose_does_not_move() {
        let Some(named) = NAMED_POSES.iter().find(|pose| pose.name == "walk-passing") else {
            panic!("the walk-passing pose disappeared");
        };
        let Ok(mut scene) = CharacterScene::new(CharacterSelection::Pose(named), None) else {
            panic!("the golden humanoid must compile");
        };
        let first = scene.update(1.0 / 60.0, None);
        for _ in 0..600 {
            scene.update(1.0 / 60.0, None);
        }
        let later = scene.update(1.0 / 60.0, None);
        assert_eq!(
            first.bone_world(),
            later.bone_world(),
            "a held pose drifted over ten seconds"
        );
    }

    #[test]
    fn the_sturdy_fixture_is_a_different_character_in_the_same_scene() {
        let Ok(golden) = CharacterScene::new(CharacterSelection::Idle, None) else {
            panic!("the golden humanoid must compile");
        };
        let Ok(sturdy) = CharacterScene::new(CharacterSelection::Sturdy, None) else {
            panic!("the sturdy humanoid must compile");
        };
        assert_ne!(
            golden.character().fingerprint(),
            sturdy.character().fingerprint()
        );
        assert_ne!(
            golden.character().body().height,
            sturdy.character().body().height
        );
    }
}
