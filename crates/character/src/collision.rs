//! Collision **representation**, not collision simulation.
//!
//! M5 has to be able to answer "what volume would this humanoid occupy", at
//! two levels of precision, without owning a physics world:
//!
//! - a [`BodyCapsule`] for the whole body in its rest pose, which is what a
//!   future broadphase or character controller would want;
//! - one [`PartBox`] per bone, which is what a future hit test would want.
//!
//! Both are derived from the compiled geometry rather than authored, so they
//! cannot drift from the body they describe. Nothing here integrates, resolves
//! a penetration, or applies a force, and no physics dependency is introduced:
//! there is nothing yet for one to simulate.

use glam::{Mat4, Vec3};

use crate::descriptor::CHARACTER_VOXEL_SIZE;
use crate::geometry::PartVolume;
use crate::skeleton::{BONE_COUNT, BoneId};

/// How much slack the capsule keeps around the body it contains, in world
/// units. A hundredth of a character voxel: invisible, and decisive.
const CONTAINMENT_MARGIN: f32 = 1.0e-3;

/// An upright capsule approximating the whole body, in world units, relative
/// to the point the character stands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyCapsule {
    /// Horizontal radius.
    pub radius: f32,
    /// Height of the cylindrical section between the two cap centres.
    pub segment_height: f32,
    /// Height of the lower cap's centre above the ground.
    pub base_height: f32,
}

impl BodyCapsule {
    /// Total height from the ground to the top of the upper cap.
    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.base_height + self.segment_height + self.radius
    }

    /// Whether a world-space point, relative to the character's stand point,
    /// is inside the capsule.
    #[must_use]
    pub fn contains_local(&self, point: Vec3) -> bool {
        let horizontal = (point.x * point.x + point.z * point.z).sqrt();
        if horizontal > self.radius {
            return false;
        }
        let low = self.base_height;
        let high = self.base_height + self.segment_height;
        if (low..=high).contains(&point.y) {
            return true;
        }
        let cap_centre = if point.y < low { low } else { high };
        let dy = point.y - cap_centre;
        horizontal * horizontal + dy * dy <= self.radius * self.radius
    }
}

/// One bone's box, in that part's **grid** space.
///
/// Grid space, not bone space, because that is the space the compiled mesh
/// vertices are in and the space a part's world matrix already accounts for.
/// Mixing the two double-counts the part's origin, which puts a foot exactly
/// one foot-height below the ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartBox {
    pub bone: BoneId,
    /// Box centre in grid voxel units.
    pub centre: Vec3,
    /// Half extents in grid voxel units.
    pub half_extents: Vec3,
}

impl PartBox {
    #[must_use]
    fn from_volume(volume: &PartVolume) -> Self {
        let size = Vec3::new(
            volume.dims[0] as f32,
            volume.dims[1] as f32,
            volume.dims[2] as f32,
        );
        Self {
            bone: volume.bone,
            centre: size * 0.5,
            half_extents: size * 0.5,
        }
    }

    /// The eight corners of this box, in bone-local voxel units.
    #[must_use]
    pub fn corners(&self) -> [Vec3; 8] {
        let h = self.half_extents;
        let mut corners = [Vec3::ZERO; 8];
        for (index, corner) in corners.iter_mut().enumerate() {
            let sign = |bit: usize| if index & (1 << bit) == 0 { -1.0 } else { 1.0 };
            *corner = self.centre + Vec3::new(sign(0) * h.x, sign(1) * h.y, sign(2) * h.z);
        }
        corners
    }
}

/// An axis-aligned world-space box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldAabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl WorldAabb {
    #[must_use]
    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        let mut iterator = points.into_iter();
        let first = iterator.next()?;
        let mut min = first;
        let mut max = first;
        for point in iterator {
            min = min.min(point);
            max = max.max(point);
        }
        Some(Self { min, max })
    }

    #[must_use]
    pub fn contains(&self, point: Vec3) -> bool {
        (0..3).all(|axis| point[axis] >= self.min[axis] && point[axis] <= self.max[axis])
    }

    #[must_use]
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
}

/// The whole collision representation of one compiled character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionRepresentation {
    capsule: BodyCapsule,
    boxes: [PartBox; BONE_COUNT],
}

impl CollisionRepresentation {
    /// Derives the representation from the compiled part volumes and the rest
    /// pose that places them.
    #[must_use]
    pub fn derive(
        volumes: &[PartVolume; BONE_COUNT],
        rest_world: &[crate::skeleton::Transform; BONE_COUNT],
    ) -> Self {
        let mut boxes = [PartBox::from_volume(&volumes[0]); BONE_COUNT];
        for (index, volume) in volumes.iter().enumerate() {
            boxes[index] = PartBox::from_volume(volume);
        }

        // The capsule has to contain every voxel of the rest pose, so it is
        // measured from the posed boxes rather than from the descriptor. Two
        // passes: the radius is the widest the body ever gets, and the cap
        // centres are then placed so that every point satisfies the capsule's
        // own containment test rather than merely sitting inside a bounding
        // box. A point at horizontal distance `h` needs its cap centre within
        // `sqrt(r^2 - h^2)` vertically, which is what the second pass solves.
        let scale = CHARACTER_VOXEL_SIZE;
        let mut points = Vec::with_capacity(BONE_COUNT * 8);
        let mut radius_squared = 0.0_f32;
        for (index, part) in boxes.iter().enumerate() {
            let transform = rest_world[index];
            let origin = Vec3::from_array(volumes[index].origin);
            for corner in part.corners() {
                let point = transform.transform_point(origin + corner) * scale;
                points.push(point);
                radius_squared = radius_squared.max(point.x.mul_add(point.x, point.z * point.z));
            }
        }
        let radius = radius_squared.sqrt().max(1.0e-4) + CONTAINMENT_MARGIN;

        let mut cap_top = f32::NEG_INFINITY;
        let mut cap_bottom = f32::INFINITY;
        for point in &points {
            let horizontal_squared = point.x.mul_add(point.x, point.z * point.z);
            let reach = (radius * radius - horizontal_squared).max(0.0).sqrt();
            cap_top = cap_top.max(point.y - reach);
            cap_bottom = cap_bottom.min(point.y + reach);
        }
        // The solve above puts the binding corner exactly on the capsule's
        // surface, where containment is a floating-point coin toss. Widening
        // the radius cannot help, because the cap centres are solved from it
        // and the corner stays on the surface whatever it is. Separating the
        // cap centres does: it is the one direction that gives that corner
        // slack. Raising the upper cap also handles the degenerate
        // short-and-wide case, because it only puts more of the body inside the
        // cylinder.
        let cap_bottom = cap_bottom - CONTAINMENT_MARGIN;
        let cap_top = (cap_top + CONTAINMENT_MARGIN).max(cap_bottom);
        let base_height = cap_bottom;
        let segment_height = cap_top - cap_bottom;

        Self {
            capsule: BodyCapsule {
                radius,
                segment_height,
                base_height,
            },
            boxes,
        }
    }

    #[must_use]
    pub const fn capsule(&self) -> BodyCapsule {
        self.capsule
    }

    #[must_use]
    pub const fn boxes(&self) -> &[PartBox; BONE_COUNT] {
        &self.boxes
    }

    #[must_use]
    pub const fn part_box(&self, bone: BoneId) -> &PartBox {
        &self.boxes[bone.index()]
    }

    /// World-space axis-aligned bounds of a posed character.
    ///
    /// `part_matrices` are the same matrices the renderer draws with, so the
    /// bounds describe exactly what is on screen.
    #[must_use]
    pub fn posed_bounds(&self, part_matrices: &[Mat4; BONE_COUNT]) -> Option<WorldAabb> {
        let mut points = Vec::with_capacity(BONE_COUNT * 8);
        for (index, part) in self.boxes.iter().enumerate() {
            let matrix = part_matrices[index];
            for corner in part.corners() {
                points.push(matrix.transform_point3(corner));
            }
        }
        WorldAabb::from_points(points)
    }

    /// Logical bytes this representation occupies.
    #[must_use]
    pub const fn payload_bytes(&self) -> usize {
        size_of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::{CollisionRepresentation, WorldAabb};
    use crate::descriptor::{CHARACTER_VOXEL_SIZE, CharacterDescriptor};
    use crate::geometry::part_volumes;
    use crate::skeleton::{BONE_COUNT, BoneId, Skeleton};
    use glam::Vec3;

    fn representation() -> (CollisionRepresentation, crate::descriptor::BodyMetrics) {
        let validated = match CharacterDescriptor::golden().validate() {
            Ok(validated) => validated,
            Err(error) => panic!("{error}"),
        };
        let body = *validated.body();
        let volumes = part_volumes(&body);
        let skeleton = Skeleton::derive(&body);
        (
            CollisionRepresentation::derive(&volumes, &skeleton.rest_world()),
            body,
        )
    }

    #[test]
    fn the_capsule_contains_every_rest_pose_voxel() {
        let (collision, body) = representation();
        let volumes = part_volumes(&body);
        let skeleton = Skeleton::derive(&body);
        let rest = skeleton.rest_world();
        let capsule = collision.capsule();
        for (index, part) in collision.boxes().iter().enumerate() {
            let origin = Vec3::from_array(volumes[index].origin);
            for corner in part.corners() {
                let voxel_point = rest[index].transform_point(origin + corner);
                let world = voxel_point * CHARACTER_VOXEL_SIZE;
                assert!(
                    capsule.contains_local(world),
                    "{} corner {world} escapes the capsule {capsule:?}",
                    part.bone.name()
                );
            }
        }
    }

    #[test]
    fn the_capsule_is_about_as_tall_as_the_body() {
        let (collision, body) = representation();
        let capsule = collision.capsule();
        let expected = body.height_units();
        assert!(
            (capsule.total_height() - expected).abs() < CHARACTER_VOXEL_SIZE * 2.0,
            "capsule {} against body {expected}",
            capsule.total_height()
        );
        assert!(capsule.radius > 0.0);
        assert!(capsule.segment_height >= 0.0);
    }

    #[test]
    fn the_capsule_rejects_a_point_well_outside_it() {
        let (collision, _) = representation();
        let capsule = collision.capsule();
        assert!(!capsule.contains_local(Vec3::new(10.0, 1.0, 0.0)));
        assert!(!capsule.contains_local(Vec3::new(0.0, -5.0, 0.0)));
        assert!(capsule.contains_local(Vec3::new(0.0, capsule.base_height, 0.0)));
    }

    #[test]
    fn every_part_box_matches_its_volume() {
        let (collision, body) = representation();
        let volumes = part_volumes(&body);
        for (index, part) in collision.boxes().iter().enumerate() {
            let volume = &volumes[index];
            assert_eq!(part.bone, volume.bone);
            let size = part.half_extents * 2.0;
            assert!((size.x - volume.dims[0] as f32).abs() < 1.0e-4);
            assert!((size.y - volume.dims[1] as f32).abs() < 1.0e-4);
            assert!((size.z - volume.dims[2] as f32).abs() < 1.0e-4);
        }
    }

    #[test]
    fn posed_bounds_follow_the_matrices_they_are_given() {
        let (collision, _) = representation();
        let identity = [glam::Mat4::IDENTITY; BONE_COUNT];
        let Some(first) = collision.posed_bounds(&identity) else {
            panic!("bounds over sixteen boxes cannot be empty");
        };
        let shifted = [glam::Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)); BONE_COUNT];
        let Some(second) = collision.posed_bounds(&shifted) else {
            panic!("bounds over sixteen boxes cannot be empty");
        };
        assert!((second.min.x - first.min.x - 5.0).abs() < 1.0e-4);
        assert!((second.size() - first.size()).length() < 1.0e-4);
    }

    #[test]
    fn a_box_has_eight_distinct_corners() {
        let (collision, _) = representation();
        let corners = collision.part_box(BoneId::Chest).corners();
        for (index, corner) in corners.iter().enumerate() {
            for (other_index, other) in corners.iter().enumerate() {
                if index != other_index {
                    assert!(
                        (*corner - *other).length() > 1.0e-4,
                        "corners {index} and {other_index} coincide"
                    );
                }
            }
        }
    }

    #[test]
    fn an_empty_point_set_has_no_bounds() {
        assert_eq!(WorldAabb::from_points(Vec::<Vec3>::new()), None);
        let Some(single) = WorldAabb::from_points([Vec3::ONE]) else {
            panic!("one point has bounds");
        };
        assert!(single.contains(Vec3::ONE));
        assert!(!single.contains(Vec3::ZERO));
    }
}
