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
//!
//! **The M9 weapon-choice laboratory** lives here too, because it is this
//! clearing's fight and nothing else: the same bodies, the same lunge and the
//! same adversary, with the M9 exchange offered at a QA point beside the
//! player's round start so a person can change weapon between rounds. It is
//! test infrastructure for `OWNER PLAYTEST — WEAPON CHOICE MATTERS (REVISIT)`,
//! not a second product reward site: the product exchange stays at the gate
//! (`crate::reward`), and nothing in the world marks this point. See
//! `docs/planning/M9_MEANINGFUL_REWARD.md`.

use glam::Vec2;

use veldwake_character::GroundSampler;
use veldwake_combat::{EncounterSetup, PlayerVictoryPolicy, Side, fixture};

use crate::arena;
use crate::reward::RewardSite;
use crate::traversal;

/// How far from the adversary the player starts, in world units.
///
/// Outside the lunge's band, so the first thing a player meets is the approach
/// and the choice of how to make it.
pub const START_SEPARATION: f32 = 8.0;

/// Where the weapon-choice exchange point stands, from the player's round start,
/// in world units.
///
/// To the player's **left**: the player faces the adversary down `-Z` with its
/// weapon hand on `+X`, and the follow camera sits behind and to the right, so
/// a weapon planted on the left is neither between the camera and the body nor
/// beside the blade the body carries. `1.25` is inside the M9 interact radius
/// of `1.75`, so a body standing on its start can exchange without walking,
/// and outside the widest body's `0.86` keep-out radius, so the planted blade
/// does not stand in the body — the M9 gate's lesson, that one unit off the
/// line a body and a camera use is what keeps a planted weapon legible.
pub const WEAPON_CHOICE_SITE_OFFSET: Vec2 = Vec2::new(-1.25, 0.0);

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

/// The weapon-choice laboratory: the opt-in combat initiative fight at the
/// clearing, offering the M9 exchange. The tuning, bodies and starts are the
/// initiative session's exactly; the only addition is the reward.
#[must_use]
pub fn weapon_choice_setup() -> EncounterSetup {
    let mut setup = initiative_setup_at(InitiativeSite::Clearing);
    setup.reward = Some(fixture::reward_setup());
    setup
}

/// Where the weapon-choice exchange point stands: beside the player's round
/// start, which is where every reset puts the body back.
#[must_use]
pub fn weapon_choice_site() -> Vec2 {
    initiative_setup_at(InitiativeSite::Clearing).starts[Side::Player.index()]
        + WEAPON_CHOICE_SITE_OFFSET
}

/// The weapon-choice exchange point as a [`RewardSite`], for the renderer to
/// plant a weapon on, or `None` if the ground under it does not exist.
#[must_use]
pub fn weapon_choice_reward_site(ground: &dyn GroundSampler) -> Option<RewardSite> {
    let position = weapon_choice_site();
    let ground = ground.surface(f64::from(position.x), f64::from(position.y))?;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a column index in a finite region is a small integer"
    )]
    let column = (
        f64::from(position.x).floor() as i64,
        f64::from(position.y).floor() as i64,
    );
    Some(RewardSite {
        column,
        position,
        ground,
    })
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
    // -----------------------------------------------------------------------
    // M9 revisit — the weapon-choice laboratory
    // -----------------------------------------------------------------------

    #[test]
    fn the_weapon_choice_setup_is_the_initiative_fight_plus_the_exchange() {
        let lab = super::weapon_choice_setup();
        let initiative = initiative_setup_at(InitiativeSite::Clearing);
        assert_eq!(
            lab.tuning, initiative.tuning,
            "the fight must be the owner's"
        );
        assert_eq!(lab.starts, initiative.starts);
        assert_eq!(lab.arena, initiative.arena);
        assert_eq!(lab.player_victory, initiative.player_victory);
        assert_eq!(lab.reward, Some(fixture::reward_setup()));
        assert!(
            initiative.reward.is_none(),
            "the initiative session stays M9-free"
        );
    }

    /// Where the QA exchange point stands: close enough to the round start to
    /// use without walking, clear of the body, off the line the fight is
    /// fought along, on the same level dry ground, and nowhere near the
    /// product site at the gate.
    #[test]
    fn the_weapon_choice_point_is_beside_the_start_and_out_of_the_fight() {
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let setup = super::weapon_choice_setup();
        let start = setup.starts[Side::Player.index()];
        let adversary = setup.starts[Side::Adversary.index()];
        let point = super::weapon_choice_site();
        let radius = fixture::reward_setup().interact_radius;
        let from_start = (point - start).length();
        assert!(
            from_start < radius - 0.25,
            "{from_start} is not comfortably inside the interact radius {radius}"
        );
        assert!(
            from_start > 0.86,
            "{from_start}: the planted blade is in the body"
        );
        // Distance from the segment the two bodies start on.
        let line = adversary - start;
        let along = ((point - start).dot(line) / line.length_squared()).clamp(0.0, 1.0);
        let off_line = (point - (start + line * along)).length();
        assert!(
            off_line >= 1.0,
            "{off_line}: the point is on the fight's line"
        );
        let (x, z) = (f64::from(point.x), f64::from(point.y));
        assert!(legality.walkable(x, z), "the point is not walkable ground");
        let Some(site) = super::weapon_choice_reward_site(&ground) else {
            panic!("no ground under the laboratory point");
        };
        let start_ground = ground
            .surface(f64::from(start.x), f64::from(start.y))
            .unwrap_or(f64::NAN);
        assert!(
            (site.ground - start_ground).abs() < 0.5,
            "the point is not on the start's level: {} against {start_ground}",
            site.ground
        );
        let Some(gate) = crate::reward::site(&generator) else {
            panic!("the golden world must have its product site");
        };
        assert!(
            (gate.position - point).length() > 30.0,
            "the laboratory point could be mistaken for the gate's"
        );
    }

    /// The round loop the owner's protocol depends on, in the real encounter
    /// over the real world: take the found weapon at the laboratory point,
    /// lose the round, and come back beside the point still holding it —
    /// then put it back with one more press.
    #[test]
    fn a_round_reset_puts_the_body_back_beside_the_point_still_armed() {
        use veldwake_combat::{Encounter, Intent, WeaponVariant};
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let setup = super::weapon_choice_setup();
        let point = super::weapon_choice_site();
        let world = WorldContact::terrain_with_weapon_exchange(&ground, &legality, point);
        let mut encounter = match Encounter::new(&setup, Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        encounter.arm();
        let press = Intent::idle().interacting(true);
        let _ = encounter.step(press, world);
        assert_eq!(encounter.armament().player(), WeaponVariant::Found);
        // Stand still until the adversary wins the round and the hold resets it.
        let mut resets = 0;
        for _ in 0..20_000 {
            let _ = encounter.step(Intent::idle(), world);
            resets = encounter.counters().resets;
            if resets > 0 && encounter.combatant(Side::Player).can_act() {
                break;
            }
        }
        assert!(resets > 0, "the round never reset");
        assert_eq!(
            encounter.armament().player(),
            WeaponVariant::Found,
            "the reset took the weapon back (ARM-001)"
        );
        let back = encounter.combatant(Side::Player).position();
        assert!(
            (back - setup.starts[Side::Player.index()]).length() < 1.0e-4,
            "the body did not return to its start"
        );
        let _ = encounter.step(press, world);
        assert_eq!(
            encounter.armament().player(),
            WeaponVariant::Original,
            "the point was out of reach after the reset"
        );
    }

    /// The pre-gate's terrain table: both weapons, every policy family, in the
    /// laboratory exactly as the owner will play it — the clearing, the
    /// round start, the adversary eight units away, the exchange beside the
    /// start — under the six oracle seeds.
    #[test]
    #[ignore = "measurement: prints the weapon-choice laboratory table on the golden world"]
    fn measure_both_weapons_in_the_weapon_choice_laboratory() {
        use veldwake_combat::WeaponVariant;
        use veldwake_combat::oracle::{OracleFamily, run_fight_holding};
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let point = super::weapon_choice_site();
        let world = WorldContact::terrain_with_weapon_exchange(&ground, &legality, point);
        for family in OracleFamily::all() {
            let name = family.name();
            for holding in [WeaponVariant::Original, WeaponVariant::Found] {
                let reports: Vec<FightReport> = ORACLE_SEEDS
                    .iter()
                    .enumerate()
                    .map(|(index, seed)| {
                        let mut setup = super::weapon_choice_setup();
                        setup.player_victory = PlayerVictoryPolicy::Remain;
                        setup.tuning.seed = *seed;
                        match run_fight_holding(
                            &setup,
                            world,
                            family.policy(index),
                            ORACLE_FIGHT_TICKS,
                            holding,
                        ) {
                            Ok(report) => report,
                            Err(error) => panic!("{name}: {error}"),
                        }
                    })
                    .collect();
                let t = tally(&reports);
                let ticks: u32 = reports.iter().map(|r| r.ticks).sum();
                let cut: u32 = reports.iter().map(|r| r.pressure_interrupted).sum();
                let committed: u32 = reports.iter().map(|r| r.hits_taken_committed).sum();
                let whiffs: u32 = reports.iter().map(|r| r.counters.whiffs[0]).sum();
                println!(
                    "{} | ticks {ticks} whiffs {whiffs} lunge-cut {cut} hit-while-committed {committed}",
                    describe(&format!("lab {name} {}", holding.name()), &t)
                );
                assert_eq!(t.unresolved, 0, "{name}: a spacing dodge met a live swing");
                assert_eq!(t.stuck, 0, "{name}: KI-038 at the clearing");
            }
        }
    }

    #[test]
    #[ignore = "measurement: prints the driven fights' event schedule at the clearing"]
    fn print_the_driven_schedule_at_the_clearing() {
        use crate::encounter::InitiativeDriver;
        use veldwake_combat::{CombatEvent, Encounter, WeaponVariant};
        let generator = TerrainGenerator::golden();
        let ground = TerrainGround::new(&generator);
        let legality = TerrainWalkability::new(&generator);
        let point = super::weapon_choice_site();
        // The initiative session, then the M9 weapon-choice laboratory holding
        // each weapon: the same fight with the exchange on offer, the found
        // weapon taken through the real rule on the first armed tick.
        let sessions = [
            ("initiative", None),
            ("weapon-choice", Some(WeaponVariant::Original)),
            ("weapon-choice+found", Some(WeaponVariant::Found)),
        ];
        for (session, holding) in sessions {
            let world = match holding {
                Some(_) => WorldContact::terrain_with_weapon_exchange(&ground, &legality, point),
                None => WorldContact::terrain(&ground, &legality),
            };
            for driver in [
                InitiativeDriver::Read,
                InitiativeDriver::Spam,
                InitiativeDriver::SpamRead,
            ] {
                let Some(policy) = driver.policy() else {
                    continue;
                };
                let setup = match holding {
                    Some(_) => super::weapon_choice_setup(),
                    None => initiative_setup_at(InitiativeSite::Clearing),
                };
                let mut encounter = match Encounter::new(&setup, Some(&ground)) {
                    Ok(encounter) => encounter,
                    Err(error) => panic!("{error}"),
                };
                encounter.arm();
                let mut first = holding == Some(WeaponVariant::Found);
                for _ in 0..2_400 {
                    let intent = policy.intent(&encounter).interacting(first);
                    first = false;
                    let events = encounter.step(intent, world);
                    for event in events.iter() {
                        let (name, side) = match event {
                            CombatEvent::SwingStarted { side, .. } => ("swing", side),
                            CombatEvent::SwingActive { side, .. } => ("active", side),
                            CombatEvent::SwingWhiffed { side, .. } => ("whiff", side),
                            CombatEvent::DodgeStarted { side, .. } => ("dodge", side),
                            CombatEvent::Hit { attacker, .. } => ("hit", attacker),
                            CombatEvent::Defeated { side } => ("defeated", side),
                            CombatEvent::EncounterReset => ("reset", Side::Player),
                            CombatEvent::ArmamentSwapped { .. } => ("swapped", Side::Player),
                            _ => continue,
                        };
                        println!(
                            "{session}:{} t{} {name} {} kind {:?} d {:.2} weapon {}",
                            driver.name(),
                            encounter.tick_index(),
                            side.name(),
                            encounter
                                .combatant(side)
                                .action()
                                .attack_kind()
                                .map(|kind| kind.name()),
                            encounter.separation_distance(),
                            encounter.armament().player().name(),
                        );
                    }
                }
            }
        }
    }
}
