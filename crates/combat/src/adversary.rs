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

use glam::Vec2;

use crate::combatant::{Action, Combatant, Intent};
use crate::hash::{fnv1a64, sub_hash, unit_from_hash};
use crate::spec::{AdversarySpec, CombatSeed, MovementSpec};
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
}

impl AdversaryBrain {
    #[must_use]
    pub fn new(seed: CombatSeed) -> Self {
        Self {
            state: AdversaryState::Idle,
            timer: 0,
            decisions: 0,
            seed: seed.raw(),
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

    /// Decides what to do this tick.
    ///
    /// Called once per tick, only for the adversary, and it may only read state.
    pub(crate) fn decide(
        &mut self,
        me: &Combatant,
        foe: &Combatant,
        spec: &AdversarySpec,
        movement: &MovementSpec,
    ) -> Intent {
        if me.action().is_defeated() || foe.action().is_defeated() {
            self.state = AdversaryState::Idle;
            self.timer = 0;
            return Intent::adversary(Vec2::ZERO, false);
        }
        // Being hit is the one thing that interrupts a plan. When the stagger
        // ends the brain will be in `Reposition`, so it backs off rather than
        // walking straight back into the blade that just hit it.
        if matches!(me.action(), Action::Stagger { .. }) {
            if !matches!(self.state, AdversaryState::Reposition { .. }) {
                self.state = self.choose_reposition(spec);
            }
            return Intent::adversary(Vec2::ZERO, false);
        }
        if !me.can_act() {
            // A swing or a dodge is running. The brain holds; the rules own it.
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
        let intent = brain.decide(&me, &far, &spec, &movement);
        assert_eq!(brain.state(), AdversaryState::Idle);
        assert_eq!(intent.move_world(), Vec2::ZERO);
        assert!(!intent.attack());

        let near = combatant(Side::Player, 10.0, 0.0, &character);
        let _ = brain.decide(&me, &near, &spec, &movement);
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
        let _ = brain.decide(&me, &foe, &spec, &movement);
        let intent = brain.decide(&me, &foe, &spec, &movement);
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
        let intent = brain.decide(&me, &close, &spec, &movement);
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
        let _ = brain.decide(&me, &touching, &spec, &movement);
        let _ = brain.decide(&me, &touching, &spec, &movement);
        assert!(matches!(brain.state(), AdversaryState::Reposition { .. }));
        let intent = brain.decide(&me, &touching, &spec, &movement);
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
            elapsed: 4,
            hits: [false; 2],
        });
        let foe = combatant(Side::Player, 2.0, 0.0, &character);
        let intent = brain.decide(&me, &foe, &spec, &movement);
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
        let _ = brain.decide(&me, &foe, &spec, &movement);
        me.set_action(Action::Stagger {
            elapsed: 1,
            duration: 36,
            from: Vec2::new(1.0, 0.0),
        });
        let intent = brain.decide(&me, &foe, &spec, &movement);
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
        let _ = brain.decide(&me, &foe, &spec, &movement);
        foe.set_action(Action::Defeated { elapsed: 0 });
        let intent = brain.decide(&me, &foe, &spec, &movement);
        assert_eq!(brain.state(), AdversaryState::Idle);
        assert!(!intent.attack());
        foe.set_action(Action::Free);
        me.set_action(Action::Defeated { elapsed: 0 });
        let intent = brain.decide(&me, &foe, &spec, &movement);
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
        let _ = brain.decide(&me, &foe, &spec, &movement);
        let intent = brain.decide(&me, &foe, &spec, &movement);
        assert!(intent.attack());
        assert_eq!(brain.state(), AdversaryState::Recover);
        assert_eq!(brain.timer(), spec.recover());
        for _ in 0..spec.recover() {
            let _ = brain.decide(&me, &foe, &spec, &movement);
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
                let intent = brain.decide(&me, &foe, &spec, &movement);
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
            let intent = other.decide(&me, &foe, &spec, &movement);
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
            let intent = brain.decide(&me, &foe, &spec, &movement);
            assert!(intent.move_world().is_finite());
        }
        // The capsule and the stand point stay usable throughout.
        assert!(me.hurt_capsule().is_finite());
        assert!(me.stand_point().is_finite());
    }
}
