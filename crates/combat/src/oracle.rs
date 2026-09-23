//! Behavioural oracles for the encounter: small scripted players whose results
//! are evidence about the fight rather than about one rule, and the probes that
//! derive and prove the pressure lunge's selection band.
//!
//! **Why this exists.** The M9 owner playtest found that the natural way to
//! fight the M6 adversary — walk up to it, face it, and attack whenever the
//! body can — wins every time, and a later investigation found that no single
//! adversary capability changes that
//! (`docs/planning/COMBAT_INITIATIVE.md`). The policies here are instruments,
//! not opponents, and each is deliberately simple:
//!
//! | policy | what it stands for |
//! | --- | --- |
//! | [`OraclePolicy::OwnerSpam`] | the owner's strategy: approach, face, attack whenever in range, never dodge |
//! | [`OraclePolicy::SpamRead`] | the same, having learned one thing: step off a lunge's line |
//! | [`OraclePolicy::Read`] | observe, step off the lunge's line, punish the opening; answer the primary the historical way |
//!
//! All three read only what a person can see — positions, the other body's
//! action and kind and how long it has been running — and a policy that reads
//! an adversary's action does so only after an authored observation lag.
//! None of them reads an input, a future hit or a future position.

use glam::Vec2;

use crate::combatant::{Action, AttackKind, Intent, Side};
use crate::encounter::{
    AIM_ASSIST_RANGE, CombatCounters, Encounter, EncounterError, EncounterSetup,
    PlayerVictoryPolicy, WorldContact,
};
use crate::event::CombatEvent;
use crate::fixture;
use crate::spec::CombatSeed;
use crate::tick::Ticks;

/// The seeds every oracle fight is run under.
///
/// The golden seed plus five others, the same six the M9 investigation and
/// Combat Pressure used, so a result here can be read against those records.
pub const ORACLE_SEEDS: [CombatSeed; 6] = [
    CombatSeed::GOLDEN,
    CombatSeed(0x1111_2222_3333_4444),
    CombatSeed(0x9e37_79b9_7f4a_7c15),
    CombatSeed(0xdead_beef_0bad_f00d),
    CombatSeed(0x0123_4567_89ab_cdef),
    CombatSeed(0x5555_aaaa_5555_aaaa),
];

/// How far the owner-spam player misjudges its own reach under each seed, in
/// world units. A person does not swing at exactly the distance a table says
/// connects; `±0.25` is the approximation the M9 investigation used.
pub const ORACLE_MISJUDGEMENT: [f32; 6] = [-0.25, -0.15, 0.0, 0.10, 0.20, 0.25];

/// The reaction delays the read policies are run at, in ticks: `150`, `200`,
/// `267` and `400` ms. The lowest is a fast person and the highest a slow one;
/// none of them is the `18`–`20` ticks Combat Pressure's reactive dodge needed.
pub const READ_LAGS: [Ticks; 4] = [18, 24, 32, 48];

/// Longest a fight is allowed to run, in ticks: sixty seconds. A fight that is
/// still going at the end is a stalemate.
pub const ORACLE_FIGHT_TICKS: u32 = 7_200;

/// Distance at which the read policies stop walking in and wait, in world
/// units: just outside both bodies' reach, where the M6 intentional player
/// held.
pub const READ_HOLD_DISTANCE: f32 = 3.0;

/// A scripted player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OraclePolicy {
    /// The M9 owner's natural strategy: approach the adversary, face it, and
    /// attack whenever the body can act and the adversary looks close enough.
    /// Never dodges and never waits for anything.
    OwnerSpam {
        /// Added to the distance at which it decides a swing is worth
        /// starting.
        misjudgement: f32,
    },
    /// Owner-spam that has learned to recognise the lunge: once a pressure
    /// windup has been visible for `lag` ticks it dodges sideways off the
    /// line. Otherwise it is owner-spam exactly — it still chases, still
    /// swings whenever in range, and still ignores the primary.
    SpamRead {
        misjudgement: f32,
        lag: Ticks,
        /// `+1` or `-1`: which side of the line it steps to.
        side: f32,
    },
    /// The fight this capability is built to ask for: hold just outside
    /// reach; once a lunge has been visible for `lag` ticks, leave its line
    /// sideways; punish a recovery once it has been visible as long; punish a
    /// stagger; answer the primary the historical way, by dodging away from a
    /// telegraph seen early enough; never swing into a ready opponent.
    Read {
        lag: Ticks,
        /// Leave the line by walking only, never dodging.
        walk_only: bool,
        side: f32,
    },
}

impl OraclePolicy {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OwnerSpam { .. } => "owner-spam",
            Self::SpamRead { .. } => "spam-read",
            Self::Read {
                walk_only: false, ..
            } => "read-dodge",
            Self::Read {
                walk_only: true, ..
            } => "read-walk",
        }
    }

    /// This policy's intent for the next tick, from what the encounter shows.
    #[must_use]
    pub fn intent(self, encounter: &Encounter) -> Intent {
        let me = encounter.combatant(Side::Player);
        let foe = encounter.combatant(Side::Adversary);
        let offset = foe.position() - me.position();
        let distance = offset.length();
        let toward = if distance > 1.0e-4 {
            offset / distance
        } else {
            Vec2::ZERO
        };
        let spam = |misjudgement: f32| {
            let swing = me.can_act() && distance <= AIM_ASSIST_RANGE + misjudgement;
            Intent::player(toward, swing, false)
        };
        // Sideways off the line between the two bodies, which is the lunge's
        // line because the lunge only commits when it is facing along it.
        let across = |side: f32| Vec2::new(-toward.y, toward.x) * side;
        let can_dodge = me.can_act() && me.dodge_cooldown() == 0;
        let spec = encounter.attack_spec(Side::Adversary);
        match self {
            Self::OwnerSpam { misjudgement } => spam(misjudgement),
            Self::SpamRead {
                misjudgement,
                lag,
                side,
            } => match *foe.action() {
                Action::Attack {
                    kind: AttackKind::Pressure,
                    elapsed,
                    ..
                } if elapsed < spec.active_end() && elapsed >= lag => {
                    Intent::player(across(side), false, can_dodge)
                }
                _ => spam(misjudgement),
            },
            Self::Read {
                lag,
                walk_only,
                side,
            } => {
                let hold = || {
                    let walk = if distance > READ_HOLD_DISTANCE {
                        toward
                    } else {
                        Vec2::ZERO
                    };
                    Intent::player(walk, false, false)
                };
                let punish = |ready: bool| {
                    Intent::player(toward, ready && me.can_act() && distance <= 2.8, false)
                };
                match *foe.action() {
                    Action::Attack {
                        kind: AttackKind::Pressure,
                        elapsed,
                        ..
                    } if elapsed < spec.active_end() => {
                        if elapsed >= lag {
                            Intent::player(across(side), false, !walk_only && can_dodge)
                        } else {
                            hold()
                        }
                    }
                    Action::Attack {
                        kind: AttackKind::Primary,
                        elapsed,
                        ..
                    } if elapsed < spec.active_start() => {
                        // The historical answer: dodge away from a telegraph
                        // seen in time, from inside its reach.
                        let dodge = elapsed >= lag && distance < 3.3;
                        let away = if dodge { -toward } else { Vec2::ZERO };
                        Intent::player(away, false, dodge)
                    }
                    Action::Attack { elapsed, .. } => punish(elapsed >= spec.active_end() + lag),
                    Action::Stagger { .. } => punish(true),
                    Action::Dodge { .. } | Action::Defeated { .. } => Intent::idle(),
                    Action::Free => hold(),
                }
            }
        }
    }
}

/// What one oracle fight did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FightReport {
    /// Who was still standing when the first body fell, if one did. `None` is a
    /// stalemate: sixty seconds and nobody down.
    pub winner: Option<Side>,
    pub ticks: u32,
    pub counters: CombatCounters,
    pub player_health: u16,
    pub adversary_health: u16,
    /// Longest run of ticks in which nobody was hit.
    pub longest_quiet: u32,
    /// Player hits that landed while the adversary was recovering from a lunge
    /// that missed: the opening, taken.
    pub punishes: u32,
    /// Longest run of ticks in which the adversary was trying to approach from
    /// outside striking range and did not move at all — its body against
    /// something it cannot walk through (KI-038).
    pub longest_stuck_approach: u32,
    /// The furthest either body moved in one tick, in world units: the
    /// no-teleport evidence. A knockback is the largest thing that happens in
    /// one tick, and the fastest continuous motion is a dodge or a lunge at
    /// about `0.06` a tick.
    pub max_step: f32,
}

impl FightReport {
    /// Swings the adversary made that were its historical primary.
    #[must_use]
    pub const fn adversary_primary_swings(&self) -> u32 {
        self.counters.swings[1].saturating_sub(self.counters.pressure_swings)
    }

    /// Whether this fight has to be discarded as KI-038 rather than counted as
    /// combat: the adversary spent more than a second walking into something.
    #[must_use]
    pub const fn stuck(&self) -> bool {
        self.longest_stuck_approach > STUCK_APPROACH_TICKS
    }
}

/// Ticks of a frozen approach after which a fight is KI-038 evidence rather than
/// combat evidence: one second.
pub const STUCK_APPROACH_TICKS: u32 = 120;

/// The reference fight for an oracle: the M6 bodies, weapon and tuning on open
/// ground under one seed, ending at the first defeat.
///
/// No arena — a disc would decide how far a body can retreat, and that is part
/// of what is being measured — and [`PlayerVictoryPolicy::Remain`], so a win is
/// the end of the run rather than the start of another round. The historical
/// encounter, with no pressure lunge.
#[must_use]
pub fn oracle_setup(seed: CombatSeed) -> EncounterSetup {
    let mut setup = fixture::golden_setup();
    setup.arena = None;
    setup.player_victory = PlayerVictoryPolicy::Remain;
    setup.tuning.seed = seed;
    setup
}

/// The same fight with combat initiative: the historical setup plus the
/// pressure lunge.
#[must_use]
pub fn initiative_oracle_setup(seed: CombatSeed) -> EncounterSetup {
    let mut setup = oracle_setup(seed);
    setup.tuning = fixture::initiative_tuning();
    setup.tuning.seed = seed;
    setup
}

/// Runs one policy against one setup until a body falls or the time runs out.
pub fn run_fight(
    setup: &EncounterSetup,
    world: WorldContact<'_>,
    policy: OraclePolicy,
    max_ticks: u32,
) -> Result<FightReport, EncounterError> {
    let mut encounter = Encounter::new(setup, world.ground())?;
    encounter.arm();
    let mut winner = None;
    let mut quiet = 0_u32;
    let mut longest_quiet = 0_u32;
    let mut punishes = 0_u32;
    let mut stuck = 0_u32;
    let mut longest_stuck_approach = 0_u32;
    let mut max_step = 0.0_f32;
    let mut ticks = 0_u32;
    while ticks < max_ticks && winner.is_none() {
        let spec = *encounter.attack_spec(Side::Adversary);
        let opening = matches!(
            *encounter.combatant(Side::Adversary).action(),
            Action::Attack { kind: AttackKind::Pressure, elapsed, hits, .. }
                if elapsed >= spec.active_end() && !hits[Side::Player.index()]
        );
        let before = encounter.combatant(Side::Adversary).position();
        let player_before = encounter.combatant(Side::Player).position();
        let intent = policy.intent(&encounter);
        let events = encounter.step(intent, world);
        ticks += 1;
        let mut hit = false;
        for event in events.iter() {
            match event {
                CombatEvent::Hit { attacker, .. } => {
                    hit = true;
                    if attacker == Side::Player && opening {
                        punishes += 1;
                    }
                }
                CombatEvent::Defeated { side } => winner = Some(side.other()),
                _ => {}
            }
        }
        quiet = if hit { 0 } else { quiet + 1 };
        longest_quiet = longest_quiet.max(quiet);
        max_step = max_step
            .max((encounter.combatant(Side::Player).position() - player_before).length())
            .max((encounter.combatant(Side::Adversary).position() - before).length());
        let adversary = encounter.combatant(Side::Adversary);
        let frozen = (adversary.position() - before).length() < 1.0e-4
            && adversary.action().is_free()
            && encounter.brain().state() == crate::adversary::AdversaryState::Approach
            && encounter.separation_distance() > encounter.tuning().adversary().strike_range();
        stuck = if frozen { stuck + 1 } else { 0 };
        longest_stuck_approach = longest_stuck_approach.max(stuck);
    }
    Ok(FightReport {
        winner,
        ticks,
        counters: *encounter.counters(),
        player_health: encounter.combatant(Side::Player).health().current(),
        adversary_health: encounter.combatant(Side::Adversary).health().current(),
        longest_quiet,
        punishes,
        longest_stuck_approach,
        max_step,
    })
}

/// How the probed player answers a lunge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LungeResponse {
    /// Stands still and does nothing: what the lunge must hit, everywhere in
    /// its band, or the band is a bluff.
    Nothing,
    /// Walks sideways off the line from `reaction` ticks into the windup, to
    /// the adversary's weapon side (`side > 0`) or its free-hand side.
    Walk { reaction: Ticks, side: i8 },
    /// Dodges sideways off the line at `reaction` ticks into the windup.
    Dodge { reaction: Ticks, side: i8 },
    /// The owner's strategy: runs straight at the adversary and swings the
    /// moment the aim-assist distance plus `misjudgement` hundredths of a unit
    /// says it is in range.
    Charge { misjudgement_centi: i32 },
    /// Swings at once, where it stands, on the first tick of the windup.
    SwingAtOnce,
    /// Walks straight at the adversary, and `reaction` ticks into the windup
    /// stops and leaves the line sideways — walking, or dodging if `dodge`.
    /// The person who did not see it coming until it had started.
    AdvanceThenLeave {
        reaction: Ticks,
        side: i8,
        dodge: bool,
    },
}

/// What one probed lunge did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LungeProbe {
    /// The distance the lunge was committed from.
    pub committed_at: f32,
    /// The lunge connected with the player.
    pub lunge_hit: bool,
    /// The player's blade connected with the adversary.
    pub player_hit: bool,
}

/// Commits one pressure lunge from `separation` against a player answering
/// with `response`, and reports what happened.
///
/// The band is narrowed to `separation` so the brain commits on its first
/// tick of approach, facing the player as both bodies start. Everything else
/// is the initiative fixture's tuning on flat ground, run by the real rules:
/// the same sweep, the same movement and the same interruption rule the fight
/// uses.
pub fn probe_lunge(
    separation: f32,
    response: LungeResponse,
) -> Result<Option<LungeProbe>, EncounterError> {
    probe_lunge_with(fixture::pressure(), separation, response)
}

/// [`probe_lunge`] with a lunge other than the fixture's, which is how the
/// fixture's numbers were chosen.
pub fn probe_lunge_with(
    pressure: crate::spec::AuthoredPressure,
    separation: f32,
    response: LungeResponse,
) -> Result<Option<LungeProbe>, EncounterError> {
    let ground = fixture::golden_ground();
    let mut setup = initiative_oracle_setup(CombatSeed::GOLDEN);
    setup.tuning.adversary_pressure = Some(pressure);
    if let Some(pressure) = setup.tuning.adversary_pressure.as_mut() {
        // The band may sit anywhere for a probe, but it has to stay outside
        // the primary's strike range to compile.
        pressure.select_min = (separation - 0.20).max(setup.tuning.adversary.strike_range + 0.01);
        pressure.select_max = separation + 0.01;
    }
    setup.starts = [
        Vec2::new(0.0, separation * 0.5),
        Vec2::new(0.0, -separation * 0.5),
    ];
    let mut encounter = Encounter::new(&setup, Some(&ground))?;
    encounter.arm();
    let world = WorldContact::ground_only(&ground);
    let mut committed_at = None;
    let mut lunge_hit = false;
    let mut player_hit = false;
    let mut pressed = false;
    for _ in 0..400 {
        let adversary = encounter.combatant(Side::Adversary);
        let windup = match *adversary.action() {
            Action::Attack {
                kind: AttackKind::Pressure,
                elapsed,
                ..
            } => Some(elapsed),
            _ => None,
        };
        if committed_at.is_some() && windup.is_none() {
            break;
        }
        let player = encounter.combatant(Side::Player);
        let line = adversary.position() - player.position();
        let toward = line.normalize_or_zero();
        // `toward` points from the player at the adversary, so the
        // adversary's own right — its weapon side — is `(toward.y, -toward.x)`
        // seen from the player: facing yaw zero looks down `-Z` with `+X` on
        // the right.
        let weapon_side = Vec2::new(toward.y, -toward.x);
        let intent = match response {
            LungeResponse::Nothing => Intent::idle(),
            LungeResponse::Walk { reaction, side } => {
                let go = windup.is_some_and(|tick| tick >= reaction);
                let across = weapon_side * f32::from(side.signum());
                Intent::player(if go { across } else { Vec2::ZERO }, false, false)
            }
            LungeResponse::Dodge { reaction, side } => {
                let go = !pressed && windup.is_some_and(|tick| tick >= reaction);
                pressed |= go;
                let across = weapon_side * f32::from(side.signum());
                Intent::player(if go { across } else { Vec2::ZERO }, false, go)
            }
            LungeResponse::Charge { misjudgement_centi } => {
                let range = AIM_ASSIST_RANGE + misjudgement_centi as f32 / 100.0;
                let swing = player.can_act() && line.length() <= range;
                Intent::player(toward, swing, false)
            }
            LungeResponse::AdvanceThenLeave {
                reaction,
                side,
                dodge,
            } => {
                let go = windup.is_some_and(|tick| tick >= reaction);
                if go {
                    let press = dodge && !pressed;
                    pressed |= press;
                    Intent::player(weapon_side * f32::from(side.signum()), false, press)
                } else {
                    Intent::player(toward, false, false)
                }
            }
            LungeResponse::SwingAtOnce => {
                let go = !pressed && windup.is_some();
                pressed |= go;
                Intent::player(Vec2::ZERO, go, false)
            }
        };
        let distance = encounter.separation_distance();
        let events = encounter.step(intent, world);
        if committed_at.is_none()
            && encounter.combatant(Side::Adversary).action().attack_kind()
                == Some(AttackKind::Pressure)
        {
            committed_at = Some(distance);
        }
        for event in events.iter() {
            if let CombatEvent::Hit { attacker, .. } = event {
                match attacker {
                    Side::Adversary => lunge_hit = true,
                    Side::Player => player_hit = true,
                }
            }
        }
    }
    Ok(committed_at.map(|committed_at| LungeProbe {
        committed_at,
        lunge_hit,
        player_hit,
    }))
}

/// Locked hash of the combat initiative reference.
///
/// Two fights on flat ground under the golden seed, one after the other:
/// owner-spam with no misjudgement, and the read policy at the typical `24`
/// ticks stepping to the free-hand side. Every tick until the first defeat:
/// both healths, both action labels, attack kinds and elapsed counters, both
/// positions and facings, and every event — the trace format
/// `GOLDEN_ENCOUNTER_SIGNATURE` uses, plus the kind. A retuned lunge, a moved
/// band, a changed spacing dodge or a new pose that changes where a blade goes
/// all move it, which is the intent.
///
/// It is never compared with `GOLDEN_ENCOUNTER_SIGNATURE`: that one is the
/// historical encounter and must not move; this one is the capability.
///
/// **Old** none, **new** `0x8238_2662_d859_8cf3`, **why**: first lock, taken
/// once the tuning had stopped moving.
pub const COMBAT_INITIATIVE_SIGNATURE: u64 = 0x8238_2662_d859_8cf3;

/// Computes the combat initiative reference hash from scratch.
pub fn initiative_signature() -> Result<u64, EncounterError> {
    use crate::hash::{fnv1a64, push_f32, push_u16, push_u32, push_u64};
    let ground = fixture::golden_ground();
    let mut bytes = Vec::with_capacity(64 * 1024);
    for policy in [
        OraclePolicy::OwnerSpam { misjudgement: 0.0 },
        OraclePolicy::Read {
            lag: 24,
            walk_only: false,
            side: 1.0,
        },
    ] {
        let setup = initiative_oracle_setup(CombatSeed::GOLDEN);
        let mut encounter = Encounter::new(&setup, Some(&ground))?;
        encounter.arm();
        for _ in 0..ORACLE_FIGHT_TICKS {
            let events = encounter.step(
                policy.intent(&encounter),
                WorldContact::ground_only(&ground),
            );
            push_u64(&mut bytes, encounter.tick_index());
            for side in crate::combatant::SIDES {
                let combatant = encounter.combatant(side);
                let action = combatant.action();
                push_u16(&mut bytes, combatant.health().current());
                push_u32(
                    &mut bytes,
                    fnv1a64(action.label(encounter.attack_spec(side)).as_bytes()) as u32,
                );
                push_u32(
                    &mut bytes,
                    match action.attack_kind() {
                        None => 0,
                        Some(AttackKind::Primary) => 1,
                        Some(AttackKind::Pressure) => 2,
                    },
                );
                push_u32(&mut bytes, action.elapsed());
                push_f32(&mut bytes, combatant.position().x);
                push_f32(&mut bytes, combatant.position().y);
                push_f32(&mut bytes, combatant.state().facing);
            }
            for event in events.iter() {
                push_u32(&mut bytes, fnv1a64(event.name().as_bytes()) as u32);
            }
            if encounter.outcome().is_some() {
                break;
            }
        }
    }
    Ok(fnv1a64(&bytes))
}

/// What happened after a lunge was stepped around and then answered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpeningProbe {
    /// Ticks into the whiffed lunge's recovery at which the player's answer
    /// landed, if it landed while the recovery was still running.
    pub punished_after: Option<Ticks>,
    /// Ticks the recovery ran, if nothing cut it short.
    pub recovered_after: Option<Ticks>,
}

/// Steps around one lunge from the middle of the band, waits until the missed
/// lunge's recovery has been visible for `lag` ticks, then walks in and swings
/// as soon as the body can reach: whether a person who saw the opening late can
/// still take it. A `lag` of [`Ticks::MAX`] never answers, which measures the
/// recovery itself.
pub fn probe_opening(lag: Ticks) -> Result<Option<OpeningProbe>, EncounterError> {
    let ground = fixture::golden_ground();
    let mut setup = initiative_oracle_setup(CombatSeed::GOLDEN);
    let pressure = fixture::pressure();
    let separation = (pressure.select_min + pressure.select_max) * 0.5;
    setup.starts = [
        Vec2::new(0.0, separation * 0.5),
        Vec2::new(0.0, -separation * 0.5),
    ];
    let mut encounter = Encounter::new(&setup, Some(&ground))?;
    encounter.arm();
    let world = WorldContact::ground_only(&ground);
    let mut dodged = false;
    let mut seen_lunge = false;
    let mut recovery_started = None;
    let mut recovery = None;
    let mut punished_after = None;
    for _ in 0..600 {
        let adversary = encounter.combatant(Side::Adversary);
        let player = encounter.combatant(Side::Player);
        let spec = *encounter.attack_spec(Side::Adversary);
        let line = adversary.position() - player.position();
        let toward = line.normalize_or_zero();
        let tick = encounter.tick_index();
        let intent = match *adversary.action() {
            Action::Attack {
                kind: AttackKind::Pressure,
                elapsed,
                ..
            } => {
                seen_lunge = true;
                if elapsed < spec.active_end() {
                    let go = !dodged && elapsed >= 24;
                    dodged |= go;
                    Intent::player(Vec2::new(-toward.y, toward.x), false, go)
                } else {
                    let started = *recovery_started.get_or_insert(tick);
                    let ready = tick >= started.saturating_add(u64::from(lag));
                    let swing = ready && player.can_act() && line.length() <= 2.8;
                    Intent::player(if ready { toward } else { Vec2::ZERO }, swing, false)
                }
            }
            _ => Intent::idle(),
        };
        let events = encounter.step(intent, world);
        let still_recovering = matches!(
            *encounter.combatant(Side::Adversary).action(),
            Action::Attack { kind: AttackKind::Pressure, elapsed, .. } if elapsed >= spec.active_end()
        );
        let hit = events.any(|event| {
            matches!(
                event,
                CombatEvent::Hit {
                    attacker: Side::Player,
                    ..
                }
            )
        });
        if let Some(started) = recovery_started
            && recovery.is_none()
        {
            let into = (encounter.tick_index() - started) as Ticks;
            if hit {
                punished_after = Some(into);
                recovery = Some(());
            } else if !still_recovering {
                return Ok(Some(OpeningProbe {
                    punished_after: None,
                    recovered_after: Some(into),
                }));
            }
        }
        if seen_lunge && recovery.is_some() {
            break;
        }
    }
    Ok(recovery.map(|()| OpeningProbe {
        punished_after,
        recovered_after: None,
    }))
}

/// What a lunge that connected left behind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectProbe {
    /// Ticks from the hit until the adversary could act again.
    pub adversary_free_after: u64,
    /// Ticks from the hit until the player could act again.
    pub player_free_after: u64,
    /// Ticks from the hit until the adversary started its spacing dodge.
    pub spacing_after: Option<u64>,
    /// Separation when the spacing dodge ended.
    pub separation_after_spacing: Option<f32>,
}

/// Lets one lunge land on a player who does nothing, and reports who could act
/// first afterwards and what the adversary did with that.
pub fn probe_connect() -> Result<Option<ConnectProbe>, EncounterError> {
    let ground = fixture::golden_ground();
    let mut setup = initiative_oracle_setup(CombatSeed::GOLDEN);
    let pressure = fixture::pressure();
    let separation = (pressure.select_min + pressure.select_max) * 0.5;
    setup.starts = [
        Vec2::new(0.0, separation * 0.5),
        Vec2::new(0.0, -separation * 0.5),
    ];
    let mut encounter = Encounter::new(&setup, Some(&ground))?;
    encounter.arm();
    let world = WorldContact::ground_only(&ground);
    let mut hit_at = None;
    let mut adversary_free = None;
    let mut player_free = None;
    let mut spacing = None;
    let mut spacing_end = None;
    for _ in 0..600 {
        let events = encounter.step(Intent::idle(), world);
        let tick = encounter.tick_index();
        for event in events.iter() {
            match event {
                CombatEvent::Hit {
                    attacker: Side::Adversary,
                    ..
                } if hit_at.is_none() => hit_at = Some(tick),
                CombatEvent::DodgeStarted {
                    side: Side::Adversary,
                    ..
                } if spacing.is_none() => spacing = Some(tick),
                _ => {}
            }
        }
        if let Some(hit) = hit_at {
            if adversary_free.is_none() && encounter.combatant(Side::Adversary).can_act() {
                adversary_free = Some(tick - hit);
            }
            if player_free.is_none() && encounter.combatant(Side::Player).can_act() {
                player_free = Some(tick - hit);
            }
        }
        if spacing.is_some()
            && spacing_end.is_none()
            && !encounter.combatant(Side::Adversary).action().is_dodging()
        {
            spacing_end = Some(encounter.separation_distance());
        }
        if spacing_end.is_some() && player_free.is_some() {
            break;
        }
    }
    Ok(match (hit_at, adversary_free, player_free) {
        (Some(hit), Some(adversary), Some(player)) => Some(ConnectProbe {
            adversary_free_after: adversary,
            player_free_after: player,
            spacing_after: spacing.map(|tick| tick - hit),
            separation_after_spacing: spacing_end,
        }),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        FightReport, LungeResponse, ORACLE_MISJUDGEMENT, ORACLE_SEEDS, OraclePolicy, READ_LAGS,
        initiative_oracle_setup, oracle_setup, probe_connect, probe_lunge, probe_opening,
        run_fight,
    };
    use crate::combatant::Side;
    use crate::encounter::{EncounterSetup, WorldContact};
    use crate::fixture;
    use crate::spec::CombatSeed;
    use crate::tick::Ticks;

    fn side(index: usize) -> f32 {
        if index.is_multiple_of(2) { 1.0 } else { -1.0 }
    }

    fn fights(
        setup: fn(CombatSeed) -> EncounterSetup,
        policy: impl Fn(usize) -> OraclePolicy,
    ) -> Vec<FightReport> {
        let ground = fixture::golden_ground();
        ORACLE_SEEDS
            .iter()
            .enumerate()
            .map(|(index, seed)| {
                match run_fight(
                    &setup(*seed),
                    WorldContact::ground_only(&ground),
                    policy(index),
                    super::ORACLE_FIGHT_TICKS,
                ) {
                    Ok(report) => report,
                    Err(error) => panic!("{error}"),
                }
            })
            .collect()
    }

    fn spam(index: usize) -> OraclePolicy {
        OraclePolicy::OwnerSpam {
            misjudgement: ORACLE_MISJUDGEMENT[index],
        }
    }

    fn wins(reports: &[FightReport]) -> usize {
        reports
            .iter()
            .filter(|r| r.winner == Some(Side::Player))
            .count()
    }

    fn losses(reports: &[FightReport]) -> usize {
        reports
            .iter()
            .filter(|r| r.winner == Some(Side::Adversary))
            .count()
    }

    fn total(reports: &[FightReport], field: impl Fn(&FightReport) -> u32) -> u32 {
        reports.iter().map(field).sum()
    }

    fn print(name: &str, reports: &[FightReport]) {
        println!(
            "{name:14} W/L/D {}/{}/{} health {} | lunges {} hit {} whiff {} primary {} | spacing {} unresolved {} | punishes {} player whiffs {} | ticks {} | max step {:.3} longest quiet {}",
            wins(reports),
            losses(reports),
            reports.iter().filter(|r| r.winner.is_none()).count(),
            total(reports, |r| u32::from(r.player_health)),
            total(reports, |r| r.counters.pressure_swings),
            total(reports, |r| r.counters.pressure_hits),
            total(reports, |r| r.counters.pressure_whiffs),
            total(reports, FightReport::adversary_primary_swings),
            total(reports, |r| r.counters.dodges[1]),
            total(reports, |r| r.counters.dodges_during_unresolved_swing[1]),
            total(reports, |r| r.punishes),
            total(reports, |r| r.counters.whiffs[0]),
            total(reports, |r| r.ticks),
            reports.iter().map(|r| r.max_step).fold(0.0_f32, f32::max),
            reports.iter().map(|r| r.longest_quiet).max().unwrap_or(0),
        );
    }

    /// The M9 owner finding as repository evidence, and the control that says
    /// the oracle did not change meaning when combat initiative arrived: the
    /// owner's strategy beats the historical encounter under every seed without
    /// taking a hit. Expected to keep passing for as long as the historical
    /// encounter exists.
    #[test]
    fn owner_spam_beats_the_historical_encounter_every_time_untouched() {
        let reports = fights(oracle_setup, spam);
        print("historical spam", &reports);
        for report in &reports {
            assert_eq!(report.winner, Some(Side::Player), "{report:?}");
            assert_eq!(report.player_health, 96, "{report:?}");
            assert_eq!(report.counters.hits[1], 0, "the adversary landed a hit");
            assert_eq!(report.counters.dodges[0], 0, "owner-spam never dodges");
            assert_eq!(report.counters.pressure_swings, 0);
            assert_eq!(report.counters.dodges[1], 0, "no spacing without a lunge");
        }
        let ticks: Vec<u32> = reports.iter().map(|r| r.ticks).collect();
        assert_eq!(
            ticks,
            [347, 344, 341, 412, 415, 337],
            "the recorded control"
        );
    }

    /// The other control: the read policy against the historical encounter is
    /// the M6 intentional player, and it wins every fight untouched.
    #[test]
    fn reading_beats_the_historical_encounter_every_time() {
        let reports = fights(oracle_setup, |index| OraclePolicy::Read {
            lag: 24,
            walk_only: false,
            side: side(index),
        });
        print("historical read", &reports);
        for report in &reports {
            assert_eq!(report.winner, Some(Side::Player), "{report:?}");
            assert_eq!(report.counters.hits[1], 0);
            assert!(report.counters.dodges[0] > 0, "it never dodged");
        }
    }

    /// The combat initiative gate on flat ground, in one place because the
    /// relations are between policies: owner-spam loses more than it wins,
    /// reading wins reliably at every human reaction it can use, the policy that
    /// learned only the line sits between, nobody stalls, the primary still
    /// fights, and no spacing dodge ever starts over a live swing.
    #[test]
    fn with_initiative_reading_beats_spam_and_spam_loses_more_than_it_wins() {
        let spam = fights(initiative_oracle_setup, spam);
        let spam_read = fights(initiative_oracle_setup, |index| OraclePolicy::SpamRead {
            misjudgement: ORACLE_MISJUDGEMENT[index],
            lag: 24,
            side: side(index),
        });
        print("owner-spam", &spam);
        print("spam-read", &spam_read);
        assert!(wins(&spam) <= 2, "owner-spam won {} of 6", wins(&spam));
        assert!(wins(&spam) < losses(&spam));
        assert!(total(&spam, |r| r.counters.pressure_hits) > 0);
        assert!(
            total(&spam, FightReport::adversary_primary_swings) > 0,
            "the primary must still fight at close range"
        );
        assert!(wins(&spam_read) > wins(&spam));

        let mut every = vec![spam.clone(), spam_read.clone()];
        for lag in READ_LAGS {
            for walk_only in [false, true] {
                let read = fights(initiative_oracle_setup, |index| OraclePolicy::Read {
                    lag,
                    walk_only,
                    side: side(index),
                });
                print(
                    &format!("read-{}{lag}", if walk_only { "walk" } else { "dodge" }),
                    &read,
                );
                // Every human reaction that is not already too late to step
                // off a line it keeps walking into.
                if lag <= 32 {
                    assert!(wins(&read) >= 5, "read lag {lag} won {}", wins(&read));
                    assert!(total(&read, |r| r.punishes) > total(&spam_read, |r| r.punishes));
                    assert!(
                        total(&read, |r| r.counters.whiffs[0])
                            < total(&spam_read, |r| r.counters.whiffs[0]),
                        "reading wastes fewer swings than spamming does"
                    );
                    assert!(
                        total(&read, |r| u32::from(r.player_health))
                            > total(&spam, |r| u32::from(r.player_health))
                    );
                }
                every.push(read);
            }
        }
        for reports in &every {
            for report in reports {
                assert!(report.winner.is_some(), "a stalemate: {report:?}");
                // No teleport: nothing but a knockback moves a body further
                // than a dodge does in a tick, and a knockback is `0.35`.
                assert!(report.max_step < 0.45, "a body jumped {}", report.max_step);
                // No endless kiting: nobody goes five seconds without a hit.
                assert!(
                    report.longest_quiet < 600,
                    "quiet for {}",
                    report.longest_quiet
                );
                assert_eq!(
                    report.counters.dodges_during_unresolved_swing[1], 0,
                    "a spacing dodge started over a live swing"
                );
            }
        }
        assert!(
            every
                .iter()
                .flatten()
                .map(|r| r.counters.dodges[1])
                .sum::<u32>()
                > 0,
            "the spacing dodge never happened, so the audit audited nothing"
        );
    }

    /// No bluff: a player who stands still and does nothing anywhere in the
    /// authored band is hit.
    #[test]
    fn every_distance_in_the_band_is_threatened() {
        let pressure = fixture::pressure();
        let mut distance = pressure.select_min;
        while distance <= pressure.select_max + 1.0e-4 {
            match probe_lunge(distance, LungeResponse::Nothing) {
                Ok(Some(probe)) => assert!(probe.lunge_hit, "a bluff at {distance}"),
                other => panic!("no lunge at {distance}: {other:?}"),
            }
            distance += 0.02;
        }
    }

    /// The band's far edge is tight rather than generous: just past it the lunge
    /// no longer reaches a body that does nothing, which is why the band stops
    /// where it does.
    #[test]
    fn just_past_the_band_the_lunge_would_be_a_bluff() {
        let beyond = fixture::pressure().select_max + 0.20;
        match probe_lunge(beyond, LungeResponse::Nothing) {
            Ok(Some(probe)) => assert!(!probe.lunge_hit, "the lunge reaches past its band"),
            other => panic!("no lunge at {beyond}: {other:?}"),
        }
    }

    /// The band's near edge is where charging stops working: from anywhere in
    /// it, a player who runs straight in and swings the moment it thinks it is
    /// in range — under every misjudgement the oracles use — is hit first.
    /// Below it, the most optimistic swing gets there first, which is why the
    /// band starts where it does.
    #[test]
    fn inside_the_band_charging_in_and_swinging_is_hit_first() {
        let pressure = fixture::pressure();
        let mut distance = pressure.select_min;
        while distance <= pressure.select_max + 1.0e-4 {
            for misjudgement_centi in (-25..=25).step_by(5) {
                match probe_lunge(distance, LungeResponse::Charge { misjudgement_centi }) {
                    Ok(Some(probe)) => assert!(
                        probe.lunge_hit && !probe.player_hit,
                        "at {distance} a charge misjudging by {misjudgement_centi} got through: {probe:?}"
                    ),
                    other => panic!("no lunge at {distance}: {other:?}"),
                }
            }
            distance += 0.05;
        }
        let below = pressure.select_min - 0.25;
        match probe_lunge(
            below,
            LungeResponse::Charge {
                misjudgement_centi: 25,
            },
        ) {
            Ok(Some(probe)) => assert!(
                probe.player_hit,
                "below the band an early swing should interrupt: {probe:?}"
            ),
            other => panic!("no lunge at {below}: {other:?}"),
        }
    }

    /// Swinging on the spot the moment the windup shows is not an answer: the
    /// blade is nowhere near, and the lunge arrives during its recovery.
    #[test]
    fn swinging_at_once_into_a_lunge_is_hit() {
        let pressure = fixture::pressure();
        let mut distance = pressure.select_min;
        while distance <= pressure.select_max + 1.0e-4 {
            match probe_lunge(distance, LungeResponse::SwingAtOnce) {
                Ok(Some(probe)) => assert!(probe.lunge_hit && !probe.player_hit, "{probe:?}"),
                other => panic!("no lunge at {distance}: {other:?}"),
            }
            distance += 0.05;
        }
    }

    /// The fairness gate: a lunge is escaped by leaving its line to either side
    /// at a human reaction. A dodge works at `150` to `400` ms from anywhere in
    /// the band; walking works at `150` to `267` ms. Neither needs the `18`–`20`
    /// ticks Combat Pressure's reactive dodge did.
    #[test]
    fn a_lunge_is_escaped_sideways_at_a_human_reaction_on_either_side() {
        let pressure = fixture::pressure();
        let mut distance = pressure.select_min;
        while distance <= pressure.select_max + 1.0e-4 {
            for side in [1_i8, -1] {
                for reaction in READ_LAGS {
                    match probe_lunge(distance, LungeResponse::Dodge { reaction, side }) {
                        Ok(Some(probe)) => assert!(
                            !probe.lunge_hit,
                            "dodging to side {side} at {reaction} from {distance} was hit"
                        ),
                        other => panic!("no lunge at {distance}: {other:?}"),
                    }
                }
                for reaction in READ_LAGS {
                    match probe_lunge(distance, LungeResponse::Walk { reaction, side }) {
                        Ok(Some(probe)) => assert!(
                            !probe.lunge_hit,
                            "walking to side {side} at {reaction} from {distance} was hit"
                        ),
                        other => panic!("no lunge at {distance}: {other:?}"),
                    }
                }
            }
            distance += 0.05;
        }
    }

    /// The escape a person actually makes: walking *into* the adversary when
    /// the lunge starts, then leaving the line. Closing shortens the time, which
    /// is what the `66`-tick windup was chosen against: a walking escape still
    /// works up to `40` ticks from either side, a dodge up to `48`.
    #[test]
    fn walking_into_a_lunge_still_leaves_time_to_step_off_its_line() {
        let pressure = fixture::pressure();
        let mut distance = pressure.select_min;
        while distance <= pressure.select_max + 1.0e-4 {
            for side in [1_i8, -1] {
                for (dodge, reactions) in [
                    (false, &[12_u32, 18, 24, 28, 32, 36, 40][..]),
                    (true, &[12_u32, 18, 24, 28, 32, 36, 40, 48][..]),
                ] {
                    for &reaction in reactions {
                        let response = LungeResponse::AdvanceThenLeave {
                            reaction,
                            side,
                            dodge,
                        };
                        match probe_lunge(distance, response) {
                            Ok(Some(probe)) => assert!(
                                !probe.lunge_hit,
                                "advancing, then leaving to side {side} at {reaction} \
                                 (dodge {dodge}) from {distance} was hit"
                            ),
                            other => panic!("no lunge at {distance}: {other:?}"),
                        }
                    }
                }
            }
            distance += 0.05;
        }
    }

    /// A missed lunge is an opening a person can take: stepped around, then
    /// answered after watching the recovery for as long as a slow reaction
    /// takes, and the answer still lands before the recovery ends.
    #[test]
    fn a_missed_lunge_leaves_an_opening_a_slow_reader_can_take() {
        let spec = *fixture::pressure()
            .compile()
            .unwrap_or_else(|e| panic!("{e}"))
            .attack();
        match probe_opening(u32::MAX) {
            Ok(Some(probe)) => assert_eq!(
                probe.recovered_after,
                Some(spec.recovery()),
                "a whiff runs the long recovery"
            ),
            other => panic!("the lunge never whiffed: {other:?}"),
        }
        for lag in READ_LAGS {
            match probe_opening(lag) {
                Ok(Some(probe)) => {
                    let into = probe.punished_after.unwrap_or(Ticks::MAX);
                    println!(
                        "lag {lag}: punished {into} ticks into a {} tick recovery",
                        spec.recovery()
                    );
                    assert!(
                        into < spec.recovery(),
                        "lag {lag} could not take the opening"
                    );
                }
                other => panic!("the lunge never whiffed: {other:?}"),
            }
        }
    }

    /// A lunge that connects recovers before the player it hit can act, and the
    /// adversary uses that to reopen the distance: its spacing dodge starts
    /// while the player is still locked, and ends back inside the band's reach.
    #[test]
    fn a_connected_lunge_recovers_fast_and_spaces_before_the_player_can_act() {
        let probe = match probe_connect() {
            Ok(Some(probe)) => probe,
            other => panic!("the lunge never connected: {other:?}"),
        };
        println!("{probe:?}");
        assert!(
            probe.adversary_free_after < probe.player_free_after,
            "{probe:?}"
        );
        let spacing = probe.spacing_after.unwrap_or(u64::MAX);
        assert!(
            spacing < probe.player_free_after,
            "the spacing dodge came after the player was free: {probe:?}"
        );
        let separation = probe.separation_after_spacing.unwrap_or(0.0);
        assert!(
            separation > 3.5,
            "the spacing dodge opened too little: {probe:?}"
        );
    }

    #[test]
    fn the_combat_initiative_reference_is_locked() {
        let measured = super::initiative_signature().unwrap_or_else(|e| panic!("{e}"));
        println!("COMBAT_INITIATIVE_SIGNATURE measured {measured:#018x}");
        assert_eq!(
            measured,
            super::COMBAT_INITIATIVE_SIGNATURE,
            "re-lock deliberately with an OLD/NEW/WHY paragraph"
        );
    }

    /// Same seed, same fight: the capability draws nothing new from any stream
    /// and adds no wall-clock input, so a replay is exact.
    #[test]
    fn the_same_seed_replays_the_same_initiative_fight() {
        let first = fights(initiative_oracle_setup, spam);
        let second = fights(initiative_oracle_setup, spam);
        assert_eq!(first, second);
    }

    /// The geometry the no-bluff evidence rests on, pinned where a pose change
    /// would break it first: while a lunge can connect, its blade is level and
    /// at chest height, and by the end of the travel it is on the line. The
    /// first set of lunge keys raised the point thirty degrees above level as
    /// the arm drove forward and hit no stationary target at any distance.
    #[test]
    fn the_lunge_blade_is_level_and_on_the_line_while_it_can_connect() {
        use crate::combatant::{Action, AttackKind, SwingId};
        use crate::weapon::WeaponCompiler;
        use veldwake_character::skeleton::{BoneId, Side as BodySide};
        use veldwake_character::{CharacterCompiler, CharacterState, pose_with};
        let adversary = CharacterCompiler::new()
            .compile_descriptor(&fixture::adversary_descriptor())
            .unwrap_or_else(|e| panic!("{e}"));
        let weapon = WeaponCompiler::new()
            .compile_descriptor(&fixture::weapon_descriptor())
            .unwrap_or_else(|e| panic!("{e}"));
        let spec = *fixture::pressure()
            .compile()
            .unwrap_or_else(|e| panic!("{e}"))
            .attack();
        let state = CharacterState::default();
        for elapsed in spec.active_start()..spec.active_end() {
            let action = Action::Attack {
                swing: SwingId::first(),
                kind: AttackKind::Pressure,
                elapsed,
                hits: [false; 2],
            };
            let overlay = action.overlay(&spec, BodySide::Right, state.facing);
            let posed = pose_with(&adversary, &state, None, Some(&overlay));
            let blade = weapon.blade_world(
                posed.world_matrix(),
                posed.bone_world()[BoneId::HandR.index()],
            );
            // Facing zero looks down -Z, with +X on the body's right.
            assert!(
                (blade.tip.y - blade.base.y).abs() < 0.25,
                "tick {elapsed}: the blade is not level ({:.2} to {:.2})",
                blade.base.y,
                blade.tip.y
            );
            for y in [blade.base.y, blade.tip.y] {
                assert!(
                    (1.2..=2.0).contains(&y),
                    "tick {elapsed}: blade at height {y:.2}"
                );
            }
            assert!(
                -blade.tip.z > 1.5,
                "tick {elapsed}: the point is not forward"
            );
        }
        let last = Action::Attack {
            swing: SwingId::first(),
            kind: AttackKind::Pressure,
            elapsed: spec.active_end() - 1,
            hits: [false; 2],
        };
        let overlay = last.overlay(&spec, BodySide::Right, state.facing);
        let posed = pose_with(&adversary, &state, None, Some(&overlay));
        let blade = weapon.blade_world(
            posed.world_matrix(),
            posed.bone_world()[BoneId::HandR.index()],
        );
        assert!(
            blade.tip.x.abs() < 0.3,
            "the extended point is off the line: {:.2}",
            blade.tip.x
        );
        assert!(
            blade.base.x.abs() < 0.3,
            "the extended hilt is off the line: {:.2}",
            blade.base.x
        );
    }

    /// The table the band was derived from: every distance from `3.40` to
    /// `5.00`, what a body that does nothing, one that charges and one that
    /// steps aside at each reaction meet. `docs/planning/COMBAT_INITIATIVE.md`
    /// reads its output.
    #[test]
    #[ignore = "measurement: prints the table the pressure band is derived from"]
    fn measure_the_lunge_band() {
        // `WINDUP_TICKS` and `LUNGE_TICKS` measure a variant without editing the
        // fixture; unset, this is the fixture's own lunge.
        let mut pressure = fixture::pressure();
        let env = |key: &str| std::env::var(key).ok().and_then(|v| v.parse::<f64>().ok());
        if let Some(ticks) = env("WINDUP_TICKS") {
            pressure.attack.windup_seconds = ticks / 120.0;
        }
        if let Some(ticks) = env("LUNGE_TICKS") {
            pressure.lunge_seconds = ticks / 120.0;
        }
        if let Some(distance) = env("LUNGE") {
            pressure.lunge_distance = distance as f32;
        }
        let probe_lunge = |d: f32, r: LungeResponse| super::probe_lunge_with(pressure, d, r);
        let mut distance = 3.40_f32;
        while distance <= 5.001 {
            let hit =
                |response| matches!(probe_lunge(distance, response), Ok(Some(p)) if p.lunge_hit);
            let first = |response| match probe_lunge(distance, response) {
                Ok(Some(p)) if p.player_hit => "P",
                Ok(Some(p)) if p.lunge_hit => "L",
                _ => "-",
            };
            let charge: String = [-25, 0, 25]
                .into_iter()
                .map(|misjudgement_centi| first(LungeResponse::Charge { misjudgement_centi }))
                .collect();
            let escapes = |make: fn(u32, i8) -> LungeResponse, lags: &[u32]| -> String {
                lags.iter()
                    .map(|lag| {
                        let both = [1_i8, -1].map(|side| !hit(make(*lag, side)));
                        match both {
                            [true, true] => '2',
                            [true, false] | [false, true] => '1',
                            [false, false] => '0',
                        }
                    })
                    .collect()
            };
            println!(
                "{distance:.2} still-hit {} charge(-25/0/+25) {charge} dodge R18/24/32/48 {} walk R18/24/32/48 {}",
                u8::from(hit(LungeResponse::Nothing)),
                escapes(
                    |reaction, side| LungeResponse::Dodge { reaction, side },
                    &READ_LAGS
                ),
                escapes(
                    |reaction, side| LungeResponse::Walk { reaction, side },
                    &READ_LAGS
                ),
            );
            let advancing = |dodge: bool| -> String {
                [12_u32, 18, 24, 28, 32, 36, 40, 48]
                    .iter()
                    .map(|reaction| {
                        let both = [1_i8, -1].map(|side| {
                            !hit(LungeResponse::AdvanceThenLeave {
                                reaction: *reaction,
                                side,
                                dodge,
                            })
                        });
                        match both {
                            [true, true] => '2',
                            [true, false] | [false, true] => '1',
                            [false, false] => '0',
                        }
                    })
                    .collect()
            };
            println!(
                "      advancing, leave at R12/18/24/28/32/36/40/48: walk {} dodge {}",
                advancing(false),
                advancing(true)
            );
            distance += 0.05;
        }
    }
}
