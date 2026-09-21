//! Kinematic movement against the ground a viewer can see.
//!
//! No physics engine, and Rapier specifically stays rejected. The contact
//! problem here is "which block is under this foot", which `GroundSampler`
//! answers exactly and `INVARIANTS.md` CHAR-002 requires to be exact; a solver
//! would answer it approximately and add a second source of truth for where a
//! body is. Knockback is a bounded displacement, not a response to force, so the
//! review trigger ADR-0004 set for a physics engine has not fired. ADR-0005
//! records the whole argument.
//!
//! Planar vectors here are `(x, z)` in world space, because `y` is decided by the
//! ground rather than by intent.
//!
//! The one rule everything else is built from: a proposed move is accepted only
//! if its destination is somewhere a body may be. Sliding, knockback, the dodge
//! and the push-out between two bodies all go through that same acceptance, so
//! none of them can put a body outside its bounds, off the ground, or over a step
//! it could not have walked up.
//!
//! # Exact support height, never the pelvis
//!
//! A step is judged between the **exact support surface under the body** and the
//! **exact support surface at the destination**, both from
//! [`GroundSampler::surface`]. It is deliberately *not* judged against
//! [`CharacterState::base_height`], which `veldwake-character` documents as the
//! **smoothed** height the pelvis follows: the feet snap to the exact visible
//! block top, the pelvis lags behind it, and leg IK absorbs the difference.
//!
//! That filter is presentation. Comparing against it would have made movement
//! authority depend on how far behind the pelvis happened to be, which is a
//! function of how long the body had been climbing — so the same step would be
//! legal standing still and illegal while walking, and the whole rule would be
//! history dependent for no reason anyone chose. M6 never saw it because its
//! arena is asserted exactly level; the first terraced walk would have.
//!
//! # One rule, one place, one reason
//!
//! [`check_move`] is the only implementation of the rule and it returns *why* a
//! move was refused. [`accepts`] is a wrapper over it, `try_move` uses it, and
//! an offline reachability audit uses it too — so an audit can attribute a
//! barrier to a cause without owning a second copy of the conditions.

use glam::Vec2;

use veldwake_character::{CharacterState, GroundSampler};

use crate::spec::{ArenaSpec, MovementSpec};

/// Whether a body may occupy a column at all, for reasons that are not height.
///
/// This is a **veto and never a height**. `GroundSampler` answers "what is the
/// visible solid surface here", including inside a river, where it answers with
/// the bed — and that stays exactly right for feet, IK and the pelvis. Whether a
/// body may *walk* there is a different question, and mixing the two would turn
/// a contact model into a rules model.
///
/// It is consulted for the **destination** of a move, not the source: a body
/// that somehow stands somewhere forbidden must still be able to walk out.
pub trait TraversalLegality {
    /// Whether a body may occupy the column containing `(x, z)`.
    fn walkable(&self, x: f64, z: f64) -> bool;
}

impl<T: TraversalLegality + ?Sized> TraversalLegality for &T {
    fn walkable(&self, x: f64, z: f64) -> bool {
        (**self).walkable(x, z)
    }
}

/// Why a proposed move was refused.
///
/// Ordered as [`check_move`] tests them, and that order is part of the contract
/// because an audit reports the *first* reason a move failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveBlockReason {
    /// A position, or a surface height, was not a finite number.
    NonFinite,
    /// An arena constraint was present and the destination is outside it.
    Arena,
    /// A traversal veto refused the destination column. Water is one.
    Traversal,
    /// The source or the destination column has no ground at all — outside a
    /// finite region, for instance. Absence stays absence.
    MissingGround,
    /// The destination stands more than `max_step_up` above the source.
    StepUp,
    /// The destination lies more than `max_drop` below the source.
    Drop,
}

impl MoveBlockReason {
    /// A short stable name, for reports and counters.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NonFinite => "non-finite",
            Self::Arena => "arena",
            Self::Traversal => "traversal",
            Self::MissingGround => "missing-ground",
            Self::StepUp => "step-up",
            Self::Drop => "drop",
        }
    }

    /// Every reason, in the order [`check_move`] tests them.
    pub const ALL: [Self; 6] = [
        Self::NonFinite,
        Self::Arena,
        Self::Traversal,
        Self::MissingGround,
        Self::StepUp,
        Self::Drop,
    ];
}

/// Everything a move has to satisfy.
///
/// `arena` is optional because a fight in a disc and a walk across a region are
/// the same rules with different bounds. With no arena the bounds are whatever
/// the ground and the traversal veto say, which for a finite region is the
/// region itself.
pub struct MoveRules<'a> {
    pub movement: &'a MovementSpec,
    pub arena: Option<&'a ArenaSpec>,
    pub ground: Option<&'a dyn GroundSampler>,
    pub legality: Option<&'a dyn TraversalLegality>,
}

/// What a proposed move actually achieved.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveResult {
    /// The displacement that was applied, which may be shorter than the one
    /// proposed, or zero.
    pub realized: Vec2,
    /// Whether the full proposal was refused.
    pub blocked: bool,
    /// Whether the move was resolved one axis at a time.
    pub slid: bool,
}

impl MoveResult {
    #[must_use]
    pub fn distance(&self) -> f32 {
        self.realized.length()
    }
}

/// Whether a body standing at `from` may stand at `target`, and why not.
///
/// The single implementation of the rule. The order of the checks is part of
/// the contract, because the first failure is what an audit attributes a
/// barrier to:
///
/// 1. both positions are finite;
/// 2. the arena, when there is one, contains the destination;
/// 3. the traversal veto, when there is one, allows the destination column;
/// 4. the source column has ground;
/// 5. the destination column has ground;
/// 6. the exact support heights are within `max_step_up` and `max_drop`.
///
/// Both heights come from [`GroundSampler::surface`] at the two positions.
/// Neither is the pelvis; see the module documentation for why that matters.
pub fn check_move(rules: &MoveRules<'_>, from: Vec2, target: Vec2) -> Result<(), MoveBlockReason> {
    if !from.is_finite() || !target.is_finite() {
        return Err(MoveBlockReason::NonFinite);
    }
    if let Some(arena) = rules.arena
        && !arena.contains(target)
    {
        return Err(MoveBlockReason::Arena);
    }
    if let Some(legality) = rules.legality
        && !legality.walkable(f64::from(target.x), f64::from(target.y))
    {
        return Err(MoveBlockReason::Traversal);
    }
    let Some(ground) = rules.ground else {
        // With no sampler there is no terrain to disagree with, which is the
        // case the pure-rules tests and the diagnostic corridor run in.
        return Ok(());
    };
    // Absence stays absence: a body may not walk off the edge of a finite
    // region into a floor that was never generated.
    let Some(source) = ground.surface(f64::from(from.x), f64::from(from.y)) else {
        return Err(MoveBlockReason::MissingGround);
    };
    let Some(destination) = ground.surface(f64::from(target.x), f64::from(target.y)) else {
        return Err(MoveBlockReason::MissingGround);
    };
    let source = source as f32;
    let destination = destination as f32;
    if !source.is_finite() || !destination.is_finite() {
        return Err(MoveBlockReason::NonFinite);
    }
    if destination - source > rules.movement.max_step_up() {
        return Err(MoveBlockReason::StepUp);
    }
    if source - destination > rules.movement.max_drop() {
        return Err(MoveBlockReason::Drop);
    }
    Ok(())
}

/// Whether a body standing at `from` may stand at `target`.
///
/// A wrapper over [`check_move`], kept because most callers only need the
/// verdict and reading `check_move(..).is_ok()` at every call site would say
/// less than this name does.
#[must_use]
pub fn accepts(rules: &MoveRules<'_>, from: Vec2, target: Vec2) -> bool {
    check_move(rules, from, target).is_ok()
}

/// Moves a body by a planar displacement, as far as the rules allow.
///
/// Tries the whole displacement, then each axis alone, so a body slides along
/// what stopped it instead of sticking to it.
pub fn try_move(state: &mut CharacterState, delta: Vec2, rules: &MoveRules<'_>) -> MoveResult {
    if !delta.is_finite() || delta.length_squared() <= 0.0 {
        return MoveResult::default();
    }
    let from = Vec2::new(state.x, state.z);
    if accepts(rules, from, from + delta) {
        state.x = from.x + delta.x;
        state.z = from.y + delta.y;
        return MoveResult {
            realized: delta,
            blocked: false,
            slid: false,
        };
    }
    for candidate in [Vec2::new(delta.x, 0.0), Vec2::new(0.0, delta.y)] {
        if candidate.length_squared() <= 0.0 {
            continue;
        }
        if accepts(rules, from, from + candidate) {
            state.x = from.x + candidate.x;
            state.z = from.y + candidate.y;
            return MoveResult {
                realized: candidate,
                blocked: false,
                slid: true,
            };
        }
    }
    MoveResult {
        realized: Vec2::ZERO,
        blocked: true,
        slid: false,
    }
}

/// How much overlap two bodies have, in world units.
#[must_use]
pub fn overlap(first: Vec2, second: Vec2, first_radius: f32, second_radius: f32) -> f32 {
    let reach = first_radius + second_radius;
    let distance = (second - first).length();
    if distance >= reach {
        0.0
    } else {
        reach - distance
    }
}

/// Pushes two overlapping bodies apart, through the ordinary movement rules.
///
/// Returns how much of the overlap was resolved. Each body takes half, and if
/// one of them is blocked its share is offered to the other — so a body pinned
/// against the arena edge does not trap the other inside it. Two bodies that are
/// both blocked stay overlapped, which is honest: the alternative is putting one
/// of them somewhere it may not be.
pub fn separate(
    first: &mut CharacterState,
    second: &mut CharacterState,
    radii: (f32, f32),
    rules: &MoveRules<'_>,
) -> f32 {
    let (first_radius, second_radius) = radii;
    let a = Vec2::new(first.x, first.z);
    let b = Vec2::new(second.x, second.z);
    let overlap = overlap(a, b, first_radius, second_radius);
    if overlap <= 0.0 {
        return 0.0;
    }
    let offset = b - a;
    let length = offset.length();
    // Coincident bodies have no direction to separate along, so one is chosen
    // deterministically rather than left to a division by zero.
    let direction = if length > 1.0e-5 {
        offset / length
    } else {
        Vec2::new(1.0, 0.0)
    };

    let half = overlap * 0.5;
    let first_moved = try_move(first, -direction * half, rules)
        .realized
        .dot(-direction)
        .max(0.0);
    let second_moved = try_move(second, direction * half, rules)
        .realized
        .dot(direction)
        .max(0.0);
    let mut resolved = first_moved + second_moved;

    // Whatever one of them could not do, offer to the other.
    let remaining = overlap - resolved;
    if remaining > 1.0e-5 {
        let extra = try_move(second, direction * remaining, rules)
            .realized
            .dot(direction)
            .max(0.0);
        resolved += extra;
        let still = overlap - resolved;
        if still > 1.0e-5 {
            resolved += try_move(first, -direction * still, rules)
                .realized
                .dot(-direction)
                .max(0.0);
        }
    }
    resolved.min(overlap)
}

/// Turns a facing toward a target, by at most `max_delta` radians.
///
/// Takes the shorter way round, so a body never spins the long way to face
/// something just behind it.
#[must_use]
pub fn turn_toward(current: f32, target: f32, max_delta: f32) -> f32 {
    if !current.is_finite() || !target.is_finite() || !max_delta.is_finite() {
        return if current.is_finite() { current } else { 0.0 };
    }
    let tau = std::f32::consts::TAU;
    let difference =
        (target - current + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    if difference.abs() <= max_delta {
        return wrap_angle(target);
    }
    wrap_angle(current + difference.signum() * max_delta)
}

/// Wraps an angle into `[-pi, pi)`.
///
/// Half open at the top, which is what `rem_euclid` gives: half a turn either way
/// comes back as `-pi`. The only thing that matters downstream is that the value
/// never grows without bound.
#[must_use]
pub fn wrap_angle(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

/// The facing that looks along a planar direction, in the character's own
/// convention: yaw zero faces `-Z` and positive yaw turns toward `+X`.
#[must_use]
pub fn facing_of(direction: Vec2) -> Option<f32> {
    if !direction.is_finite() || direction.length_squared() <= 1.0e-12 {
        return None;
    }
    Some(direction.x.atan2(-direction.y))
}

#[cfg(test)]
mod tests {
    use super::{
        MoveBlockReason, MoveResult, MoveRules, TraversalLegality, accepts, check_move, facing_of,
        overlap, separate, try_move, turn_toward, wrap_angle,
    };
    use crate::spec::{ArenaSpec, AuthoredMovement, MovementSpec};
    use glam::Vec2;
    use std::f32::consts::{FRAC_PI_2, PI};
    use veldwake_character::ground::{BoundedGround, FlatGround, StepGround, SteppedRamp};
    use veldwake_character::{CharacterState, GroundSampler};

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

    fn arena(radius: f32) -> ArenaSpec {
        match ArenaSpec::new(Vec2::ZERO, radius) {
            Ok(arena) => arena,
            Err(error) => panic!("{error}"),
        }
    }

    fn state_at(x: f32, z: f32, base: f32) -> CharacterState {
        CharacterState {
            x,
            z,
            base_height: base,
            grounded: true,
            ..CharacterState::default()
        }
    }

    #[test]
    fn an_ordinary_move_is_applied_whole() {
        let movement = movement();
        let arena = arena(10.0);
        let ground = FlatGround::at(4.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        let mut state = state_at(0.0, 0.0, 4.0);
        let result = try_move(&mut state, Vec2::new(0.5, -0.25), &rules);
        assert!(!result.blocked);
        assert!(!result.slid);
        assert!((state.x - 0.5).abs() < 1.0e-6);
        assert!((state.z + 0.25).abs() < 1.0e-6);
        assert!((result.distance() - Vec2::new(0.5, -0.25).length()).abs() < 1.0e-6);
    }

    #[test]
    fn a_zero_or_hostile_move_changes_nothing() {
        let movement = movement();
        let arena = arena(10.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: None,
            legality: None,
        };
        let mut state = state_at(1.0, 2.0, 0.0);
        for delta in [
            Vec2::ZERO,
            Vec2::splat(f32::NAN),
            Vec2::new(f32::INFINITY, 0.0),
        ] {
            let before = (state.x, state.z);
            let result = try_move(&mut state, delta, &rules);
            assert_eq!(result, MoveResult::default());
            assert_eq!((state.x, state.z), before);
        }
    }

    #[test]
    fn a_step_up_inside_the_bound_is_walkable_and_one_above_it_is_not() {
        let movement = movement();
        let arena = arena(10.0);
        // A step at x = 1 rising exactly one world unit, which is the bound.
        let low = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 5.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&low),
            legality: None,
        };
        let mut state = state_at(0.5, 0.0, 4.0);
        assert!(!try_move(&mut state, Vec2::new(1.0, 0.0), &rules).blocked);
        assert!(state.x > 1.0, "a one-voxel step is walkable");

        let high = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 5.5,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&high),
            legality: None,
        };
        let mut state = state_at(0.5, 0.0, 4.0);
        let result = try_move(&mut state, Vec2::new(1.0, 0.0), &rules);
        assert!(result.blocked, "a step above the bound must refuse");
        assert!(
            (state.x - 0.5).abs() < 1.0e-6,
            "a refused move does not move"
        );
    }

    #[test]
    fn a_drop_past_the_bound_is_refused() {
        let movement = movement();
        let arena = arena(10.0);
        let cliff = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 0.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&cliff),
            legality: None,
        };
        // Walking from the high side to the low side is a four-unit drop.
        let mut state = state_at(0.5, 0.0, 4.0);
        assert!(try_move(&mut state, Vec2::new(1.0, 0.0), &rules).blocked);
    }

    #[test]
    fn a_blocked_diagonal_slides_along_the_axis_that_is_free() {
        let movement = movement();
        let arena = arena(10.0);
        let wall = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 9.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&wall),
            legality: None,
        };
        let mut state = state_at(0.5, 0.0, 4.0);
        let result = try_move(&mut state, Vec2::new(1.0, 1.0), &rules);
        assert!(!result.blocked);
        assert!(result.slid, "the move must resolve one axis at a time");
        assert!((state.x - 0.5).abs() < 1.0e-6, "x was the blocked axis");
        assert!((state.z - 1.0).abs() < 1.0e-6, "z was free");
    }

    #[test]
    fn absence_of_ground_is_refused_rather_than_treated_as_a_floor() {
        let movement = movement();
        let arena = arena(100.0);
        let inner = FlatGround::at(4.0);
        let bounded = BoundedGround {
            inner,
            min_x: -5.0,
            max_x: 5.0,
            min_z: -5.0,
            max_z: 5.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&bounded),
            legality: None,
        };
        let mut state = state_at(4.5, 0.0, 4.0);
        let result = try_move(&mut state, Vec2::new(1.0, 0.0), &rules);
        assert!(result.blocked, "there is no floor past the region's edge");
        assert!(bounded.surface(6.0, 0.0).is_none());
    }

    #[test]
    fn the_arena_boundary_stops_a_body_wherever_the_ground_is_fine() {
        let movement = movement();
        let arena = arena(3.0);
        let ground = FlatGround::at(0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        // A move whose only component leaves the arena is refused outright:
        // there is no free axis to slide along.
        let mut state = state_at(2.9, 0.0, 0.0);
        let result = try_move(&mut state, Vec2::new(1.0, 0.0), &rules);
        assert!(result.blocked, "there is nothing to slide along here");
        assert!(
            (state.x - 2.9).abs() < 1.0e-6,
            "a refused move does not move"
        );
        // A diagonal one slides along the component that stays inside.
        let mut state = state_at(2.9, 0.0, 0.0);
        let result = try_move(&mut state, Vec2::new(1.0, 0.4), &rules);
        assert!(!result.blocked && result.slid);
        assert!(arena.contains(Vec2::new(state.x, state.z)));
        assert!((state.z - 0.4).abs() < 1.0e-6);
        assert!(!accepts(&rules, Vec2::ZERO, Vec2::new(4.0, 0.0)));
        assert!(accepts(&rules, Vec2::ZERO, Vec2::new(1.0, 1.0)));
        assert!(!accepts(&rules, Vec2::splat(f32::NAN), Vec2::ZERO));
        assert!(!accepts(&rules, Vec2::ZERO, Vec2::splat(f32::NAN)));
    }

    #[test]
    fn with_no_sampler_only_the_arena_constrains_a_move() {
        let movement = movement();
        let arena = arena(2.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: None,
            legality: None,
        };
        assert!(accepts(&rules, Vec2::ZERO, Vec2::new(1.0, 0.0)));
        assert!(!accepts(&rules, Vec2::ZERO, Vec2::new(3.0, 0.0)));
    }

    #[test]
    fn overlap_is_zero_when_two_bodies_are_clear_of_each_other() {
        assert_eq!(overlap(Vec2::ZERO, Vec2::new(2.0, 0.0), 0.5, 0.5), 0.0);
        assert!((overlap(Vec2::ZERO, Vec2::new(0.5, 0.0), 0.5, 0.5) - 0.5).abs() < 1.0e-6);
        assert!((overlap(Vec2::ZERO, Vec2::ZERO, 0.5, 0.5) - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn separation_resolves_the_whole_overlap_on_open_ground() {
        let movement = movement();
        let arena = arena(20.0);
        let ground = FlatGround::at(0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        let mut first = state_at(0.0, 0.0, 0.0);
        let mut second = state_at(0.4, 0.0, 0.0);
        let resolved = separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!((resolved - 0.6).abs() < 1.0e-4, "resolved {resolved}");
        let remaining = overlap(
            Vec2::new(first.x, first.z),
            Vec2::new(second.x, second.z),
            0.5,
            0.5,
        );
        assert!(remaining < 1.0e-3, "still overlapping by {remaining}");
        // Each took half, so the midpoint did not move.
        assert!(((first.x + second.x) * 0.5 - 0.2).abs() < 1.0e-4);
    }

    #[test]
    fn coincident_bodies_separate_deterministically_and_without_a_nan() {
        let movement = movement();
        let arena = arena(20.0);
        let ground = FlatGround::at(0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        let mut first = state_at(1.0, 1.0, 0.0);
        let mut second = state_at(1.0, 1.0, 0.0);
        let resolved = separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!(resolved > 0.9, "resolved {resolved}");
        for value in [first.x, first.z, second.x, second.z] {
            assert!(value.is_finite(), "separation produced {value}");
        }
        // The fallback direction is fixed, so the result is reproducible.
        let mut third = state_at(1.0, 1.0, 0.0);
        let mut fourth = state_at(1.0, 1.0, 0.0);
        let again = separate(&mut third, &mut fourth, (0.5, 0.5), &rules);
        assert!((resolved - again).abs() < 1.0e-6);
        assert!((first.x - third.x).abs() < 1.0e-6);
        assert!((second.x - fourth.x).abs() < 1.0e-6);
    }

    #[test]
    fn a_body_pinned_at_the_arena_edge_gives_its_share_to_the_other() {
        let movement = movement();
        let arena = arena(3.0);
        let ground = FlatGround::at(0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        // `second` sits on the boundary and cannot be pushed further out.
        let mut first = state_at(2.4, 0.0, 0.0);
        let mut second = state_at(2.98, 0.0, 0.0);
        let resolved = separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!(resolved > 0.3, "resolved only {resolved}");
        assert!(arena.contains(Vec2::new(first.x, first.z)));
        assert!(arena.contains(Vec2::new(second.x, second.z)));
        assert!(
            first.x < 2.4,
            "the unpinned body took the remainder, x is {}",
            first.x
        );
    }

    #[test]
    fn separation_never_puts_a_body_over_a_forbidden_step() {
        let movement = movement();
        let arena = arena(20.0);
        let wall = StepGround {
            edge_x: 1.0,
            low: 0.0,
            high: 6.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&wall),
            legality: None,
        };
        // Pushing `second` toward +x would climb a six-unit wall.
        let mut first = state_at(0.2, 0.0, 0.0);
        let mut second = state_at(0.6, 0.0, 0.0);
        separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!(second.x < 1.0, "second climbed the wall to {}", second.x);
        for state in [&first, &second] {
            assert!(accepts(
                &rules,
                Vec2::new(state.x, state.z),
                Vec2::new(state.x, state.z)
            ));
        }
    }

    #[test]
    fn both_bodies_blocked_leaves_an_honest_overlap_rather_than_an_illegal_position() {
        let movement = movement();
        // An arena so small that neither body can move at all.
        let arena = arena(0.05);
        let ground = FlatGround::at(0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        let mut first = state_at(0.0, 0.0, 0.0);
        let mut second = state_at(0.01, 0.0, 0.0);
        let resolved = separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!(resolved < 0.2, "nothing should have moved far: {resolved}");
        for state in [&first, &second] {
            assert!(
                accepts(
                    &rules,
                    Vec2::new(state.x, state.z),
                    Vec2::new(state.x, state.z)
                ),
                "a blocked separation must not place a body illegally"
            );
        }
    }

    /// A ground whose height depends on `x` alone, so a test can prove which
    /// position a rule sampled rather than assuming it.
    #[derive(Clone, Copy, Debug)]
    struct AxisGround {
        /// Height is this multiple of `x`, quantized to whole units.
        slope: f64,
    }

    impl GroundSampler for AxisGround {
        fn surface(&self, x: f64, _z: f64) -> Option<f64> {
            Some((self.slope * x).floor())
        }
    }

    /// A veto that refuses a half-plane and says nothing about height.
    #[derive(Clone, Copy, Debug)]
    struct RefuseBeyond {
        x: f64,
    }

    impl TraversalLegality for RefuseBeyond {
        fn walkable(&self, x: f64, _z: f64) -> bool {
            x < self.x
        }
    }

    #[test]
    fn a_rise_of_exactly_the_bound_is_walkable_and_an_epsilon_more_is_not() {
        let movement = movement();
        let arena = arena(10.0);
        let exact = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 4.0 + f64::from(movement.max_step_up()),
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&exact),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::new(0.5, 0.0), Vec2::new(1.5, 0.0)),
            Ok(()),
            "a rise of exactly max_step_up is the bound, and the bound is inclusive"
        );

        let over = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 4.0 + f64::from(movement.max_step_up()) + 1.0e-4,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&over),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::new(0.5, 0.0), Vec2::new(1.5, 0.0)),
            Err(MoveBlockReason::StepUp)
        );
    }

    #[test]
    fn a_drop_of_exactly_the_bound_is_walkable_and_an_epsilon_more_is_not() {
        let movement = movement();
        let arena = arena(10.0);
        let exact = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 4.0 - f64::from(movement.max_drop()),
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&exact),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::new(0.5, 0.0), Vec2::new(1.5, 0.0)),
            Ok(())
        );

        let over = StepGround {
            edge_x: 1.0,
            low: 4.0,
            high: 4.0 - f64::from(movement.max_drop()) - 1.0e-4,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&over),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::new(0.5, 0.0), Vec2::new(1.5, 0.0)),
            Err(MoveBlockReason::Drop)
        );
    }

    #[test]
    fn a_step_is_judged_from_the_ground_under_the_body_and_never_from_the_pelvis() {
        // The defect this test exists to prevent: `CharacterState::base_height`
        // is the **smoothed** pelvis height, so judging a step against it made
        // the same step legal standing still and illegal while walking, purely
        // because of how long the body had been climbing.
        let movement = movement();
        let arena = arena(64.0);
        let ground = AxisGround { slope: 1.0 };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };

        // Three bodies on identical trajectories: one exactly settled, one with
        // a pelvis half a unit behind the ground, one absurdly far behind.
        // Nothing about the movement may differ.
        let mut settled = state_at(0.0, 0.0, 0.0);
        let mut lagging = state_at(0.0, 0.0, -0.5);
        let mut absurd = state_at(0.0, 0.0, -40.0);
        for _ in 0..600 {
            let delta = Vec2::new(0.028_333, 0.0);
            let a = try_move(&mut settled, delta, &rules);
            let b = try_move(&mut lagging, delta, &rules);
            let c = try_move(&mut absurd, delta, &rules);
            assert_eq!(a.blocked, b.blocked, "a lagging pelvis changed a verdict");
            assert_eq!(a.blocked, c.blocked, "a lagging pelvis changed a verdict");
            assert_eq!(a.realized, b.realized);
            assert_eq!(a.realized, c.realized);
        }
        assert!(
            (settled.x - lagging.x).abs() < 1.0e-6 && (settled.x - absurd.x).abs() < 1.0e-6,
            "identical trajectories diverged: {} {} {}",
            settled.x,
            lagging.x,
            absurd.x
        );
        assert!(settled.x > 0.0, "the walk never started");
    }

    #[test]
    fn a_body_climbs_consecutive_one_voxel_terraces_without_stalling() {
        // A real staircase at a real walking speed. Each tick advances less
        // than a tenth of a voxel, so the body crosses every terrace edge
        // mid-stride rather than from a standstill — which is exactly the case
        // a pelvis-based rule could not take.
        let movement = movement();
        let arena = arena(256.0);
        let ground = SteppedRamp::terrain(0.5, 0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        let per_tick = movement.speed() / 120.0;
        let mut state = state_at(0.0, 0.0, 0.0);
        let mut blocked = 0_u32;
        let mut heights = std::collections::BTreeSet::new();
        for _ in 0..2_400 {
            let result = try_move(&mut state, Vec2::new(per_tick, 0.0), &rules);
            if result.blocked {
                blocked += 1;
            }
            let Some(height) = ground.surface(f64::from(state.x), f64::from(state.z)) else {
                panic!("the stepped ramp answers everywhere");
            };
            heights.insert(height as i64);
        }
        assert_eq!(blocked, 0, "a one-voxel terrace stopped a walking body");
        assert!(
            heights.len() >= 30,
            "the body climbed only {} terraces",
            heights.len()
        );
        let travelled = state.x;
        let expected = per_tick * 2_400.0;
        assert!(
            (travelled - expected).abs() < 1.0e-2,
            "the walk lost ground: {travelled} against {expected}"
        );
    }

    #[test]
    fn a_traversal_veto_refuses_a_destination_and_decides_no_height() {
        let movement = movement();
        let arena = arena(64.0);
        let ground = FlatGround::at(3.0);
        let veto = RefuseBeyond { x: 2.0 };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: Some(&veto),
        };
        assert_eq!(
            check_move(&rules, Vec2::new(1.0, 0.0), Vec2::new(1.5, 0.0)),
            Ok(())
        );
        assert_eq!(
            check_move(&rules, Vec2::new(1.0, 0.0), Vec2::new(2.5, 0.0)),
            Err(MoveBlockReason::Traversal)
        );
        // The veto is asked about the destination only: a body that somehow
        // stands in a forbidden column must still be able to walk out of it.
        assert_eq!(
            check_move(&rules, Vec2::new(9.0, 0.0), Vec2::new(1.0, 0.0)),
            Ok(()),
            "a veto on the source would trap a body for ever"
        );
        // And it never supplies a height: with a step the ground refuses, the
        // refusal is the step's.
        let stepped = StepGround {
            edge_x: 1.2,
            low: 0.0,
            high: 9.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&stepped),
            legality: Some(&veto),
        };
        assert_eq!(
            check_move(&rules, Vec2::new(1.0, 0.0), Vec2::new(1.5, 0.0)),
            Err(MoveBlockReason::StepUp)
        );
    }

    /// A sampler that answers with a number no comparison can order.
    ///
    /// Not a hypothetical: a ground adapter reading a field that has gone
    /// non-finite would hand `check_move` a height whose `>` and `<` are both
    /// false, and a rule written as "refuse when the rise is too large" would
    /// then silently accept every step. The guard is there; this is what asks
    /// whether it still is.
    struct NonFiniteGround {
        at_x: f32,
        value: f64,
    }

    impl GroundSampler for NonFiniteGround {
        fn surface(&self, x: f64, _z: f64) -> Option<f64> {
            if x >= f64::from(self.at_x) {
                Some(self.value)
            } else {
                Some(0.0)
            }
        }
    }

    #[test]
    fn a_move_is_refused_for_every_shape_of_unorderable_number() {
        let movement = movement();
        let flat = FlatGround::at(0.0);

        // Infinity is as non-finite as NaN, on either end of the move.
        let rules = MoveRules {
            movement: &movement,
            arena: None,
            ground: Some(&flat),
            legality: None,
        };
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                check_move(&rules, Vec2::ZERO, Vec2::new(bad, 0.0)),
                Err(MoveBlockReason::NonFinite),
                "a destination of {bad} was not refused"
            );
            assert_eq!(
                check_move(&rules, Vec2::new(0.0, bad), Vec2::ZERO),
                Err(MoveBlockReason::NonFinite),
                "a source of {bad} was not refused"
            );
        }

        // And a ground that answers with one, at the destination or under the
        // body. Either way the answer is a refusal, never an accepted step
        // over a comparison that cannot be made.
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let broken = NonFiniteGround { at_x: 1.0, value };
            let rules = MoveRules {
                movement: &movement,
                arena: None,
                ground: Some(&broken),
                legality: None,
            };
            assert_eq!(
                check_move(&rules, Vec2::ZERO, Vec2::new(1.5, 0.0)),
                Err(MoveBlockReason::NonFinite),
                "a destination height of {value} was not refused"
            );
            assert_eq!(
                check_move(&rules, Vec2::new(1.5, 0.0), Vec2::ZERO),
                Err(MoveBlockReason::NonFinite),
                "a source height of {value} was not refused"
            );
        }
    }

    #[test]
    fn the_arena_bounds_the_destination_and_never_the_body_already_outside_it() {
        // Deliberate, and worth pinning because it reads like an oversight.
        // `check_move` asks whether the place a body is going is inside the
        // arena; it never asks where the body is now. A body that starts or is
        // knocked outside must be able to walk back in, and a rule that tested
        // the source would freeze it there for ever.
        let movement = movement();
        let arena = arena(2.0);
        let flat = FlatGround::at(0.0);
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&flat),
            legality: None,
        };
        let outside = Vec2::new(2.4, 0.0);
        assert!(!arena.contains(outside), "the fixture must start outside");
        assert_eq!(
            check_move(&rules, outside, Vec2::new(1.9, 0.0)),
            Ok(()),
            "a body outside the arena may step back into it"
        );
        // The other half of the same rule, and the part that is a real
        // constraint rather than a kindness: only the destination counts, so a
        // step that merely moves closer while staying outside is refused just
        // like a step further out. A body outside an arena is pinned until one
        // step reaches inside. No path in this crate produces such a body -
        // `try_move`, `separate` and knockback all go through these rules - so
        // this is the shape of the rule, written down, not a live hazard.
        assert_eq!(
            check_move(&rules, outside, Vec2::new(2.3, 0.0)),
            Err(MoveBlockReason::Arena),
            "a destination still outside is refused however much closer it is"
        );
        assert_eq!(
            check_move(&rules, outside, Vec2::new(2.5, 0.0)),
            Err(MoveBlockReason::Arena),
            "and so is one further out"
        );
    }

    #[test]
    fn every_block_reason_is_reachable_and_the_first_that_applies_is_reported() {
        let movement = movement();
        let arena = arena(2.0);
        let ground = StepGround {
            edge_x: 1.0,
            low: 0.0,
            high: 9.0,
        };
        let veto = RefuseBeyond { x: 1.5 };
        let bounded = BoundedGround {
            inner: FlatGround::at(0.0),
            min_x: -1.0,
            max_x: 1.0,
            min_z: -1.0,
            max_z: 1.0,
        };

        // Non-finite beats everything, including an arena that would also refuse.
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: Some(&veto),
        };
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::splat(f32::NAN)),
            Err(MoveBlockReason::NonFinite)
        );
        // Arena beats the veto: the destination is outside both.
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(8.0, 0.0)),
            Err(MoveBlockReason::Arena)
        );
        // Veto beats the step: the destination is past both.
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(1.8, 0.0)),
            Err(MoveBlockReason::Traversal)
        );
        // Step, with nothing else in the way.
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&ground),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(1.2, 0.0)),
            Err(MoveBlockReason::StepUp)
        );
        // Drop.
        let cliff = StepGround {
            edge_x: 1.0,
            low: 0.0,
            high: -9.0,
        };
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&cliff),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(1.2, 0.0)),
            Err(MoveBlockReason::Drop)
        );
        // Missing ground, at the destination and at the source.
        let rules = MoveRules {
            movement: &movement,
            arena: Some(&arena),
            ground: Some(&bounded),
            legality: None,
        };
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(1.5, 0.0)),
            Err(MoveBlockReason::MissingGround)
        );
        assert_eq!(
            check_move(&rules, Vec2::new(1.5, 0.0), Vec2::ZERO),
            Err(MoveBlockReason::MissingGround)
        );

        // Every declared reason has a distinct name, so a report can be read.
        let names: std::collections::BTreeSet<&str> = MoveBlockReason::ALL
            .iter()
            .map(|reason| reason.name())
            .collect();
        assert_eq!(names.len(), MoveBlockReason::ALL.len());
    }

    #[test]
    fn without_an_arena_the_ground_and_the_veto_are_the_whole_bound() {
        let movement = movement();
        let bounded = BoundedGround {
            inner: FlatGround::at(0.0),
            min_x: -4.0,
            max_x: 4.0,
            min_z: -4.0,
            max_z: 4.0,
        };
        let veto = RefuseBeyond { x: 2.0 };
        let rules = MoveRules {
            movement: &movement,
            arena: None,
            ground: Some(&bounded),
            legality: Some(&veto),
        };
        // Far outside every arena the fixtures use, and perfectly legal.
        assert_eq!(
            check_move(&rules, Vec2::new(-3.5, -3.5), Vec2::new(-3.0, -3.0)),
            Ok(())
        );
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(2.5, 0.0)),
            Err(MoveBlockReason::Traversal)
        );
        assert_eq!(
            check_move(&rules, Vec2::ZERO, Vec2::new(0.0, 5.0)),
            Err(MoveBlockReason::MissingGround)
        );
    }

    #[test]
    fn turning_takes_the_shorter_way_and_stops_at_the_target() {
        assert!((turn_toward(0.0, 0.5, 1.0) - 0.5).abs() < 1.0e-6);
        assert!((turn_toward(0.0, 1.0, 0.25) - 0.25).abs() < 1.0e-6);
        // Just past half a turn: the short way is negative.
        let turned = turn_toward(0.0, PI + 0.3, 0.1);
        assert!(turned < 0.0, "turned the long way to {turned}");
        // Wrapping does not make the angle grow without bound.
        let mut facing = 0.0;
        for _ in 0..200 {
            facing = turn_toward(facing, 3.0, 0.1);
            assert!(facing.abs() <= PI + 1.0e-5);
        }
        assert!((facing - 3.0).abs() < 1.0e-5);
        assert_eq!(turn_toward(f32::NAN, 1.0, 0.1), 0.0);
        assert!(turn_toward(0.5, f32::NAN, 0.1).is_finite());
    }

    #[test]
    fn angles_wrap_into_a_half_turn_either_way() {
        assert!((wrap_angle(0.0)).abs() < 1.0e-6);
        // Half a turn either way lands on the closed end of `[-pi, pi)`.
        assert!((wrap_angle(PI * 3.0) + PI).abs() < 1.0e-5);
        assert!((wrap_angle(-PI * 3.0) + PI).abs() < 1.0e-5);
        assert!((wrap_angle(PI * 0.5) - PI * 0.5).abs() < 1.0e-6);
        for turns in -8..8 {
            let angle = wrap_angle(0.7 + turns as f32 * PI * 2.0);
            assert!((angle - 0.7).abs() < 1.0e-4, "{turns} turns gave {angle}");
        }
        assert_eq!(wrap_angle(f32::NAN), 0.0);
    }

    #[test]
    fn a_direction_maps_to_the_facing_the_character_convention_uses() {
        // Yaw zero faces -Z; positive yaw turns toward +X. The same convention
        // `veldwake-character`'s `facing_direction` uses, checked against it.
        match facing_of(Vec2::new(0.0, -1.0)) {
            Some(yaw) => assert!(yaw.abs() < 1.0e-6),
            None => panic!("a unit direction must have a facing"),
        }
        match facing_of(Vec2::new(1.0, 0.0)) {
            Some(yaw) => assert!((yaw - FRAC_PI_2).abs() < 1.0e-6),
            None => panic!("a unit direction must have a facing"),
        }
        assert!(facing_of(Vec2::ZERO).is_none());
        assert!(facing_of(Vec2::splat(f32::NAN)).is_none());
        // Round trip against the character crate's own convention.
        for direction in [
            Vec2::new(0.0, -1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(-0.6, 0.8),
        ] {
            let Some(yaw) = facing_of(direction) else {
                panic!("no facing for {direction}");
            };
            let forward = veldwake_character::pose::facing_direction(yaw);
            let planar = Vec2::new(forward.x, forward.z).normalize();
            let wanted = direction.normalize();
            assert!(
                (planar - wanted).length() < 1.0e-5,
                "{direction} became {planar}"
            );
        }
    }
}
