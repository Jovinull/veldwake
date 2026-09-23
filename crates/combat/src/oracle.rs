//! Behavioural oracles for the encounter: small scripted players whose results
//! are evidence about the fight rather than about one rule.
//!
//! **Why this exists.** The M9 owner playtest found that the natural way to
//! fight the M6 adversary — walk up to it, face it, and attack whenever the
//! body can — wins every time, and that no bounded retuning of the existing
//! encounter changed that. That finding was produced by throwaway harnesses
//! outside the repository. This module turns it into repository evidence:
//! [`OraclePolicy::OwnerSpam`] is the owner's behaviour as a policy, and
//! [`OraclePolicy::Intentional`] is the fight M6 was designed around — read the
//! telegraph, get out of the way, punish the commitment. Both are deliberately
//! simple. They are instruments, not opponents, and a policy clever enough to
//! need its own tests would be measuring itself.
//!
//! [`probe_reactive_evasion`] is the Combat Pressure measurement: whether the
//! adversary's body, dodging after a given reaction delay, escapes the player's
//! swing. It needs no adversary dodge in the rules, because the rules are
//! side-neutral: it swaps the two bodies' roles instead. See
//! `docs/planning/COMBAT_PRESSURE.md`.

use glam::Vec2;

use veldwake_character::GroundSampler;

use crate::combatant::{Action, Intent, Side};
use crate::encounter::{
    AIM_ASSIST_RANGE, CombatCounters, Encounter, EncounterError, EncounterSetup,
    PlayerVictoryPolicy, WorldContact,
};
use crate::event::CombatEvent;
use crate::fixture;
use crate::spec::{AuthoredAdversary, AuthoredDodge, CombatSeed};
use crate::tick::Ticks;

/// The seeds every oracle fight is run under.
///
/// The golden seed plus five others, the same six the M9 investigation used,
/// so a result here can be read against that record.
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

/// How long the intentional player takes to act on what it sees, in ticks.
///
/// `24` ticks, `200` ms: the middle of the `18`–`32` range the M9
/// investigation used for a human approximation. The intentional player reads
/// a telegraph only once it has been visible this long, and a recovery only
/// once it has lasted this long.
pub const OBSERVATION_LAG: Ticks = 24;

/// Longest a fight is allowed to run, in ticks: sixty seconds.
pub const ORACLE_FIGHT_TICKS: u32 = 7_200;

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
    /// The fight M6 was built around: hold just outside the adversary's reach,
    /// dodge its telegraph once it has been seen, punish its recovery and its
    /// stagger, and never swing into a ready opponent.
    Intentional,
}

impl OraclePolicy {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OwnerSpam { .. } => "owner-spam",
            Self::Intentional => "intentional",
        }
    }

    /// This policy's intent for the next tick, from what the encounter shows.
    ///
    /// Reads only positions, actions and their elapsed counters — what a
    /// person can see — and never an input or a future.
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
        match self {
            Self::OwnerSpam { misjudgement } => {
                let swing = me.can_act() && distance <= AIM_ASSIST_RANGE + misjudgement;
                Intent::player(toward, swing, false)
            }
            Self::Intentional => {
                let spec = encounter.attack_spec(Side::Adversary);
                let punish = |ready: bool| {
                    Intent::player(toward, ready && me.can_act() && distance <= 2.8, false)
                };
                match *foe.action() {
                    Action::Attack { elapsed, .. } if elapsed < spec.active_start() => {
                        let dodge = elapsed >= OBSERVATION_LAG && distance < 3.3;
                        let away = if dodge { -toward } else { Vec2::ZERO };
                        Intent::player(away, false, dodge)
                    }
                    Action::Attack { elapsed, .. } => {
                        punish(elapsed >= spec.active_end() + OBSERVATION_LAG)
                    }
                    Action::Stagger { .. } => punish(true),
                    Action::Dodge { .. } | Action::Defeated { .. } => Intent::idle(),
                    Action::Free => {
                        let walk = if distance > 3.0 { toward } else { Vec2::ZERO };
                        Intent::player(walk, false, false)
                    }
                }
            }
        }
    }
}

/// What one oracle fight did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FightReport {
    /// Who was still standing when the first body fell, if one did.
    pub winner: Option<Side>,
    pub ticks: u32,
    pub counters: CombatCounters,
    pub player_health: u16,
    pub adversary_health: u16,
    /// Longest run of ticks in which nobody was hit.
    pub longest_quiet: u32,
}

/// The reference fight for an oracle: the M6 bodies, weapon and tuning on open
/// ground under one seed, ending at the first defeat.
///
/// No arena — a disc would decide how far a body can retreat, and that is part
/// of what is being measured — and [`PlayerVictoryPolicy::Remain`], so a win is
/// the end of the run rather than the start of another round.
#[must_use]
pub fn oracle_setup(seed: CombatSeed) -> EncounterSetup {
    let mut setup = fixture::golden_setup();
    setup.arena = None;
    setup.player_victory = PlayerVictoryPolicy::Remain;
    setup.tuning.seed = seed;
    setup
}

/// Runs one policy against one setup until a body falls or the time runs out.
pub fn run_fight(
    setup: &EncounterSetup,
    ground: &dyn GroundSampler,
    policy: OraclePolicy,
    max_ticks: u32,
) -> Result<FightReport, EncounterError> {
    let mut encounter = Encounter::new(setup, Some(ground))?;
    encounter.arm();
    let mut winner = None;
    let mut quiet = 0_u32;
    let mut longest_quiet = 0_u32;
    let mut ticks = 0_u32;
    while ticks < max_ticks && winner.is_none() {
        let intent = policy.intent(&encounter);
        let events = encounter.step(intent, WorldContact::ground_only(ground));
        ticks += 1;
        let mut hit = false;
        for event in events.iter() {
            match event {
                CombatEvent::Hit { .. } => hit = true,
                CombatEvent::Defeated { side } => winner = Some(side.other()),
                _ => {}
            }
        }
        quiet = if hit { 0 } else { quiet + 1 };
        longest_quiet = longest_quiet.max(quiet);
    }
    Ok(FightReport {
        winner,
        ticks,
        counters: *encounter.counters(),
        player_health: encounter.combatant(Side::Player).health().current(),
        adversary_health: encounter.combatant(Side::Adversary).health().current(),
        longest_quiet,
    })
}

/// Whether the adversary's body escapes the player's swing by dodging sideways
/// `reaction` ticks after the swing became visible, from `separation` world
/// units apart.
///
/// **The roles are swapped, and that is the point.** The rules have no
/// adversary dodge and this measurement must not add one, but they are
/// side-neutral: one timeline, one hit query, one dodge rule for both bodies.
/// So the adversary's *body* is placed on the player side, where a dodge can be
/// asked for, and the player's body, weapon hand and attack spec are placed on
/// the adversary side, whose brain is tuned to commit at once from where it
/// stands. What is measured is exactly "the adversary's body, dodging this far
/// this fast after this delay, against the player's swing" — through the same
/// sweep, the same aim assist and the same movement rules the real fight uses.
///
/// The dodge goes perpendicular to the line between the bodies: the direction
/// that stays close enough for anything to follow it. Returns `None` if the
/// swing never started.
pub fn probe_reactive_evasion(
    dodge: AuthoredDodge,
    reaction: Ticks,
    separation: f32,
    ground: &dyn GroundSampler,
) -> Result<Option<bool>, EncounterError> {
    let mut setup = fixture::golden_setup();
    setup.player = fixture::adversary_descriptor();
    setup.adversary = fixture::player_descriptor();
    setup.tuning.adversary_attack = fixture::player_attack();
    setup.tuning.dodge = dodge;
    setup.tuning.adversary = AuthoredAdversary {
        aggro_radius: 14.0,
        strike_range: separation + 0.05,
        min_range: 0.10,
        ..fixture::adversary()
    };
    setup.arena = None;
    setup.starts = [
        Vec2::new(0.0, separation * 0.5),
        Vec2::new(0.0, -separation * 0.5),
    ];
    let mut encounter = Encounter::new(&setup, Some(ground))?;
    encounter.arm();
    let mut swung = false;
    let mut dodged = false;
    let mut hit = false;
    for _ in 0..240 {
        let swing_tick = match encounter.combatant(Side::Adversary).action() {
            Action::Attack { elapsed, .. } => Some(*elapsed),
            _ => None,
        };
        swung |= swing_tick.is_some();
        let press = !dodged && swing_tick == Some(reaction);
        let direction = if press {
            let line = encounter.combatant(Side::Adversary).position()
                - encounter.combatant(Side::Player).position();
            Vec2::new(-line.y, line.x).normalize_or_zero()
        } else {
            Vec2::ZERO
        };
        let events = encounter.step(
            Intent::player(direction, false, press),
            WorldContact::ground_only(ground),
        );
        dodged |= press;
        hit |= events.any(|event| {
            matches!(
                event,
                CombatEvent::Hit {
                    victim: Side::Player,
                    ..
                }
            )
        });
        if swung && swing_tick.is_none() {
            break;
        }
    }
    Ok(swung.then_some(!hit))
}

/// Separations a reactive-evasion measurement is taken at, in world units:
/// from just outside the closest the two bodies can stand (their capsules sum
/// to about `1.47`) to the far edge of the player's reach.
pub const EVASION_SEPARATIONS: [f32; 6] = [1.5, 1.7, 2.0, 2.3, 2.6, 2.85];

/// The response-dodge candidates Combat Pressure was asked to measure: the
/// player's own dodge as the control, the two shorter sidesteps the owner
/// named, and one faster correction derived from the measurement.
#[must_use]
pub fn response_dodge_candidates() -> [(&'static str, AuthoredDodge); 4] {
    let cooldown_seconds = fixture::dodge().cooldown_seconds;
    [
        ("36 ticks / 2.2 u (control)", fixture::dodge()),
        (
            "24 ticks / 1.5 u",
            AuthoredDodge {
                duration_seconds: 24.0 / 120.0,
                cooldown_seconds,
                distance: 1.5,
            },
        ),
        (
            "20 ticks / 1.2 u",
            AuthoredDodge {
                duration_seconds: 20.0 / 120.0,
                cooldown_seconds,
                distance: 1.2,
            },
        ),
        (
            "20 ticks / 1.6 u (derived, faster)",
            AuthoredDodge {
                duration_seconds: 20.0 / 120.0,
                cooldown_seconds,
                distance: 1.6,
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        EVASION_SEPARATIONS, ORACLE_FIGHT_TICKS, ORACLE_MISJUDGEMENT, ORACLE_SEEDS, OraclePolicy,
        oracle_setup, probe_reactive_evasion, response_dodge_candidates, run_fight,
    };
    use crate::combatant::Side;
    use crate::fixture;

    fn fights(policy: impl Fn(usize) -> OraclePolicy) -> Vec<super::FightReport> {
        let ground = fixture::golden_ground();
        ORACLE_SEEDS
            .iter()
            .enumerate()
            .map(|(index, seed)| {
                match run_fight(
                    &oracle_setup(*seed),
                    &ground,
                    policy(index),
                    ORACLE_FIGHT_TICKS,
                ) {
                    Ok(report) => report,
                    Err(error) => panic!("{error}"),
                }
            })
            .collect()
    }

    fn print(name: &str, reports: &[super::FightReport]) {
        for report in reports {
            let c = &report.counters;
            println!(
                "{name}: winner={:?} ticks={} php={} ahp={} p swings/hits/whiffs={}/{}/{} a swings/hits={}/{} a dodges={} quiet={}",
                report.winner,
                report.ticks,
                report.player_health,
                report.adversary_health,
                c.swings[0],
                c.hits[0],
                c.whiffs[0],
                c.swings[1],
                c.hits[1],
                c.dodges[1],
                report.longest_quiet,
            );
        }
    }

    /// KI-039 on the M9 branch, as repository evidence: the owner's natural
    /// strategy beats the historical encounter under every oracle seed without
    /// taking a single hit. If a change to the encounter makes this fail, that
    /// change is exactly what Combat Pressure is looking for — update this test
    /// deliberately and say why, never to get green.
    #[test]
    fn owner_spam_beats_the_historical_encounter_every_time_untouched() {
        let reports = fights(|index| OraclePolicy::OwnerSpam {
            misjudgement: ORACLE_MISJUDGEMENT[index],
        });
        print("owner-spam", &reports);
        for report in &reports {
            assert_eq!(report.winner, Some(Side::Player), "{report:?}");
            assert_eq!(report.counters.hits[1], 0, "the adversary landed a hit");
            assert_eq!(report.counters.dodges[0], 0, "owner-spam never dodges");
        }
    }

    /// The control: the fight M6 designed also wins every time, and it wins by
    /// dodging the adversary's telegraph rather than by trading.
    #[test]
    fn intentional_play_beats_the_historical_encounter_every_time() {
        let reports = fights(|_| OraclePolicy::Intentional);
        print("intentional", &reports);
        for report in &reports {
            assert_eq!(report.winner, Some(Side::Player), "{report:?}");
            assert_eq!(report.counters.hits[1], 0, "the adversary landed a hit");
            assert!(report.counters.dodges[0] > 0, "it never dodged");
            assert!(
                report.counters.swings[1] > 0,
                "it never let the adversary commit"
            );
        }
    }

    /// The Combat Pressure fairness cliff. Dodging sideways after a reaction of
    /// `18` ticks escapes the player's swing from every separation; by `24`
    /// ticks — `200` ms, the low end of the reaction delay Combat Pressure was
    /// asked to reach — none of the owner's candidates escapes from any. The
    /// player's windup is `22` ticks, so a reaction of `24` starts after the
    /// blade is already live, and a shorter dodge at the same speed cannot
    /// change that: duration was never the constraint.
    #[test]
    fn a_reactive_sidestep_escapes_the_players_swing_only_below_a_human_reaction_time() {
        let ground = fixture::golden_ground();
        let escapes = |dodge, reaction| {
            EVASION_SEPARATIONS
                .iter()
                .filter(|separation| {
                    match probe_reactive_evasion(dodge, reaction, **separation, &ground) {
                        Ok(Some(escaped)) => escaped,
                        Ok(None) => panic!("the swing never started at {separation}"),
                        Err(error) => panic!("{error}"),
                    }
                })
                .count()
        };
        for (name, dodge) in response_dodge_candidates() {
            let row: Vec<usize> = [18, 20, 22, 24, 27, 30]
                .iter()
                .map(|reaction| escapes(dodge, *reaction))
                .collect();
            println!("{name}: escapes of 6 at R 18/20/22/24/27/30 = {row:?}");
            assert_eq!(
                row[0],
                EVASION_SEPARATIONS.len(),
                "{name} must escape at 18"
            );
            assert_eq!(row[4], 0, "{name} escaped at 27");
            assert_eq!(row[5], 0, "{name} escaped at 30");
            if !name.contains("derived") {
                assert_eq!(row[3], 0, "{name} escaped at 24");
            }
        }
    }
}
