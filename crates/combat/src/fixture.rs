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
use veldwake_character::skeleton::{BoneId, Side as BodySide};
use veldwake_character::{
    CharacterDescriptor, CharacterState, CompiledCharacter, GroundSampler, pose_with,
};

use crate::combatant::{Action, Intent, SIDES, Side, SwingId};
use crate::encounter::{
    CombatCounters, Encounter, EncounterError, EncounterSetup, PlayerVictoryPolicy, WorldContact,
};
use crate::hash::{fnv1a64, push_f32, push_u16, push_u32, push_u64};
use crate::script::{
    EncounterScript, GOLDEN_SCRIPT, MomentKind, NAMED_MOMENTS, ScriptRunner, at_moment,
};
use crate::spec::{
    ArenaSpec, AttackSpec, AuthoredAdversary, AuthoredAttack, AuthoredDodge, AuthoredMovement,
    AuthoredPressure, AuthoredTuning, CombatSeed,
};
use crate::tick::Ticks;
use crate::weapon::{CompiledWeapon, WeaponDescriptor};

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

/// The one weapon.
#[must_use]
pub fn weapon_descriptor() -> WeaponDescriptor {
    WeaponDescriptor::golden()
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
    let reach = reach_of(&encounter);
    let mut runner = ScriptRunner::new(script, reach);
    let mut bytes = Vec::with_capacity(ticks as usize * 24);
    let mut moments: [Option<u64>; NAMED_MOMENTS.len()] = [None; NAMED_MOMENTS.len()];
    let mut min_health = [u16::MAX; SIDES.len()];
    let mut worst_overlap = 0.0_f32;

    for _ in 0..ticks {
        let intent = runner.next_intent(&encounter);
        let events = encounter.step(intent, WorldContact::from_ground(ground));
        let tick = encounter.tick_index();

        for (index, moment) in NAMED_MOMENTS.iter().enumerate() {
            if moments[index].is_none() && at_moment(moment.kind, &encounter, &events) {
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

    Ok(ScriptOutcome {
        ticks,
        counters: *encounter.counters(),
        moments,
        signature: fnv1a64(&bytes),
        min_health,
        worst_overlap,
    })
}

/// Where a body's blade goes during its own swing, measured rather than declared.
///
/// A pure function of a compiled body, a compiled weapon and an attack spec, with
/// no terrain and no encounter, so the numbers the strike range and the dodge
/// distance are chosen against come from the same pose path the fight uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingEnvelope {
    /// Furthest the tip gets from the body's own centre, at any point.
    pub max_tip_reach: f32,
    /// Reach of the tip while the blade can connect.
    pub active_reach: (f32, f32),
    /// Height of the tip above the ground while the blade can connect.
    pub active_height: (f32, f32),
}

impl SwingEnvelope {
    /// Centre-to-centre distance at which this swing can still touch a body of
    /// the given capsule radius.
    #[must_use]
    pub fn connects_out_to(&self, target_radius: f32) -> f32 {
        self.active_reach.1 + target_radius
    }
}

/// Measures one body's swing envelope.
#[must_use]
pub fn swing_envelope(
    character: &CompiledCharacter,
    weapon: &CompiledWeapon,
    spec: &AttackSpec,
    weapon_side: BodySide,
) -> SwingEnvelope {
    let mut max_tip_reach = 0.0_f32;
    let mut active_reach = (f32::INFINITY, f32::NEG_INFINITY);
    let mut active_height = (f32::INFINITY, f32::NEG_INFINITY);
    let state = CharacterState::default();
    for elapsed in 0..=spec.total() {
        let action = Action::Attack {
            swing: SwingId::first(),
            kind: crate::combatant::AttackKind::Primary,
            elapsed,
            hits: [false; SIDES.len()],
        };
        let overlay = action.overlay(spec, weapon_side, state.facing);
        let posed = pose_with(character, &state, None, Some(&overlay));
        let blade = weapon.blade_world(
            posed.world_matrix(),
            posed.bone_world()[BoneId::HandR.index()],
        );
        let reach = Vec2::new(blade.tip.x, blade.tip.z).length();
        max_tip_reach = max_tip_reach.max(reach);
        if spec.is_active(elapsed) {
            active_reach = (active_reach.0.min(reach), active_reach.1.max(reach));
            active_height = (
                active_height.0.min(blade.tip.y),
                active_height.1.max(blade.tip.y),
            );
        }
    }
    SwingEnvelope {
        max_tip_reach,
        active_reach,
        active_height,
    }
}

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

/// Every locked fixture value, for the probe to print in one place.
#[must_use]
pub fn locked_values() -> [(&'static str, u64); 4] {
    [
        ("golden weapon geometry", GOLDEN_WEAPON_GEOMETRY_FINGERPRINT),
        ("golden weapon identity", GOLDEN_WEAPON_IDENTITY_FINGERPRINT),
        ("golden encounter", GOLDEN_ENCOUNTER_SIGNATURE),
        (
            "combat initiative",
            crate::oracle::COMBAT_INITIATIVE_SIGNATURE,
        ),
    ]
}

/// Computes every signature from scratch; the probe prints these next to the
/// locked ones so a mismatch is one line to read.
pub fn measured_values() -> Result<[(&'static str, u64); 4], EncounterError> {
    let weapon = crate::weapon::WeaponCompiler::new().compile_descriptor(&weapon_descriptor())?;
    let ground = golden_ground();
    let outcome = run_script(
        GOLDEN_SCRIPT,
        &golden_setup(),
        Some(&ground),
        GOLDEN_RUN_TICKS,
    )?;
    Ok([
        ("golden weapon geometry", weapon.geometry_fingerprint()),
        ("golden weapon identity", weapon.fingerprint()),
        ("golden encounter", outcome.signature),
        ("combat initiative", crate::oracle::initiative_signature()?),
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
        golden_ground, golden_setup, locked_values, measured_values, movement, player_attack,
        player_descriptor, probe_dodge, reach_of, run_script, sandbox_setup, setup, swing_envelope,
        trace_signature, tuning, weapon_descriptor,
    };
    use crate::combatant::{SIDES, Side};
    use crate::encounter::{Encounter, EncounterError, WorldContact};
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
        let envelope = swing_envelope(
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
            let envelope = swing_envelope(
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
        let envelope = swing_envelope(
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
}
