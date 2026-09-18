//! Analytical two-bone inverse kinematics.
//!
//! Closed form, by the law of cosines. No iteration, no solver state, no
//! convergence threshold: the same inputs give the same answer on the streaming
//! thread, in a headless test, and in a probe.
//!
//! The only case that needs care is an unreachable target. Rather than
//! iterating toward something impossible or producing a `NaN`, the solver
//! clamps the distance into the range the two bones can actually span and
//! reports that it did.

use glam::{Quat, Vec3};

/// The result of one two-bone solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBoneSolution {
    /// Unit direction the upper bone must point, in the space the inputs were
    /// given in.
    pub upper_direction: Vec3,
    /// Flexion of the middle joint, in radians. Never negative, so the joint
    /// cannot hyperextend.
    pub joint_flex: f32,
    /// Whether the target was reachable without clamping.
    pub reached: bool,
}

/// Solves a two-bone chain.
///
/// `origin` is the root joint, `target` is where the chain's tip must land,
/// and `pole` is the direction the middle joint should bend toward. Lengths
/// are in the same units as the positions.
#[must_use]
pub fn solve_two_bone(
    origin: Vec3,
    target: Vec3,
    upper_length: f32,
    lower_length: f32,
    pole: Vec3,
) -> TwoBoneSolution {
    // This API is public and is also the last numerical boundary before a
    // pose reaches a matrix.  Do not let a hostile length turn `inf - inf`
    // into a NaN later in the cosine law.
    let length = |value: f32| {
        if value.is_finite() && value > 1.0e-4 {
            value
        } else {
            1.0e-4
        }
    };
    let upper = length(upper_length);
    let lower = length(lower_length);
    let fallback = Vec3::NEG_Y;

    let to_target = target - origin;
    let distance_raw = to_target.length();
    let direction = if distance_raw > 1.0e-5 && to_target.is_finite() {
        to_target / distance_raw
    } else {
        fallback
    };
    if !direction.is_finite() {
        return TwoBoneSolution {
            upper_direction: fallback,
            joint_flex: 0.0,
            reached: false,
        };
    }

    // Keep the reachable interval strictly inside the two singular endpoints,
    // but scale that margin down for the deliberately supported minimum-size
    // chain. A fixed `1e-4` margin makes two `1e-4` bones invert min/max and
    // turns `clamp` into a panic.
    let reach_margin = (upper.min(lower) * 0.25).min(1.0e-4);
    let minimum = (upper - lower).abs() + reach_margin;
    let maximum = upper + lower - reach_margin;
    let distance = if distance_raw.is_finite() {
        distance_raw.clamp(minimum, maximum)
    } else {
        maximum
    };
    let reached = distance_raw.is_finite()
        && (minimum..=maximum).contains(&distance_raw)
        && distance_raw > 1.0e-5;

    // Interior angle at the middle joint, then the flexion away from straight.
    let cos_joint = ((upper * upper + lower * lower - distance * distance) / (2.0 * upper * lower))
        .clamp(-1.0, 1.0);
    let joint_flex = std::f32::consts::PI - cos_joint.acos();

    // Angle between the upper bone and the origin-to-target line.
    let cos_offset = ((upper * upper + distance * distance - lower * lower)
        / (2.0 * upper * distance))
        .clamp(-1.0, 1.0);
    let offset = cos_offset.acos();

    // Rotate the aim direction toward the pole by that offset; the axis is
    // perpendicular to the bend plane, so the middle joint ends up on the
    // pole's side of the line.
    let pole = if pole.is_finite() && pole.length_squared() > 1.0e-8 {
        pole
    } else {
        Vec3::NEG_Z
    };
    let mut axis = direction.cross(pole);
    if axis.length_squared() < 1.0e-8 {
        // The pole is parallel to the aim; any perpendicular axis will do and
        // a fixed one keeps the result deterministic.
        axis = direction.cross(Vec3::X);
        if axis.length_squared() < 1.0e-8 {
            axis = direction.cross(Vec3::Z);
        }
    }
    let upper_direction = if axis.length_squared() < 1.0e-8 {
        direction
    } else {
        Quat::from_axis_angle(axis.normalize(), offset) * direction
    };

    let upper_direction =
        if upper_direction.is_finite() && upper_direction.length_squared() > 1.0e-6 {
            upper_direction.normalize()
        } else {
            fallback
        };

    TwoBoneSolution {
        upper_direction,
        joint_flex: if joint_flex.is_finite() {
            joint_flex.max(0.0)
        } else {
            0.0
        },
        reached,
    }
}

/// Limits a rotation's angle without changing its axis.
///
/// A non-finite rotation or a non-positive/non-finite limit is invalid input
/// and resolves to identity rather than propagating an invalid transform.
///
/// Used where a joint range has to hold after a rotation was built from a
/// direction rather than from an angle.
#[must_use]
pub fn clamp_rotation_angle(rotation: Quat, maximum: f32) -> Quat {
    if !rotation.is_finite() || !maximum.is_finite() || maximum <= 0.0 {
        return Quat::IDENTITY;
    }
    let angle = 2.0 * rotation.w.clamp(-1.0, 1.0).abs().acos();
    if !angle.is_finite() {
        return Quat::IDENTITY;
    }
    if angle <= maximum {
        return rotation.normalize();
    }
    Quat::IDENTITY.slerp(rotation, maximum / angle)
}

/// Reconstructs the tip of a solved chain, for tests and diagnostics.
#[must_use]
pub fn chain_tip(
    origin: Vec3,
    solution: TwoBoneSolution,
    upper_length: f32,
    lower_length: f32,
    pole: Vec3,
) -> Vec3 {
    let origin = if origin.is_finite() {
        origin
    } else {
        Vec3::ZERO
    };
    let upper_length = if upper_length.is_finite() {
        upper_length.max(0.0)
    } else {
        0.0
    };
    let lower_length = if lower_length.is_finite() {
        lower_length.max(0.0)
    } else {
        0.0
    };
    let upper_direction = if solution.upper_direction.is_finite()
        && solution.upper_direction.length_squared() > 1.0e-8
    {
        solution.upper_direction.normalize()
    } else {
        Vec3::NEG_Y
    };
    let pole = if pole.is_finite() && pole.length_squared() > 1.0e-8 {
        pole
    } else {
        Vec3::NEG_Z
    };
    let knee = origin + upper_direction * upper_length;
    // The lower bone continues from the upper one, rotated by the flexion in
    // the same plane the solver used.
    let mut axis = upper_direction.cross(pole);
    if axis.length_squared() < 1.0e-8 {
        axis = upper_direction.cross(Vec3::X);
        if axis.length_squared() < 1.0e-8 {
            axis = upper_direction.cross(Vec3::Z);
        }
    }
    let lower_direction = if axis.length_squared() < 1.0e-8 {
        upper_direction
    } else {
        Quat::from_axis_angle(
            axis.normalize(),
            -if solution.joint_flex.is_finite() {
                solution.joint_flex.max(0.0)
            } else {
                0.0
            },
        ) * upper_direction
    };
    knee + lower_direction * lower_length
}

#[cfg(test)]
mod tests {
    use super::{chain_tip, clamp_rotation_angle, solve_two_bone};
    use glam::{Quat, Vec3};

    const UPPER: f32 = 5.0;
    const LOWER: f32 = 6.0;
    const POLE: Vec3 = Vec3::NEG_Z;

    #[test]
    fn a_reachable_target_is_reached() {
        let origin = Vec3::new(0.0, 14.0, 0.0);
        for target in [
            Vec3::new(0.0, 4.0, 0.0),
            Vec3::new(0.0, 6.0, -2.0),
            Vec3::new(1.0, 5.5, 2.0),
            Vec3::new(-2.0, 7.0, -3.0),
        ] {
            let solution = solve_two_bone(origin, target, UPPER, LOWER, POLE);
            assert!(solution.reached, "target {target} reported unreachable");
            let tip = chain_tip(origin, solution, UPPER, LOWER, POLE);
            assert!(
                (tip - target).length() < 1.0e-3,
                "target {target} solved to {tip}"
            );
            assert!(solution.joint_flex >= 0.0);
            assert!(solution.joint_flex.is_finite());
        }
    }

    #[test]
    fn an_unreachable_target_extends_the_chain_without_a_nan() {
        let origin = Vec3::ZERO;
        let far = Vec3::new(0.0, -40.0, 0.0);
        let solution = solve_two_bone(origin, far, UPPER, LOWER, POLE);
        assert!(!solution.reached);
        assert!(solution.upper_direction.is_finite());
        assert!(solution.joint_flex.is_finite());
        assert!(
            solution.joint_flex < 0.05,
            "an over-extended chain should be nearly straight, not {}",
            solution.joint_flex
        );
        let tip = chain_tip(origin, solution, UPPER, LOWER, POLE);
        assert!(tip.is_finite());
        assert!(tip.y < 0.0, "the chain points at the target");
    }

    #[test]
    fn a_target_inside_the_dead_zone_folds_without_a_nan() {
        let origin = Vec3::ZERO;
        let near = Vec3::new(0.0, -0.2, 0.0);
        let solution = solve_two_bone(origin, near, UPPER, LOWER, POLE);
        assert!(!solution.reached);
        assert!(solution.upper_direction.is_finite());
        assert!(solution.joint_flex.is_finite());
        assert!(solution.joint_flex > 1.0, "the chain did not fold");
    }

    #[test]
    fn a_degenerate_or_hostile_input_never_propagates_a_nan() {
        let origin = Vec3::ZERO;
        for target in [
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 0.0),
            Vec3::new(f32::INFINITY, -1.0, 0.0),
        ] {
            let solution = solve_two_bone(origin, target, UPPER, LOWER, POLE);
            assert!(solution.upper_direction.is_finite(), "{target}");
            assert!(solution.joint_flex.is_finite(), "{target}");
        }
        for (upper, lower, pole) in [
            (0.0, 0.0, POLE),
            (-1.0, 2.0, Vec3::ZERO),
            (f32::NAN, 2.0, Vec3::new(f32::NAN, 0.0, 0.0)),
            (
                f32::INFINITY,
                f32::NEG_INFINITY,
                Vec3::new(f32::INFINITY, 0.0, 0.0),
            ),
        ] {
            let solution = solve_two_bone(origin, Vec3::new(0.0, -3.0, 0.0), upper, lower, pole);
            assert!(
                solution.upper_direction.is_finite(),
                "{upper} {lower} {pole}"
            );
            assert!(solution.joint_flex.is_finite(), "{upper} {lower} {pole}");
            assert!(chain_tip(Vec3::NAN, solution, upper, lower, pole).is_finite());
        }
    }

    #[test]
    fn the_middle_joint_bends_toward_the_pole() {
        let origin = Vec3::new(0.0, 14.0, 0.0);
        let target = Vec3::new(0.0, 6.0, 0.0);
        let solution = solve_two_bone(origin, target, UPPER, LOWER, POLE);
        let knee = origin + solution.upper_direction * UPPER;
        assert!(
            knee.z < origin.z - 0.05,
            "the knee bent to {} instead of forward",
            knee.z
        );
        // And the same chain with the opposite pole bends the other way.
        let back = solve_two_bone(origin, target, UPPER, LOWER, Vec3::Z);
        let back_knee = origin + back.upper_direction * UPPER;
        assert!(back_knee.z > origin.z + 0.05);
    }

    #[test]
    fn the_joint_never_hyperextends_over_a_sweep_of_targets() {
        let origin = Vec3::new(0.0, 14.0, 0.0);
        let mut y = 14.0_f32 - (UPPER + LOWER);
        while y < 14.0 {
            for z in [-4.0_f32, -1.0, 0.0, 2.0] {
                let solution = solve_two_bone(origin, Vec3::new(0.0, y, z), UPPER, LOWER, POLE);
                assert!(
                    solution.joint_flex >= 0.0,
                    "flexion went negative at y {y} z {z}"
                );
                assert!(
                    solution.joint_flex <= std::f32::consts::PI,
                    "flexion exceeded a fold at y {y} z {z}"
                );
            }
            y += 0.13;
        }
    }

    #[test]
    fn clamping_a_rotation_limits_its_angle_and_leaves_small_ones_alone() {
        let small = Quat::from_rotation_x(0.2);
        assert_eq!(clamp_rotation_angle(small, 0.5), small);
        let large = Quat::from_rotation_x(1.2);
        let clamped = clamp_rotation_angle(large, 0.5);
        let angle = 2.0 * clamped.w.clamp(-1.0, 1.0).abs().acos();
        assert!((angle - 0.5).abs() < 1.0e-3, "clamped to {angle}");
        assert_eq!(clamp_rotation_angle(large, 0.0), Quat::IDENTITY);
        assert_eq!(
            clamp_rotation_angle(Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0), 0.5),
            Quat::IDENTITY
        );
    }
}
