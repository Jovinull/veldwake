//! Rigid voxel body parts: how a validated body becomes sixteen small voxel
//! volumes and sixteen meshes.
//!
//! **Rigid, not skinned.** The style bible requires that voxel structure stay
//! visible and intentional, and skinning deforms the grid: a bent elbow turns
//! cubes into sheared boxes, which is a generic mesh wearing a voxel costume.
//! Rigid parts also make the geometry static, so a character is compiled once,
//! uploaded once, and never re-uploaded; animation writes transforms only.
//!
//! A rigid joint opens a hole when it flexes unless the two volumes share
//! material there, so every non-root part extends at least one voxel past its
//! joint toward its parent. That is the character style contract's joint
//! overlap rule and it is asserted against the compiled volumes, not assumed.
//!
//! Meshing reuses `veldwake-voxel`'s exposed-face mesher unchanged. The
//! boundary policy is [`BoundaryPolicy::Expose`] and that is a deliberate
//! choice, not an inherited default: a body part is free standing, so every
//! face of it that touches nothing is a face a viewer can see.

use veldwake_voxel::{BoundaryPolicy, CHUNK_EDGE, Chunk, ChunkNeighborhood, Mesh, VoxelId};

use crate::descriptor::BodyMetrics;
use crate::material::CharacterMaterial;
use crate::skeleton::{ALL_BONES, BONE_COUNT, BoneId, Side};

/// Largest edge, in voxels, that one body part may occupy.
///
/// It is the scratch grid's edge. Declaring it lets compilation reuse exactly
/// one grid for every part instead of sizing an allocation per body.
pub const MAX_PART_EDGE: i32 = CHUNK_EDGE as i32;

/// An axis-aligned voxel box in a bone's local frame.
///
/// `origin` is where the volume's `(0, 0, 0)` cell sits, in bone-local voxel
/// units, and may be fractional: a limb bone runs down the centre of an
/// odd-width limb, so the grid corner falls on a half voxel while every voxel
/// still lands on the character's integer lattice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartVolume {
    pub bone: BoneId,
    pub origin: [f32; 3],
    pub dims: [i32; 3],
}

impl PartVolume {
    /// Inclusive-exclusive bounds in bone-local voxel units.
    #[must_use]
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let max = [
            self.origin[0] + self.dims[0] as f32,
            self.origin[1] + self.dims[1] as f32,
            self.origin[2] + self.dims[2] as f32,
        ];
        (self.origin, max)
    }

    /// Whether the bone's own origin falls inside this volume.
    ///
    /// The character style contract's first joint rule: a part that does not
    /// contain its own joint cannot hide that joint at any angle.
    #[must_use]
    pub fn contains_bone_origin(&self) -> bool {
        let (min, max) = self.bounds();
        (0..3).all(|axis| min[axis] <= 0.0 && max[axis] >= 0.0)
    }

    #[must_use]
    pub fn cell_count(&self) -> i32 {
        self.dims[0] * self.dims[1] * self.dims[2]
    }
}

/// The sixteen part volumes a body derives to.
///
/// Public because the probe prints them and the collision representation is
/// built from them; it is data, not a builder.
#[must_use]
pub fn part_volumes(body: &BodyMetrics) -> [PartVolume; BONE_COUNT] {
    let limb = body.limb_thickness;
    let half_limb = limb as f32 * 0.5;
    let torso_half = body.torso_half_width;
    let waist_half = body.waist_width / 2;
    let head_half = body.head_width / 2;

    let limb_volume = |bone: BoneId, height: i32, drop: i32| PartVolume {
        bone,
        origin: [-half_limb, -(drop as f32), -((limb / 2) as f32)],
        dims: [limb, height, limb],
    };

    let mut volumes = [PartVolume {
        bone: BoneId::Root,
        origin: [0.0; 3],
        dims: [1; 3],
    }; BONE_COUNT];

    for bone in ALL_BONES {
        volumes[bone.index()] = match bone {
            BoneId::Root => PartVolume {
                bone,
                origin: [-(torso_half as f32), -1.0, -((body.torso_depth / 2) as f32)],
                dims: [
                    body.hip_width,
                    body.spine_y - body.hip_y + 1,
                    body.torso_depth,
                ],
            },
            BoneId::Spine => PartVolume {
                bone,
                origin: [-(waist_half as f32), -1.0, -((body.waist_depth / 2) as f32)],
                dims: [
                    body.waist_width,
                    body.chest_y - body.spine_y + 1,
                    body.waist_depth,
                ],
            },
            BoneId::Chest => PartVolume {
                bone,
                origin: [-(torso_half as f32), -1.0, -((body.torso_depth / 2) as f32)],
                dims: [
                    body.hip_width,
                    body.head_bottom - body.chest_y + 2,
                    body.torso_depth,
                ],
            },
            BoneId::Head => PartVolume {
                bone,
                origin: [
                    -(head_half as f32),
                    (body.head_bottom - body.neck_y) as f32,
                    -((body.head_depth / 2) as f32),
                ],
                dims: [body.head_width, body.head_height, body.head_depth],
            },
            BoneId::UpperArmL | BoneId::UpperArmR => {
                limb_volume(bone, body.upper_arm_length + 1, body.upper_arm_length)
            }
            BoneId::ForearmL | BoneId::ForearmR => {
                limb_volume(bone, body.forearm_length + 1, body.forearm_length)
            }
            BoneId::HandL | BoneId::HandR => PartVolume {
                bone,
                origin: [
                    -half_limb,
                    -(body.hand_length as f32),
                    -((body.hand_depth / 2) as f32),
                ],
                dims: [limb, body.hand_length + 1, body.hand_depth],
            },
            BoneId::ThighL | BoneId::ThighR => {
                limb_volume(bone, body.thigh_length + 2, body.thigh_length + 1)
            }
            BoneId::ShinL | BoneId::ShinR => limb_volume(
                bone,
                body.knee_y - body.ankle_y + 2,
                body.knee_y - body.ankle_y + 1,
            ),
            BoneId::FootL | BoneId::FootR => PartVolume {
                bone,
                origin: [
                    -half_limb,
                    -(body.ankle_y as f32),
                    -((body.foot_length - 1) as f32),
                ],
                dims: [limb, body.ankle_y, body.foot_length],
            },
        };
    }
    volumes
}

/// The material at one cell of one part's volume.
///
/// A pure function of the part, the body, and the cell, which is what makes a
/// compiled character reproducible and what lets a test walk the whole body
/// looking for a material boundary that would not read.
#[must_use]
pub fn material_at(volume: &PartVolume, body: &BodyMetrics, cell: [i32; 3]) -> CharacterMaterial {
    let [x, y, z] = cell;
    let [width, height, depth] = volume.dims;
    // The character faces `-Z`, and a volume's origin is its most negative
    // corner, so cell `z == 0` is the front and `z == depth - 1` is the back.
    match volume.bone {
        BoneId::Root => {
            if y == height - 1 {
                CharacterMaterial::Belt
            } else {
                CharacterMaterial::TrouserCloth
            }
        }
        BoneId::Spine => CharacterMaterial::TunicPrimary,
        BoneId::Chest => {
            let band_low = (height - 4).max(1);
            let band_high = (height - 2).max(band_low + 1);
            let stripe_low = width / 2 - 1;
            let stripe_high = width / 2 + 1;
            if z == 0 && (stripe_low..stripe_high).contains(&x) {
                CharacterMaterial::Accent
            } else if (band_low..band_high).contains(&y) {
                CharacterMaterial::TunicSecondary
            } else {
                CharacterMaterial::TunicPrimary
            }
        }
        BoneId::Head => {
            // The cap covers the crown; one more row of it reaches down the
            // back of the head, which is what stops the hair reading as a lid.
            // The eyes sit a row below any hair so the two never touch.
            let cap = height - 2;
            if y == 0 {
                CharacterMaterial::SkinShade
            } else if y >= cap || (y == cap - 1 && z == depth - 1) {
                CharacterMaterial::Hair
            } else if y == 1 && z == 0 && (x == 0 || x == width - 1) {
                CharacterMaterial::EyeDark
            } else {
                CharacterMaterial::Skin
            }
        }
        BoneId::UpperArmL | BoneId::UpperArmR => CharacterMaterial::TunicPrimary,
        BoneId::ForearmL | BoneId::ForearmR => {
            if y >= height - 2 {
                CharacterMaterial::TunicPrimary
            } else {
                CharacterMaterial::Skin
            }
        }
        BoneId::HandL | BoneId::HandR => {
            if y == 0 {
                CharacterMaterial::SkinShade
            } else {
                CharacterMaterial::Skin
            }
        }
        BoneId::ThighL | BoneId::ThighR => CharacterMaterial::TrouserCloth,
        BoneId::ShinL | BoneId::ShinR => {
            // The boot cuff is exactly as tall as the foot, so the two read as
            // one piece of leather rather than as two.
            let boot_top = body.foot_height.clamp(2, height - 1);
            if y < boot_top {
                CharacterMaterial::BootLeather
            } else {
                CharacterMaterial::TrouserCloth
            }
        }
        BoneId::FootL | BoneId::FootR => CharacterMaterial::BootLeather,
    }
    // Left and right parts are mirror images of one another and every rule
    // above is symmetric in `x`, so no side-dependent branch is needed. The
    // mirror test in `compiler` is what keeps that true.
}

/// Why a body could not be turned into geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryError {
    /// A part is larger than the scratch grid the compiler reuses.
    PartTooLarge {
        bone: BoneId,
        dims: [i32; 3],
        maximum: i32,
    },
    /// A part has a zero or negative extent.
    PartIsEmpty { bone: BoneId, dims: [i32; 3] },
    /// A part does not contain its own joint, so flexing it would open a hole.
    JointNotCovered { bone: BoneId },
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PartTooLarge {
                bone,
                dims,
                maximum,
            } => write!(
                formatter,
                "part {} is {dims:?} voxels, beyond the {maximum}-voxel scratch edge",
                bone.name()
            ),
            Self::PartIsEmpty { bone, dims } => {
                write!(formatter, "part {} is empty: {dims:?}", bone.name())
            }
            Self::JointNotCovered { bone } => write!(
                formatter,
                "part {} does not contain its own joint origin",
                bone.name()
            ),
        }
    }
}

impl std::error::Error for GeometryError {}

/// Checks every part volume before any voxel is written.
pub fn validate_volumes(volumes: &[PartVolume; BONE_COUNT]) -> Result<(), GeometryError> {
    for volume in volumes {
        if volume.dims.iter().any(|extent| *extent <= 0) {
            return Err(GeometryError::PartIsEmpty {
                bone: volume.bone,
                dims: volume.dims,
            });
        }
        if volume.dims.iter().any(|extent| *extent > MAX_PART_EDGE) {
            return Err(GeometryError::PartTooLarge {
                bone: volume.bone,
                dims: volume.dims,
                maximum: MAX_PART_EDGE,
            });
        }
        if !volume.contains_bone_origin() {
            return Err(GeometryError::JointNotCovered { bone: volume.bone });
        }
    }
    Ok(())
}

/// One reusable scratch grid.
///
/// Compilation writes a part, meshes it, and clears exactly the cells it
/// wrote, so the whole run holds one grid rather than one per part. The grid
/// is `32³` because [`MAX_PART_EDGE`] is validated before anything is written.
pub struct PartScratch {
    grid: Chunk,
}

impl Default for PartScratch {
    fn default() -> Self {
        Self::new()
    }
}

impl PartScratch {
    #[must_use]
    pub fn new() -> Self {
        Self {
            grid: Chunk::empty(),
        }
    }

    /// Transient bytes this scratch holds. Reported by the probe so the peak
    /// is a measurement rather than an estimate.
    #[must_use]
    pub const fn payload_bytes(&self) -> usize {
        Chunk::BYTES
    }

    /// Writes one part, meshes it, and leaves the grid empty again.
    ///
    /// Returns the mesh, the number of solid voxels, and the per-material
    /// counts, in declaration order.
    pub fn build(
        &mut self,
        volume: &PartVolume,
        body: &BodyMetrics,
    ) -> Result<(Mesh, u32, [u32; crate::material::ALL_MATERIALS.len()]), GeometryError> {
        let [width, height, depth] = volume.dims;
        if width <= 0 || height <= 0 || depth <= 0 {
            return Err(GeometryError::PartIsEmpty {
                bone: volume.bone,
                dims: volume.dims,
            });
        }
        if width > MAX_PART_EDGE || height > MAX_PART_EDGE || depth > MAX_PART_EDGE {
            return Err(GeometryError::PartTooLarge {
                bone: volume.bone,
                dims: volume.dims,
                maximum: MAX_PART_EDGE,
            });
        }

        let mut counts = [0_u32; crate::material::ALL_MATERIALS.len()];
        let mut solids = 0_u32;
        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let material = material_at(volume, body, [x, y, z]);
                    counts[material_index(material)] += 1;
                    solids += 1;
                    // Coordinates are bounded by the checks above, so the
                    // write cannot be out of range; the result is still
                    // consumed rather than discarded.
                    if self
                        .grid
                        .write(x as usize, y as usize, z as usize, material.voxel_id())
                        .is_err()
                    {
                        return Err(GeometryError::PartTooLarge {
                            bone: volume.bone,
                            dims: volume.dims,
                            maximum: MAX_PART_EDGE,
                        });
                    }
                }
            }
        }

        // `Expose` is the right policy for a free-standing part: there is no
        // neighbour, and every outward face is one a viewer can see.
        let mesh = match veldwake_voxel::mesh_exposed_faces_with_neighbors(
            ChunkNeighborhood::new(&self.grid),
            BoundaryPolicy::Expose,
        ) {
            Ok(mesh) => mesh,
            // `Expose` never reports a missing neighbour.
            Err(_) => {
                return Err(GeometryError::PartIsEmpty {
                    bone: volume.bone,
                    dims: volume.dims,
                });
            }
        };

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let _ = self
                        .grid
                        .write(x as usize, y as usize, z as usize, VoxelId::AIR);
                }
            }
        }

        Ok((mesh, solids, counts))
    }
}

fn material_index(material: CharacterMaterial) -> usize {
    for (index, candidate) in crate::material::ALL_MATERIALS.into_iter().enumerate() {
        if candidate == material {
            return index;
        }
    }
    // `ALL_MATERIALS` is exhaustive by construction and both live in this
    // crate, so this is unreachable; returning zero keeps the counter honest
    // instead of panicking on a worker thread.
    debug_assert!(false, "material missing from ALL_MATERIALS");
    0
}

/// Which side a part belongs to, for the mirror tests.
#[must_use]
pub const fn part_side(volume: &PartVolume) -> Side {
    volume.bone.side()
}

#[cfg(test)]
mod tests {
    use super::{MAX_PART_EDGE, PartScratch, material_at, part_volumes, validate_volumes};
    use crate::descriptor::{CharacterDescriptor, Proportions};
    use crate::material::CharacterMaterial;
    use crate::skeleton::{ALL_BONES, BoneId, Side};

    fn body() -> crate::descriptor::BodyMetrics {
        match CharacterDescriptor::golden().validate() {
            Ok(validated) => *validated.body(),
            Err(error) => panic!("{error}"),
        }
    }

    #[test]
    fn every_part_contains_its_joint_and_fits_the_scratch_grid() {
        let body = body();
        let volumes = part_volumes(&body);
        assert!(validate_volumes(&volumes).is_ok());
        for volume in &volumes {
            assert!(
                volume.contains_bone_origin(),
                "{} does not cover its joint",
                volume.bone.name()
            );
            assert!(volume.dims.iter().all(|d| *d > 0 && *d <= MAX_PART_EDGE));
        }
    }

    #[test]
    fn the_golden_parts_have_the_reference_extents() {
        let body = body();
        let volumes = part_volumes(&body);
        let dims = |bone: BoneId| volumes[bone.index()].dims;
        assert_eq!(dims(BoneId::Root), [8, 4, 5]);
        assert_eq!(dims(BoneId::Spine), [6, 3, 3]);
        assert_eq!(dims(BoneId::Chest), [8, 6, 5]);
        assert_eq!(dims(BoneId::Head), [4, 5, 3]);
        assert_eq!(dims(BoneId::UpperArmR), [3, 6, 3]);
        assert_eq!(dims(BoneId::ForearmR), [3, 5, 3]);
        assert_eq!(dims(BoneId::HandR), [3, 4, 4]);
        assert_eq!(dims(BoneId::ThighR), [3, 7, 3]);
        assert_eq!(dims(BoneId::ShinR), [3, 8, 3]);
        assert_eq!(dims(BoneId::FootR), [3, 3, 5]);
    }

    #[test]
    fn left_and_right_parts_are_identical_volumes() {
        let body = body();
        let volumes = part_volumes(&body);
        for bone in ALL_BONES {
            if bone.side() == Side::Centre {
                continue;
            }
            let mine = volumes[bone.index()];
            let theirs = volumes[bone.mirrored().index()];
            assert_eq!(mine.dims, theirs.dims, "{} differs in size", bone.name());
            assert_eq!(
                mine.origin,
                theirs.origin,
                "{} differs in local origin",
                bone.name()
            );
        }
    }

    #[test]
    fn a_part_meshes_and_leaves_the_scratch_grid_clean() {
        let body = body();
        let volumes = part_volumes(&body);
        let mut scratch = PartScratch::new();
        for volume in &volumes {
            let outcome = scratch.build(volume, &body);
            let (mesh, solids, counts) = match outcome {
                Ok(parts) => parts,
                Err(error) => panic!("{} failed to build: {error}", volume.bone.name()),
            };
            assert_eq!(solids as i32, volume.cell_count());
            assert_eq!(counts.iter().sum::<u32>(), solids);
            assert!(mesh.quad_count() > 0, "{} is empty", volume.bone.name());
            assert_eq!(mesh.indices().len() % 6, 0);
            assert_eq!(mesh.vertices().len(), mesh.quad_count() * 4);
        }
        // Rebuilding the first part after the whole pass must give the same
        // mesh, which is only true if every build cleared what it wrote.
        let first = &volumes[0];
        let again = match scratch.build(first, &body) {
            Ok(parts) => parts,
            Err(error) => panic!("{error}"),
        };
        let fresh = match PartScratch::new().build(first, &body) {
            Ok(parts) => parts,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(again.0, fresh.0, "the scratch grid was left dirty");
    }

    #[test]
    fn a_solid_box_meshes_to_exactly_its_own_surface() {
        let body = body();
        let volumes = part_volumes(&body);
        let mut scratch = PartScratch::new();
        for volume in &volumes {
            let [w, h, d] = volume.dims;
            let expected = 2 * (w * h + h * d + w * d);
            let (mesh, _, _) = match scratch.build(volume, &body) {
                Ok(parts) => parts,
                Err(error) => panic!("{error}"),
            };
            assert_eq!(
                mesh.quad_count() as i32,
                expected,
                "{} has {} quads, not the {expected} of a solid box",
                volume.bone.name(),
                mesh.quad_count()
            );
        }
    }

    #[test]
    fn the_head_carries_exactly_two_eye_voxels() {
        let body = body();
        let volumes = part_volumes(&body);
        let head = volumes[BoneId::Head.index()];
        let mut eyes = 0;
        for z in 0..head.dims[2] {
            for y in 0..head.dims[1] {
                for x in 0..head.dims[0] {
                    if material_at(&head, &body, [x, y, z]) == CharacterMaterial::EyeDark {
                        eyes += 1;
                    }
                }
            }
        }
        assert_eq!(eyes, 2, "the face detail budget is two voxels");
    }

    #[test]
    fn a_sturdy_body_produces_different_but_valid_volumes() {
        let sturdy = match (CharacterDescriptor {
            proportions: Proportions::sturdy(),
            ..CharacterDescriptor::golden()
        })
        .validate()
        {
            Ok(validated) => *validated.body(),
            Err(error) => panic!("{error}"),
        };
        let golden = part_volumes(&body());
        let other = part_volumes(&sturdy);
        assert!(validate_volumes(&other).is_ok());
        assert_ne!(golden, other, "the compiler ignores the descriptor");
    }
}
