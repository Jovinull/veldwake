//! Whether a blade touched a body, answered by sweeping rather than by looking.
//!
//! The defect this module exists to avoid is "a hit happened because two boxes
//! overlapped on the frame somebody sampled". A blade tip crosses several
//! character voxels inside a single authoritative tick, so a test that only
//! looks at where the blade *ended up* misses the body it passed through. Both
//! endpoints of the blade are therefore kept from the previous tick and the
//! segment is interpolated between the two poses, with the substep count taken
//! from the **larger** of the two endpoint displacements — which is what makes
//! a rotation about a nearly stationary hand safe, because there the tip moves
//! and the base does not.
//!
//! Everything here is closed form. There is no solver, no iteration count to
//! tune, and no tolerance that decides correctness.

use glam::Vec3;

use veldwake_character::CHARACTER_VOXEL_SIZE;

/// Most substeps one tick's sweep may take.
///
/// A bound rather than a budget: it exists so a pathological pose cannot turn
/// one tick into unbounded work. The high-water mark is reported, and a sweep
/// that needs more than this has a pose problem rather than a sampling problem.
pub const MAX_SWEEP_SUBSTEPS: u32 = 16;

/// How far a blade endpoint may travel between two substeps.
///
/// One character voxel. The blade is two voxels thick, so a step of one voxel
/// cannot pass a body between samples: the swept volumes of adjacent substeps
/// overlap by construction.
pub const SWEEP_MAX_ADVANCE: f32 = CHARACTER_VOXEL_SIZE;

/// A line segment in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub base: Vec3,
    pub tip: Vec3,
}

impl Segment {
    #[must_use]
    pub const fn new(base: Vec3, tip: Vec3) -> Self {
        Self { base, tip }
    }

    #[must_use]
    pub fn length(&self) -> f32 {
        (self.tip - self.base).length()
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.base.is_finite() && self.tip.is_finite()
    }

    /// Linear interpolation of both endpoints toward another segment.
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            base: self.base.lerp(other.base, t),
            tip: self.tip.lerp(other.tip, t),
        }
    }
}

/// An upright-or-otherwise capsule in world space: a segment with a radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub axis: Segment,
    pub radius: f32,
}

impl Capsule {
    #[must_use]
    pub const fn new(axis: Segment, radius: f32) -> Self {
        Self { axis, radius }
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.axis.is_finite() && self.radius.is_finite()
    }

    /// Whether a point is inside the capsule.
    #[must_use]
    pub fn contains(&self, point: Vec3) -> bool {
        let (_, _, squared) = closest_points(self.axis, Segment::new(point, point));
        squared <= self.radius * self.radius
    }
}

/// The closest pair of points between two segments, and the squared distance.
///
/// The standard clamped-parameter solution. Both degenerate cases — one segment
/// of zero length, and both of zero length — are handled explicitly, because a
/// blade at rest and two bodies at the same position are both ordinary inputs
/// here and neither may produce a `NaN`.
#[must_use]
pub fn closest_points(first: Segment, second: Segment) -> (Vec3, Vec3, f32) {
    const EPSILON: f32 = 1.0e-12;
    let d1 = first.tip - first.base;
    let d2 = second.tip - second.base;
    let r = first.base - second.base;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);

    let (s, t) = if a <= EPSILON && e <= EPSILON {
        (0.0, 0.0)
    } else if a <= EPSILON {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= EPSILON {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denominator = a.mul_add(e, -(b * b));
            let mut s = if denominator > EPSILON {
                (b.mul_add(f, -(c * e)) / denominator).clamp(0.0, 1.0)
            } else {
                // Parallel segments: any `s` is as good, so take the start and
                // let the `t` clamp below place the pair.
                0.0
            };
            let mut t = b.mul_add(s, f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };

    let p1 = first.base + d1 * s;
    let p2 = second.base + d2 * t;
    (p1, p2, (p1 - p2).length_squared())
}

/// A blade moving from where it was to where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    pub start: Segment,
    pub end: Segment,
    /// Half-thickness of the blade, in world units.
    pub radius: f32,
}

impl Sweep {
    #[must_use]
    pub const fn new(start: Segment, end: Segment, radius: f32) -> Self {
        Self { start, end, radius }
    }

    /// The larger of the two endpoint displacements.
    ///
    /// The conservative measure: a swing rotates about the hand, so the base
    /// barely moves while the tip crosses a long arc, and taking the base alone
    /// would undersample exactly the fast part.
    #[must_use]
    pub fn advance(&self) -> f32 {
        let base = (self.end.base - self.start.base).length();
        let tip = (self.end.tip - self.start.tip).length();
        base.max(tip)
    }

    /// How many interpolated samples this sweep needs.
    #[must_use]
    pub fn substeps(&self) -> u32 {
        let advance = self.advance();
        if !advance.is_finite() || advance <= SWEEP_MAX_ADVANCE {
            return 1;
        }
        let wanted = (advance / SWEEP_MAX_ADVANCE).ceil();
        if wanted >= f32::from(u16::MAX) {
            return MAX_SWEEP_SUBSTEPS;
        }
        (wanted as u32).clamp(1, MAX_SWEEP_SUBSTEPS)
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.start.is_finite() && self.end.is_finite() && self.radius.is_finite()
    }
}

/// Where and when a sweep first touched a capsule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit {
    /// A point on the struck body's surface, for a visual effect to grow from.
    pub point: Vec3,
    /// Which substep touched, counting from one.
    pub substep: u32,
    /// How many substeps this sweep took, so the high-water mark is observable.
    pub substeps: u32,
}

/// Sweeps a blade against a capsule and reports the first contact.
///
/// The sweep is sampled at substep boundaries including its end, so a blade
/// that is already touching at the start of the tick is caught by the first
/// sample rather than missed.
#[must_use]
pub fn sweep_capsule(sweep: &Sweep, capsule: &Capsule) -> Option<SweepHit> {
    if !sweep.is_finite() || !capsule.is_finite() {
        return None;
    }
    let substeps = sweep.substeps();
    let reach = sweep.radius + capsule.radius;
    let reach_squared = reach * reach;
    for step in 0..=substeps {
        let t = step as f32 / substeps as f32;
        let blade = sweep.start.lerp(&sweep.end, t);
        let (on_blade, on_axis, squared) = closest_points(blade, capsule.axis);
        if squared > reach_squared {
            continue;
        }
        // Grow the effect from the struck surface rather than from the axis
        // inside the body or from the blade outside it.
        let offset = on_blade - on_axis;
        let point = if offset.length_squared() > 1.0e-12 {
            on_axis + offset.normalize() * capsule.radius
        } else {
            on_axis
        };
        return Some(SweepHit {
            point,
            substep: step.max(1),
            substeps,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        Capsule, MAX_SWEEP_SUBSTEPS, SWEEP_MAX_ADVANCE, Segment, Sweep, closest_points,
        sweep_capsule,
    };
    use glam::Vec3;

    fn upright(x: f32, z: f32, radius: f32) -> Capsule {
        Capsule::new(
            Segment::new(Vec3::new(x, 0.5, z), Vec3::new(x, 1.8, z)),
            radius,
        )
    }

    #[test]
    fn crossing_segments_have_the_distance_geometry_says() {
        // Two perpendicular segments a known distance apart on the y axis.
        let a = Segment::new(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let b = Segment::new(Vec3::new(0.0, 0.5, -1.0), Vec3::new(0.0, 0.5, 1.0));
        let (_, _, squared) = closest_points(a, b);
        assert!(
            (squared - 0.25).abs() < 1.0e-6,
            "squared distance {squared}"
        );
    }

    #[test]
    fn parallel_segments_report_their_separation() {
        let a = Segment::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0));
        let b = Segment::new(Vec3::new(0.0, 1.0, 0.0), Vec3::new(2.0, 1.0, 0.0));
        let (_, _, squared) = closest_points(a, b);
        assert!((squared - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn disjoint_collinear_segments_measure_end_to_end() {
        let a = Segment::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let b = Segment::new(Vec3::new(3.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0));
        let (_, _, squared) = closest_points(a, b);
        assert!((squared - 4.0).abs() < 1.0e-6, "squared distance {squared}");
    }

    #[test]
    fn degenerate_segments_never_produce_a_nan() {
        let point = Segment::new(Vec3::ZERO, Vec3::ZERO);
        let other = Segment::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
        let (_, _, squared) = closest_points(point, other);
        assert!(squared.is_finite());
        assert!((squared - 1.0).abs() < 1.0e-6);
        let (_, _, coincident) = closest_points(point, point);
        assert_eq!(coincident, 0.0);
        let line = Segment::new(Vec3::new(-1.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0));
        let (_, _, squared) = closest_points(point, line);
        assert!((squared - 1.0).abs() < 1.0e-6);
        let (_, _, squared) = closest_points(line, point);
        assert!((squared - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn a_translating_blade_hits_a_body_in_its_path() {
        let start = Segment::new(Vec3::new(0.0, 1.2, -2.0), Vec3::new(0.0, 1.2, -1.0));
        let end = Segment::new(Vec3::new(0.0, 1.2, 0.5), Vec3::new(0.0, 1.2, 1.5));
        let sweep = Sweep::new(start, end, 0.08);
        let hit = sweep_capsule(&sweep, &upright(0.0, 0.0, 0.5));
        assert!(hit.is_some(), "a blade driven through a body must connect");
    }

    #[test]
    fn a_blade_that_passes_beside_a_body_does_not_hit_it() {
        let start = Segment::new(Vec3::new(3.0, 1.2, -2.0), Vec3::new(3.0, 1.2, -1.0));
        let end = Segment::new(Vec3::new(3.0, 1.2, 1.0), Vec3::new(3.0, 1.2, 2.0));
        let sweep = Sweep::new(start, end, 0.08);
        assert!(sweep_capsule(&sweep, &upright(0.0, 0.0, 0.5)).is_none());
    }

    #[test]
    fn a_blade_that_passes_above_a_body_does_not_hit_it() {
        let start = Segment::new(Vec3::new(0.0, 3.0, -2.0), Vec3::new(0.0, 3.0, -1.0));
        let end = Segment::new(Vec3::new(0.0, 3.0, 1.0), Vec3::new(0.0, 3.0, 2.0));
        let sweep = Sweep::new(start, end, 0.08);
        assert!(sweep_capsule(&sweep, &upright(0.0, 0.0, 0.5)).is_none());
    }

    #[test]
    fn a_rotation_only_sweep_is_sampled_by_the_tip_not_the_base() {
        // The base barely moves and the tip crosses the body. Sampling by the
        // base alone would take one substep and miss.
        let hand = Vec3::new(0.0, 1.2, -1.4);
        let start = Segment::new(hand, hand + Vec3::new(0.0, 1.3, 0.0));
        let end = Segment::new(hand, hand + Vec3::new(0.0, -0.2, 1.3));
        let sweep = Sweep::new(start, end, 0.08);
        assert!(sweep.substeps() > 4, "the tip's arc must be subdivided");
        assert!(sweep_capsule(&sweep, &upright(0.0, -0.1, 0.5)).is_some());
    }

    #[test]
    fn a_translating_and_rotating_sweep_still_connects() {
        let start = Segment::new(Vec3::new(0.2, 1.4, -2.2), Vec3::new(0.2, 2.4, -2.0));
        let end = Segment::new(Vec3::new(0.1, 1.1, -0.3), Vec3::new(0.1, 0.9, 0.9));
        let sweep = Sweep::new(start, end, 0.08);
        assert!(sweep_capsule(&sweep, &upright(0.0, 0.0, 0.5)).is_some());
    }

    #[test]
    fn a_blade_teleported_through_a_body_is_caught_by_the_sweep() {
        // The anti-tunnelling case, made deliberately extreme: the blade is
        // clear of the body at both ends of the tick and passed through it in
        // between. A per-sample overlap test at the endpoints reports nothing.
        let body = upright(0.0, 0.0, 0.5);
        let start = Segment::new(Vec3::new(0.0, 1.2, -6.0), Vec3::new(0.0, 1.2, -5.0));
        let end = Segment::new(Vec3::new(0.0, 1.2, 5.0), Vec3::new(0.0, 1.2, 6.0));
        let (_, _, at_start) = closest_points(start, body.axis);
        let (_, _, at_end) = closest_points(end, body.axis);
        let reach = 0.08 + body.radius;
        assert!(at_start > reach * reach && at_end > reach * reach);
        let sweep = Sweep::new(start, end, 0.08);
        assert_eq!(
            sweep.substeps(),
            MAX_SWEEP_SUBSTEPS,
            "an 11-unit sweep saturates the bound"
        );
        assert!(
            sweep_capsule(&sweep, &body).is_some(),
            "the sweep must catch what the endpoints miss"
        );
    }

    #[test]
    fn a_blade_already_touching_is_caught_on_the_first_sample() {
        let start = Segment::new(Vec3::new(0.0, 1.2, -0.4), Vec3::new(0.0, 1.2, 0.4));
        let sweep = Sweep::new(start, start, 0.08);
        let hit = sweep_capsule(&sweep, &upright(0.0, 0.0, 0.5));
        match hit {
            Some(hit) => {
                assert_eq!(hit.substep, 1);
                assert_eq!(hit.substeps, 1);
            }
            None => panic!("a stationary blade inside the body must report a hit"),
        }
    }

    #[test]
    fn substep_counts_are_bounded_and_never_zero() {
        let origin = Segment::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(Sweep::new(origin, origin, 0.08).substeps(), 1);
        let tiny = Segment::new(
            Vec3::new(SWEEP_MAX_ADVANCE * 0.5, 0.0, 0.0),
            Vec3::new(SWEEP_MAX_ADVANCE * 0.5, 1.0, 0.0),
        );
        assert_eq!(Sweep::new(origin, tiny, 0.08).substeps(), 1);
        let far = Segment::new(Vec3::new(1000.0, 0.0, 0.0), Vec3::new(1000.0, 1.0, 0.0));
        assert_eq!(Sweep::new(origin, far, 0.08).substeps(), MAX_SWEEP_SUBSTEPS);
    }

    #[test]
    fn a_non_finite_sweep_or_capsule_reports_no_hit_instead_of_a_nan() {
        let good = Segment::new(Vec3::new(0.0, 1.2, -0.4), Vec3::new(0.0, 1.2, 0.4));
        let bad = Segment::new(Vec3::new(f32::NAN, 1.2, 0.0), Vec3::new(0.0, 1.2, 0.4));
        assert!(sweep_capsule(&Sweep::new(bad, good, 0.08), &upright(0.0, 0.0, 0.5)).is_none());
        assert!(sweep_capsule(&Sweep::new(good, bad, 0.08), &upright(0.0, 0.0, 0.5)).is_none());
        assert!(
            sweep_capsule(
                &Sweep::new(good, good, 0.08),
                &Capsule::new(good, f32::INFINITY)
            )
            .is_none()
        );
    }

    #[test]
    fn hits_work_the_same_far_from_the_origin_and_at_negative_coordinates() {
        // The arena is at a negative coordinate, so this is the ordinary case
        // rather than an exotic one.
        for centre in [
            Vec3::new(-69.0, 77.0, 49.0),
            Vec3::new(-400.0, 12.0, -400.0),
            Vec3::new(399.5, 90.0, -399.5),
        ] {
            let body = Capsule::new(
                Segment::new(centre + Vec3::Y * 0.5, centre + Vec3::Y * 1.8),
                0.5,
            );
            let start = Segment::new(
                centre + Vec3::new(0.0, 1.2, -2.0),
                centre + Vec3::new(0.0, 1.2, -1.0),
            );
            let end = Segment::new(
                centre + Vec3::new(0.0, 1.2, 0.5),
                centre + Vec3::new(0.0, 1.2, 1.5),
            );
            assert!(
                sweep_capsule(&Sweep::new(start, end, 0.08), &body).is_some(),
                "a hit at {centre} must behave like a hit at the origin"
            );
        }
    }

    #[test]
    fn the_contact_point_sits_on_the_struck_surface() {
        let body = upright(0.0, 0.0, 0.5);
        let start = Segment::new(Vec3::new(0.0, 1.2, -2.0), Vec3::new(0.0, 1.2, -1.0));
        let end = Segment::new(Vec3::new(0.0, 1.2, 0.2), Vec3::new(0.0, 1.2, 1.2));
        match sweep_capsule(&Sweep::new(start, end, 0.08), &body) {
            Some(hit) => {
                let (_, _, squared) = closest_points(body.axis, Segment::new(hit.point, hit.point));
                let distance = squared.sqrt();
                assert!(
                    (distance - body.radius).abs() < 1.0e-4,
                    "contact {distance} from the axis, radius {}",
                    body.radius
                );
            }
            None => panic!("expected a hit"),
        }
    }

    #[test]
    fn a_capsule_knows_what_is_inside_it() {
        let body = upright(0.0, 0.0, 0.5);
        assert!(body.contains(Vec3::new(0.0, 1.0, 0.0)));
        assert!(body.contains(Vec3::new(0.3, 0.5, 0.0)));
        assert!(!body.contains(Vec3::new(0.8, 1.0, 0.0)));
        assert!(!body.contains(Vec3::new(0.0, 2.6, 0.0)));
        // The caps are hemispheres, not flat ends.
        assert!(body.contains(Vec3::new(0.0, 2.2, 0.0)));
    }

    #[test]
    fn a_segment_reports_its_own_length_and_finiteness() {
        let segment = Segment::new(Vec3::ZERO, Vec3::new(3.0, 4.0, 0.0));
        assert!((segment.length() - 5.0).abs() < 1.0e-6);
        assert!(segment.is_finite());
        assert!(!Segment::new(Vec3::ZERO, Vec3::splat(f32::NAN)).is_finite());
    }
}
