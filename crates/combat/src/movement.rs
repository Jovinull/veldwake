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
//! none of them can put a body outside the arena, off the ground, or over a step
//! it could not have walked up.

use glam::Vec2;

use veldwake_character::{CharacterState, GroundSampler};

use crate::spec::{ArenaSpec, MovementSpec};

/// Everything a move has to satisfy.
pub struct MoveRules<'a> {
    pub movement: &'a MovementSpec,
    pub arena: &'a ArenaSpec,
    pub ground: Option<&'a dyn GroundSampler>,
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

/// Whether a body standing at `base_height` may stand at `target`.
#[must_use]
pub fn accepts(rules: &MoveRules<'_>, base_height: f32, target: Vec2) -> bool {
    if !target.is_finite() || !base_height.is_finite() {
        return false;
    }
    if !rules.arena.contains(target) {
        return false;
    }
    let Some(ground) = rules.ground else {
        // With no sampler there is no terrain to disagree with, which is the
        // case the pure-rules tests and the diagnostic corridor run in.
        return true;
    };
    match ground.surface(f64::from(target.x), f64::from(target.y)) {
        // Absence stays absence: a body may not walk off the edge of a finite
        // region into a floor that was never generated.
        None => false,
        Some(height) => {
            let height = height as f32;
            height - base_height <= rules.movement.max_step_up()
                && base_height - height <= rules.movement.max_drop()
        }
    }
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
    let base = state.base_height;
    if accepts(rules, base, from + delta) {
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
        if accepts(rules, base, from + candidate) {
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
        MoveResult, MoveRules, accepts, facing_of, overlap, separate, try_move, turn_toward,
        wrap_angle,
    };
    use crate::spec::{ArenaSpec, AuthoredMovement, MovementSpec};
    use glam::Vec2;
    use std::f32::consts::{FRAC_PI_2, PI};
    use veldwake_character::ground::{BoundedGround, FlatGround, StepGround};
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
            arena: &arena,
            ground: Some(&ground),
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
            arena: &arena,
            ground: None,
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
            arena: &arena,
            ground: Some(&low),
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
            arena: &arena,
            ground: Some(&high),
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
            arena: &arena,
            ground: Some(&cliff),
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
            arena: &arena,
            ground: Some(&wall),
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
            arena: &arena,
            ground: Some(&bounded),
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
            arena: &arena,
            ground: Some(&ground),
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
        assert!(!accepts(&rules, 0.0, Vec2::new(4.0, 0.0)));
        assert!(accepts(&rules, 0.0, Vec2::new(1.0, 1.0)));
        assert!(!accepts(&rules, f32::NAN, Vec2::ZERO));
        assert!(!accepts(&rules, 0.0, Vec2::splat(f32::NAN)));
    }

    #[test]
    fn with_no_sampler_only_the_arena_constrains_a_move() {
        let movement = movement();
        let arena = arena(2.0);
        let rules = MoveRules {
            movement: &movement,
            arena: &arena,
            ground: None,
        };
        assert!(accepts(&rules, 0.0, Vec2::new(1.0, 0.0)));
        assert!(!accepts(&rules, 0.0, Vec2::new(3.0, 0.0)));
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
            arena: &arena,
            ground: Some(&ground),
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
            arena: &arena,
            ground: Some(&ground),
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
            arena: &arena,
            ground: Some(&ground),
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
            arena: &arena,
            ground: Some(&wall),
        };
        // Pushing `second` toward +x would climb a six-unit wall.
        let mut first = state_at(0.2, 0.0, 0.0);
        let mut second = state_at(0.6, 0.0, 0.0);
        separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!(second.x < 1.0, "second climbed the wall to {}", second.x);
        for state in [&first, &second] {
            assert!(accepts(
                &rules,
                state.base_height,
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
            arena: &arena,
            ground: Some(&ground),
        };
        let mut first = state_at(0.0, 0.0, 0.0);
        let mut second = state_at(0.01, 0.0, 0.0);
        let resolved = separate(&mut first, &mut second, (0.5, 0.5), &rules);
        assert!(resolved < 0.2, "nothing should have moved far: {resolved}");
        for state in [&first, &second] {
            assert!(
                accepts(&rules, state.base_height, Vec2::new(state.x, state.z)),
                "a blocked separation must not place a body illegally"
            );
        }
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
