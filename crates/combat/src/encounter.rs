//! One encounter between two combatants, advanced one tick at a time.
//!
//! [`Encounter::step`] takes **no duration**. One call is one tick of
//! [`COMBAT_TICK_HZ`](crate::tick::COMBAT_TICK_HZ), and the client turns wall
//! time into a count of them with [`CombatClock`](crate::tick::CombatClock).
//!
//! # The canonical tick order
//!
//! The order is part of the determinism contract, so it is written down and
//! tested rather than left to emerge from loop order. Within a step:
//!
//! 1. **freeze** — a frozen combatant advances no timeline, no gait phase and no
//!    position, and is not re-posed. The hitstop counter is decremented at the
//!    *end* of the tick, so a hitstop of `n` ticks freezes exactly `n` of them.
//! 2. **intent** — the player's from the caller, the adversary's from its brain,
//!    which decides only while the authoritative action is free.
//! 3. **action** — finish actions that are over, start requested ones, then emit
//!    the phase events of actions already running.
//! 4. **movement** — in the fixed order `[Player, Adversary]`: velocity from the
//!    intent and the action, the step and arena rules, the realised speed, the
//!    facing, and then the gait and ground update.
//! 5. **separation** — push overlapping bodies apart through the same movement
//!    rules, so a push-out cannot place a body somewhere it may not be.
//! 6. **pose** — each body is posed with its action overlay; the previous tick's
//!    blade is kept and the new one recorded.
//! 7. **hit** — in the fixed order `[Player, Adversary]`: sweep the blade from
//!    the previous position to the current one against the foe's hurt capsule,
//!    then apply damage, stagger, knockback, hitstop and events.
//! 8. **outcome** — defeat hold and reset.
//! 9. **advance** — elapsed tick counters increase and an action that has run out
//!    returns to free on its own last tick; cooldowns and the hitstop count down.
//!
//! Two policies follow from step 7 and are documented rather than accidental:
//!
//! - **A combatant defeated earlier in the same tick does not land its own
//!   swing.** Death cancels the blade. Since the player resolves first, this
//!   deliberately gives the player the tie in a mutual-kill tick.
//! - **Hitstop starts on the tick that produced it**, at step 9: the hit is fully
//!   resolved and published first, and then neither body's timeline advances. A
//!   hitstop of `n` ticks therefore freezes exactly `n` advances, the first being
//!   the hit tick's, and it cannot reorder anything inside that tick.

use std::cell::Cell;

use glam::{Mat4, Vec2};

use veldwake_character::skeleton::{BoneId, Side as BodySide};
use veldwake_character::{
    CharacterCompiler, CharacterDescriptor, CharacterError, CharacterState, CompiledCharacter,
    GroundSampler, pose_with,
};

use crate::adversary::AdversaryBrain;
use crate::combatant::{Action, Combatant, Health, Intent, SIDES, Side};
use crate::event::{CombatEvent, StepEvents};
use crate::hit::{Segment, Sweep, sweep_capsule};
use crate::movement::{MoveRules, facing_of, separate, try_move, turn_toward};
use crate::spec::{AttackSpec, AuthoredTuning, EncounterTuning, SpecError};
use crate::tick::{Ticks, tick_seconds};
use crate::weapon::{CompiledWeapon, WeaponCompiler, WeaponDescriptor, WeaponError};

/// Which descriptors, which weapon and which tuning an encounter is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EncounterSetup {
    pub player: CharacterDescriptor,
    pub adversary: CharacterDescriptor,
    pub weapon: WeaponDescriptor,
    pub tuning: AuthoredTuning,
    /// Where the player starts, relative to the arena centre, in world units.
    pub player_offset: Vec2,
    /// Where the adversary starts, relative to the arena centre.
    pub adversary_offset: Vec2,
}

/// Why an encounter could not be built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EncounterError {
    Character(CharacterError),
    Weapon(WeaponError),
    Spec(SpecError),
}

impl std::fmt::Display for EncounterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Character(error) => write!(formatter, "combatant did not compile: {error}"),
            Self::Weapon(error) => write!(formatter, "weapon did not compile: {error}"),
            Self::Spec(error) => write!(formatter, "tuning did not compile: {error}"),
        }
    }
}

impl std::error::Error for EncounterError {}

impl From<CharacterError> for EncounterError {
    fn from(error: CharacterError) -> Self {
        Self::Character(error)
    }
}

impl From<WeaponError> for EncounterError {
    fn from(error: WeaponError) -> Self {
        Self::Weapon(error)
    }
}

impl From<SpecError> for EncounterError {
    fn from(error: SpecError) -> Self {
        Self::Spec(error)
    }
}

/// What an encounter has done, for the report line and the probe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CombatCounters {
    pub ticks: u64,
    pub swings: [u32; SIDES.len()],
    /// Hits landed *by* each side.
    pub hits: [u32; SIDES.len()],
    pub whiffs: [u32; SIDES.len()],
    pub dodges: [u32; SIDES.len()],
    /// Dodges asked for and refused, by cooldown or by a running action.
    pub dodges_refused: [u32; SIDES.len()],
    pub staggers: [u32; SIDES.len()],
    pub defeats: [u32; SIDES.len()],
    pub resets: u32,
    /// Moves the rules refused outright.
    pub blocked_moves: [u32; SIDES.len()],
    /// Ticks on which the two bodies had to be pushed apart.
    pub separations: u32,
    pub hit_queries: u64,
    /// Highest substep count any sweep has needed.
    pub sweep_substeps_max: u32,
    /// Extra contacts suppressed because the swing had already hit.
    pub multi_hit_suppressed: u32,
    /// Events a tick could not hold. Must stay zero.
    pub events_dropped: u32,
    pub frozen_ticks: [u32; SIDES.len()],
}

/// A ground sampler that counts what was asked of it.
///
/// Presentation-free and allocation-free, and the only way to turn "how many
/// ground queries does a tick make" into a measurement rather than a guess.
pub struct CountingGround<'a> {
    inner: &'a dyn GroundSampler,
    queries: Cell<u64>,
}

impl<'a> CountingGround<'a> {
    #[must_use]
    pub fn new(inner: &'a dyn GroundSampler) -> Self {
        Self {
            inner,
            queries: Cell::new(0),
        }
    }

    #[must_use]
    pub fn queries(&self) -> u64 {
        self.queries.get()
    }
}

impl GroundSampler for CountingGround<'_> {
    fn surface(&self, x: f64, z: f64) -> Option<f64> {
        self.queries.set(self.queries.get().saturating_add(1));
        self.inner.surface(x, z)
    }
}

/// One fight between two combatants.
pub struct Encounter {
    tuning: EncounterTuning,
    characters: [CompiledCharacter; SIDES.len()],
    weapon: CompiledWeapon,
    combatants: [Combatant; SIDES.len()],
    brain: AdversaryBrain,
    offsets: [Vec2; SIDES.len()],
    tick: u64,
    armed: bool,
    /// Who was defeated, while the defeat hold runs.
    outcome: Option<Side>,
    counters: CombatCounters,
}

impl Encounter {
    /// Compiles both bodies, the weapon and the tuning, and settles the two on
    /// the ground.
    pub fn new(
        setup: &EncounterSetup,
        ground: Option<&dyn GroundSampler>,
    ) -> Result<Self, EncounterError> {
        let tuning = setup.tuning.compile()?;
        let mut compiler = CharacterCompiler::new();
        let player = compiler.compile_descriptor(&setup.player)?;
        let adversary = compiler.compile_descriptor(&setup.adversary)?;
        let weapon = WeaponCompiler::new().compile_descriptor(&setup.weapon)?;
        let characters = [player, adversary];
        let offsets = [setup.player_offset, setup.adversary_offset];
        let healths = [
            Health::full(tuning.player_health()),
            Health::full(tuning.adversary_health()),
        ];

        let mut combatants = Vec::with_capacity(SIDES.len());
        for side in SIDES {
            let index = side.index();
            let state = start_state(tuning.arena().centre(), offsets, side, ground);
            let character = &characters[index];
            let posed = pose_with(character, &state, ground, None);
            combatants.push(Combatant::new(
                side,
                state,
                healths[index],
                character.collision().capsule(),
                BodySide::Right,
                posed,
            ));
        }
        let Ok(combatants) = <[Combatant; SIDES.len()]>::try_from(combatants) else {
            // `SIDES` has two entries and the loop pushed one per side.
            unreachable!("one combatant per side");
        };

        let mut encounter = Self {
            tuning,
            characters,
            weapon,
            combatants,
            brain: AdversaryBrain::new(tuning.seed()),
            offsets,
            tick: 0,
            armed: false,
            outcome: None,
            counters: CombatCounters::default(),
        };
        // Give both bodies their carry pose and a blade to sweep from, so the
        // first armed tick does not sweep from nowhere.
        for side in SIDES {
            encounter.repose(side, ground);
        }
        Ok(encounter)
    }

    /// Starts the fight.
    ///
    /// An encounter begins paused, because the world takes about a minute to
    /// stream in on the audited host and a fight that starts on its own is over
    /// before anybody sees it.
    pub fn arm(&mut self) {
        self.armed = true;
    }

    #[must_use]
    pub const fn is_armed(&self) -> bool {
        self.armed
    }

    #[must_use]
    pub const fn tick_index(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub const fn tuning(&self) -> &EncounterTuning {
        &self.tuning
    }

    #[must_use]
    pub fn combatant(&self, side: Side) -> &Combatant {
        &self.combatants[side.index()]
    }

    #[must_use]
    pub fn character(&self, side: Side) -> &CompiledCharacter {
        &self.characters[side.index()]
    }

    #[must_use]
    pub const fn weapon(&self) -> &CompiledWeapon {
        &self.weapon
    }

    #[must_use]
    pub const fn brain(&self) -> &AdversaryBrain {
        &self.brain
    }

    #[must_use]
    pub const fn counters(&self) -> &CombatCounters {
        &self.counters
    }

    /// Who is currently defeated, while the hold before a reset runs.
    #[must_use]
    pub const fn outcome(&self) -> Option<Side> {
        self.outcome
    }

    /// The attack spec that governs one side's swings.
    #[must_use]
    pub fn attack_spec(&self, side: Side) -> &AttackSpec {
        match side {
            Side::Player => self.tuning.player_attack(),
            Side::Adversary => self.tuning.adversary_attack(),
        }
    }

    /// Planar distance between the two bodies' centres.
    #[must_use]
    pub fn separation_distance(&self) -> f32 {
        (self.combatants[1].position() - self.combatants[0].position()).length()
    }

    /// The world matrix of one side's weapon, for the renderer.
    #[must_use]
    pub fn weapon_matrix(&self, side: Side) -> Mat4 {
        let posed = self.combatants[side.index()].posed();
        self.weapon.matrix(
            posed.world_matrix(),
            posed.bone_world()[BoneId::HandR.index()],
        )
    }

    /// One side's blade in world space, right now.
    #[must_use]
    pub fn blade_world(&self, side: Side) -> Segment {
        let posed = self.combatants[side.index()].posed();
        self.weapon.blade_world(
            posed.world_matrix(),
            posed.bone_world()[BoneId::HandR.index()],
        )
    }

    /// Advances the fight by exactly one tick.
    pub fn step(&mut self, input: Intent, ground: Option<&dyn GroundSampler>) -> StepEvents {
        let mut events = StepEvents::new();
        self.tick = self.tick.saturating_add(1);
        self.counters.ticks = self.counters.ticks.saturating_add(1);

        // 1. freeze
        for side in SIDES {
            let index = side.index();
            if self.combatants[index].is_frozen() {
                self.counters.frozen_ticks[index] =
                    self.counters.frozen_ticks[index].saturating_add(1);
            }
        }

        if !self.armed {
            // Paused, but alive: the idle breath keeps running so a faceoff is
            // not two statues.
            for side in SIDES {
                let index = side.index();
                self.combatants[index].state_mut().time += tick_seconds();
                self.repose(side, ground);
            }
            return events;
        }

        // 2. intent
        let player_intent = if self.combatants[0].is_frozen() {
            Intent::idle()
        } else {
            input
        };
        let adversary_intent = if self.combatants[1].is_frozen() {
            Intent::idle()
        } else {
            let (me, foe) = (&self.combatants[1], &self.combatants[0]);
            self.brain
                .decide(me, foe, self.tuning.adversary(), self.tuning.movement())
        };
        let intents = [player_intent, adversary_intent];

        // 3. action
        for side in SIDES {
            self.start_action(side, intents[side.index()], &mut events);
        }
        for side in SIDES {
            self.phase_events(side, &mut events);
        }

        // 4. movement
        for side in SIDES {
            self.move_combatant(side, intents[side.index()], ground);
        }

        // 5. separation
        self.separate_bodies(ground);

        // 6. pose
        for side in SIDES {
            if !self.combatants[side.index()].is_frozen() {
                self.repose(side, ground);
            }
        }

        // 7. hit
        for side in SIDES {
            self.resolve_hits(side, ground, &mut events);
        }

        // 8. outcome
        self.update_outcome(ground, &mut events);

        // 9. advance
        for side in SIDES {
            self.advance_action(side);
        }

        self.counters.events_dropped = self
            .counters
            .events_dropped
            .saturating_add(u32::from(events.dropped()));
        events
    }

    /// Starts an attack or a dodge if one was asked for and is legal.
    fn start_action(&mut self, side: Side, intent: Intent, events: &mut StepEvents) {
        let index = side.index();
        if !self.combatants[index].can_act() {
            if intent.dodge() {
                self.counters.dodges_refused[index] =
                    self.counters.dodges_refused[index].saturating_add(1);
            }
            return;
        }
        // Attack wins over dodge when both arrive in one tick: committing is the
        // decision the slice is about, and a tie has to resolve somewhere.
        if intent.attack() {
            let swing = self.combatants[index].take_swing_id();
            self.combatants[index].set_action(Action::Attack {
                swing,
                elapsed: 0,
                hits: [false; SIDES.len()],
            });
            self.counters.swings[index] = self.counters.swings[index].saturating_add(1);
            events.push(CombatEvent::SwingStarted { side, swing });
            return;
        }
        if intent.dodge() {
            if self.combatants[index].dodge_cooldown() > 0 {
                self.counters.dodges_refused[index] =
                    self.counters.dodges_refused[index].saturating_add(1);
                return;
            }
            let dodge = *self.tuning.dodge();
            // A dodge with no direction goes straight backwards, which is what a
            // player who pressed it while standing still means by it.
            let facing = self.combatants[index].state().facing;
            let direction = if intent.move_world().length_squared() > 1.0e-6 {
                intent.move_world().normalize()
            } else {
                let forward = veldwake_character::pose::facing_direction(facing);
                -Vec2::new(forward.x, forward.z)
            };
            self.combatants[index].set_action(Action::Dodge {
                elapsed: 0,
                duration: dodge.duration(),
                direction,
            });
            self.combatants[index].start_dodge_cooldown(&dodge);
            self.counters.dodges[index] = self.counters.dodges[index].saturating_add(1);
            events.push(CombatEvent::DodgeStarted { side, direction });
        }
    }

    /// Emits the events of a phase that begins on this tick.
    fn phase_events(&mut self, side: Side, events: &mut StepEvents) {
        let index = side.index();
        if self.combatants[index].is_frozen() {
            return;
        }
        let spec = *self.attack_spec(side);
        let Action::Attack {
            swing,
            elapsed,
            hits,
        } = *self.combatants[index].action()
        else {
            return;
        };
        if elapsed == spec.active_start() {
            events.push(CombatEvent::SwingActive { side, swing });
        } else if elapsed == spec.active_end() && !hits[side.other().index()] {
            self.counters.whiffs[index] = self.counters.whiffs[index].saturating_add(1);
            events.push(CombatEvent::SwingWhiffed { side, swing });
        }
    }

    /// Applies one body's movement intent under the rules.
    fn move_combatant(&mut self, side: Side, intent: Intent, ground: Option<&dyn GroundSampler>) {
        let index = side.index();
        if self.combatants[index].is_frozen() {
            return;
        }
        let step = tick_seconds();
        let spec = *self.attack_spec(side);
        let movement = *self.tuning.movement();
        let action = *self.combatants[index].action();
        let facing = self.combatants[index].state().facing;

        // Velocity is decided by the action first and the intent second: a
        // committed swing or dodge owns the body.
        let velocity = match action {
            Action::Free => intent.move_world() * movement.speed(),
            Action::Attack { elapsed, .. } => {
                if elapsed < spec.active_end() {
                    // The step-in is spread evenly over windup and active, so a
                    // swing arrives at the distance its telegraph promised.
                    let ticks = spec.active_end().max(1);
                    let forward = veldwake_character::pose::facing_direction(facing);
                    let per_tick = spec.step_in() / ticks as f32;
                    Vec2::new(forward.x, forward.z) * (per_tick / step)
                } else {
                    intent.move_world() * movement.speed() * movement.recovery_speed_scale()
                }
            }
            Action::Dodge { direction, .. } => direction * self.tuning.dodge().speed(),
            Action::Stagger { .. } | Action::Defeated { .. } => Vec2::ZERO,
        };

        // The specs are copied into locals because `MoveRules` borrows them and
        // the move below needs the combatant mutably at the same time.
        let arena = *self.tuning.arena();
        let rules = MoveRules {
            movement: &movement,
            arena: &arena,
            ground,
        };
        let result = try_move(self.combatants[index].state_mut(), velocity * step, &rules);
        if result.blocked {
            self.counters.blocked_moves[index] =
                self.counters.blocked_moves[index].saturating_add(1);
        }

        // The gait's phase advances with the distance actually travelled, not
        // with the distance asked for. Without this a body pushed against a wall
        // or against the other combatant slides its feet.
        let realised = result.distance() / step;
        let state = self.combatants[index].state_mut();
        state.speed = realised;

        // Facing: a committed action owns it, otherwise the body turns toward
        // what it is doing.
        let max_turn = movement.turn_rate() * step;
        let target = if intent.face_foe() {
            let to_foe = self.combatants[side.other().index()].position()
                - self.combatants[index].position();
            facing_of(to_foe)
        } else {
            facing_of(intent.move_world())
        };
        let turnable = matches!(action, Action::Free)
            || matches!(action, Action::Attack { elapsed, .. } if elapsed >= spec.active_end());
        if turnable && let Some(target) = target {
            let turned = turn_toward(facing, target, max_turn);
            self.combatants[index].state_mut().facing = turned;
        }

        let character = &self.characters[index];
        self.combatants[index]
            .state_mut()
            .advance(step, character, ground);
    }

    /// Pushes the two bodies apart if they overlap.
    fn separate_bodies(&mut self, ground: Option<&dyn GroundSampler>) {
        let radii = (
            self.combatants[0].capsule().radius,
            self.combatants[1].capsule().radius,
        );
        let overlap = crate::movement::overlap(
            self.combatants[0].position(),
            self.combatants[1].position(),
            radii.0,
            radii.1,
        );
        if overlap <= 0.0 {
            return;
        }
        self.counters.separations = self.counters.separations.saturating_add(1);
        let movement = *self.tuning.movement();
        let arena = *self.tuning.arena();
        let rules = MoveRules {
            movement: &movement,
            arena: &arena,
            ground,
        };
        let (first, second) = self.combatants.split_at_mut(1);
        let Some(first) = first.first_mut() else {
            unreachable!("the array has two entries");
        };
        let Some(second) = second.first_mut() else {
            unreachable!("the array has two entries");
        };
        separate(first.state_mut(), second.state_mut(), radii, &rules);
    }

    /// Recomputes one body's pose and records where its blade is.
    fn repose(&mut self, side: Side, ground: Option<&dyn GroundSampler>) {
        let index = side.index();
        let spec = *self.attack_spec(side);
        let combatant = &self.combatants[index];
        let overlay =
            combatant
                .action()
                .overlay(&spec, combatant.weapon_side(), combatant.state().facing);
        let state = *combatant.state();
        let previous = self.blade_world(side);
        let posed = pose_with(&self.characters[index], &state, ground, Some(&overlay));
        self.combatants[index].set_posed(posed);
        self.combatants[index].set_blade(previous);
    }

    /// Sweeps one side's blade and applies what it touched.
    fn resolve_hits(
        &mut self,
        attacker: Side,
        ground: Option<&dyn GroundSampler>,
        events: &mut StepEvents,
    ) {
        let index = attacker.index();
        if self.combatants[index].is_frozen() {
            return;
        }
        let spec = *self.attack_spec(attacker);
        let Action::Attack { elapsed, hits, .. } = *self.combatants[index].action() else {
            return;
        };
        if !spec.is_active(elapsed) {
            return;
        }
        // Death cancels the swing: a body defeated earlier in this same tick does
        // not land the blade it had in the air.
        if self.combatants[index].action().is_defeated() {
            return;
        }
        let victim = attacker.other();
        let victim_index = victim.index();
        if self.combatants[victim_index].action().is_defeated() {
            return;
        }
        if hits[victim_index] {
            self.counters.multi_hit_suppressed =
                self.counters.multi_hit_suppressed.saturating_add(1);
            return;
        }

        let end = self.blade_world(attacker);
        let start = self.combatants[index].previous_blade().unwrap_or(end);
        let sweep = Sweep::new(start, end, self.weapon.blade_radius_world());
        self.counters.hit_queries = self.counters.hit_queries.saturating_add(1);
        self.counters.sweep_substeps_max = self.counters.sweep_substeps_max.max(sweep.substeps());
        let capsule = self.combatants[victim_index].hurt_capsule();
        let Some(contact) = sweep_capsule(&sweep, &capsule) else {
            return;
        };

        // One hit per swing, marked on the swing itself.
        if !self.combatants[index].register_hit(victim) {
            self.counters.multi_hit_suppressed =
                self.counters.multi_hit_suppressed.saturating_add(1);
            return;
        }
        self.counters.hits[index] = self.counters.hits[index].saturating_add(1);

        let damage = spec.damage();
        let remaining = self.combatants[victim_index].health_mut().apply(damage);
        let from = {
            let offset =
                self.combatants[victim_index].position() - self.combatants[index].position();
            if offset.length_squared() > 1.0e-8 {
                offset.normalize()
            } else {
                Vec2::new(1.0, 0.0)
            }
        };
        events.push(CombatEvent::Hit {
            attacker,
            victim,
            point: contact.point,
            damage,
            remaining,
            from,
        });

        if remaining == 0 {
            self.combatants[victim_index].set_action(Action::Defeated { elapsed: 0 });
            self.counters.defeats[victim_index] =
                self.counters.defeats[victim_index].saturating_add(1);
            events.push(CombatEvent::Defeated { side: victim });
            if self.outcome.is_none() {
                self.outcome = Some(victim);
            }
        } else {
            self.combatants[victim_index].set_action(Action::Stagger {
                elapsed: 0,
                duration: spec.stagger(),
                from,
            });
            self.counters.staggers[victim_index] =
                self.counters.staggers[victim_index].saturating_add(1);
            events.push(CombatEvent::Staggered { side: victim });
            // Knockback goes through the same movement rules as a footstep, then
            // the victim is re-posed so the drawn body is where it was pushed.
            let movement = *self.tuning.movement();
            let arena = *self.tuning.arena();
            let rules = MoveRules {
                movement: &movement,
                arena: &arena,
                ground,
            };
            let _ = try_move(
                self.combatants[victim_index].state_mut(),
                from * spec.knockback(),
                &rules,
            );
            self.repose(victim, ground);
        }

        // Hitstop lands on both. It takes hold at step 9 of this same tick, after
        // everything above has been resolved and published, so the first frozen
        // advance is this tick's.
        let hitstop = spec.hitstop();
        if hitstop > 0 {
            self.combatants[index].set_hitstop(hitstop);
            self.combatants[victim_index].set_hitstop(hitstop);
        }
    }

    /// Runs the defeat hold and resets the encounter when it expires.
    fn update_outcome(&mut self, ground: Option<&dyn GroundSampler>, events: &mut StepEvents) {
        let Some(defeated) = self.outcome else {
            return;
        };
        let elapsed = match self.combatants[defeated.index()].action() {
            Action::Defeated { elapsed } => *elapsed,
            // The defeated body's action was replaced, which would be a bug in
            // the ordering above. Resetting is the safe answer and the counter
            // below makes it visible.
            _ => self.tuning.defeat_hold(),
        };
        if elapsed < self.tuning.defeat_hold() {
            return;
        }
        for side in SIDES {
            let state = start_state(self.tuning.arena().centre(), self.offsets, side, ground);
            self.combatants[side.index()].reset(state);
        }
        self.brain.reset();
        self.outcome = None;
        self.counters.resets = self.counters.resets.saturating_add(1);
        for side in SIDES {
            self.repose(side, ground);
            self.combatants[side.index()].clear_blade();
        }
        events.push(CombatEvent::EncounterReset);
    }

    /// Moves one body's action one tick further on, ending it if it is over.
    ///
    /// An action that has run its course returns to `Free` on its own last tick
    /// rather than on the next one, so `total` ticks of a swing means exactly
    /// `total` and there is no tick where a spent action is still current.
    fn advance_action(&mut self, side: Side) {
        let index = side.index();
        if self.combatants[index].is_frozen() {
            // The hitstop is spent at the end of the tick it froze, so a hitstop
            // of `n` ticks freezes exactly `n` of them.
            self.combatants[index].tick_hitstop();
            return;
        }
        self.combatants[index].tick_dodge_cooldown();
        let spec = *self.attack_spec(side);
        let action = *self.combatants[index].action();
        let advanced = match action {
            Action::Free => Action::Free,
            Action::Attack {
                swing,
                elapsed,
                hits,
            } => Action::Attack {
                swing,
                elapsed: elapsed.saturating_add(1),
                hits,
            },
            Action::Dodge {
                elapsed,
                duration,
                direction,
            } => Action::Dodge {
                elapsed: elapsed.saturating_add(1),
                duration,
                direction,
            },
            Action::Stagger {
                elapsed,
                duration,
                from,
            } => Action::Stagger {
                elapsed: elapsed.saturating_add(1),
                duration,
                from,
            },
            Action::Defeated { elapsed } => Action::Defeated {
                elapsed: elapsed.saturating_add(1),
            },
        };
        let over = match advanced {
            Action::Attack { elapsed, .. } => elapsed >= spec.total(),
            Action::Dodge {
                elapsed, duration, ..
            }
            | Action::Stagger {
                elapsed, duration, ..
            } => elapsed >= duration,
            // A defeat holds until the encounter resets it.
            Action::Free | Action::Defeated { .. } => false,
        };
        self.combatants[index].set_action(if over { Action::Free } else { advanced });
    }
}

/// Where one side starts, settled on the ground.
fn start_state(
    centre: Vec2,
    offsets: [Vec2; SIDES.len()],
    side: Side,
    ground: Option<&dyn GroundSampler>,
) -> CharacterState {
    let index = side.index();
    let position = centre + offsets[index];
    let other = centre + offsets[side.other().index()];
    // Each starts facing the other, because a faceoff is the first frame of
    // evidence and two bodies looking past each other is not one.
    let facing = facing_of(other - position).unwrap_or(0.0);
    CharacterState::standing(position.x, position.y, facing, ground)
}

/// Ticks a whole action would take, for the probe and the tests.
#[must_use]
pub fn action_duration(action: &Action, spec: &AttackSpec) -> Ticks {
    match action {
        Action::Free => 0,
        Action::Attack { .. } => spec.total(),
        Action::Dodge { duration, .. } | Action::Stagger { duration, .. } => *duration,
        Action::Defeated { .. } => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{CountingGround, Encounter, EncounterError, EncounterSetup, action_duration};
    use crate::combatant::{Action, Intent, SIDES, Side};
    use crate::event::CombatEvent;
    use crate::fixture;
    use crate::spec::AuthoredTuning;
    use glam::Vec2;
    use veldwake_character::ground::{FlatGround, StepGround};
    use veldwake_character::skeleton::BoneId;
    use veldwake_character::{CHARACTER_VOXEL_SIZE, GroundSampler};

    fn ground() -> FlatGround {
        fixture::golden_ground()
    }

    fn armed(setup: &EncounterSetup, ground: &dyn GroundSampler) -> Encounter {
        let mut encounter = match Encounter::new(setup, Some(ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("the golden encounter must build: {error}"),
        };
        encounter.arm();
        encounter
    }

    /// Steps with no player input until the adversary's swing reaches a tick.
    fn wait_for_adversary_windup(
        encounter: &mut Encounter,
        ground: &dyn GroundSampler,
        target: u32,
        limit: u32,
    ) -> bool {
        for _ in 0..limit {
            if matches!(encounter.combatant(Side::Adversary).action(),
                Action::Attack { elapsed, .. } if *elapsed == target)
            {
                return true;
            }
            let _ = encounter.step(Intent::idle(), Some(ground));
        }
        false
    }

    #[test]
    fn a_step_takes_no_duration_and_advances_exactly_one_tick() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        assert_eq!(encounter.tick_index(), 0);
        for expected in 1..=10 {
            let _ = encounter.step(Intent::idle(), Some(&ground));
            assert_eq!(encounter.tick_index(), expected);
        }
        assert_eq!(encounter.counters().ticks, 10);
    }

    #[test]
    fn an_unarmed_encounter_breathes_but_does_not_fight() {
        let ground = ground();
        let setup = fixture::golden_setup();
        let mut encounter = match Encounter::new(&setup, Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        assert!(!encounter.is_armed());
        let before = encounter.combatant(Side::Adversary).position();
        for _ in 0..600 {
            let events = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), true, true),
                Some(&ground),
            );
            assert!(events.is_empty(), "a paused encounter must publish nothing");
        }
        assert_eq!(
            encounter.combatant(Side::Adversary).position(),
            before,
            "nobody moves before the fight is armed"
        );
        assert!(encounter.combatant(Side::Player).action().is_free());
        // The idle breath still runs, which is what keeps a faceoff alive.
        assert!(encounter.combatant(Side::Player).state().time > 4.9);
        encounter.arm();
        assert!(encounter.is_armed());
    }

    #[test]
    fn the_active_window_opens_and_closes_on_the_exact_ticks_the_spec_names() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let mut active_ticks = Vec::new();
        let mut started = None;
        let mut went_active = None;
        let mut whiffed = None;
        for tick in 0..spec.total() + 4 {
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            for event in events.iter() {
                match event {
                    CombatEvent::SwingStarted {
                        side: Side::Player, ..
                    } => started = Some(tick),
                    CombatEvent::SwingActive {
                        side: Side::Player, ..
                    } => went_active = Some(tick),
                    CombatEvent::SwingWhiffed {
                        side: Side::Player, ..
                    } => whiffed = Some(tick),
                    _ => {}
                }
            }
            if let Action::Attack { elapsed, .. } = encounter.combatant(Side::Player).action()
                && spec.is_active(*elapsed)
            {
                active_ticks.push(*elapsed);
            }
        }
        assert_eq!(
            started,
            Some(0),
            "the swing starts on the tick it is asked for"
        );
        // The phase events land on the tick the phase begins, counted from the
        // swing's start rather than from the encounter's.
        assert_eq!(
            went_active,
            Some(spec.active_start()),
            "the active phase must announce itself on its first tick"
        );
        assert_eq!(
            whiffed,
            Some(spec.active_end()),
            "an untouched swing must announce its miss on the first recovery tick"
        );
        // Exactly the ticks the spec declares, no more and no fewer.
        let expected: Vec<u32> = (spec.active_start()..spec.active_end()).collect();
        assert_eq!(active_ticks, expected, "the active window drifted");
    }

    #[test]
    fn a_swing_returns_the_body_to_free_on_the_tick_after_its_last() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let _ = encounter.step(Intent::player(Vec2::ZERO, true, false), Some(&ground));
        // One step has already run, so the swing has `total - 1` left.
        for remaining in 1..spec.total() {
            assert!(
                encounter.combatant(Side::Player).action().is_attacking(),
                "the swing ended early, at tick {remaining} of {}",
                spec.total()
            );
            let _ = encounter.step(Intent::idle(), Some(&ground));
        }
        assert!(
            encounter.combatant(Side::Player).action().is_free(),
            "the swing must be over after exactly {} ticks",
            spec.total()
        );
        assert_eq!(
            action_duration(
                &Action::Attack {
                    swing: crate::combatant::SwingId::first(),
                    elapsed: 0,
                    hits: [false; 2]
                },
                &spec
            ),
            spec.total()
        );
        assert_eq!(action_duration(&Action::Free, &spec), 0);
    }

    #[test]
    fn one_swing_lands_exactly_one_hit_however_long_it_overlaps() {
        let ground = ground();
        let mut encounter = armed(&fixture::sandbox_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let mut hits = 0;
        for tick in 0..spec.total() + 2 {
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            hits += events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        CombatEvent::Hit {
                            attacker: Side::Player,
                            ..
                        }
                    )
                })
                .count();
        }
        assert_eq!(hits, 1, "one swing must land once");
        assert!(
            encounter.counters().multi_hit_suppressed > 0,
            "the suppression must have had work to do, or the test proves nothing"
        );
    }

    #[test]
    fn swinging_whenever_the_body_is_free_lands_one_hit_per_swing() {
        // The real pattern rather than a fixed schedule: the first hit freezes the
        // attacker and knocks the victim back, so "press again after exactly
        // `total` ticks" is not what a player does and not what the rules expect.
        let ground = ground();
        let mut encounter = armed(&fixture::sandbox_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let mut hits = 0_u32;
        let mut swings = 0_u32;
        for _ in 0..spec.total() * 6 {
            let free = encounter.combatant(Side::Player).can_act();
            let events = encounter.step(
                Intent::player(Vec2::new(0.0, -1.0), free, false),
                Some(&ground),
            );
            for event in events.iter() {
                match event {
                    CombatEvent::Hit {
                        attacker: Side::Player,
                        ..
                    } => hits += 1,
                    CombatEvent::SwingStarted {
                        side: Side::Player, ..
                    } => swings += 1,
                    _ => {}
                }
            }
        }
        assert!(swings >= 3, "only {swings} swings in six timelines");
        assert!(hits >= 2, "only {hits} hits from {swings} swings");
        assert!(
            hits <= swings,
            "{hits} hits from {swings} swings means a swing hit twice"
        );
        assert_eq!(encounter.counters().hits[0], hits);
        assert_eq!(encounter.counters().swings[0], swings);
    }

    #[test]
    fn a_hit_damages_staggers_knocks_back_and_freezes_both_bodies() {
        let ground = ground();
        let mut encounter = armed(&fixture::sandbox_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let before_health = encounter.combatant(Side::Adversary).health().current();
        let mut hit_event = None;
        for tick in 0..spec.total() {
            // The player steps in while it swings, so the baseline for the
            // knockback is the victim's position at the start of the hit tick
            // rather than at the start of the swing.
            let victim_before = encounter.combatant(Side::Adversary).position();
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            if let Some(event) = events.first_hit() {
                hit_event = Some(event);
                // Everything a hit produces, on the tick it produced it.
                assert!(
                    events.any(|event| matches!(
                        event,
                        CombatEvent::Staggered {
                            side: Side::Adversary
                        }
                    )),
                    "a hit must stagger"
                );
                assert!(
                    matches!(
                        encounter.combatant(Side::Adversary).action(),
                        Action::Stagger { .. }
                    ),
                    "the victim must be in a stagger"
                );
                let CombatEvent::Hit { from, .. } = event else {
                    panic!("first_hit returned something else");
                };
                let pushed =
                    (encounter.combatant(Side::Adversary).position() - victim_before).dot(from);
                assert!(
                    pushed > spec.knockback() * 0.5,
                    "the victim moved {pushed:.4} along the hit, expected about {:.4}",
                    spec.knockback()
                );
                break;
            }
        }
        let Some(CombatEvent::Hit {
            damage,
            remaining,
            point,
            from,
            ..
        }) = hit_event
        else {
            panic!("the swing must connect at this distance");
        };
        assert_eq!(damage, spec.damage());
        assert_eq!(remaining, before_health - damage);
        assert!(
            point.is_finite(),
            "the contact point must be usable by a VFX"
        );
        assert!(
            from.is_finite() && from.length() > 0.9,
            "the hit needs a direction"
        );
        // Hitstop lands on both, from the next tick.
        let _ = encounter.step(Intent::idle(), Some(&ground));
        assert!(encounter.combatant(Side::Player).is_frozen());
        assert!(encounter.combatant(Side::Adversary).is_frozen());
    }

    #[test]
    fn hitstop_freezes_the_timeline_without_stopping_the_tick() {
        let ground = ground();
        let mut encounter = armed(&fixture::sandbox_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        for tick in 0..spec.total() {
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            if events.first_hit().is_some() {
                break;
            }
        }
        let frozen_elapsed = encounter.combatant(Side::Player).action().elapsed();
        let frozen_position = encounter.combatant(Side::Player).position();
        let ticks_before = encounter.tick_index();
        // The hit tick was itself the first frozen advance, so there are
        // `hitstop - 1` left.
        for _ in 0..spec.hitstop() - 1 {
            let _ = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), false, false),
                Some(&ground),
            );
            assert_eq!(
                encounter.combatant(Side::Player).action().elapsed(),
                frozen_elapsed,
                "a frozen timeline must not advance"
            );
            assert_eq!(
                encounter.combatant(Side::Player).position(),
                frozen_position,
                "a frozen body must not move, even when asked to"
            );
        }
        assert_eq!(
            encounter.tick_index(),
            ticks_before + u64::from(spec.hitstop() - 1),
            "the encounter's own clock keeps running"
        );
        assert_eq!(
            encounter.counters().frozen_ticks[0],
            spec.hitstop() - 1,
            "the freeze is counted"
        );
        // And then it thaws, on the tick after the last frozen one.
        let _ = encounter.step(Intent::idle(), Some(&ground));
        assert!(!encounter.combatant(Side::Player).is_frozen());
        assert_eq!(
            encounter.combatant(Side::Player).action().elapsed(),
            frozen_elapsed + 1,
            "the timeline must resume where it stopped"
        );
    }

    #[test]
    fn death_cancels_the_blade_that_was_already_swinging() {
        // Both bodies swing with their active windows overlapping and one hit
        // point each. The policy is that hits resolve in the fixed order
        // `[Player, Adversary]` and a body defeated earlier in the same tick does
        // not land its own swing — so exactly one of them falls, never both, and
        // the survivor is untouched.
        let ground = ground();
        let mut setup = fixture::golden_setup();
        setup.player_offset = Vec2::new(0.0, 1.0);
        setup.adversary_offset = Vec2::new(0.0, -1.0);
        setup.tuning = AuthoredTuning {
            player_health: 1,
            adversary_health: 1,
            ..setup.tuning
        };
        let mut encounter = armed(&setup, &ground);
        let player_spec = *encounter.attack_spec(Side::Player);
        let adversary_spec = *encounter.attack_spec(Side::Adversary);
        // Line the two active windows up: the player's opens at `windup`, so the
        // press has to happen that many ticks before the adversary's does.
        let press_at = adversary_spec.active_start() - player_spec.active_start();
        assert!(
            wait_for_adversary_windup(&mut encounter, &ground, press_at, 4_000),
            "the adversary never committed"
        );
        let mut defeats = Vec::new();
        let mut hits = Vec::new();
        for tick in 0..adversary_spec.total() {
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            for event in events.iter() {
                match event {
                    CombatEvent::Defeated { side } => defeats.push(side),
                    CombatEvent::Hit { attacker, .. } => hits.push(attacker),
                    _ => {}
                }
            }
            if !defeats.is_empty() {
                break;
            }
        }
        assert_eq!(
            defeats.len(),
            1,
            "a double defeat must be impossible: {defeats:?}"
        );
        let Some(fallen) = defeats.first().copied() else {
            panic!("somebody has to fall with one hit point each");
        };
        assert_eq!(
            encounter.outcome(),
            Some(fallen),
            "the outcome must name the body that fell"
        );
        let survivor = fallen.other();
        assert_eq!(
            encounter.combatant(survivor).health().current(),
            1,
            "the survivor must be untouched: death cancels the other blade"
        );
        assert_eq!(hits.len(), 1, "only one blade may land: {hits:?}");
        assert_eq!(hits.first().copied(), Some(survivor));
        // And the fixed order is the player's, which is what gives the player the
        // tie when both blades would connect on the same tick.
        assert_eq!(SIDES[0], Side::Player);
    }

    #[test]
    fn a_defeat_holds_and_then_resets_the_whole_encounter() {
        let ground = ground();
        let mut setup = fixture::sandbox_setup();
        setup.tuning = AuthoredTuning {
            adversary_health: 1,
            ..setup.tuning
        };
        let mut encounter = armed(&setup, &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let hold = encounter.tuning().defeat_hold();
        let start = encounter.combatant(Side::Player).position();
        for tick in 0..spec.total() {
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            if events.any(|event| matches!(event, CombatEvent::Defeated { .. })) {
                break;
            }
        }
        assert_eq!(encounter.outcome(), Some(Side::Adversary));
        let mut reset = false;
        for _ in 0..hold * 3 {
            let events = encounter.step(Intent::idle(), Some(&ground));
            if events.any(|event| matches!(event, CombatEvent::EncounterReset)) {
                reset = true;
                break;
            }
        }
        assert!(reset, "the encounter must come back on its own");
        assert!(encounter.outcome().is_none());
        assert_eq!(encounter.counters().resets, 1);
        for side in SIDES {
            let combatant = encounter.combatant(side);
            assert_eq!(
                combatant.health().current(),
                combatant.health().max(),
                "{} must come back whole",
                side.name()
            );
            assert!(combatant.action().is_free());
            assert_eq!(combatant.hitstop(), 0);
        }
        assert!(
            (encounter.combatant(Side::Player).position() - start).length() < 1.0e-4,
            "a reset must put the bodies back where they started"
        );
        // A swing identifier is never reused, even across a reset.
        let _ = encounter.step(Intent::player(Vec2::ZERO, true, false), Some(&ground));
        match encounter.combatant(Side::Player).action() {
            Action::Attack { swing, .. } => assert!(swing.raw() > 0),
            other => panic!("expected a swing, got {other:?}"),
        }
    }

    #[test]
    fn a_reset_publishes_its_event_even_with_a_hit_in_the_same_tick() {
        // Presentation drains events per tick, so a reset arriving beside a hit
        // must not be lost: a VFX or a sound queued from that hit is about to
        // reference bodies that have just moved.
        let ground = ground();
        let mut setup = fixture::sandbox_setup();
        setup.tuning = AuthoredTuning {
            adversary_health: 1,
            defeat_hold_seconds: 1.0 / 120.0,
            ..setup.tuning
        };
        let mut encounter = armed(&setup, &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let mut saw_defeat = false;
        let mut saw_reset = false;
        for tick in 0..spec.total() + 8 {
            let events =
                encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            saw_defeat |= events.any(|event| matches!(event, CombatEvent::Defeated { .. }));
            saw_reset |= events.any(|event| matches!(event, CombatEvent::EncounterReset));
            assert_eq!(events.dropped(), 0, "a tick lost an event");
        }
        assert!(saw_defeat && saw_reset, "both events must reach the caller");
    }

    #[test]
    fn a_dodge_is_refused_during_an_action_and_while_it_is_cooling_down() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let dodge = *encounter.tuning().dodge();
        let _ = encounter.step(
            Intent::player(Vec2::new(0.0, -1.0), false, true),
            Some(&ground),
        );
        assert!(encounter.combatant(Side::Player).action().is_dodging());
        assert_eq!(encounter.counters().dodges[0], 1);
        // Asking again mid dodge is refused and counted.
        let _ = encounter.step(Intent::player(Vec2::ZERO, false, true), Some(&ground));
        assert_eq!(encounter.counters().dodges[0], 1);
        assert!(encounter.counters().dodges_refused[0] >= 1);
        // And asking to attack mid dodge is refused too: commitment is the point.
        let _ = encounter.step(Intent::player(Vec2::ZERO, true, false), Some(&ground));
        assert!(encounter.combatant(Side::Player).action().is_dodging());
        for _ in 0..dodge.duration() {
            let _ = encounter.step(Intent::idle(), Some(&ground));
        }
        assert!(encounter.combatant(Side::Player).action().is_free());
        // Still cooling down.
        let _ = encounter.step(Intent::player(Vec2::ZERO, false, true), Some(&ground));
        assert_eq!(encounter.counters().dodges[0], 1, "the cooldown must hold");
        for _ in 0..dodge.cooldown() + 2 {
            let _ = encounter.step(Intent::idle(), Some(&ground));
        }
        let _ = encounter.step(
            Intent::player(Vec2::new(0.0, -1.0), false, true),
            Some(&ground),
        );
        assert_eq!(encounter.counters().dodges[0], 2, "and then release");
    }

    #[test]
    fn a_dodge_with_no_direction_goes_backwards() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let before = encounter.separation_distance();
        let _ = encounter.step(Intent::player(Vec2::ZERO, false, true), Some(&ground));
        match encounter.combatant(Side::Player).action() {
            Action::Dodge { direction, .. } => {
                let forward = veldwake_character::pose::facing_direction(
                    encounter.combatant(Side::Player).state().facing,
                );
                let back = -Vec2::new(forward.x, forward.z);
                assert!(
                    (*direction - back).length() < 1.0e-4,
                    "a dodge with no direction must go straight back"
                );
            }
            other => panic!("expected a dodge, got {other:?}"),
        }
        for _ in 0..30 {
            let _ = encounter.step(Intent::idle(), Some(&ground));
        }
        assert!(
            encounter.separation_distance() > before,
            "backwards must mean away from the body in front"
        );
    }

    #[test]
    fn a_dodge_escapes_a_swing_it_is_early_enough_for_and_not_one_it_is_late_for() {
        // The dodge distance is chosen from this, not from arithmetic: the first
        // active ticks pass above a body and the last ones pass through it, so
        // reach alone gets the answer wrong.
        let ground = ground();
        let setup = fixture::golden_setup();
        let encounter = match Encounter::new(&setup, Some(&ground)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        let spec = *encounter.attack_spec(Side::Adversary);
        drop(encounter);
        let probe = |pressed_at, direction| match fixture::probe_dodge(
            &setup,
            Some(&ground),
            pressed_at,
            direction,
            2_400,
        ) {
            Ok(probe) => probe,
            Err(error) => panic!("{error}"),
        };
        for direction in [
            fixture::DodgeDirection::Away,
            fixture::DodgeDirection::Lateral,
        ] {
            // Early in the telegraph: escapes.
            let early = probe(4, direction);
            assert!(early.started, "the dodge must be accepted");
            assert!(
                !early.hit,
                "dodging {} early in the telegraph must escape",
                direction.name()
            );
            // After the blade is already live: hit.
            let late = probe(spec.active_start() + 2, direction);
            assert!(
                late.hit,
                "dodging {} after the blade is out must not save anybody",
                direction.name()
            );
            // There is a real boundary between the two, and it is inside the
            // telegraph rather than at its edge.
            let mut last_escape = None;
            for pressed_at in 1..spec.active_end() {
                if !probe(pressed_at, direction).hit {
                    last_escape = Some(pressed_at);
                }
            }
            let Some(last_escape) = last_escape else {
                panic!("no dodge escapes a {} swing at all", direction.name());
            };
            assert!(
                last_escape < spec.active_start(),
                "the last escaping press ({last_escape}) must be inside the telegraph"
            );
            assert!(
                last_escape >= spec.active_start() / 2,
                "the window to react ({last_escape} of {}) is too small to be read",
                spec.active_start()
            );
        }
    }

    #[test]
    fn the_adversary_never_reaches_its_active_window_without_a_whole_telegraph() {
        // The readability invariant: a blade that goes live without its full
        // warning is the defect the whole encounter is judged on.
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Adversary);
        let mut windup_ticks = 0_u32;
        let mut swings = 0_u32;
        let mut previous: Option<u32> = None;
        for _ in 0..6_000 {
            let _ = encounter.step(Intent::idle(), Some(&ground));
            let elapsed = match encounter.combatant(Side::Adversary).action() {
                Action::Attack { elapsed, .. } => Some(*elapsed),
                _ => None,
            };
            match (previous, elapsed) {
                (_, Some(now)) if now < spec.active_start() => windup_ticks += 1,
                (Some(before), Some(now)) if now == spec.active_start() => {
                    assert_eq!(
                        before,
                        spec.active_start() - 1,
                        "the blade went live without the tick before it"
                    );
                    // A swing's tick zero happens inside the step that starts it,
                    // so an observer outside the step sees the windup from one.
                    assert!(
                        windup_ticks + 1 >= spec.active_start(),
                        "only {windup_ticks} ticks of telegraph before the blade"
                    );
                    swings += 1;
                    windup_ticks = 0;
                }
                (_, None) => windup_ticks = 0,
                _ => {}
            }
            previous = elapsed;
        }
        assert!(
            swings >= 2,
            "the adversary must actually swing, got {swings}"
        );
    }

    #[test]
    fn the_hurt_capsule_contains_the_core_of_the_body_at_every_moment_of_a_fight() {
        // The accepted limitation, turned into a checked property: the arms, the
        // hands and the weapon may leave the capsule, and a swing that only
        // grazes an outstretched arm therefore does not register. The head, the
        // torso and the legs may not leave it.
        let core = [
            BoneId::Root,
            BoneId::Spine,
            BoneId::Chest,
            BoneId::Head,
            BoneId::ThighL,
            BoneId::ThighR,
        ];
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let mut runner = crate::script::ScriptRunner::new(
            crate::script::GOLDEN_SCRIPT,
            fixture::reach_of(&encounter),
        );
        let mut worst = 0.0_f32;
        for _ in 0..fixture::GOLDEN_RUN_TICKS {
            let intent = runner.next_intent(&encounter);
            let _ = encounter.step(intent, Some(&ground));
            if runner.finished() {
                runner.restart();
            }
            for side in SIDES {
                let combatant = encounter.combatant(side);
                let capsule = combatant.hurt_capsule();
                let collision = encounter.character(side).collision();
                let matrices = combatant.posed().part_matrices();
                for bone in core {
                    let part = collision.part_box(bone);
                    let matrix = matrices[bone.index()];
                    for corner in part.corners() {
                        let point = matrix.transform_point3(corner);
                        if !capsule.contains(point) {
                            let (_, on_axis, squared) = crate::hit::closest_points(
                                capsule.axis,
                                crate::hit::Segment::new(point, point),
                            );
                            let overshoot = squared.sqrt() - capsule.radius;
                            worst = worst.max(overshoot);
                            assert!(
                                overshoot <= CHARACTER_VOXEL_SIZE,
                                "{} {}'s corner is {overshoot:.4} outside its own hurt capsule (axis point {on_axis})",
                                side.name(),
                                bone.name()
                            );
                        }
                    }
                }
            }
        }
        // A number rather than a pass: the margin is what a later change erodes.
        assert!(
            worst <= CHARACTER_VOXEL_SIZE,
            "worst core overshoot was {worst:.4} world units"
        );
    }

    #[test]
    fn the_blade_never_jumps_between_two_ticks_of_a_swing() {
        // A blade that teleports at a phase change would let the sweep report a
        // hit in geometry the animation never showed.
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        let mut previous = encounter.blade_world(Side::Player);
        let mut worst_tip = 0.0_f32;
        let mut worst_base = 0.0_f32;
        for tick in 0..spec.total() + 4 {
            let _ = encounter.step(Intent::player(Vec2::ZERO, tick == 0, false), Some(&ground));
            let blade = encounter.blade_world(Side::Player);
            worst_tip = worst_tip.max((blade.tip - previous.tip).length());
            worst_base = worst_base.max((blade.base - previous.base).length());
            previous = blade;
        }
        // The fastest part of the swing moves the tip about a fifth of a world
        // unit per tick; a discontinuity would be several times that.
        assert!(
            worst_tip < 0.5,
            "the tip jumped {worst_tip:.4} world units in one tick"
        );
        assert!(
            worst_base < 0.25,
            "the base jumped {worst_base:.4} world units in one tick"
        );
    }

    #[test]
    fn a_passive_player_is_eventually_defeated() {
        // The other half of the loop: the player must be able to lose.
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let mut defeated = None;
        for _ in 0..12_000 {
            let events = encounter.step(Intent::idle(), Some(&ground));
            for event in events.iter() {
                if let CombatEvent::Defeated { side } = event {
                    defeated = Some(side);
                }
            }
            if defeated.is_some() {
                break;
            }
        }
        assert_eq!(
            defeated,
            Some(Side::Player),
            "standing still in front of an armed adversary must be fatal"
        );
    }

    #[test]
    fn the_arena_holds_both_bodies_whatever_they_are_asked_to_do() {
        let ground = ground();
        let setup = fixture::golden_setup();
        let mut encounter = armed(&setup, &ground);
        let arena = *encounter.tuning().arena();
        for tick in 0..4_000 {
            // Drive hard at the boundary, alternating direction.
            let push = if (tick / 200) % 2 == 0 {
                Vec2::new(1.0, 0.3)
            } else {
                Vec2::new(-0.6, -1.0)
            };
            let _ = encounter.step(
                Intent::player(push, tick % 97 == 0, tick % 53 == 0),
                Some(&ground),
            );
            for side in SIDES {
                let position = encounter.combatant(side).position();
                assert!(
                    arena.contains(position),
                    "{} left the arena at {position}",
                    side.name()
                );
                assert!(position.is_finite());
            }
        }
    }

    #[test]
    fn separation_keeps_the_two_bodies_out_of_each_other() {
        let ground = ground();
        let mut setup = fixture::golden_setup();
        // Start them on top of each other, which is the case a division by the
        // distance would turn into a NaN.
        setup.player_offset = Vec2::ZERO;
        setup.adversary_offset = Vec2::ZERO;
        let mut encounter = armed(&setup, &ground);
        let mut worst = 0.0_f32;
        for tick in 0..2_000 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.0, -1.0), tick % 61 == 0, false),
                Some(&ground),
            );
            let overlap = crate::movement::overlap(
                encounter.combatant(Side::Player).position(),
                encounter.combatant(Side::Adversary).position(),
                encounter.combatant(Side::Player).capsule().radius,
                encounter.combatant(Side::Adversary).capsule().radius,
            );
            if tick > 4 {
                worst = worst.max(overlap);
            }
            for side in SIDES {
                assert!(encounter.combatant(side).position().is_finite());
            }
        }
        assert!(
            worst < 0.02,
            "the bodies stayed {worst:.4} world units inside each other"
        );
        assert!(encounter.counters().separations > 0);
    }

    #[test]
    fn terrain_a_body_may_not_climb_stops_it_without_stopping_the_fight() {
        let step = StepGround {
            edge_x: 1.5,
            low: 0.0,
            high: 6.0,
        };
        let mut setup = fixture::golden_setup();
        setup.player_offset = Vec2::new(0.0, 0.0);
        setup.adversary_offset = Vec2::new(0.0, -2.0);
        let mut encounter = armed(&setup, &step);
        for _ in 0..1_200 {
            let _ = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), false, false),
                Some(&step),
            );
            for side in SIDES {
                let position = encounter.combatant(side).position();
                assert!(
                    position.x < 1.5,
                    "{} climbed a wall to {position}",
                    side.name()
                );
                assert!(position.is_finite());
            }
        }
        assert!(
            encounter.counters().blocked_moves[0] > 0,
            "the wall must actually have refused something"
        );
    }

    #[test]
    fn an_encounter_with_no_ground_still_runs() {
        // The diagnostic corridor has no walkable surface, and a client that
        // selects it must not be able to crash the fight.
        let setup = fixture::golden_setup();
        let mut encounter = match Encounter::new(&setup, None) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        encounter.arm();
        for tick in 0..600 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.4, -0.9), tick % 71 == 0, tick % 43 == 0),
                None,
            );
            for side in SIDES {
                assert!(encounter.combatant(side).position().is_finite());
                assert!(encounter.combatant(side).posed().is_finite());
            }
        }
    }

    #[test]
    fn hostile_intent_cannot_produce_an_impossible_state() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let hostile = [
            Vec2::splat(f32::NAN),
            Vec2::new(f32::INFINITY, 0.0),
            Vec2::new(0.0, f32::NEG_INFINITY),
            Vec2::splat(1.0e30),
            Vec2::ZERO,
        ];
        for tick in 0..2_000 {
            let move_world = hostile[tick % hostile.len()];
            let events = encounter.step(Intent::player(move_world, true, true), Some(&ground));
            assert_eq!(events.dropped(), 0);
            for side in SIDES {
                let combatant = encounter.combatant(side);
                assert!(combatant.position().is_finite());
                assert!(combatant.state().facing.is_finite());
                assert!(combatant.state().base_height.is_finite());
                assert!(combatant.state().speed.is_finite());
                assert!(combatant.state().phase.is_finite());
                assert!(combatant.posed().is_finite());
                assert!(combatant.hurt_capsule().is_finite());
                assert!(encounter.blade_world(side).is_finite());
                assert!(combatant.health().current() <= combatant.health().max());
            }
        }
    }

    #[test]
    fn a_fight_far_from_the_origin_behaves_like_one_at_it() {
        // The client's arena is at a negative coordinate, so this is the ordinary
        // case rather than an exotic one.
        let ground = ground();
        let reference = match fixture::run_script(
            crate::script::GOLDEN_SCRIPT,
            &fixture::golden_setup(),
            Some(&ground),
            1_500,
        ) {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error}"),
        };
        for centre in [
            Vec2::new(-69.0, 49.0),
            Vec2::new(-399.0, -399.0),
            Vec2::new(380.0, -120.0),
        ] {
            let setup = fixture::setup(centre, fixture::ARENA_RADIUS);
            let outcome = match fixture::run_script(
                crate::script::GOLDEN_SCRIPT,
                &setup,
                Some(&ground),
                1_500,
            ) {
                Ok(outcome) => outcome,
                Err(error) => panic!("{error}"),
            };
            assert_eq!(
                outcome.counters.hits, reference.counters.hits,
                "the fight at {centre} landed different hits"
            );
            assert_eq!(outcome.counters.swings, reference.counters.swings);
            assert_eq!(outcome.min_health, reference.min_health);
            assert_eq!(outcome.counters.events_dropped, 0);
        }
    }

    #[test]
    fn the_same_inputs_replay_exactly() {
        let ground = ground();
        let run = || match fixture::run_script(
            crate::script::GOLDEN_SCRIPT,
            &fixture::golden_setup(),
            Some(&ground),
            2_000,
        ) {
            Ok(outcome) => outcome,
            Err(error) => panic!("{error}"),
        };
        let first = run();
        let second = run();
        assert_eq!(first, second, "two runs of one script disagreed");
    }

    #[test]
    fn the_counting_ground_reports_what_a_tick_actually_asks_of_the_world() {
        let ground = ground();
        let counting = CountingGround::new(&ground);
        let setup = fixture::golden_setup();
        let mut encounter = match Encounter::new(&setup, Some(&counting)) {
            Ok(encounter) => encounter,
            Err(error) => panic!("{error}"),
        };
        encounter.arm();
        let before = counting.queries();
        for _ in 0..100 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.0, -1.0), false, false),
                Some(&counting),
            );
        }
        let per_tick = (counting.queries() - before) as f64 / 100.0;
        assert!(
            per_tick > 0.0 && per_tick < 80.0,
            "{per_tick} ground queries per tick is not a plausible number"
        );
        assert_eq!(counting.surface(0.0, 0.0), ground.surface(0.0, 0.0));
    }

    #[test]
    fn a_broken_setup_is_refused_with_a_typed_error() {
        let ground = ground();
        let mut setup = fixture::golden_setup();
        setup.tuning = AuthoredTuning {
            player_health: 0,
            ..setup.tuning
        };
        match Encounter::new(&setup, Some(&ground)) {
            Ok(_) => panic!("an encounter with no health must be refused"),
            Err(error) => {
                assert!(matches!(error, EncounterError::Spec(_)));
                assert!(!error.to_string().is_empty());
            }
        }
        let mut setup = fixture::golden_setup();
        setup.weapon.blade_thickness = 0;
        match Encounter::new(&setup, Some(&ground)) {
            Ok(_) => panic!("a broken weapon must be refused"),
            Err(error) => assert!(matches!(error, EncounterError::Weapon(_))),
        }
        let mut setup = fixture::golden_setup();
        setup.player.proportions.total_height_units = 0.1;
        match Encounter::new(&setup, Some(&ground)) {
            Ok(_) => panic!("a broken body must be refused"),
            Err(error) => assert!(matches!(error, EncounterError::Character(_))),
        }
    }

    #[test]
    fn the_weapon_follows_the_hand_that_holds_it() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        for side in SIDES {
            let posed = encounter.combatant(side).posed();
            let hand = posed.bone_world()[BoneId::HandR.index()];
            let hand_world = posed.world_matrix().transform_point3(hand.translation);
            let blade = encounter.blade_world(side);
            let grip_distance = (blade.base - hand_world).length();
            // The guard sits a couple of grip voxels from the wrist, never adrift.
            assert!(
                grip_distance < 0.6,
                "{}'s blade starts {grip_distance:.3} world units from its hand",
                side.name()
            );
        }
        // And it keeps following while the body swings and walks.
        for tick in 0..300 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.6, -0.8), tick % 80 == 0, false),
                Some(&ground),
            );
            let posed = encounter.combatant(Side::Player).posed();
            let hand = posed.bone_world()[BoneId::HandR.index()];
            let hand_world = posed.world_matrix().transform_point3(hand.translation);
            let blade = encounter.blade_world(Side::Player);
            assert!(
                (blade.base - hand_world).length() < 0.6,
                "the weapon came loose at tick {tick}"
            );
            let length = (blade.tip - blade.base).length();
            assert!(
                (length - encounter.weapon().blade().world_length()).abs() < 1.0e-4,
                "the blade changed length"
            );
        }
    }

    #[test]
    fn the_players_feet_stay_on_the_ground_the_client_can_see() {
        // M5's contact contract still holds while fighting: the gait phase
        // advances with the distance actually travelled, so a body pushed against
        // the other one does not slide its feet.
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        for tick in 0..1_200 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.0, -1.0), tick % 120 == 0, false),
                Some(&ground),
            );
            for side in SIDES {
                let combatant = encounter.combatant(side);
                for contact in combatant.posed().contacts() {
                    assert!(contact.sole_height.is_finite());
                    if contact.grounded {
                        assert!(
                            contact.clearance().abs() < 0.5,
                            "{} has a sole {:.3} from the ground",
                            side.name(),
                            contact.clearance()
                        );
                    }
                }
                // Realised speed, not intended speed.
                assert!(combatant.state().speed >= 0.0);
                assert!(combatant.state().speed < 20.0);
            }
        }
    }
}
