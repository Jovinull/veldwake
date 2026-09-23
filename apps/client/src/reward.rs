//! Where the found weapon stands, derived from the gate the world already
//! built.
//!
//! **No search, and no change to `veldwake-procedural`.** M8 derived a gate and
//! recorded its tallest column; the learning it wrote down is that *a gate's
//! tallest column is the lintel over its opening*, so
//! [`veldwake_procedural::LandmarkInstance`]'s `crown_column` already names the
//! centre of the passage.
//! M9 reads that one field, turns it into a world position with the client's
//! own [`column_centre`], and asks the client's
//! own surface grid how high the ground under it is. That is the whole
//! derivation: no candidate lattice, no region sweep, no second placement
//! search, and therefore nothing added to the eager landmark plan whose cost
//! KI-035 already measures.
//!
//! The split follows the M8 boundary exactly. `veldwake-procedural` owns gate
//! geometry and knows nothing about weapons, combat, intents or players;
//! `veldwake-combat` owns the exchange rule and knows nothing about worlds; and
//! this module is the one place the two meet, because the client is where a
//! world position becomes a gameplay fact.
//!
//! **The object is not solid.** It has no keep-out, no collision and no entry
//! in the traversal veto, so `TRAVERSAL_RULE_VERSION` does not move and the
//! gate stays exactly as walkable as M8 left it (LAND-002). A body walks
//! through the weapon, which is the honest consequence of adding no object
//! collision system.

use glam::{Mat4, Vec2, Vec3};

use veldwake_character::{CHARACTER_VOXEL_SIZE, GroundSampler};
use veldwake_combat::armament::RewardSetup;
use veldwake_combat::weapon::{CompiledWeapon, WeaponDescriptor};
use veldwake_procedural::landmark::SpanAxis;
use veldwake_procedural::{LandmarkPlan, SilhouetteClass, TerrainGenerator};

use crate::character::TerrainGround;
use crate::traversal::{column_centre, fnv1a64};

/// Version of the M9 reward adapter's own semantics: which landmark hosts the
/// exchange, which feature of it the anchor is, and how the object is placed.
///
/// Separate from the found weapon's profile version, which is combat's, and
/// separate from every world version, which is procedural's. Bumped when this
/// module changes where the weapon stands or how it is stood there.
pub const REWARD_ADAPTER_VERSION: u32 = 1;

/// How far into the ground the planted blade is sunk, in world units.
///
/// A quarter of a world unit, three character voxels. A blade whose tip rests
/// exactly on the surface reads as balanced on it; a little bite reads as
/// driven in. It is presentation and nothing depends on it but the picture.
pub const SITE_SINK: f32 = 0.25;

/// How far the anchor sits from the centre of the gate's opening, in columns
/// along the gate's own span axis.
///
/// **A measured correction, not a preference.** The first implementation put
/// the weapon at the exact centre of the opening, which is also the line a
/// player walks through a gate and the line the follow camera looks down. The
/// captures are unambiguous: at three world units the planted weapon is a grey
/// slab directly between the camera and the body, occluding the torso and both
/// legs, and the only part of the frame that reads as a sword is its shadow.
/// It destroyed the object's readability and the gate's at once.
///
/// One column to the side is the whole fix. The golden gate's opening is seven
/// columns across, so `±1` is still well inside it and still framed by both
/// shafts; the walking line passes `1.0` world unit away, which is outside the
/// widest body's `0.86` keep-out radius and comfortably inside the `1.75`
/// interaction radius. Nothing was searched for and no placement machinery was
/// added: the anchor is still the gate's own opening centre, offset by one
/// column of the gate's own span axis.
pub const SITE_SPAN_OFFSET: i64 = -1;

/// Yaw of the planted weapon, in radians.
///
/// Zero, and measured rather than chosen. The gate's opening is crossed along
/// `x` and the player approaches it from the overlook to the west, so a viewer
/// looks down the world `x` axis. The blade is two voxels thick on its own `x`
/// and six wide on its own `z`, so at zero yaw the wide face is square to that
/// viewer and the silhouette is as large as the weapon has.
pub const SITE_YAW: f32 = 0.0;

/// Locked identity of the M9 exchange as this world places it.
///
/// **Old** none, **new** `0x0815_132f_1a6b_7572`, **why**: first lock, M9. It
/// folds the world fingerprint, the adapter version, the anchor column and the
/// terrain face under it, the interact radius, both weapons' identity
/// fingerprints, both compiled attack specs and the found weapon's profile
/// version.
///
/// **Old** `0x0815_132f_1a6b_7572`, **new** `0x0f08_fbf7_08e3_206d`, **why**:
/// the anchor moved one column, from the exact centre of the gate's opening at
/// `(-7, 58)` to `(-7, 57)`. The centre is also the line a body walks through a
/// gate and the line the follow camera looks down, and the driven captures show
/// the planted weapon occluding the whole torso and both legs from three world
/// units out — the only thing in the frame that read as a sword was its shadow.
/// See [`SITE_SPAN_OFFSET`]. Nothing else in the signature changed: same world,
/// same weapons, same specs, same radius.
///
/// **Old** `0x0f08_fbf7_08e3_206d`, **new** `0x08ac_216e_2ef0_0962`, **why**:
/// the owner's approved retune after the M9 revisit's owner playtest failed
/// (2026-09-23): the found attack's damage `32` -> `28` and windup `0.26` s ->
/// `0.30` s (`31` -> `36` ticks), and nothing else. The signature folds both
/// compiled attack specs, so the found one's windup and damage move it; the
/// anchor, its ground, the radius, both weapon identities and
/// `FOUND_WEAPON_PROFILE_VERSION` (still `1`) are unchanged.
///
/// **It is deliberately not part of any chunk-cache key.** A disk-cache entry
/// answers one question — will the source produce the same chunk bytes? — and
/// a weapon standing in a gate changes no chunk byte at all. Folding gameplay
/// identity into the cache key would invalidate every cached chunk in the world
/// for a change that generated nothing.
pub const REWARD_BEHAVIOR_SIGNATURE: u64 = 0x08ac_216e_2ef0_0962;

/// Where the fixed exchange stands in one world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RewardSite {
    /// The gate column the anchor is, in world columns.
    pub column: (i64, i64),
    /// The centre of that column, which is the position the rules measure
    /// against.
    pub position: Vec2,
    /// The face a sole rests on there, from the same query the body uses.
    pub ground: f64,
}

/// The gate that hosts the exchange, if this world composed one.
///
/// The gate and not the spire, because the spire hosts the fight: one direction
/// from the overlook ends in an adversary and the other ends in a choice, and
/// putting both at one landmark would have removed the choice M8 built.
#[must_use]
pub fn host(plan: &LandmarkPlan) -> Option<&veldwake_procedural::LandmarkInstance> {
    plan.instances()
        .iter()
        .find(|instance| instance.class() == SilhouetteClass::Gate)
}

/// The anchor column: the gate's opening centre, one column to the side.
///
/// The crown column of a gate is the lintel over its passage, so it is the
/// centre of the opening. [`SITE_SPAN_OFFSET`] then steps one column along the
/// gate's own span axis, for the reason that constant records.
#[must_use]
pub fn anchor_column(plan: &LandmarkPlan) -> Option<(i64, i64)> {
    let gate = host(plan)?;
    let (x, z) = gate.crown_column;
    Some(match gate.descriptor.axis {
        SpanAxis::X => (x + SITE_SPAN_OFFSET, z),
        SpanAxis::Z => (x, z + SITE_SPAN_OFFSET),
    })
}

/// Resolves the exchange site in one world, or `None` when the world composed
/// no gate.
///
/// A world without the golden composition simply has no exchange, exactly as a
/// world without landmarks has no discovery (KI-033). Nothing is placed
/// somewhere the composition does not hold.
#[must_use]
pub fn site(generator: &TerrainGenerator) -> Option<RewardSite> {
    let column = anchor_column(generator.landmarks())?;
    let position = column_centre(column.0, column.1);
    let ground =
        TerrainGround::new(generator).surface(f64::from(position.x), f64::from(position.y))?;
    Some(RewardSite {
        column,
        position,
        ground,
    })
}

/// The world matrix of the weapon standing at the site.
///
/// The same composition the renderer already uses for a weapon in a hand, with
/// a placement in the world where the body's own matrix would be: a translation
/// and the character voxel scale, then the weapon's grid origin. The height is
/// solved rather than authored — the blade's own tip is put on the ground and
/// then sunk by [`SITE_SINK`] — so a differently proportioned weapon plants
/// itself correctly without a second number being edited.
#[must_use]
pub fn site_matrix(site: &RewardSite, weapon: &CompiledWeapon) -> Mat4 {
    let origin = Vec3::from_array(weapon.metrics().origin);
    // Where the blade's point sits relative to the hand origin, in grid cells.
    let tip_local = origin.y + weapon.blade().tip.y;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a terrain face is a small integer height in a finite region"
    )]
    let ground = site.ground as f32;
    let placement = Vec3::new(
        site.position.x,
        ground - tip_local * CHARACTER_VOXEL_SIZE - SITE_SINK,
        site.position.y,
    );
    Mat4::from_translation(placement)
        * Mat4::from_rotation_y(SITE_YAW)
        * Mat4::from_scale(Vec3::splat(CHARACTER_VOXEL_SIZE))
        * Mat4::from_translation(origin)
}

/// Everything that decides what the M9 exchange is, folded into one value.
#[must_use]
pub fn reward_signature(
    generator: &TerrainGenerator,
    site: &RewardSite,
    setup: &RewardSetup,
    original: &WeaponDescriptor,
) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"veldwake.m9.reward");
    bytes.extend_from_slice(&generator.fingerprint().to_le_bytes());
    bytes.extend_from_slice(&REWARD_ADAPTER_VERSION.to_le_bytes());
    bytes.extend_from_slice(&veldwake_combat::fixture::FOUND_WEAPON_PROFILE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&site.column.0.to_le_bytes());
    bytes.extend_from_slice(&site.column.1.to_le_bytes());
    bytes.extend_from_slice(&site.ground.to_bits().to_le_bytes());
    bytes.extend_from_slice(&setup.interact_radius.to_bits().to_le_bytes());
    for descriptor in [original, &setup.weapon] {
        bytes.extend_from_slice(
            &veldwake_combat::weapon::WeaponIdentity::of(descriptor)
                .fingerprint()
                .to_le_bytes(),
        );
    }
    // Both swings, as the runtime will actually run them: ticks, not seconds.
    for authored in [&veldwake_combat::fixture::player_attack(), &setup.attack] {
        match authored.compile() {
            Ok(spec) => {
                for value in [
                    spec.windup(),
                    spec.active(),
                    spec.recovery(),
                    spec.stagger(),
                    spec.hitstop(),
                ] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                bytes.extend_from_slice(&spec.damage().to_le_bytes());
                bytes.extend_from_slice(&spec.step_in().to_bits().to_le_bytes());
                bytes.extend_from_slice(&spec.knockback().to_bits().to_le_bytes());
            }
            // An authored swing that cannot compile cannot reach a runtime
            // either; folding a marker keeps the signature total rather than
            // silently equal to a valid one.
            Err(_) => bytes.extend_from_slice(b"uncompilable"),
        }
    }
    // The authored initial armament: the player starts with the original.
    bytes.extend_from_slice(
        &veldwake_combat::ArmamentState::initial()
            .player()
            .index()
            .to_le_bytes(),
    );
    fnv1a64(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        REWARD_BEHAVIOR_SIGNATURE, SITE_SINK, anchor_column, host, reward_signature, site,
        site_matrix,
    };
    use crate::character::TerrainGround;
    use crate::traversal::{
        BODY_KEEP_OUT_HEIGHT, BODY_KEEP_OUT_RADIUS, ROUTE_START_X, ROUTE_START_Z, SurfaceGrid,
        audit, column_centre,
    };
    use glam::Vec3;
    use veldwake_character::CHARACTER_VOXEL_SIZE;
    use veldwake_combat::fixture;
    use veldwake_combat::weapon::WeaponCompiler;
    use veldwake_procedural::{SilhouetteClass, TerrainGenerator};

    fn golden() -> TerrainGenerator {
        TerrainGenerator::golden()
    }

    #[test]
    fn the_anchor_is_the_gates_own_opening_and_nothing_was_searched_for() {
        let generator = golden();
        let plan = generator.landmarks();
        let Some(gate) = host(plan) else {
            panic!("the golden world must carry a gate");
        };
        assert_eq!(gate.class(), SilhouetteClass::Gate);
        let Some(anchor) = anchor_column(plan) else {
            panic!("a gate must give an anchor");
        };
        // One column along the gate's span axis from the opening centre, and
        // nothing else: no search, no lattice, no second placement rule.
        let expected = match gate.descriptor.axis {
            veldwake_procedural::landmark::SpanAxis::X => (
                gate.crown_column.0 + super::SITE_SPAN_OFFSET,
                gate.crown_column.1,
            ),
            veldwake_procedural::landmark::SpanAxis::Z => (
                gate.crown_column.0,
                gate.crown_column.1 + super::SITE_SPAN_OFFSET,
            ),
        };
        assert_eq!(anchor, expected);
        // Still inside the gate's own footprint, so it is still framed by both
        // shafts rather than standing outside the structure.
        let (min_x, max_x, min_z, max_z) = gate.bounds;
        assert!(
            anchor.0 >= min_x && anchor.0 <= max_x,
            "the anchor left the gate on x"
        );
        assert!(
            anchor.1 >= min_z && anchor.1 <= max_z,
            "the anchor left the gate on z"
        );
        // And off the line a body walks through the opening, by enough that the
        // widest body's keep-out radius never reaches it.
        let offset =
            (anchor.0 - gate.crown_column.0).abs() + (anchor.1 - gate.crown_column.1).abs();
        assert_eq!(offset, 1, "the anchor is not exactly one column off centre");
        assert!(
            f64::from(i32::try_from(offset).unwrap_or(1)) > BODY_KEEP_OUT_RADIUS,
            "the walking line still passes through the object"
        );
    }

    #[test]
    fn the_site_is_dry_level_clear_and_outside_every_keep_out() {
        let generator = golden();
        let Some(resolved) = site(&generator) else {
            panic!("the golden world must resolve a site");
        };
        let grid = SurfaceGrid::sample(&generator);
        let (x, z) = resolved.column;

        assert!(!grid.has_water(x, z), "the site is in water");
        assert!(
            !grid.blocked_by_landmark(x, z),
            "the site is inside landmark stone"
        );
        assert!(grid.standable(x, z), "a body cannot stand at the site");

        // Level enough to interact from, out to the interaction radius.
        let Some(centre_face) = grid.support_at(x, z) else {
            panic!("the site column must have a surface");
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the interact radius is under two world units"
        )]
        let reach = fixture::FOUND_INTERACT_RADIUS.ceil() as i64;
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                if let Some(face) = grid.support_at(x + dx, z + dz) {
                    assert!(
                        (face - centre_face).abs() <= 1,
                        "the ground around the site is not level at ({}, {})",
                        x + dx,
                        z + dz
                    );
                }
            }
        }

        // The site is outside every landmark keep-out the body carries.
        let plan = generator.landmarks();
        if let Some((low, high)) = plan.column(x, z) {
            let body_top = i64::from(centre_face) + BODY_KEEP_OUT_HEIGHT.ceil() as i64;
            assert!(
                low > body_top,
                "landmark stone {low}..{high} reaches the body standing on face {centre_face}"
            );
        }
    }

    #[test]
    fn the_site_is_reachable_on_foot_from_where_a_session_starts() {
        let generator = golden();
        let Some(resolved) = site(&generator) else {
            panic!("the golden world must resolve a site");
        };
        let grid = SurfaceGrid::sample(&generator);
        let Ok(movement) = fixture::movement().compile() else {
            panic!("the movement spec must compile");
        };
        let report = audit(&grid, &movement, (ROUTE_START_X, ROUTE_START_Z));
        let steps = report.distance_to(&grid, resolved.column.0, resolved.column.1);
        assert!(
            steps.is_some(),
            "the exchange site cannot be walked to from the route start"
        );
    }

    #[test]
    fn a_session_does_not_begin_inside_its_own_reward() {
        let generator = golden();
        let Some(resolved) = site(&generator) else {
            panic!("the golden world must resolve a site");
        };
        let start = column_centre(ROUTE_START_X, ROUTE_START_Z);
        let distance = (resolved.position - start).length();
        assert!(
            distance > fixture::FOUND_INTERACT_RADIUS * 4.0,
            "the route start is {distance} from the site, inside its own reward"
        );
    }

    #[test]
    fn the_planted_blade_stands_on_the_ground_it_was_put_on() {
        let generator = golden();
        let Some(resolved) = site(&generator) else {
            panic!("the golden world must resolve a site");
        };
        let Ok(weapon) =
            WeaponCompiler::new().compile_descriptor(&fixture::found_weapon_descriptor())
        else {
            panic!("the found weapon must compile");
        };
        let matrix = site_matrix(&resolved, &weapon);
        let tip = matrix.transform_point3(weapon.blade().tip);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a terrain face is a small integer height"
        )]
        let ground = resolved.ground as f32;
        assert!(
            (tip.y - (ground - SITE_SINK)).abs() < 1.0e-4,
            "the blade tip is at {} and the ground is {ground}",
            tip.y
        );
        // The horizontal position is the column centre, not an edge.
        assert!((tip.x - resolved.position.x).abs() < 1.0e-4);
        assert!((tip.z - resolved.position.y).abs() < 1.0e-4);
        // And the pommel is above the ground by the weapon's own length.
        let base = matrix.transform_point3(weapon.blade().base);
        assert!(base.y > tip.y, "the weapon is planted upside down");
        let length = (base - tip).length();
        assert!(
            (length - weapon.blade().world_length()).abs() < 1.0e-4,
            "the planted blade is the wrong length: {length}"
        );
        // The scale really is the character voxel scale and not something else.
        let up = matrix.transform_vector3(Vec3::Y);
        assert!((up.length() - CHARACTER_VOXEL_SIZE).abs() < 1.0e-5);
    }

    #[test]
    fn the_reward_identity_is_locked() {
        let generator = golden();
        let Some(resolved) = site(&generator) else {
            panic!("the golden world must resolve a site");
        };
        let measured = reward_signature(
            &generator,
            &resolved,
            &fixture::reward_setup(),
            &fixture::weapon_descriptor(),
        );
        assert_eq!(
            measured, REWARD_BEHAVIOR_SIGNATURE,
            "the M9 exchange identity moved; re-lock deliberately with a reason"
        );
    }

    #[test]
    fn resolving_a_reward_writes_no_voxel_into_the_world() {
        // The guard the M9 design promised: if generated chunk bytes ever have
        // to change for the reward, stop and explain. Nothing in
        // `veldwake-procedural` knows an exchange exists, so this compares the
        // chunks around the gate from a generator the reward was resolved
        // against with the chunks from one it was not. A future change that
        // made placement reach into generation would fail here rather than
        // quietly invalidating every cached chunk in the world.
        let untouched = golden();
        let probed = golden();
        let Some(resolved) = site(&probed) else {
            panic!("the golden world must resolve a site");
        };
        let _ = site_matrix(&resolved, &{
            let Ok(weapon) =
                WeaponCompiler::new().compile_descriptor(&fixture::found_weapon_descriptor())
            else {
                panic!("the found weapon must compile");
            };
            weapon
        });

        let Some(gate) = host(probed.landmarks()) else {
            panic!("the golden world must carry a gate");
        };
        let (min_x, max_x, min_z, max_z) = gate.bounds;
        let chunk_of = |value: i64| -> i32 {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "region columns are hundreds; the chunk index fits an i32"
            )]
            let index = value.div_euclid(veldwake_voxel::CHUNK_EDGE as i64) as i32;
            index
        };
        let mut compared = 0_u32;
        for chunk_z in chunk_of(min_z - 4)..=chunk_of(max_z + 4) {
            for chunk_y in -1..=2 {
                for chunk_x in chunk_of(min_x - 4)..=chunk_of(max_x + 4) {
                    let coord = veldwake_voxel::ChunkCoord::new(chunk_x, chunk_y, chunk_z);
                    assert_eq!(
                        untouched.generate(coord),
                        probed.generate(coord),
                        "resolving the reward changed the voxels of {coord:?}"
                    );
                    compared += 1;
                }
            }
        }
        assert!(compared > 0, "no chunk was compared");
        assert_eq!(
            untouched.fingerprint(),
            probed.fingerprint(),
            "resolving the reward moved the world identity"
        );
    }

    #[test]
    fn the_gate_is_still_walkable_with_the_weapon_standing_in_it() {
        // LAND-002, re-proved with the object present. The planted weapon has
        // no keep-out, no collision and no entry in the traversal veto, so the
        // crossing must be exactly the crossing M8 proved — and it must be the
        // same crossing whichever weapon the player happens to be holding,
        // because an armament is not a world fact.
        use veldwake_combat::armament::WeaponVariant;
        use veldwake_combat::encounter::{Encounter, WorldContact};
        use veldwake_combat::{Intent, Side};

        let generator = golden();
        let Some(resolved) = site(&generator) else {
            panic!("the golden world must resolve a site");
        };
        let grid = SurfaceGrid::sample(&generator);
        let Ok(movement) = fixture::movement().compile() else {
            panic!("the movement spec must compile");
        };
        let Some(gate) = host(generator.landmarks()) else {
            panic!("the golden world must carry a gate");
        };
        let (low_x, high_x, low_z, high_z) = gate.bounds;
        // The opening is crossed along the short axis of the footprint.
        let corridor = 3_i64;
        let start = (low_x - corridor, (low_z + high_z) / 2);
        let goal = (high_x + corridor, (low_z + high_z) / 2);
        let report = audit(&grid, &movement, start);
        let Some(path) = report.path_to(&grid, goal.0, goal.1) else {
            panic!("no walk crosses the gate from {start:?} to {goal:?}");
        };
        assert!(
            path.iter()
                .any(|(x, z)| *x >= low_x && *x <= high_x && *z >= low_z && *z <= high_z),
            "the crossing does not pass through the gate footprint"
        );

        let ground = TerrainGround::new(&generator);
        let veto = crate::traversal::TerrainWalkability::new(&generator);

        for variant in [WeaponVariant::Original, WeaponVariant::Found] {
            let mut setup = crate::traversal::traversal_setup();
            setup.starts[Side::Player.index()] = column_centre(start.0, start.1);
            let Ok(mut encounter) = Encounter::new(&setup, Some(&ground)) else {
                panic!("the traversal encounter must build");
            };
            encounter.arm();
            let world =
                WorldContact::terrain_with_weapon_exchange(&ground, &veto, resolved.position);
            if variant == WeaponVariant::Found {
                // Teleporting is not available, so exchange where the body is:
                // the point of this test is the crossing, not the walk to it.
                let here = encounter.combatant(Side::Player).position();
                let at_hand = WorldContact::terrain_with_weapon_exchange(&ground, &veto, here);
                let _ = encounter.step(
                    Intent::player(glam::Vec2::ZERO, false, false).interacting(true),
                    at_hand,
                );
                assert_eq!(encounter.armament().player(), WeaponVariant::Found);
            }

            // Walk the audited crossing at walk speed through the real rules.
            let mut refused = 0_u32;
            for pair in path.windows(2) {
                let target = column_centre(pair[1].0, pair[1].1);
                for _ in 0..600 {
                    let here = encounter.combatant(Side::Player).position();
                    let offset = target - here;
                    if offset.length() < 0.15 {
                        break;
                    }
                    let before = here;
                    let _ = encounter.step(
                        Intent::player(offset.normalize_or_zero(), false, false),
                        world,
                    );
                    if (encounter.combatant(Side::Player).position() - before).length() < 1.0e-5 {
                        refused += 1;
                        break;
                    }
                }
            }
            let finished = encounter.combatant(Side::Player).position();
            let target = column_centre(goal.0, goal.1);
            assert!(
                refused == 0,
                "the body was refused {refused} times crossing the gate holding the {} weapon",
                variant.name()
            );
            assert!(
                (finished - target).length() < 2.0,
                "the body holding the {} weapon stopped {} from the far side",
                variant.name(),
                (finished - target).length()
            );
        }
    }

    #[test]
    fn the_traversal_session_carries_the_exchange() {
        let setup = crate::traversal::traversal_setup();
        let Some(reward) = setup.reward else {
            panic!("the traversal session must offer the exchange");
        };
        assert_eq!(reward.weapon, fixture::found_weapon_descriptor());
        assert_eq!(reward.attack, fixture::found_attack());
        assert!((reward.interact_radius - fixture::FOUND_INTERACT_RADIUS).abs() < f32::EPSILON);
        // And the M6 arena session does not, because M9 is a traversal slice.
        let arena = fixture::setup(glam::Vec2::ZERO, fixture::ARENA_RADIUS);
        assert!(arena.reward.is_none());
    }
}
