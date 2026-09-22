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

/// The same pose, placed against the two bodies instead of against the arena.
///
/// A fixed pose cannot frame a moment. The named moments happen wherever the
/// fight has drifted to by the tick they occur on, and the `defeat` capture
/// proved what that costs: the defeat landed three units off the arena centre
/// with both bodies almost exactly in line with a camera that looks along `x`,
/// so one body stood in front of the other and the frame showed a single figure
/// standing alone. Nothing was wrong with the renderer or the pose; the camera
/// was pointed at a place rather than at a fight.
///
/// So the pose's `offset` is reinterpreted, for a frozen moment only, as a
/// distance in the **fight's** own frame: `x` across the line between the two
/// bodies, `y` above the ground they stand on, `z` along that line from the
/// midpoint. Every moment is then framed the same way whatever the fight did to
/// get there, which is what makes two captures comparable.
/// Returns the position and the yaw and pitch, in radians, for
/// [`Camera::place`] — rather than a whole camera, so that the aspect ratio the
/// window set survives, and so that the arithmetic can be checked without one.
#[must_use]
pub fn frame_the_fight(
    pose: &CombatCameraPose,
    player: Vec3,
    adversary: Vec3,
) -> Option<(Vec3, f32, f32)> {
    if !player.is_finite() || !adversary.is_finite() {
        return None;
    }
    let midpoint = (player + adversary) * 0.5;
    let along = adversary - player;
    let along = Vec2::new(along.x, along.z);
    // Two bodies standing on the same spot have no line between them to frame,
    // and separation makes that impossible in practice; refusing is still
    // cheaper than dividing by zero.
    let along = along.try_normalize()?;
    let across = Vec2::new(-along.y, along.x);

    let offset = pose.offset;
    let planar = across * offset[0] + along * offset[2];
    let position = Vec3::new(
        midpoint.x + planar.x,
        midpoint.y + offset[1],
        midpoint.z + planar.y,
    );
    // Look at the midpoint at the height a chest sits, not at the grass between
    // their feet.
    let target = midpoint + Vec3::Y * MOMENT_LOOK_HEIGHT;
    let to_target = target - position;
    let planar_length = Vec2::new(to_target.x, to_target.z).length();
    if planar_length <= f32::EPSILON {
        return None;
    }
    let yaw = to_target.x.atan2(-to_target.z);
    let pitch = to_target.y.atan2(planar_length);
    tracing::info!(
        pose = pose.name,
        intent = pose.intent,
        position = ?position.to_array(),
        yaw_degrees = yaw.to_degrees(),
        pitch_degrees = pitch.to_degrees(),
        separation = Vec2::new(adversary.x - player.x, adversary.z - player.z).length(),
        "combat camera framing the fight"
    );
    Some((position, yaw, pitch))
}

/// How far above the ground a moment camera aims, in world units.
///
/// Chest height on the taller of the two bodies. Aiming at the midpoint itself
/// puts the grass in the middle of the frame and the heads at the top edge.
const MOMENT_LOOK_HEIGHT: f32 = 1.35;

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
        MOMENT_LOOK_HEIGHT, OPEN_RADIUS, OPEN_RISE, centre, combat_camera_pose, floor,
        frame_the_fight,
    };
    use crate::character::TerrainGround;
    use glam::{Vec2, Vec3};
    use veldwake_character::GroundSampler;

    #[test]
    fn framing_the_fight_puts_both_bodies_in_front_of_the_camera() {
        // The property the `defeat` capture violated. Whatever the fight has
        // drifted to, and whichever way round the two bodies are standing, both
        // of them have to be in front of the camera and neither may be directly
        // behind the other.
        let pose = match combat_camera_pose("combat-side") {
            Some(pose) => pose,
            None => panic!("combat-side is a pose"),
        };
        // Every bearing, a fight at a time.
        for step in 0_u8..24 {
            let angle = f32::from(i16::from(step)) * std::f32::consts::TAU / 24.0;
            let separation = 2.3;
            let player = Vec3::new(-65.0, 12.0, 51.0);
            let adversary =
                player + Vec3::new(angle.cos() * separation, 0.0, angle.sin() * separation);
            let (position, yaw, pitch) = match frame_the_fight(pose, player, adversary) {
                Some(placed) => placed,
                None => panic!("a fight {separation} apart has a line between its bodies"),
            };
            assert!(position.is_finite(), "{position} at bearing {angle}");
            assert!(yaw.is_finite() && pitch.is_finite());

            let pitch_cos = pitch.cos();
            let forward = Vec3::new(yaw.sin() * pitch_cos, pitch.sin(), -yaw.cos() * pitch_cos);
            for (name, body) in [("player", player), ("adversary", adversary)] {
                let to_body = body + Vec3::Y * MOMENT_LOOK_HEIGHT - position;
                let ahead = to_body.normalize_or_zero().dot(forward);
                assert!(
                    ahead > 0.80,
                    "the {name} is {ahead:.3} ahead of the camera at bearing {:.1} degrees",
                    angle.to_degrees()
                );
            }
            // Neither body hides the other: the camera looks across the line
            // between them, so their bearings differ.
            let to_player = (player - position).normalize_or_zero();
            let to_adversary = (adversary - position).normalize_or_zero();
            let apart = to_player.dot(to_adversary).clamp(-1.0, 1.0).acos();
            assert!(
                apart.to_degrees() > 8.0,
                "the two bodies are {:.2} degrees apart on screen at bearing {:.1}",
                apart.to_degrees(),
                angle.to_degrees()
            );
        }
    }

    #[test]
    fn framing_the_fight_refuses_what_it_cannot_frame() {
        let pose = match combat_camera_pose("combat-close") {
            Some(pose) => pose,
            None => panic!("combat-close is a pose"),
        };
        let here = Vec3::new(1.0, 2.0, 3.0);
        // Two bodies on one spot have no line between them. Separation makes it
        // impossible in a fight; refusing is still cheaper than a division by
        // zero reaching a projection matrix.
        assert!(frame_the_fight(pose, here, here).is_none());
        assert!(frame_the_fight(pose, here, Vec3::new(f32::NAN, 0.0, 0.0)).is_none());
        assert!(frame_the_fight(pose, Vec3::splat(f32::INFINITY), here).is_none());
    }

    #[test]
    fn framing_the_fight_keeps_the_pose_distances_it_was_given() {
        // The offsets are reinterpreted, not discarded: `combat-close` still has
        // to be nearer than `combat-side`, or a contact frame is no longer a
        // contact frame.
        let player = Vec3::new(-69.0, 10.0, 52.0);
        let adversary = Vec3::new(-69.0, 10.0, 49.5);
        let midpoint = (player + adversary) * 0.5;
        let distance = |name: &str| {
            let pose = match combat_camera_pose(name) {
                Some(pose) => pose,
                None => panic!("{name} is a pose"),
            };
            let (position, _, _) = match frame_the_fight(pose, player, adversary) {
                Some(placed) => placed,
                None => panic!("{name} could not frame the fight"),
            };
            (position - midpoint).length()
        };
        let close = distance("combat-close");
        let side = distance("combat-side");
        let wide = distance("combat-wide");
        assert!(
            close < side,
            "close {close:.2} is not nearer than side {side:.2}"
        );
        assert!(
            side < wide,
            "side {side:.2} is not nearer than wide {wide:.2}"
        );
    }

    use veldwake_combat::fixture;
    use veldwake_procedural::TerrainGenerator;

    fn generator() -> TerrainGenerator {
        TerrainGenerator::golden()
    }

    #[test]
    fn the_arena_is_a_level_open_clearing() {
        let generator = generator();
        let ground = TerrainGround::new(&generator);
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
                        !vegetation.occupied(x + dx, y, z + dz),
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
                    !vegetation.occupied(x as i64, y, z as i64),
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
