//! The one enemy's mind: four states, two timers, one deterministic draw.
//!
//! **The brain produces intent and nothing else.** It does not carry a second
//! attack timeline: while the authoritative [`crate::combatant::Action`]
//! is not `Free` it simply holds, and the swing it asked for is run by exactly
//! the rules the player's swing is run by. Two sources of truth for "am I in the
//! middle of a swing" is the defect this shape exists to avoid.
//!
//! Four states and eight transitions do not need a behaviour tree, and a utility
//! system would need utility functions to choose between three options.
//! `RISK_REGISTER.md` R-002 names a framework built before its consumers, and
//! there is no second consumer.
//!
//! Variation comes from a named deterministic stream plus an explicit decision
//! index, so the sequence is reproducible from the seed and adding a draw
//! somewhere else cannot perturb it.
//!
//! **Combat initiative** adds two decisions and two bits of memory, and only
//! when the tuning authors a pressure lunge; without one, every branch below
//! is the M6 brain exactly. The lunge is chosen from middle distance, by the
//! distance and the body's own facing. The spacing dodge is chosen by the
//! body's **own** history — its own stagger has just ended, or its own lunge
//! has just connected — and never by anything the player is doing: the player
//! is read for where it is and nothing else.
//! `docs/planning/COMBAT_INITIATIVE.md` records why that line is the one that
//! matters.

use glam::Vec2;

use crate::combatant::{Action, AttackKind, Combatant, Intent};
use crate::encounter::AIM_ASSIST_CONE;
use crate::hash::{fnv1a64, sub_hash, unit_from_hash};
use crate::movement::{facing_of, wrap_angle};
use crate::spec::{AdversarySpec, CombatSeed, MovementSpec, PressureSpec};
use crate::tick::Ticks;

/// What the adversary is trying to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AdversaryState {
    /// The player is out of range and nothing is happening.
    Idle,
    /// Closing, until inside striking distance.
    Approach,
    /// Backing off and circling, either because it is too close or because it
    /// has just swung and is choosing not to commit again immediately.
    Reposition {
        /// Which way it circles: `-1` or `+1`.
        lateral: f32,
    },
    /// A deliberate pause after a swing, so the player gets a window.
    Recover,
}

impl AdversaryState {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Approach => "approach",
            Self::Reposition { .. } => "reposition",
            Self::Recover => "recover",
        }
    }
}

/// Named streams, so one decision cannot perturb another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamLabel {
    /// Whether to close in or circle after a pause.
    NextMove,
    /// Which way to circle.
    Lateral,
    /// How long to circle for.
    RepositionLength,
}

impl StreamLabel {
    const fn label(self) -> &'static [u8] {
        match self {
            Self::NextMove => b"adversary-next-move",
            Self::Lateral => b"adversary-lateral",
            Self::RepositionLength => b"adversary-reposition-length",
        }
    }
}

/// The adversary's whole mind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdversaryBrain {
    state: AdversaryState,
    timer: Ticks,
    decisions: u32,
    seed: u64,
    /// A spacing dodge is owed: the body's own stagger has ended, or its own
    /// lunge connected, and it has not yet taken the dodge that answers that.
    /// Never set without a pressure spec.
    spacing_owed: bool,
    /// The running action is this brain's own lunge, and whether it has
    /// connected so far. Read from its own `Action`, never inferred.
    lunge: Option<bool>,
}

impl AdversaryBrain {
    #[must_use]
    pub fn new(seed: CombatSeed) -> Self {
        Self {
            state: AdversaryState::Idle,
            timer: 0,
            decisions: 0,
            seed: seed.raw(),
            spacing_owed: false,
            lunge: None,
        }
    }

    #[must_use]
    pub const fn state(&self) -> AdversaryState {
        self.state
    }

    #[must_use]
    pub const fn timer(&self) -> Ticks {
        self.timer
    }

    /// How many decisions the stream has served, which is what makes a replay
    /// reproducible.
    #[must_use]
    pub const fn decisions(&self) -> u32 {
        self.decisions
    }

    /// Returns to the start, for an encounter reset.
    pub fn reset(&mut self) {
        self.state = AdversaryState::Idle;
        self.timer = 0;
        self.spacing_owed = false;
        self.lunge = None;
        // `decisions` keeps counting: a reset is a new round of the same
        // encounter, not a new encounter, and rewinding the stream would make
        // every round identical.
    }

    fn draw(&mut self, label: StreamLabel) -> f64 {
        let stream = fnv1a64(label.label()) ^ self.seed;
        let value = unit_from_hash(sub_hash(stream, u64::from(self.decisions)));
        self.decisions = self.decisions.wrapping_add(1);
        value
    }

    fn choose_reposition(&mut self, spec: &AdversarySpec) -> AdversaryState {
        let lateral = if self.draw(StreamLabel::Lateral) < 0.5 {
            -1.0
        } else {
            1.0
        };
        let length = self.draw(StreamLabel::RepositionLength);
        // Between half and one and a half of the authored length, so the rhythm
        // is not a metronome.
        let scale = 0.5 + length as f32;
        self.timer = ((spec.reposition() as f32 * scale) as Ticks).max(1);
        AdversaryState::Reposition { lateral }
    }

    /// Whether the spacing dodge is owed right now. For the tests and the
    /// report line; nothing outside the brain acts on it.
    #[must_use]
    pub const fn spacing_owed(&self) -> bool {
        self.spacing_owed
    }

    /// Decides what to do this tick.
    ///
    /// Called once per tick, only for the adversary, and it may only read state.
    /// Of the other body it reads the position and whether it is defeated;
    /// its action, its input and its future are none of the brain's business.
    pub(crate) fn decide(
        &mut self,
        me: &Combatant,
        foe: &Combatant,
        spec: &AdversarySpec,
        movement: &MovementSpec,
        pressure: Option<&PressureSpec>,
    ) -> Intent {
        if me.action().is_defeated() || foe.action().is_defeated() {
            self.state = AdversaryState::Idle;
            self.timer = 0;
            self.spacing_owed = false;
            self.lunge = None;
            return Intent::adversary(Vec2::ZERO, false);
        }
        // Being hit is the one thing that interrupts a plan. When the stagger
        // ends the brain will be in `Reposition`, so it backs off rather than
        // walking straight back into the blade that just hit it.
        if matches!(me.action(), Action::Stagger { .. }) {
            if !matches!(self.state, AdversaryState::Reposition { .. }) {
                self.state = self.choose_reposition(spec);
            }
            // With a lunge to take back the initiative with, the body answers
            // its own stagger with space: the first tick it is free, it dodges
            // away. The trigger is this body's own state and nothing else.
            if pressure.is_some() {
                self.spacing_owed = true;
            }
            self.lunge = None;
            return Intent::adversary(Vec2::ZERO, false);
        }
        if !me.can_act() {
            // A swing or a dodge is running. The brain holds; the rules own it.
            // It remembers one thing: whether its own lunge has connected.
            if me.action().attack_kind() == Some(AttackKind::Pressure) {
                self.lunge = Some(me.action().connected());
            }
            return Intent::adversary(Vec2::ZERO, false);
        }

        let to_foe = foe.position() - me.position();
        let distance = to_foe.length();
        let toward = if distance > 1.0e-4 {
            to_foe / distance
        } else {
            Vec2::new(0.0, -1.0)
        };
        let speed_fraction = |wanted: f32| (wanted / movement.speed().max(1.0e-4)).clamp(0.0, 1.0);

        // The first free tick after its own lunge. A lunge that connected
        // leaves the other body staggered in front of it, and the space the
        // lunge closed is reopened before anything else; one that missed has
        // already paid for it with its long recovery.
        if let Some(connected) = self.lunge.take() {
            self.spacing_owed |= connected;
        }
        if self.spacing_owed {
            self.spacing_owed = false;
            if me.dodge_cooldown() == 0 {
                self.state = AdversaryState::Approach;
                return Intent::adversary_dodge(-toward);
            }
        }
        // The lunge may be chosen from the band, facing the player closely
        // enough for the swing-start aim to close the rest: inside the same
        // cone every swing is aimed through. A lunge committed facing further
        // off than that would lock a line that does not pass through the
        // player — a threat that does not threaten, which is a bluff — so the
        // body turns first. The first measurement of this rule used a
        // six-degree tolerance instead and lost almost every lunge after a
        // spacing dodge: the facing is held through a stagger and a dodge, the
        // player moves meanwhile, and the body was still turning when the
        // distance left the band.
        let lunge_ready = pressure.is_some_and(|pressure| {
            pressure.selects(distance)
                && facing_of(to_foe).is_some_and(|bearing| {
                    wrap_angle(bearing - me.state().facing).abs() <= AIM_ASSIST_CONE
                })
        });

        match self.state {
            AdversaryState::Idle => {
                if distance <= spec.aggro_radius() {
                    self.state = AdversaryState::Approach;
                }
                Intent::adversary(Vec2::ZERO, false)
            }
            AdversaryState::Approach => {
                if distance > spec.aggro_radius() {
                    self.state = AdversaryState::Idle;
                    return Intent::adversary(Vec2::ZERO, false);
                }
                if lunge_ready {
                    return self.commit_lunge();
                }
                if distance < spec.min_range() {
                    self.state = self.choose_reposition(spec);
                    return Intent::adversary(Vec2::ZERO, false);
                }
                if distance <= spec.strike_range() {
                    // Commit. The authoritative timeline takes over from here and
                    // the brain will hold until it is free again.
                    self.state = AdversaryState::Recover;
                    self.timer = spec.recover();
                    return Intent::adversary(Vec2::ZERO, true);
                }
                Intent::adversary(toward * speed_fraction(spec.approach_speed()), false)
            }
            AdversaryState::Reposition { lateral } => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.state = AdversaryState::Approach;
                }
                if lunge_ready {
                    self.state = AdversaryState::Approach;
                    return self.commit_lunge();
                }
                // Away and around: a straight retreat reads as fleeing, and a
                // pure circle never opens the distance it came to open.
                let away = -toward;
                let side = Vec2::new(-away.y, away.x) * lateral;
                let blend = away * 0.75 + side * 0.66;
                let direction = if blend.length_squared() > 1.0e-8 {
                    blend.normalize()
                } else {
                    away
                };
                Intent::adversary(direction * speed_fraction(spec.reposition_speed()), false)
            }
            AdversaryState::Recover => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    // The one real choice this mind makes: close again, or give
                    // the player room and come back round.
                    self.state = if self.draw(StreamLabel::NextMove) < 0.55 {
                        AdversaryState::Approach
                    } else {
                        self.choose_reposition(spec)
                    };
                }
                Intent::adversary(Vec2::ZERO, false)
            }
        }
    }

    /// Commits the pressure lunge. The brain stays in `Approach`: there is no
    /// pause after a lunge, because a missed one's opening is its own long
    /// recovery, and a connected one is followed by the spacing dodge.
    fn commit_lunge(&mut self) -> Intent {
        self.lunge = Some(false);
        Intent::adversary_pressure()
    }
}

#[cfg(test)]
mod tests {
    use super::{AdversaryBrain, AdversaryState, StreamLabel};
    use crate::combatant::{Action, Combatant, Health, Side};
    use crate::spec::{
        AdversarySpec, AuthoredAdversary, AuthoredMovement, CombatSeed, MovementSpec,
    };
    use glam::Vec2;
    use veldwake_character::skeleton::Side as BodySide;
    use veldwake_character::{
        CharacterCompiler, CharacterState, CompiledCharacter, PosedCharacter, fixture, pose,
    };

    fn character() -> CompiledCharacter {
        match CharacterCompiler::new().compile_descriptor(&fixture::golden_descriptor()) {
            Ok(character) => character,
            Err(error) => panic!("{error}"),
        }
    }

    fn posed(character: &CompiledCharacter) -> PosedCharacter {
        pose(character, &CharacterState::default(), None)
    }

    fn combatant(side: Side, x: f32, z: f32, character: &CompiledCharacter) -> Combatant {
        let state = CharacterState {
            x,
            z,
            grounded: true,
            ..CharacterState::default()
        };
        Combatant::new(
            side,
            state,
            Health::full(100),
            character.collision().capsule(),
            crate::hurt::HurtVolume::derive(character),
            BodySide::Right,
            posed(character),
        )
    }

    fn spec() -> AdversarySpec {
        match (AuthoredAdversary {
            aggro_radius: 14.0,
            strike_range: 2.6,
            min_range: 1.6,
            approach_speed: 2.4,
            reposition_speed: 1.8,
            recover_seconds: 0.5,
            reposition_seconds: 0.8,
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        }
    }

    fn movement() -> MovementSpec {
        match (AuthoredMovement {
            speed: 3.4,
            turn_rate: 9.0,
            max_step_up: 1.0,
            max_drop: 2.0,
            recovery_speed_scale: 0.25,
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn a_distant_player_is_ignored_until_inside_the_aggro_radius() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let far = combatant(Side::Player, 40.0, 0.0, &character);
        let intent = brain.decide(&me, &far, &spec, &movement, None);
        assert_eq!(brain.state(), AdversaryState::Idle);
        assert_eq!(intent.move_world(), Vec2::ZERO);
        assert!(!intent.attack());

        let near = combatant(Side::Player, 10.0, 0.0, &character);
        let _ = brain.decide(&me, &near, &spec, &movement, None);
        assert_eq!(brain.state(), AdversaryState::Approach);
    }

    #[test]
    fn it_closes_the_distance_and_commits_inside_striking_range() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let foe = combatant(Side::Player, 8.0, 0.0, &character);
        // Wake up, then approach.
        let _ = brain.decide(&me, &foe, &spec, &movement, None);
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert!(
            intent.move_world().x > 0.0,
            "it must walk toward the player"
        );
        assert!(intent.face_foe());
        assert!(!intent.attack());
        // The move magnitude is the approach speed as a fraction of the shared
        // movement speed, not a raw speed.
        let expected = spec.approach_speed() / movement.speed();
        assert!((intent.move_world().length() - expected).abs() < 1.0e-5);

        let close = combatant(Side::Player, 2.0, 0.0, &character);
        let intent = brain.decide(&me, &close, &spec, &movement, None);
        assert!(intent.attack(), "inside strike range it must swing");
        assert_eq!(brain.state(), AdversaryState::Recover);
    }

    #[test]
    fn it_backs_off_when_the_player_is_too_close_to_swing_at() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let touching = combatant(Side::Player, 0.8, 0.0, &character);
        let _ = brain.decide(&me, &touching, &spec, &movement, None);
        let _ = brain.decide(&me, &touching, &spec, &movement, None);
        assert!(matches!(brain.state(), AdversaryState::Reposition { .. }));
        let intent = brain.decide(&me, &touching, &spec, &movement, None);
        assert!(
            intent.move_world().x < 0.0,
            "it must open the distance, not close it"
        );
        assert!(!intent.attack());
    }

    #[test]
    fn a_swing_in_progress_makes_the_brain_hold_rather_than_run_a_second_timeline() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let mut me = combatant(Side::Adversary, 0.0, 0.0, &character);
        me.set_action(Action::Attack {
            swing: crate::combatant::SwingId(0),
            kind: crate::combatant::AttackKind::Primary,
            elapsed: 4,
            hits: [false; 2],
        });
        let foe = combatant(Side::Player, 2.0, 0.0, &character);
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert_eq!(intent.move_world(), Vec2::ZERO);
        assert!(!intent.attack(), "it must not ask for a second swing");
    }

    #[test]
    fn being_hit_makes_it_back_off_when_it_recovers() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let mut me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let foe = combatant(Side::Player, 2.0, 0.0, &character);
        // Wake it into Approach first.
        let _ = brain.decide(&me, &foe, &spec, &movement, None);
        me.set_action(Action::Stagger {
            elapsed: 1,
            duration: 36,
            from: Vec2::new(1.0, 0.0),
        });
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert_eq!(intent.move_world(), Vec2::ZERO, "a staggered body holds");
        assert!(matches!(brain.state(), AdversaryState::Reposition { .. }));
    }

    #[test]
    fn a_defeated_body_on_either_side_stops_the_fight() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let mut me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let mut foe = combatant(Side::Player, 2.0, 0.0, &character);
        let _ = brain.decide(&me, &foe, &spec, &movement, None);
        foe.set_action(Action::Defeated { elapsed: 0 });
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert_eq!(brain.state(), AdversaryState::Idle);
        assert!(!intent.attack());
        foe.set_action(Action::Free);
        me.set_action(Action::Defeated { elapsed: 0 });
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert!(!intent.attack());
        assert_eq!(intent.move_world(), Vec2::ZERO);
    }

    #[test]
    fn the_pause_after_a_swing_lasts_the_authored_number_of_ticks() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let foe = combatant(Side::Player, 2.0, 0.0, &character);
        let _ = brain.decide(&me, &foe, &spec, &movement, None);
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert!(intent.attack());
        assert_eq!(brain.state(), AdversaryState::Recover);
        assert_eq!(brain.timer(), spec.recover());
        for _ in 0..spec.recover() {
            let _ = brain.decide(&me, &foe, &spec, &movement, None);
        }
        assert_ne!(
            brain.state(),
            AdversaryState::Recover,
            "the pause must end on its own"
        );
    }

    #[test]
    fn the_decision_stream_is_reproducible_from_the_seed() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let foe = combatant(Side::Player, 2.2, 0.0, &character);
        let run = || {
            let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
            let mut trace = Vec::new();
            for _ in 0..600 {
                let intent = brain.decide(&me, &foe, &spec, &movement, None);
                trace.push((brain.state().name(), intent.attack(), brain.timer()));
            }
            (trace, brain.decisions())
        };
        let (first, first_decisions) = run();
        let (second, second_decisions) = run();
        assert_eq!(first, second, "the same seed must replay exactly");
        assert_eq!(first_decisions, second_decisions);
        assert!(first_decisions > 0, "the brain must actually draw");

        // A different seed must produce a different sequence, or the stream is
        // not doing anything.
        let mut other = AdversaryBrain::new(CombatSeed(0x1234_5678_9abc_def0));
        let mut trace = Vec::new();
        for _ in 0..600 {
            let intent = other.decide(&me, &foe, &spec, &movement, None);
            trace.push((other.state().name(), intent.attack(), other.timer()));
        }
        assert_ne!(first, trace, "a different seed must behave differently");
    }

    #[test]
    fn a_reset_returns_the_mind_to_idle_without_rewinding_the_stream() {
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let before = brain.decisions();
        let _ = brain.draw(StreamLabel::NextMove);
        brain.reset();
        assert_eq!(brain.state(), AdversaryState::Idle);
        assert_eq!(brain.timer(), 0);
        assert!(
            brain.decisions() > before,
            "a reset must not make every round identical"
        );
    }

    #[test]
    fn every_state_names_itself() {
        for state in [
            AdversaryState::Idle,
            AdversaryState::Approach,
            AdversaryState::Reposition { lateral: 1.0 },
            AdversaryState::Recover,
        ] {
            assert!(!state.name().is_empty());
        }
    }

    #[test]
    fn coincident_bodies_do_not_produce_a_nan_intent() {
        let character = character();
        let spec = spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 1.0, 1.0, &character);
        let foe = combatant(Side::Player, 1.0, 1.0, &character);
        for _ in 0..200 {
            let intent = brain.decide(&me, &foe, &spec, &movement, None);
            assert!(intent.move_world().is_finite());
        }
        // The capsule and the stand point stay usable throughout.
        assert!(me.hurt_capsule().is_finite());
        assert!(me.stand_point().is_finite());
    }

    // -----------------------------------------------------------------------
    // Combat initiative
    // -----------------------------------------------------------------------

    fn pressure() -> crate::spec::PressureSpec {
        match crate::fixture::pressure().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        }
    }

    fn fixture_spec() -> AdversarySpec {
        match crate::fixture::adversary().compile() {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        }
    }

    /// Everything the player could be doing while standing in one place, so a
    /// decision can be shown not to depend on it.
    fn foe_actions() -> [Action; 5] {
        use crate::combatant::{AttackKind, SwingId};
        let attack = |elapsed| Action::Attack {
            swing: SwingId::first(),
            kind: AttackKind::Primary,
            elapsed,
            hits: [false; 2],
        };
        [
            Action::Free,
            attack(1),
            attack(23),
            attack(60),
            Action::Dodge {
                elapsed: 3,
                duration: 36,
                direction: Vec2::new(1.0, 0.0),
            },
        ]
    }

    /// A brain awake in `Approach`, facing down `-Z` at a player `distance`
    /// away along that line.
    fn awake(
        distance: f32,
        character: &CompiledCharacter,
    ) -> (AdversaryBrain, Combatant, Combatant) {
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 0.0, 0.0, character);
        let foe = combatant(Side::Player, 0.0, -distance, character);
        let spec = fixture_spec();
        let movement = movement();
        let pressure = pressure();
        let _ = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        assert_eq!(brain.state(), AdversaryState::Approach);
        (brain, me, foe)
    }

    #[test]
    fn inside_the_band_it_lunges_and_inside_strike_range_it_swings_its_primary() {
        use crate::combatant::AttackKind;
        let character = character();
        let spec = fixture_spec();
        let movement = movement();
        let pressure = pressure();
        let middle = (pressure.select_min() + pressure.select_max()) * 0.5;
        let (mut brain, me, foe) = awake(middle, &character);
        let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        assert!(intent.attack());
        assert_eq!(intent.attack_kind(), AttackKind::Pressure);

        // Two attacks, and the historical one keeps its place at close range.
        let (mut brain, me, foe) = awake(2.0, &character);
        let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        assert!(intent.attack(), "inside strike range it must still swing");
        assert_eq!(intent.attack_kind(), AttackKind::Primary);

        // Between the two it neither lunges nor swings: it closes.
        let between = (spec.strike_range() + pressure.select_min()) * 0.5;
        let (mut brain, me, foe) = awake(between, &character);
        let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        assert!(!intent.attack());
        assert!(intent.move_world().length() > 0.0);

        // Beyond the band it closes too.
        let (mut brain, me, foe) = awake(pressure.select_max() + 0.5, &character);
        let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        assert!(!intent.attack());
    }

    #[test]
    fn a_lunge_is_not_committed_facing_away_from_the_player() {
        let character = character();
        let spec = fixture_spec();
        let movement = movement();
        let pressure = pressure();
        let middle = (pressure.select_min() + pressure.select_max()) * 0.5;
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let me = combatant(Side::Adversary, 0.0, 0.0, &character);
        // The player is off to the body's side: ninety degrees from its facing.
        let foe = combatant(Side::Player, middle, 0.0, &character);
        let _ = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
        assert!(
            !intent.attack(),
            "a lunge locked off the player's bearing is a bluff"
        );
        assert!(intent.face_foe(), "it turns first");
    }

    #[test]
    fn the_lunge_decision_does_not_depend_on_what_the_player_is_doing() {
        let character = character();
        let spec = fixture_spec();
        let movement = movement();
        let pressure = pressure();
        for distance in [2.0, 3.3, pressure.select_min(), pressure.select_max(), 6.0] {
            let mut intents = Vec::new();
            for action in foe_actions() {
                let (mut brain, me, mut foe) = awake(distance, &character);
                foe.set_action(action);
                intents.push(brain.decide(&me, &foe, &spec, &movement, Some(&pressure)));
            }
            assert!(
                intents.windows(2).all(|pair| pair[0] == pair[1]),
                "at {distance} the decision changed with the player's action: {intents:?}"
            );
        }
    }

    #[test]
    fn its_own_stagger_is_answered_with_a_spacing_dodge_whatever_the_player_does() {
        let character = character();
        let spec = fixture_spec();
        let movement = movement();
        let pressure = pressure();
        let mut intents = Vec::new();
        for action in foe_actions() {
            let (mut brain, mut me, mut foe) = awake(2.0, &character);
            me.set_action(Action::Stagger {
                elapsed: 5,
                duration: 36,
                from: Vec2::new(0.0, 1.0),
            });
            let held = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
            assert!(!held.dodge() && !held.attack(), "a staggered body holds");
            assert!(brain.spacing_owed());
            me.set_action(Action::Free);
            foe.set_action(action);
            let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
            assert!(intent.dodge(), "the first free tick is the spacing dodge");
            // Away from the player, who stands down -Z.
            assert!(intent.move_world().y > 0.9, "{:?}", intent.move_world());
            intents.push(intent);
        }
        assert!(
            intents.windows(2).all(|pair| pair[0] == pair[1]),
            "the spacing dodge changed with the player's action: {intents:?}"
        );
    }

    #[test]
    fn without_a_lunge_its_stagger_is_answered_the_historical_way() {
        let character = character();
        let spec = fixture_spec();
        let movement = movement();
        let mut brain = AdversaryBrain::new(CombatSeed::GOLDEN);
        let mut me = combatant(Side::Adversary, 0.0, 0.0, &character);
        let foe = combatant(Side::Player, 0.0, -2.0, &character);
        let _ = brain.decide(&me, &foe, &spec, &movement, None);
        me.set_action(Action::Stagger {
            elapsed: 5,
            duration: 36,
            from: Vec2::new(0.0, 1.0),
        });
        let _ = brain.decide(&me, &foe, &spec, &movement, None);
        assert!(!brain.spacing_owed());
        me.set_action(Action::Free);
        let intent = brain.decide(&me, &foe, &spec, &movement, None);
        assert!(!intent.dodge(), "the historical brain has no spacing dodge");
        assert!(matches!(brain.state(), AdversaryState::Reposition { .. }));
    }

    #[test]
    fn its_own_lunge_is_followed_by_space_only_when_it_connected() {
        use crate::combatant::{AttackKind, SwingId};
        let character = character();
        let spec = fixture_spec();
        let movement = movement();
        let pressure = pressure();
        for connected in [true, false] {
            let middle = (pressure.select_min() + pressure.select_max()) * 0.5;
            let (mut brain, mut me, foe) = awake(middle, &character);
            let intent = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
            assert_eq!(intent.attack_kind(), AttackKind::Pressure);
            me.set_action(Action::Attack {
                swing: SwingId::first(),
                kind: AttackKind::Pressure,
                elapsed: 70,
                hits: [connected, false],
            });
            let _ = brain.decide(&me, &foe, &spec, &movement, Some(&pressure));
            me.set_action(Action::Free);
            // Close now, as a body that has just lunged is.
            let near = combatant(Side::Player, 0.0, -1.6, &character);
            let intent = brain.decide(&me, &near, &spec, &movement, Some(&pressure));
            assert_eq!(
                intent.dodge(),
                connected,
                "connected {connected}: the spacing dodge answers a hit, not a miss"
            );
        }
    }
}
