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
use crate::combatant::{Action, AttackKind, Combatant, Health, Intent, SIDES, Side};
use crate::event::{CombatEvent, StepEvents};
use crate::hit::{Segment, Sweep, moving_substeps, sweep_moving_capsule};
use crate::hurt::HurtVolume;
use crate::movement::{
    MoveRules, TraversalLegality, facing_of, separate, try_move, turn_toward, wrap_angle,
};
use crate::spec::{ArenaSpec, AttackSpec, AuthoredTuning, EncounterTuning, SpecError};
use crate::tick::{Ticks, tick_seconds};
use crate::weapon::{CompiledWeapon, WeaponCompiler, WeaponDescriptor, WeaponError};

/// Which descriptors, which weapon and which tuning an encounter is built from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EncounterSetup {
    pub player: CharacterDescriptor,
    pub adversary: CharacterDescriptor,
    pub weapon: WeaponDescriptor,
    pub tuning: AuthoredTuning,
    /// Where each body starts, in world units, indexed by [`Side`].
    ///
    /// **Absolute positions, not offsets from a centre.** A duel in a disc can
    /// be described either way; a traversal cannot, because the player starts at
    /// one end of a route and the adversary at the other, and writing that as a
    /// centre plus two large opposite offsets would name a point that is neither
    /// body, neither the arena, nor anywhere a fight happens.
    pub starts: [Vec2; SIDES.len()],
    /// The disc a body may not leave, when there is one.
    ///
    /// `None` means the bounds are whatever the ground and the traversal veto
    /// say — for a finite region, the region. A fight in a scanned clearing sets
    /// it; a walk across a valley does not.
    pub arena: Option<ArenaSpec>,
    /// What happens when the **adversary** is defeated.
    ///
    /// Named for the side it is about. A policy called "victory" alone reads as
    /// though it governed both outcomes, and it does not: a defeated player
    /// always runs the defeat hold and then returns both bodies to their
    /// configured starts.
    pub player_victory: PlayerVictoryPolicy,
    /// Radians added to each body's start facing, indexed by [`Side`].
    ///
    /// Zero for a fight, which is the default and the only value any fixture
    /// uses: two bodies look at each other on the first frame. It exists because
    /// facing is otherwise only reachable by walking, and some questions need a
    /// body that is standing still and looking the wrong way — how much facing
    /// error one swing forgives, whether the adversary turns to face a player
    /// behind it.
    pub facing_offsets: [f32; SIDES.len()],
}

/// What an encounter needs from the world it is happening in.
///
/// Two different questions that a single sampler must not be asked to answer:
/// [`GroundSampler`] says *where the visible solid surface is*, including the
/// river bed inside a river, and a traversal veto says *whether a body may walk
/// there at all*. Bundling them into one named value rather than passing two
/// bare `Option<&dyn _>` parameters is a correctness choice: two optional trait
/// references of the same shape at a call site swap places silently.
#[derive(Clone, Copy, Default)]
pub struct WorldContact<'a> {
    ground: Option<&'a dyn GroundSampler>,
    legality: Option<&'a dyn TraversalLegality>,
}

impl<'a> WorldContact<'a> {
    /// A real world: a surface to stand on and a rule about where a body may go.
    #[must_use]
    pub const fn terrain(
        ground: &'a dyn GroundSampler,
        legality: &'a dyn TraversalLegality,
    ) -> Self {
        Self {
            ground: Some(ground),
            legality: Some(legality),
        }
    }

    /// A surface with no traversal veto, which is every fixture and regression
    /// that predates traversal.
    #[must_use]
    pub const fn ground_only(ground: &'a dyn GroundSampler) -> Self {
        Self {
            ground: Some(ground),
            legality: None,
        }
    }

    /// The same, for a caller that already holds an optional sampler.
    #[must_use]
    pub const fn from_ground(ground: Option<&'a dyn GroundSampler>) -> Self {
        Self {
            ground,
            legality: None,
        }
    }

    /// No world at all: the pure-rules tests and the diagnostic corridor.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            ground: None,
            legality: None,
        }
    }

    #[must_use]
    pub const fn ground(&self) -> Option<&'a dyn GroundSampler> {
        self.ground
    }

    #[must_use]
    pub const fn legality(&self) -> Option<&'a dyn TraversalLegality> {
        self.legality
    }
}

/// What happens once the adversary is defeated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlayerVictoryPolicy {
    /// After the defeat hold, both bodies return to their configured starts and
    /// the brain resets: a new round of the same fight. This is M6's behaviour
    /// and every M6 fixture keeps it, which is why their signatures did not move.
    #[default]
    ResetEncounter,
    /// The defeated adversary stays where it fell and nothing resets. The player
    /// is not moved, the brain stays idle, and the encounter keeps stepping so
    /// the player can simply walk away. This is what lets a session continue
    /// past a fight without inventing a death or respawn system.
    Remain,
}

/// Where an encounter's outcome has got to.
///
/// Three states rather than an `Option<Side>` plus a flag, because "held" and
/// "settled" need different work and conflating them made the settled case
/// redo the hold's arithmetic on every tick for ever.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resolution {
    /// Nobody is down.
    Fighting,
    /// Somebody is down and the defeat hold is running.
    Holding(Side),
    /// The hold finished and the policy chose not to reset. Nothing further
    /// happens: no reset, no event, no repeated processing.
    Settled(Side),
}

impl Resolution {
    const fn defeated(self) -> Option<Side> {
        match self {
            Self::Fighting => None,
            Self::Holding(side) | Self::Settled(side) => Some(side),
        }
    }
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
    /// Pressure lunges started, landed and missed. Only the adversary has one,
    /// so these are the adversary's; `swings[1] - pressure_swings` is how many
    /// of its swings were the historical primary.
    pub pressure_swings: u32,
    pub pressure_hits: u32,
    pub pressure_whiffs: u32,
    pub dodges: [u32; SIDES.len()],
    /// Dodges asked for and refused, by cooldown or by a running action.
    pub dodges_refused: [u32; SIDES.len()],
    /// Dodges that started while the **other** body had a swing under way that
    /// had not yet resolved — begun, active window not yet over, and not yet
    /// connected with this body.
    ///
    /// For the player this is the ordinary use of a dodge. For the adversary
    /// it is the audit that its spacing dodge is not a reaction to a swing:
    /// the brain takes that dodge only after its own stagger or its own
    /// connected lunge, and this counter must stay at zero for it.
    pub dodges_during_unresolved_swing: [u32; SIDES.len()],
    /// Dodges whose realised displacement fell short of
    /// [`TRUNCATED_DODGE_FRACTION`] of the authored distance, because the
    /// world refused part of it.
    pub dodges_truncated: [u32; SIDES.len()],
    /// Moves refused outright, or resolved one axis at a time, while dodging.
    /// A subset of `blocked_moves` and `slid_moves`.
    pub dodge_blocked_moves: [u32; SIDES.len()],
    pub dodge_slid_moves: [u32; SIDES.len()],
    pub staggers: [u32; SIDES.len()],
    /// Swings that were turned onto the other body as they started. Countable
    /// because an assist that fires on every swing, or on none, is a tuning
    /// error rather than an assist.
    pub aim_assists: [u32; SIDES.len()],
    pub defeats: [u32; SIDES.len()],
    pub resets: u32,
    /// Moves the rules refused outright: no axis of the proposal was legal.
    pub blocked_moves: [u32; SIDES.len()],
    /// Moves resolved one axis at a time, because the whole proposal was not
    /// legal but part of it was.
    ///
    /// **A body held against a barrier slides; it does not block.** The first
    /// M7 water run walked due south into the river, stopped dead in `z` at the
    /// waterline and travelled twenty-three world units west along the shore —
    /// and reported `blocked_moves = 0` the whole way, because the `x`
    /// component of every refused move was accepted. Without this counter a log
    /// cannot tell a free walk from a body pinned against water.
    pub slid_moves: [u32; SIDES.len()],
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

/// How much of a dodge's authored distance has to be realised for it not to
/// count as truncated.
///
/// Three quarters: a dodge that slid a little along a slope still read as a
/// dodge in every capture, and one stopped short by stone at under three
/// quarters of its length did not.
pub const TRUNCATED_DODGE_FRACTION: f32 = 0.75;

/// One fight between two combatants.
pub struct Encounter {
    tuning: EncounterTuning,
    characters: [CompiledCharacter; SIDES.len()],
    weapon: CompiledWeapon,
    combatants: [Combatant; SIDES.len()],
    brain: AdversaryBrain,
    starts: [Vec2; SIDES.len()],
    arena: Option<ArenaSpec>,
    player_victory: PlayerVictoryPolicy,
    /// Radians added to each body's facing on a start or a reset, so a reset
    /// puts a body back where the setup asked rather than where a fight left it.
    facing_offsets: [f32; SIDES.len()],
    tick: u64,
    armed: bool,
    resolution: Resolution,
    counters: CombatCounters,
    /// Where each body's running dodge started, so its realised displacement
    /// can be measured when it ends. Diagnostic only: nothing reads it back
    /// into a rule.
    dodge_origins: [Option<Vec2>; SIDES.len()],
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
        let starts = setup.starts;
        let healths = [
            Health::full(tuning.player_health()),
            Health::full(tuning.adversary_health()),
        ];

        let mut combatants = Vec::with_capacity(SIDES.len());
        for side in SIDES {
            let index = side.index();
            let state = start_state(starts, setup.facing_offsets[index], side, ground);
            let character = &characters[index];
            let posed = pose_with(character, &state, ground, None);
            combatants.push(Combatant::new(
                side,
                state,
                healths[index],
                character.collision().capsule(),
                HurtVolume::derive(character),
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
            starts,
            arena: setup.arena,
            player_victory: setup.player_victory,
            facing_offsets: setup.facing_offsets,
            tick: 0,
            armed: false,
            resolution: Resolution::Fighting,
            counters: CombatCounters::default(),
            dodge_origins: [None; SIDES.len()],
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

    /// Who is defeated, while the hold runs and after it has settled.
    #[must_use]
    pub const fn outcome(&self) -> Option<Side> {
        self.resolution.defeated()
    }

    /// Whether the outcome has finished resolving and nothing further will
    /// happen to it.
    ///
    /// Only [`PlayerVictoryPolicy::Remain`] reaches this: under
    /// `ResetEncounter` the hold always ends in a reset, so the outcome goes
    /// straight back to `None`.
    #[must_use]
    pub const fn outcome_settled(&self) -> bool {
        matches!(self.resolution, Resolution::Settled(_))
    }

    /// The attack spec that governs one side's running swing, or its primary
    /// when it is not swinging.
    ///
    /// Read from the kind the running `Action::Attack` carries, so a lunge is
    /// timed, moved, posed and recovered by the lunge's spec from its first
    /// tick to its last, and nothing can ask the wrong one half way through.
    #[must_use]
    pub fn attack_spec(&self, side: Side) -> &AttackSpec {
        let kind = self.combatants[side.index()]
            .action()
            .attack_kind()
            .unwrap_or(AttackKind::Primary);
        match self.attack_spec_for(side, kind) {
            Some(spec) => spec,
            None => self.primary_spec(side),
        }
    }

    /// The spec of one of a side's two attacks, if that side has it.
    ///
    /// Every side has a primary. Only the adversary can have a pressure lunge,
    /// and only when its tuning authors one.
    #[must_use]
    pub fn attack_spec_for(&self, side: Side, kind: AttackKind) -> Option<&AttackSpec> {
        match (side, kind) {
            (_, AttackKind::Primary) => Some(self.primary_spec(side)),
            (Side::Adversary, AttackKind::Pressure) => self
                .tuning
                .adversary_pressure()
                .map(crate::spec::PressureSpec::attack),
            (Side::Player, AttackKind::Pressure) => None,
        }
    }

    /// The dodge one side runs.
    ///
    /// The shared historical dodge, except for an adversary with a pressure
    /// lunge, whose only dodge is its spacing dodge. One action and one rule
    /// either way; only the distance and the time differ.
    #[must_use]
    pub fn dodge_spec(&self, side: Side) -> &crate::spec::DodgeSpec {
        match (side, self.tuning.adversary_pressure()) {
            (Side::Adversary, Some(pressure)) => pressure.spacing(),
            _ => self.tuning.dodge(),
        }
    }

    fn primary_spec(&self, side: Side) -> &AttackSpec {
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
    pub fn step(&mut self, input: Intent, world: WorldContact<'_>) -> StepEvents {
        let ground = world.ground();
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
            self.brain.decide(
                me,
                foe,
                self.tuning.adversary(),
                self.tuning.movement(),
                self.tuning.adversary_pressure(),
            )
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
            self.move_combatant(side, intents[side.index()], world);
        }

        // 5. separation
        self.separate_bodies(world);

        // 6. pose
        for side in SIDES {
            if !self.combatants[side.index()].is_frozen() {
                self.repose(side, ground);
            }
        }

        // 7. hit
        for side in SIDES {
            self.resolve_hits(side, world, &mut events);
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

    /// Turns a body onto the other one at the instant a swing starts, if it is
    /// already close enough and nearly facing it.
    ///
    /// **Why this exists, and why it is this small.** Facing follows movement,
    /// which is right for locomotion and is what makes a body turn as it walks.
    /// It also means a body that stands still to swing cannot track a target
    /// that is moving, and the adversary's brain steers continuously — so the
    /// adversary aims perfectly and the player cannot. Branch QA played the
    /// encounter through a closed loop and measured the result: from good
    /// positions, inside the range and the facing window that `combat-probe
    /// aim` says connect, the player landed one swing in four while the
    /// adversary landed almost every one. That is the "artificially difficult
    /// despite good positioning" case, and this is the bounded answer to it.
    ///
    /// The bounds are the whole design:
    ///
    /// - **At swing start only.** Nothing tracks during the swing, so a target
    ///   that moves after the blade is committed is still missed.
    /// - **Inside [`AIM_ASSIST_CONE`] only.** A body facing away does not
    ///   snap round; this closes the last few degrees of a turn the body was
    ///   already most of the way through.
    /// - **Inside [`AIM_ASSIST_RANGE`] only.** Beyond the reach of the attack
    ///   there is nothing to assist.
    /// - **One other body**, because the encounter has exactly two. This is not
    ///   target selection and there is nothing to select.
    /// - **No camera involvement at all.** The camera is not consulted and not
    ///   moved; this is a rule of the domain and applies identically to both
    ///   sides, which keeps "both run the same rules" true.
    ///
    /// A pressure lunge is aimed by the same rule, with the one difference its
    /// distance demands: its range is the far edge of the band it is chosen
    /// from rather than [`AIM_ASSIST_RANGE`]. The facing a lunge ends up with is
    /// then locked for the whole action, which is what makes the line a line.
    fn aim_at_the_other_body(&mut self, side: Side, kind: AttackKind) {
        let index = side.index();
        let from = self.combatants[index].position();
        let to = self.combatants[side.other().index()].position();
        let offset = to - from;
        let range = match (kind, self.tuning.adversary_pressure()) {
            (AttackKind::Pressure, Some(pressure)) => pressure.select_max(),
            _ => AIM_ASSIST_RANGE,
        };
        if offset.length() > range {
            return;
        }
        let Some(bearing) = facing_of(offset) else {
            return;
        };
        let facing = self.combatants[index].state().facing;
        if wrap_angle(bearing - facing).abs() > AIM_ASSIST_CONE {
            return;
        }
        self.combatants[index].state_mut().facing = bearing;
        self.counters.aim_assists[index] = self.counters.aim_assists[index].saturating_add(1);
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
            // A kind the side does not have is its primary. Only a brain with
            // a pressure spec ever asks for one, so this is a guard, not a rule.
            let kind = match intent.attack_kind() {
                AttackKind::Pressure
                    if self.attack_spec_for(side, AttackKind::Pressure).is_some() =>
                {
                    AttackKind::Pressure
                }
                _ => AttackKind::Primary,
            };
            self.aim_at_the_other_body(side, kind);
            let swing = self.combatants[index].take_swing_id();
            self.combatants[index].set_action(Action::Attack {
                swing,
                kind,
                elapsed: 0,
                hits: [false; SIDES.len()],
            });
            self.counters.swings[index] = self.counters.swings[index].saturating_add(1);
            if kind == AttackKind::Pressure {
                self.counters.pressure_swings = self.counters.pressure_swings.saturating_add(1);
            }
            events.push(CombatEvent::SwingStarted { side, swing });
            return;
        }
        if intent.dodge() {
            if self.combatants[index].dodge_cooldown() > 0 {
                self.counters.dodges_refused[index] =
                    self.counters.dodges_refused[index].saturating_add(1);
                return;
            }
            let dodge = *self.dodge_spec(side);
            // The audit: was the other body in the middle of a swing that had
            // not resolved yet? Counted, never consulted.
            let other = side.other();
            let other_spec = *self.attack_spec(other);
            if let Action::Attack { elapsed, hits, .. } = *self.combatants[other.index()].action()
                && elapsed < other_spec.active_end()
                && !hits[index]
            {
                self.counters.dodges_during_unresolved_swing[index] =
                    self.counters.dodges_during_unresolved_swing[index].saturating_add(1);
            }
            self.dodge_origins[index] = Some(self.combatants[index].position());
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
            kind,
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
            if kind == AttackKind::Pressure {
                self.counters.pressure_whiffs = self.counters.pressure_whiffs.saturating_add(1);
            }
            events.push(CombatEvent::SwingWhiffed { side, swing });
        }
    }

    /// Applies one body's movement intent under the rules.
    fn move_combatant(&mut self, side: Side, intent: Intent, world: WorldContact<'_>) {
        let ground = world.ground();
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
                    let mut velocity = Vec2::new(forward.x, forward.z) * (per_tick / step);
                    // A lunge travels its whole distance along the committed
                    // line over its last ticks, so the threat arrives with the
                    // blade. Every historical attack has no lunge and skips
                    // this, bit for bit.
                    if spec.is_lunging(elapsed) {
                        let lunge_per_tick = spec.lunge() / spec.lunge_ticks().max(1) as f32;
                        velocity += Vec2::new(forward.x, forward.z) * (lunge_per_tick / step);
                    }
                    velocity
                } else {
                    intent.move_world() * movement.speed() * movement.recovery_speed_scale()
                }
            }
            Action::Dodge { direction, .. } => direction * self.dodge_spec(side).speed(),
            Action::Stagger { .. } | Action::Defeated { .. } => Vec2::ZERO,
        };

        // The specs are copied into locals because `MoveRules` borrows them and
        // the move below needs the combatant mutably at the same time.
        let arena = self.arena;
        let rules = MoveRules {
            movement: &movement,
            arena: arena.as_ref(),
            ground,
            legality: world.legality(),
        };
        let result = try_move(self.combatants[index].state_mut(), velocity * step, &rules);
        if result.blocked {
            self.counters.blocked_moves[index] =
                self.counters.blocked_moves[index].saturating_add(1);
        }
        if result.slid {
            self.counters.slid_moves[index] = self.counters.slid_moves[index].saturating_add(1);
        }
        if action.is_dodging() {
            if result.blocked {
                self.counters.dodge_blocked_moves[index] =
                    self.counters.dodge_blocked_moves[index].saturating_add(1);
            }
            if result.slid {
                self.counters.dodge_slid_moves[index] =
                    self.counters.dodge_slid_moves[index].saturating_add(1);
            }
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
        // A primary swing may turn again once its blade is spent. A lunge may
        // not, from its first tick to its last: its line is the promise its
        // telegraph made, and a missed lunge that swung round to face the body
        // that stepped aside would not look exposed.
        let turnable = matches!(action, Action::Free)
            || matches!(action, Action::Attack { elapsed, kind: AttackKind::Primary, .. }
                if elapsed >= spec.active_end());
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
    fn separate_bodies(&mut self, world: WorldContact<'_>) {
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
        let arena = self.arena;
        let rules = MoveRules {
            movement: &movement,
            arena: arena.as_ref(),
            ground: world.ground(),
            legality: world.legality(),
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
        let previous_hurt = combatant.hurt_capsule();
        let posed = pose_with(&self.characters[index], &state, ground, Some(&overlay));
        self.combatants[index].set_posed(posed, self.characters[index].collision());
        self.combatants[index].set_blade(previous);
        self.combatants[index].set_previous_hurt_capsule(previous_hurt);
    }

    /// Sweeps one side's blade and applies what it touched.
    fn resolve_hits(&mut self, attacker: Side, world: WorldContact<'_>, events: &mut StepEvents) {
        let ground = world.ground();
        let index = attacker.index();
        if self.combatants[index].is_frozen() {
            return;
        }
        let spec = *self.attack_spec(attacker);
        let Action::Attack {
            elapsed,
            hits,
            kind,
            ..
        } = *self.combatants[index].action()
        else {
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
        let capsule_end = self.combatants[victim_index].hurt_capsule();
        let capsule_start = self.combatants[victim_index]
            .previous_hurt_capsule()
            .unwrap_or(capsule_end);
        self.counters.hit_queries = self.counters.hit_queries.saturating_add(1);
        self.counters.sweep_substeps_max = self.counters.sweep_substeps_max.max(moving_substeps(
            &sweep,
            &capsule_start,
            &capsule_end,
        ));
        let Some(contact) = sweep_moving_capsule(&sweep, &capsule_start, &capsule_end) else {
            return;
        };

        // One hit per swing, marked on the swing itself.
        if !self.combatants[index].register_hit(victim) {
            self.counters.multi_hit_suppressed =
                self.counters.multi_hit_suppressed.saturating_add(1);
            return;
        }
        self.counters.hits[index] = self.counters.hits[index].saturating_add(1);
        if kind == AttackKind::Pressure {
            self.counters.pressure_hits = self.counters.pressure_hits.saturating_add(1);
        }

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
            if matches!(self.resolution, Resolution::Fighting) {
                self.resolution = Resolution::Holding(victim);
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
            let arena = self.arena;
            let rules = MoveRules {
                movement: &movement,
                arena: arena.as_ref(),
                ground,
                legality: world.legality(),
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

    /// Runs the defeat hold, then applies the policy for who went down.
    ///
    /// A settled outcome returns immediately and for ever: under
    /// [`PlayerVictoryPolicy::Remain`] the defeated adversary simply stays
    /// where it fell, and re-deciding that on every tick of the rest of a
    /// session would be work whose only possible result is the same answer.
    fn update_outcome(&mut self, ground: Option<&dyn GroundSampler>, events: &mut StepEvents) {
        let Resolution::Holding(defeated) = self.resolution else {
            return;
        };
        let elapsed = match self.combatants[defeated.index()].action() {
            Action::Defeated { elapsed } => *elapsed,
            // The defeated body's action was replaced, which would be a bug in
            // the ordering above. Resolving is the safe answer and the counter
            // below makes it visible.
            _ => self.tuning.defeat_hold(),
        };
        if elapsed < self.tuning.defeat_hold() {
            return;
        }
        if defeated == Side::Adversary && self.player_victory == PlayerVictoryPolicy::Remain {
            // The player won and keeps the world it is standing in. Nothing
            // moves, nothing resets, and no event is published, because nothing
            // happened that presentation has to react to: the body was already
            // `Defeated` and said so when it fell.
            self.resolution = Resolution::Settled(defeated);
            return;
        }
        for side in SIDES {
            let state = start_state(self.starts, self.facing_offsets[side.index()], side, ground);
            self.combatants[side.index()].reset(state);
        }
        self.dodge_origins = [None; SIDES.len()];
        self.brain.reset();
        self.resolution = Resolution::Fighting;
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
                kind,
                elapsed,
                hits,
            } => Action::Attack {
                swing,
                kind,
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
            // The recovery a swing runs depends on what it met. For every
            // primary the two are equal and this is `spec.total()`.
            Action::Attack { elapsed, .. } => elapsed >= spec.total_for(advanced.connected()),
            Action::Dodge {
                elapsed, duration, ..
            }
            | Action::Stagger {
                elapsed, duration, ..
            } => elapsed >= duration,
            // A defeat holds until the encounter resets it.
            Action::Free | Action::Defeated { .. } => false,
        };
        if over && advanced.is_dodging() {
            self.measure_dodge(side);
        }
        self.combatants[index].set_action(if over { Action::Free } else { advanced });
    }

    /// How far one finished dodge actually carried the body, against how far
    /// it was authored to.
    fn measure_dodge(&mut self, side: Side) {
        let index = side.index();
        let Some(origin) = self.dodge_origins[index].take() else {
            return;
        };
        let realised = (self.combatants[index].position() - origin).length();
        if realised < self.dodge_spec(side).distance() * TRUNCATED_DODGE_FRACTION {
            self.counters.dodges_truncated[index] =
                self.counters.dodges_truncated[index].saturating_add(1);
        }
    }
}

/// Where one side starts, settled on the ground.
fn start_state(
    starts: [Vec2; SIDES.len()],
    facing_offset: f32,
    side: Side,
    ground: Option<&dyn GroundSampler>,
) -> CharacterState {
    let index = side.index();
    let position = starts[index];
    let other = starts[side.other().index()];
    // Each starts facing the other, because a faceoff is the first frame of
    // evidence and two bodies looking past each other is not one. The offset is
    // zero in every fixture and exists so a measurement can ask for the
    // opposite.
    let facing = facing_of(other - position).unwrap_or(0.0) + facing_offset;
    CharacterState::standing(position.x, position.y, facing, ground)
}

/// How far off a body may already be looking and still be turned onto the other
/// one when its swing starts, in radians.
///
/// `0.61` is thirty-five degrees. `combat-probe aim` measures the window a
/// swing connects in as roughly `-20` to `+10` degrees at the far end of the
/// reach and `-45` to `+14` close in, so a cone of thirty-five degrees closes
/// the last part of a turn a body was already most of the way through and does
/// nothing for one that is facing elsewhere. It is deliberately smaller than a
/// quarter turn: a body attacking behind itself keeps missing.
pub const AIM_ASSIST_CONE: f32 = 0.61;

/// How far away the other body may be and still be aimed at, in world units.
///
/// `2.90`, just past the `2.8949` centre-to-centre distance a hit connects out
/// to. Beyond the reach of the attack there is nothing to assist.
pub const AIM_ASSIST_RANGE: f32 = 2.90;

/// Ticks a whole action would take, for the probe and the tests.
#[must_use]
pub fn action_duration(action: &Action, spec: &AttackSpec) -> Ticks {
    match action {
        Action::Free => 0,
        Action::Attack { .. } => spec.total_for(action.connected()),
        Action::Dodge { duration, .. } | Action::Stagger { duration, .. } => *duration,
        Action::Defeated { .. } => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AIM_ASSIST_CONE, AIM_ASSIST_RANGE, CountingGround, Encounter, EncounterError,
        EncounterSetup, PlayerVictoryPolicy, TraversalLegality, WorldContact, action_duration,
    };
    use crate::combatant::{Action, AttackKind, Intent, SIDES, Side};
    use crate::event::CombatEvent;
    use crate::fixture;
    use crate::spec::AuthoredTuning;
    use glam::Vec2;
    use veldwake_character::ground::{FlatGround, StepGround, SteppedRamp};
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
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(ground));
        }
        false
    }

    #[test]
    fn a_step_takes_no_duration_and_advances_exactly_one_tick() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        assert_eq!(encounter.tick_index(), 0);
        for expected in 1..=10 {
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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
                WorldContact::ground_only(&ground),
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
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, true, false),
            WorldContact::ground_only(&ground),
        );
        // One step has already run, so the swing has `total - 1` left.
        for remaining in 1..spec.total() {
            assert!(
                encounter.combatant(Side::Player).action().is_attacking(),
                "the swing ended early, at tick {remaining} of {}",
                spec.total()
            );
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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
                    kind: crate::combatant::AttackKind::Primary,
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
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
            // Toward the adversary, not toward a fixed compass direction.
            // Facing follows movement, and a knockback slides the victim
            // sideways, so a player walking due south ends up swinging thirty
            // degrees past a body it is standing against. That is a real
            // property of movement-driven facing and it belongs in a test about
            // facing; here it is a variable that has nothing to do with whether
            // one swing lands one hit.
            let toward = encounter.combatant(Side::Adversary).position()
                - encounter.combatant(Side::Player).position();
            let events = encounter.step(
                Intent::player(toward, free, false),
                WorldContact::ground_only(&ground),
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
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
        let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
        assert!(encounter.combatant(Side::Player).is_frozen());
        assert!(encounter.combatant(Side::Adversary).is_frozen());
    }

    #[test]
    fn hitstop_freezes_the_timeline_without_stopping_the_tick() {
        let ground = ground();
        let mut encounter = armed(&fixture::sandbox_setup(), &ground);
        let spec = *encounter.attack_spec(Side::Player);
        for tick in 0..spec.total() {
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
                WorldContact::ground_only(&ground),
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
        let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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
        setup.starts = [Vec2::new(0.0, 1.0), Vec2::new(0.0, -1.0)];
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
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
            if events.any(|event| matches!(event, CombatEvent::Defeated { .. })) {
                break;
            }
        }
        assert_eq!(encounter.outcome(), Some(Side::Adversary));
        let mut reset = false;
        for _ in 0..hold * 3 {
            let events = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, true, false),
            WorldContact::ground_only(&ground),
        );
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
            let events = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
            WorldContact::ground_only(&ground),
        );
        assert!(encounter.combatant(Side::Player).action().is_dodging());
        assert_eq!(encounter.counters().dodges[0], 1);
        // Asking again mid dodge is refused and counted.
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, false, true),
            WorldContact::ground_only(&ground),
        );
        assert_eq!(encounter.counters().dodges[0], 1);
        assert!(encounter.counters().dodges_refused[0] >= 1);
        // And asking to attack mid dodge is refused too: commitment is the point.
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, true, false),
            WorldContact::ground_only(&ground),
        );
        assert!(encounter.combatant(Side::Player).action().is_dodging());
        for _ in 0..dodge.duration() {
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
        }
        assert!(encounter.combatant(Side::Player).action().is_free());
        // Still cooling down.
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, false, true),
            WorldContact::ground_only(&ground),
        );
        assert_eq!(encounter.counters().dodges[0], 1, "the cooldown must hold");
        for _ in 0..dodge.cooldown() + 2 {
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
        }
        let _ = encounter.step(
            Intent::player(Vec2::new(0.0, -1.0), false, true),
            WorldContact::ground_only(&ground),
        );
        assert_eq!(encounter.counters().dodges[0], 2, "and then release");
    }

    #[test]
    fn a_dodge_with_no_direction_goes_backwards() {
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let before = encounter.separation_distance();
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, false, true),
            WorldContact::ground_only(&ground),
        );
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
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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
            let _ = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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
    fn a_swing_is_aimed_only_from_inside_its_cone_and_its_range() {
        // The bounds are the whole design, so each one is asserted. A body
        // already nearly facing the other is turned the last few degrees; a
        // body facing away, or too far to reach, is left exactly as it was.
        let ground = ground();
        let aim = |offset_degrees: f32, range: f32| {
            let mut setup = fixture::sandbox_setup();
            setup.starts = [Vec2::new(0.0, range * 0.5), Vec2::new(0.0, -range * 0.5)];
            setup.facing_offsets[Side::Player.index()] = offset_degrees.to_radians();
            let mut encounter = armed(&setup, &ground);
            let before = encounter.combatant(Side::Player).state().facing;
            let _ = encounter.step(
                Intent::player(Vec2::ZERO, true, false),
                WorldContact::ground_only(&ground),
            );
            let after = encounter.combatant(Side::Player).state().facing;
            let assists = encounter.counters().aim_assists[Side::Player.index()];
            (before, after, assists)
        };

        // Inside both bounds: turned onto the other body, and counted.
        let (before, after, assists) = aim(30.0, 2.0);
        assert_eq!(assists, 1, "a swing inside the cone was not aimed");
        assert!(
            (after - before).abs() > 0.1,
            "the facing did not move: {before} to {after}"
        );
        assert!(
            after.abs() < 1.0e-4,
            "the swing was not aimed at the body: {after}"
        );

        // Outside the cone: untouched.
        let (before, after, assists) = aim(80.0, 2.0);
        assert_eq!(assists, 0, "a swing facing away was aimed anyway");
        assert!((after - before).abs() < 1.0e-6, "{before} became {after}");

        // Inside the cone but beyond the reach: untouched.
        let (before, after, assists) = aim(30.0, AIM_ASSIST_RANGE + 0.5);
        assert_eq!(assists, 0, "a swing out of range was aimed anyway");
        assert!((after - before).abs() < 1.0e-6, "{before} became {after}");

        // Exactly on the cone boundary is inside it; a hair past is not.
        let just_inside = AIM_ASSIST_CONE.to_degrees() - 0.5;
        let just_outside = AIM_ASSIST_CONE.to_degrees() + 0.5;
        assert_eq!(aim(just_inside, 2.0).2, 1);
        assert_eq!(aim(just_outside, 2.0).2, 0);
        // And it is symmetric: turning left is the same rule as turning right.
        assert_eq!(aim(-just_inside, 2.0).2, 1);
        assert_eq!(aim(-just_outside, 2.0).2, 0);
    }

    #[test]
    fn aiming_happens_at_the_swing_and_never_during_it() {
        // A swing that kept tracking would be a homing attack. The facing is
        // set once, when the blade commits, and then the body is on its own.
        let ground = ground();
        let mut setup = fixture::sandbox_setup();
        setup.starts = [Vec2::new(0.0, 1.0), Vec2::new(0.0, -1.0)];
        setup.facing_offsets[Side::Player.index()] = 0.3;
        let mut encounter = armed(&setup, &ground);
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, true, false),
            WorldContact::ground_only(&ground),
        );
        assert_eq!(encounter.counters().aim_assists[Side::Player.index()], 1);
        let aimed = encounter.combatant(Side::Player).state().facing;

        // The rest of the swing, standing still: the facing must not move again
        // however the other body drifts.
        let spec = *encounter.attack_spec(Side::Player);
        for _ in 0..spec.total() {
            let _ = encounter.step(
                Intent::player(Vec2::ZERO, false, false),
                WorldContact::ground_only(&ground),
            );
            let now = encounter.combatant(Side::Player).state().facing;
            assert!(
                (now - aimed).abs() < 1.0e-6,
                "the facing moved mid swing: {aimed} to {now}"
            );
        }
        assert_eq!(
            encounter.counters().aim_assists[Side::Player.index()],
            1,
            "the assist fired more than once for one swing"
        );
    }

    #[test]
    fn a_dodge_is_never_aimed() {
        // Only a swing is aimed. A dodge has a direction of its own and taking
        // it over would be a different and much larger decision.
        let ground = ground();
        let mut setup = fixture::sandbox_setup();
        setup.facing_offsets[Side::Player.index()] = 0.3;
        let mut encounter = armed(&setup, &ground);
        let before = encounter.combatant(Side::Player).state().facing;
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, false, true),
            WorldContact::ground_only(&ground),
        );
        assert_eq!(encounter.counters().aim_assists[Side::Player.index()], 0);
        assert!(encounter.combatant(Side::Player).action().is_dodging());
        let after = encounter.combatant(Side::Player).state().facing;
        assert!((after - before).abs() < 1.0e-6, "a dodge turned the body");
    }

    #[test]
    fn the_first_blade_of_a_tick_takes_the_other_one_out_of_the_air() {
        // What §21 asks to be tested, and what it turned out to be.
        //
        // The question was which of two hits landing on one tick is published
        // first. The answer is that there is no such tick: hits resolve in the
        // fixed order player, then adversary, and a hit staggers its victim —
        // so by the time the adversary's own query runs it is no longer
        // attacking. Two blades cannot land together because the first one
        // takes the second out of the air. That is a better property than a
        // tie-break, and it is the one asserted here.
        //
        // The situation is constructed rather than waited for: the loop starts
        // the player's swing whenever the two active windows would overlap, and
        // knockback is off on both sides so that the shove cannot be what
        // separates them. Nineteen such swings happen in one reference fight,
        // and the counters below prove the case was exercised rather than
        // quietly missed.
        let ground = ground();
        let mut setup = fixture::golden_setup();
        setup.tuning.player_attack.knockback = 0.0;
        setup.tuning.adversary_attack.knockback = 0.0;
        let mut encounter = armed(&setup, &ground);
        let player_spec = *encounter.attack_spec(Side::Player);
        let adversary_spec = *encounter.attack_spec(Side::Adversary);

        let mut overlapping_swings = 0_u32;
        let mut landed_while_adversary_was_swinging = 0_u32;
        let mut same_tick_pairs = 0_u32;
        for _ in 0..fixture::GOLDEN_RUN_TICKS {
            let toward = encounter.combatant(Side::Adversary).position()
                - encounter.combatant(Side::Player).position();
            let adversary = encounter.combatant(Side::Adversary).action();
            // Start the swing whenever the two active windows would *overlap*,
            // not only when they would start together: twelve ticks of overlap
            // is a window worth aiming at, one tick of coincidence is not.
            let aligned = match adversary.attack_phase(&adversary_spec) {
                Some(crate::combatant::AttackPhase::Windup) => {
                    let until = adversary_spec
                        .active_start()
                        .saturating_sub(adversary.elapsed());
                    let earliest = player_spec
                        .active_start()
                        .saturating_sub(adversary_spec.active());
                    until >= earliest && until <= player_spec.active_end()
                }
                _ => false,
            };
            let attack = aligned && encounter.combatant(Side::Player).can_act();
            if attack {
                overlapping_swings += 1;
            }
            let adversary_was_attacking =
                encounter.combatant(Side::Adversary).action().is_attacking();
            let events = encounter.step(
                Intent::player(toward, attack, false),
                WorldContact::ground_only(&ground),
            );

            let hits: Vec<Side> = events
                .iter()
                .filter_map(|event| match event {
                    CombatEvent::Hit { attacker, .. } => Some(attacker),
                    _ => None,
                })
                .collect();
            if hits.len() >= 2 {
                same_tick_pairs += 1;
                assert_eq!(
                    hits[0],
                    Side::Player,
                    "a tick published {:?} before the player's hit",
                    hits[0]
                );
            }
            if adversary_was_attacking && hits.contains(&Side::Player) {
                landed_while_adversary_was_swinging += 1;
                // The consequence: the adversary is staggered out of its swing
                // in the same tick, so it is no longer attacking and its blade
                // never arrives.
                let after = encounter.combatant(Side::Adversary).action();
                assert!(
                    !after.is_attacking(),
                    "the adversary kept swinging through a hit: {after:?}"
                );
                assert!(
                    !hits.contains(&Side::Adversary),
                    "both blades landed on one tick, which the stagger should have prevented"
                );
            }
        }

        // A vacuous pass would be worse than a failure, so the test says
        // whether it ever saw the case it exists for.
        assert!(
            overlapping_swings > 0,
            "the player never swung into the adversary's active window"
        );
        assert!(
            landed_while_adversary_was_swinging > 0,
            "no hit ever landed on a body that was mid-swing, so nothing was tested: \
             {overlapping_swings} overlapping swings"
        );
        assert_eq!(
            same_tick_pairs, 0,
            "two blades landed on the same tick {same_tick_pairs} times; if that is now \
             possible the ordering above is load-bearing and this test should assert it"
        );
    }

    #[test]
    fn the_hurt_capsule_contains_the_core_of_the_body_at_every_moment_of_a_fight() {
        // The accepted limitation, turned into a checked property. Everything
        // that swings is outside the volume — both arms, both legs below the
        // hip, the feet, the weapon — because measurement showed a single
        // capsule that held a leg at the top of its stride was as wide as the
        // hands reach, which took hits in empty air. What is left, the torso
        // column of `HURT_CORE`, may not leave it. The list is imported rather
        // than repeated so that widening the core cannot quietly widen the
        // claim as well.
        let core = crate::hurt::HURT_CORE;
        let ground = ground();
        let mut encounter = armed(&fixture::golden_setup(), &ground);
        let mut runner = crate::script::ScriptRunner::new(
            crate::script::GOLDEN_SCRIPT,
            fixture::reach_of(&encounter),
        );
        let mut worst = 0.0_f32;
        for _ in 0..fixture::GOLDEN_RUN_TICKS {
            let intent = runner.next_intent(&encounter);
            let _ = encounter.step(intent, WorldContact::ground_only(&ground));
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
    fn a_swing_aimed_at_the_body_connects_at_every_range_it_claims_and_misses_a_body_behind() {
        // Two bounds on the same window, and both of them were defects at some
        // point. A window that does not contain zero means a player who aims at
        // the body in front of them misses it, which is a game that cannot be
        // played. A window that contains sixty degrees means the volume being
        // swept is not the shape of a body — the first hurt volume here was M5's
        // whole-body capsule, and it registered hits with the blade still raised
        // over the shoulder and its tip in the air above the head.
        //
        // The bodies stand still throughout: the only thing under test is the
        // arc. `facing_offsets` is how the attacker ends up looking the wrong
        // way without walking there, because walking to turn also changes the
        // range being measured.
        let ground = ground();
        let swings = |range: f32, error_degrees: f32| {
            let mut setup = fixture::sandbox_setup();
            setup.tuning.player_attack.knockback = 0.0;
            setup.starts = [Vec2::new(0.0, range * 0.5), Vec2::new(0.0, -range * 0.5)];
            setup.facing_offsets[Side::Player.index()] = error_degrees.to_radians();
            let mut encounter = armed(&setup, &ground);
            let spec = *encounter.attack_spec(Side::Player);
            let mut landed = false;
            for tick in 0..spec.total() + 2 {
                let events = encounter.step(
                    Intent::player(Vec2::ZERO, tick == 0, false),
                    WorldContact::ground_only(&ground),
                );
                landed |= events
                    .iter()
                    .any(|event| matches!(event, CombatEvent::Hit { .. }));
            }
            landed
        };

        // Every range from bodies almost touching out to the reach the fixture
        // measures, in tenths of a world unit.
        let reach = fixture::reach_of(&armed(&fixture::sandbox_setup(), &ground));
        for step in 0..=12_u8 {
            let range = 1.6 + f32::from(i16::from(step)) * 0.1;
            if range > reach {
                continue;
            }
            assert!(
                swings(range, 0.0),
                "a swing aimed straight at a body {range:.2} away did not connect, \
                 with a reach of {reach:.2}"
            );
            for error in [-90.0, -75.0, 75.0, 90.0] {
                assert!(
                    !swings(range, error),
                    "a swing {error} degrees off the body still hit it at {range:.2}, \
                     so the volume being swept is wider than a body"
                );
            }
        }
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
            let _ = encounter.step(
                Intent::player(Vec2::ZERO, tick == 0, false),
                WorldContact::ground_only(&ground),
            );
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
            let events = encounter.step(Intent::idle(), WorldContact::ground_only(&ground));
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

    /// A veto that refuses a half-plane, standing in for water.
    #[derive(Clone, Copy, Debug)]
    struct RefuseBeyondZ {
        z: f32,
    }

    impl TraversalLegality for RefuseBeyondZ {
        fn walkable(&self, _x: f64, z: f64) -> bool {
            z < f64::from(self.z)
        }
    }

    #[test]
    fn a_real_encounter_walks_a_stepped_ramp_without_the_pelvis_stopping_it() {
        // The whole authority path, not `try_move` alone: `CharacterState`'s
        // pelvis filter runs for real here, and the rule must be untouched by
        // it. The adversary is left far away and dormant so the only thing
        // moving the player is the intent.
        let ground = SteppedRamp::terrain(0.5, 0.0);
        let mut setup = fixture::golden_setup();
        setup.arena = None;
        setup.starts = [Vec2::new(0.0, 0.0), Vec2::new(0.0, -400.0)];
        let mut encounter = armed(&setup, &ground);

        let start_x = encounter.combatant(Side::Player).position().x;
        let mut blocked_before = encounter.counters().blocked_moves[Side::Player.index()];
        let mut worst_stall = 0_u32;
        let mut stall = 0_u32;
        for _ in 0..2_400 {
            let _ = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), false, false),
                WorldContact::ground_only(&ground),
            );
            let blocked = encounter.counters().blocked_moves[Side::Player.index()];
            if blocked > blocked_before {
                stall += 1;
                worst_stall = worst_stall.max(stall);
            } else {
                stall = 0;
            }
            blocked_before = blocked;
        }
        let travelled = encounter.combatant(Side::Player).position().x - start_x;
        // Twenty seconds at the walk speed, minus nothing: a terrace must not
        // cost the body ground.
        let expected = encounter.tuning().movement().speed() * 20.0;
        assert!(
            travelled > expected * 0.98,
            "the walk lost ground on a staircase: {travelled} against {expected}"
        );
        assert_eq!(
            worst_stall, 0,
            "a one-voxel terrace stalled a walking body for {worst_stall} ticks"
        );
        let Some(height) = ground.surface(
            f64::from(encounter.combatant(Side::Player).position().x),
            0.0,
        ) else {
            panic!("the ramp answers everywhere");
        };
        assert!(height > 30.0, "the body barely climbed: {height}");
    }

    #[test]
    fn a_traversal_veto_stops_a_body_in_a_real_encounter_and_lets_it_slide() {
        let ground = FlatGround::at(0.0);
        let veto = RefuseBeyondZ { z: 2.0 };
        let mut setup = fixture::golden_setup();
        setup.arena = None;
        setup.starts = [Vec2::new(0.0, 0.0), Vec2::new(0.0, -400.0)];
        let mut encounter = armed(&setup, &ground);
        for _ in 0..1_200 {
            // Straight at the barrier, and diagonally along it.
            let _ = encounter.step(
                Intent::player(Vec2::new(0.4, 1.0), false, false),
                WorldContact::terrain(&ground, &veto),
            );
            let position = encounter.combatant(Side::Player).position();
            assert!(
                position.y < 2.0,
                "a body crossed a traversal veto to {position}"
            );
            assert!(position.is_finite());
        }
        assert!(
            encounter.combatant(Side::Player).position().x > 1.0,
            "a body stopped by a veto must still slide along it"
        );
        // And the slide is *counted*, because it is what a barrier looks like
        // from a log: the first M7 water run walked into the river, was held at
        // the waterline, slid twenty-three units along the shore, and reported
        // `blocked_moves = 0` for every one of them.
        assert!(
            encounter.counters().slid_moves[Side::Player.index()] > 0,
            "a body held against a barrier reported no slide"
        );
        assert_eq!(
            encounter.counters().blocked_moves[Side::Player.index()],
            0,
            "this body was never refused outright, only redirected"
        );
    }

    #[test]
    fn a_defeated_adversary_remains_and_the_session_continues() {
        let ground = ground();
        let mut setup = fixture::sandbox_setup();
        setup.player_victory = PlayerVictoryPolicy::Remain;
        let mut encounter = armed(&setup, &ground);

        // Hit it until it falls.
        let mut ticks = 0_u32;
        while encounter.outcome().is_none() && ticks < 4_000 {
            let _ = encounter.step(
                Intent::player(Vec2::ZERO, ticks.is_multiple_of(40), false),
                WorldContact::ground_only(&ground),
            );
            ticks += 1;
        }
        assert_eq!(
            encounter.outcome(),
            Some(Side::Adversary),
            "the sandbox adversary never fell"
        );
        assert!(!encounter.outcome_settled(), "the hold has not run yet");

        let resets_before = encounter.counters().resets;
        let where_the_player_stood = encounter.combatant(Side::Player).position();
        let where_the_body_fell = encounter.combatant(Side::Adversary).position();

        // Well past the defeat hold, walking away the whole time.
        let mut reset_events = 0_u32;
        for _ in 0..3_000 {
            let events = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), false, false),
                WorldContact::ground_only(&ground),
            );
            reset_events += events
                .iter()
                .filter(|event| matches!(event, CombatEvent::EncounterReset))
                .count() as u32;
        }
        assert_eq!(reset_events, 0, "a remaining victory published a reset");
        assert_eq!(
            encounter.counters().resets,
            resets_before,
            "a remaining victory reset the encounter"
        );
        assert_eq!(encounter.outcome(), Some(Side::Adversary));
        assert!(encounter.outcome_settled(), "the outcome never settled");
        assert!(
            encounter.combatant(Side::Adversary).action().is_defeated(),
            "the defeated body got up"
        );
        assert!(
            (encounter.combatant(Side::Adversary).position() - where_the_body_fell).length()
                < 1.0e-3,
            "the defeated body moved after it fell"
        );
        assert_eq!(
            encounter.brain().state(),
            crate::adversary::AdversaryState::Idle,
            "the brain of a defeated body must stay idle"
        );
        assert!(
            encounter.combatant(Side::Player).position().x > where_the_player_stood.x + 1.0,
            "the player could not walk away from its own victory"
        );
    }

    #[test]
    fn a_defeated_player_returns_to_its_configured_start() {
        let ground = ground();
        let mut setup = fixture::golden_setup();
        setup.player_victory = PlayerVictoryPolicy::Remain;
        // Far apart on purpose, so a reset to "the arena centre" would be
        // visibly wrong: each body must return to its own start.
        setup.arena = None;
        // Inside the adversary's aggro radius so the fight actually happens,
        // and far from the origin so a reset to "the arena centre" would be
        // visibly wrong: each body must return to its own configured start.
        setup.starts = [Vec2::new(-61.5, 12.0), Vec2::new(-61.5, 9.0)];
        let mut encounter = armed(&setup, &ground);

        // Stand still in front of an adversary that walks over and kills you.
        let mut ticks = 0_u32;
        while encounter.counters().resets == 0 && ticks < 40_000 {
            let _ = encounter.step(
                Intent::player(Vec2::ZERO, false, false),
                WorldContact::ground_only(&ground),
            );
            ticks += 1;
        }
        assert_eq!(
            encounter.counters().resets,
            1,
            "a defeated player must reset the encounter"
        );
        assert_eq!(encounter.outcome(), None, "a reset clears the outcome");
        assert!(!encounter.outcome_settled());
        for side in SIDES {
            let expected = setup.starts[side.index()];
            let actual = encounter.combatant(side).position();
            assert!(
                (actual - expected).length() < 1.0e-3,
                "{} reset to {actual} rather than to its start {expected}",
                side.name()
            );
        }
        assert_eq!(
            encounter.brain().state(),
            crate::adversary::AdversaryState::Idle,
            "a reset returns the brain to idle"
        );
    }

    #[test]
    fn a_distant_adversary_is_dormant_and_costs_the_stream_nothing() {
        let ground = ground();
        let mut setup = fixture::golden_setup();
        setup.arena = None;
        // Well outside the aggro radius and staying there.
        setup.starts = [Vec2::new(0.0, 0.0), Vec2::new(0.0, -200.0)];
        let mut encounter = armed(&setup, &ground);

        // One tick first, because `start_state` hands out an unwrapped facing
        // and the first `turn_toward` normalises `pi` to `-pi`. That is a
        // representation moving, not a body, and the property under test is
        // that nothing moves *afterwards*.
        let _ = encounter.step(
            Intent::player(Vec2::ZERO, false, false),
            WorldContact::ground_only(&ground),
        );
        let at_rest = encounter.combatant(Side::Adversary).position();
        let facing = encounter.combatant(Side::Adversary).state().facing;
        let decisions = encounter.brain().decisions();
        for _ in 0..10_000 {
            let events = encounter.step(
                Intent::player(Vec2::ZERO, false, false),
                WorldContact::ground_only(&ground),
            );
            assert!(events.is_empty(), "a dormant adversary produced an event");
        }
        assert_eq!(
            encounter.brain().state(),
            crate::adversary::AdversaryState::Idle
        );
        assert_eq!(
            encounter.brain().decisions(),
            decisions,
            "a dormant brain drew from its deterministic stream"
        );
        assert_eq!(
            encounter.combatant(Side::Adversary).position(),
            at_rest,
            "a dormant adversary moved"
        );
        assert_eq!(
            encounter.combatant(Side::Adversary).state().facing,
            facing,
            "a dormant adversary turned"
        );
        assert_eq!(encounter.counters().swings[Side::Adversary.index()], 0);
        assert_eq!(
            encounter.counters().blocked_moves[Side::Adversary.index()],
            0
        );
    }

    #[test]
    fn an_adversary_wakes_when_the_player_comes_inside_its_aggro_radius() {
        let ground = ground();
        let aggro = fixture::adversary().aggro_radius;
        let mut setup = fixture::golden_setup();
        setup.arena = None;
        setup.starts = [Vec2::new(0.0, aggro + 6.0), Vec2::new(0.0, 0.0)];
        let mut encounter = armed(&setup, &ground);
        assert_eq!(
            encounter.brain().state(),
            crate::adversary::AdversaryState::Idle
        );

        let mut woke_at = None;
        for tick in 0..4_000_u64 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.0, -1.0), false, false),
                WorldContact::ground_only(&ground),
            );
            if woke_at.is_none()
                && encounter.brain().state() != crate::adversary::AdversaryState::Idle
            {
                woke_at = Some((tick, encounter.separation_distance()));
            }
        }
        let Some((tick, distance)) = woke_at else {
            panic!("the adversary never woke up");
        };
        assert!(tick > 0, "it woke before the player had moved");
        assert!(
            distance <= aggro + 0.1,
            "it woke at {distance}, outside its aggro radius of {aggro}"
        );

        // The same trace produces the same wake tick.
        let mut again = armed(&setup, &ground);
        let mut second = None;
        for tick in 0..4_000_u64 {
            let _ = again.step(
                Intent::player(Vec2::new(0.0, -1.0), false, false),
                WorldContact::ground_only(&ground),
            );
            if second.is_none() && again.brain().state() != crate::adversary::AdversaryState::Idle {
                second = Some(tick);
            }
        }
        assert_eq!(second, Some(tick), "the wake tick is not reproducible");
    }

    #[test]
    fn the_arena_holds_both_bodies_whatever_they_are_asked_to_do() {
        let ground = ground();
        let setup = fixture::golden_setup();
        let mut encounter = armed(&setup, &ground);
        let Some(arena) = setup.arena else {
            panic!("the golden fixture has an arena");
        };
        for tick in 0..4_000 {
            // Drive hard at the boundary, alternating direction.
            let push = if (tick / 200) % 2 == 0 {
                Vec2::new(1.0, 0.3)
            } else {
                Vec2::new(-0.6, -1.0)
            };
            let _ = encounter.step(
                Intent::player(push, tick % 97 == 0, tick % 53 == 0),
                WorldContact::ground_only(&ground),
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
        setup.starts = [Vec2::ZERO, Vec2::ZERO];
        let mut encounter = armed(&setup, &ground);
        let mut worst = 0.0_f32;
        for tick in 0..2_000 {
            let _ = encounter.step(
                Intent::player(Vec2::new(0.0, -1.0), tick % 61 == 0, false),
                WorldContact::ground_only(&ground),
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
        setup.starts = [Vec2::new(0.0, 0.0), Vec2::new(0.0, -2.0)];
        let mut encounter = armed(&setup, &step);
        for _ in 0..1_200 {
            let _ = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), false, false),
                WorldContact::ground_only(&step),
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
                WorldContact::none(),
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
            let events = encounter.step(
                Intent::player(move_world, true, true),
                WorldContact::ground_only(&ground),
            );
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
                WorldContact::ground_only(&counting),
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
                WorldContact::ground_only(&ground),
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
                WorldContact::ground_only(&ground),
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

    // -----------------------------------------------------------------------
    // Combat initiative
    // -----------------------------------------------------------------------

    /// The initiative tuning on open flat ground, the two bodies `separation`
    /// apart and facing each other.
    fn initiative(separation: f32) -> EncounterSetup {
        let mut setup = fixture::golden_setup();
        setup.tuning = fixture::initiative_tuning();
        setup.arena = None;
        setup.starts = [
            Vec2::new(0.0, separation * 0.5),
            Vec2::new(0.0, -separation * 0.5),
        ];
        setup
    }

    fn middle_of_the_band() -> f32 {
        let pressure = fixture::pressure();
        (pressure.select_min + pressure.select_max) * 0.5
    }

    #[test]
    fn a_lunge_is_run_by_its_own_spec_along_the_line_it_committed_to() {
        let ground = ground();
        let mut encounter = armed(&initiative(middle_of_the_band()), &ground);
        let pressure = *encounter
            .tuning()
            .adversary_pressure()
            .unwrap_or_else(|| panic!("the initiative tuning has a lunge"))
            .attack();
        let primary = *encounter.tuning().adversary_attack();
        assert_eq!(
            *encounter.attack_spec(Side::Adversary),
            primary,
            "not swinging: primary"
        );
        let mut start = None;
        let mut facing = None;
        let mut dodged = false;
        for _ in 0..400 {
            let adversary = encounter.combatant(Side::Adversary);
            let lunging = adversary.action().attack_kind() == Some(AttackKind::Pressure);
            if lunging {
                assert_eq!(*encounter.attack_spec(Side::Adversary), pressure);
                let now = adversary.state().facing;
                let locked = *facing.get_or_insert(now);
                assert!(
                    (now - locked).abs() < 1.0e-6,
                    "the line turned during the lunge"
                );
                start.get_or_insert(adversary.position());
            } else if start.is_some() {
                break;
            }
            // The player steps off the line early, so the lunge meets nothing
            // and travels its whole distance.
            let press = lunging && !dodged && adversary.action().elapsed() >= 20;
            dodged |= press;
            let _ = encounter.step(
                Intent::player(Vec2::new(1.0, 0.0), false, press),
                WorldContact::ground_only(&ground),
            );
            if let Some(origin) = start
                && !encounter.combatant(Side::Adversary).action().is_attacking()
            {
                let travelled = (encounter.combatant(Side::Adversary).position() - origin).length();
                assert!(
                    (travelled - pressure.lunge()).abs() < 0.05,
                    "travelled {travelled}, authored {}",
                    pressure.lunge()
                );
            }
        }
        assert!(start.is_some(), "the lunge never started");
        assert_eq!(encounter.counters().pressure_swings, 1);
        assert_eq!(encounter.counters().pressure_whiffs, 1);
        assert_eq!(encounter.counters().pressure_hits, 0);
    }

    #[test]
    fn a_connected_lunge_recovers_briefly_and_a_missed_one_does_not() {
        let ground = ground();
        for dodge in [false, true] {
            let mut encounter = armed(&initiative(middle_of_the_band()), &ground);
            let spec = *encounter
                .tuning()
                .adversary_pressure()
                .unwrap_or_else(|| panic!("the initiative tuning has a lunge"))
                .attack();
            // Every tick on which the lunge was the adversary's action,
            // including the one that started it and the one it ended on.
            let mut steps = 0_u32;
            let mut dodged = false;
            for _ in 0..500 {
                let before = encounter.combatant(Side::Adversary).action().attack_kind()
                    == Some(AttackKind::Pressure);
                let press = dodge
                    && before
                    && !dodged
                    && encounter.combatant(Side::Adversary).action().elapsed() >= 20;
                dodged |= press;
                // Standing still unless dodging: a walk sideways would escape
                // the lunge on its own.
                let across = if dodge {
                    Vec2::new(1.0, 0.0)
                } else {
                    Vec2::ZERO
                };
                let _ = encounter.step(
                    Intent::player(across, false, press),
                    WorldContact::ground_only(&ground),
                );
                let after = encounter.combatant(Side::Adversary).action().attack_kind()
                    == Some(AttackKind::Pressure);
                if before || after {
                    steps += 1;
                }
                if before && !after {
                    break;
                }
            }
            let connected = encounter.counters().pressure_hits == 1;
            assert_eq!(connected, !dodge);
            // A hit freezes both bodies for the hitstop, and a frozen tick
            // does not advance the action.
            let frozen = if connected { spec.hitstop() } else { 0 };
            assert_eq!(
                steps,
                spec.total_for(connected) + frozen,
                "connected {connected}: the lunge ran {steps} ticks"
            );
        }
    }

    #[test]
    fn the_adversary_spaces_with_its_own_dodge_and_the_player_keeps_the_historical_one() {
        let ground = ground();
        let encounter = armed(&initiative(6.0), &ground);
        let spacing = *encounter
            .tuning()
            .adversary_pressure()
            .unwrap_or_else(|| panic!("the initiative tuning has a lunge"))
            .spacing();
        assert_eq!(*encounter.dodge_spec(Side::Adversary), spacing);
        assert_eq!(
            *encounter.dodge_spec(Side::Player),
            *encounter.tuning().dodge()
        );
        let historical = armed(&fixture::golden_setup(), &ground);
        assert_eq!(
            *historical.dodge_spec(Side::Adversary),
            *historical.tuning().dodge(),
            "without a lunge the adversary's dodge is the shared one"
        );
    }

    #[test]
    fn a_spacing_dodge_on_open_ground_opens_its_whole_distance() {
        let ground = ground();
        // Close enough that the player's first swing lands and staggers the
        // adversary, which is the spacing dodge's trigger.
        let mut encounter = armed(&initiative(2.4), &ground);
        let mut dodge_started = None;
        let mut swung = false;
        for _ in 0..400 {
            let swing = !swung && encounter.combatant(Side::Player).can_act();
            swung |= swing;
            let events = encounter.step(
                Intent::player(Vec2::ZERO, swing, false),
                WorldContact::ground_only(&ground),
            );
            if events.any(|event| {
                matches!(
                    event,
                    CombatEvent::DodgeStarted {
                        side: Side::Adversary,
                        ..
                    }
                )
            }) {
                dodge_started = Some(encounter.combatant(Side::Adversary).position());
            }
            if dodge_started.is_some()
                && !encounter.combatant(Side::Adversary).action().is_dodging()
            {
                break;
            }
        }
        assert!(
            dodge_started.is_some(),
            "the stagger was never answered with space"
        );
        let counters = encounter.counters();
        assert_eq!(counters.dodges[1], 1);
        assert_eq!(counters.dodges_truncated[1], 0);
        assert_eq!(counters.dodge_blocked_moves[1], 0);
        assert_eq!(counters.dodges_during_unresolved_swing[1], 0);
    }
}
