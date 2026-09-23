//! The M6 fixtures: two named bodies, one weapon, one tuning, one encounter.
//!
//! These exist so an intentional change to combat is a conscious decision rather
//! than a surprise, following the shape M4 set for terrain and M5 for a
//! character: named descriptors lock intent, fingerprints lock exact output, and
//! named moments lock the states captures are taken at.
//!
//! The two bodies are **the two M5 fixtures**, and that is a deliberate saving
//! rather than laziness. `golden` and `sturdy` are already proved to be two
//! different people in one style — different in every proportion and in the
//! palette — and inventing a third body to be the enemy would add a descriptor
//! with no new capability and a fourth signature to keep.

use glam::Vec2;

use veldwake_character::descriptor::{CharacterSeed, Proportions};
use veldwake_character::fixture::golden_descriptor;
use veldwake_character::ground::FlatGround;
use veldwake_character::material::{GarmentScheme, HairTone, PaletteChoice, SkinTone};
use veldwake_character::{CharacterDescriptor, GroundSampler};

use crate::armament::RewardSetup;
use crate::combatant::{Action, Intent, SIDES, Side};
use crate::encounter::{
    CombatCounters, Encounter, EncounterError, EncounterSetup, PlayerVictoryPolicy, WorldContact,
};
use crate::hash::{fnv1a64, push_f32, push_u16, push_u32, push_u64};
use crate::material::WeaponScheme;
use crate::script::{
    EncounterScript, GOLDEN_SCRIPT, MomentKind, NAMED_MOMENTS, ScriptRunner, at_moment,
};
use crate::spec::{
    ArenaSpec, AuthoredAdversary, AuthoredAttack, AuthoredDodge, AuthoredMovement,
    AuthoredPressure, AuthoredTuning, CombatSeed,
};
use crate::tick::Ticks;
use crate::weapon::{WeaponDescriptor, WeaponSeed};

/// The body the player drives.
#[must_use]
pub fn player_descriptor() -> CharacterDescriptor {
    golden_descriptor()
}

/// The body that fights back.
///
/// **Its own descriptor, and the reason is a test that failed.** The first draft
/// reused M5's `sturdy` fixture, on the argument that two locked bodies already
/// prove the compiler is a compiler and a third would add nothing. Then
/// `the_two_bodies_are_distinguishable_at_a_glance` found that `golden` and
/// `sturdy` share an absolute hip width of eight voxels *and* a limb thickness of
/// three: `sturdy` is broader only in proportion to its own height, and it is
/// **shorter** than the player. An enemy that is smaller than you and the same
/// width is not an enemy a viewer reads at a glance.
///
/// This one is `sturdy`'s proportions carried to a taller body: thirty-three
/// character voxels against the player's twenty-eight, eleven voxels of hip
/// against eight, four of limb against three. It looms.
#[must_use]
pub fn adversary_descriptor() -> CharacterDescriptor {
    CharacterDescriptor {
        proportions: Proportions {
            total_height_units: 2.75,
            // `0.15` rather than `sturdy`'s `0.20`, and a capture is why. At
            // `0.20` a thirty-three voxel body gets a hand seven voxels deep —
            // as deep as it is long — and the close contact frame came back with
            // two brown slabs and two brass guards piled where the blades meet,
            // one indistinct mass instead of a hit. `0.15` gives five, which the
            // style contract's `1.20`–`1.80` hand-to-forearm band still allows,
            // and the hand reads as a fist holding a grip.
            hand_depth_fraction: 0.15,
            ..Proportions::sturdy()
        },
        seed: CharacterSeed(0x4144_5645_5253_0001),
        palette: PaletteChoice {
            skin: SkinTone::Deep,
            hair: HairTone::Dark,
            garment: GarmentScheme::SlateWool,
        },
        ..CharacterDescriptor::golden()
    }
}

/// The weapon a combatant starts with, and the only one the adversary ever
/// holds.
#[must_use]
pub fn weapon_descriptor() -> WeaponDescriptor {
    WeaponDescriptor::golden()
}

/// Version of the M9 found-weapon profile in
/// `docs/audiovisual/COMBAT_STYLE.md`.
///
/// **Deliberately not [`crate::weapon::COMBAT_STYLE_VERSION`].** That constant
/// is hashed into every [`crate::weapon::WeaponIdentity`], so bumping it
/// because M9 adds a second accepted weapon would change the historical M6
/// weapon's identity while its descriptor, its geometry and its behaviour are
/// untouched. The M9 profile is additive under the same shared combat grammar,
/// so it gets its own narrow version, which participates in the M9 signatures
/// and in nothing historical.
pub const FOUND_WEAPON_PROFILE_VERSION: u32 = 1;

/// The weapon standing at the exchange site: a found longblade.
///
/// **Not a two-handed sword, and the documentation must not call it one.** The
/// weapon hangs off `HandR` exactly as the original does and the free hand does
/// nothing; M9 adds no second-hand contact and no two-handed pose. What is
/// longer is the blade, the guard and the grip, and the grip is longer because
/// a heavier blade wants more lever, not because a second fist is on it.
///
/// Every number here is evidence-backed rather than chosen by taste. The blade
/// is `20` character voxels against the original's `14`, which is the largest
/// length that still wins the reference fight under the pessimistic
/// both-sides-armed measurement. The grip pitch is `1.10` rather than the
/// original's `0.90` because at `0.90` a twenty-voxel blade hangs *below* the
/// terrain in the carry pose — measured at `-0.029` world units, against the
/// original's `+0.276` — and `1.10` puts the tip at `+0.359`, clearing better
/// than the weapon the player already carries. The blade is six voxels across
/// the swing plane rather than four purely for silhouette: width costs nothing
/// in reach and nothing in clearance, and it is what makes the two read apart
/// at a glance. [`WeaponScheme::DarkIron`] already existed and was drawn by
/// nothing; M9 uses it rather than authoring a second palette.
#[must_use]
pub fn found_weapon_descriptor() -> WeaponDescriptor {
    WeaponDescriptor {
        seed: WeaponSeed(0x5645_4c44_4652_4541),
        blade_length: 20,
        blade_width: 6,
        blade_thickness: 2,
        guard_width: 8,
        guard_height: 2,
        grip_length: 6,
        grip_thickness: 2,
        pommel_width: 4,
        pommel_height: 2,
        grip_above_hand: 2,
        grip_pitch: 1.10,
        scheme: WeaponScheme::DarkIron,
        ..WeaponDescriptor::golden()
    }
}

/// The swing the found longblade executes.
///
/// The sidegrade, in one table. Against [`player_attack`]: windup `0.30`
/// against `0.18`, recovery `0.48` against `0.34`, damage `28` against `24`.
/// Compiled that is `36 / 14 / 58` ticks and `108` total against `22 / 12 / 41`
/// and `75`.
///
/// The relationship the numbers were chosen for, and the one a test asserts:
/// the extra reach buys the player about as much closing time as the extra
/// commitment costs. The adversary approaches at `2.40` u/s and commits at
/// `2.20`, so swinging from the found weapon's connect-out rather than the
/// original's buys roughly `28` ticks before the adversary is in its own range,
/// and the longer action costs `108 - 75 = 33` ticks of lock (`28` before the
/// retune below). Against that M6 approach the two are close to even; against
/// combat initiative they were not, which is what the retune answers.
///
/// `step_in` falls to `0.30` because a weapon that already reaches does not
/// need to close as much, and `knockback` rises to `0.50` because a heavier
/// blade that lands should buy space.
///
/// **Retuned once, by the owner, after the M9 revisit's owner playtest failed
/// (2026-09-23).** Old: damage `32` (three swings to fell `96` health) and a
/// windup of `0.26` s (`31` ticks). New: damage `28` (four swings, like the
/// original's `24`, each still heavier) and a windup of `0.30` s (`36` ticks).
/// Why: the found weapon's reach gave a careful player easier contact every
/// fight while its commitment was almost never charged, and needing one hit
/// fewer compounded that; `docs/planning/M9_MEANINGFUL_REWARD.md` records the
/// causal investigation. Nothing else about the weapon changed.
#[must_use]
pub fn found_attack() -> AuthoredAttack {
    AuthoredAttack {
        windup_seconds: 0.30,
        active_seconds: 0.12,
        recovery_seconds: 0.48,
        stagger_seconds: 0.34,
        hitstop_seconds: 0.08,
        damage: 28,
        step_in: 0.30,
        knockback: 0.50,
    }
}

/// How close a body must be to the exchange site to swap weapons, in world
/// units.
///
/// `1.75`: the player's capsule radius is `0.627`, so this is a little over a
/// body and an arm. It is comfortably inside the gate opening the client
/// anchors the site in, and comfortably outside anything at the route start, so
/// a session cannot begin inside its own reward.
pub const FOUND_INTERACT_RADIUS: f32 = 1.75;

/// The exchange the M9 traversal session is configured with.
#[must_use]
pub fn reward_setup() -> RewardSetup {
    RewardSetup {
        weapon: found_weapon_descriptor(),
        attack: found_attack(),
        interact_radius: FOUND_INTERACT_RADIUS,
    }
}

/// The player's swing.
///
/// Short windup, brief window, long recovery: a commitment the player can feel
/// without losing the ability to answer a telegraph.
#[must_use]
pub fn player_attack() -> AuthoredAttack {
    AuthoredAttack {
        windup_seconds: 0.18,
        active_seconds: 0.10,
        recovery_seconds: 0.34,
        stagger_seconds: 0.30,
        hitstop_seconds: 0.07,
        damage: 24,
        step_in: 0.35,
        knockback: 0.35,
    }
}

/// The adversary's swing.
///
/// The windup is two and a half times the player's, and that is the whole
/// readability claim: the telegraph is the only warning, so it has to be long
/// enough to read and short enough to still be a threat.
#[must_use]
pub fn adversary_attack() -> AuthoredAttack {
    AuthoredAttack {
        windup_seconds: 0.45,
        active_seconds: 0.12,
        recovery_seconds: 0.60,
        stagger_seconds: 0.30,
        hitstop_seconds: 0.07,
        damage: 18,
        step_in: 0.45,
        knockback: 0.30,
    }
}

/// The dodge.
///
/// `2.2` world units in `0.30` seconds. The distance is not a taste: it has to
/// exceed the slack between the adversary's reach and the two bodies' radii, and
/// `a_dodge_can_actually_escape_the_swing_it_is_for` checks the arithmetic.
#[must_use]
pub fn dodge() -> AuthoredDodge {
    AuthoredDodge {
        duration_seconds: 0.30,
        cooldown_seconds: 0.25,
        distance: 2.2,
    }
}

/// How both bodies move.
#[must_use]
pub fn movement() -> AuthoredMovement {
    AuthoredMovement {
        // About `2.9` leg lengths per second for the golden humanoid, which the
        // gait blends as a fast walk going on a run.
        speed: 3.4,
        turn_rate: 9.0,
        // One terrain voxel up, two down: the leg is `1.167` world units, so a
        // single terrace of its own world is a step it can take.
        max_step_up: 1.0,
        max_drop: 2.0,
        recovery_speed_scale: 0.25,
    }
}

/// The adversary's ranges and pauses.
#[must_use]
pub fn adversary() -> AuthoredAdversary {
    AuthoredAdversary {
        aggro_radius: 14.0,
        // Comfortably inside the adversary's own measured reach. `combat-probe
        // reach` says its blade connects out to `2.58` centre to centre, and the
        // first draft of this number was `2.6` — outside that, so the swing only
        // landed because of the step-in. Committing from inside reach is what
        // makes a telegraph a promise rather than a bluff.
        strike_range: 2.2,
        // Just above the closest the two bodies can be: the capsules are `0.64`
        // and `0.63`, so the push-out never lets them nearer than `1.27`.
        min_range: 1.4,
        approach_speed: 2.4,
        reposition_speed: 1.8,
        recover_seconds: 0.55,
        reposition_seconds: 0.80,
    }
}

/// The adversary's pressure lunge: combat initiative's second attack.
///
/// Every number was measured, not carried over; `docs/planning/COMBAT_INITIATIVE.md`
/// derives each one and records the values it replaced. The three that were
/// moved by evidence:
///
/// - **windup `0.55` s, not the primary's `0.45`.** At `54` ticks a player
///   walking straight at the lunge had to leave its line within `28` ticks
///   (`233` ms) to escape by walking, and the real client reproduced the
///   failure at about `280` ms. At `66` ticks walking escapes up to `40` ticks
///   and dodging up to `48`, from either side, anywhere in the band;
/// - **whiff recovery `1.00` s.** A reader reacting at `400` ms to a missed
///   lunge landed its answer one tick before `0.90` s ran out, which is not an
///   opening; at `1.00` s it lands with thirteen ticks to spare;
/// - **the band `4.35`–`4.65`.** Its near edge is where a player running
///   straight in and swinging, under every misjudgement the oracles use, stops
///   getting its blade there first; its far edge is inside the distance at
///   which the lunge still reaches a body that does nothing (`4.70`).
#[must_use]
pub fn pressure() -> AuthoredPressure {
    AuthoredPressure {
        attack: AuthoredAttack {
            windup_seconds: 0.55,
            active_seconds: 0.12,
            // A lunge that met nothing is spent.
            recovery_seconds: 1.00,
            stagger_seconds: 0.30,
            hitstop_seconds: 0.07,
            damage: 18,
            // No walk-in during the windup: the whole approach is the lunge.
            step_in: 0.0,
            knockback: 0.30,
        },
        // A lunge that connected is stopped by the body it met.
        connect_recovery_seconds: 0.10,
        lunge_distance: 2.0,
        lunge_seconds: 32.0 / 120.0,
        select_min: 4.35,
        select_max: 4.65,
        // The player's dodge speed, carried further: three world units in
        // `0.40` s is `7.5` u/s against the player's `7.33`.
        spacing: AuthoredDodge {
            duration_seconds: 0.40,
            cooldown_seconds: 0.25,
            distance: 3.0,
        },
    }
}

/// The historical tuning with the pressure lunge added, and nothing else
/// changed.
#[must_use]
pub fn initiative_tuning() -> AuthoredTuning {
    AuthoredTuning {
        adversary_pressure: Some(pressure()),
        ..tuning()
    }
}

/// How far apart the two start, in world units.
pub const START_SEPARATION: f32 = 6.0;

/// The player's starting offset from the arena centre.
pub const PLAYER_OFFSET: Vec2 = Vec2::new(0.0, START_SEPARATION * 0.5);
/// The adversary's starting offset from the arena centre.
pub const ADVERSARY_OFFSET: Vec2 = Vec2::new(0.0, -START_SEPARATION * 0.5);

/// The arena radius every recorded encounter uses.
///
/// `5.5` rather than the `7.0` this started at, and the reason came from the
/// world rather than from the rules. The client's arena is a scanned column of
/// the M4 golden region, and the best clearing it has is free of vegetation only
/// out to seven world units; a seven-unit arena would let a body reach the
/// boundary and stand inside a shrub, because there is no vegetation collision.
/// At `5.5` the widest body plus the arena radius is `6.36`, comfortably inside
/// the clearing, so the whole fight happens on ground a viewer can see.
pub const ARENA_RADIUS: f32 = 5.5;

/// The whole combat tuning.
///
/// Numbers only. Where the fight happens is [`setup`]'s business, because an
/// arena centre is a place in a world and this is a table of durations,
/// distances and health.
#[must_use]
pub fn tuning() -> AuthoredTuning {
    AuthoredTuning {
        player_attack: player_attack(),
        adversary_attack: adversary_attack(),
        dodge: dodge(),
        movement: movement(),
        adversary: adversary(),
        // No second attack: the historical encounter is exactly M6's.
        adversary_pressure: None,
        // Five of the adversary's hits, four of the player's: the player has room
        // to learn and the adversary is not a sponge.
        player_health: 96,
        adversary_health: 96,
        defeat_hold_seconds: 2.5,
        seed: CombatSeed::GOLDEN,
    }
}

/// The reference setup, around an arena the caller places.
///
/// The centre is a parameter because the headless fixture stands at the origin
/// while the client's arena is a scanned column of the golden region, and a
/// signature that moved with the world would be useless.
///
/// # Panics
///
/// Never in practice: every radius passed here is a positive constant, and
/// `ArenaSpec::new` rejects only a non-finite centre or a non-positive radius.
/// A fixture that quietly produced a differently shaped arena would be worse
/// than one that stops.
#[must_use]
pub fn setup(arena_centre: Vec2, arena_radius: f32) -> EncounterSetup {
    let Ok(arena) = ArenaSpec::new(arena_centre, arena_radius) else {
        panic!("the fixture arena radius {arena_radius} is not a positive finite number");
    };
    EncounterSetup {
        player: player_descriptor(),
        adversary: adversary_descriptor(),
        weapon: weapon_descriptor(),
        tuning: tuning(),
        starts: [
            arena_centre + PLAYER_OFFSET,
            arena_centre + ADVERSARY_OFFSET,
        ],
        arena: Some(arena),
        // M6's behaviour, and the reason every M6 signature is unchanged: a
        // defeated adversary ends a round rather than a session.
        player_victory: PlayerVictoryPolicy::ResetEncounter,
        facing_offsets: [0.0; SIDES.len()],
        // No exchange. Every M6, M7 and M8 fixture builds through here, and
        // this is why none of their signatures moved in M9.
        reward: None,
    }
}

/// The headless reference setup: the origin, on a flat floor.
#[must_use]
pub fn golden_setup() -> EncounterSetup {
    setup(Vec2::ZERO, ARENA_RADIUS)
}

/// A target dummy, for the tests that are about one rule rather than a fight.
///
/// The two bodies start inside the player's reach and the adversary's aggro
/// radius is small enough that its brain never wakes up, so a test about damage
/// or about one-hit-per-swing measures that and not a second swing arriving from
/// the other side. It is **not** a fight and no evidence about the encounter is
/// taken from it.
#[must_use]
pub fn sandbox_setup() -> EncounterSetup {
    let mut setup = golden_setup();
    setup.starts = [Vec2::new(0.0, 0.9), Vec2::new(0.0, -0.9)];
    setup.tuning.adversary = AuthoredAdversary {
        aggro_radius: 0.30,
        strike_range: 0.20,
        min_range: 0.10,
        ..adversary()
    };
    setup
}

/// The reference setup **with** the M9 exchange configured.
///
/// The same fight, the same arena and the same tuning as [`golden_setup`]; the
/// only difference is that a found weapon is standing at a site. The player
/// still starts holding the original weapon, because process construction
/// always does.
#[must_use]
pub fn found_setup() -> EncounterSetup {
    EncounterSetup {
        reward: Some(reward_setup()),
        ..golden_setup()
    }
}

/// Where the headless exchange site stands: exactly where the player starts,
/// so a fixture can perform the exchange on its first tick without walking.
#[must_use]
pub fn found_site() -> Vec2 {
    golden_setup().starts[Side::Player.index()]
}

/// The floor the headless fixture stands on.
#[must_use]
pub fn golden_ground() -> FlatGround {
    FlatGround::at(0.0)
}

/// What one scripted run did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptOutcome {
    pub ticks: u64,
    pub counters: CombatCounters,
    /// The first tick each named moment occurred on, in `NAMED_MOMENTS` order.
    pub moments: [Option<u64>; NAMED_MOMENTS.len()],
    /// Hash of the whole per-tick trace.
    pub signature: u64,
    /// Lowest health each side reached.
    pub min_health: [u16; SIDES.len()],
    /// Highest overlap the two bodies ever had after separation, in world units.
    pub worst_overlap: f32,
}

impl ScriptOutcome {
    /// Whether every named moment happened at least once.
    #[must_use]
    pub fn every_moment_reached(&self) -> bool {
        self.moments.iter().all(Option::is_some)
    }

    /// The moments that never happened, by name.
    #[must_use]
    pub fn missing_moments(&self) -> Vec<&'static str> {
        NAMED_MOMENTS
            .iter()
            .zip(self.moments)
            .filter(|(_, tick)| tick.is_none())
            .map(|(moment, _)| moment.name)
            .collect()
    }
}

/// Runs a script against a fresh encounter and reports everything it did.
///
/// One function, used by the probe, by the tests and by the signature, so the
/// three cannot disagree about what the reference encounter is.
pub fn run_script(
    script: EncounterScript,
    setup: &EncounterSetup,
    ground: Option<&dyn GroundSampler>,
    ticks: u64,
) -> Result<ScriptOutcome, EncounterError> {
    let mut encounter = Encounter::new(setup, ground)?;
    encounter.arm();
    Ok(trace_script(&mut encounter, script, ground, ticks))
}

/// The same reference script, run by a player who has exchanged weapons.
///
/// Builds [`found_setup`], performs **one real exchange** through the
/// authoritative rule — an interact intent against a world whose exchange site
/// is exactly where the player stands — and then traces the reference script
/// exactly as [`run_script`] does.
///
/// The exchange is deliberately performed rather than authored: there is no way
/// to construct an encounter that begins with the found weapon, because process
/// construction always starts a session holding the original one.
///
/// The trace **format** is the M6 format, byte for byte. The trace **bytes**
/// are not, and are not meant to be: this is a different fixture doing
/// different work with a different weapon, and its signature is locked
/// independently.
pub fn run_found_script(
    script: EncounterScript,
    ground: Option<&dyn GroundSampler>,
    ticks: u64,
) -> Result<ScriptOutcome, EncounterError> {
    let setup = found_setup();
    let mut encounter = Encounter::new(&setup, ground)?;
    encounter.arm();
    let site = found_site();
    let world = match ground {
        Some(ground) => WorldContact::terrain_with_weapon_exchange(ground, &NoVeto, site),
        None => WorldContact::none(),
    };
    // One tick, one press, one exchange. Untraced on purpose: what the lock is
    // about is the fight that follows, not the swap that set it up.
    let _ = encounter.step(
        Intent::player(Vec2::ZERO, false, false).interacting(true),
        world,
    );
    Ok(trace_script(&mut encounter, script, ground, ticks))
}

/// A traversal veto that refuses nothing, so a headless exchange can use the
/// one `WorldContact` constructor that carries a site without inventing terrain
/// rules the fixture does not have.
struct NoVeto;

impl crate::movement::TraversalLegality for NoVeto {
    fn walkable(&self, _x: f64, _z: f64) -> bool {
        true
    }
}

/// Traces an armed encounter through a script, in the one trace format every
/// locked signature uses.
fn trace_script(
    encounter: &mut Encounter,
    script: EncounterScript,
    ground: Option<&dyn GroundSampler>,
    ticks: u64,
) -> ScriptOutcome {
    let reach = reach_of(encounter);
    let mut runner = ScriptRunner::new(script, reach);
    let mut bytes = Vec::with_capacity(ticks as usize * 24);
    let mut moments: [Option<u64>; NAMED_MOMENTS.len()] = [None; NAMED_MOMENTS.len()];
    let mut min_health = [u16::MAX; SIDES.len()];
    let mut worst_overlap = 0.0_f32;

    for _ in 0..ticks {
        let intent = runner.next_intent(encounter);
        let events = encounter.step(intent, WorldContact::from_ground(ground));
        let tick = encounter.tick_index();

        for (index, moment) in NAMED_MOMENTS.iter().enumerate() {
            if moments[index].is_none() && at_moment(moment.kind, encounter, &events) {
                moments[index] = Some(tick);
            }
        }
        for side in SIDES {
            let combatant = encounter.combatant(side);
            min_health[side.index()] = min_health[side.index()].min(combatant.health().current());
        }
        worst_overlap = worst_overlap.max(crate::movement::overlap(
            encounter.combatant(Side::Player).position(),
            encounter.combatant(Side::Adversary).position(),
            encounter.combatant(Side::Player).capsule().radius,
            encounter.combatant(Side::Adversary).capsule().radius,
        ));

        push_u64(&mut bytes, tick);
        for side in SIDES {
            let combatant = encounter.combatant(side);
            push_u16(&mut bytes, combatant.health().current());
            push_u32(
                &mut bytes,
                fnv1a64(
                    combatant
                        .action()
                        .label(encounter.attack_spec(side))
                        .as_bytes(),
                ) as u32,
            );
            push_u32(&mut bytes, combatant.action().elapsed());
            push_f32(&mut bytes, combatant.position().x);
            push_f32(&mut bytes, combatant.position().y);
            push_f32(&mut bytes, combatant.state().facing);
        }
        for event in events.iter() {
            push_u32(&mut bytes, fnv1a64(event.name().as_bytes()) as u32);
        }
        if runner.finished() {
            runner.restart();
        }
    }

    ScriptOutcome {
        ticks,
        counters: *encounter.counters(),
        moments,
        signature: fnv1a64(&bytes),
        min_health,
        worst_overlap,
    }
}

// Where a body's blade goes during its own swing used to be measured here.
// M9 moved it to `crate::reach`, because the encounter's aim-assist range is
// now derived from the same geometry and a fixture may not be a dependency of
// a rule. The fixtures, the probe and the encounter all call
// `reach::attack_envelope`; there is no second copy of the formula.

/// Which way a probed dodge goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DodgeDirection {
    /// Directly away from the adversary.
    Away,
    /// Across the adversary's swing, which leaves the plane the blade sweeps in.
    Lateral,
}

impl DodgeDirection {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Away => "away",
            Self::Lateral => "lateral",
        }
    }
}

/// What happened when the player dodged at one moment of the adversary's swing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DodgeProbe {
    /// Which tick of the adversary's swing the dodge was pressed on.
    pub pressed_at: Ticks,
    /// Whether the dodge was accepted at all.
    pub started: bool,
    /// Whether the player was hit anyway.
    pub hit: bool,
    /// Closest the two bodies came during the adversary's active window.
    pub closest: f32,
}

/// Runs one dodge experiment: stand still, then dodge on a chosen tick of the
/// adversary's swing, and report whether the blade still landed.
///
/// This is how the dodge distance is chosen. Hand arithmetic over reach, radius
/// and step-in gets the order of magnitude and then gets the answer wrong,
/// because the blade's height matters as much as its reach: the first active
/// ticks pass above a body and the last ones pass through it.
pub fn probe_dodge(
    setup: &EncounterSetup,
    ground: Option<&dyn GroundSampler>,
    pressed_at: Ticks,
    direction: DodgeDirection,
    ticks: u64,
) -> Result<DodgeProbe, EncounterError> {
    let mut encounter = Encounter::new(setup, ground)?;
    encounter.arm();
    let spec = *encounter.attack_spec(Side::Adversary);
    let mut started = false;
    let mut hit = false;
    let mut closest = f32::INFINITY;
    for _ in 0..ticks {
        let swing_tick = match encounter.combatant(Side::Adversary).action() {
            Action::Attack { elapsed, .. } => Some(*elapsed),
            _ => None,
        };
        let press = !started && swing_tick == Some(pressed_at);
        let move_world = if press {
            let to_foe = encounter.combatant(Side::Adversary).position()
                - encounter.combatant(Side::Player).position();
            let away = if to_foe.length_squared() > 1.0e-8 {
                -to_foe.normalize()
            } else {
                Vec2::new(0.0, 1.0)
            };
            match direction {
                DodgeDirection::Away => away,
                DodgeDirection::Lateral => Vec2::new(-away.y, away.x),
            }
        } else {
            Vec2::ZERO
        };
        let events = encounter.step(
            Intent::player(move_world, false, press),
            WorldContact::from_ground(ground),
        );
        if press && encounter.combatant(Side::Player).action().is_dodging() {
            started = true;
        }
        if let Some(elapsed) = swing_tick
            && spec.is_active(elapsed)
        {
            closest = closest.min(encounter.separation_distance());
        }
        if events.any(|event| {
            matches!(
                event,
                crate::event::CombatEvent::Hit {
                    victim: Side::Player,
                    ..
                }
            )
        }) {
            hit = true;
        }
        // One swing is the whole experiment.
        if started && !encounter.combatant(Side::Adversary).action().is_attacking() && hit {
            break;
        }
        if encounter.counters().swings[Side::Adversary.index()] > 1 {
            break;
        }
    }
    Ok(DodgeProbe {
        pressed_at,
        started,
        hit,
        closest: if closest.is_finite() { closest } else { 0.0 },
    })
}

/// Hash of an arbitrary trace, so a caller can compare two runs it built itself.
///
/// Exposed because the partition-equivalence evidence has to build its own trace:
/// it drives the encounter through the clock rather than through
/// [`run_script`], which is the whole point of that comparison.
#[must_use]
pub fn trace_signature(bytes: &[u8]) -> u64 {
    fnv1a64(bytes)
}

/// How far the player's blade reaches from its own body centre, in world units.
///
/// Measured from the compiled geometry and the arm rather than declared, so a
/// longer blade or a taller body changes it without anything else being edited.
#[must_use]
pub fn reach_of(encounter: &Encounter) -> f32 {
    let centre = encounter.combatant(Side::Player).position();
    let blade = encounter.blade_world(Side::Player);
    let tip = Vec2::new(blade.tip.x, blade.tip.z);
    (tip - centre).length()
}

/// How many ticks the reference run lasts.
///
/// Two and a half passes of the script, so the repeating exchange is exercised
/// as a loop rather than as one pass.
pub const GOLDEN_RUN_TICKS: u64 = 4_500;

/// Locked hash of the golden weapon's compiled surface.
pub const GOLDEN_WEAPON_GEOMETRY_FINGERPRINT: u64 = 0x8a6b_18ed_d4a2_b879;
/// Locked hash of the golden weapon's identity.
pub const GOLDEN_WEAPON_IDENTITY_FINGERPRINT: u64 = 0x084b_f386_500b_b0e4;
/// Locked hash of the whole reference encounter.
///
/// Every tick of [`GOLDEN_RUN_TICKS`] of [`GOLDEN_SCRIPT`]: both healths, both
/// action labels and elapsed counters, both positions and facings, and every
/// event. A retuned attack, a changed rule, a different adversary decision or a
/// moved arena all move it, which is the intent.
///
/// Re-locked twice during M6A, both times deliberately and for a stated reason.
///
/// **Old** `0x34fe041f95262079`, **new** `0xe858e3405cf21d0f`, **why**: an action
/// used to return to `Free` on the tick *after* its last rather than on its last,
/// which gave every swing, dodge and stagger one dead tick where a spent action
/// was still current. The fight was otherwise identical and the frozen-tick counts
/// fell from `80` to `70` because the dead tick is gone.
///
/// **Old** `0xe858e3405cf21d0f`, **new** `0x38e207fc7ead6c47`, **why**: the
/// adversary got its own body and the scripted player learned to read telegraphs.
/// `the_two_bodies_are_distinguishable_at_a_glance` rejected reusing M5's
/// `sturdy` fixture as the enemy — it is *shorter* than the player and shares its
/// absolute hip width and limb thickness — so the adversary is now a taller,
/// broader descriptor of its own. With a longer reach on the other side, a
/// scripted player that dodged in only one leg of ten lost every run, which is the
/// right outcome for that player and the wrong reference encounter.
///
/// **Old** `0x38e207fc7ead6c47`, **new** `0xc2fc91e36fbd0ec4`, **why**: the first
/// real close-up contact capture showed the adversary's hands as two slabs seven
/// voxels deep piled where the blades meet, so its hand depth came down from
/// `0.20` of body height to `0.15`. That moves its capsule radius from `0.8563`
/// to `0.8274` and therefore every separation in the fight. The outcome is
/// unchanged: six hits each, one defeat, the player down to six health.
///
/// **Old** `0xc2fc91e36fbd0ec4`, **new** `0x008e8bd64f62f267`, **why**: the same
/// capture, read a second time and with the debug volumes on, said the hit
/// itself was wrong. A hit was landing on the fourth of twelve active ticks with
/// the sword still raised over the shoulder and its tip in the air above the
/// adversary's head, because the volume being tested was M5's whole-body capsule
/// — `0.8274` wide on a body `0.50` through the chest, reaching from `0.52`
/// below the ground to above the crown. [`crate::hurt`] replaces it with a
/// torso-column volume refitted every tick, which moves the hit to the eighth
/// active tick, with the blade at chest height and through the body. Every
/// position in the fight follows from that. It is a better fight, too, and not
/// only a more legible one: the adversary now has to aim, so it whiffs seven of
/// thirteen swings instead of connecting almost every time, and the player
/// finishes on `24` health rather than `6`.
///
/// **Old** `0x008e8bd64f62f267`, **new** `0x64157522d2535658`, **why**: a swing
/// now turns the body onto the other one as it commits, inside a thirty-five
/// degree cone and the reach of the attack. Branch QA played the encounter
/// through a closed loop and could not win it: from positions the aim table
/// says connect, the player landed one swing in four, because facing follows
/// movement and a body that stands still to swing cannot track one that is
/// moving. The scripted fight barely notices — it already aimed, so the outcome
/// is the same five hits to four with the adversary defeated — but every
/// facing in it is a hair different and the trace follows.
pub const GOLDEN_ENCOUNTER_SIGNATURE: u64 = 0x6415_7522_d253_5658;

/// Locked hash of the found weapon's compiled surface.
///
/// **Old** none, **new** `0x0723_e9dd_aeff_d4d5`, **why**: first lock, M9. The
/// geometry of the found longblade, and palette-independent by construction —
/// the same voxels under a different scheme give the same value, which is why
/// this and the identity below are separate numbers.
pub const FOUND_WEAPON_GEOMETRY_FINGERPRINT: u64 = 0x0723_e9dd_aeff_d4d5;
/// Locked hash of the found weapon's identity.
///
/// **Old** none, **new** `0xa09f_cd9b_fa45_d87a`, **why**: first lock, M9. The
/// descriptor, the compiler version and the style version. It moves when the
/// found weapon's shape, seed or palette moves, and it must **not** move when
/// something historical does — which is the reason M9 did not touch
/// [`crate::weapon::COMBAT_STYLE_VERSION`] and gave the profile its own
/// [`FOUND_WEAPON_PROFILE_VERSION`] instead.
pub const FOUND_WEAPON_IDENTITY_FINGERPRINT: u64 = 0xa09f_cd9b_fa45_d87a;
/// Locked hash of the reference encounter fought with the found weapon.
///
/// **Old** none, **new** `0x735a_9961_f661_8e7d`, **why**: first lock, M9.
/// [`run_found_script`] exchanges once through the authoritative rule and then
/// traces [`GOLDEN_SCRIPT`] for [`GOLDEN_RUN_TICKS`] in **the same trace format
/// as [`GOLDEN_ENCOUNTER_SIGNATURE`]**. It is a different fixture doing
/// different work with a different weapon, so a different value is the expected
/// result and not a regression; the two are never compared for equality.
///
/// **Old** `0x735a_9961_f661_8e7d`, **new** `0xa5b7_8ee1_c589_a499`, **why**:
/// the owner's approved retune after the M9 revisit's owner playtest failed
/// (2026-09-23): the found attack's damage `32` -> `28` and windup `0.26` s ->
/// `0.30` s (`31` -> `36` ticks), and nothing else. The reference fight holds
/// the found weapon, so its trace follows the new timing and damage; the found
/// weapon's geometry and identity did not move.
pub const FOUND_ENCOUNTER_SIGNATURE: u64 = 0xa5b7_8ee1_c589_a499;

/// Every locked fixture value, for the probe to print in one place.
#[must_use]
pub fn locked_values() -> [(&'static str, u64); 7] {
    [
        ("golden weapon geometry", GOLDEN_WEAPON_GEOMETRY_FINGERPRINT),
        ("golden weapon identity", GOLDEN_WEAPON_IDENTITY_FINGERPRINT),
        ("golden encounter", GOLDEN_ENCOUNTER_SIGNATURE),
        (
            "combat initiative",
            crate::oracle::COMBAT_INITIATIVE_SIGNATURE,
        ),
        ("found weapon geometry", FOUND_WEAPON_GEOMETRY_FINGERPRINT),
        ("found weapon identity", FOUND_WEAPON_IDENTITY_FINGERPRINT),
        ("found encounter", FOUND_ENCOUNTER_SIGNATURE),
    ]
}

/// Computes every signature from scratch; the probe prints these next to the
/// locked ones so a mismatch is one line to read.
pub fn measured_values() -> Result<[(&'static str, u64); 7], EncounterError> {
    let mut compiler = crate::weapon::WeaponCompiler::new();
    let weapon = compiler.compile_descriptor(&weapon_descriptor())?;
    let found = compiler.compile_descriptor(&found_weapon_descriptor())?;
    let ground = golden_ground();
    let outcome = run_script(
        GOLDEN_SCRIPT,
        &golden_setup(),
        Some(&ground),
        GOLDEN_RUN_TICKS,
    )?;
    let found_outcome = run_found_script(GOLDEN_SCRIPT, Some(&ground), GOLDEN_RUN_TICKS)?;
    Ok([
        ("golden weapon geometry", weapon.geometry_fingerprint()),
        ("golden weapon identity", weapon.fingerprint()),
        ("golden encounter", outcome.signature),
        ("combat initiative", crate::oracle::initiative_signature()?),
        ("found weapon geometry", found.geometry_fingerprint()),
        ("found weapon identity", found.fingerprint()),
        ("found encounter", found_outcome.signature),
    ])
}

/// The moment kinds a reference run is required to reach.
///
/// Every one of them, which is the point: a script that cannot produce a dodge or
/// a defeat cannot be the evidence for either.
#[must_use]
pub fn required_moments() -> Vec<MomentKind> {
    NAMED_MOMENTS.iter().map(|moment| moment.kind).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ADVERSARY_OFFSET, ARENA_RADIUS, DodgeDirection, GOLDEN_ENCOUNTER_SIGNATURE,
        GOLDEN_RUN_TICKS, PLAYER_OFFSET, START_SEPARATION, adversary, adversary_descriptor, dodge,
        found_attack, found_weapon_descriptor, golden_ground, golden_setup, locked_values,
        measured_values, movement, player_attack, player_descriptor, probe_dodge, reach_of,
        run_script, sandbox_setup, setup, trace_signature, tuning, weapon_descriptor,
    };
    use crate::combatant::{SIDES, Side};
    use crate::encounter::{Encounter, EncounterError, WorldContact};
    use crate::reach::attack_envelope;
    use crate::script::{GOLDEN_SCRIPT, NAMED_MOMENTS};
    use crate::weapon::WeaponCompiler;
    use glam::Vec2;
    use std::collections::BTreeSet;
    use veldwake_character::material::luminance;
    use veldwake_character::{CharacterCompiler, skeleton::Side as BodySide};

    #[test]
    fn every_locked_value_still_matches_what_the_rules_produce() -> Result<(), EncounterError> {
        let measured = measured_values()?;
        let locked = locked_values();
        let mut mismatches = Vec::new();
        for (index, (name, value)) in measured.into_iter().enumerate() {
            let (locked_name, locked_value) = locked[index];
            assert_eq!(name, locked_name, "the fixture tables drifted apart");
            if value != locked_value {
                mismatches.push(format!(
                    "{name}: locked {locked_value:#018x} measured {value:#018x}"
                ));
            }
        }
        assert!(
            mismatches.is_empty(),
            "combat changed. Update the locked values deliberately, with old, new and why:\n{}",
            mismatches.join("\n")
        );
        Ok(())
    }

    #[test]
    fn the_reference_script_reaches_every_named_moment() -> Result<(), EncounterError> {
        let ground = golden_ground();
        let outcome = run_script(
            GOLDEN_SCRIPT,
            &golden_setup(),
            Some(&ground),
            GOLDEN_RUN_TICKS,
        )?;
        assert!(
            outcome.every_moment_reached(),
            "the evidence script cannot produce {:?}",
            outcome.missing_moments()
        );
        assert_eq!(outcome.signature, GOLDEN_ENCOUNTER_SIGNATURE);
        assert_eq!(outcome.counters.events_dropped, 0);
        // A fight rather than a demonstration: both sides swing, both connect,
        // both miss, and somebody falls.
        for side in SIDES {
            let index = side.index();
            assert!(
                outcome.counters.swings[index] >= 3,
                "{} only swung {} times",
                side.name(),
                outcome.counters.swings[index]
            );
            assert!(
                outcome.counters.hits[index] >= 2,
                "{} only landed {} hits",
                side.name(),
                outcome.counters.hits[index]
            );
            assert!(
                outcome.counters.whiffs[index] >= 1,
                "{} never missed, which means spacing does not matter",
                side.name()
            );
        }
        assert!(outcome.counters.dodges[0] >= 1, "the player never dodged");
        // The reference fight is a win, and a close one: the player's health gets
        // down to single figures on the way. Losing is demonstrated separately by
        // `a_passive_player_is_eventually_defeated`.
        assert_eq!(
            outcome.counters.defeats[1], 1,
            "the adversary must fall once"
        );
        assert_eq!(
            outcome.counters.defeats[0], 0,
            "and the player must survive it"
        );
        assert!(
            u32::from(outcome.min_health[0]) < outcome.counters.hits[1] * 18,
            "the player must actually be in danger"
        );
        assert_eq!(
            outcome.counters.resets, 1,
            "and the encounter must come back"
        );
        assert!(
            outcome.worst_overlap < 0.01,
            "the bodies overlapped by {:.4}",
            outcome.worst_overlap
        );
        Ok(())
    }

    #[test]
    fn a_reference_run_is_reproducible_and_a_changed_arena_is_not_the_same_fight() {
        let ground = golden_ground();
        let first = match run_script(GOLDEN_SCRIPT, &golden_setup(), Some(&ground), 1_200) {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error}"),
        };
        let second = match run_script(GOLDEN_SCRIPT, &golden_setup(), Some(&ground), 1_200) {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(first.signature, second.signature);
        // A different arena centre moves the trace, because positions are in it.
        let moved = match run_script(
            GOLDEN_SCRIPT,
            &setup(Vec2::new(-69.0, 49.0), ARENA_RADIUS),
            Some(&ground),
            1_200,
        ) {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error}"),
        };
        assert_ne!(first.signature, moved.signature);
        // But the fight itself is the same one.
        assert_eq!(first.counters.hits, moved.counters.hits);
        assert_eq!(first.counters.swings, moved.counters.swings);
        assert_ne!(trace_signature(b"a"), trace_signature(b"b"));
    }

    #[test]
    fn the_adversary_commits_from_inside_its_own_measured_reach() {
        // The number this test protects was wrong in the first draft: the strike
        // range was outside the adversary's own reach, so its swing only landed
        // because of the step-in. A telegraph has to be a promise.
        let ground = golden_ground();
        let encounter = match Encounter::new(&golden_setup(), Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        let spec = *encounter.attack_spec(Side::Adversary);
        let envelope = attack_envelope(
            encounter.character(Side::Adversary),
            encounter.weapon(),
            &spec,
            BodySide::Right,
        );
        let target_radius = encounter.combatant(Side::Player).capsule().radius;
        let connects = envelope.connects_out_to(target_radius);
        let strike = adversary().strike_range;
        assert!(
            strike < connects,
            "the adversary commits at {strike} but only reaches {connects}"
        );
        // And not so far inside that it has to walk through the player first.
        let closest = encounter.combatant(Side::Player).capsule().radius
            + encounter.combatant(Side::Adversary).capsule().radius;
        assert!(
            strike > closest,
            "it commits at {strike}, closer than the {closest} the bodies allow"
        );
    }

    #[test]
    fn the_player_can_reach_what_the_adversary_reaches_them_from() {
        let ground = golden_ground();
        let encounter = match Encounter::new(&golden_setup(), Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        let mut connects = [0.0_f32; SIDES.len()];
        for side in SIDES {
            let spec = *encounter.attack_spec(side);
            let envelope = attack_envelope(
                encounter.character(side),
                encounter.weapon(),
                &spec,
                BodySide::Right,
            );
            connects[side.index()] =
                envelope.connects_out_to(encounter.combatant(side.other()).capsule().radius);
            assert!(
                envelope.max_tip_reach > 1.5,
                "{} barely reaches past itself",
                side.name()
            );
            // The blade has to pass through the band a body occupies, or it can
            // only ever hit air.
            let capsule = encounter.combatant(side.other()).hurt_capsule();
            let top = capsule.axis.tip.y + capsule.radius;
            let bottom = capsule.axis.base.y - capsule.radius;
            assert!(
                envelope.active_height.0 < top && envelope.active_height.1 > bottom,
                "{}'s blade sweeps {:?}, outside the {bottom}..{top} a body occupies",
                side.name(),
                envelope.active_height
            );
        }
        // Neither body has a reach the other cannot answer.
        let ratio = connects[0] / connects[1];
        assert!(
            (0.8..=1.25).contains(&ratio),
            "the reaches are lopsided: {connects:?}"
        );
        // The fight fits in its arena with room to circle: the arena is wider
        // across than three swings are long.
        assert!(
            ARENA_RADIUS * 2.0 > connects[0] * 3.0,
            "an arena {} across is too small for a reach of {}",
            ARENA_RADIUS * 2.0,
            connects[0]
        );
    }

    #[test]
    fn a_dodge_can_actually_escape_the_swing_it_is_for() {
        // The arithmetic half of the dodge claim; `probe_dodge` above is the
        // behavioural half. Both are needed: reach alone says the dodge is
        // comfortable, and the blade's height is what decides when it is not.
        let ground = golden_ground();
        let setup = golden_setup();
        let encounter = match Encounter::new(&setup, Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        let spec = *encounter.attack_spec(Side::Adversary);
        let envelope = attack_envelope(
            encounter.character(Side::Adversary),
            encounter.weapon(),
            &spec,
            BodySide::Right,
        );
        let connects = envelope.connects_out_to(encounter.combatant(Side::Player).capsule().radius);
        let slack = connects - adversary().strike_range + adversary_step_in();
        assert!(
            dodge().distance > slack,
            "a dodge of {} cannot cover the {slack} between the commit and the reach",
            dodge().distance
        );
        // The dodge also has to be over before the next swing can land.
        let dodge_spec = match dodge().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert!(
            dodge_spec.duration() < spec.total(),
            "a dodge that outlasts a whole swing is a dodge that cannot be punished"
        );
        assert!(
            dodge_spec.speed() > movement().speed,
            "a dodge slower than a walk is not a dodge"
        );
    }

    fn adversary_step_in() -> f32 {
        match super::adversary_attack().compile() {
            Ok(spec) => spec.step_in(),
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn dodging_early_escapes_and_dodging_late_does_not() {
        let ground = golden_ground();
        let setup = golden_setup();
        for direction in [DodgeDirection::Away, DodgeDirection::Lateral] {
            let early = match probe_dodge(&setup, Some(&ground), 6, direction, 2_400) {
                Ok(probe) => probe,
                Err(error) => panic!("{error}"),
            };
            assert!(
                early.started && !early.hit,
                "{} early failed",
                direction.name()
            );
            assert!(early.closest > 0.0);
            assert!(!direction.name().is_empty());
        }
    }

    #[test]
    fn the_two_bodies_are_distinguishable_at_a_glance() {
        // A fight between two bodies a viewer cannot tell apart is not readable,
        // whatever the timings do.
        let mut compiler = CharacterCompiler::new();
        let player = match compiler.compile_descriptor(&player_descriptor()) {
            Ok(character) => character,
            Err(error) => panic!("{error}"),
        };
        let adversary = match compiler.compile_descriptor(&adversary_descriptor()) {
            Ok(character) => character,
            Err(error) => panic!("{error}"),
        };
        assert_ne!(
            player.fingerprint(),
            adversary.fingerprint(),
            "the two bodies must be two bodies"
        );
        // Different silhouette: height and width both differ by a real margin.
        let player_body = player.body();
        let adversary_body = adversary.body();
        assert!(
            (player_body.height - adversary_body.height).abs() >= 2,
            "the two are {} and {} voxels tall",
            player_body.height,
            adversary_body.height
        );
        // Bigger in every measure a silhouette is made of, not merely in one.
        // The ratios are close — both are heroic proportions — so the reading
        // comes from absolute size, which is what a viewer compares.
        for (name, player, adversary) in [
            ("height", player_body.height, adversary_body.height),
            (
                "shoulder span",
                player_body.shoulder_span,
                adversary_body.shoulder_span,
            ),
            ("hip width", player_body.hip_width, adversary_body.hip_width),
            (
                "limb thickness",
                player_body.limb_thickness,
                adversary_body.limb_thickness,
            ),
        ] {
            assert!(
                adversary > player,
                "the adversary's {name} is {adversary} against the player's {player}"
            );
        }
        assert!(
            adversary_body.height - player_body.height >= 4,
            "a body only {} voxels taller does not loom",
            adversary_body.height - player_body.height
        );
        // And a different palette. **Not by value on the tunic**, and that is a
        // finding rather than an oversight: all three of M5's garment schemes
        // share one luminance ladder on purpose, so their primary tunics sit
        // within a ten-thousandth of each other and no choice of scheme can
        // separate two characters by value there. What separates them is hue on
        // the garment and value on the skin.
        let tunic = veldwake_character::CharacterMaterial::TunicPrimary;
        let player_tunic = player.palette().albedo(tunic);
        let adversary_tunic = adversary.palette().albedo(tunic);
        assert_ne!(
            dominant_channel(player_tunic),
            dominant_channel(adversary_tunic),
            "the two tunics are led by the same channel, so they are the same colour"
        );
        let distance = ((player_tunic[0] - adversary_tunic[0]).powi(2)
            + (player_tunic[1] - adversary_tunic[1]).powi(2)
            + (player_tunic[2] - adversary_tunic[2]).powi(2))
        .sqrt();
        assert!(
            distance >= 0.15,
            "the two tunics are only {distance:.4} apart in linear RGB"
        );
        let skin = veldwake_character::CharacterMaterial::Skin;
        let skin_separation = (luminance(player.palette().albedo(skin))
            - luminance(adversary.palette().albedo(skin)))
        .abs();
        assert!(
            skin_separation >= 0.08,
            "the two skins are only {skin_separation:.4} apart in value"
        );
    }

    /// Which channel leads a colour, which is the cheapest statement of hue that
    /// does not need a colour-space conversion.
    fn dominant_channel(albedo: [f32; 3]) -> usize {
        let mut best = 0;
        for index in 1..3 {
            if albedo[index] > albedo[best] {
                best = index;
            }
        }
        best
    }

    #[test]
    fn the_starting_positions_are_apart_and_inside_the_arena() {
        let arena_radius = ARENA_RADIUS;
        assert!((PLAYER_OFFSET - ADVERSARY_OFFSET).length() - START_SEPARATION < 1.0e-6);
        for offset in [PLAYER_OFFSET, ADVERSARY_OFFSET] {
            assert!(
                offset.length() < arena_radius,
                "a body starts outside its own arena"
            );
        }
        // Far enough apart that a faceoff is a faceoff rather than a scuffle.
        assert!(START_SEPARATION > adversary().strike_range * 2.0);
    }

    #[test]
    fn the_sandbox_adversary_never_wakes_up() {
        let ground = golden_ground();
        let mut encounter = match Encounter::new(&sandbox_setup(), Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        encounter.arm();
        for _ in 0..2_000 {
            let _ = encounter.step(
                crate::combatant::Intent::idle(),
                WorldContact::ground_only(&ground),
            );
        }
        assert_eq!(
            encounter.counters().swings[Side::Adversary.index()],
            0,
            "the target dummy must not fight back"
        );
        assert_eq!(
            encounter.combatant(Side::Player).health().current(),
            encounter.combatant(Side::Player).health().max()
        );
    }

    #[test]
    fn the_whole_tuning_compiles_and_the_telegraph_is_the_longer_one() {
        let compiled = match tuning().compile() {
            Ok(tuning) => tuning,
            Err(error) => panic!("{error}"),
        };
        assert!(
            compiled.adversary_attack().windup() > compiled.player_attack().windup() * 2,
            "the adversary's telegraph is what makes the fight readable"
        );
        // The player's whole swing is shorter than the adversary's telegraph, so
        // a read is always punishable.
        assert!(
            compiled.player_attack().total() < compiled.adversary_attack().active_start() * 2,
            "there is no window to punish a telegraph in"
        );
        assert!(compiled.defeat_hold() > compiled.player_attack().total());
        // The arena is placement, not tuning, and lives on the setup now.
        let Some(arena) = golden_setup().arena else {
            panic!("the golden fixture has an arena");
        };
        assert_eq!(arena.radius(), ARENA_RADIUS);
        assert_eq!(arena.centre(), Vec2::ZERO);
        // Health is a whole number of hits, which is what makes a fight countable.
        let hits_to_fall = compiled.adversary_health() / compiled.player_attack().damage();
        assert!(
            (3..=6).contains(&hits_to_fall),
            "the adversary takes {hits_to_fall} hits to fall"
        );
    }

    #[test]
    fn the_weapon_fixture_is_the_one_the_encounter_uses() {
        let ground = golden_ground();
        let encounter = match Encounter::new(&golden_setup(), Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        let standalone = match WeaponCompiler::new().compile_descriptor(&weapon_descriptor()) {
            Ok(weapon) => weapon,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(encounter.weapon().fingerprint(), standalone.fingerprint());
        assert_eq!(
            encounter.weapon().geometry_fingerprint(),
            standalone.geometry_fingerprint()
        );
        // Both sides carry the same one weapon, which is what "one weapon" means.
        assert!(reach_of(&encounter) > 0.0);
    }

    #[test]
    fn every_named_moment_is_named_once_and_says_what_it_is_for() {
        let names: BTreeSet<&str> = NAMED_MOMENTS.iter().map(|moment| moment.name).collect();
        assert_eq!(names.len(), NAMED_MOMENTS.len(), "a moment name repeats");
        let kinds: BTreeSet<crate::script::MomentKind> =
            NAMED_MOMENTS.iter().map(|moment| moment.kind).collect();
        assert_eq!(kinds.len(), NAMED_MOMENTS.len(), "a moment kind repeats");
        assert!(
            NAMED_MOMENTS.len() >= 10,
            "too few moments for real evidence"
        );
        for moment in NAMED_MOMENTS {
            assert!(!moment.intent.is_empty(), "{} has no intent", moment.name);
            match crate::script::moment(moment.name) {
                Some(found) => assert_eq!(found.kind, moment.kind),
                None => panic!("{} cannot be looked up", moment.name),
            }
        }
        assert!(crate::script::moment("not-a-moment").is_none());
        assert_eq!(super::required_moments().len(), NAMED_MOMENTS.len());
    }

    #[test]
    fn the_players_attack_is_the_one_the_fixture_declares() {
        let authored = player_attack();
        let compiled = match authored.compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(compiled.damage(), authored.damage);
        assert!(
            compiled.hitstop() > 0,
            "a hit with no freeze reads as a nudge"
        );
        assert!(compiled.knockback() > 0.0);
        assert!(compiled.stagger() > compiled.hitstop());
    }

    // -----------------------------------------------------------------------
    // M9 — the found weapon as a fixture
    // -----------------------------------------------------------------------

    #[test]
    fn the_found_weapon_compiles_and_matches_its_locks() {
        let mut compiler = WeaponCompiler::new();
        let Ok(found) = compiler.compile_descriptor(&found_weapon_descriptor()) else {
            panic!("the found weapon must compile");
        };
        assert_eq!(
            found.geometry_fingerprint(),
            super::FOUND_WEAPON_GEOMETRY_FINGERPRINT,
            "the found weapon's geometry moved"
        );
        assert_eq!(
            found.fingerprint(),
            super::FOUND_WEAPON_IDENTITY_FINGERPRINT,
            "the found weapon's identity moved"
        );
    }

    #[test]
    fn adding_the_found_weapon_left_the_historical_weapon_untouched() {
        // The M9 profile is additive under the same combat grammar. The shared
        // style version is hashed into every identity, so bumping it would have
        // changed the M6 weapon while its descriptor, geometry and behaviour
        // were untouched. It stays at one, and this is the assertion that says
        // so out loud.
        assert_eq!(
            crate::weapon::COMBAT_STYLE_VERSION,
            1,
            "M9 must not bump the shared style version"
        );
        assert_eq!(super::FOUND_WEAPON_PROFILE_VERSION, 1);
        let mut compiler = WeaponCompiler::new();
        let Ok(original) = compiler.compile_descriptor(&weapon_descriptor()) else {
            panic!("the original weapon must compile");
        };
        assert_eq!(
            original.fingerprint(),
            super::GOLDEN_WEAPON_IDENTITY_FINGERPRINT
        );
        assert_eq!(
            original.geometry_fingerprint(),
            super::GOLDEN_WEAPON_GEOMETRY_FINGERPRINT
        );
    }

    #[test]
    fn the_found_swing_compiles_to_the_longer_commitment_the_profile_claims() {
        let Ok(found) = found_attack().compile() else {
            panic!("the found attack must compile");
        };
        let Ok(original) = player_attack().compile() else {
            panic!("the player attack must compile");
        };
        // The owner's retune after the revisit's playtest: windup `31` -> `36`
        // ticks, damage `32` -> `28`; active and recovery unchanged.
        assert_eq!(found.windup(), 36);
        assert_eq!(found.active(), 14);
        assert_eq!(found.recovery(), 58);
        assert_eq!(found.total(), 108);
        assert_eq!(found.damage(), 28);
        assert!(found.total() > original.total());
        // Four swings to fell a ninety-six health adversary with either
        // weapon, and each found swing still hits harder.
        let swings = |spec: &crate::spec::AttackSpec| 96_u16.div_ceil(spec.damage());
        assert_eq!(swings(&found), 4);
        assert_eq!(swings(&original), 4);
        assert!(found.damage() > original.damage());
    }

    #[test]
    fn the_two_weapons_are_distinguishable_without_a_stats_panel() {
        let mut compiler = WeaponCompiler::new();
        let Ok(original) = compiler.compile_descriptor(&weapon_descriptor()) else {
            panic!("the original weapon must compile");
        };
        let Ok(found) = compiler.compile_descriptor(&found_weapon_descriptor()) else {
            panic!("the found weapon must compile");
        };
        // Length, mass and palette all move, and the silhouette claim is the
        // blade rather than the total, because the blade is what a viewer reads.
        assert!(
            found.blade().world_length() > original.blade().world_length() * 1.35,
            "the found blade is not visibly longer: {} against {}",
            found.blade().world_length(),
            original.blade().world_length()
        );
        assert!(
            found.solid_voxels() > original.solid_voxels() * 3 / 2,
            "the found weapon is not visibly heavier: {} against {}",
            found.solid_voxels(),
            original.solid_voxels()
        );
        assert_ne!(
            found_weapon_descriptor().scheme,
            weapon_descriptor().scheme,
            "the two weapons share a palette"
        );
    }

    #[test]
    fn the_found_weapon_does_not_hang_through_the_ground_when_carried() {
        // The grip pitch is `1.10` rather than the original's `0.90` because of
        // exactly this measurement: at `0.90` a twenty-voxel blade's tip sits
        // below the terrain in the carry pose.
        let mut compiler = veldwake_character::CharacterCompiler::new();
        let Ok(body) = compiler.compile_descriptor(&player_descriptor()) else {
            panic!("the player must compile");
        };
        let mut weapons = WeaponCompiler::new();
        let mut tip_height = |descriptor: &crate::weapon::WeaponDescriptor| -> f32 {
            let Ok(weapon) = weapons.compile_descriptor(descriptor) else {
                panic!("the weapon must compile");
            };
            let state = veldwake_character::CharacterState::default();
            let posed = veldwake_character::pose_with(&body, &state, None, None);
            weapon
                .blade_world(
                    posed.world_matrix(),
                    posed.bone_world()[veldwake_character::BoneId::HandR.index()],
                )
                .tip
                .y
        };
        let original = tip_height(&weapon_descriptor());
        let found = tip_height(&found_weapon_descriptor());
        assert!(found > 0.0, "the found blade drags: tip at {found}");
        assert!(
            found >= original,
            "the found blade clears worse than the original: {found} against {original}"
        );
    }

    #[test]
    fn the_found_reference_run_is_locked_and_is_not_the_m6_run() {
        let ground = golden_ground();
        let Ok(found) = super::run_found_script(GOLDEN_SCRIPT, Some(&ground), GOLDEN_RUN_TICKS)
        else {
            panic!("the found reference run must complete");
        };
        assert_eq!(
            found.signature,
            super::FOUND_ENCOUNTER_SIGNATURE,
            "the found reference encounter moved"
        );
        let Ok(original) = run_script(
            GOLDEN_SCRIPT,
            &golden_setup(),
            Some(&ground),
            GOLDEN_RUN_TICKS,
        ) else {
            panic!("the M6 reference run must complete");
        };
        assert_eq!(original.signature, GOLDEN_ENCOUNTER_SIGNATURE);
        assert_ne!(
            found.signature, original.signature,
            "two different fights produced one signature"
        );
        assert_eq!(found.counters.events_dropped, 0);
        // The exchange really happened, and exactly once.
        assert_eq!(found.counters.interacts[Side::Player.index()], 1);
    }
}
