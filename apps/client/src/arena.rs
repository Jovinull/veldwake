//! Where the fight happens, in the golden region.
//!
//! `veldwake-combat` takes an arena as a centre and a radius and knows nothing
//! about terrain; this module is the other half — the one column of the M4 golden
//! world that satisfies what a fight needs, and the test that keeps it honest.
//!
//! An arena is a fixture and gets the same treatment a camera pose does, for the
//! reason `LEARNINGS.md` already records: three of M4's six poses were written
//! from arithmetic and put the camera inside a hillside or inside a canopy. A
//! scanned column with an asserted predicate is the cheaper answer.

use glam::{Vec2, Vec3};

use veldwake_character::GroundSampler;
use veldwake_combat::fixture;
use veldwake_procedural::TerrainGenerator;

use crate::camera::Camera;
use crate::character::TerrainGround;

/// Where the encounter is fought, in world voxels.
///
/// This is **M5's portrait clearing**, and reusing it is a real saving rather than
/// a coincidence: that column was already scanned and asserted to be dry, level
/// for five units, free of vegetation for seven and open for sixteen, and it
/// turns out to be level for eight and open for twenty as well. The ten character
/// camera poses are placed relative to whatever the character stands on, so they
/// frame the fight without a second stand point.
///
/// The predicate below is the arena's own and is stricter than M5's in the two
/// ways a fight needs: level and open far enough that two bodies moving inside a
/// seven-unit arena never meet a terrace, and a follow camera behind either of
/// them is never inside a hillside.
pub const ARENA_X: f32 = -69.0;
/// See [`ARENA_X`].
pub const ARENA_Z: f32 = 49.0;

/// How far from the centre a body may go.
///
/// The combat crate's own fixture radius, restated here only so the terrain
/// predicate and the rules cannot disagree about how big the fight is.
pub const ARENA_RADIUS: f32 = fixture::ARENA_RADIUS;

/// How far out the ground has to be level.
///
/// The arena radius plus the widest body's capsule, rounded up, so a body
/// standing against the boundary is still wholly on level ground.
pub const LEVEL_RADIUS: i64 = 7;

/// How far out there may be no vegetation.
///
/// How far out there may be no vegetation.
///
/// **This is the number that decided the arena's size, not the other way round.**
/// Eleven was the first value, and a scan of the whole golden region at a
/// four-voxel stride found *no* column that satisfies it together with the other
/// three conditions. Nine and ten both fail here too: this clearing has plants at
/// `10.77`, exactly `10.0` and `8.25` units out. Seven is what M5 proved and what
/// the world actually offers, so the arena radius came down to `5.5` instead —
/// `5.5` plus the widest body's `0.86` is `6.36`, inside the clearing, so no body
/// can reach the boundary and stand in a shrub it cannot collide with.
pub const CLEAR_RADIUS: i64 = 7;

/// How far out nothing may rise far above the floor.
///
/// The follow camera sits behind a body at about five world units, and a capture
/// looks past it, so the opening has to be wider than the fight.
pub const OPEN_RADIUS: i64 = 20;

/// How far above the arena floor the surrounding ground may rise.
pub const OPEN_RISE: f64 = 3.0;

/// The arena centre as a planar vector, which is what the rules take.
#[must_use]
pub const fn centre() -> Vec2 {
    Vec2::new(ARENA_X, ARENA_Z)
}

/// The height of the arena floor, or `None` outside the region.
#[must_use]
pub fn floor(generator: &TerrainGenerator) -> Option<f64> {
    TerrainGround::new(generator).surface(f64::from(ARENA_X), f64::from(ARENA_Z))
}

/// A camera pose that frames the fight rather than the landscape.
///
/// The combat counterpart to `character::CHARACTER_CAMERA_POSES`, and it lives
/// here for the same reason the arena does: the combat crate has no business
/// knowing where in a world a fight happens.
#[derive(Clone, Copy, Debug)]
pub struct CombatCameraPose {
    pub name: &'static str,
    /// World position relative to the arena floor at its centre.
    pub offset: [f32; 3],
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    /// What this pose is meant to show, so a capture is judged against an
    /// intention rather than against taste.
    pub intent: &'static str,
}

/// Every camera pose the combat captures use.
///
/// The two bodies start on the arena's `z` axis, the player at `+3` and the
/// adversary at `-3`, so a side view looks along `x` and an over-the-shoulder view
/// looks along `-z`. Yaw is degrees clockwise from `-Z`, the convention every
/// other pose in the client already uses.
pub const COMBAT_CAMERA_POSES: &[CombatCameraPose] = &[
    CombatCameraPose {
        name: "combat-side",
        offset: [-9.0, 3.2, 0.0],
        yaw_degrees: 90.0,
        pitch_degrees: -12.0,
        intent: "both bodies in profile: spacing, reach, and who is bigger",
    },
    CombatCameraPose {
        name: "combat-close",
        offset: [-4.6, 1.9, 0.0],
        yaw_degrees: 90.0,
        pitch_degrees: -5.0,
        intent: "the contact frames: does the blade touch the body it hurt?",
    },
    CombatCameraPose {
        name: "combat-shoulder",
        offset: [0.0, 2.1, 8.0],
        yaw_degrees: 0.0,
        pitch_degrees: -9.0,
        intent: "what a player sees: the telegraph read from behind the player",
    },
    CombatCameraPose {
        name: "combat-wide",
        offset: [-11.0, 6.0, 10.0],
        // `48` degrees, not `132`. Yaw is clockwise from `-Z`, so a camera that
        // sits at `-x` and `+z` looks back toward `+x` and `-z` at forty-eight;
        // the first draft had the sign wrong and looked past both bodies, which
        // `the_side_and_shoulder_poses_see_both_bodies` caught.
        yaw_degrees: 48.0,
        pitch_degrees: -19.0,
        intent: "the encounter in its clearing: does the fight belong to the world?",
    },
    CombatCameraPose {
        name: "combat-plan",
        offset: [0.0, 13.0, 0.2],
        yaw_degrees: 0.0,
        pitch_degrees: -78.0,
        intent: "spacing from above, where a dodge is a distance rather than a pose",
    },
];

/// The named combat camera pose, if it is one.
#[must_use]
pub fn combat_camera_pose(name: &str) -> Option<&'static CombatCameraPose> {
    let wanted = name.trim().to_ascii_lowercase();
    COMBAT_CAMERA_POSES
        .iter()
        .find(|pose| pose.name.eq_ignore_ascii_case(&wanted))
}

/// A camera at a named combat pose, if that name is one.
#[must_use]
pub fn spawn_combat_camera(name: &str, generator: &TerrainGenerator) -> Option<Camera> {
    let pose = combat_camera_pose(name)?;
    let height = floor(generator)?;
    let position = [
        ARENA_X + pose.offset[0],
        height as f32 + pose.offset[1],
        ARENA_Z + pose.offset[2],
    ];
    tracing::info!(
        pose = pose.name,
        intent = pose.intent,
        ?position,
        "combat camera pose"
    );
    Some(Camera::at(
        Vec3::from_array(position),
        pose.yaw_degrees,
        pose.pitch_degrees,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        ARENA_RADIUS, ARENA_X, ARENA_Z, CLEAR_RADIUS, COMBAT_CAMERA_POSES, LEVEL_RADIUS,
        OPEN_RADIUS, OPEN_RISE, centre, floor,
    };
    use crate::character::TerrainGround;
    use glam::Vec2;
    use veldwake_character::GroundSampler;
    use veldwake_combat::fixture;
    use veldwake_procedural::TerrainGenerator;

    fn generator() -> TerrainGenerator {
        TerrainGenerator::golden()
    }

    #[test]
    fn the_arena_is_a_level_open_clearing() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let field = generator.field();
        let vegetation = generator.vegetation();
        let x = ARENA_X as i64;
        let z = ARENA_Z as i64;
        let Some(height) = floor(&generator) else {
            panic!("the arena is outside the region");
        };
        assert!(
            !ground
                .sample(f64::from(ARENA_X), f64::from(ARENA_Z))
                .is_submerged(),
            "the arena is under water"
        );

        // Level out to the arena radius plus a body: a terrace inside the fight
        // would change what a swing reaches without anything saying so.
        for dz in -LEVEL_RADIUS..=LEVEL_RADIUS {
            for dx in -LEVEL_RADIUS..=LEVEL_RADIUS {
                if dx * dx + dz * dz > LEVEL_RADIUS * LEVEL_RADIUS {
                    continue;
                }
                let sx = (x + dx) as f64;
                let sz = (z + dz) as f64;
                assert_eq!(
                    ground.surface(sx, sz),
                    Some(height),
                    "the arena floor steps at ({sx}, {sz})"
                );
                assert!(
                    !ground.sample(sx, sz).is_submerged(),
                    "the arena is wet at ({sx}, {sz})"
                );
            }
        }

        // Open out to twenty, so a follow camera behind either body is looking
        // across a clearing rather than out of a hillside.
        for dz in -OPEN_RADIUS..=OPEN_RADIUS {
            for dx in -OPEN_RADIUS..=OPEN_RADIUS {
                if dx * dx + dz * dz > OPEN_RADIUS * OPEN_RADIUS {
                    continue;
                }
                let sx = (x + dx) as f64;
                let sz = (z + dz) as f64;
                let Some(there) = ground.surface(sx, sz) else {
                    panic!("the arena reaches the region edge at ({sx}, {sz})");
                };
                assert!(
                    there - height <= OPEN_RISE,
                    "the ground at ({sx}, {sz}) stands {} above the arena floor",
                    there - height
                );
            }
        }

        // Clear of vegetation further out than the fight itself, and tall enough
        // to cover the taller body plus a raised blade.
        let base = height as i64;
        for dz in -CLEAR_RADIUS..=CLEAR_RADIUS {
            for dx in -CLEAR_RADIUS..=CLEAR_RADIUS {
                if dx * dx + dz * dz > CLEAR_RADIUS * CLEAR_RADIUS {
                    continue;
                }
                for y in base..base + 16 {
                    assert!(
                        !vegetation.occupied(field, x + dx, y, z + dz),
                        "vegetation stands at ({}, {y}, {}) in the arena",
                        x + dx,
                        z + dz
                    );
                }
            }
        }
    }

    #[test]
    fn both_bodies_start_inside_the_arena_and_on_its_floor() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let Some(height) = floor(&generator) else {
            panic!("the arena is outside the region");
        };
        for offset in [fixture::PLAYER_OFFSET, fixture::ADVERSARY_OFFSET] {
            let position = centre() + offset;
            assert!(
                offset.length() < ARENA_RADIUS,
                "a body starts outside its own arena"
            );
            assert_eq!(
                ground.surface(f64::from(position.x), f64::from(position.y)),
                Some(height),
                "a body starts off the arena floor at {position}"
            );
        }
    }

    #[test]
    fn every_combat_camera_pose_stands_in_open_air_above_the_ground() {
        // A pose is a fixture and gets the same test content gets. Three of M4's
        // six poses were written from arithmetic and ended up inside a hillside
        // or inside a canopy, which is why this exists.
        let generator = generator();
        let ground = TerrainGround::new(&generator);
        let field = generator.field();
        let vegetation = generator.vegetation();
        let Some(height) = floor(&generator) else {
            panic!("the arena is outside the region");
        };
        let mut names = std::collections::BTreeSet::new();
        for pose in COMBAT_CAMERA_POSES {
            assert!(names.insert(pose.name), "{} repeats", pose.name);
            assert!(!pose.intent.is_empty(), "{} has no intent", pose.name);
            let x = f64::from(ARENA_X + pose.offset[0]);
            let z = f64::from(ARENA_Z + pose.offset[2]);
            let eye = height + f64::from(pose.offset[1]);
            let Some(there) = ground.surface(x, z) else {
                panic!("{} stands outside the region", pose.name);
            };
            assert!(
                eye > there + 0.5,
                "{} sits {} above the ground under it",
                pose.name,
                eye - there
            );
            // And nothing grows where the camera is.
            let cell = eye as i64;
            for y in cell - 1..=cell + 1 {
                assert!(
                    !vegetation.occupied(field, x as i64, y, z as i64),
                    "{} stands inside a plant",
                    pose.name
                );
            }
            match super::spawn_combat_camera(pose.name, &generator) {
                Some(_) => {}
                None => panic!("{} cannot be spawned", pose.name),
            }
        }
        assert!(super::combat_camera_pose("not-a-pose").is_none());
        assert!(super::combat_camera_pose("  COMBAT-SIDE ").is_some());
    }

    #[test]
    fn the_side_and_shoulder_poses_see_both_bodies() {
        // A pose that frames one body is not a pose for a fight. Both starting
        // positions have to be in front of the camera rather than behind it.
        for name in [
            "combat-side",
            "combat-close",
            "combat-shoulder",
            "combat-wide",
        ] {
            let Some(pose) = super::combat_camera_pose(name) else {
                panic!("{name} is not a pose");
            };
            let eye = Vec2::new(ARENA_X + pose.offset[0], ARENA_Z + pose.offset[2]);
            let yaw = pose.yaw_degrees.to_radians();
            let forward = Vec2::new(yaw.sin(), -yaw.cos());
            for offset in [fixture::PLAYER_OFFSET, fixture::ADVERSARY_OFFSET] {
                let body = centre() + offset;
                let toward = body - eye;
                assert!(
                    toward.normalize().dot(forward) > 0.6,
                    "{name} does not look at the body at {body}"
                );
            }
        }
    }

    #[test]
    fn the_clearing_covers_the_whole_fight() {
        // The rules stop a body at the arena radius, so the level and clear
        // ground both have to reach that far plus the widest body's own capsule.
        // This is the arithmetic that set the arena's size.
        let widest = 0.86_f32;
        assert!(
            LEVEL_RADIUS as f32 >= ARENA_RADIUS + widest,
            "level ground of {LEVEL_RADIUS} does not cover an arena of {ARENA_RADIUS}"
        );
        assert!(
            CLEAR_RADIUS as f32 >= ARENA_RADIUS + widest,
            "a body can reach the boundary and stand in a shrub"
        );
        const { assert!(OPEN_RADIUS > CLEAR_RADIUS) };
    }
}
