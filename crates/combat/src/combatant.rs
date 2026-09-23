//! The two combatants, and the typed state one of them can be in.
//!
//! There are exactly two, they are indexed by [`Side`], and that is the whole
//! entity model. No identifier space, no allocator, no map, no ECS: with a fixed
//! pair every state is exhaustively matchable and an impossible combination is a
//! compile error rather than a runtime check. ADR-0005 records why extending to
//! a third actor is a decision about an entity model rather than another array.
//!
//! What distinguishes the player from the adversary is **one field and who
//! produces the intent**. Both run the same timeline, the same hit query, the
//! same damage, the same movement rules and the same reaction. If they needed
//! two code paths the model would be wrong.

use glam::{Vec2, Vec3};

use veldwake_character::action::ActionOverlay;
use veldwake_character::collision::{BodyCapsule, CollisionRepresentation};
use veldwake_character::skeleton::Side as BodySide;
use veldwake_character::{CharacterState, PosedCharacter};

use crate::hit::{Capsule, Segment};
use crate::hurt::HurtVolume;
use crate::spec::{AttackSpec, DodgeSpec};
use crate::tick::Ticks;

/// Which of the two combatants.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Side {
    Player,
    Adversary,
}

/// Both sides, in the order every tick resolves them.
///
/// The order is part of the determinism contract: a tick that would otherwise
/// depend on iteration order resolves the player first.
pub const SIDES: [Side; 2] = [Side::Player, Side::Adversary];

impl Side {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Player => 0,
            Self::Adversary => 1,
        }
    }

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Player => Self::Adversary,
            Self::Adversary => Self::Player,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Adversary => "adversary",
        }
    }
}

/// How much punishment a combatant has left.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Health {
    current: u16,
    max: u16,
}

impl Health {
    #[must_use]
    pub const fn full(max: u16) -> Self {
        Self { current: max, max }
    }

    /// A partially spent body.
    ///
    /// `current` is clamped to `max`, so no caller can describe a body with
    /// more health than it can hold. A fight only ever produces these by
    /// damaging a full one; this exists for the tests that have to ask what a
    /// readout does at every health between the two ends.
    #[must_use]
    pub const fn new(current: u16, max: u16) -> Self {
        Self {
            current: if current > max { max } else { current },
            max,
        }
    }

    #[must_use]
    pub const fn current(&self) -> u16 {
        self.current
    }

    #[must_use]
    pub const fn max(&self) -> u16 {
        self.max
    }

    #[must_use]
    pub const fn is_defeated(&self) -> bool {
        self.current == 0
    }

    /// How much is left, in `[0, 1]`. Used by the world-space readout.
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.max == 0 {
            return 0.0;
        }
        f32::from(self.current) / f32::from(self.max)
    }

    /// Applies damage and returns what is left.
    ///
    /// Saturating, so damage at zero cannot wrap a body back to full.
    pub fn apply(&mut self, damage: u16) -> u16 {
        self.current = self.current.saturating_sub(damage);
        self.current
    }

    /// Back to full, for an encounter reset.
    pub fn restore(&mut self) {
        self.current = self.max;
    }
}

/// A swing's identity.
///
/// Monotonic per combatant and never reused inside an encounter, which is what
/// makes "this swing has already hit that body" a fact rather than a guess.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SwingId(pub(crate) u32);

impl SwingId {
    /// The identifier a combatant's first swing gets.
    ///
    /// Exposed so a measurement can build an attack state without an encounter;
    /// nothing else may invent one, because uniqueness is what makes
    /// one-hit-per-swing a fact.
    #[must_use]
    pub const fn first() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Which of the two attacks a swing is.
///
/// Two, and that is the whole universe: no moveset, no registry, no list of
/// attacks. `Primary` is the historical M6 swing every combatant has.
/// `Pressure` is the adversary's lunge from middle distance, and exists only in
/// an encounter whose tuning authors one
/// ([`crate::spec::AuthoredPressure`]). The player only ever swings `Primary`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AttackKind {
    #[default]
    Primary,
    Pressure,
}

impl AttackKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Pressure => "pressure",
        }
    }
}

/// Which phase of a swing a tick is in.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AttackPhase {
    /// Anticipation. For the adversary, the telegraph.
    Windup,
    /// The blade can connect.
    Active,
    /// Commitment: the swing is spent.
    Recovery,
}

impl AttackPhase {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Windup => "windup",
            Self::Active => "active",
            Self::Recovery => "recovery",
        }
    }
}

/// What a combatant is doing.
///
/// Every duration is a tick count, and every elapsed counter is a tick count, so
/// a phase boundary is an integer comparison and a fractional tick does not
/// exist. The directions are the one non-integer part, which is why this is
/// `PartialEq` and not `Eq`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Able to move, attack and dodge.
    Free,
    Attack {
        swing: SwingId,
        /// Which of the two attacks this is. Everything that needs the spec
        /// of a running swing reads it from here, so a swing cannot change
        /// kind half way through.
        kind: AttackKind,
        elapsed: Ticks,
        /// Which sides this swing has already hit, indexed by [`Side::index`].
        hits: [bool; SIDES.len()],
    },
    Dodge {
        elapsed: Ticks,
        duration: Ticks,
        /// World-space direction of travel.
        direction: Vec2,
    },
    Stagger {
        elapsed: Ticks,
        duration: Ticks,
        /// World-space direction the hit came from.
        from: Vec2,
    },
    Defeated {
        elapsed: Ticks,
    },
}

impl Action {
    #[must_use]
    pub const fn is_free(&self) -> bool {
        matches!(self, Self::Free)
    }

    #[must_use]
    pub const fn is_defeated(&self) -> bool {
        matches!(self, Self::Defeated { .. })
    }

    #[must_use]
    pub const fn is_attacking(&self) -> bool {
        matches!(self, Self::Attack { .. })
    }

    #[must_use]
    pub const fn is_dodging(&self) -> bool {
        matches!(self, Self::Dodge { .. })
    }

    /// Which attack is running, if one is.
    #[must_use]
    pub const fn attack_kind(&self) -> Option<AttackKind> {
        match self {
            Self::Attack { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// Whether a running swing has connected with anybody.
    ///
    /// A swing can only ever hit the other body, so any mark means it
    /// connected. This is what an outcome-dependent recovery is decided by.
    #[must_use]
    pub const fn connected(&self) -> bool {
        match self {
            Self::Attack { hits, .. } => hits[0] || hits[1],
            _ => false,
        }
    }

    /// The phase of a swing, given the spec that governs it.
    #[must_use]
    pub const fn attack_phase(&self, spec: &AttackSpec) -> Option<AttackPhase> {
        match self {
            Self::Attack { elapsed, .. } => {
                if *elapsed < spec.active_start() {
                    Some(AttackPhase::Windup)
                } else if *elapsed < spec.active_end() {
                    Some(AttackPhase::Active)
                } else {
                    Some(AttackPhase::Recovery)
                }
            }
            _ => None,
        }
    }

    /// A label for a report line.
    #[must_use]
    pub const fn label(&self, spec: &AttackSpec) -> &'static str {
        match self.attack_phase(spec) {
            Some(AttackPhase::Windup) => "windup",
            Some(AttackPhase::Active) => "active",
            Some(AttackPhase::Recovery) => "recovery",
            None => match self {
                Self::Free => "free",
                Self::Dodge { .. } => "dodge",
                Self::Stagger { .. } => "stagger",
                Self::Defeated { .. } => "defeated",
                Self::Attack { .. } => "attack",
            },
        }
    }

    /// Elapsed ticks inside the current action, or zero when free.
    #[must_use]
    pub const fn elapsed(&self) -> Ticks {
        match self {
            Self::Free => 0,
            Self::Attack { elapsed, .. }
            | Self::Dodge { elapsed, .. }
            | Self::Stagger { elapsed, .. }
            | Self::Defeated { elapsed } => *elapsed,
        }
    }

    /// The pose layer this action asks for.
    ///
    /// `Free` still carries a weapon, which is a pose and not an absence of one:
    /// a rigid blade driven only by the gait's arm swing reaches the ground.
    #[must_use]
    pub fn overlay(&self, spec: &AttackSpec, weapon_side: BodySide, facing: f32) -> ActionOverlay {
        match self {
            Self::Free => ActionOverlay::carry(weapon_side),
            // A fixed window rather than the defeat hold: the collapse takes
            // as long as a collapse takes, and how long the body then lies
            // there before the fight resets is a separate decision.
            Self::Defeated { elapsed } => {
                ActionOverlay::defeated(weapon_side, progress(*elapsed, DEFEAT_SAG_TICKS))
            }
            Self::Attack {
                kind: AttackKind::Primary,
                elapsed,
                ..
            } => ActionOverlay::attack(
                weapon_side,
                progress(*elapsed, spec.total()),
                spec.windup_fraction(),
                spec.active_fraction(),
            ),
            // A lunge is posed against the recovery it is actually running:
            // the short one once it has connected, the long one otherwise.
            // The windup and active keys are phase-local, so the total
            // changing on the tick of a hit moves nothing that is on screen;
            // only the recovery, which a hit can never interrupt, is shorter.
            Self::Attack {
                kind: AttackKind::Pressure,
                elapsed,
                ..
            } => {
                let total = spec.total_for(self.connected()).max(1);
                ActionOverlay::lunge(
                    weapon_side,
                    progress(*elapsed, total),
                    spec.windup() as f32 / total as f32,
                    spec.active() as f32 / total as f32,
                )
            }
            Self::Dodge {
                elapsed,
                duration,
                direction,
            } => ActionOverlay::dodge(
                weapon_side,
                progress(*elapsed, *duration),
                relative_angle(*direction, facing),
            ),
            Self::Stagger {
                elapsed,
                duration,
                from,
            } => ActionOverlay::stagger(
                weapon_side,
                progress(*elapsed, *duration),
                relative_angle(*from, facing),
            ),
        }
    }
}

/// How long a defeated body takes to sag into its final pose, in ticks.
///
/// A fifth of a second. Long enough not to snap, short enough that the body is
/// already still by the time a viewer has registered what happened, and
/// deliberately independent of the defeat hold: the hold is how long the fight
/// waits before resetting, which is a pacing decision and not an animation one.
pub const DEFEAT_SAG_TICKS: Ticks = 24;

/// Where a tick sits inside an action, in `[0, 1]`.
#[must_use]
pub fn progress(elapsed: Ticks, duration: Ticks) -> f32 {
    if duration == 0 {
        return 1.0;
    }
    (elapsed as f32 / duration as f32).clamp(0.0, 1.0)
}

/// A world direction expressed relative to a body's facing, in radians.
///
/// Zero is straight ahead. The character's facing convention is the client
/// camera's: yaw zero faces `-Z` and `+yaw` turns toward `+X`.
#[must_use]
pub fn relative_angle(direction: Vec2, facing: f32) -> f32 {
    if direction.length_squared() <= 1.0e-12 || !direction.is_finite() || !facing.is_finite() {
        return 0.0;
    }
    let world = direction.x.atan2(-direction.y);
    let relative = world - facing;
    // Wrap into (-pi, pi] so a pose never sees a growing angle.
    let tau = std::f32::consts::TAU;
    let wrapped = (relative + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    if wrapped.is_finite() { wrapped } else { 0.0 }
}

/// What a combatant wants to do this tick.
///
/// The player's comes from latched input; the adversary's comes from its brain.
/// Both go through the same rules, which is the point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Intent {
    move_world: Vec2,
    attack: bool,
    /// Which attack `attack` asks for. Always `Primary` for the player.
    attack_kind: AttackKind,
    dodge: bool,
    face_foe: bool,
}

impl Intent {
    /// Nothing at all.
    #[must_use]
    pub fn idle() -> Self {
        Self::default()
    }

    /// What the client submits: a planar direction and two latched buttons.
    ///
    /// The direction is sanitised here rather than trusted: a client that hands
    /// over a `NaN` axis or an over-long vector gets a clamped intent, not a
    /// body that teleports.
    #[must_use]
    pub fn player(move_world: Vec2, attack: bool, dodge: bool) -> Self {
        Self {
            move_world: sanitise(move_world),
            attack,
            attack_kind: AttackKind::Primary,
            dodge,
            face_foe: false,
        }
    }

    /// What the adversary's brain produces.
    #[must_use]
    pub(crate) fn adversary(move_world: Vec2, attack: bool) -> Self {
        Self {
            move_world: sanitise(move_world),
            attack,
            attack_kind: AttackKind::Primary,
            dodge: false,
            face_foe: true,
        }
    }

    /// The adversary commits its pressure lunge, standing where it is.
    #[must_use]
    pub(crate) fn adversary_pressure() -> Self {
        Self {
            move_world: Vec2::ZERO,
            attack: true,
            attack_kind: AttackKind::Pressure,
            dodge: false,
            face_foe: true,
        }
    }

    /// The adversary takes its spacing dodge in `direction`.
    ///
    /// The same dodge the player has, run by the same rule: no new action, no
    /// invulnerability, the same movement legality and the same cooldown.
    #[must_use]
    pub(crate) fn adversary_dodge(direction: Vec2) -> Self {
        Self {
            move_world: sanitise(direction),
            attack: false,
            attack_kind: AttackKind::Primary,
            dodge: true,
            face_foe: true,
        }
    }

    #[must_use]
    pub const fn move_world(&self) -> Vec2 {
        self.move_world
    }

    #[must_use]
    pub const fn attack(&self) -> bool {
        self.attack
    }

    #[must_use]
    pub const fn attack_kind(&self) -> AttackKind {
        self.attack_kind
    }

    #[must_use]
    pub const fn dodge(&self) -> bool {
        self.dodge
    }

    #[must_use]
    pub const fn face_foe(&self) -> bool {
        self.face_foe
    }
}

fn sanitise(direction: Vec2) -> Vec2 {
    if !direction.is_finite() {
        return Vec2::ZERO;
    }
    let length = direction.length();
    if length > 1.0 {
        direction / length
    } else {
        direction
    }
}

/// One combatant's whole mutable state.
///
/// The compiled character and the compiled weapon are **not** here: they are
/// identity, they never change, and the encounter owns one of each per side.
#[derive(Clone, Debug, PartialEq)]
pub struct Combatant {
    side: Side,
    state: CharacterState,
    health: Health,
    action: Action,
    next_swing: u32,
    hitstop: Ticks,
    dodge_cooldown: Ticks,
    capsule: BodyCapsule,
    hurt: HurtVolume,
    weapon_side: BodySide,
    posed: PosedCharacter,
    /// The blade at the end of the previous tick, which is what a sweep starts
    /// from. `None` until the first pose exists.
    blade: Option<Segment>,
    /// The hurt capsule at the previous tick's pose, paired with `blade` so
    /// contact accounts for realised motion of both combatants.
    previous_hurt: Option<Capsule>,
}

impl Combatant {
    /// Builds a combatant around an already posed body.
    #[must_use]
    pub fn new(
        side: Side,
        state: CharacterState,
        health: Health,
        capsule: BodyCapsule,
        hurt: HurtVolume,
        weapon_side: BodySide,
        posed: PosedCharacter,
    ) -> Self {
        Self {
            side,
            state,
            health,
            action: Action::Free,
            next_swing: 0,
            hitstop: 0,
            dodge_cooldown: 0,
            capsule,
            hurt,
            weapon_side,
            posed,
            blade: None,
            previous_hurt: None,
        }
    }

    #[must_use]
    pub const fn side(&self) -> Side {
        self.side
    }

    #[must_use]
    pub const fn state(&self) -> &CharacterState {
        &self.state
    }

    #[must_use]
    pub const fn health(&self) -> Health {
        self.health
    }

    #[must_use]
    pub const fn action(&self) -> &Action {
        &self.action
    }

    #[must_use]
    pub const fn hitstop(&self) -> Ticks {
        self.hitstop
    }

    #[must_use]
    pub const fn dodge_cooldown(&self) -> Ticks {
        self.dodge_cooldown
    }

    #[must_use]
    pub const fn weapon_side(&self) -> BodySide {
        self.weapon_side
    }

    #[must_use]
    pub const fn posed(&self) -> &PosedCharacter {
        &self.posed
    }

    /// The blade at the end of the previous tick.
    #[must_use]
    pub const fn previous_blade(&self) -> Option<Segment> {
        self.blade
    }

    /// The hurt capsule at the end of the previous tick.
    #[must_use]
    pub const fn previous_hurt_capsule(&self) -> Option<Capsule> {
        self.previous_hurt
    }

    /// Planar position.
    #[must_use]
    pub fn position(&self) -> Vec2 {
        Vec2::new(self.state.x, self.state.z)
    }

    /// The point the body stands on.
    #[must_use]
    pub fn stand_point(&self) -> Vec3 {
        self.state.stand_point()
    }

    /// The volume a blade has to touch to hurt this body.
    ///
    /// [`HurtVolume`] measured from the body's core, placed at the stand point.
    /// Not the M5 whole-body capsule beside it: that one is as wide as the hands
    /// reach and took hits in the air above the head. The arms, the hands, the
    /// feet and the weapon can leave this volume, and a swing that only grazes
    /// an outstretched arm therefore does not register; the head, chest, pelvis
    /// and legs staying inside it through every combat moment is an asserted
    /// property rather than a hope.
    #[must_use]
    pub fn hurt_capsule(&self) -> Capsule {
        self.hurt.placed(self.stand_point())
    }

    #[must_use]
    pub const fn hurt(&self) -> HurtVolume {
        self.hurt
    }

    /// The whole-body capsule, which is what keeps two bodies from standing
    /// inside each other. Never the hit volume; see [`Self::hurt_capsule`].
    #[must_use]
    pub const fn capsule(&self) -> BodyCapsule {
        self.capsule
    }

    /// Whether this body can start an action this tick.
    #[must_use]
    pub const fn can_act(&self) -> bool {
        self.hitstop == 0 && self.action.is_free()
    }

    /// Whether this tick's timers advance at all.
    #[must_use]
    pub const fn is_frozen(&self) -> bool {
        self.hitstop > 0
    }

    // The mutators below are crate-private on purpose: a combatant changes only
    // inside an encounter tick, in the documented order.

    pub(crate) fn state_mut(&mut self) -> &mut CharacterState {
        &mut self.state
    }

    pub(crate) fn set_action(&mut self, action: Action) {
        self.action = action;
    }

    pub(crate) fn take_swing_id(&mut self) -> SwingId {
        let id = SwingId(self.next_swing);
        self.next_swing = self.next_swing.wrapping_add(1);
        id
    }

    pub(crate) fn set_hitstop(&mut self, ticks: Ticks) {
        self.hitstop = self.hitstop.max(ticks);
    }

    pub(crate) fn tick_hitstop(&mut self) {
        self.hitstop = self.hitstop.saturating_sub(1);
    }

    pub(crate) fn tick_dodge_cooldown(&mut self) {
        self.dodge_cooldown = self.dodge_cooldown.saturating_sub(1);
    }

    pub(crate) fn start_dodge_cooldown(&mut self, spec: &DodgeSpec) {
        self.dodge_cooldown = spec.duration().saturating_add(spec.cooldown());
    }

    pub(crate) fn health_mut(&mut self) -> &mut Health {
        &mut self.health
    }

    /// Replaces the pose and refits the hurt volume to it.
    ///
    /// The two go together on purpose. A pose without its volume would leave a
    /// hit being decided against the body of a tick ago.
    pub(crate) fn set_posed(&mut self, posed: PosedCharacter, collision: &CollisionRepresentation) {
        self.hurt = HurtVolume::from_pose(collision, posed.part_matrices(), self.stand_point());
        self.posed = posed;
    }

    pub(crate) fn set_blade(&mut self, blade: Segment) {
        self.blade = Some(blade);
    }

    pub(crate) fn set_previous_hurt_capsule(&mut self, hurt: Capsule) {
        self.previous_hurt = Some(hurt);
    }

    pub(crate) fn clear_blade(&mut self) {
        self.blade = None;
        self.previous_hurt = None;
    }

    pub(crate) fn reset(&mut self, state: CharacterState) {
        self.state = state;
        self.health.restore();
        self.action = Action::Free;
        self.hitstop = 0;
        self.dodge_cooldown = 0;
        self.blade = None;
        self.previous_hurt = None;
        // `next_swing` deliberately keeps counting: a swing identifier is unique
        // for the life of the encounter, not for the life of one round.
    }

    /// Marks a target as hit by the current swing, and reports whether it was
    /// already marked.
    pub(crate) fn register_hit(&mut self, target: Side) -> bool {
        if let Action::Attack { hits, .. } = &mut self.action {
            let index = target.index();
            if hits[index] {
                return false;
            }
            hits[index] = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Action, AttackPhase, Health, Intent, SIDES, Side, SwingId, progress, relative_angle,
    };
    use crate::spec::{AuthoredAttack, AuthoredDodge};
    use glam::Vec2;
    use std::f32::consts::{FRAC_PI_2, PI};
    use veldwake_character::skeleton::Side as BodySide;

    fn attack_spec() -> crate::spec::AttackSpec {
        match (AuthoredAttack {
            windup_seconds: 0.18,
            active_seconds: 0.10,
            recovery_seconds: 0.34,
            stagger_seconds: 0.30,
            hitstop_seconds: 0.07,
            damage: 24,
            step_in: 0.35,
            knockback: 0.35,
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn the_two_sides_index_a_fixed_pair_and_know_their_opposite() {
        assert_eq!(SIDES.len(), 2);
        assert_eq!(Side::Player.index(), 0);
        assert_eq!(Side::Adversary.index(), 1);
        assert_eq!(Side::Player.other(), Side::Adversary);
        assert_eq!(Side::Adversary.other(), Side::Player);
        assert_eq!(SIDES[0], Side::Player, "the player resolves first");
        for side in SIDES {
            assert!(!side.name().is_empty());
        }
    }

    #[test]
    fn health_saturates_and_cannot_wrap_back_to_full() {
        let mut health = Health::full(100);
        assert_eq!(health.current(), 100);
        assert_eq!(health.max(), 100);
        assert!(!health.is_defeated());
        assert_eq!(health.apply(30), 70);
        assert_eq!(health.apply(80), 0, "damage past zero saturates");
        assert!(health.is_defeated());
        assert_eq!(health.apply(1_000), 0, "a defeated body stays defeated");
        assert_eq!(health.fraction(), 0.0);
        health.restore();
        assert_eq!(health.current(), 100);
        assert!((health.fraction() - 1.0).abs() < 1.0e-6);
        assert_eq!(Health::full(0).fraction(), 0.0);
    }

    #[test]
    fn the_attack_phase_boundaries_are_the_specs_own() {
        let spec = attack_spec();
        let at = |elapsed| {
            Action::Attack {
                swing: SwingId(0),
                kind: crate::combatant::AttackKind::Primary,
                elapsed,
                hits: [false; 2],
            }
            .attack_phase(&spec)
        };
        assert_eq!(at(0), Some(AttackPhase::Windup));
        assert_eq!(at(spec.active_start() - 1), Some(AttackPhase::Windup));
        assert_eq!(at(spec.active_start()), Some(AttackPhase::Active));
        assert_eq!(at(spec.active_end() - 1), Some(AttackPhase::Active));
        assert_eq!(at(spec.active_end()), Some(AttackPhase::Recovery));
        assert_eq!(at(spec.total() - 1), Some(AttackPhase::Recovery));
        assert_eq!(Action::Free.attack_phase(&spec), None);
        for phase in [
            AttackPhase::Windup,
            AttackPhase::Active,
            AttackPhase::Recovery,
        ] {
            assert!(!phase.name().is_empty());
        }
    }

    #[test]
    fn every_action_has_a_label_and_an_elapsed_count() {
        let spec = attack_spec();
        let cases = [
            (Action::Free, "free", 0),
            (
                Action::Attack {
                    swing: SwingId(1),
                    kind: crate::combatant::AttackKind::Primary,
                    elapsed: 5,
                    hits: [false; 2],
                },
                "windup",
                5,
            ),
            (
                Action::Attack {
                    swing: SwingId(1),
                    kind: crate::combatant::AttackKind::Primary,
                    elapsed: spec.active_start(),
                    hits: [false; 2],
                },
                "active",
                spec.active_start(),
            ),
            (
                Action::Attack {
                    swing: SwingId(1),
                    kind: crate::combatant::AttackKind::Primary,
                    elapsed: spec.active_end(),
                    hits: [false; 2],
                },
                "recovery",
                spec.active_end(),
            ),
            (
                Action::Dodge {
                    elapsed: 3,
                    duration: 36,
                    direction: Vec2::Y,
                },
                "dodge",
                3,
            ),
            (
                Action::Stagger {
                    elapsed: 4,
                    duration: 36,
                    from: Vec2::Y,
                },
                "stagger",
                4,
            ),
            (Action::Defeated { elapsed: 9 }, "defeated", 9),
        ];
        for (action, label, elapsed) in cases {
            assert_eq!(action.label(&spec), label);
            assert_eq!(action.elapsed(), elapsed);
        }
        assert!(Action::Free.is_free());
        assert!(Action::Defeated { elapsed: 0 }.is_defeated());
        assert!(
            Action::Attack {
                swing: SwingId(0),
                kind: crate::combatant::AttackKind::Primary,
                elapsed: 0,
                hits: [false; 2]
            }
            .is_attacking()
        );
        assert!(
            Action::Dodge {
                elapsed: 0,
                duration: 1,
                direction: Vec2::Y
            }
            .is_dodging()
        );
    }

    #[test]
    fn a_free_combatant_still_carries_its_weapon() {
        // The arithmetic reason `Carry` exists: without it a rigid blade driven
        // by the gait's arm swing reaches the ground.
        let spec = attack_spec();
        let overlay = Action::Free.overlay(&spec, BodySide::Right, 0.0);
        assert_eq!(
            overlay.kind(),
            veldwake_character::ActionKind::Carry,
            "a free combatant is not empty handed"
        );
        assert_eq!(overlay.weapon_side(), BodySide::Right);
    }

    #[test]
    fn an_attacks_overlay_progress_follows_its_tick_count() {
        let spec = attack_spec();
        let overlay = |elapsed| {
            Action::Attack {
                swing: SwingId(0),
                kind: crate::combatant::AttackKind::Primary,
                elapsed,
                hits: [false; 2],
            }
            .overlay(&spec, BodySide::Right, 0.0)
        };
        assert_eq!(overlay(0).progress(), 0.0);
        assert!((overlay(spec.total()).progress() - 1.0).abs() < 1.0e-6);
        // The pose's phase boundary is the rules' phase boundary.
        let at_active = overlay(spec.active_start()).progress();
        assert!((at_active - spec.windup_fraction()).abs() < 1.0e-6);
    }

    #[test]
    fn progress_is_bounded_and_defined_for_a_zero_length_action() {
        assert_eq!(progress(0, 10), 0.0);
        assert!((progress(5, 10) - 0.5).abs() < 1.0e-6);
        assert_eq!(progress(10, 10), 1.0);
        assert_eq!(progress(99, 10), 1.0);
        assert_eq!(progress(0, 0), 1.0);
    }

    #[test]
    fn a_direction_becomes_an_angle_relative_to_the_bodys_own_facing() {
        // Facing zero looks along -Z, so a hit arriving from -Z is straight
        // ahead and one from +X is to the body's right.
        assert!(relative_angle(Vec2::new(0.0, -1.0), 0.0).abs() < 1.0e-6);
        assert!((relative_angle(Vec2::new(1.0, 0.0), 0.0) - FRAC_PI_2).abs() < 1.0e-6);
        assert!((relative_angle(Vec2::new(0.0, 1.0), 0.0).abs() - PI).abs() < 1.0e-5);
        // Turning the body turns the relative angle with it.
        assert!(relative_angle(Vec2::new(1.0, 0.0), FRAC_PI_2).abs() < 1.0e-6);
        // Degenerate and hostile inputs are zero rather than a NaN.
        assert_eq!(relative_angle(Vec2::ZERO, 0.0), 0.0);
        assert_eq!(relative_angle(Vec2::splat(f32::NAN), 0.0), 0.0);
        assert_eq!(relative_angle(Vec2::Y, f32::NAN), 0.0);
        // The result never grows past half a turn.
        for turns in 0..16 {
            let facing = turns as f32 * 0.7;
            let angle = relative_angle(Vec2::new(0.3, -0.9), facing);
            assert!(angle.abs() <= PI + 1.0e-5, "angle {angle}");
        }
    }

    #[test]
    fn an_intent_sanitises_what_a_client_hands_it() {
        let long = Intent::player(Vec2::new(3.0, 4.0), false, false);
        assert!((long.move_world().length() - 1.0).abs() < 1.0e-6);
        let short = Intent::player(Vec2::new(0.3, 0.0), true, true);
        assert!((short.move_world().length() - 0.3).abs() < 1.0e-6);
        assert!(short.attack());
        assert!(short.dodge());
        assert!(!short.face_foe());
        let hostile = Intent::player(Vec2::new(f32::NAN, 1.0), false, false);
        assert_eq!(hostile.move_world(), Vec2::ZERO);
        let idle = Intent::idle();
        assert_eq!(idle.move_world(), Vec2::ZERO);
        assert!(!idle.attack());
        let adversary = Intent::adversary(Vec2::new(0.0, -2.0), true);
        assert!(adversary.face_foe());
        assert!(adversary.attack());
        assert!(!adversary.dodge());
        assert!((adversary.move_world().length() - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn a_dodge_cooldown_covers_the_dodge_and_the_pause_after_it() {
        let dodge = match (AuthoredDodge {
            duration_seconds: 0.30,
            cooldown_seconds: 0.25,
            distance: 2.2,
        })
        .compile()
        {
            Ok(spec) => spec,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(dodge.duration() + dodge.cooldown(), 66);
    }
}
