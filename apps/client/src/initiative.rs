//! Combat initiative in the golden region: where the opt-in fight stands, and
//! the real-terrain evidence that the capability survives the world.
//!
//! `veldwake-combat` proves the pressure lunge, the spacing dodge and the
//! outcome-dependent recovery on flat ground. Flat ground is a diagnosis. This
//! module runs the same oracle policies over the real adapters —
//! [`crate::character::TerrainGround`] and [`crate::traversal::TerrainWalkability`] over the golden world — in two
//! places that answer two different questions:
//!
//! - **the open validation site** is M6's arena clearing, the discovery
//!   overlook: level, dry and open, with no landmark near. It answers *does the
//!   capability work in the world*, and it is where the opt-in QA session
//!   stands;
//! - **the spire** is where the M8 adversary actually waits, with landmark stone
//!   around it. It answers *what the stone does to it*, and the approach from the
//!   east is KI-038's witness, reported separately and never counted as combat.
//!
//! Nothing here moves the M8 adversary, changes the world or builds navigation.
//! See `docs/planning/COMBAT_INITIATIVE.md`.

use glam::Vec2;

use veldwake_combat::{EncounterSetup, PlayerVictoryPolicy, fixture};

use crate::arena;
use crate::traversal;

/// How far from the adversary the player starts, in world units.
///
/// Outside the lunge's band, so the first thing a player meets is the approach
/// and the choice of how to make it.
pub const START_SEPARATION: f32 = 8.0;

/// Environment variable choosing where a combat initiative session stands.
const SITE_VARIABLE: &str = "VELDWAKE_INITIATIVE_SITE";

/// Where a combat initiative session stands.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InitiativeSite {
    /// The open clearing. The default, and the owner's.
    #[default]
    Clearing,
    /// The M8 adversary's own column at the spire, with the player coming from
    /// the west so the spire's stone stands behind the adversary: the witness
    /// for a spacing dodge the world refuses. QA only.
    SpireWest,
}

impl InitiativeSite {
    /// Reads `VELDWAKE_INITIATIVE_SITE`: `clearing` (the default) or
    /// `spire-west`. Anything else warns and stands at the clearing.
    #[must_use]
    pub fn from_environment() -> Self {
        match std::env::var(SITE_VARIABLE) {
            Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
                "" | "clearing" => Self::Clearing,
                "spire-west" => Self::SpireWest,
                other => {
                    tracing::warn!(
                        value = other,
                        "unknown VELDWAKE_INITIATIVE_SITE; using the clearing"
                    );
                    Self::Clearing
                }
            },
            Err(_) => Self::Clearing,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clearing => "clearing",
            Self::SpireWest => "spire-west",
        }
    }
}

/// The opt-in combat initiative encounter: the historical bodies, weapon and
/// tuning plus the pressure lunge, at a site — the open clearing unless QA asks
/// for the spire's witness.
///
/// No arena disc — a boundary would decide how far a body can back off, which
/// is part of the capability — and a defeat on either side starts another round,
/// so the owner can fight it as many times as a session allows.
#[must_use]
pub fn initiative_setup_at(site: InitiativeSite) -> EncounterSetup {
    let mut setup = fixture::golden_setup();
    setup.tuning = fixture::initiative_tuning();
    setup.starts = match site {
        InitiativeSite::Clearing => {
            let centre = arena::centre();
            [
                centre + Vec2::new(0.0, START_SEPARATION * 0.5),
                centre - Vec2::new(0.0, START_SEPARATION * 0.5),
            ]
        }
        InitiativeSite::SpireWest => {
            let adversary = traversal::column_centre(
                traversal::ADVERSARY_COLUMN.0,
                traversal::ADVERSARY_COLUMN.1,
            );
            [adversary + Vec2::new(-START_SEPARATION, 0.0), adversary]
        }
    };
    setup.arena = None;
    setup.player_victory = PlayerVictoryPolicy::ResetEncounter;
    setup
}

#[cfg(test)]
mod tests {
    use super::{InitiativeSite, initiative_setup_at};
    use crate::arena;
    use crate::character::TerrainGround;
    use crate::traversal::{ADVERSARY_COLUMN, TerrainWalkability, column_centre};
    use glam::Vec2;
    use veldwake_character::GroundSampler;
    use veldwake_combat::oracle::{
        FightReport, ORACLE_FIGHT_TICKS, ORACLE_MISJUDGEMENT, ORACLE_SEEDS, OraclePolicy, run_fight,
    };
    use veldwake_combat::{EncounterSetup, PlayerVictoryPolicy, Side, TraversalLegality};
    use veldwake_combat::{WorldContact, fixture};
    use veldwake_procedural::TerrainGenerator;

    /// One approach: where the adversary waits and where the player starts.
    struct Approach {
        name: &'static str,
        adversary: Vec2,
        player: Vec2,
    }

    fn approaches_around(site: Vec2, distance: f32) -> [(&'static str, Vec2); 4] {
        [
            ("north", site + Vec2::new(0.0, -distance)),
            ("south", site + Vec2::new(0.0, distance)),
            ("east", site + Vec2::new(distance, 0.0)),
            ("west", site + Vec2::new(-distance, 0.0)),
        ]
    }

    fn setup_for(approach: &Approach, seed: veldwake_combat::CombatSeed) -> EncounterSetup {
        let mut setup = initiative_setup_at(InitiativeSite::Clearing);
        setup.starts = [approach.player, approach.adversary];
        setup.player_victory = PlayerVictoryPolicy::Remain;
        setup.tuning.seed = seed;
        setup
    }

    fn side(index: usize) -> f32 {
        if index.is_multiple_of(2) { 1.0 } else { -1.0 }
    }

    struct Tally {
        wins: usize,
        losses: usize,
        draws: usize,
        stuck: usize,
        health: u32,
        punishes: u32,
        lunges: u32,
        primaries: u32,
        spacing: u32,
        spacing_truncated: u32,
        spacing_blocked: u32,
        spacing_slid: u32,
        unresolved: u32,
    }

    fn tally(reports: &[FightReport]) -> Tally {
        let combat: Vec<&FightReport> = reports.iter().filter(|r| !r.stuck()).collect();
        Tally {
            wins: combat
                .iter()
                .filter(|r| r.winner == Some(Side::Player))
                .count(),
            losses: combat
                .iter()
                .filter(|r| r.winner == Some(Side::Adversary))
                .count(),
            draws: combat.iter().filter(|r| r.winner.is_none()).count(),
            stuck: reports.len() - combat.len(),
            health: combat.iter().map(|r| u32::from(r.player_health)).sum(),
            punishes: combat.iter().map(|r| r.punishes).sum(),
            lunges: combat.iter().map(|r| r.counters.pressure_swings).sum(),
            primaries: combat.iter().map(|r| r.adversary_primary_swings()).sum(),
            spacing: reports.iter().map(|r| r.counters.dodges[1]).sum(),
            spacing_truncated: reports.iter().map(|r| r.counters.dodges_truncated[1]).sum(),
            spacing_blocked: reports
                .iter()
                .map(|r| r.counters.dodge_blocked_moves[1])
                .sum(),
            spacing_slid: reports.iter().map(|r| r.counters.dodge_slid_moves[1]).sum(),
            unresolved: reports
                .iter()
                .map(|r| r.counters.dodges_during_unresolved_swing[1])
                .sum(),
        }
    }

    fn describe(label: &str, t: &Tally) -> String {
        format!(
            "{label}: W/L/D {}/{}/{} stuck {} hp {} punish {} lunges {} primary {} spacing {} (truncated {} blocked {} slid {}) unresolved {}",
            t.wins,
            t.losses,
            t.draws,
            t.stuck,
            t.health,
            t.punishes,
            t.lunges,
            t.primaries,
            t.spacing,
            t.spacing_truncated,
            t.spacing_blocked,
            t.spacing_slid,
            t.unresolved
        )
    }

    fn fights(
        world: WorldContact<'_>,
        approach: &Approach,
        policy: impl Fn(usize) -> OraclePolicy,
    ) -> Vec<FightReport> {
        ORACLE_SEEDS
            .iter()
            .enumerate()
            .map(|(index, seed)| {
                match run_fight(
                    &setup_for(approach, *seed),
                    world,
                    policy(index),
                    ORACLE_FIGHT_TICKS,
                ) {
                    Ok(report) => report,
                    Err(error) => panic!("{}: {error}", approach.name),
                }
            })
            .collect()
    }

    fn spam(index: usize) -> OraclePolicy {
        OraclePolicy::OwnerSpam {
            misjudgement: ORACLE_MISJUDGEMENT[index],
        }
    }

    fn spam_read(index: usize) -> OraclePolicy {
        OraclePolicy::SpamRead {
            misjudgement: ORACLE_MISJUDGEMENT[index],
            lag: 24,
            side: side(index),
        }
    }

    fn read(index: usize) -> OraclePolicy {
        OraclePolicy::Read {
            lag: 24,
            walk_only: false,
            side: side(index),
        }
    }

    /// Every start must be somewhere a body may stand, or the approach is not
    /// an approach.
    fn assert_standable(
        ground: &TerrainGround<'_>,
        legality: &TerrainWalkability<'_>,
        approach: &Approach,
    ) {
        for point in [approach.player, approach.adversary] {
            let (x, z) = (f64::from(point.x), f64::from(point.y));
            assert!(
                ground.surface(x, z).is_some() && legality.walkable(x, z),
                "{}: {point:?} is not standable",
                approach.name
            );
        }
    }

    #[test]
    fn the_opt_in_setup_is_the_historical_fight_plus_the_lunge_at_the_clearing() {
        let setup = initiative_setup_at(InitiativeSite::Clearing);
        let historical = fixture::golden_setup();
        assert!(setup.tuning.adversary_pressure.is_some());
        assert_eq!(
            setup.tuning.player_attack, historical.tuning.player_attack,
            "the player's attack is historical"
        );
        assert_eq!(
            setup.tuning.adversary_attack, historical.tuning.adversary_attack,
            "the adversary's primary is historical"
        );
        assert_eq!(setup.tuning.dodge, historical.tuning.dodge);
        assert!(setup.arena.is_none());
        let centre = arena::centre();
        let mid = (setup.starts[0] + setup.starts[1]) * 0.5;
        assert!(
            (mid - centre).length() < 1.0e-5,
            "the fight is centred on the clearing"
        );
        assert!(
            (setup.starts[0] - setup.starts[1]).length() > 4.6,
            "it starts outside the band"
        );
    }

    /// The real-terrain gate at the open clearing: four approaches, six seeds,
    /// the three policies, the real adapters.
    #[test]
    fn at_the_open_clearing_reading_beats_spam_and_spam_loses_more_than_it_wins() {
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &legality);
        let site = arena::centre();
        let (mut spam_wins, mut spam_losses) = (0, 0);
        for (name, player) in approaches_around(site, super::START_SEPARATION) {
            let approach = Approach {
                name,
                adversary: site,
                player,
            };
            assert_standable(&ground, &legality, &approach);
            let spam = tally(&fights(world, &approach, spam));
            let spam_read = tally(&fights(world, &approach, spam_read));
            let read = tally(&fights(world, &approach, read));
            println!("{}", describe(&format!("open {name} owner-spam"), &spam));
            println!(
                "{}",
                describe(&format!("open {name} spam-read"), &spam_read)
            );
            println!("{}", describe(&format!("open {name} read"), &read));
            for t in [&spam, &spam_read, &read] {
                assert_eq!(t.stuck, 0, "{name}: KI-038 has no business at the clearing");
                assert_eq!(t.draws, 0, "{name}: a stalemate");
                assert_eq!(t.unresolved, 0, "{name}: a spacing dodge met a live swing");
            }
            assert!(spam.wins <= 2, "{name}: owner-spam won {} of 6", spam.wins);
            assert!(read.wins >= 5, "{name}: read won only {} of 6", read.wins);
            assert!(read.health >= spam_read.health && spam_read.health >= spam.health);
            assert!(spam.lunges > 0 && read.punishes > 0);
            spam_wins += spam.wins;
            spam_losses += spam.losses;
        }
        assert!(
            spam_wins < spam_losses,
            "owner-spam won {spam_wins} and lost {spam_losses} at the clearing"
        );
    }

    /// The same policies where the adversary actually stands, with the spire's
    /// stone four and a half units east of it.
    ///
    /// Three approaches are combat evidence: the derived route's own last leg,
    /// which arrives from the south-east; north; and south. Two are witnesses
    /// and are reported, never counted as combat: **west**, where the player
    /// comes from the open side and the adversary's spacing dodge — straight
    /// away from the player — goes into the stone behind it; and **east**,
    /// where the stone stands between the two bodies and the adversary walks
    /// into it (KI-038).
    #[test]
    fn at_the_spire_the_capability_holds_where_the_stone_leaves_it_room() {
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &legality);
        let site = column_centre(ADVERSARY_COLUMN.0, ADVERSARY_COLUMN.1);
        let distance = super::START_SEPARATION;
        let route_leg = (Vec2::new(-109.5, 90.5) - site).normalize_or_zero();
        let combat = [
            ("route", site + route_leg * distance),
            ("north", site + Vec2::new(0.0, -distance)),
            ("south", site + Vec2::new(0.0, distance)),
        ];
        let (mut spam_wins, mut spam_losses) = (0, 0);
        for (name, player) in combat {
            let approach = Approach {
                name,
                adversary: site,
                player,
            };
            assert_standable(&ground, &legality, &approach);
            let spam = tally(&fights(world, &approach, spam));
            let read = tally(&fights(world, &approach, read));
            println!("{}", describe(&format!("spire {name} owner-spam"), &spam));
            println!("{}", describe(&format!("spire {name} read"), &read));
            for t in [&spam, &read] {
                assert_eq!(t.stuck, 0, "{name}: stuck against stone");
                assert_eq!(t.draws, 0, "{name}: a stalemate");
                assert_eq!(t.unresolved, 0, "{name}: a spacing dodge met a live swing");
            }
            assert!(spam.wins <= 2, "{name}: owner-spam won {} of 6", spam.wins);
            assert!(read.wins >= 5, "{name}: read won only {} of 6", read.wins);
            spam_wins += spam.wins;
            spam_losses += spam.losses;
        }
        assert!(spam_wins < spam_losses);
        println!("spire combat approaches, owner-spam: {spam_wins} won, {spam_losses} lost");

        for (name, player) in [
            (
                "west (stone behind the adversary)",
                site + Vec2::new(-distance, 0.0),
            ),
            ("east (KI-038: stone between)", site + Vec2::new(16.0, 0.0)),
        ] {
            let approach = Approach {
                name,
                adversary: site,
                player,
            };
            assert_standable(&ground, &legality, &approach);
            let spam = tally(&fights(world, &approach, spam));
            let read = tally(&fights(world, &approach, read));
            println!("{}", describe(&format!("witness {name} owner-spam"), &spam));
            println!("{}", describe(&format!("witness {name} read"), &read));
            assert_eq!(spam.unresolved + read.unresolved, 0);
        }
    }

    #[test]
    fn the_spire_witness_puts_the_stone_behind_the_adversary() {
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let setup = initiative_setup_at(InitiativeSite::SpireWest);
        let adversary = column_centre(ADVERSARY_COLUMN.0, ADVERSARY_COLUMN.1);
        assert_eq!(setup.starts[1], adversary, "the M8 adversary's own column");
        assert!(
            setup.starts[0].x < adversary.x,
            "the player comes from the west"
        );
        let approach = Approach {
            name: "spire-west",
            adversary: setup.starts[1],
            player: setup.starts[0],
        };
        assert_standable(&ground, &legality, &approach);
        // East of the adversary, a spacing dodge's length away, is stone.
        let behind = adversary + Vec2::new(3.0, 0.0);
        assert!(
            !legality.walkable(f64::from(behind.x), f64::from(behind.y)),
            "the witness needs stone behind the adversary"
        );
    }

    /// The schedule a driven session at the clearing follows from arming, for
    /// choosing `initiative:<driver>@<tick>` captures. Deterministic, so the
    /// client frozen at a tick shows exactly what this prints for it.
    #[test]
    #[ignore = "measurement: prints the driven fights' event schedule at the clearing"]
    fn print_the_driven_schedule_at_the_clearing() {
        use crate::encounter::InitiativeDriver;
        use veldwake_combat::{CombatEvent, Encounter};
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let world = WorldContact::terrain(&ground, &legality);
        for driver in [
            InitiativeDriver::Read,
            InitiativeDriver::Spam,
            InitiativeDriver::SpamRead,
        ] {
            let Some(policy) = driver.policy() else {
                continue;
            };
            let setup = initiative_setup_at(InitiativeSite::Clearing);
            let mut encounter = match Encounter::new(&setup, Some(&ground)) {
                Ok(encounter) => encounter,
                Err(error) => panic!("{error}"),
            };
            encounter.arm();
            for _ in 0..2_400 {
                let events = encounter.step(policy.intent(&encounter), world);
                for event in events.iter() {
                    let (name, side) = match event {
                        CombatEvent::SwingStarted { side, .. } => ("swing", side),
                        CombatEvent::SwingActive { side, .. } => ("active", side),
                        CombatEvent::SwingWhiffed { side, .. } => ("whiff", side),
                        CombatEvent::DodgeStarted { side, .. } => ("dodge", side),
                        CombatEvent::Hit { attacker, .. } => ("hit", attacker),
                        CombatEvent::Defeated { side } => ("defeated", side),
                        CombatEvent::EncounterReset => ("reset", Side::Player),
                        _ => continue,
                    };
                    println!(
                        "{} t{} {name} {} kind {:?} d {:.2}",
                        driver.name(),
                        encounter.tick_index(),
                        side.name(),
                        encounter
                            .combatant(side)
                            .action()
                            .attack_kind()
                            .map(|kind| kind.name()),
                        encounter.separation_distance()
                    );
                }
            }
        }
    }
}
