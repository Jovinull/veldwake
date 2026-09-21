//! A scripted player, so an encounter can be evidence.
//!
//! Two problems this solves. A hit window is a tenth of a second, which nobody
//! can screenshot on purpose, and a capture taken months apart has to be the same
//! picture. M5 answered the same problem for a gait with eight frozen phases, one
//! run each; this is the combat version: a script drives the player, the whole
//! encounter is deterministic, and a [`NamedMoment`] is found by running the
//! script until the state it describes occurs.
//!
//! **The script reads the encounter and is still deterministic.** It has to:
//! "walk toward the adversary" is not expressible as a world direction when the
//! adversary moves. Since the simulation is a pure function of its inputs and the
//! script is a pure function of the simulation, the whole run replays exactly —
//! and because it is keyed to ticks rather than to seconds it replays identically
//! at any frame rate, which is what the partition-equivalence claim needs.

use glam::Vec2;

use crate::combatant::{Action, Intent, Side};
use crate::encounter::Encounter;
use crate::event::{CombatEvent, StepEvents};
use crate::tick::Ticks;

/// How a scripted leg moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptMotion {
    /// Stand still.
    Hold,
    /// Walk toward the adversary.
    Close,
    /// Walk away from the adversary.
    Retreat,
    /// Circle to the adversary's left.
    StrafeLeft,
    /// Circle to the adversary's right.
    StrafeRight,
}

impl ScriptMotion {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Close => "close",
            Self::Retreat => "retreat",
            Self::StrafeLeft => "strafe-left",
            Self::StrafeRight => "strafe-right",
        }
    }

    fn direction(self, encounter: &Encounter) -> Vec2 {
        let to_foe = encounter.combatant(Side::Adversary).position()
            - encounter.combatant(Side::Player).position();
        let toward = if to_foe.length_squared() > 1.0e-8 {
            to_foe.normalize()
        } else {
            Vec2::new(0.0, -1.0)
        };
        match self {
            Self::Hold => Vec2::ZERO,
            Self::Close => toward,
            Self::Retreat => -toward,
            Self::StrafeLeft => Vec2::new(-toward.y, toward.x),
            Self::StrafeRight => Vec2::new(toward.y, -toward.x),
        }
    }
}

/// When a scripted button is pressed.
///
/// Every trigger fires at most once per leg, which is what makes a scripted press
/// behave like a latched key rather than like a held one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptTrigger {
    Never,
    /// On the leg's first tick.
    OnEntry,
    /// When the adversary's telegraph is in its last third, which is the window a
    /// player reading the swing would use.
    OnLateTelegraph,
    /// When the adversary is committed and cannot answer: its recovery.
    OnAdversaryRecovery,
    /// When the adversary is inside the player's own reach and free.
    OnOpening,
}

impl ScriptTrigger {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::OnEntry => "on-entry",
            Self::OnLateTelegraph => "on-late-telegraph",
            Self::OnAdversaryRecovery => "on-adversary-recovery",
            Self::OnOpening => "on-opening",
        }
    }

    fn fires(self, encounter: &Encounter, leg_tick: Ticks, reach: f32) -> bool {
        match self {
            Self::Never => false,
            Self::OnEntry => leg_tick == 0,
            Self::OnLateTelegraph => {
                let spec = encounter.attack_spec(Side::Adversary);
                match encounter.combatant(Side::Adversary).action() {
                    Action::Attack { elapsed, .. } => {
                        *elapsed * 3 >= spec.active_start() * 2 && *elapsed < spec.active_start()
                    }
                    _ => false,
                }
            }
            Self::OnAdversaryRecovery => {
                let spec = encounter.attack_spec(Side::Adversary);
                match encounter.combatant(Side::Adversary).action() {
                    Action::Attack { elapsed, .. } => *elapsed >= spec.active_end(),
                    _ => false,
                }
            }
            Self::OnOpening => {
                encounter.separation_distance() <= reach
                    && !encounter.combatant(Side::Adversary).action().is_defeated()
            }
        }
    }
}

/// One stretch of scripted behaviour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScriptLeg {
    pub name: &'static str,
    pub ticks: Ticks,
    pub motion: ScriptMotion,
    pub attack: ScriptTrigger,
    pub dodge: ScriptTrigger,
}

/// A named reproducible encounter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncounterScript {
    pub name: &'static str,
    pub legs: &'static [ScriptLeg],
}

impl EncounterScript {
    /// Total ticks the script lasts.
    #[must_use]
    pub fn duration(&self) -> u64 {
        self.legs.iter().map(|leg| u64::from(leg.ticks)).sum()
    }
}

/// Runs a script against an encounter.
///
/// The runner's own position in the script is part of the deterministic state, so
/// two runs of the same script produce the same trace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptRunner {
    script: EncounterScript,
    leg: usize,
    leg_tick: Ticks,
    attack_fired: bool,
    dodge_fired: bool,
    /// How far the player can reach, used by [`ScriptTrigger::OnOpening`].
    reach: f32,
    finished: bool,
}

impl ScriptRunner {
    #[must_use]
    pub fn new(script: EncounterScript, reach: f32) -> Self {
        Self {
            script,
            leg: 0,
            leg_tick: 0,
            attack_fired: false,
            dodge_fired: false,
            reach,
            finished: script.legs.is_empty(),
        }
    }

    #[must_use]
    pub const fn finished(&self) -> bool {
        self.finished
    }

    /// The leg the runner is on, for a report line.
    #[must_use]
    pub fn leg_name(&self) -> &'static str {
        self.script
            .legs
            .get(self.leg)
            .map_or("finished", |leg| leg.name)
    }

    #[must_use]
    pub const fn leg_tick(&self) -> Ticks {
        self.leg_tick
    }

    /// The intent for the tick about to run, advancing the script by one tick.
    ///
    /// A finished script holds: the encounter keeps running, which is what a
    /// capture waiting for a settle needs.
    pub fn next_intent(&mut self, encounter: &Encounter) -> Intent {
        let Some(leg) = self.script.legs.get(self.leg).copied() else {
            self.finished = true;
            return Intent::idle();
        };
        let attack = !self.attack_fired && leg.attack.fires(encounter, self.leg_tick, self.reach);
        let dodge =
            !attack && !self.dodge_fired && leg.dodge.fires(encounter, self.leg_tick, self.reach);
        if attack {
            self.attack_fired = true;
        }
        if dodge {
            self.dodge_fired = true;
        }
        let intent = Intent::player(leg.motion.direction(encounter), attack, dodge);

        self.leg_tick = self.leg_tick.saturating_add(1);
        if self.leg_tick >= leg.ticks {
            self.leg = self.leg.saturating_add(1);
            self.leg_tick = 0;
            self.attack_fired = false;
            self.dodge_fired = false;
            if self.leg >= self.script.legs.len() {
                self.finished = true;
            }
        }
        intent
    }

    /// Starts the script again from its first leg.
    pub fn restart(&mut self) {
        self.leg = 0;
        self.leg_tick = 0;
        self.attack_fired = false;
        self.dodge_fired = false;
        self.finished = self.script.legs.is_empty();
    }
}

/// A frame of the encounter worth capturing.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MomentKind {
    /// Both standing, apart, before anything has happened.
    Faceoff,
    /// The adversary's telegraph has just begun.
    AdversaryTelegraphEarly,
    /// The adversary's telegraph is about to end.
    AdversaryTelegraphLate,
    /// The player's blade is rising.
    PlayerAnticipation,
    /// The player's blade can connect.
    PlayerActive,
    /// The tick a player's hit landed.
    ConfirmedHit,
    /// The peak of the adversary's recoil.
    HitReaction,
    /// The adversary's blade can connect.
    AdversaryActive,
    /// The player is still dodging and the adversary's swing has gone all the
    /// way past its active window without touching it.
    ///
    /// The "successful" is load-bearing and was not always checked. The first
    /// version of this asked only whether the player was dodging while the
    /// blade happened to be live, which is a different and much weaker claim:
    /// branch QA captured the moment and found the player staggered ten ticks
    /// later, so the frame labelled *avoidance as geometry* was a frame of a
    /// dodge that failed. A swing that has run out its whole active window
    /// without hitting the dodger is a swing that missed.
    SuccessfulDodge,
    /// The tick the player was hit.
    PlayerHit,
    /// A body has run out of health.
    Defeat,
}

/// One named moment, with what a capture of it is meant to show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedMoment {
    pub name: &'static str,
    pub kind: MomentKind,
    pub intent: &'static str,
}

/// Every moment the combat evidence is captured at.
pub const NAMED_MOMENTS: &[NamedMoment] = &[
    NamedMoment {
        name: "faceoff",
        kind: MomentKind::Faceoff,
        intent: "two bodies that read as opponents: distinguishable, armed, apart",
    },
    NamedMoment {
        name: "telegraph-early",
        kind: MomentKind::AdversaryTelegraphEarly,
        intent: "the first frame of the warning: is the intent already visible?",
    },
    NamedMoment {
        name: "telegraph-late",
        kind: MomentKind::AdversaryTelegraphLate,
        intent: "the last frame before the blade moves, which is the decision point",
    },
    NamedMoment {
        name: "player-anticipation",
        kind: MomentKind::PlayerAnticipation,
        intent: "the player's own windup: commitment the player can feel",
    },
    NamedMoment {
        name: "player-active",
        kind: MomentKind::PlayerActive,
        intent: "the cut in flight, where the blade is where the hit is",
    },
    NamedMoment {
        name: "confirmed-hit",
        kind: MomentKind::ConfirmedHit,
        intent: "the contact frame: does the blade visibly touch the body it hurt?",
    },
    NamedMoment {
        name: "hit-reaction",
        kind: MomentKind::HitReaction,
        intent: "the recoil, which is what makes a hit read as a consequence",
    },
    NamedMoment {
        name: "adversary-active",
        kind: MomentKind::AdversaryActive,
        intent: "the adversary's blade out, so its reach can be judged against its telegraph",
    },
    NamedMoment {
        name: "successful-dodge",
        kind: MomentKind::SuccessfulDodge,
        intent: "the player out of the way while the blade is live: avoidance as geometry",
    },
    NamedMoment {
        name: "player-hit",
        kind: MomentKind::PlayerHit,
        intent: "the player taking a hit, which has to read as clearly as landing one",
    },
    NamedMoment {
        name: "defeat",
        kind: MomentKind::Defeat,
        intent: "the end of the loop: is a defeat legible, or does a body just stop?",
    },
];

/// The named moment with this name, if there is one.
#[must_use]
pub fn moment(name: &str) -> Option<&'static NamedMoment> {
    let wanted = name.trim().to_ascii_lowercase();
    NAMED_MOMENTS
        .iter()
        .find(|moment| moment.name.eq_ignore_ascii_case(&wanted))
}

/// Whether the encounter is at a named moment right now.
///
/// A pure predicate over the state and the tick's events, so a client can freeze
/// on the first occurrence and a test can find it without a GPU.
#[must_use]
pub fn at_moment(kind: MomentKind, encounter: &Encounter, events: &StepEvents) -> bool {
    let player = encounter.combatant(Side::Player);
    let adversary = encounter.combatant(Side::Adversary);
    match kind {
        MomentKind::Faceoff => {
            player.action().is_free()
                && adversary.action().is_free()
                && encounter.separation_distance() > encounter.tuning().adversary().strike_range()
        }
        MomentKind::AdversaryTelegraphEarly => {
            let spec = encounter.attack_spec(Side::Adversary);
            matches!(adversary.action(), Action::Attack { elapsed, .. }
                if *elapsed * 4 <= spec.active_start())
                && adversary.action().is_attacking()
        }
        MomentKind::AdversaryTelegraphLate => {
            let spec = encounter.attack_spec(Side::Adversary);
            matches!(adversary.action(), Action::Attack { elapsed, .. }
                if *elapsed * 8 >= spec.active_start() * 7 && *elapsed < spec.active_start())
        }
        MomentKind::PlayerAnticipation => {
            let spec = encounter.attack_spec(Side::Player);
            matches!(player.action(), Action::Attack { elapsed, .. }
                if *elapsed * 2 >= spec.active_start() && *elapsed < spec.active_start())
        }
        MomentKind::PlayerActive => {
            let spec = encounter.attack_spec(Side::Player);
            matches!(player.action(), Action::Attack { elapsed, .. } if spec.is_active(*elapsed))
        }
        MomentKind::ConfirmedHit => events.any(|event| {
            matches!(
                event,
                CombatEvent::Hit {
                    attacker: Side::Player,
                    ..
                }
            )
        }),
        MomentKind::PlayerHit => events.any(|event| {
            matches!(
                event,
                CombatEvent::Hit {
                    attacker: Side::Adversary,
                    ..
                }
            )
        }),
        MomentKind::HitReaction => {
            matches!(adversary.action(), Action::Stagger { elapsed, duration, .. }
            if *elapsed * 4 >= *duration && *elapsed * 2 <= *duration)
        }
        MomentKind::AdversaryActive => {
            let spec = encounter.attack_spec(Side::Adversary);
            matches!(adversary.action(), Action::Attack { elapsed, .. } if spec.is_active(*elapsed))
        }
        MomentKind::SuccessfulDodge => {
            let spec = encounter.attack_spec(Side::Adversary);
            player.action().is_dodging()
                && matches!(adversary.action(), Action::Attack { elapsed, hits, .. }
                    if *elapsed >= spec.active_end() && !hits[Side::Player.index()])
        }
        MomentKind::Defeat => {
            events.any(|event| matches!(event, CombatEvent::Defeated { .. }))
                || encounter.outcome().is_some()
        }
    }
}

/// The reference encounter every combat capture and measurement refers to.
///
/// A repeating exchange rather than one scripted swing, because the moments have
/// to *occur* and a single pass cannot produce a dodge, a hit, a reaction and a
/// defeat. Read it as a player would describe it: wait, read the swing, dodge,
/// punish, back off, repeat.
///
/// Most legs carry [`ScriptTrigger::OnLateTelegraph`] on the dodge, because that
/// is the habit the fight is *about*: a scripted player that only dodges in the
/// leg named after dodging loses, which is the correct outcome for a player who
/// ignores six of every seven telegraphs and not a useful reference encounter.
/// The one leg that deliberately does not dodge is `stand-and-take-one`, which is
/// how the `player-hit` moment is guaranteed to occur.
pub const GOLDEN_SCRIPT: EncounterScript = EncounterScript {
    name: "arena-exchange",
    legs: &[
        ScriptLeg {
            // The opening: close enough to be noticed, still enough to be read.
            name: "wait",
            ticks: 260,
            motion: ScriptMotion::Hold,
            attack: ScriptTrigger::Never,
            dodge: ScriptTrigger::Never,
        },
        ScriptLeg {
            name: "read-and-dodge",
            ticks: 180,
            motion: ScriptMotion::Hold,
            attack: ScriptTrigger::Never,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            name: "punish",
            ticks: 180,
            motion: ScriptMotion::Close,
            attack: ScriptTrigger::OnOpening,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            name: "back-off",
            ticks: 150,
            motion: ScriptMotion::Retreat,
            attack: ScriptTrigger::Never,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            name: "circle",
            ticks: 150,
            motion: ScriptMotion::StrafeLeft,
            attack: ScriptTrigger::Never,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            // Long enough for one swing to land and no longer: this leg exists to
            // guarantee the `player-hit` moment, not to lose the fight.
            name: "stand-and-take-one",
            ticks: 160,
            motion: ScriptMotion::Hold,
            attack: ScriptTrigger::Never,
            dodge: ScriptTrigger::Never,
        },
        ScriptLeg {
            name: "close-in",
            ticks: 150,
            motion: ScriptMotion::Close,
            attack: ScriptTrigger::OnOpening,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            name: "press",
            ticks: 150,
            motion: ScriptMotion::Close,
            attack: ScriptTrigger::OnAdversaryRecovery,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            name: "strafe-right",
            ticks: 120,
            motion: ScriptMotion::StrafeRight,
            attack: ScriptTrigger::Never,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
        ScriptLeg {
            name: "finish",
            ticks: 300,
            motion: ScriptMotion::Close,
            attack: ScriptTrigger::OnOpening,
            dodge: ScriptTrigger::OnLateTelegraph,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::{
        EncounterScript, GOLDEN_SCRIPT, MomentKind, NAMED_MOMENTS, ScriptLeg, ScriptMotion,
        ScriptRunner, ScriptTrigger, at_moment, moment,
    };
    use crate::combatant::{Action, Side};
    use crate::encounter::{Encounter, WorldContact};
    use crate::event::{CombatEvent, StepEvents};
    use crate::fixture;
    use glam::Vec2;
    use std::collections::BTreeSet;

    fn encounter() -> Encounter {
        let ground = fixture::golden_ground();
        let mut encounter = match Encounter::new(&fixture::golden_setup(), Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        encounter.arm();
        encounter
    }

    #[test]
    fn the_successful_dodge_moment_is_a_dodge_that_actually_succeeded() {
        // Branch QA captured this moment and found the player staggered ten
        // ticks after it, because the predicate only asked whether a dodge was
        // in progress while a blade happened to be live. The frame labelled
        // "avoidance as geometry" was a frame of a dodge that failed.
        //
        // The oracle here is the event stream rather than the action's own
        // bookkeeping: replay to the moment and check that the adversary swing
        // the player was dodging never published a hit against the player.
        let ground = fixture::golden_ground();
        let setup = fixture::golden_setup();
        let mut encounter = match Encounter::new(&setup, Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        encounter.arm();
        let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&encounter));

        // Which adversary swing is in the air, and whether it has ever hit the
        // player, tracked independently of the action state.
        let mut swing_hit_player = false;
        let mut swing_id = None;
        let mut found = None;
        for _ in 0..fixture::GOLDEN_RUN_TICKS {
            let intent = runner.next_intent(&encounter);
            let events = encounter.step(intent, WorldContact::ground_only(&ground));
            if runner.finished() {
                runner.restart();
            }
            for event in events.iter() {
                match event {
                    CombatEvent::SwingStarted {
                        side: Side::Adversary,
                        swing,
                    } => {
                        swing_id = Some(swing);
                        swing_hit_player = false;
                    }
                    CombatEvent::Hit {
                        victim: Side::Player,
                        ..
                    } => swing_hit_player = true,
                    _ => {}
                }
            }
            if at_moment(MomentKind::SuccessfulDodge, &encounter, &events) {
                found = Some((encounter.tick_index(), swing_hit_player, swing_id));
                break;
            }
        }

        let (tick, hit, swing) = match found {
            Some(found) => found,
            None => panic!("the reference script never reaches a successful dodge"),
        };
        assert!(
            swing.is_some(),
            "the moment fired at tick {tick} with no adversary swing in the air"
        );
        assert!(
            !hit,
            "the moment fired at tick {tick} on a swing that had already hit the player"
        );

        // And the dodge survives the rest of that swing: the adversary cannot
        // still land it, because its active window is behind it.
        let spec = *encounter.attack_spec(Side::Adversary);
        match encounter.combatant(Side::Adversary).action() {
            Action::Attack { elapsed, hits, .. } => {
                assert!(
                    *elapsed >= spec.active_end(),
                    "the blade is still live at elapsed {elapsed} of {}",
                    spec.active_end()
                );
                assert!(!hits[Side::Player.index()], "the swing had hit the player");
            }
            other => panic!("the adversary is not swinging: {other:?}"),
        }
        assert!(encounter.combatant(Side::Player).action().is_dodging());
    }

    #[test]
    fn every_leg_is_named_once_and_lasts_a_real_number_of_ticks() {
        let names: BTreeSet<&str> = GOLDEN_SCRIPT.legs.iter().map(|leg| leg.name).collect();
        assert_eq!(names.len(), GOLDEN_SCRIPT.legs.len(), "a leg name repeats");
        for leg in GOLDEN_SCRIPT.legs {
            assert!(leg.ticks > 0, "{} lasts no time at all", leg.name);
            assert!(!leg.motion.name().is_empty());
            assert!(!leg.attack.name().is_empty());
            assert!(!leg.dodge.name().is_empty());
        }
        assert_eq!(
            GOLDEN_SCRIPT.duration(),
            GOLDEN_SCRIPT
                .legs
                .iter()
                .map(|leg| u64::from(leg.ticks))
                .sum::<u64>()
        );
        assert!(
            GOLDEN_SCRIPT.duration() < fixture::GOLDEN_RUN_TICKS,
            "the reference run must exercise the script as a loop"
        );
    }

    #[test]
    fn the_scripted_player_reads_telegraphs_in_most_legs() {
        // The habit the fight is about. One leg deliberately does not, which is
        // how the `player-hit` moment is guaranteed.
        let reading = GOLDEN_SCRIPT
            .legs
            .iter()
            .filter(|leg| leg.dodge == ScriptTrigger::OnLateTelegraph)
            .count();
        let passive = GOLDEN_SCRIPT
            .legs
            .iter()
            .filter(|leg| leg.dodge == ScriptTrigger::Never)
            .count();
        assert!(reading >= 6, "only {reading} legs read a telegraph");
        assert!(passive >= 1, "no leg leaves the player open");
        let attacking = GOLDEN_SCRIPT
            .legs
            .iter()
            .filter(|leg| leg.attack != ScriptTrigger::Never)
            .count();
        assert!(attacking >= 3, "only {attacking} legs ever swing");
    }

    #[test]
    fn each_motion_points_where_its_name_says() {
        let encounter = encounter();
        let toward = (encounter.combatant(Side::Adversary).position()
            - encounter.combatant(Side::Player).position())
        .normalize();
        assert_eq!(ScriptMotion::Hold.direction(&encounter), Vec2::ZERO);
        let close = ScriptMotion::Close.direction(&encounter);
        assert!((close - toward).length() < 1.0e-5);
        let retreat = ScriptMotion::Retreat.direction(&encounter);
        assert!((retreat + toward).length() < 1.0e-5);
        let left = ScriptMotion::StrafeLeft.direction(&encounter);
        let right = ScriptMotion::StrafeRight.direction(&encounter);
        assert!(left.dot(toward).abs() < 1.0e-5, "a strafe must be sideways");
        assert!(
            (left + right).length() < 1.0e-5,
            "the two must be opposites"
        );
        for motion in [
            ScriptMotion::Hold,
            ScriptMotion::Close,
            ScriptMotion::Retreat,
            ScriptMotion::StrafeLeft,
            ScriptMotion::StrafeRight,
        ] {
            assert!(motion.direction(&encounter).is_finite());
            assert!(!motion.name().is_empty());
        }
    }

    #[test]
    fn a_trigger_fires_at_most_once_per_leg() {
        // A scripted press has to behave like a latched key rather than a held
        // one, or a leg that lasts two hundred ticks asks for two hundred swings.
        let script = EncounterScript {
            name: "one-press",
            legs: &[
                ScriptLeg {
                    name: "press",
                    ticks: 40,
                    motion: ScriptMotion::Hold,
                    attack: ScriptTrigger::OnEntry,
                    dodge: ScriptTrigger::Never,
                },
                ScriptLeg {
                    name: "press-again",
                    ticks: 40,
                    motion: ScriptMotion::Hold,
                    attack: ScriptTrigger::OnEntry,
                    dodge: ScriptTrigger::Never,
                },
            ],
        };
        let encounter = encounter();
        let mut runner = ScriptRunner::new(script, 2.0);
        let mut presses = 0;
        for _ in 0..80 {
            if runner.next_intent(&encounter).attack() {
                presses += 1;
            }
        }
        assert_eq!(presses, 2, "one press per leg, not one per tick");
        assert!(runner.finished());
    }

    #[test]
    fn a_finished_runner_holds_and_can_be_restarted() {
        let script = EncounterScript {
            name: "short",
            legs: &[ScriptLeg {
                name: "only",
                ticks: 3,
                motion: ScriptMotion::Close,
                attack: ScriptTrigger::OnEntry,
                dodge: ScriptTrigger::Never,
            }],
        };
        let encounter = encounter();
        let mut runner = ScriptRunner::new(script, 2.0);
        assert!(!runner.finished());
        assert_eq!(runner.leg_name(), "only");
        for _ in 0..3 {
            let _ = runner.next_intent(&encounter);
        }
        assert!(runner.finished());
        assert_eq!(runner.leg_name(), "finished");
        let held = runner.next_intent(&encounter);
        assert_eq!(held.move_world(), Vec2::ZERO);
        assert!(!held.attack());
        runner.restart();
        assert!(!runner.finished());
        assert_eq!(runner.leg_tick(), 0);
        assert!(
            runner.next_intent(&encounter).attack(),
            "a restart presses again"
        );
    }

    #[test]
    fn an_empty_script_is_finished_before_it_starts() {
        let script = EncounterScript {
            name: "nothing",
            legs: &[],
        };
        let encounter = encounter();
        let mut runner = ScriptRunner::new(script, 2.0);
        assert!(runner.finished());
        assert_eq!(
            runner.next_intent(&encounter),
            crate::combatant::Intent::idle()
        );
        assert_eq!(script.duration(), 0);
    }

    #[test]
    fn two_runners_on_one_script_agree_tick_for_tick() {
        let ground = fixture::golden_ground();
        let trace = || {
            let mut encounter = encounter();
            let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, 2.0);
            let mut seen = Vec::new();
            for _ in 0..900 {
                let intent = runner.next_intent(&encounter);
                let _ = encounter.step(intent, WorldContact::ground_only(&ground));
                seen.push((
                    runner.leg_name(),
                    runner.leg_tick(),
                    intent.attack(),
                    intent.dodge(),
                ));
            }
            seen
        };
        assert_eq!(trace(), trace());
    }

    #[test]
    fn the_moment_predicates_disagree_with_each_other() {
        // A predicate that is true whenever another is would make two captures of
        // one frame, so the interesting property is that they separate.
        let ground = fixture::golden_ground();
        let mut fight = encounter();
        let mut runner = ScriptRunner::new(GOLDEN_SCRIPT, fixture::reach_of(&fight));
        let mut seen: Vec<BTreeSet<MomentKind>> = Vec::new();
        for _ in 0..1_500 {
            let intent = runner.next_intent(&fight);
            let events = fight.step(intent, WorldContact::ground_only(&ground));
            let mut matching = BTreeSet::new();
            for named in NAMED_MOMENTS {
                if at_moment(named.kind, &fight, &events) {
                    matching.insert(named.kind);
                }
            }
            seen.push(matching);
            if runner.finished() {
                runner.restart();
            }
        }
        // Every kind occurs on some tick without the faceoff, which is the one
        // that overlaps nothing else by construction.
        let mut alone = BTreeSet::new();
        for matching in &seen {
            if matching.len() == 1 {
                alone.extend(matching.iter().copied());
            }
        }
        assert!(
            alone.len() >= 4,
            "only {} moments ever occur on their own",
            alone.len()
        );
        assert!(
            seen.iter()
                .any(|matching| matching.contains(&MomentKind::Faceoff)),
            "a faceoff must happen"
        );
        // And an empty tick matches nothing except possibly the faceoff.
        let quiet = StepEvents::new();
        for named in NAMED_MOMENTS {
            if named.kind == MomentKind::Faceoff {
                continue;
            }
            let fresh = encounter();
            assert!(
                !at_moment(named.kind, &fresh, &quiet),
                "{} matches a fight that has not started",
                named.name
            );
        }
    }

    #[test]
    fn a_moment_can_be_looked_up_by_name_however_it_is_written() {
        match moment("Player-Active") {
            Some(found) => assert_eq!(found.kind, MomentKind::PlayerActive),
            None => panic!("a moment lookup must ignore case"),
        }
        match moment("  defeat  ") {
            Some(found) => assert_eq!(found.kind, MomentKind::Defeat),
            None => panic!("a moment lookup must ignore surrounding space"),
        }
        assert!(moment("").is_none());
    }
}
